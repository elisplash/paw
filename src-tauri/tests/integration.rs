// Single integration test binary — consolidates all test files to avoid
// linking 4 separate 700MB binaries (OOM on codespace).
//
// Run with: cargo test --test integration
//
// Each module retains its original tests unchanged.

use parking_lot::Mutex;
use paw_temp_lib::engine::sessions::SessionStore;
use rusqlite::Connection;
use std::sync::Arc;

/// Shared test helper: in-memory SessionStore.
pub fn test_store() -> SessionStore {
    let conn = Connection::open_in_memory().expect("Failed to open in-memory DB");
    conn.execute_batch("PRAGMA journal_mode=WAL;").ok();
    paw_temp_lib::engine::sessions::schema_for_testing(&conn);
    SessionStore {
        conn: Arc::new(Mutex::new(conn)),
    }
}

mod config_persistence;
mod loop_detection;
mod memory_roundtrip;
mod session_lifecycle;
mod tool_classification;
