// Paw Agent Engine — Google Gemini Provider
// Implements the AiProvider golden trait.
// Preserves the two-pass thought-part parsing for Gemini thinking models.

use crate::engine::types::*;
use crate::atoms::traits::{AiProvider, ProviderError};
use crate::engine::providers::openai::{MAX_RETRIES, is_retryable_status, retry_delay, parse_retry_after};
use crate::engine::http::CircuitBreaker;
use async_trait::async_trait;
use futures::StreamExt;
use log::{info, warn, error};
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::LazyLock;

/// Circuit breaker shared across all Google/Gemini requests.
static GOOGLE_CIRCUIT: LazyLock<CircuitBreaker> = LazyLock::new(|| CircuitBreaker::new(5, 60));

// ── Struct ────────────────────────────────────────────────────────────────────

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
            client: Client::builder()
                .connect_timeout(std::time::Duration::from_secs(10))
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .unwrap_or_default(),
            base_url,
            api_key: config.api_key.clone(),
        }
    }

    fn format_messages(messages: &[Message]) -> (Option<Value>, Vec<Value>) {
        let mut system_instruction: Option<Value> = None;
        let mut contents: Vec<Value> = Vec::new();

        for msg in messages {
            if msg.role == Role::System {
                // Merge multiple system messages into one systemInstruction
                let text = msg.content.as_text();
                if let Some(ref mut existing) = system_instruction {
                    // Append to existing system instruction
                    let prev_text = existing["parts"][0]["text"]
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    let merged = format!("{}\n\n{}", prev_text, text);
                    existing["parts"][0]["text"] = json!(merged);
                } else {
                    system_instruction = Some(json!({
                        "parts": [{"text": text}]
                    }));
                }
                continue;
            }

            let role = match msg.role {
                Role::User | Role::Tool => "user",
                Role::Assistant => "model",
                _ => "user",
            };

            if msg.role == Role::Tool {
                if let Some(tc_id) = &msg.tool_call_id {
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
                    // Echo back thought parts (from thinking models) before functionCall parts
                    for tc in tool_calls {
                        for tp in &tc.thought_parts {
                            let mut thought_part = json!({
                                "thought": true,
                                "text": tp.text,
                            });
                            if !tp.thought_signature.is_empty() {
                                thought_part["thoughtSignature"] = json!(tp.thought_signature);
                            }
                            parts.push(thought_part);
                        }
                    }
                    for tc in tool_calls {
                        let args: Value = serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(json!({}));
                        let mut fc_part = json!({
                            "functionCall": {
                                "name": tc.function.name,
                                "args": args,
                            }
                        });
                        if let Some(sig) = &tc.thought_signature {
                            fc_part["thoughtSignature"] = json!(sig);
                        }
                        parts.push(fc_part);
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
                                ContentBlock::Document { mime_type, data, name: _ } => {
                                    // Gemini supports PDFs natively via inlineData
                                    parts.push(json!({
                                        "inlineData": {
                                            "mimeType": mime_type,
                                            "data": data,
                                        }
                                    }));
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

        // ── Merge consecutive same-role messages ──────────────────────
        // Gemini requires strictly alternating user/model turns.
        // Consecutive user or model messages cause INVALID_ARGUMENT 400.
        let mut merged: Vec<Value> = Vec::new();
        for entry in contents {
            let entry_role = entry["role"].as_str().unwrap_or("").to_string();
            let can_merge = !merged.is_empty()
                && merged.last().and_then(|e| e["role"].as_str()).map(|r| r == entry_role).unwrap_or(false)
                && entry_role != "function"; // never merge function responses

            if can_merge {
                // Merge parts into the previous entry
                if let Some(last) = merged.last_mut() {
                    if let (Some(existing_parts), Some(new_parts)) = (
                        last["parts"].as_array().cloned(),
                        entry["parts"].as_array(),
                    ) {
                        let mut combined = existing_parts;
                        combined.extend(new_parts.iter().cloned());
                        last["parts"] = json!(combined);
                    }
                }
            } else {
                merged.push(entry);
            }
        }

        (system_instruction, merged)
    }

    /// Strip schema fields that Gemini doesn't support and fix invalid patterns.
    ///
    /// Gemini (especially Flash Lite) rejects:
    /// - `additionalProperties`, `$schema`, `$ref`
    /// - `"required": []` (empty array — must be omitted)
    /// - `"properties": {}` when `type: "object"` (needs at least one prop)
    fn sanitize_schema(val: &Value) -> Value {
        match val {
            Value::Object(map) => {
                let mut clean = serde_json::Map::new();
                for (k, v) in map {
                    // Gemini rejects these OpenAPI fields
                    if k == "additionalProperties" || k == "$schema" || k == "$ref" {
                        continue;
                    }
                    // Strip empty "required": [] — Gemini rejects this
                    if k == "required" {
                        if let Value::Array(arr) = v {
                            if arr.is_empty() {
                                continue;
                            }
                        }
                    }
                    // Strip empty "properties": {} — Gemini rejects type:object with no props
                    if k == "properties" {
                        if let Value::Object(props) = v {
                            if props.is_empty() {
                                continue;
                            }
                        }
                    }
                    clean.insert(k.clone(), Self::sanitize_schema(v));
                }
                // If we stripped properties from a type:object, also strip the type
                // to let Gemini infer it (otherwise it complains about object with no props)
                if clean.get("type").and_then(|v| v.as_str()) == Some("object")
                    && !clean.contains_key("properties")
                {
                    clean.remove("type");
                }
                Value::Object(clean)
            }
            Value::Array(arr) => Value::Array(arr.iter().map(Self::sanitize_schema).collect()),
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

    /// Inner implementation with full SSE + retry logic + error classification.
    async fn chat_stream_inner(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        model: &str,
        temperature: Option<f64>,
    ) -> Result<Vec<StreamChunk>, ProviderError> {
        let url = format!(
            "{}/models/{}:streamGenerateContent?alt=sse&key={}",
            self.base_url.trim_end_matches('/'),
            model,
            self.api_key
        );

        let (system_instruction, mut contents) = Self::format_messages(messages);

        // Guard: Gemini requires at least one content entry.
        // After heavy context truncation (large system prompt + community skills),
        // contents can be empty, causing 400 "contents is not specified".
        if contents.is_empty() {
            contents.push(json!({
                "role": "user",
                "parts": [{"text": "Hello"}]
            }));
            warn!("[engine] Google: contents was empty after formatting, injected fallback user message");
        }

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

        // Circuit breaker: reject immediately if too many recent failures
        if let Err(msg) = GOOGLE_CIRCUIT.check() {
            return Err(ProviderError::Transport(msg));
        }

        let mut last_error = String::new();
        let mut last_status: u16 = 0;
        let mut retry_after: Option<u64> = None;
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let delay = retry_delay(attempt - 1, retry_after.take()).await;
                warn!("[engine] Google retry {}/{} after {}ms", attempt, MAX_RETRIES, delay.as_millis());
            }

            let response = match self.client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    GOOGLE_CIRCUIT.record_failure();
                    last_error = format!("HTTP request failed: {}", e);
                    last_status = 0;
                    if attempt < MAX_RETRIES { continue; }
                    return Err(ProviderError::Transport(last_error));
                }
            };

            if !response.status().is_success() {
                let status = response.status().as_u16();
                last_status = status;
                retry_after = response.headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(parse_retry_after);
                let body_text = response.text().await.unwrap_or_default();
                last_error = format!("API error {}: {}", status, &body_text[..body_text.len().min(200)]);
                error!("[engine] Google error {}: {}", status, &body_text[..body_text.len().min(500)]);

                GOOGLE_CIRCUIT.record_failure();

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

            let mut chunks = Vec::new();
            let mut byte_stream = response.bytes_stream();
            let mut buffer = String::new();

            while let Some(result) = byte_stream.next().await {
                let bytes = result.map_err(|e| {
                    ProviderError::Transport(format!("Stream read error: {}", e))
                })?;
                buffer.push_str(&String::from_utf8_lossy(&bytes));

                while let Some(line_end) = buffer.find('\n') {
                    let line = buffer[..line_end].trim().to_string();
                    buffer = buffer[line_end + 1..].to_string();

                    if let Some(data) = line.strip_prefix("data: ") {
                        if let Ok(v) = serde_json::from_str::<Value>(data) {
                            // Extract actual model version from Google's response
                            let api_model = v["modelVersion"].as_str().map(|s| s.to_string());

                            // Parse Google's streaming format
                            let mut fc_index_counter: usize = 0; // unique index per function call
                            if let Some(candidates) = v["candidates"].as_array() {
                                for candidate in candidates {
                                    let content = &candidate["content"];
                                    let finish_reason = candidate["finishReason"].as_str()
                                        .map(|s| s.to_string());

                                    // Detect blocked/empty responses (e.g. SAFETY, RECITATION, OTHER)
                                    if content.is_null() || content["parts"].is_null() {
                                        if let Some(ref reason) = finish_reason {
                                            if reason != "STOP" {
                                                let safety_info = candidate.get("safetyRatings")
                                                    .map(|r| r.to_string())
                                                    .unwrap_or_default();
                                                warn!(
                                                    "[engine] Google: empty response with finishReason={} safety={}",
                                                    reason,
                                                    &safety_info[..safety_info.len().min(300)]
                                                );
                                                // Emit a visible error chunk so the agent loop can surface it
                                                let msg = match reason.as_str() {
                                                    "SAFETY" => "My response was blocked by Google's safety filter. Try rephrasing your request.".to_string(),
                                                    "RECITATION" => "My response was blocked by a recitation filter. Try rephrasing.".to_string(),
                                                    "MAX_TOKENS" => "I ran out of output tokens. Try shortening the conversation or compacting the session.".to_string(),
                                                    "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" =>
                                                        format!("Response blocked ({reason}). Try rephrasing your request."),
                                                    other => format!(
                                                        "The model returned an empty response (reason: {other}). Please retry or rephrase."
                                                    ),
                                                };
                                                chunks.push(StreamChunk {
                                                    delta_text: Some(msg),
                                                    tool_calls: vec![],
                                                    finish_reason: finish_reason.clone(),
                                                    usage: None,
                                                    model: api_model.clone(),
                                                    thought_parts: vec![],
                                                });
                                            }
                                        }
                                        continue;
                                    }

                                    if let Some(parts) = content["parts"].as_array() {
                                        // First pass: collect thought parts (they accompany function calls)
                                        let mut collected_thoughts: Vec<ThoughtPart> = Vec::new();
                                        for part in parts {
                                            if part.get("thought").and_then(|v| v.as_bool()).unwrap_or(false) {
                                                if let (Some(text), Some(sig)) = (
                                                    part["text"].as_str(),
                                                    part.get("thoughtSignature")
                                                        .or_else(|| part.get("thought_signature"))
                                                        .and_then(|v| v.as_str())
                                                ) {
                                                    info!("[engine] Google: captured thought part with signature (len={})", text.len());
                                                    collected_thoughts.push(ThoughtPart {
                                                        text: text.to_string(),
                                                        thought_signature: sig.to_string(),
                                                    });
                                                }
                                            }
                                        }

                                        // Second pass: process text and functionCall parts
                                        for part in parts {
                                            // Skip thought parts (already collected)
                                            if part.get("thought").and_then(|v| v.as_bool()).unwrap_or(false) {
                                                continue;
                                            }
                                            if let Some(text) = part["text"].as_str() {
                                                chunks.push(StreamChunk {
                                                    delta_text: Some(text.to_string()),
                                                    tool_calls: vec![],
                                                    finish_reason: finish_reason.clone(),
                                                    usage: None,
                                                    model: api_model.clone(),
                                                    thought_parts: vec![],
                                                });
                                            }
                                            if let Some(fc) = part.get("functionCall") {
                                                let name = fc["name"].as_str().unwrap_or("").to_string();
                                                let args = fc["args"].clone();
                                                // thought_signature can be at the part level OR inside functionCall
                                                let thought_sig = part.get("thoughtSignature")
                                                    .or_else(|| part.get("thought_signature"))
                                                    .or_else(|| fc.get("thoughtSignature"))
                                                    .or_else(|| fc.get("thought_signature"))
                                                    .and_then(|v| v.as_str())
                                                    .map(|s| s.to_string());
                                                if thought_sig.is_some() {
                                                    info!("[engine] Google: captured thoughtSignature for fn={}", name);
                                                } else {
                                                    warn!("[engine] Google: NO thoughtSignature found for fn={} (part keys: {:?})", name, part.as_object().map(|o| o.keys().collect::<Vec<_>>()));
                                                }
                                                let fc_idx = fc_index_counter;
                                                fc_index_counter += 1;
                                                chunks.push(StreamChunk {
                                                    delta_text: None,
                                                    tool_calls: vec![ToolCallDelta {
                                                        index: fc_idx,
                                                        id: Some(format!("call_{}", uuid::Uuid::new_v4())),
                                                        function_name: Some(name),
                                                        arguments_delta: Some(serde_json::to_string(&args).unwrap_or_default()),
                                                        thought_signature: thought_sig,
                                                    }],
                                                    finish_reason: finish_reason.clone(),
                                                    usage: None,
                                                    model: api_model.clone(),
                                                    // Attach thought parts to the first functionCall chunk
                                                    thought_parts: collected_thoughts.clone(),
                                                });
                                                // Only attach thoughts to first function call chunk
                                                collected_thoughts.clear();
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
                                            ..Default::default()
                                        }),
                                        model: api_model.clone(),
                                        thought_parts: vec![],
                                    });
                                }
                            }
                        }
                    }
                }
            }

            GOOGLE_CIRCUIT.record_success();
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

// ── AiProvider trait implementation ───────────────────────────────────────────

#[async_trait]
impl AiProvider for GoogleProvider {
    fn name(&self) -> &str {
        "google"
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::Google
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        model: &str,
        temperature: Option<f64>,
    ) -> Result<Vec<StreamChunk>, ProviderError> {
        self.chat_stream_inner(messages, tools, model, temperature).await
    }
}
