// Paw Agent Engine — OpenAI-Compatible Provider
// Handles: OpenAI, OpenRouter, Ollama, Azure OpenAI, and any OpenAI-compatible REST API.
// Implements the AiProvider Golden Trait.

use crate::atoms::traits::{AiProvider, ModelInfo, ProviderError};
use crate::engine::types::{
    ContentBlock, Message, MessageContent, ProviderConfig, ProviderKind, StreamChunk, TokenUsage,
    ToolCallDelta, ToolDefinition,
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
use std::sync::LazyLock;

/// Circuit breaker shared across all OpenAI-compatible requests.
static OPENAI_CIRCUIT: LazyLock<CircuitBreaker> = LazyLock::new(|| CircuitBreaker::new(5, 60));

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

        // Azure AI Foundry: normalise the user's URL so it ends with /models.
        // Users may paste the full resource URL, a specific deployment path,
        // or even the whole /models/chat/completions endpoint.  We strip
        // everything after the host and append /models so that
        // chat_stream() can simply append /chat/completions?api-version=….
        if config.kind == ProviderKind::AzureFoundry {
            let trimmed = base_url.trim_end_matches('/');
            // Strip known Azure path suffixes so we're left with the base.
            let host_base = trimmed
                .split("/openai")
                .next()
                .unwrap_or(trimmed)
                .split("/models")
                .next()
                .unwrap_or(trimmed)
                .trim_end_matches('/');
            base_url = format!("{}/models", host_base);
        }

        let is_azure = base_url.contains(".azure.com");
        OpenAiProvider {
            client: pinned_client(),
            base_url,
            api_key: Zeroizing::new(config.api_key.clone()),
            is_azure,
            provider_kind: config.kind,
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
        let url = if self.is_azure {
            // Azure AI Services: append api-version query param
            let base = self.base_url.trim_end_matches('/');
            if base.contains('?') {
                format!("{}/chat/completions", base)
            } else {
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
            body["temperature"] = json!(temp);
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
        if let Err(msg) = OPENAI_CIRCUIT.check() {
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
                    OPENAI_CIRCUIT.record_failure();
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

                OPENAI_CIRCUIT.record_failure();

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
                            OPENAI_CIRCUIT.record_success();
                            return Ok(chunks);
                        }
                    }
                }
            }

            OPENAI_CIRCUIT.record_success();
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
            let base = self.base_url.trim_end_matches('/');
            format!("{}?api-version=2025-03-01-preview", base)
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
