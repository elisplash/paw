// Canvas Components — CRUD for Agent Canvas widget data.
// Each row maps a component to a session or dashboard with a type + JSON data.

use super::SessionStore;
use crate::atoms::error::EngineResult;
use crate::atoms::types::CanvasComponentRow;
use rusqlite::params;

impl SessionStore {
    /// Insert or update a canvas component.
    /// Uses ON CONFLICT to upsert — same pattern as skill_outputs.
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_canvas_component(
        &self,
        id: &str,
        session_id: Option<&str>,
        dashboard_id: Option<&str>,
        agent_id: &str,
        component_type: &str,
        title: &str,
        data: &str,
        position: Option<&str>,
    ) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO canvas_components
                (id, session_id, dashboard_id, agent_id, component_type, title, data, position)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
                component_type = excluded.component_type,
                title = excluded.title,
                data = excluded.data,
                position = excluded.position,
                updated_at = datetime('now')",
            params![
                id,
                session_id,
                dashboard_id,
                agent_id,
                component_type,
                title,
                data,
                position
            ],
        )?;
        Ok(())
    }

    /// List canvas components for a session.
    pub fn list_canvas_by_session(
        &self,
        session_id: &str,
    ) -> EngineResult<Vec<CanvasComponentRow>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, session_id, dashboard_id, agent_id, component_type,
                    title, data, position, created_at, updated_at
             FROM canvas_components
             WHERE session_id = ?1
             ORDER BY created_at ASC",
        )?;
        let rows = stmt
            .query_map(params![session_id], map_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// List canvas components for a saved dashboard.
    pub fn list_canvas_by_dashboard(
        &self,
        dashboard_id: &str,
    ) -> EngineResult<Vec<CanvasComponentRow>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, session_id, dashboard_id, agent_id, component_type,
                    title, data, position, created_at, updated_at
             FROM canvas_components
             WHERE dashboard_id = ?1
             ORDER BY created_at ASC",
        )?;
        let rows = stmt
            .query_map(params![dashboard_id], map_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// List the most recent canvas components across all sessions (fallback view).
    pub fn list_canvas_recent(&self, limit: u32) -> EngineResult<Vec<CanvasComponentRow>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, session_id, dashboard_id, agent_id, component_type,
                    title, data, position, created_at, updated_at
             FROM canvas_components
             ORDER BY updated_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit], map_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Update a canvas component partially (patch).
    pub fn patch_canvas_component(
        &self,
        id: &str,
        title: Option<&str>,
        data: Option<&str>,
        position: Option<&str>,
    ) -> EngineResult<bool> {
        let conn = self.conn.lock();
        // Build SET clause dynamically based on which fields are provided.
        let mut sets = Vec::new();
        let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(t) = title {
            sets.push("title = ?");
            values.push(Box::new(t.to_string()));
        }
        if let Some(d) = data {
            sets.push("data = ?");
            values.push(Box::new(d.to_string()));
        }
        if let Some(p) = position {
            sets.push("position = ?");
            values.push(Box::new(p.to_string()));
        }
        if sets.is_empty() {
            return Ok(false);
        }
        sets.push("updated_at = datetime('now')");

        let sql = format!(
            "UPDATE canvas_components SET {} WHERE id = ?",
            sets.join(", ")
        );
        values.push(Box::new(id.to_string()));

        let params: Vec<&dyn rusqlite::ToSql> = values.iter().map(|v| v.as_ref()).collect();
        let updated = conn.execute(&sql, params.as_slice())?;
        Ok(updated > 0)
    }

    /// Delete a canvas component by ID.
    pub fn delete_canvas_component(&self, id: &str) -> EngineResult<bool> {
        let conn = self.conn.lock();
        let deleted = conn.execute("DELETE FROM canvas_components WHERE id = ?1", params![id])?;
        Ok(deleted > 0)
    }

    /// Delete all canvas components for a session.
    pub fn clear_canvas_session(&self, session_id: &str) -> EngineResult<u64> {
        let conn = self.conn.lock();
        let deleted = conn.execute(
            "DELETE FROM canvas_components WHERE session_id = ?1",
            params![session_id],
        )?;
        Ok(deleted as u64)
    }
}

/// Map a SQLite row to a CanvasComponentRow.
fn map_row(row: &rusqlite::Row) -> rusqlite::Result<CanvasComponentRow> {
    Ok(CanvasComponentRow {
        id: row.get(0)?,
        session_id: row.get(1)?,
        dashboard_id: row.get(2)?,
        agent_id: row.get(3)?,
        component_type: row.get(4)?,
        title: row.get(5)?,
        data: row.get(6)?,
        position: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
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
    fn upsert_and_list_by_session() {
        let store = test_store();
        store
            .upsert_canvas_component(
                "cc-1",
                Some("sess-1"),
                None,
                "default",
                "metric",
                "CPU Usage",
                r#"{"value":"72%"}"#,
                None,
            )
            .unwrap();
        let items = store.list_canvas_by_session("sess-1").unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "CPU Usage");
        assert_eq!(items[0].component_type, "metric");
    }

    #[test]
    fn upsert_updates_existing() {
        let store = test_store();
        store
            .upsert_canvas_component(
                "cc-1",
                Some("sess-1"),
                None,
                "default",
                "metric",
                "CPU",
                r#"{"value":"50%"}"#,
                None,
            )
            .unwrap();
        store
            .upsert_canvas_component(
                "cc-1",
                Some("sess-1"),
                None,
                "default",
                "chart",
                "CPU Chart",
                r#"{"series":[]}"#,
                None,
            )
            .unwrap();
        let items = store.list_canvas_by_session("sess-1").unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "CPU Chart");
        assert_eq!(items[0].component_type, "chart");
    }

    #[test]
    fn patch_component() {
        let store = test_store();
        store
            .upsert_canvas_component(
                "cc-1",
                Some("sess-1"),
                None,
                "default",
                "metric",
                "Old Title",
                "{}",
                None,
            )
            .unwrap();
        let patched = store
            .patch_canvas_component("cc-1", Some("New Title"), None, None)
            .unwrap();
        assert!(patched);
        let items = store.list_canvas_by_session("sess-1").unwrap();
        assert_eq!(items[0].title, "New Title");
    }

    #[test]
    fn delete_component() {
        let store = test_store();
        store
            .upsert_canvas_component(
                "cc-1",
                Some("sess-1"),
                None,
                "default",
                "status",
                "Test",
                "{}",
                None,
            )
            .unwrap();
        assert!(store.delete_canvas_component("cc-1").unwrap());
        assert!(!store.delete_canvas_component("cc-1").unwrap());
    }

    #[test]
    fn clear_session() {
        let store = test_store();
        for i in 0..3 {
            store
                .upsert_canvas_component(
                    &format!("cc-{i}"),
                    Some("sess-1"),
                    None,
                    "default",
                    "status",
                    &format!("Item {i}"),
                    "{}",
                    None,
                )
                .unwrap();
        }
        let cleared = store.clear_canvas_session("sess-1").unwrap();
        assert_eq!(cleared, 3);
        assert!(store.list_canvas_by_session("sess-1").unwrap().is_empty());
    }
}
