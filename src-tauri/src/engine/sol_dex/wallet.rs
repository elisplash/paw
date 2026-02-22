// Solana DEX — Wallet
// pubkey_from_secret, execute_sol_wallet_create

use std::collections::HashMap;
use log::info;
use super::rpc::rpc_call;
use crate::atoms::error::{EngineError, EngineResult};

/// Derive Solana public key (base58) from ed25519 secret key bytes
#[allow(dead_code)]
pub(crate) fn pubkey_from_secret(secret_bytes: &[u8; 32]) -> EngineResult<String> {
    use ed25519_dalek::SigningKey;
    let signing_key = SigningKey::from_bytes(secret_bytes);
    let public_key = signing_key.verifying_key();
    Ok(bs58::encode(public_key.as_bytes()).into_string())
}

/// sol_wallet_create — Generate ed25519 keypair, store in vault
pub async fn execute_sol_wallet_create(
    _args: &serde_json::Value,
    creds: &HashMap<String, String>,
    app_handle: &tauri::AppHandle,
) -> EngineResult<String> {
    use tauri::Manager;

    // Check if wallet already exists
    if creds.contains_key("SOLANA_PRIVATE_KEY") && creds.contains_key("SOLANA_WALLET_ADDRESS") {
        let addr = creds.get("SOLANA_WALLET_ADDRESS").unwrap();
        return Ok(format!(
            "Solana wallet already exists!\n\nAddress: {}\n\nTo create a new wallet, first remove the existing credentials in Skills → Solana DEX Trading.",
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

    let state = app_handle.try_state::<crate::engine::state::EngineState>()
        .ok_or(EngineError::Other("Engine state not available".into()))?;
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
