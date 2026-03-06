// Dashboards — CRUD for saved Agent Canvas dashboards.
// Each row represents a named, optionally pinned dashboard
// that persists independently of its originating chat session.

use super::SessionStore;
use crate::atoms::error::EngineResult;
use crate::atoms::types::DashboardRow;
use rusqlite::params;

impl SessionStore {
    /// Create a new saved dashboard.
    #[allow(clippy::too_many_arguments)]
    pub fn create_dashboard(
        &self,
        id: &str,
        name: &str,
        icon: &str,
        agent_id: &str,
        source_session_id: Option<&str>,
        template_id: Option<&str>,
        pinned: bool,
        refresh_interval: Option<&str>,
        refresh_prompt: Option<&str>,
    ) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO dashboards
                (id, name, icon, agent_id, source_session_id, template_id,
                 pinned, refresh_interval, refresh_prompt)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id,
                name,
                icon,
                agent_id,
                source_session_id,
                template_id,
                pinned as i32,
                refresh_interval,
                refresh_prompt,
            ],
        )?;
        Ok(())
    }

    /// Get a single dashboard by ID.
    pub fn get_dashboard(&self, id: &str) -> EngineResult<Option<DashboardRow>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, icon, agent_id, source_session_id, template_id,
                    pinned, refresh_interval, refresh_prompt, last_refreshed_at,
                    created_at, updated_at
             FROM dashboards WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], map_dashboard_row)?;
        match rows.next() {
            Some(Ok(row)) => Ok(Some(row)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    /// List all saved dashboards, newest first.
    pub fn list_dashboards(&self) -> EngineResult<Vec<DashboardRow>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, icon, agent_id, source_session_id, template_id,
                    pinned, refresh_interval, refresh_prompt, last_refreshed_at,
                    created_at, updated_at
             FROM dashboards
             ORDER BY pinned DESC, updated_at DESC",
        )?;
        let rows = stmt
            .query_map([], map_dashboard_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// List only pinned dashboards (for sidebar display).
    pub fn list_pinned_dashboards(&self) -> EngineResult<Vec<DashboardRow>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, icon, agent_id, source_session_id, template_id,
                    pinned, refresh_interval, refresh_prompt, last_refreshed_at,
                    created_at, updated_at
             FROM dashboards
             WHERE pinned = 1
             ORDER BY updated_at DESC",
        )?;
        let rows = stmt
            .query_map([], map_dashboard_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Update a dashboard's metadata (name, icon, pinned, refresh settings).
    pub fn update_dashboard(
        &self,
        id: &str,
        name: Option<&str>,
        icon: Option<&str>,
        pinned: Option<bool>,
        refresh_interval: Option<Option<&str>>,
        refresh_prompt: Option<Option<&str>>,
    ) -> EngineResult<bool> {
        let conn = self.conn.lock();
        let mut sets = Vec::new();
        let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(n) = name {
            sets.push("name = ?");
            values.push(Box::new(n.to_string()));
        }
        if let Some(i) = icon {
            sets.push("icon = ?");
            values.push(Box::new(i.to_string()));
        }
        if let Some(p) = pinned {
            sets.push("pinned = ?");
            values.push(Box::new(p as i32));
        }
        if let Some(ri) = refresh_interval {
            sets.push("refresh_interval = ?");
            values.push(Box::new(ri.map(|s| s.to_string())));
        }
        if let Some(rp) = refresh_prompt {
            sets.push("refresh_prompt = ?");
            values.push(Box::new(rp.map(|s| s.to_string())));
        }
        if sets.is_empty() {
            return Ok(false);
        }
        sets.push("updated_at = datetime('now')");

        let sql = format!("UPDATE dashboards SET {} WHERE id = ?", sets.join(", "));
        values.push(Box::new(id.to_string()));

        let params: Vec<&dyn rusqlite::ToSql> = values.iter().map(|v| v.as_ref()).collect();
        let updated = conn.execute(&sql, params.as_slice())?;
        Ok(updated > 0)
    }

    /// Mark a dashboard's last_refreshed_at to now.
    pub fn touch_dashboard_refreshed(&self, id: &str) -> EngineResult<bool> {
        let conn = self.conn.lock();
        let updated = conn.execute(
            "UPDATE dashboards SET last_refreshed_at = datetime('now'),
                                   updated_at = datetime('now')
             WHERE id = ?1",
            params![id],
        )?;
        Ok(updated > 0)
    }

    /// Delete a dashboard and all its canvas components.
    pub fn delete_dashboard(&self, id: &str) -> EngineResult<bool> {
        let conn = self.conn.lock();
        // Delete child components first.
        conn.execute(
            "DELETE FROM canvas_components WHERE dashboard_id = ?1",
            params![id],
        )?;
        let deleted = conn.execute("DELETE FROM dashboards WHERE id = ?1", params![id])?;
        Ok(deleted > 0)
    }

    /// Clone all canvas components from a session to a dashboard scope.
    /// Used when saving an ephemeral canvas as a named dashboard.
    pub fn clone_components_to_dashboard(
        &self,
        source_session_id: &str,
        dashboard_id: &str,
    ) -> EngineResult<u64> {
        let conn = self.conn.lock();
        let count = conn.execute(
            "INSERT INTO canvas_components
                (id, session_id, dashboard_id, agent_id, component_type, title, data, position)
             SELECT
                'cc-' || hex(randomblob(8)),
                NULL,
                ?2,
                agent_id, component_type, title, data, position
             FROM canvas_components
             WHERE session_id = ?1",
            params![source_session_id, dashboard_id],
        )?;
        Ok(count as u64)
    }
}

/// Map a SQLite row to a DashboardRow.
fn map_dashboard_row(row: &rusqlite::Row) -> rusqlite::Result<DashboardRow> {
    Ok(DashboardRow {
        id: row.get(0)?,
        name: row.get(1)?,
        icon: row.get(2)?,
        agent_id: row.get(3)?,
        source_session_id: row.get(4)?,
        template_id: row.get(5)?,
        pinned: row.get::<_, i32>(6)? != 0,
        refresh_interval: row.get(7)?,
        refresh_prompt: row.get(8)?,
        last_refreshed_at: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
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
        SessionStore {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    #[test]
    fn create_and_get_dashboard() {
        let store = test_store();
        store
            .create_dashboard(
                "dash-1",
                "CI Dashboard",
                "rocket_launch",
                "default",
                Some("sess-1"),
                None,
                true,
                Some("30m"),
                Some("Refresh CI metrics"),
            )
            .unwrap();
        let dash = store.get_dashboard("dash-1").unwrap().unwrap();
        assert_eq!(dash.name, "CI Dashboard");
        assert!(dash.pinned);
        assert_eq!(dash.refresh_interval, Some("30m".into()));
    }

    #[test]
    fn list_dashboards_pinned_first() {
        let store = test_store();
        store
            .create_dashboard(
                "d-1", "Unpinned", "star", "default", None, None, false, None, None,
            )
            .unwrap();
        store
            .create_dashboard(
                "d-2", "Pinned", "pin", "default", None, None, true, None, None,
            )
            .unwrap();
        let list = store.list_dashboards().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "Pinned");
    }

    #[test]
    fn update_dashboard() {
        let store = test_store();
        store
            .create_dashboard(
                "d-1", "Old Name", "star", "default", None, None, false, None, None,
            )
            .unwrap();
        store
            .update_dashboard("d-1", Some("New Name"), None, Some(true), None, None)
            .unwrap();
        let dash = store.get_dashboard("d-1").unwrap().unwrap();
        assert_eq!(dash.name, "New Name");
        assert!(dash.pinned);
    }

    #[test]
    fn delete_dashboard_cascades() {
        let store = test_store();
        store
            .create_dashboard("d-1", "Temp", "x", "default", None, None, false, None, None)
            .unwrap();
        store
            .upsert_canvas_component(
                "cc-1",
                None,
                Some("d-1"),
                "default",
                "metric",
                "Test",
                "{}",
                None,
            )
            .unwrap();
        assert!(store.delete_dashboard("d-1").unwrap());
        assert!(store.list_canvas_by_dashboard("d-1").unwrap().is_empty());
    }

    #[test]
    fn clone_components_to_dashboard() {
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
                "cc-2",
                Some("sess-1"),
                None,
                "default",
                "chart",
                "Load",
                r#"{"series":[]}"#,
                None,
            )
            .unwrap();
        let cloned = store
            .clone_components_to_dashboard("sess-1", "dash-1")
            .unwrap();
        assert_eq!(cloned, 2);
        let comps = store.list_canvas_by_dashboard("dash-1").unwrap();
        assert_eq!(comps.len(), 2);
    }
}
