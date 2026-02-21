// Solana DEX — Transaction Building & Signing
// sign_solana_transaction, decode/encode_compact_u16, build_solana_transaction, derive_ata

use log::info;use crate::atoms::error::{EngineError, EngineResult};
// ── Transaction Signing ───────────────────────────────────────────────

/// Sign a pre-built Solana transaction (legacy or versioned v0).
///
/// **Legacy:**
///   [num_signatures (compact-u16)] [signature_slots (N×64)] [message]
///
/// **Versioned (v0):**
///   [0x80 version prefix] [num_signatures (compact-u16)] [signature_slots (N×64)] [versioned_message]
///
/// If byte 0 has high bit set (>= 0x80), it's a versioned tx (version = byte & 0x7F).
/// We sign the message portion with ed25519 and place the signature in the first slot.
pub(crate) fn sign_solana_transaction(tx_bytes: &[u8], secret_key: &[u8; 32]) -> EngineResult<Vec<u8>> {
    if tx_bytes.is_empty() {
        return Err(EngineError::Other("Empty transaction".into()));
    }

    use ed25519_dalek::{Signer, SigningKey};

    let signing_key = SigningKey::from_bytes(secret_key);

    // Detect versioned vs legacy transaction
    // If first byte has high bit set, it's a versioned transaction prefix
    let (version_prefix_len, is_versioned) = if tx_bytes[0] >= 0x80 {
        let version = tx_bytes[0] & 0x7F;
        info!("[sol_dex] Detected versioned transaction (v{})", version);
        (1usize, true)
    } else {
        (0usize, false)
    };

    // Parse compact-u16 for num_signatures (after version prefix if present)
    let (num_sigs, sig_header_len) = decode_compact_u16(&tx_bytes[version_prefix_len..])?;
    if num_sigs == 0 {
        return Err(EngineError::Other("Transaction has 0 signatures required".into()));
    }

    let sigs_start = version_prefix_len + sig_header_len;
    let sigs_end = sigs_start + (num_sigs as usize * 64);
    if sigs_end > tx_bytes.len() {
        return Err(EngineError::Other(format!("Transaction too short: need {} bytes for {} signatures, have {} (versioned={})",
            sigs_end, num_sigs, tx_bytes.len(), is_versioned)));
    }

    // Message is everything after the signature slots
    let message = &tx_bytes[sigs_end..];

    // For versioned transactions, we need to sign: [version_prefix] + [message]
    // For legacy transactions, we just sign: [message]
    let signature = if is_versioned {
        // Versioned: the "message" that gets signed includes the version prefix byte
        let mut signable = Vec::with_capacity(1 + message.len());
        signable.push(tx_bytes[0]); // version prefix (0x80 for v0)
        signable.extend_from_slice(message);
        signing_key.sign(&signable)
    } else {
        signing_key.sign(message)
    };

    // Build the signed transaction
    let mut signed = tx_bytes.to_vec();
    // Place our signature in the first slot
    signed[sigs_start..sigs_start + 64].copy_from_slice(&signature.to_bytes());

    info!("[sol_dex] Transaction signed (versioned={}, sigs={}, msg_len={})", is_versioned, num_sigs, message.len());

    Ok(signed)
}

/// Decode Solana compact-u16 encoding
/// Returns (value, bytes_consumed)
pub(crate) fn decode_compact_u16(data: &[u8]) -> EngineResult<(u16, usize)> {
    if data.is_empty() {
        return Err(EngineError::Other("Empty data for compact-u16".into()));
    }

    let first = data[0] as u16;
    if first < 0x80 {
        return Ok((first, 1));
    }

    if data.len() < 2 {
        return Err(EngineError::Other("Truncated compact-u16".into()));
    }
    let second = data[1] as u16;
    if second < 0x80 {
        return Ok(((first & 0x7F) | (second << 7), 2));
    }

    if data.len() < 3 {
        return Err(EngineError::Other("Truncated compact-u16".into()));
    }
    let third = data[2] as u16;
    Ok(((first & 0x7F) | ((second & 0x7F) << 7) | (third << 14), 3))
}

/// Encode a compact-u16 value (Solana serialization)
pub(crate) fn encode_compact_u16(val: u16) -> Vec<u8> {
    if val < 0x80 {
        vec![val as u8]
    } else if val < 0x4000 {
        vec![(val & 0x7F | 0x80) as u8, (val >> 7) as u8]
    } else {
        vec![(val & 0x7F | 0x80) as u8, ((val >> 7) & 0x7F | 0x80) as u8, (val >> 14) as u8]
    }
}

/// Build a Solana legacy transaction from scratch (message + signature slots).
/// accounts: Vec of (pubkey_bytes, is_signer, is_writable)
/// instructions: Vec of (program_id_index, account_indices, data)
pub(crate) fn build_solana_transaction(
    recent_blockhash: &[u8; 32],
    accounts: &[([u8; 32], bool, bool)],
    instructions: &[(u8, Vec<u8>, Vec<u8>)],
) -> Vec<u8> {
    // Count signers and read-only accounts
    let num_signers = accounts.iter().filter(|(_, s, _)| *s).count() as u8;
    let num_readonly_signed = accounts.iter().filter(|(_, s, w)| *s && !*w).count() as u8;
    let num_readonly_unsigned = accounts.iter().filter(|(_, s, w)| !*s && !*w).count() as u8;

    // Build message
    let mut message = Vec::new();
    // Header: num_required_signatures, num_readonly_signed, num_readonly_unsigned
    message.push(num_signers);
    message.push(num_readonly_signed);
    message.push(num_readonly_unsigned);

    // Account addresses (compact-u16 length + keys)
    message.extend_from_slice(&encode_compact_u16(accounts.len() as u16));
    for (pubkey, _, _) in accounts {
        message.extend_from_slice(pubkey);
    }

    // Recent blockhash
    message.extend_from_slice(recent_blockhash);

    // Instructions
    message.extend_from_slice(&encode_compact_u16(instructions.len() as u16));
    for (program_id_idx, acct_indices, data) in instructions {
        message.push(*program_id_idx);
        // Account indices
        message.extend_from_slice(&encode_compact_u16(acct_indices.len() as u16));
        message.extend_from_slice(acct_indices);
        // Data
        message.extend_from_slice(&encode_compact_u16(data.len() as u16));
        message.extend_from_slice(data);
    }

    // Full transaction: num_signatures (compact-u16) + signature slots (zeroed) + message
    let mut tx = Vec::new();
    tx.extend_from_slice(&encode_compact_u16(num_signers as u16));
    // Zeroed signature slots
    for _ in 0..num_signers {
        tx.extend_from_slice(&[0u8; 64]);
    }
    tx.extend_from_slice(&message);
    tx
}

/// Derive Associated Token Account (ATA) address
/// ATA = PDA of [wallet, TOKEN_PROGRAM_ID, mint] with ATA_PROGRAM_ID
pub(crate) fn derive_ata(wallet: &[u8; 32], mint: &[u8; 32], token_program: &[u8; 32]) -> EngineResult<[u8; 32]> {
    use sha2::Digest;
    let ata_program = bs58::decode("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL")
        .into_vec().map_err(|e| EngineError::Other(format!("ATA program decode: {}", e)))?;

    // PDA: sha256([wallet, token_program, mint, ata_program, "ProgramDerivedAddress"])
    // Try finding a valid PDA (bump seed from 255 down to 0)
    for bump in (0u8..=255).rev() {
        let mut hasher = sha2::Sha256::new();
        hasher.update(wallet);
        hasher.update(token_program);
        hasher.update(mint);
        hasher.update(&[bump]);
        hasher.update(&ata_program);
        hasher.update(b"ProgramDerivedAddress");
        let hash = hasher.finalize();

        // Valid PDA must NOT be on the ed25519 curve
        // We check by trying to decompress as an ed25519 point
        let point_bytes: [u8; 32] = hash[..32].try_into().unwrap();
        if ed25519_dalek::VerifyingKey::from_bytes(&point_bytes).is_err() {
            return Ok(point_bytes);
        }
    }
    Err(EngineError::Other("Could not derive ATA address".into()))
}
