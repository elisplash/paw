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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

mod agent_files;
mod agent_messages;
mod canvas;
mod config;
mod dashboard_tabs;
mod dashboard_windows;
mod dashboards;
pub(crate) mod embedding;
pub(crate) mod engram;
mod flows;
mod memories;
mod messages;
mod positions;
mod projects;
pub(crate) mod schema;
#[allow(clippy::module_inception)]
mod sessions;
mod skill_outputs;
mod skill_storage;
mod squads;
mod tasks;
pub(crate) mod telemetry;
mod templates;
mod trades;

// ── Re-exports (preserve crate::engine::sessions::* API) ─────────────────────

pub use embedding::f32_vec_to_bytes;
pub use skill_outputs::SkillOutput;
pub use skill_storage::SkillStorageItem;

/// Get the path to the engine's SQLite database.
pub fn engine_db_path() -> PathBuf {
    crate::engine::paths::engine_db_path()
}

/// Number of read-only connections in the pool.
/// SQLite WAL mode supports concurrent readers; 4 connections covers
/// typical agent concurrency without excessive file-handle overhead.
const READ_POOL_SIZE: usize = 4;

/// Thread-safe database wrapper with read/write separation.
///
/// Write connection: `conn` — single `Arc<Mutex<Connection>>` for all mutations.
/// Read pool: `read_pool` — N read-only WAL connections with round-robin dispatch.
/// SQLite WAL mode allows concurrent readers alongside a single writer.
pub struct SessionStore {
    /// The write connection, protected by a Mutex and shareable via Arc.
    /// `pub` for integration tests that need to construct an in-memory store.
    pub conn: Arc<Mutex<Connection>>,
    /// Pool of read-only connections for concurrent search/query operations.
    /// Round-robin selection via `read_idx` atomic counter.
    read_pool: Vec<Arc<Mutex<Connection>>>,
    /// Atomic counter for round-robin read pool selection.
    read_idx: AtomicUsize,
}

impl SessionStore {
    /// Open (or create) the engine database and initialize tables.
    pub fn open() -> EngineResult<Self> {
        let path = engine_db_path();
        info!("[engine] Opening session store at {:?}", path);

        let conn = Connection::open(&path)?;

        conn.execute_batch("PRAGMA journal_mode=WAL;")?;

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

        // ── Read pool: WAL-mode read-only connections ───────────────────
        let mut read_pool = Vec::with_capacity(READ_POOL_SIZE);
        for i in 0..READ_POOL_SIZE {
            let rc = Connection::open_with_flags(
                &path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )?;
            rc.execute_batch("PRAGMA journal_mode=WAL;")?;
            // Readers don't need secure_delete or auto_vacuum — those are writer-only.
            // Set a busy timeout so readers don't fail during writer checkpoints.
            rc.busy_timeout(std::time::Duration::from_millis(2000))?;
            info!("[engine] Read pool connection {} opened", i);
            read_pool.push(Arc::new(Mutex::new(rc)));
        }

        Ok(SessionStore {
            conn: Arc::new(Mutex::new(conn)),
            read_pool,
            read_idx: AtomicUsize::new(0),
        })
    }

    /// Get a cloneable reference to the database connection.
    /// Used by subsystems that need `Arc<Mutex<Connection>>` (e.g., tool_registry, speculative).
    pub fn conn(&self) -> Arc<Mutex<Connection>> {
        Arc::clone(&self.conn)
    }

    /// Get a read-only connection from the pool (round-robin).
    ///
    /// Use this for search and query operations that don't mutate state.
    /// Multiple threads can hold different read connections simultaneously,
    /// eliminating serialization on the write mutex for read-heavy workloads.
    ///
    /// Falls back to the write connection if the read pool is empty (tests).
    pub fn read_conn(&self) -> Arc<Mutex<Connection>> {
        if self.read_pool.is_empty() {
            return Arc::clone(&self.conn);
        }
        let idx = self.read_idx.fetch_add(1, Ordering::Relaxed) % self.read_pool.len();
        Arc::clone(&self.read_pool[idx])
    }

    /// Open an in-memory database for tests.
    #[cfg(test)]
    pub fn open_in_memory() -> EngineResult<Self> {
        let conn = Connection::open_in_memory()?;
        schema::run_migrations(&conn)?;
        Ok(SessionStore {
            conn: Arc::new(Mutex::new(conn)),
            read_pool: Vec::new(),
            read_idx: AtomicUsize::new(0),
        })
    }

    /// Wrap a raw connection as a lightweight SessionStore (no read pool).
    /// Used by background tasks that open their own connection to the same DB.
    pub fn from_connection(conn: Connection) -> Self {
        SessionStore {
            conn: Arc::new(Mutex::new(conn)),
            read_pool: Vec::new(),
            read_idx: AtomicUsize::new(0),
        }
    }
}

/// Initialise an already-open connection with the full schema.
/// Used by integration tests that create in-memory databases.
pub fn schema_for_testing(conn: &Connection) {
    schema::run_migrations(conn).expect("schema_for_testing: migrations failed");
}
