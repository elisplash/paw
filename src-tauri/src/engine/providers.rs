// Paw Agent Engine — AI Provider Clients
// Direct HTTP calls to AI APIs with SSE streaming.
// No WebSocket gateway, no middleman.

use crate::engine::types::*;
use futures::StreamExt;
use log::{info, warn, error};
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

/// Retry configuration for transient API errors.
const MAX_RETRIES: u32 = 3;
const INITIAL_RETRY_DELAY_MS: u64 = 1000;

/// Check if an HTTP status code should be retried.
fn is_retryable_status(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 529)
}

/// Sleep with exponential backoff. Returns delay used.
async fn retry_delay(attempt: u32) -> Duration {
    let delay = Duration::from_millis(INITIAL_RETRY_DELAY_MS * 2u64.pow(attempt));
    tokio::time::sleep(delay).await;
    delay
}

// ── OpenAI-compatible provider ─────────────────────────────────────────
// Works for: OpenAI, OpenRouter, Ollama, any OpenAI-compatible API

pub struct OpenAiProvider {
    client: Client,
    base_url: String,
    api_key: String,
}

impl OpenAiProvider {
    pub fn new(config: &ProviderConfig) -> Self {
        let base_url = config.base_url.clone()
            .unwrap_or_else(|| config.kind.default_base_url().to_string());
        OpenAiProvider {
            client: Client::new(),
            base_url,
            api_key: config.api_key.clone(),
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
                });
            }
        }

        // Parse usage from the final chunk (OpenAI includes it when stream_options.include_usage is set,
        // and also in the last chunk of standard streams)
        let usage = v.get("usage").and_then(|u| {
            let input = u["prompt_tokens"].as_u64().unwrap_or(0);
            let output = u["completion_tokens"].as_u64().unwrap_or(0);
            if input > 0 || output > 0 {
                Some(TokenUsage {
                    input_tokens: input,
                    output_tokens: output,
                    total_tokens: u["total_tokens"].as_u64().unwrap_or(input + output),
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
        })
    }
}

impl OpenAiProvider {
    pub async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        model: &str,
        temperature: Option<f64>,
    ) -> Result<Vec<StreamChunk>, String> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

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

        // Retry loop for transient errors
        let mut last_error = String::new();
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let delay = retry_delay(attempt - 1).await;
                warn!("[engine] OpenAI retry {}/{} after {}ms", attempt, MAX_RETRIES, delay.as_millis());
            }

            let response = match self.client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await {
                    Ok(r) => r,
                    Err(e) => {
                        last_error = format!("HTTP request failed: {}", e);
                        if attempt < MAX_RETRIES { continue; }
                        return Err(last_error);
                    }
                };

            if !response.status().is_success() {
                let status = response.status().as_u16();
                let body_text = response.text().await.unwrap_or_default();
                last_error = format!("API error {}: {}", status, &body_text[..body_text.len().min(200)]);
                error!("[engine] OpenAI error {}: {}", status, &body_text[..body_text.len().min(500)]);
                if is_retryable_status(status) && attempt < MAX_RETRIES {
                    continue;
                }
                return Err(last_error);
            }

            // Read SSE stream
            let mut chunks = Vec::new();
            let mut byte_stream = response.bytes_stream();
            let mut buffer = String::new();

            while let Some(result) = byte_stream.next().await {
                let bytes = result.map_err(|e| format!("Stream read error: {}", e))?;
                buffer.push_str(&String::from_utf8_lossy(&bytes));

                // Process complete SSE lines
                while let Some(line_end) = buffer.find('\n') {
                    let line = buffer[..line_end].trim().to_string();
                    buffer = buffer[line_end + 1..].to_string();

                    if line.starts_with("data: ") {
                        let data = &line[6..];
                        if let Some(chunk) = Self::parse_sse_chunk(data) {
                            chunks.push(chunk);
                        } else if data == "[DONE]" {
                            return Ok(chunks);
                        }
                    }
                }
            }

            return Ok(chunks);
        }

        Err(last_error)
    }
}

// ── Anthropic provider ─────────────────────────────────────────────────

pub struct AnthropicProvider {
    client: Client,
    base_url: String,
    api_key: String,
}

impl AnthropicProvider {
    pub fn new(config: &ProviderConfig) -> Self {
        let base_url = config.base_url.clone()
            .unwrap_or_else(|| config.kind.default_base_url().to_string());
        AnthropicProvider {
            client: Client::new(),
            base_url,
            api_key: config.api_key.clone(),
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
                        let input: Value = serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(json!({}));
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
                                        if let Some((media_type, b64)) = rest.split_once(";base64,") {
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

    fn format_tools(tools: &[ToolDefinition]) -> Vec<Value> {
        tools.iter().map(|t| {
            json!({
                "name": t.function.name,
                "description": t.function.description,
                "input_schema": t.function.parameters,
            })
        }).collect()
    }

    fn parse_sse_event(data: &str) -> Option<StreamChunk> {
        let v: Value = serde_json::from_str(data).ok()?;
        let event_type = v["type"].as_str()?;

        match event_type {
            "content_block_delta" => {
                let delta = &v["delta"];
                let delta_type = delta["type"].as_str().unwrap_or("");
                match delta_type {
                    "text_delta" => {
                        Some(StreamChunk {
                            delta_text: delta["text"].as_str().map(|s| s.to_string()),
                            tool_calls: vec![],
                            finish_reason: None,
                            usage: None,
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
                                arguments_delta: delta["partial_json"].as_str().map(|s| s.to_string()),
                            }],
                            finish_reason: None,
                            usage: None,
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
                        }],
                        finish_reason: None,
                        usage: None,
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
                })
            }
            "message_start" => {
                // Anthropic message_start contains input token count
                let usage = v.get("message").and_then(|m| m.get("usage")).and_then(|u| {
                    let input = u["input_tokens"].as_u64().unwrap_or(0);
                    if input > 0 {
                        Some(TokenUsage {
                            input_tokens: input,
                            output_tokens: 0,
                            total_tokens: input,
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
                })
            }
            "message_stop" => {
                Some(StreamChunk {
                    delta_text: None,
                    tool_calls: vec![],
                    finish_reason: Some("stop".into()),
                    usage: None,
                })
            }
            _ => None,
        }
    }
}

impl AnthropicProvider {
    pub async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        model: &str,
        temperature: Option<f64>,
    ) -> Result<Vec<StreamChunk>, String> {
        let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));

        let (system, formatted_messages) = Self::format_messages(messages);

        let mut body = json!({
            "model": model,
            "messages": formatted_messages,
            "max_tokens": 8192,
            "stream": true,
        });

        if let Some(sys) = system {
            body["system"] = json!(sys);
        }
        if !tools.is_empty() {
            body["tools"] = json!(Self::format_tools(tools));
        }
        if let Some(temp) = temperature {
            body["temperature"] = json!(temp);
        }

        info!("[engine] Anthropic request to {} model={}", url, model);

        // Retry loop for transient errors
        let mut last_error = String::new();
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let delay = retry_delay(attempt - 1).await;
                warn!("[engine] Anthropic retry {}/{} after {}ms", attempt, MAX_RETRIES, delay.as_millis());
            }

            let response = match self.client
                .post(&url)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await {
                    Ok(r) => r,
                    Err(e) => {
                        last_error = format!("HTTP request failed: {}", e);
                        if attempt < MAX_RETRIES { continue; }
                        return Err(last_error);
                    }
                };

            if !response.status().is_success() {
                let status = response.status().as_u16();
                let body_text = response.text().await.unwrap_or_default();
                last_error = format!("API error {}: {}", status, &body_text[..body_text.len().min(200)]);
                error!("[engine] Anthropic error {}: {}", status, &body_text[..body_text.len().min(500)]);
                if is_retryable_status(status) && attempt < MAX_RETRIES {
                    continue;
                }
                return Err(last_error);
            }

            let mut chunks = Vec::new();
            let mut byte_stream = response.bytes_stream();
            let mut buffer = String::new();

            while let Some(result) = byte_stream.next().await {
                let bytes = result.map_err(|e| format!("Stream read error: {}", e))?;
                buffer.push_str(&String::from_utf8_lossy(&bytes));

                while let Some(line_end) = buffer.find('\n') {
                    let line = buffer[..line_end].trim().to_string();
                    buffer = buffer[line_end + 1..].to_string();

                    if line.starts_with("data: ") {
                        let data = &line[6..];
                        if let Some(chunk) = Self::parse_sse_event(data) {
                            chunks.push(chunk);
                        }
                    }
                }
            }

            return Ok(chunks);
        }

        Err(last_error)
    }
}

// ── Google Gemini provider ─────────────────────────────────────────────

pub struct GoogleProvider {
    client: Client,
    base_url: String,
    api_key: String,
}

impl GoogleProvider {
    pub fn new(config: &ProviderConfig) -> Self {
        let base_url = config.base_url.clone()
            .unwrap_or_else(|| config.kind.default_base_url().to_string());
        GoogleProvider {
            client: Client::new(),
            base_url,
            api_key: config.api_key.clone(),
        }
    }

    fn format_messages(messages: &[Message]) -> (Option<Value>, Vec<Value>) {
        let mut system_instruction = None;
        let mut contents = Vec::new();

        for msg in messages {
            if msg.role == Role::System {
                system_instruction = Some(json!({
                    "parts": [{"text": msg.content.as_text()}]
                }));
                continue;
            }

            let role = match msg.role {
                Role::User | Role::Tool => "user",
                Role::Assistant => "model",
                _ => "user",
            };

            if msg.role == Role::Tool {
                if let Some(tc_id) = &msg.tool_call_id {
                    // Find the function name from tool_call_id — use the id as name fallback
                    let fn_name = msg.name.clone().unwrap_or_else(|| tc_id.clone());
                    contents.push(json!({
                        "role": "function",
                        "parts": [{
                            "functionResponse": {
                                "name": fn_name,
                                "response": {
                                    "result": msg.content.as_text()
                                }
                            }
                        }]
                    }));
                }
            } else if msg.role == Role::Assistant {
                if let Some(tool_calls) = &msg.tool_calls {
                    let mut parts: Vec<Value> = vec![];
                    let text = msg.content.as_text();
                    if !text.is_empty() {
                        parts.push(json!({"text": text}));
                    }
                    for tc in tool_calls {
                        let args: Value = serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(json!({}));
                        parts.push(json!({
                            "functionCall": {
                                "name": tc.function.name,
                                "args": args,
                            }
                        }));
                    }
                    contents.push(json!({
                        "role": "model",
                        "parts": parts,
                    }));
                } else {
                    contents.push(json!({
                        "role": role,
                        "parts": [{"text": msg.content.as_text()}]
                    }));
                }
            } else {
                // Handle user messages — support vision (image) blocks
                match &msg.content {
                    MessageContent::Blocks(blocks) => {
                        let mut parts: Vec<Value> = Vec::new();
                        for block in blocks {
                            match block {
                                ContentBlock::Text { text } => {
                                    parts.push(json!({"text": text}));
                                }
                                ContentBlock::ImageUrl { image_url } => {
                                    // Gemini uses inlineData format for base64 images
                                    if let Some(rest) = image_url.url.strip_prefix("data:") {
                                        if let Some((mime_type, b64)) = rest.split_once(";base64,") {
                                            parts.push(json!({
                                                "inlineData": {
                                                    "mimeType": mime_type,
                                                    "data": b64,
                                                }
                                            }));
                                        }
                                    } else {
                                        // External URL — use fileData
                                        parts.push(json!({
                                            "fileData": {
                                                "fileUri": image_url.url,
                                            }
                                        }));
                                    }
                                }
                            }
                        }
                        contents.push(json!({
                            "role": role,
                            "parts": parts,
                        }));
                    }
                    MessageContent::Text(s) => {
                        contents.push(json!({
                            "role": role,
                            "parts": [{"text": s}]
                        }));
                    }
                }
            }
        }

        (system_instruction, contents)
    }

    /// Strip schema fields that Gemini doesn't support (additionalProperties, etc.)
    fn sanitize_schema(val: &Value) -> Value {
        match val {
            Value::Object(map) => {
                let mut clean = serde_json::Map::new();
                for (k, v) in map {
                    // Gemini rejects these OpenAPI fields
                    if k == "additionalProperties" || k == "$schema" || k == "$ref" {
                        continue;
                    }
                    clean.insert(k.clone(), Self::sanitize_schema(v));
                }
                Value::Object(clean)
            }
            Value::Array(arr) => Value::Array(arr.iter().map(|v| Self::sanitize_schema(v)).collect()),
            other => other.clone(),
        }
    }

    fn format_tools(tools: &[ToolDefinition]) -> Value {
        let function_declarations: Vec<Value> = tools.iter().map(|t| {
            json!({
                "name": t.function.name,
                "description": t.function.description,
                "parameters": Self::sanitize_schema(&t.function.parameters),
            })
        }).collect();

        json!([{
            "functionDeclarations": function_declarations
        }])
    }
}

impl GoogleProvider {
    pub async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        model: &str,
        temperature: Option<f64>,
    ) -> Result<Vec<StreamChunk>, String> {
        let url = format!(
            "{}/models/{}:streamGenerateContent?alt=sse&key={}",
            self.base_url.trim_end_matches('/'),
            model,
            self.api_key
        );

        let (system_instruction, contents) = Self::format_messages(messages);

        let mut body = json!({
            "contents": contents,
        });

        if let Some(sys) = system_instruction {
            body["systemInstruction"] = sys;
        }
        if !tools.is_empty() {
            body["tools"] = Self::format_tools(tools);
        }
        if let Some(temp) = temperature {
            body["generationConfig"] = json!({"temperature": temp});
        }

        info!("[engine] Google request model={}", model);

        // Retry loop for transient errors
        let mut last_error = String::new();
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let delay = retry_delay(attempt - 1).await;
                warn!("[engine] Google retry {}/{} after {}ms", attempt, MAX_RETRIES, delay.as_millis());
            }

            let response = match self.client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await {
                    Ok(r) => r,
                    Err(e) => {
                        last_error = format!("HTTP request failed: {}", e);
                        if attempt < MAX_RETRIES { continue; }
                        return Err(last_error);
                    }
                };

            if !response.status().is_success() {
                let status = response.status().as_u16();
                let body_text = response.text().await.unwrap_or_default();
                last_error = format!("API error {}: {}", status, &body_text[..body_text.len().min(200)]);
                error!("[engine] Google error {}: {}", status, &body_text[..body_text.len().min(500)]);
                if is_retryable_status(status) && attempt < MAX_RETRIES {
                    continue;
                }
                return Err(last_error);
            }

            let mut chunks = Vec::new();
            let mut byte_stream = response.bytes_stream();
            let mut buffer = String::new();

            while let Some(result) = byte_stream.next().await {
                let bytes = result.map_err(|e| format!("Stream read error: {}", e))?;
                buffer.push_str(&String::from_utf8_lossy(&bytes));

                while let Some(line_end) = buffer.find('\n') {
                    let line = buffer[..line_end].trim().to_string();
                    buffer = buffer[line_end + 1..].to_string();

                    if line.starts_with("data: ") {
                        let data = &line[6..];
                        if let Ok(v) = serde_json::from_str::<Value>(data) {
                            // Parse Google's streaming format
                            if let Some(candidates) = v["candidates"].as_array() {
                                for candidate in candidates {
                                    let content = &candidate["content"];
                                    let finish_reason = candidate["finishReason"].as_str()
                                        .map(|s| s.to_string());

                                    if let Some(parts) = content["parts"].as_array() {
                                        for part in parts {
                                            if let Some(text) = part["text"].as_str() {
                                                chunks.push(StreamChunk {
                                                    delta_text: Some(text.to_string()),
                                                    tool_calls: vec![],
                                                    finish_reason: finish_reason.clone(),
                                                    usage: None,
                                            });
                                        }
                                        if let Some(fc) = part.get("functionCall") {
                                            let name = fc["name"].as_str().unwrap_or("").to_string();
                                            let args = fc["args"].clone();
                                            chunks.push(StreamChunk {
                                                delta_text: None,
                                                tool_calls: vec![ToolCallDelta {
                                                    index: 0,
                                                    id: Some(format!("call_{}", uuid::Uuid::new_v4())),
                                                    function_name: Some(name),
                                                    arguments_delta: Some(serde_json::to_string(&args).unwrap_or_default()),
                                                }],
                                                finish_reason: finish_reason.clone(),
                                                usage: None,
                                            });
                                        }
                                    }
                                }
                            }
                        }

                        // Gemini reports usage in usageMetadata
                        if let Some(um) = v.get("usageMetadata") {
                            let input = um["promptTokenCount"].as_u64().unwrap_or(0);
                            let output = um["candidatesTokenCount"].as_u64().unwrap_or(0);
                            if input > 0 || output > 0 {
                                chunks.push(StreamChunk {
                                    delta_text: None,
                                    tool_calls: vec![],
                                    finish_reason: None,
                                    usage: Some(TokenUsage {
                                        input_tokens: input,
                                        output_tokens: output,
                                        total_tokens: um["totalTokenCount"].as_u64().unwrap_or(input + output),
                                    }),
                                });
                            }
                        }
                    }
                }
            }
        }

            return Ok(chunks);
        }

        Err(last_error)
    }
}

// ── Provider factory ───────────────────────────────────────────────────

pub enum AnyProvider {
    OpenAi(OpenAiProvider),
    Anthropic(AnthropicProvider),
    Google(GoogleProvider),
}

impl AnyProvider {
    pub fn from_config(config: &ProviderConfig) -> Self {
        match config.kind {
            ProviderKind::OpenAI | ProviderKind::OpenRouter | ProviderKind::Ollama | ProviderKind::Custom => {
                AnyProvider::OpenAi(OpenAiProvider::new(config))
            }
            ProviderKind::Anthropic => {
                AnyProvider::Anthropic(AnthropicProvider::new(config))
            }
            ProviderKind::Google => {
                AnyProvider::Google(GoogleProvider::new(config))
            }
        }
    }

    pub async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        model: &str,
        temperature: Option<f64>,
    ) -> Result<Vec<StreamChunk>, String> {
        match self {
            AnyProvider::OpenAi(p) => p.chat_stream(messages, tools, model, temperature).await,
            AnyProvider::Anthropic(p) => p.chat_stream(messages, tools, model, temperature).await,
            AnyProvider::Google(p) => p.chat_stream(messages, tools, model, temperature).await,
        }
    }

    pub fn kind(&self) -> ProviderKind {
        match self {
            AnyProvider::OpenAi(_) => ProviderKind::OpenAI,
            AnyProvider::Anthropic(_) => ProviderKind::Anthropic,
            AnyProvider::Google(_) => ProviderKind::Google,
        }
    }
}
