// Paw Agent Engine — DEX Token Resolution

use super::constants::{KNOWN_TOKENS, WETH_ADDRESS};
use crate::atoms::error::{EngineError, EngineResult};

/// Resolve a token symbol or address to (address, decimals).
pub(crate) fn resolve_token(symbol_or_address: &str) -> EngineResult<(String, u8)> {
    let input = symbol_or_address.trim().to_uppercase();

    // Check known tokens by symbol
    for (sym, addr, dec) in KNOWN_TOKENS {
        if input == *sym {
            return Ok((addr.to_string(), *dec));
        }
    }

    // Check if it's an address
    let lower = symbol_or_address.trim().to_lowercase();
    if lower.starts_with("0x") && lower.len() == 42 {
        // Unknown token — assume 18 decimals (caller can override)
        return Ok((symbol_or_address.trim().to_string(), 18));
    }

    Err(EngineError::Other(format!(
        "Unknown token '{}'. Use a known symbol ({}) or provide the ERC-20 contract address.",
        symbol_or_address,
        KNOWN_TOKENS.iter().map(|(s, _, _)| *s).collect::<Vec<_>>().join(", ")
    )))
}

/// For swaps, if token_in is "ETH" we need to use WETH as the Uniswap input.
/// Returns (address, decimals, is_native_eth).
pub(crate) fn resolve_for_swap(symbol_or_address: &str) -> EngineResult<(String, u8, bool)> {
    let input = symbol_or_address.trim().to_uppercase();
    if input == "ETH" {
        // Swap uses WETH but sends ETH value
        Ok((WETH_ADDRESS.to_string(), 18, true))
    } else {
        let (addr, dec) = resolve_token(symbol_or_address)?;
        Ok((addr, dec, false))
    }
}
