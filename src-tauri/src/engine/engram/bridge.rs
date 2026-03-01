// ── Engram: Bridge to Engine ─────────────────────────────────────────────────
//
// Bridge functions that connect the Engram memory system to the existing
// engine tools and commands. Provides the same interface as the old
// `engine::memory` module but routes through Engram's three-tier store.
//
// This bridge allows incremental migration:
//   - Agent tools (`memory_store`, `memory_search`) route through Engram
//   - Auto-capture stores to Engram's episodic tier
//   - Search uses Engram's BM25+vector+graph fusion
//   - The old `engine::memory` module remains for backward compatibility

use crate::atoms::engram_types::{
    ConsolidationState, EpisodicMemory, MemoryScope, MemorySearchConfig, MemorySource,
    TieredContent,
};
use crate::atoms::error::EngineResult;
use crate::engine::engram::encryption;
use crate::engine::memory::EmbeddingClient;
use crate::engine::sessions::SessionStore;
use log::{info, warn};

// ═════════════════════════════════════════════════════════════════════════════
// Store (bridge for memory_store tool)
// ═════════════════════════════════════════════════════════════════════════════

/// Store a memory through Engram. Equivalent to the old `memory::store_memory`.
///
/// Creates an episodic memory with deduplication. The `source` is marked as
/// `Explicit` for user/agent-initiated stores.
pub async fn store(
    store: &SessionStore,
    content: &str,
    category: &str,
    importance: f32,
    embedding_client: Option<&EmbeddingClient>,
    agent_id: Option<&str>,
    session_id: Option<&str>,
) -> EngineResult<Option<String>> {
    // §10.17 Input validation
    encryption::validate_memory_input(content, category)?;

    // §10.5 PII detection + field-level encryption
    let enc_key = encryption::get_memory_encryption_key().ok();
    let stored_content = if let Some(ref key) = enc_key {
        let prepared = encryption::prepare_for_storage(content, key)?;
        if prepared.tier != encryption::MemorySecurityTier::Cleartext {
            info!(
                "[engram] Memory classified as {:?} — encrypting ({} PII types detected)",
                prepared.tier,
                prepared.pii_types.len()
            );
        }
        prepared.content
    } else {
        warn!("[engram] No encryption key available — storing cleartext");
        content.to_string()
    };

    let id = uuid::Uuid::new_v4().to_string();

    let mem = EpisodicMemory {
        id: id.clone(),
        content: TieredContent::from_text(&stored_content),
        outcome: None,
        category: category.to_string(),
        importance: importance.min(1.0).max(0.0),
        agent_id: agent_id.unwrap_or("default").to_string(),
        session_id: session_id.unwrap_or("unknown").to_string(),
        source: MemorySource::Explicit,
        consolidation_state: ConsolidationState::Fresh,
        strength: 1.0,
        scope: MemoryScope {
            global: agent_id.is_none(),
            agent_id: agent_id.map(|s| s.to_string()),
            ..Default::default()
        },
        embedding: None,
        embedding_model: None,
        negative_contexts: vec![],
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        last_accessed_at: None,
        access_count: 0,
    };

    // Use dedup to avoid storing duplicates
    let result = super::graph::store_episodic_dedup(store, mem, embedding_client, None).await?;

    Ok(result)
}

/// Store an auto-captured memory (from fact extraction or session summary).
///
/// Uses `AutoCapture` source and lower default importance.
pub async fn store_auto_capture(
    store: &SessionStore,
    content: &str,
    category: &str,
    embedding_client: Option<&EmbeddingClient>,
    agent_id: Option<&str>,
    session_id: Option<&str>,
    channel: Option<&str>,
    channel_user_id: Option<&str>,
) -> EngineResult<Option<String>> {
    // §10.17 Input validation (lenient for auto-capture — skip empty check)
    if content.len() > encryption::MAX_MEMORY_CONTENT_BYTES {
        warn!(
            "[engram] Auto-capture content too large ({} bytes), skipping",
            content.len()
        );
        return Ok(None);
    }

    // §10.5 PII-aware encryption
    let enc_key = encryption::get_memory_encryption_key().ok();
    let stored_content = if let Some(ref key) = enc_key {
        let prepared = encryption::prepare_for_storage(content, key)?;
        prepared.content
    } else {
        content.to_string()
    };

    let id = uuid::Uuid::new_v4().to_string();

    let mem = EpisodicMemory {
        id,
        content: TieredContent::from_text(&stored_content),
        outcome: None,
        category: category.to_string(),
        importance: 0.4, // auto-captured starts lower
        agent_id: agent_id.unwrap_or("default").to_string(),
        session_id: session_id.unwrap_or("unknown").to_string(),
        source: MemorySource::AutoCapture,
        consolidation_state: ConsolidationState::Fresh,
        strength: 1.0,
        scope: MemoryScope {
            global: false,
            agent_id: agent_id.map(|s| s.to_string()),
            channel: channel.map(|s| s.to_string()),
            channel_user_id: channel_user_id.map(|s| s.to_string()),
            ..Default::default()
        },
        embedding: None,
        embedding_model: None,
        negative_contexts: vec![],
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        last_accessed_at: None,
        access_count: 0,
    };

    super::graph::store_episodic_dedup(store, mem, embedding_client, None).await
}

// ═════════════════════════════════════════════════════════════════════════════
// Search (bridge for memory_search tool)
// ═════════════════════════════════════════════════════════════════════════════

/// A simplified search result for tool output and backward compatibility.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: String,
    pub content: String,
    pub category: String,
    pub memory_type: String,
    pub score: f64,
}

/// Search Engram memories. Equivalent to the old `memory::search_memories`
/// but uses BM25+vector+graph fusion.
pub async fn search(
    store: &SessionStore,
    query: &str,
    limit: usize,
    _threshold: f64,
    embedding_client: Option<&EmbeddingClient>,
    agent_id: Option<&str>,
) -> EngineResult<Vec<SearchResult>> {
    // §10.15 FTS5 query sanitization
    let sanitized_query = encryption::sanitize_fts5_query(query);
    if sanitized_query.is_empty() {
        return Ok(vec![]);
    }

    let scope = MemoryScope {
        global: agent_id.is_none(),
        agent_id: agent_id.map(|s| s.to_string()),
        ..Default::default()
    };

    let config = MemorySearchConfig::default();

    let recall_result = super::graph::search(
        store,
        &sanitized_query,
        &scope,
        &config,
        embedding_client,
        limit * 100,
        None, // No momentum blending from bridge (callers with WorkingMemory can pass it)
    )
    .await?;

    // §10.5 Decrypt encrypted content on retrieval
    let enc_key = encryption::get_memory_encryption_key().ok();

    let mapped: Vec<SearchResult> = recall_result
        .memories
        .into_iter()
        .take(limit)
        .map(|r| {
            // Decrypt if encrypted
            let content = if let Some(ref key) = enc_key {
                encryption::decrypt_memory_content(&r.content, key)
                    .unwrap_or_else(|_| r.content.clone())
            } else {
                r.content
            };
            // §10.14 Prompt injection scanning on recalled content
            let content = encryption::sanitize_recalled_memory(&content);

            SearchResult {
                id: r.memory_id,
                content,
                category: r.category,
                memory_type: r.memory_type.to_string(),
                score: r.trust_score.composite() as f64,
            }
        })
        .collect();

    // §10.12 Log redaction
    info!(
        "[engram:bridge] Search '{}' → {} results",
        encryption::safe_log_preview(query, 50),
        mapped.len()
    );

    Ok(mapped)
}

// ═════════════════════════════════════════════════════════════════════════════
// Stats
// ═════════════════════════════════════════════════════════════════════════════

/// Get memory statistics from Engram.
pub fn stats(store: &SessionStore) -> EngineResult<super::graph::EngramStats> {
    super::graph::memory_stats(store)
}

// ═════════════════════════════════════════════════════════════════════════════
// Maintenance (for background timer)
// ═════════════════════════════════════════════════════════════════════════════

/// Run all background maintenance tasks.
/// Call this periodically (e.g. every 5 minutes).
pub async fn run_maintenance(
    store: &SessionStore,
    embedding_client: Option<&EmbeddingClient>,
    half_life_days: f32,
    gc_importance_threshold: i32,
) -> EngineResult<MaintenanceReport> {
    // 1. Consolidation
    let consolidation =
        super::consolidation::run_consolidation(store, embedding_client, None).await?;

    // 2. Decay
    let decayed = super::graph::apply_decay(store, half_life_days)?;

    // 3. Garbage collection
    let gc_count = super::graph::garbage_collect(store, gc_importance_threshold, 100)?;

    // 4. Self-healing gap injection (§4.5)
    // Convert detected knowledge gaps into natural-language prompts
    // that can be injected into working memory as clarifying questions.
    let gap_prompts: Vec<String> = consolidation
        .gaps
        .iter()
        .map(|gap| {
            use super::consolidation::GapKind;
            match &gap.kind {
                GapKind::StaleHighUse => {
                    format!("[Memory Gap] Frequently used knowledge may be outdated: {}. Consider verifying.", gap.description)
                }
                GapKind::UnresolvedContradiction => {
                    format!("[Memory Conflict] Contradictory facts detected: {}. Clarification needed.", gap.description)
                }
                GapKind::IncompleteSchema => {
                    format!("[Incomplete Knowledge] Only partial information exists: {}. More context would help.", gap.description)
                }
            }
        })
        .collect();

    let report = MaintenanceReport {
        consolidation,
        memories_decayed: decayed,
        memories_gc: gc_count,
        gap_prompts,
    };

    info!(
        "[engram:maintenance] consolidation: {} triples, decay: {}, gc: {}",
        report.consolidation.triples_created, report.memories_decayed, report.memories_gc,
    );

    Ok(report)
}

/// Summary of a maintenance cycle.
#[derive(Debug, Clone)]
pub struct MaintenanceReport {
    pub consolidation: super::consolidation::ConsolidationReport,
    pub memories_decayed: usize,
    pub memories_gc: usize,
    /// Knowledge gaps detected during consolidation, formatted for working memory injection.
    pub gap_prompts: Vec<String>,
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_result_format() {
        let result = SearchResult {
            id: "test-123".to_string(),
            content: "User prefers dark mode".to_string(),
            category: "preference".to_string(),
            memory_type: "Episodic".to_string(),
            score: 0.85,
        };
        assert_eq!(result.memory_type, "Episodic");
        assert!(result.score > 0.5);
    }
}
