// Solana DEX — Portfolio & Token Info
// execute_sol_balance, execute_sol_portfolio, execute_sol_token_info

use std::collections::HashMap;
use super::constants::KNOWN_TOKENS;
use super::helpers::{lamports_to_amount, resolve_token};
use super::rpc::{get_sol_balance, get_token_accounts, rpc_call};
use crate::atoms::error::EngineResult;

/// sol_balance — Check SOL + SPL token balances
pub async fn execute_sol_balance(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> EngineResult<String> {
    let rpc_url = creds.get("SOLANA_RPC_URL")
        .ok_or("Missing SOLANA_RPC_URL. Configure your Solana RPC endpoint in Skills → Solana DEX Trading.")?;
    let wallet = creds.get("SOLANA_WALLET_ADDRESS")
        .ok_or("No Solana wallet found. Use sol_wallet_create first.")?;

    let token_filter = args.get("token").and_then(|v| v.as_str());

    let mut output = format!("**Solana Wallet**: `{}`\n\n", wallet);

    // Get SOL balance
    let sol_lamports = get_sol_balance(rpc_url, wallet).await?;
    let sol_amount = lamports_to_amount(sol_lamports, 9);
    output.push_str(&format!("**SOL**: {} SOL\n", sol_amount));

    if let Some(token_sym) = token_filter {
        // Check specific token
        let (mint, known_decimals) = resolve_token(token_sym)?;
        if mint == "So11111111111111111111111111111111111111112" {
            // Already showed SOL balance above
        } else {
            let accounts = get_token_accounts(rpc_url, wallet).await?;
            let mut found = false;
            for (acct_mint, amount, decimals, _) in &accounts {
                if acct_mint == &mint {
                    let dec = if *decimals > 0 { *decimals } else { known_decimals };
                    output.push_str(&format!("**{}**: {}\n", token_sym.to_uppercase(), lamports_to_amount(*amount, dec)));
                    found = true;
                    break;
                }
            }
            if !found {
                output.push_str(&format!("**{}**: 0 (no token account)\n", token_sym.to_uppercase()));
            }
        }
    } else {
        // Show all SPL token balances
        let accounts = get_token_accounts(rpc_url, wallet).await?;
        if !accounts.is_empty() {
            output.push_str("\n**SPL Tokens**:\n");
            for (mint, amount, decimals, _) in &accounts {
                // Try to resolve symbol
                let symbol = KNOWN_TOKENS.iter()
                    .find(|(_, addr, _)| addr == mint)
                    .map(|(sym, _, _)| sym.to_string())
                    .unwrap_or_else(|| format!("{}…{}", &mint[..4], &mint[mint.len()-4..]));
                output.push_str(&format!("  {} : {}\n", symbol, lamports_to_amount(*amount, *decimals)));
            }
        }
    }

    Ok(output)
}

/// sol_portfolio — Multi-token balance scan
pub async fn execute_sol_portfolio(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> EngineResult<String> {
    let rpc_url = creds.get("SOLANA_RPC_URL")
        .ok_or("Missing SOLANA_RPC_URL.")?;
    let wallet = creds.get("SOLANA_WALLET_ADDRESS")
        .ok_or("No Solana wallet. Use sol_wallet_create first.")?;

    let mut output = format!("## Solana Portfolio\n**Wallet**: `{}`\n\n", wallet);

    // SOL balance
    let sol_lamports = get_sol_balance(rpc_url, wallet).await?;
    let sol_amount = lamports_to_amount(sol_lamports, 9);
    output.push_str("| Token | Balance |\n|-------|--------|\n");
    output.push_str(&format!("| SOL | {} |\n", sol_amount));

    // All SPL token accounts
    let accounts = get_token_accounts(rpc_url, wallet).await?;

    // Extra mints to check from args
    let _extra_mints: Vec<String> = args.get("tokens")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();

    for (mint, amount, decimals, _) in &accounts {
        let symbol = KNOWN_TOKENS.iter()
            .find(|(_, addr, _)| addr == mint)
            .map(|(sym, _, _)| sym.to_string())
            .unwrap_or_else(|| format!("{}…{}", &mint[..4], &mint[mint.len()-4..]));
        output.push_str(&format!("| {} | {} |\n", symbol, lamports_to_amount(*amount, *decimals)));
    }

    if accounts.is_empty() {
        output.push_str("\n_No SPL token holdings found._\n");
    }

    // Get cluster version as network info
    if let Ok(ver) = rpc_call(rpc_url, "getVersion", serde_json::json!([])).await {
        let v = ver.get("solana-core").and_then(|v| v.as_str()).unwrap_or("?");
        output.push_str(&format!("\n**Network**: Solana Mainnet (node v{})\n", v));
    }

    Ok(output)
}

/// sol_token_info — Get on-chain token metadata
pub async fn execute_sol_token_info(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> EngineResult<String> {
    let rpc_url = creds.get("SOLANA_RPC_URL")
        .ok_or("Missing SOLANA_RPC_URL.")?;

    let mint_input = args["mint_address"].as_str()
        .or_else(|| args["token_address"].as_str())
        .ok_or("sol_token_info: missing 'mint_address'")?;

    // Resolve symbol to mint if needed
    let (mint, _) = resolve_token(mint_input)?;

    let mut output = format!("## Solana Token Info\n**Mint**: `{}`\n\n", mint);

    // Get mint account info
    let mint_result = rpc_call(rpc_url, "getAccountInfo", serde_json::json!([
        mint,
        { "encoding": "jsonParsed" }
    ])).await?;

    let account = mint_result.get("value");
    if account.is_none() || account.unwrap().is_null() {
        return Err(format!("Mint account not found: {}", mint).into());
    }
    let account = account.unwrap();

    // Parse mint info
    let parsed = account.pointer("/data/parsed/info");
    if let Some(info) = parsed {
        let decimals = info.get("decimals").and_then(|v| v.as_u64()).unwrap_or(0);
        let supply = info.get("supply").and_then(|v| v.as_str()).unwrap_or("0");
        let supply_val: u64 = supply.parse().unwrap_or(0);
        let supply_human = lamports_to_amount(supply_val, decimals as u8);

        let freeze_auth = info.get("freezeAuthority").and_then(|v| v.as_str());
        let mint_auth = info.get("mintAuthority").and_then(|v| v.as_str());

        let is_initialized = info.get("isInitialized").and_then(|v| v.as_bool()).unwrap_or(false);

        // Resolve known symbol
        let symbol = KNOWN_TOKENS.iter()
            .find(|(_, addr, _)| *addr == mint)
            .map(|(sym, _, _)| *sym)
            .unwrap_or("Unknown");

        output.push_str("| Field | Value |\n|-------|-------|\n");
        output.push_str(&format!("| Symbol | {} |\n", symbol));
        output.push_str(&format!("| Decimals | {} |\n", decimals));
        output.push_str(&format!("| Total Supply | {} |\n", supply_human));
        output.push_str(&format!("| Initialized | {} |\n", is_initialized));
        output.push_str(&format!("| Mint Authority | {} |\n",
            mint_auth.map(|a| format!("`{}`", a)).unwrap_or("None (fixed supply)".into())));
        output.push_str(&format!("| Freeze Authority | {} |\n",
            freeze_auth.map(|a| format!("`{}`", a)).unwrap_or("None (unfrozen)".into())));

        // Safety assessment
        let mut warnings = Vec::new();
        if mint_auth.is_some() {
            warnings.push("⚠️ Mint authority is set — new tokens can be minted");
        }
        if freeze_auth.is_some() {
            warnings.push("⚠️ Freeze authority is set — accounts can be frozen");
        }

        if !warnings.is_empty() {
            output.push_str("\n**Safety Notes**:\n");
            for w in warnings {
                output.push_str(&format!("- {}\n", w));
            }
        } else {
            output.push_str("\n✅ No mint or freeze authority — supply is fixed and accounts cannot be frozen.\n");
        }
    } else {
        output.push_str("Could not parse token metadata. This may not be a standard SPL token.\n");
    }

    // Account owner (program)
    let owner = account.get("owner").and_then(|v| v.as_str()).unwrap_or("unknown");
    let program_name = match owner {
        "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA" => "SPL Token Program",
        "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb" => "Token-2022 Program",
        _ => owner,
    };
    output.push_str(&format!("\n**Token Program**: {}\n", program_name));
    output.push_str(&format!("\n🔗 [View on Solscan](https://solscan.io/token/{})\n", mint));

    Ok(output)
}
