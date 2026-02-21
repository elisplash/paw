// Paw Agent Engine — DEX Wallet Creation

use super::constants::chain_name;
use super::primitives::{address_from_pubkey, hex_encode};
use super::rpc::eth_chain_id;
use crate::atoms::error::{EngineError, EngineResult};
use std::collections::HashMap;
use log::info;

/// Create a new Ethereum wallet and store the private key in the vault.
pub async fn execute_dex_wallet_create(
    _args: &serde_json::Value,
    creds: &HashMap<String, String>,
    app_handle: &tauri::AppHandle,
) -> EngineResult<String> {
    if creds.contains_key("DEX_PRIVATE_KEY") && creds.contains_key("DEX_WALLET_ADDRESS") {
        let addr = creds.get("DEX_WALLET_ADDRESS")
            .ok_or(EngineError::Other("DEX_WALLET_ADDRESS not found in credentials".into()))?;
        return Ok(format!(
            "Wallet already exists!\n\nAddress: {}\n\nTo create a new wallet, first remove the existing credentials in Settings → Skills → DEX Trading.",
            addr
        ));
    }

    // Generate a new secp256k1 keypair
    use k256::ecdsa::SigningKey;
    let signing_key = SigningKey::random(&mut rand::thread_rng());
    let verifying_key = signing_key.verifying_key();

    // Get uncompressed public key bytes
    let pubkey_bytes = verifying_key.to_encoded_point(false);
    let address = address_from_pubkey(pubkey_bytes.as_bytes());

    // Store private key encrypted in vault
    let private_key_hex = hex_encode(&signing_key.to_bytes());

    use tauri::Manager;
    let state = app_handle.try_state::<crate::engine::state::EngineState>()
        .ok_or(EngineError::Other("Engine state not available".into()))?;
    let vault_key = crate::engine::skills::get_vault_key()?;

    let encrypted_key = crate::engine::skills::encrypt_credential(&private_key_hex, &vault_key);
    state.store.set_skill_credential("dex", "DEX_PRIVATE_KEY", &encrypted_key)?;

    let encrypted_addr = crate::engine::skills::encrypt_credential(&address, &vault_key);
    state.store.set_skill_credential("dex", "DEX_WALLET_ADDRESS", &encrypted_addr)?;

    info!("[dex] Created new wallet: {}", address);

    let network_name = if let Some(rpc_url) = creds.get("DEX_RPC_URL") {
        match eth_chain_id(rpc_url).await {
            Ok(id) => chain_name(id),
            Err(_) => "Unknown",
        }
    } else {
        "Not connected (configure RPC URL)"
    };

    Ok(format!(
        "✅ New wallet created!\n\nAddress: {}\nNetwork: {}\n\n⚠️ This wallet has zero balance. Send ETH to this address to fund it before trading.\n\n🔒 Private key is encrypted and stored in your OS keychain vault. The AI agent never sees it.",
        address, network_name
    ))
}
