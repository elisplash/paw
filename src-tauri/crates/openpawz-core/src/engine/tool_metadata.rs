// ── Tool Metadata Registry ──────────────────────────────────────────────────
//
// Single source of truth for all tool classification: safety tiers,
// mutability, domain grouping, and worker/orchestrator permissions.
//
// Previously these properties were scattered across 4–5 independent
// `&[&str]` arrays that had to be kept in manual sync. Now every
// consumer (agent_loop, worker_delegate, orchestrator, speculative,
// tool_index) queries this registry.
//
// Adding a new tool: add ONE entry to `TOOL_REGISTRY`.

use std::collections::HashMap;
use std::sync::LazyLock;

// ═════════════════════════════════════════════════════════════════════════════
// Enums
// ═════════════════════════════════════════════════════════════════════════════

/// Safety tier — determines Human-in-the-Loop (HIL) approval behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolTier {
    /// Auto-approved, no modal. Read-only and informational.
    Safe,
    /// Auto-approved. Local reversible side-effects (write_file, memory_store).
    Reversible,
    /// Requires approval (or "Always Allow"). External side-effects (email, API).
    External,
    /// Always requires approval. Dangerous / financial operations.
    Dangerous,
}

/// Read/write classification for speculative execution caching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolMutability {
    /// No side effects — results can be cached and speculated.
    ReadOnly,
    /// Modifies local state (files, DB). Cache can be invalidated.
    WriteLocal,
    /// External side effects (email, API, trade). Never speculate.
    WriteSideEffect,
}

/// Functional domain grouping for tool RAG and sibling suggestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolDomain {
    System,
    Filesystem,
    Web,
    Identity,
    Memory,
    Agents,
    Communication,
    Squads,
    Tasks,
    Skills,
    Canvas,
    Dashboard,
    Storage,
    Email,
    Messaging,
    Github,
    Integrations,
    Coinbase,
    Solana,
    Dex,
    Meta,
    Services,
    Google,
    Discord,
    Discourse,
    Trello,
    Microsoft,
    Mcp,
    N8n,
    Other,
}

// ═════════════════════════════════════════════════════════════════════════════
// Registry entry
// ═════════════════════════════════════════════════════════════════════════════

/// Complete classification for a single tool.
#[derive(Debug, Clone)]
pub struct ToolMeta {
    pub name: &'static str,
    pub tier: ToolTier,
    pub mutability: ToolMutability,
    pub domain: ToolDomain,
    /// Whether workers (delegated subtasks) can use this tool.
    pub worker_allowed: bool,
    /// Whether the orchestrator auto-approves this tool.
    pub orchestrator_safe: bool,
}

// ═════════════════════════════════════════════════════════════════════════════
// The registry — single source of truth
// ═════════════════════════════════════════════════════════════════════════════

/// Macro to reduce boilerplate for registry entries.
macro_rules! tool {
    ($name:expr, $tier:ident, $mut:ident, $domain:ident, $worker:expr, $orch:expr) => {
        ToolMeta {
            name: $name,
            tier: ToolTier::$tier,
            mutability: ToolMutability::$mut,
            domain: ToolDomain::$domain,
            worker_allowed: $worker,
            orchestrator_safe: $orch,
        }
    };
}

/// Complete tool registry. Add new tools here — nowhere else.
pub static TOOL_REGISTRY: &[ToolMeta] = &[
    // ── System ──────────────────────────────────────────────────────────
    tool!("exec", Dangerous, WriteSideEffect, System, false, false),
    tool!(
        "run_command",
        Dangerous,
        WriteSideEffect,
        System,
        false,
        false
    ),
    // ── Filesystem ──────────────────────────────────────────────────────
    tool!("read_file", Safe, ReadOnly, Filesystem, true, true),
    tool!(
        "write_file",
        Reversible,
        WriteLocal,
        Filesystem,
        false,
        false
    ),
    tool!(
        "append_file",
        Reversible,
        WriteLocal,
        Filesystem,
        false,
        false
    ),
    tool!(
        "delete_file",
        Dangerous,
        WriteLocal,
        Filesystem,
        false,
        false
    ),
    tool!("list_directory", Safe, ReadOnly, Filesystem, true, true),
    // ── Web ─────────────────────────────────────────────────────────────
    tool!("fetch", Safe, ReadOnly, Web, true, true),
    tool!("web_search", Safe, ReadOnly, Web, true, true),
    tool!("web_read", Safe, ReadOnly, Web, true, true),
    tool!("web_screenshot", Safe, ReadOnly, Web, true, true),
    tool!("web_browse", Safe, ReadOnly, Web, true, true),
    // ── Identity ────────────────────────────────────────────────────────
    tool!("soul_read", Safe, ReadOnly, Identity, true, true),
    tool!("soul_write", Reversible, WriteLocal, Identity, true, true),
    tool!("soul_list", Safe, ReadOnly, Identity, true, true),
    tool!("self_info", Safe, ReadOnly, Identity, true, true),
    tool!(
        "update_profile",
        Reversible,
        WriteLocal,
        Identity,
        true,
        false
    ),
    // ── Memory ──────────────────────────────────────────────────────────
    tool!("memory_search", Safe, ReadOnly, Memory, true, true),
    tool!("memory_store", Reversible, WriteLocal, Memory, true, true),
    tool!(
        "memory_knowledge",
        Reversible,
        WriteLocal,
        Memory,
        true,
        false
    ),
    tool!("memory_stats", Safe, ReadOnly, Memory, true, false),
    // ── Agents ──────────────────────────────────────────────────────────
    tool!("agent_list", Safe, ReadOnly, Agents, true, false),
    tool!("agent_skills", Safe, ReadOnly, Agents, true, false),
    tool!(
        "agent_skill_assign",
        Reversible,
        WriteLocal,
        Agents,
        true,
        false
    ),
    // ── Communication ───────────────────────────────────────────────────
    tool!(
        "agent_send_message",
        Reversible,
        WriteSideEffect,
        Communication,
        true,
        true
    ),
    tool!(
        "agent_read_messages",
        Safe,
        ReadOnly,
        Communication,
        true,
        true
    ),
    // ── Squads ──────────────────────────────────────────────────────────
    tool!("create_squad", Reversible, WriteLocal, Squads, true, true),
    tool!("list_squads", Safe, ReadOnly, Squads, true, true),
    tool!("manage_squad", Reversible, WriteLocal, Squads, true, true),
    tool!(
        "squad_broadcast",
        Reversible,
        WriteSideEffect,
        Squads,
        true,
        true
    ),
    // ── Tasks ───────────────────────────────────────────────────────────
    tool!("create_task", Reversible, WriteLocal, Tasks, true, true),
    tool!("list_tasks", Safe, ReadOnly, Tasks, true, true),
    tool!("manage_task", Reversible, WriteLocal, Tasks, true, false),
    // ── Skills ──────────────────────────────────────────────────────────
    tool!("skill_search", Safe, ReadOnly, Skills, true, false),
    tool!("skill_list", Safe, ReadOnly, Skills, true, false),
    tool!("skill_install", Reversible, WriteLocal, Skills, true, false),
    // ── Canvas ──────────────────────────────────────────────────────────
    tool!("canvas_push", Safe, WriteLocal, Canvas, true, true),
    tool!("canvas_update", Safe, WriteLocal, Canvas, true, true),
    tool!("canvas_remove", Safe, WriteLocal, Canvas, true, true),
    tool!("canvas_clear", Safe, WriteLocal, Canvas, true, true),
    tool!("canvas_save", Safe, WriteLocal, Canvas, true, true),
    tool!("canvas_load", Safe, ReadOnly, Canvas, true, true),
    tool!("canvas_list_dashboards", Safe, ReadOnly, Canvas, true, true),
    tool!(
        "canvas_delete_dashboard",
        Safe,
        WriteLocal,
        Canvas,
        true,
        true
    ),
    tool!("canvas_list_templates", Safe, ReadOnly, Canvas, true, true),
    tool!("canvas_from_template", Safe, WriteLocal, Canvas, true, true),
    tool!(
        "canvas_create_template",
        Safe,
        WriteLocal,
        Canvas,
        true,
        true
    ),
    // ── Dashboard / Storage ─────────────────────────────────────────────
    tool!("skill_output", Safe, ReadOnly, Dashboard, true, false),
    tool!(
        "delete_skill_output",
        Reversible,
        WriteLocal,
        Dashboard,
        true,
        false
    ),
    tool!(
        "skill_store_set",
        Reversible,
        WriteLocal,
        Storage,
        true,
        false
    ),
    tool!("skill_store_get", Safe, ReadOnly, Storage, true, false),
    tool!("skill_store_list", Safe, ReadOnly, Storage, true, false),
    tool!(
        "skill_store_delete",
        Reversible,
        WriteLocal,
        Storage,
        true,
        false
    ),
    // ── Email ───────────────────────────────────────────────────────────
    tool!("email_send", External, WriteSideEffect, Email, false, false),
    tool!("email_read", Safe, ReadOnly, Email, true, true),
    // ── Messaging ───────────────────────────────────────────────────────
    tool!(
        "slack_send",
        External,
        WriteSideEffect,
        Messaging,
        false,
        false
    ),
    tool!("slack_read", Safe, ReadOnly, Messaging, true, true),
    tool!(
        "telegram_send",
        External,
        WriteSideEffect,
        Messaging,
        false,
        false
    ),
    tool!("telegram_read", Safe, ReadOnly, Messaging, true, false),
    // ── Github ──────────────────────────────────────────────────────────
    tool!(
        "github_api",
        External,
        WriteSideEffect,
        Github,
        false,
        false
    ),
    // ── Integrations ────────────────────────────────────────────────────
    tool!(
        "rest_api_call",
        External,
        WriteSideEffect,
        Integrations,
        false,
        false
    ),
    tool!(
        "webhook_send",
        External,
        WriteSideEffect,
        Integrations,
        false,
        false
    ),
    tool!(
        "image_generate",
        External,
        WriteSideEffect,
        Integrations,
        true,
        true
    ),
    tool!(
        "service_api",
        External,
        WriteSideEffect,
        Services,
        false,
        false
    ),
    // ── Meta ────────────────────────────────────────────────────────────
    tool!("request_tools", Safe, ReadOnly, Meta, true, false),
    tool!("mcp_refresh", Safe, ReadOnly, Mcp, true, true),
    tool!("execute_plan", Safe, ReadOnly, Other, true, false),
    // ── n8n ─────────────────────────────────────────────────────────────
    tool!("search_ncnodes", Safe, ReadOnly, N8n, true, true),
    tool!("n8n_list_workflows", Safe, ReadOnly, N8n, true, true),
    // ── Google Workspace ────────────────────────────────────────────────
    tool!("google_gmail_list", Safe, ReadOnly, Google, true, false),
    tool!("google_gmail_read", Safe, ReadOnly, Google, true, false),
    tool!(
        "google_gmail_send",
        External,
        WriteSideEffect,
        Google,
        true,
        false
    ),
    tool!(
        "google_docs_create",
        External,
        WriteSideEffect,
        Google,
        true,
        false
    ),
    tool!("google_drive_list", Safe, ReadOnly, Google, true, false),
    tool!("google_drive_read", Safe, ReadOnly, Google, true, false),
    tool!(
        "google_drive_upload",
        External,
        WriteSideEffect,
        Google,
        true,
        false
    ),
    tool!(
        "google_drive_share",
        External,
        WriteSideEffect,
        Google,
        true,
        false
    ),
    tool!("google_calendar_list", Safe, ReadOnly, Google, true, false),
    tool!(
        "google_calendar_create",
        External,
        WriteSideEffect,
        Google,
        true,
        false
    ),
    tool!("google_sheets_read", Safe, ReadOnly, Google, true, false),
    tool!(
        "google_sheets_append",
        External,
        WriteSideEffect,
        Google,
        true,
        false
    ),
    tool!("google_api", External, WriteSideEffect, Google, true, false),
    // ── Trello ──────────────────────────────────────────────────────────
    tool!("trello_list_boards", Safe, ReadOnly, Trello, true, false),
    tool!("trello_get_board", Safe, ReadOnly, Trello, true, false),
    tool!("trello_get_lists", Safe, ReadOnly, Trello, true, false),
    tool!("trello_get_cards", Safe, ReadOnly, Trello, true, false),
    tool!("trello_get_card", Safe, ReadOnly, Trello, true, false),
    tool!("trello_search", Safe, ReadOnly, Trello, true, false),
    tool!("trello_get_labels", Safe, ReadOnly, Trello, true, false),
    tool!("trello_get_members", Safe, ReadOnly, Trello, true, false),
    tool!(
        "trello_create_board",
        External,
        WriteSideEffect,
        Trello,
        true,
        false
    ),
    tool!(
        "trello_update_board",
        External,
        WriteSideEffect,
        Trello,
        true,
        false
    ),
    tool!(
        "trello_create_list",
        External,
        WriteSideEffect,
        Trello,
        true,
        false
    ),
    tool!(
        "trello_update_list",
        External,
        WriteSideEffect,
        Trello,
        true,
        false
    ),
    tool!(
        "trello_archive_list",
        External,
        WriteSideEffect,
        Trello,
        true,
        false
    ),
    tool!(
        "trello_create_card",
        External,
        WriteSideEffect,
        Trello,
        true,
        false
    ),
    tool!(
        "trello_update_card",
        External,
        WriteSideEffect,
        Trello,
        true,
        false
    ),
    tool!(
        "trello_move_card",
        External,
        WriteSideEffect,
        Trello,
        true,
        false
    ),
    tool!(
        "trello_add_comment",
        External,
        WriteSideEffect,
        Trello,
        true,
        false
    ),
    tool!(
        "trello_create_label",
        External,
        WriteSideEffect,
        Trello,
        true,
        false
    ),
    tool!(
        "trello_update_label",
        External,
        WriteSideEffect,
        Trello,
        true,
        false
    ),
    tool!(
        "trello_add_label",
        External,
        WriteSideEffect,
        Trello,
        true,
        false
    ),
    tool!(
        "trello_remove_label",
        External,
        WriteSideEffect,
        Trello,
        true,
        false
    ),
    tool!(
        "trello_create_checklist",
        External,
        WriteSideEffect,
        Trello,
        true,
        false
    ),
    tool!(
        "trello_add_checklist_item",
        External,
        WriteSideEffect,
        Trello,
        true,
        false
    ),
    tool!(
        "trello_toggle_checklist_item",
        External,
        WriteSideEffect,
        Trello,
        true,
        false
    ),
    // ── Coinbase ────────────────────────────────────────────────────────
    tool!("coinbase_prices", Safe, ReadOnly, Coinbase, true, false),
    tool!("coinbase_balance", Safe, ReadOnly, Coinbase, true, false),
    tool!(
        "coinbase_wallet_create",
        Dangerous,
        WriteSideEffect,
        Coinbase,
        true,
        false
    ),
    tool!(
        "coinbase_trade",
        Dangerous,
        WriteSideEffect,
        Coinbase,
        true,
        false
    ),
    tool!(
        "coinbase_transfer",
        Dangerous,
        WriteSideEffect,
        Coinbase,
        true,
        false
    ),
    // ── Solana DEX ──────────────────────────────────────────────────────
    tool!("sol_balance", Safe, ReadOnly, Solana, true, false),
    tool!("sol_quote", Safe, ReadOnly, Solana, true, false),
    tool!("sol_portfolio", Safe, ReadOnly, Solana, true, false),
    tool!("sol_token_info", Safe, ReadOnly, Solana, true, false),
    tool!("sol_swap", Dangerous, WriteSideEffect, Solana, true, false),
    tool!(
        "sol_transfer",
        Dangerous,
        WriteSideEffect,
        Solana,
        true,
        false
    ),
    tool!(
        "sol_wallet_create",
        Dangerous,
        WriteSideEffect,
        Solana,
        true,
        false
    ),
    // ── EVM DEX ─────────────────────────────────────────────────────────
    tool!("dex_balance", Safe, ReadOnly, Dex, true, false),
    tool!("dex_quote", Safe, ReadOnly, Dex, true, false),
    tool!("dex_portfolio", Safe, ReadOnly, Dex, true, false),
    tool!("dex_token_info", Safe, ReadOnly, Dex, true, false),
    tool!("dex_check_token", Safe, ReadOnly, Dex, true, false),
    tool!("dex_search_token", Safe, ReadOnly, Dex, true, false),
    tool!("dex_watch_wallet", Safe, ReadOnly, Dex, true, false),
    tool!("dex_whale_transfers", Safe, ReadOnly, Dex, true, false),
    tool!("dex_top_traders", Safe, ReadOnly, Dex, true, false),
    tool!("dex_trending", Safe, ReadOnly, Dex, true, false),
    tool!("dex_swap", Dangerous, WriteSideEffect, Dex, true, false),
    tool!("dex_transfer", Dangerous, WriteSideEffect, Dex, true, false),
    tool!(
        "dex_wallet_create",
        Dangerous,
        WriteSideEffect,
        Dex,
        true,
        false
    ),
    // ── Orchestrator-only (not exposed to regular agents) ───────────────
    tool!("delegate_task", Safe, ReadOnly, Other, true, true),
    tool!("check_agent_status", Safe, ReadOnly, Other, true, true),
    tool!(
        "send_agent_message",
        Reversible,
        WriteSideEffect,
        Other,
        true,
        true
    ),
    tool!(
        "project_complete",
        Reversible,
        WriteSideEffect,
        Other,
        true,
        true
    ),
    tool!(
        "create_sub_agent",
        Reversible,
        WriteSideEffect,
        Other,
        true,
        true
    ),
    tool!("report_progress", Reversible, WriteLocal, Other, true, true),
    tool!(
        "manage_session",
        Reversible,
        WriteLocal,
        Agents,
        true,
        false
    ),
    tool!("create_agent", Reversible, WriteLocal, Agents, true, false),
];

// ═════════════════════════════════════════════════════════════════════════════
// Indexed lookups (built once, used everywhere)
// ═════════════════════════════════════════════════════════════════════════════

static TOOL_MAP: LazyLock<HashMap<&'static str, &'static ToolMeta>> =
    LazyLock::new(|| TOOL_REGISTRY.iter().map(|m| (m.name, m)).collect());

/// Look up metadata for a known tool. Returns `None` for MCP/dynamic tools.
pub fn get(name: &str) -> Option<&'static ToolMeta> {
    TOOL_MAP.get(name).copied()
}

/// Get the safety tier for a tool. Unknown tools default to `External`
/// (requires approval) which is the safe default.
pub fn tier(name: &str) -> ToolTier {
    get(name).map_or(ToolTier::External, |m| m.tier)
}

/// Get the mutability classification. Unknown tools default to `WriteSideEffect`
/// (never speculate) which is the safe default.
pub fn mutability(name: &str) -> ToolMutability {
    // Known tools — exact registry lookup
    if let Some(m) = get(name) {
        return m.mutability;
    }
    // Dynamic / prefix-based tools — use naming convention heuristics.
    // Conservative: only _list/_get/_search/_read/_price/_balance are ReadOnly.
    dynamic_mutability(name)
}

/// Get the domain for a tool. Unknown tools use prefix-based heuristics.
pub fn domain(name: &str) -> ToolDomain {
    if let Some(m) = get(name) {
        return m.domain;
    }
    // Prefix-based fallback for dynamic tools
    if name.starts_with("mcp_") {
        return ToolDomain::Mcp;
    }
    if name.starts_with("discord_") {
        return ToolDomain::Discord;
    }
    if name.starts_with("discourse_") {
        return ToolDomain::Discourse;
    }
    if name.starts_with("google_") || name.starts_with("gmail_") {
        return ToolDomain::Google;
    }
    if name.starts_with("trello_") {
        return ToolDomain::Trello;
    }
    if name.starts_with("outlook_")
        || name.starts_with("onedrive_")
        || name.starts_with("teams_")
        || name.starts_with("ms_tasks_")
        || name.starts_with("onenote_")
        || name == "microsoft_api"
    {
        return ToolDomain::Microsoft;
    }
    if name.starts_with("dex_") {
        return ToolDomain::Dex;
    }
    if name.starts_with("sol_") {
        return ToolDomain::Solana;
    }
    ToolDomain::Other
}

/// Check if a tool is allowed for worker agents (delegated subtasks).
pub fn worker_allowed(name: &str) -> bool {
    // MCP tools use naming convention blocklist
    if name.starts_with("mcp_") {
        return !is_blocked_mcp_name(name);
    }
    get(name).is_some_and(|m| m.worker_allowed)
}

/// Check if a tool is auto-approved in the orchestrator.
pub fn orchestrator_safe(name: &str) -> bool {
    get(name).is_some_and(|m| m.orchestrator_safe)
}

/// Get the domain name as a string (for tool_index compatibility).
pub fn domain_str(name: &str) -> &'static str {
    match domain(name) {
        ToolDomain::System => "system",
        ToolDomain::Filesystem => "filesystem",
        ToolDomain::Web => "web",
        ToolDomain::Identity => "identity",
        ToolDomain::Memory => "memory",
        ToolDomain::Agents => "agents",
        ToolDomain::Communication => "communication",
        ToolDomain::Squads => "squads",
        ToolDomain::Tasks => "tasks",
        ToolDomain::Skills => "skills",
        ToolDomain::Canvas => "canvas",
        ToolDomain::Dashboard => "dashboard",
        ToolDomain::Storage => "storage",
        ToolDomain::Email => "email",
        ToolDomain::Messaging => "messaging",
        ToolDomain::Github => "github",
        ToolDomain::Integrations => "integrations",
        ToolDomain::Coinbase => "coinbase",
        ToolDomain::Solana => "solana",
        ToolDomain::Dex => "dex",
        ToolDomain::Meta => "meta",
        ToolDomain::Services => "services",
        ToolDomain::Google => "google",
        ToolDomain::Discord => "discord",
        ToolDomain::Discourse => "discourse",
        ToolDomain::Trello => "trello",
        ToolDomain::Microsoft => "microsoft",
        ToolDomain::Mcp => "mcp",
        ToolDomain::N8n => "n8n",
        ToolDomain::Other => "other",
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// MCP tool heuristics
// ═════════════════════════════════════════════════════════════════════════════

/// Blocked MCP tool name patterns for worker agents.
const BLOCKED_MCP_PATTERNS: &[&str] = &[
    "exec",
    "shell",
    "run_command",
    "terminal",
    "system",
    "write_file",
    "delete_file",
    "remove_file",
    "file_write",
    "rm_rf",
    "rmdir",
    "unlink",
];

/// Check if an MCP tool name matches a dangerous pattern.
fn is_blocked_mcp_name(name: &str) -> bool {
    let stripped = name.strip_prefix("mcp_").unwrap_or(name);
    // Strip server prefix (e.g., "mcp_myserver_exec" → "exec")
    let after_server = stripped
        .find('_')
        .map(|i| &stripped[i + 1..])
        .unwrap_or(stripped);
    BLOCKED_MCP_PATTERNS
        .iter()
        .any(|pat| after_server.contains(pat))
}

/// Naming-convention mutability for any dynamic/prefixed tool.
/// Applies the same read-only heuristics used for MCP tools to all prefixed
/// tools (discord_, google_, gmail_, trello_, discourse_, dex_, sol_, etc.).
fn dynamic_mutability(name: &str) -> ToolMutability {
    let lower = name.to_lowercase();
    if lower.contains("_list")
        || lower.contains("_get")
        || lower.contains("_search")
        || lower.contains("_read")
        || lower.contains("_price")
        || lower.contains("_balance")
    {
        ToolMutability::ReadOnly
    } else {
        ToolMutability::WriteSideEffect
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Convenience: collect tools by tier
// ═════════════════════════════════════════════════════════════════════════════

/// All tools in the given tier.
pub fn tools_in_tier(tier: ToolTier) -> Vec<&'static str> {
    TOOL_REGISTRY
        .iter()
        .filter(|m| m.tier == tier)
        .map(|m| m.name)
        .collect()
}

/// All tools that are safe + reversible (tiers 1+2) — auto-approved.
pub fn auto_approved_tools() -> Vec<&'static str> {
    TOOL_REGISTRY
        .iter()
        .filter(|m| matches!(m.tier, ToolTier::Safe | ToolTier::Reversible))
        .map(|m| m.name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_duplicate_tool_names() {
        let mut seen = std::collections::HashSet::new();
        for meta in TOOL_REGISTRY {
            assert!(
                seen.insert(meta.name),
                "Duplicate tool name in TOOL_REGISTRY: {}",
                meta.name
            );
        }
    }

    #[test]
    fn dangerous_tools_are_not_orchestrator_safe() {
        for meta in TOOL_REGISTRY {
            if meta.tier == ToolTier::Dangerous {
                assert!(
                    !meta.orchestrator_safe,
                    "Dangerous tool {} should not be orchestrator_safe",
                    meta.name
                );
            }
        }
    }

    #[test]
    fn worker_blocked_tools_not_auto_approved() {
        for meta in TOOL_REGISTRY {
            if !meta.worker_allowed {
                assert!(
                    !matches!(meta.tier, ToolTier::Safe),
                    "Tool {} is blocked for workers but in the Safe tier — review classification",
                    meta.name
                );
            }
        }
    }

    #[test]
    fn read_only_tools_in_safe_tier() {
        for meta in TOOL_REGISTRY {
            if meta.mutability == ToolMutability::ReadOnly && meta.tier == ToolTier::Dangerous {
                panic!(
                    "ReadOnly tool {} is in Dangerous tier — should it be lower?",
                    meta.name
                );
            }
        }
    }

    #[test]
    fn lookup_works() {
        assert_eq!(tier("exec"), ToolTier::Dangerous);
        assert_eq!(tier("fetch"), ToolTier::Safe);
        assert_eq!(tier("email_send"), ToolTier::External);
        assert_eq!(tier("unknown_tool"), ToolTier::External); // safe default

        assert!(worker_allowed("fetch"));
        assert!(!worker_allowed("exec"));
        assert!(!worker_allowed("mcp_server_exec"));
        assert!(worker_allowed("mcp_server_list_items"));

        assert_eq!(domain_str("read_file"), "filesystem");
        assert_eq!(domain_str("mcp_something"), "mcp");
        assert_eq!(domain_str("discord_send"), "discord");
    }

    #[test]
    fn mcp_mutability_heuristics() {
        assert_eq!(
            mutability("mcp_server_list_items"),
            ToolMutability::ReadOnly
        );
        assert_eq!(mutability("mcp_server_get_user"), ToolMutability::ReadOnly);
        assert_eq!(
            mutability("mcp_server_create_item"),
            ToolMutability::WriteSideEffect
        );
    }
}
