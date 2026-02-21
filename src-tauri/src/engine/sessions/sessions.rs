use rusqlite::params;
use log::info;
use crate::engine::types::Session;
use super::SessionStore;

impl SessionStore {
    // ── Session CRUD ───────────────────────────────────────────────────

    pub fn create_session(&self, id: &str, model: &str, system_prompt: Option<&str>, agent_id: Option<&str>) -> Result<Session, String> {
        let conn = self.conn.lock();

        conn.execute(
            "INSERT INTO sessions (id, model, system_prompt, agent_id) VALUES (?1, ?2, ?3, ?4)",
            params![id, model, system_prompt, agent_id],
        ).map_err(|e| format!("Failed to create session: {}", e))?;

        Ok(Session {
            id: id.to_string(),
            label: None,
            model: model.to_string(),
            system_prompt: system_prompt.map(|s| s.to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            message_count: 0,
            agent_id: agent_id.map(|s| s.to_string()),
        })
    }

    pub fn list_sessions(&self, limit: i64) -> Result<Vec<Session>, String> {
        self.list_sessions_filtered(limit, None)
    }

    /// List sessions, optionally filtered by agent_id.
    pub fn list_sessions_filtered(&self, limit: i64, agent_id: Option<&str>) -> Result<Vec<Session>, String> {
        let conn = self.conn.lock();

        let (sql, params_vec): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(aid) = agent_id {
            (
                "SELECT id, label, model, system_prompt, created_at, updated_at, message_count, agent_id \
                 FROM sessions WHERE agent_id = ?1 ORDER BY updated_at DESC LIMIT ?2".to_string(),
                vec![Box::new(aid.to_string()) as Box<dyn rusqlite::types::ToSql>, Box::new(limit)],
            )
        } else {
            (
                "SELECT id, label, model, system_prompt, created_at, updated_at, message_count, agent_id \
                 FROM sessions ORDER BY updated_at DESC LIMIT ?1".to_string(),
                vec![Box::new(limit) as Box<dyn rusqlite::types::ToSql>],
            )
        };

        let mut stmt = conn.prepare(&sql).map_err(|e| format!("Prepare error: {}", e))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();

        let sessions = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(Session {
                id: row.get(0)?,
                label: row.get(1)?,
                model: row.get(2)?,
                system_prompt: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                message_count: row.get(6)?,
                agent_id: row.get(7)?,
            })
        }).map_err(|e| format!("Query error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(sessions)
    }

    pub fn get_session(&self, id: &str) -> Result<Option<Session>, String> {
        let conn = self.conn.lock();

        let result = conn.query_row(
            "SELECT id, label, model, system_prompt, created_at, updated_at, message_count, agent_id
             FROM sessions WHERE id = ?1",
            params![id],
            |row| {
                Ok(Session {
                    id: row.get(0)?,
                    label: row.get(1)?,
                    model: row.get(2)?,
                    system_prompt: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                    message_count: row.get(6)?,
                    agent_id: row.get(7)?,
                })
            },
        );

        match result {
            Ok(session) => Ok(Some(session)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("Query error: {}", e)),
        }
    }

    pub fn rename_session(&self, id: &str, label: &str) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE sessions SET label = ?1, updated_at = datetime('now') WHERE id = ?2",
            params![label, id],
        ).map_err(|e| format!("Update error: {}", e))?;
        Ok(())
    }

    pub fn delete_session(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM messages WHERE session_id = ?1", params![id])
            .map_err(|e| format!("Delete messages error: {}", e))?;
        conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])
            .map_err(|e| format!("Delete session error: {}", e))?;
        Ok(())
    }

    /// Clear all messages for a session but keep the session itself.
    pub fn clear_messages(&self, session_id: &str) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM messages WHERE session_id = ?1", params![session_id])
            .map_err(|e| format!("Delete messages error: {}", e))?;
        conn.execute(
            "UPDATE sessions SET message_count = 0, updated_at = datetime('now') WHERE id = ?1",
            params![session_id],
        ).map_err(|e| format!("Update session error: {}", e))?;
        info!("[engine] Cleared all messages for session {}", session_id);
        Ok(())
    }

    /// Bulk-delete sessions with 0 messages that are older than `max_age_secs`.
    /// Skips the `exclude_id` session (the user's current session).
    /// Returns the number of sessions deleted.
    pub fn cleanup_empty_sessions(&self, max_age_secs: i64, exclude_id: Option<&str>) -> Result<usize, String> {
        let conn = self.conn.lock();
        let deleted = if let Some(eid) = exclude_id {
            conn.execute(
                "DELETE FROM sessions WHERE message_count = 0 \
                 AND updated_at < datetime('now', ?1) \
                 AND id != ?2",
                params![format!("-{} seconds", max_age_secs), eid],
            )
        } else {
            conn.execute(
                "DELETE FROM sessions WHERE message_count = 0 \
                 AND updated_at < datetime('now', ?1)",
                params![format!("-{} seconds", max_age_secs)],
            )
        }.map_err(|e| format!("Cleanup error: {}", e))?;

        if deleted > 0 {
            info!("[engine] Cleaned up {} empty session(s) older than {}s", deleted, max_age_secs);
        }
        Ok(deleted)
    }

    /// Prune a session's message history, keeping only the most recent `keep`
    /// messages.  Used by the cron heartbeat to prevent context accumulation
    /// across recurring task runs — the #1 cause of runaway token costs.
    ///
    /// Returns the number of messages deleted.
    pub fn prune_session_messages(&self, session_id: &str, keep: i64) -> Result<usize, String> {
        let conn = self.conn.lock();

        // Count current messages
        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE session_id = ?1",
            params![session_id],
            |r| r.get(0),
        ).map_err(|e| format!("Count error: {}", e))?;

        if total <= keep {
            return Ok(0);
        }

        // Delete oldest messages, keeping the most recent `keep`
        let deleted = conn.execute(
            "DELETE FROM messages WHERE session_id = ?1 AND id NOT IN (
                SELECT id FROM messages WHERE session_id = ?1
                ORDER BY created_at DESC LIMIT ?2
            )",
            params![session_id, keep],
        ).map_err(|e| format!("Prune error: {}", e))?;

        // Update session message count
        conn.execute(
            "UPDATE sessions SET
                message_count = (SELECT COUNT(*) FROM messages WHERE session_id = ?1),
                updated_at = datetime('now')
             WHERE id = ?1",
            params![session_id],
        ).map_err(|e| format!("Update session error: {}", e))?;

        if deleted > 0 {
            info!("[engine] Pruned {} old messages from session {} (kept {})", deleted, session_id, keep);
        }

        Ok(deleted)
    }
}
