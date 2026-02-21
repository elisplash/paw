// Solana DEX — Jupiter Aggregator (with PumpPortal Fallback)
// execute_sol_quote, execute_sol_quote_jupiter, execute_sol_swap, execute_sol_swap_jupiter

use std::collections::HashMap;
use std::time::Duration;
use log::info;
use super::constants::{JUPITER_API, DEFAULT_SLIPPAGE_BPS, MAX_SLIPPAGE_BPS};
use super::helpers::{resolve_token, amount_to_lamports, lamports_to_amount, parse_solana_keypair, slippage_pct};
use super::rpc::{rpc_call, resolve_decimals_on_chain, check_tx_confirmation};
use super::pumpportal::{is_jupiter_route_error, pumpportal_get_tx, pumpportal_swap};
use super::transaction::sign_solana_transaction;
use crate::atoms::error::{EngineResult, EngineError};

// ── Quote ─────────────────────────────────────────────────────────────

/// sol_quote — Get swap quote (Jupiter → PumpPortal availability check)
pub async fn execute_sol_quote(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> EngineResult<String> {
    let rpc_url = creds.get("SOLANA_RPC_URL")
        .ok_or("Missing SOLANA_RPC_URL.")?;
    let api_key = creds.get("JUPITER_API_KEY");
    // JUPITER_API_KEY is optional — PumpPortal doesn't need one

    let token_in_str = args["token_in"].as_str().ok_or("sol_quote: missing 'token_in'")?;
    let token_out_str = args["token_out"].as_str().ok_or("sol_quote: missing 'token_out'")?;
    let amount_str = args["amount"].as_str().ok_or("sol_quote: missing 'amount'")?;
    let slippage_bps = args.get("slippage_bps").and_then(|v| v.as_u64()).unwrap_or(DEFAULT_SLIPPAGE_BPS);

    if slippage_bps > MAX_SLIPPAGE_BPS {
        return Err(format!("Slippage too high: {}bps. Max is {}bps ({}%)", slippage_bps, MAX_SLIPPAGE_BPS, MAX_SLIPPAGE_BPS / 100).into());
    }

    let (input_mint, input_decimals) = resolve_token(token_in_str)?;
    let (output_mint, _output_decimals) = resolve_token(token_out_str)?;

    // Resolve actual decimals on-chain for unknown tokens (critical for correct amount)
    let in_decimals = resolve_decimals_on_chain(rpc_url, &input_mint, input_decimals).await;
    let amount_raw = amount_to_lamports(amount_str, in_decimals)?;

    // ── Try Jupiter first ──────────────────────────────────────────────
    let jupiter_result = if let Some(api_key) = api_key {
        execute_sol_quote_jupiter(
            rpc_url, api_key, &input_mint, &output_mint,
            amount_str, amount_raw, in_decimals, _output_decimals,
            slippage_bps, token_in_str, token_out_str,
        ).await
    } else {
        Err("No JUPITER_API_KEY — skipping Jupiter".into())
    };

    let jupiter_err_msg = match &jupiter_result {
        Ok(_) => String::new(),
        Err(e) => e.to_string(),
    };

    match jupiter_result {
        Ok(result) => return Ok(result),
        Err(ref e) if is_jupiter_route_error(&e.to_string()) => {
            info!("[sol_dex] Jupiter quote route failed: {} — checking PumpPortal", e);
        }
        Err(ref e) => {
            info!("[sol_dex] Jupiter quote error: {} — checking PumpPortal", e);
        }
    }

    // ── PumpPortal fallback: check if the token is tradeable ──────────
    let sol_mint = "So11111111111111111111111111111111111111112";
    let dummy_wallet = String::from("11111111111111111111111111111111");
    let wallet = creds.get("SOLANA_WALLET_ADDRESS")
        .unwrap_or(&dummy_wallet); // dummy for quote

    let (action, mint, pp_amount, denom_in_sol) = if input_mint == sol_mint {
        ("buy", output_mint.as_str(), amount_str.to_string(), true)
    } else if output_mint == sol_mint {
        ("sell", input_mint.as_str(), amount_raw.to_string(), false)
    } else {
        return Err(format!("No route found on Jupiter ({}) and PumpPortal only supports SOL pairs.", jupiter_err_msg).into());
    };

    let slippage_pct = slippage_pct(slippage_bps);

    // Try to get a transaction from PumpPortal — if it succeeds, the token is tradeable
    match pumpportal_get_tx(wallet, action, mint, &pp_amount, denom_in_sol, slippage_pct).await {
        Ok(tx_bytes) => {
            let token_in_upper = token_in_str.to_uppercase();
            let token_out_upper = token_out_str.to_uppercase();
            Ok(format!(
                "## Quote: {} {} → {} (via PumpPortal)\n\n\
                | Field | Value |\n|-------|-------|\n\
                | Input | {} {} |\n\
                | Output | {} |\n\
                | Route | PumpPortal auto ({}) |\n\
                | Slippage | {}% |\n\
                | Status | ✅ Route available (tx: {} bytes) |\n\n\
                _Jupiter had no route — PumpPortal can execute this via pump.fun/PumpSwap/Raydium._\n\
                _Exact output amount determined at execution time. Use **sol_swap** to execute._",
                amount_str, token_in_upper, token_out_upper,
                amount_str, token_in_upper,
                token_out_upper,
                action,
                slippage_pct,
                tx_bytes.len(),
            ))
        }
        Err(pump_err) => {
            Err(format!(
                "No route found on any DEX:\n• Jupiter: {}\n• PumpPortal: {}\n\n\
                This token may have zero liquidity.",
                jupiter_err_msg, pump_err
            ).into())
        }
    }
}

/// Jupiter-specific quote execution
#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_sol_quote_jupiter(
    rpc_url: &str,
    api_key: &str,
    input_mint: &str,
    output_mint: &str,
    amount_str: &str,
    amount_raw: u64,
    in_decimals: u8,
    output_decimals_hint: u8,
    slippage_bps: u64,
    token_in_str: &str,
    token_out_str: &str,
) -> EngineResult<String> {
    // Call Jupiter Quote API (Metis v1)
    let client = reqwest::Client::new();
    let url = format!(
        "{}/quote?inputMint={}&outputMint={}&amount={}&slippageBps={}&restrictIntermediateTokens=true",
        JUPITER_API, input_mint, output_mint, amount_raw, slippage_bps
    );

    info!("[sol_dex] Getting Jupiter quote: {} {} → {}", amount_str, token_in_str, token_out_str);

    let resp = client.get(&url)
        .header("x-api-key", api_key)
        .timeout(Duration::from_secs(15))
        .send()
        .await?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().await?;

    if !status.is_success() {
        let msg = body.get("error").and_then(|v| v.as_str())
            .or_else(|| body.get("message").and_then(|v| v.as_str()))
            .unwrap_or("Unknown error");
        return Err(format!("Jupiter quote failed: {}", msg).into());
    }

    // Parse quote response
    let out_amount_raw = body.get("outAmount").and_then(|v| v.as_str())
        .ok_or("Missing outAmount in Jupiter response")?;
    let out_amount: u64 = out_amount_raw.parse().unwrap_or(0);

    let other_amount_threshold = body.get("otherAmountThreshold").and_then(|v| v.as_str()).unwrap_or("0");
    let min_out: u64 = other_amount_threshold.parse().unwrap_or(0);

    let price_impact_pct = body.get("priceImpactPct").and_then(|v| v.as_str()).unwrap_or("0");

    // Get output token decimals — resolve on-chain for unknown tokens
    let out_decimals = resolve_decimals_on_chain(rpc_url, output_mint, output_decimals_hint).await;

    let out_human = lamports_to_amount(out_amount, out_decimals);
    let min_human = lamports_to_amount(min_out, out_decimals);

    // Route info
    let route_plan = body.get("routePlan").and_then(|v| v.as_array());
    let route_info = if let Some(routes) = route_plan {
        let labels: Vec<String> = routes.iter()
            .filter_map(|r| r.pointer("/swapInfo/label").and_then(|v| v.as_str()))
            .map(|s| s.to_string())
            .collect();
        if labels.is_empty() { "Direct".into() } else { labels.join(" → ") }
    } else {
        "Direct".into()
    };

    // Calculate exchange rate
    let in_f = amount_raw as f64 / 10f64.powi(in_decimals as i32);
    let out_f = out_amount as f64 / 10f64.powi(out_decimals as i32);
    let rate = if in_f > 0.0 { out_f / in_f } else { 0.0 };

    let token_in_upper = token_in_str.to_uppercase();
    let token_out_upper = token_out_str.to_uppercase();

    Ok(format!(
        "## Jupiter Quote: {} {} → {}\n\n\
        | Field | Value |\n|-------|-------|\n\
        | Input | {} {} |\n\
        | Expected Output | {} {} |\n\
        | Min Output (slippage {:.1}%) | {} {} |\n\
        | Exchange Rate | 1 {} = {:.6} {} |\n\
        | Price Impact | {}% |\n\
        | Route | {} |\n\n\
        _Use **sol_swap** to execute this trade._",
        amount_str, token_in_upper, token_out_upper,
        amount_str, token_in_upper,
        out_human, token_out_upper,
        slippage_bps as f64 / 100.0, min_human, token_out_upper,
        token_in_upper, rate, token_out_upper,
        price_impact_pct,
        route_info
    ))
}

// ── Swap ──────────────────────────────────────────────────────────────

/// sol_swap — Execute a swap via Jupiter → PumpPortal fallback
pub async fn execute_sol_swap(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> EngineResult<String> {
    let rpc_url = creds.get("SOLANA_RPC_URL")
        .ok_or("Missing SOLANA_RPC_URL.")?;
    let wallet = creds.get("SOLANA_WALLET_ADDRESS")
        .ok_or("No Solana wallet. Use sol_wallet_create first.")?;
    let private_key_b58 = creds.get("SOLANA_PRIVATE_KEY")
        .ok_or("No Solana private key. Use sol_wallet_create first.")?;
    let api_key = creds.get("JUPITER_API_KEY");
    // JUPITER_API_KEY is optional now — we can still trade via PumpPortal without it

    let token_in_str = args["token_in"].as_str().ok_or("sol_swap: missing 'token_in'")?;
    let token_out_str = args["token_out"].as_str().ok_or("sol_swap: missing 'token_out'")?;
    let amount_str = args["amount"].as_str().ok_or("sol_swap: missing 'amount'")?;
    let _reason = args["reason"].as_str().unwrap_or("No reason given");
    let slippage_bps = args.get("slippage_bps").and_then(|v| v.as_u64()).unwrap_or(DEFAULT_SLIPPAGE_BPS);

    if slippage_bps > MAX_SLIPPAGE_BPS {
        return Err(format!("Slippage too high: {}bps (max {}bps)", slippage_bps, MAX_SLIPPAGE_BPS).into());
    }

    let (input_mint, input_decimals) = resolve_token(token_in_str)?;
    let (output_mint, _output_decimals) = resolve_token(token_out_str)?;

    // Resolve actual decimals on-chain for unknown tokens (critical for sells)
    let in_decimals = resolve_decimals_on_chain(rpc_url, &input_mint, input_decimals).await;
    let amount_raw = amount_to_lamports(amount_str, in_decimals)?;

    // Parse private key early (needed for both Jupiter and PumpPortal paths)
    let secret_bytes = parse_solana_keypair(private_key_b58)?;

    // ── Try Jupiter first ──────────────────────────────────────────────
    let jupiter_result = if let Some(api_key) = api_key {
        execute_sol_swap_jupiter(
            rpc_url, wallet, &secret_bytes, api_key,
            &input_mint, &output_mint, amount_str, amount_raw,
            in_decimals, _output_decimals, slippage_bps,
            token_in_str, token_out_str,
        ).await
    } else {
        Err("No JUPITER_API_KEY — skipping Jupiter, trying PumpPortal".into())
    };

    let jupiter_err_msg = match jupiter_result {
        Ok(result) => return Ok(result),
        Err(ref e) if is_jupiter_route_error(&e.to_string()) => {
            info!("[sol_dex] Jupiter route failed: {} \u2014 falling back to PumpPortal", e);
            e.to_string()
        }
        Err(ref e) => {
            // Non-routing error from Jupiter \u2014 still try PumpPortal as last resort
            info!("[sol_dex] Jupiter error: {} \u2014 attempting PumpPortal fallback", e);
            e.to_string()
        }
    };

    // ── PumpPortal fallback ────────────────────────────────────────────
    info!("[sol_dex] Trying PumpPortal for {} {} → {}", amount_str, token_in_str, token_out_str);
    let pump_result = pumpportal_swap(
        rpc_url, wallet, &secret_bytes,
        &input_mint, &output_mint,
        amount_str, amount_raw, in_decimals,
        slippage_bps, token_in_str, token_out_str,
    ).await;

    match pump_result {
        Ok(result) => Ok(result),
        Err(pump_err) => {
            // Both failed — return both errors for debugging
            let jupiter_err = jupiter_err_msg;
            Err(format!(
                "Swap failed on both routes:\n• Jupiter: {}\n• PumpPortal: {}\n\n\
                This token may have zero liquidity on all DEXes.",
                jupiter_err, pump_err
            ).into())
        }
    }
}

/// Jupiter-specific swap execution (extracted from execute_sol_swap)
#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_sol_swap_jupiter(
    rpc_url: &str,
    wallet: &str,
    secret_bytes: &[u8; 32],
    api_key: &str,
    input_mint: &str,
    output_mint: &str,
    amount_str: &str,
    amount_raw: u64,
    _in_decimals: u8,
    output_decimals_hint: u8,
    slippage_bps: u64,
    token_in_str: &str,
    token_out_str: &str,
) -> EngineResult<String> {
    let client = reqwest::Client::new();

    // Step 1: Get Jupiter quote (Metis v1)
    let quote_url = format!(
        "{}/quote?inputMint={}&outputMint={}&amount={}&slippageBps={}&restrictIntermediateTokens=true",
        JUPITER_API, input_mint, output_mint, amount_raw, slippage_bps
    );

    info!("[sol_dex] Getting Jupiter quote for swap: {} {} → {}", amount_str, token_in_str, token_out_str);

    let quote_resp = client.get(&quote_url)
        .header("x-api-key", api_key)
        .timeout(Duration::from_secs(15))
        .send()
        .await?;

    let quote: serde_json::Value = quote_resp.json().await?;

    if quote.get("error").is_some() || quote.get("outAmount").is_none() {
        let msg = quote.get("error").and_then(|v| v.as_str())
            .or_else(|| quote.get("message").and_then(|v| v.as_str()))
            .unwrap_or("No route found");
        return Err(format!("Jupiter quote failed: {}", msg).into());
    }

    let out_amount_str = quote.get("outAmount").and_then(|v| v.as_str()).unwrap_or("0");
    let out_amount: u64 = out_amount_str.parse().unwrap_or(0);
    let out_decimals = resolve_decimals_on_chain(rpc_url, output_mint, output_decimals_hint).await;
    let out_human = lamports_to_amount(out_amount, out_decimals);

    // Step 2: Get swap transaction from Jupiter
    info!("[sol_dex] Getting swap transaction from Jupiter...");

    let swap_body = serde_json::json!({
        "quoteResponse": quote,
        "userPublicKey": wallet,
        "wrapAndUnwrapSol": true,
        "dynamicComputeUnitLimit": true,
        "dynamicSlippage": true,
        "prioritizationFeeLamports": {
            "priorityLevelWithMaxLamports": {
                "maxLamports": 1_000_000,
                "priorityLevel": "veryHigh"
            }
        }
    });

    let swap_resp = client.post(format!("{}/swap", JUPITER_API))
        .header("x-api-key", api_key)
        .json(&swap_body)
        .timeout(Duration::from_secs(30))
        .send()
        .await?;

    let swap_data: serde_json::Value = swap_resp.json().await?;

    let swap_tx_b64 = swap_data.get("swapTransaction").and_then(|v| v.as_str())
        .ok_or("Jupiter did not return a swap transaction")?;

    // Step 3: Decode, sign, and send the transaction
    info!("[sol_dex] Signing and sending transaction...");

    let tx_bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, swap_tx_b64)
        .map_err(|e| EngineError::Other(e.to_string()))?;

    // Sign the transaction
    let signed_tx = sign_solana_transaction(&tx_bytes, secret_bytes)?;
    let signed_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &signed_tx);

    // Step 4: Send the signed transaction
    let send_result = rpc_call(rpc_url, "sendTransaction", serde_json::json!([
        signed_b64,
        { "encoding": "base64", "skipPreflight": false, "maxRetries": 3 }
    ])).await?;

    let tx_sig = send_result.as_str().unwrap_or("unknown");

    info!("[sol_dex] Swap sent! Tx: {}", tx_sig);

    // Step 5: Wait briefly and check confirmation
    let confirmation = check_tx_confirmation(rpc_url, tx_sig).await;

    let token_in_upper = token_in_str.to_uppercase();
    let token_out_upper = token_out_str.to_uppercase();

    Ok(format!(
        "## Solana Swap Executed\n\n\
        | Field | Value |\n|-------|-------|\n\
        | Sold | {} {} |\n\
        | Received (est.) | {} {} |\n\
        | Status | {} |\n\
        | Transaction | [{}](https://solscan.io/tx/{}) |\n\n\
        _Check Solscan for final confirmation._",
        amount_str, token_in_upper,
        out_human, token_out_upper,
        confirmation,
        &tx_sig[..std::cmp::min(16, tx_sig.len())], tx_sig
    ))
}
