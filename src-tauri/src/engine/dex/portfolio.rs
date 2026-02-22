// Paw Agent Engine — DEX Portfolio / Balance Queries

use super::abi::encode_balance_of;
use super::constants::{chain_name, KNOWN_TOKENS};
use super::primitives::{parse_address, raw_to_amount};
use super::rpc::{eth_call, eth_chain_id, eth_get_balance};
use super::tokens::resolve_token;
use std::collections::HashMap;
use crate::atoms::error::EngineResult;

/// Check ETH and ERC-20 token balances for a single token or all known tokens.
pub async fn execute_dex_balance(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> EngineResult<String> {
    let rpc_url = creds.get("DEX_RPC_URL").ok_or("Missing DEX_RPC_URL. Configure your RPC endpoint (Infura/Alchemy) in Skills → DEX Trading.")?;
    let wallet_address = creds.get("DEX_WALLET_ADDRESS").ok_or("No wallet found. Use dex_wallet_create first.")?;

    // Optional: specific token to check
    let token = args.get("token").and_then(|v| v.as_str());

    let mut output = format!("Wallet: {}\n\n", wallet_address);

    // Always show ETH balance
    let eth_balance_hex = eth_get_balance(rpc_url, wallet_address).await?;
    let eth_balance = raw_to_amount(&eth_balance_hex, 18)?;
    output.push_str(&format!("ETH: {} ETH\n", eth_balance));

    if let Some(token_sym) = token {
        // Check specific token
        let (token_addr, decimals) = resolve_token(token_sym)?;
        if token_addr != "0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE" {
            let wallet_bytes = parse_address(wallet_address)?;
            let calldata = encode_balance_of(&wallet_bytes);
            let result = eth_call(rpc_url, &token_addr, &calldata).await?;
            let balance = raw_to_amount(&result, decimals)?;
            output.push_str(&format!("{}: {}\n", token_sym.to_uppercase(), balance));
        }
    } else {
        // Check common tokens
        let wallet_bytes = parse_address(wallet_address)?;
        for (sym, addr, dec) in KNOWN_TOKENS {
            if *sym == "ETH" { continue; }
            let calldata = encode_balance_of(&wallet_bytes);
            if let Ok(result) = eth_call(rpc_url, addr, &calldata).await {
                if let Ok(balance) = raw_to_amount(&result, *dec) {
                    if balance != "0" {
                        output.push_str(&format!("{}: {}\n", sym, balance));
                    }
                }
            }
        }
    }

    Ok(output)
}

/// Check multiple token balances at once (full portfolio view).
pub async fn execute_dex_portfolio(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> EngineResult<String> {
    let rpc_url = creds.get("DEX_RPC_URL").ok_or("Missing DEX_RPC_URL")?;
    let wallet_address = creds.get("DEX_WALLET_ADDRESS").ok_or("No wallet. Use dex_wallet_create first.")?;

    let wallet_bytes = parse_address(wallet_address)?;

    let mut output = format!("📊 Portfolio for {}\n\n", wallet_address);

    // ETH balance
    let eth_hex = eth_get_balance(rpc_url, wallet_address).await?;
    let eth_balance = raw_to_amount(&eth_hex, 18)?;
    output.push_str(&format!("  ETH: {} ETH\n", eth_balance));

    // Check all known tokens
    let mut has_tokens = false;
    for (sym, addr, dec) in KNOWN_TOKENS {
        if *sym == "ETH" { continue; }
        let calldata = encode_balance_of(&wallet_bytes);
        if let Ok(result) = eth_call(rpc_url, addr, &calldata).await {
            if let Ok(balance) = raw_to_amount(&result, *dec) {
                if balance != "0" {
                    output.push_str(&format!("  {}: {}\n", sym, balance));
                    has_tokens = true;
                }
            }
        }
    }

    // Also check any custom tokens specified
    if let Some(tokens) = args.get("tokens").and_then(|v| v.as_array()) {
        for token in tokens {
            if let Some(addr) = token.as_str() {
                let calldata = encode_balance_of(&wallet_bytes);
                if let Ok(result) = eth_call(rpc_url, addr, &calldata).await {
                    if let Ok(balance) = raw_to_amount(&result, 18) {
                        if balance != "0" {
                            output.push_str(&format!("  {}: {}\n", addr, balance));
                            has_tokens = true;
                        }
                    }
                }
            }
        }
    }

    if !has_tokens {
        output.push_str("\n  No ERC-20 token balances found.\n");
    }

    // Get chain info
    if let Ok(id) = eth_chain_id(rpc_url).await {
        let chain = chain_name(id);
        output.push_str(&format!("\nNetwork: {} (chain ID {})\n", chain, id));
    }

    Ok(output)
}
