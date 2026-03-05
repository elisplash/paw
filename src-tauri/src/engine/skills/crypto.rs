// Pawz Agent Engine — Skill Vault Encryption
// AES-256-GCM authenticated encryption with a random key stored in the OS keychain.
// Each encryption generates a fresh 12-byte nonce.
// Storage format: "aes:" + base64(nonce || ciphertext || tag)

use crate::atoms::error::{EngineError, EngineResult};
use crate::engine::key_vault;
use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use log::{error, info};
use std::sync::RwLock;
use zeroize::Zeroizing;

/// Prefix for AES-256-GCM encrypted values.
const AES_PREFIX: &str = "aes:";

/// Expected AES-256 key length in bytes.
const EXPECTED_KEY_LEN: usize = 32;

/// In-memory cache for the vault encryption key.
/// - `RwLock` allows concurrent readers (5–20+ per chat turn) without blocking.
/// - `Zeroizing<Vec<u8>>` securely zeros key material when the value is dropped
///   or replaced, preventing it from lingering in freed memory.
/// - `unwrap_or_else(|e| e.into_inner())` recovers from a poisoned lock instead
///   of panicking the whole app.
static VAULT_KEY_CACHE: RwLock<Option<Zeroizing<Vec<u8>>>> = RwLock::new(None);

/// Get or create the vault encryption key from the OS keychain.
/// The result is cached in-memory for the lifetime of the process so the
/// OS keychain is only accessed once per session.
///
/// Security properties:
/// - Key material wrapped in `Zeroizing` — zeroed on drop/replace.
/// - RwLock for concurrent reads without contention.
/// - Poison-safe — recovers from panicked threads instead of crashing.
/// - Key length validated before caching.
/// - Returns `Zeroizing<Vec<u8>>` so callers' copies are also zeroed on drop.
pub fn get_vault_key() -> EngineResult<Zeroizing<Vec<u8>>> {
    // Fast path: return cached key (read lock — many readers allowed)
    {
        let guard = VAULT_KEY_CACHE.read().unwrap_or_else(|e| e.into_inner());
        if let Some(ref key) = *guard {
            return Ok(Zeroizing::new(key.to_vec()));
        }
    }
    // Slow path: acquire write lock and double-check (prevents TOCTOU race
    // where two threads both see None in the read lock above)
    let mut guard = VAULT_KEY_CACHE.write().unwrap_or_else(|e| e.into_inner());
    if let Some(ref key) = *guard {
        return Ok(Zeroizing::new(key.to_vec()));
    }
    // Read from OS keychain (only happens once per session)
    let key = load_vault_key_from_keychain()?;
    if key.len() != EXPECTED_KEY_LEN {
        error!(
            "[vault] Keychain returned key with unexpected length {} (expected {})",
            key.len(),
            EXPECTED_KEY_LEN
        );
        return Err(EngineError::Other(format!(
            "Vault key length mismatch: got {} bytes, expected {}",
            key.len(),
            EXPECTED_KEY_LEN
        )));
    }
    let result = Zeroizing::new(key.to_vec());
    *guard = Some(key); // key is already Zeroizing<Vec<u8>> from load fn
    info!("[vault] Vault key loaded and cached from OS keychain");
    Ok(result)
}

/// Read (or create) the vault key from the unified key vault.
/// Returns `Zeroizing<Vec<u8>>` so callers don't need to manually zero.
fn load_vault_key_from_keychain() -> EngineResult<Zeroizing<Vec<u8>>> {
    if let Some(key_b64) = key_vault::get(key_vault::PURPOSE_SKILL_VAULT) {
        let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &key_b64)
            .map_err(|e| {
                error!("[vault] Failed to decode stored vault key: {}", e);
                EngineError::Other(format!("Failed to decode vault key: {}", e))
            })?;
        return Ok(Zeroizing::new(decoded));
    }
    // No key exists — generate a new random key using OS CSPRNG
    use rand::rngs::OsRng;
    use rand::RngCore;
    let mut key = Zeroizing::new(vec![0u8; 32]);
    OsRng.fill_bytes(&mut key);
    let key_b64 = Zeroizing::new(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        key.as_slice(),
    ));
    key_vault::set(key_vault::PURPOSE_SKILL_VAULT, &key_b64);
    info!("[vault] Created new vault encryption key in unified vault");
    Ok(key)
}

/// Encrypt a plaintext credential value using AES-256-GCM.
/// Returns "aes:" + base64(nonce || ciphertext_with_tag).
pub fn encrypt_credential(plaintext: &str, key: &[u8]) -> String {
    let cipher = Aes256Gcm::new_from_slice(key).expect("AES-256-GCM key must be 32 bytes");

    // Generate a random 12-byte nonce using OS CSPRNG
    let mut nonce_bytes = [0u8; 12];
    use rand::rngs::OsRng;
    use rand::RngCore;
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .expect("AES-256-GCM encryption should not fail");

    // Pack: nonce (12) || ciphertext+tag
    let mut packed = Vec::with_capacity(12 + ciphertext.len());
    packed.extend_from_slice(&nonce_bytes);
    packed.extend_from_slice(&ciphertext);

    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &packed);
    format!("{}{}", AES_PREFIX, encoded)
}

/// Decrypt a credential value (AES-256-GCM only).
/// Expects the "aes:" prefix — rejects any other format.
pub fn decrypt_credential(encrypted: &str, key: &[u8]) -> EngineResult<String> {
    match encrypted.strip_prefix(AES_PREFIX) {
        Some(aes_payload) => decrypt_aes_gcm(aes_payload, key),
        None => Err(EngineError::Other(
            "Unrecognised credential format (expected AES-256-GCM)".into(),
        )),
    }
}

/// AES-256-GCM decryption. Input is base64(nonce || ciphertext+tag).
fn decrypt_aes_gcm(encoded: &str, key: &[u8]) -> EngineResult<String> {
    let packed = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
        .map_err(|e| EngineError::Other(e.to_string()))?;

    if packed.len() < 12 + 16 {
        // Minimum: 12-byte nonce + 16-byte tag (empty plaintext)
        return Err("Ciphertext too short".into());
    }

    let (nonce_bytes, ciphertext) = packed.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|_| "Invalid key length (expected 32 bytes)")?;

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| "Decryption failed — wrong key or corrupted data")?;

    String::from_utf8(plaintext).map_err(|e| EngineError::Other(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> Vec<u8> {
        vec![0xAB; 32]
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = test_key();
        let plaintext = "sk-live-abc123_secret_token";
        let encrypted = encrypt_credential(plaintext, &key);
        assert!(encrypted.starts_with(AES_PREFIX));
        let decrypted = decrypt_credential(&encrypted, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encrypt_decrypt_empty_string() {
        let key = test_key();
        let encrypted = encrypt_credential("", &key);
        assert!(encrypted.starts_with(AES_PREFIX));
        let decrypted = decrypt_credential(&encrypted, &key).unwrap();
        assert_eq!(decrypted, "");
    }

    #[test]
    fn wrong_key_returns_error() {
        let key1 = vec![0xAB; 32];
        let key2 = vec![0xCD; 32];
        let plaintext = "my-secret-api-key";
        let encrypted = encrypt_credential(plaintext, &key1);
        // AES-GCM detects wrong key via authentication tag — returns Err, not garbage
        let result = decrypt_credential(&encrypted, &key2);
        assert!(result.is_err());
    }

    #[test]
    fn encrypt_long_text_beyond_key_length() {
        let key = vec![0x42; 32];
        let plaintext = "x".repeat(1000); // much longer than 32-byte key
        let encrypted = encrypt_credential(&plaintext, &key);
        let decrypted = decrypt_credential(&encrypted, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn invalid_base64_returns_error() {
        let key = test_key();
        let result = decrypt_credential(&format!("{}not!valid!base64!!!", AES_PREFIX), &key);
        assert!(result.is_err());
    }

    #[test]
    fn each_encryption_produces_different_ciphertext() {
        let key = test_key();
        let plaintext = "same-input-every-time";
        let enc1 = encrypt_credential(plaintext, &key);
        let enc2 = encrypt_credential(plaintext, &key);
        // Random nonce means different ciphertext each time
        assert_ne!(enc1, enc2);
        // Both decrypt to the same plaintext
        assert_eq!(decrypt_credential(&enc1, &key).unwrap(), plaintext);
        assert_eq!(decrypt_credential(&enc2, &key).unwrap(), plaintext);
    }

    #[test]
    fn tampered_ciphertext_returns_error() {
        let key = test_key();
        let encrypted = encrypt_credential("sensitive-data", &key);
        // Flip a byte in the base64-encoded ciphertext
        let payload = &encrypted[AES_PREFIX.len()..];
        let mut raw =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, payload).unwrap();
        if let Some(byte) = raw.last_mut() {
            *byte ^= 0xFF;
        }
        let tampered = format!(
            "{}{}",
            AES_PREFIX,
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &raw)
        );
        assert!(decrypt_credential(&tampered, &key).is_err());
    }

    #[test]
    fn truncated_ciphertext_returns_error() {
        let key = test_key();
        // Too short — less than nonce (12) + tag (16)
        let short = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, [0u8; 10]);
        let result = decrypt_credential(&format!("{}{}", AES_PREFIX, short), &key);
        assert!(result.is_err());
    }

    #[test]
    fn unrecognised_format_returns_error() {
        let key = test_key();
        let result = decrypt_credential("not-aes-prefix-data", &key);
        assert!(result.is_err());
    }

    #[test]
    fn unicode_plaintext_roundtrip() {
        let key = test_key();
        let plaintext = "p@$$w0rd-with-emojis-🔑🛡️-and-日本語";
        let encrypted = encrypt_credential(plaintext, &key);
        let decrypted = decrypt_credential(&encrypted, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}
