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

use crate::atoms::error::EngineResult;
use log::info;
use parking_lot::Mutex;
use rusqlite::Connection;
use std::path::PathBuf;

mod agent_files;
mod agent_messages;
mod config;
pub(crate) mod embedding;
mod engram;
mod flows;
mod memories;
mod messages;
mod positions;
mod projects;
mod schema;
#[allow(clippy::module_inception)]
mod sessions;
mod skill_outputs;
mod skill_storage;
mod squads;
mod tasks;
mod trades;

// ── Re-exports (preserve crate::engine::sessions::* API) ─────────────────────

pub use embedding::f32_vec_to_bytes;
pub use skill_outputs::SkillOutput;
pub use skill_storage::SkillStorageItem;

/// Get the path to the engine's SQLite database.
pub fn engine_db_path() -> PathBuf {
    crate::engine::paths::engine_db_path()
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

        // ── Anti-forensic: reduce file-size side-channel leakage ────────
        // Use 8KB pages (vs default 4KB) so the DB grows in coarser
        // quanta, reducing the precision of a vault-size oracle attack.
        // Also enable secure_delete so freed pages are zeroed, preventing
        // deleted memory content from lingering in unallocated pages.
        // See: KDBX inner-content padding (analogous threat model).
        conn.execute_batch("PRAGMA page_size = 8192;").ok();
        conn.execute_batch("PRAGMA secure_delete = ON;").ok();
        conn.execute_batch("PRAGMA auto_vacuum = INCREMENTAL;").ok();

        schema::run_migrations(&conn)?;

        Ok(SessionStore {
            conn: Mutex::new(conn),
        })
    }
}

/// Initialise an already-open connection with the full schema.
/// Used by integration tests that create in-memory databases.
pub fn schema_for_testing(conn: &Connection) {
    schema::run_migrations(conn).expect("schema_for_testing: migrations failed");
}
