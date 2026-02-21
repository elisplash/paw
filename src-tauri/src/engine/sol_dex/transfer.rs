// Solana DEX — Outbound Transfer (SOL + SPL Tokens)
// execute_sol_transfer

use std::collections::HashMap;
use log::info;
use super::constants::TOKEN_PROGRAM_ID;
use super::helpers::{resolve_token, amount_to_lamports, lamports_to_amount, parse_solana_keypair};
use super::rpc::{rpc_call, get_sol_balance, get_token_accounts, check_tx_confirmation};
use super::transaction::{sign_solana_transaction, build_solana_transaction, derive_ata};
use crate::atoms::error::{EngineResult, EngineError};

/// Transfer SOL or SPL tokens to an external address.
pub async fn execute_sol_transfer(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> EngineResult<String> {
    let rpc_url = creds.get("SOLANA_RPC_URL").ok_or("Missing SOLANA_RPC_URL.")?;
    let wallet = creds.get("SOLANA_WALLET_ADDRESS").ok_or("No Solana wallet. Use sol_wallet_create first.")?;
    let private_key_b58 = creds.get("SOLANA_PRIVATE_KEY").ok_or("No Solana private key.")?;

    let currency = args["currency"].as_str().ok_or("sol_transfer: missing 'currency'")?;
    let amount_str = args["amount"].as_str().ok_or("sol_transfer: missing 'amount'")?;
    let to_address = args["to_address"].as_str().ok_or("sol_transfer: missing 'to_address'")?;
    let _reason = args["reason"].as_str().unwrap_or("transfer");

    // Validate recipient address
    let to_bytes_vec = bs58::decode(to_address).into_vec()
        .map_err(|e| EngineError::Other(e.to_string()))?;
    if to_bytes_vec.len() != 32 {
        return Err(format!("Invalid Solana address length: expected 32 bytes, got {}", to_bytes_vec.len()).into());
    }
    let mut to_pubkey = [0u8; 32];
    to_pubkey.copy_from_slice(&to_bytes_vec);

    // Decode sender keypair
    let secret_bytes = parse_solana_keypair(private_key_b58)?;

    let sender_pubkey_vec = bs58::decode(wallet).into_vec()
        .map_err(|e| EngineError::Other(e.to_string()))?;
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
        .map_err(|e| EngineError::Other(e.to_string()))?;
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
            return Err(format!("Insufficient SOL balance. Have: {} SOL, need: {} SOL + fees", bal_display, amount_str).into());
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
            .map_err(|e| EngineError::Other(e.to_string()))?;
        let mut mint_pubkey = [0u8; 32];
        mint_pubkey.copy_from_slice(&mint_bytes_vec);

        // Check sender's token balance
        let token_accounts = get_token_accounts(rpc_url, wallet).await?;
        let sender_token = token_accounts.iter()
            .find(|(m, _, _, _)| m == &mint_addr);
        match sender_token {
            None => return Err(format!("No {} token account found in wallet", currency_upper).into()),
            Some((_, amt, _, _)) if *amt < raw_amount => {
                return Err(format!("Insufficient {} balance. Have: {}, need: {}",
                    currency_upper, lamports_to_amount(*amt, decimals), amount_str).into());
            }
            _ => {}
        }
        let sender_ata_addr = sender_token.unwrap().3.clone();
        let sender_ata_bytes = bs58::decode(&sender_ata_addr).into_vec()
            .map_err(|e| EngineError::Other(e.to_string()))?;
        let mut sender_ata = [0u8; 32];
        sender_ata.copy_from_slice(&sender_ata_bytes);

        // Check SOL for fees
        let sol_balance = get_sol_balance(rpc_url, wallet).await?;
        if sol_balance < 10_000 {
            return Err("Insufficient SOL for transaction fees (~0.00001 SOL needed)".into());
        }

        // Derive recipient ATA
        let token_program_bytes = bs58::decode(TOKEN_PROGRAM_ID).into_vec()
            .map_err(|e| EngineError::Other(e.to_string()))?;
        let mut token_program = [0u8; 32];
        token_program.copy_from_slice(&token_program_bytes);

        let recipient_ata = derive_ata(&to_pubkey, &mint_pubkey, &token_program)?;

        // Check if recipient ATA exists
        let recipient_ata_b58 = bs58::encode(&recipient_ata).into_string();
        let ata_exists = rpc_call(rpc_url, "getAccountInfo", serde_json::json!([
            recipient_ata_b58, { "encoding": "base64" }
        ])).await.map(|r| r.get("value").is_some_and(|v| !v.is_null())).unwrap_or(false);

        // ATA Program ID
        let ata_program_bytes = bs58::decode("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL")
            .into_vec()
            .map_err(|e| EngineError::Other(e.to_string()))?;
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
    let confirmation = check_tx_confirmation(rpc_url, &tx_sig).await;

    info!("[sol_dex] Transfer {} {} → {} | tx: {}", amount_str, currency_upper, to_address, tx_sig);

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
