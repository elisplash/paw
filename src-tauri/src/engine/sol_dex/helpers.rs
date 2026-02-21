// Solana DEX — Helpers
// parse_solana_keypair, slippage_pct, resolve_token, lamports_to_amount, amount_to_lamports

use super::constants::KNOWN_TOKENS;
use crate::atoms::error::{EngineError, EngineResult};

// ── Keypair Parsing ───────────────────────────────────────────────────

/// Decode a base58-encoded Solana keypair and extract the 32-byte secret key.
pub(crate) fn parse_solana_keypair(private_key_b58: &str) -> EngineResult<[u8; 32]> {
    let keypair_bytes = bs58::decode(private_key_b58).into_vec()
        .map_err(|e| EngineError::Other(format!("Invalid private key encoding: {}", e)))?;
    if keypair_bytes.len() < 64 {
        return Err(EngineError::Other("Invalid Solana keypair (expected 64 bytes)".into()));
    }
    let mut secret_bytes = [0u8; 32];
    secret_bytes.copy_from_slice(&keypair_bytes[..32]);
    Ok(secret_bytes)
}

// ── Slippage ──────────────────────────────────────────────────────────

/// Convert slippage BPS to a whole-percent value (minimum 1%).
pub(crate) fn slippage_pct(bps: u64) -> u64 {
    std::cmp::max(bps / 100, 1)
}


/// Resolve a token symbol or mint address to (mint_address, decimals)
/// Returns decimals=0 for unknown tokens — caller should use resolve_decimals_on_chain()
pub(crate) fn resolve_token(sym_or_addr: &str) -> EngineResult<(String, u8)> {
    let upper = sym_or_addr.trim().to_uppercase();

    // Check known tokens
    for (sym, addr, dec) in KNOWN_TOKENS {
        if upper == *sym {
            return Ok((addr.to_string(), *dec));
        }
    }

    // If it looks like a base58 address (32-44 chars, base58 alphabet)
    let trimmed = sym_or_addr.trim();
    if trimmed.len() >= 32 && trimmed.len() <= 44 && bs58::decode(trimmed).into_vec().is_ok() {
        // Unknown decimals — we'll query on-chain
        return Ok((trimmed.to_string(), 0));
    }

    Err(EngineError::Other(format!("Unknown Solana token: '{}'. Use a mint address or known symbol: {}", sym_or_addr,
        KNOWN_TOKENS.iter().map(|(s, _, _)| *s).collect::<Vec<_>>().join(", "))))
}

/// Format lamports to SOL (9 decimals) or SPL token amount
pub(crate) fn lamports_to_amount(lamports: u64, decimals: u8) -> String {
    if decimals == 0 {
        return lamports.to_string();
    }
    let divisor = 10u64.pow(decimals as u32);
    let whole = lamports / divisor;
    let frac = lamports % divisor;
    if frac == 0 {
        whole.to_string()
    } else {
        let frac_str = format!("{:0>width$}", frac, width = decimals as usize);
        let trimmed = frac_str.trim_end_matches('0');
        format!("{}.{}", whole, trimmed)
    }
}

/// Parse amount string (e.g. "1.5") to smallest units given decimals
pub(crate) fn amount_to_lamports(amount_str: &str, decimals: u8) -> EngineResult<u64> {
    let amount_str = amount_str.trim();
    if let Some(dot_pos) = amount_str.find('.') {
        let whole: u64 = amount_str[..dot_pos].parse().map_err(|e| EngineError::Other(format!("Invalid amount: {}", e)))?;
        let frac_str = &amount_str[dot_pos + 1..];
        let frac_len = frac_str.len();
        if frac_len > decimals as usize {
            return Err(EngineError::Other(format!("Too many decimal places (max {})", decimals)));
        }
        let frac: u64 = frac_str.parse().map_err(|e| EngineError::Other(format!("Invalid fractional: {}", e)))?;
        let multiplier = 10u64.pow((decimals as u32) - frac_len as u32);
        Ok(whole * 10u64.pow(decimals as u32) + frac * multiplier)
    } else {
        let whole: u64 = amount_str.parse().map_err(|e| EngineError::Other(format!("Invalid amount: {}", e)))?;
        Ok(whole * 10u64.pow(decimals as u32))
    }
}
