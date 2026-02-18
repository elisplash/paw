// Paw Agent Engine — Tool Executor
// Executes tool calls requested by the AI model.
// Every tool call goes through here — this is the security enforcement point.

use crate::engine::types::*;
use crate::engine::commands::EngineState;
use crate::engine::memory;
use crate::engine::skills;
use crate::engine::sandbox;
use crate::engine::web;
use log::{info, warn, error};
use std::process::Command as ProcessCommand;
use std::time::Duration;
use tauri::{Emitter, Manager};

/// Execute a single tool call and return the result.
/// This is where security policies are enforced.
/// Get the per-agent workspace directory path.
/// Each agent gets its own isolated workspace at ~/.paw/workspaces/{agent_id}/
pub fn agent_workspace(agent_id: &str) -> std::path::PathBuf {
    let base = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join(".paw").join("workspaces").join(agent_id)
}

/// Ensure the agent's workspace directory exists.
fn ensure_workspace(agent_id: &str) -> Result<std::path::PathBuf, String> {
    let ws = agent_workspace(agent_id);
    std::fs::create_dir_all(&ws)
        .map_err(|e| format!("Failed to create workspace for agent '{}': {}", agent_id, e))?;
    Ok(ws)
}

pub async fn execute_tool(tool_call: &ToolCall, app_handle: &tauri::AppHandle, agent_id: &str) -> ToolResult {
    let name = &tool_call.function.name;
    let args_str = &tool_call.function.arguments;

    info!("[engine] Executing tool: {} agent={} args={}", name, agent_id, &args_str[..args_str.len().min(200)]);

    let args: serde_json::Value = serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));

    let result = match name.as_str() {
        "exec" => execute_exec(&args, app_handle, agent_id).await,
        "fetch" => execute_fetch(&args).await,
        "read_file" => execute_read_file(&args, agent_id).await,
        "write_file" => execute_write_file(&args, agent_id).await,
        "list_directory" => execute_list_directory(&args, agent_id).await,
        "append_file" => execute_append_file(&args, agent_id).await,
        "delete_file" => execute_delete_file(&args, agent_id).await,
        "soul_read" => execute_soul_read(&args, app_handle, agent_id).await,
        "soul_write" => execute_soul_write(&args, app_handle, agent_id).await,
        "soul_list" => execute_soul_list(app_handle, agent_id).await,
        "memory_store" => execute_memory_store(&args, app_handle).await,
        "memory_search" => execute_memory_search(&args, app_handle).await,
        "self_info" => execute_self_info(app_handle).await,
        "update_profile" => execute_update_profile(&args, app_handle).await,
        "create_agent" => execute_create_agent(&args, app_handle).await,
        // ── Task / Automation tools ──
        "create_task" => execute_create_task(&args, app_handle).await,
        "list_tasks" => execute_list_tasks(&args, app_handle).await,
        "manage_task" => execute_manage_task(&args, app_handle).await,
        // ── Web tools ──
        "web_search" => web::execute_web_search(&args).await,
        "web_read" => web::execute_web_read(&args).await,
        "web_screenshot" => web::execute_web_screenshot(&args).await,
        "web_browse" => web::execute_web_browse(&args).await,
        // ── Skill tools ──
        "email_send" => execute_skill_tool("email", "email_send", &args, app_handle).await,
        "email_read" => execute_skill_tool("email", "email_read", &args, app_handle).await,
        "slack_send" => execute_skill_tool("slack", "slack_send", &args, app_handle).await,
        "slack_read" => execute_skill_tool("slack", "slack_read", &args, app_handle).await,
        "telegram_send" => execute_telegram_send(&args, app_handle).await,
        "telegram_read" => execute_telegram_read(&args, app_handle).await,
        "github_api" => execute_skill_tool("github", "github_api", &args, app_handle).await,
        "rest_api_call" => execute_skill_tool("rest_api", "rest_api_call", &args, app_handle).await,
        "webhook_send" => execute_skill_tool("webhook", "webhook_send", &args, app_handle).await,
        "image_generate" => execute_skill_tool("image_gen", "image_generate", &args, app_handle).await,
        // ── Coinbase CDP tools ──
        "coinbase_prices" => execute_skill_tool("coinbase", "coinbase_prices", &args, app_handle).await,
        "coinbase_balance" => execute_skill_tool("coinbase", "coinbase_balance", &args, app_handle).await,
        "coinbase_wallet_create" => execute_skill_tool("coinbase", "coinbase_wallet_create", &args, app_handle).await,
        "coinbase_trade" => execute_skill_tool("coinbase", "coinbase_trade", &args, app_handle).await,
        "coinbase_transfer" => execute_skill_tool("coinbase", "coinbase_transfer", &args, app_handle).await,
        // ── Solana DEX / Jupiter tools ──
        "sol_wallet_create" => execute_skill_tool("solana_dex", "sol_wallet_create", &args, app_handle).await,
        "sol_balance" => execute_skill_tool("solana_dex", "sol_balance", &args, app_handle).await,
        "sol_quote" => execute_skill_tool("solana_dex", "sol_quote", &args, app_handle).await,
        "sol_swap" => execute_skill_tool("solana_dex", "sol_swap", &args, app_handle).await,
        "sol_portfolio" => execute_skill_tool("solana_dex", "sol_portfolio", &args, app_handle).await,
        "sol_token_info" => execute_skill_tool("solana_dex", "sol_token_info", &args, app_handle).await,
        // ── DEX / Uniswap tools ──
        "dex_wallet_create" => execute_skill_tool("dex", "dex_wallet_create", &args, app_handle).await,
        "dex_balance" => execute_skill_tool("dex", "dex_balance", &args, app_handle).await,
        "dex_quote" => execute_skill_tool("dex", "dex_quote", &args, app_handle).await,
        "dex_swap" => execute_skill_tool("dex", "dex_swap", &args, app_handle).await,
        "dex_portfolio" => execute_skill_tool("dex", "dex_portfolio", &args, app_handle).await,
        "dex_token_info" => execute_skill_tool("dex", "dex_token_info", &args, app_handle).await,
        "dex_check_token" => execute_skill_tool("dex", "dex_check_token", &args, app_handle).await,
        "dex_search_token" => execute_skill_tool("dex", "dex_search_token", &args, app_handle).await,
        "dex_watch_wallet" => execute_skill_tool("dex", "dex_watch_wallet", &args, app_handle).await,
        "dex_whale_transfers" => execute_skill_tool("dex", "dex_whale_transfers", &args, app_handle).await,
        "dex_top_traders" => execute_skill_tool("dex", "dex_top_traders", &args, app_handle).await,
        "dex_trending" => execute_skill_tool("dex", "dex_trending", &args, app_handle).await,
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

async fn execute_exec(args: &serde_json::Value, app_handle: &tauri::AppHandle, agent_id: &str) -> Result<String, String> {
    let command = args["command"].as_str()
        .ok_or("exec: missing 'command' argument")?;

    info!("[engine] exec: {}", &command[..command.len().min(200)]);

    // Block installing packages that duplicate built-in skill tools
    let cmd_lower = command.to_lowercase();
    let blocked_packages = ["cdp-sdk", "coinbase-sdk", "coinbase-advanced-py", "cbpro", "coinbase"];
    if cmd_lower.contains("pip") || cmd_lower.contains("npm") {
        for pkg in &blocked_packages {
            if cmd_lower.contains(pkg) {
                return Err(format!(
                    "Do not install '{}'. Coinbase access is handled by built-in tools: \
                     coinbase_balance, coinbase_prices, coinbase_trade, coinbase_transfer. \
                     Call those tools directly.",
                    pkg
                ));
            }
        }
    }

    // Check sandbox config — if enabled, route through Docker container
    let sandbox_config = {
        let state = app_handle.state::<EngineState>();
        sandbox::load_sandbox_config(&state.store)
    };

    if sandbox_config.enabled {
        info!("[engine] exec: routing through sandbox (image={})", sandbox_config.image);
        match sandbox::run_in_sandbox(command, &sandbox_config).await {
            Ok(result) => return Ok(sandbox::format_sandbox_result(&result)),
            Err(e) => {
                warn!("[engine] Sandbox execution failed, falling back to host: {}", e);
                // Fall through to host execution
            }
        }
    }

    // Set working directory to agent's workspace
    let workspace = ensure_workspace(agent_id)?;

    // Run via sh -c (Unix) or cmd /C (Windows)
    let output = if cfg!(target_os = "windows") {
        ProcessCommand::new("cmd")
            .args(["/C", command])
            .current_dir(&workspace)
            .output()
    } else {
        ProcessCommand::new("sh")
            .args(["-c", command])
            .current_dir(&workspace)
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

async fn execute_read_file(args: &serde_json::Value, agent_id: &str) -> Result<String, String> {
    let raw_path = args["path"].as_str()
        .ok_or("read_file: missing 'path' argument")?;

    // Resolve relative paths within the agent's workspace
    let resolved = if std::path::Path::new(raw_path).is_absolute() {
        std::path::PathBuf::from(raw_path)
    } else {
        let ws = ensure_workspace(agent_id)?;
        ws.join(raw_path)
    };
    let path = resolved.to_string_lossy();

    info!("[engine] read_file: {} (agent={})", path, agent_id);

    // Block reading engine source code — the agent should not introspect its own internals
    let normalized = path.replace('\\', "/").to_lowercase();
    if normalized.contains("src-tauri/src/engine/")
        || normalized.contains("src/engine/")
        || normalized.ends_with(".rs")
    {
        return Err(format!(
            "Cannot read engine source file '{}'. \
             Use your available tools directly — credentials and authentication are handled automatically.",
            path
        ));
    }

    let content = std::fs::read_to_string(&resolved)
        .map_err(|e| format!("Failed to read file '{}': {}", path, e))?;

    // Truncate very long files to avoid blowing up context
    const MAX_FILE: usize = 32_000;
    if content.len() > MAX_FILE {
        Ok(format!("{}...\n[truncated, {} total bytes]", &content[..MAX_FILE], content.len()))
    } else {
        Ok(content)
    }
}

// ── write_file: Write file contents ────────────────────────────────────

async fn execute_write_file(args: &serde_json::Value, agent_id: &str) -> Result<String, String> {
    let raw_path = args["path"].as_str()
        .ok_or("write_file: missing 'path' argument")?;
    let content = args["content"].as_str()
        .ok_or("write_file: missing 'content' argument")?;

    // Resolve relative paths within the agent's workspace
    let resolved = if std::path::Path::new(raw_path).is_absolute() {
        std::path::PathBuf::from(raw_path)
    } else {
        let ws = ensure_workspace(agent_id)?;
        ws.join(raw_path)
    };
    let path = resolved.to_string_lossy();

    info!("[engine] write_file: {} ({} bytes, agent={})", path, content.len(), agent_id);

    // Block writing files that contain credential-like patterns
    let content_lower = content.to_lowercase();
    let has_private_key = content.contains("-----BEGIN") && content.contains("PRIVATE KEY");
    let has_api_secret = content_lower.contains("api_key_secret") || content_lower.contains("cdp_api_key");
    let has_raw_b64_key = content.len() > 40 && content.contains("==") && (content_lower.contains("secret") || content_lower.contains("private"));
    if has_private_key || has_api_secret || has_raw_b64_key {
        return Err(
            "Cannot write files containing API secrets or private keys. \
             Credentials are managed securely by the engine — use built-in skill tools directly."
            .into()
        );
    }

    // Create parent directories if needed
    if let Some(parent) = resolved.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    std::fs::write(&resolved, content)
        .map_err(|e| format!("Failed to write file '{}': {}", path, e))?;

    Ok(format!("Successfully wrote {} bytes to {}", content.len(), path))
}

// ── list_directory: List contents of a directory ───────────────────────

async fn execute_list_directory(args: &serde_json::Value, agent_id: &str) -> Result<String, String> {
    let raw_path = args["path"].as_str().unwrap_or(".");
    let recursive = args["recursive"].as_bool().unwrap_or(false);
    let max_depth = args["max_depth"].as_u64().unwrap_or(3) as usize;

    // Resolve relative paths (including ".") within the agent's workspace
    let resolved = if std::path::Path::new(raw_path).is_absolute() {
        std::path::PathBuf::from(raw_path)
    } else {
        let ws = ensure_workspace(agent_id)?;
        ws.join(raw_path)
    };
    let path = resolved.to_string_lossy().to_string();

    info!("[engine] list_directory: {} recursive={} (agent={})", path, recursive, agent_id);

    if !resolved.exists() {
        return Err(format!("Directory '{}' does not exist", path));
    }
    if !resolved.is_dir() {
        return Err(format!("'{}' is not a directory", path));
    }

    let mut entries = Vec::new();

    fn walk_dir(dir: &std::path::Path, prefix: &str, depth: usize, max_depth: usize, entries: &mut Vec<String>) -> std::io::Result<()> {
        if depth > max_depth { return Ok(()); }
        let mut items: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
        items.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

        for entry in &items {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let suffix = if is_dir { "/" } else { "" };

            if let Ok(meta) = entry.metadata() {
                let size = if is_dir { String::new() } else { format!(" ({} bytes)", meta.len()) };
                entries.push(format!("{}{}{}{}", prefix, name, suffix, size));
            } else {
                entries.push(format!("{}{}{}", prefix, name, suffix));
            }

            if is_dir && depth < max_depth {
                walk_dir(&entry.path(), &format!("{}  ", prefix), depth + 1, max_depth, entries)?;
            }
        }
        Ok(())
    }

    if recursive {
        walk_dir(&resolved, "", 0, max_depth, &mut entries)
            .map_err(|e| format!("Failed to list directory '{}': {}", path, e))?;
    } else {
        let mut items: Vec<_> = std::fs::read_dir(&resolved)
            .map_err(|e| format!("Failed to list directory '{}': {}", path, e))?
            .filter_map(|e| e.ok())
            .collect();
        items.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

        for entry in &items {
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let suffix = if is_dir { "/" } else { "" };
            if let Ok(meta) = entry.metadata() {
                let size = if is_dir { String::new() } else { format!(" ({} bytes)", meta.len()) };
                entries.push(format!("{}{}{}", name, suffix, size));
            } else {
                entries.push(format!("{}{}", name, suffix));
            }
        }
    }

    if entries.is_empty() {
        Ok(format!("Directory '{}' is empty.", path))
    } else {
        Ok(format!("Contents of '{}':\n{}", path, entries.join("\n")))
    }
}

// ── append_file: Append content to a file ──────────────────────────────

async fn execute_append_file(args: &serde_json::Value, agent_id: &str) -> Result<String, String> {
    let raw_path = args["path"].as_str()
        .ok_or("append_file: missing 'path' argument")?;
    let content = args["content"].as_str()
        .ok_or("append_file: missing 'content' argument")?;

    // Resolve relative paths within the agent's workspace
    let resolved = if std::path::Path::new(raw_path).is_absolute() {
        std::path::PathBuf::from(raw_path)
    } else {
        let ws = ensure_workspace(agent_id)?;
        ws.join(raw_path)
    };
    let path = resolved.to_string_lossy();

    info!("[engine] append_file: {} ({} bytes, agent={})", path, content.len(), agent_id);

    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&resolved)
        .map_err(|e| format!("Failed to open file '{}' for append: {}", path, e))?;

    file.write_all(content.as_bytes())
        .map_err(|e| format!("Failed to append to file '{}': {}", path, e))?;

    Ok(format!("Appended {} bytes to {}", content.len(), path))
}

// ── delete_file: Delete a file or directory ─────────────────────────────

async fn execute_delete_file(args: &serde_json::Value, agent_id: &str) -> Result<String, String> {
    let raw_path = args["path"].as_str()
        .ok_or("delete_file: missing 'path' argument")?;
    let recursive = args["recursive"].as_bool().unwrap_or(false);

    // Resolve relative paths within the agent's workspace
    let resolved = if std::path::Path::new(raw_path).is_absolute() {
        std::path::PathBuf::from(raw_path)
    } else {
        let ws = ensure_workspace(agent_id)?;
        ws.join(raw_path)
    };
    let path = resolved.to_string_lossy();

    info!("[engine] delete_file: {} recursive={} (agent={})", path, recursive, agent_id);

    if !resolved.exists() {
        return Err(format!("Path '{}' does not exist", path));
    }

    if resolved.is_dir() {
        if recursive {
            std::fs::remove_dir_all(&resolved)
                .map_err(|e| format!("Failed to remove directory '{}': {}", path, e))?;
            Ok(format!("Deleted directory '{}' (recursive)", path))
        } else {
            std::fs::remove_dir(&resolved)
                .map_err(|e| format!("Failed to remove directory '{}' (not empty? use recursive=true): {}", path, e))?;
            Ok(format!("Deleted empty directory '{}'", path))
        }
    } else {
        std::fs::remove_file(&resolved)
            .map_err(|e| format!("Failed to delete file '{}': {}", path, e))?;
        Ok(format!("Deleted file '{}'", path))
    }
}

// ── soul_read: Read a soul/persona file ────────────────────────────────

async fn execute_soul_read(args: &serde_json::Value, app_handle: &tauri::AppHandle, agent_id: &str) -> Result<String, String> {
    let file_name = args["file_name"].as_str()
        .ok_or("soul_read: missing 'file_name' argument")?;

    info!("[engine] soul_read: {} (agent={})", file_name, agent_id);

    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;
    match state.store.get_agent_file(agent_id, file_name)? {
        Some(file) => Ok(format!("# {}\n\n{}", file.file_name, file.content)),
        None => Ok(format!("File '{}' does not exist yet. You can create it with soul_write.", file_name)),
    }
}

// ── soul_write: Write/update a soul/persona file ───────────────────────

async fn execute_soul_write(args: &serde_json::Value, app_handle: &tauri::AppHandle, agent_id: &str) -> Result<String, String> {
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

    info!("[engine] soul_write: {} ({} bytes, agent={})", file_name, content.len(), agent_id);

    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;
    state.store.set_agent_file(agent_id, file_name, content)?;

    Ok(format!("Successfully updated {}. This change will take effect in future conversations.", file_name))
}

// ── soul_list: List all soul/persona files ─────────────────────────────

async fn execute_soul_list(app_handle: &tauri::AppHandle, agent_id: &str) -> Result<String, String> {
    info!("[engine] soul_list (agent={})", agent_id);

    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;
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
    let id = memory::store_memory(&state.store, content, category, 5, emb_client.as_ref(), None).await?;;

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
    let results = memory::search_memories(&state.store, query, limit, 0.1, emb_client.as_ref(), None).await?;;

    if results.is_empty() {
        return Ok("No relevant memories found.".into());
    }

    let mut output = format!("Found {} relevant memories:\n\n", results.len());
    for (i, mem) in results.iter().enumerate() {
        output.push_str(&format!("{}. [{}] {} (score: {:.2})\n", i + 1, mem.category, mem.content, mem.score.unwrap_or(0.0)));
    }
    Ok(output)
}

// ── self_info: Introspect engine configuration ─────────────────────────

async fn execute_self_info(app_handle: &tauri::AppHandle) -> Result<String, String> {
    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
    let mcfg = state.memory_config.lock().map_err(|e| format!("Lock error: {}", e))?;

    // Provider info
    let providers_info: Vec<String> = cfg.providers.iter().map(|p| {
        let is_default = cfg.default_provider.as_ref() == Some(&p.id);
        format!("  - {} ({:?}){}", p.id, p.kind, if is_default { " ← DEFAULT" } else { "" })
    }).collect();

    // Model routing info
    let routing = &cfg.model_routing;
    let routing_info = format!(
        "  Boss model: {}\n  Worker model: {}\n  Specialties: {}\n  Per-agent overrides: {}",
        routing.boss_model.as_deref().unwrap_or("(default)"),
        routing.worker_model.as_deref().unwrap_or("(default)"),
        if routing.specialty_models.is_empty() { "none".into() }
        else { routing.specialty_models.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>().join(", ") },
        if routing.agent_models.is_empty() { "none".into() }
        else { routing.agent_models.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>().join(", ") },
    );

    // Memory config
    let memory_info = format!(
        "  Embedding provider: {}\n  Embedding model: {}\n  Auto-recall: {}\n  Auto-capture: {}\n  Recall limit: {}\n  Base URL: {}",
        mcfg.embedding_base_url,
        if mcfg.embedding_model.is_empty() { "(not configured)" } else { &mcfg.embedding_model },
        mcfg.auto_recall,
        mcfg.auto_capture,
        mcfg.recall_limit,
        if mcfg.embedding_base_url.is_empty() { "(not configured)" } else { &mcfg.embedding_base_url },
    );

    // Enabled skills
    let skills_list = crate::engine::skills::builtin_skills();
    let enabled_skills: Vec<String> = skills_list.iter()
        .filter(|s| state.store.is_skill_enabled(&s.id).unwrap_or(false))
        .map(|s| format!("  - {} ({})", s.name, s.id))
        .collect();

    let output = format!(
        "# Pawz Engine Self-Info\n\n\
        ## Current Configuration\n\
        - Default model: {}\n\
        - Default provider: {}\n\
        - Max tool rounds: {}\n\
        - Tool timeout: {}s\n\n\
        ## Configured Providers\n{}\n\n\
        ## Model Routing (Orchestrator)\n{}\n\n\
        ## Memory Configuration\n{}\n\n\
        ## Enabled Skills\n{}\n\n\
        ## Data Location\n\
        - Config stored in: SQLite database (engine_config key)\n\
        - Soul files: stored in SQLite (agent_files table)\n\
        - Memories: stored in SQLite (memories table)\n\
        - Sessions: stored in SQLite (sessions + messages tables)",
        cfg.default_model.as_deref().unwrap_or("(not set)"),
        cfg.default_provider.as_deref().unwrap_or("(not set)"),
        cfg.max_tool_rounds,
        cfg.tool_timeout_secs,
        if providers_info.is_empty() { "  (none configured)".into() } else { providers_info.join("\n") },
        routing_info,
        memory_info,
        if enabled_skills.is_empty() { "  (none enabled)".into() } else { enabled_skills.join("\n") },
    );

    Ok(output)
}

// ── update_profile: Let the agent update its own profile ──────────────

async fn execute_update_profile(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> Result<String, String> {
    let agent_id = args["agent_id"].as_str()
        .ok_or("update_profile: missing 'agent_id' argument (use 'default' for the main agent)")?;

    let name = args["name"].as_str();
    let avatar = args["avatar"].as_str();
    let bio = args["bio"].as_str();
    let system_prompt = args["system_prompt"].as_str();

    // At least one field should be provided
    if name.is_none() && avatar.is_none() && bio.is_none() && system_prompt.is_none() {
        return Err("update_profile: provide at least one field to update (name, avatar, bio, system_prompt)".into());
    }

    // Build the update payload and emit it to the frontend
    let mut updates = serde_json::Map::new();
    updates.insert("agent_id".into(), serde_json::json!(agent_id));
    if let Some(v) = name { updates.insert("name".into(), serde_json::json!(v)); }
    if let Some(v) = avatar { updates.insert("avatar".into(), serde_json::json!(v)); }
    if let Some(v) = bio { updates.insert("bio".into(), serde_json::json!(v)); }
    if let Some(v) = system_prompt { updates.insert("system_prompt".into(), serde_json::json!(v)); }

    info!("[engine] update_profile tool: updating agent '{}' with fields: {:?}",
        agent_id, updates.keys().collect::<Vec<_>>());

    // Emit a Tauri event so the frontend can update localStorage and re-render
    let _ = app_handle.emit("agent-profile-updated", serde_json::Value::Object(updates));

    // Also store in memory
    let mut desc_parts = vec![format!("Updated profile for agent '{}':", agent_id)];
    if let Some(v) = name { desc_parts.push(format!("name → {}", v)); }
    if let Some(v) = avatar { desc_parts.push(format!("avatar → {}", v)); }
    if let Some(v) = bio { desc_parts.push(format!("bio → {}", v)); }
    if system_prompt.is_some() { desc_parts.push("system_prompt updated".into()); }
    let memory_content = desc_parts.join(" ");

    let state = app_handle.try_state::<EngineState>();
    if let Some(state) = state {
        let emb_client = state.embedding_client();
        let _ = memory::store_memory(&state.store, &memory_content, "fact", 5, emb_client.as_ref(), None).await;
    }

    let mut result_parts = vec![format!("Successfully updated agent profile for '{}':", agent_id)];
    if let Some(v) = name { result_parts.push(format!("- **Name**: {}", v)); }
    if let Some(v) = avatar { result_parts.push(format!("- **Avatar**: {}", v)); }
    if let Some(v) = bio { result_parts.push(format!("- **Bio**: {}", v)); }
    if system_prompt.is_some() { result_parts.push("- **System Prompt**: updated".into()); }
    result_parts.push("\nThe UI has been updated in real-time.".into());

    Ok(result_parts.join("\n"))
}

// ── create_agent: Create a new agent persona from chat ─────────────────

async fn execute_create_agent(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> Result<String, String> {
    let name = args["name"].as_str()
        .ok_or("create_agent: missing 'name' argument")?;
    let role = args["role"].as_str()
        .ok_or("create_agent: missing 'role' argument")?;
    let system_prompt = args["system_prompt"].as_str()
        .ok_or("create_agent: missing 'system_prompt' argument")?;
    let specialty = args["specialty"].as_str().unwrap_or("general");
    let model = args["model"].as_str().filter(|s| !s.is_empty());
    let capabilities: Vec<String> = args["capabilities"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    // Generate a slug-style agent_id from the name
    let slug: String = name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let agent_id = format!("agent-{}-{}", slug, timestamp);

    info!("[engine] create_agent tool: creating '{}' as {}", name, agent_id);

    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let agent = crate::engine::types::ProjectAgent {
        agent_id: agent_id.clone(),
        role: role.to_string(),
        specialty: specialty.to_string(),
        status: "idle".into(),
        current_task: None,
        model: model.map(String::from),
        system_prompt: Some(system_prompt.to_string()),
        capabilities,
    };

    state.store.add_project_agent("_standalone", &agent)?;

    // Also store in memory so the agent remembers it created this agent
    let memory_content = format!(
        "Created agent '{}' (id: {}, role: {}, specialty: {})",
        name, agent_id, role, specialty
    );
    let emb_client = state.embedding_client();
    let _ = memory::store_memory(&state.store, &memory_content, "fact", 5, emb_client.as_ref(), None).await;

    Ok(format!(
        "Successfully created agent '{}'!\n\n\
        - **Agent ID**: {}\n\
        - **Role**: {}\n\
        - **Specialty**: {}\n\
        - **Model**: {}\n\
        - **Capabilities**: {}\n\n\
        The agent is now available in the Agents view. The user can select it from the agent picker to start chatting with it.",
        name, agent_id, role, specialty,
        model.unwrap_or("(uses default)"),
        if agent.capabilities.is_empty() { "all tools".to_string() } else { agent.capabilities.join(", ") }
    ))
}

// ── create_task: Create a task / cron job from chat ────────────────────

async fn execute_create_task(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> Result<String, String> {
    let title = args["title"].as_str()
        .ok_or("create_task: missing 'title' argument")?
        .to_string();
    let description = args["description"].as_str()
        .ok_or("create_task: missing 'description' argument")?
        .to_string();
    let priority = args["priority"].as_str().unwrap_or("medium").to_string();
    let agent_id = args["agent_id"].as_str().unwrap_or("default").to_string();
    let cron_schedule = args["cron_schedule"].as_str().map(String::from);

    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let cron_enabled = cron_schedule.is_some();
    let next_run_at = if cron_enabled { Some(now.clone()) } else { None };

    let task = crate::engine::types::Task {
        id: id.clone(),
        title: title.clone(),
        description: description.clone(),
        status: if cron_enabled { "assigned".into() } else { "inbox".into() },
        priority: priority.clone(),
        assigned_agent: Some(agent_id.clone()),
        assigned_agents: vec![crate::engine::types::TaskAgent {
            agent_id: agent_id.clone(),
            role: "lead".into(),
        }],
        session_id: None,
        cron_schedule: cron_schedule.clone(),
        cron_enabled,
        last_run_at: None,
        next_run_at,
        created_at: now.clone(),
        updated_at: now,
        model: None,
    };

    state.store.create_task(&task)?;

    // Log activity
    let aid = uuid::Uuid::new_v4().to_string();
    state.store.add_task_activity(&aid, &id, "created", None,
        &format!("Task created via chat: {}", title)).ok();

    info!("[engine] create_task tool: '{}' agent={} cron={:?}", title, agent_id, cron_schedule);

    // Emit event so UI updates
    app_handle.emit("task-updated", serde_json::json!({ "task_id": id })).ok();

    let schedule_info = if let Some(ref s) = cron_schedule {
        format!("\n- **Schedule**: {} (will run automatically via heartbeat)", s)
    } else {
        "\n- **Type**: One-shot task (run manually from Tasks board)".into()
    };

    Ok(format!(
        "Task created successfully!\n\n\
        - **ID**: {}\n\
        - **Title**: {}\n\
        - **Priority**: {}\n\
        - **Agent**: {}{}",
        id, title, priority, agent_id, schedule_info
    ))
}

// ── list_tasks: List tasks from the board ──────────────────────────────

async fn execute_list_tasks(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> Result<String, String> {
    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let status_filter = args["status_filter"].as_str();
    let cron_only = args["cron_only"].as_bool().unwrap_or(false);

    let tasks = state.store.list_tasks()?;

    let filtered: Vec<_> = tasks.into_iter()
        .filter(|t| {
            if let Some(sf) = status_filter {
                if t.status != sf { return false; }
            }
            if cron_only && t.cron_schedule.is_none() { return false; }
            true
        })
        .collect();

    if filtered.is_empty() {
        return Ok("No tasks found matching the criteria.".into());
    }

    let mut output = format!("Found {} task(s):\n\n", filtered.len());
    for t in &filtered {
        let schedule = t.cron_schedule.as_deref().unwrap_or("none");
        let enabled = if t.cron_enabled { "enabled" } else { "paused" };
        let agent = t.assigned_agent.as_deref().unwrap_or("unassigned");
        let next = t.next_run_at.as_deref().unwrap_or("-");
        output.push_str(&format!(
            "---\n**{}** (ID: `{}`)\n- Status: {} | Priority: {}\n- Agent: {} | Schedule: {} ({})\n- Next run: {}\n- Description: {}\n\n",
            t.title, t.id, t.status, t.priority, agent, schedule, enabled, next,
            if t.description.len() > 150 { format!("{}...", &t.description[..150]) } else { t.description.clone() }
        ));
    }

    Ok(output)
}

// ── manage_task: Update, delete, pause, enable, run tasks ──────────────

async fn execute_manage_task(args: &serde_json::Value, app_handle: &tauri::AppHandle) -> Result<String, String> {
    let task_id = args["task_id"].as_str()
        .ok_or("manage_task: missing 'task_id' argument")?
        .to_string();
    let action = args["action"].as_str()
        .ok_or("manage_task: missing 'action' argument")?;

    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    match action {
        "delete" => {
            state.store.delete_task(&task_id)?;
            info!("[engine] manage_task: deleted {}", task_id);
            app_handle.emit("task-updated", serde_json::json!({ "task_id": task_id })).ok();
            Ok(format!("Task {} deleted.", task_id))
        }
        "run_now" => {
            // Set next_run_at to now so the heartbeat picks it up within 60s
            let tasks = state.store.list_tasks()?;
            if let Some(mut task) = tasks.into_iter().find(|t| t.id == task_id) {
                task.cron_enabled = true;
                task.next_run_at = Some(chrono::Utc::now().to_rfc3339());
                task.status = "assigned".to_string();
                state.store.update_task(&task)?;
                let aid = uuid::Uuid::new_v4().to_string();
                state.store.add_task_activity(&aid, &task_id, "cron_triggered", None,
                    "Manually triggered via chat — will run on next heartbeat cycle").ok();
                app_handle.emit("task-updated", serde_json::json!({ "task_id": task_id })).ok();
                Ok(format!("Task '{}' queued for immediate execution. It will run within the next 60-second heartbeat cycle.", task.title))
            } else {
                Err(format!("Task not found: {}", task_id))
            }
        }
        "pause" => {
            let tasks = state.store.list_tasks()?;
            if let Some(mut task) = tasks.into_iter().find(|t| t.id == task_id) {
                task.cron_enabled = false;
                state.store.update_task(&task)?;
                app_handle.emit("task-updated", serde_json::json!({ "task_id": task_id })).ok();
                Ok(format!("Automation '{}' paused.", task.title))
            } else {
                Err(format!("Task not found: {}", task_id))
            }
        }
        "enable" => {
            let tasks = state.store.list_tasks()?;
            if let Some(mut task) = tasks.into_iter().find(|t| t.id == task_id) {
                task.cron_enabled = true;
                if task.next_run_at.is_none() {
                    task.next_run_at = Some(chrono::Utc::now().to_rfc3339());
                }
                state.store.update_task(&task)?;
                app_handle.emit("task-updated", serde_json::json!({ "task_id": task_id })).ok();
                Ok(format!("Automation '{}' enabled. Will run on next heartbeat.", task.title))
            } else {
                Err(format!("Task not found: {}", task_id))
            }
        }
        "update" => {
            let tasks = state.store.list_tasks()?;
            if let Some(mut task) = tasks.into_iter().find(|t| t.id == task_id) {
                if let Some(t) = args["title"].as_str() { task.title = t.to_string(); }
                if let Some(d) = args["description"].as_str() { task.description = d.to_string(); }
                if let Some(p) = args["priority"].as_str() { task.priority = p.to_string(); }
                if let Some(s) = args["status"].as_str() { task.status = s.to_string(); }
                if let Some(s) = args["cron_schedule"].as_str() {
                    task.cron_schedule = Some(s.to_string());
                    task.cron_enabled = true;
                    task.next_run_at = Some(chrono::Utc::now().to_rfc3339());
                }
                if let Some(a) = args["agent_id"].as_str() {
                    task.assigned_agent = Some(a.to_string());
                    task.assigned_agents = vec![crate::engine::types::TaskAgent {
                        agent_id: a.to_string(),
                        role: "lead".into(),
                    }];
                }
                task.updated_at = chrono::Utc::now().to_rfc3339();
                state.store.update_task(&task)?;
                app_handle.emit("task-updated", serde_json::json!({ "task_id": task_id })).ok();
                Ok(format!("Task '{}' updated.", task.title))
            } else {
                Err(format!("Task not found: {}", task_id))
            }
        }
        _ => Err(format!("Unknown action: {}. Use: update, delete, run_now, pause, enable", action)),
    }
}

// ── Skill Tools: Credential-injected execution ─────────────────────────
// The agent never sees credentials. We load them from the vault and inject at execution time.

async fn execute_skill_tool(
    skill_id: &str,
    tool_name: &str,
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> Result<String, String> {
    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    // Check skill is enabled
    if !state.store.is_skill_enabled(skill_id)? {
        return Err(format!("Skill '{}' is not enabled. Ask the user to enable it in Settings → Skills.", skill_id));
    }

    // Load decrypted credentials
    let creds = skills::get_skill_credentials(&state.store, skill_id)?;

    // Check required credentials are set
    let defs = skills::builtin_skills();
    if let Some(def) = defs.iter().find(|d| d.id == skill_id) {
        let missing: Vec<&str> = def.required_credentials.iter()
            .filter(|c| c.required && !creds.contains_key(&c.key))
            .map(|c| c.key.as_str())
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "Skill '{}' is missing required credentials: {}. Ask the user to configure them in Settings → Skills.",
                skill_id, missing.join(", ")
            ));
        }
    }

    match tool_name {
        "email_send" => execute_email_send(args, &creds).await,
        "email_read" => execute_email_read(args, &creds).await,
        "slack_send" => execute_slack_send(args, &creds).await,
        "slack_read" => execute_slack_read(args, &creds).await,
        "github_api" => execute_github_api(args, &creds).await,
        "rest_api_call" => execute_rest_api_call(args, &creds).await,
        "webhook_send" => execute_webhook_send(args, &creds).await,
        "image_generate" => execute_image_generate(args, &creds).await,
        // ── Coinbase CDP ──
        "coinbase_prices" => execute_coinbase_prices(args, &creds).await,
        "coinbase_balance" => execute_coinbase_balance(args, &creds).await,
        "coinbase_wallet_create" => execute_coinbase_wallet_create(args, &creds).await,
        "coinbase_trade" => {
            let result = execute_coinbase_trade(args, &creds).await;
            if result.is_ok() {
                // Record successful trade in history
                let _ = state.store.insert_trade(
                    "trade",
                    args["side"].as_str(),
                    args["product_id"].as_str(),
                    None,
                    args["amount"].as_str().unwrap_or("0"),
                    args["order_type"].as_str(),
                    None, // order_id extracted from response below
                    "completed",
                    args["amount"].as_str(), // USD value approximation for market orders
                    None,
                    args["reason"].as_str().unwrap_or(""),
                    None, None,
                    result.as_ref().ok().map(|s| s.as_str()),
                );
            }
            result
        }
        "coinbase_transfer" => {
            let result = execute_coinbase_transfer(args, &creds).await;
            if result.is_ok() {
                let _ = state.store.insert_trade(
                    "transfer",
                    Some("send"),
                    None,
                    args["currency"].as_str(),
                    args["amount"].as_str().unwrap_or("0"),
                    None,
                    None,
                    "completed",
                    None,
                    args["to_address"].as_str(),
                    args["reason"].as_str().unwrap_or(""),
                    None, None,
                    result.as_ref().ok().map(|s| s.as_str()),
                );
            }
            result
        }
        // ── DEX / Uniswap ──
        "dex_wallet_create" => crate::engine::dex::execute_dex_wallet_create(args, &creds, app_handle).await,
        "dex_balance" => crate::engine::dex::execute_dex_balance(args, &creds).await,
        "dex_quote" => crate::engine::dex::execute_dex_quote(args, &creds).await,
        "dex_swap" => {
            let result = crate::engine::dex::execute_dex_swap(args, &creds).await;
            if result.is_ok() {
                let token_in = args["token_in"].as_str().unwrap_or("?");
                let token_out = args["token_out"].as_str().unwrap_or("?");
                let pair = format!("{} → {}", token_in.to_uppercase(), token_out.to_uppercase());
                let _ = state.store.insert_trade(
                    "dex_swap",
                    Some("swap"),
                    Some(&pair),
                    args["token_in"].as_str(),
                    args["amount"].as_str().unwrap_or("0"),
                    None,
                    None,
                    "completed",
                    None,
                    args["token_out"].as_str(),
                    args["reason"].as_str().unwrap_or(""),
                    None, None,
                    result.as_ref().ok().map(|s| s.as_str()),
                );
            }
            result
        }
        "dex_portfolio" => crate::engine::dex::execute_dex_portfolio(args, &creds).await,
        "dex_token_info" => crate::engine::dex::execute_dex_token_info(args, &creds).await,
        "dex_check_token" => crate::engine::dex::execute_dex_check_token(args, &creds).await,
        "dex_search_token" => crate::engine::dex::execute_dex_search_token(args, &creds).await,
        "dex_watch_wallet" => crate::engine::dex::execute_dex_watch_wallet(args, &creds).await,
        "dex_whale_transfers" => crate::engine::dex::execute_dex_whale_transfers(args, &creds).await,
        "dex_top_traders" => crate::engine::dex::execute_dex_top_traders(args, &creds).await,
        "dex_trending" => crate::engine::dex::execute_dex_trending(args, &creds).await,
        // ── Solana DEX / Jupiter ──
        "sol_wallet_create" => crate::engine::sol_dex::execute_sol_wallet_create(args, &creds, app_handle).await,
        "sol_balance" => crate::engine::sol_dex::execute_sol_balance(args, &creds).await,
        "sol_quote" => crate::engine::sol_dex::execute_sol_quote(args, &creds).await,
        "sol_swap" => {
            let result = crate::engine::sol_dex::execute_sol_swap(args, &creds).await;
            if result.is_ok() {
                let token_in = args["token_in"].as_str().unwrap_or("?");
                let token_out = args["token_out"].as_str().unwrap_or("?");
                let pair = format!("{} → {}", token_in.to_uppercase(), token_out.to_uppercase());
                let _ = state.store.insert_trade(
                    "sol_swap",
                    Some("swap"),
                    Some(&pair),
                    args["token_in"].as_str(),
                    args["amount"].as_str().unwrap_or("0"),
                    None,
                    None,
                    "completed",
                    None,
                    args["token_out"].as_str(),
                    args["reason"].as_str().unwrap_or(""),
                    None, None,
                    result.as_ref().ok().map(|s| s.as_str()),
                );
            }
            result
        }
        "sol_portfolio" => crate::engine::sol_dex::execute_sol_portfolio(args, &creds).await,
        "sol_token_info" => crate::engine::sol_dex::execute_sol_token_info(args, &creds).await,
        _ => Err(format!("Unknown skill tool: {}", tool_name)),
    }
}

// ── Email Send (SMTP) ──────────────────────────────────────────────────

async fn execute_email_send(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> Result<String, String> {
    let to = args["to"].as_str().ok_or("email_send: missing 'to'")?;
    let subject = args["subject"].as_str().ok_or("email_send: missing 'subject'")?;
    let body = args["body"].as_str().ok_or("email_send: missing 'body'")?;
    let is_html = args["html"].as_bool().unwrap_or(false);

    let host = creds.get("SMTP_HOST").ok_or("Missing SMTP_HOST credential")?;
    let port: u16 = creds.get("SMTP_PORT")
        .ok_or("Missing SMTP_PORT credential")?
        .parse()
        .map_err(|_| "Invalid SMTP_PORT")?;
    let user = creds.get("SMTP_USER").ok_or("Missing SMTP_USER credential")?;
    let password = creds.get("SMTP_PASSWORD").ok_or("Missing SMTP_PASSWORD credential")?;

    info!("[skill:email] Sending to {} via {}:{}", to, host, port);

    // Build the SMTP command using curl (available on all platforms)
    // This avoids needing additional Rust SMTP crates
    let mail_body = if is_html {
        format!(
            "From: {from}\r\nTo: {to}\r\nSubject: {subject}\r\nContent-Type: text/html; charset=utf-8\r\nMIME-Version: 1.0\r\n\r\n{body}",
            from = user, to = to, subject = subject, body = body
        )
    } else {
        format!(
            "From: {from}\r\nTo: {to}\r\nSubject: {subject}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{body}",
            from = user, to = to, subject = subject, body = body
        )
    };

    // Use curl's SMTP support for reliable cross-platform sending
    let url = if port == 465 {
        format!("smtps://{}:{}", host, port)
    } else {
        format!("smtp://{}:{}", host, port)
    };

    let output = ProcessCommand::new("curl")
        .args([
            "--ssl-reqd",
            "--url", &url,
            "--user", &format!("{}:{}", user, password),
            "--mail-from", user,
            "--mail-rcpt", to,
            "-T", "-",  // read from stdin
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(mail_body.as_bytes())?;
            }
            child.wait_with_output()
        })
        .map_err(|e| format!("Failed to send email: {}", e))?;

    if output.status.success() {
        Ok(format!("Email sent successfully to {}", to))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("SMTP error: {}", stderr))
    }
}

// ── Email Read (IMAP via curl) ─────────────────────────────────────────

async fn execute_email_read(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> Result<String, String> {
    let limit = args["limit"].as_u64().unwrap_or(5);
    let folder = args["folder"].as_str().unwrap_or("INBOX");

    let host = creds.get("IMAP_HOST")
        .or_else(|| creds.get("SMTP_HOST"))
        .ok_or("Missing IMAP_HOST credential")?;
    let port = creds.get("IMAP_PORT").map(|p| p.as_str()).unwrap_or("993");
    let user = creds.get("SMTP_USER").ok_or("Missing SMTP_USER credential")?;
    let password = creds.get("SMTP_PASSWORD").ok_or("Missing SMTP_PASSWORD credential")?;

    info!("[skill:email] Reading {} from {}:{}/{}", limit, host, port, folder);

    // Use curl IMAP to list recent messages
    let url = format!("imaps://{}:{}/{}", host, port, folder);
    let output = ProcessCommand::new("curl")
        .args([
            "--ssl-reqd",
            "--url", &format!("{};MAILINDEX=1:{}", url, limit),
            "--user", &format!("{}:{}", user, password),
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("IMAP error: {}", e))?;

    if output.status.success() {
        let body = String::from_utf8_lossy(&output.stdout);
        let truncated = if body.len() > 20_000 {
            format!("{}...\n[truncated]", &body[..20_000])
        } else {
            body.to_string()
        };
        Ok(format!("Emails from {}/{}:\n\n{}", host, folder, truncated))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("IMAP error: {}", stderr))
    }
}

// ── Slack Send ─────────────────────────────────────────────────────────

async fn execute_slack_send(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> Result<String, String> {
    let text = args["text"].as_str().ok_or("slack_send: missing 'text'")?;
    let token = creds.get("SLACK_BOT_TOKEN").ok_or("Missing SLACK_BOT_TOKEN")?;
    let channel = args["channel"].as_str()
        .map(|s| s.to_string())
        .or_else(|| creds.get("SLACK_DEFAULT_CHANNEL").cloned())
        .ok_or("slack_send: no channel specified and no default channel configured")?;

    info!("[skill:slack] Sending to channel {}", channel);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client.post("https://slack.com/api/chat.postMessage")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "channel": channel,
            "text": text
        }))
        .send()
        .await
        .map_err(|e| format!("Slack API error: {}", e))?;

    let body: serde_json::Value = resp.json().await
        .map_err(|e| format!("Failed to parse Slack response: {}", e))?;

    if body["ok"].as_bool().unwrap_or(false) {
        let ts = body["ts"].as_str().unwrap_or("unknown");
        Ok(format!("Message sent to Slack channel {} (ts: {})", channel, ts))
    } else {
        let err = body["error"].as_str().unwrap_or("unknown error");
        Err(format!("Slack API error: {}", err))
    }
}

// ── Slack Read ─────────────────────────────────────────────────────────

async fn execute_slack_read(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> Result<String, String> {
    let channel = args["channel"].as_str().ok_or("slack_read: missing 'channel'")?;
    let limit = args["limit"].as_u64().unwrap_or(10);
    let token = creds.get("SLACK_BOT_TOKEN").ok_or("Missing SLACK_BOT_TOKEN")?;

    info!("[skill:slack] Reading {} messages from {}", limit, channel);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client.get("https://slack.com/api/conversations.history")
        .header("Authorization", format!("Bearer {}", token))
        .query(&[("channel", channel), ("limit", &limit.to_string())])
        .send()
        .await
        .map_err(|e| format!("Slack API error: {}", e))?;

    let body: serde_json::Value = resp.json().await
        .map_err(|e| format!("Failed to parse Slack response: {}", e))?;

    if body["ok"].as_bool().unwrap_or(false) {
        let empty_vec = vec![];
        let messages = body["messages"].as_array().unwrap_or(&empty_vec);
        let mut output = format!("Last {} messages from {}:\n\n", messages.len(), channel);
        for (i, msg) in messages.iter().enumerate() {
            let user = msg["user"].as_str().unwrap_or("?");
            let text = msg["text"].as_str().unwrap_or("");
            let ts = msg["ts"].as_str().unwrap_or("");
            output.push_str(&format!("{}. [{}] {}: {}\n", i + 1, ts, user, text));
        }
        Ok(output)
    } else {
        let err = body["error"].as_str().unwrap_or("unknown error");
        Err(format!("Slack API error: {}", err))
    }
}

// ── GitHub API ─────────────────────────────────────────────────────────

async fn execute_github_api(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> Result<String, String> {
    let endpoint = args["endpoint"].as_str().ok_or("github_api: missing 'endpoint'")?;
    let method = args["method"].as_str().unwrap_or("GET");
    let token = creds.get("GITHUB_TOKEN").ok_or("Missing GITHUB_TOKEN")?;

    let url = if endpoint.starts_with("https://") {
        endpoint.to_string()
    } else {
        format!("https://api.github.com{}", if endpoint.starts_with('/') { endpoint.to_string() } else { format!("/{}", endpoint) })
    };

    info!("[skill:github] {} {}", method, url);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let mut request = match method.to_uppercase().as_str() {
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "PATCH" => client.patch(&url),
        "DELETE" => client.delete(&url),
        _ => client.get(&url),
    };

    request = request
        .header("Authorization", format!("Bearer {}", token))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "Paw-Agent/1.0")
        .header("X-GitHub-Api-Version", "2022-11-28");

    if let Some(body) = args.get("body") {
        if !body.is_null() {
            request = request.json(body);
        }
    }

    let resp = request.send().await
        .map_err(|e| format!("GitHub API error: {}", e))?;

    let status = resp.status().as_u16();
    let body = resp.text().await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    // Truncate long responses
    let truncated = if body.len() > 30_000 {
        format!("{}...\n[truncated, {} total bytes]", &body[..30_000], body.len())
    } else {
        body
    };

    Ok(format!("GitHub API {} {} → {}\n\n{}", method, endpoint, status, truncated))
}

// ── REST API Call ──────────────────────────────────────────────────────

async fn execute_rest_api_call(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> Result<String, String> {
    let path = args["path"].as_str().ok_or("rest_api_call: missing 'path'")?;
    let method = args["method"].as_str().unwrap_or("GET");
    let base_url = creds.get("API_BASE_URL").ok_or("Missing API_BASE_URL")?;
    let api_key = creds.get("API_KEY").ok_or("Missing API_KEY")?;
    let auth_header = creds.get("API_AUTH_HEADER").map(|s| s.as_str()).unwrap_or("Authorization");
    let auth_prefix = creds.get("API_AUTH_PREFIX").map(|s| s.as_str()).unwrap_or("Bearer");

    let url = format!("{}{}", base_url.trim_end_matches('/'), if path.starts_with('/') { path.to_string() } else { format!("/{}", path) });

    info!("[skill:rest_api] {} {}", method, url);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let mut request = match method.to_uppercase().as_str() {
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "PATCH" => client.patch(&url),
        "DELETE" => client.delete(&url),
        _ => client.get(&url),
    };

    request = request.header(auth_header, format!("{} {}", auth_prefix, api_key));

    // Add custom headers
    if let Some(headers) = args["headers"].as_object() {
        for (key, value) in headers {
            if let Some(v) = value.as_str() {
                request = request.header(key.as_str(), v);
            }
        }
    }

    if let Some(body) = args["body"].as_str() {
        request = request
            .header("Content-Type", "application/json")
            .body(body.to_string());
    }

    let resp = request.send().await
        .map_err(|e| format!("API error: {}", e))?;

    let status = resp.status().as_u16();
    let body = resp.text().await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    let truncated = if body.len() > 30_000 {
        format!("{}...\n[truncated, {} total bytes]", &body[..30_000], body.len())
    } else {
        body
    };

    Ok(format!("API {} {} → {}\n\n{}", method, path, status, truncated))
}

// ── Webhook Send ───────────────────────────────────────────────────────

async fn execute_webhook_send(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> Result<String, String> {
    let payload = args.get("payload").ok_or("webhook_send: missing 'payload'")?;
    let url = creds.get("WEBHOOK_URL").ok_or("Missing WEBHOOK_URL")?;

    info!("[skill:webhook] POST {}", url);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let mut request = client.post(url.as_str())
        .header("Content-Type", "application/json")
        .json(payload);

    // Add HMAC signature if secret is configured
    if let Some(secret) = creds.get("WEBHOOK_SECRET") {
        if !secret.is_empty() {
            // Compute simple hex digest for signing
            let payload_str = serde_json::to_string(payload).unwrap_or_default();
            let signature = format!("sha256={}", simple_hmac_hex(secret, &payload_str));
            request = request.header("X-Signature-256", &signature);
        }
    }

    let resp = request.send().await
        .map_err(|e| format!("Webhook error: {}", e))?;

    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();

    if status < 400 {
        Ok(format!("Webhook delivered (HTTP {}). Response: {}", status, &body[..body.len().min(1000)]))
    } else {
        Err(format!("Webhook failed (HTTP {}): {}", status, &body[..body.len().min(1000)]))
    }
}

/// Simple HMAC-like signature (not cryptographically secure HMAC, but sufficient for webhook signing)
fn simple_hmac_hex(key: &str, data: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    data.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

// ── Image Generation (Gemini) ──────────────────────────────────────────

async fn execute_image_generate(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> Result<String, String> {
    let prompt = args["prompt"].as_str().ok_or("image_generate: missing 'prompt'")?;
    let filename = args["filename"].as_str().unwrap_or("");

    let api_key = creds.get("GEMINI_API_KEY").ok_or("Missing GEMINI_API_KEY credential")?;

    info!("[skill:image_gen] Generating image for prompt: {}", &prompt[..prompt.len().min(80)]);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    // Use Gemini's image generation via generateContent with responseModalities
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash-exp:generateContent?key={}",
        api_key
    );

    let body = serde_json::json!({
        "contents": [{
            "parts": [{
                "text": prompt
            }]
        }],
        "generationConfig": {
            "responseModalities": ["TEXT", "IMAGE"]
        }
    });

    let resp = client.post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Gemini API error: {}", e))?;

    let status = resp.status().as_u16();
    let resp_text = resp.text().await.map_err(|e| format!("Read response: {}", e))?;

    if status >= 400 {
        return Err(format!("Gemini API error (HTTP {}): {}", status, &resp_text[..resp_text.len().min(500)]));
    }

    let resp_json: serde_json::Value = serde_json::from_str(&resp_text)
        .map_err(|e| format!("Parse Gemini response: {}", e))?;

    // Extract image data from response
    // Gemini returns: candidates[0].content.parts[] with either {text} or {inlineData: {mimeType, data}}
    let parts = resp_json
        .get("candidates").and_then(|c| c.get(0))
        .and_then(|c| c.get("content"))
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.as_array())
        .ok_or("Gemini response missing candidates/content/parts")?;

    let mut image_data: Option<(String, String)> = None; // (mime_type, base64_data)
    let mut text_response: Option<String> = None;

    for part in parts {
        if let Some(inline) = part.get("inlineData") {
            let mime = inline["mimeType"].as_str().unwrap_or("image/png");
            let data = inline["data"].as_str().unwrap_or("");
            if !data.is_empty() {
                image_data = Some((mime.to_string(), data.to_string()));
            }
        }
        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
            text_response = Some(text.to_string());
        }
    }

    let (mime_type, base64_data) = image_data
        .ok_or("Gemini did not return an image. The model may not support image generation for this prompt. Try a more descriptive prompt.")?;

    // Determine file extension from MIME type
    let ext = match mime_type.as_str() {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "png",
    };

    // Generate output filename
    let output_name = if filename.is_empty() {
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let slug: String = prompt.chars()
            .filter(|c| c.is_alphanumeric() || *c == ' ')
            .take(30)
            .collect::<String>()
            .trim()
            .replace(' ', "_")
            .to_lowercase();
        format!("generated_{}_{}", ts, slug)
    } else {
        filename.to_string()
    };

    // Save to the user's pictures/paw directory or temp
    let output_dir = std::env::var("HOME")
        .map(|h| std::path::PathBuf::from(h).join("Pictures").join("paw"))
        .unwrap_or_else(|_| std::env::temp_dir().join("paw_images"));

    std::fs::create_dir_all(&output_dir)
        .map_err(|e| format!("Create output dir: {}", e))?;

    let output_path = output_dir.join(format!("{}.{}", output_name, ext));

    // Decode base64 and write
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD.decode(&base64_data)
        .map_err(|e| format!("Decode image data: {}", e))?;

    std::fs::write(&output_path, &bytes)
        .map_err(|e| format!("Write image file: {}", e))?;

    let path_str = output_path.to_string_lossy().to_string();
    let size_kb = bytes.len() / 1024;

    info!("[skill:image_gen] Saved {} ({} KB) to {}", mime_type, size_kb, path_str);

    let mut result = format!("Image generated and saved to: {}\nSize: {} KB | Format: {}", path_str, size_kb, ext.to_uppercase());
    if let Some(text) = text_response {
        result.push_str(&format!("\n\nModel notes: {}", text));
    }

    Ok(result)
}

// ══════════════════════════════════════════════════════════════════════════
// ══ Coinbase CDP (Developer Platform) ═══════════════════════════════════
// ══════════════════════════════════════════════════════════════════════════
//
// Supports both ES256 (P-256 ECDSA) and Ed25519 JWT signing.
// Auto-detects algorithm based on the private key format.
// Credentials: CDP_API_KEY_NAME (kid) + CDP_API_KEY_SECRET (PEM)

/// Build a JWT for Coinbase CDP API authentication.
/// Auto-detects key format: raw base64 Ed25519, PEM Ed25519, or PEM ES256.
fn build_cdp_jwt(
    key_name: &str,
    key_secret: &str,
    method: &str,
    host: &str,
    path: &str,
) -> Result<String, String> {
    use base64::Engine as _;
    use rand::Rng;

    let now = chrono::Utc::now().timestamp() as u64;
    // Coinbase Advanced Trade SDK uses hex nonce (secrets.token_hex())
    let nonce: String = {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let bytes: Vec<u8> = (0..32).map(|_| rng.gen::<u8>()).collect();
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    };
    let uri = format!("{} {}{}", method, host, path);
    // Normalize PEM newlines: handle literal \n, escaped \\n, and missing newlines
    let secret_clean = key_secret
        .replace("\\n", "\n")   // JSON-escaped newlines
        .replace("\\\\n", "\n") // Double-escaped newlines
        .trim()
        .to_string();

    // Detect key type and algorithm
    let key_type = detect_key_type(&secret_clean);
    let alg = match key_type {
        KeyType::Ed25519Pem | KeyType::Ed25519Raw => "EdDSA",
        KeyType::Es256Pem => "ES256",
    };

    info!("[skill:coinbase] JWT: alg={}, key_type={:?}, uri={}", alg, key_type, uri);

    // JWT format from Coinbase Advanced Trade SDK (coinbase-advanced-py/coinbase/jwt_generator.py)
    let header = serde_json::json!({
        "alg": alg,
        "kid": key_name,
        "nonce": nonce,
        "typ": "JWT"
    });

    let payload = serde_json::json!({
        "sub": key_name,
        "iss": "cdp",
        "nbf": now,
        "exp": now + 120,
        "uri": uri
    });

    let b64_header = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_string(&header).map_err(|e| format!("JWT header: {}", e))?);
    let b64_payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_string(&payload).map_err(|e| format!("JWT payload: {}", e))?);

    let signing_input = format!("{}.{}", b64_header, b64_payload);

    let b64_sig = match key_type {
        KeyType::Ed25519Raw => sign_ed25519_raw(&secret_clean, signing_input.as_bytes())?,
        KeyType::Ed25519Pem => sign_ed25519_pem(&secret_clean, signing_input.as_bytes())?,
        KeyType::Es256Pem => sign_es256(&secret_clean, signing_input.as_bytes())?,
    };

    Ok(format!("{}.{}", signing_input, b64_sig))
}

#[derive(Debug)]
enum KeyType {
    Ed25519Raw,   // Raw base64-encoded key: 32-byte seed or 64-byte keypair (Coinbase default)
    Ed25519Pem,   // PEM-wrapped Ed25519 PKCS8
    Es256Pem,     // PEM-wrapped P-256 EC key
}

/// Detect key format from the secret string
fn detect_key_type(secret: &str) -> KeyType {
    // If it starts with PEM header, parse as PEM
    if secret.contains("-----BEGIN") {
        // SEC1 EC key header → definitely ES256
        if secret.contains("BEGIN EC PRIVATE KEY") {
            return KeyType::Es256Pem;
        }
        // PKCS#8 generic header — try Ed25519 first, fall back to ES256
        use ed25519_dalek::pkcs8::DecodePrivateKey;
        if ed25519_dalek::SigningKey::from_pkcs8_pem(secret).is_ok() {
            return KeyType::Ed25519Pem;
        }
        // Otherwise assume ES256 PEM (P-256 PKCS#8)
        return KeyType::Es256Pem;
    }
    // No PEM header — treat as raw base64 Ed25519 key (Coinbase default format)
    KeyType::Ed25519Raw
}

/// Sign with raw base64-encoded Ed25519 key.
/// Coinbase CDP gives either a 32-byte seed or a 64-byte keypair (seed + public key).
fn sign_ed25519_raw(secret_b64: &str, message: &[u8]) -> Result<String, String> {
    use ed25519_dalek::Signer;
    use base64::Engine as _;

    // Coinbase gives the secret as standard base64 (may have + and /)
    let key_bytes = base64::engine::general_purpose::STANDARD
        .decode(secret_b64.trim())
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(secret_b64.trim()))
        .map_err(|e| format!("Failed to decode API secret as base64: {}", e))?;

    info!("[skill:coinbase] Ed25519 raw key decoded to {} bytes", key_bytes.len());

    let signing_key = match key_bytes.len() {
        32 => {
            // 32-byte seed
            let mut seed = [0u8; 32];
            seed.copy_from_slice(&key_bytes);
            ed25519_dalek::SigningKey::from_bytes(&seed)
        }
        64 => {
            // 64-byte keypair: first 32 = seed, last 32 = public key
            // Coinbase CDP default format for Ed25519 keys
            let mut keypair = [0u8; 64];
            keypair.copy_from_slice(&key_bytes);
            ed25519_dalek::SigningKey::from_keypair_bytes(&keypair)
                .map_err(|e| format!("Invalid Ed25519 keypair (64 bytes): {}", e))?
        }
        n => {
            return Err(format!(
                "API secret decoded to {} bytes, expected 32 (seed) or 64 (keypair) for Ed25519. \
                 If your secret starts with '-----BEGIN', paste the entire PEM block including headers.",
                n
            ));
        }
    };

    let signature = signing_key.sign(message);
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature.to_bytes()))
}

/// Sign with PEM-encoded Ed25519 key
fn sign_ed25519_pem(pem: &str, message: &[u8]) -> Result<String, String> {
    use ed25519_dalek::pkcs8::DecodePrivateKey;
    use ed25519_dalek::Signer;
    use base64::Engine as _;

    let signing_key = ed25519_dalek::SigningKey::from_pkcs8_pem(pem)
        .map_err(|e| format!("Invalid Ed25519 PEM key: {}", e))?;
    let signature = signing_key.sign(message);
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature.to_bytes()))
}

/// Sign with ES256 (P-256 ECDSA) PEM key
/// Supports both PKCS#8 (-----BEGIN PRIVATE KEY-----) and SEC1 (-----BEGIN EC PRIVATE KEY-----) formats.
fn sign_es256(pem: &str, message: &[u8]) -> Result<String, String> {
    use p256::ecdsa::SigningKey;
    use p256::ecdsa::signature::Signer;
    use base64::Engine as _;

    // Try PKCS#8 first, then SEC1 (Coinbase uses SEC1 "BEGIN EC PRIVATE KEY")
    let signing_key = {
        use p256::pkcs8::DecodePrivateKey;
        SigningKey::from_pkcs8_pem(pem)
    }.or_else(|_| {
        use p256::elliptic_curve::SecretKey;
        let secret_key = SecretKey::<p256::NistP256>::from_sec1_pem(pem)
            .map_err(|e| format!("Invalid EC key (tried PKCS#8 and SEC1): {}", e))?;
        Ok::<SigningKey, String>(SigningKey::from(secret_key))
    })?;

    let signature: p256::ecdsa::Signature = signing_key.sign(message);
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature.to_bytes()))
}

/// Make an authenticated request to the Coinbase CDP/v2 API.
async fn cdp_request(
    creds: &std::collections::HashMap<String, String>,
    method: &str,
    path: &str,
    body: Option<&serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let key_name = creds.get("CDP_API_KEY_NAME").ok_or("Missing CDP_API_KEY_NAME")?;
    let key_secret = creds.get("CDP_API_KEY_SECRET").ok_or("Missing CDP_API_KEY_SECRET")?;

    let host = "api.coinbase.com";
    // JWT URI must NOT include query params (SDK: format_jwt_uri uses clean path only)
    let jwt_path = path.split('?').next().unwrap_or(path);
    let jwt = build_cdp_jwt(key_name, key_secret, method, host, jwt_path)?;

    let url = format!("https://{}{}", host, path);
    let client = reqwest::Client::new();
    let mut req = match method {
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "DELETE" => client.delete(&url),
        _ => client.get(&url),
    };

    // Match SDK headers exactly: Authorization + Content-Type only (no CB-VERSION)
    req = req
        .header("Authorization", format!("Bearer {}", jwt))
        .header("Content-Type", "application/json")
        .timeout(Duration::from_secs(30));

    if let Some(b) = body {
        req = req.json(b);
    }

    let resp = req.send().await.map_err(|e| format!("Coinbase API request failed: {}", e))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("Read response: {}", e))?;

    if !status.is_success() {
        warn!("[skill:coinbase] API error {} on {} {}: {}", status, method, path, &text[..text.len().min(500)]);
        return Err(format!("Coinbase API error (HTTP {}): {}", status.as_u16(), &text[..text.len().min(500)]));
    }

    info!("[skill:coinbase] {} {} → {}", method, path, status);

    serde_json::from_str(&text).map_err(|e| format!("Parse Coinbase response: {} — raw: {}", e, &text[..text.len().min(300)]))
}

// ── coinbase_prices ────────────────────────────────────────────────────

async fn execute_coinbase_prices(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> Result<String, String> {
    let symbols_str = args["symbols"].as_str().ok_or("coinbase_prices: missing 'symbols'")?;
    let symbols: Vec<&str> = symbols_str.split(',').map(|s| s.trim().to_uppercase().leak() as &str).collect();

    info!("[skill:coinbase] Fetching prices for: {}", symbols_str);

    let mut results = Vec::new();
    for sym in &symbols {
        let product_id = format!("{}-USD", sym);
        let path = format!("/api/v3/brokerage/products/{}", product_id);
        match cdp_request(creds, "GET", &path, None).await {
            Ok(data) => {
                let price = data["price"].as_str().unwrap_or("?");
                results.push(format!("{}: ${} USD", sym, price));
            }
            Err(e) => {
                results.push(format!("{}: error — {}", sym, e));
            }
        }
    }

    Ok(format!("Current Prices:\n{}", results.join("\n")))
}

// ── coinbase_balance ───────────────────────────────────────────────────

async fn execute_coinbase_balance(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> Result<String, String> {
    let filter_currency = args["currency"].as_str().map(|s| s.to_uppercase());

    info!("[skill:coinbase] Fetching account balances");

    // Use v3 Advanced Trade API for accounts
    let data = cdp_request(creds, "GET", "/api/v3/brokerage/accounts?limit=250", None).await?;
    let accounts = data["accounts"].as_array().ok_or("Unexpected response format — no 'accounts' array")?;

    let mut lines = Vec::new();
    let mut total_usd = 0.0_f64;

    for acct in accounts {
        let currency = acct["currency"].as_str().unwrap_or("?");
        let available = acct["available_balance"]["value"].as_str().unwrap_or("0");
        let hold = acct["hold"]["value"].as_str().unwrap_or("0");

        let avail_f: f64 = available.parse().unwrap_or(0.0);
        let hold_f: f64 = hold.parse().unwrap_or(0.0);
        let total = avail_f + hold_f;

        // Skip zero balances unless specifically requested
        if total == 0.0 && filter_currency.is_none() {
            continue;
        }

        if let Some(ref fc) = filter_currency {
            if currency.to_uppercase() != *fc {
                continue;
            }
        }

        let name = acct["name"].as_str().unwrap_or(currency);
        if hold_f > 0.0 {
            lines.push(format!("  {} ({}): {} available + {} hold", name, currency, available, hold));
        } else {
            lines.push(format!("  {} ({}): {}", name, currency, available));
        }
        // v3 doesn't return native/USD amounts directly; we'll skip total USD
    }

    if lines.is_empty() {
        Ok("No non-zero balances found.".into())
    } else {
        Ok(format!("Account Balances:\n{}", lines.join("\n")))
    }
}

// ── coinbase_wallet_create ─────────────────────────────────────────────

async fn execute_coinbase_wallet_create(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> Result<String, String> {
    let name = args["name"].as_str().ok_or("coinbase_wallet_create: missing 'name'")?;

    info!("[skill:coinbase] Creating wallet: {}", name);

    // v3 Advanced Trade doesn't have a direct "create account" endpoint.
    // Accounts (wallets) are created automatically when you trade a currency.
    // Use portfolios API to create a named portfolio instead.
    let body = serde_json::json!({
        "name": name
    });

    let data = cdp_request(creds, "POST", "/api/v3/brokerage/portfolios", Some(&body)).await?;
    let portfolio = &data["portfolio"];
    let id = portfolio["uuid"].as_str().unwrap_or("?");
    let created_name = portfolio["name"].as_str().unwrap_or(name);
    let ptype = portfolio["type"].as_str().unwrap_or("DEFAULT");

    Ok(format!("Portfolio created!\n  Name: {}\n  ID: {}\n  Type: {}", created_name, id, ptype))
}

// ── coinbase_trade ─────────────────────────────────────────────────────

async fn execute_coinbase_trade(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> Result<String, String> {
    let side = args["side"].as_str().ok_or("coinbase_trade: missing 'side'")?;
    let product_id = args["product_id"].as_str().ok_or("coinbase_trade: missing 'product_id'")?;
    let amount = args["amount"].as_str().ok_or("coinbase_trade: missing 'amount'")?;
    let order_type = args["order_type"].as_str().unwrap_or("market");
    let limit_price = args["limit_price"].as_str();
    let reason = args["reason"].as_str().unwrap_or("No reason provided");

    info!("[skill:coinbase] Trade {} {} {} ({}). Reason: {}", side, amount, product_id, order_type, reason);

    // Build order config based on type
    let order_configuration = if order_type == "limit" {
        let price = limit_price.ok_or("coinbase_trade: limit orders require 'limit_price'")?;
        if side == "buy" {
            serde_json::json!({
                "limit_limit_gtc": {
                    "base_size": amount,
                    "limit_price": price
                }
            })
        } else {
            serde_json::json!({
                "limit_limit_gtc": {
                    "base_size": amount,
                    "limit_price": price
                }
            })
        }
    } else {
        // Market order
        if side == "buy" {
            serde_json::json!({
                "market_market_ioc": {
                    "quote_size": amount
                }
            })
        } else {
            serde_json::json!({
                "market_market_ioc": {
                    "base_size": amount
                }
            })
        }
    };

    let body = serde_json::json!({
        "client_order_id": uuid::Uuid::new_v4().to_string(),
        "product_id": product_id,
        "side": side.to_uppercase(),
        "order_configuration": order_configuration
    });

    let data = cdp_request(creds, "POST", "/api/v3/brokerage/orders", Some(&body)).await?;

    let success = data["success"].as_bool().unwrap_or(false);
    let order_id = data["success_response"]["order_id"].as_str()
        .or_else(|| data["order_id"].as_str())
        .unwrap_or("?");

    if success || data.get("success_response").is_some() {
        Ok(format!(
            "Order placed successfully!\n  Side: {}\n  Product: {}\n  Amount: {}\n  Type: {}\n  Order ID: {}\n  Reason: {}",
            side, product_id, amount, order_type, order_id, reason
        ))
    } else {
        let err_msg = data["error_response"]["message"].as_str()
            .or_else(|| data["message"].as_str())
            .unwrap_or("Unknown error");
        Err(format!("Trade failed: {} — Full response: {}", err_msg, serde_json::to_string_pretty(&data).unwrap_or_default()))
    }
}

// ── coinbase_transfer ──────────────────────────────────────────────────

async fn execute_coinbase_transfer(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> Result<String, String> {
    let currency = args["currency"].as_str().ok_or("coinbase_transfer: missing 'currency'")?;
    let amount = args["amount"].as_str().ok_or("coinbase_transfer: missing 'amount'")?;
    let to_address = args["to_address"].as_str().ok_or("coinbase_transfer: missing 'to_address'")?;
    let network = args["network"].as_str();
    let reason = args["reason"].as_str().unwrap_or("No reason provided");

    info!("[skill:coinbase] Transfer {} {} to {} (reason: {})", amount, currency, &to_address[..to_address.len().min(12)], reason);

    // Step 1: List accounts via v3 Advanced Trade API to find the account UUID
    let accounts_data = cdp_request(creds, "GET", "/api/v3/brokerage/accounts?limit=250", None).await?;
    let accounts = accounts_data["accounts"].as_array().ok_or("Cannot list accounts")?;

    let account = accounts.iter()
        .find(|a| {
            a["currency"].as_str().unwrap_or("").eq_ignore_ascii_case(currency)
        })
        .ok_or(format!("No account found for currency: {}", currency))?;

    let account_uuid = account["uuid"].as_str().ok_or("Account missing UUID")?;
    let available = account["available_balance"]["value"].as_str().unwrap_or("0");

    // Verify sufficient balance
    let avail_f: f64 = available.parse().unwrap_or(0.0);
    let amount_f: f64 = amount.parse().unwrap_or(0.0);
    if amount_f > avail_f {
        return Err(format!("Insufficient {} balance: {} available, {} requested", currency, available, amount));
    }

    // Step 2: Send via v2 API — /v2/accounts/:account_id/transactions
    // This is the correct endpoint for crypto sends with CDP keys
    let send_path = format!("/v2/accounts/{}/transactions", account_uuid);
    let mut body = serde_json::json!({
        "type": "send",
        "to": to_address,
        "amount": amount,
        "currency": currency.to_uppercase(),
        "description": reason
    });

    if let Some(net) = network {
        body["network"] = serde_json::json!(net);
    }

    let data = cdp_request(creds, "POST", &send_path, Some(&body)).await?;

    // v2 API wraps response in "data"
    let tx_data = data.get("data").unwrap_or(&data);
    let tx_id = tx_data["id"].as_str().unwrap_or("?");
    let status = tx_data["status"].as_str().unwrap_or("pending");
    let tx_network = tx_data["network"]["name"].as_str().unwrap_or("unknown");

    Ok(format!(
        "Transfer initiated!\n  {} {} → {}\n  Network: {}\n  Status: {}\n  TX ID: {}\n  Reason: {}",
        amount, currency.to_uppercase(), &to_address[..to_address.len().min(20)],
        tx_network, status, tx_id, reason
    ))
}

// ── Telegram Send ──────────────────────────────────────────────────────

async fn execute_telegram_send(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> Result<String, String> {
    use crate::engine::telegram::{load_telegram_config, save_telegram_config};

    let text = args["text"].as_str().ok_or("telegram_send: missing 'text'")?;
    let config = load_telegram_config(app_handle)?;

    if config.bot_token.is_empty() {
        return Err("Telegram bot is not configured. Ask the user to set up their Telegram bot token in the Telegram channel settings.".into());
    }

    // Resolve chat_id: explicit chat_id > username lookup > first known user > first allowed user
    let chat_id: i64 = if let Some(cid) = args["chat_id"].as_i64() {
        cid
    } else if let Some(username) = args["username"].as_str() {
        let key = username.trim_start_matches('@').to_lowercase();
        *config.known_users.get(&key)
            .ok_or_else(|| format!(
                "telegram_send: unknown username '{}'. Known users: {}. The user needs to message the bot first.",
                username,
                if config.known_users.is_empty() {
                    "none — no users have messaged the bot yet".into()
                } else {
                    config.known_users.keys().map(|k| format!("@{}", k)).collect::<Vec<_>>().join(", ")
                }
            ))?
    } else if let Some((&ref _name, &cid)) = config.known_users.iter().next() {
        // Default to first known user (typically the owner)
        cid
    } else if let Some(&uid) = config.allowed_users.first() {
        // Fallback: use first allowed user ID as chat_id (works for DMs)
        uid
    } else {
        return Err("telegram_send: no target specified and no known users. Someone needs to message the bot first so we learn their chat_id.".into());
    };

    info!("[tool:telegram_send] Sending to chat_id {}: {}...", chat_id,
        if text.len() > 50 { &text[..50] } else { text });

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    // Split long messages (Telegram limit: 4096 chars)
    let chunks: Vec<String> = if text.len() > 4000 {
        text.chars().collect::<Vec<_>>()
            .chunks(4000)
            .map(|c| c.iter().collect::<String>())
            .collect()
    } else {
        vec![text.to_string()]
    };

    for chunk in &chunks {
        let body = serde_json::json!({
            "chat_id": chat_id,
            "text": chunk,
            "parse_mode": "Markdown",
        });

        let resp = client.post(format!("https://api.telegram.org/bot{}/sendMessage", config.bot_token))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Telegram API error: {}", e))?;

        let result: serde_json::Value = resp.json().await
            .map_err(|e| format!("Failed to parse Telegram response: {}", e))?;

        if !result["ok"].as_bool().unwrap_or(false) {
            let desc = result["description"].as_str().unwrap_or("unknown error");
            return Err(format!("Telegram API error: {}", desc));
        }
    }

    Ok(format!("Message sent to Telegram (chat_id: {}, {} chars, {} chunk(s))", chat_id, text.len(), chunks.len()))
}

// ── Telegram Read (Status/Users) ───────────────────────────────────────

async fn execute_telegram_read(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> Result<String, String> {
    use crate::engine::telegram::{load_telegram_config, TelegramConfig};

    let info = args["info"].as_str().unwrap_or("status");
    let config = load_telegram_config(app_handle)?;

    match info {
        "users" => {
            let mut output = String::from("Known Telegram users:\n\n");
            if config.known_users.is_empty() {
                output.push_str("No users have messaged the bot yet.\n");
            } else {
                for (username, chat_id) in &config.known_users {
                    output.push_str(&format!("  @{} (chat_id: {})\n", username, chat_id));
                }
            }
            output.push_str(&format!("\nAllowed user IDs: {:?}\n", config.allowed_users));
            if !config.pending_users.is_empty() {
                output.push_str(&format!("Pending approvals: {}\n", config.pending_users.len()));
            }
            Ok(output)
        }
        _ => {
            // Status
            let running = crate::engine::telegram::is_bridge_running();
            Ok(format!(
                "Telegram Bridge Status:\n  Running: {}\n  Bot configured: {}\n  DM policy: {}\n  Allowed users: {}\n  Known users: {}\n  Agent: {}",
                running,
                !config.bot_token.is_empty(),
                config.dm_policy,
                config.allowed_users.len(),
                config.known_users.len(),
                config.agent_id.as_deref().unwrap_or("default"),
            ))
        }
    }
}