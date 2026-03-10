// pawz-code — provider.rs
// Streaming LLM calls for Anthropic (Claude) and OpenAI-compatible APIs.
// Returns a LlmResult with the final text, tool calls, and usage.

use crate::claude_code;
use crate::config::Config;
use crate::types::{FunctionCall, LlmResult, Message, ToolCall, ToolDef, TokenUsage};
use anyhow::{bail, Result};
use futures::StreamExt;
use serde_json::Value;

// ── Public entry point ───────────────────────────────────────────────────────

/// Call the configured LLM provider with streaming. Calls `on_delta` for every
/// text token. Returns the complete LlmResult when the stream is done.
///
/// Provider dispatch:
/// - "claude_code"        — Claude Code CLI subprocess (no API key required)
/// - "openai"             — OpenAI Chat Completions API
/// - "openai-compatible"  — Any OpenAI-compatible API (Ollama, OpenRouter, etc.)
/// - anything else        — Anthropic Messages API (default)
pub async fn call_streaming(
    config: &Config,
    client: &reqwest::Client,
    system: &str,
    messages: &[Message],
    tools: &[ToolDef],
    on_delta: impl Fn(&str) + Send + Sync,
    model_override: Option<&str>,
) -> Result<LlmResult> {
    // Build an ephemeral config with the resolved model so the inner
    // functions don't need to know about role routing.
    let effective_config;
    let cfg = if let Some(m) = model_override {
        effective_config = Config { model: m.to_owned(), ..config.clone() };
        &effective_config
    } else {
        config
    };

    match cfg.provider.as_str() {
        "claude_code" => {
            claude_code::call_claude_code(cfg, system, messages, tools, on_delta).await
        }
        "openai" | "openai-compatible" => {
            call_openai(cfg, client, system, messages, tools, on_delta).await
        }
        _ => call_anthropic(cfg, client, system, messages, tools, on_delta).await,
    }
}

// ── Message conversion helpers ───────────────────────────────────────────────

/// Convert internal messages to Anthropic API format.
fn to_anthropic_messages(messages: &[Message]) -> Vec<Value> {
    messages
        .iter()
        .map(|m| {
            let content: Vec<Value> = m
                .blocks
                .iter()
                .map(|b| match b {
                    crate::types::ContentBlock::Text { text } => {
                        serde_json::json!({"type": "text", "text": text})
                    }
                    crate::types::ContentBlock::ToolUse { id, name, input } => {
                        serde_json::json!({"type": "tool_use", "id": id, "name": name, "input": input})
                    }
                    crate::types::ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    } => {
                        serde_json::json!({
                            "type": "tool_result",
                            "tool_use_id": tool_use_id,
                            "content": content,
                            "is_error": is_error
                        })
                    }
                })
                .collect();

            // Collapse single text block to plain string for efficiency
            if content.len() == 1 {
                if let Some(text) = content[0]["text"].as_str() {
                    return serde_json::json!({"role": m.role, "content": text});
                }
            }
            serde_json::json!({"role": m.role, "content": content})
        })
        .collect()
}

/// Convert internal messages to OpenAI API format.
fn to_openai_messages(system: &str, messages: &[Message]) -> Vec<Value> {
    let mut out = vec![serde_json::json!({"role": "system", "content": system})];
    for m in messages {
        // Check if this message contains tool_use blocks (assistant) or tool_result blocks (user)
        let has_tool_use = m.blocks.iter().any(|b| matches!(b, crate::types::ContentBlock::ToolUse { .. }));
        let has_tool_result = m.blocks.iter().any(|b| matches!(b, crate::types::ContentBlock::ToolResult { .. }));

        if has_tool_use {
            // OpenAI: assistant message with tool_calls
            let text: String = m
                .blocks
                .iter()
                .filter_map(|b| {
                    if let crate::types::ContentBlock::Text { text } = b {
                        Some(text.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            let tool_calls: Vec<Value> = m
                .blocks
                .iter()
                .filter_map(|b| {
                    if let crate::types::ContentBlock::ToolUse { id, name, input } = b {
                        Some(serde_json::json!({
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": serde_json::to_string(input).unwrap_or_default()
                            }
                        }))
                    } else {
                        None
                    }
                })
                .collect();
            out.push(serde_json::json!({
                "role": "assistant",
                "content": if text.is_empty() { Value::Null } else { Value::String(text) },
                "tool_calls": tool_calls
            }));
        } else if has_tool_result {
            // OpenAI: one tool message per result
            for b in &m.blocks {
                if let crate::types::ContentBlock::ToolResult { tool_use_id, content, .. } = b {
                    out.push(serde_json::json!({
                        "role": "tool",
                        "tool_call_id": tool_use_id,
                        "content": content
                    }));
                }
            }
        } else {
            // Plain text message
            let text = m
                .blocks
                .iter()
                .filter_map(|b| {
                    if let crate::types::ContentBlock::Text { text } = b {
                        Some(text.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            out.push(serde_json::json!({"role": m.role, "content": text}));
        }
    }
    out
}

fn tools_to_anthropic(tools: &[ToolDef]) -> Vec<Value> {
    tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.parameters
            })
        })
        .collect()
}

fn tools_to_openai(tools: &[ToolDef]) -> Vec<Value> {
    tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters
                }
            })
        })
        .collect()
}

// ── Anthropic streaming ──────────────────────────────────────────────────────

async fn call_anthropic(
    config: &Config,
    client: &reqwest::Client,
    system: &str,
    messages: &[Message],
    tools: &[ToolDef],
    on_delta: impl Fn(&str) + Send + Sync,
) -> Result<LlmResult> {
    let base_url = config
        .base_url
        .as_deref()
        .unwrap_or("https://api.anthropic.com");

    let body = serde_json::json!({
        "model": config.model,
        "max_tokens": 8192,
        "system": system,
        "messages": to_anthropic_messages(messages),
        "tools": tools_to_anthropic(tools),
        "stream": true
    });

    let resp = client
        .post(format!("{}/v1/messages", base_url))
        .header("x-api-key", &config.api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let err_body = resp.text().await.unwrap_or_default();
        bail!("Anthropic API error {}: {}", status, err_body);
    }

    // --- Parse SSE stream ---
    // content blocks indexed by position
    let mut text_blocks: std::collections::HashMap<usize, String> = std::collections::HashMap::new();
    let mut tool_blocks: std::collections::HashMap<usize, AnthropicToolBlock> =
        std::collections::HashMap::new();
    let mut stop_reason = String::from("end_turn");
    let mut usage = None;
    let mut model = None;
    let mut full_text = String::new();

    let mut stream = resp.bytes_stream();
    let mut buf = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buf.push_str(&String::from_utf8_lossy(&chunk));

        loop {
            if let Some(nl) = buf.find('\n') {
                let line = buf[..nl].trim().to_string();
                buf = buf[nl + 1..].to_string();

                if line.is_empty() || line.starts_with(':') {
                    continue;
                }

                // SSE format: "data: {...}" — ignore lines with only "event: ..."
                if let Some(json_str) = line.strip_prefix("data: ") {
                    if json_str == "[DONE]" {
                        break;
                    }
                    if let Ok(ev) = serde_json::from_str::<Value>(json_str) {
                        process_anthropic_event(
                            &ev,
                            &mut text_blocks,
                            &mut tool_blocks,
                            &mut stop_reason,
                            &mut usage,
                            &mut model,
                            &mut full_text,
                            &on_delta,
                        );
                    }
                }
            } else {
                break; // need more data
            }
        }
    }

    // Build tool calls from completed blocks
    let mut tool_calls = Vec::new();
    let mut sorted: Vec<_> = tool_blocks.into_iter().collect();
    sorted.sort_by_key(|(idx, _)| *idx);
    for (_, block) in sorted {
        tool_calls.push(ToolCall {
            id: block.id,
            call_type: "function".into(),
            function: FunctionCall {
                name: block.name,
                arguments: block.input_json,
            },
        });
    }

    Ok(LlmResult {
        text: full_text,
        tool_calls,
        usage,
        model,
        stop_reason,
    })
}

#[derive(Default)]
struct AnthropicToolBlock {
    id: String,
    name: String,
    input_json: String,
}

#[allow(clippy::too_many_arguments)]
fn process_anthropic_event(
    ev: &Value,
    text_blocks: &mut std::collections::HashMap<usize, String>,
    tool_blocks: &mut std::collections::HashMap<usize, AnthropicToolBlock>,
    stop_reason: &mut String,
    usage: &mut Option<TokenUsage>,
    model: &mut Option<String>,
    full_text: &mut String,
    on_delta: &(impl Fn(&str) + Send + Sync),
) {
    let ev_type = ev["type"].as_str().unwrap_or("");

    match ev_type {
        "message_start" => {
            if let Some(m) = ev["message"]["model"].as_str() {
                *model = Some(m.to_string());
            }
            if let Some(input) = ev["message"]["usage"]["input_tokens"].as_u64() {
                *usage = Some(TokenUsage {
                    input_tokens: input,
                    output_tokens: 0,
                    total_tokens: input,
                });
            }
        }
        "content_block_start" => {
            let idx = ev["index"].as_u64().unwrap_or(0) as usize;
            let block = &ev["content_block"];
            match block["type"].as_str().unwrap_or("") {
                "text" => {
                    let init = block["text"].as_str().unwrap_or("").to_string();
                    text_blocks.insert(idx, init);
                }
                "tool_use" => {
                    let tb = AnthropicToolBlock {
                        id: block["id"].as_str().unwrap_or("").to_string(),
                        name: block["name"].as_str().unwrap_or("").to_string(),
                        input_json: String::new(),
                    };
                    tool_blocks.insert(idx, tb);
                }
                _ => {}
            }
        }
        "content_block_delta" => {
            let idx = ev["index"].as_u64().unwrap_or(0) as usize;
            let delta = &ev["delta"];
            match delta["type"].as_str().unwrap_or("") {
                "text_delta" => {
                    let text = delta["text"].as_str().unwrap_or("");
                    full_text.push_str(text);
                    if let Some(block) = text_blocks.get_mut(&idx) {
                        block.push_str(text);
                    }
                    on_delta(text);
                }
                "input_json_delta" => {
                    let partial = delta["partial_json"].as_str().unwrap_or("");
                    if let Some(block) = tool_blocks.get_mut(&idx) {
                        block.input_json.push_str(partial);
                    }
                }
                _ => {}
            }
        }
        "message_delta" => {
            if let Some(reason) = ev["delta"]["stop_reason"].as_str() {
                *stop_reason = reason.to_string();
            }
            if let Some(out_tokens) = ev["usage"]["output_tokens"].as_u64() {
                if let Some(u) = usage.as_mut() {
                    u.output_tokens = out_tokens;
                    u.total_tokens = u.input_tokens + out_tokens;
                }
            }
        }
        _ => {}
    }
}

// ── OpenAI-compatible streaming ──────────────────────────────────────────────

async fn call_openai(
    config: &Config,
    client: &reqwest::Client,
    system: &str,
    messages: &[Message],
    tools: &[ToolDef],
    on_delta: impl Fn(&str) + Send + Sync,
) -> Result<LlmResult> {
    let base_url = config
        .base_url
        .as_deref()
        .unwrap_or("https://api.openai.com");

    let mut body = serde_json::json!({
        "model": config.model,
        "messages": to_openai_messages(system, messages),
        "stream": true,
        "stream_options": {"include_usage": true}
    });

    if !tools.is_empty() {
        body["tools"] = serde_json::json!(tools_to_openai(tools));
        body["tool_choice"] = serde_json::json!("auto");
    }

    let resp = client
        .post(format!("{}/v1/chat/completions", base_url))
        .header(
            "Authorization",
            format!("Bearer {}", config.api_key),
        )
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let err_body = resp.text().await.unwrap_or_default();
        bail!("OpenAI API error {}: {}", status, err_body);
    }

    let mut full_text = String::new();
    let mut stop_reason = String::from("stop");
    let mut usage = None;
    let mut model = None;

    // Accumulate tool calls by index
    let mut tool_acc: std::collections::HashMap<usize, OpenAiToolAcc> =
        std::collections::HashMap::new();

    let mut stream = resp.bytes_stream();
    let mut buf = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buf.push_str(&String::from_utf8_lossy(&chunk));

        loop {
            if let Some(nl) = buf.find('\n') {
                let line = buf[..nl].trim().to_string();
                buf = buf[nl + 1..].to_string();

                if line.is_empty() {
                    continue;
                }
                if let Some(json_str) = line.strip_prefix("data: ") {
                    if json_str == "[DONE]" {
                        break;
                    }
                    if let Ok(ev) = serde_json::from_str::<Value>(json_str) {
                        if let Some(m) = ev["model"].as_str() {
                            model = Some(m.to_string());
                        }
                        if let Some(choices) = ev["choices"].as_array() {
                            for choice in choices {
                                let delta = &choice["delta"];
                                if let Some(text) = delta["content"].as_str() {
                                    if !text.is_empty() {
                                        full_text.push_str(text);
                                        on_delta(text);
                                    }
                                }
                                if let Some(tcs) = delta["tool_calls"].as_array() {
                                    for tc in tcs {
                                        let idx = tc["index"].as_u64().unwrap_or(0) as usize;
                                        let acc = tool_acc
                                            .entry(idx)
                                            .or_insert_with(OpenAiToolAcc::default);
                                        if let Some(id) = tc["id"].as_str() {
                                            acc.id = id.to_string();
                                        }
                                        if let Some(name) = tc["function"]["name"].as_str() {
                                            acc.name = name.to_string();
                                        }
                                        if let Some(args) = tc["function"]["arguments"].as_str() {
                                            acc.arguments.push_str(args);
                                        }
                                    }
                                }
                                if let Some(reason) = choice["finish_reason"].as_str() {
                                    if !reason.is_empty() {
                                        stop_reason = reason.to_string();
                                    }
                                }
                            }
                        }
                        if let Some(u) = ev["usage"].as_object() {
                            let inp = u
                                .get("prompt_tokens")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            let out = u
                                .get("completion_tokens")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            usage = Some(TokenUsage {
                                input_tokens: inp,
                                output_tokens: out,
                                total_tokens: inp + out,
                            });
                        }
                    }
                }
            } else {
                break;
            }
        }
    }

    let mut tool_calls = Vec::new();
    let mut sorted: Vec<_> = tool_acc.into_iter().collect();
    sorted.sort_by_key(|(idx, _)| *idx);
    for (_, acc) in sorted {
        if !acc.name.is_empty() {
            tool_calls.push(ToolCall {
                id: acc.id,
                call_type: "function".into(),
                function: FunctionCall {
                    name: acc.name,
                    arguments: acc.arguments,
                },
            });
        }
    }

    Ok(LlmResult {
        text: full_text,
        tool_calls,
        usage,
        model,
        stop_reason,
    })
}

#[derive(Default)]
struct OpenAiToolAcc {
    id: String,
    name: String,
    arguments: String,
}
