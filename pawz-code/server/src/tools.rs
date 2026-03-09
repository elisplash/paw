// pawz-code — tools.rs
// Code-focused tool implementations for the standalone developer agent.
//
// Tools: exec, read_file, write_file, list_directory, grep, fetch,
//        remember, recall, apply_patch, git_status, git_diff,
//        workspace_map, file_summary, search_symbols,
//        engram_store, engram_recall
//
// Security: sensitive path checks and exec command filters mirror the main
// Pawz engine to keep the same safety guarantees.

use crate::state::AppState;
use crate::types::ToolDef;
use anyhow::{bail, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

// ── Tool Definitions (sent to LLM) ───────────────────────────────────────────

pub fn all_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "exec",
            description: "Execute a shell command. Returns stdout + stderr. Use for git, \
                cargo/pnpm/npm, file ops, grep, find, gh CLI, docker, kubectl, and any other \
                local command.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Shell command to execute (run in sh -c)"
                    },
                    "cwd": {
                        "type": "string",
                        "description": "Working directory (absolute path, optional)"
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "description": "Timeout in seconds (default 60, max 300)"
                    }
                },
                "required": ["command"]
            }),
        },
        ToolDef {
            name: "read_file",
            description: "Read the contents of a file. Optionally read a specific line range.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path to the file"
                    },
                    "start_line": {
                        "type": "integer",
                        "description": "First line to read, 1-based (optional)"
                    },
                    "end_line": {
                        "type": "integer",
                        "description": "Last line to read, 1-based inclusive (optional)"
                    }
                },
                "required": ["path"]
            }),
        },
        ToolDef {
            name: "write_file",
            description: "Write or overwrite a file with new content. \
                Creates parent directories if needed.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path to the file"
                    },
                    "content": {
                        "type": "string",
                        "description": "Full file content to write"
                    }
                },
                "required": ["path", "content"]
            }),
        },
        ToolDef {
            name: "list_directory",
            description: "List the contents of a directory.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path to directory"
                    },
                    "recursive": {
                        "type": "boolean",
                        "description": "Recurse into subdirectories (default false)"
                    }
                },
                "required": ["path"]
            }),
        },
        ToolDef {
            name: "grep",
            description: "Search for a regex pattern in files. Returns matching lines with context.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern to search for"
                    },
                    "path": {
                        "type": "string",
                        "description": "File or directory to search (absolute path)"
                    },
                    "recursive": {
                        "type": "boolean",
                        "description": "Search recursively (default true)"
                    },
                    "context_lines": {
                        "type": "integer",
                        "description": "Lines of context around each match (default 2)"
                    }
                },
                "required": ["pattern", "path"]
            }),
        },
        ToolDef {
            name: "fetch",
            description: "Make an HTTP request and return the response body. \
                Use for documentation, APIs, or checking URLs.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "URL to fetch"
                    },
                    "method": {
                        "type": "string",
                        "enum": ["GET", "POST"],
                        "description": "HTTP method (default GET)"
                    },
                    "body": {
                        "type": "string",
                        "description": "Request body for POST"
                    },
                    "headers": {
                        "type": "object",
                        "description": "Additional request headers"
                    }
                },
                "required": ["url"]
            }),
        },
        ToolDef {
            name: "remember",
            description: "Persist a note in long-term memory across sessions. \
                Use this to remember decisions, architecture choices, conventions, \
                recurring context, or anything worth keeping.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Short label for this memory (used for recall)"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to remember"
                    },
                    "tags": {
                        "type": "string",
                        "description": "Comma-separated tags (optional)"
                    }
                },
                "required": ["key", "content"]
            }),
        },
        ToolDef {
            name: "recall",
            description: "Search long-term memory for stored notes.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search terms to look up in memory"
                    }
                },
                "required": ["query"]
            }),
        },
        // ── New coding-specific tools ──────────────────────────────────────
        ToolDef {
            name: "apply_patch",
            description: "Apply a unified diff patch to a file. \
                Use this for targeted edits instead of rewriting the whole file. \
                The patch must be in standard unified diff format (--- a/file +++ b/file).",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path to the file to patch"
                    },
                    "patch": {
                        "type": "string",
                        "description": "Unified diff patch content"
                    }
                },
                "required": ["path", "patch"]
            }),
        },
        ToolDef {
            name: "git_status",
            description: "Get the current git status of a repository. \
                Shows staged, unstaged, and untracked files.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path to the git repository root (optional, uses workspace_root if omitted)"
                    }
                }
            }),
        },
        ToolDef {
            name: "git_diff",
            description: "Get the git diff for a repository. Shows what has changed. \
                Can show staged diff, unstaged diff, or diff against a specific ref.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path to the git repository root (optional)"
                    },
                    "staged": {
                        "type": "boolean",
                        "description": "Show staged changes only (default false = show unstaged)"
                    },
                    "ref": {
                        "type": "string",
                        "description": "Compare against this ref (e.g. 'HEAD~1', 'main')"
                    },
                    "file": {
                        "type": "string",
                        "description": "Limit diff to this specific file (optional)"
                    }
                }
            }),
        },
        ToolDef {
            name: "workspace_map",
            description: "Generate a compact map of the workspace file structure. \
                Much cheaper than list_directory for understanding repo layout. \
                Use this instead of recursive list_directory for large repos.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path to root (uses workspace_root if omitted)"
                    },
                    "depth": {
                        "type": "integer",
                        "description": "Max depth to traverse (default 3)"
                    }
                }
            }),
        },
        ToolDef {
            name: "file_summary",
            description: "Generate a structural summary of a source file showing \
                function/struct/class definitions without reading the full content. \
                Use when you need to understand file structure, not full content.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path to the source file"
                    }
                },
                "required": ["path"]
            }),
        },
        ToolDef {
            name: "search_symbols",
            description: "Search for function/struct/class/const definitions by name across the workspace. \
                Returns file paths and line numbers of matches.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "symbol": {
                        "type": "string",
                        "description": "Symbol name to search for (partial match supported)"
                    },
                    "path": {
                        "type": "string",
                        "description": "Directory to search in (uses workspace_root if omitted)"
                    },
                    "kind": {
                        "type": "string",
                        "enum": ["function", "struct", "class", "const", "type", "any"],
                        "description": "Symbol kind to search for (default: any)"
                    }
                },
                "required": ["symbol"]
            }),
        },
        ToolDef {
            name: "engram_store",
            description: "Store a compressed understanding of the codebase in Engram. \
                Use this for architecture facts, module relationships, key entrypoints, \
                and patterns that are stable across sessions. \
                This is for structural understanding, not factual notes (use remember for facts).",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "Identifier for this engram entry (e.g. 'auth_flow', 'db_schema')"
                    },
                    "content": {
                        "type": "string",
                        "description": "Compressed description of this architectural understanding"
                    },
                    "kind": {
                        "type": "string",
                        "enum": ["architecture", "module", "pattern", "summary", "entrypoint"],
                        "description": "Category of this engram (default: summary)"
                    },
                    "scope": {
                        "type": "string",
                        "description": "Workspace scope (defaults to configured workspace_root)"
                    }
                },
                "required": ["key", "content"]
            }),
        },
        ToolDef {
            name: "engram_recall",
            description: "Search the Engram for stored codebase understanding. \
                Use at the start of complex tasks to load relevant architectural context.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search terms to find relevant engram entries"
                    },
                    "scope": {
                        "type": "string",
                        "description": "Limit search to this workspace scope (optional)"
                    }
                },
                "required": ["query"]
            }),
        },
    ]
}

/// Dispatch a tool call by name. Returns Some(result) if the tool was handled,
/// None if the tool name is unknown.
pub async fn execute(
    name: &str,
    args: &Value,
    state: &AppState,
) -> Option<Result<String>> {
    match name {
        "exec" => Some(tool_exec(args).await),
        "read_file" => Some(tool_read_file(args)),
        "write_file" => Some(tool_write_file(args)),
        "list_directory" => Some(tool_list_directory(args)),
        "grep" => Some(tool_grep(args).await),
        "fetch" => Some(tool_fetch(args).await),
        "remember" => Some(tool_remember(args, state)),
        "recall" => Some(tool_recall(args, state)),
        "apply_patch" => Some(tool_apply_patch(args).await),
        "git_status" => Some(tool_git_status(args, state).await),
        "git_diff" => Some(tool_git_diff(args, state).await),
        "workspace_map" => Some(tool_workspace_map(args, state)),
        "file_summary" => Some(tool_file_summary(args)),
        "search_symbols" => Some(tool_search_symbols(args, state).await),
        "engram_store" => Some(tool_engram_store(args, state)),
        "engram_recall" => Some(tool_engram_recall(args, state)),
        _ => None,
    }
}

// ── Sensitive path guard ─────────────────────────────────────────────────────

const SENSITIVE_PATHS: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".aws/credentials",
    ".aws/config",
    ".config/gcloud",
    ".azure",
    ".npmrc",
    ".pypirc",
    ".docker/config.json",
    ".kube/config",
    ".config/1password",
    ".local/share/keyrings",
    "Library/Keychains",
    ".bashrc",
    ".bash_history",
    ".zsh_history",
    ".profile",
    "/etc/shadow",
    "/etc/passwd",
    "/etc/sudoers",
    ".paw/db",
    ".paw/keys",
];

fn check_path(path: &Path, op: &str) -> Result<()> {
    let path_str = path.to_string_lossy().to_lowercase();
    for sensitive in SENSITIVE_PATHS {
        if path_str.contains(sensitive) {
            bail!("{}: access to '{}' is blocked by security policy", op, path.display());
        }
    }
    // Block path traversal
    if path_str.contains("..") {
        bail!("{}: path traversal via '..' is not allowed", op);
    }
    Ok(())
}

// ── exec ─────────────────────────────────────────────────────────────────────

async fn tool_exec(args: &Value) -> Result<String> {
    let command = args["command"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("exec: missing 'command'"))?;

    // §Security: block credential exfiltration patterns
    let cmd_lower = command.to_lowercase();
    let blocked_patterns = [
        "cat .ssh", "cat ~/.ssh", "base64 .ssh", "tar .ssh",
        "curl.*id_rsa", "wget.*id_rsa",
        "nc -e", "nc -l.*-e", "bash -i",
        "python.*pty", "perl.*socket",
        "/etc/shadow", "/etc/passwd",
    ];
    for pat in &blocked_patterns {
        if cmd_lower.contains(pat) {
            bail!("exec: command blocked by security policy");
        }
    }

    let cwd = args["cwd"].as_str();
    let timeout_secs = args["timeout_secs"].as_u64().unwrap_or(60).min(300);

    log::info!("[exec] cmd={} cwd={:?}", &command[..command.len().min(200)], cwd);

    let mut cmd = tokio::process::Command::new("sh");
    cmd.arg("-c").arg(command);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let start = Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        cmd.output(),
    )
    .await;

    match result {
        Err(_) => bail!("exec: timed out after {}s", timeout_secs),
        Ok(Err(e)) => bail!("exec: failed to spawn: {}", e),
        Ok(Ok(output)) => {
            let elapsed = start.elapsed().as_millis();
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let exit_code = output.status.code().unwrap_or(-1);

            let mut out = String::new();
            if !stdout.is_empty() {
                out.push_str(&stdout);
            }
            if !stderr.is_empty() {
                if !out.is_empty() {
                    out.push_str("\n--- stderr ---\n");
                }
                out.push_str(&stderr);
            }
            if out.is_empty() {
                out = format!("(exit {})", exit_code);
            }
            // Truncate very long outputs
            const MAX_OUTPUT: usize = 50_000;
            if out.len() > MAX_OUTPUT {
                out.truncate(MAX_OUTPUT);
                out.push_str("\n... (truncated)");
            }
            log::info!("[exec] done in {}ms, exit={}", elapsed, exit_code);
            Ok(out)
        }
    }
}

// ── read_file ────────────────────────────────────────────────────────────────

fn tool_read_file(args: &Value) -> Result<String> {
    let path_str = args["path"].as_str().ok_or_else(|| anyhow::anyhow!("read_file: missing 'path'"))?;
    let path = PathBuf::from(path_str);
    check_path(&path, "read_file")?;

    if !path.exists() {
        bail!("read_file: file not found: {}", path.display());
    }
    if path.is_dir() {
        bail!("read_file: '{}' is a directory — use list_directory instead", path.display());
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("read_file: {}", e))?;

    let start_line = args["start_line"].as_u64().unwrap_or(0) as usize;
    let end_line = args["end_line"].as_u64().unwrap_or(0) as usize;

    if start_line > 0 || end_line > 0 {
        let lines: Vec<&str> = content.lines().collect();
        let start = start_line.saturating_sub(1);
        let end = if end_line > 0 { end_line.min(lines.len()) } else { lines.len() };
        Ok(lines[start..end].join("\n"))
    } else {
        // Truncate very large files
        const MAX_SIZE: usize = 100_000;
        if content.len() > MAX_SIZE {
            Ok(format!("{}\n... (truncated — {} total bytes)", &content[..MAX_SIZE], content.len()))
        } else {
            Ok(content)
        }
    }
}

// ── write_file ───────────────────────────────────────────────────────────────

fn tool_write_file(args: &Value) -> Result<String> {
    let path_str = args["path"].as_str().ok_or_else(|| anyhow::anyhow!("write_file: missing 'path'"))?;
    let content = args["content"].as_str().ok_or_else(|| anyhow::anyhow!("write_file: missing 'content'"))?;

    let path = PathBuf::from(path_str);
    check_path(&path, "write_file")?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("write_file: could not create dirs: {}", e))?;
    }

    std::fs::write(&path, content)
        .map_err(|e| anyhow::anyhow!("write_file: {}", e))?;

    log::info!("[write_file] wrote {} bytes to {}", content.len(), path.display());
    Ok(format!("Written {} bytes to {}", content.len(), path.display()))
}

// ── list_directory ───────────────────────────────────────────────────────────

fn tool_list_directory(args: &Value) -> Result<String> {
    let path_str = args["path"].as_str().ok_or_else(|| anyhow::anyhow!("list_directory: missing 'path'"))?;
    let recursive = args["recursive"].as_bool().unwrap_or(false);

    let path = PathBuf::from(path_str);
    check_path(&path, "list_directory")?;

    if !path.exists() {
        bail!("list_directory: path not found: {}", path.display());
    }
    if !path.is_dir() {
        bail!("list_directory: '{}' is not a directory", path.display());
    }

    let mut entries = Vec::new();
    collect_entries(&path, &path, recursive, &mut entries, 0)?;
    entries.sort();
    Ok(entries.join("\n"))
}

fn collect_entries(
    base: &Path,
    dir: &Path,
    recursive: bool,
    out: &mut Vec<String>,
    depth: usize,
) -> Result<()> {
    if depth > 5 {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        let rel = entry
            .path()
            .strip_prefix(base)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| entry.file_name().to_string_lossy().to_string());
        if meta.is_dir() {
            out.push(format!("{}/", rel));
            if recursive {
                collect_entries(base, &entry.path(), recursive, out, depth + 1)?;
            }
        } else {
            out.push(rel);
        }
    }
    Ok(())
}

// ── grep ─────────────────────────────────────────────────────────────────────

async fn tool_grep(args: &Value) -> Result<String> {
    let pattern = args["pattern"].as_str().ok_or_else(|| anyhow::anyhow!("grep: missing 'pattern'"))?;
    let path_str = args["path"].as_str().ok_or_else(|| anyhow::anyhow!("grep: missing 'path'"))?;
    let recursive = args["recursive"].as_bool().unwrap_or(true);
    let context = args["context_lines"].as_u64().unwrap_or(2);

    let path = PathBuf::from(path_str);
    check_path(&path, "grep")?;

    // Use exec under the hood via sh for portability
    let flag = if context > 0 { format!("-C{}", context) } else { String::new() };
    let recurse_flag = if recursive && path.is_dir() { "-r" } else { "" };
    let command = format!("grep -nE {} {} -- {} {}",
        flag, recurse_flag,
        shell_escape(pattern), shell_escape(path_str));

    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("grep: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if stdout.is_empty() {
        Ok("No matches found.".into())
    } else {
        const MAX: usize = 20_000;
        if stdout.len() > MAX {
            Ok(format!("{}\n... (truncated)", &stdout[..MAX]))
        } else {
            Ok(stdout)
        }
    }
}

fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

// ── fetch ────────────────────────────────────────────────────────────────────

async fn tool_fetch(args: &Value) -> Result<String> {
    let url = args["url"].as_str().ok_or_else(|| anyhow::anyhow!("fetch: missing 'url'"))?;

    // §Security: block SSRF to internal cloud metadata endpoints
    let url_lower = url.to_lowercase();
    let blocked_prefixes = [
        "http://169.254.", "http://metadata.google",
        "http://100.100.", "http://192.168.0.1",
    ];
    for prefix in &blocked_prefixes {
        if url_lower.starts_with(prefix) {
            bail!("fetch: URL blocked by security policy (SSRF prevention)");
        }
    }

    let method = args["method"].as_str().unwrap_or("GET");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let mut req = match method {
        "POST" => client.post(url),
        _ => client.get(url),
    };

    if let Some(hdrs) = args["headers"].as_object() {
        for (k, v) in hdrs {
            if let Some(v) = v.as_str() {
                req = req.header(k.as_str(), v);
            }
        }
    }
    if let Some(body) = args["body"].as_str() {
        req = req.body(body.to_string());
    }

    let resp = req.send().await?;
    let status = resp.status();
    let body = resp.text().await?;

    const MAX: usize = 30_000;
    let truncated = if body.len() > MAX {
        format!("{}\n... (truncated)", &body[..MAX])
    } else {
        body
    };

    Ok(format!("HTTP {}\n\n{}", status, truncated))
}

// ── remember / recall ────────────────────────────────────────────────────────

fn tool_remember(args: &Value, state: &AppState) -> Result<String> {
    let key = args["key"].as_str().ok_or_else(|| anyhow::anyhow!("remember: missing 'key'"))?;
    let content = args["content"].as_str().ok_or_else(|| anyhow::anyhow!("remember: missing 'content'"))?;
    let tags = args["tags"].as_str();
    crate::memory::remember(state, key, content, tags)?;
    Ok(format!("Remembered: {}", key))
}

fn tool_recall(args: &Value, state: &AppState) -> Result<String> {
    let query = args["query"].as_str().ok_or_else(|| anyhow::anyhow!("recall: missing 'query'"))?;
    let results = crate::memory::recall(state, query)?;
    if results.is_empty() {
        return Ok("No memories found.".into());
    }
    let formatted: Vec<String> = results
        .into_iter()
        .map(|(k, v)| format!("**{}**: {}", k, v))
        .collect();
    Ok(formatted.join("\n\n"))
}

// ── apply_patch ──────────────────────────────────────────────────────────────

async fn tool_apply_patch(args: &Value) -> Result<String> {
    let path_str = args["path"].as_str().ok_or_else(|| anyhow::anyhow!("apply_patch: missing 'path'"))?;
    let patch = args["patch"].as_str().ok_or_else(|| anyhow::anyhow!("apply_patch: missing 'patch'"))?;

    let path = PathBuf::from(path_str);
    check_path(&path, "apply_patch")?;

    if !path.exists() {
        bail!("apply_patch: file not found: {}", path.display());
    }

    // Write patch to a temp file and apply with `patch` command
    let tmp_patch = std::env::temp_dir().join(format!("pawz-patch-{}.diff", uuid_short()));
    std::fs::write(&tmp_patch, patch)
        .map_err(|e| anyhow::anyhow!("apply_patch: failed to write temp patch: {}", e))?;

    let output = tokio::process::Command::new("patch")
        .arg("--no-backup-if-mismatch")
        .arg("-p0")
        .arg(path_str)
        .arg(&tmp_patch)
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("apply_patch: failed to run patch command: {}", e))?;

    // Clean up temp file
    let _ = std::fs::remove_file(&tmp_patch);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if output.status.success() {
        log::info!("[apply_patch] patched {}", path_str);
        Ok(format!("Patch applied successfully to {}\n{}", path_str, stdout))
    } else {
        bail!("apply_patch: patch failed:\n{}\n{}", stdout, stderr)
    }
}

fn uuid_short() -> String {
    uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("tmp").to_string()
}

// ── git_status ───────────────────────────────────────────────────────────────

async fn tool_git_status(args: &Value, state: &AppState) -> Result<String> {
    let cwd = args["path"]
        .as_str()
        .or_else(|| state.config.workspace_root.as_deref())
        .ok_or_else(|| anyhow::anyhow!("git_status: no path provided and no workspace_root configured"))?;

    let output = tokio::process::Command::new("git")
        .arg("status")
        .arg("--short")
        .arg("--branch")
        .current_dir(cwd)
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("git_status: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        bail!("git_status: {}", stderr);
    }

    Ok(if stdout.is_empty() { "No changes (clean working tree)".into() } else { stdout })
}

// ── git_diff ─────────────────────────────────────────────────────────────────

async fn tool_git_diff(args: &Value, state: &AppState) -> Result<String> {
    let cwd = args["path"]
        .as_str()
        .or_else(|| state.config.workspace_root.as_deref())
        .ok_or_else(|| anyhow::anyhow!("git_diff: no path provided and no workspace_root configured"))?;

    let staged = args["staged"].as_bool().unwrap_or(false);
    let git_ref = args["ref"].as_str();
    let file = args["file"].as_str();

    let mut cmd = tokio::process::Command::new("git");
    cmd.arg("diff");

    if staged {
        cmd.arg("--staged");
    }

    if let Some(r) = git_ref {
        cmd.arg(r);
    }

    if let Some(f) = file {
        cmd.arg("--").arg(f);
    }

    cmd.current_dir(cwd);

    let output = cmd.output().await.map_err(|e| anyhow::anyhow!("git_diff: {}", e))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        bail!("git_diff: {}", stderr);
    }

    if stdout.is_empty() {
        return Ok("No differences found.".into());
    }

    const MAX: usize = 40_000;
    if stdout.len() > MAX {
        Ok(format!("{}\n... (truncated)", &stdout[..MAX]))
    } else {
        Ok(stdout)
    }
}

// ── workspace_map ─────────────────────────────────────────────────────────────

fn tool_workspace_map(args: &Value, state: &AppState) -> Result<String> {
    let root_str = args["path"]
        .as_str()
        .or_else(|| state.config.workspace_root.as_deref())
        .ok_or_else(|| anyhow::anyhow!("workspace_map: no path provided and no workspace_root configured"))?;

    let depth = args["depth"].as_u64().unwrap_or(3) as usize;
    let root = PathBuf::from(root_str);
    check_path(&root, "workspace_map")?;

    if !root.exists() {
        bail!("workspace_map: path not found: {}", root.display());
    }

    Ok(crate::reduction::workspace_map(&root, depth))
}

// ── file_summary ──────────────────────────────────────────────────────────────

fn tool_file_summary(args: &Value) -> Result<String> {
    let path_str = args["path"].as_str().ok_or_else(|| anyhow::anyhow!("file_summary: missing 'path'"))?;
    let path = PathBuf::from(path_str);
    check_path(&path, "file_summary")?;

    if !path.exists() {
        bail!("file_summary: file not found: {}", path.display());
    }

    Ok(crate::reduction::file_summary(&path))
}

// ── search_symbols ────────────────────────────────────────────────────────────

async fn tool_search_symbols(args: &Value, state: &AppState) -> Result<String> {
    let symbol = args["symbol"].as_str().ok_or_else(|| anyhow::anyhow!("search_symbols: missing 'symbol'"))?;
    let search_path = args["path"]
        .as_str()
        .or_else(|| state.config.workspace_root.as_deref())
        .ok_or_else(|| anyhow::anyhow!("search_symbols: no path and no workspace_root configured"))?;

    let kind = args["kind"].as_str().unwrap_or("any");

    // Build a grep pattern based on the kind and language
    let pattern = match kind {
        "function" => format!(
            r"(fn |function |def |async fn |async function |async def ).*{}",
            regex_escape(symbol)
        ),
        "struct" => format!(r"(struct |class ).*{}", regex_escape(symbol)),
        "const" => format!(r"(const |let |var ).*{}", regex_escape(symbol)),
        "type" => format!(r"(type |interface |enum ).*{}", regex_escape(symbol)),
        _ => format!(
            r"(fn |function |def |struct |class |const |let |impl |type |interface |enum ).*{}",
            regex_escape(symbol)
        ),
    };

    // Skip build dirs
    let command = format!(
        "grep -rn --include='*.rs' --include='*.ts' --include='*.tsx' \
         --include='*.js' --include='*.py' --include='*.go' \
         --exclude-dir=target --exclude-dir=node_modules --exclude-dir=.git \
         -E -- {} {}",
        shell_escape(&pattern),
        shell_escape(search_path)
    );

    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("search_symbols: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    if stdout.is_empty() {
        Ok(format!("No symbol '{}' found.", symbol))
    } else {
        const MAX: usize = 15_000;
        if stdout.len() > MAX {
            Ok(format!("{}\n... (truncated)", &stdout[..MAX]))
        } else {
            Ok(stdout)
        }
    }
}

fn regex_escape(s: &str) -> String {
    // Escape special regex chars
    s.chars().fold(String::new(), |mut acc, c| {
        if "[](){}.*+?^$|\\".contains(c) {
            acc.push('\\');
        }
        acc.push(c);
        acc
    })
}

// ── engram_store / engram_recall ──────────────────────────────────────────────

fn tool_engram_store(args: &Value, state: &AppState) -> Result<String> {
    let key = args["key"].as_str().ok_or_else(|| anyhow::anyhow!("engram_store: missing 'key'"))?;
    let content = args["content"].as_str().ok_or_else(|| anyhow::anyhow!("engram_store: missing 'content'"))?;
    let kind = args["kind"].as_str().unwrap_or("summary");
    let scope = args["scope"]
        .as_str()
        .or_else(|| state.config.workspace_root.as_deref())
        .unwrap_or("global");

    crate::engram::store(state, scope, key, content, kind)?;
    Ok(format!("Engram stored: [{}] {} (scope: {})", kind, key, scope))
}

fn tool_engram_recall(args: &Value, state: &AppState) -> Result<String> {
    let query = args["query"].as_str().ok_or_else(|| anyhow::anyhow!("engram_recall: missing 'query'"))?;
    let scope = args["scope"].as_str();

    let results = crate::engram::search(state, query, scope)?;
    if results.is_empty() {
        return Ok("No engram entries found.".into());
    }

    let formatted: Vec<String> = results
        .iter()
        .filter_map(|r| {
            let key = r["key"].as_str()?;
            let content = r["content"].as_str()?;
            let kind = r["kind"].as_str().unwrap_or("summary");
            Some(format!("[{}] **{}**: {}", kind, key, content))
        })
        .collect();

    Ok(formatted.join("\n\n"))
}
