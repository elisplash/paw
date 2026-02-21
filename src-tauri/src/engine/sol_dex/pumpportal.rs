// Solana DEX — PumpPortal Fallback
// is_jupiter_route_error, pumpportal_get_tx, pumpportal_swap

use log::info;
use std::time::Duration;
use super::constants::PUMPPORTAL_API;
use super::helpers::slippage_pct;
use super::rpc::{rpc_call, check_tx_confirmation};
use super::transaction::sign_solana_transaction;
use crate::atoms::error::EngineResult;

// ── PumpPortal Fallback ────────────────────────────────────────────────

/// Check if a Jupiter error indicates a routing failure (no liquidity path).
/// These errors mean we should try PumpPortal as a fallback.
pub(crate) fn is_jupiter_route_error(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("no route")
        || lower.contains("0x1788")
        || lower.contains("route not found")
        || lower.contains("no valid route")
        || lower.contains("could not find any routes")
        || lower.contains("insufficient liquidity")
}

/// Call PumpPortal's local-trade API to get a serialized transaction for signing.
/// This routes through pump.fun bonding curve, PumpSwap AMM, Raydium, etc.
/// Returns the raw transaction bytes ready for signing.
pub(crate) async fn pumpportal_get_tx(
    wallet_pubkey: &str,
    action: &str,        // "buy" or "sell"
    mint: &str,          // token mint address
    amount: &str,        // SOL amount (if buy, denominatedInSol=true) or token amount (if sell)
    denominated_in_sol: bool,
    slippage_pct: u64,   // percent, not bps
) -> EngineResult<Vec<u8>> {
    let client = reqwest::Client::new();

    let body = serde_json::json!({
        "publicKey": wallet_pubkey,
        "action": action,
        "mint": mint,
        "amount": amount,
        "denominatedInSol": if denominated_in_sol { "true" } else { "false" },
        "slippage": slippage_pct,
        "priorityFee": 0.001,  // 0.001 SOL priority fee
        "pool": "auto"         // auto-detect: pump, raydium, pump-amm, etc.
    });

    info!("[sol_dex] PumpPortal {} request: mint={} amount={} denomInSol={} slippage={}%",
        action, mint, amount, denominated_in_sol, slippage_pct);

    let resp = client.post(PUMPPORTAL_API)
        .header("Content-Type", "application/json")
        .json(&body)
        .timeout(Duration::from_secs(30))
        .send()
        .await?;

    let status = resp.status();

    // PumpPortal returns raw bytes (serialized transaction) on success,
    // or JSON error on failure
    if !status.is_success() {
        let err_text = resp.text().await.unwrap_or_else(|_| "Unknown error".into());
        // Try to parse as JSON error
        if let Ok(err_json) = serde_json::from_str::<serde_json::Value>(&err_text) {
            let msg = err_json.get("message").or(err_json.get("error"))
                .and_then(|v| v.as_str())
                .unwrap_or(&err_text);
            return Err(format!("PumpPortal error ({}): {}", status, msg).into());
        }
        return Err(format!("PumpPortal error ({}): {}", status, err_text).into());
    }

    let tx_bytes = resp.bytes().await?;

    if tx_bytes.is_empty() {
        return Err("PumpPortal returned empty transaction".into());
    }

    info!("[sol_dex] PumpPortal returned {} byte transaction", tx_bytes.len());
    Ok(tx_bytes.to_vec())
}

/// Execute a swap via PumpPortal (fallback when Jupiter has no route).
/// Handles the full flow: get tx → sign → send → confirm.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn pumpportal_swap(
    rpc_url: &str,
    wallet: &str,
    secret_bytes: &[u8; 32],
    input_mint: &str,
    output_mint: &str,
    amount_str: &str,
    amount_raw: u64,
    _in_decimals: u8,
    slippage_bps: u64,
    token_in_str: &str,
    token_out_str: &str,
) -> EngineResult<String> {
    let sol_mint = "So11111111111111111111111111111111111111112";
    let slippage_pct = slippage_pct(slippage_bps); // bps → percent, minimum 1%

    // Determine action and parameters
    // Buying = SOL → token, Selling = token → SOL
    let (action, mint, pp_amount, denom_in_sol) = if input_mint == sol_mint {
        // Buying a token with SOL
        ("buy", output_mint, amount_str.to_string(), true)
    } else if output_mint == sol_mint {
        // Selling a token for SOL — use "100%" or the raw token amount
        ("sell", input_mint, amount_raw.to_string(), false)
    } else {
        return Err("PumpPortal only supports SOL ↔ token swaps (not token-to-token)".into());
    };

    info!("[sol_dex] PumpPortal fallback: {} {} (mint={}) amount={}", action, token_in_str, mint, pp_amount);

    // Step 1: Get serialized transaction from PumpPortal
    let tx_bytes = pumpportal_get_tx(wallet, action, mint, &pp_amount, denom_in_sol, slippage_pct).await?;

    // Step 2: Sign the transaction locally
    let signed_tx = sign_solana_transaction(&tx_bytes, secret_bytes)?;
    let signed_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &signed_tx);

    // Step 3: Send via our RPC
    info!("[sol_dex] Sending PumpPortal-built transaction via RPC...");
    let send_result = rpc_call(rpc_url, "sendTransaction", serde_json::json!([
        signed_b64,
        { "encoding": "base64", "skipPreflight": true, "maxRetries": 3 }
    ])).await?;

    let tx_sig = send_result.as_str().unwrap_or("unknown");
    info!("[sol_dex] PumpPortal swap sent! Tx: {}", tx_sig);

    // Step 4: Wait and check confirmation
    let confirmation = check_tx_confirmation(rpc_url, tx_sig).await;

    let token_in_upper = token_in_str.to_uppercase();
    let token_out_upper = token_out_str.to_uppercase();

    Ok(format!(
        "## Solana Swap Executed (via PumpPortal)\n\n\
        | Field | Value |\n|-------|-------|\n\
        | Sold | {} {} |\n\
        | Buying | {} |\n\
        | Route | PumpPortal ({}) |\n\
        | Status | {} |\n\
        | Transaction | [{}](https://solscan.io/tx/{}) |\n\n\
        _Routed via PumpPortal (pump.fun/PumpSwap/Raydium). Jupiter had no route for this pair._\n\
        _Check Solscan for final confirmation._",
        amount_str, token_in_upper,
        token_out_upper,
        action,
        confirmation,
        &tx_sig[..std::cmp::min(16, tx_sig.len())], tx_sig
    ))
}
