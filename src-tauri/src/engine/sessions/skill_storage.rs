// sessions/skill_storage.rs — Persistent KV store per skill (Phase F.6).
// Each skill gets its own namespaced key-value storage.

use super::SessionStore;
use crate::atoms::error::EngineResult;
use rusqlite::params;
use serde::{Deserialize, Serialize};

/// A single key-value entry in a skill's storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillStorageItem {
    pub skill_id: String,
    pub key: String,
    pub value: String,
    pub updated_at: String,
}

impl SessionStore {
    /// Set a key-value pair in a skill's storage (upsert).
    pub fn skill_store_set(&self, skill_id: &str, key: &str, value: &str) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO skill_storage (skill_id, key, value, updated_at)
             VALUES (?1, ?2, ?3, datetime('now'))
             ON CONFLICT(skill_id, key) DO UPDATE SET value = ?3, updated_at = datetime('now')",
            params![skill_id, key, value],
        )?;
        Ok(())
    }

    /// Get a single value from a skill's storage.
    pub fn skill_store_get(&self, skill_id: &str, key: &str) -> EngineResult<Option<String>> {
        let conn = self.conn.lock();
        let result = conn.query_row(
            "SELECT value FROM skill_storage WHERE skill_id = ?1 AND key = ?2",
            params![skill_id, key],
            |row| row.get(0),
        );
        match result {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// List all key-value pairs for a skill.
    pub fn skill_store_list(&self, skill_id: &str) -> EngineResult<Vec<SkillStorageItem>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT skill_id, key, value, updated_at FROM skill_storage
             WHERE skill_id = ?1 ORDER BY key",
        )?;
        let items = stmt
            .query_map(params![skill_id], |row| {
                Ok(SkillStorageItem {
                    skill_id: row.get(0)?,
                    key: row.get(1)?,
                    value: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(items)
    }

    /// Delete a single key from a skill's storage.
    pub fn skill_store_delete(&self, skill_id: &str, key: &str) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM skill_storage WHERE skill_id = ?1 AND key = ?2",
            params![skill_id, key],
        )?;
        Ok(())
    }

    /// Delete all storage for a skill (used during uninstall).
    pub fn skill_store_clear(&self, skill_id: &str) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM skill_storage WHERE skill_id = ?1",
            params![skill_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::sessions::schema::run_migrations;
    use parking_lot::Mutex;
    use rusqlite::Connection;

    fn test_store() -> SessionStore {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        SessionStore {
            conn: Mutex::new(conn),
        }
    }

    #[test]
    fn test_set_and_get() {
        let store = test_store();
        store.skill_store_set("s1", "api_url", "https://example.com").unwrap();
        let val = store.skill_store_get("s1", "api_url").unwrap();
        assert_eq!(val, Some("https://example.com".to_string()));
    }

    #[test]
    fn test_get_missing() {
        let store = test_store();
        let val = store.skill_store_get("s1", "no-key").unwrap();
        assert_eq!(val, None);
    }

    #[test]
    fn test_upsert() {
        let store = test_store();
        store.skill_store_set("s1", "k1", "v1").unwrap();
        store.skill_store_set("s1", "k1", "v2").unwrap();
        let val = store.skill_store_get("s1", "k1").unwrap();
        assert_eq!(val, Some("v2".to_string()));
    }

    #[test]
    fn test_list() {
        let store = test_store();
        store.skill_store_set("s1", "b_key", "b").unwrap();
        store.skill_store_set("s1", "a_key", "a").unwrap();
        store.skill_store_set("s2", "other", "x").unwrap();
        let items = store.skill_store_list("s1").unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].key, "a_key"); // sorted by key
        assert_eq!(items[1].key, "b_key");
    }

    #[test]
    fn test_delete() {
        let store = test_store();
        store.skill_store_set("s1", "k1", "v1").unwrap();
        store.skill_store_delete("s1", "k1").unwrap();
        let val = store.skill_store_get("s1", "k1").unwrap();
        assert_eq!(val, None);
    }

    #[test]
    fn test_clear() {
        let store = test_store();
        store.skill_store_set("s1", "k1", "v1").unwrap();
        store.skill_store_set("s1", "k2", "v2").unwrap();
        store.skill_store_clear("s1").unwrap();
        let items = store.skill_store_list("s1").unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn test_isolation() {
        let store = test_store();
        store.skill_store_set("s1", "k1", "v1").unwrap();
        store.skill_store_set("s2", "k1", "v2").unwrap();
        assert_eq!(store.skill_store_get("s1", "k1").unwrap(), Some("v1".to_string()));
        assert_eq!(store.skill_store_get("s2", "k1").unwrap(), Some("v2".to_string()));
    }
}
