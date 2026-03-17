// ── Engram: SQLCipher Integration (§10.1-10.3) ──────────────────────────────
//
// Full-database encryption via SQLCipher.
//
// When the `sqlcipher` feature is enabled at compile time, the bundled
// SQLite is replaced with SQLCipher. At runtime, this module:
//   1. Derives a database encryption key from the master keychain key
//   2. Applies PRAGMA key on every connection open
//   3. Provides a migration path from plaintext → encrypted DB
//
// Without the `sqlcipher` feature, all functions are no-ops.

#[cfg(feature = "sqlcipher")]
use crate::atoms::error::EngineError;
use crate::atoms::error::EngineResult;
#[cfg(feature = "sqlcipher")]
use log::info;
#[cfg(feature = "sqlcipher")]
use log::warn;
use rusqlite::Connection;

/// Apply SQLCipher encryption key to a database connection.
///
/// Must be called immediately after `Connection::open` and before any
/// other SQL statements. On non-SQLCipher builds, this is a no-op.
pub fn apply_encryption_key(conn: &Connection) -> EngineResult<()> {
    #[cfg(feature = "sqlcipher")]
    {
        let key = derive_db_key()?;
        let key_hex = key.iter().map(|b| format!("{:02x}", b)).collect::<String>();
        conn.execute_batch(&format!("PRAGMA key = \"x'{}'\";", key_hex))
            .map_err(|e| EngineError::Other(format!("SQLCipher PRAGMA key failed: {}", e)))?;
        conn.execute_batch("PRAGMA cipher_memory_security = ON;")
            .map_err(|e| EngineError::Other(format!("SQLCipher memory security failed: {}", e)))?;
        info!("[sqlcipher] Database encryption key applied");
    }
    #[cfg(not(feature = "sqlcipher"))]
    {
        let _ = conn;
    }
    Ok(())
}

/// Check if the current build has SQLCipher support.
pub fn is_sqlcipher_available() -> bool {
    cfg!(feature = "sqlcipher")
}

/// Migrate a plaintext database to SQLCipher encryption.
///
/// This creates a new encrypted database, copies all data, then
/// replaces the original file. The original is securely overwritten.
///
/// Only available when `sqlcipher` feature is enabled.
#[cfg(feature = "sqlcipher")]
pub fn migrate_to_encrypted(db_path: &std::path::Path) -> EngineResult<()> {
    use std::fs;

    let encrypted_path = db_path.with_extension("db.encrypted");
    let key = derive_db_key()?;
    let key_hex = key.iter().map(|b| format!("{:02x}", b)).collect::<String>();

    // Open plaintext source
    let src = Connection::open(db_path)
        .map_err(|e| EngineError::Other(format!("Cannot open source DB: {}", e)))?;

    // Attach encrypted destination
    src.execute_batch(&format!(
        "ATTACH DATABASE '{}' AS encrypted KEY \"x'{}'\";",
        encrypted_path.display(),
        key_hex
    ))
    .map_err(|e| EngineError::Other(format!("Cannot attach encrypted DB: {}", e)))?;

    // Export all data
    src.execute_batch("SELECT sqlcipher_export('encrypted');")
        .map_err(|e| EngineError::Other(format!("sqlcipher_export failed: {}", e)))?;

    src.execute_batch("DETACH DATABASE encrypted;")
        .map_err(|e| EngineError::Other(format!("Detach failed: {}", e)))?;

    drop(src);

    // Replace original with encrypted version
    let backup_path = db_path.with_extension("db.plaintext.bak");
    fs::rename(db_path, &backup_path)
        .map_err(|e| EngineError::Other(format!("Cannot backup original DB: {}", e)))?;
    fs::rename(&encrypted_path, db_path)
        .map_err(|e| EngineError::Other(format!("Cannot replace with encrypted DB: {}", e)))?;

    // Securely overwrite the plaintext backup
    let metadata = fs::metadata(&backup_path)
        .map_err(|e| EngineError::Other(format!("Cannot read backup metadata: {}", e)))?;
    let size = metadata.len() as usize;
    let zeros = vec![0u8; size.min(1024 * 1024)]; // Cap at 1MB chunks
    if let Ok(mut f) = fs::OpenOptions::new().write(true).open(&backup_path) {
        use std::io::Write;
        let mut remaining = size;
        while remaining > 0 {
            let chunk = remaining.min(zeros.len());
            let _ = f.write_all(&zeros[..chunk]);
            remaining -= chunk;
        }
    }
    let _ = fs::remove_file(&backup_path);

    info!(
        "[sqlcipher] Migration complete: {} is now encrypted",
        db_path.display()
    );
    Ok(())
}

/// Derive the database encryption key from the master keychain key.
#[cfg(feature = "sqlcipher")]
fn derive_db_key() -> EngineResult<[u8; 32]> {
    use hkdf::Hkdf;
    use sha2::Sha256;

    let master = super::encryption::get_memory_encryption_key()?;
    let salt = b"engram-sqlcipher-db-key-v1";
    let hk = Hkdf::<Sha256>::new(Some(salt), &master);
    let mut key = [0u8; 32];
    hk.expand(b"database", &mut key)
        .map_err(|e| EngineError::Other(format!("HKDF expand failed: {}", e)))?;
    Ok(key)
}
