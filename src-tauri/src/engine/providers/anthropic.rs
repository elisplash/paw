// Paw Agent Engine — Anthropic Claude Provider
// Implements the AiProvider golden trait.
// All Claude-specific SSE event parsing and prompt-caching logic lives here.

use crate::atoms::traits::{AiProvider, ProviderError};
use crate::engine::http::{
    pinned_client, sign_and_log_request, update_last_audit_status, CircuitBreaker,
};
use crate::engine::providers::openai::{
    is_retryable_status, parse_retry_after, retry_delay, MAX_RETRIES,
};
use crate::engine::types::*;
use async_trait::async_trait;
use futures::StreamExt;
use log::{error, info, warn};
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::LazyLock;
use zeroize::Zeroizing;

/// Circuit breaker shared across all Anthropic requests.
static ANTHROPIC_CIRCUIT: LazyLock<CircuitBreaker> = LazyLock::new(|| CircuitBreaker::new(5, 60));

// ── Struct ────────────────────────────────────────────────────────────────────

pub struct AnthropicProvider {
    client: Client,
    base_url: String,
    /// API key wrapped in Zeroizing<> — automatically zeroed from RAM on drop.
    api_key: Zeroizing<String>,
    is_azure: bool,
}

impl AnthropicProvider {
    pub fn new(config: &ProviderConfig) -> Self {
        let base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| config.kind.default_base_url().to_string());
        let is_azure = base_url.contains(".azure.com");
        AnthropicProvider {
            client: pinned_client(),
            base_url,
            api_key: Zeroizing::new(config.api_key.clone()),
            is_azure,
        }
    }

    fn format_messages(messages: &[Message]) -> (Option<String>, Vec<Value>) {
        let mut system = None;
        let mut formatted = Vec::new();

        for msg in messages {
            if msg.role == Role::System {
                system = Some(msg.content.as_text());
                continue;
            }

            let role = match msg.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::Tool => "user", // Anthropic uses user role for tool results
                _ => "user",
            };

            if msg.role == Role::Tool {
                // Tool results in Anthropic format
                if let Some(tc_id) = &msg.tool_call_id {
                    formatted.push(json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": tc_id,
                            "content": msg.content.as_text(),
                        }]
                    }));
                }
            } else if msg.role == Role::Assistant {
                if let Some(tool_calls) = &msg.tool_calls {
                    // Assistant message with tool use
                    let mut content_blocks: Vec<Value> = vec![];
                    let text = msg.content.as_text();
                    if !text.is_empty() {
                        content_blocks.push(json!({"type": "text", "text": text}));
                    }
                    for tc in tool_calls {
                        let input: Value =
                            serde_json::from_str(&tc.function.arguments).unwrap_or(json!({}));
                        content_blocks.push(json!({
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.function.name,
                            "input": input,
                        }));
                    }
                    formatted.push(json!({
                        "role": "assistant",
                        "content": content_blocks,
                    }));
                } else {
                    formatted.push(json!({
                        "role": role,
                        "content": msg.content.as_text(),
                    }));
                }
            } else {
                // Handle user messages — support vision (image) blocks
                match &msg.content {
                    MessageContent::Blocks(blocks) => {
                        let mut content_blocks: Vec<Value> = Vec::new();
                        for block in blocks {
                            match block {
                                ContentBlock::Text { text } => {
                                    content_blocks.push(json!({"type": "text", "text": text}));
                                }
                                ContentBlock::ImageUrl { image_url } => {
                                    // Anthropic uses base64 source format, not URL
                                    // data:image/png;base64,... → extract media_type and data
                                    if let Some(rest) = image_url.url.strip_prefix("data:") {
                                        if let Some((media_type, b64)) = rest.split_once(";base64,")
                                        {
                                            content_blocks.push(json!({
                                                "type": "image",
                                                "source": {
                                                    "type": "base64",
                                                    "media_type": media_type,
                                                    "data": b64,
                                                }
                                            }));
                                        }
                                    } else {
                                        // Plain URL — use url source type
                                        content_blocks.push(json!({
                                            "type": "image",
                                            "source": {
                                                "type": "url",
                                                "url": image_url.url,
                                            }
                                        }));
                                    }
                                }
                                ContentBlock::Document {
                                    mime_type,
                                    data,
                                    name: _,
                                } => {
                                    // Anthropic supports PDFs natively as document content blocks
                                    content_blocks.push(json!({
                                        "type": "document",
                                        "source": {
                                            "type": "base64",
                                            "media_type": mime_type,
                                            "data": data,
                                        }
                                    }));
                                }
                            }
                        }
                        formatted.push(json!({
                            "role": role,
                            "content": content_blocks,
                        }));
                    }
                    MessageContent::Text(s) => {
                        formatted.push(json!({
                            "role": role,
                            "content": s,
                        }));
                    }
                }
            }
        }

        (system, formatted)
    }

    /// Post-process formatted messages to add cache_control breakpoints
    /// for multi-turn conversations. Marks the second-to-last user turn
    /// so Anthropic caches the conversation prefix (system + tools + history).
    fn add_turn_cache_breakpoints(messages: &mut [Value]) {
        if messages.len() < 4 {
            return;
        } // Need enough turns for caching to matter

        // Find the second-to-last user message (the breakpoint)
        // This ensures all messages up to this point are cached,
        // and only the last user message + response are billed at full rate.
        let mut user_indices: Vec<usize> = Vec::new();
        for (i, msg) in messages.iter().enumerate() {
            if msg["role"].as_str() == Some("user") {
                user_indices.push(i);
            }
        }
        if user_indices.len() < 2 {
            return;
        }
        let breakpoint_idx = user_indices[user_indices.len() - 2];

        // Add cache_control to the last content block of the breakpoint message
        if let Some(msg) = messages.get_mut(breakpoint_idx) {
            if let Some(content) = msg.get_mut("content") {
                if let Some(arr) = content.as_array_mut() {
                    // Add cache_control to last block in the content array
                    if let Some(last_block) = arr.last_mut() {
                        if let Some(obj) = last_block.as_object_mut() {
                            obj.insert("cache_control".into(), json!({"type": "ephemeral"}));
                        }
                    }
                } else if content.is_string() {
                    // Convert string content to content blocks with cache_control
                    let text = content.as_str().unwrap_or("").to_string();
                    *content = json!([{
                        "type": "text",
                        "text": text,
                        "cache_control": {"type": "ephemeral"}
                    }]);
                }
            }
        }
    }

    fn format_tools(tools: &[ToolDefinition]) -> Vec<Value> {
        tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.function.name,
                    "description": t.function.description,
                    "input_schema": t.function.parameters,
                })
            })
            .collect()
    }

    fn parse_sse_event(data: &str) -> Option<StreamChunk> {
        let v: Value = serde_json::from_str(data).ok()?;
        let event_type = v["type"].as_str()?;

        match event_type {
            "content_block_delta" => {
                let delta = &v["delta"];
                let delta_type = delta["type"].as_str().unwrap_or("");
                match delta_type {
                    "text_delta" => Some(StreamChunk {
                        delta_text: delta["text"].as_str().map(|s| s.to_string()),
                        tool_calls: vec![],
                        finish_reason: None,
                        usage: None,
                        model: None,
                        thought_parts: vec![],
                        thinking_text: None,
                    }),
                    "thinking_delta" => {
                        // Anthropic extended thinking: stream the reasoning text
                        Some(StreamChunk {
                            delta_text: None,
                            tool_calls: vec![],
                            finish_reason: None,
                            usage: None,
                            model: None,
                            thought_parts: vec![],
                            thinking_text: delta["thinking"].as_str().map(|s| s.to_string()),
                        })
                    }
                    "input_json_delta" => {
                        let index = v["index"].as_u64().unwrap_or(0) as usize;
                        Some(StreamChunk {
                            delta_text: None,
                            tool_calls: vec![ToolCallDelta {
                                index,
                                id: None,
                                function_name: None,
                                arguments_delta: delta["partial_json"]
                                    .as_str()
                                    .map(|s| s.to_string()),
                                thought_signature: None,
                            }],
                            finish_reason: None,
                            usage: None,
                            model: None,
                            thought_parts: vec![],
                            thinking_text: None,
                        })
                    }
                    _ => None,
                }
            }
            "content_block_start" => {
                let block = &v["content_block"];
                let block_type = block["type"].as_str().unwrap_or("");
                if block_type == "tool_use" {
                    let index = v["index"].as_u64().unwrap_or(0) as usize;
                    Some(StreamChunk {
                        delta_text: None,
                        tool_calls: vec![ToolCallDelta {
                            index,
                            id: block["id"].as_str().map(|s| s.to_string()),
                            function_name: block["name"].as_str().map(|s| s.to_string()),
                            arguments_delta: None,
                            thought_signature: None,
                        }],
                        finish_reason: None,
                        usage: None,
                        model: None,
                        thought_parts: vec![],
                        thinking_text: None,
                    })
                } else {
                    None
                }
            }
            "message_delta" => {
                let stop_reason = v["delta"]["stop_reason"].as_str().map(|s| s.to_string());
                // Anthropic reports usage in message_delta
                let usage = v.get("usage").and_then(|u| {
                    let output = u["output_tokens"].as_u64().unwrap_or(0);
                    if output > 0 {
                        Some(TokenUsage {
                            input_tokens: 0, // Anthropic reports input in message_start
                            output_tokens: output,
                            total_tokens: output,
                            ..Default::default()
                        })
                    } else {
                        None
                    }
                });
                Some(StreamChunk {
                    delta_text: None,
                    tool_calls: vec![],
                    finish_reason: stop_reason,
                    usage,
                    model: None,
                    thought_parts: vec![],
                    thinking_text: None,
                })
            }
            "message_start" => {
                // Anthropic message_start contains input token count AND the actual model name
                let msg = v.get("message");
                let model = msg.and_then(|m| m["model"].as_str()).map(|s| s.to_string());
                let usage = msg.and_then(|m| m.get("usage")).and_then(|u| {
                    let input = u["input_tokens"].as_u64().unwrap_or(0);
                    let cache_create = u["cache_creation_input_tokens"].as_u64().unwrap_or(0);
                    let cache_read = u["cache_read_input_tokens"].as_u64().unwrap_or(0);
                    if cache_create > 0 || cache_read > 0 {
                        info!("[engine] Anthropic cache: {} tokens created, {} tokens read (input: {})",
                            cache_create, cache_read, input);
                    }
                    if input > 0 {
                        Some(TokenUsage {
                            input_tokens: input,
                            output_tokens: 0,
                            total_tokens: input,
                            cache_creation_tokens: cache_create,
                            cache_read_tokens: cache_read,
                        })
                    } else {
                        None
                    }
                });
                Some(StreamChunk {
                    delta_text: None,
                    tool_calls: vec![],
                    finish_reason: None,
                    usage,
                    model,
                    thought_parts: vec![],
                    thinking_text: None,
                })
            }
            "message_stop" => Some(StreamChunk {
                delta_text: None,
                tool_calls: vec![],
                finish_reason: Some("stop".into()),
                usage: None,
                model: None,
                thought_parts: vec![],
                thinking_text: None,
            }),
            _ => None,
        }
    }

    /// Inner implementation with full SSE + retry logic + error classification.
    async fn chat_stream_inner(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        model: &str,
        temperature: Option<f64>,
        thinking_level: Option<&str>,
    ) -> Result<Vec<StreamChunk>, ProviderError> {
        let url = if self.is_azure {
            let base = self.base_url.trim_end_matches('/');
            if base.contains('?') {
                format!("{}/v1/messages", base)
            } else {
                format!("{}/v1/messages?api-version=2024-08-01", base)
            }
        } else {
            format!("{}/v1/messages", self.base_url.trim_end_matches('/'))
        };

        let (system, mut formatted_messages) = Self::format_messages(messages);

        // ── Multi-turn conversation caching: mark a breakpoint in the
        // conversation history so Anthropic caches the prefix. This
        // gives ~90% discount on input tokens for the cached portion.
        Self::add_turn_cache_breakpoints(&mut formatted_messages);

        // Model-aware max_tokens via the Engram model capability registry.
        // Replaces the hardcoded 4096/8192 split with accurate per-model values.
        // e.g. claude-opus-4-6 → 32768, claude-3-haiku → 4096, claude-sonnet → 8192.
        let max_tokens = crate::engine::engram::model_caps::resolve_max_output_tokens(model);

        let mut body = json!({
            "model": model,
            "messages": formatted_messages,
            "max_tokens": max_tokens,
            "stream": true,
        });

        // ── Prompt caching: send system prompt as content blocks with
        // cache_control on the last block.  Anthropic caches the entire
        // prefix up to and including the marked block, giving a 90%
        // discount on input tokens for subsequent requests within 5 min.
        if let Some(sys) = system {
            body["system"] = json!([
                {
                    "type": "text",
                    "text": sys,
                    "cache_control": { "type": "ephemeral" }
                }
            ]);
        }
        if !tools.is_empty() {
            let mut tool_list = Self::format_tools(tools);
            // Mark the last tool for caching so the entire tools prefix is
            // included in the cached segment along with the system prompt.
            if let Some(last) = tool_list.last_mut() {
                if let Some(obj) = last.as_object_mut() {
                    obj.insert("cache_control".into(), json!({ "type": "ephemeral" }));
                }
            }
            body["tools"] = json!(tool_list);
        }
        if let Some(temp) = temperature {
            body["temperature"] = json!(temp);
        }

        // Anthropic extended thinking — requires setting a budget and increasing max_tokens
        if let Some(level) = thinking_level {
            if level != "none" {
                let budget = match level {
                    "low" => 4096,
                    "high" => 32768,
                    _ => 16384, // medium
                };
                body["thinking"] = json!({
                    "type": "enabled",
                    "budget_tokens": budget,
                });
                // Extended thinking requires higher max_tokens to include both
                // thinking + response tokens. Override the model-aware max_tokens.
                body["max_tokens"] = json!(budget + max_tokens as i64);
                info!(
                    "[engine] Anthropic: extended thinking enabled (budget={})",
                    budget
                );
            }
        }

        info!("[engine] Anthropic request to {} model={}", url, model);

        // Circuit breaker: reject immediately if too many recent failures
        if let Err(msg) = ANTHROPIC_CIRCUIT.check() {
            return Err(ProviderError::Transport(msg));
        }

        let mut last_error = String::new();
        let mut last_status: u16 = 0;
        let mut retry_after: Option<u64> = None;
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let delay = retry_delay(attempt - 1, retry_after.take()).await;
                warn!(
                    "[engine] Anthropic retry {}/{} after {}ms",
                    attempt,
                    MAX_RETRIES,
                    delay.as_millis()
                );
            }

            let mut req = self
                .client
                .post(&url)
                .header("anthropic-version", "2023-06-01")
                .header("Content-Type", "application/json")
                .header(
                    "anthropic-beta",
                    "prompt-caching-2024-07-31,interleaved-thinking-2025-05-14",
                );
            if self.is_azure {
                req = req.header("api-key", self.api_key.as_str());
            } else {
                req = req.header("x-api-key", self.api_key.as_str());
            }

            // Sign the outbound request body for tamper detection
            let body_bytes = serde_json::to_vec(&body).unwrap_or_default();
            sign_and_log_request("anthropic", model, &body_bytes);

            let response = match req.json(&body).send().await {
                Ok(r) => {
                    update_last_audit_status(r.status().as_u16());
                    r
                }
                Err(e) => {
                    ANTHROPIC_CIRCUIT.record_failure();
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
                    "API error {}: {}",
                    status,
                    crate::engine::types::truncate_utf8(&body_text, 200)
                );
                error!(
                    "[engine] Anthropic error {}: {}",
                    status,
                    crate::engine::types::truncate_utf8(&body_text, 500)
                );

                ANTHROPIC_CIRCUIT.record_failure();

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

            let mut chunks = Vec::new();
            let mut byte_stream = response.bytes_stream();
            let mut buffer = String::new();

            while let Some(result) = byte_stream.next().await {
                let bytes = result
                    .map_err(|e| ProviderError::Transport(format!("Stream read error: {}", e)))?;
                buffer.push_str(&String::from_utf8_lossy(&bytes));

                while let Some(line_end) = buffer.find('\n') {
                    let line = buffer[..line_end].trim().to_string();
                    buffer = buffer[line_end + 1..].to_string();

                    if let Some(data) = line.strip_prefix("data: ") {
                        if let Some(chunk) = Self::parse_sse_event(data) {
                            chunks.push(chunk);
                        }
                    }
                }
            }

            ANTHROPIC_CIRCUIT.record_success();
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
}

// ── AiProvider trait implementation ───────────────────────────────────────────

#[async_trait]
impl AiProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::Anthropic
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        model: &str,
        temperature: Option<f64>,
        thinking_level: Option<&str>,
    ) -> Result<Vec<StreamChunk>, ProviderError> {
        self.chat_stream_inner(messages, tools, model, temperature, thinking_level)
            .await
    }
}
