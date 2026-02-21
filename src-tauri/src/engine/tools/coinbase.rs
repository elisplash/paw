// Paw Agent Engine — Coinbase CDP tools
// Includes full CDP JWT signing (Ed25519 raw/PEM, ES256) and all coinbase_* executors.

use crate::atoms::types::*;
use crate::engine::state::EngineState;
use crate::atoms::error::{EngineResult, EngineError};
use log::{info, warn};
use std::time::Duration;
use tauri::Manager;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "coinbase_prices".into(),
                description: "Get current spot prices for one or more crypto assets from Coinbase. Returns USD prices. Credentials are auto-injected — just call this tool directly.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "symbols": { "type": "string", "description": "Comma-separated crypto symbols (e.g. 'BTC,ETH,SOL'). Use standard ticker symbols." }
                    },
                    "required": ["symbols"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "coinbase_balance".into(),
                description: "Check wallet/account balances on Coinbase. Returns all non-zero balances with USD values. Credentials are auto-injected — just call this tool directly.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "currency": { "type": "string", "description": "Optional: filter to a specific currency (e.g. 'BTC'). Omit to see all balances." }
                    }
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "coinbase_wallet_create".into(),
                description: "Create a new Coinbase wallet. This creates an MPC-secured wallet on the Coinbase platform.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Human-readable name for the wallet (e.g. 'Trading Wallet', 'Savings')" }
                    },
                    "required": ["name"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "coinbase_trade".into(),
                description: "Execute a crypto trade on Coinbase. REQUIRES USER APPROVAL. Always explain your reasoning and include risk parameters before calling this.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "side": { "type": "string", "enum": ["buy", "sell"], "description": "Trade direction: 'buy' or 'sell'" },
                        "product_id": { "type": "string", "description": "Trading pair (e.g. 'BTC-USD', 'ETH-USD', 'SOL-USD')" },
                        "amount": { "type": "string", "description": "Amount in quote currency for buys (e.g. '100' for $100 of BTC) or base currency for sells (e.g. '0.5' for 0.5 BTC)" },
                        "order_type": { "type": "string", "enum": ["market", "limit"], "description": "Order type: 'market' (immediate) or 'limit' (at specific price). Default: market" },
                        "limit_price": { "type": "string", "description": "Limit price (required if order_type is 'limit')" },
                        "reason": { "type": "string", "description": "Your analysis and reasoning for this trade. This is shown to the user for approval." }
                    },
                    "required": ["side", "product_id", "amount", "reason"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "coinbase_transfer".into(),
                description: "Send crypto from your Coinbase account to an external address. REQUIRES USER APPROVAL. Double-check addresses — crypto transfers are irreversible.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "currency": { "type": "string", "description": "Currency to send (e.g. 'BTC', 'ETH', 'USDC')" },
                        "amount": { "type": "string", "description": "Amount to send (e.g. '0.01')" },
                        "to_address": { "type": "string", "description": "Destination wallet address" },
                        "network": { "type": "string", "description": "Network to send on (e.g. 'base', 'ethereum', 'bitcoin'). Default: native network for the currency." },
                        "reason": { "type": "string", "description": "Reason for this transfer" }
                    },
                    "required": ["currency", "amount", "to_address", "reason"]
                }),
            },
        },
    ]
}

pub async fn execute(
    name: &str,
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> Option<Result<String, String>> {
    let creds = match super::get_skill_creds("coinbase", app_handle) {
        Ok(c) => c,
        Err(e) => return Some(Err(e.to_string())),
    };
    let state = app_handle.state::<EngineState>();
    Some(match name {
        "coinbase_prices"        => execute_coinbase_prices(args, &creds).await.map_err(|e| e.to_string()),
        "coinbase_balance"       => execute_coinbase_balance(args, &creds).await.map_err(|e| e.to_string()),
        "coinbase_wallet_create" => execute_coinbase_wallet_create(args, &creds).await.map_err(|e| e.to_string()),
        "coinbase_trade" => {
            let result = execute_coinbase_trade(args, &creds).await;
            if result.is_ok() {
                let _ = state.store.insert_trade(
                    "trade", args["side"].as_str(), args["product_id"].as_str(),
                    None, args["amount"].as_str().unwrap_or("0"),
                    args["order_type"].as_str(), None, "completed",
                    args["amount"].as_str(), None,
                    args["reason"].as_str().unwrap_or(""),
                    None, None, result.as_ref().ok().map(|s| s.as_str()),
                );
            }
            result.map_err(|e| e.to_string())
        }
        "coinbase_transfer" => {
            let result = execute_coinbase_transfer(args, &creds).await;
            if result.is_ok() {
                let _ = state.store.insert_trade(
                    "transfer", Some("send"), None,
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

// ══════════════════════════════════════════════════════════════════════════
// ══ CDP JWT Signing Helpers ═══════════════════════════════════════════════
// ══════════════════════════════════════════════════════════════════════════

/// Build a JWT for Coinbase CDP API authentication.
/// Auto-detects key format: raw base64 Ed25519, PEM Ed25519, or PEM ES256.
fn build_cdp_jwt(
    key_name: &str,
    key_secret: &str,
    method: &str,
    host: &str,
    path: &str,
) -> EngineResult<String> {
    use base64::Engine as _;

    let now = chrono::Utc::now().timestamp() as u64;
    let nonce: String = {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let bytes: Vec<u8> = (0..32).map(|_| rng.gen::<u8>()).collect();
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    };
    let uri = format!("{} {}{}", method, host, path);
    let secret_clean = key_secret
        .replace("\\n", "\n")
        .replace("\\\\n", "\n")
        .trim()
        .to_string();

    let key_type = detect_key_type(&secret_clean);
    let alg = match key_type {
        KeyType::Ed25519Pem | KeyType::Ed25519Raw => "EdDSA",
        KeyType::Es256Pem => "ES256",
    };

    info!("[skill:coinbase] JWT: alg={}, key_type={:?}, uri={}", alg, key_type, uri);

    let header = serde_json::json!({
        "alg": alg,
        "kid": key_name,
        "nonce": nonce,
        "typ": "JWT"
    });

    let payload = serde_json::json!({
        "sub": key_name,
        "iss": "cdp",
        "nbf": now,
        "exp": now + 120,
        "uri": uri
    });

    let b64_header = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_string(&header)?);
    let b64_payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_string(&payload)?);

    let signing_input = format!("{}.{}", b64_header, b64_payload);

    let b64_sig = match key_type {
        KeyType::Ed25519Raw => sign_ed25519_raw(&secret_clean, signing_input.as_bytes())?,
        KeyType::Ed25519Pem => sign_ed25519_pem(&secret_clean, signing_input.as_bytes())?,
        KeyType::Es256Pem   => sign_es256(&secret_clean, signing_input.as_bytes())?,
    };

    Ok(format!("{}.{}", signing_input, b64_sig))
}

#[derive(Debug)]
enum KeyType {
    Ed25519Raw,
    Ed25519Pem,
    Es256Pem,
}

fn detect_key_type(secret: &str) -> KeyType {
    if secret.contains("-----BEGIN") {
        if secret.contains("BEGIN EC PRIVATE KEY") {
            return KeyType::Es256Pem;
        }
        use ed25519_dalek::pkcs8::DecodePrivateKey;
        if ed25519_dalek::SigningKey::from_pkcs8_pem(secret).is_ok() {
            return KeyType::Ed25519Pem;
        }
        return KeyType::Es256Pem;
    }
    KeyType::Ed25519Raw
}

fn sign_ed25519_raw(secret_b64: &str, message: &[u8]) -> EngineResult<String> {
    use ed25519_dalek::Signer;
    use base64::Engine as _;

    let key_bytes = base64::engine::general_purpose::STANDARD
        .decode(secret_b64.trim())
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(secret_b64.trim()))
        .map_err(|e| EngineError::Other(e.to_string()))?;

    info!("[skill:coinbase] Ed25519 raw key decoded to {} bytes", key_bytes.len());

    let signing_key = match key_bytes.len() {
        32 => {
            let mut seed = [0u8; 32];
            seed.copy_from_slice(&key_bytes);
            ed25519_dalek::SigningKey::from_bytes(&seed)
        }
        64 => {
            let mut keypair = [0u8; 64];
            keypair.copy_from_slice(&key_bytes);
            ed25519_dalek::SigningKey::from_keypair_bytes(&keypair)
                .map_err(|e| EngineError::Other(e.to_string()))?
        }
        n => {
            return Err(format!(
                "API secret decoded to {} bytes, expected 32 (seed) or 64 (keypair) for Ed25519. \
                 If your secret starts with '-----BEGIN', paste the entire PEM block including headers.",
                n
            ).into());
        }
    };

    let signature = signing_key.sign(message);
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature.to_bytes()))
}

fn sign_ed25519_pem(pem: &str, message: &[u8]) -> EngineResult<String> {
    use ed25519_dalek::pkcs8::DecodePrivateKey;
    use ed25519_dalek::Signer;
    use base64::Engine as _;

    let signing_key = ed25519_dalek::SigningKey::from_pkcs8_pem(pem)
        .map_err(|e| EngineError::Other(e.to_string()))?;
    let signature = signing_key.sign(message);
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature.to_bytes()))
}

fn sign_es256(pem: &str, message: &[u8]) -> EngineResult<String> {
    use p256::ecdsa::SigningKey;
    use p256::ecdsa::signature::Signer;
    use base64::Engine as _;

    let signing_key = {
        use p256::pkcs8::DecodePrivateKey;
        SigningKey::from_pkcs8_pem(pem)
    }.or_else(|_| {
        use p256::elliptic_curve::SecretKey;
        let secret_key = SecretKey::<p256::NistP256>::from_sec1_pem(pem)
            .map_err(|e| EngineError::Other(e.to_string()))?;
        Ok::<SigningKey, EngineError>(SigningKey::from(secret_key))
    })?;

    let signature: p256::ecdsa::Signature = signing_key.sign(message);
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature.to_bytes()))
}

async fn cdp_request(
    creds: &std::collections::HashMap<String, String>,
    method: &str,
    path: &str,
    body: Option<&serde_json::Value>,
) -> EngineResult<serde_json::Value> {
    let key_name = creds.get("CDP_API_KEY_NAME").ok_or("Missing CDP_API_KEY_NAME")?;
    let key_secret = creds.get("CDP_API_KEY_SECRET").ok_or("Missing CDP_API_KEY_SECRET")?;

    let host = "api.coinbase.com";
    let jwt_path = path.split('?').next().unwrap_or(path);
    let jwt = build_cdp_jwt(key_name, key_secret, method, host, jwt_path)?;

    let url = format!("https://{}{}", host, path);
    let client = reqwest::Client::new();
    let mut req = match method {
        "POST"   => client.post(&url),
        "PUT"    => client.put(&url),
        "DELETE" => client.delete(&url),
        _        => client.get(&url),
    };

    req = req
        .header("Authorization", format!("Bearer {}", jwt))
        .header("Content-Type", "application/json")
        .timeout(Duration::from_secs(30));

    if let Some(b) = body {
        req = req.json(b);
    }

    let resp = req.send().await?;
    let status = resp.status();
    let text = resp.text().await?;

    if !status.is_success() {
        warn!("[skill:coinbase] API error {} on {} {}: {}", status, method, path, &text[..text.len().min(500)]);
        return Err(format!("Coinbase API error (HTTP {}): {}", status.as_u16(), &text[..text.len().min(500)]).into());
    }

    info!("[skill:coinbase] {} {} -> {}", method, path, status);

    serde_json::from_str(&text).map_err(|e| EngineError::Other(format!("Parse Coinbase response: {} — raw: {}", e, &text[..text.len().min(300)])))
}

// ── coinbase_prices ──

async fn execute_coinbase_prices(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> EngineResult<String> {
    let symbols_str = args["symbols"].as_str().ok_or("coinbase_prices: missing 'symbols'")?;
    let symbols: Vec<String> = symbols_str.split(',').map(|s| s.trim().to_uppercase().to_string()).collect();

    info!("[skill:coinbase] Fetching prices for: {}", symbols_str);

    let mut results = Vec::new();
    for sym in &symbols {
        let product_id = format!("{}-USD", sym);
        let path = format!("/api/v3/brokerage/products/{}", product_id);
        match cdp_request(creds, "GET", &path, None).await {
            Ok(data) => {
                let price = data["price"].as_str().unwrap_or("?");
                results.push(format!("{}: ${} USD", sym, price));
            }
            Err(e) => results.push(format!("{}: error — {}", sym, e)),
        }
    }

    Ok(format!("Current Prices:\n{}", results.join("\n")))
}

// ── coinbase_balance ──

async fn execute_coinbase_balance(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> EngineResult<String> {
    let filter_currency = args["currency"].as_str().map(|s| s.to_uppercase());
    info!("[skill:coinbase] Fetching account balances");

    let data = cdp_request(creds, "GET", "/api/v3/brokerage/accounts?limit=250", None).await?;
    let accounts = data["accounts"].as_array().ok_or("Unexpected response format — no 'accounts' array")?;

    let mut lines = Vec::new();
    for acct in accounts {
        let currency = acct["currency"].as_str().unwrap_or("?");
        let available = acct["available_balance"]["value"].as_str().unwrap_or("0");
        let hold = acct["hold"]["value"].as_str().unwrap_or("0");
        let avail_f: f64 = available.parse().unwrap_or(0.0);
        let hold_f: f64 = hold.parse().unwrap_or(0.0);
        let total = avail_f + hold_f;

        if total == 0.0 && filter_currency.is_none() { continue; }
        if let Some(ref fc) = filter_currency {
            if currency.to_uppercase() != *fc { continue; }
        }

        let name = acct["name"].as_str().unwrap_or(currency);
        if hold_f > 0.0 {
            lines.push(format!("  {} ({}): {} available + {} hold", name, currency, available, hold));
        } else {
            lines.push(format!("  {} ({}): {}", name, currency, available));
        }
    }

    if lines.is_empty() {
        Ok("No non-zero balances found.".into())
    } else {
        Ok(format!("Account Balances:\n{}", lines.join("\n")))
    }
}

// ── coinbase_wallet_create ──

async fn execute_coinbase_wallet_create(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> EngineResult<String> {
    let name = args["name"].as_str().ok_or("coinbase_wallet_create: missing 'name'")?;
    info!("[skill:coinbase] Creating wallet: {}", name);

    let body = serde_json::json!({ "name": name });
    let data = cdp_request(creds, "POST", "/api/v3/brokerage/portfolios", Some(&body)).await?;
    let portfolio = &data["portfolio"];
    let id = portfolio["uuid"].as_str().unwrap_or("?");
    let created_name = portfolio["name"].as_str().unwrap_or(name);
    let ptype = portfolio["type"].as_str().unwrap_or("DEFAULT");

    Ok(format!("Portfolio created!\n  Name: {}\n  ID: {}\n  Type: {}", created_name, id, ptype))
}

// ── coinbase_trade ──

async fn execute_coinbase_trade(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> EngineResult<String> {
    let side = args["side"].as_str().ok_or("coinbase_trade: missing 'side'")?;
    let product_id = args["product_id"].as_str().ok_or("coinbase_trade: missing 'product_id'")?;
    let amount = args["amount"].as_str().ok_or("coinbase_trade: missing 'amount'")?;
    let order_type = args["order_type"].as_str().unwrap_or("market");
    let limit_price = args["limit_price"].as_str();
    let reason = args["reason"].as_str().unwrap_or("No reason provided");

    info!("[skill:coinbase] Trade {} {} {} ({}). Reason: {}", side, amount, product_id, order_type, reason);

    let order_configuration = if order_type == "limit" {
        let price = limit_price.ok_or("coinbase_trade: limit orders require 'limit_price'")?;
        serde_json::json!({ "limit_limit_gtc": { "base_size": amount, "limit_price": price } })
    } else if side == "buy" {
        serde_json::json!({ "market_market_ioc": { "quote_size": amount } })
    } else {
        serde_json::json!({ "market_market_ioc": { "base_size": amount } })
    };

    let body = serde_json::json!({
        "client_order_id": uuid::Uuid::new_v4().to_string(),
        "product_id": product_id,
        "side": side.to_uppercase(),
        "order_configuration": order_configuration
    });

    let data = cdp_request(creds, "POST", "/api/v3/brokerage/orders", Some(&body)).await?;

    let success = data["success"].as_bool().unwrap_or(false);
    let order_id = data["success_response"]["order_id"].as_str()
        .or_else(|| data["order_id"].as_str())
        .unwrap_or("?");

    if success || data.get("success_response").is_some() {
        Ok(format!(
            "Order placed successfully!\n  Side: {}\n  Product: {}\n  Amount: {}\n  Type: {}\n  Order ID: {}\n  Reason: {}",
            side, product_id, amount, order_type, order_id, reason
        ))
    } else {
        let err_msg = data["error_response"]["message"].as_str()
            .or_else(|| data["message"].as_str())
            .unwrap_or("Unknown error");
        Err(format!("Trade failed: {} — Full response: {}", err_msg, serde_json::to_string_pretty(&data).unwrap_or_default()).into())
    }
}

// ── coinbase_transfer ──

async fn execute_coinbase_transfer(
    args: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> EngineResult<String> {
    let currency = args["currency"].as_str().ok_or("coinbase_transfer: missing 'currency'")?;
    let amount = args["amount"].as_str().ok_or("coinbase_transfer: missing 'amount'")?;
    let to_address = args["to_address"].as_str().ok_or("coinbase_transfer: missing 'to_address'")?;
    let network = args["network"].as_str();
    let reason = args["reason"].as_str().unwrap_or("No reason provided");

    info!("[skill:coinbase] Transfer {} {} to {} (reason: {})", amount, currency, &to_address[..to_address.len().min(12)], reason);

    let accounts_data = cdp_request(creds, "GET", "/api/v3/brokerage/accounts?limit=250", None).await?;
    let accounts = accounts_data["accounts"].as_array().ok_or("Cannot list accounts")?;

    let account = accounts.iter()
        .find(|a| a["currency"].as_str().unwrap_or("").eq_ignore_ascii_case(currency))
        .ok_or(format!("No account found for currency: {}", currency))?;

    let account_uuid = account["uuid"].as_str().ok_or("Account missing UUID")?;
    let available = account["available_balance"]["value"].as_str().unwrap_or("0");
    let avail_f: f64 = available.parse().unwrap_or(0.0);
    let amount_f: f64 = amount.parse().unwrap_or(0.0);
    if amount_f > avail_f {
        return Err(format!("Insufficient {} balance: {} available, {} requested", currency, available, amount).into());
    }

    let send_path = format!("/v2/accounts/{}/transactions", account_uuid);
    let mut body = serde_json::json!({
        "type": "send",
        "to": to_address,
        "amount": amount,
        "currency": currency.to_uppercase(),
        "description": reason
    });

    if let Some(net) = network {
        body["network"] = serde_json::json!(net);
    }

    let data = cdp_request(creds, "POST", &send_path, Some(&body)).await?;
    let tx_data = data.get("data").unwrap_or(&data);
    let tx_id = tx_data["id"].as_str().unwrap_or("?");
    let status = tx_data["status"].as_str().unwrap_or("pending");
    let tx_network = tx_data["network"]["name"].as_str().unwrap_or("unknown");

    Ok(format!(
        "Transfer initiated!\n  {} {} -> {}\n  Network: {}\n  Status: {}\n  TX ID: {}\n  Reason: {}",
        amount, currency.to_uppercase(), &to_address[..to_address.len().min(20)],
        tx_network, status, tx_id, reason
    ))
}
