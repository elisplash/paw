// ── Engram: SessionStore DB Layer ────────────────────────────────────────────
//
// Low-level CRUD operations for the Engram three-tier memory system.
// All methods follow the existing pattern: &self, lock conn, rusqlite params.
//
// Schema lives in engine/engram/schema.rs. Column names here MUST match that schema.

use super::embedding::{bytes_to_f32_vec, cosine_similarity, f32_vec_to_bytes};
use super::SessionStore;
use crate::atoms::engram_types::{
    AuditEntry, AuditOperation, ConsolidationState, EdgeType, EpisodicMemory, MemoryEdge,
    MemoryScope, MemorySource, ProceduralMemory, ProceduralStep, SemanticMemory, TieredContent,
    TrustScore, WorkingMemorySnapshot,
};
use crate::atoms::error::EngineResult;
use rusqlite::params;

// ═════════════════════════════════════════════════════════════════════════════
// Episodic Memories
// ═════════════════════════════════════════════════════════════════════════════

impl SessionStore {
    /// Store a new episodic memory. Upserts on id collision.
    pub fn engram_store_episodic(&self, mem: &EpisodicMemory) -> EngineResult<()> {
        let conn = self.conn.lock();
        let source_str = format!("{:?}", mem.source);
        let consolidation_str = match mem.consolidation_state {
            ConsolidationState::Fresh => "raw",
            ConsolidationState::Consolidated => "consolidated",
            ConsolidationState::Archived => "archived",
        };
        let embedding_bytes = mem.embedding.as_ref().map(|v| f32_vec_to_bytes(v));

        // Map MemoryScope fields to individual columns
        let scope_global = if mem.scope.global { 1i32 } else { 0 };
        let scope_project_id = mem.scope.project_id.as_deref().unwrap_or("");
        let scope_squad_id = mem.scope.squad_id.as_deref().unwrap_or("");
        let scope_agent_id = mem.scope.agent_id.as_deref().unwrap_or(&mem.agent_id);
        let scope_channel = mem.scope.channel.as_deref().unwrap_or("");
        let scope_channel_user_id = mem.scope.channel_user_id.as_deref().unwrap_or("");

        conn.execute(
            "INSERT OR REPLACE INTO episodic_memories (
                id, content_full, content_summary, content_key_fact, content_tags,
                category, source, session_id, agent_id,
                scope_global, scope_project_id, scope_squad_id, scope_agent_id,
                scope_channel, scope_channel_user_id,
                trust_source, trust_consistency, trust_recency, trust_user_feedback,
                consolidation_state, importance,
                embedding, embedding_model,
                access_count
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, ?7, ?8, ?9,
                ?10, ?11, ?12, ?13,
                ?14, ?15,
                ?16, ?17, ?18, ?19,
                ?20, ?21,
                ?22, ?23,
                ?24
            )",
            params![
                mem.id,
                mem.content.full,
                mem.content.summary,
                mem.content.key_fact,
                mem.content.tags,
                mem.category,
                source_str,
                mem.session_id,
                mem.agent_id,
                scope_global,
                scope_project_id,
                scope_squad_id,
                scope_agent_id,
                scope_channel,
                scope_channel_user_id,
                0.5_f32,
                0.5_f32,
                1.0_f32,
                0.5_f32,
                consolidation_str,
                mem.importance as i32,
                embedding_bytes,
                mem.embedding_model,
                mem.access_count as i32,
            ],
        )?;

        Ok(())
    }

    /// Get an episodic memory by ID.
    pub fn engram_get_episodic(&self, id: &str) -> EngineResult<Option<EpisodicMemory>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, content_full, content_summary, content_key_fact, content_tags,
                    category, importance, agent_id, session_id, source,
                    consolidation_state,
                    scope_global, scope_project_id, scope_squad_id, scope_agent_id,
                    scope_channel, scope_channel_user_id,
                    embedding, embedding_model,
                    created_at, last_accessed_at, access_count
             FROM episodic_memories WHERE id = ?1",
        )?;

        let result = stmt
            .query_row(params![id], |row| Self::episodic_from_row(row))
            .optional()?;

        Ok(result)
    }

    /// Delete an episodic memory by ID.
    pub fn engram_delete_episodic(&self, id: &str) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM episodic_memories WHERE id = ?1", params![id])?;
        conn.execute(
            "DELETE FROM memory_edges WHERE source_id = ?1 OR target_id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Update trust scores for an episodic memory.
    pub fn engram_update_trust(&self, id: &str, trust: &TrustScore) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE episodic_memories
             SET trust_source = ?2, trust_consistency = ?3,
                 trust_recency = ?4, trust_user_feedback = ?5
             WHERE id = ?1",
            params![
                id,
                trust.relevance,
                trust.accuracy,
                trust.freshness,
                trust.utility
            ],
        )?;
        Ok(())
    }

    /// Record an access — bump access_count and last_accessed_at.
    pub fn engram_record_access(&self, id: &str, importance_boost: f32) -> EngineResult<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        conn.execute(
            "UPDATE episodic_memories
             SET access_count = access_count + 1,
                 last_accessed_at = ?2,
                 importance = MIN(10, importance + ?3)
             WHERE id = ?1",
            params![id, now, importance_boost as i32],
        )?;
        Ok(())
    }

    /// Update the consolidation state.
    pub fn engram_set_consolidation_state(
        &self,
        id: &str,
        state: ConsolidationState,
    ) -> EngineResult<()> {
        let conn = self.conn.lock();
        let state_str = match state {
            ConsolidationState::Fresh => "raw",
            ConsolidationState::Consolidated => "consolidated",
            ConsolidationState::Archived => "archived",
        };
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        conn.execute(
            "UPDATE episodic_memories
             SET consolidation_state = ?2, consolidation_count = consolidation_count + 1,
                 last_consolidated_at = ?3
             WHERE id = ?1",
            params![id, state_str, now],
        )?;
        Ok(())
    }

    /// Set the inferred metadata JSON for an episodic memory (§35.3).
    /// Called during consolidation after metadata inference extracts
    /// technologies, file paths, URLs, language, etc.
    pub fn engram_set_inferred_metadata(&self, id: &str, metadata_json: &str) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE episodic_memories SET inferred_metadata = ?2 WHERE id = ?1",
            params![id, metadata_json],
        )?;
        Ok(())
    }

    /// Update embedding for an episodic memory.
    pub fn engram_update_episodic_embedding(
        &self,
        id: &str,
        embedding: &[f32],
        model: &str,
    ) -> EngineResult<()> {
        let conn = self.conn.lock();
        let bytes = f32_vec_to_bytes(embedding);
        conn.execute(
            "UPDATE episodic_memories SET embedding = ?2, embedding_model = ?3 WHERE id = ?1",
            params![id, bytes, model],
        )?;
        Ok(())
    }

    /// List episodic memories without embeddings (for backfill).
    pub fn engram_list_episodic_without_embeddings(
        &self,
        limit: usize,
    ) -> EngineResult<Vec<EpisodicMemory>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, content_full, content_summary, content_key_fact, content_tags,
                    category, importance, agent_id, session_id, source,
                    consolidation_state,
                    scope_global, scope_project_id, scope_squad_id, scope_agent_id,
                    scope_channel, scope_channel_user_id,
                    embedding, embedding_model,
                    created_at, last_accessed_at, access_count
             FROM episodic_memories
             WHERE embedding IS NULL
             ORDER BY created_at DESC
             LIMIT ?1",
        )?;

        let rows = stmt
            .query_map(params![limit as i64], |row| Self::episodic_from_row(row))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(rows)
    }

    /// BM25 full-text search on episodic memories.
    pub fn engram_search_episodic_bm25(
        &self,
        query: &str,
        scope: &MemoryScope,
        limit: usize,
    ) -> EngineResult<Vec<(EpisodicMemory, f64)>> {
        let conn = self.conn.lock();
        let agent_filter = scope.agent_id.as_deref().unwrap_or("");

        // Use agent-scoped or unscoped SQL
        let sql = if agent_filter.is_empty() || scope.global {
            "SELECT em.id, em.content_full, em.content_summary, em.content_key_fact, em.content_tags,
                    em.category, em.importance, em.agent_id, em.session_id, em.source,
                    em.consolidation_state,
                    em.scope_global, em.scope_project_id, em.scope_squad_id, em.scope_agent_id,
                    em.scope_channel, em.scope_channel_user_id,
                    em.embedding, em.embedding_model,
                    em.created_at, em.last_accessed_at, em.access_count,
                    fts.rank
             FROM episodic_memories em
             JOIN episodic_memories_fts fts ON em.id = fts.id
             WHERE episodic_memories_fts MATCH ?1
             ORDER BY fts.rank
             LIMIT ?2"
        } else {
            "SELECT em.id, em.content_full, em.content_summary, em.content_key_fact, em.content_tags,
                    em.category, em.importance, em.agent_id, em.session_id, em.source,
                    em.consolidation_state,
                    em.scope_global, em.scope_project_id, em.scope_squad_id, em.scope_agent_id,
                    em.scope_channel, em.scope_channel_user_id,
                    em.embedding, em.embedding_model,
                    em.created_at, em.last_accessed_at, em.access_count,
                    fts.rank
             FROM episodic_memories em
             JOIN episodic_memories_fts fts ON em.id = fts.id
             WHERE episodic_memories_fts MATCH ?1
               AND (em.scope_agent_id = ?3 OR em.scope_global = 1 OR em.scope_agent_id = '')
             ORDER BY fts.rank
             LIMIT ?2"
        };

        let mut stmt = conn.prepare(sql)?;
        let rows = if agent_filter.is_empty() || scope.global {
            stmt.query_map(params![query, limit as i64], |row| {
                let mem = Self::episodic_from_row(row)?;
                let rank: f64 = row.get(22)?;
                Ok((mem, -rank))
            })?
            .filter_map(|r| r.ok())
            .collect()
        } else {
            stmt.query_map(params![query, limit as i64, agent_filter], |row| {
                let mem = Self::episodic_from_row(row)?;
                let rank: f64 = row.get(22)?;
                Ok((mem, -rank))
            })?
            .filter_map(|r| r.ok())
            .collect()
        };

        Ok(rows)
    }

    /// Vector similarity search on episodic memories.
    pub fn engram_search_episodic_vector(
        &self,
        query_embedding: &[f32],
        scope: &MemoryScope,
        limit: usize,
        threshold: f64,
    ) -> EngineResult<Vec<(EpisodicMemory, f64)>> {
        let conn = self.conn.lock();

        let mut stmt = conn.prepare(
            "SELECT id, content_full, content_summary, content_key_fact, content_tags,
                    category, importance, agent_id, session_id, source,
                    consolidation_state,
                    scope_global, scope_project_id, scope_squad_id, scope_agent_id,
                    scope_channel, scope_channel_user_id,
                    embedding, embedding_model,
                    created_at, last_accessed_at, access_count
             FROM episodic_memories
             WHERE embedding IS NOT NULL",
        )?;

        let mut scored: Vec<(EpisodicMemory, f64)> = stmt
            .query_map([], |row| Self::episodic_from_row(row))?
            .filter_map(|r| r.ok())
            .filter(|mem| scope_matches(scope, &mem.scope))
            .filter_map(|mem| {
                mem.embedding.clone().as_ref().map(|emb| {
                    let sim = cosine_similarity(emb, query_embedding);
                    (mem, sim)
                })
            })
            .filter(|(_, sim)| *sim >= threshold)
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        Ok(scored)
    }

    /// List episodic memories eligible for GC.
    pub fn engram_list_gc_candidates(
        &self,
        importance_threshold: i32,
        limit: usize,
    ) -> EngineResult<Vec<String>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id FROM episodic_memories
             WHERE importance <= ?1 AND consolidation_state != 'archived'
             ORDER BY importance ASC, last_accessed_at ASC
             LIMIT ?2",
        )?;
        let ids = stmt
            .query_map(params![importance_threshold, limit as i64], |row| {
                row.get::<_, String>(0)
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(ids)
    }

    /// List episodic memories ready for consolidation.
    pub fn engram_list_consolidation_candidates(
        &self,
        older_than_secs: u64,
        limit: usize,
    ) -> EngineResult<Vec<EpisodicMemory>> {
        let conn = self.conn.lock();
        let cutoff = chrono::Utc::now() - chrono::Duration::seconds(older_than_secs as i64);
        let cutoff_str = cutoff.format("%Y-%m-%dT%H:%M:%SZ").to_string();

        let mut stmt = conn.prepare(
            "SELECT id, content_full, content_summary, content_key_fact, content_tags,
                    category, importance, agent_id, session_id, source,
                    consolidation_state,
                    scope_global, scope_project_id, scope_squad_id, scope_agent_id,
                    scope_channel, scope_channel_user_id,
                    embedding, embedding_model,
                    created_at, last_accessed_at, access_count
             FROM episodic_memories
             WHERE consolidation_state = 'raw'
               AND created_at < ?1
             ORDER BY created_at ASC
             LIMIT ?2",
        )?;

        let rows = stmt
            .query_map(params![cutoff_str, limit as i64], |row| {
                Self::episodic_from_row(row)
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(rows)
    }

    /// Count episodic memories.
    pub fn engram_count_episodic(&self, agent_id: Option<&str>) -> EngineResult<usize> {
        let conn = self.conn.lock();
        let count: i64 = if let Some(aid) = agent_id {
            conn.query_row(
                "SELECT COUNT(*) FROM episodic_memories WHERE scope_agent_id = ?1",
                params![aid],
                |r| r.get(0),
            )?
        } else {
            conn.query_row("SELECT COUNT(*) FROM episodic_memories", [], |r| r.get(0))?
        };
        Ok(count as usize)
    }

    /// Update episodic memory content and optionally re-embed.
    pub fn engram_update_episodic_content(
        &self,
        id: &str,
        new_content: &str,
        new_embedding: Option<&[f32]>,
    ) -> EngineResult<bool> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let embedding_bytes = new_embedding.map(|v| f32_vec_to_bytes(v));

        let rows = if let Some(ref emb) = embedding_bytes {
            conn.execute(
                "UPDATE episodic_memories
                 SET content_full = ?2, embedding = ?3, last_accessed_at = ?4
                 WHERE id = ?1",
                params![id, new_content, emb, now],
            )?
        } else {
            conn.execute(
                "UPDATE episodic_memories
                 SET content_full = ?2, last_accessed_at = ?3
                 WHERE id = ?1",
                params![id, new_content, now],
            )?
        };
        Ok(rows > 0)
    }

    /// List episodic memories with scope and optional category filtering.
    pub fn engram_list_episodic(
        &self,
        scope: &MemoryScope,
        category: Option<&str>,
        limit: usize,
    ) -> EngineResult<Vec<EpisodicMemory>> {
        let conn = self.conn.lock();
        let agent_filter = scope.agent_id.as_deref().unwrap_or("");

        let mut sql = String::from(
            "SELECT id, content_full, content_summary, content_key_fact, content_tags,
                    category, importance, agent_id, session_id, source,
                    consolidation_state,
                    scope_global, scope_project_id, scope_squad_id, scope_agent_id,
                    scope_channel, scope_channel_user_id,
                    embedding, embedding_model,
                    created_at, last_accessed_at, access_count
             FROM episodic_memories WHERE 1=1",
        );

        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if !agent_filter.is_empty() && !scope.global {
            sql.push_str(" AND (scope_agent_id = ? OR scope_global = 1 OR scope_agent_id = '')");
            param_values.push(Box::new(agent_filter.to_string()));
        }

        if let Some(cat) = category {
            sql.push_str(" AND category = ?");
            param_values.push(Box::new(cat.to_string()));
        }

        sql.push_str(" ORDER BY created_at DESC LIMIT ?");
        param_values.push(Box::new(limit as i64));

        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();
        let rows = stmt
            .query_map(param_refs.as_slice(), |row| Self::episodic_from_row(row))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(rows)
    }

    // ── Episodic row mapper (column order must match SELECTs above) ──

    fn episodic_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<EpisodicMemory> {
        let embedding_bytes: Option<Vec<u8>> = row.get(17)?;
        let embedding = embedding_bytes.map(|b| bytes_to_f32_vec(&b));
        let scope_global: i32 = row.get(11)?;
        let importance_raw: i32 = row.get::<_, i32>(6).unwrap_or(5);

        Ok(EpisodicMemory {
            id: row.get(0)?,
            content: TieredContent {
                full: row.get(1)?,
                summary: row.get(2)?,
                key_fact: row.get(3)?,
                tags: row.get(4)?,
            },
            outcome: None,
            category: row.get(5)?,
            importance: importance_raw as f32,
            agent_id: row.get(7)?,
            session_id: row.get(8)?,
            source: MemorySource::default(),
            consolidation_state: match row.get::<_, String>(10).unwrap_or_default().as_str() {
                "consolidated" => ConsolidationState::Consolidated,
                "archived" => ConsolidationState::Archived,
                _ => ConsolidationState::Fresh,
            },
            strength: importance_raw as f32 / 10.0,
            scope: MemoryScope {
                global: scope_global != 0,
                project_id: non_empty_opt(row.get::<_, Option<String>>(12)?),
                squad_id: non_empty_opt(row.get::<_, Option<String>>(13)?),
                agent_id: non_empty_opt(row.get::<_, Option<String>>(14)?),
                channel: non_empty_opt(row.get::<_, Option<String>>(15)?),
                channel_user_id: non_empty_opt(row.get::<_, Option<String>>(16)?),
            },
            embedding,
            embedding_model: row.get(18)?,
            negative_contexts: Vec::new(),
            created_at: row.get(19)?,
            last_accessed_at: row.get(20)?,
            access_count: row.get::<_, i32>(21).unwrap_or(0) as u32,
        })
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Semantic Memories
// ═════════════════════════════════════════════════════════════════════════════

impl SessionStore {
    /// Store a new semantic memory (SPO triple).
    pub fn engram_store_semantic(&self, mem: &SemanticMemory) -> EngineResult<()> {
        let conn = self.conn.lock();
        let scope_agent = mem.scope.agent_id.as_deref().unwrap_or("");
        let scope_project = mem.scope.project_id.as_deref().unwrap_or("");
        let scope_channel = mem.scope.channel.as_deref().unwrap_or("");
        let embedding_bytes = mem.embedding.as_ref().map(|v| f32_vec_to_bytes(v));

        conn.execute(
            "INSERT OR REPLACE INTO semantic_memories (
                id, subject, predicate, object,
                confidence, version, supersedes_id,
                scope_agent_id, scope_project_id, scope_channel,
                source, embedding,
                created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                mem.id,
                mem.subject,
                mem.predicate,
                mem.object,
                mem.confidence,
                mem.version as i32,
                mem.contradiction_of,
                scope_agent,
                scope_project,
                scope_channel,
                if mem.is_user_explicit {
                    "user"
                } else {
                    "extraction"
                },
                embedding_bytes,
                mem.created_at,
                mem.updated_at,
            ],
        )?;

        Ok(())
    }

    /// Get a semantic memory by ID.
    pub fn engram_get_semantic(&self, id: &str) -> EngineResult<Option<SemanticMemory>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, subject, predicate, object,
                    confidence, version, supersedes_id,
                    scope_agent_id, scope_project_id, scope_channel,
                    source, embedding,
                    created_at, updated_at
             FROM semantic_memories WHERE id = ?1",
        )?;

        let result = stmt
            .query_row(params![id], |row| Self::semantic_from_row(row))
            .optional()?;

        Ok(result)
    }

    /// Delete a semantic memory.
    pub fn engram_delete_semantic(&self, id: &str) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM semantic_memories WHERE id = ?1", params![id])?;
        conn.execute(
            "DELETE FROM memory_edges WHERE source_id = ?1 OR target_id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// BM25 search on semantic memories.
    pub fn engram_search_semantic_bm25(
        &self,
        query: &str,
        scope: &MemoryScope,
        limit: usize,
    ) -> EngineResult<Vec<(SemanticMemory, f64)>> {
        let conn = self.conn.lock();
        let agent_filter = scope.agent_id.as_deref().unwrap_or("");

        let sql = if agent_filter.is_empty() || scope.global {
            "SELECT sm.id, sm.subject, sm.predicate, sm.object,
                    sm.confidence, sm.version, sm.supersedes_id,
                    sm.scope_agent_id, sm.scope_project_id, sm.scope_channel,
                    sm.source, sm.embedding,
                    sm.created_at, sm.updated_at,
                    fts.rank
             FROM semantic_memories sm
             JOIN semantic_memories_fts fts ON sm.id = fts.id
             WHERE semantic_memories_fts MATCH ?1
             ORDER BY fts.rank
             LIMIT ?2"
        } else {
            "SELECT sm.id, sm.subject, sm.predicate, sm.object,
                    sm.confidence, sm.version, sm.supersedes_id,
                    sm.scope_agent_id, sm.scope_project_id, sm.scope_channel,
                    sm.source, sm.embedding,
                    sm.created_at, sm.updated_at,
                    fts.rank
             FROM semantic_memories sm
             JOIN semantic_memories_fts fts ON sm.id = fts.id
             WHERE semantic_memories_fts MATCH ?1
               AND (sm.scope_agent_id = ?3 OR sm.scope_agent_id = '')
             ORDER BY fts.rank
             LIMIT ?2"
        };

        let mut stmt = conn.prepare(sql)?;
        let rows = if agent_filter.is_empty() || scope.global {
            stmt.query_map(params![query, limit as i64], |row| {
                let mem = Self::semantic_from_row(row)?;
                let rank: f64 = row.get(14)?;
                Ok((mem, -rank))
            })?
            .filter_map(|r| r.ok())
            .collect()
        } else {
            stmt.query_map(params![query, limit as i64, agent_filter], |row| {
                let mem = Self::semantic_from_row(row)?;
                let rank: f64 = row.get(14)?;
                Ok((mem, -rank))
            })?
            .filter_map(|r| r.ok())
            .collect()
        };

        Ok(rows)
    }

    /// Look up semantic triples by subject.
    pub fn engram_lookup_by_subject(
        &self,
        subject: &str,
        scope: &MemoryScope,
    ) -> EngineResult<Vec<SemanticMemory>> {
        let conn = self.conn.lock();
        let agent_filter = scope.agent_id.as_deref().unwrap_or("");

        let sql = if agent_filter.is_empty() {
            "SELECT id, subject, predicate, object,
                    confidence, version, supersedes_id,
                    scope_agent_id, scope_project_id, scope_channel,
                    source, embedding,
                    created_at, updated_at
             FROM semantic_memories WHERE subject = ?1
             ORDER BY confidence DESC"
        } else {
            "SELECT id, subject, predicate, object,
                    confidence, version, supersedes_id,
                    scope_agent_id, scope_project_id, scope_channel,
                    source, embedding,
                    created_at, updated_at
             FROM semantic_memories
             WHERE subject = ?1 AND (scope_agent_id = ?2 OR scope_agent_id = '')
             ORDER BY confidence DESC"
        };

        let mut stmt = conn.prepare(sql)?;
        let rows = if agent_filter.is_empty() {
            stmt.query_map(params![subject], |row| Self::semantic_from_row(row))?
                .filter_map(|r| r.ok())
                .collect()
        } else {
            stmt.query_map(params![subject, agent_filter], |row| {
                Self::semantic_from_row(row)
            })?
            .filter_map(|r| r.ok())
            .collect()
        };

        Ok(rows)
    }

    /// Count semantic memories.
    pub fn engram_count_semantic(&self) -> EngineResult<usize> {
        let conn = self.conn.lock();
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM semantic_memories", [], |r| r.get(0))?;
        Ok(count as usize)
    }

    /// Count procedural memories.
    pub fn engram_count_procedural(&self) -> EngineResult<usize> {
        let conn = self.conn.lock();
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM procedural_memories", [], |r| r.get(0))?;
        Ok(count as usize)
    }

    /// Count graph edges.
    pub fn engram_count_edges(&self) -> EngineResult<usize> {
        let conn = self.conn.lock();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM memory_edges", [], |r| r.get(0))?;
        Ok(count as usize)
    }

    // ── Semantic row mapper ─────────────────────────────────────────

    fn semantic_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SemanticMemory> {
        let embedding_bytes: Option<Vec<u8>> = row.get(11)?;
        let embedding = embedding_bytes.map(|b| bytes_to_f32_vec(&b));
        let source_str: String = row.get::<_, String>(10).unwrap_or_default();

        Ok(SemanticMemory {
            id: row.get(0)?,
            subject: row.get(1)?,
            predicate: row.get(2)?,
            object: row.get(3)?,
            full_text: format!(
                "{} {} {}",
                row.get::<_, String>(1).unwrap_or_default(),
                row.get::<_, String>(2).unwrap_or_default(),
                row.get::<_, String>(3).unwrap_or_default(),
            ),
            category: "general".into(),
            confidence: row.get(4)?,
            is_user_explicit: source_str == "user",
            contradiction_of: row.get(6)?,
            scope: MemoryScope {
                agent_id: non_empty_opt(row.get::<_, Option<String>>(7)?),
                project_id: non_empty_opt(row.get::<_, Option<String>>(8)?),
                channel: non_empty_opt(row.get::<_, Option<String>>(9)?),
                ..Default::default()
            },
            embedding,
            embedding_model: None,
            version: row.get::<_, i32>(5).unwrap_or(1) as u32,
            created_at: row.get(12)?,
            updated_at: row.get(13)?,
        })
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Procedural Memories
// ═════════════════════════════════════════════════════════════════════════════

impl SessionStore {
    /// Store a new procedural memory.
    pub fn engram_store_procedural(&self, mem: &ProceduralMemory) -> EngineResult<()> {
        let conn = self.conn.lock();
        let scope_agent = mem.scope.agent_id.as_deref().unwrap_or("");
        let scope_project = mem.scope.project_id.as_deref().unwrap_or("");
        let steps_json = serde_json::to_string(&mem.steps).unwrap_or_else(|_| "[]".into());

        let success_count = (mem.success_rate * mem.execution_count as f32) as i32;
        let failure_count = mem.execution_count as i32 - success_count;

        conn.execute(
            "INSERT OR REPLACE INTO procedural_memories (
                id, trigger_pattern, steps_json,
                success_count, failure_count,
                scope_agent_id, scope_project_id,
                created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                mem.id,
                mem.trigger,
                steps_json,
                success_count,
                failure_count,
                scope_agent,
                scope_project,
                mem.created_at,
                mem.updated_at,
            ],
        )?;

        Ok(())
    }

    /// Get a procedural memory by ID.
    pub fn engram_get_procedural(&self, id: &str) -> EngineResult<Option<ProceduralMemory>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, trigger_pattern, steps_json, success_count, failure_count,
                    scope_agent_id, scope_project_id,
                    created_at, updated_at
             FROM procedural_memories WHERE id = ?1",
        )?;

        let result = stmt
            .query_row(params![id], |row| Self::procedural_from_row(row))
            .optional()?;

        Ok(result)
    }

    /// Delete a procedural memory.
    pub fn engram_delete_procedural(&self, id: &str) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM procedural_memories WHERE id = ?1", params![id])?;
        conn.execute(
            "DELETE FROM memory_edges WHERE source_id = ?1 OR target_id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Record successful execution.
    pub fn engram_record_procedural_success(&self, id: &str) -> EngineResult<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        conn.execute(
            "UPDATE procedural_memories
             SET success_count = success_count + 1, updated_at = ?2, last_used_at = ?2
             WHERE id = ?1",
            params![id, now],
        )?;
        Ok(())
    }

    /// Record failed execution.
    pub fn engram_record_procedural_failure(&self, id: &str) -> EngineResult<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        conn.execute(
            "UPDATE procedural_memories
             SET failure_count = failure_count + 1, updated_at = ?2, last_used_at = ?2
             WHERE id = ?1",
            params![id, now],
        )?;
        Ok(())
    }

    /// Search procedural memories by trigger.
    pub fn engram_search_procedural(
        &self,
        query: &str,
        scope: &MemoryScope,
        limit: usize,
    ) -> EngineResult<Vec<ProceduralMemory>> {
        let conn = self.conn.lock();
        let agent_filter = scope.agent_id.as_deref().unwrap_or("");
        let like_query = format!("%{}%", query);

        let sql = if agent_filter.is_empty() {
            "SELECT id, trigger_pattern, steps_json, success_count, failure_count,
                    scope_agent_id, scope_project_id, created_at, updated_at
             FROM procedural_memories
             WHERE trigger_pattern LIKE ?1
             ORDER BY (success_count * 1.0 / MAX(success_count + failure_count, 1)) DESC
             LIMIT ?2"
        } else {
            "SELECT id, trigger_pattern, steps_json, success_count, failure_count,
                    scope_agent_id, scope_project_id, created_at, updated_at
             FROM procedural_memories
             WHERE trigger_pattern LIKE ?1
               AND (scope_agent_id = ?3 OR scope_agent_id = '')
             ORDER BY (success_count * 1.0 / MAX(success_count + failure_count, 1)) DESC
             LIMIT ?2"
        };

        let mut stmt = conn.prepare(sql)?;
        let rows = if agent_filter.is_empty() {
            stmt.query_map(params![like_query, limit as i64], |row| {
                Self::procedural_from_row(row)
            })?
            .filter_map(|r| r.ok())
            .collect()
        } else {
            stmt.query_map(params![like_query, limit as i64, agent_filter], |row| {
                Self::procedural_from_row(row)
            })?
            .filter_map(|r| r.ok())
            .collect()
        };

        Ok(rows)
    }

    // ── Procedural row mapper ───────────────────────────────────────

    fn procedural_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProceduralMemory> {
        let steps_json: String = row.get(2)?;
        let steps: Vec<ProceduralStep> = serde_json::from_str(&steps_json).unwrap_or_default();

        let success_count: i32 = row.get(3)?;
        let failure_count: i32 = row.get(4)?;
        let total = success_count + failure_count;
        let success_rate = if total > 0 {
            success_count as f32 / total as f32
        } else {
            0.0
        };

        Ok(ProceduralMemory {
            id: row.get(0)?,
            trigger: row.get(1)?,
            steps,
            success_rate,
            execution_count: total as u32,
            scope: MemoryScope {
                agent_id: non_empty_opt(row.get::<_, Option<String>>(5)?),
                project_id: non_empty_opt(row.get::<_, Option<String>>(6)?),
                ..Default::default()
            },
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Memory Edges (Graph)
// ═════════════════════════════════════════════════════════════════════════════

impl SessionStore {
    /// Add an edge between two memories.
    pub fn engram_add_edge(&self, edge: &MemoryEdge) -> EngineResult<()> {
        let conn = self.conn.lock();
        let edge_type_str = edge.edge_type.to_string();
        let id = format!("{}:{}:{}", edge.source_id, edge.target_id, edge_type_str);
        conn.execute(
            "INSERT OR REPLACE INTO memory_edges (id, source_id, target_id, edge_type, weight, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, edge.source_id, edge.target_id, edge_type_str, edge.weight, edge.created_at],
        )?;
        Ok(())
    }

    /// Remove an edge.
    pub fn engram_remove_edge(
        &self,
        source_id: &str,
        target_id: &str,
        edge_type: &EdgeType,
    ) -> EngineResult<()> {
        let conn = self.conn.lock();
        let edge_type_str = edge_type.to_string();
        conn.execute(
            "DELETE FROM memory_edges WHERE source_id = ?1 AND target_id = ?2 AND edge_type = ?3",
            params![source_id, target_id, edge_type_str],
        )?;
        Ok(())
    }

    /// Get all outgoing edges from a memory.
    pub fn engram_get_edges_from(&self, source_id: &str) -> EngineResult<Vec<MemoryEdge>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT source_id, target_id, edge_type, weight, created_at
             FROM memory_edges WHERE source_id = ?1 ORDER BY weight DESC",
        )?;
        let edges = stmt
            .query_map(params![source_id], |row| Self::edge_from_row(row))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(edges)
    }

    /// Get all incoming edges to a memory.
    pub fn engram_get_edges_to(&self, target_id: &str) -> EngineResult<Vec<MemoryEdge>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT source_id, target_id, edge_type, weight, created_at
             FROM memory_edges WHERE target_id = ?1 ORDER BY weight DESC",
        )?;
        let edges = stmt
            .query_map(params![target_id], |row| Self::edge_from_row(row))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(edges)
    }

    /// Spreading activation: 1-hop neighbors with summed weighted edges.
    pub fn engram_spreading_activation(
        &self,
        seed_ids: &[String],
        min_weight: f32,
    ) -> EngineResult<Vec<(String, f32)>> {
        let conn = self.conn.lock();
        let mut result: std::collections::HashMap<String, f32> = std::collections::HashMap::new();

        for seed in seed_ids {
            let mut stmt = conn.prepare(
                "SELECT source_id, target_id, weight
                 FROM memory_edges
                 WHERE (source_id = ?1 OR target_id = ?1) AND weight >= ?2",
            )?;

            let neighbors: Vec<(String, f32)> = stmt
                .query_map(params![seed, min_weight], |row| {
                    let src: String = row.get(0)?;
                    let tgt: String = row.get(1)?;
                    let w: f32 = row.get(2)?;
                    let neighbor = if src == *seed { tgt } else { src };
                    Ok((neighbor, w))
                })?
                .filter_map(|r| r.ok())
                .collect();

            for (neighbor, weight) in neighbors {
                if !seed_ids.contains(&neighbor) {
                    *result.entry(neighbor).or_insert(0.0) += weight;
                }
            }
        }

        let mut sorted: Vec<(String, f32)> = result.into_iter().collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(sorted)
    }

    // ── Edge row mapper ─────────────────────────────────────────────

    fn edge_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryEdge> {
        let edge_type_str: String = row.get(2)?;
        let edge_type = edge_type_str
            .parse::<EdgeType>()
            .unwrap_or(EdgeType::RelatedTo);

        Ok(MemoryEdge {
            source_id: row.get(0)?,
            target_id: row.get(1)?,
            edge_type,
            weight: row.get(3)?,
            created_at: row.get(4)?,
        })
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Working Memory Snapshots
// ═════════════════════════════════════════════════════════════════════════════

impl SessionStore {
    /// Save a working memory snapshot for an agent.
    pub fn engram_save_snapshot(&self, snapshot: &WorkingMemorySnapshot) -> EngineResult<()> {
        let conn = self.conn.lock();
        let json = serde_json::to_string(snapshot).unwrap_or_else(|_| "{}".into());
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let slot_count = snapshot.slots.len() as i32;
        let total_tokens: i32 = snapshot.slots.iter().map(|s| s.token_cost as i32).sum();

        conn.execute(
            "INSERT OR REPLACE INTO working_memory_snapshots
             (agent_id, snapshot_json, slot_count, total_tokens, saved_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![snapshot.agent_id, json, slot_count, total_tokens, now],
        )?;
        Ok(())
    }

    /// Load the latest working memory snapshot for an agent.
    pub fn engram_load_snapshot(
        &self,
        agent_id: &str,
    ) -> EngineResult<Option<WorkingMemorySnapshot>> {
        let conn = self.conn.lock();
        let mut stmt =
            conn.prepare("SELECT snapshot_json FROM working_memory_snapshots WHERE agent_id = ?1")?;

        let result = stmt
            .query_row(params![agent_id], |row| row.get::<_, String>(0))
            .optional()?;

        match result {
            Some(json) => {
                let snapshot: WorkingMemorySnapshot =
                    serde_json::from_str(&json).unwrap_or_default();
                Ok(Some(snapshot))
            }
            None => Ok(None),
        }
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Audit Log
// ═════════════════════════════════════════════════════════════════════════════

impl SessionStore {
    /// Append to the memory audit log.
    pub fn engram_audit_log(
        &self,
        operation: &str,
        memory_id: &str,
        agent_id: &str,
        session_id: &str,
        detail: Option<&str>,
    ) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO memory_audit_log (operation, memory_id, agent_id, session_id, details_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![operation, memory_id, agent_id, session_id, detail],
        )?;
        Ok(())
    }

    /// Get recent audit entries for a memory.
    pub fn engram_audit_history(
        &self,
        memory_id: &str,
        limit: usize,
    ) -> EngineResult<Vec<AuditEntry>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT operation, memory_id, agent_id, details_json, created_at
             FROM memory_audit_log WHERE memory_id = ?1
             ORDER BY created_at DESC LIMIT ?2",
        )?;

        let entries = stmt
            .query_map(params![memory_id, limit as i64], |row| {
                Ok(AuditEntry {
                    operation: AuditOperation::Store,
                    memory_id: row.get(1)?,
                    actor: row.get(2)?,
                    detail: row.get(3)?,
                    timestamp: row.get(4)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(entries)
    }

    // ── Anti-forensic: Secure Memory Erasure ─────────────────────────────
    //
    // KDBX zeroes inner content before deletion so freed pages contain no
    // recoverable plaintext. SQLite's PRAGMA secure_delete helps but only
    // applies to the B-tree layer. We belt-and-suspenders by overwriting
    // content fields with zeros before DELETE so even pre-secure_delete
    // SQLite builds are protected, and the WAL never contains the original
    // plaintext in the same page as the DELETE marker.

    /// Securely erase an episodic memory: zero all content fields, then delete.
    /// This prevents content recovery via SQLite page forensics or WAL replay.
    pub fn engram_secure_erase_episodic(&self, id: &str) -> EngineResult<()> {
        let conn = self.conn.lock();
        // Phase 1: overwrite all content fields with empty/zero values
        conn.execute(
            "UPDATE episodic_memories SET
                content_full = '', content_summary = NULL, content_key_fact = NULL,
                content_tags = NULL, category = '', embedding = NULL,
                embedding_model = NULL, session_id = NULL, agent_id = ''
             WHERE id = ?1",
            params![id],
        )?;
        // Phase 2: delete the zeroed row and orphan edges
        conn.execute("DELETE FROM episodic_memories WHERE id = ?1", params![id])?;
        conn.execute(
            "DELETE FROM memory_edges WHERE source_id = ?1 OR target_id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Securely erase a semantic memory.
    pub fn engram_secure_erase_semantic(&self, id: &str) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE semantic_memories SET
                subject = '', predicate = '', object = '',
                embedding = NULL, supersedes_id = NULL
             WHERE id = ?1",
            params![id],
        )?;
        conn.execute("DELETE FROM semantic_memories WHERE id = ?1", params![id])?;
        conn.execute(
            "DELETE FROM memory_edges WHERE source_id = ?1 OR target_id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Securely erase a procedural memory.
    pub fn engram_secure_erase_procedural(&self, id: &str) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE procedural_memories SET
                trigger_pattern = '', steps_json = '[]'
             WHERE id = ?1",
            params![id],
        )?;
        conn.execute("DELETE FROM procedural_memories WHERE id = ?1", params![id])?;
        conn.execute(
            "DELETE FROM memory_edges WHERE source_id = ?1 OR target_id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Re-pad the database to the next bucket boundary after bulk deletions.
    /// Call after garbage collection or user-initiated purges.
    pub fn engram_repad(&self) -> EngineResult<()> {
        let conn = self.conn.lock();
        crate::engine::engram::schema::pad_to_bucket(&conn)
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Helpers
// ═════════════════════════════════════════════════════════════════════════════

/// Convert empty strings to None (for scope fields stored as '' in SQLite).
fn non_empty_opt(val: Option<String>) -> Option<String> {
    val.filter(|s| !s.is_empty())
}

/// Check if a memory's scope is visible to the given search scope.
fn scope_matches(search_scope: &MemoryScope, memory_scope: &MemoryScope) -> bool {
    if search_scope.global || memory_scope.global {
        return true;
    }
    if let (Some(ref s), Some(ref m)) = (&search_scope.agent_id, &memory_scope.agent_id) {
        if s == m {
            return true;
        }
    }
    if let (Some(ref s), Some(ref m)) = (&search_scope.project_id, &memory_scope.project_id) {
        if s == m {
            return true;
        }
    }
    false
}

/// Extension trait: query_row returning Option on no rows.
trait OptionalRow<T> {
    fn optional(self) -> rusqlite::Result<Option<T>>;
}

impl<T> OptionalRow<T> for rusqlite::Result<T> {
    fn optional(self) -> rusqlite::Result<Option<T>> {
        match self {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
