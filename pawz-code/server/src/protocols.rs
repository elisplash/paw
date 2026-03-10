// pawz-code — protocols.rs
// Protocol loader: configurable, versioned reasoning protocol packs.
//
// Protocols are loaded from ~/.pawz-code/protocols/ as .toml or .md files.
// They shape the agent's reasoning loop, tool use discipline, and verification habits.
//
// Core protocol packs (loaded by default if no custom files found):
//   - coding       — general coding discipline
//   - edit         — targeted editing patterns
//   - repo_safety  — safety before destructive ops
//   - token        — token reduction habits
//   - verification — verify before claiming success
//   - long_task    — stable long-running task management
//   - memory_write — when and how to remember
//   - diff_review  — review diffs before applying

use crate::state::AppState;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// ── Protocol store ───────────────────────────────────────────────────────────

/// In-memory protocol store shared via AppState.
/// Map of protocol_name → protocol_text
pub type ProtocolStore = Arc<Mutex<HashMap<String, String>>>;

// ── Built-in protocol definitions ────────────────────────────────────────────

fn builtin_protocols() -> HashMap<String, String> {
    let mut map = HashMap::new();

    map.insert(
        "coding".into(),
        "## Coding Protocol\n\
         - Always read before writing. Use read_file, list_directory, grep first.\n\
         - Make small, targeted edits. One logical change at a time.\n\
         - Show what changed and why after every file modification.\n\
         - Run tests or build checks after changes when available.\n\
         - Use recall at the start of complex tasks to surface relevant context.\n\
         - Use remember proactively for architecture decisions and conventions.\n\
         - Prefer exec with git commands (git diff, git status) to understand context.\n\
         - Never assume a file's content — read it first.".into(),
    );

    map.insert(
        "edit".into(),
        "## Edit Protocol\n\
         - Prefer surgical edits over full-file rewrites.\n\
         - For small changes: read the relevant section, write only that section.\n\
         - For large changes: read the full file first, then plan the edit.\n\
         - Always verify the edit result with read_file after writing.\n\
         - If a write fails or produces unexpected output, read and re-assess.\n\
         - Leave surrounding code unchanged unless explicitly asked to refactor.".into(),
    );

    map.insert(
        "repo_safety".into(),
        "## Repo Safety Protocol\n\
         - Never force-push or rebase published branches without explicit confirmation.\n\
         - Never delete files without reading them first.\n\
         - Never truncate files — always rewrite complete content.\n\
         - Check git status and git diff before destructive operations.\n\
         - Prefer dry-run flags when available (e.g. rm -n, rsync --dry-run).\n\
         - Never modify .env files, secret files, or credential stores.\n\
         - When in doubt, show what you would do and ask for confirmation.".into(),
    );

    map.insert(
        "token".into(),
        "## Token Reduction Protocol\n\
         - Never dump full file trees or large directory listings unnecessarily.\n\
         - Use grep/search to find specific relevant sections rather than reading whole files.\n\
         - Prefer workspace_map summaries over raw directory listings for large repos.\n\
         - Use file_summary for large files when only structure matters.\n\
         - Prefer rolling task summaries over full history replay.\n\
         - Recall from memory rather than re-deriving known context.\n\
         - Route classification tasks to cheap models; reserve large context for architecture.".into(),
    );

    map.insert(
        "verification".into(),
        "## Verification Protocol\n\
         - Never claim a task is complete without verifying the output.\n\
         - After writing files: read them back and confirm content is correct.\n\
         - After running commands: check exit codes and output for errors.\n\
         - After making code changes: run the build or tests when possible.\n\
         - If verification fails: fix the issue before reporting success.\n\
         - State what you verified and what the outcome was.".into(),
    );

    map.insert(
        "long_task".into(),
        "## Long Task Protocol\n\
         - Break large tasks into clearly named sub-tasks.\n\
         - Remember the overall goal at the start of each sub-task.\n\
         - Use remember to checkpoint progress after each significant milestone.\n\
         - If approaching max rounds: summarise what was done and what remains.\n\
         - Prefer stable incremental progress over big-bang changes.\n\
         - If context becomes stale: use recall to refresh relevant facts.".into(),
    );

    map.insert(
        "memory_write".into(),
        "## Memory Write Protocol\n\
         - Use remember when you learn a repeatable fact about this codebase.\n\
         - Key naming: use descriptive snake_case keys (e.g. 'db_schema', 'auth_flow').\n\
         - Update existing memory when facts change — don't accumulate stale duplicates.\n\
         - Remember architecture decisions, not implementation details that will change.\n\
         - Use engram_store for compressed structural understanding of the whole codebase.\n\
         - Use recall at the start of sessions to load relevant prior context.".into(),
    );

    map.insert(
        "diff_review".into(),
        "## Diff Review Protocol\n\
         - Before applying any patch: read the full diff and confirm it matches intent.\n\
         - Check for unintended whitespace changes, encoding issues, or truncation.\n\
         - After applying: verify with git diff or by reading the resulting file.\n\
         - For multi-file changes: review each file's changes individually.\n\
         - When showing diffs in VS Code: use the showDiff command for visual review.\n\
         - Never auto-apply patches to generated files, lock files, or binary files.".into(),
    );

    map
}

// ── Load ─────────────────────────────────────────────────────────────────────

/// Load protocols from disk (if available) or fall back to built-ins.
/// Custom protocols in ~/.pawz-code/protocols/ override built-ins by name.
pub fn load_protocols(state: &AppState) {
    let mut protocols = builtin_protocols();

    // Try to load custom protocols from disk
    let protocols_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".pawz-code")
        .join("protocols");

    if protocols_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&protocols_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "md" || ext == "toml" || ext == "txt" {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            log::info!("[protocols] Loaded custom protocol: {}", stem);
                            protocols.insert(stem.to_string(), content);
                        }
                    }
                }
            }
        }
    }

    // Store in AppState protocol store
    let mut store = state.protocols.lock().unwrap_or_else(|e| e.into_inner());
    *store = protocols;
    log::info!("[protocols] Loaded {} protocols", store.len());
}

/// Get list of loaded protocol names.
pub fn loaded_protocol_names(state: &AppState) -> Vec<String> {
    let store = state.protocols.lock().unwrap_or_else(|e| e.into_inner());
    let mut names: Vec<String> = store.keys().cloned().collect();
    names.sort();
    names
}

/// Get a specific protocol's text.
pub fn get_protocol(state: &AppState, name: &str) -> Option<String> {
    let store = state.protocols.lock().unwrap_or_else(|e| e.into_inner());
    store.get(name).cloned()
}

/// Build a combined protocol context string for the system prompt.
/// Returns the selected protocols formatted for injection into the prompt.
pub fn build_protocol_context(state: &AppState, names: &[&str]) -> String {
    let store = state.protocols.lock().unwrap_or_else(|e| e.into_inner());
    let mut parts = Vec::new();
    for name in names {
        if let Some(text) = store.get(*name) {
            parts.push(text.as_str().to_string());
        }
    }
    parts.join("\n\n")
}

/// Build the default protocol context (coding + edit + repo_safety + verification).
pub fn default_protocol_context(state: &AppState) -> String {
    build_protocol_context(
        state,
        &["coding", "edit", "repo_safety", "verification", "memory_write"],
    )
}
