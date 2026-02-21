// Paw Agent Engine — OpenAI-Compatible Provider
// Handles: OpenAI, OpenRouter, Ollama, Azure OpenAI, and any OpenAI-compatible REST API.
// Implements the AiProvider Golden Trait.

use async_trait::async_trait;
use crate::atoms::traits::{AiProvider, ProviderError};
use crate::engine::types::{
    ContentBlock, Message, MessageContent, ProviderConfig, ProviderKind,
    StreamChunk, ToolCallDelta, ToolDefinition, TokenUsage,
};
use futures::StreamExt;
use log::{info, warn, error};
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

// ── Shared retry utilities ─────────────────────────────────────────────────
// Re-export from engine::http so existing callers (anthropic, google) work
// with `use super::openai::{MAX_RETRIES, ...}` unchanged.

pub(crate) use crate::engine::http::{
    MAX_RETRIES, is_retryable_status, retry_delay, parse_retry_after,
};

// Import the circuit breaker for provider-level use
use crate::engine::http::CircuitBreaker;
use std::sync::LazyLock;

/// Circuit breaker shared across all OpenAI-compatible requests.
static OPENAI_CIRCUIT: LazyLock<CircuitBreaker> = LazyLock::new(|| CircuitBreaker::new(5, 60));

// ── OpenAI provider struct ─────────────────────────────────────────────────

pub struct OpenAiProvider {
    client: Client,
    base_url: String,
    api_key: String,
    is_azure: bool,
}

impl OpenAiProvider {
    pub fn new(config: &ProviderConfig) -> Self {
        let base_url = config.base_url.clone()
            .unwrap_or_else(|| config.kind.default_base_url().to_string());
        let is_azure = base_url.contains(".azure.com");
        OpenAiProvider {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(120))
                .build()
                .unwrap_or_default(),
            base_url,
            api_key: config.api_key.clone(),
            is_azure,
        }
    }

    fn format_messages(messages: &[Message]) -> Vec<Value> {
        messages.iter().map(|msg| {
            let content_val = match &msg.content {
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
            }
            if let Some(id) = &msg.tool_call_id {
                m["tool_call_id"] = json!(id);
            }
            if let Some(name) = &msg.name {
                m["name"] = json!(name);
            }
            m
        }).collect()
    }

    fn format_tools(tools: &[ToolDefinition]) -> Vec<Value> {
        tools.iter().map(|t| {
            json!({
                "type": t.tool_type,
                "function": {
                    "name": t.function.name,
                    "description": t.function.description,
                    "parameters": t.function.parameters,
                }
            })
        }).collect()
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

        let mut tool_calls = Vec::new();
        if let Some(tcs) = delta["tool_calls"].as_array() {
            for tc in tcs {
                let index = tc["index"].as_u64().unwrap_or(0) as usize;
                let id = tc["id"].as_str().map(|s| s.to_string());
                let func = &tc["function"];
                let function_name = func["name"].as_str().map(|s| s.to_string());
                let arguments_delta = func["arguments"].as_str().map(|s| s.to_string());
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
        ProviderKind::OpenAI
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
    ) -> Result<Vec<StreamChunk>, ProviderError> {
        let url = if self.is_azure {
            // Azure AI Services: append api-version query param
            let base = self.base_url.trim_end_matches('/');
            if base.contains('?') {
                format!("{}/chat/completions", base)
            } else {
                format!("{}/chat/completions?api-version=2024-05-01-preview", base)
            }
        } else {
            format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
        };

        let mut body = json!({
            "model": model,
            "messages": Self::format_messages(messages),
            "stream": true,
            "stream_options": {"include_usage": true},
        });

        if !tools.is_empty() {
            body["tools"] = json!(Self::format_tools(tools));
        }
        if let Some(temp) = temperature {
            body["temperature"] = json!(temp);
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
                warn!("[engine] OpenAI retry {}/{} after {}ms", attempt, MAX_RETRIES, delay.as_millis());
            }

            // Azure uses api-key header; everyone else uses Bearer token
            let mut req = self.client
                .post(&url)
                .header("Content-Type", "application/json");
            if self.is_azure {
                req = req.header("api-key", &self.api_key);
            } else {
                req = req.header("Authorization", format!("Bearer {}", self.api_key));
            }

            let response = match req.json(&body).send().await {
                Ok(r) => r,
                Err(e) => {
                    OPENAI_CIRCUIT.record_failure();
                    last_error = format!("HTTP request failed: {}", e);
                    last_status = 0;
                    if attempt < MAX_RETRIES { continue; }
                    return Err(ProviderError::Transport(last_error));
                }
            };

            if !response.status().is_success() {
                let status = response.status().as_u16();
                last_status = status;
                // Parse Retry-After header before consuming body
                retry_after = response.headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(parse_retry_after);
                let body_text = response.text().await.unwrap_or_default();
                last_error = format!("API error {}: {}", status,
                    crate::engine::types::truncate_utf8(&body_text, 200));
                error!("[engine] OpenAI error {}: {}", status,
                    crate::engine::types::truncate_utf8(&body_text, 500));

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
                    Err(ProviderError::Api { status, message: last_error })
                };
            }

            // ── Read SSE stream (exact logic preserved) ──────────────────
            let mut chunks = Vec::new();
            let mut byte_stream = response.bytes_stream();
            let mut buffer = String::new();

            while let Some(result) = byte_stream.next().await {
                let bytes = result.map_err(|e| {
                    ProviderError::Transport(format!("Stream read error: {}", e))
                })?;
                buffer.push_str(&String::from_utf8_lossy(&bytes));

                // Process complete SSE lines
                while let Some(line_end) = buffer.find('\n') {
                    let line = buffer[..line_end].trim().to_string();
                    buffer = buffer[line_end + 1..].to_string();

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
            s => Err(ProviderError::Api { status: s, message: last_error }),
        }
    }
}
