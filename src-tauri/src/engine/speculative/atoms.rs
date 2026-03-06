// ─────────────────────────────────────────────────────────────────────────────
// Speculative Tool Execution — Atoms
//
// Pure types, constants, and deterministic functions for speculative execution.
// No side effects. No I/O. No async. No state.
//
// Speculative execution predicts the next tool call based on historical
// transition patterns and optionally pre-fires read-only operations while
// the model is still generating.  This is the agent equivalent of CPU branch
// prediction — execute the likely next step speculatively, discard on mismatch.
//
// Safety invariant: only READ operations are ever speculated.  Writes, creates,
// deletes, and mutations are never pre-fired.  This eliminates rollback
// complexity entirely.
// ─────────────────────────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Constants ──────────────────────────────────────────────────────────────

/// Default speculation threshold: only speculate when P(next|current) ≥ 0.5.
pub const DEFAULT_SPECULATION_THRESHOLD: f64 = 0.5;

/// Cache time-to-live in milliseconds (30 seconds).
/// Speculative results older than this are discarded.
pub const CACHE_TTL_MS: u64 = 30_000;

/// Maximum number of cached speculative results.
pub const MAX_CACHE_ENTRIES: usize = 64;

/// Maximum transition records stored in the database.
/// Older entries are pruned when this limit is exceeded.
pub const MAX_TRANSITIONS_STORED: usize = 10_000;

/// Connection warming timeout in milliseconds.
pub const WARM_TIMEOUT_MS: u64 = 5_000;

/// Default number of top candidates to consider.
pub const DEFAULT_TOP_K: usize = 3;

/// Minimum observations before trusting a transition probability.
/// Prevents speculation on one-off coincidences.
pub const MIN_OBSERVATION_COUNT: u64 = 3;

// ── Types ──────────────────────────────────────────────────────────────────

/// Whether a tool performs read-only or write/mutating operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolMutability {
    /// Tool only reads data — safe for speculative execution.
    ReadOnly,
    /// Tool writes, creates, deletes, or mutates — never speculate.
    Write,
}

/// A single transition record (from one tool to another with a count).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolTransition {
    pub from_tool: String,
    pub to_tool: String,
    pub count: u64,
    pub last_seen: i64,
}

/// Transition probability matrix: `from_tool → { to_tool → count }`.
pub type TransitionMatrix = HashMap<String, HashMap<String, u64>>;

/// A candidate for speculative execution.
#[derive(Debug, Clone)]
pub struct SpeculationCandidate {
    /// The tool predicted to be called next.
    pub tool_name: String,
    /// Probability based on historical transitions.
    pub probability: f64,
}

/// Outcome of a speculative execution attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpeculationOutcome {
    /// Prediction matched the actual tool call — result was used.
    Hit,
    /// Prediction did not match — result was discarded.
    Miss,
    /// Speculation was cancelled before the result could be used.
    Cancelled,
    /// Speculative result expired (TTL exceeded) before use.
    Expired,
}

/// Result of a speculative tool execution, stored in the cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeculativeResult {
    /// Tool that was speculatively executed.
    pub tool_name: String,
    /// Hash of the arguments used.
    pub args_hash: u64,
    /// The tool's output string.
    pub output: String,
    /// Whether the speculative execution succeeded.
    pub success: bool,
    /// Time-to-live in milliseconds.
    pub ttl_ms: u64,
}

/// A cache entry holding a speculative result with timing metadata.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// Tool name (lookup key, part 1).
    pub tool_name: String,
    /// Hash of the arguments (lookup key, part 2).
    pub args_hash: u64,
    /// The cached speculative result.
    pub result: SpeculativeResult,
    /// When this entry was created (epoch milliseconds).
    pub created_at_ms: u64,
}

/// Target for TCP/TLS connection pre-warming.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmTarget {
    /// Hostname to connect to.
    pub host: String,
    /// Port number (typically 443 for HTTPS).
    pub port: u16,
    /// Whether TLS should be used.
    pub tls: bool,
    /// Connection timeout in milliseconds.
    pub timeout_ms: u64,
}

/// Configuration for the speculation engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeculationConfig {
    /// Minimum transition probability to trigger speculation.
    pub threshold: f64,
    /// Maximum number of candidates to evaluate per prediction.
    pub top_k: usize,
    /// Cache TTL in milliseconds.
    pub cache_ttl_ms: u64,
    /// Maximum number of cache entries.
    pub max_cache_entries: usize,
    /// Minimum observation count before trusting a transition.
    pub min_observations: u64,
    /// Whether speculation is enabled at all.
    pub enabled: bool,
    /// Whether connection warming is enabled.
    pub warm_connections: bool,
}

/// Statistics about cache performance.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStats {
    /// Current number of entries in the cache.
    pub entries: usize,
    /// Total cache hits.
    pub hits: u64,
    /// Total cache misses.
    pub misses: u64,
    /// Total entries evicted (expired or capacity overflow).
    pub evictions: u64,
}

/// Overall speculation statistics for a session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SpeculationStats {
    /// Total predictions made.
    pub predictions: u64,
    /// Predictions that matched the actual tool call.
    pub hits: u64,
    /// Predictions that did not match.
    pub misses: u64,
    /// Predictions cancelled before resolution.
    pub cancellations: u64,
    /// Speculative results that expired before use.
    pub expirations: u64,
    /// Connections pre-warmed.
    pub connections_warmed: u64,
}

// ── Implementations ────────────────────────────────────────────────────────

impl Default for SpeculationConfig {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_SPECULATION_THRESHOLD,
            top_k: DEFAULT_TOP_K,
            cache_ttl_ms: CACHE_TTL_MS,
            max_cache_entries: MAX_CACHE_ENTRIES,
            min_observations: MIN_OBSERVATION_COUNT,
            enabled: true,
            warm_connections: true,
        }
    }
}

impl WarmTarget {
    /// Create a new HTTPS warm target with default timeout.
    pub fn https(host: &str, port: u16) -> Self {
        Self {
            host: host.to_string(),
            port,
            tls: true,
            timeout_ms: WARM_TIMEOUT_MS,
        }
    }
}

// ── Pure Functions ─────────────────────────────────────────────────────────

/// Classify a tool as read-only or write based on its name.
///
/// Conservative: unknown tools default to `Write` (never speculate on unknowns).
/// Only well-known read patterns are classified as `ReadOnly`.
pub fn classify_tool_mutability(tool_name: &str) -> ToolMutability {
    // ── Explicit read-only tools ───────────────────────────────────────
    match tool_name {
        // Filesystem reads
        "read_file" | "list_directory" => return ToolMutability::ReadOnly,

        // Web reads
        "fetch" | "web_search" | "web_read" | "web_screenshot" | "web_browse" => {
            return ToolMutability::ReadOnly;
        }

        // Identity reads
        "soul_read" | "soul_list" | "self_info" => return ToolMutability::ReadOnly,

        // Memory reads
        "memory_search" => return ToolMutability::ReadOnly,

        // Agent reads
        "agent_list" | "agent_skills" | "agent_read_messages" => {
            return ToolMutability::ReadOnly;
        }

        // Squad reads
        "list_squads" => return ToolMutability::ReadOnly,

        // Task reads
        "list_tasks" => return ToolMutability::ReadOnly,

        // Skill reads
        "skill_list" | "skill_search" => return ToolMutability::ReadOnly,

        // Canvas reads
        "canvas_list_dashboards" | "canvas_list_templates" | "canvas_load" => {
            return ToolMutability::ReadOnly;
        }

        // Dashboard reads
        "skill_output" => return ToolMutability::ReadOnly,

        // Storage reads
        "skill_store_get" | "skill_store_list" => return ToolMutability::ReadOnly,

        // Email reads
        "email_read" => return ToolMutability::ReadOnly,

        // Messaging reads
        "slack_read" | "telegram_read" => return ToolMutability::ReadOnly,

        // Trading reads (view-only)
        "coinbase_prices" | "coinbase_balance" => return ToolMutability::ReadOnly,

        // Tool RAG meta-tool
        "request_tools" => return ToolMutability::ReadOnly,

        _ => {}
    }

    // ── Pattern-based read detection for prefixed tools ────────────────

    // Google Workspace / Gmail reads
    if (tool_name.starts_with("google_") || tool_name.starts_with("gmail_"))
        && (tool_name.contains("_list")
            || tool_name.contains("_get")
            || tool_name.contains("_search")
            || tool_name.contains("_read"))
    {
        return ToolMutability::ReadOnly;
    }

    // Discord reads
    if tool_name.starts_with("discord_")
        && (tool_name.contains("_list") || tool_name.contains("_get"))
    {
        return ToolMutability::ReadOnly;
    }

    // Discourse reads
    if tool_name.starts_with("discourse_")
        && (tool_name.contains("_list")
            || tool_name.contains("_get")
            || tool_name.contains("_search"))
    {
        return ToolMutability::ReadOnly;
    }

    // Trello reads
    if tool_name.starts_with("trello_")
        && (tool_name.contains("_list") || tool_name.contains("_get"))
    {
        return ToolMutability::ReadOnly;
    }

    // MCP tools — conservative: only list/get/search/read are safe
    if tool_name.starts_with("mcp_")
        && (tool_name.contains("_list")
            || tool_name.contains("_get")
            || tool_name.contains("_search")
            || tool_name.contains("_read"))
    {
        return ToolMutability::ReadOnly;
    }

    // DEX reads
    if tool_name.starts_with("dex_")
        && (tool_name.contains("_price")
            || tool_name.contains("_balance")
            || tool_name.contains("_list"))
    {
        return ToolMutability::ReadOnly;
    }

    // Solana reads
    if tool_name.starts_with("sol_")
        && (tool_name.contains("_balance")
            || tool_name.contains("_price")
            || tool_name.contains("_list")
            || tool_name.contains("_get"))
    {
        return ToolMutability::ReadOnly;
    }

    // Default: Write (conservative — never speculate on unknown tools)
    ToolMutability::Write
}

/// Convenience: check if a tool is read-only (safe for speculation).
pub fn is_read_only_tool(tool_name: &str) -> bool {
    classify_tool_mutability(tool_name) == ToolMutability::ReadOnly
}

/// Convert raw transition counts to normalized probabilities.
///
/// Returns a list of `(tool_name, probability)` sorted descending by probability.
pub fn compute_transition_probabilities(counts: &HashMap<String, u64>) -> Vec<(String, f64)> {
    let total: u64 = counts.values().sum();
    if total == 0 {
        return Vec::new();
    }
    let mut probs: Vec<(String, f64)> = counts
        .iter()
        .map(|(tool, &count)| (tool.clone(), count as f64 / total as f64))
        .collect();
    probs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    probs
}

/// Predict the next most likely read-only tools based on the transition matrix.
///
/// Only returns candidates that are:
/// 1. Above the threshold probability
/// 2. Classified as read-only (safe for speculation)
/// 3. Backed by enough observations (`min_observations`)
pub fn predict_next_tools(
    current_tool: &str,
    matrix: &TransitionMatrix,
    config: &SpeculationConfig,
) -> Vec<SpeculationCandidate> {
    let Some(successors) = matrix.get(current_tool) else {
        return Vec::new();
    };

    // Require minimum observation count before trusting probabilities
    let total: u64 = successors.values().sum();
    if total < config.min_observations {
        return Vec::new();
    }

    let probs = compute_transition_probabilities(successors);
    probs
        .into_iter()
        .filter(|(tool, p)| *p >= config.threshold && is_read_only_tool(tool))
        .take(config.top_k)
        .map(|(tool, probability)| SpeculationCandidate {
            tool_name: tool,
            probability,
        })
        .collect()
}

/// Check if a speculation candidate meets all configuration requirements.
pub fn should_speculate(candidate: &SpeculationCandidate, config: &SpeculationConfig) -> bool {
    config.enabled
        && candidate.probability >= config.threshold
        && is_read_only_tool(&candidate.tool_name)
}

/// Check if a cache entry is still valid (not expired).
pub fn is_cache_valid(entry: &CacheEntry, now_ms: u64) -> bool {
    now_ms.saturating_sub(entry.created_at_ms) < entry.result.ttl_ms
}

/// Increment or insert a transition count in the in-memory matrix.
pub fn merge_transition_count(matrix: &mut TransitionMatrix, from: &str, to: &str) {
    let count = matrix
        .entry(from.to_string())
        .or_default()
        .entry(to.to_string())
        .or_insert(0);
    *count += 1;
}

/// Map a tool domain to the API host for connection pre-warming.
///
/// Returns `None` for local-only domains (filesystem, memory, identity, etc.)
/// where there is no remote endpoint to warm.
pub fn warm_target_for_domain(domain: &str) -> Option<WarmTarget> {
    match domain {
        "google" => Some(WarmTarget::https("www.googleapis.com", 443)),
        "email" => Some(WarmTarget::https("gmail.googleapis.com", 443)),
        "discord" => Some(WarmTarget::https("discord.com", 443)),
        "slack" => Some(WarmTarget::https("slack.com", 443)),
        "telegram" => Some(WarmTarget::https("api.telegram.org", 443)),
        "github" => Some(WarmTarget::https("api.github.com", 443)),
        "coinbase" => Some(WarmTarget::https("api.coinbase.com", 443)),
        "trello" => Some(WarmTarget::https("api.trello.com", 443)),
        "web" => Some(WarmTarget::https("www.google.com", 443)),
        // Discourse: site-dependent, cannot pre-warm generically
        // Local-only: filesystem, memory, identity, system, agents, canvas, etc.
        _ => None,
    }
}

/// Hash tool arguments deterministically for cache key generation.
///
/// The hash is based on the canonical JSON serialization of the arguments,
/// so structurally identical arguments always produce the same hash.
pub fn hash_tool_args(args: &serde_json::Value) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let canonical = serde_json::to_string(args).unwrap_or_default();
    let mut hasher = DefaultHasher::new();
    canonical.hash(&mut hasher);
    hasher.finish()
}

/// Compute the overall hit rate from speculation statistics.
pub fn compute_hit_rate(stats: &SpeculationStats) -> f64 {
    let total = stats.hits + stats.misses;
    if total == 0 {
        return 0.0;
    }
    stats.hits as f64 / total as f64
}

/// Determine the outcome of a speculation given predicted vs actual tool.
pub fn resolve_outcome(
    predicted: &str,
    actual: &str,
    expired: bool,
    cancelled: bool,
) -> SpeculationOutcome {
    if cancelled {
        SpeculationOutcome::Cancelled
    } else if expired {
        SpeculationOutcome::Expired
    } else if predicted == actual {
        SpeculationOutcome::Hit
    } else {
        SpeculationOutcome::Miss
    }
}

/// Format a human-readable summary of speculation performance.
pub fn format_speculation_summary(stats: &SpeculationStats) -> String {
    let hit_rate = compute_hit_rate(stats);
    format!(
        "Speculation: {} predictions, {} hits ({:.1}%), {} misses, \
         {} cancelled, {} expired, {} connections warmed",
        stats.predictions,
        stats.hits,
        hit_rate * 100.0,
        stats.misses,
        stats.cancellations,
        stats.expirations,
        stats.connections_warmed,
    )
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── Tool mutability classification ─────────────────────────────────

    #[test]
    fn read_tools_classified_readonly() {
        let reads = [
            "read_file",
            "list_directory",
            "fetch",
            "web_search",
            "web_read",
            "web_screenshot",
            "web_browse",
            "soul_read",
            "soul_list",
            "self_info",
            "memory_search",
            "agent_list",
            "agent_skills",
            "agent_read_messages",
            "list_squads",
            "list_tasks",
            "skill_list",
            "skill_search",
            "canvas_list_dashboards",
            "canvas_list_templates",
            "canvas_load",
            "skill_output",
            "skill_store_get",
            "skill_store_list",
            "email_read",
            "slack_read",
            "telegram_read",
            "coinbase_prices",
            "coinbase_balance",
            "request_tools",
        ];
        for tool in &reads {
            assert_eq!(
                classify_tool_mutability(tool),
                ToolMutability::ReadOnly,
                "{tool} should be ReadOnly"
            );
        }
    }

    #[test]
    fn write_tools_classified_write() {
        let writes = [
            "write_file",
            "append_file",
            "delete_file",
            "exec",
            "soul_write",
            "memory_store",
            "email_send",
            "slack_send",
            "telegram_send",
            "create_agent",
            "create_task",
            "manage_task",
            "canvas_push",
            "canvas_clear",
            "canvas_remove",
            "coinbase_trade",
            "coinbase_transfer",
        ];
        for tool in &writes {
            assert_eq!(
                classify_tool_mutability(tool),
                ToolMutability::Write,
                "{tool} should be Write"
            );
        }
    }

    #[test]
    fn unknown_tools_default_to_write() {
        assert_eq!(
            classify_tool_mutability("unknown_mystery_tool"),
            ToolMutability::Write
        );
        assert_eq!(
            classify_tool_mutability("do_something"),
            ToolMutability::Write
        );
    }

    #[test]
    fn google_pattern_read_classification() {
        assert!(is_read_only_tool("google_calendar_list"));
        assert!(is_read_only_tool("google_calendar_get"));
        assert!(is_read_only_tool("gmail_search"));
        assert!(is_read_only_tool("gmail_read"));
        assert!(!is_read_only_tool("google_calendar_create"));
        assert!(!is_read_only_tool("gmail_send"));
    }

    #[test]
    fn discord_pattern_read_classification() {
        assert!(is_read_only_tool("discord_list_channels"));
        assert!(is_read_only_tool("discord_get_message"));
        assert!(!is_read_only_tool("discord_send_message"));
        assert!(!is_read_only_tool("discord_create_channel"));
    }

    #[test]
    fn mcp_pattern_read_classification() {
        assert!(is_read_only_tool("mcp_list_items"));
        assert!(is_read_only_tool("mcp_get_record"));
        assert!(is_read_only_tool("mcp_search_docs"));
        assert!(!is_read_only_tool("mcp_create_item"));
        assert!(!is_read_only_tool("mcp_execute_action"));
    }

    #[test]
    fn trello_pattern_read_classification() {
        assert!(is_read_only_tool("trello_list_boards"));
        assert!(is_read_only_tool("trello_get_card"));
        assert!(!is_read_only_tool("trello_create_card"));
        assert!(!is_read_only_tool("trello_delete_card"));
    }

    #[test]
    fn dex_and_sol_reads() {
        assert!(is_read_only_tool("dex_price_check"));
        assert!(is_read_only_tool("dex_balance_of"));
        assert!(is_read_only_tool("sol_get_balance"));
        assert!(is_read_only_tool("sol_list_tokens"));
        assert!(!is_read_only_tool("dex_swap"));
        assert!(!is_read_only_tool("sol_transfer"));
    }

    // ── Transition probabilities ───────────────────────────────────────

    #[test]
    fn compute_probabilities_basic() {
        let mut counts = HashMap::new();
        counts.insert("tool_a".into(), 3);
        counts.insert("tool_b".into(), 7);
        let probs = compute_transition_probabilities(&counts);
        assert_eq!(probs.len(), 2);
        assert_eq!(probs[0].0, "tool_b"); // highest first
        assert!((probs[0].1 - 0.7).abs() < 0.001);
        assert_eq!(probs[1].0, "tool_a");
        assert!((probs[1].1 - 0.3).abs() < 0.001);
    }

    #[test]
    fn compute_probabilities_empty() {
        let counts: HashMap<String, u64> = HashMap::new();
        let probs = compute_transition_probabilities(&counts);
        assert!(probs.is_empty());
    }

    #[test]
    fn compute_probabilities_single() {
        let mut counts = HashMap::new();
        counts.insert("only_tool".into(), 5);
        let probs = compute_transition_probabilities(&counts);
        assert_eq!(probs.len(), 1);
        assert!((probs[0].1 - 1.0).abs() < 0.001);
    }

    // ── Prediction ─────────────────────────────────────────────────────

    #[test]
    fn predict_next_tools_above_threshold() {
        let mut matrix: TransitionMatrix = HashMap::new();
        let mut successors = HashMap::new();
        // memory_search is read-only, should be returned
        successors.insert("memory_search".into(), 8);
        // email_send is Write, should be filtered out
        successors.insert("email_send".into(), 2);
        matrix.insert("google_calendar_list".into(), successors);

        let config = SpeculationConfig::default();
        let candidates = predict_next_tools("google_calendar_list", &matrix, &config);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].tool_name, "memory_search");
        assert!((candidates[0].probability - 0.8).abs() < 0.001);
    }

    #[test]
    fn predict_next_tools_below_threshold() {
        let mut matrix: TransitionMatrix = HashMap::new();
        let mut successors = HashMap::new();
        // Even distribution — all below 0.5 threshold
        successors.insert("memory_search".into(), 2);
        successors.insert("email_read".into(), 2);
        successors.insert("list_tasks".into(), 2);
        successors.insert("read_file".into(), 1);
        matrix.insert("soul_read".into(), successors);

        let config = SpeculationConfig::default();
        let candidates = predict_next_tools("soul_read", &matrix, &config);
        assert!(candidates.is_empty());
    }

    #[test]
    fn predict_next_tools_unknown_current() {
        let matrix: TransitionMatrix = HashMap::new();
        let config = SpeculationConfig::default();
        let candidates = predict_next_tools("nonexistent", &matrix, &config);
        assert!(candidates.is_empty());
    }

    #[test]
    fn predict_respects_min_observations() {
        let mut matrix: TransitionMatrix = HashMap::new();
        let mut successors = HashMap::new();
        // Only 2 observations, below MIN_OBSERVATION_COUNT (3)
        successors.insert("memory_search".into(), 2);
        matrix.insert("soul_read".into(), successors);

        let config = SpeculationConfig::default(); // min_observations = 3
        let candidates = predict_next_tools("soul_read", &matrix, &config);
        assert!(candidates.is_empty());
    }

    #[test]
    fn predict_respects_top_k() {
        let mut matrix: TransitionMatrix = HashMap::new();
        let mut successors = HashMap::new();
        successors.insert("read_file".into(), 100);
        successors.insert("memory_search".into(), 90);
        successors.insert("list_tasks".into(), 80);
        successors.insert("email_read".into(), 70);
        successors.insert("soul_read".into(), 60);
        matrix.insert("self_info".into(), successors);

        let config = SpeculationConfig {
            top_k: 2,
            threshold: 0.1,
            min_observations: 1,
            ..SpeculationConfig::default()
        };
        let candidates = predict_next_tools("self_info", &matrix, &config);
        assert_eq!(candidates.len(), 2);
    }

    // ── should_speculate ───────────────────────────────────────────────

    #[test]
    fn should_speculate_checks_all_conditions() {
        let candidate = SpeculationCandidate {
            tool_name: "memory_search".into(),
            probability: 0.7,
        };
        let config = SpeculationConfig::default();
        assert!(should_speculate(&candidate, &config));

        // Disabled config
        let disabled = SpeculationConfig {
            enabled: false,
            ..SpeculationConfig::default()
        };
        assert!(!should_speculate(&candidate, &disabled));

        // Write tool
        let write_candidate = SpeculationCandidate {
            tool_name: "email_send".into(),
            probability: 0.9,
        };
        assert!(!should_speculate(&write_candidate, &config));

        // Below threshold
        let low_prob = SpeculationCandidate {
            tool_name: "memory_search".into(),
            probability: 0.3,
        };
        assert!(!should_speculate(&low_prob, &config));
    }

    // ── Cache validity ─────────────────────────────────────────────────

    #[test]
    fn cache_validity_fresh_vs_expired() {
        let entry = CacheEntry {
            tool_name: "memory_search".into(),
            args_hash: 12345,
            result: SpeculativeResult {
                tool_name: "memory_search".into(),
                args_hash: 12345,
                output: "found it".into(),
                success: true,
                ttl_ms: CACHE_TTL_MS,
            },
            created_at_ms: 1000,
        };
        // 10 seconds later — still valid
        assert!(is_cache_valid(&entry, 11_000));
        // 31 seconds later — expired (30s TTL)
        assert!(!is_cache_valid(&entry, 32_000));
        // Exactly at TTL boundary — expired
        assert!(!is_cache_valid(&entry, 31_000));
    }

    // ── Transition matrix merge ────────────────────────────────────────

    #[test]
    fn merge_transition_count_new_and_existing() {
        let mut matrix: TransitionMatrix = HashMap::new();

        merge_transition_count(&mut matrix, "a", "b");
        assert_eq!(matrix["a"]["b"], 1);

        merge_transition_count(&mut matrix, "a", "b");
        assert_eq!(matrix["a"]["b"], 2);

        merge_transition_count(&mut matrix, "a", "c");
        assert_eq!(matrix["a"]["c"], 1);
        assert_eq!(matrix["a"]["b"], 2); // unchanged
    }

    // ── Warm targets ───────────────────────────────────────────────────

    #[test]
    fn warm_target_known_domains() {
        assert!(warm_target_for_domain("google").is_some());
        assert!(warm_target_for_domain("email").is_some());
        assert!(warm_target_for_domain("discord").is_some());
        assert!(warm_target_for_domain("slack").is_some());
        assert!(warm_target_for_domain("telegram").is_some());
        assert!(warm_target_for_domain("github").is_some());
        assert!(warm_target_for_domain("coinbase").is_some());
        assert!(warm_target_for_domain("trello").is_some());
        assert!(warm_target_for_domain("web").is_some());

        let google = warm_target_for_domain("google").unwrap();
        assert_eq!(google.host, "www.googleapis.com");
        assert_eq!(google.port, 443);
        assert!(google.tls);
    }

    #[test]
    fn warm_target_local_domains_return_none() {
        assert!(warm_target_for_domain("filesystem").is_none());
        assert!(warm_target_for_domain("memory").is_none());
        assert!(warm_target_for_domain("identity").is_none());
        assert!(warm_target_for_domain("system").is_none());
        assert!(warm_target_for_domain("agents").is_none());
        assert!(warm_target_for_domain("canvas").is_none());
        assert!(warm_target_for_domain("other").is_none());
    }

    #[test]
    fn warm_target_https_constructor() {
        let target = WarmTarget::https("example.com", 8443);
        assert_eq!(target.host, "example.com");
        assert_eq!(target.port, 8443);
        assert!(target.tls);
        assert_eq!(target.timeout_ms, WARM_TIMEOUT_MS);
    }

    // ── Argument hashing ───────────────────────────────────────────────

    #[test]
    fn hash_tool_args_deterministic() {
        let args = json!({"query": "test", "limit": 10});
        let h1 = hash_tool_args(&args);
        let h2 = hash_tool_args(&args);
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_tool_args_different_for_different_args() {
        let args1 = json!({"query": "test"});
        let args2 = json!({"query": "other"});
        assert_ne!(hash_tool_args(&args1), hash_tool_args(&args2));
    }

    // ── Statistics ─────────────────────────────────────────────────────

    #[test]
    fn hit_rate_computation() {
        let stats = SpeculationStats {
            predictions: 10,
            hits: 7,
            misses: 3,
            ..Default::default()
        };
        assert!((compute_hit_rate(&stats) - 0.7).abs() < 0.001);
    }

    #[test]
    fn hit_rate_zero_predictions() {
        let stats = SpeculationStats::default();
        assert_eq!(compute_hit_rate(&stats), 0.0);
    }

    // ── Outcome resolution ─────────────────────────────────────────────

    #[test]
    fn resolve_outcome_variants() {
        assert_eq!(
            resolve_outcome("a", "a", false, false),
            SpeculationOutcome::Hit
        );
        assert_eq!(
            resolve_outcome("a", "b", false, false),
            SpeculationOutcome::Miss
        );
        assert_eq!(
            resolve_outcome("a", "b", false, true),
            SpeculationOutcome::Cancelled
        );
        assert_eq!(
            resolve_outcome("a", "a", true, false),
            SpeculationOutcome::Expired
        );
        // Cancelled takes precedence over expired
        assert_eq!(
            resolve_outcome("a", "a", true, true),
            SpeculationOutcome::Cancelled
        );
    }

    // ── Summary formatting ─────────────────────────────────────────────

    #[test]
    fn format_summary_output() {
        let stats = SpeculationStats {
            predictions: 100,
            hits: 65,
            misses: 35,
            cancellations: 5,
            expirations: 2,
            connections_warmed: 42,
        };
        let summary = format_speculation_summary(&stats);
        assert!(summary.contains("100 predictions"));
        assert!(summary.contains("65 hits"));
        assert!(summary.contains("65.0%"));
        assert!(summary.contains("35 misses"));
        assert!(summary.contains("5 cancelled"));
        assert!(summary.contains("2 expired"));
        assert!(summary.contains("42 connections warmed"));
    }

    // ── Default config ─────────────────────────────────────────────────

    #[test]
    fn default_config_sane() {
        let config = SpeculationConfig::default();
        assert_eq!(config.threshold, 0.5);
        assert_eq!(config.top_k, 3);
        assert_eq!(config.cache_ttl_ms, 30_000);
        assert_eq!(config.max_cache_entries, 64);
        assert_eq!(config.min_observations, 3);
        assert!(config.enabled);
        assert!(config.warm_connections);
    }
}
