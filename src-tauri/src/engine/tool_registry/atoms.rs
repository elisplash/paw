// ─────────────────────────────────────────────────────────────────────────────
// Embedding-Indexed Tool Registry — Atoms
//
// Pure types, constants, and deterministic functions for the persistent
// tool registry. No I/O, no network calls, no SQLite.
//
// The registry makes Tool RAG persistent, hierarchical, and auto-updating:
//   1. Embeddings stored in SQLite (survive restarts)
//   2. Incremental indexing (only new/changed tools re-embedded)
//   3. Hierarchical search (domain → tool)
//   4. Four-tier failover: Ollama → Cloud API → BM25 → Domain keyword
// ─────────────────────────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};

// ── Types ──────────────────────────────────────────────────────────────────

/// Source of a tool — determines priority during deduplication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolSource {
    /// Built-in tool (fastest, no external dependency)
    Builtin,
    /// MCP tool from n8n or custom MCP server
    Mcp,
    /// Community skill package
    Community,
}

impl ToolSource {
    /// Convert to database string.
    pub fn as_str(&self) -> &'static str {
        match self {
            ToolSource::Builtin => "builtin",
            ToolSource::Mcp => "mcp",
            ToolSource::Community => "community",
        }
    }

    /// Parse from database string.
    pub fn parse(s: &str) -> Self {
        match s {
            "builtin" => ToolSource::Builtin,
            "mcp" => ToolSource::Mcp,
            "community" => ToolSource::Community,
            _ => ToolSource::Community,
        }
    }
}

/// Record in the tool_embeddings table.
#[derive(Debug, Clone)]
pub struct ToolEmbeddingRecord {
    pub tool_name: String,
    pub description: String,
    pub embedding: Vec<f32>,
    pub domain: String,
    pub source: ToolSource,
    pub updated_at: i64,
}

/// Which search tier is currently active for tool discovery.
/// Lower numeric value = better tier (used in promote_tier_if_available).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SearchTier {
    /// Tier 1: Local Ollama embeddings (~50ms, zero cost, full privacy)
    LocalEmbedding = 0,
    /// Tier 2: Cloud embedding API (OpenAI, Google — tiny cost)
    CloudEmbedding = 1,
    /// Tier 3: BM25 keyword search (no embeddings needed)
    Bm25 = 2,
    /// Tier 4: Domain keyword matching (absolute fallback, always works)
    DomainKeyword = 3,
}

// ── Constants ──────────────────────────────────────────────────────────────

/// Minimum cosine similarity for a tool to be considered a match.
pub const MIN_RELEVANCE: f64 = 0.55;

/// Score above which a domain gets fully expanded (all sibling tools included).
pub const DOMAIN_EXPAND_STRONG: f64 = 0.70;

/// Default top-K tools to return from search.
pub const DEFAULT_TOP_K: usize = 6;

/// Maximum number of tools to return after domain expansion.
pub const MAX_RESULTS: usize = 30;

/// How many consecutive embedding failures before circuit-breaking.
pub const EMBED_CIRCUIT_BREAKER_THRESHOLD: u32 = 3;

// ── BM25 Scoring ──────────────────────────────────────────────────────────

/// BM25 parameters (standard values).
const BM25_K1: f64 = 1.2;
const BM25_B: f64 = 0.75;

/// Tokenize a string for BM25: lowercase, split on non-alphanumeric, filter short tokens.
pub fn bm25_tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.len() >= 2)
        .map(|w| w.to_string())
        .collect()
}

/// Compute BM25 score for a single document against a query.
///
/// `doc_tokens` — tokenized document
/// `query_tokens` — tokenized query
/// `avg_doc_len` — average document length in corpus
/// `doc_count` — total number of documents
/// `doc_freq` — map of term → number of documents containing it
///
/// Returns a non-negative score (higher = more relevant).
pub fn bm25_score(
    doc_tokens: &[String],
    query_tokens: &[String],
    avg_doc_len: f64,
    doc_count: usize,
    doc_freq: &std::collections::HashMap<String, usize>,
) -> f64 {
    let dl = doc_tokens.len() as f64;
    let mut score = 0.0;

    for qt in query_tokens {
        // Term frequency in this document
        let tf = doc_tokens.iter().filter(|t| t == &qt).count() as f64;
        if tf == 0.0 {
            continue;
        }

        // Inverse document frequency
        let df = doc_freq.get(qt).copied().unwrap_or(0) as f64;
        let idf = ((doc_count as f64 - df + 0.5) / (df + 0.5) + 1.0).ln();

        // BM25 formula
        let tf_norm =
            (tf * (BM25_K1 + 1.0)) / (tf + BM25_K1 * (1.0 - BM25_B + BM25_B * dl / avg_doc_len));
        score += idf * tf_norm;
    }

    score
}

// ── Domain Keyword Classifier ──────────────────────────────────────────────

/// Tier 4 fallback: classify a query into a domain by keyword rules.
/// Returns the likely domain(s) for the query. Always returns at least one.
pub fn classify_domain_by_keywords(query: &str) -> Vec<&'static str> {
    let q = query.to_lowercase();
    let mut domains = Vec::new();

    // Direct domain name matches
    let domain_keywords: &[(&[&str], &str)] = &[
        (
            &["email", "mail", "gmail", "inbox", "send message", "compose"],
            "email",
        ),
        (
            &[
                "calendar",
                "meeting",
                "standup",
                "schedule",
                "event",
                "appointment",
            ],
            "google",
        ),
        (
            &["drive", "docs", "sheets", "spreadsheet", "document"],
            "google",
        ),
        (&["slack"], "messaging"),
        (&["telegram"], "messaging"),
        (&["discord"], "discord"),
        (&["discourse", "forum"], "discourse"),
        (&["trello", "kanban", "board", "card"], "trello"),
        (
            &["github", "issue", "pull request", "pr ", "repo"],
            "github",
        ),
        (
            &["file", "folder", "directory", "read file", "write file"],
            "filesystem",
        ),
        (&["terminal", "shell", "command", "exec", "bash"], "system"),
        (
            &[
                "web",
                "browse",
                "search",
                "fetch",
                "http",
                "url",
                "screenshot",
            ],
            "web",
        ),
        (&["memory", "remember", "recall", "forget"], "memory"),
        (&["soul", "identity", "profile", "persona"], "identity"),
        (&["agent", "create agent", "squad"], "agents"),
        (&["task", "automation", "cron", "schedule task"], "tasks"),
        (&["skill", "install", "package"], "skills"),
        (&["canvas", "dashboard", "widget", "bento"], "canvas"),
        (&["coinbase", "crypto", "bitcoin", "ethereum"], "coinbase"),
        (&["dex", "uniswap", "swap", "liquidity"], "dex"),
        (&["solana", "jupiter", "sol "], "solana"),
        (&["trade", "trading", "price", "portfolio"], "coinbase"),
        (
            &["webhook", "rest api", "api call", "integration"],
            "integrations",
        ),
        (
            &["image", "generate image", "dall-e", "picture"],
            "integrations",
        ),
    ];

    for (keywords, domain) in domain_keywords {
        for keyword in *keywords {
            if q.contains(keyword) && !domains.contains(domain) {
                domains.push(*domain);
            }
        }
    }

    // Fallback: if nothing matched, return "other"
    if domains.is_empty() {
        domains.push("other");
    }

    domains
}

// ── Cosine Similarity ──────────────────────────────────────────────────────

/// Cosine similarity between two embedding vectors.
/// Returns value in [-1.0, 1.0]. Higher = more similar.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
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

/// Embedding bytes ↔ f32 vec conversion (same as sessions/embedding.rs).
pub fn f32_vec_to_bytes(vec: &[f32]) -> Vec<u8> {
    vec.iter().flat_map(|f| f.to_le_bytes()).collect()
}

pub fn bytes_to_f32_vec(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // ── ToolSource ─────────────────────────────────────────────────

    #[test]
    fn tool_source_roundtrip() {
        for src in &[ToolSource::Builtin, ToolSource::Mcp, ToolSource::Community] {
            assert_eq!(ToolSource::parse(src.as_str()), *src);
        }
    }

    #[test]
    fn tool_source_unknown_defaults_community() {
        assert_eq!(ToolSource::parse("unknown"), ToolSource::Community);
    }

    // ── BM25 ───────────────────────────────────────────────────────

    #[test]
    fn bm25_tokenize_basic() {
        let tokens = bm25_tokenize("Send an email to john@example.com");
        assert!(tokens.contains(&"send".to_string()));
        assert!(tokens.contains(&"email".to_string()));
        assert!(tokens.contains(&"john".to_string()));
        // "an" has 2 chars so it passes the >= 2 filter
        assert!(tokens.contains(&"an".to_string()));
        // single-char "a" would be filtered out
        let tokens2 = bm25_tokenize("a b cc");
        assert!(!tokens2.contains(&"a".to_string()));
        assert!(!tokens2.contains(&"b".to_string()));
        assert!(tokens2.contains(&"cc".to_string()));
    }

    #[test]
    fn bm25_tokenize_underscores_preserved() {
        let tokens = bm25_tokenize("google_calendar_create");
        assert!(tokens.contains(&"google_calendar_create".to_string()));
    }

    #[test]
    fn bm25_score_exact_match_high() {
        let doc = bm25_tokenize("email_send: Send an email via SMTP");
        let query = bm25_tokenize("send email");
        let mut doc_freq = HashMap::new();
        doc_freq.insert("send".to_string(), 1);
        doc_freq.insert("email".to_string(), 1);

        let score = bm25_score(&doc, &query, 6.0, 10, &doc_freq);
        assert!(score > 0.0, "Exact match should have positive score");
    }

    #[test]
    fn bm25_score_no_overlap_zero() {
        let doc = bm25_tokenize("coinbase_prices: Get cryptocurrency prices");
        let query = bm25_tokenize("send email");
        let doc_freq = HashMap::new();

        let score = bm25_score(&doc, &query, 6.0, 10, &doc_freq);
        assert_eq!(score, 0.0, "No token overlap should give zero score");
    }

    #[test]
    fn bm25_relevant_doc_scores_higher() {
        let doc_email = bm25_tokenize("email_send: Send an email message to a recipient");
        let doc_crypto = bm25_tokenize("coinbase_trade: Trade cryptocurrency on Coinbase");
        let query = bm25_tokenize("send email to user");

        let mut doc_freq = HashMap::new();
        doc_freq.insert("send".to_string(), 2);
        doc_freq.insert("email".to_string(), 1);
        doc_freq.insert("to".to_string(), 5);

        let score_email = bm25_score(&doc_email, &query, 7.0, 10, &doc_freq);
        let score_crypto = bm25_score(&doc_crypto, &query, 7.0, 10, &doc_freq);

        assert!(
            score_email > score_crypto,
            "Email doc ({}) should score higher than crypto doc ({})",
            score_email,
            score_crypto
        );
    }

    // ── Domain Keyword Classifier ──────────────────────────────────

    #[test]
    fn classify_email_query() {
        let domains = classify_domain_by_keywords("send an email to john");
        assert!(domains.contains(&"email"));
    }

    #[test]
    fn classify_calendar_query() {
        let domains = classify_domain_by_keywords("schedule a meeting for tomorrow");
        assert!(domains.contains(&"google"));
    }

    #[test]
    fn classify_multi_domain() {
        let domains = classify_domain_by_keywords("send email and create a calendar event");
        assert!(domains.contains(&"email"));
        assert!(domains.contains(&"google"));
    }

    #[test]
    fn classify_crypto_trading() {
        let domains = classify_domain_by_keywords("check bitcoin price on coinbase");
        assert!(domains.contains(&"coinbase"));
    }

    #[test]
    fn classify_unknown_returns_other() {
        let domains = classify_domain_by_keywords("xyzzy plugh");
        assert_eq!(domains, vec!["other"]);
    }

    #[test]
    fn classify_web_search() {
        let domains = classify_domain_by_keywords("search the web for rust tutorials");
        assert!(domains.contains(&"web"));
    }

    #[test]
    fn classify_discord() {
        let domains = classify_domain_by_keywords("send a message on discord");
        assert!(domains.contains(&"discord"));
    }

    // ── Cosine Similarity ──────────────────────────────────────────

    #[test]
    fn cosine_identical() {
        let v = vec![1.0f32, 2.0, 3.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_orthogonal() {
        let a = vec![1.0f32, 0.0, 0.0];
        let b = vec![0.0f32, 1.0, 0.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn cosine_empty() {
        let a: Vec<f32> = vec![];
        assert_eq!(cosine_similarity(&a, &a), 0.0);
    }

    #[test]
    fn cosine_different_length() {
        let a = vec![1.0f32, 2.0];
        let b = vec![1.0f32, 2.0, 3.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    // ── f32 bytes roundtrip ────────────────────────────────────────

    #[test]
    fn f32_bytes_roundtrip() {
        let original = vec![1.0f32, -2.5, std::f32::consts::PI, 0.0, f32::MAX];
        let bytes = f32_vec_to_bytes(&original);
        let restored = bytes_to_f32_vec(&bytes);
        assert_eq!(original, restored);
    }

    #[test]
    fn f32_bytes_empty() {
        let empty: Vec<f32> = vec![];
        let bytes = f32_vec_to_bytes(&empty);
        assert!(bytes.is_empty());
        assert!(bytes_to_f32_vec(&bytes).is_empty());
    }
}
