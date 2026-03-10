// pawz-code — state.rs
// Shared application state: config, SQLite memory DB, SSE broadcast channel,
// and active-run tracking (for cancellation support).

use crate::config::Config;
use anyhow::Result;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Arc<Mutex<Connection>>,
    /// Broadcast channel — every SSE connection subscribes; agent loop publishes here.
    pub sse_tx: broadcast::Sender<String>,
    /// Active runs: run_id → cancel flag. Set to true to request cancellation.
    pub active_runs: Arc<Mutex<HashMap<String, bool>>>,
    /// Loaded protocol store: name → text
    pub protocols: Arc<Mutex<HashMap<String, String>>>,
}

impl AppState {
    pub fn new(config: Config) -> Result<Self> {
        let db_path = Config::db_path();
        std::fs::create_dir_all(db_path.parent().unwrap())?;

        let conn = Connection::open(&db_path)?;
        init_schema(&conn)?;

        let (tx, _) = broadcast::channel::<String>(4096);

        Ok(AppState {
            config: Arc::new(config),
            db: Arc::new(Mutex::new(conn)),
            sse_tx: tx,
            active_runs: Arc::new(Mutex::new(HashMap::new())),
            protocols: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Broadcast a serialised EngineEvent JSON string to all SSE subscribers.
    pub fn fire(&self, json: String) {
        if let Err(e) = self.sse_tx.send(json) {
            log::warn!("[state] SSE broadcast send failed (no active subscribers or channel full): {}", e);
        }
    }

    /// Register a new run as active.
    pub fn register_run(&self, run_id: &str) {
        let mut runs = self.active_runs.lock().unwrap_or_else(|e| e.into_inner());
        runs.insert(run_id.to_string(), false);
    }

    /// Mark a run as cancelled.
    pub fn cancel_run(&self, run_id: &str) -> bool {
        let mut runs = self.active_runs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(flag) = runs.get_mut(run_id) {
            *flag = true;
            true
        } else {
            false
        }
    }

    /// Check if a run has been cancelled.
    pub fn is_cancelled(&self, run_id: &str) -> bool {
        let runs = self.active_runs.lock().unwrap_or_else(|e| e.into_inner());
        runs.get(run_id).copied().unwrap_or(false)
    }

    /// Remove a run from the active set.
    pub fn deregister_run(&self, run_id: &str) {
        let mut runs = self.active_runs.lock().unwrap_or_else(|e| e.into_inner());
        runs.remove(run_id);
    }

    /// Return the count of currently active runs.
    pub fn active_run_count(&self) -> usize {
        let runs = self.active_runs.lock().unwrap_or_else(|e| e.into_inner());
        runs.len()
    }
}

fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;

        CREATE TABLE IF NOT EXISTS messages (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id  TEXT NOT NULL,
            role        TEXT NOT NULL,
            content_json TEXT NOT NULL,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_msg_session ON messages(session_id, id);

        CREATE TABLE IF NOT EXISTS memories (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            key         TEXT NOT NULL,
            content     TEXT NOT NULL,
            tags        TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_mem_key ON memories(key);

        CREATE TABLE IF NOT EXISTS sessions (
            id          TEXT PRIMARY KEY,
            title       TEXT,
            summary     TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS engram (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            scope       TEXT NOT NULL DEFAULT 'global',
            key         TEXT NOT NULL,
            content     TEXT NOT NULL,
            kind        TEXT NOT NULL DEFAULT 'summary',
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_engram_scope_key ON engram(scope, key);
        "#,
    )
}
