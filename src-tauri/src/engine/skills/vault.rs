// Pawz Agent Engine — Skill Vault (SessionStore credential methods)
// SQLite-backed credential storage: CRUD, enabled state, custom instructions.

use crate::engine::sessions::SessionStore;

impl SessionStore {
    /// Initialize the skill vault tables (call from open()).
    pub fn init_skill_tables(&self) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS skill_credentials (
                skill_id TEXT NOT NULL,
                cred_key TEXT NOT NULL,
                cred_value TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (skill_id, cred_key)
            );

            CREATE TABLE IF NOT EXISTS skill_state (
                skill_id TEXT PRIMARY KEY,
                enabled INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS skill_custom_instructions (
                skill_id TEXT PRIMARY KEY,
                instructions TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
        ").map_err(|e| format!("Failed to create skill tables: {}", e))?;
        Ok(())
    }

    /// Store a credential for a skill.
    /// Value is stored encrypted (caller must encrypt before calling).
    pub fn set_skill_credential(&self, skill_id: &str, key: &str, encrypted_value: &str) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO skill_credentials (skill_id, cred_key, cred_value, updated_at)
             VALUES (?1, ?2, ?3, datetime('now'))
             ON CONFLICT(skill_id, cred_key) DO UPDATE SET cred_value = ?3, updated_at = datetime('now')",
            rusqlite::params![skill_id, key, encrypted_value],
        ).map_err(|e| format!("Set credential error: {}", e))?;
        Ok(())
    }

    /// Get a credential for a skill (returns encrypted value).
    pub fn get_skill_credential(&self, skill_id: &str, key: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock();
        let result = conn.query_row(
            "SELECT cred_value FROM skill_credentials WHERE skill_id = ?1 AND cred_key = ?2",
            rusqlite::params![skill_id, key],
            |row: &rusqlite::Row| row.get::<_, String>(0),
        );
        match result {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("Query error: {}", e)),
        }
    }

    /// Delete a credential for a skill.
    pub fn delete_skill_credential(&self, skill_id: &str, key: &str) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM skill_credentials WHERE skill_id = ?1 AND cred_key = ?2",
            rusqlite::params![skill_id, key],
        ).map_err(|e| format!("Delete credential error: {}", e))?;
        Ok(())
    }

    /// Delete ALL credentials for a skill.
    pub fn delete_all_skill_credentials(&self, skill_id: &str) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM skill_credentials WHERE skill_id = ?1",
            rusqlite::params![skill_id],
        ).map_err(|e| format!("Delete credentials error: {}", e))?;
        Ok(())
    }

    /// List which credential keys are set for a skill (not the values).
    pub fn list_skill_credential_keys(&self, skill_id: &str) -> Result<Vec<String>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT cred_key FROM skill_credentials WHERE skill_id = ?1 ORDER BY cred_key"
        ).map_err(|e| format!("Prepare error: {}", e))?;
        let keys: Vec<String> = stmt.query_map(rusqlite::params![skill_id], |row: &rusqlite::Row| row.get::<_, String>(0))
            .map_err(|e| format!("Query error: {}", e))?
            .filter_map(|r: Result<String, rusqlite::Error>| r.ok())
            .collect();
        Ok(keys)
    }

    /// Get/set skill enabled state.
    pub fn set_skill_enabled(&self, skill_id: &str, enabled: bool) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO skill_state (skill_id, enabled, updated_at) VALUES (?1, ?2, datetime('now'))
             ON CONFLICT(skill_id) DO UPDATE SET enabled = ?2, updated_at = datetime('now')",
            rusqlite::params![skill_id, enabled as i32],
        ).map_err(|e| format!("Set skill state error: {}", e))?;
        Ok(())
    }

    pub fn is_skill_enabled(&self, skill_id: &str) -> Result<bool, String> {
        let conn = self.conn.lock();
        let result = conn.query_row(
            "SELECT enabled FROM skill_state WHERE skill_id = ?1",
            rusqlite::params![skill_id],
            |row: &rusqlite::Row| row.get::<_, i32>(0),
        );
        match result {
            Ok(v) => Ok(v != 0),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(false),
            Err(e) => Err(format!("Query error: {}", e)),
        }
    }

    /// Get custom instructions for a skill (if any).
    pub fn get_skill_custom_instructions(&self, skill_id: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock();
        let result = conn.query_row(
            "SELECT instructions FROM skill_custom_instructions WHERE skill_id = ?1",
            rusqlite::params![skill_id],
            |row: &rusqlite::Row| row.get::<_, String>(0),
        );
        match result {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("Query error: {}", e)),
        }
    }

    /// Set custom instructions for a skill.
    /// Pass empty string to clear (falls back to defaults).
    pub fn set_skill_custom_instructions(&self, skill_id: &str, instructions: &str) -> Result<(), String> {
        let conn = self.conn.lock();
        if instructions.is_empty() {
            conn.execute(
                "DELETE FROM skill_custom_instructions WHERE skill_id = ?1",
                rusqlite::params![skill_id],
            ).map_err(|e| format!("Delete error: {}", e))?;
        } else {
            conn.execute(
                "INSERT INTO skill_custom_instructions (skill_id, instructions, updated_at)
                 VALUES (?1, ?2, datetime('now'))
                 ON CONFLICT(skill_id) DO UPDATE SET instructions = ?2, updated_at = datetime('now')",
                rusqlite::params![skill_id, instructions],
            ).map_err(|e| format!("Set instructions error: {}", e))?;
        }
        Ok(())
    }
}
