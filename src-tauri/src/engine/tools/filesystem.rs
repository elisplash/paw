// Paw Agent Engine — Filesystem tools
// read_file, write_file, list_directory, append_file, delete_file

use crate::atoms::types::*;
use crate::atoms::error::EngineResult;
use log::{info, warn};

/// Sensitive paths that agents must never read or write.
/// Checked against the canonicalized path (lowercased on case-insensitive OS).
const SENSITIVE_PATHS: &[&str] = &[
    // Credentials & secrets
    ".ssh",
    ".gnupg",
    ".gnome-keyring",
    ".password-store",
    ".aws/credentials",
    ".config/gcloud",
    ".azure",
    ".npmrc",
    ".pypirc",
    ".docker/config.json",
    ".kube/config",
    // System
    "/etc/shadow",
    "/etc/passwd",
    "/etc/sudoers",
    // Paw engine internals
    ".paw/db",
    ".paw/keys",
    "src-tauri/src/engine",
];

/// Resolve a raw path (absolute or relative to agent workspace) into a
/// canonicalized `PathBuf`.  Returns `Err` if the path escapes the agent
/// workspace via `..` traversal or targets a sensitive location.
///
/// `operation` is used in error messages (e.g. "read_file", "write_file").
fn resolve_and_validate(
    raw_path: &str,
    agent_id: &str,
    operation: &str,
) -> EngineResult<std::path::PathBuf> {
    let resolved = if std::path::Path::new(raw_path).is_absolute() {
        std::path::PathBuf::from(raw_path)
    } else {
        let ws = super::ensure_workspace(agent_id)?;
        ws.join(raw_path)
    };

    // Canonicalize: resolve symlinks and `..` segments.
    // For operations on not-yet-existing files (write, append), canonicalize the parent.
    let canonical = if resolved.exists() {
        resolved.canonicalize()
            .map_err(|e| format!("{}: failed to resolve path '{}': {}", operation, raw_path, e))?
    } else if let Some(parent) = resolved.parent() {
        if parent.exists() {
            let canon_parent = parent.canonicalize()
                .map_err(|e| format!("{}: failed to resolve parent of '{}': {}", operation, raw_path, e))?;
            canon_parent.join(resolved.file_name().unwrap_or_default())
        } else {
            // Parent doesn't exist yet — use the raw resolved path but check for `..`
            resolved.clone()
        }
    } else {
        resolved.clone()
    };

    // Block `..` in the raw path to prevent traversal even when canonicalize isn't decisive
    if raw_path.contains("..") {
        let ws = super::agent_workspace(agent_id);
        if let Ok(canon_ws) = ws.canonicalize() {
            if !canonical.starts_with(&canon_ws) {
                warn!("[engine] {} blocked path traversal: {} (agent={})", operation, raw_path, agent_id);
                return Err(format!(
                    "{}: path '{}' escapes the agent workspace. Use paths within your workspace or absolute paths to allowed locations.",
                    operation, raw_path
                ).into());
            }
        }
    }

    // Check against sensitive paths
    let path_str = canonical.to_string_lossy();
    for sensitive in SENSITIVE_PATHS {
        if sensitive.starts_with('/') {
            // Absolute sensitive path
            if path_str.starts_with(sensitive) {
                warn!("[engine] {} blocked access to sensitive path: {} (agent={})", operation, raw_path, agent_id);
                return Err(format!(
                    "{}: access to '{}' is blocked by security policy. This path contains sensitive system or credential data.",
                    operation, raw_path
                ).into());
            }
        } else {
            // Relative sensitive path — check if it appears as a path component
            let needle = format!("/{}/", sensitive);
            let needle_end = format!("/{}", sensitive);
            if path_str.contains(&needle) || path_str.ends_with(&needle_end) {
                warn!("[engine] {} blocked access to sensitive path: {} (matched '{}', agent={})", operation, raw_path, sensitive, agent_id);
                return Err(format!(
                    "{}: access to '{}' is blocked by security policy. This path contains sensitive credential data.",
                    operation, raw_path
                ).into());
            }
        }
    }

    Ok(canonical)
}

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "read_file".into(),
                description: "Read the contents of a file on the user's machine.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Absolute or relative file path to read" }
                    },
                    "required": ["path"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "write_file".into(),
                description: "Write content to a file on the user's machine. Creates the file if it doesn't exist, overwrites if it does.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Absolute or relative file path to write" },
                        "content": { "type": "string", "description": "The content to write to the file" }
                    },
                    "required": ["path", "content"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "list_directory".into(),
                description: "List files and subdirectories in a directory. Returns names, sizes, and types. Optionally recurse into subdirectories.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Directory path to list (default: current directory)" },
                        "recursive": { "type": "boolean", "description": "If true, list contents recursively (default: false)" },
                        "max_depth": { "type": "integer", "description": "Maximum recursion depth (default: 3)" }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "append_file".into(),
                description: "Append content to the end of a file. Creates the file if it doesn't exist. Unlike write_file, this preserves existing content.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path to append to" },
                        "content": { "type": "string", "description": "Content to append to the file" }
                    },
                    "required": ["path", "content"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "delete_file".into(),
                description: "Delete a file or directory from the filesystem. For directories, set recursive=true to delete non-empty directories.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path to the file or directory to delete" },
                        "recursive": { "type": "boolean", "description": "If true and path is a directory, delete it and all contents (default: false)" }
                    },
                    "required": ["path"]
                }),
            },
        },
    ]
}

pub async fn execute(
    name: &str,
    args: &serde_json::Value,
    agent_id: &str,
) -> Option<Result<String, String>> {
    match name {
        "read_file"      => Some(execute_read_file(args, agent_id).await.map_err(|e| e.to_string())),
        "write_file"     => Some(execute_write_file(args, agent_id).await.map_err(|e| e.to_string())),
        "list_directory" => Some(execute_list_directory(args, agent_id).await.map_err(|e| e.to_string())),
        "append_file"    => Some(execute_append_file(args, agent_id).await.map_err(|e| e.to_string())),
        "delete_file"    => Some(execute_delete_file(args, agent_id).await.map_err(|e| e.to_string())),
        _ => None,
    }
}

async fn execute_read_file(args: &serde_json::Value, agent_id: &str) -> EngineResult<String> {
    let raw_path = args["path"].as_str().ok_or("read_file: missing 'path' argument")?;
    let resolved = resolve_and_validate(raw_path, agent_id, "read_file")?;
    let path = resolved.to_string_lossy();

    info!("[engine] read_file: {} (agent={})", path, agent_id);

    let normalized = path.replace('\\', "/").to_lowercase();
    if normalized.contains("src-tauri/src/engine/")
        || normalized.contains("src/engine/")
        || normalized.ends_with(".rs")
    {
        return Err(format!(
            "Cannot read engine source file '{}'. \
             Use your available tools directly — credentials and authentication are handled automatically.",
            path
        ).into());
    }

    let content = std::fs::read_to_string(&resolved)
        .map_err(|e| format!("Failed to read file '{}': {}", path, e))?;

    const MAX_FILE: usize = 32_000;
    if content.len() > MAX_FILE {
        Ok(format!("{}...\n[truncated, {} total bytes]", &content[..MAX_FILE], content.len()))
    } else {
        Ok(content)
    }
}

async fn execute_write_file(args: &serde_json::Value, agent_id: &str) -> EngineResult<String> {
    let raw_path = args["path"].as_str().ok_or("write_file: missing 'path' argument")?;
    let content = args["content"].as_str().ok_or("write_file: missing 'content' argument")?;
    let resolved = resolve_and_validate(raw_path, agent_id, "write_file")?;
    let path = resolved.to_string_lossy();

    info!("[engine] write_file: {} ({} bytes, agent={})", path, content.len(), agent_id);

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

    if let Some(parent) = resolved.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&resolved, content)
        .map_err(|e| format!("Failed to write file '{}': {}", path, e))?;

    Ok(format!("Successfully wrote {} bytes to {}", content.len(), path))
}

async fn execute_list_directory(args: &serde_json::Value, agent_id: &str) -> EngineResult<String> {
    let raw_path = args["path"].as_str().unwrap_or(".");
    let recursive = args["recursive"].as_bool().unwrap_or(false);
    let max_depth = args["max_depth"].as_u64().unwrap_or(3) as usize;

    let resolved = resolve_and_validate(raw_path, agent_id, "list_directory")?;
    let path = resolved.to_string_lossy().to_string();

    info!("[engine] list_directory: {} recursive={} (agent={})", path, recursive, agent_id);

    if !resolved.exists() {
        return Err(format!("Directory '{}' does not exist", path).into());
    }
    if !resolved.is_dir() {
        return Err(format!("'{}' is not a directory", path).into());
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

async fn execute_append_file(args: &serde_json::Value, agent_id: &str) -> EngineResult<String> {
    let raw_path = args["path"].as_str().ok_or("append_file: missing 'path' argument")?;
    let content = args["content"].as_str().ok_or("append_file: missing 'content' argument")?;
    let resolved = resolve_and_validate(raw_path, agent_id, "append_file")?;
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

async fn execute_delete_file(args: &serde_json::Value, agent_id: &str) -> EngineResult<String> {
    let raw_path = args["path"].as_str().ok_or("delete_file: missing 'path' argument")?;
    let recursive = args["recursive"].as_bool().unwrap_or(false);
    let resolved = resolve_and_validate(raw_path, agent_id, "delete_file")?;
    let path = resolved.to_string_lossy();

    info!("[engine] delete_file: {} recursive={} (agent={})", path, recursive, agent_id);

    if !resolved.exists() {
        return Err(format!("Path '{}' does not exist", path).into());
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
