// Skill Outputs — CRUD for dashboard widget data persisted by agents.
// Each row maps a (skill_id, agent_id) pair to a JSON blob of structured data
// plus a widget_type that tells the frontend how to render it.

use super::SessionStore;
use crate::atoms::error::EngineResult;
use rusqlite::params;
use serde::{Deserialize, Serialize};

/// A persisted skill output row, returned to the frontend for widget rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillOutput {
    pub id: String,
    pub skill_id: String,
    pub agent_id: String,
    pub widget_type: String,
    pub title: String,
    /// JSON-encoded structured data.
    pub data: String,
    pub created_at: String,
    pub updated_at: String,
}

impl SessionStore {
    /// Upsert a skill output — one row per (skill_id, agent_id).
    /// If the row already exists, update widget_type, title, data, and updated_at.
    pub fn upsert_skill_output(
        &self,
        id: &str,
        skill_id: &str,
        agent_id: &str,
        widget_type: &str,
        title: &str,
        data: &str,
    ) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO skill_outputs (id, skill_id, agent_id, widget_type, title, data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                widget_type = excluded.widget_type,
                title = excluded.title,
                data = excluded.data,
                updated_at = datetime('now')",
            params![id, skill_id, agent_id, widget_type, title, data],
        )?;
        Ok(())
    }

    /// List all skill outputs, optionally filtered by skill_id and/or agent_id.
    pub fn list_skill_outputs(
        &self,
        skill_id: Option<&str>,
        agent_id: Option<&str>,
    ) -> EngineResult<Vec<SkillOutput>> {
        let conn = self.conn.lock();

        let (sql, bind): (String, Vec<String>) = match (skill_id, agent_id) {
            (Some(sid), Some(aid)) => (
                "SELECT id, skill_id, agent_id, widget_type, title, data, created_at, updated_at
                 FROM skill_outputs
                 WHERE skill_id = ?1 AND agent_id = ?2
                 ORDER BY updated_at DESC"
                    .into(),
                vec![sid.to_string(), aid.to_string()],
            ),
            (Some(sid), None) => (
                "SELECT id, skill_id, agent_id, widget_type, title, data, created_at, updated_at
                 FROM skill_outputs
                 WHERE skill_id = ?1
                 ORDER BY updated_at DESC"
                    .into(),
                vec![sid.to_string()],
            ),
            (None, Some(aid)) => (
                "SELECT id, skill_id, agent_id, widget_type, title, data, created_at, updated_at
                 FROM skill_outputs
                 WHERE agent_id = ?1
                 ORDER BY updated_at DESC"
                    .into(),
                vec![aid.to_string()],
            ),
            (None, None) => (
                "SELECT id, skill_id, agent_id, widget_type, title, data, created_at, updated_at
                 FROM skill_outputs
                 ORDER BY updated_at DESC"
                    .into(),
                vec![],
            ),
        };

        let mut stmt = conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> =
            bind.iter().map(|s| s as &dyn rusqlite::ToSql).collect();

        let rows = stmt
            .query_map(params.as_slice(), |row| {
                Ok(SkillOutput {
                    id: row.get(0)?,
                    skill_id: row.get(1)?,
                    agent_id: row.get(2)?,
                    widget_type: row.get(3)?,
                    title: row.get(4)?,
                    data: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    /// Delete a specific skill output by ID.
    pub fn delete_skill_output(&self, id: &str) -> EngineResult<bool> {
        let conn = self.conn.lock();
        let deleted = conn.execute("DELETE FROM skill_outputs WHERE id = ?1", params![id])?;
        Ok(deleted > 0)
    }

    /// Delete all outputs for a skill (used on skill uninstall).
    pub fn delete_skill_outputs_by_skill(&self, skill_id: &str) -> EngineResult<u64> {
        let conn = self.conn.lock();
        let deleted = conn.execute(
            "DELETE FROM skill_outputs WHERE skill_id = ?1",
            params![skill_id],
        )?;
        Ok(deleted as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::sessions::schema_for_testing;
    use parking_lot::Mutex;
    use rusqlite::Connection;
    use std::sync::Arc;

    fn test_store() -> SessionStore {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA journal_mode = WAL;").unwrap();
        schema_for_testing(&conn);
        SessionStore::from_connection(conn)
    }

    #[test]
    fn upsert_and_list() {
        let store = test_store();
        store
            .upsert_skill_output(
                "so-1",
                "weather",
                "default",
                "status",
                "Weather",
                r#"{"temp":"22C"}"#,
            )
            .unwrap();
        let all = store.list_skill_outputs(None, None).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].skill_id, "weather");
        assert_eq!(all[0].widget_type, "status");
    }

    #[test]
    fn upsert_updates_existing() {
        let store = test_store();
        store
            .upsert_skill_output(
                "so-1",
                "weather",
                "default",
                "status",
                "Weather",
                r#"{"temp":"22C"}"#,
            )
            .unwrap();
        store
            .upsert_skill_output(
                "so-1",
                "weather",
                "default",
                "metric",
                "Weather v2",
                r#"{"temp":"30C"}"#,
            )
            .unwrap();
        let all = store.list_skill_outputs(None, None).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].title, "Weather v2");
        assert_eq!(all[0].widget_type, "metric");
    }

    #[test]
    fn list_filters_by_skill() {
        let store = test_store();
        store
            .upsert_skill_output("so-1", "weather", "default", "status", "Weather", "{}")
            .unwrap();
        store
            .upsert_skill_output("so-2", "stocks", "default", "table", "Stocks", "{}")
            .unwrap();
        let filtered = store.list_skill_outputs(Some("weather"), None).unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].skill_id, "weather");
    }

    #[test]
    fn list_filters_by_agent() {
        let store = test_store();
        store
            .upsert_skill_output("so-1", "weather", "default", "status", "W1", "{}")
            .unwrap();
        store
            .upsert_skill_output("so-2", "weather", "agent-2", "status", "W2", "{}")
            .unwrap();
        let filtered = store.list_skill_outputs(None, Some("agent-2")).unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].agent_id, "agent-2");
    }

    #[test]
    fn delete_skill_output() {
        let store = test_store();
        store
            .upsert_skill_output("so-1", "weather", "default", "status", "X", "{}")
            .unwrap();
        let deleted = store.delete_skill_output("so-1").unwrap();
        assert!(deleted);
        let all = store.list_skill_outputs(None, None).unwrap();
        assert_eq!(all.len(), 0);
    }

    #[test]
    fn delete_nonexistent() {
        let store = test_store();
        let deleted = store.delete_skill_output("nope").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn delete_by_skill() {
        let store = test_store();
        store
            .upsert_skill_output("so-1", "weather", "default", "status", "A", "{}")
            .unwrap();
        store
            .upsert_skill_output("so-2", "weather", "agent-2", "status", "B", "{}")
            .unwrap();
        store
            .upsert_skill_output("so-3", "stocks", "default", "table", "C", "{}")
            .unwrap();
        let count = store.delete_skill_outputs_by_skill("weather").unwrap();
        assert_eq!(count, 2);
        let all = store.list_skill_outputs(None, None).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].skill_id, "stocks");
    }
}
