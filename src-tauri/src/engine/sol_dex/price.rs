// Solana DEX — DexScreener Price Helper
// get_token_price_usd

use std::time::Duration;
use crate::atoms::error::{EngineError, EngineResult};

/// Fetch the current USD price of a Solana token by mint address.
/// Uses DexScreener's token endpoint which returns all pairs for a token.
/// Returns the price from the pair with the highest liquidity.
pub async fn get_token_price_usd(mint: &str) -> EngineResult<f64> {
    let url = format!("https://api.dexscreener.com/latest/dex/tokens/{}", mint);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (compatible; PawAgent/1.0)")
        .build()?;

    let resp = client.get(&url)
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(EngineError::Other(format!("DexScreener returned status {}", resp.status())));
    }

    let body: serde_json::Value = resp.json().await?;

    let pairs = body["pairs"].as_array()
        .ok_or_else(|| EngineError::Other(format!("No pairs found for mint {}", &mint[..std::cmp::min(12, mint.len())])))?;

    if pairs.is_empty() {
        return Err(EngineError::Other(format!("No trading pairs for mint {}", &mint[..std::cmp::min(12, mint.len())])));
    }

    // Find the pair with highest USD liquidity for best price accuracy
    let mut best_price: Option<f64> = None;
    let mut best_liq: f64 = 0.0;

    for pair in pairs {
        if let Some(price_str) = pair["priceUsd"].as_str() {
            if let Ok(price) = price_str.parse::<f64>() {
                let liq = pair["liquidity"]["usd"].as_f64().unwrap_or(0.0);
                if liq > best_liq || best_price.is_none() {
                    best_price = Some(price);
                    best_liq = liq;
                }
            }
        }
    }

    best_price.ok_or_else(|| EngineError::Other(format!("No USD price found for mint {}", &mint[..std::cmp::min(12, mint.len())])))
}
