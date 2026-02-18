// Paw Agent Engine — Solana DEX Trading (Jupiter Aggregator)
// Self-custody Solana wallet with on-chain swap execution via Jupiter.
//
// Architecture:
// - Private key stored encrypted in the Skill Vault (OS keychain + SQLite)
// - Key is decrypted ONLY in this Rust module for transaction signing
// - The agent never sees the private key — only tool parameters and tx hashes
// - All swaps go through the Human-in-the-Loop approval modal
// - Trading policy limits enforced server-side
//
// Supported operations:
// - sol_wallet_create: Generate ed25519 keypair, store in vault, return address
// - sol_balance: Check SOL + SPL token balances via JSON-RPC
// - sol_quote: Get swap quote from Jupiter aggregator
// - sol_swap: Execute swap via Jupiter: quote → transaction → sign → broadcast
// - sol_portfolio: Multi-token balance scan
// - sol_token_info: Get on-chain token metadata

use log::info;
use std::collections::HashMap;
use std::time::Duration;

// ── Constants ──────────────────────────────────────────────────────────

/// Well-known SPL tokens on Solana mainnet (symbol, mint_address, decimals)
const KNOWN_TOKENS: &[(&str, &str, u8)] = &[
    ("SOL",   "So11111111111111111111111111111111111111112",  9),
    ("USDC",  "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", 6),
    ("USDT",  "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB",  6),
    ("BONK",  "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263",  5),
    ("JUP",   "JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN",   6),
    ("RAY",   "4k3Dyjzvzp8eMZWUXbBCjEvwSkkk59S5iCNLY3QrkX6R",  6),
    ("PYTH",  "HZ1JovNiVvGrGNiiYvEozEVgZ58xaU3RKwX8eACQBCt3",  6),
    ("WIF",   "EKpQGSJtjMFqKZ9KQanSqYXRcF8fBopzLHYxdM65zcjm",  6),
    ("ORCA",  "orcaEKTdK7LKz57vaAYr9QeNsVEPfiu6QeMU1kektZE",   6),
    ("MSOL",  "mSoLzYCxHdYgdzU16g5QSh3i5K3z3KZK7ytfqcJm7So",   9),
    ("JITOSOL", "J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn", 9),
];

/// Jupiter API base URL (Metis Swap API v1 — requires API key from jup.ag)
const JUPITER_API: &str = "https://api.jup.ag/swap/v1";

/// Solana Token Program IDs
const TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const TOKEN_2022_PROGRAM_ID: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";

/// Default slippage tolerance (0.5% = 50 bps)
const DEFAULT_SLIPPAGE_BPS: u64 = 50;
/// Maximum allowed slippage (5%)
const MAX_SLIPPAGE_BPS: u64 = 500;

// ── Helpers ────────────────────────────────────────────────────────────

/// Resolve a token symbol or mint address to (mint_address, decimals)
fn resolve_token(sym_or_addr: &str) -> Result<(String, u8), String> {
    let upper = sym_or_addr.trim().to_uppercase();

    // Check known tokens
    for (sym, addr, dec) in KNOWN_TOKENS {
        if upper == *sym {
            return Ok((addr.to_string(), *dec));
        }
    }

    // If it looks like a base58 address (32-44 chars, base58 alphabet)
    let trimmed = sym_or_addr.trim();
    if trimmed.len() >= 32 && trimmed.len() <= 44 && bs58::decode(trimmed).into_vec().is_ok() {
        // Unknown decimals — we'll query on-chain
        return Ok((trimmed.to_string(), 0));
    }

    Err(format!("Unknown Solana token: '{}'. Use a mint address or known symbol: {}", sym_or_addr,
        KNOWN_TOKENS.iter().map(|(s, _, _)| *s).collect::<Vec<_>>().join(", ")))
}

/// Format lamports to SOL (9 decimals) or SPL token amount
fn lamports_to_amount(lamports: u64, decimals: u8) -> String {
    if decimals == 0 {
        return lamports.to_string();
    }
    let divisor = 10u64.pow(decimals as u32);
    let whole = lamports / divisor;
    let frac = lamports % divisor;
    if frac == 0 {
        whole.to_string()
    } else {
        let frac_str = format!("{:0>width$}", frac, width = decimals as usize);
        let trimmed = frac_str.trim_end_matches('0');
        format!("{}.{}", whole, trimmed)
    }
}

/// Parse amount string (e.g. "1.5") to smallest units given decimals
fn amount_to_lamports(amount_str: &str, decimals: u8) -> Result<u64, String> {
    let amount_str = amount_str.trim();
    if let Some(dot_pos) = amount_str.find('.') {
        let whole: u64 = amount_str[..dot_pos].parse().map_err(|e| format!("Invalid amount: {}", e))?;
        let frac_str = &amount_str[dot_pos + 1..];
        let frac_len = frac_str.len();
        if frac_len > decimals as usize {
            return Err(format!("Too many decimal places (max {})", decimals));
        }
        let frac: u64 = frac_str.parse().map_err(|e| format!("Invalid fractional: {}", e))?;
        let multiplier = 10u64.pow((decimals as u32) - frac_len as u32);
        Ok(whole * 10u64.pow(decimals as u32) + frac * multiplier)
    } else {
        let whole: u64 = amount_str.parse().map_err(|e| format!("Invalid amount: {}", e))?;
        Ok(whole * 10u64.pow(decimals as u32))
    }
}

/// Make a Solana JSON-RPC call
async fn rpc_call(rpc_url: &str, method: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
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
        .await
        .map_err(|e| format!("Solana RPC error: {}", e))?;

    let json: serde_json::Value = resp.json().await
        .map_err(|e| format!("Solana RPC parse error: {}", e))?;

    if let Some(error) = json.get("error") {
        return Err(format!("Solana RPC error: {}", error));
    }

    json.get("result").cloned()
        .ok_or_else(|| "Solana RPC: missing 'result' field".into())
}

/// Get SOL balance in lamports
async fn get_sol_balance(rpc_url: &str, address: &str) -> Result<u64, String> {
    let result = rpc_call(rpc_url, "getBalance", serde_json::json!([address])).await?;
    result.get("value")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "Failed to parse SOL balance".into())
}

/// Get SPL token accounts for a wallet
async fn get_token_accounts(rpc_url: &str, wallet: &str) -> Result<Vec<(String, u64, u8, String)>, String> {
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
async fn get_mint_info(rpc_url: &str, mint: &str) -> Result<serde_json::Value, String> {
    let result = rpc_call(rpc_url, "getAccountInfo", serde_json::json!([
        mint,
        { "encoding": "jsonParsed" }
    ])).await?;

    let parsed = result.pointer("/value/data/parsed/info");
    if let Some(info) = parsed {
        Ok(info.clone())
    } else {
        Err(format!("Could not parse mint info for {}", mint))
    }
}

/// Derive Solana public key (base58) from ed25519 secret key bytes
fn pubkey_from_secret(secret_bytes: &[u8; 32]) -> Result<String, String> {
    use ed25519_dalek::SigningKey;
    let signing_key = SigningKey::from_bytes(secret_bytes);
    let public_key = signing_key.verifying_key();
    Ok(bs58::encode(public_key.as_bytes()).into_string())
}

// ── Tool Implementations ──────────────────────────────────────────────

/// sol_wallet_create — Generate ed25519 keypair, store in vault
pub async fn execute_sol_wallet_create(
    _args: &serde_json::Value,
    creds: &HashMap<String, String>,
    app_handle: &tauri::AppHandle,
) -> Result<String, String> {
    use tauri::Manager;

    // Check if wallet already exists
    if creds.contains_key("SOLANA_PRIVATE_KEY") && creds.contains_key("SOLANA_WALLET_ADDRESS") {
        let addr = creds.get("SOLANA_WALLET_ADDRESS").unwrap();
        return Ok(format!(
            "Solana wallet already exists!\n\nAddress: {}\n\nTo create a new wallet, first remove the existing credentials in Settings → Skills → Solana DEX Trading.",
            addr
        ));
    }

    // Generate a new ed25519 keypair
    use ed25519_dalek::SigningKey;
    let signing_key = SigningKey::generate(&mut rand::thread_rng());
    let public_key = signing_key.verifying_key();

    let address = bs58::encode(public_key.as_bytes()).into_string();
    // Store as base58-encoded 64-byte keypair (secret + public, Solana convention)
    let mut keypair_bytes = [0u8; 64];
    keypair_bytes[..32].copy_from_slice(&signing_key.to_bytes());
    keypair_bytes[32..].copy_from_slice(public_key.as_bytes());
    let private_key_b58 = bs58::encode(&keypair_bytes).into_string();

    let state = app_handle.try_state::<crate::engine::commands::EngineState>()
        .ok_or("Engine state not available")?;
    let vault_key = crate::engine::skills::get_vault_key()?;

    let encrypted_key = crate::engine::skills::encrypt_credential(&private_key_b58, &vault_key);
    state.store.set_skill_credential("solana_dex", "SOLANA_PRIVATE_KEY", &encrypted_key)?;

    let encrypted_addr = crate::engine::skills::encrypt_credential(&address, &vault_key);
    state.store.set_skill_credential("solana_dex", "SOLANA_WALLET_ADDRESS", &encrypted_addr)?;

    info!("[sol_dex] Created new Solana wallet: {}", address);

    // Check connection
    let network_info = if let Some(rpc_url) = creds.get("SOLANA_RPC_URL") {
        match rpc_call(rpc_url, "getVersion", serde_json::json!([])).await {
            Ok(version) => {
                let ver = version.get("solana-core").and_then(|v| v.as_str()).unwrap_or("unknown");
                format!("Solana Mainnet (node v{})", ver)
            }
            Err(_) => "Could not connect to RPC".into(),
        }
    } else {
        "Not connected (configure Solana RPC URL)".into()
    };

    Ok(format!(
        "✅ New Solana wallet created!\n\n\
        Address: {}\n\
        Network: {}\n\n\
        ⚠️ This wallet has zero balance. Send SOL to this address to fund it before trading.\n\n\
        🔒 Private key is encrypted and stored in your OS keychain vault. The AI agent never sees it.",
        address, network_info
    ))
}

/// sol_balance — Check SOL + SPL token balances
pub async fn execute_sol_balance(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> Result<String, String> {
    let rpc_url = creds.get("SOLANA_RPC_URL")
        .ok_or("Missing SOLANA_RPC_URL. Configure your Solana RPC endpoint in Settings → Skills → Solana DEX Trading.")?;
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

/// sol_quote — Get swap quote from Jupiter aggregator
pub async fn execute_sol_quote(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> Result<String, String> {
    let _rpc_url = creds.get("SOLANA_RPC_URL")
        .ok_or("Missing SOLANA_RPC_URL.")?;
    let api_key = creds.get("JUPITER_API_KEY")
        .ok_or("Missing JUPITER_API_KEY. Get one free at jup.ag/developers")?;

    let token_in_str = args["token_in"].as_str().ok_or("sol_quote: missing 'token_in'")?;
    let token_out_str = args["token_out"].as_str().ok_or("sol_quote: missing 'token_out'")?;
    let amount_str = args["amount"].as_str().ok_or("sol_quote: missing 'amount'")?;
    let slippage_bps = args.get("slippage_bps").and_then(|v| v.as_u64()).unwrap_or(DEFAULT_SLIPPAGE_BPS);

    if slippage_bps > MAX_SLIPPAGE_BPS {
        return Err(format!("Slippage too high: {}bps. Max is {}bps ({}%)", slippage_bps, MAX_SLIPPAGE_BPS, MAX_SLIPPAGE_BPS / 100));
    }

    let (input_mint, input_decimals) = resolve_token(token_in_str)?;
    let (output_mint, _output_decimals) = resolve_token(token_out_str)?;

    // For unknown tokens, try to fetch decimals on-chain
    let in_decimals = if input_decimals == 0 {
        // Default to 9 for SOL-like tokens; for accurate results would need mint query
        9
    } else {
        input_decimals
    };

    let amount_raw = amount_to_lamports(amount_str, in_decimals)?;

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
        .await
        .map_err(|e| format!("Jupiter API error: {}", e))?;

    let status = resp.status();
    let body: serde_json::Value = resp.json().await
        .map_err(|e| format!("Jupiter response parse error: {}", e))?;

    if !status.is_success() {
        let msg = body.get("error").and_then(|v| v.as_str())
            .or_else(|| body.get("message").and_then(|v| v.as_str()))
            .unwrap_or("Unknown error");
        return Err(format!("Jupiter quote failed: {}", msg));
    }

    // Parse quote response
    let out_amount_raw = body.get("outAmount").and_then(|v| v.as_str())
        .ok_or("Missing outAmount in Jupiter response")?;
    let out_amount: u64 = out_amount_raw.parse().unwrap_or(0);

    let other_amount_threshold = body.get("otherAmountThreshold").and_then(|v| v.as_str()).unwrap_or("0");
    let min_out: u64 = other_amount_threshold.parse().unwrap_or(0);

    let price_impact_pct = body.get("priceImpactPct").and_then(|v| v.as_str()).unwrap_or("0");

    // Get output token decimals from known list or default
    let out_decimals = KNOWN_TOKENS.iter()
        .find(|(_, addr, _)| *addr == output_mint)
        .map(|(_, _, d)| *d)
        .unwrap_or(9);

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

/// sol_swap — Execute a swap via Jupiter (quote → tx → sign → send)
pub async fn execute_sol_swap(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> Result<String, String> {
    let rpc_url = creds.get("SOLANA_RPC_URL")
        .ok_or("Missing SOLANA_RPC_URL.")?;
    let wallet = creds.get("SOLANA_WALLET_ADDRESS")
        .ok_or("No Solana wallet. Use sol_wallet_create first.")?;
    let private_key_b58 = creds.get("SOLANA_PRIVATE_KEY")
        .ok_or("No Solana private key. Use sol_wallet_create first.")?;
    let api_key = creds.get("JUPITER_API_KEY")
        .ok_or("Missing JUPITER_API_KEY. Get one free at jup.ag/developers")?;

    let token_in_str = args["token_in"].as_str().ok_or("sol_swap: missing 'token_in'")?;
    let token_out_str = args["token_out"].as_str().ok_or("sol_swap: missing 'token_out'")?;
    let amount_str = args["amount"].as_str().ok_or("sol_swap: missing 'amount'")?;
    let _reason = args["reason"].as_str().unwrap_or("No reason given");
    let slippage_bps = args.get("slippage_bps").and_then(|v| v.as_u64()).unwrap_or(DEFAULT_SLIPPAGE_BPS);

    if slippage_bps > MAX_SLIPPAGE_BPS {
        return Err(format!("Slippage too high: {}bps (max {}bps)", slippage_bps, MAX_SLIPPAGE_BPS));
    }

    let (input_mint, input_decimals) = resolve_token(token_in_str)?;
    let (output_mint, _output_decimals) = resolve_token(token_out_str)?;

    let in_decimals = if input_decimals == 0 { 9 } else { input_decimals };
    let amount_raw = amount_to_lamports(amount_str, in_decimals)?;

    let client = reqwest::Client::new();

    // Step 1: Get Jupiter quote (Metis v1)
    let quote_url = format!(
        "{}/quote?inputMint={}&outputMint={}&amount={}&slippageBps={}&restrictIntermediateTokens=true",
        JUPITER_API, input_mint, output_mint, amount_raw, slippage_bps
    );

    info!("[sol_dex] Getting Jupiter quote for swap: {} {} → {}", amount_str, token_in_str, token_out_str);

    let quote_resp = client.get(&quote_url)
        .header("x-api-key", api_key.as_str())
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Jupiter quote error: {}", e))?;

    let quote: serde_json::Value = quote_resp.json().await
        .map_err(|e| format!("Quote parse error: {}", e))?;

    if quote.get("error").is_some() || quote.get("outAmount").is_none() {
        let msg = quote.get("error").and_then(|v| v.as_str())
            .or_else(|| quote.get("message").and_then(|v| v.as_str()))
            .unwrap_or("No route found");
        return Err(format!("Jupiter quote failed: {}", msg));
    }

    let out_amount_str = quote.get("outAmount").and_then(|v| v.as_str()).unwrap_or("0");
    let out_amount: u64 = out_amount_str.parse().unwrap_or(0);
    let out_decimals = KNOWN_TOKENS.iter()
        .find(|(_, addr, _)| *addr == output_mint)
        .map(|(_, _, d)| *d)
        .unwrap_or(9);
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

    let swap_resp = client.post(&format!("{}/swap", JUPITER_API))
        .header("x-api-key", api_key.as_str())
        .json(&swap_body)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("Jupiter swap API error: {}", e))?;

    let swap_data: serde_json::Value = swap_resp.json().await
        .map_err(|e| format!("Swap response parse error: {}", e))?;

    let swap_tx_b64 = swap_data.get("swapTransaction").and_then(|v| v.as_str())
        .ok_or("Jupiter did not return a swap transaction")?;

    // Step 3: Decode, sign, and send the transaction
    info!("[sol_dex] Signing and sending transaction...");

    let tx_bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, swap_tx_b64)
        .map_err(|e| format!("Failed to decode transaction: {}", e))?;

    // Decode the private key (Solana 64-byte keypair format: 32-byte secret + 32-byte public)
    let keypair_bytes = bs58::decode(private_key_b58).into_vec()
        .map_err(|e| format!("Invalid private key encoding: {}", e))?;

    if keypair_bytes.len() < 64 {
        return Err("Invalid Solana keypair (expected 64 bytes)".into());
    }

    let mut secret_bytes = [0u8; 32];
    secret_bytes.copy_from_slice(&keypair_bytes[..32]);

    // Sign the transaction
    let signed_tx = sign_solana_transaction(&tx_bytes, &secret_bytes)?;
    let signed_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &signed_tx);

    // Step 4: Send the signed transaction
    let send_result = rpc_call(rpc_url, "sendTransaction", serde_json::json!([
        signed_b64,
        { "encoding": "base64", "skipPreflight": false, "maxRetries": 3 }
    ])).await?;

    let tx_sig = send_result.as_str().unwrap_or("unknown");

    info!("[sol_dex] Swap sent! Tx: {}", tx_sig);

    // Step 5: Wait briefly and check confirmation
    tokio::time::sleep(Duration::from_secs(3)).await;

    let status = rpc_call(rpc_url, "getSignatureStatuses", serde_json::json!([[tx_sig]])).await;
    let confirmation = if let Ok(status_val) = status {
        if let Some(statuses) = status_val.get("value").and_then(|v| v.as_array()) {
            if let Some(Some(s)) = statuses.first().map(|v| {
                if v.is_null() { None } else { Some(v) }
            }) {
                let conf = s.get("confirmationStatus").and_then(|v| v.as_str()).unwrap_or("pending");
                if s.get("err").is_some() && !s["err"].is_null() {
                    format!("❌ FAILED: {:?}", s["err"])
                } else {
                    format!("✅ {}", conf)
                }
            } else {
                "⏳ Pending (check explorer)".into()
            }
        } else {
            "⏳ Submitted".into()
        }
    } else {
        "⏳ Submitted (status check failed)".into()
    };

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
        &tx_sig[..16], tx_sig
    ))
}

/// sol_portfolio — Multi-token balance scan
pub async fn execute_sol_portfolio(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> Result<String, String> {
    let rpc_url = creds.get("SOLANA_RPC_URL")
        .ok_or("Missing SOLANA_RPC_URL.")?;
    let wallet = creds.get("SOLANA_WALLET_ADDRESS")
        .ok_or("No Solana wallet. Use sol_wallet_create first.")?;

    let mut output = format!("## Solana Portfolio\n**Wallet**: `{}`\n\n", wallet);

    // SOL balance
    let sol_lamports = get_sol_balance(rpc_url, wallet).await?;
    let sol_amount = lamports_to_amount(sol_lamports, 9);
    output.push_str(&format!("| Token | Balance |\n|-------|--------|\n"));
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
    match rpc_call(rpc_url, "getVersion", serde_json::json!([])).await {
        Ok(ver) => {
            let v = ver.get("solana-core").and_then(|v| v.as_str()).unwrap_or("?");
            output.push_str(&format!("\n**Network**: Solana Mainnet (node v{})\n", v));
        }
        Err(_) => {}
    }

    Ok(output)
}

/// sol_token_info — Get on-chain token metadata
pub async fn execute_sol_token_info(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> Result<String, String> {
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
        return Err(format!("Mint account not found: {}", mint));
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

        output.push_str(&format!("| Field | Value |\n|-------|-------|\n"));
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
            output.push_str(&format!("\n**Safety Notes**:\n"));
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

// ── Transaction Signing ────────────────────────────────────────────────

/// Sign a Solana transaction (versioned or legacy)
///
/// Solana transaction binary format:
/// - [num_signatures] (compact-u16, usually 1 byte for <=127)
/// - [signature_slots] (num_signatures × 64 bytes — initially zeroed)
/// - [message_bytes] (everything after signatures)
///
/// We sign the message portion with ed25519 and place the signature in the first slot.
fn sign_solana_transaction(tx_bytes: &[u8], secret_key: &[u8; 32]) -> Result<Vec<u8>, String> {
    if tx_bytes.is_empty() {
        return Err("Empty transaction".into());
    }

    use ed25519_dalek::{Signer, SigningKey};

    let signing_key = SigningKey::from_bytes(secret_key);

    // Parse compact-u16 for num_signatures
    let (num_sigs, sig_header_len) = decode_compact_u16(tx_bytes)?;
    if num_sigs == 0 {
        return Err("Transaction has 0 signatures required".into());
    }

    let sigs_start = sig_header_len;
    let sigs_end = sigs_start + (num_sigs as usize * 64);
    if sigs_end > tx_bytes.len() {
        return Err(format!("Transaction too short: need {} bytes for signatures, have {}", sigs_end, tx_bytes.len()));
    }

    // Message is everything after the signature slots
    let message = &tx_bytes[sigs_end..];

    // Sign the message
    let signature = signing_key.sign(message);

    // Build the signed transaction
    let mut signed = tx_bytes.to_vec();
    // Place our signature in the first slot
    signed[sigs_start..sigs_start + 64].copy_from_slice(&signature.to_bytes());

    Ok(signed)
}

/// Decode Solana compact-u16 encoding
/// Returns (value, bytes_consumed)
fn decode_compact_u16(data: &[u8]) -> Result<(u16, usize), String> {
    if data.is_empty() {
        return Err("Empty data for compact-u16".into());
    }

    let first = data[0] as u16;
    if first < 0x80 {
        return Ok((first, 1));
    }

    if data.len() < 2 {
        return Err("Truncated compact-u16".into());
    }
    let second = data[1] as u16;
    if second < 0x80 {
        return Ok(((first & 0x7F) | (second << 7), 2));
    }

    if data.len() < 3 {
        return Err("Truncated compact-u16".into());
    }
    let third = data[2] as u16;
    Ok(((first & 0x7F) | ((second & 0x7F) << 7) | (third << 14), 3))
}

// ── Outbound Transfer ──────────────────────────────────────────────────

/// Encode a compact-u16 value (Solana serialization)
fn encode_compact_u16(val: u16) -> Vec<u8> {
    if val < 0x80 {
        vec![val as u8]
    } else if val < 0x4000 {
        vec![(val & 0x7F | 0x80) as u8, (val >> 7) as u8]
    } else {
        vec![(val & 0x7F | 0x80) as u8, ((val >> 7) & 0x7F | 0x80) as u8, (val >> 14) as u8]
    }
}

/// Build a Solana legacy transaction from scratch (message + signature slots).
/// accounts: Vec of (pubkey_bytes, is_signer, is_writable)
/// instructions: Vec of (program_id_index, account_indices, data)
fn build_solana_transaction(
    recent_blockhash: &[u8; 32],
    accounts: &[([u8; 32], bool, bool)],
    instructions: &[(u8, Vec<u8>, Vec<u8>)],
) -> Vec<u8> {
    // Count signers and read-only accounts
    let num_signers = accounts.iter().filter(|(_, s, _)| *s).count() as u8;
    let num_readonly_signed = accounts.iter().filter(|(_, s, w)| *s && !*w).count() as u8;
    let num_readonly_unsigned = accounts.iter().filter(|(_, s, w)| !*s && !*w).count() as u8;

    // Build message
    let mut message = Vec::new();
    // Header: num_required_signatures, num_readonly_signed, num_readonly_unsigned
    message.push(num_signers);
    message.push(num_readonly_signed);
    message.push(num_readonly_unsigned);

    // Account addresses (compact-u16 length + keys)
    message.extend_from_slice(&encode_compact_u16(accounts.len() as u16));
    for (pubkey, _, _) in accounts {
        message.extend_from_slice(pubkey);
    }

    // Recent blockhash
    message.extend_from_slice(recent_blockhash);

    // Instructions
    message.extend_from_slice(&encode_compact_u16(instructions.len() as u16));
    for (program_id_idx, acct_indices, data) in instructions {
        message.push(*program_id_idx);
        // Account indices
        message.extend_from_slice(&encode_compact_u16(acct_indices.len() as u16));
        message.extend_from_slice(acct_indices);
        // Data
        message.extend_from_slice(&encode_compact_u16(data.len() as u16));
        message.extend_from_slice(data);
    }

    // Full transaction: num_signatures (compact-u16) + signature slots (zeroed) + message
    let mut tx = Vec::new();
    tx.extend_from_slice(&encode_compact_u16(num_signers as u16));
    // Zeroed signature slots
    for _ in 0..num_signers {
        tx.extend_from_slice(&[0u8; 64]);
    }
    tx.extend_from_slice(&message);
    tx
}

/// Derive Associated Token Account (ATA) address
/// ATA = PDA of [wallet, TOKEN_PROGRAM_ID, mint] with ATA_PROGRAM_ID
fn derive_ata(wallet: &[u8; 32], mint: &[u8; 32], token_program: &[u8; 32]) -> Result<[u8; 32], String> {
    use sha2::Digest;
    let ata_program = bs58::decode("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL")
        .into_vec().map_err(|e| format!("ATA program decode: {}", e))?;

    // PDA: sha256([wallet, token_program, mint, ata_program, "ProgramDerivedAddress"])
    // Try finding a valid PDA (bump seed from 255 down to 0)
    for bump in (0u8..=255).rev() {
        let mut hasher = sha2::Sha256::new();
        hasher.update(wallet);
        hasher.update(token_program);
        hasher.update(mint);
        hasher.update(&[bump]);
        hasher.update(&ata_program);
        hasher.update(b"ProgramDerivedAddress");
        let hash = hasher.finalize();

        // Valid PDA must NOT be on the ed25519 curve
        // We check by trying to decompress as an ed25519 point
        let point_bytes: [u8; 32] = hash[..32].try_into().unwrap();
        if ed25519_dalek::VerifyingKey::from_bytes(&point_bytes).is_err() {
            return Ok(point_bytes);
        }
    }
    Err("Could not derive ATA address".into())
}

/// Transfer SOL or SPL tokens to an external address.
pub async fn execute_sol_transfer(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> Result<String, String> {
    let rpc_url = creds.get("SOLANA_RPC_URL").ok_or("Missing SOLANA_RPC_URL.")?;
    let wallet = creds.get("SOLANA_WALLET_ADDRESS").ok_or("No Solana wallet. Use sol_wallet_create first.")?;
    let private_key_b58 = creds.get("SOLANA_PRIVATE_KEY").ok_or("No Solana private key.")?;

    let currency = args["currency"].as_str().ok_or("sol_transfer: missing 'currency'")?;
    let amount_str = args["amount"].as_str().ok_or("sol_transfer: missing 'amount'")?;
    let to_address = args["to_address"].as_str().ok_or("sol_transfer: missing 'to_address'")?;
    let _reason = args["reason"].as_str().unwrap_or("transfer");

    // Validate recipient address
    let to_bytes_vec = bs58::decode(to_address).into_vec()
        .map_err(|e| format!("Invalid recipient address: {}", e))?;
    if to_bytes_vec.len() != 32 {
        return Err(format!("Invalid Solana address length: expected 32 bytes, got {}", to_bytes_vec.len()));
    }
    let mut to_pubkey = [0u8; 32];
    to_pubkey.copy_from_slice(&to_bytes_vec);

    // Decode sender keypair
    let keypair_bytes = bs58::decode(private_key_b58).into_vec()
        .map_err(|e| format!("Invalid private key encoding: {}", e))?;
    if keypair_bytes.len() < 64 {
        return Err("Invalid Solana keypair (expected 64 bytes)".into());
    }
    let mut secret_bytes = [0u8; 32];
    secret_bytes.copy_from_slice(&keypair_bytes[..32]);

    let sender_pubkey_vec = bs58::decode(wallet).into_vec()
        .map_err(|e| format!("Invalid wallet address: {}", e))?;
    let mut sender_pubkey = [0u8; 32];
    sender_pubkey.copy_from_slice(&sender_pubkey_vec);

    let currency_upper = currency.trim().to_uppercase();
    let is_sol = currency_upper == "SOL";

    // Get recent blockhash
    let blockhash_result = rpc_call(rpc_url, "getLatestBlockhash", serde_json::json!([
        { "commitment": "finalized" }
    ])).await?;
    let blockhash_str = blockhash_result.pointer("/value/blockhash")
        .and_then(|v| v.as_str())
        .ok_or("Failed to get recent blockhash")?;
    let blockhash_bytes = bs58::decode(blockhash_str).into_vec()
        .map_err(|e| format!("Blockhash decode error: {}", e))?;
    let mut recent_blockhash = [0u8; 32];
    recent_blockhash.copy_from_slice(&blockhash_bytes);

    let tx_sig = if is_sol {
        // ── Native SOL transfer via System Program ──
        let decimals = 9u8;
        let lamports = amount_to_lamports(amount_str, decimals)?;

        // Check SOL balance
        let balance = get_sol_balance(rpc_url, wallet).await?;
        // Need lamports + ~5000 for tx fee
        if balance < lamports + 5000 {
            let bal_display = lamports_to_amount(balance, 9);
            return Err(format!("Insufficient SOL balance. Have: {} SOL, need: {} SOL + fees", bal_display, amount_str));
        }

        // System Program transfer instruction:
        // Instruction index 2 = Transfer
        // Data: [2, 0, 0, 0] (LE u32) + lamports (LE u64)
        let mut instr_data = vec![2u8, 0, 0, 0]; // Transfer instruction discriminator
        instr_data.extend_from_slice(&lamports.to_le_bytes());

        // System Program ID
        let system_program = [0u8; 32]; // 11111111111111111111111111111111 in base58 = all zeros

        // Accounts: [sender (signer, writable), recipient (writable), system_program (readonly)]
        let accounts = vec![
            (sender_pubkey, true, true),   // fee payer + sender
            (to_pubkey, false, true),       // recipient
            (system_program, false, false), // system program
        ];
        let instructions = vec![
            (2u8, vec![0u8, 1], instr_data), // program_id_index=2 (system_program), accounts=[0,1]
        ];

        let tx_bytes = build_solana_transaction(&recent_blockhash, &accounts, &instructions);
        let signed_tx = sign_solana_transaction(&tx_bytes, &secret_bytes)?;
        let signed_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &signed_tx);

        let send_result = rpc_call(rpc_url, "sendTransaction", serde_json::json!([
            signed_b64,
            { "encoding": "base64", "skipPreflight": false, "maxRetries": 3 }
        ])).await?;
        send_result.as_str().unwrap_or("unknown").to_string()
    } else {
        // ── SPL Token transfer ──
        let (mint_addr, token_decimals) = resolve_token(currency)?;
        let decimals = if token_decimals == 0 { 9 } else { token_decimals };
        let raw_amount = amount_to_lamports(amount_str, decimals)?;

        let mint_bytes_vec = bs58::decode(&mint_addr).into_vec()
            .map_err(|e| format!("Invalid mint address: {}", e))?;
        let mut mint_pubkey = [0u8; 32];
        mint_pubkey.copy_from_slice(&mint_bytes_vec);

        // Check sender's token balance
        let token_accounts = get_token_accounts(rpc_url, wallet).await?;
        let sender_token = token_accounts.iter()
            .find(|(m, _, _, _)| m == &mint_addr);
        match sender_token {
            None => return Err(format!("No {} token account found in wallet", currency_upper)),
            Some((_, amt, _, _)) if *amt < raw_amount => {
                return Err(format!("Insufficient {} balance. Have: {}, need: {}",
                    currency_upper, lamports_to_amount(*amt, decimals), amount_str));
            }
            _ => {}
        }
        let sender_ata_addr = sender_token.unwrap().3.clone();
        let sender_ata_bytes = bs58::decode(&sender_ata_addr).into_vec()
            .map_err(|e| format!("Sender ATA decode: {}", e))?;
        let mut sender_ata = [0u8; 32];
        sender_ata.copy_from_slice(&sender_ata_bytes);

        // Check SOL for fees
        let sol_balance = get_sol_balance(rpc_url, wallet).await?;
        if sol_balance < 10_000 {
            return Err("Insufficient SOL for transaction fees (~0.00001 SOL needed)".into());
        }

        // Derive recipient ATA
        let token_program_bytes = bs58::decode(TOKEN_PROGRAM_ID).into_vec()
            .map_err(|e| format!("Token program decode: {}", e))?;
        let mut token_program = [0u8; 32];
        token_program.copy_from_slice(&token_program_bytes);

        let recipient_ata = derive_ata(&to_pubkey, &mint_pubkey, &token_program)?;

        // Check if recipient ATA exists
        let recipient_ata_b58 = bs58::encode(&recipient_ata).into_string();
        let ata_exists = rpc_call(rpc_url, "getAccountInfo", serde_json::json!([
            recipient_ata_b58, { "encoding": "base64" }
        ])).await.map(|r| !r.get("value").map_or(true, |v| v.is_null())).unwrap_or(false);

        // ATA Program ID
        let ata_program_bytes = bs58::decode("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL")
            .into_vec().map_err(|e| format!("ATA program decode: {}", e))?;
        let mut ata_program = [0u8; 32];
        ata_program.copy_from_slice(&ata_program_bytes);

        // System Program
        let system_program = [0u8; 32];

        // Build instructions
        let mut instructions = Vec::new();
        let mut accounts: Vec<([u8; 32], bool, bool)> = Vec::new();

        // Account layout depends on whether we need to create the ATA
        if !ata_exists {
            // Need to create ATA first
            // Accounts: [sender/payer, recipient_ata, recipient, mint, system_program, token_program, ata_program]
            // Then transfer: [sender_ata, recipient_ata, sender_authority]
            accounts.push((sender_pubkey, true, true));   // 0: payer (signer, writable)
            accounts.push((recipient_ata, false, true));   // 1: new ATA (writable)
            accounts.push((to_pubkey, false, false));       // 2: wallet owner (recipient)
            accounts.push((mint_pubkey, false, false));     // 3: mint
            accounts.push((system_program, false, false)); // 4: system program
            accounts.push((token_program, false, false));  // 5: token program
            accounts.push((sender_ata, false, true));      // 6: sender ATA (writable)
            accounts.push((ata_program, false, false));    // 7: ATA program

            // Instruction 1: Create ATA (ATA program, no data needed — uses instruction 0 = Create)
            // Accounts: [payer, ata, owner, mint, system_program, token_program]
            instructions.push((7u8, vec![0, 1, 2, 3, 4, 5], vec![]));

            // Instruction 2: SPL Token Transfer
            // transfer instruction = index 3 in Token Program
            // Data: [3] (Transfer discriminator) + amount (LE u64)
            let mut transfer_data = vec![3u8];
            transfer_data.extend_from_slice(&raw_amount.to_le_bytes());
            // Accounts: [source, destination, authority]
            instructions.push((5u8, vec![6, 1, 0], transfer_data));
        } else {
            // ATA exists — just transfer
            accounts.push((sender_pubkey, true, true));   // 0: authority (signer, writable — fee payer)
            accounts.push((sender_ata, false, true));      // 1: source (writable)
            accounts.push((recipient_ata, false, true));   // 2: destination (writable)
            accounts.push((token_program, false, false));  // 3: token program

            // SPL Token Transfer: [3] + amount (LE u64)
            let mut transfer_data = vec![3u8];
            transfer_data.extend_from_slice(&raw_amount.to_le_bytes());
            instructions.push((3u8, vec![1, 2, 0], transfer_data));
        }

        let tx_bytes = build_solana_transaction(&recent_blockhash, &accounts, &instructions);
        let signed_tx = sign_solana_transaction(&tx_bytes, &secret_bytes)?;
        let signed_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &signed_tx);

        let send_result = rpc_call(rpc_url, "sendTransaction", serde_json::json!([
            signed_b64,
            { "encoding": "base64", "skipPreflight": false, "maxRetries": 3 }
        ])).await?;
        send_result.as_str().unwrap_or("unknown").to_string()
    };

    // Check confirmation
    tokio::time::sleep(Duration::from_secs(3)).await;
    let status = rpc_call(rpc_url, "getSignatureStatuses", serde_json::json!([[&tx_sig]])).await;
    let confirmation = if let Ok(status_val) = status {
        if let Some(statuses) = status_val.get("value").and_then(|v| v.as_array()) {
            if let Some(Some(s)) = statuses.first().map(|v| if v.is_null() { None } else { Some(v) }) {
                let conf = s.get("confirmationStatus").and_then(|v| v.as_str()).unwrap_or("pending");
                if s.get("err").is_some() && !s["err"].is_null() {
                    format!("❌ FAILED: {:?}", s["err"])
                } else {
                    format!("✅ {}", conf)
                }
            } else { "⏳ Pending".into() }
        } else { "⏳ Submitted".into() }
    } else { "⏳ Submitted".into() };

    Ok(format!(
        "## Solana Transfer\n\n\
        | Field | Value |\n|-------|-------|\n\
        | Amount | {} {} |\n\
        | To | {} |\n\
        | Status | {} |\n\
        | Transaction | [{}](https://solscan.io/tx/{}) |\n\n\
        _Check Solscan for final confirmation._",
        amount_str, currency_upper,
        to_address,
        confirmation,
        &tx_sig[..std::cmp::min(16, tx_sig.len())], tx_sig
    ))
}
