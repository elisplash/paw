// Paw Agent Engine — DEX Swap Execution (Uniswap V3)

use super::abi::{
    build_multihop_path, encode_allowance, encode_approve, encode_exact_input,
    encode_exact_input_single, encode_quote_exact_input, encode_quote_exact_input_single,
    u256_to_quantity_hex,
};
use super::constants::{
    explorer_tx_url, DEFAULT_FEE_TIER, DEFAULT_SLIPPAGE_BPS, MAX_SLIPPAGE_BPS, UNISWAP_QUOTER_V2,
    UNISWAP_SWAP_ROUTER_02, WETH_ADDRESS,
};
use super::primitives::{
    amount_to_raw, hex_decode, hex_encode, parse_address, parse_u256_decimal, raw_to_amount,
};
use super::rpc::{
    eth_call, eth_chain_id, eth_estimate_gas, eth_get_transaction_count,
    eth_get_transaction_receipt, eth_send_raw_transaction, get_gas_fees,
};
use super::tokens::resolve_for_swap;
use super::tx::sign_eip1559_transaction;
use std::collections::HashMap;
use std::time::Duration;
use log::info;
use crate::atoms::error::{EngineResult, EngineError};

/// Get a swap quote from Uniswap V3 Quoter.
pub async fn execute_dex_quote(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> EngineResult<String> {
    let rpc_url = creds.get("DEX_RPC_URL").ok_or("Missing DEX_RPC_URL")?;
    let token_in_sym = args["token_in"].as_str().ok_or("dex_quote: missing 'token_in'")?;
    let token_out_sym = args["token_out"].as_str().ok_or("dex_quote: missing 'token_out'")?;
    let amount = args["amount"].as_str().ok_or("dex_quote: missing 'amount'")?;

    let (token_in_addr, token_in_dec, _is_eth) = resolve_for_swap(token_in_sym)?;
    let (token_out_addr, token_out_dec, _) = resolve_for_swap(token_out_sym)?;

    let fee_tier = args.get("fee_tier")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_FEE_TIER) as u32;

    // Convert amount to raw units
    let amount_raw = amount_to_raw(amount, token_in_dec)?;
    let amount_u256 = parse_u256_decimal(&amount_raw)?;

    let token_in_bytes = parse_address(&token_in_addr)?;
    let token_out_bytes = parse_address(&token_out_addr)?;
    let weth_bytes = parse_address(WETH_ADDRESS)?;

    // Try single-hop first, then multi-hop through WETH if direct pool doesn't exist
    let mut used_multihop = false;
    let result = {
        let single_calldata = encode_quote_exact_input_single(
            &token_in_bytes,
            &token_out_bytes,
            &amount_u256,
            fee_tier,
        );
        match eth_call(rpc_url, UNISWAP_QUOTER_V2, &single_calldata).await {
            Ok(r) => Ok(r),
            Err(_) if token_in_bytes != weth_bytes && token_out_bytes != weth_bytes => {
                // Try multi-hop: tokenIn → WETH → tokenOut
                info!("[dex] Single-hop quote failed, trying multi-hop through WETH");
                used_multihop = true;
                let path = build_multihop_path(
                    &[&token_in_bytes, &weth_bytes, &token_out_bytes],
                    &[fee_tier, fee_tier],
                );
                let multi_calldata = encode_quote_exact_input(&path, &amount_u256);
                eth_call(rpc_url, UNISWAP_QUOTER_V2, &multi_calldata).await
                    
            }
            Err(e) => Err(e),
        }
    }?;

    // The quoter returns (amountOut, sqrtPriceX96After, initializedTicksCrossed, gasEstimate)
    // amountOut is the first 32 bytes
    let result_bytes = hex_decode(&result)?;
    if result_bytes.len() < 32 {
        return Err(format!("Unexpected quoter response length: {} bytes", result_bytes.len()).into());
    }

    let amount_out_bytes: [u8; 32] = result_bytes[..32].try_into()
        .map_err(|_| "Failed to parse 32-byte amount from quoter response")?;
    let amount_out_hex = hex_encode(&amount_out_bytes);
    let amount_out = raw_to_amount(&amount_out_hex, token_out_dec)?;

    // Calculate price
    let in_f64: f64 = amount.parse().unwrap_or(0.0);
    let out_f64: f64 = amount_out.parse().unwrap_or(0.0);
    let price = if in_f64 > 0.0 { out_f64 / in_f64 } else { 0.0 };

    let slippage_bps = args.get("slippage_bps")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_SLIPPAGE_BPS);

    let min_out = out_f64 * (10000.0 - slippage_bps as f64) / 10000.0;

    let route_info = if used_multihop {
        format!("Route: {} → WETH → {} (multi-hop)", token_in_sym.to_uppercase(), token_out_sym.to_uppercase())
    } else {
        format!("Route: {} → {} (direct)", token_in_sym.to_uppercase(), token_out_sym.to_uppercase())
    };

    Ok(format!(
        "Swap Quote: {} {} → {} {}\n\nInput: {} {}\nExpected Output: {} {}\nMinimum Output ({}% slippage): {:.6} {}\nExchange Rate: 1 {} = {:.6} {}\n{}\nFee Tier: {}%\n\nUse dex_swap to execute this trade.",
        amount, token_in_sym.to_uppercase(),
        amount_out, token_out_sym.to_uppercase(),
        amount, token_in_sym.to_uppercase(),
        amount_out, token_out_sym.to_uppercase(),
        slippage_bps as f64 / 100.0,
        min_out, token_out_sym.to_uppercase(),
        token_in_sym.to_uppercase(), price, token_out_sym.to_uppercase(),
        route_info,
        fee_tier as f64 / 10000.0,
    ))
}

/// Execute a token swap on Uniswap V3.
pub async fn execute_dex_swap(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> EngineResult<String> {
    let rpc_url = creds.get("DEX_RPC_URL").ok_or("Missing DEX_RPC_URL")?;
    let wallet_address = creds.get("DEX_WALLET_ADDRESS").ok_or("No wallet. Use dex_wallet_create first.")?;
    let private_key_hex = creds.get("DEX_PRIVATE_KEY").ok_or("Missing private key")?;

    let token_in_sym = args["token_in"].as_str().ok_or("dex_swap: missing 'token_in'")?;
    let token_out_sym = args["token_out"].as_str().ok_or("dex_swap: missing 'token_out'")?;
    let amount = args["amount"].as_str().ok_or("dex_swap: missing 'amount'")?;
    let _reason = args["reason"].as_str().unwrap_or("swap");

    let slippage_bps = args.get("slippage_bps")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_SLIPPAGE_BPS);

    if slippage_bps > MAX_SLIPPAGE_BPS {
        return Err(format!("Slippage {}bps exceeds maximum allowed {}bps ({}%)", slippage_bps, MAX_SLIPPAGE_BPS, MAX_SLIPPAGE_BPS as f64 / 100.0).into());
    }

    let fee_tier = args.get("fee_tier")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_FEE_TIER) as u32;

    let (token_in_addr, token_in_dec, is_eth_in) = resolve_for_swap(token_in_sym)?;
    let (token_out_addr, token_out_dec, _) = resolve_for_swap(token_out_sym)?;

    let amount_raw = amount_to_raw(amount, token_in_dec)?;
    let amount_u256 = parse_u256_decimal(&amount_raw)?;

    let token_in_bytes = parse_address(&token_in_addr)?;
    let token_out_bytes = parse_address(&token_out_addr)?;
    let wallet_bytes = parse_address(wallet_address)?;

    info!("[dex] Swap: {} {} → {} (wallet: {})", amount, token_in_sym, token_out_sym, wallet_address);

    // Step 1: Get quote for minimum output calculation — try single-hop, fall back to multi-hop via WETH
    let weth_bytes = parse_address(WETH_ADDRESS)?;
    let mut use_multihop = false;
    let expected_out: [u8; 32] = {
        let single_calldata = encode_quote_exact_input_single(
            &token_in_bytes,
            &token_out_bytes,
            &amount_u256,
            fee_tier,
        );
        match eth_call(rpc_url, UNISWAP_QUOTER_V2, &single_calldata).await {
            Ok(r) => {
                let qb = hex_decode(&r)?;
                if qb.len() < 32 { return Err("Invalid quoter response".into()); }
                qb[..32].try_into().map_err(|_| "Quoter response byte conversion failed")?
            },
            Err(_) if token_in_bytes != weth_bytes && token_out_bytes != weth_bytes => {
                info!("[dex] Single-hop quote failed, trying multi-hop through WETH");
                use_multihop = true;
                let path = build_multihop_path(
                    &[&token_in_bytes, &weth_bytes, &token_out_bytes],
                    &[fee_tier, fee_tier],
                );
                let multi_calldata = encode_quote_exact_input(&path, &amount_u256);
                let r = eth_call(rpc_url, UNISWAP_QUOTER_V2, &multi_calldata).await?;
                let qb = hex_decode(&r)?;
                if qb.len() < 32 { return Err("Invalid quoter response".into()); }
                qb[..32].try_into().map_err(|_| "Quoter response byte conversion failed")?
            },
            Err(e) => return Err(e),
        }
    };

    // Apply slippage to get minimum output
    let expected_out_hex = hex_encode(&expected_out);
    let expected_out_f64: f64 = raw_to_amount(&expected_out_hex, token_out_dec)?.parse().unwrap_or(0.0);
    let min_out_f64 = expected_out_f64 * (10000.0 - slippage_bps as f64) / 10000.0;
    let min_out_raw = amount_to_raw(&format!("{:.width$}", min_out_f64, width = token_out_dec as usize), token_out_dec)?;
    let min_out_u256 = parse_u256_decimal(&min_out_raw)?;

    // Step 2: If not ETH, check and set token approval
    if !is_eth_in {
        let router_bytes = parse_address(UNISWAP_SWAP_ROUTER_02)?;
        let allowance_data = encode_allowance(&wallet_bytes, &router_bytes);
        let allowance_result = eth_call(rpc_url, &token_in_addr, &allowance_data).await?;
        let allowance_bytes = hex_decode(&allowance_result)?;

        // Check if allowance is sufficient
        let mut needs_approval = true;
        if allowance_bytes.len() >= 32 {
            let allowance_slice: [u8; 32] = allowance_bytes[..32].try_into()
                .map_err(|_| "Failed to parse allowance bytes")?;
            needs_approval = allowance_slice < amount_u256;
        }

        if needs_approval {
            info!("[dex] Approving token {} for router", token_in_addr);
            let max_approval = [0xffu8; 32]; // type(uint256).max
            let approve_data = encode_approve(&router_bytes, &max_approval);

            let pk_bytes = hex_decode(private_key_hex)?;
            let signing_key = k256::ecdsa::SigningKey::from_slice(&pk_bytes)
                .map_err(|e| EngineError::Other(e.to_string()))?;

            let chain_id = eth_chain_id(rpc_url).await?;
            let nonce = eth_get_transaction_count(rpc_url, wallet_address).await?;
            let (priority_fee, max_fee) = get_gas_fees(rpc_url).await?;
            let gas = eth_estimate_gas(rpc_url, wallet_address, &token_in_addr, &approve_data, "0x0").await?;

            let mut token_in_addr_bytes = [0u8; 20];
            token_in_addr_bytes.copy_from_slice(&hex_decode(&token_in_addr)?[..20]);

            let signed_approve = sign_eip1559_transaction(
                chain_id, nonce, priority_fee, max_fee, gas,
                &token_in_addr_bytes, &[0u8; 32], &approve_data, &signing_key,
            )?;

            let approve_hash = eth_send_raw_transaction(rpc_url, &signed_approve).await?;
            info!("[dex] Approval tx: {}", approve_hash);

            // Wait for approval to be mined (poll for up to 60 seconds)
            for _ in 0..30 {
                tokio::time::sleep(Duration::from_secs(2)).await;
                if let Ok(Some(receipt)) = eth_get_transaction_receipt(rpc_url, &approve_hash).await {
                    let status = receipt.get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("0x0");
                    if status == "0x1" {
                        info!("[dex] Token approval confirmed");
                        break;
                    } else {
                        return Err(format!("Token approval transaction failed (reverted). Tx: {}", approve_hash).into());
                    }
                }
            }
        }
    }

    // Step 3: Build the swap transaction (single-hop or multi-hop as determined by quote)
    let swap_data = if use_multihop {
        let path = build_multihop_path(
            &[&token_in_bytes, &weth_bytes, &token_out_bytes],
            &[fee_tier, fee_tier],
        );
        encode_exact_input(
            &path,
            &wallet_bytes,
            &amount_u256,
            &min_out_u256,
        )
    } else {
        encode_exact_input_single(
            &token_in_bytes,
            &token_out_bytes,
            fee_tier,
            &wallet_bytes,
            &amount_u256,
            &min_out_u256,
        )
    };

    let pk_bytes = hex_decode(private_key_hex)?;
    let signing_key = k256::ecdsa::SigningKey::from_slice(&pk_bytes)
        .map_err(|e| EngineError::Other(e.to_string()))?;

    let chain_id = eth_chain_id(rpc_url).await?;
    let nonce = eth_get_transaction_count(rpc_url, wallet_address).await?;
    let (priority_fee, max_fee) = get_gas_fees(rpc_url).await?;

    // Value is the ETH amount if swapping from ETH, otherwise 0
    let value = if is_eth_in { amount_u256 } else { [0u8; 32] };
    let value_hex = if is_eth_in { u256_to_quantity_hex(&value) } else { "0x0".into() };

    let router_bytes = parse_address(UNISWAP_SWAP_ROUTER_02)?;
    let gas = eth_estimate_gas(rpc_url, wallet_address, UNISWAP_SWAP_ROUTER_02, &swap_data, &value_hex).await
        .unwrap_or(300_000); // fallback gas limit for swaps

    let signed_tx = sign_eip1559_transaction(
        chain_id, nonce, priority_fee, max_fee, gas,
        &router_bytes, &value, &swap_data, &signing_key,
    )?;

    // Step 4: Broadcast
    let tx_hash = eth_send_raw_transaction(rpc_url, &signed_tx).await?;
    info!("[dex] Swap tx broadcast: {}", tx_hash);

    // Step 5: Wait for confirmation (up to 2 minutes)
    let mut confirmed = false;
    let mut final_status = "pending";
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_secs(2)).await;
        match eth_get_transaction_receipt(rpc_url, &tx_hash).await {
            Ok(Some(receipt)) => {
                let status = receipt.get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0x0");
                if status == "0x1" {
                    confirmed = true;
                    final_status = "confirmed";
                } else {
                    final_status = "reverted";
                }
                break;
            }
            Ok(None) => continue, // Not mined yet
            Err(_) => continue,
        }
    }

    let network = explorer_tx_url(chain_id);

    let expected_out_display = raw_to_amount(&expected_out_hex, token_out_dec).unwrap_or("?".into());

    Ok(format!(
        "{} Swap {}\n\n{} {} → ~{} {}\nSlippage tolerance: {}%\nTransaction: {}{}\nStatus: {}\n\n{}",
        if confirmed { "✅" } else { "⚠️" },
        if confirmed { "Confirmed" } else { "Submitted" },
        amount, token_in_sym.to_uppercase(),
        expected_out_display, token_out_sym.to_uppercase(),
        slippage_bps as f64 / 100.0,
        network, tx_hash,
        final_status,
        if !confirmed && final_status == "pending" {
            "Transaction is still pending. Check the explorer link for status."
        } else if final_status == "reverted" {
            "Transaction reverted! The swap may have failed due to slippage or liquidity issues. Your tokens are safe."
        } else { "" },
    ))
}
