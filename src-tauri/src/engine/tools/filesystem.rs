// Paw Agent Engine — Filesystem tools
// read_file, write_file, list_directory, append_file, delete_file

use crate::atoms::types::*;
use log::info;

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
        "read_file"      => Some(execute_read_file(args, agent_id).await),
        "write_file"     => Some(execute_write_file(args, agent_id).await),
        "list_directory" => Some(execute_list_directory(args, agent_id).await),
        "append_file"    => Some(execute_append_file(args, agent_id).await),
        "delete_file"    => Some(execute_delete_file(args, agent_id).await),
        _ => None,
    }
}

async fn execute_read_file(args: &serde_json::Value, agent_id: &str) -> Result<String, String> {
    let raw_path = args["path"].as_str().ok_or("read_file: missing 'path' argument")?;

    let resolved = if std::path::Path::new(raw_path).is_absolute() {
        std::path::PathBuf::from(raw_path)
    } else {
        let ws = super::ensure_workspace(agent_id)?;
        ws.join(raw_path)
    };
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
        ));
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

async fn execute_write_file(args: &serde_json::Value, agent_id: &str) -> Result<String, String> {
    let raw_path = args["path"].as_str().ok_or("write_file: missing 'path' argument")?;
    let content = args["content"].as_str().ok_or("write_file: missing 'content' argument")?;

    let resolved = if std::path::Path::new(raw_path).is_absolute() {
        std::path::PathBuf::from(raw_path)
    } else {
        let ws = super::ensure_workspace(agent_id)?;
        ws.join(raw_path)
    };
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
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    std::fs::write(&resolved, content)
        .map_err(|e| format!("Failed to write file '{}': {}", path, e))?;

    Ok(format!("Successfully wrote {} bytes to {}", content.len(), path))
}

async fn execute_list_directory(args: &serde_json::Value, agent_id: &str) -> Result<String, String> {
    let raw_path = args["path"].as_str().unwrap_or(".");
    let recursive = args["recursive"].as_bool().unwrap_or(false);
    let max_depth = args["max_depth"].as_u64().unwrap_or(3) as usize;

    let resolved = if std::path::Path::new(raw_path).is_absolute() {
        std::path::PathBuf::from(raw_path)
    } else {
        let ws = super::ensure_workspace(agent_id)?;
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

async fn execute_append_file(args: &serde_json::Value, agent_id: &str) -> Result<String, String> {
    let raw_path = args["path"].as_str().ok_or("append_file: missing 'path' argument")?;
    let content = args["content"].as_str().ok_or("append_file: missing 'content' argument")?;

    let resolved = if std::path::Path::new(raw_path).is_absolute() {
        std::path::PathBuf::from(raw_path)
    } else {
        let ws = super::ensure_workspace(agent_id)?;
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

async fn execute_delete_file(args: &serde_json::Value, agent_id: &str) -> Result<String, String> {
    let raw_path = args["path"].as_str().ok_or("delete_file: missing 'path' argument")?;
    let recursive = args["recursive"].as_bool().unwrap_or(false);

    let resolved = if std::path::Path::new(raw_path).is_absolute() {
        std::path::PathBuf::from(raw_path)
    } else {
        let ws = super::ensure_workspace(agent_id)?;
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
