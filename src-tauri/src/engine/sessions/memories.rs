use rusqlite::params;
use crate::engine::types::{Memory, MemoryStats};
use super::SessionStore;
use super::embedding::{bytes_to_f32_vec, cosine_similarity};
use crate::atoms::error::EngineResult;

impl Memory {
    /// Map a row with columns (id, content, category, importance, created_at, agent_id) → Memory.
    /// Used by search_memories_keyword, list_memories, list_memories_without_embeddings.
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        let importance: i32 = row.get(3)?;
        let agent_id: String = row.get::<_, String>(5).unwrap_or_default();
        Ok(Memory {
            id: row.get(0)?,
            content: row.get(1)?,
            category: row.get(2)?,
            importance: importance as u8,
            created_at: row.get(4)?,
            score: None,
            agent_id: if agent_id.is_empty() { None } else { Some(agent_id) },
        })
    }
}

impl SessionStore {
    // ── Memory CRUD ────────────────────────────────────────────────────

    pub fn store_memory(&self, id: &str, content: &str, category: &str, importance: u8, embedding: Option<&[u8]>, agent_id: Option<&str>) -> EngineResult<()> {
        let conn = self.conn.lock();
        let aid = agent_id.unwrap_or("");
        conn.execute(
            "INSERT OR REPLACE INTO memories (id, content, category, importance, embedding, agent_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, content, category, importance as i32, embedding, aid],
        )?;

        // Sync FTS5 index
        conn.execute(
            "INSERT OR REPLACE INTO memories_fts (id, content, category, agent_id) VALUES (?1, ?2, ?3, ?4)",
            params![id, content, category, aid],
        ).ok(); // Best-effort FTS sync
        Ok(())
    }

    pub fn delete_memory(&self, id: &str) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM memories WHERE id = ?1", params![id])?;
        // Sync FTS5 index
        conn.execute("DELETE FROM memories_fts WHERE id = ?1", params![id]).ok();
        Ok(())
    }

    pub fn memory_stats(&self) -> EngineResult<MemoryStats> {
        let conn = self.conn.lock();

        let total: i64 = conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))?;

        let has_embeddings: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM memories WHERE embedding IS NOT NULL", [], |r| r.get(0)
        ).unwrap_or(false);

        let mut stmt = conn.prepare(
            "SELECT category, COUNT(*) FROM memories GROUP BY category ORDER BY COUNT(*) DESC"
        )?;

        let categories: Vec<(String, i64)> = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

        Ok(MemoryStats { total_memories: total, categories, has_embeddings })
    }

    /// Search memories by cosine similarity against a query embedding.
    /// Falls back to keyword search if no embeddings are stored.
    pub fn search_memories_by_embedding(&self, query_embedding: &[f32], limit: usize, threshold: f64, agent_id: Option<&str>) -> EngineResult<Vec<Memory>> {
        let conn = self.conn.lock();

        let mut stmt = conn.prepare(
            "SELECT id, content, category, importance, embedding, created_at, agent_id FROM memories WHERE embedding IS NOT NULL"
        )?;

        let mut scored: Vec<(Memory, f64)> = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let content: String = row.get(1)?;
            let category: String = row.get(2)?;
            let importance: i32 = row.get(3)?;
            let embedding_blob: Vec<u8> = row.get(4)?;
            let created_at: String = row.get(5)?;
            let mem_agent_id: String = row.get::<_, String>(6).unwrap_or_default();
            Ok((id, content, category, importance as u8, embedding_blob, created_at, mem_agent_id))
        })?
        .filter_map(|r| r.ok())
        .filter_map(|(id, content, category, importance, blob, created_at, mem_agent_id)| {
            // Filter by agent_id if specified
            if let Some(aid) = agent_id {
                if !mem_agent_id.is_empty() && mem_agent_id != aid {
                    return None;
                }
            }
            let stored_emb = bytes_to_f32_vec(&blob);
            let score = cosine_similarity(query_embedding, &stored_emb);
            if score >= threshold {
                Some((Memory { id, content, category, importance, created_at, score: Some(score), agent_id: if mem_agent_id.is_empty() { None } else { Some(mem_agent_id) } }, score))
            } else {
                None
            }
        })
        .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        Ok(scored.into_iter().map(|(m, _)| m).collect())
    }

    /// BM25 full-text search via FTS5 — much better than LIKE keyword search.
    pub fn search_memories_bm25(&self, query: &str, limit: usize, agent_id: Option<&str>) -> EngineResult<Vec<Memory>> {
        let conn = self.conn.lock();

        // FTS5 match query — escape special characters
        let fts_query = query
            .replace('"', "\"\"")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" OR ");

        let sql = if let Some(aid) = agent_id {
            // Filter: memories with matching agent_id OR no agent_id (shared)
            let mut stmt = conn.prepare(
                "SELECT f.id, f.content, f.category, f.agent_id, rank,
                        m.importance, m.created_at
                 FROM memories_fts f
                 JOIN memories m ON m.id = f.id
                 WHERE memories_fts MATCH ?1
                   AND (f.agent_id = '' OR f.agent_id = ?2)
                 ORDER BY rank
                 LIMIT ?3"
            )?;

            let memories: Vec<Memory> = stmt.query_map(params![fts_query, aid, limit as i64], |row| {
                let bm25_rank: f64 = row.get(4)?;
                Ok(Memory {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    category: row.get(2)?,
                    importance: { let i: i32 = row.get(5)?; i as u8 },
                    created_at: row.get(6)?,
                    score: Some(-bm25_rank), // FTS5 rank is negative (lower=better), negate for consistency
                    agent_id: { let a: String = row.get(3)?; if a.is_empty() { None } else { Some(a) } },
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
            return Ok(memories);
        } else {
            "SELECT f.id, f.content, f.category, f.agent_id, rank,
                    m.importance, m.created_at
             FROM memories_fts f
             JOIN memories m ON m.id = f.id
             WHERE memories_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2"
        };

        let mut stmt = conn.prepare(sql)?;
        let memories: Vec<Memory> = stmt.query_map(params![fts_query, limit as i64], |row| {
            let bm25_rank: f64 = row.get(4)?;
            Ok(Memory {
                id: row.get(0)?,
                content: row.get(1)?,
                category: row.get(2)?,
                importance: { let i: i32 = row.get(5)?; i as u8 },
                created_at: row.get(6)?,
                score: Some(-bm25_rank),
                agent_id: { let a: String = row.get(3)?; if a.is_empty() { None } else { Some(a) } },
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

        Ok(memories)
    }

    /// Keyword-based fallback search (no embeddings needed).
    pub fn search_memories_keyword(&self, query: &str, limit: usize) -> EngineResult<Vec<Memory>> {
        let conn = self.conn.lock();

        let pattern = format!("%{}%", query.to_lowercase());
        let mut stmt = conn.prepare(
            "SELECT id, content, category, importance, created_at, agent_id FROM memories
             WHERE LOWER(content) LIKE ?1
             ORDER BY importance DESC, created_at DESC
             LIMIT ?2"
        )?;

        let memories = stmt.query_map(params![pattern, limit as i64], Memory::from_row)?
        .filter_map(|r| r.ok())
        .collect();

        Ok(memories)
    }

    /// Get all memories (for export / listing), newest first.
    pub fn list_memories(&self, limit: usize) -> EngineResult<Vec<Memory>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, content, category, importance, created_at, agent_id FROM memories
             ORDER BY created_at DESC LIMIT ?1"
        )?;

        let memories = stmt.query_map(params![limit as i64], Memory::from_row)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(memories)
    }

    /// List memories that have no embedding vector (for backfill).
    pub fn list_memories_without_embeddings(&self, limit: usize) -> EngineResult<Vec<Memory>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, content, category, importance, created_at, agent_id FROM memories
             WHERE embedding IS NULL
             ORDER BY created_at DESC LIMIT ?1"
        )?;

        let memories = stmt.query_map(params![limit as i64], Memory::from_row)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(memories)
    }

    /// Update the embedding for an existing memory (used by backfill).
    pub fn update_memory_embedding(&self, id: &str, embedding: &[u8]) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE memories SET embedding = ?2 WHERE id = ?1",
            params![id, embedding],
        )?;
        Ok(())
    }

    /// Get memories created today — lightweight daily context injection.
    /// Returns a compact summary string (max 10 entries, highest importance first).
    pub fn get_todays_memories(&self, agent_id: &str) -> EngineResult<Option<String>> {
        let conn = self.conn.lock();
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let today_start = format!("{} 00:00:00", today);
        let mut stmt = conn.prepare(
            "SELECT content, category FROM memories
             WHERE created_at >= ?1 AND (agent_id = ?2 OR agent_id = '')
             ORDER BY importance DESC, created_at DESC
             LIMIT 10"
        )?;

        let rows: Vec<(String, String)> = stmt.query_map(params![today_start, agent_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .filter_map(|r| r.ok())
        .collect();

        if rows.is_empty() {
            return Ok(None);
        }

        let mut lines = Vec::new();
        for (content, category) in &rows {
            // Truncate long entries to keep the block compact
            let short = if content.len() > 200 { format!("{}…", &content[..200]) } else { content.clone() };
            lines.push(format!("- [{}] {}", category, short));
        }
        Ok(Some(format!("## Today's Memory Notes ({})\n{}", today, lines.join("\n"))))
    }
}
