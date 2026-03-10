// pawz-code — claude_code.rs
// Robust subprocess provider for the Claude Code CLI.
//
// Invocation:
//   claude --output-format stream-json --print
//   (prompt piped via stdin)
//
// Design goals (addressed explicitly):
//
// 1. Defensive JSON parsing
//    Buffer stdout bytes and only attempt JSON parsing when a complete frame
//    is detected (i.e. a newline-terminated chunk that starts with '{').
//    A single unparseable chunk never aborts the stream — we skip it and
//    continue reading.
//
// 2. CLI noise stripping
//    The claude CLI writes ANSI escape sequences, carriage returns, spinner
//    characters, and other terminal artefacts to stdout/stderr. We strip all
//    of these before attempting JSON parsing so they never cause parse errors.
//
// 3. Partial chunk handling
//    Raw bytes arriving from the subprocess may be split at arbitrary
//    boundaries. We maintain a byte-level accumulation buffer; we only
//    attempt to parse a segment once we see a terminating newline (or the
//    stream closes with leftover bytes).
//
// 4. Graceful retry
//    If the subprocess exits with an error or yields zero usable text, we
//    retry once before surfacing an error to the caller. The retry is silent
//    from the user's perspective.
//
// 5. Configurable binary path
//    config.claude_binary_path overrides the default ("claude") so users can
//    point at the exact binary returned by `which claude`.

use crate::config::Config;
use crate::types::{LlmResult, Message, ToolDef, TokenUsage};
use anyhow::{bail, Context, Result};
use std::process::Stdio;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

// ── Constants ────────────────────────────────────────────────────────────────

/// Maximum number of silent retries on subprocess failure / zero-text response.
const MAX_RETRIES: u32 = 1;

/// Initial read buffer size (bytes). Grows as needed.
const READ_BUF_SIZE: usize = 8192;

// ── Public entry point ───────────────────────────────────────────────────────

/// Spawn the claude CLI subprocess, parse its stream-json output, and emit
/// text deltas via `on_delta`. Returns the complete LlmResult.
///
/// The reduction pipeline has already run — `system` and `messages` contain
/// the compressed, optimised prompt; we just need to forward it to claude.
pub async fn call_claude_code(
    config: &Config,
    system: &str,
    messages: &[Message],
    _tools: &[ToolDef], // Claude manages its own tool loop
    on_delta: impl Fn(&str) + Send + Sync,
) -> Result<LlmResult> {
    let binary = config
        .claude_binary_path
        .as_deref()
        .unwrap_or("claude")
        .to_owned();

    let prompt = build_prompt(system, messages);

    let mut last_err: Option<anyhow::Error> = None;

    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            log::warn!(
                "[claude_code] attempt {} failed ({}), retrying…",
                attempt,
                last_err
                    .as_ref()
                    .map(|e| e.to_string())
                    .unwrap_or_default()
            );
        }

        match run_once(&binary, config, &prompt, &on_delta).await {
            Ok(result) if !result.text.is_empty() => return Ok(result),
            Ok(_empty) => {
                last_err = Some(anyhow::anyhow!("claude returned empty response"));
            }
            Err(e) => {
                last_err = Some(e);
            }
        }
    }

    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("claude subprocess failed after retries")))
}

// ── Single subprocess invocation ─────────────────────────────────────────────

async fn run_once(
    binary: &str,
    config: &Config,
    prompt: &str,
    on_delta: &(impl Fn(&str) + Send + Sync),
) -> Result<LlmResult> {
    let mut cmd = Command::new(binary);
    cmd.arg("--output-format")
        .arg("stream-json")
        .arg("--print") // non-interactive; reads prompt from stdin
        .arg("--verbose") // required when using --output-format=stream-json with --print
        // Disable any colour/spinner output that would pollute stdout
        .env("NO_COLOR", "1")
        .env("TERM", "dumb");

    // Pass model if explicitly configured with a real Claude model name.
    // Skip if the sentinel "claude_code" is set (let claude pick its default).
    if !config.model.is_empty()
        && config.model != "claude_code"
        && config.model.starts_with("claude")
    {
        cmd.arg("--model").arg(&config.model);
    }

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped()); // capture stderr so we can report it on failure

    let mut child = cmd
        .spawn()
        .with_context(|| format!("failed to spawn claude binary: {binary}"))?;

    // ── Write prompt to stdin, then close the pipe ───────────────────────────
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(prompt.as_bytes())
            .await
            .context("failed to write prompt to claude stdin")?;
        // Closing stdin signals EOF to claude so it starts processing
    }

    // ── Read stdout with explicit byte-level buffering ───────────────────────
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("claude: stdout pipe not available"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("claude: stderr pipe not available"))?;

    // Spawn stderr collector (non-blocking, best-effort)
    let stderr_handle = tokio::spawn(async move {
        let mut buf = Vec::new();
        let mut reader = tokio::io::BufReader::new(stderr);
        let _ = reader.read_to_end(&mut buf).await;
        String::from_utf8_lossy(&buf).into_owned()
    });

    let mut raw_buf: Vec<u8> = Vec::with_capacity(READ_BUF_SIZE);
    let mut read_buf = vec![0u8; READ_BUF_SIZE];
    let mut stdout = tokio::io::BufReader::new(stdout);

    let mut full_text = String::new();
    let mut input_tokens = 0u64;
    let mut output_tokens = 0u64;
    let mut actual_model: Option<String> = None;

    loop {
        let n = stdout
            .read(&mut read_buf)
            .await
            .context("error reading claude stdout")?;

        if n == 0 {
            // EOF — process any remaining bytes in the buffer
            if !raw_buf.is_empty() {
                if let Some(line) = extract_clean_line(&raw_buf) {
                    process_line(
                        &line,
                        &mut full_text,
                        &mut input_tokens,
                        &mut output_tokens,
                        &mut actual_model,
                        on_delta,
                    );
                }
            }
            break;
        }

        raw_buf.extend_from_slice(&read_buf[..n]);

        // Process all complete newline-terminated frames from the buffer
        loop {
            // Find the next newline
            if let Some(nl_pos) = raw_buf.iter().position(|&b| b == b'\n') {
                let frame = raw_buf.drain(..=nl_pos).collect::<Vec<u8>>();
                if let Some(line) = extract_clean_line(&frame) {
                    process_line(
                        &line,
                        &mut full_text,
                        &mut input_tokens,
                        &mut output_tokens,
                        &mut actual_model,
                        on_delta,
                    );
                }
            } else {
                break; // no complete frame yet — wait for more bytes
            }
        }
    }

    // ── Wait for subprocess exit ─────────────────────────────────────────────
    let status = child.wait().await.context("failed to wait for claude")?;
    let stderr_text = stderr_handle.await.unwrap_or_default();

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        let detail = if !stderr_text.is_empty() {
            format!(" — stderr: {}", stderr_text.trim())
        } else {
            String::new()
        };
        bail!("claude exited with code {}{}", code, detail);
    }

    let usage = if input_tokens > 0 || output_tokens > 0 {
        Some(TokenUsage {
            input_tokens,
            output_tokens,
            total_tokens: input_tokens + output_tokens,
        })
    } else {
        None
    };

    Ok(LlmResult {
        text: full_text,
        tool_calls: vec![], // Claude's tool loop is transparent to us
        usage,
        model: actual_model.or_else(|| Some(config.model.clone())),
        stop_reason: "end_turn".into(),
    })
}

// ── Frame extraction & noise stripping ───────────────────────────────────────

/// Strip CLI noise from a raw byte frame and return a cleaned UTF-8 string
/// if it looks like it might contain a JSON object. Returns None for frames
/// that are purely noise (empty after stripping, or don't contain '{').
fn extract_clean_line(raw: &[u8]) -> Option<String> {
    let s = String::from_utf8_lossy(raw);
    let cleaned = strip_cli_noise(&s);
    // Only attempt JSON parsing if the cleaned line starts with '{'
    if cleaned.starts_with('{') {
        Some(cleaned)
    } else {
        None
    }
}

/// Remove terminal artefacts that the claude CLI may emit:
/// - ANSI/VT escape sequences  (ESC [ ... m  and  ESC ] ... ST)
/// - Carriage returns
/// - ASCII control characters (except printable space)
/// - Unicode spinner/progress characters (⠙⠸⠼⠴⠦⠧⠇⠋)
///
/// This is intentionally conservative: we never modify the content of JSON
/// strings, only strip bytes that are structurally impossible to appear in
/// valid JSON (the \x1b escape used for ANSI codes is not valid in JSON
/// strings unless escaped as \\u001b).
fn strip_cli_noise(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            // ESC byte — start of ANSI/VT escape sequence
            0x1b => {
                i += 1; // consume ESC
                if i < bytes.len() {
                    match bytes[i] {
                        // CSI sequence: ESC [ ... (ends at first non-param non-intermed byte)
                        b'[' => {
                            i += 1;
                            while i < bytes.len()
                                && (bytes[i] == b';'
                                    || bytes[i].is_ascii_digit()
                                    || (0x20..=0x2f).contains(&bytes[i]))
                            {
                                i += 1;
                            }
                            i += 1; // consume the final command byte
                        }
                        // OSC sequence: ESC ] ... ST (where ST = ESC \ or BEL)
                        b']' => {
                            i += 1;
                            while i < bytes.len() {
                                if bytes[i] == 0x07 {
                                    // BEL
                                    i += 1;
                                    break;
                                }
                                if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\'
                                {
                                    i += 2;
                                    break;
                                }
                                i += 1;
                            }
                        }
                        // Other two-character escape sequences
                        _ => {
                            i += 1;
                        }
                    }
                }
            }
            // Carriage return — skip
            b'\r' => {
                i += 1;
            }
            // Other ASCII control characters (0x00–0x1f except 0x09 tab, 0x0a newline)
            b if b < 0x20 && b != b'\t' && b != b'\n' => {
                i += 1;
            }
            _ => {
                // Decode the UTF-8 character
                let ch_str = &s[i..];
                if let Some(ch) = ch_str.chars().next() {
                    // Filter Unicode spinner/progress characters commonly emitted by CLIs
                    match ch {
                        '⠙' | '⠸' | '⠼' | '⠴' | '⠦' | '⠧' | '⠇' | '⠋' | '⠹' | '⠺'
                        | '⠻' | '⠽' | '⠾' | '⠿' => {
                            i += ch.len_utf8();
                        }
                        _ => {
                            out.push(ch);
                            i += ch.len_utf8();
                        }
                    }
                } else {
                    i += 1; // skip invalid UTF-8 byte
                }
            }
        }
    }

    // Trim leading/trailing whitespace that often surrounds spinner lines
    out.trim().to_owned()
}

// ── JSON event processor ─────────────────────────────────────────────────────

/// Attempt to parse a cleaned line as a claude stream-json event. All parsing
/// errors are silently swallowed — a single bad frame never aborts the stream.
fn process_line(
    line: &str,
    full_text: &mut String,
    input_tokens: &mut u64,
    output_tokens: &mut u64,
    actual_model: &mut Option<String>,
    on_delta: &(impl Fn(&str) + Send + Sync),
) {
    let ev: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            log::trace!("[claude_code] skipping unparseable frame: {} — {:?}", e, &line[..line.len().min(120)]);
            return;
        }
    };

    match ev["type"].as_str().unwrap_or("") {
        // ── Initialisation ───────────────────────────────────────────────────
        "system" => {
            if let Some(m) = ev["model"].as_str() {
                *actual_model = Some(m.to_owned());
            }
        }

        // ── Assistant turn ───────────────────────────────────────────────────
        // Contains complete content blocks (not incremental deltas).
        // We split text into smaller chunks before emitting so the UI
        // feels more responsive.
        "assistant" => {
            // Model capture
            if actual_model.is_none() {
                if let Some(m) = ev["message"]["model"].as_str() {
                    *actual_model = Some(m.to_owned());
                }
            }

            // Usage accumulation (these are cumulative per-turn)
            if let Some(u) = ev["message"]["usage"].as_object() {
                *input_tokens = u
                    .get("input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(*input_tokens);
                *output_tokens += u
                    .get("output_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
            }

            // Extract text from content blocks
            if let Some(content) = ev["message"]["content"].as_array() {
                for block in content {
                    if block["type"].as_str() == Some("text") {
                        if let Some(text) = block["text"].as_str() {
                            if !text.is_empty() {
                                emit_as_deltas(text, on_delta);
                                full_text.push_str(text);
                            }
                        }
                    }
                }
            }
        }

        // ── Final result ─────────────────────────────────────────────────────
        "result" => {
            // Error sub-types are logged but don't abort — we rely on the
            // subprocess exit code check for authoritative error detection.
            if ev["is_error"].as_bool().unwrap_or(false)
                || ev["subtype"].as_str() == Some("error")
            {
                let msg = ev["error"]
                    .as_str()
                    .or_else(|| ev["result"].as_str())
                    .unwrap_or("unknown error");
                log::warn!("[claude_code] result error: {}", msg);
                return;
            }

            // Use aggregate usage from the result frame (more accurate than summing turns)
            if let Some(u) = ev["usage"].as_object() {
                if let Some(inp) = u.get("input_tokens").and_then(|v| v.as_u64()) {
                    *input_tokens = inp;
                }
                if let Some(out) = u.get("output_tokens").and_then(|v| v.as_u64()) {
                    *output_tokens = out;
                }
            }

            // Fallback: use result.result text if we somehow have nothing yet
            if full_text.is_empty() {
                if let Some(result) = ev["result"].as_str() {
                    if !result.is_empty() {
                        emit_as_deltas(result, on_delta);
                        full_text.push_str(result);
                    }
                }
            }
        }

        // user / tool_use / tool_result / unknown — ignored intentionally
        _ => {}
    }
}

// ── Prompt builder ───────────────────────────────────────────────────────────

/// Serialise the system context and conversation history into a plain-text
/// prompt suitable for `claude --print`. The reduction pipeline has already
/// compressed the history, so we just need to encode it faithfully.
fn build_prompt(system: &str, messages: &[Message]) -> String {
    let mut out = String::with_capacity(system.len() + 512);

    if !system.is_empty() {
        out.push_str("<system_context>\n");
        out.push_str(system);
        out.push_str("\n</system_context>\n\n");
    }

    for msg in messages {
        let text = extract_text(msg);
        if text.is_empty() {
            continue;
        }
        match msg.role.as_str() {
            "user" => {
                out.push_str("Human: ");
                out.push_str(&text);
                out.push('\n');
            }
            "assistant" => {
                out.push_str("Assistant: ");
                out.push_str(&text);
                out.push('\n');
            }
            _ => {
                out.push_str(&text);
                out.push('\n');
            }
        }
    }

    out
}

fn extract_text(msg: &Message) -> String {
    msg.blocks
        .iter()
        .filter_map(|b| match b {
            crate::types::ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ── Streaming helper ─────────────────────────────────────────────────────────

/// Emit a text block as a series of small chunks so the VS Code extension
/// renders a smooth streaming effect rather than one large pop.
///
/// The claude CLI delivers complete message objects, not sub-token increments.
/// We split on word boundaries to simulate streaming granularity.
fn emit_as_deltas(text: &str, on_delta: &impl Fn(&str)) {
    // Short texts: emit whole
    if text.len() <= 60 {
        on_delta(text);
        return;
    }

    // Longer texts: split roughly every ~40 chars at a word boundary
    const TARGET: usize = 40;
    let mut start = 0usize;
    let mut last_boundary = 0usize;

    for (i, ch) in text.char_indices() {
        if ch.is_whitespace() || ch == '\n' {
            last_boundary = i + ch.len_utf8();
        }
        if i > start + TARGET && last_boundary > start {
            on_delta(&text[start..last_boundary]);
            start = last_boundary;
        }
    }

    if start < text.len() {
        on_delta(&text[start..]);
    }
}
