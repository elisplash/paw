// Solana DEX — RPC Client
// rpc_call, get_sol_balance, get_token_accounts, get_mint_info,
// resolve_decimals_on_chain, check_tx_confirmation

use log::info;
use std::time::Duration;
use super::constants::{TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID};
use crate::atoms::error::{EngineError, EngineResult};

/// Make a Solana JSON-RPC call
pub(crate) async fn rpc_call(rpc_url: &str, method: &str, params: serde_json::Value) -> EngineResult<serde_json::Value> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params
    });

    let resp = client.post(rpc_url)
        .json(&body)
        .timeout(Duration::from_secs(30))
        .send()
        .await?;

    let json: serde_json::Value = resp.json().await?;

    if let Some(error) = json.get("error") {
        return Err(EngineError::Other(format!("Solana RPC error: {}", error)));
    }

    json.get("result").cloned()
        .ok_or_else(|| EngineError::Other("Solana RPC: missing 'result' field".into()))
}

/// Get SOL balance in lamports
pub(crate) async fn get_sol_balance(rpc_url: &str, address: &str) -> EngineResult<u64> {
    let result = rpc_call(rpc_url, "getBalance", serde_json::json!([address])).await?;
    result.get("value")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| EngineError::Other("Failed to parse SOL balance".into()))
}

/// Get SPL token accounts for a wallet
pub(crate) async fn get_token_accounts(rpc_url: &str, wallet: &str) -> EngineResult<Vec<(String, u64, u8, String)>> {
    // Returns (mint, amount, decimals, token_account_address)
    let mut all_accounts = Vec::new();

    // Query Token Program
    for program_id in &[TOKEN_PROGRAM_ID, TOKEN_2022_PROGRAM_ID] {
        let result = rpc_call(rpc_url, "getTokenAccountsByOwner", serde_json::json!([
            wallet,
            { "programId": program_id },
            { "encoding": "jsonParsed" }
        ])).await;

        if let Ok(result) = result {
            if let Some(accounts) = result.get("value").and_then(|v| v.as_array()) {
                for acct in accounts {
                    let parsed = acct.pointer("/account/data/parsed/info");
                    if let Some(info) = parsed {
                        let mint = info.get("mint").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let token_amount = info.pointer("/tokenAmount");
                        if let Some(ta) = token_amount {
                            let amount_str = ta.get("amount").and_then(|v| v.as_str()).unwrap_or("0");
                            let amount: u64 = amount_str.parse().unwrap_or(0);
                            let decimals = ta.get("decimals").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
                            let pubkey = acct.get("pubkey").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            if amount > 0 {
                                all_accounts.push((mint, amount, decimals, pubkey));
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(all_accounts)
}

/// Get token metadata (name, symbol, decimals, supply) from on-chain mint
pub(crate) async fn get_mint_info(rpc_url: &str, mint: &str) -> EngineResult<serde_json::Value> {
    let result = rpc_call(rpc_url, "getAccountInfo", serde_json::json!([
        mint,
        { "encoding": "jsonParsed" }
    ])).await?;

    let parsed = result.pointer("/value/data/parsed/info");
    if let Some(info) = parsed {
        Ok(info.clone())
    } else {
        Err(EngineError::Other(format!("Could not parse mint info for {}", mint)))
    }
}

/// Resolve actual decimals for a token mint by querying on-chain.
/// Falls back to 9 (SOL-like) if the query fails.
pub(crate) async fn resolve_decimals_on_chain(rpc_url: &str, mint: &str, known_decimals: u8) -> u8 {
    if known_decimals > 0 {
        return known_decimals;
    }
    // Query on-chain mint info
    match get_mint_info(rpc_url, mint).await {
        Ok(info) => {
            if let Some(d) = info.get("decimals").and_then(|v| v.as_u64()) {
                info!("[sol_dex] Resolved on-chain decimals for {}: {}", &mint[..8.min(mint.len())], d);
                d as u8
            } else {
                info!("[sol_dex] No decimals in mint info for {}, defaulting to 9", &mint[..8.min(mint.len())]);
                9
            }
        }
        Err(e) => {
            info!("[sol_dex] Failed to get mint info for {}: {}, defaulting to 9", &mint[..8.min(mint.len())], e);
            9
        }
    }
}

// ── Transaction Confirmation ─────────────────────────────────────────

/// Wait 3 seconds then poll getSignatureStatuses and return a human-readable status.
/// Used by jupiter.rs, pumpportal.rs, and transfer.rs after sending transactions.
pub(crate) async fn check_tx_confirmation(rpc_url: &str, tx_sig: &str) -> String {
    tokio::time::sleep(Duration::from_secs(3)).await;
    let status = rpc_call(rpc_url, "getSignatureStatuses", serde_json::json!([[tx_sig]])).await;
    if let Ok(status_val) = status {
        if let Some(statuses) = status_val.get("value").and_then(|v| v.as_array()) {
            if let Some(Some(s)) = statuses.first().map(|v| if v.is_null() { None } else { Some(v) }) {
                let conf = s.get("confirmationStatus").and_then(|v| v.as_str()).unwrap_or("pending");
                if s.get("err").is_some() && !s["err"].is_null() {
                    return format!("❌ FAILED: {:?}", s["err"]);
                } else {
                    return format!("✅ {}", conf);
                }
            } else {
                return "⏳ Pending (check explorer)".into();
            }
        }
    }
    "⏳ Submitted".into()
}
