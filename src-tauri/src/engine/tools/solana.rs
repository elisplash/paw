// Paw Agent Engine — Solana DEX / Jupiter tools

use crate::atoms::types::*;
use crate::engine::state::EngineState;
use tauri::Manager;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition { tool_type: "function".into(), function: FunctionDefinition {
            name: "sol_wallet_create".into(),
            description: "Create a new self-custody Solana wallet (ed25519). The private key is encrypted and stored in the OS keychain vault — you never see it. Returns the wallet address.".into(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        }},
        ToolDefinition { tool_type: "function".into(), function: FunctionDefinition {
            name: "sol_balance".into(),
            description: "Check SOL and SPL token balances for the Solana wallet. If no token specified, shows SOL and all tokens with non-zero balances.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "token": { "type": "string", "description": "Specific token to check (e.g. 'USDC', 'BONK', 'JUP', or a mint address). Omit to check all tokens." }
                }
            }),
        }},
        ToolDefinition { tool_type: "function".into(), function: FunctionDefinition {
            name: "sol_quote".into(),
            description: "Get a swap quote from Jupiter aggregator on Solana without executing. Shows expected output amount, exchange rate, price impact, and route. ALWAYS use this before sol_swap.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "token_in": { "type": "string", "description": "Token to sell (e.g. 'SOL', 'USDC', 'BONK', or mint address)" },
                    "token_out": { "type": "string", "description": "Token to buy (e.g. 'USDC', 'SOL', 'JUP', or mint address)" },
                    "amount": { "type": "string", "description": "Amount of token_in to swap (e.g. '1.5', '100')" },
                    "slippage_bps": { "type": "integer", "description": "Slippage tolerance in basis points. Default: 50 (0.5%). Max: 500 (5%)" }
                },
                "required": ["token_in", "token_out", "amount"]
            }),
        }},
        ToolDefinition { tool_type: "function".into(), function: FunctionDefinition {
            name: "sol_swap".into(),
            description: "Execute a token swap on Solana via Jupiter aggregator. REQUIRES USER APPROVAL. Gets a quote from Jupiter, builds and signs the transaction, then broadcasts it. The private key never leaves the vault.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "token_in": { "type": "string", "description": "Token to sell (e.g. 'SOL', 'USDC', 'BONK')" },
                    "token_out": { "type": "string", "description": "Token to buy (e.g. 'USDC', 'SOL', 'JUP')" },
                    "amount": { "type": "string", "description": "Amount of token_in to swap (e.g. '0.5', '100')" },
                    "reason": { "type": "string", "description": "Reason for this swap (shown in approval modal and trade history)" },
                    "slippage_bps": { "type": "integer", "description": "Slippage tolerance in basis points. Default: 50 (0.5%). Max: 500 (5%)" }
                },
                "required": ["token_in", "token_out", "amount", "reason"]
            }),
        }},
        ToolDefinition { tool_type: "function".into(), function: FunctionDefinition {
            name: "sol_portfolio".into(),
            description: "Get a complete Solana portfolio view: SOL balance + all SPL token balances + network info.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "tokens": { "type": "array", "items": { "type": "string" }, "description": "Additional SPL token mint addresses to check beyond auto-detected holdings" }
                }
            }),
        }},
        ToolDefinition { tool_type: "function".into(), function: FunctionDefinition {
            name: "sol_token_info".into(),
            description: "Get on-chain info about any SPL token: decimals, total supply, mint authority, freeze authority, and token program. Queries the blockchain directly.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "mint_address": { "type": "string", "description": "The SPL token mint address or known symbol (e.g. 'USDC', 'BONK', or a base58 mint address)" }
                },
                "required": ["mint_address"]
            }),
        }},
        ToolDefinition { tool_type: "function".into(), function: FunctionDefinition {
            name: "sol_transfer".into(),
            description: "Transfer SOL or SPL tokens from your Solana wallet to any external Solana address. REQUIRES USER APPROVAL. Wallet needs SOL for transaction fees (~0.005 SOL).".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "currency": { "type": "string", "description": "Token to send: 'SOL' for native SOL, or a token symbol or SPL mint address" },
                    "amount": { "type": "string", "description": "Amount to send in human-readable units (e.g. '1.5' for 1.5 SOL, '100' for 100 USDC)" },
                    "to_address": { "type": "string", "description": "Recipient Solana address (base58-encoded public key)" },
                    "reason": { "type": "string", "description": "Brief explanation of why this transfer is being made" }
                },
                "required": ["currency", "amount", "to_address", "reason"]
            }),
        }},
    ]
}

pub async fn execute(
    name: &str,
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> Option<Result<String, String>> {
    let creds = match super::get_skill_creds("solana_dex", app_handle) {
        Ok(c) => c,
        Err(e) => return Some(Err(e.to_string())),
    };
    let state = app_handle.state::<EngineState>();
    Some(match name {
        "sol_wallet_create" => crate::engine::sol_dex::execute_sol_wallet_create(args, &creds, app_handle).await.map_err(|e| e.to_string()),
        "sol_balance"       => crate::engine::sol_dex::execute_sol_balance(args, &creds).await.map_err(|e| e.to_string()),
        "sol_quote"         => crate::engine::sol_dex::execute_sol_quote(args, &creds).await.map_err(|e| e.to_string()),
        "sol_swap" => {
            let result = crate::engine::sol_dex::execute_sol_swap(args, &creds).await;
            if result.is_ok() {
                let token_in = args["token_in"].as_str().unwrap_or("?");
                let token_out = args["token_out"].as_str().unwrap_or("?");
                let pair = format!("{} -> {}", token_in.to_uppercase(), token_out.to_uppercase());
                let _ = state.store.insert_trade(
                    "sol_swap", Some("swap"), Some(&pair),
                    args["token_in"].as_str(), args["amount"].as_str().unwrap_or("0"),
                    None, None, "completed", None,
                    args["token_out"].as_str(),
                    args["reason"].as_str().unwrap_or(""),
                    None, None, result.as_ref().ok().map(|s| s.as_str()),
                );

                // Auto-open position on BUY (when spending SOL to get a token)
                let is_buy = {
                    let tin = token_in.trim().to_uppercase();
                    tin == "SOL" || tin == "SO11111111111111111111111111111111111111112"
                };
                if is_buy {
                    let amount_sol_str = args["amount"].as_str().unwrap_or("0");
                    let amount_sol: f64 = amount_sol_str.parse().unwrap_or(0.0);
                    let output_mint = token_out.trim().to_string();
                    let symbol = token_out.trim().to_uppercase();
                    let app = app_handle.clone();
                    let result_text = result.as_ref().map(|r| r.clone()).unwrap_or_default();
                    tokio::spawn(async move {
                        let price = crate::engine::sol_dex::get_token_price_usd(&output_mint).await.unwrap_or(0.0);
                        let received_amount: f64 = result_text
                            .lines()
                            .find(|l| l.contains("Received"))
                            .and_then(|l| {
                                let parts: Vec<&str> = l.split('|').collect();
                                if parts.len() >= 3 {
                                    parts[2].split_whitespace().next()
                                        .and_then(|s| s.replace(',', "").parse::<f64>().ok())
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(0.0);

                        if received_amount > 0.0 && price > 0.0 {
                            if let Some(st) = app.try_state::<EngineState>() {
                                match st.store.insert_position(
                                    &output_mint, &symbol, price, amount_sol,
                                    received_amount, 0.30, 2.0, None,
                                ) {
                                    Ok(id) => log::info!("[positions] Auto-opened position {} for {} tokens of {}", id, received_amount, symbol),
                                    Err(e) => log::warn!("[positions] Failed to auto-open position: {}", e),
                                }
                            }
                        } else {
                            log::info!("[positions] Skipped position for {} — price={}, amount={}", symbol, price, received_amount);
                        }
                    });
                }
            }
            result.map_err(|e| e.to_string())
        }
        "sol_portfolio"  => crate::engine::sol_dex::execute_sol_portfolio(args, &creds).await.map_err(|e| e.to_string()),
        "sol_token_info" => crate::engine::sol_dex::execute_sol_token_info(args, &creds).await.map_err(|e| e.to_string()),
        "sol_transfer" => {
            let result = crate::engine::sol_dex::execute_sol_transfer(args, &creds).await;
            if result.is_ok() {
                let currency = args["currency"].as_str().unwrap_or("?");
                let _ = state.store.insert_trade(
                    "sol_transfer", Some("transfer"), Some(&currency.to_uppercase()),
                    args["currency"].as_str(), args["amount"].as_str().unwrap_or("0"),
                    None, None, "completed", None,
                    args["to_address"].as_str(),
                    args["reason"].as_str().unwrap_or(""),
                    None, None, result.as_ref().ok().map(|s| s.as_str()),
                );
            }
            result.map_err(|e| e.to_string())
        }
        _ => return None,
    })
}
