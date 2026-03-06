// Paw Agent Engine — Tool RAG Index (Intent-Stated Retrieval)
//
// The "Librarian" pattern: instead of dumping 400+ tool definitions into every
// LLM request (~40,000 tokens), we embed tool descriptions and let the agent
// discover tools on demand via semantic search.
//
// Architecture:
//   PATRON  (Cloud LLM)  — sees core tools + `request_tools` meta-tool
//   LIBRARIAN (Ollama)    — embeds queries, searches tool index, returns schemas
//   LIBRARY  (ToolIndex)  — holds tool definitions + their embedding vectors
//
// On startup, all tool definitions are embedded once (~400 vectors × 768 dims).
// When the agent calls `request_tools("send email to john")`, we embed the
// query, compute cosine similarity, and return the top-K matching tools.
// Those tools get injected into the next agent loop round.
//
// Token savings: ~5,000-8,500 tokens per request (25% of a 32K context window).

use crate::atoms::error::EngineResult;
use crate::atoms::types::ToolDefinition;
use crate::engine::memory::EmbeddingClient;
use crate::engine::util::safe_truncate;
use log::{info, warn};
use std::collections::{HashMap, HashSet};

/// A tool definition paired with its embedding vector.
struct IndexedTool {
    definition: ToolDefinition,
    /// Semantic embedding of "{name}: {description}"
    embedding: Vec<f32>,
    /// Skill domain for grouping (e.g., "trading", "email", "web")
    domain: String,
}

/// In-memory index of all tool definitions with their embeddings.
/// Supports semantic search for tool discovery ("the librarian").
pub struct ToolIndex {
    tools: Vec<IndexedTool>,
    /// Whether the index has been populated with embeddings.
    ready: bool,
}

/// Core tools that are ALWAYS sent to the model (never gated behind request_tools).
/// These are the basics every agent needs to function.
pub const CORE_TOOLS: &[&str] = &[
    "memory_store",
    "memory_search",
    "soul_read",
    "soul_write",
    "soul_list",
    "self_info",
    "read_file",
    "write_file",
    "list_directory",
    "request_tools",
];

/// Map tool names to their skill domain for grouping.
pub fn tool_domain(name: &str) -> &'static str {
    match name {
        // System & Files
        "exec" => "system",
        "read_file" | "write_file" | "append_file" | "delete_file" | "list_directory" => {
            "filesystem"
        }

        // Web & Research
        "fetch" => "web",
        "web_search" | "web_read" | "web_screenshot" | "web_browse" => "web",

        // Identity & Memory
        "soul_read" | "soul_write" | "soul_list" => "identity",
        "memory_store" | "memory_search" => "memory",
        "self_info" | "update_profile" => "identity",

        // Agent Management
        "create_agent" | "agent_list" | "agent_skills" | "agent_skill_assign"
        | "manage_session" => "agents",

        // Inter-Agent Communication
        "agent_send_message" | "agent_read_messages" => "communication",

        // Squads
        "create_squad" | "list_squads" | "manage_squad" | "squad_broadcast" => "squads",

        // Tasks & Automation
        "create_task" | "list_tasks" | "manage_task" => "tasks",

        // Skills Ecosystem
        "skill_search" | "skill_install" | "skill_list" => "skills",

        // Canvas (Agent Canvas — bento-grid dashboard widgets)
        "canvas_push" | "canvas_update" | "canvas_remove" | "canvas_clear" => "canvas",
        "canvas_save" | "canvas_load" | "canvas_list_dashboards" | "canvas_delete_dashboard" => {
            "canvas"
        }
        "canvas_list_templates" | "canvas_from_template" | "canvas_create_template" => "canvas",

        // Dashboard & Storage
        "skill_output" | "delete_skill_output" => "dashboard",
        "skill_store_set" | "skill_store_get" | "skill_store_list" | "skill_store_delete" => {
            "storage"
        }

        // Email
        "email_send" | "email_read" => "email",

        // Messaging
        "slack_send" | "slack_read" => "messaging",
        "telegram_send" | "telegram_read" => "messaging",

        // Discord (channels, messages, roles, members, server)
        n if n.starts_with("discord_") => "discord",

        // Discourse (topics, posts, categories, users, search, admin)
        n if n.starts_with("discourse_") => "discourse",

        // Trello (boards, lists, cards, labels, checklists, members)
        n if n.starts_with("trello_") => "trello",

        // GitHub
        "github_api" => "github",

        // Google Workspace
        n if n.starts_with("google_") => "google",

        // Integrations
        "rest_api_call" | "webhook_send" | "image_generate" => "integrations",

        // Trading — Coinbase
        "coinbase_prices"
        | "coinbase_balance"
        | "coinbase_wallet_create"
        | "coinbase_trade"
        | "coinbase_transfer" => "coinbase",

        // Trading — DEX
        n if n.starts_with("dex_") => "dex",

        // Trading — Solana
        n if n.starts_with("sol_") => "solana",

        // MCP tools
        n if n.starts_with("mcp_") => "mcp",

        // Tool RAG meta-tool
        "request_tools" => "meta",

        _ => "other",
    }
}

/// Describe each skill domain in a compact summary for the system prompt.
/// The agent sees these summaries instead of full tool definitions.
pub fn domain_summaries() -> Vec<(&'static str, &'static str, &'static str)> {
    // (domain_id, icon, description)
    vec![
        ("system", "terminal", "Execute shell commands"),
        (
            "filesystem",
            "folder",
            "Read, write, delete, list files in your workspace",
        ),
        (
            "web",
            "language",
            "Search the web, browse pages, take screenshots, fetch URLs",
        ),
        (
            "identity",
            "person",
            "Read/update your soul files and profile",
        ),
        (
            "memory",
            "psychology",
            "Store and search long-term memories",
        ),
        (
            "agents",
            "group",
            "Create and manage AI agents, assign skills",
        ),
        ("communication", "chat", "Send/read messages between agents"),
        (
            "squads",
            "groups",
            "Create agent teams, broadcast to squad members",
        ),
        (
            "tasks",
            "task_alt",
            "Create tasks, manage automations, set cron schedules",
        ),
        (
            "skills",
            "extension",
            "Search, install, and list community skills",
        ),
        (
            "dashboard",
            "dashboard",
            "Push data to the Today dashboard widgets",
        ),
        (
            "canvas",
            "grid_view",
            "Agent Canvas — create, update, and manage bento-grid dashboard widgets, save/load dashboards, use templates",
        ),
        (
            "storage",
            "storage",
            "Persistent key-value storage for extensions",
        ),
        ("email", "mail", "Send and read emails via IMAP/SMTP"),
        (
            "google",
            "mail",
            "Google Workspace — Gmail, Calendar, Drive, Sheets, Docs",
        ),
        ("messaging", "forum", "Slack and Telegram messaging"),
        (
            "discord",
            "forum",
            "Discord server management — list, create, and organize channels; send messages",
        ),
        (
            "discourse",
            "forum",
            "Discourse forum management — topics, posts, categories, users, search, tags, badges, groups, site settings, backups",
        ),
        (
            "trello",
            "view_kanban",
            "Trello project management — boards, lists, cards, labels, checklists",
        ),
        ("github", "code", "GitHub API calls (issues, PRs, repos)"),
        (
            "integrations",
            "api",
            "REST APIs, webhooks, image generation",
        ),
        (
            "coinbase",
            "trending_up",
            "Coinbase exchange — prices, balances, trades, transfers",
        ),
        (
            "dex",
            "trending_up",
            "DEX/Uniswap — swaps, quotes, whale tracking, trending tokens",
        ),
        (
            "solana",
            "trending_up",
            "Solana/Jupiter — swaps, quotes, token info, portfolio",
        ),
    ]
}

impl Default for ToolIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolIndex {
    /// Create a new empty tool index.
    pub fn new() -> Self {
        ToolIndex {
            tools: Vec::new(),
            ready: false,
        }
    }

    /// Populate the index by embedding all tool definitions.
    /// Called once on startup (or lazily on first request_tools call).
    /// Uses the existing Ollama embedding client (~50ms per tool, ~4s total).
    pub async fn build(&mut self, all_tools: &[ToolDefinition], client: &EmbeddingClient) {
        info!(
            "[tool-index] Building tool index for {} definitions...",
            all_tools.len()
        );
        self.tools.clear();

        let mut success = 0;
        let mut failed = 0;
        let mut consecutive_failures = 0;
        let mut embed_disabled = false;

        for tool in all_tools {
            // Circuit breaker: if we've had 3+ consecutive failures, skip embedding
            // for the rest (the provider likely doesn't support embeddings at all).
            if embed_disabled {
                self.tools.push(IndexedTool {
                    definition: tool.clone(),
                    embedding: Vec::new(),
                    domain: tool_domain(&tool.function.name).to_string(),
                });
                failed += 1;
                continue;
            }

            let text = format!("{}: {}", tool.function.name, tool.function.description);
            match client.embed(&text).await {
                Ok(embedding) => {
                    self.tools.push(IndexedTool {
                        definition: tool.clone(),
                        embedding,
                        domain: tool_domain(&tool.function.name).to_string(),
                    });
                    success += 1;
                    consecutive_failures = 0;
                }
                Err(e) => {
                    let err_str = e.to_string();
                    if failed == 0 {
                        // Only log the full error once
                        warn!(
                            "[tool-index] Failed to embed tool '{}': {}",
                            tool.function.name, err_str
                        );
                    }
                    // Still add the tool without embedding — it can be found by name or domain
                    self.tools.push(IndexedTool {
                        definition: tool.clone(),
                        embedding: Vec::new(),
                        domain: tool_domain(&tool.function.name).to_string(),
                    });
                    failed += 1;
                    consecutive_failures += 1;

                    // After 3 consecutive failures, give up embedding entirely
                    if consecutive_failures >= 3 {
                        warn!(
                            "[tool-index] Embedding unavailable after {} consecutive failures — skipping remaining {} tools",
                            consecutive_failures,
                            all_tools.len() - success - failed
                        );
                        embed_disabled = true;
                    }
                }
            }
        }

        self.ready = true;
        info!(
            "[tool-index] Index built: {} tools ({} embedded, {} unembedded)",
            self.tools.len(),
            success,
            failed
        );
    }

    /// Whether the index has been populated.
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// Search the index for tools matching a query.
    /// Returns top-K tools sorted by cosine similarity.
    ///
    /// Also includes all tools from the same domain as the best match,
    /// so "send email" returns both email_send AND email_read.
    pub async fn search(
        &self,
        query: &str,
        top_k: usize,
        client: &EmbeddingClient,
    ) -> EngineResult<Vec<ToolDefinition>> {
        if self.tools.is_empty() {
            return Ok(Vec::new());
        }

        // ── Fast path: if the query is (or contains) a known domain name,
        //    return the entire domain immediately. This fixes single-word
        //    queries like "trello" that embedding models match poorly.
        let query_lower = query.to_lowercase();
        let known_domains: Vec<String> = {
            let mut d: HashSet<String> = HashSet::new();
            for t in &self.tools {
                d.insert(t.domain.clone());
            }
            d.into_iter().collect()
        };
        for domain in &known_domains {
            // Match "trello", "trello cards", "send trello card", etc.
            if query_lower == *domain
                || query_lower.starts_with(&format!("{} ", domain))
                || query_lower.ends_with(&format!(" {}", domain))
                || query_lower.contains(&format!(" {} ", domain))
            {
                let domain_tools: Vec<ToolDefinition> = self
                    .tools
                    .iter()
                    .filter(|t| t.domain == *domain)
                    .map(|t| t.definition.clone())
                    .collect();
                if !domain_tools.is_empty() {
                    info!(
                        "[tool-index] Domain keyword match: '{}' → {} tools from '{}' domain",
                        query,
                        domain_tools.len(),
                        domain
                    );
                    return Ok(domain_tools);
                }
            }
        }

        // Embed the query
        let query_vec = client.embed(query).await?;

        // Score every tool by cosine similarity
        let mut scored: Vec<(usize, f64)> = self
            .tools
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.embedding.is_empty())
            .map(|(i, t)| {
                let sim = cosine_similarity(&query_vec, &t.embedding);
                (i, sim)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top-K
        let top: Vec<(usize, f64)> = scored.into_iter().take(top_k).collect();

        if top.is_empty() {
            return Ok(Vec::new());
        }

        // Collect the matching domains — gated by quality thresholds.
        // A domain gets expanded (all sibling tools included) only when:
        //   a) The best match from that domain scores >= DOMAIN_EXPAND_STRONG (one strong hit), OR
        //   b) 2+ tools from that domain appear in top-K above MIN_RELEVANCE.
        const MIN_RELEVANCE: f64 = 0.55;
        const DOMAIN_EXPAND_STRONG: f64 = 0.70;

        let mut matched_domains: HashSet<String> = HashSet::new();
        let mut domain_best_score: HashMap<String, f64> = HashMap::new();
        let mut domain_hit_count: HashMap<String, u32> = HashMap::new();
        let mut result_names: HashSet<String> = HashSet::new();
        let mut results: Vec<ToolDefinition> = Vec::new();

        for (idx, score) in &top {
            let tool = &self.tools[*idx];
            info!(
                "[tool-index] Match: {} (domain={}, score={:.3})",
                tool.definition.function.name, tool.domain, score
            );
            // Always include direct hits above minimum relevance
            if *score >= MIN_RELEVANCE {
                if result_names.insert(tool.definition.function.name.clone()) {
                    results.push(tool.definition.clone());
                }
                // Track per-domain stats for expansion decision
                let best = domain_best_score.entry(tool.domain.clone()).or_insert(0.0);
                if *score > *best {
                    *best = *score;
                }
                *domain_hit_count.entry(tool.domain.clone()).or_insert(0) += 1;
            }
        }

        // Decide which domains deserve full expansion
        for (domain, best_score) in &domain_best_score {
            let hits = domain_hit_count.get(domain).copied().unwrap_or(0);
            if *best_score >= DOMAIN_EXPAND_STRONG || hits >= 2 {
                matched_domains.insert(domain.clone());
                info!(
                    "[tool-index] Expanding domain '{}' (best={:.3}, hits={})",
                    domain, best_score, hits
                );
            }
        }

        // Include sibling tools from matched domains (e.g., email_read for email_send)
        for tool in &self.tools {
            if matched_domains.contains(&tool.domain)
                && result_names.insert(tool.definition.function.name.clone())
            {
                results.push(tool.definition.clone());
            }
        }

        // Also do an exact name/keyword match for tools that might not embed well
        let query_lower = query.to_lowercase();
        for tool in &self.tools {
            let name = &tool.definition.function.name;
            if (query_lower.contains(name) || name.contains(&query_lower.replace(' ', "_")))
                && result_names.insert(name.clone())
            {
                results.push(tool.definition.clone());
            }
        }

        info!(
            "[tool-index] Search '{}' → {} tools (from {} domains)",
            safe_truncate(query, 60),
            results.len(),
            matched_domains.len()
        );

        Ok(results)
    }

    /// Get all tools in a specific domain.
    pub fn get_domain_tools(&self, domain: &str) -> Vec<ToolDefinition> {
        self.tools
            .iter()
            .filter(|t| t.domain == domain)
            .map(|t| t.definition.clone())
            .collect()
    }

    /// Get all tool definitions (for building the full index).
    pub fn all_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.iter().map(|t| t.definition.clone()).collect()
    }

    /// Check if the index contains a tool by name.
    #[allow(dead_code)]
    pub fn has_tool(&self, name: &str) -> bool {
        self.tools
            .iter()
            .any(|t| t.definition.function.name == name)
    }
}

/// Cosine similarity between two vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f64;
    let mut mag_a = 0.0f64;
    let mut mag_b = 0.0f64;
    for (x, y) in a.iter().zip(b.iter()) {
        let x = *x as f64;
        let y = *y as f64;
        dot += x * y;
        mag_a += x * x;
        mag_b += y * y;
    }
    let denom = mag_a.sqrt() * mag_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── tool_domain ────────────────────────────────────────────────

    #[test]
    fn system_tool() {
        assert_eq!(tool_domain("exec"), "system");
    }

    #[test]
    fn filesystem_tools() {
        assert_eq!(tool_domain("read_file"), "filesystem");
        assert_eq!(tool_domain("write_file"), "filesystem");
        assert_eq!(tool_domain("append_file"), "filesystem");
        assert_eq!(tool_domain("delete_file"), "filesystem");
        assert_eq!(tool_domain("list_directory"), "filesystem");
    }

    #[test]
    fn web_tools() {
        assert_eq!(tool_domain("fetch"), "web");
        assert_eq!(tool_domain("web_search"), "web");
        assert_eq!(tool_domain("web_read"), "web");
        assert_eq!(tool_domain("web_screenshot"), "web");
        assert_eq!(tool_domain("web_browse"), "web");
    }

    #[test]
    fn identity_tools() {
        assert_eq!(tool_domain("soul_read"), "identity");
        assert_eq!(tool_domain("soul_write"), "identity");
        assert_eq!(tool_domain("soul_list"), "identity");
        assert_eq!(tool_domain("self_info"), "identity");
        assert_eq!(tool_domain("update_profile"), "identity");
    }

    #[test]
    fn memory_tools() {
        assert_eq!(tool_domain("memory_store"), "memory");
        assert_eq!(tool_domain("memory_search"), "memory");
    }

    #[test]
    fn agent_tools() {
        assert_eq!(tool_domain("create_agent"), "agents");
        assert_eq!(tool_domain("agent_list"), "agents");
        assert_eq!(tool_domain("agent_skills"), "agents");
        assert_eq!(tool_domain("agent_skill_assign"), "agents");
        assert_eq!(tool_domain("manage_session"), "agents");
    }

    #[test]
    fn communication_tools() {
        assert_eq!(tool_domain("agent_send_message"), "communication");
        assert_eq!(tool_domain("agent_read_messages"), "communication");
    }

    #[test]
    fn squad_tools() {
        assert_eq!(tool_domain("create_squad"), "squads");
        assert_eq!(tool_domain("list_squads"), "squads");
        assert_eq!(tool_domain("manage_squad"), "squads");
        assert_eq!(tool_domain("squad_broadcast"), "squads");
    }

    #[test]
    fn task_tools() {
        assert_eq!(tool_domain("create_task"), "tasks");
        assert_eq!(tool_domain("list_tasks"), "tasks");
        assert_eq!(tool_domain("manage_task"), "tasks");
    }

    #[test]
    fn skill_tools() {
        assert_eq!(tool_domain("skill_search"), "skills");
        assert_eq!(tool_domain("skill_install"), "skills");
        assert_eq!(tool_domain("skill_list"), "skills");
    }

    #[test]
    fn dashboard_tools() {
        assert_eq!(tool_domain("skill_output"), "dashboard");
        assert_eq!(tool_domain("delete_skill_output"), "dashboard");
    }

    #[test]
    fn storage_tools() {
        assert_eq!(tool_domain("skill_store_set"), "storage");
        assert_eq!(tool_domain("skill_store_get"), "storage");
        assert_eq!(tool_domain("skill_store_list"), "storage");
        assert_eq!(tool_domain("skill_store_delete"), "storage");
    }

    #[test]
    fn email_tools() {
        assert_eq!(tool_domain("email_send"), "email");
        assert_eq!(tool_domain("email_read"), "email");
    }

    #[test]
    fn messaging_tools() {
        assert_eq!(tool_domain("slack_send"), "messaging");
        assert_eq!(tool_domain("slack_read"), "messaging");
        assert_eq!(tool_domain("telegram_send"), "messaging");
        assert_eq!(tool_domain("telegram_read"), "messaging");
    }

    #[test]
    fn discord_prefix_tools() {
        assert_eq!(tool_domain("discord_send"), "discord");
        assert_eq!(tool_domain("discord_list_channels"), "discord");
        assert_eq!(tool_domain("discord_create_channel"), "discord");
    }

    #[test]
    fn trello_prefix_tools() {
        assert_eq!(tool_domain("trello_boards"), "trello");
        assert_eq!(tool_domain("trello_cards"), "trello");
        assert_eq!(tool_domain("trello_create_card"), "trello");
    }

    #[test]
    fn github_tool() {
        assert_eq!(tool_domain("github_api"), "github");
    }

    #[test]
    fn google_prefix_tools() {
        assert_eq!(tool_domain("google_calendar"), "google");
        assert_eq!(tool_domain("google_drive"), "google");
        assert_eq!(tool_domain("google_sheets"), "google");
    }

    #[test]
    fn integration_tools() {
        assert_eq!(tool_domain("rest_api_call"), "integrations");
        assert_eq!(tool_domain("webhook_send"), "integrations");
        assert_eq!(tool_domain("image_generate"), "integrations");
    }

    #[test]
    fn coinbase_trading_tools() {
        assert_eq!(tool_domain("coinbase_prices"), "coinbase");
        assert_eq!(tool_domain("coinbase_balance"), "coinbase");
        assert_eq!(tool_domain("coinbase_wallet_create"), "coinbase");
        assert_eq!(tool_domain("coinbase_trade"), "coinbase");
        assert_eq!(tool_domain("coinbase_transfer"), "coinbase");
    }

    #[test]
    fn dex_prefix_tools() {
        assert_eq!(tool_domain("dex_swap"), "dex");
        assert_eq!(tool_domain("dex_quote"), "dex");
    }

    #[test]
    fn solana_prefix_tools() {
        assert_eq!(tool_domain("sol_swap"), "solana");
        assert_eq!(tool_domain("sol_balance"), "solana");
    }

    #[test]
    fn mcp_prefix_tools() {
        assert_eq!(tool_domain("mcp_custom_tool"), "mcp");
        assert_eq!(tool_domain("mcp_server_action"), "mcp");
    }

    #[test]
    fn meta_tool() {
        assert_eq!(tool_domain("request_tools"), "meta");
    }

    #[test]
    fn unknown_tool_returns_other() {
        assert_eq!(tool_domain("completely_unknown"), "other");
        assert_eq!(tool_domain(""), "other");
    }

    // ── cosine_similarity ──────────────────────────────────────────

    #[test]
    fn identical_vectors() {
        let v = vec![1.0f32, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn orthogonal_vectors() {
        let a = vec![1.0f32, 0.0, 0.0];
        let b = vec![0.0f32, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn opposite_vectors() {
        let a = vec![1.0f32, 0.0];
        let b = vec![-1.0f32, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn different_length_returns_zero() {
        let a = vec![1.0f32, 2.0];
        let b = vec![1.0f32, 2.0, 3.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn empty_vectors_return_zero() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn zero_magnitude_returns_zero() {
        let a = vec![0.0f32, 0.0, 0.0];
        let b = vec![1.0f32, 2.0, 3.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn similar_vectors_high_similarity() {
        let a = vec![1.0f32, 2.0, 3.0];
        let b = vec![1.1f32, 2.1, 3.1];
        let sim = cosine_similarity(&a, &b);
        assert!(sim > 0.99);
    }

    // ── CORE_TOOLS ─────────────────────────────────────────────────

    #[test]
    fn core_tools_contains_essentials() {
        assert!(CORE_TOOLS.contains(&"memory_store"));
        assert!(CORE_TOOLS.contains(&"memory_search"));
        assert!(CORE_TOOLS.contains(&"read_file"));
        assert!(CORE_TOOLS.contains(&"write_file"));
        assert!(CORE_TOOLS.contains(&"request_tools"));
    }

    #[test]
    fn core_tools_count() {
        assert_eq!(CORE_TOOLS.len(), 10);
    }

    // ── domain_summaries ───────────────────────────────────────────

    #[test]
    fn domain_summaries_not_empty() {
        let summaries = domain_summaries();
        assert!(summaries.len() >= 20);
    }

    #[test]
    fn domain_summaries_have_unique_ids() {
        let summaries = domain_summaries();
        let ids: Vec<_> = summaries.iter().map(|(id, _, _)| *id).collect();
        let unique: HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len(), "domain IDs should be unique");
    }

    #[test]
    fn domain_summaries_include_key_domains() {
        let summaries = domain_summaries();
        let ids: Vec<_> = summaries.iter().map(|(id, _, _)| *id).collect();
        assert!(ids.contains(&"system"));
        assert!(ids.contains(&"filesystem"));
        assert!(ids.contains(&"web"));
        assert!(ids.contains(&"email"));
        assert!(ids.contains(&"tasks"));
        assert!(ids.contains(&"discord"));
        assert!(ids.contains(&"trello"));
    }

    // ── ToolIndex::new ─────────────────────────────────────────────

    #[test]
    fn new_index_not_ready() {
        let idx = ToolIndex::new();
        assert!(!idx.is_ready());
    }

    #[test]
    fn new_index_empty_definitions() {
        let idx = ToolIndex::new();
        assert!(idx.all_definitions().is_empty());
    }

    #[test]
    fn new_index_has_tool_returns_false() {
        let idx = ToolIndex::new();
        assert!(!idx.has_tool("read_file"));
    }

    #[test]
    fn default_is_same_as_new() {
        let idx = ToolIndex::default();
        assert!(!idx.is_ready());
        assert!(idx.all_definitions().is_empty());
    }

    #[test]
    fn get_domain_tools_empty_index() {
        let idx = ToolIndex::new();
        assert!(idx.get_domain_tools("filesystem").is_empty());
    }
}
