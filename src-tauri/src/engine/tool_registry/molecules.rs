// ─────────────────────────────────────────────────────────────────────────────
// Embedding-Indexed Tool Registry — Molecules
//
// Side-effectful operations: SQLite persistence, embedding computation,
// incremental indexing, hierarchical search with failover.
//
// This module owns the persistent tool_embeddings table and provides
// the four-tier search failover:
//   Tier 1: Vector search (local Ollama embeddings)
//   Tier 2: Vector search (cloud embedding API)
//   Tier 3: BM25 keyword search (no embeddings needed)
//   Tier 4: Domain keyword matching (absolute fallback)
// ─────────────────────────────────────────────────────────────────────────────

use super::atoms::{
    self, bytes_to_f32_vec, f32_vec_to_bytes, SearchTier, ToolEmbeddingRecord, ToolSource,
    DOMAIN_EXPAND_STRONG, MAX_RESULTS, MIN_RELEVANCE,
};
use crate::atoms::error::EngineResult;
use crate::atoms::types::ToolDefinition;
use crate::engine::memory::EmbeddingClient;
use crate::engine::tool_index::tool_domain;
use log::{info, warn};
use parking_lot::Mutex;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

// ── Persistent Registry ────────────────────────────────────────────────────

/// Persistent tool embedding registry backed by SQLite.
///
/// Stores tool embeddings across restarts. Supports incremental updates
/// (only re-embed changed tools) and hierarchical search.
pub struct PersistentToolRegistry {
    /// Current search tier (auto-detected based on embedding availability).
    current_tier: SearchTier,
    /// Cached domain centroids for Tier 1/2 hierarchical search.
    /// Domain name → centroid embedding.
    domain_centroids: HashMap<String, Vec<f32>>,
}

impl Default for PersistentToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PersistentToolRegistry {
    pub fn new() -> Self {
        PersistentToolRegistry {
            current_tier: SearchTier::DomainKeyword, // start at lowest, promote on success
            domain_centroids: HashMap::new(),
        }
    }

    /// Current active search tier.
    pub fn current_tier(&self) -> SearchTier {
        self.current_tier
    }

    // ── Save & Load ────────────────────────────────────────────────────

    /// Save a tool embedding to the persistent store.
    pub fn save_embedding(conn: &Connection, record: &ToolEmbeddingRecord) -> EngineResult<()> {
        let embedding_bytes = f32_vec_to_bytes(&record.embedding);
        conn.execute(
            "INSERT OR REPLACE INTO tool_embeddings \
             (tool_name, description, embedding, domain, source, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                record.tool_name,
                record.description,
                embedding_bytes,
                record.domain,
                record.source.as_str(),
                record.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Load all cached embeddings from the store.
    pub fn load_all(conn: &Connection) -> EngineResult<Vec<ToolEmbeddingRecord>> {
        let mut stmt = conn.prepare(
            "SELECT tool_name, description, embedding, domain, source, updated_at \
             FROM tool_embeddings",
        )?;
        let records = stmt
            .query_map([], |row| {
                let embedding_bytes: Vec<u8> = row.get(2)?;
                Ok(ToolEmbeddingRecord {
                    tool_name: row.get(0)?,
                    description: row.get(1)?,
                    embedding: bytes_to_f32_vec(&embedding_bytes),
                    domain: row.get(3)?,
                    source: ToolSource::parse(&row.get::<_, String>(4)?),
                    updated_at: row.get(5)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(records)
    }

    /// Load embeddings for a specific domain.
    pub fn load_domain(conn: &Connection, domain: &str) -> EngineResult<Vec<ToolEmbeddingRecord>> {
        let mut stmt = conn.prepare(
            "SELECT tool_name, description, embedding, domain, source, updated_at \
             FROM tool_embeddings WHERE domain = ?1",
        )?;
        let records = stmt
            .query_map(rusqlite::params![domain], |row| {
                let embedding_bytes: Vec<u8> = row.get(2)?;
                Ok(ToolEmbeddingRecord {
                    tool_name: row.get(0)?,
                    description: row.get(1)?,
                    embedding: bytes_to_f32_vec(&embedding_bytes),
                    domain: row.get(3)?,
                    source: ToolSource::parse(&row.get::<_, String>(4)?),
                    updated_at: row.get(5)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(records)
    }

    /// Count cached embeddings.
    pub fn count(conn: &Connection) -> EngineResult<usize> {
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM tool_embeddings", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Get tool names that are already cached.
    pub fn cached_tool_names(conn: &Connection) -> EngineResult<HashSet<String>> {
        let mut stmt = conn.prepare("SELECT tool_name FROM tool_embeddings")?;
        let names: HashSet<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(names)
    }

    /// Remove tools that are no longer in the active tool set.
    pub fn prune_stale(conn: &Connection, active_names: &HashSet<String>) -> EngineResult<usize> {
        let cached = Self::cached_tool_names(conn)?;
        let stale: Vec<&String> = cached.difference(active_names).collect();

        if stale.is_empty() {
            return Ok(0);
        }

        let count = stale.len();
        for name in &stale {
            conn.execute(
                "DELETE FROM tool_embeddings WHERE tool_name = ?1",
                rusqlite::params![name],
            )?;
        }

        info!("[tool-registry] Pruned {} stale tool embeddings", count);
        Ok(count)
    }

    // ── Incremental Indexing ───────────────────────────────────────────

    /// Incrementally index tools: only embed those not already cached.
    ///
    /// Returns (embedded_count, skipped_count, failed_count).
    pub async fn incremental_index(
        &mut self,
        tools: &[ToolDefinition],
        client: &EmbeddingClient,
        conn: &Arc<Mutex<Connection>>,
    ) -> (usize, usize, usize) {
        let cached_names = {
            let db = conn.lock();
            Self::cached_tool_names(&db).unwrap_or_default()
        };

        let now = chrono::Utc::now().timestamp();
        let mut embedded = 0;
        let mut skipped = 0;
        let mut failed = 0;
        let mut consecutive_failures: u32 = 0;

        for tool in tools {
            let name = &tool.function.name;

            // Skip if already cached
            if cached_names.contains(name) {
                skipped += 1;
                continue;
            }

            // Circuit breaker: too many consecutive failures
            if consecutive_failures >= atoms::EMBED_CIRCUIT_BREAKER_THRESHOLD {
                // Save without embedding — still searchable by BM25/keyword
                let record = ToolEmbeddingRecord {
                    tool_name: name.clone(),
                    description: tool.function.description.clone(),
                    embedding: Vec::new(),
                    domain: tool_domain(name).to_string(),
                    source: classify_tool_source(name),
                    updated_at: now,
                };
                let db = conn.lock();
                Self::save_embedding(&db, &record).ok();
                failed += 1;
                continue;
            }

            let text = format!("{}: {}", name, tool.function.description);
            match client.embed(&text).await {
                Ok(embedding) => {
                    let record = ToolEmbeddingRecord {
                        tool_name: name.clone(),
                        description: tool.function.description.clone(),
                        embedding,
                        domain: tool_domain(name).to_string(),
                        source: classify_tool_source(name),
                        updated_at: now,
                    };
                    let db = conn.lock();
                    Self::save_embedding(&db, &record).ok();
                    embedded += 1;
                    consecutive_failures = 0;
                }
                Err(e) => {
                    if failed == 0 {
                        warn!("[tool-registry] Embedding failed for '{}': {}", name, e);
                    }
                    // Save without embedding
                    let record = ToolEmbeddingRecord {
                        tool_name: name.clone(),
                        description: tool.function.description.clone(),
                        embedding: Vec::new(),
                        domain: tool_domain(name).to_string(),
                        source: classify_tool_source(name),
                        updated_at: now,
                    };
                    let db = conn.lock();
                    Self::save_embedding(&db, &record).ok();
                    failed += 1;
                    consecutive_failures += 1;
                }
            }
        }

        // Update search tier based on results
        if embedded > 0 {
            self.current_tier = SearchTier::LocalEmbedding;
        } else if failed > 0 && skipped > 0 {
            // Cached embeddings exist but new ones failed — still usable
            self.current_tier = SearchTier::LocalEmbedding;
        }

        // Rebuild domain centroids
        {
            let db = conn.lock();
            self.rebuild_centroids(&db);
        }

        info!(
            "[tool-registry] Incremental index: {} embedded, {} skipped (cached), {} failed. Tier={:?}",
            embedded, skipped, failed, self.current_tier
        );

        (embedded, skipped, failed)
    }

    // ── Hierarchical Search ────────────────────────────────────────────

    /// Search for tools using the current best-available tier.
    ///
    /// Failover: Vector → BM25 → Domain keyword.
    pub async fn search(
        &self,
        query: &str,
        top_k: usize,
        client: Option<&EmbeddingClient>,
        conn: &Arc<Mutex<Connection>>,
    ) -> EngineResult<Vec<ToolSearchResult>> {
        // Try vector search first (Tier 1 or 2)
        if let Some(emb_client) = client {
            if matches!(
                self.current_tier,
                SearchTier::LocalEmbedding | SearchTier::CloudEmbedding
            ) {
                match self.vector_search(query, top_k, emb_client, conn).await {
                    Ok(results) if !results.is_empty() => return Ok(results),
                    Ok(_) => {
                        info!("[tool-registry] Vector search returned empty, falling back to BM25");
                    }
                    Err(e) => {
                        warn!(
                            "[tool-registry] Vector search failed ({}), falling back to BM25",
                            e
                        );
                    }
                }
            }
        }

        // Tier 3: BM25 keyword search
        let bm25_results = self.bm25_search(query, top_k, conn);
        if !bm25_results.is_empty() {
            return Ok(bm25_results);
        }

        // Tier 4: Domain keyword matching (always returns something)
        Ok(self.domain_keyword_search(query, conn))
    }

    /// Tier 1/2: Vector search with hierarchical domain expansion.
    async fn vector_search(
        &self,
        query: &str,
        top_k: usize,
        client: &EmbeddingClient,
        conn: &Arc<Mutex<Connection>>,
    ) -> EngineResult<Vec<ToolSearchResult>> {
        let query_vec = client.embed(query).await?;

        // Stage 1: If domain centroids exist, find the best-matching domain first
        let target_domain = if !self.domain_centroids.is_empty() {
            let mut best_domain = None;
            let mut best_score = 0.0f64;
            for (domain, centroid) in &self.domain_centroids {
                let sim = atoms::cosine_similarity(&query_vec, centroid);
                if sim > best_score {
                    best_score = sim;
                    best_domain = Some(domain.clone());
                }
            }
            if best_score >= MIN_RELEVANCE {
                info!(
                    "[tool-registry] Hierarchical: top domain '{}' (score={:.3})",
                    best_domain.as_deref().unwrap_or("?"),
                    best_score
                );
                best_domain
            } else {
                None
            }
        } else {
            None
        };

        // Stage 2: Search within the target domain (or all tools)
        let records = {
            let db = conn.lock();
            if let Some(ref domain) = target_domain {
                Self::load_domain(&db, domain)?
            } else {
                Self::load_all(&db)?
            }
        };

        // Score by cosine similarity
        let mut scored: Vec<(ToolSearchResult, f64)> = records
            .iter()
            .filter(|r| !r.embedding.is_empty())
            .map(|r| {
                let sim = atoms::cosine_similarity(&query_vec, &r.embedding);
                (
                    ToolSearchResult {
                        tool_name: r.tool_name.clone(),
                        description: r.description.clone(),
                        domain: r.domain.clone(),
                        source: r.source,
                        score: sim,
                        tier: SearchTier::LocalEmbedding,
                    },
                    sim,
                )
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Collect top-K above minimum relevance
        let mut results: Vec<ToolSearchResult> = Vec::new();
        let mut matched_domains: HashSet<String> = HashSet::new();
        let mut domain_best: HashMap<String, f64> = HashMap::new();
        let mut domain_hits: HashMap<String, u32> = HashMap::new();

        for (result, score) in scored.iter().take(top_k) {
            if *score >= MIN_RELEVANCE {
                results.push(result.clone());
                let best = domain_best.entry(result.domain.clone()).or_insert(0.0);
                if *score > *best {
                    *best = *score;
                }
                *domain_hits.entry(result.domain.clone()).or_insert(0) += 1;
            }
        }

        // Domain expansion: include sibling tools for strong matches
        for (domain, best_score) in &domain_best {
            let hits = domain_hits.get(domain).copied().unwrap_or(0);
            if *best_score >= DOMAIN_EXPAND_STRONG || hits >= 2 {
                matched_domains.insert(domain.clone());
            }
        }

        if !matched_domains.is_empty() {
            let db = conn.lock();
            for domain in &matched_domains {
                let siblings = Self::load_domain(&db, domain).unwrap_or_default();
                for sib in siblings {
                    if !results.iter().any(|r| r.tool_name == sib.tool_name) {
                        results.push(ToolSearchResult {
                            tool_name: sib.tool_name,
                            description: sib.description,
                            domain: sib.domain,
                            source: sib.source,
                            score: 0.0, // sibling, not directly matched
                            tier: SearchTier::LocalEmbedding,
                        });
                    }
                }
            }
        }

        // Cap results
        results.truncate(MAX_RESULTS);
        Ok(results)
    }

    /// Tier 3: BM25 keyword search over cached tool descriptions.
    fn bm25_search(
        &self,
        query: &str,
        top_k: usize,
        conn: &Arc<Mutex<Connection>>,
    ) -> Vec<ToolSearchResult> {
        let records = {
            let db = conn.lock();
            Self::load_all(&db).unwrap_or_default()
        };

        if records.is_empty() {
            return Vec::new();
        }

        let query_tokens = atoms::bm25_tokenize(query);
        if query_tokens.is_empty() {
            return Vec::new();
        }

        // Build document frequency map
        let doc_texts: Vec<Vec<String>> = records
            .iter()
            .map(|r| atoms::bm25_tokenize(&format!("{}: {}", r.tool_name, r.description)))
            .collect();

        let avg_doc_len =
            doc_texts.iter().map(|d| d.len()).sum::<usize>() as f64 / doc_texts.len() as f64;

        let mut doc_freq: HashMap<String, usize> = HashMap::new();
        for doc in &doc_texts {
            let unique: HashSet<&String> = doc.iter().collect();
            for term in unique {
                *doc_freq.entry(term.clone()).or_insert(0) += 1;
            }
        }

        // Score each document
        let mut scored: Vec<(usize, f64)> = doc_texts
            .iter()
            .enumerate()
            .map(|(i, doc)| {
                let score =
                    atoms::bm25_score(doc, &query_tokens, avg_doc_len, records.len(), &doc_freq);
                (i, score)
            })
            .filter(|(_, score)| *score > 0.0)
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        scored
            .into_iter()
            .take(top_k)
            .map(|(i, score)| {
                let r = &records[i];
                ToolSearchResult {
                    tool_name: r.tool_name.clone(),
                    description: r.description.clone(),
                    domain: r.domain.clone(),
                    source: r.source,
                    score,
                    tier: SearchTier::Bm25,
                }
            })
            .collect()
    }

    /// Tier 4: Domain keyword matching — absolute fallback, always returns something.
    fn domain_keyword_search(
        &self,
        query: &str,
        conn: &Arc<Mutex<Connection>>,
    ) -> Vec<ToolSearchResult> {
        let domains = atoms::classify_domain_by_keywords(query);
        let mut results = Vec::new();

        let db = conn.lock();
        for domain in domains {
            let domain_records = Self::load_domain(&db, domain).unwrap_or_default();
            for r in domain_records {
                if !results
                    .iter()
                    .any(|res: &ToolSearchResult| res.tool_name == r.tool_name)
                {
                    results.push(ToolSearchResult {
                        tool_name: r.tool_name,
                        description: r.description,
                        domain: r.domain,
                        source: r.source,
                        score: 0.0,
                        tier: SearchTier::DomainKeyword,
                    });
                }
            }
        }

        results.truncate(MAX_RESULTS);
        results
    }

    // ── Domain Centroids ───────────────────────────────────────────────

    /// Rebuild domain centroids from cached embeddings.
    /// A centroid is the average embedding of all tools in a domain.
    fn rebuild_centroids(&mut self, conn: &Connection) {
        let records = Self::load_all(conn).unwrap_or_default();
        let mut domain_vecs: HashMap<String, Vec<Vec<f32>>> = HashMap::new();

        for r in &records {
            if !r.embedding.is_empty() {
                domain_vecs
                    .entry(r.domain.clone())
                    .or_default()
                    .push(r.embedding.clone());
            }
        }

        self.domain_centroids.clear();
        for (domain, vecs) in &domain_vecs {
            if vecs.is_empty() {
                continue;
            }
            let dim = vecs[0].len();
            let mut centroid = vec![0.0f32; dim];
            for v in vecs {
                if v.len() == dim {
                    for (i, val) in v.iter().enumerate() {
                        centroid[i] += val;
                    }
                }
            }
            let n = vecs.len() as f32;
            for val in &mut centroid {
                *val /= n;
            }
            self.domain_centroids.insert(domain.clone(), centroid);
        }

        info!(
            "[tool-registry] Rebuilt {} domain centroids",
            self.domain_centroids.len()
        );
    }

    /// Check if Ollama is available for embeddings (Tier 1 promotion).
    pub fn promote_tier_if_available(&mut self, tier: SearchTier) {
        if tier as u8 <= self.current_tier as u8 {
            info!(
                "[tool-registry] Promoting search tier: {:?} → {:?}",
                self.current_tier, tier
            );
            self.current_tier = tier;
        }
    }
}

// ── Search Result ──────────────────────────────────────────────────────────

/// Result from a tool registry search.
#[derive(Debug, Clone)]
pub struct ToolSearchResult {
    pub tool_name: String,
    pub description: String,
    pub domain: String,
    pub source: ToolSource,
    /// Relevance score (meaning depends on tier: cosine sim or BM25 score).
    pub score: f64,
    /// Which search tier produced this result.
    pub tier: SearchTier,
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Classify a tool name into its source category.
fn classify_tool_source(name: &str) -> ToolSource {
    if name.starts_with("mcp_") {
        ToolSource::Mcp
    } else {
        ToolSource::Builtin
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::sessions::schema;
    use rusqlite::Connection as RusqliteConnection;

    fn setup_db() -> Arc<Mutex<RusqliteConnection>> {
        let conn = RusqliteConnection::open_in_memory().unwrap();
        schema::run_migrations(&conn).unwrap();
        Arc::new(Mutex::new(conn))
    }

    fn make_record(
        name: &str,
        desc: &str,
        domain: &str,
        embedding: Vec<f32>,
    ) -> ToolEmbeddingRecord {
        ToolEmbeddingRecord {
            tool_name: name.to_string(),
            description: desc.to_string(),
            embedding,
            domain: domain.to_string(),
            source: ToolSource::Builtin,
            updated_at: 1000,
        }
    }

    #[test]
    fn save_and_load_embedding() {
        let conn = setup_db();
        let db = conn.lock();

        let record = make_record("email_send", "Send an email", "email", vec![1.0, 2.0, 3.0]);

        PersistentToolRegistry::save_embedding(&db, &record).unwrap();

        let loaded = PersistentToolRegistry::load_all(&db).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].tool_name, "email_send");
        assert_eq!(loaded[0].embedding, vec![1.0, 2.0, 3.0]);
        assert_eq!(loaded[0].domain, "email");
    }

    #[test]
    fn save_upserts_existing() {
        let conn = setup_db();
        let db = conn.lock();

        let record1 = make_record("email_send", "Send email v1", "email", vec![1.0]);
        PersistentToolRegistry::save_embedding(&db, &record1).unwrap();

        let record2 = make_record("email_send", "Send email v2", "email", vec![2.0]);
        PersistentToolRegistry::save_embedding(&db, &record2).unwrap();

        let loaded = PersistentToolRegistry::load_all(&db).unwrap();
        assert_eq!(loaded.len(), 1); // upserted, not duplicated
        assert_eq!(loaded[0].description, "Send email v2");
        assert_eq!(loaded[0].embedding, vec![2.0]);
    }

    #[test]
    fn count_embeddings() {
        let conn = setup_db();
        let db = conn.lock();

        assert_eq!(PersistentToolRegistry::count(&db).unwrap(), 0);

        PersistentToolRegistry::save_embedding(&db, &make_record("a", "A", "system", vec![1.0]))
            .unwrap();
        PersistentToolRegistry::save_embedding(&db, &make_record("b", "B", "email", vec![2.0]))
            .unwrap();

        assert_eq!(PersistentToolRegistry::count(&db).unwrap(), 2);
    }

    #[test]
    fn cached_tool_names_returns_set() {
        let conn = setup_db();
        let db = conn.lock();

        PersistentToolRegistry::save_embedding(
            &db,
            &make_record("alpha", "Alpha", "system", vec![]),
        )
        .unwrap();
        PersistentToolRegistry::save_embedding(&db, &make_record("beta", "Beta", "web", vec![]))
            .unwrap();

        let names = PersistentToolRegistry::cached_tool_names(&db).unwrap();
        assert!(names.contains("alpha"));
        assert!(names.contains("beta"));
        assert_eq!(names.len(), 2);
    }

    #[test]
    fn prune_stale_removes_old_tools() {
        let conn = setup_db();
        let db = conn.lock();

        PersistentToolRegistry::save_embedding(
            &db,
            &make_record("keep_me", "Keep", "system", vec![]),
        )
        .unwrap();
        PersistentToolRegistry::save_embedding(
            &db,
            &make_record("remove_me", "Stale", "system", vec![]),
        )
        .unwrap();

        let mut active = HashSet::new();
        active.insert("keep_me".to_string());

        let pruned = PersistentToolRegistry::prune_stale(&db, &active).unwrap();
        assert_eq!(pruned, 1);
        assert_eq!(PersistentToolRegistry::count(&db).unwrap(), 1);
    }

    #[test]
    fn load_domain_filters() {
        let conn = setup_db();
        let db = conn.lock();

        PersistentToolRegistry::save_embedding(
            &db,
            &make_record("email_send", "Send email", "email", vec![1.0]),
        )
        .unwrap();
        PersistentToolRegistry::save_embedding(
            &db,
            &make_record("web_search", "Search web", "web", vec![2.0]),
        )
        .unwrap();

        let email_tools = PersistentToolRegistry::load_domain(&db, "email").unwrap();
        assert_eq!(email_tools.len(), 1);
        assert_eq!(email_tools[0].tool_name, "email_send");
    }

    #[test]
    fn bm25_search_finds_relevant_tools() {
        let conn = setup_db();
        {
            let db = conn.lock();
            PersistentToolRegistry::save_embedding(
                &db,
                &make_record(
                    "email_send",
                    "Send an email message to a recipient via SMTP",
                    "email",
                    vec![],
                ),
            )
            .unwrap();
            PersistentToolRegistry::save_embedding(
                &db,
                &make_record(
                    "coinbase_trade",
                    "Trade cryptocurrency on Coinbase exchange",
                    "coinbase",
                    vec![],
                ),
            )
            .unwrap();
            PersistentToolRegistry::save_embedding(
                &db,
                &make_record(
                    "web_search",
                    "Search the web for information",
                    "web",
                    vec![],
                ),
            )
            .unwrap();
        }

        let registry = PersistentToolRegistry::new();
        let results = registry.bm25_search("send email", 3, &conn);

        assert!(!results.is_empty(), "BM25 should find at least one result");
        assert_eq!(
            results[0].tool_name, "email_send",
            "email_send should be the top BM25 result"
        );
        assert_eq!(results[0].tier, SearchTier::Bm25);
    }

    #[test]
    fn domain_keyword_search_always_returns() {
        let conn = setup_db();
        {
            let db = conn.lock();
            PersistentToolRegistry::save_embedding(
                &db,
                &make_record("email_send", "Send an email", "email", vec![]),
            )
            .unwrap();
        }

        let registry = PersistentToolRegistry::new();
        let results = registry.domain_keyword_search("send an email to john", &conn);
        assert!(
            !results.is_empty(),
            "Domain keyword search should always return results"
        );
        assert_eq!(results[0].tier, SearchTier::DomainKeyword);
    }

    #[test]
    fn classify_tool_source_mcp() {
        assert_eq!(classify_tool_source("mcp_n8n_workflow"), ToolSource::Mcp);
    }

    #[test]
    fn classify_tool_source_builtin() {
        assert_eq!(classify_tool_source("email_send"), ToolSource::Builtin);
    }

    #[test]
    fn rebuild_centroids_computes_averages() {
        let conn = setup_db();
        {
            let db = conn.lock();
            PersistentToolRegistry::save_embedding(
                &db,
                &make_record("email_send", "Send email", "email", vec![2.0, 4.0]),
            )
            .unwrap();
            PersistentToolRegistry::save_embedding(
                &db,
                &make_record("email_read", "Read email", "email", vec![4.0, 6.0]),
            )
            .unwrap();
            PersistentToolRegistry::save_embedding(
                &db,
                &make_record("web_search", "Search web", "web", vec![1.0, 1.0]),
            )
            .unwrap();
        }

        let mut registry = PersistentToolRegistry::new();
        {
            let db = conn.lock();
            registry.rebuild_centroids(&db);
        }

        assert_eq!(registry.domain_centroids.len(), 2);
        // Email centroid: (2+4)/2=3.0, (4+6)/2=5.0
        let email_centroid = registry.domain_centroids.get("email").unwrap();
        assert_eq!(email_centroid, &vec![3.0f32, 5.0]);
        // Web centroid: just the one vector
        let web_centroid = registry.domain_centroids.get("web").unwrap();
        assert_eq!(web_centroid, &vec![1.0f32, 1.0]);
    }
}
