// Paw Agent Engine — OpenAI-Compatible Provider
// Handles: OpenAI, OpenRouter, Ollama, Azure OpenAI, and any OpenAI-compatible REST API.
// Implements the AiProvider Golden Trait.

use crate::atoms::traits::{AiProvider, ModelInfo, ProviderError};
use crate::engine::types::{
    ContentBlock, Message, MessageContent, ProviderConfig, ProviderKind, Role, StreamChunk,
    TokenUsage, ToolCallDelta, ToolDefinition,
};
use async_trait::async_trait;
use futures::StreamExt;
use log::{error, info, warn};
use reqwest::Client;
use serde_json::{json, Value};
use zeroize::Zeroizing;

// Import constrained decoding for strict mode / JSON format enforcement
use crate::engine::constrained;

// ── Shared retry utilities ─────────────────────────────────────────────────
// Re-export from engine::http so existing callers (anthropic, google) work
// with `use super::openai::{MAX_RETRIES, ...}` unchanged.

pub(crate) use crate::engine::http::{
    is_retryable_status, parse_retry_after, retry_delay, MAX_RETRIES,
};

// Import the circuit breaker and security utilities
use crate::engine::http::{
    pinned_client, sign_and_log_request, update_last_audit_status, CircuitBreaker,
};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

/// Per-endpoint circuit breakers so failures from one provider/model
/// (e.g. o3-pro on Azure) don't trip the breaker for unrelated providers
/// (e.g. Claude on Azure).
static OPENAI_CIRCUITS: LazyLock<Mutex<HashMap<String, Arc<CircuitBreaker>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Get (or create) the circuit breaker for a given base URL.
fn get_circuit(base_url: &str) -> Arc<CircuitBreaker> {
    let mut map = OPENAI_CIRCUITS.lock().unwrap();
    map.entry(base_url.to_string())
        .or_insert_with(|| Arc::new(CircuitBreaker::new(5, 60)))
        .clone()
}

/// Returns true for OpenAI reasoning models that reject the `temperature`
/// parameter (only the default value 1 is accepted).
fn is_reasoning_model(model: &str) -> bool {
    let m = model.to_lowercase();
    m.starts_with("o1") || m.starts_with("o3") || m.starts_with("o4")
}

// ── OpenAI provider struct ─────────────────────────────────────────────────

pub struct OpenAiProvider {
    client: Client,
    base_url: String,
    /// API key wrapped in Zeroizing<> — automatically zeroed from RAM on drop.
    api_key: Zeroizing<String>,
    is_azure: bool,
    /// The concrete provider variant — needed for constrained decoding
    /// capability detection (e.g. Ollama vs OpenAI vs DeepSeek).
    provider_kind: ProviderKind,
    /// Per-endpoint circuit breaker — isolates failures to a single provider.
    circuit: Arc<CircuitBreaker>,
    /// True when the endpoint uses the OpenAI Responses API format
    /// (e.g. Azure AI Foundry o3-pro at /openai/responses).
    is_responses_api: bool,
}

impl OpenAiProvider {
    pub fn new(config: &ProviderConfig) -> Self {
        let mut base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| config.kind.default_base_url().to_string());

        // Ollama's OpenAI-compatible endpoint lives at /v1.  The DB may
        // store the raw Ollama URL (http://…:11434) without the suffix —
        // normalise it here so chat requests hit /v1/chat/completions.
        if config.kind == ProviderKind::Ollama {
            let trimmed = base_url.trim_end_matches('/');
            if !trimmed.ends_with("/v1") {
                base_url = format!("{}/v1", trimmed);
            }
        }

        // Azure AI Foundry: Users paste the full Target URI from the Foundry
        // portal.  Three URL patterns exist:
        //
        //  1. .../models/chat/completions?api-version=…  (unified inference — grok, kimi, etc.)
        //  2. .../openai/deployments/{name}/chat/completions?api-version=…  (classic Azure OpenAI)
        //  3. .../openai/responses?api-version=…  (Responses API — gpt-5.4, o3-pro)
        //
        // Anthropic URLs (.../anthropic/v1/messages) are routed to
        // AnthropicProvider in mod.rs and should never reach here — but if
        // they do, leave the URL untouched as a safety net.
        //
        // If the URL already contains /chat/completions, store it as-is.
        // If it's a /responses URL, preserve it for the Responses API path.
        // If it's a bare resource URL, normalise to /models as a fallback.
        let mut is_responses_api = false;
        if config.kind == ProviderKind::AzureFoundry {
            let trimmed = base_url.trim_end_matches('/');

            if trimmed.contains("/anthropic") {
                // Anthropic wire format — should have been routed to
                // AnthropicProvider. Keep as-is; chat_stream will fail
                // gracefully with a clear error rather than mangling the URL.
                base_url = trimmed.to_string();
            } else if trimmed.contains("/chat/completions") {
                // Full Target URI — already a chat/completions endpoint.
                // Use as-is, preserving the api-version query param.
                base_url = trimmed.to_string();
            } else if trimmed.contains("/openai/responses") {
                // Responses API endpoint (o3-pro, etc.) — preserve as-is.
                // These models do NOT support /chat/completions.
                base_url = trimmed.to_string();
                is_responses_api = true;
            } else if trimmed.contains("/openai") {
                // Other Azure OpenAI path — convert to deployment-based
                // chat/completions using the model name.
                let host = trimmed
                    .split("/openai")
                    .next()
                    .unwrap_or(trimmed)
                    .trim_end_matches('/');
                let api_version = trimmed
                    .split("api-version=")
                    .nth(1)
                    .and_then(|v| v.split('&').next())
                    .unwrap_or("2025-03-01-preview");
                let model = config.default_model.as_deref().unwrap_or("gpt-4o");
                base_url = format!(
                    "{}/openai/deployments/{}/chat/completions?api-version={}",
                    host, model, api_version
                );
            } else {
                // Bare resource URL (e.g. https://xxx.services.ai.azure.com)
                let host_base = trimmed
                    .split("/models")
                    .next()
                    .unwrap_or(trimmed)
                    .trim_end_matches('/');
                base_url = format!("{}/models", host_base);
            }
        }

        let is_azure = base_url.contains(".azure.com");
        let circuit = get_circuit(&base_url);
        OpenAiProvider {
            client: pinned_client(),
            base_url,
            api_key: Zeroizing::new(config.api_key.clone()),
            is_azure,
            provider_kind: config.kind,
            circuit,
            is_responses_api,
        }
    }

    fn format_messages(messages: &[Message]) -> Vec<Value> {
        messages
            .iter()
            .map(|msg| {
                let content_val =
                    match &msg.content {
                        MessageContent::Text(s) => json!(s),
                        MessageContent::Blocks(blocks) => {
                            let parts: Vec<Value> = blocks.iter().map(|b| match b {
                        ContentBlock::Text { text } => json!({"type": "text", "text": text}),
                        ContentBlock::ImageUrl { image_url } => json!({
                            "type": "image_url",
                            "image_url": {
                                "url": image_url.url,
                                "detail": image_url.detail.as_deref().unwrap_or("auto"),
                            }
                        }),
                        ContentBlock::Document { mime_type, data, name } => json!({
                            "type": "file",
                            "file": {
                                "filename": name.as_deref().unwrap_or("document.pdf"),
                                "file_data": format!("data:{};base64,{}", mime_type, data),
                            }
                        }),
                    }).collect();
                            json!(parts)
                        }
                    };
                let mut m = json!({
                    "role": msg.role,
                    "content": content_val,
                });
                if let Some(tc) = &msg.tool_calls {
                    m["tool_calls"] = json!(tc);
                    // Debug: log tool_call IDs being sent to API
                    for t in tc {
                        log::debug!(
                            "[format-msg] assistant tool_call: id={:?} fn={}\",",
                            t.id,
                            t.function.name
                        );
                    }
                }
                if let Some(id) = &msg.tool_call_id {
                    m["tool_call_id"] = json!(id);
                    log::debug!(
                        "[format-msg] tool result: tool_call_id={:?} role={:?}",
                        id,
                        msg.role
                    );
                }
                if let Some(name) = &msg.name {
                    m["name"] = json!(name);
                }
                m
            })
            .collect()
    }

    fn format_tools(tools: &[ToolDefinition]) -> Vec<Value> {
        tools
            .iter()
            .map(|t| {
                json!({
                    "type": t.tool_type,
                    "function": {
                        "name": t.function.name,
                        "description": t.function.description,
                        "parameters": t.function.parameters,
                    }
                })
            })
            .collect()
    }

    /// Format messages for the OpenAI Responses API (`/responses` endpoint).
    ///
    /// Converts our internal `Message` array into the Responses API `input`
    /// format. Regular messages use the shorthand `{role, content}` form.
    /// Tool results use `{type: "function_call_output", call_id, output}`.
    /// Assistant tool-call messages emit `{type: "function_call", ...}` items.
    fn format_responses_input(messages: &[Message]) -> Vec<Value> {
        let mut input = Vec::new();
        for msg in messages {
            match msg.role {
                Role::Tool => {
                    // Tool result → function_call_output
                    let content = match &msg.content {
                        MessageContent::Text(s) => s.clone(),
                        MessageContent::Blocks(blocks) => blocks
                            .iter()
                            .filter_map(|b| match b {
                                ContentBlock::Text { text } => Some(text.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("\n"),
                    };
                    if let Some(call_id) = &msg.tool_call_id {
                        input.push(json!({
                            "type": "function_call_output",
                            "call_id": call_id,
                            "output": content,
                        }));
                    }
                }
                _ => {
                    // For assistant messages with tool_calls, emit function_call items
                    if let Some(tc) = &msg.tool_calls {
                        // Emit text content if present
                        if let MessageContent::Text(s) = &msg.content {
                            if !s.is_empty() {
                                input.push(json!({
                                    "role": "assistant",
                                    "content": s,
                                }));
                            }
                        }
                        for call in tc {
                            input.push(json!({
                                "type": "function_call",
                                "call_id": call.id,
                                "name": call.function.name,
                                "arguments": call.function.arguments,
                            }));
                        }
                    } else {
                        // Regular message — use shorthand format
                        let content_val = match &msg.content {
                            MessageContent::Text(s) => json!(s),
                            MessageContent::Blocks(blocks) => {
                                let parts: Vec<Value> = blocks
                                    .iter()
                                    .map(|b| match b {
                                        ContentBlock::Text { text } => {
                                            json!({"type": "input_text", "text": text})
                                        }
                                        ContentBlock::ImageUrl { image_url } => json!({
                                            "type": "input_image",
                                            "image_url": image_url.url,
                                        }),
                                        ContentBlock::Document {
                                            mime_type: _,
                                            data: _,
                                            name: _,
                                        } => {
                                            // Responses API doesn't have a direct file input;
                                            // fall back to text description
                                            json!({"type": "input_text", "text": "[document attached]"})
                                        }
                                    })
                                    .collect();
                                json!(parts)
                            }
                        };
                        input.push(json!({
                            "role": msg.role,
                            "content": content_val,
                        }));
                    }
                }
            }
        }
        input
    }

    /// Send a request via the OpenAI Responses API (`/openai/responses`).
    ///
    /// Used for models like o3-pro on Azure AI Foundry that only support
    /// the Responses API, not Chat Completions.
    async fn chat_stream_responses(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        model: &str,
        temperature: Option<f64>,
        thinking_level: Option<&str>,
    ) -> Result<Vec<StreamChunk>, ProviderError> {
        let url = &self.base_url;

        let input = Self::format_responses_input(messages);
        let mut body = json!({
            "model": model,
            "input": input,
            "stream": true,
        });

        if !tools.is_empty() {
            // Responses API uses a flat tool format with top-level
            // name/description/parameters (NOT nested under "function").
            let resp_tools: Vec<Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "name": t.function.name,
                        "description": t.function.description,
                        "parameters": t.function.parameters,
                    })
                })
                .collect();
            body["tools"] = json!(resp_tools);
        }
        if let Some(temp) = temperature {
            if !is_reasoning_model(model) {
                body["temperature"] = json!(temp);
            }
        }
        if let Some(level) = thinking_level {
            let effort = match level {
                "low" => "low",
                "high" => "high",
                _ => "medium",
            };
            body["reasoning"] = json!({ "effort": effort });
        }

        info!(
            "[engine] OpenAI Responses API request to {} model={}",
            url, model
        );

        if let Err(msg) = self.circuit.check() {
            return Err(ProviderError::Transport(msg));
        }

        let mut last_error = String::new();
        let mut last_status: u16 = 0;
        let mut retry_after: Option<u64> = None;

        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let delay = retry_delay(attempt - 1, retry_after.take()).await;
                warn!(
                    "[engine] Responses API retry {}/{} after {}ms",
                    attempt,
                    MAX_RETRIES,
                    delay.as_millis()
                );
            }

            let mut req = self
                .client
                .post(url)
                .header("Content-Type", "application/json");
            if self.is_azure {
                req = req.header("api-key", self.api_key.as_str());
            } else {
                req = req.header("Authorization", format!("Bearer {}", self.api_key.as_str()));
            }

            let body_bytes = serde_json::to_vec(&body).unwrap_or_default();
            sign_and_log_request("openai-responses", model, &body_bytes);

            let response = match req.json(&body).send().await {
                Ok(r) => {
                    update_last_audit_status(r.status().as_u16());
                    r
                }
                Err(e) => {
                    self.circuit.record_failure();
                    last_error = format!("HTTP request failed: {}", e);
                    last_status = 0;
                    if attempt < MAX_RETRIES {
                        continue;
                    }
                    return Err(ProviderError::Transport(last_error));
                }
            };

            if !response.status().is_success() {
                let status = response.status().as_u16();
                last_status = status;
                retry_after = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(parse_retry_after);
                let body_text = response.text().await.unwrap_or_default();
                last_error = format!(
                    "API error {} at {}: {}",
                    status,
                    url,
                    crate::engine::types::truncate_utf8(&body_text, 200)
                );
                error!(
                    "[engine] Responses API error {}: {}",
                    status,
                    crate::engine::types::truncate_utf8(&body_text, 500)
                );

                self.circuit.record_failure();

                if status == 401 || status == 403 {
                    return Err(ProviderError::Auth(last_error));
                }
                if is_retryable_status(status) && attempt < MAX_RETRIES {
                    continue;
                }
                return if status == 429 {
                    Err(ProviderError::RateLimited {
                        message: last_error,
                        retry_after_secs: retry_after.take(),
                    })
                } else {
                    Err(ProviderError::Api {
                        status,
                        message: last_error,
                    })
                };
            }

            // ── Parse Responses API SSE stream ──────────────────────
            // Events use `event: <type>\ndata: <json>\n\n` format.
            let mut chunks = Vec::new();
            let mut byte_stream = response.bytes_stream();
            let mut raw_buf: Vec<u8> = Vec::new();
            let mut current_event = String::new();

            while let Some(result) = byte_stream.next().await {
                let bytes = result
                    .map_err(|e| ProviderError::Transport(format!("Stream read error: {}", e)))?;
                raw_buf.extend_from_slice(&bytes);

                while let Some(pos) = raw_buf.iter().position(|&b| b == b'\n') {
                    let line_bytes = raw_buf[..pos].to_vec();
                    raw_buf = raw_buf[pos + 1..].to_vec();

                    let line = match std::str::from_utf8(&line_bytes) {
                        Ok(s) => s.trim().to_string(),
                        Err(_) => continue,
                    };

                    if line.is_empty() {
                        current_event.clear();
                        continue;
                    }

                    if let Some(event_type) = line.strip_prefix("event: ") {
                        current_event = event_type.to_string();
                        continue;
                    }

                    if let Some(data) = line.strip_prefix("data: ") {
                        let v: Value = match serde_json::from_str(data) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };

                        match current_event.as_str() {
                            "response.output_text.delta" => {
                                if let Some(delta) = v["delta"].as_str() {
                                    chunks.push(StreamChunk {
                                        delta_text: Some(delta.to_string()),
                                        tool_calls: vec![],
                                        finish_reason: None,
                                        usage: None,
                                        model: None,
                                        thought_parts: vec![],
                                        thinking_text: None,
                                    });
                                }
                            }
                            "response.reasoning_summary_text.delta" => {
                                if let Some(delta) = v["delta"].as_str() {
                                    chunks.push(StreamChunk {
                                        delta_text: None,
                                        tool_calls: vec![],
                                        finish_reason: None,
                                        usage: None,
                                        model: None,
                                        thought_parts: vec![],
                                        thinking_text: Some(delta.to_string()),
                                    });
                                }
                            }
                            "response.output_item.added" => {
                                if v["type"].as_str() == Some("function_call") {
                                    let output_index = v["output_index"].as_u64().unwrap_or(0);
                                    let call_id = v["call_id"].as_str().unwrap_or("").to_string();
                                    let name = v["name"].as_str().unwrap_or("").to_string();
                                    chunks.push(StreamChunk {
                                        delta_text: None,
                                        tool_calls: vec![ToolCallDelta {
                                            index: output_index as usize,
                                            id: Some(call_id),
                                            function_name: Some(name),
                                            arguments_delta: None,
                                            thought_signature: None,
                                        }],
                                        finish_reason: None,
                                        usage: None,
                                        model: None,
                                        thought_parts: vec![],
                                        thinking_text: None,
                                    });
                                }
                            }
                            "response.function_call_arguments.delta" => {
                                let output_index = v["output_index"].as_u64().unwrap_or(0);
                                if let Some(delta) = v["delta"].as_str() {
                                    chunks.push(StreamChunk {
                                        delta_text: None,
                                        tool_calls: vec![ToolCallDelta {
                                            index: output_index as usize,
                                            id: None,
                                            function_name: None,
                                            arguments_delta: Some(delta.to_string()),
                                            thought_signature: None,
                                        }],
                                        finish_reason: None,
                                        usage: None,
                                        model: None,
                                        thought_parts: vec![],
                                        thinking_text: None,
                                    });
                                }
                            }
                            "response.completed" => {
                                let usage = v.get("usage").and_then(|u| {
                                    let input_tok = u["input_tokens"].as_u64().unwrap_or(0);
                                    let output_tok = u["output_tokens"].as_u64().unwrap_or(0);
                                    if input_tok > 0 || output_tok > 0 {
                                        Some(TokenUsage {
                                            input_tokens: input_tok,
                                            output_tokens: output_tok,
                                            total_tokens: u["total_tokens"]
                                                .as_u64()
                                                .unwrap_or(input_tok + output_tok),
                                            ..Default::default()
                                        })
                                    } else {
                                        None
                                    }
                                });
                                let model_name = v["model"].as_str().map(|s| s.to_string());
                                chunks.push(StreamChunk {
                                    delta_text: None,
                                    tool_calls: vec![],
                                    finish_reason: Some("stop".to_string()),
                                    usage,
                                    model: model_name,
                                    thought_parts: vec![],
                                    thinking_text: None,
                                });
                                self.circuit.record_success();
                                return Ok(chunks);
                            }
                            _ => {} // Ignore other event types
                        }
                    }
                }
            }

            self.circuit.record_success();
            return Ok(chunks);
        }

        match last_status {
            0 => Err(ProviderError::Transport(last_error)),
            429 => Err(ProviderError::RateLimited {
                message: last_error,
                retry_after_secs: retry_after,
            }),
            s => Err(ProviderError::Api {
                status: s,
                message: last_error,
            }),
        }
    }

    /// Parse a single SSE data line from an OpenAI-compatible stream.
    fn parse_sse_chunk(data: &str) -> Option<StreamChunk> {
        if data == "[DONE]" {
            return None;
        }

        let v: Value = serde_json::from_str(data).ok()?;

        // Extract the actual model name returned by the API
        let model = v["model"].as_str().map(|s| s.to_string());

        let choice = v["choices"].get(0)?;
        let delta = &choice["delta"];
        let finish_reason = choice["finish_reason"].as_str().map(|s| s.to_string());

        let delta_text = delta["content"].as_str().map(|s| s.to_string());

        // OpenAI reasoning models (o1, o3, o4-mini) emit reasoning in a separate field
        let thinking_text = delta
            .get("reasoning_content")
            .or_else(|| delta.get("reasoning"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let mut tool_calls = Vec::new();
        if let Some(tcs) = delta["tool_calls"].as_array() {
            for tc in tcs {
                let index = tc["index"].as_u64().unwrap_or(0) as usize;
                let id = tc["id"].as_str().map(|s| s.to_string());
                let func = &tc["function"];
                let function_name = func["name"].as_str().map(|s| s.to_string());
                let arguments_delta = func["arguments"].as_str().map(|s| s.to_string());
                if id.is_some() || function_name.is_some() {
                    log::debug!(
                        "[sse-debug] tool_call delta: index={} id={:?} name={:?}",
                        index,
                        id,
                        function_name
                    );
                }
                tool_calls.push(ToolCallDelta {
                    index,
                    id,
                    function_name,
                    arguments_delta,
                    thought_signature: None,
                });
            }
        }

        // Parse usage from the final chunk (OpenAI includes it when
        // stream_options.include_usage is set, and also in the last chunk
        // of standard streams).
        let usage = v.get("usage").and_then(|u| {
            let input = u["prompt_tokens"].as_u64().unwrap_or(0);
            let output = u["completion_tokens"].as_u64().unwrap_or(0);
            if input > 0 || output > 0 {
                Some(TokenUsage {
                    input_tokens: input,
                    output_tokens: output,
                    total_tokens: u["total_tokens"].as_u64().unwrap_or(input + output),
                    ..Default::default()
                })
            } else {
                None
            }
        });

        Some(StreamChunk {
            delta_text,
            tool_calls,
            finish_reason,
            usage,
            model,
            thought_parts: vec![],
            thinking_text,
        })
    }
}

// ── AiProvider implementation ──────────────────────────────────────────────

#[async_trait]
impl AiProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn kind(&self) -> ProviderKind {
        self.provider_kind
    }

    /// Send a chat completion request with SSE streaming.
    /// Handles Azure OpenAI (api-key header + api-version query param) and
    /// standard OpenAI-compatible APIs (Bearer token).
    async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        model: &str,
        temperature: Option<f64>,
        thinking_level: Option<&str>,
    ) -> Result<Vec<StreamChunk>, ProviderError> {
        // Responses API models (o3-pro, etc.) use a completely different
        // request/response format — delegate to the dedicated method.
        if self.is_responses_api {
            return self
                .chat_stream_responses(messages, tools, model, temperature, thinking_level)
                .await;
        }

        let url = if self.is_azure {
            if self.base_url.contains("/chat/completions") {
                // Full endpoint URL — already normalised in constructor.
                // Preserves the user's api-version and path.
                self.base_url.clone()
            } else {
                // Legacy base URL (e.g. /models) — append path.
                let base = self.base_url.trim_end_matches('/');
                format!("{}/chat/completions?api-version=2025-03-01-preview", base)
            }
        } else {
            format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
        };

        let mut body = json!({
            "model": model,
            "messages": Self::format_messages(messages),
            "stream": true,
            "stream_options": {"include_usage": true},
            "max_completion_tokens": 8192,
        });

        if !tools.is_empty() {
            let constraint_config = constrained::detect_constraints(self.provider_kind, model);
            let mut formatted_tools = json!(Self::format_tools(tools));

            // Apply strict mode for OpenAI Structured Outputs (gpt-4o, o1, o3, etc.)
            if let Some(arr) = formatted_tools.as_array_mut() {
                constrained::apply_openai_strict(arr, &constraint_config);
            }

            body["tools"] = formatted_tools;

            // Apply Ollama JSON format mode to constrain output to valid JSON
            constrained::apply_ollama_json_format(&mut body, &constraint_config);

            if constraint_config.strict_tools {
                info!(
                    "[engine] OpenAI constrained decoding: strict=true for model={}",
                    model
                );
            } else if constraint_config.json_format {
                info!(
                    "[engine] Ollama constrained decoding: format=json for model={}",
                    model
                );
            }
        }
        if let Some(temp) = temperature {
            if !is_reasoning_model(model) {
                body["temperature"] = json!(temp);
            }
        }

        // OpenAI reasoning models (o1, o3, o4-mini) support reasoning_effort
        if let Some(level) = thinking_level {
            let effort = match level {
                "low" => "low",
                "high" => "high",
                _ => "medium",
            };
            body["reasoning_effort"] = json!(effort);
        }

        info!("[engine] OpenAI request to {} model={}", url, model);

        // Circuit breaker: reject immediately if too many recent failures
        if let Err(msg) = self.circuit.check() {
            return Err(ProviderError::Transport(msg));
        }

        // Retry loop for transient errors
        let mut last_error = String::new();
        let mut last_status: u16 = 0;
        let mut retry_after: Option<u64> = None;

        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let delay = retry_delay(attempt - 1, retry_after.take()).await;
                warn!(
                    "[engine] OpenAI retry {}/{} after {}ms",
                    attempt,
                    MAX_RETRIES,
                    delay.as_millis()
                );
            }

            // Azure uses api-key header; everyone else uses Bearer token
            let mut req = self
                .client
                .post(&url)
                .header("Content-Type", "application/json");
            if self.is_azure {
                req = req.header("api-key", self.api_key.as_str());
            } else {
                req = req.header("Authorization", format!("Bearer {}", self.api_key.as_str()));
            }

            // Sign the outbound request body for tamper detection
            let body_bytes = serde_json::to_vec(&body).unwrap_or_default();
            sign_and_log_request("openai", model, &body_bytes);

            let response = match req.json(&body).send().await {
                Ok(r) => {
                    update_last_audit_status(r.status().as_u16());
                    r
                }
                Err(e) => {
                    self.circuit.record_failure();
                    last_error = format!("HTTP request failed: {}", e);
                    last_status = 0;
                    if attempt < MAX_RETRIES {
                        continue;
                    }
                    return Err(ProviderError::Transport(last_error));
                }
            };

            if !response.status().is_success() {
                let status = response.status().as_u16();
                last_status = status;
                // Parse Retry-After header before consuming body
                retry_after = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(parse_retry_after);
                let body_text = response.text().await.unwrap_or_default();
                last_error = format!(
                    "API error {} at {}: {}",
                    status,
                    url,
                    crate::engine::types::truncate_utf8(&body_text, 200)
                );
                error!(
                    "[engine] OpenAI error {} — url={} model={}: {}",
                    status,
                    url,
                    model,
                    crate::engine::types::truncate_utf8(&body_text, 500)
                );

                self.circuit.record_failure();

                // Auth errors are never retried
                if status == 401 || status == 403 {
                    return Err(ProviderError::Auth(last_error));
                }
                if is_retryable_status(status) && attempt < MAX_RETRIES {
                    continue;
                }
                // Non-retryable API error or retries exhausted
                return if status == 429 {
                    Err(ProviderError::RateLimited {
                        message: last_error,
                        retry_after_secs: retry_after.take(),
                    })
                } else {
                    Err(ProviderError::Api {
                        status,
                        message: last_error,
                    })
                };
            }

            // ── Read SSE stream ─────────────────────────────────────────
            // Accumulate raw bytes to avoid `from_utf8_lossy` corrupting
            // multi-byte UTF-8 sequences that span TCP packet boundaries.
            let mut chunks = Vec::new();
            let mut byte_stream = response.bytes_stream();
            let mut raw_buf: Vec<u8> = Vec::new();

            while let Some(result) = byte_stream.next().await {
                let bytes = result
                    .map_err(|e| ProviderError::Transport(format!("Stream read error: {}", e)))?;
                raw_buf.extend_from_slice(&bytes);

                // Process complete SSE lines (delimited by \n)
                while let Some(pos) = raw_buf.iter().position(|&b| b == b'\n') {
                    let line_bytes = raw_buf[..pos].to_vec();
                    raw_buf = raw_buf[pos + 1..].to_vec();

                    // Convert the complete line to UTF-8 (lossless for valid data)
                    let line = match std::str::from_utf8(&line_bytes) {
                        Ok(s) => s.trim().to_string(),
                        Err(_) => continue, // skip malformed lines
                    };

                    if let Some(data) = line.strip_prefix("data: ") {
                        if let Some(chunk) = Self::parse_sse_chunk(data) {
                            chunks.push(chunk);
                        } else if data == "[DONE]" {
                            self.circuit.record_success();
                            return Ok(chunks);
                        }
                    }
                }
            }

            self.circuit.record_success();
            return Ok(chunks);
        }

        // All retries exhausted — classify the last error
        match last_status {
            0 => Err(ProviderError::Transport(last_error)),
            429 => Err(ProviderError::RateLimited {
                message: last_error,
                retry_after_secs: retry_after,
            }),
            s => Err(ProviderError::Api {
                status: s,
                message: last_error,
            }),
        }
    }

    /// List available models from the provider.
    /// For Azure AI Foundry this calls `GET /models?api-version=…` which
    /// returns all deployed models in the resource.
    /// For Ollama this calls `GET /api/tags`.
    /// For other OpenAI-compatible APIs this calls `GET /models`.
    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        let url = if self.is_azure {
            // Reconstruct the models list URL from the stored endpoint.
            let base = &self.base_url;
            let api_version = base
                .split("api-version=")
                .nth(1)
                .and_then(|v| v.split('&').next())
                .unwrap_or("2025-03-01-preview");

            if base.contains("/models/chat/completions") {
                // Unified inference — strip /chat/completions to get /models
                let models_base = base.split("/chat/completions").next().unwrap_or(base);
                let models_base = models_base.split('?').next().unwrap_or(models_base);
                format!(
                    "{}?api-version={}",
                    models_base.trim_end_matches('/'),
                    api_version
                )
            } else if base.contains("/openai/deployments/") {
                // Classic Azure OpenAI — use /openai/models endpoint
                let host = base
                    .split("/openai")
                    .next()
                    .unwrap_or(base)
                    .trim_end_matches('/');
                format!("{}/openai/models?api-version={}", host, api_version)
            } else {
                // Fallback — base is already /models or similar
                let clean = base.split('?').next().unwrap_or(base).trim_end_matches('/');
                format!("{}?api-version={}", clean, api_version)
            }
        } else if self.provider_kind == ProviderKind::Ollama {
            // Ollama's model list endpoint is /api/tags, not /v1/models
            let base = self.base_url.trim_end_matches('/');
            let host = base.split("/v1").next().unwrap_or(base);
            format!("{}/api/tags", host)
        } else {
            let base = self.base_url.trim_end_matches('/');
            format!("{}/models", base)
        };

        info!("[engine] Listing models from {}", url);

        let mut req = self.client.get(&url);
        if self.is_azure {
            req = req.header("api-key", self.api_key.as_str());
        } else {
            req = req.header("Authorization", format!("Bearer {}", self.api_key.as_str()));
        }

        let response = req
            .send()
            .await
            .map_err(|e| ProviderError::Transport(format!("list_models request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::Api {
                status,
                message: format!("list_models error: {}", body),
            });
        }

        let body: Value = response
            .json()
            .await
            .map_err(|e| ProviderError::Transport(format!("list_models parse error: {}", e)))?;

        let mut models = Vec::new();

        // Azure AI Foundry returns { "data": [ { "id": "...", ... } ] }
        // OpenAI returns the same format.
        // Ollama returns { "models": [ { "name": "...", ... } ] }
        let items = body["data"]
            .as_array()
            .or_else(|| body["models"].as_array());

        if let Some(arr) = items {
            for item in arr {
                let id = item["id"]
                    .as_str()
                    .or_else(|| item["name"].as_str())
                    .unwrap_or_default()
                    .to_string();
                if id.is_empty() {
                    continue;
                }
                let name = item["name"]
                    .as_str()
                    .or_else(|| item["id"].as_str())
                    .unwrap_or(&id)
                    .to_string();
                models.push(ModelInfo {
                    id: id.clone(),
                    name,
                    context_window: item["context_window"].as_u64(),
                    max_output: item["max_output"].as_u64(),
                });
            }
        }

        info!("[engine] Found {} models", models.len());
        Ok(models)
    }
}
