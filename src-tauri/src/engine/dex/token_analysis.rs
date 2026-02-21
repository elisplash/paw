// Paw Agent Engine — DEX Token Analysis (info + safety check)

use super::abi::{
    decode_abi_string, encode_decimals, encode_name, encode_owner, encode_quote_exact_input_single,
    encode_symbol, encode_total_supply,
};
use super::constants::{chain_name, UNISWAP_QUOTER_V2, WETH_ADDRESS};
use super::primitives::{
    amount_to_raw, eip55_checksum, hex_decode, hex_encode, parse_address, parse_u256_decimal,
    raw_to_amount,
};
use super::rpc::{eth_call, eth_chain_id, eth_get_balance, rpc_call};
use std::collections::HashMap;
use crate::atoms::error::EngineResult;

/// Get comprehensive token info by reading on-chain ERC-20 data directly via RPC.
/// No website scraping needed — this queries the blockchain.
pub async fn execute_dex_token_info(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> EngineResult<String> {
    let rpc_url = creds.get("DEX_RPC_URL").ok_or("Missing DEX_RPC_URL")?;
    let token_address = args["token_address"].as_str()
        .ok_or("dex_token_info: missing 'token_address'. Provide the ERC-20 contract address.")?;

    // Validate address format
    let addr_clean = token_address.trim();
    if !addr_clean.starts_with("0x") || addr_clean.len() != 42 {
        return Err(format!("Invalid contract address format: '{}'. Must be 0x + 40 hex chars.", addr_clean).into());
    }

    let mut output = format!("Token Analysis: {}\n\n", addr_clean);

    // 1. Name
    match eth_call(rpc_url, addr_clean, &encode_name()).await {
        Ok(result) => {
            if let Ok(name) = decode_abi_string(&result) {
                output.push_str(&format!("  Name: {}\n", name));
            }
        }
        Err(_) => { output.push_str("  Name: Could not read (non-standard contract)\n"); }
    }

    // 2. Symbol
    match eth_call(rpc_url, addr_clean, &encode_symbol()).await {
        Ok(result) => {
            if let Ok(symbol) = decode_abi_string(&result) {
                output.push_str(&format!("  Symbol: {}\n", symbol));
            }
        }
        Err(_) => { output.push_str("  Symbol: Could not read\n"); }
    }

    // 3. Decimals
    let mut token_decimals = 18u8;
    match eth_call(rpc_url, addr_clean, &encode_decimals()).await {
        Ok(result) => {
            let bytes = hex_decode(&result).unwrap_or_default();
            if bytes.len() >= 32 {
                token_decimals = bytes[31];
                output.push_str(&format!("  Decimals: {}\n", token_decimals));
            }
        }
        Err(_) => { output.push_str("  Decimals: 18 (assumed)\n"); }
    }

    // 4. Total Supply
    match eth_call(rpc_url, addr_clean, &encode_total_supply()).await {
        Ok(result) => {
            if let Ok(supply) = raw_to_amount(&result, token_decimals) {
                output.push_str(&format!("  Total Supply: {}\n", supply));
            }
        }
        Err(_) => { output.push_str("  Total Supply: Could not read\n"); }
    }

    // 5. Owner (if the contract has an owner function — indicates centralization risk)
    match eth_call(rpc_url, addr_clean, &encode_owner()).await {
        Ok(result) => {
            let bytes = hex_decode(&result).unwrap_or_default();
            if bytes.len() >= 32 {
                let owner_addr = &bytes[12..32];
                let zero_addr = [0u8; 20];
                if owner_addr == zero_addr {
                    output.push_str("  Owner: Renounced (0x0) [SAFE]\n");
                } else {
                    let owner_hex = eip55_checksum(owner_addr);
                    output.push_str(&format!("  Owner: {} [WARNING: not renounced — owner can modify contract]\n", owner_hex));
                }
            }
        }
        Err(_) => { output.push_str("  Owner: No owner() function (may be immutable) [SAFE]\n"); }
    }

    // 6. Contract code size (is it actually a contract?)
    let code_result = rpc_call(rpc_url, "eth_getCode", serde_json::json!([addr_clean, "latest"])).await;
    if let Ok(code) = code_result {
        let code_str = code.as_str().unwrap_or("0x");
        let code_len = (code_str.len() - 2) / 2;
        if code_len == 0 {
            output.push_str("  Contract: NO CODE \u2014 this is an EOA (wallet), not a token!\n");
        } else {
            output.push_str(&format!("  Contract: {} bytes of bytecode [OK]\n", code_len));
        }
    }

    // 7. Check ETH balance of the contract
    if let Ok(bal_hex) = eth_get_balance(rpc_url, addr_clean).await {
        if let Ok(eth_bal) = raw_to_amount(&bal_hex, 18) {
            if eth_bal != "0" {
                output.push_str(&format!("  Contract ETH balance: {} ETH\n", eth_bal));
            }
        }
    }

    // 8. Check if the token can be quoted on Uniswap (basic swap viability)
    output.push_str("\n  Swap Viability:\n");

    let token_bytes = parse_address(addr_clean)?;
    let tiny_amount = parse_u256_decimal("1000000000000000")?; // 0.001 ETH in wei
    let weth_addr_bytes = parse_address(WETH_ADDRESS)?;

    for fee in &[3000u32, 10000, 500, 100] {
        let quote_data = encode_quote_exact_input_single(
            &weth_addr_bytes,
            &token_bytes,
            &tiny_amount,
            *fee,
        );

        if let Ok(result) = eth_call(rpc_url, UNISWAP_QUOTER_V2, &quote_data).await {
                let result_bytes = hex_decode(&result).unwrap_or_default();
                if result_bytes.len() >= 32 {
                    let amount_out: [u8; 32] = result_bytes[..32].try_into()
                        .map_err(|_| "Byte conversion failed")?;
                    let out_hex = hex_encode(&amount_out);
                    if let Ok(out_amount) = raw_to_amount(&out_hex, token_decimals) {
                        output.push_str(&format!("    Uniswap V3 pool found ({}% fee tier) [OK]\n", *fee as f64 / 10000.0));
                        output.push_str(&format!("    Quote: 0.001 WETH -> {} tokens\n", out_amount));

                        // Honeypot check: try reverse quote (can you SELL?)
                        if let Ok(sell_raw) = amount_to_raw(&out_amount, token_decimals) {
                            if let Ok(sell_u256) = parse_u256_decimal(&sell_raw) {
                                let reverse_quote = encode_quote_exact_input_single(
                                    &token_bytes,
                                    &weth_addr_bytes,
                                    &sell_u256,
                                    *fee,
                                );
                                match eth_call(rpc_url, UNISWAP_QUOTER_V2, &reverse_quote).await {
                                    Ok(rev_result) => {
                                        let rev_bytes = hex_decode(&rev_result).unwrap_or_default();
                                        if rev_bytes.len() >= 32 {
                                            let rev_out: [u8; 32] = rev_bytes[..32].try_into()
                                                .map_err(|_| "Byte conversion failed")?;
                                            let rev_hex = hex_encode(&rev_out);
                                            if let Ok(rev_amount) = raw_to_amount(&rev_hex, 18) {
                                                let rev_f: f64 = rev_amount.parse().unwrap_or(0.0);
                                                let original = 0.001f64;
                                                let round_trip_loss = ((original - rev_f) / original * 100.0).abs();
                                                output.push_str(&format!("    SELL quote works: {} tokens -> {} WETH [OK]\n", out_amount, rev_amount));
                                                output.push_str(&format!("    Round-trip loss: {:.2}% (fees + slippage)\n", round_trip_loss));
                                                if round_trip_loss > 50.0 {
                                                    output.push_str("    [DANGER] HIGH ROUND-TRIP LOSS — possible honeypot or extreme tax\n");
                                                } else if round_trip_loss > 10.0 {
                                                    output.push_str("    [WARNING] Moderate tax detected — check tokenomics\n");
                                                } else {
                                                    output.push_str("    Normal fee range — not a honeypot [OK]\n");
                                                }
                                            }
                                        }
                                    }
                                    Err(_) => {
                                        output.push_str("    [DANGER] SELL BLOCKED — cannot get reverse quote. LIKELY HONEYPOT!\n");
                                    }
                                }
                            }
                        }
                        break; // Found a working pool, done
                    }
                }
        }
    }

    // 9. Chain info
    if let Ok(chain_id) = eth_chain_id(rpc_url).await {
        let chain = chain_name(chain_id);
        output.push_str(&format!("\n  Network: {} (chain ID {})\n", chain, chain_id));
    }

    Ok(output)
}

/// Perform automated safety checks on a token contract.
/// Simulates buy AND sell to detect honeypots, checks ownership, analyzes on-chain data.
pub async fn execute_dex_check_token(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> EngineResult<String> {
    let rpc_url = creds.get("DEX_RPC_URL").ok_or("Missing DEX_RPC_URL")?;
    let token_address = args["token_address"].as_str()
        .ok_or("dex_check_token: missing 'token_address'")?;

    let addr_clean = token_address.trim();
    if !addr_clean.starts_with("0x") || addr_clean.len() != 42 {
        return Err(format!("Invalid address: '{}'", addr_clean).into());
    }

    let mut output = String::from("Token Safety Report\n\n");
    let mut risk_score = 0u32;
    let mut flags: Vec<String> = Vec::new();

    let token_bytes = parse_address(addr_clean)?;
    let weth_bytes = parse_address(WETH_ADDRESS)?;

    // Check 1: Is it actually a contract?
    let code_result = rpc_call(rpc_url, "eth_getCode", serde_json::json!([addr_clean, "latest"])).await;
    match code_result {
        Ok(code) => {
            let code_str = code.as_str().unwrap_or("0x");
            let code_len = (code_str.len() - 2) / 2;
            if code_len == 0 {
                output.push_str("FATAL: Address has no contract code — this is a wallet address, not a token!\n");
                return Ok(output);
            }
            output.push_str(&format!("[OK] Contract verified ({} bytes)\n", code_len));
        }
        Err(e) => { output.push_str(&format!("[WARNING] Could not check contract code: {}\n", e)); }
    }

    // Check 2: ERC-20 standard compliance
    let has_name = eth_call(rpc_url, addr_clean, &encode_name()).await.is_ok();
    let has_symbol = eth_call(rpc_url, addr_clean, &encode_symbol()).await.is_ok();
    let has_decimals = eth_call(rpc_url, addr_clean, &encode_decimals()).await.is_ok();
    let has_supply = eth_call(rpc_url, addr_clean, &encode_total_supply()).await.is_ok();

    if has_name && has_symbol && has_decimals && has_supply {
        output.push_str("[OK] ERC-20 standard compliant (name, symbol, decimals, totalSupply)\n");
    } else {
        output.push_str("[WARNING] Non-standard ERC-20 — missing some functions\n");
        risk_score += 2;
        flags.push("Non-standard ERC-20".into());
    }

    // Get token decimals
    let mut token_decimals = 18u8;
    if let Ok(result) = eth_call(rpc_url, addr_clean, &encode_decimals()).await {
        let bytes = hex_decode(&result).unwrap_or_default();
        if bytes.len() >= 32 { token_decimals = bytes[31]; }
    }

    // Check 3: Ownership
    match eth_call(rpc_url, addr_clean, &encode_owner()).await {
        Ok(result) => {
            let bytes = hex_decode(&result).unwrap_or_default();
            if bytes.len() >= 32 {
                let owner_addr = &bytes[12..32];
                if owner_addr == [0u8; 20] {
                    output.push_str("[OK] Ownership renounced (owner = 0x0)\n");
                } else {
                    let owner_hex = eip55_checksum(owner_addr);
                    output.push_str(&format!("[WARNING] Owner: {} — can potentially modify contract\n", owner_hex));
                    risk_score += 3;
                    flags.push("Owner not renounced".into());
                }
            }
        }
        Err(_) => {
            output.push_str("[OK] No owner() function — likely immutable\n");
        }
    }

    // Check 4: HONEYPOT TEST
    output.push_str("\nHoneypot Test:\n");
    let tiny_amount = parse_u256_decimal("1000000000000000")?; // 0.001 ETH

    let mut can_buy = false;
    let mut can_sell = false;

    for fee in &[3000u32, 10000, 500, 100] {
        let buy_quote = encode_quote_exact_input_single(&weth_bytes, &token_bytes, &tiny_amount, *fee);
        if let Ok(result) = eth_call(rpc_url, UNISWAP_QUOTER_V2, &buy_quote).await {
                let result_bytes = hex_decode(&result).unwrap_or_default();
                if result_bytes.len() >= 32 {
                    let out: [u8; 32] = result_bytes[..32].try_into()
                        .map_err(|_| "Byte conversion failed")?;
                    let out_hex = hex_encode(&out);
                    if let Ok(amount) = raw_to_amount(&out_hex, token_decimals) {
                        can_buy = true;
                        output.push_str(&format!("  [OK] BUY works: 0.001 WETH -> {} tokens ({}% fee)\n", amount, *fee as f64 / 10000.0));

                        // Try to sell
                        if let Ok(sell_raw) = amount_to_raw(&amount, token_decimals) {
                            if let Ok(sell_u256) = parse_u256_decimal(&sell_raw) {
                                let sell_quote = encode_quote_exact_input_single(&token_bytes, &weth_bytes, &sell_u256, *fee);
                                match eth_call(rpc_url, UNISWAP_QUOTER_V2, &sell_quote).await {
                                    Ok(rev) => {
                                        let rev_bytes = hex_decode(&rev).unwrap_or_default();
                                        if rev_bytes.len() >= 32 {
                                            let rev_out: [u8; 32] = rev_bytes[..32].try_into()
                                                .map_err(|_| "Byte conversion failed")?;
                                            let rev_hex = hex_encode(&rev_out);
                                            if let Ok(rev_amount) = raw_to_amount(&rev_hex, 18) {
                                                can_sell = true;
                                                let rev_f: f64 = rev_amount.parse().unwrap_or(0.0);
                                                let loss_pct = ((0.001 - rev_f) / 0.001 * 100.0).abs();

                                                output.push_str(&format!("  [OK] SELL works: {} tokens -> {} WETH\n", amount, rev_amount));

                                                if loss_pct > 50.0 {
                                                    output.push_str(&format!("  [DANGER] EXTREME TAX: {:.1}% round-trip loss — PROBABLE HONEYPOT or >25% tax\n", loss_pct));
                                                    risk_score += 10;
                                                    flags.push(format!("Extreme tax: {:.1}%", loss_pct));
                                                } else if loss_pct > 20.0 {
                                                    output.push_str(&format!("  [WARNING] HIGH TAX: {:.1}% round-trip loss — likely 10%+ buy/sell tax\n", loss_pct));
                                                    risk_score += 5;
                                                    flags.push(format!("High tax: {:.1}%", loss_pct));
                                                } else if loss_pct > 5.0 {
                                                    output.push_str(&format!("  [WARNING] Moderate tax: {:.1}% round-trip loss\n", loss_pct));
                                                    risk_score += 2;
                                                    flags.push(format!("Tax: {:.1}%", loss_pct));
                                                } else {
                                                    output.push_str(&format!("  [OK] Normal: {:.1}% round-trip loss (just pool fees)\n", loss_pct));
                                                }
                                            }
                                        }
                                    }
                                    Err(_) => {
                                        output.push_str("  [DANGER] SELL FAILED — quoter reverted. LIKELY HONEYPOT!\n");
                                        risk_score += 15;
                                        flags.push("Sell blocked — honeypot".into());
                                    }
                                }
                            }
                        }
                        break;
                    }
                }
        }
    }

    if !can_buy {
        output.push_str("  No Uniswap V3 liquidity pool found for this token\n");
        risk_score += 5;
        flags.push("No Uniswap V3 pool".into());
    }

    // Total supply
    if let Ok(result) = eth_call(rpc_url, addr_clean, &encode_total_supply()).await {
        if let Ok(total) = raw_to_amount(&result, token_decimals) {
            output.push_str(&format!("\nSupply: {} total tokens\n", total));
        }
    }

    // Final risk assessment
    output.push_str("\n────────────────────────────────────────\n");
    output.push_str(&format!("Risk Score: {}/30\n", risk_score.min(30)));

    if risk_score == 0 {
        output.push_str("LOW RISK — All checks passed\n");
    } else if risk_score <= 5 {
        output.push_str("MODERATE RISK — Some concerns, proceed with caution\n");
    } else if risk_score <= 10 {
        output.push_str("HIGH RISK — Significant red flags detected\n");
    } else {
        output.push_str("CRITICAL RISK — DO NOT TRADE — Multiple severe issues\n");
    }

    if !flags.is_empty() {
        output.push_str(&format!("\nFlags: {}\n", flags.join(", ")));
    }

    if !can_sell && can_buy {
        output.push_str("\nVERDICT: HONEYPOT — You can buy but CANNOT sell. Do NOT trade this token.\n");
    }

    Ok(output)
}
