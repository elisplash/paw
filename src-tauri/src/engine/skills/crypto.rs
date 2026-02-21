// Pawz Agent Engine — Skill Vault Encryption
// AES-256-GCM authenticated encryption with a random key stored in the OS keychain.
// Each encryption generates a fresh 12-byte nonce.
// Storage format: "aes:" + base64(nonce || ciphertext || tag)
// Legacy XOR-encrypted values (no prefix) are auto-migrated on read.

use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use aes_gcm::aead::Aead;
use crate::atoms::error::EngineResult;
use log::{error, info, warn};

const VAULT_KEYRING_SERVICE: &str = "paw-skill-vault";
const VAULT_KEYRING_USER: &str = "encryption-key";

/// Prefix for AES-256-GCM encrypted values (distinguishes from legacy XOR).
const AES_PREFIX: &str = "aes:";

/// Get or create the vault encryption key from the OS keychain.
pub fn get_vault_key() -> EngineResult<Vec<u8>> {
    let entry = keyring::Entry::new(VAULT_KEYRING_SERVICE, VAULT_KEYRING_USER)
        .map_err(|e| {
            error!("[vault] Keyring init failed — OS keychain unavailable: {}", e);
            format!("Keyring init failed: {}", e)
        })?;

    match entry.get_password() {
        Ok(key_b64) => {
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &key_b64)
                .map_err(|e| {
                    error!("[vault] Failed to decode stored vault key: {}", e);
                    format!("Failed to decode vault key: {}", e)
                })
        }
        Err(keyring::Error::NoEntry) => {
            // Generate a new random key
            use rand::Rng;
            let mut key = vec![0u8; 32];
            rand::thread_rng().fill(&mut key[..]);
            let key_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &key);
            entry.set_password(&key_b64)
                .map_err(|e| {
                    error!("[vault] Failed to store vault key in keychain: {}", e);
                    format!("Failed to store vault key in keychain: {}", e)
                })?;
            info!("[vault] Created new vault encryption key in OS keychain");
            Ok(key)
        }
        Err(e) => {
            error!("[vault] OS keychain error — credential storage blocked: {}", e);
            Err(e.into())
        }
    }
}

/// Encrypt a plaintext credential value using AES-256-GCM.
/// Returns "aes:" + base64(nonce || ciphertext_with_tag).
pub fn encrypt_credential(plaintext: &str, key: &[u8]) -> String {
    let cipher = Aes256Gcm::new_from_slice(key)
        .expect("AES-256-GCM key must be 32 bytes");

    // Generate a random 12-byte nonce
    let mut nonce_bytes = [0u8; 12];
    use rand::Rng;
    rand::thread_rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher.encrypt(nonce, plaintext.as_bytes())
        .expect("AES-256-GCM encryption should not fail");

    // Pack: nonce (12) || ciphertext+tag
    let mut packed = Vec::with_capacity(12 + ciphertext.len());
    packed.extend_from_slice(&nonce_bytes);
    packed.extend_from_slice(&ciphertext);

    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &packed);
    format!("{}{}", AES_PREFIX, encoded)
}

/// Decrypt a credential value.
/// Handles both AES-256-GCM ("aes:" prefix) and legacy XOR (no prefix).
pub fn decrypt_credential(encrypted: &str, key: &[u8]) -> EngineResult<String> {
    if let Some(aes_payload) = encrypted.strip_prefix(AES_PREFIX) {
        decrypt_aes_gcm(aes_payload, key)
    } else {
        // Legacy XOR — auto-migrate caller should re-encrypt after reading
        warn!("[vault] Decrypting legacy XOR credential — will migrate to AES-GCM on next save");
        decrypt_xor_legacy(encrypted, key)
    }
}

/// AES-256-GCM decryption. Input is base64(nonce || ciphertext+tag).
fn decrypt_aes_gcm(encoded: &str, key: &[u8]) -> EngineResult<String> {
    let packed = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)?;

    if packed.len() < 12 + 16 {
        // Minimum: 12-byte nonce + 16-byte tag (empty plaintext)
        return Err("Ciphertext too short".into());
    }

    let (nonce_bytes, ciphertext) = packed.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|_| "Invalid key length (expected 32 bytes)")?;

    let plaintext = cipher.decrypt(nonce, ciphertext)
        .map_err(|_| "Decryption failed — wrong key or corrupted data")?;

    String::from_utf8(plaintext)
}

/// Legacy XOR decryption for backward compatibility.
fn decrypt_xor_legacy(encrypted_b64: &str, key: &[u8]) -> EngineResult<String> {
    let encrypted = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encrypted_b64)?;
    let decrypted: Vec<u8> = encrypted.iter().enumerate().map(|(i, b)| b ^ key[i % key.len()]).collect();
    String::from_utf8(decrypted)
}

/// Check if a stored value is using legacy XOR encryption (needs migration).
pub fn is_legacy_encrypted(value: &str) -> bool {
    !value.starts_with(AES_PREFIX)
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
        let mut raw = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, payload).unwrap();
        if let Some(byte) = raw.last_mut() {
            *byte ^= 0xFF;
        }
        let tampered = format!("{}{}", AES_PREFIX,
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &raw));
        assert!(decrypt_credential(&tampered, &key).is_err());
    }

    #[test]
    fn truncated_ciphertext_returns_error() {
        let key = test_key();
        // Too short — less than nonce (12) + tag (16)
        let short = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &[0u8; 10]);
        let result = decrypt_credential(&format!("{}{}", AES_PREFIX, short), &key);
        assert!(result.is_err());
    }

    #[test]
    fn legacy_xor_backward_compat() {
        let key = test_key();
        // Simulate a legacy XOR-encrypted value (no "aes:" prefix)
        let plaintext = "old-api-key-from-before-migration";
        let bytes = plaintext.as_bytes();
        let xor_encrypted: Vec<u8> = bytes.iter().enumerate()
            .map(|(i, b)| b ^ key[i % key.len()])
            .collect();
        let legacy_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &xor_encrypted);

        // Should still decrypt via legacy path
        assert!(is_legacy_encrypted(&legacy_b64));
        let decrypted = decrypt_credential(&legacy_b64, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn is_legacy_detects_correctly() {
        assert!(is_legacy_encrypted("c29tZV9iYXNlNjQ="));        // No prefix
        assert!(!is_legacy_encrypted("aes:c29tZV9iYXNlNjQ="));   // Has prefix
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
