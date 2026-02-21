use rusqlite::params;
use super::SessionStore;

impl SessionStore {
    // ── Config storage ─────────────────────────────────────────────────

    pub fn get_config(&self, key: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock();
        let result = conn.query_row(
            "SELECT value FROM engine_config WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("Config read error: {}", e)),
        }
    }

    pub fn set_config(&self, key: &str, value: &str) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO engine_config (key, value) VALUES (?1, ?2)",
            params![key, value],
        ).map_err(|e| format!("Config write error: {}", e))?;
        Ok(())
    }
}
