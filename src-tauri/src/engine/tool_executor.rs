// Paw Agent Engine — Tool Executor
// Executes tool calls requested by the AI model.
// Every tool call goes through here — this is the security enforcement point.

use crate::engine::types::*;
use crate::engine::commands::EngineState;
use crate::engine::memory;
use log::{info, warn, error};
use std::process::Command as ProcessCommand;
use std::time::Duration;
use tauri::Manager;

/// Execute a single tool call and return the result.
/// This is where security policies are enforced.
pub async fn execute_tool(tool_call: &ToolCall, app_handle: &tauri::AppHandle) -> ToolResult {
    let name = &tool_call.function.name;
    let args_str = &tool_call.function.arguments;

    info!("[engine] Executing tool: {} args={}", name, &args_str[..args_str.len().min(200)]);

    let args: serde_json::Value = serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));

    let result = match name.as_str() {
        "exec" => execute_exec(&args).await,
        "fetch" => execute_fetch(&args).await,
        "read_file" => execute_read_file(&args).await,
        "write_file" => execute_write_file(&args).await,
        "soul_read" => execute_soul_read(&args, app_handle).await,
        "soul_write" => execute_soul_write(&args, app_handle).await,
        "soul_list" => execute_soul_list(app_handle).await,
        "memory_store" => execute_memory_store(&args, app_handle).await,
        "memory_search" => execute_memory_search(&args, app_handle).await,
        _ => Err(format!("Unknown tool: {}", name)),
    };

    match result {
        Ok(output) => ToolResult {
            tool_call_id: tool_call.id.clone(),
            output,
            success: true,
        },
        Err(err) => ToolResult {
            tool_call_id: tool_call.id.clone(),
            output: format!("Error: {}", err),
            success: false,
        },
    }
}

// ── exec: Run shell commands ───────────────────────────────────────────

async fn execute_exec(args: &serde_json::Value) -> Result<String, String> {
    let command = args["command"].as_str()
        .ok_or("exec: missing 'command' argument")?;

    info!("[engine] exec: {}", &command[..command.len().min(200)]);

    // Run via sh -c (Unix) or cmd /C (Windows)
    let output = if cfg!(target_os = "windows") {
        ProcessCommand::new("cmd")
            .args(["/C", command])
            .output()
    } else {
        ProcessCommand::new("sh")
            .args(["-c", command])
            .output()
    };

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();

            // Combine stdout + stderr, truncate to avoid flooding the context window
            let mut result = String::new();
            if !stdout.is_empty() {
                result.push_str(&stdout);
            }
            if !stderr.is_empty() {
                if !result.is_empty() {
                    result.push_str("\n--- stderr ---\n");
                }
                result.push_str(&stderr);
            }
            if result.is_empty() {
                result = format!("(exit code: {})", out.status.code().unwrap_or(-1));
            }

            // Truncate very long output to avoid blowing up the context window
            const MAX_OUTPUT: usize = 50_000;
            if result.len() > MAX_OUTPUT {
                result.truncate(MAX_OUTPUT);
                result.push_str("\n\n... [output truncated]");
            }

            Ok(result)
        }
        Err(e) => Err(format!("Failed to execute command: {}", e)),
    }
}

// ── fetch: HTTP requests ───────────────────────────────────────────────

async fn execute_fetch(args: &serde_json::Value) -> Result<String, String> {
    let url = args["url"].as_str()
        .ok_or("fetch: missing 'url' argument")?;
    let method = args["method"].as_str().unwrap_or("GET");

    info!("[engine] fetch: {} {}", method, url);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let mut request = match method.to_uppercase().as_str() {
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "PATCH" => client.patch(url),
        "DELETE" => client.delete(url),
        "HEAD" => client.head(url),
        _ => client.get(url),
    };

    // Add custom headers
    if let Some(headers) = args["headers"].as_object() {
        for (key, value) in headers {
            if let Some(v) = value.as_str() {
                request = request.header(key.as_str(), v);
            }
        }
    }

    // Add body
    if let Some(body) = args["body"].as_str() {
        request = request.body(body.to_string());
    }

    let response = request.send().await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    let status = response.status().as_u16();
    let body = response.text().await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    // Truncate long responses
    const MAX_BODY: usize = 50_000;
    let truncated = if body.len() > MAX_BODY {
        format!("{}...\n[truncated, {} total bytes]", &body[..MAX_BODY], body.len())
    } else {
        body
    };

    Ok(format!("HTTP {} {}\n\n{}", status, if status < 400 { "OK" } else { "Error" }, truncated))
}

// ── read_file: Read file contents ──────────────────────────────────────

async fn execute_read_file(args: &serde_json::Value) -> Result<String, String> {
    let path = args["path"].as_str()
        .ok_or("read_file: missing 'path' argument")?;

    info!("[engine] read_file: {}", path);

    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file '{}': {}", path, e))?;

    // Truncate very long files
    const MAX_FILE: usize = 100_000;
    if content.len() > MAX_FILE {
        Ok(format!("{}...\n[truncated, {} total bytes]", &content[..MAX_FILE], content.len()))
    } else {
        Ok(content)
    }
}

// ── write_file: Write file contents ────────────────────────────────────

async fn execute_write_file(args: &serde_json::Value) -> Result<String, String> {
    let path = args["path"].as_str()
        .ok_or("write_file: missing 'path' argument")?;
    let content = args["content"].as_str()
        .ok_or("write_file: missing 'content' argument")?;

    info!("[engine] write_file: {} ({} bytes)", path, content.len());

    // Create parent directories if needed
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    std::fs::write(path, content)
        .map_err(|e| format!("Failed to write file '{}': {}", path, e))?;

    Ok(format!("Successfully wrote {} bytes to {}", content.len(), path))
}

// ── soul_read: Read a soul/persona file ────────────────────────────────

async fn execute_soul_read(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> Result<String, String> {
    let file_name = args["file_name"].as_str()
        .ok_or("soul_read: missing 'file_name' argument")?;

    info!("[engine] soul_read: {}", file_name);

    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let agent_id = "default";
    match state.store.get_agent_file(agent_id, file_name)? {
        Some(file) => Ok(format!("# {}\n\n{}", file.file_name, file.content)),
        None => Ok(format!("File '{}' does not exist yet. You can create it with soul_write.", file_name)),
    }
}

// ── soul_write: Write/update a soul/persona file ───────────────────────

async fn execute_soul_write(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> Result<String, String> {
    let file_name = args["file_name"].as_str()
        .ok_or("soul_write: missing 'file_name' argument")?;
    let content = args["content"].as_str()
        .ok_or("soul_write: missing 'content' argument")?;

    // Validate file name — only allow known soul files to prevent abuse
    let allowed_files = ["IDENTITY.md", "SOUL.md", "USER.md", "AGENTS.md", "TOOLS.md"];
    if !allowed_files.contains(&file_name) {
        return Err(format!(
            "soul_write: '{}' is not an allowed soul file. Allowed: {}",
            file_name,
            allowed_files.join(", ")
        ));
    }

    info!("[engine] soul_write: {} ({} bytes)", file_name, content.len());

    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let agent_id = "default";
    state.store.set_agent_file(agent_id, file_name, content)?;

    Ok(format!("Successfully updated {}. This change will take effect in future conversations.", file_name))
}

// ── soul_list: List all soul/persona files ─────────────────────────────

async fn execute_soul_list(app_handle: &tauri::AppHandle) -> Result<String, String> {
    info!("[engine] soul_list");

    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let agent_id = "default";
    let files = state.store.list_agent_files(agent_id)?;

    if files.is_empty() {
        return Ok("No soul files exist yet. You can create them with soul_write. Available files:\n- IDENTITY.md (your name, role, purpose)\n- SOUL.md (personality, values, voice)\n- USER.md (facts about the user)\n- AGENTS.md (other agents)\n- TOOLS.md (tool preferences)".into());
    }

    let mut output = String::from("Soul files:\n");
    for f in &files {
        output.push_str(&format!("- {} ({} bytes, updated {})\n", f.file_name, f.content.len(), f.updated_at));
    }
    output.push_str("\nUse soul_read to view a file, soul_write to update one.");
    Ok(output)
}

// ── memory_store: Store a memory ───────────────────────────────────────

async fn execute_memory_store(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> Result<String, String> {
    let content = args["content"].as_str()
        .ok_or("memory_store: missing 'content' argument")?;
    let category = args["category"].as_str().unwrap_or("general");

    info!("[engine] memory_store: category={} len={}", category, content.len());

    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let emb_client = state.embedding_client();
    let id = memory::store_memory(&state.store, content, category, 5, emb_client.as_ref()).await?;

    Ok(format!("Memory stored (id: {}). I'll recall this automatically when it's relevant.", &id[..8]))
}

// ── memory_search: Search memories ─────────────────────────────────────

async fn execute_memory_search(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> Result<String, String> {
    let query = args["query"].as_str()
        .ok_or("memory_search: missing 'query' argument")?;
    let limit = args["limit"].as_u64().unwrap_or(5) as usize;

    info!("[engine] memory_search: query='{}' limit={}", &query[..query.len().min(100)], limit);

    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let emb_client = state.embedding_client();
    let results = memory::search_memories(&state.store, query, limit, 0.1, emb_client.as_ref()).await?;

    if results.is_empty() {
        return Ok("No relevant memories found.".into());
    }

    let mut output = format!("Found {} relevant memories:\n\n", results.len());
    for (i, mem) in results.iter().enumerate() {
        output.push_str(&format!("{}. [{}] {} (score: {:.2})\n", i + 1, mem.category, mem.content, mem.score.unwrap_or(0.0)));
    }
    Ok(output)
}
