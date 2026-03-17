// ── Engram: Encrypted Export/Import (§10.9) ─────────────────────────────────
//
// Encrypted memory backup and restore.
// Exports memories as an AES-256-GCM encrypted JSON archive with
// integrity verification via HMAC.
//
// Format:
//   Magic: "ENGRAM-EXPORT-V1\n"
//   HMAC-SHA256(archive_key, ciphertext) — 64 hex chars + "\n"
//   Base64(AES-256-GCM(archive_key, JSON(MemoryExport)))
//
// The archive key is derived from a user-supplied passphrase via HKDF
// (not Argon2 — passphrase is combined with the master keychain key,
// so brute-force is already blocked by the keychain).

use crate::atoms::engram_types::EpisodicMemory;
use crate::atoms::error::{EngineError, EngineResult};
use crate::engine::sessions::SessionStore;
use log::info;
use serde::{Deserialize, Serialize};

/// Magic bytes at the start of an export file.
const EXPORT_MAGIC: &str = "ENGRAM-EXPORT-V1\n";

/// Exported memory archive (plaintext, before encryption).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryExport {
    /// Export format version.
    pub version: u32,
    /// When this export was created.
    pub exported_at: String,
    /// Agent ID that owns the memories (or "global").
    pub agent_id: String,
    /// Episodic memories.
    pub episodic: Vec<EpisodicMemory>,
}

/// Derive an archive encryption key from a passphrase + master key.
fn derive_archive_key(passphrase: &str) -> EngineResult<[u8; 32]> {
    use hkdf::Hkdf;
    use sha2::Sha256;

    let master = super::encryption::get_memory_encryption_key()?;
    let salt = format!("engram-export-v1:{}", passphrase);
    let hk = Hkdf::<Sha256>::new(Some(salt.as_bytes()), &master);
    let mut key = [0u8; 32];
    hk.expand(b"archive", &mut key)
        .map_err(|e| EngineError::Other(format!("HKDF expand failed: {}", e)))?;
    Ok(key)
}

/// Export all memories for an agent (or global) as an encrypted archive.
///
/// Returns the encrypted archive bytes ready to be written to a file.
pub fn export_encrypted(
    store: &SessionStore,
    agent_id: &str,
    passphrase: &str,
) -> EngineResult<Vec<u8>> {
    use crate::atoms::engram_types::MemoryScope;

    let scope = if agent_id == "global" {
        MemoryScope::global()
    } else {
        MemoryScope {
            global: false,
            agent_id: Some(agent_id.to_string()),
            ..Default::default()
        }
    };

    // Collect episodic memories (the primary tier)
    let episodic = store.engram_list_episodic(&scope, None, 100_000)?;

    let export = MemoryExport {
        version: 1,
        exported_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        agent_id: agent_id.to_string(),
        episodic,
    };

    let json = serde_json::to_string(&export)
        .map_err(|e| EngineError::Other(format!("JSON serialization failed: {}", e)))?;

    // Encrypt
    let key = derive_archive_key(passphrase)?;
    let ciphertext = super::encryption::encrypt_memory_content(&json, &key)?;

    // HMAC
    let hmac_hex = compute_archive_hmac(&key, ciphertext.as_bytes())?;

    // Assemble: magic + HMAC + ciphertext
    let mut output = Vec::new();
    output.extend_from_slice(EXPORT_MAGIC.as_bytes());
    output.extend_from_slice(hmac_hex.as_bytes());
    output.push(b'\n');
    output.extend_from_slice(ciphertext.as_bytes());

    info!(
        "[engram:export] Exported {} episodic memories for '{}'",
        export.episodic.len(),
        agent_id,
    );

    Ok(output)
}

/// Import memories from an encrypted archive.
///
/// Decrypts and verifies the archive, then stores all memories.
/// Existing memories with the same ID are skipped (no overwrite).
pub fn import_encrypted(
    store: &SessionStore,
    archive: &[u8],
    passphrase: &str,
) -> EngineResult<ImportReport> {
    // Parse: magic + HMAC + ciphertext
    let archive_str = std::str::from_utf8(archive)
        .map_err(|e| EngineError::Other(format!("Invalid UTF-8: {}", e)))?;

    if !archive_str.starts_with(EXPORT_MAGIC) {
        return Err(EngineError::Other(
            "Invalid export file: missing magic header".into(),
        ));
    }

    let rest = &archive_str[EXPORT_MAGIC.len()..];
    let newline_pos = rest
        .find('\n')
        .ok_or_else(|| EngineError::Other("Invalid export file: missing HMAC".into()))?;

    let stored_hmac = &rest[..newline_pos];
    let ciphertext = &rest[newline_pos + 1..];

    // Verify HMAC
    let key = derive_archive_key(passphrase)?;
    let expected_hmac = compute_archive_hmac(&key, ciphertext.as_bytes())?;

    let valid: bool =
        subtle::ConstantTimeEq::ct_eq(stored_hmac.as_bytes(), expected_hmac.as_bytes()).into();
    if !valid {
        return Err(EngineError::Other(
            "Archive integrity check failed — wrong passphrase or corrupted file".into(),
        ));
    }

    // Decrypt
    let json = super::encryption::decrypt_memory_content(ciphertext, &key)?;
    let export: MemoryExport = serde_json::from_str(&json)
        .map_err(|e| EngineError::Other(format!("Invalid archive JSON: {}", e)))?;

    // Import
    let mut imported_episodic = 0usize;
    let mut skipped = 0usize;

    for mem in &export.episodic {
        match store.engram_store_episodic(mem) {
            Ok(_) => imported_episodic += 1,
            Err(_) => skipped += 1, // likely duplicate ID
        }
    }

    info!(
        "[engram:import] Imported {} episodic ({} skipped) from '{}'",
        imported_episodic, skipped, export.agent_id,
    );

    Ok(ImportReport {
        imported_episodic,
        skipped,
        source_agent: export.agent_id,
        exported_at: export.exported_at,
    })
}

/// Report from an import operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportReport {
    pub imported_episodic: usize,
    pub skipped: usize,
    pub source_agent: String,
    pub exported_at: String,
}

/// Compute HMAC-SHA256 for archive integrity.
fn compute_archive_hmac(key: &[u8], data: &[u8]) -> EngineResult<String> {
    use hkdf::Hkdf;
    use sha2::Sha256;

    // Derive a separate HMAC key from the archive key
    let hk = Hkdf::<Sha256>::new(Some(b"engram-archive-hmac-v1"), key);
    let mut hmac_key = [0u8; 32];
    hk.expand(b"hmac", &mut hmac_key)
        .map_err(|e| EngineError::Other(format!("HKDF expand failed: {}", e)))?;

    let mut mac = <hmac::Hmac<Sha256> as hmac::Mac>::new_from_slice(&hmac_key)
        .map_err(|e| EngineError::Other(format!("HMAC init failed: {}", e)))?;
    hmac::Mac::update(&mut mac, data);
    let result = hmac::Mac::finalize(mac);
    let hex_str: String = result
        .into_bytes()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();
    Ok(hex_str)
}
