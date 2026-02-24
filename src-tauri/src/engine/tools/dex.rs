// Paw Agent Engine — DEX / Uniswap V3 tools

use crate::atoms::types::*;
use crate::engine::state::EngineState;
use tauri::Manager;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition { tool_type: "function".into(), function: FunctionDefinition {
            name: "dex_wallet_create".into(),
            description: "Create a new self-custody Ethereum wallet. The private key is encrypted and stored in the OS keychain vault — you never see it. Returns the wallet address.".into(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        }},
        ToolDefinition { tool_type: "function".into(), function: FunctionDefinition {
            name: "dex_balance".into(),
            description: "Check ETH and ERC-20 token balances for the DEX wallet. If no token specified, shows ETH and all tokens with non-zero balances.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "token": { "type": "string", "description": "Specific token to check (e.g. 'USDC', 'WBTC', or a contract address). Omit to check all known tokens." }
                }
            }),
        }},
        ToolDefinition { tool_type: "function".into(), function: FunctionDefinition {
            name: "dex_quote".into(),
            description: "Get a swap quote from Uniswap V3 without executing. Shows expected output amount, exchange rate, and minimum output with slippage protection. ALWAYS use this before dex_swap.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "token_in": { "type": "string", "description": "Token to sell (e.g. 'ETH', 'USDC', 'WBTC', or contract address)" },
                    "token_out": { "type": "string", "description": "Token to buy (e.g. 'USDC', 'ETH', 'UNI', or contract address)" },
                    "amount": { "type": "string", "description": "Amount of token_in to swap (e.g. '0.5', '100')" },
                    "fee_tier": { "type": "integer", "description": "Uniswap V3 fee tier in bps: 100, 500, 3000, 10000. Default: 3000" },
                    "slippage_bps": { "type": "integer", "description": "Slippage tolerance in basis points. Default: 50 (0.5%). Max: 500 (5%)" }
                },
                "required": ["token_in", "token_out", "amount"]
            }),
        }},
        ToolDefinition { tool_type: "function".into(), function: FunctionDefinition {
            name: "dex_swap".into(),
            description: "Execute a token swap on Uniswap V3. REQUIRES USER APPROVAL. Gets a quote, handles token approval if needed, builds and signs the transaction, then broadcasts it. The private key never leaves the vault.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "token_in": { "type": "string", "description": "Token to sell (e.g. 'ETH', 'USDC', 'WBTC')" },
                    "token_out": { "type": "string", "description": "Token to buy (e.g. 'USDC', 'ETH', 'UNI')" },
                    "amount": { "type": "string", "description": "Amount of token_in to swap (e.g. '0.1', '50')" },
                    "reason": { "type": "string", "description": "Reason for this swap (shown in approval modal and trade history)" },
                    "fee_tier": { "type": "integer", "description": "Uniswap V3 fee tier: 100, 500, 3000 (default), or 10000" },
                    "slippage_bps": { "type": "integer", "description": "Slippage tolerance in basis points. Default: 50 (0.5%). Max: 500 (5%)" }
                },
                "required": ["token_in", "token_out", "amount", "reason"]
            }),
        }},
        ToolDefinition { tool_type: "function".into(), function: FunctionDefinition {
            name: "dex_portfolio".into(),
            description: "Get a complete portfolio view: ETH balance + all known ERC-20 token balances + network info.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "tokens": { "type": "array", "items": { "type": "string" }, "description": "Additional ERC-20 contract addresses to check beyond the built-in list" }
                }
            }),
        }},
        ToolDefinition { tool_type: "function".into(), function: FunctionDefinition {
            name: "dex_token_info".into(),
            description: "Get comprehensive on-chain info about any ERC-20 token by its contract address. Reads name, symbol, decimals, total supply, owner, contract code size, and tests swap viability on Uniswap V3.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "token_address": { "type": "string", "description": "The ERC-20 contract address to analyze (0x-prefixed, 42 chars)" }
                },
                "required": ["token_address"]
            }),
        }},
        ToolDefinition { tool_type: "function".into(), function: FunctionDefinition {
            name: "dex_check_token".into(),
            description: "Run automated safety checks on a token contract before trading. Tests: contract verification, ERC-20 compliance, ownership renouncement, HONEYPOT detection (simulates buy AND sell on Uniswap), round-trip tax analysis. Returns a risk score 0-30 and explicit honeypot verdict. Always run this before trading any new token.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "token_address": { "type": "string", "description": "The ERC-20 contract address to safety-check (0x-prefixed, 42 chars)" }
                },
                "required": ["token_address"]
            }),
        }},
        ToolDefinition { tool_type: "function".into(), function: FunctionDefinition {
            name: "dex_search_token".into(),
            description: "Search for tokens by name or symbol to find their contract addresses, prices, volume, and liquidity. Uses the DexScreener API. Supports all chains (Ethereum, Base, Arbitrum, etc.).".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Token name or symbol to search for (e.g. 'KIMCHI', 'pepe', 'uniswap')" },
                    "chain": { "type": "string", "description": "Optional: filter results to a specific chain (e.g. 'base', 'ethereum', 'arbitrum')" },
                    "max_results": { "type": "integer", "description": "Maximum results to return (default 10, max 25)" }
                },
                "required": ["query"]
            }),
        }},
        ToolDefinition { tool_type: "function".into(), function: FunctionDefinition {
            name: "dex_watch_wallet".into(),
            description: "Monitor any wallet address: shows ETH balance, known token holdings, and recent ERC-20 transfers (buys/sells). Use this to track smart money wallets, alpha traders, and whale activity.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "wallet_address": { "type": "string", "description": "The wallet address to monitor (0x-prefixed)" },
                    "blocks_back": { "type": "integer", "description": "How many blocks back to scan for transfers (default 1000, ~3 hours on mainnet)" },
                    "tokens": { "type": "array", "items": { "type": "string" }, "description": "Additional token contract addresses to check holdings for" }
                },
                "required": ["wallet_address"]
            }),
        }},
        ToolDefinition { tool_type: "function".into(), function: FunctionDefinition {
            name: "dex_whale_transfers".into(),
            description: "Scan recent large transfers of a specific token to detect whale accumulation or distribution.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "token_address": { "type": "string", "description": "The ERC-20 token contract address to scan" },
                    "blocks_back": { "type": "integer", "description": "How many blocks back to scan (default 2000)" },
                    "min_amount": { "type": "string", "description": "Minimum transfer amount to show (in token units)" }
                },
                "required": ["token_address"]
            }),
        }},
        ToolDefinition { tool_type: "function".into(), function: FunctionDefinition {
            name: "dex_top_traders".into(),
            description: "Analyze on-chain Transfer events for a token to discover the most profitable wallets. Profiles each wallet by PnL, trade count, timing, and trader classification.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "token_address": { "type": "string", "description": "The ERC-20 token contract address to analyze traders for" },
                    "blocks_back": { "type": "integer", "description": "How many blocks back to scan (default 5000)" },
                    "min_trades": { "type": "integer", "description": "Minimum number of trades for a wallet to be included (default 2)" }
                },
                "required": ["token_address"]
            }),
        }},
        ToolDefinition { tool_type: "function".into(), function: FunctionDefinition {
            name: "dex_trending".into(),
            description: "Get trending and recently boosted tokens from DexScreener. No API key needed. Use chain filter to focus on specific networks.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "chain": { "type": "string", "description": "Optional: filter to a specific chain (e.g. 'ethereum', 'base', 'solana', 'arbitrum')" },
                    "max_results": { "type": "integer", "description": "Maximum results per category (default 20, max 50)" }
                }
            }),
        }},
        ToolDefinition { tool_type: "function".into(), function: FunctionDefinition {
            name: "dex_transfer".into(),
            description: "Transfer ETH or ERC-20 tokens from your DEX wallet to any external Ethereum address. REQUIRES USER APPROVAL.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "currency": { "type": "string", "description": "Token to send: 'ETH' for native Ether, or a token symbol or ERC-20 contract address" },
                    "amount": { "type": "string", "description": "Amount to send in human-readable units (e.g. '0.5' for 0.5 ETH, '100' for 100 USDC)" },
                    "to_address": { "type": "string", "description": "Recipient Ethereum address (0x-prefixed, 42 characters)" },
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
    let creds = match super::get_skill_creds("dex", app_handle) {
        Ok(c) => c,
        Err(e) => return Some(Err(e.to_string())),
    };
    let state = app_handle.state::<EngineState>();
    Some(match name {
        "dex_wallet_create"  => crate::engine::dex::execute_dex_wallet_create(args, &creds, app_handle).await.map_err(|e| e.to_string()),
        "dex_balance"        => crate::engine::dex::execute_dex_balance(args, &creds).await.map_err(|e| e.to_string()),
        "dex_quote"          => crate::engine::dex::execute_dex_quote(args, &creds).await.map_err(|e| e.to_string()),
        "dex_swap" => {
            let result = crate::engine::dex::execute_dex_swap(args, &creds).await;
            if result.is_ok() {
                let token_in = args["token_in"].as_str().unwrap_or("?");
                let token_out = args["token_out"].as_str().unwrap_or("?");
                let pair = format!("{} -> {}", token_in.to_uppercase(), token_out.to_uppercase());
                let _ = state.store.insert_trade(
                    "dex_swap", Some("swap"), Some(&pair),
                    args["token_in"].as_str(), args["amount"].as_str().unwrap_or("0"),
                    None, None, "completed", None,
                    args["token_out"].as_str(),
                    args["reason"].as_str().unwrap_or(""),
                    None, None, result.as_ref().ok().map(|s| s.as_str()),
                );
            }
            result.map_err(|e| e.to_string())
        }
        "dex_portfolio"      => crate::engine::dex::execute_dex_portfolio(args, &creds).await.map_err(|e| e.to_string()),
        "dex_token_info"     => crate::engine::dex::execute_dex_token_info(args, &creds).await.map_err(|e| e.to_string()),
        "dex_check_token"    => crate::engine::dex::execute_dex_check_token(args, &creds).await.map_err(|e| e.to_string()),
        "dex_search_token"   => crate::engine::dex::execute_dex_search_token(args, &creds).await.map_err(|e| e.to_string()),
        "dex_watch_wallet"   => crate::engine::dex::execute_dex_watch_wallet(args, &creds).await.map_err(|e| e.to_string()),
        "dex_whale_transfers"=> crate::engine::dex::execute_dex_whale_transfers(args, &creds).await.map_err(|e| e.to_string()),
        "dex_top_traders"    => crate::engine::dex::execute_dex_top_traders(args, &creds).await.map_err(|e| e.to_string()),
        "dex_trending"       => crate::engine::dex::execute_dex_trending(args, &creds).await.map_err(|e| e.to_string()),
        "dex_transfer" => {
            let result = crate::engine::dex::execute_dex_transfer(args, &creds).await;
            if result.is_ok() {
                let currency = args["currency"].as_str().unwrap_or("?");
                let _ = state.store.insert_trade(
                    "dex_transfer", Some("transfer"), Some(&currency.to_uppercase()),
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
