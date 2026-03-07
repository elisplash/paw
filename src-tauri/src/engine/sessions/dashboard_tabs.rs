// Dashboard Tabs — CRUD for tab state persistence.
// Tracks which dashboards are open as tabs, their order,
// which is active, and which OS window owns each tab.

use super::SessionStore;
use crate::atoms::error::EngineResult;
use crate::atoms::types::DashboardTabRow;
use rusqlite::params;

impl SessionStore {
    /// Open a new tab for a dashboard in a given window.
    pub fn open_tab(&self, id: &str, dashboard_id: &str, window_id: &str) -> EngineResult<()> {
        let conn = self.conn.lock();
        // Get next tab_order for this window.
        let max_order: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(tab_order), -1) FROM dashboard_tabs WHERE window_id = ?1",
                params![window_id],
                |row| row.get(0),
            )
            .unwrap_or(-1);

        conn.execute(
            "INSERT OR REPLACE INTO dashboard_tabs
                (id, dashboard_id, tab_order, active, window_id)
             VALUES (?1, ?2, ?3, 0, ?4)",
            params![id, dashboard_id, max_order + 1, window_id],
        )?;
        Ok(())
    }

    /// Close (remove) a tab by ID.
    pub fn close_tab(&self, id: &str) -> EngineResult<bool> {
        let conn = self.conn.lock();
        let deleted = conn.execute("DELETE FROM dashboard_tabs WHERE id = ?1", params![id])?;
        Ok(deleted > 0)
    }

    /// Set a tab as active (deactivates all others in the same window).
    pub fn activate_tab(&self, id: &str, window_id: &str) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE dashboard_tabs SET active = 0 WHERE window_id = ?1",
            params![window_id],
        )?;
        conn.execute(
            "UPDATE dashboard_tabs SET active = 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Reorder a tab to a new position within its window.
    /// Shifts other tabs accordingly.
    pub fn reorder_tab(&self, id: &str, new_order: i32) -> EngineResult<()> {
        let conn = self.conn.lock();

        // Get current window_id and order for this tab.
        let (window_id, old_order): (String, i32) = conn.query_row(
            "SELECT window_id, tab_order FROM dashboard_tabs WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        if old_order == new_order {
            return Ok(());
        }

        // Shift tabs between old and new positions.
        if new_order < old_order {
            conn.execute(
                "UPDATE dashboard_tabs SET tab_order = tab_order + 1
                 WHERE window_id = ?1 AND tab_order >= ?2 AND tab_order < ?3",
                params![window_id, new_order, old_order],
            )?;
        } else {
            conn.execute(
                "UPDATE dashboard_tabs SET tab_order = tab_order - 1
                 WHERE window_id = ?1 AND tab_order > ?2 AND tab_order <= ?3",
                params![window_id, old_order, new_order],
            )?;
        }

        conn.execute(
            "UPDATE dashboard_tabs SET tab_order = ?1 WHERE id = ?2",
            params![new_order, id],
        )?;
        Ok(())
    }

    /// List all tabs for a window, ordered by tab_order.
    pub fn list_tabs(&self, window_id: &str) -> EngineResult<Vec<DashboardTabRow>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, dashboard_id, tab_order, active, window_id, created_at
             FROM dashboard_tabs
             WHERE window_id = ?1
             ORDER BY tab_order ASC",
        )?;
        let rows = stmt
            .query_map(params![window_id], map_tab_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// List all tabs across all windows (for restore on startup).
    pub fn list_all_tabs(&self) -> EngineResult<Vec<DashboardTabRow>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, dashboard_id, tab_order, active, window_id, created_at
             FROM dashboard_tabs
             ORDER BY window_id ASC, tab_order ASC",
        )?;
        let rows = stmt
            .query_map([], map_tab_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Get the active tab for a window (if any).
    pub fn get_active_tab(&self, window_id: &str) -> EngineResult<Option<DashboardTabRow>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, dashboard_id, tab_order, active, window_id, created_at
             FROM dashboard_tabs
             WHERE window_id = ?1 AND active = 1
             LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![window_id], map_tab_row)?;
        match rows.next() {
            Some(Ok(row)) => Ok(Some(row)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    /// Close all tabs for a window.
    pub fn close_all_tabs(&self, window_id: &str) -> EngineResult<u64> {
        let conn = self.conn.lock();
        let deleted = conn.execute(
            "DELETE FROM dashboard_tabs WHERE window_id = ?1",
            params![window_id],
        )?;
        Ok(deleted as u64)
    }
}

/// Map a SQLite row to a DashboardTabRow.
fn map_tab_row(row: &rusqlite::Row) -> rusqlite::Result<DashboardTabRow> {
    Ok(DashboardTabRow {
        id: row.get(0)?,
        dashboard_id: row.get(1)?,
        tab_order: row.get(2)?,
        active: row.get::<_, i32>(3)? != 0,
        window_id: row.get(4)?,
        created_at: row.get(5)?,
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
    fn open_and_list_tabs() {
        let store = test_store();
        store.open_tab("t-1", "dash-a", "main").unwrap();
        store.open_tab("t-2", "dash-b", "main").unwrap();
        let tabs = store.list_tabs("main").unwrap();
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs[0].dashboard_id, "dash-a");
        assert_eq!(tabs[0].tab_order, 0);
        assert_eq!(tabs[1].tab_order, 1);
    }

    #[test]
    fn activate_tab() {
        let store = test_store();
        store.open_tab("t-1", "dash-a", "main").unwrap();
        store.open_tab("t-2", "dash-b", "main").unwrap();
        store.activate_tab("t-2", "main").unwrap();
        let active = store.get_active_tab("main").unwrap().unwrap();
        assert_eq!(active.id, "t-2");
        assert!(active.active);
    }

    #[test]
    fn close_tab() {
        let store = test_store();
        store.open_tab("t-1", "dash-a", "main").unwrap();
        assert!(store.close_tab("t-1").unwrap());
        assert!(store.list_tabs("main").unwrap().is_empty());
    }

    #[test]
    fn reorder_tab() {
        let store = test_store();
        store.open_tab("t-1", "dash-a", "main").unwrap();
        store.open_tab("t-2", "dash-b", "main").unwrap();
        store.open_tab("t-3", "dash-c", "main").unwrap();
        // Move t-3 (order=2) to position 0.
        store.reorder_tab("t-3", 0).unwrap();
        let tabs = store.list_tabs("main").unwrap();
        assert_eq!(tabs[0].dashboard_id, "dash-c");
        assert_eq!(tabs[1].dashboard_id, "dash-a");
        assert_eq!(tabs[2].dashboard_id, "dash-b");
    }

    #[test]
    fn close_all_tabs() {
        let store = test_store();
        store.open_tab("t-1", "dash-a", "main").unwrap();
        store.open_tab("t-2", "dash-b", "main").unwrap();
        let cleared = store.close_all_tabs("main").unwrap();
        assert_eq!(cleared, 2);
    }
}
