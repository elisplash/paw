// Dashboard Windows — Geometry persistence for pop-out windows.
// Remembers window position and size per dashboard so the next
// pop-out restores the same layout.

use super::SessionStore;
use crate::atoms::error::EngineResult;
use crate::atoms::types::DashboardWindowRow;
use rusqlite::params;

impl SessionStore {
    /// Save or update window geometry for a dashboard.
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_window_geometry(
        &self,
        dashboard_id: &str,
        x: Option<i32>,
        y: Option<i32>,
        width: i32,
        height: i32,
        monitor: Option<i32>,
        popped_out: bool,
    ) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO dashboard_windows
                (dashboard_id, x, y, width, height, monitor, popped_out)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(dashboard_id) DO UPDATE SET
                x = excluded.x,
                y = excluded.y,
                width = excluded.width,
                height = excluded.height,
                monitor = excluded.monitor,
                popped_out = excluded.popped_out,
                updated_at = datetime('now')",
            params![
                dashboard_id,
                x,
                y,
                width,
                height,
                monitor,
                popped_out as i32
            ],
        )?;
        Ok(())
    }

    /// Get stored window geometry for a dashboard.
    pub fn get_window_geometry(
        &self,
        dashboard_id: &str,
    ) -> EngineResult<Option<DashboardWindowRow>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT dashboard_id, x, y, width, height, monitor, popped_out, updated_at
             FROM dashboard_windows WHERE dashboard_id = ?1",
        )?;
        let mut rows = stmt.query_map(params![dashboard_id], map_window_row)?;
        match rows.next() {
            Some(Ok(row)) => Ok(Some(row)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    /// List all dashboards that were popped out (for restore on startup).
    pub fn list_popped_out_windows(&self) -> EngineResult<Vec<DashboardWindowRow>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT dashboard_id, x, y, width, height, monitor, popped_out, updated_at
             FROM dashboard_windows
             WHERE popped_out = 1
             ORDER BY updated_at DESC",
        )?;
        let rows = stmt
            .query_map([], map_window_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Mark a dashboard window as no longer popped out.
    pub fn mark_window_closed(&self, dashboard_id: &str) -> EngineResult<bool> {
        let conn = self.conn.lock();
        let updated = conn.execute(
            "UPDATE dashboard_windows SET popped_out = 0, updated_at = datetime('now')
             WHERE dashboard_id = ?1",
            params![dashboard_id],
        )?;
        Ok(updated > 0)
    }

    /// Delete window geometry for a dashboard.
    pub fn delete_window_geometry(&self, dashboard_id: &str) -> EngineResult<bool> {
        let conn = self.conn.lock();
        let deleted = conn.execute(
            "DELETE FROM dashboard_windows WHERE dashboard_id = ?1",
            params![dashboard_id],
        )?;
        Ok(deleted > 0)
    }
}

/// Map a SQLite row to a DashboardWindowRow.
fn map_window_row(row: &rusqlite::Row) -> rusqlite::Result<DashboardWindowRow> {
    Ok(DashboardWindowRow {
        dashboard_id: row.get(0)?,
        x: row.get(1)?,
        y: row.get(2)?,
        width: row.get(3)?,
        height: row.get(4)?,
        monitor: row.get(5)?,
        popped_out: row.get::<_, i32>(6)? != 0,
        updated_at: row.get(7)?,
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
    fn upsert_and_get_geometry() {
        let store = test_store();
        store
            .upsert_window_geometry("dash-1", Some(100), Some(200), 1200, 800, None, true)
            .unwrap();
        let win = store.get_window_geometry("dash-1").unwrap().unwrap();
        assert_eq!(win.x, Some(100));
        assert_eq!(win.y, Some(200));
        assert_eq!(win.width, 1200);
        assert!(win.popped_out);
    }

    #[test]
    fn upsert_updates_existing() {
        let store = test_store();
        store
            .upsert_window_geometry("dash-1", None, None, 900, 700, None, false)
            .unwrap();
        store
            .upsert_window_geometry("dash-1", Some(50), Some(50), 1000, 750, Some(1), true)
            .unwrap();
        let win = store.get_window_geometry("dash-1").unwrap().unwrap();
        assert_eq!(win.width, 1000);
        assert!(win.popped_out);
    }

    #[test]
    fn list_popped_out() {
        let store = test_store();
        store
            .upsert_window_geometry("d-1", None, None, 900, 700, None, true)
            .unwrap();
        store
            .upsert_window_geometry("d-2", None, None, 900, 700, None, false)
            .unwrap();
        let popped = store.list_popped_out_windows().unwrap();
        assert_eq!(popped.len(), 1);
        assert_eq!(popped[0].dashboard_id, "d-1");
    }

    #[test]
    fn mark_closed() {
        let store = test_store();
        store
            .upsert_window_geometry("d-1", None, None, 900, 700, None, true)
            .unwrap();
        store.mark_window_closed("d-1").unwrap();
        let win = store.get_window_geometry("d-1").unwrap().unwrap();
        assert!(!win.popped_out);
    }
}
