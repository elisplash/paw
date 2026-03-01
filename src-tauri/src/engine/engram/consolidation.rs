// ── Engram: Consolidation Engine ─────────────────────────────────────────────
//
// Asynchronous pipeline that processes raw episodic memories into
// higher-order semantic and procedural knowledge.
//
// Pipeline stages:
//   1. Candidate selection  – fetch "raw" episodic memories older than threshold
//   2. Embedding enrichment – generate embeddings for un-embedded candidates
//   3. Similarity clustering – build cosine-similarity graph, find components
//   4. Schema extraction    – extract SPO triples from each cluster
//   5. Contradiction detection & resolution
//   6. Gap detection        – find incomplete schemas, stale knowledge
//   7. Mark processed       – set consolidation_state = Consolidated/Archived

use crate::atoms::engram_types::{
    ConsolidationState, EdgeType, EpisodicMemory, MemoryEdge, SemanticMemory,
};
use crate::atoms::error::EngineResult;
use crate::engine::engram::metadata_inference;
use crate::engine::memory::EmbeddingClient;
use crate::engine::sessions::SessionStore;
use log::{info, warn};

// ═════════════════════════════════════════════════════════════════════════════
// Configuration
// ═════════════════════════════════════════════════════════════════════════════

/// Minimum cluster size for semantic extraction.
const MIN_CLUSTER_SIZE: usize = 3;

/// Cosine similarity threshold for same-cluster membership.
const CLUSTER_SIMILARITY_THRESHOLD: f64 = 0.75;

/// Minimum age (in seconds) before an episodic memory is consolidation-eligible.
const CANDIDATE_MIN_AGE_SECS: u64 = 300; // 5min

/// Max candidates to fetch per consolidation run.
const CANDIDATE_BATCH_SIZE: usize = 200;

/// Confidence boost when a new memory corroborates an existing semantic triple.
const CORROBORATION_CONFIDENCE_BOOST: f32 = 0.05;

/// Confidence transfer ratio from superseded to superseding triple.
const CONTRADICTION_CONFIDENCE_TRANSFER: f32 = 0.2;

/// Max gap suggestions to surface per consolidation run.
const MAX_GAP_SUGGESTIONS: usize = 2;

// ═════════════════════════════════════════════════════════════════════════════
// Public API
// ═════════════════════════════════════════════════════════════════════════════

/// Run one full consolidation cycle. Typically called on a timer (e.g. every 5min).
///
/// Returns `ConsolidationReport` summarizing what happened.
pub async fn run_consolidation(
    store: &SessionStore,
    embedding_client: Option<&EmbeddingClient>,
    merge_threshold: Option<f64>,
) -> EngineResult<ConsolidationReport> {
    let threshold = merge_threshold.unwrap_or(CLUSTER_SIMILARITY_THRESHOLD);
    let mut report = ConsolidationReport::default();

    // ── 1. Fetch candidates ──────────────────────────────────────────────
    let candidates =
        store.engram_list_consolidation_candidates(CANDIDATE_MIN_AGE_SECS, CANDIDATE_BATCH_SIZE)?;

    if candidates.is_empty() {
        return Ok(report);
    }
    report.candidates_found = candidates.len();
    info!(
        "[engram:consolidation] Processing {} candidates",
        candidates.len()
    );

    // ── 2. Enrich embeddings ─────────────────────────────────────────────
    let mut enriched = candidates;
    if let Some(client) = embedding_client {
        enrich_embeddings(store, &mut enriched, client).await;
    }

    // ── 3. Cluster similar memories ──────────────────────────────────────
    let clusters = build_clusters(&enriched, threshold);
    report.clusters_formed = clusters.len();

    // ── 4. Extract semantic triples from clusters ────────────────────────
    for cluster in &clusters {
        let extracted = extract_and_store_semantics(store, cluster, embedding_client).await?;
        report.triples_created += extracted.triples_created;
        report.contradictions_resolved += extracted.contradictions;
    }

    // ── 5. Handle singletons (not part of any cluster) ───────────────────
    let clustered_ids: std::collections::HashSet<&str> = clusters
        .iter()
        .flat_map(|c| c.iter().map(|m| m.id.as_str()))
        .collect();

    for mem in &enriched {
        if !clustered_ids.contains(mem.id.as_str()) {
            // Single memory — still mark as consolidated so we don't re-process
            store.engram_set_consolidation_state(&mem.id, ConsolidationState::Consolidated)?;
            report.singletons_marked += 1;
        }
    }

    // ── 5.5 Metadata inference (§35.3) ───────────────────────────────────
    // Extract structured metadata (technologies, file paths, URLs, language)
    // from all candidates. This enriches the memories for filtered search.
    let mut metadata_enriched = 0usize;
    for mem in &enriched {
        let meta = metadata_inference::infer_metadata_full(&mem.content.full);
        if !meta.is_empty() {
            if let Some(json) = metadata_inference::serialize_metadata(&meta) {
                store.engram_set_inferred_metadata(&mem.id, &json).ok();
                metadata_enriched += 1;
            }
        }
    }
    report.metadata_enriched = metadata_enriched;
    if metadata_enriched > 0 {
        info!(
            "[engram:consolidation] Enriched {} memories with inferred metadata",
            metadata_enriched
        );
    }

    // ── 6. Gap detection ─────────────────────────────────────────────────
    let gaps = detect_gaps(store)?;
    report.gaps_detected = gaps.len();

    if report.gaps_detected > 0 {
        info!(
            "[engram:consolidation] Detected {} knowledge gaps",
            report.gaps_detected
        );
    }

    report.gaps = gaps;

    // ── 7. Audit ─────────────────────────────────────────────────────────
    store.engram_audit_log(
        "consolidation_run",
        "system",
        "system",
        "system",
        Some(&format!(
            "candidates={} clusters={} triples={} contradictions={} gaps={} metadata={}",
            report.candidates_found,
            report.clusters_formed,
            report.triples_created,
            report.contradictions_resolved,
            report.gaps_detected,
            report.metadata_enriched,
        )),
    )?;

    info!(
        "[engram:consolidation] Done — {} triples, {} contradictions, {} gaps, {} metadata",
        report.triples_created,
        report.contradictions_resolved,
        report.gaps_detected,
        report.metadata_enriched
    );

    Ok(report)
}

// ═════════════════════════════════════════════════════════════════════════════
// Report
// ═════════════════════════════════════════════════════════════════════════════

/// Summary of a single consolidation run.
#[derive(Debug, Default, Clone)]
pub struct ConsolidationReport {
    pub candidates_found: usize,
    pub clusters_formed: usize,
    pub triples_created: usize,
    pub contradictions_resolved: usize,
    pub singletons_marked: usize,
    pub gaps_detected: usize,
    /// How many memories had metadata extracted (§35.3).
    pub metadata_enriched: usize,
    /// Detected knowledge gaps for injection into working memory (§4.5).
    pub gaps: Vec<KnowledgeGap>,
}

/// A detected gap in the knowledge graph.
#[derive(Debug, Clone)]
pub struct KnowledgeGap {
    pub kind: GapKind,
    pub description: String,
    /// Related memory IDs for context.
    pub related_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum GapKind {
    /// Semantic triple exists but related triples are missing.
    IncompleteSchema,
    /// Two memories claim contradictory facts with similar confidence.
    UnresolvedContradiction,
    /// Frequently accessed memory that hasn't been updated in a long time.
    StaleHighUse,
}

// ═════════════════════════════════════════════════════════════════════════════
// Internal: Embedding Enrichment
// ═════════════════════════════════════════════════════════════════════════════

async fn enrich_embeddings(
    store: &SessionStore,
    memories: &mut [EpisodicMemory],
    client: &EmbeddingClient,
) {
    let mut enriched = 0usize;
    for mem in memories.iter_mut() {
        if mem.embedding.is_some() {
            continue;
        }
        match client.embed(&mem.content.full).await {
            Ok(emb) => {
                // Persist embedding
                if store
                    .engram_update_episodic_embedding(&mem.id, &emb, client.model_name())
                    .is_ok()
                {
                    mem.embedding = Some(emb);
                    mem.embedding_model = Some(client.model_name().to_string());
                    enriched += 1;
                }
            }
            Err(e) => {
                warn!(
                    "[engram:consolidation] Failed to embed memory {}: {}",
                    mem.id, e
                );
            }
        }
    }
    if enriched > 0 {
        info!(
            "[engram:consolidation] Enriched {} memories with embeddings",
            enriched
        );
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Internal: Similarity Clustering (Union-Find)
// ═════════════════════════════════════════════════════════════════════════════

/// Build connected components via union-find on cosine similarity above threshold.
/// Returns only clusters with >= MIN_CLUSTER_SIZE members.
fn build_clusters(memories: &[EpisodicMemory], threshold: f64) -> Vec<Vec<EpisodicMemory>> {
    let n = memories.len();
    if n < MIN_CLUSTER_SIZE {
        return vec![];
    }

    let mut parent: Vec<usize> = (0..n).collect();
    let mut rank: Vec<usize> = vec![0; n];

    // Build similarity edges
    for i in 0..n {
        for j in (i + 1)..n {
            if let (Some(emb_i), Some(emb_j)) = (&memories[i].embedding, &memories[j].embedding) {
                let sim = cosine_sim(emb_i, emb_j);
                if sim >= threshold {
                    union(&mut parent, &mut rank, i, j);
                }
            }
        }
    }

    // Collect components
    let mut components: std::collections::HashMap<usize, Vec<usize>> =
        std::collections::HashMap::new();
    for i in 0..n {
        let root = find(&mut parent, i);
        components.entry(root).or_default().push(i);
    }

    components
        .into_values()
        .filter(|indices| indices.len() >= MIN_CLUSTER_SIZE)
        .map(|indices| indices.into_iter().map(|i| memories[i].clone()).collect())
        .collect()
}

fn find(parent: &mut [usize], i: usize) -> usize {
    if parent[i] != i {
        parent[i] = find(parent, parent[i]);
    }
    parent[i]
}

fn union(parent: &mut [usize], rank: &mut [usize], a: usize, b: usize) {
    let ra = find(parent, a);
    let rb = find(parent, b);
    if ra == rb {
        return;
    }
    if rank[ra] < rank[rb] {
        parent[ra] = rb;
    } else if rank[ra] > rank[rb] {
        parent[rb] = ra;
    } else {
        parent[rb] = ra;
        rank[ra] += 1;
    }
}

/// Cosine similarity between two f32 vectors.
fn cosine_sim(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut dot, mut na, mut nb) = (0.0f64, 0.0f64, 0.0f64);
    for (x, y) in a.iter().zip(b.iter()) {
        let (fx, fy) = (*x as f64, *y as f64);
        dot += fx * fy;
        na += fx * fx;
        nb += fy * fy;
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom < 1e-12 {
        0.0
    } else {
        dot / denom
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Internal: Semantic Extraction from Clusters
// ═════════════════════════════════════════════════════════════════════════════

struct ExtractionResult {
    triples_created: usize,
    contradictions: usize,
}

/// Extract semantic knowledge from a cluster of similar episodic memories.
///
/// Strategy: lightweight local extraction without LLM dependency.
///   - Find the most "representative" memory (highest importance)
///   - Build SPO triple: subject=category, predicate="relates_to", object=key_fact
///   - Check for contradictions with existing semantic memories
///   - Link all cluster members to the new/updated semantic memory
async fn extract_and_store_semantics(
    store: &SessionStore,
    cluster: &[EpisodicMemory],
    embedding_client: Option<&EmbeddingClient>,
) -> EngineResult<ExtractionResult> {
    let mut result = ExtractionResult {
        triples_created: 0,
        contradictions: 0,
    };

    if cluster.is_empty() {
        return Ok(result);
    }

    // Pick the representative memory (highest importance)
    let representative = cluster
        .iter()
        .max_by(|a, b| {
            a.importance
                .partial_cmp(&b.importance)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap();

    // Derive the scope from the representative
    let scope = representative.scope.clone();

    // Build subject from category, predicate from common pattern, object from content
    let subject = extract_subject(representative);
    let predicate = extract_predicate(cluster);
    let object = extract_object(representative);

    // Compute confidence: weighted average of member importance values
    let total_importance: f32 = cluster.iter().map(|m| m.importance).sum();
    let confidence = (total_importance / cluster.len() as f32).min(1.0);

    // Check for existing triple with same subject+predicate
    let existing = store.engram_lookup_by_subject(&subject, &scope)?;
    let mut contradictions = 0usize;

    for existing_mem in &existing {
        if existing_mem.predicate == predicate {
            if existing_mem.object == object {
                // Corroboration: boost confidence of existing triple
                let boosted_confidence =
                    (existing_mem.confidence + CORROBORATION_CONFIDENCE_BOOST).min(1.0);
                let mut updated = existing_mem.clone();
                updated.confidence = boosted_confidence;
                updated.updated_at =
                    Some(chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
                store.engram_store_semantic(&updated)?;

                // Link cluster members → existing triple
                link_cluster_to_semantic(store, cluster, &existing_mem.id)?;

                info!(
                    "[engram:consolidation] Corroborated triple {} (confidence → {:.2})",
                    existing_mem.id, boosted_confidence
                );

                return Ok(result);
            }

            // Contradiction: same subject+predicate, different object
            contradictions += 1;

            // Recency wins — create new triple superseding old
            let new_id = uuid::Uuid::new_v4().to_string();
            let transferred_confidence =
                existing_mem.confidence * CONTRADICTION_CONFIDENCE_TRANSFER;

            let new_triple = SemanticMemory {
                id: new_id.clone(),
                subject: subject.clone(),
                predicate: predicate.clone(),
                object: object.clone(),
                full_text: format!("{} {} {}", subject, predicate, object),
                category: representative.category.clone(),
                confidence: (confidence + transferred_confidence).min(1.0),
                is_user_explicit: false,
                contradiction_of: Some(existing_mem.id.clone()),
                scope: scope.clone(),
                embedding: None,
                embedding_model: None,
                version: existing_mem.version + 1,
                created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                updated_at: None,
            };

            // Embed if possible
            let mut to_store = new_triple;
            if let Some(client) = embedding_client {
                match client.embed(&to_store.full_text).await {
                    Ok(emb) => {
                        to_store.embedding_model = Some(client.model_name().to_string());
                        to_store.embedding = Some(emb);
                    }
                    Err(e) => {
                        warn!("[engram:consolidation] Failed to embed new triple: {}", e);
                    }
                }
            }

            store.engram_store_semantic(&to_store)?;

            // Create Contradicts edge
            store.engram_add_edge(&MemoryEdge {
                source_id: new_id.clone(),
                target_id: existing_mem.id.clone(),
                edge_type: EdgeType::Contradicts,
                weight: 1.0,
                created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            })?;

            // Link cluster → new triple
            link_cluster_to_semantic(store, cluster, &new_id)?;

            // Audit the contradiction
            store.engram_audit_log(
                "contradiction_resolved",
                &new_id,
                &representative.agent_id,
                &representative.session_id,
                Some(&format!(
                    "supersedes {} (old_object={}, new_object={})",
                    existing_mem.id, existing_mem.object, object
                )),
            )?;

            result.contradictions = contradictions;
            result.triples_created += 1;
            return Ok(result);
        }
    }

    // No existing triple — create new one
    let new_id = uuid::Uuid::new_v4().to_string();
    let new_triple = SemanticMemory {
        id: new_id.clone(),
        subject: subject.clone(),
        predicate: predicate.clone(),
        object,
        full_text: format!(
            "{} {} {}",
            subject,
            predicate,
            extract_object(representative)
        ),
        category: representative.category.clone(),
        confidence,
        is_user_explicit: false,
        contradiction_of: None,
        scope: scope.clone(),
        embedding: None,
        embedding_model: None,
        version: 1,
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        updated_at: None,
    };

    let mut to_store = new_triple;
    if let Some(client) = embedding_client {
        match client.embed(&to_store.full_text).await {
            Ok(emb) => {
                to_store.embedding_model = Some(client.model_name().to_string());
                to_store.embedding = Some(emb);
            }
            Err(e) => warn!("[engram:consolidation] Failed to embed triple: {}", e),
        }
    }

    store.engram_store_semantic(&to_store)?;
    link_cluster_to_semantic(store, cluster, &new_id)?;

    result.triples_created += 1;
    result.contradictions = contradictions;

    info!(
        "[engram:consolidation] Extracted triple {} ({} {} ...)",
        new_id, subject, predicate
    );

    Ok(result)
}

/// Link all cluster members to a semantic memory and mark them as Archived.
fn link_cluster_to_semantic(
    store: &SessionStore,
    cluster: &[EpisodicMemory],
    semantic_id: &str,
) -> EngineResult<()> {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    for mem in cluster {
        store.engram_add_edge(&MemoryEdge {
            source_id: mem.id.clone(),
            target_id: semantic_id.to_string(),
            edge_type: EdgeType::ConsolidatedInto,
            weight: 1.0,
            created_at: now.clone(),
        })?;
        store.engram_set_consolidation_state(&mem.id, ConsolidationState::Archived)?;
    }
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// Internal: Subject / Predicate / Object Extraction
// ═════════════════════════════════════════════════════════════════════════════

/// Extract a subject from the representative memory.
/// Uses category as the primary subject, or falls back to first significant word.
fn extract_subject(mem: &EpisodicMemory) -> String {
    if !mem.category.is_empty() {
        return mem.category.clone();
    }
    // Fall back to first significant word from key_fact or content
    let text = mem.content.key_fact.as_deref().unwrap_or(&mem.content.full);

    text.split_whitespace()
        .find(|w| w.len() > 3)
        .unwrap_or("unknown")
        .to_lowercase()
}

/// Extract a predicate that best represents what the cluster is about.
/// Looks for common terms across all members' key facts / tags.
fn extract_predicate(cluster: &[EpisodicMemory]) -> String {
    // Collect tags across cluster
    let mut tag_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for mem in cluster {
        if let Some(tags_str) = &mem.content.tags {
            for tag in tags_str.split_whitespace() {
                let t = tag
                    .trim_matches(|c: char| !c.is_alphanumeric())
                    .to_lowercase();
                if t.len() > 2 {
                    *tag_counts.entry(t).or_default() += 1;
                }
            }
        }
        // Also extract from content keywords
        for word in mem.content.full.split_whitespace() {
            let w = word
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();
            if w.len() > 4 {
                *tag_counts.entry(w).or_default() += 1;
            }
        }
    }

    // Pick the most common non-trivial term as predicate
    tag_counts
        .into_iter()
        .filter(|(k, _)| !STOP_WORDS.contains(&k.as_str()))
        .max_by_key(|(_, count)| *count)
        .map(|(word, _)| format!("relates_to_{}", word))
        .unwrap_or_else(|| "relates_to".to_string())
}

/// Extract an object (the key fact or summary) from the representative memory.
fn extract_object(mem: &EpisodicMemory) -> String {
    mem.content
        .key_fact
        .clone()
        .or_else(|| mem.content.summary.clone())
        .unwrap_or_else(|| {
            // Truncate full content to keep object reasonable
            let full = &mem.content.full;
            if full.len() > 200 {
                format!("{}...", &full[..200])
            } else {
                full.clone()
            }
        })
}

/// Common stop words to filter out of predicate extraction.
const STOP_WORDS: &[&str] = &[
    "the", "and", "for", "that", "this", "with", "from", "have", "been", "will", "would", "could",
    "should", "about", "into", "they", "their", "there", "what", "when", "where", "which", "while",
    "also", "just", "like", "more", "than", "then", "very", "some", "other", "each", "every",
];

// ═════════════════════════════════════════════════════════════════════════════
// Internal: Gap Detection
// ═════════════════════════════════════════════════════════════════════════════

/// Detect knowledge gaps that could be surfaced to the agent.
/// Returns up to MAX_GAP_SUGGESTIONS gaps.
fn detect_gaps(store: &SessionStore) -> EngineResult<Vec<KnowledgeGap>> {
    let mut gaps = Vec::new();

    // ── 1. Stale high-use memories ───────────────────────────────────────
    // Memories accessed frequently but not updated in 90+ days
    {
        let conn = store.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, content_full, access_count, last_accessed_at
             FROM episodic_memories
             WHERE access_count >= 5
               AND consolidation_state != 'archived'
               AND last_accessed_at < datetime('now', '-90 days')
             ORDER BY access_count DESC
             LIMIT ?1",
        )?;

        let rows: Vec<(String, String)> = stmt
            .query_map(rusqlite::params![MAX_GAP_SUGGESTIONS], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();

        for (id, content) in rows {
            let snippet = if content.len() > 80 {
                format!("{}...", &content[..80])
            } else {
                content
            };
            gaps.push(KnowledgeGap {
                kind: GapKind::StaleHighUse,
                description: format!(
                    "Frequently accessed memory may be outdated: \"{}\"",
                    snippet
                ),
                related_ids: vec![id],
            });
        }
    }

    // ── 2. Unresolved contradictions ─────────────────────────────────────
    // Semantic triples that point to contradictions with similar confidence
    if gaps.len() < MAX_GAP_SUGGESTIONS {
        let conn = store.conn.lock();
        let remaining = MAX_GAP_SUGGESTIONS - gaps.len();
        let mut stmt = conn.prepare(
            "SELECT s1.id, s2.id, s1.subject, s1.predicate, s1.object, s2.object,
                    s1.confidence, s2.confidence
             FROM semantic_memories s1
             JOIN memory_edges e ON e.source_id = s1.id AND e.edge_type = 'Contradicts'
             JOIN semantic_memories s2 ON s2.id = e.target_id
             WHERE ABS(s1.confidence - s2.confidence) < 0.15
             LIMIT ?1",
        )?;

        let rows: Vec<(String, String, String, String, String, String)> = stmt
            .query_map(rusqlite::params![remaining], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        for (id1, id2, subject, predicate, obj1, obj2) in rows {
            gaps.push(KnowledgeGap {
                kind: GapKind::UnresolvedContradiction,
                description: format!(
                    "Conflicting knowledge: {} {} \"{}\" vs \"{}\"",
                    subject, predicate, obj1, obj2
                ),
                related_ids: vec![id1, id2],
            });
        }
    }

    // ── 3. Incomplete schemas (subjects with few triples) ────────────────
    // If a subject has exactly 1 triple, it might benefit from more context
    if gaps.len() < MAX_GAP_SUGGESTIONS {
        let conn = store.conn.lock();
        let remaining = MAX_GAP_SUGGESTIONS - gaps.len();
        let mut stmt = conn.prepare(
            "SELECT subject, COUNT(*) as cnt
             FROM semantic_memories
             GROUP BY subject
             HAVING cnt = 1
             LIMIT ?1",
        )?;

        let rows: Vec<String> = stmt
            .query_map(rusqlite::params![remaining], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();

        for subject in rows {
            gaps.push(KnowledgeGap {
                kind: GapKind::IncompleteSchema,
                description: format!(
                    "Subject \"{}\" has only 1 known fact — knowledge may be incomplete",
                    subject
                ),
                related_ids: vec![],
            });
        }
    }

    Ok(gaps)
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atoms::engram_types::{MemoryScope, MemorySource, TieredContent};

    fn make_mem(
        id: &str,
        content: &str,
        embedding: Option<Vec<f32>>,
        importance: f32,
    ) -> EpisodicMemory {
        EpisodicMemory {
            id: id.to_string(),
            content: TieredContent {
                full: content.to_string(),
                summary: None,
                key_fact: Some(content.to_string()),
                tags: Some("test".to_string()),
            },
            outcome: None,
            category: "testing".to_string(),
            importance,
            agent_id: "agent-1".to_string(),
            session_id: "sess-1".to_string(),
            source: MemorySource::AutoCapture,
            consolidation_state: ConsolidationState::Fresh,
            strength: 1.0,
            scope: MemoryScope::default(),
            embedding,
            embedding_model: None,
            negative_contexts: vec![],
            created_at: "2025-01-01T00:00:00Z".to_string(),
            last_accessed_at: None,
            access_count: 0,
        }
    }

    #[test]
    fn test_cosine_sim_identical() {
        let v = vec![1.0, 0.0, 0.0];
        assert!((cosine_sim(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_sim_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!(cosine_sim(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_sim_empty() {
        assert_eq!(cosine_sim(&[], &[]), 0.0);
    }

    #[test]
    fn test_union_find() {
        let mut parent: Vec<usize> = (0..5).collect();
        let mut rank = vec![0; 5];

        union(&mut parent, &mut rank, 0, 1);
        union(&mut parent, &mut rank, 2, 3);
        union(&mut parent, &mut rank, 1, 3);

        assert_eq!(find(&mut parent, 0), find(&mut parent, 2));
        assert_ne!(find(&mut parent, 0), find(&mut parent, 4));
    }

    #[test]
    fn test_build_clusters_too_few() {
        let memories = vec![
            make_mem("a", "hello", Some(vec![1.0, 0.0]), 0.5),
            make_mem("b", "world", Some(vec![1.0, 0.0]), 0.5),
        ];
        let clusters = build_clusters(&memories, 0.5);
        assert!(clusters.is_empty(), "Need >= 3 members for a cluster");
    }

    #[test]
    fn test_build_clusters_all_similar() {
        let memories = vec![
            make_mem("a", "content a", Some(vec![1.0, 0.1, 0.0]), 0.5),
            make_mem("b", "content b", Some(vec![1.0, 0.2, 0.0]), 0.6),
            make_mem("c", "content c", Some(vec![1.0, 0.15, 0.0]), 0.7),
        ];
        let clusters = build_clusters(&memories, 0.9);
        assert_eq!(clusters.len(), 1, "All similar vectors → 1 cluster");
        assert_eq!(clusters[0].len(), 3);
    }

    #[test]
    fn test_build_clusters_two_groups() {
        let memories = vec![
            make_mem("a", "group1", Some(vec![1.0, 0.0, 0.0]), 0.5),
            make_mem("b", "group1", Some(vec![0.99, 0.1, 0.0]), 0.5),
            make_mem("c", "group1", Some(vec![0.98, 0.05, 0.0]), 0.5),
            make_mem("d", "group2", Some(vec![0.0, 0.0, 1.0]), 0.5),
            make_mem("e", "group2", Some(vec![0.0, 0.1, 0.99]), 0.5),
            make_mem("f", "group2", Some(vec![0.0, 0.05, 0.98]), 0.5),
        ];
        let clusters = build_clusters(&memories, 0.9);
        assert_eq!(clusters.len(), 2, "Two distinct groups → 2 clusters");
    }

    #[test]
    fn test_extract_subject_from_category() {
        let mem = make_mem("a", "some content", None, 0.5);
        assert_eq!(extract_subject(&mem), "testing");
    }

    #[test]
    fn test_extract_object_from_key_fact() {
        let mem = make_mem("a", "the key fact", None, 0.5);
        assert_eq!(extract_object(&mem), "the key fact");
    }

    #[test]
    fn test_extract_predicate_from_tags() {
        let cluster = vec![
            make_mem("a", "rust programming language", None, 0.5),
            make_mem("b", "rust development tools", None, 0.5),
            make_mem("c", "rust cargo build", None, 0.5),
        ];
        let pred = extract_predicate(&cluster);
        // Should contain "relates_to_" prefix
        assert!(pred.starts_with("relates_to_"), "Got: {}", pred);
    }
}
