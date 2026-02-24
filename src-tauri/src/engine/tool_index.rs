// Paw Agent Engine — Tool RAG Index (Intent-Stated Retrieval)
//
// The "Librarian" pattern: instead of dumping 75+ tool definitions into every
// LLM request (~7,500 tokens), we embed tool descriptions and let the agent
// discover tools on demand via semantic search.
//
// Architecture:
//   PATRON  (Cloud LLM)  — sees core tools + `request_tools` meta-tool
//   LIBRARIAN (Ollama)    — embeds queries, searches tool index, returns schemas
//   LIBRARY  (ToolIndex)  — holds tool definitions + their embedding vectors
//
// On startup, all tool definitions are embedded once (~75 vectors × 768 dims).
// When the agent calls `request_tools("send email to john")`, we embed the
// query, compute cosine similarity, and return the top-K matching tools.
// Those tools get injected into the next agent loop round.
//
// Token savings: ~5,000-8,500 tokens per request (25% of a 32K context window).

use crate::atoms::types::ToolDefinition;
use crate::engine::memory::EmbeddingClient;
use crate::atoms::error::EngineResult;
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
fn tool_domain(name: &str) -> &'static str {
    match name {
        // System & Files
        "exec" => "system",
        "read_file" | "write_file" | "append_file" | "delete_file" | "list_directory" => "filesystem",

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

        // Dashboard & Storage
        "skill_output" | "delete_skill_output" => "dashboard",
        "skill_store_set" | "skill_store_get" | "skill_store_list" | "skill_store_delete" => "storage",

        // Email
        "email_send" | "email_read" => "email",

        // Messaging
        "slack_send" | "slack_read" => "messaging",
        "telegram_send" | "telegram_read" => "messaging",

        // Discord
        "discord_setup_channels" | "discord_list_channels" | "discord_send_message" | "discord_delete_channels" => "discord",

        // GitHub
        "github_api" => "github",

        // Google Workspace
        n if n.starts_with("google_") => "google",

        // Integrations
        "rest_api_call" | "webhook_send" | "image_generate" => "integrations",

        // Trading — Coinbase
        "coinbase_prices" | "coinbase_balance" | "coinbase_wallet_create"
        | "coinbase_trade" | "coinbase_transfer" => "coinbase",

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
        ("system",        "terminal",     "Execute shell commands"),
        ("filesystem",    "folder",       "Read, write, delete, list files in your workspace"),
        ("web",           "language",     "Search the web, browse pages, take screenshots, fetch URLs"),
        ("identity",      "person",       "Read/update your soul files and profile"),
        ("memory",        "psychology",   "Store and search long-term memories"),
        ("agents",        "group",        "Create and manage AI agents, assign skills"),
        ("communication", "chat",         "Send/read messages between agents"),
        ("squads",        "groups",       "Create agent teams, broadcast to squad members"),
        ("tasks",         "task_alt",     "Create tasks, manage automations, set cron schedules"),
        ("skills",        "extension",    "Search, install, and list community skills"),
        ("dashboard",     "dashboard",    "Push data to the Today dashboard widgets"),
        ("storage",       "storage",      "Persistent key-value storage for extensions"),
        ("email",         "mail",         "Send and read emails via IMAP/SMTP"),
        ("google",        "mail",         "Google Workspace — Gmail, Calendar, Drive, Sheets, Docs"),
        ("messaging",     "forum",        "Slack and Telegram messaging"),
        ("discord",       "forum",        "Discord server management — list, create, and organize channels; send messages"),
        ("github",        "code",         "GitHub API calls (issues, PRs, repos)"),
        ("integrations",  "api",          "REST APIs, webhooks, image generation"),
        ("coinbase",      "trending_up",  "Coinbase exchange — prices, balances, trades, transfers"),
        ("dex",           "trending_up",  "DEX/Uniswap — swaps, quotes, whale tracking, trending tokens"),
        ("solana",        "trending_up",  "Solana/Jupiter — swaps, quotes, token info, portfolio"),
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
        info!("[tool-index] Building tool index for {} definitions...", all_tools.len());
        self.tools.clear();

        let mut success = 0;
        let mut failed = 0;

        for tool in all_tools {
            let text = format!("{}: {}", tool.function.name, tool.function.description);
            match client.embed(&text).await {
                Ok(embedding) => {
                    self.tools.push(IndexedTool {
                        definition: tool.clone(),
                        embedding,
                        domain: tool_domain(&tool.function.name).to_string(),
                    });
                    success += 1;
                }
                Err(e) => {
                    warn!("[tool-index] Failed to embed tool '{}': {}", tool.function.name, e);
                    // Still add the tool without embedding — it can be found by name or domain
                    self.tools.push(IndexedTool {
                        definition: tool.clone(),
                        embedding: Vec::new(),
                        domain: tool_domain(&tool.function.name).to_string(),
                    });
                    failed += 1;
                }
            }
        }

        self.ready = true;
        info!(
            "[tool-index] Index built: {} tools ({} embedded, {} unembedded)",
            self.tools.len(), success, failed
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

        // Embed the query
        let query_vec = client.embed(query).await?;

        // Score every tool by cosine similarity
        let mut scored: Vec<(usize, f64)> = self.tools.iter().enumerate()
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
                if *score > *best { *best = *score; }
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
            &query[..query.len().min(60)],
            results.len(),
            matched_domains.len()
        );

        Ok(results)
    }

    /// Get all tools in a specific domain.
    pub fn get_domain_tools(&self, domain: &str) -> Vec<ToolDefinition> {
        self.tools.iter()
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
        self.tools.iter().any(|t| t.definition.function.name == name)
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
    if denom == 0.0 { 0.0 } else { dot / denom }
}
