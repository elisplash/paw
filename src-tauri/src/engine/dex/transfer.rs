// Paw Agent Engine — DEX ERC-20 / ETH Transfer

use super::abi::{encode_balance_of, encode_transfer};
use super::constants::explorer_tx_url;
use super::primitives::{amount_to_raw, hex_decode, parse_address, parse_u256_decimal, raw_to_amount};
use super::rpc::{
    eth_chain_id, eth_estimate_gas, eth_get_balance, eth_call, eth_get_transaction_count,
    eth_get_transaction_receipt, eth_send_raw_transaction, get_gas_fees,
};
use super::tokens::resolve_token;
use super::tx::sign_eip1559_transaction;
use std::collections::HashMap;
use std::time::Duration;
use crate::atoms::error::{EngineResult, EngineError};

/// Transfer ETH or ERC-20 tokens to an external address.
/// For ETH: simple value transfer (21000 gas, no calldata).
/// For ERC-20: calls transfer(address,uint256) on the token contract.
pub async fn execute_dex_transfer(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> EngineResult<String> {
    let rpc_url = creds.get("DEX_RPC_URL").ok_or("Missing DEX_RPC_URL")?;
    let wallet_address = creds.get("DEX_WALLET_ADDRESS").ok_or("No wallet. Use dex_wallet_create first.")?;
    let private_key_hex = creds.get("DEX_PRIVATE_KEY").ok_or("Missing private key")?;

    let currency = args["currency"].as_str().ok_or("dex_transfer: missing 'currency'")?;
    let amount_str = args["amount"].as_str().ok_or("dex_transfer: missing 'amount'")?;
    let to_address = args["to_address"].as_str().ok_or("dex_transfer: missing 'to_address'")?;
    let _reason = args["reason"].as_str().unwrap_or("transfer");

    // Validate recipient address
    let to_bytes = parse_address(to_address)?;

    let currency_upper = currency.trim().to_uppercase();
    let is_eth = currency_upper == "ETH";

    let pk_bytes = hex_decode(private_key_hex)?;
    let signing_key = k256::ecdsa::SigningKey::from_slice(&pk_bytes)
        .map_err(|e| EngineError::Other(e.to_string()))?;

    let chain_id = eth_chain_id(rpc_url).await?;
    let nonce = eth_get_transaction_count(rpc_url, wallet_address).await?;
    let (priority_fee, max_fee) = get_gas_fees(rpc_url).await?;

    let tx_hash = if is_eth {
        // ── Native ETH transfer ──
        let decimals = 18u8;
        let amount_raw = amount_to_raw(amount_str, decimals)?;
        let value_u256 = parse_u256_decimal(&amount_raw)?;

        // Check ETH balance
        let balance_hex = eth_get_balance(rpc_url, wallet_address).await?;
        let balance_bytes = hex_decode(&balance_hex)?;
        let mut balance_u256 = [0u8; 32];
        let offset = 32usize.saturating_sub(balance_bytes.len());
        balance_u256[offset..].copy_from_slice(&balance_bytes[..std::cmp::min(balance_bytes.len(), 32)]);
        if balance_u256 < value_u256 {
            let bal_display = raw_to_amount(&balance_hex, decimals).unwrap_or("?".into());
            return Err(format!("Insufficient ETH balance. Have: {} ETH, need: {} ETH", bal_display, amount_str).into());
        }

        // ETH transfer: 21000 gas, empty data
        let gas = 21_000u64;
        let signed_tx = sign_eip1559_transaction(
            chain_id, nonce, priority_fee, max_fee, gas,
            &to_bytes, &value_u256, &[], &signing_key,
        )?;
        eth_send_raw_transaction(rpc_url, &signed_tx).await?
    } else {
        // ── ERC-20 transfer ──
        let (token_addr, decimals) = resolve_token(currency)?;
        let amount_raw = amount_to_raw(amount_str, decimals)?;
        let amount_u256 = parse_u256_decimal(&amount_raw)?;

        // Check ERC-20 balance
        let wallet_bytes = parse_address(wallet_address)?;
        let bal_data = encode_balance_of(&wallet_bytes);
        let bal_result = eth_call(rpc_url, &token_addr, &bal_data).await?;
        let bal_bytes = hex_decode(&bal_result)?;
        let mut balance_u256 = [0u8; 32];
        let offset = 32usize.saturating_sub(bal_bytes.len());
        balance_u256[offset..].copy_from_slice(&bal_bytes[..std::cmp::min(bal_bytes.len(), 32)]);
        if balance_u256 < amount_u256 {
            let bal_display = raw_to_amount(&bal_result, decimals).unwrap_or("?".into());
            return Err(format!("Insufficient {} balance. Have: {}, need: {}", currency_upper, bal_display, amount_str).into());
        }

        // Check ETH balance for gas
        let eth_balance_hex = eth_get_balance(rpc_url, wallet_address).await?;
        let eth_balance_bytes = hex_decode(&eth_balance_hex)?;
        let mut eth_balance = [0u8; 32];
        let eth_off = 32usize.saturating_sub(eth_balance_bytes.len());
        eth_balance[eth_off..].copy_from_slice(&eth_balance_bytes[..std::cmp::min(eth_balance_bytes.len(), 32)]);
        if eth_balance == [0u8; 32] {
            return Err("No ETH for gas fees. Deposit ETH to your wallet first.".into());
        }

        // Build transfer(to, amount) calldata
        let transfer_data = encode_transfer(&to_bytes, &amount_u256);

        // Token contract address as [u8; 20]
        let mut token_addr_bytes = [0u8; 20];
        let token_addr_raw = hex_decode(&token_addr)?;
        token_addr_bytes.copy_from_slice(&token_addr_raw[..20]);

        let gas = eth_estimate_gas(rpc_url, wallet_address, &token_addr, &transfer_data, "0x0").await
            .unwrap_or(65_000);

        let signed_tx = sign_eip1559_transaction(
            chain_id, nonce, priority_fee, max_fee, gas,
            &token_addr_bytes, &[0u8; 32], &transfer_data, &signing_key,
        )?;
        eth_send_raw_transaction(rpc_url, &signed_tx).await?
    };

    // Wait for confirmation (up to 2 min)
    let mut confirmed = false;
    let mut final_status = "pending";
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_secs(2)).await;
        match eth_get_transaction_receipt(rpc_url, &tx_hash).await {
            Ok(Some(receipt)) => {
                let status = receipt.get("status").and_then(|v| v.as_str()).unwrap_or("0x0");
                if status == "0x1" { confirmed = true; final_status = "confirmed"; }
                else { final_status = "reverted"; }
                break;
            }
            _ => continue,
        }
    }

    let network = explorer_tx_url(chain_id);

    Ok(format!(
        "{} Transfer {}\n\n{} {} → {}\nTx: {}{}\nStatus: {}",
        if confirmed { "✅" } else { "⚠️" },
        if confirmed { "Confirmed" } else { "Submitted" },
        amount_str, currency_upper,
        to_address,
        network, tx_hash, final_status,
    ))
}
