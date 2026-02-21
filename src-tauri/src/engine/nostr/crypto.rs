// Paw Agent Engine — Nostr Cryptography
//
// Event signing (secp256k1 Schnorr / BIP-340), NIP-04 encrypted DMs
// (ECDH + AES-256-CBC), pubkey derivation, and hex utilities.

use serde_json::json;
use crate::atoms::error::EngineResult;

// ── Nostr Event Signing (secp256k1 Schnorr / BIP-340) ─────────────────
//
// NIP-01 event structure:
//   id: sha256([0, pubkey, created_at, kind, tags, content])
//   sig: schnorr signature of id using secret key (via k256 crate)

/// Create and sign a Nostr event with arbitrary kind and tags.
pub(crate) fn sign_event(
    secret_key: &[u8],
    pubkey_hex: &str,
    kind: u64,
    tags: &serde_json::Value,
    content: &str,
) -> EngineResult<serde_json::Value> {
    use sha2::{Sha256, Digest};
    use k256::schnorr::SigningKey;

    let created_at = chrono::Utc::now().timestamp();

    // Serialize for id computation: [0, pubkey, created_at, kind, tags, content]
    let serialized = json!([0, pubkey_hex, created_at, kind, tags, content]);
    let serialized_str = serde_json::to_string(&serialized)?;

    let mut hasher = Sha256::new();
    hasher.update(serialized_str.as_bytes());
    let id_bytes = hasher.finalize();
    let id_hex = hex_encode(&id_bytes);

    // BIP-340 Schnorr signature over the event id
    let signing_key = SigningKey::from_bytes(secret_key)?;
    let aux_rand: [u8; 32] = rand::random();
    let sig = signing_key.sign_raw(&id_bytes, &aux_rand)?;
    let sig_hex = hex_encode(&sig.to_bytes());

    Ok(json!({
        "id": id_hex,
        "pubkey": pubkey_hex,
        "created_at": created_at,
        "kind": kind,
        "tags": tags,
        "content": content,
        "sig": sig_hex,
    }))
}

/// Build a kind-1 public reply event (NIP-01).
pub(crate) fn build_reply_event(
    secret_key: &[u8],
    pubkey_hex: &str,
    content: &str,
    reply_to_id: &str,
    reply_to_pk: &str,
) -> EngineResult<serde_json::Value> {
    let tags = json!([
        ["e", reply_to_id, "", "reply"],
        ["p", reply_to_pk]
    ]);
    sign_event(secret_key, pubkey_hex, 1, &tags, content)
}

// ── NIP-04 Encrypted DMs (ECDH + AES-256-CBC) ─────────────────────────
//
// NIP-04 protocol for kind-4 events:
//   1. ECDH shared secret = x-coordinate of (our_privkey × their_pubkey)
//   2. AES-256-CBC encrypt with random 16-byte IV and PKCS#7 padding
//   3. Content format: base64(ciphertext) + "?iv=" + base64(iv)
//
// Note: NIP-04 is deprecated in favor of NIP-44 (ChaCha20 + HMAC-SHA256)
// with NIP-17 gift wrapping. Kind-4 DMs remain widely supported by
// clients (Damus, Amethyst, Primal, etc.).

/// Compute ECDH shared secret (x-coordinate) between our secret key and a pubkey.
fn compute_shared_secret(secret_key: &[u8], pubkey_hex: &str) -> EngineResult<[u8; 32]> {
    let sk = k256::SecretKey::from_slice(secret_key)?;

    // BIP-340 x-only pubkey → SEC1 compressed (prepend 0x02)
    let pk_bytes = hex_decode(pubkey_hex)?;
    if pk_bytes.len() != 32 {
        return Err(format!("Invalid pubkey length: {} (expected 32)", pk_bytes.len()).into());
    }
    let mut sec1 = Vec::with_capacity(33);
    sec1.push(0x02);
    sec1.extend_from_slice(&pk_bytes);
    let pk = k256::PublicKey::from_sec1_bytes(&sec1)?;

    use k256::elliptic_curve::ecdh::diffie_hellman;
    let shared = diffie_hellman(sk.to_nonzero_scalar(), pk.as_affine());
    let mut out = [0u8; 32];
    out.copy_from_slice(shared.raw_secret_bytes().as_slice());
    Ok(out)
}

/// NIP-04 encrypt: AES-256-CBC with ECDH shared key.
pub(crate) fn nip04_encrypt(secret_key: &[u8], receiver_pk_hex: &str, plaintext: &str) -> EngineResult<String> {
    use base64::Engine;
    use cbc::cipher::{BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};

    let shared = compute_shared_secret(secret_key, receiver_pk_hex)?;
    let iv: [u8; 16] = rand::random();

    let pt = plaintext.as_bytes();
    // Buffer: plaintext + up to 16 bytes PKCS#7 padding
    let mut buf = vec![0u8; pt.len() + 16];
    buf[..pt.len()].copy_from_slice(pt);

    let ciphertext = cbc::Encryptor::<aes::Aes256>::new_from_slices(&shared, &iv)?
        .encrypt_padded_mut::<Pkcs7>(&mut buf, pt.len())?;

    let b64 = base64::engine::general_purpose::STANDARD;
    Ok(format!("{}?iv={}", b64.encode(ciphertext), b64.encode(iv)))
}

/// NIP-04 decrypt: AES-256-CBC with ECDH shared key.
pub(crate) fn nip04_decrypt(secret_key: &[u8], sender_pk_hex: &str, content: &str) -> EngineResult<String> {
    use base64::Engine;
    use cbc::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};

    let parts: Vec<&str> = content.split("?iv=").collect();
    if parts.len() != 2 {
        return Err("Invalid NIP-04 format (expected base64?iv=base64)".into());
    }

    let b64 = base64::engine::general_purpose::STANDARD;
    let ciphertext = b64.decode(parts[0].trim())?;
    let iv = b64.decode(parts[1].trim())?;
    if iv.len() != 16 {
        return Err(format!("Invalid IV length: {} (expected 16)", iv.len()).into());
    }

    let shared = compute_shared_secret(secret_key, sender_pk_hex)?;

    let mut buf = ciphertext;
    let plaintext = cbc::Decryptor::<aes::Aes256>::new_from_slices(&shared, &iv)?
        .decrypt_padded_mut::<Pkcs7>(&mut buf)?;

    String::from_utf8(plaintext.to_vec())
}

// ── secp256k1 Pubkey Derivation (BIP-340 x-only) ──────────────────────
//
// Nostr uses the x-coordinate of the secp256k1 public key (BIP-340).
// We use the `k256` crate (already a dependency for DEX/Ethereum wallet)
// to perform proper elliptic curve point multiplication.

pub(crate) fn derive_pubkey(secret_key: &[u8]) -> EngineResult<Vec<u8>> {
    use k256::elliptic_curve::sec1::ToEncodedPoint;

    let sk = k256::SecretKey::from_slice(secret_key)?;
    let pk = sk.public_key();
    let point = pk.to_encoded_point(true); // compressed
    // BIP-340 x-only: skip the 0x02/0x03 prefix byte, take the 32-byte x-coordinate
    let compressed = point.as_bytes();
    if compressed.len() != 33 {
        return Err("Unexpected compressed pubkey length".into());
    }
    Ok(compressed[1..].to_vec())
}

// ── Hex Utils ──────────────────────────────────────────────────────────

pub(crate) fn hex_decode(hex: &str) -> EngineResult<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return Err("Odd hex length".into());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
        .collect()
}

pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Generate a deterministic test keypair
    fn test_secret_key() -> Vec<u8> {
        // A valid secp256k1 secret key (32 bytes, non-zero, < curve order)
        hex_decode("e8f32e723decf4051aefac8e2c93c9c5b214313817cdb01a1494b917c8436b35").unwrap()
    }

    #[test]
    fn derive_pubkey_produces_32_bytes() {
        let sk = test_secret_key();
        let pk = derive_pubkey(&sk).unwrap();
        assert_eq!(pk.len(), 32);
    }

    #[test]
    fn derive_pubkey_deterministic() {
        let sk = test_secret_key();
        let pk1 = derive_pubkey(&sk).unwrap();
        let pk2 = derive_pubkey(&sk).unwrap();
        assert_eq!(pk1, pk2);
    }

    #[test]
    fn sign_event_produces_valid_fields() {
        let sk = test_secret_key();
        let pk = derive_pubkey(&sk).unwrap();
        let pk_hex = hex_encode(&pk);
        let tags = serde_json::json!([]);
        let event = sign_event(&sk, &pk_hex, 1, &tags, "hello nostr").unwrap();
        assert!(event["id"].as_str().unwrap().len() == 64);
        assert!(event["sig"].as_str().unwrap().len() == 128);
        assert_eq!(event["kind"].as_u64().unwrap(), 1);
        assert_eq!(event["content"].as_str().unwrap(), "hello nostr");
    }

    #[test]
    fn nip04_encrypt_decrypt_roundtrip() {
        // Generate two keypairs
        let sk1 = hex_decode("e8f32e723decf4051aefac8e2c93c9c5b214313817cdb01a1494b917c8436b35").unwrap();
        let sk2 = hex_decode("0b1c4c1a5e0c3d5e7f9a1b3c5d7e9f0a2b4c6d8e0f1a3b5c7d9e1f0a2b4c6d8e").unwrap();
        let pk1 = derive_pubkey(&sk1).unwrap();
        let pk2 = derive_pubkey(&sk2).unwrap();
        let pk1_hex = hex_encode(&pk1);
        let pk2_hex = hex_encode(&pk2);

        let plaintext = "Hello, this is a secret message!";
        let encrypted = nip04_encrypt(&sk1, &pk2_hex, plaintext).unwrap();

        // Encrypted should contain ?iv= separator
        assert!(encrypted.contains("?iv="));

        // Decrypt with sk2 + pk1 (ECDH is symmetric)
        let decrypted = nip04_decrypt(&sk2, &pk1_hex, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn nip04_invalid_format() {
        let sk = test_secret_key();
        let result = nip04_decrypt(&sk, &"00".repeat(32), "no-iv-separator");
        assert!(result.is_err());
    }

    #[test]
    fn hex_encode_decode_roundtrip() {
        let original = vec![0xde, 0xad, 0xbe, 0xef];
        let encoded = hex_encode(&original);
        let decoded = hex_decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn hex_decode_odd_length_errors() {
        assert!(hex_decode("abc").is_err());
    }
}
