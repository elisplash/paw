// Paw Agent Engine — Session Manager
// Stores conversation history in SQLite via rusqlite.
// Independent of the Tauri SQL plugin — uses its own connection pool
// for the engine's data, separate from the frontend's paw.db.
//
// Module layout:
//   sessions       — session CRUD (create, list, get, rename, delete, prune)
//   messages       — message CRUD + context loading + tool-pair sanitization
//   config         — key/value engine config store
//   trades         — trade history insert/query/summary
//   positions      — stop-loss / take-profit position tracking
//   agent_files    — soul/persona file CRUD + context composition
//   memories       — vector+FTS memory store + search
//   tasks          — task CRUD, cron scheduling, task agents
//   projects       — project CRUD, project agents, message bus
//   embedding      — bytes_to_f32_vec, f32_vec_to_bytes, cosine_similarity

use log::info;
use rusqlite::Connection;
use std::path::PathBuf;
use parking_lot::Mutex;
use crate::atoms::error::EngineResult;

#[allow(clippy::module_inception)]
mod sessions;
mod messages;
mod config;
mod trades;
mod positions;
mod agent_files;
mod memories;
mod tasks;
mod projects;
mod schema;
pub(crate) mod embedding;
mod skill_outputs;
mod skill_storage;

// ── Re-exports (preserve crate::engine::sessions::* API) ─────────────────────

pub use embedding::f32_vec_to_bytes;
pub use skill_outputs::SkillOutput;
pub use skill_storage::SkillStorageItem;

/// Get the path to the engine's SQLite database.
pub fn engine_db_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_default();
    let dir = home.join(".paw");
    std::fs::create_dir_all(&dir).ok();
    dir.join("engine.db")
}

/// Thread-safe database wrapper.
pub struct SessionStore {
    /// The SQLite connection, protected by a Mutex.
    /// `pub` for integration tests that need to construct an in-memory store.
    pub conn: Mutex<Connection>,
}

impl SessionStore {
    /// Open (or create) the engine database and initialize tables.
    pub fn open() -> EngineResult<Self> {
        let path = engine_db_path();
        info!("[engine] Opening session store at {:?}", path);

        let conn = Connection::open(&path)?;

        conn.execute_batch("PRAGMA journal_mode=WAL;").ok();

        schema::run_migrations(&conn)?;

        Ok(SessionStore { conn: Mutex::new(conn) })
    }
}

/// Initialise an already-open connection with the full schema.
/// Used by integration tests that create in-memory databases.
pub fn schema_for_testing(conn: &Connection) {
    schema::run_migrations(conn).expect("schema_for_testing: migrations failed");
}
