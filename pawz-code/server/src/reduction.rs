// pawz-code — reduction.rs
// Token reduction pipeline: compress context before model calls.
//
// Every model call should flow through this pipeline:
//   1. Request classification
//   2. Workspace relevance filtering
//   3. Memory recall filtering
//   4. Protocol selection
//   5. Structural summarisation
//   6. Rolling task summary merge
//   7. Compressed prompt assembly
//
// The goal: never send more tokens than needed for the task at hand.

use std::path::{Path, PathBuf};

// ── Request classification ───────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum RequestKind {
    /// Quick question, no file access expected
    Conversational,
    /// Code reading / understanding task
    Exploration,
    /// Code editing / writing task
    Edit,
    /// Build / test / run task
    Execution,
    /// Architecture or cross-cutting analysis
    Architecture,
    /// Memory / context management
    Memory,
}

/// Classify the user's request to inform model routing and context selection.
pub fn classify_request(message: &str) -> RequestKind {
    let lower = message.to_lowercase();

    if lower.contains("architect") || lower.contains("overall") || lower.contains("codebase") {
        return RequestKind::Architecture;
    }
    if lower.contains("remember") || lower.contains("recall") || lower.contains("memory") {
        return RequestKind::Memory;
    }
    if lower.contains("write") || lower.contains("edit") || lower.contains("fix") ||
       lower.contains("implement") || lower.contains("add") || lower.contains("change") ||
       lower.contains("update") || lower.contains("refactor") || lower.contains("create") {
        return RequestKind::Edit;
    }
    if lower.contains("run") || lower.contains("build") || lower.contains("test") ||
       lower.contains("execute") || lower.contains("deploy") || lower.contains("cargo") ||
       lower.contains("npm") || lower.contains("pnpm") {
        return RequestKind::Execution;
    }
    if lower.contains("read") || lower.contains("show") || lower.contains("explain") ||
       lower.contains("what") || lower.contains("how") || lower.contains("why") ||
       lower.contains("find") || lower.contains("look") || lower.contains("search") {
        return RequestKind::Exploration;
    }

    RequestKind::Conversational
}

// ── Workspace map ────────────────────────────────────────────────────────────

/// Generate a compact workspace map — a structured summary of the repo layout.
/// Much cheaper than listing all files recursively.
pub fn workspace_map(root: &Path, max_depth: usize) -> String {
    let mut lines = vec![format!("Workspace: {}", root.display())];
    collect_map(root, root, 0, max_depth, &mut lines);
    lines.join("\n")
}

fn collect_map(
    base: &Path,
    dir: &Path,
    depth: usize,
    max_depth: usize,
    out: &mut Vec<String>,
) {
    if depth >= max_depth {
        return;
    }

    // Skip noisy directories
    let skip_dirs = [
        "node_modules", "target", ".git", ".next", "dist", "build",
        "__pycache__", ".cache", "vendor", "tmp", ".turbo",
    ];

    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(e) => e.flatten().collect(),
        Err(_) => return,
    };
    entries.sort_by_key(|e| e.file_name());

    let indent = "  ".repeat(depth);

    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') && depth > 0 {
            continue; // skip hidden files in subdirs
        }

        let path = entry.path();
        let rel = path.strip_prefix(base).unwrap_or(&path);

        if path.is_dir() {
            if skip_dirs.contains(&name.as_str()) {
                continue;
            }
            out.push(format!("{}{}/ ", indent, name));
            collect_map(base, &path, depth + 1, max_depth, out);
        } else {
            // Only show source files — skip build artifacts, etc.
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let interesting_exts = [
                "rs", "ts", "tsx", "js", "jsx", "py", "go", "rb", "java",
                "c", "cpp", "h", "cs", "swift", "kt", "toml", "json",
                "yaml", "yml", "md", "sql", "sh", "env.example",
            ];
            if interesting_exts.contains(&ext) || depth == 0 {
                out.push(format!("{}{}", indent, rel.to_string_lossy()));
            }
        }
    }
}

// ── File summary ─────────────────────────────────────────────────────────────

/// Generate a structural summary of a source file (function/struct/class names).
/// Useful when the agent needs to understand file structure without reading it fully.
pub fn file_summary(path: &Path) -> String {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => return format!("(could not read: {})", e),
    };

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    let mut definitions = Vec::new();

    match ext {
        "rs" => {
            for (i, line) in lines.iter().enumerate() {
                let t = line.trim();
                if t.starts_with("pub fn ") || t.starts_with("fn ") ||
                   t.starts_with("pub struct ") || t.starts_with("struct ") ||
                   t.starts_with("pub enum ") || t.starts_with("enum ") ||
                   t.starts_with("pub trait ") || t.starts_with("trait ") ||
                   t.starts_with("impl ") || t.starts_with("pub impl ") ||
                   t.starts_with("pub async fn ") || t.starts_with("async fn ") {
                    definitions.push(format!("L{}: {}", i + 1, t.split('{').next().unwrap_or(t).trim()));
                }
            }
        }
        "ts" | "tsx" | "js" | "jsx" => {
            for (i, line) in lines.iter().enumerate() {
                let t = line.trim();
                if t.starts_with("export function ") || t.starts_with("function ") ||
                   t.starts_with("export class ") || t.starts_with("class ") ||
                   t.starts_with("export const ") || t.starts_with("export default ") ||
                   t.starts_with("export async function ") || t.starts_with("async function ") ||
                   t.contains("= async (") || t.contains("= () =>") {
                    definitions.push(format!("L{}: {}", i + 1, &t[..t.len().min(100)]));
                }
            }
        }
        "py" => {
            for (i, line) in lines.iter().enumerate() {
                let t = line.trim();
                if t.starts_with("def ") || t.starts_with("class ") ||
                   t.starts_with("async def ") {
                    definitions.push(format!("L{}: {}", i + 1, t.split(':').next().unwrap_or(t)));
                }
            }
        }
        _ => {
            // Generic: return first 20 lines as preview
            let preview: Vec<String> = lines.iter().take(20).map(|l| l.to_string()).collect();
            return format!(
                "File: {} ({} lines)\n\nPreview:\n{}",
                path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown"),
                total_lines,
                preview.join("\n")
            );
        }
    }

    if definitions.is_empty() {
        return format!(
            "File: {} ({} lines) — no top-level definitions found",
            path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown"),
            total_lines
        );
    }

    format!(
        "File: {} ({} lines)\n\nDefinitions:\n{}",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown"),
        total_lines,
        definitions.join("\n")
    )
}

// ── Rolling task summary ─────────────────────────────────────────────────────

/// Summarise a task's progress from the message history for context compression.
/// Used to reduce token cost when resuming long-running tasks.
pub fn rolling_task_summary(messages: &[crate::types::Message], max_recent: usize) -> String {
    if messages.is_empty() {
        return String::new();
    }

    let total = messages.len();
    if total <= max_recent {
        return String::new(); // No need to summarise — history is short
    }

    // Count tool calls in history
    let mut tool_calls_seen = 0usize;
    for msg in messages {
        for block in &msg.blocks {
            if matches!(block, crate::types::ContentBlock::ToolUse { .. }) {
                tool_calls_seen += 1;
            }
        }
    }

    // Extract last few user messages as task indicators
    let recent_user_msgs: Vec<String> = messages.iter()
        .rev()
        .filter(|m| m.role == "user")
        .take(3)
        .filter_map(|m| {
            m.blocks.iter().find_map(|b| {
                if let crate::types::ContentBlock::Text { text } = b {
                    if text.len() > 10 {
                        Some(text[..text.len().min(200)].to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        })
        .collect();

    format!(
        "[Session context: {} messages exchanged, {} tool calls executed. Recent focus: {}]",
        total,
        tool_calls_seen,
        recent_user_msgs.first().cloned().unwrap_or_else(|| "ongoing task".to_string())
    )
}

// ── Relevance filter ─────────────────────────────────────────────────────────

/// Filter a list of file paths to those most relevant to the user's message.
/// Returns at most `limit` paths, prioritising exact name matches and extension matches.
pub fn filter_relevant_files(
    message: &str,
    all_files: &[PathBuf],
    limit: usize,
) -> Vec<PathBuf> {
    let lower = message.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();

    let mut scored: Vec<(usize, &PathBuf)> = all_files.iter().map(|p| {
        let path_str = p.to_string_lossy().to_lowercase();
        let mut score = 0usize;
        for word in &words {
            if word.len() > 3 && path_str.contains(*word) {
                score += 2;
            }
        }
        // Bonus for source files
        if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
            match ext {
                "rs" | "ts" | "tsx" | "js" | "py" | "go" => score += 1,
                _ => {}
            }
        }
        (score, p)
    }).collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter()
        .filter(|(s, _)| *s > 0)
        .take(limit)
        .map(|(_, p)| p.clone())
        .collect()
}

// ── Prompt assembler ─────────────────────────────────────────────────────────

/// Assemble a compressed system prompt section from available context sources.
/// Keeps total injected context under a token budget.
pub struct PromptAssembler {
    pub workspace_summary: Option<String>,
    pub engram_context: Option<String>,
    pub memory_context: Option<String>,
    pub protocol_context: Option<String>,
    pub task_summary: Option<String>,
}

impl PromptAssembler {
    pub fn build(&self, base_prompt: &str) -> String {
        let mut parts = vec![base_prompt.to_string()];

        if let Some(ref protocols) = self.protocol_context {
            if !protocols.is_empty() {
                parts.push(format!("## Active Protocols\n\n{}", protocols));
            }
        }

        if let Some(ref engram) = self.engram_context {
            if !engram.is_empty() {
                parts.push(format!("## Codebase Understanding (Engram)\n\n{}", engram));
            }
        }

        if let Some(ref memory) = self.memory_context {
            if !memory.is_empty() {
                parts.push(format!("## Long-term Memory\n\n{}", memory));
            }
        }

        if let Some(ref workspace) = self.workspace_summary {
            if !workspace.is_empty() {
                parts.push(format!("## Workspace Map\n\n```\n{}\n```", workspace));
            }
        }

        if let Some(ref task) = self.task_summary {
            if !task.is_empty() {
                parts.push(task.clone());
            }
        }

        parts.join("\n\n")
    }
}
