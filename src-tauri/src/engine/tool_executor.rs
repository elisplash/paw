// Paw Agent Engine — Tool Executor
// Executes tool calls requested by the AI model.
// Every tool call goes through here — this is the security enforcement point.

use crate::engine::types::*;
use crate::engine::commands::EngineState;
use crate::engine::memory;
use crate::engine::skills;
use crate::engine::web;
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
        "list_directory" => execute_list_directory(&args).await,
        "append_file" => execute_append_file(&args).await,
        "delete_file" => execute_delete_file(&args).await,
        "soul_read" => execute_soul_read(&args, app_handle).await,
        "soul_write" => execute_soul_write(&args, app_handle).await,
        "soul_list" => execute_soul_list(app_handle).await,
        "memory_store" => execute_memory_store(&args, app_handle).await,
        "memory_search" => execute_memory_search(&args, app_handle).await,
        "self_info" => execute_self_info(app_handle).await,
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
        "github_api" => execute_skill_tool("github", "github_api", &args, app_handle).await,
        "rest_api_call" => execute_skill_tool("rest_api", "rest_api_call", &args, app_handle).await,
        "webhook_send" => execute_skill_tool("webhook", "webhook_send", &args, app_handle).await,
        "image_generate" => execute_skill_tool("image_gen", "image_generate", &args, app_handle).await,
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

// ── list_directory: List contents of a directory ───────────────────────

async fn execute_list_directory(args: &serde_json::Value) -> Result<String, String> {
    let path = args["path"].as_str().unwrap_or(".");
    let recursive = args["recursive"].as_bool().unwrap_or(false);
    let max_depth = args["max_depth"].as_u64().unwrap_or(3) as usize;

    info!("[engine] list_directory: {} recursive={}", path, recursive);

    let dir = std::path::Path::new(path);
    if !dir.exists() {
        return Err(format!("Directory '{}' does not exist", path));
    }
    if !dir.is_dir() {
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
        walk_dir(dir, "", 0, max_depth, &mut entries)
            .map_err(|e| format!("Failed to list directory '{}': {}", path, e))?;
    } else {
        let mut items: Vec<_> = std::fs::read_dir(dir)
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

async fn execute_append_file(args: &serde_json::Value) -> Result<String, String> {
    let path = args["path"].as_str()
        .ok_or("append_file: missing 'path' argument")?;
    let content = args["content"].as_str()
        .ok_or("append_file: missing 'content' argument")?;

    info!("[engine] append_file: {} ({} bytes)", path, content.len());

    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("Failed to open file '{}' for append: {}", path, e))?;

    file.write_all(content.as_bytes())
        .map_err(|e| format!("Failed to append to file '{}': {}", path, e))?;

    Ok(format!("Appended {} bytes to {}", content.len(), path))
}

// ── delete_file: Delete a file or directory ─────────────────────────────

async fn execute_delete_file(args: &serde_json::Value) -> Result<String, String> {
    let path = args["path"].as_str()
        .ok_or("delete_file: missing 'path' argument")?;
    let recursive = args["recursive"].as_bool().unwrap_or(false);

    info!("[engine] delete_file: {} recursive={}", path, recursive);

    let p = std::path::Path::new(path);
    if !p.exists() {
        return Err(format!("Path '{}' does not exist", path));
    }

    if p.is_dir() {
        if recursive {
            std::fs::remove_dir_all(path)
                .map_err(|e| format!("Failed to remove directory '{}': {}", path, e))?;
            Ok(format!("Deleted directory '{}' (recursive)", path))
        } else {
            std::fs::remove_dir(path)
                .map_err(|e| format!("Failed to remove directory '{}' (not empty? use recursive=true): {}", path, e))?;
            Ok(format!("Deleted empty directory '{}'", path))
        }
    } else {
        std::fs::remove_file(path)
            .map_err(|e| format!("Failed to delete file '{}': {}", path, e))?;
        Ok(format!("Deleted file '{}'", path))
    }
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

// ── self_info: Introspect engine configuration ─────────────────────────

async fn execute_self_info(app_handle: &tauri::AppHandle) -> Result<String, String> {
    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
    let mcfg = state.memory_config.lock().map_err(|e| format!("Lock error: {}", e))?;

    // Provider info
    let providers_info: Vec<String> = cfg.providers.iter().map(|p| {
        let is_default = cfg.default_provider.as_ref() == Some(&p.id);
        format!("  - {} ({:?}) [id: {}]{}", p.name, p.kind, p.id, if is_default { " ← DEFAULT" } else { "" })
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
        mcfg.embedding_provider,
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
