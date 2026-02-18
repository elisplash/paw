// Paw Agent Engine — DEX Trading (Uniswap / EVM)
// Self-custody Ethereum wallet with on-chain swap execution.
//
// Architecture:
// - Private key stored encrypted in the Skill Vault (OS keychain + SQLite)
// - Key is decrypted ONLY in this Rust module for transaction signing
// - The agent never sees the private key — only tool parameters and tx hashes
// - All swaps go through the Human-in-the-Loop approval modal
// - Trading policy limits (max trade, daily cap) enforced server-side
//
// Supported operations:
// - dex_wallet_create: Generate secp256k1 keypair, store in vault, return address
// - dex_balance: Check ETH + ERC-20 balances via JSON-RPC
// - dex_quote: Get swap quote from Uniswap V3 Quoter
// - dex_swap: Execute swap: quote → approve → build tx → sign → broadcast
// - dex_portfolio: Multi-token balance check

use log::info;
use std::collections::HashMap;
use std::time::Duration;
use tauri::Manager;

// ── Constants ──────────────────────────────────────────────────────────

/// Well-known ERC-20 tokens on Ethereum mainnet
const KNOWN_TOKENS: &[(&str, &str, u8)] = &[
    ("ETH",  "0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE", 18),
    ("WETH", "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", 18),
    ("USDC", "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", 6),
    ("USDT", "0xdAC17F958D2ee523a2206206994597C13D831ec7", 6),
    ("DAI",  "0x6B175474E89094C44Da98b954EedeAC495271d0F", 18),
    ("WBTC", "0x2260FAC5E5542a773Aa44fBCfeDf7C193bc2C599", 8),
    ("UNI",  "0x1f9840a85d5aF5bf1D1762F925BDADdC4201F984", 18),
    ("LINK", "0x514910771AF9Ca656af840dff83E8264EcF986CA", 18),
    ("PEPE", "0x6982508145454Ce325dDbE47a25d4ec3d2311933", 18),
    ("SHIB", "0x95aD61b0a150d79219dCF64E1E6Cc01f0B64C4cE", 18),
    ("ARB",  "0xB50721BCf8d664c30412Cfbc6cf7a15145234ad1", 18),
    ("AAVE", "0x7Fc66500c84A76Ad7e9c93437bFc5Ac33E2DDaE9", 18),
];

/// Uniswap V3 contract addresses (Ethereum mainnet)
const UNISWAP_QUOTER_V2: &str = "0x61fFE014bA17989E743c5F6cB21bF9697530B21e";
const UNISWAP_SWAP_ROUTER_02: &str = "0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45";
const WETH_ADDRESS: &str = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";

/// Default slippage tolerance (0.5%)
const DEFAULT_SLIPPAGE_BPS: u64 = 50;
/// Maximum allowed slippage (5%)
const MAX_SLIPPAGE_BPS: u64 = 500;
/// Default fee tier for Uniswap V3 (0.3%)
const DEFAULT_FEE_TIER: u64 = 3000;

// ── Ethereum Primitives ────────────────────────────────────────────────

/// Keccak-256 hash (Ethereum's hash function)
fn keccak256(data: &[u8]) -> [u8; 32] {
    use tiny_keccak::{Hasher, Keccak};
    let mut hasher = Keccak::v256();
    let mut output = [0u8; 32];
    hasher.update(data);
    hasher.finalize(&mut output);
    output
}

/// Hex-encode bytes with 0x prefix
fn hex_encode(data: &[u8]) -> String {
    format!("0x{}", data.iter().map(|b| format!("{:02x}", b)).collect::<String>())
}

/// Hex-decode a 0x-prefixed string
fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.len() % 2 != 0 {
        return Err("Odd-length hex string".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| format!("Hex decode: {}", e)))
        .collect()
}

/// Derive Ethereum address from secp256k1 public key
fn address_from_pubkey(pubkey_uncompressed: &[u8]) -> String {
    // Skip the 0x04 prefix (uncompressed key marker), hash the 64-byte x||y
    let hash = keccak256(&pubkey_uncompressed[1..]);
    // Address is last 20 bytes
    let addr = &hash[12..];
    // EIP-55 checksum encoding
    eip55_checksum(addr)
}

/// EIP-55 mixed-case checksum address
fn eip55_checksum(addr_bytes: &[u8]) -> String {
    let hex_addr: String = addr_bytes.iter().map(|b| format!("{:02x}", b)).collect();
    let hash = keccak256(hex_addr.as_bytes());
    let mut checksummed = String::with_capacity(42);
    checksummed.push_str("0x");
    for (i, c) in hex_addr.chars().enumerate() {
        let hash_nibble = if i % 2 == 0 { hash[i / 2] >> 4 } else { hash[i / 2] & 0x0f };
        if hash_nibble >= 8 {
            checksummed.push(c.to_ascii_uppercase());
        } else {
            checksummed.push(c);
        }
    }
    checksummed
}

/// Parse an address string to 20 bytes
fn parse_address(addr: &str) -> Result<[u8; 20], String> {
    let bytes = hex_decode(addr)?;
    if bytes.len() != 20 {
        return Err(format!("Invalid address length: {} bytes", bytes.len()));
    }
    let mut arr = [0u8; 20];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

/// Parse a U256 from decimal string
fn parse_u256_decimal(s: &str) -> Result<[u8; 32], String> {
    // Simple decimal-to-big-endian conversion
    let mut result = [0u8; 32];

    // Handle scientific notation
    if s.contains('e') || s.contains('E') {
        return Err("Scientific notation not supported, use plain decimal".into());
    }

    // Convert decimal string to bytes
    let mut digits: Vec<u8> = Vec::new();
    for c in s.chars() {
        if !c.is_ascii_digit() {
            return Err(format!("Invalid decimal character: {}", c));
        }
        digits.push(c as u8 - b'0');
    }

    // Convert to big-endian bytes using repeated division by 256
    let mut big = digits;
    let mut byte_pos = 31i32;
    while !big.is_empty() && !(big.len() == 1 && big[0] == 0) && byte_pos >= 0 {
        let mut remainder = 0u16;
        let mut quotient = Vec::new();
        for &d in &big {
            let val = remainder * 10 + d as u16;
            let q = val / 256;
            remainder = val % 256;
            if !quotient.is_empty() || q > 0 {
                quotient.push(q as u8);
            }
        }
        result[byte_pos as usize] = remainder as u8;
        byte_pos -= 1;
        big = quotient;
    }
    Ok(result)
}

/// Convert a token amount with decimals to raw units
/// e.g., "1.5" with 18 decimals → "1500000000000000000"
fn amount_to_raw(amount: &str, decimals: u8) -> Result<String, String> {
    let parts: Vec<&str> = amount.split('.').collect();
    if parts.len() > 2 {
        return Err("Invalid amount format".into());
    }
    let integer_part = parts[0];
    let decimal_part = if parts.len() == 2 { parts[1] } else { "" };

    if decimal_part.len() > decimals as usize {
        return Err(format!("Too many decimal places (max {} for this token)", decimals));
    }

    let padded_decimals = format!("{:0<width$}", decimal_part, width = decimals as usize);
    let raw = format!("{}{}", integer_part, padded_decimals);
    // Strip leading zeros but keep at least "0"
    let trimmed = raw.trim_start_matches('0');
    if trimmed.is_empty() { Ok("0".into()) } else { Ok(trimmed.into()) }
}

/// Convert raw units to human-readable amount
fn raw_to_amount(raw_hex: &str, decimals: u8) -> Result<String, String> {
    let raw_bytes = hex_decode(raw_hex)?;
    // Convert big-endian bytes to decimal string
    let mut value = Vec::new();
    for &b in &raw_bytes {
        // Multiply existing value by 256 and add new byte
        let mut carry = b as u16;
        for d in value.iter_mut().rev() {
            let val = *d as u16 * 256 + carry;
            *d = (val % 10) as u8;
            carry = val / 10;
        }
        while carry > 0 {
            value.insert(0, (carry % 10) as u8);
            carry /= 10;
        }
    }
    if value.is_empty() {
        value.push(0);
    }

    let decimal_str: String = value.iter().map(|d| (d + b'0') as char).collect();

    if decimals == 0 {
        return Ok(decimal_str);
    }

    let dec = decimals as usize;
    if decimal_str.len() <= dec {
        let padded = format!("{:0>width$}", decimal_str, width = dec + 1);
        let (int_part, frac_part) = padded.split_at(padded.len() - dec);
        Ok(format!("{}.{}", int_part, frac_part.trim_end_matches('0')).trim_end_matches('.').to_string())
    } else {
        let (int_part, frac_part) = decimal_str.split_at(decimal_str.len() - dec);
        let trimmed_frac = frac_part.trim_end_matches('0');
        if trimmed_frac.is_empty() {
            Ok(int_part.to_string())
        } else {
            Ok(format!("{}.{}", int_part, trimmed_frac))
        }
    }
}

// ── ABI Encoding ───────────────────────────────────────────────────────

/// Compute 4-byte function selector from signature
fn function_selector(sig: &str) -> [u8; 4] {
    let hash = keccak256(sig.as_bytes());
    let mut sel = [0u8; 4];
    sel.copy_from_slice(&hash[..4]);
    sel
}

/// ABI-encode an address (left-padded to 32 bytes)
fn abi_encode_address(addr: &[u8; 20]) -> Vec<u8> {
    let mut encoded = vec![0u8; 12]; // 12 zero bytes
    encoded.extend_from_slice(addr);
    encoded
}

/// ABI-encode a uint256 from big-endian bytes
fn abi_encode_uint256(val: &[u8; 32]) -> Vec<u8> {
    val.to_vec()
}

/// ABI-encode a uint24 (fee tier) as uint256
fn abi_encode_uint24_as_uint256(val: u32) -> Vec<u8> {
    let mut encoded = vec![0u8; 32];
    encoded[29] = ((val >> 16) & 0xFF) as u8;
    encoded[30] = ((val >> 8) & 0xFF) as u8;
    encoded[31] = (val & 0xFF) as u8;
    encoded
}

/// Encode ERC-20 balanceOf(address)
fn encode_balance_of(address: &[u8; 20]) -> Vec<u8> {
    let selector = function_selector("balanceOf(address)");
    let mut data = selector.to_vec();
    data.extend_from_slice(&abi_encode_address(address));
    data
}

/// Encode ERC-20 approve(address, uint256)
fn encode_approve(spender: &[u8; 20], amount: &[u8; 32]) -> Vec<u8> {
    let selector = function_selector("approve(address,uint256)");
    let mut data = selector.to_vec();
    data.extend_from_slice(&abi_encode_address(spender));
    data.extend_from_slice(&abi_encode_uint256(amount));
    data
}

/// Encode ERC-20 allowance(owner, spender)
fn encode_allowance(owner: &[u8; 20], spender: &[u8; 20]) -> Vec<u8> {
    let selector = function_selector("allowance(address,address)");
    let mut data = selector.to_vec();
    data.extend_from_slice(&abi_encode_address(owner));
    data.extend_from_slice(&abi_encode_address(spender));
    data
}

/// Encode Uniswap V3 QuoterV2.quoteExactInput for multi-hop paths
/// quoteExactInput(bytes path, uint256 amountIn) → (uint256 amountOut, ...)
fn encode_quote_exact_input(
    path: &[u8],
    amount_in: &[u8; 32],
) -> Vec<u8> {
    let selector = function_selector("quoteExactInput(bytes,uint256)");
    let mut data = selector.to_vec();
    // ABI: offset to path (dynamic), amountIn
    let mut offset = [0u8; 32];
    offset[31] = 64; // offset = 0x40 (2 * 32 bytes)
    data.extend_from_slice(&offset);
    data.extend_from_slice(&abi_encode_uint256(amount_in));
    // path: length + data (padded to 32 bytes)
    let mut path_len = [0u8; 32];
    let plen = path.len();
    path_len[28] = ((plen >> 24) & 0xFF) as u8;
    path_len[29] = ((plen >> 16) & 0xFF) as u8;
    path_len[30] = ((plen >> 8) & 0xFF) as u8;
    path_len[31] = (plen & 0xFF) as u8;
    data.extend_from_slice(&path_len);
    data.extend_from_slice(path);
    // Pad to 32-byte boundary
    let pad = (32 - (plen % 32)) % 32;
    data.extend(vec![0u8; pad]);
    data
}

/// Encode Uniswap V3 SwapRouter02.exactInput for multi-hop swaps
/// exactInput((bytes path, address recipient, uint256 amountIn, uint256 amountOutMinimum))
fn encode_exact_input(
    path: &[u8],
    recipient: &[u8; 20],
    amount_in: &[u8; 32],
    amount_out_minimum: &[u8; 32],
) -> Vec<u8> {
    let selector = function_selector("exactInput((bytes,address,uint256,uint256))");
    let mut data = selector.to_vec();
    // Struct with dynamic field: offset to start of struct = 0x20
    let mut struct_offset = [0u8; 32];
    struct_offset[31] = 32;
    data.extend_from_slice(&struct_offset);
    // Inside struct: offset to path data, recipient, amountIn, amountOutMinimum
    let mut path_offset = [0u8; 32];
    path_offset[31] = 128; // 4 * 32 bytes offset
    data.extend_from_slice(&path_offset);
    data.extend_from_slice(&abi_encode_address(recipient));
    data.extend_from_slice(&abi_encode_uint256(amount_in));
    data.extend_from_slice(&abi_encode_uint256(amount_out_minimum));
    // path: length + data (padded to 32 bytes)
    let mut path_len = [0u8; 32];
    let plen = path.len();
    path_len[28] = ((plen >> 24) & 0xFF) as u8;
    path_len[29] = ((plen >> 16) & 0xFF) as u8;
    path_len[30] = ((plen >> 8) & 0xFF) as u8;
    path_len[31] = (plen & 0xFF) as u8;
    data.extend_from_slice(&path_len);
    data.extend_from_slice(path);
    let pad = (32 - (plen % 32)) % 32;
    data.extend(vec![0u8; pad]);
    data
}

/// Build a Uniswap V3 multi-hop path: token0 + fee + token1 + fee + token2 ...
/// Each hop is: 20 bytes (address) + 3 bytes (fee as uint24, big-endian)
fn build_multihop_path(tokens: &[&[u8; 20]], fees: &[u32]) -> Vec<u8> {
    let mut path = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        path.extend_from_slice(*token);
        if i < fees.len() {
            path.push(((fees[i] >> 16) & 0xFF) as u8);
            path.push(((fees[i] >> 8) & 0xFF) as u8);
            path.push((fees[i] & 0xFF) as u8);
        }
    }
    path
}

/// Strip leading zero bytes from a byte slice (for RLP encoding of r, s values)
fn strip_leading_zeros(data: &[u8]) -> Vec<u8> {
    let first_nonzero = data.iter().position(|&b| b != 0);
    match first_nonzero {
        Some(pos) => data[pos..].to_vec(),
        None => vec![],
    }
}

/// Convert a u256 (big-endian [u8; 32]) to a quantity hex string ("0x1234", no leading zeros)
fn u256_to_quantity_hex(val: &[u8; 32]) -> String {
    let stripped = strip_leading_zeros(val);
    if stripped.is_empty() {
        "0x0".to_string()
    } else {
        format!("0x{}", stripped.iter().map(|b| format!("{:02x}", b)).collect::<String>())
    }
}

/// Encode Uniswap V3 QuoterV2.quoteExactInputSingle
/// quoteExactInputSingle((address,address,uint256,uint24,uint160))
fn encode_quote_exact_input_single(
    token_in: &[u8; 20],
    token_out: &[u8; 20],
    amount_in: &[u8; 32],
    fee: u32,
) -> Vec<u8> {
    let selector = function_selector("quoteExactInputSingle((address,address,uint256,uint24,uint160))");
    let mut data = selector.to_vec();

    // Struct is encoded inline as: token_in, token_out, amountIn, fee, sqrtPriceLimitX96
    data.extend_from_slice(&abi_encode_address(token_in));
    data.extend_from_slice(&abi_encode_address(token_out));
    data.extend_from_slice(&abi_encode_uint256(amount_in));
    data.extend_from_slice(&abi_encode_uint24_as_uint256(fee));
    data.extend_from_slice(&[0u8; 32]); // sqrtPriceLimitX96 = 0 (no limit)
    data
}

/// Encode Uniswap V3 SwapRouter02.exactInputSingle
/// exactInputSingle((address,address,uint24,address,uint256,uint256,uint160))
fn encode_exact_input_single(
    token_in: &[u8; 20],
    token_out: &[u8; 20],
    fee: u32,
    recipient: &[u8; 20],
    amount_in: &[u8; 32],
    amount_out_minimum: &[u8; 32],
) -> Vec<u8> {
    let selector = function_selector("exactInputSingle((address,address,uint24,address,uint256,uint256,uint160))");
    let mut data = selector.to_vec();

    data.extend_from_slice(&abi_encode_address(token_in));
    data.extend_from_slice(&abi_encode_address(token_out));
    data.extend_from_slice(&abi_encode_uint24_as_uint256(fee));
    data.extend_from_slice(&abi_encode_address(recipient));
    data.extend_from_slice(&abi_encode_uint256(amount_in));
    data.extend_from_slice(&abi_encode_uint256(amount_out_minimum));
    data.extend_from_slice(&[0u8; 32]); // sqrtPriceLimitX96 = 0
    data
}

// ── RLP Encoding ───────────────────────────────────────────────────────

/// RLP-encode a single byte string
fn rlp_encode_bytes(data: &[u8]) -> Vec<u8> {
    if data.len() == 1 && data[0] < 0x80 {
        return data.to_vec();
    }
    if data.is_empty() {
        return vec![0x80];
    }
    if data.len() <= 55 {
        let mut encoded = vec![(0x80 + data.len()) as u8];
        encoded.extend_from_slice(data);
        encoded
    } else {
        let len_bytes = to_minimal_be_bytes(data.len());
        let mut encoded = vec![(0xb7 + len_bytes.len()) as u8];
        encoded.extend_from_slice(&len_bytes);
        encoded.extend_from_slice(data);
        encoded
    }
}

/// RLP-encode a list of already-RLP-encoded items
fn rlp_encode_list(items: &[Vec<u8>]) -> Vec<u8> {
    let payload: Vec<u8> = items.iter().flat_map(|i| i.clone()).collect();
    if payload.len() <= 55 {
        let mut encoded = vec![(0xc0 + payload.len()) as u8];
        encoded.extend_from_slice(&payload);
        encoded
    } else {
        let len_bytes = to_minimal_be_bytes(payload.len());
        let mut encoded = vec![(0xf7 + len_bytes.len()) as u8];
        encoded.extend_from_slice(&len_bytes);
        encoded.extend_from_slice(&payload);
        encoded
    }
}

/// Convert usize to minimal big-endian byte representation
fn to_minimal_be_bytes(val: usize) -> Vec<u8> {
    if val == 0 { return vec![]; }
    let bytes = val.to_be_bytes();
    let first_nonzero = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len() - 1);
    bytes[first_nonzero..].to_vec()
}

/// Encode a u64 as minimal big-endian bytes (for RLP)
fn u64_to_minimal_be(val: u64) -> Vec<u8> {
    if val == 0 { return vec![]; }
    let bytes = val.to_be_bytes();
    let first_nonzero = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len() - 1);
    bytes[first_nonzero..].to_vec()
}

/// Encode a u256 (big-endian [u8; 32]) as minimal big-endian bytes
fn u256_to_minimal_be(val: &[u8; 32]) -> Vec<u8> {
    let first_nonzero = val.iter().position(|&b| b != 0);
    match first_nonzero {
        Some(pos) => val[pos..].to_vec(),
        None => vec![], // represents zero
    }
}

// ── EIP-1559 Transaction Building & Signing ────────────────────────────

/// Build and sign an EIP-1559 (Type 2) transaction
fn sign_eip1559_transaction(
    chain_id: u64,
    nonce: u64,
    max_priority_fee_per_gas: u64,
    max_fee_per_gas: u64,
    gas_limit: u64,
    to: &[u8; 20],
    value: &[u8; 32],
    data: &[u8],
    private_key: &k256::ecdsa::SigningKey,
) -> Result<Vec<u8>, String> {
    // EIP-1559 unsigned tx: 0x02 || RLP([chain_id, nonce, max_priority_fee, max_fee, gas, to, value, data, access_list])
    let items = vec![
        rlp_encode_bytes(&u64_to_minimal_be(chain_id)),
        rlp_encode_bytes(&u64_to_minimal_be(nonce)),
        rlp_encode_bytes(&u64_to_minimal_be(max_priority_fee_per_gas)),
        rlp_encode_bytes(&u64_to_minimal_be(max_fee_per_gas)),
        rlp_encode_bytes(&u64_to_minimal_be(gas_limit)),
        rlp_encode_bytes(to),
        rlp_encode_bytes(&u256_to_minimal_be(value)),
        rlp_encode_bytes(data),
        rlp_encode_list(&[]), // access_list (empty)
    ];

    let unsigned_rlp = rlp_encode_list(&items);

    // Hash = keccak256(0x02 || unsigned_rlp)
    let mut to_hash = vec![0x02u8];
    to_hash.extend_from_slice(&unsigned_rlp);
    let tx_hash = keccak256(&to_hash);

    // Sign with secp256k1
    let (signature, recovery_id) = private_key
        .sign_prehash_recoverable(&tx_hash)
        .map_err(|e| format!("Transaction signing failed: {}", e))?;

    let sig_bytes = signature.to_bytes();
    let r = &sig_bytes[..32];
    let s = &sig_bytes[32..];
    let v = recovery_id.to_byte(); // 0 or 1

    // Signed tx: 0x02 || RLP([chain_id, nonce, max_priority_fee, max_fee, gas, to, value, data, access_list, v, r, s])
    // Note: v is encoded as an integer (0 = empty bytes, 1 = [0x01]), r and s strip leading zeros
    let mut signed_items = items;
    signed_items.push(rlp_encode_bytes(&u64_to_minimal_be(v as u64)));
    signed_items.push(rlp_encode_bytes(&strip_leading_zeros(r)));
    signed_items.push(rlp_encode_bytes(&strip_leading_zeros(s)));

    let signed_rlp = rlp_encode_list(&signed_items);

    let mut result = vec![0x02u8];
    result.extend_from_slice(&signed_rlp);
    Ok(result)
}

// ── JSON-RPC Helpers ───────────────────────────────────────────────────

async fn rpc_call(
    rpc_url: &str,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": 1
    });

    let resp = client
        .post(rpc_url)
        .json(&body)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("RPC request failed: {}", e))?;

    let result: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("RPC response parse error: {}", e))?;

    if let Some(error) = result.get("error") {
        return Err(format!("RPC error: {}", error));
    }

    result.get("result")
        .cloned()
        .ok_or_else(|| "RPC response missing 'result' field".into())
}

/// Get ETH balance of an address
async fn eth_get_balance(rpc_url: &str, address: &str) -> Result<String, String> {
    let result = rpc_call(rpc_url, "eth_getBalance", serde_json::json!([address, "latest"])).await?;
    result.as_str().map(String::from).ok_or("Invalid balance result".into())
}

/// Call a contract (read-only)
async fn eth_call(rpc_url: &str, to: &str, data: &[u8]) -> Result<String, String> {
    let result = rpc_call(rpc_url, "eth_call", serde_json::json!([
        { "to": to, "data": hex_encode(data) },
        "latest"
    ])).await?;
    result.as_str().map(String::from).ok_or("Invalid eth_call result".into())
}

/// Get the next nonce for an address
async fn eth_get_transaction_count(rpc_url: &str, address: &str) -> Result<u64, String> {
    let result = rpc_call(rpc_url, "eth_getTransactionCount", serde_json::json!([address, "latest"])).await?;
    let hex = result.as_str().ok_or("Invalid nonce result")?;
    u64::from_str_radix(hex.strip_prefix("0x").unwrap_or(hex), 16)
        .map_err(|e| format!("Parse nonce: {}", e))
}

/// Get current gas fees (EIP-1559)
async fn get_gas_fees(rpc_url: &str) -> Result<(u64, u64), String> {
    // Get base fee from latest block
    let block = rpc_call(rpc_url, "eth_getBlockByNumber", serde_json::json!(["latest", false])).await?;
    let base_fee_hex = block.get("baseFeePerGas")
        .and_then(|v| v.as_str())
        .ok_or("Missing baseFeePerGas")?;
    let base_fee = u64::from_str_radix(base_fee_hex.strip_prefix("0x").unwrap_or(base_fee_hex), 16)
        .map_err(|e| format!("Parse base fee: {}", e))?;

    // Priority fee: reasonable default of 1.5 gwei
    let max_priority_fee = 1_500_000_000u64; // 1.5 gwei

    // Max fee = 2 * base_fee + priority fee (gives room for next block)
    let max_fee = base_fee * 2 + max_priority_fee;

    Ok((max_priority_fee, max_fee))
}

/// Estimate gas for a transaction
async fn eth_estimate_gas(
    rpc_url: &str,
    from: &str,
    to: &str,
    data: &[u8],
    value: &str,
) -> Result<u64, String> {
    let result = rpc_call(rpc_url, "eth_estimateGas", serde_json::json!([{
        "from": from,
        "to": to,
        "data": hex_encode(data),
        "value": value
    }])).await?;
    let hex = result.as_str().ok_or("Invalid gas estimate")?;
    let estimate = u64::from_str_radix(hex.strip_prefix("0x").unwrap_or(hex), 16)
        .map_err(|e| format!("Parse gas estimate: {}", e))?;
    // Add 20% buffer
    Ok(estimate * 120 / 100)
}

/// Broadcast a signed transaction
async fn eth_send_raw_transaction(rpc_url: &str, signed_tx: &[u8]) -> Result<String, String> {
    let result = rpc_call(rpc_url, "eth_sendRawTransaction", serde_json::json!([hex_encode(signed_tx)])).await?;
    result.as_str().map(String::from).ok_or("Invalid tx hash result".into())
}

/// Get chain ID
async fn eth_chain_id(rpc_url: &str) -> Result<u64, String> {
    let result = rpc_call(rpc_url, "eth_chainId", serde_json::json!([])).await?;
    let hex = result.as_str().ok_or("Invalid chain ID")?;
    u64::from_str_radix(hex.strip_prefix("0x").unwrap_or(hex), 16)
        .map_err(|e| format!("Parse chain ID: {}", e))
}

/// Get transaction receipt (to check if tx was mined)
async fn eth_get_transaction_receipt(rpc_url: &str, tx_hash: &str) -> Result<Option<serde_json::Value>, String> {
    let result = rpc_call(rpc_url, "eth_getTransactionReceipt", serde_json::json!([tx_hash])).await?;
    if result.is_null() { Ok(None) } else { Ok(Some(result)) }
}

/// Chunked eth_getLogs — splits large block ranges into smaller chunks to avoid
/// RPC provider limits (many free tiers limit to 500-2000 blocks per request).
/// Returns all matching logs combined from all chunks.
async fn chunked_get_logs(
    rpc_url: &str,
    address: Option<&str>,
    from_block: u64,
    to_block: u64,
    topics: Vec<Option<serde_json::Value>>,
    chunk_size: u64,
) -> Result<Vec<serde_json::Value>, String> {
    let mut all_logs: Vec<serde_json::Value> = Vec::new();
    let mut chunk_from = from_block;

    while chunk_from <= to_block {
        let chunk_to = std::cmp::min(chunk_from + chunk_size - 1, to_block);
        let from_hex = format!("0x{:x}", chunk_from);
        let to_hex = format!("0x{:x}", chunk_to);

        let mut filter = serde_json::json!({
            "fromBlock": from_hex,
            "toBlock": to_hex,
            "topics": topics,
        });
        if let Some(addr) = address {
            filter["address"] = serde_json::json!(addr);
        }

        let result = rpc_call(rpc_url, "eth_getLogs", serde_json::json!([filter])).await?;
        if let Some(logs) = result.as_array() {
            all_logs.extend(logs.iter().cloned());
        }

        chunk_from = chunk_to + 1;
    }

    Ok(all_logs)
}

// ── Token Helpers ──────────────────────────────────────────────────────

/// Resolve a token symbol or address to (address, decimals)
fn resolve_token(symbol_or_address: &str) -> Result<(String, u8), String> {
    let input = symbol_or_address.trim().to_uppercase();

    // Check known tokens by symbol
    for (sym, addr, dec) in KNOWN_TOKENS {
        if input == *sym {
            return Ok((addr.to_string(), *dec));
        }
    }

    // Check if it's an address
    let lower = symbol_or_address.trim().to_lowercase();
    if lower.starts_with("0x") && lower.len() == 42 {
        // Unknown token — assume 18 decimals (caller can override)
        return Ok((symbol_or_address.trim().to_string(), 18));
    }

    Err(format!(
        "Unknown token '{}'. Use a known symbol ({}) or provide the ERC-20 contract address.",
        symbol_or_address,
        KNOWN_TOKENS.iter().map(|(s, _, _)| *s).collect::<Vec<_>>().join(", ")
    ))
}

/// For swaps, if token_in is "ETH" we need to use WETH as the Uniswap input
fn resolve_for_swap(symbol_or_address: &str) -> Result<(String, u8, bool), String> {
    let input = symbol_or_address.trim().to_uppercase();
    if input == "ETH" {
        // Swap uses WETH but sends ETH value
        Ok((WETH_ADDRESS.to_string(), 18, true))
    } else {
        let (addr, dec) = resolve_token(symbol_or_address)?;
        Ok((addr, dec, false))
    }
}

// ── Tool Execute Functions ─────────────────────────────────────────────

/// Create a new Ethereum wallet and store the private key in the vault
pub async fn execute_dex_wallet_create(
    _args: &serde_json::Value,
    creds: &HashMap<String, String>,
    app_handle: &tauri::AppHandle,
) -> Result<String, String> {
    // Check if wallet already exists
    if creds.contains_key("DEX_PRIVATE_KEY") && creds.contains_key("DEX_WALLET_ADDRESS") {
        let addr = creds.get("DEX_WALLET_ADDRESS").unwrap();
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

    let state = app_handle.try_state::<crate::engine::commands::EngineState>()
        .ok_or("Engine state not available")?;
    let vault_key = crate::engine::skills::get_vault_key()?;

    let encrypted_key = crate::engine::skills::encrypt_credential(&private_key_hex, &vault_key);
    state.store.set_skill_credential("dex", "DEX_PRIVATE_KEY", &encrypted_key)?;

    let encrypted_addr = crate::engine::skills::encrypt_credential(&address, &vault_key);
    state.store.set_skill_credential("dex", "DEX_WALLET_ADDRESS", &encrypted_addr)?;

    info!("[dex] Created new wallet: {}", address);

    let chain_name = if let Some(rpc_url) = creds.get("DEX_RPC_URL") {
        match eth_chain_id(rpc_url).await {
            Ok(1) => "Ethereum Mainnet",
            Ok(5) => "Goerli Testnet",
            Ok(11155111) => "Sepolia Testnet",
            Ok(137) => "Polygon",
            Ok(42161) => "Arbitrum One",
            Ok(10) => "Optimism",
            Ok(8453) => "Base",
            Ok(id) => return Ok(format!(
                "✅ New wallet created!\n\nAddress: {}\nChain ID: {}\n\n⚠️ This wallet has zero balance. Send ETH to this address to fund it before trading.\n\n🔒 Private key is encrypted and stored in your OS keychain vault. The AI agent never sees it.",
                address, id
            )),
            Err(_) => "Unknown",
        }
    } else {
        "Not connected (configure RPC URL)"
    };

    Ok(format!(
        "✅ New wallet created!\n\nAddress: {}\nNetwork: {}\n\n⚠️ This wallet has zero balance. Send ETH to this address to fund it before trading.\n\n🔒 Private key is encrypted and stored in your OS keychain vault. The AI agent never sees it.",
        address, chain_name
    ))
}

/// Check ETH and ERC-20 token balances
pub async fn execute_dex_balance(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> Result<String, String> {
    let rpc_url = creds.get("DEX_RPC_URL").ok_or("Missing DEX_RPC_URL. Configure your RPC endpoint (Infura/Alchemy) in Settings → Skills → DEX Trading.")?;
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
            match eth_call(rpc_url, addr, &calldata).await {
                Ok(result) => {
                    if let Ok(balance) = raw_to_amount(&result, *dec) {
                        if balance != "0" {
                            output.push_str(&format!("{}: {}\n", sym, balance));
                        }
                    }
                }
                Err(_) => {} // Skip tokens that fail (might not exist on this chain)
            }
        }
    }

    Ok(output)
}

/// Get a swap quote from Uniswap V3 Quoter
pub async fn execute_dex_quote(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> Result<String, String> {
    let rpc_url = creds.get("DEX_RPC_URL").ok_or("Missing DEX_RPC_URL")?;
    let token_in_sym = args["token_in"].as_str().ok_or("dex_quote: missing 'token_in'")?;
    let token_out_sym = args["token_out"].as_str().ok_or("dex_quote: missing 'token_out'")?;
    let amount = args["amount"].as_str().ok_or("dex_quote: missing 'amount'")?;

    let (token_in_addr, token_in_dec, _is_eth) = resolve_for_swap(token_in_sym)?;
    let (token_out_addr, token_out_dec, _) = resolve_for_swap(token_out_sym)?;

    let fee_tier = args.get("fee_tier")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_FEE_TIER) as u32;

    // Convert amount to raw units
    let amount_raw = amount_to_raw(amount, token_in_dec)?;
    let amount_u256 = parse_u256_decimal(&amount_raw)?;

    let token_in_bytes = parse_address(&token_in_addr)?;
    let token_out_bytes = parse_address(&token_out_addr)?;
    let weth_bytes = parse_address(WETH_ADDRESS)?;

    // Try single-hop first, then multi-hop through WETH if direct pool doesn't exist
    let mut used_multihop = false;
    let result = {
        let single_calldata = encode_quote_exact_input_single(
            &token_in_bytes,
            &token_out_bytes,
            &amount_u256,
            fee_tier,
        );
        match eth_call(rpc_url, UNISWAP_QUOTER_V2, &single_calldata).await {
            Ok(r) => Ok(r),
            Err(_) if token_in_bytes != weth_bytes && token_out_bytes != weth_bytes => {
                // Try multi-hop: tokenIn → WETH → tokenOut
                info!("[dex] Single-hop quote failed, trying multi-hop through WETH");
                used_multihop = true;
                let path = build_multihop_path(
                    &[&token_in_bytes, &weth_bytes, &token_out_bytes],
                    &[fee_tier, fee_tier],
                );
                let multi_calldata = encode_quote_exact_input(&path, &amount_u256);
                eth_call(rpc_url, UNISWAP_QUOTER_V2, &multi_calldata).await
                    .map_err(|e| format!("Both single-hop and multi-hop (via WETH) quotes failed: {}", e))
            }
            Err(e) => Err(e),
        }
    }?;

    // The quoter returns (amountOut, sqrtPriceX96After, initializedTicksCrossed, gasEstimate)
    // amountOut is the first 32 bytes
    let result_bytes = hex_decode(&result)?;
    if result_bytes.len() < 32 {
        return Err(format!("Unexpected quoter response length: {} bytes", result_bytes.len()));
    }

    let amount_out_bytes: [u8; 32] = result_bytes[..32].try_into().unwrap();
    let amount_out_hex = hex_encode(&amount_out_bytes);
    let amount_out = raw_to_amount(&amount_out_hex, token_out_dec)?;

    // Calculate price
    let in_f64: f64 = amount.parse().unwrap_or(0.0);
    let out_f64: f64 = amount_out.parse().unwrap_or(0.0);
    let price = if in_f64 > 0.0 { out_f64 / in_f64 } else { 0.0 };

    let slippage_bps = args.get("slippage_bps")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_SLIPPAGE_BPS);

    let min_out = out_f64 * (10000.0 - slippage_bps as f64) / 10000.0;

    let route_info = if used_multihop {
        format!("Route: {} → WETH → {} (multi-hop)", token_in_sym.to_uppercase(), token_out_sym.to_uppercase())
    } else {
        format!("Route: {} → {} (direct)", token_in_sym.to_uppercase(), token_out_sym.to_uppercase())
    };

    Ok(format!(
        "Swap Quote: {} {} → {} {}\n\nInput: {} {}\nExpected Output: {} {}\nMinimum Output ({}% slippage): {:.6} {}\nExchange Rate: 1 {} = {:.6} {}\n{}\nFee Tier: {}%\n\nUse dex_swap to execute this trade.",
        amount, token_in_sym.to_uppercase(),
        amount_out, token_out_sym.to_uppercase(),
        amount, token_in_sym.to_uppercase(),
        amount_out, token_out_sym.to_uppercase(),
        slippage_bps as f64 / 100.0,
        min_out, token_out_sym.to_uppercase(),
        token_in_sym.to_uppercase(), price, token_out_sym.to_uppercase(),
        route_info,
        fee_tier as f64 / 10000.0,
    ))
}

/// Execute a token swap on Uniswap V3
pub async fn execute_dex_swap(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> Result<String, String> {
    let rpc_url = creds.get("DEX_RPC_URL").ok_or("Missing DEX_RPC_URL")?;
    let wallet_address = creds.get("DEX_WALLET_ADDRESS").ok_or("No wallet. Use dex_wallet_create first.")?;
    let private_key_hex = creds.get("DEX_PRIVATE_KEY").ok_or("Missing private key")?;

    let token_in_sym = args["token_in"].as_str().ok_or("dex_swap: missing 'token_in'")?;
    let token_out_sym = args["token_out"].as_str().ok_or("dex_swap: missing 'token_out'")?;
    let amount = args["amount"].as_str().ok_or("dex_swap: missing 'amount'")?;
    let _reason = args["reason"].as_str().unwrap_or("swap");

    let slippage_bps = args.get("slippage_bps")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_SLIPPAGE_BPS);

    if slippage_bps > MAX_SLIPPAGE_BPS {
        return Err(format!("Slippage {}bps exceeds maximum allowed {}bps ({}%)", slippage_bps, MAX_SLIPPAGE_BPS, MAX_SLIPPAGE_BPS as f64 / 100.0));
    }

    let fee_tier = args.get("fee_tier")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_FEE_TIER) as u32;

    let (token_in_addr, token_in_dec, is_eth_in) = resolve_for_swap(token_in_sym)?;
    let (token_out_addr, token_out_dec, _) = resolve_for_swap(token_out_sym)?;

    let amount_raw = amount_to_raw(amount, token_in_dec)?;
    let amount_u256 = parse_u256_decimal(&amount_raw)?;

    let token_in_bytes = parse_address(&token_in_addr)?;
    let token_out_bytes = parse_address(&token_out_addr)?;
    let wallet_bytes = parse_address(wallet_address)?;

    info!("[dex] Swap: {} {} → {} (wallet: {})", amount, token_in_sym, token_out_sym, wallet_address);

    // Step 1: Get quote for minimum output calculation — try single-hop, fall back to multi-hop via WETH
    let weth_bytes = parse_address(WETH_ADDRESS)?;
    let mut use_multihop = false;
    let expected_out: [u8; 32] = {
        let single_calldata = encode_quote_exact_input_single(
            &token_in_bytes,
            &token_out_bytes,
            &amount_u256,
            fee_tier,
        );
        match eth_call(rpc_url, UNISWAP_QUOTER_V2, &single_calldata).await {
            Ok(r) => {
                let qb = hex_decode(&r)?;
                if qb.len() < 32 { return Err("Invalid quoter response".into()); }
                qb[..32].try_into().unwrap()
            },
            Err(_) if token_in_bytes != weth_bytes && token_out_bytes != weth_bytes => {
                info!("[dex] Single-hop quote failed, trying multi-hop through WETH");
                use_multihop = true;
                let path = build_multihop_path(
                    &[&token_in_bytes, &weth_bytes, &token_out_bytes],
                    &[fee_tier, fee_tier],
                );
                let multi_calldata = encode_quote_exact_input(&path, &amount_u256);
                let r = eth_call(rpc_url, UNISWAP_QUOTER_V2, &multi_calldata).await
                    .map_err(|e| format!("Both single-hop and multi-hop quotes failed: {}", e))?;
                let qb = hex_decode(&r)?;
                if qb.len() < 32 { return Err("Invalid quoter response".into()); }
                qb[..32].try_into().unwrap()
            },
            Err(e) => return Err(e),
        }
    };

    // Apply slippage to get minimum output
    let expected_out_hex = hex_encode(&expected_out);
    let expected_out_f64: f64 = raw_to_amount(&expected_out_hex, token_out_dec)?.parse().unwrap_or(0.0);
    let min_out_f64 = expected_out_f64 * (10000.0 - slippage_bps as f64) / 10000.0;
    let min_out_raw = amount_to_raw(&format!("{:.width$}", min_out_f64, width = token_out_dec as usize), token_out_dec)?;
    let min_out_u256 = parse_u256_decimal(&min_out_raw)?;

    // Step 2: If not ETH, check and set token approval
    if !is_eth_in {
        let router_bytes = parse_address(UNISWAP_SWAP_ROUTER_02)?;
        let allowance_data = encode_allowance(&wallet_bytes, &router_bytes);
        let allowance_result = eth_call(rpc_url, &token_in_addr, &allowance_data).await?;
        let allowance_bytes = hex_decode(&allowance_result)?;

        // Check if allowance is sufficient
        let mut needs_approval = true;
        if allowance_bytes.len() >= 32 {
            // Compare: if allowance >= amount, no approval needed
            let allowance_slice: [u8; 32] = allowance_bytes[..32].try_into().unwrap();
            needs_approval = allowance_slice < amount_u256;
        }

        if needs_approval {
            info!("[dex] Approving token {} for router", token_in_addr);
            let max_approval = [0xffu8; 32]; // type(uint256).max
            let approve_data = encode_approve(&router_bytes, &max_approval);

            let pk_bytes = hex_decode(private_key_hex)?;
            let signing_key = k256::ecdsa::SigningKey::from_slice(&pk_bytes)
                .map_err(|e| format!("Invalid private key: {}", e))?;

            let chain_id = eth_chain_id(rpc_url).await?;
            let nonce = eth_get_transaction_count(rpc_url, wallet_address).await?;
            let (priority_fee, max_fee) = get_gas_fees(rpc_url).await?;
            let gas = eth_estimate_gas(rpc_url, wallet_address, &token_in_addr, &approve_data, "0x0").await?;

            let mut token_in_addr_bytes = [0u8; 20];
            token_in_addr_bytes.copy_from_slice(&hex_decode(&token_in_addr)?[..20]);

            let signed_approve = sign_eip1559_transaction(
                chain_id, nonce, priority_fee, max_fee, gas,
                &token_in_addr_bytes, &[0u8; 32], &approve_data, &signing_key,
            )?;

            let approve_hash = eth_send_raw_transaction(rpc_url, &signed_approve).await?;
            info!("[dex] Approval tx: {}", approve_hash);

            // Wait for approval to be mined (poll for up to 60 seconds)
            for _ in 0..30 {
                tokio::time::sleep(Duration::from_secs(2)).await;
                if let Ok(Some(receipt)) = eth_get_transaction_receipt(rpc_url, &approve_hash).await {
                    let status = receipt.get("status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("0x0");
                    if status == "0x1" {
                        info!("[dex] Token approval confirmed");
                        break;
                    } else {
                        return Err(format!("Token approval transaction failed (reverted). Tx: {}", approve_hash));
                    }
                }
            }
        }
    }

    // Step 3: Build the swap transaction (single-hop or multi-hop as determined by quote)
    let swap_data = if use_multihop {
        let path = build_multihop_path(
            &[&token_in_bytes, &weth_bytes, &token_out_bytes],
            &[fee_tier, fee_tier],
        );
        encode_exact_input(
            &path,
            &wallet_bytes,
            &amount_u256,
            &min_out_u256,
        )
    } else {
        encode_exact_input_single(
            &token_in_bytes,
            &token_out_bytes,
            fee_tier,
            &wallet_bytes,
            &amount_u256,
            &min_out_u256,
        )
    };

    let pk_bytes = hex_decode(private_key_hex)?;
    let signing_key = k256::ecdsa::SigningKey::from_slice(&pk_bytes)
        .map_err(|e| format!("Invalid private key: {}", e))?;

    let chain_id = eth_chain_id(rpc_url).await?;
    let nonce = eth_get_transaction_count(rpc_url, wallet_address).await?;
    let (priority_fee, max_fee) = get_gas_fees(rpc_url).await?;

    // Value is the ETH amount if swapping from ETH, otherwise 0
    let value = if is_eth_in { amount_u256 } else { [0u8; 32] };
    let value_hex = if is_eth_in { u256_to_quantity_hex(&value) } else { "0x0".into() };

    let router_bytes = parse_address(UNISWAP_SWAP_ROUTER_02)?;
    let gas = eth_estimate_gas(rpc_url, wallet_address, UNISWAP_SWAP_ROUTER_02, &swap_data, &value_hex).await
        .unwrap_or(300_000); // fallback gas limit for swaps

    let signed_tx = sign_eip1559_transaction(
        chain_id, nonce, priority_fee, max_fee, gas,
        &router_bytes, &value, &swap_data, &signing_key,
    )?;

    // Step 4: Broadcast
    let tx_hash = eth_send_raw_transaction(rpc_url, &signed_tx).await?;
    info!("[dex] Swap tx broadcast: {}", tx_hash);

    // Step 5: Wait for confirmation (up to 2 minutes)
    let mut confirmed = false;
    let mut final_status = "pending";
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_secs(2)).await;
        match eth_get_transaction_receipt(rpc_url, &tx_hash).await {
            Ok(Some(receipt)) => {
                let status = receipt.get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0x0");
                if status == "0x1" {
                    confirmed = true;
                    final_status = "confirmed";
                } else {
                    final_status = "reverted";
                }
                break;
            }
            Ok(None) => continue, // Not mined yet
            Err(_) => continue,
        }
    }

    let network = match chain_id {
        1 => "https://etherscan.io/tx/",
        5 => "https://goerli.etherscan.io/tx/",
        11155111 => "https://sepolia.etherscan.io/tx/",
        137 => "https://polygonscan.com/tx/",
        42161 => "https://arbiscan.io/tx/",
        10 => "https://optimistic.etherscan.io/tx/",
        8453 => "https://basescan.org/tx/",
        _ => "https://etherscan.io/tx/",
    };

    let expected_out_display = raw_to_amount(&expected_out_hex, token_out_dec).unwrap_or("?".into());

    Ok(format!(
        "{} Swap {}\n\n{} {} → ~{} {}\nSlippage tolerance: {}%\nTransaction: {}{}\nStatus: {}\n\n{}",
        if confirmed { "✅" } else { "⚠️" },
        if confirmed { "Confirmed" } else { "Submitted" },
        amount, token_in_sym.to_uppercase(),
        expected_out_display, token_out_sym.to_uppercase(),
        slippage_bps as f64 / 100.0,
        network, tx_hash,
        final_status,
        if !confirmed && final_status == "pending" {
            "Transaction is still pending. Check the explorer link for status."
        } else if final_status == "reverted" {
            "Transaction reverted! The swap may have failed due to slippage or liquidity issues. Your tokens are safe."
        } else { "" },
    ))
}

/// Check multiple token balances at once
pub async fn execute_dex_portfolio(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> Result<String, String> {
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
        match eth_call(rpc_url, addr, &calldata).await {
            Ok(result) => {
                if let Ok(balance) = raw_to_amount(&result, *dec) {
                    if balance != "0" {
                        output.push_str(&format!("  {}: {}\n", sym, balance));
                        has_tokens = true;
                    }
                }
            }
            Err(_) => {}
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
    match eth_chain_id(rpc_url).await {
        Ok(id) => {
            let chain = match id {
                1 => "Ethereum Mainnet",
                5 => "Goerli Testnet",
                11155111 => "Sepolia Testnet",
                137 => "Polygon",
                42161 => "Arbitrum One",
                10 => "Optimism",
                8453 => "Base",
                _ => "Unknown",
            };
            output.push_str(&format!("\nNetwork: {} (chain ID {})\n", chain, id));
        }
        Err(_) => {}
    }

    Ok(output)
}

// ── Token Analysis Tools ───────────────────────────────────────────────

/// ABI-encode ERC-20 name() call
fn encode_name() -> Vec<u8> {
    function_selector("name()").to_vec()
}

/// ABI-encode ERC-20 symbol() call
fn encode_symbol() -> Vec<u8> {
    function_selector("symbol()").to_vec()
}

/// ABI-encode ERC-20 decimals() call
fn encode_decimals() -> Vec<u8> {
    function_selector("decimals()").to_vec()
}

/// ABI-encode ERC-20 totalSupply() call
fn encode_total_supply() -> Vec<u8> {
    function_selector("totalSupply()").to_vec()
}

/// ABI-encode ERC-20 owner() call (common in tokens)
fn encode_owner() -> Vec<u8> {
    function_selector("owner()").to_vec()
}

/// Decode an ABI-encoded string (dynamic type at offset 0)
fn decode_abi_string(hex_data: &str) -> Result<String, String> {
    let bytes = hex_decode(hex_data)?;
    if bytes.len() < 64 {
        // Might be a non-standard response — try UTF-8 directly from bytes32
        let trimmed: Vec<u8> = bytes.iter().copied().filter(|&b| b != 0).collect();
        return String::from_utf8(trimmed).map_err(|_| "Cannot decode string".into());
    }
    // Standard ABI: offset (32 bytes) + length (32 bytes) + data
    let offset_bytes: [u8; 32] = bytes[..32].try_into().map_err(|_| "Bad offset")?;
    let offset = u32::from_be_bytes(offset_bytes[28..32].try_into().unwrap()) as usize;

    if offset + 32 > bytes.len() {
        // Try bytes32 fallback
        let trimmed: Vec<u8> = bytes[..32].iter().copied().filter(|&b| b != 0).collect();
        return String::from_utf8(trimmed).map_err(|_| "Cannot decode string".into());
    }

    let len_start = offset;
    let len_bytes: [u8; 32] = bytes[len_start..len_start + 32].try_into().map_err(|_| "Bad length")?;
    let len = u32::from_be_bytes(len_bytes[28..32].try_into().unwrap()) as usize;

    let data_start = len_start + 32;
    if data_start + len > bytes.len() {
        return Err("String data exceeds response".into());
    }

    String::from_utf8(bytes[data_start..data_start + len].to_vec())
        .map_err(|_| "Invalid UTF-8 in string".into())
}

/// Get comprehensive token info by reading on-chain ERC-20 data directly via RPC.
/// No website scraping needed — this queries the blockchain.
pub async fn execute_dex_token_info(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> Result<String, String> {
    let rpc_url = creds.get("DEX_RPC_URL").ok_or("Missing DEX_RPC_URL")?;
    let token_address = args["token_address"].as_str()
        .ok_or("dex_token_info: missing 'token_address'. Provide the ERC-20 contract address.")?;

    // Validate address format
    let addr_clean = token_address.trim();
    if !addr_clean.starts_with("0x") || addr_clean.len() != 42 {
        return Err(format!("Invalid contract address format: '{}'. Must be 0x + 40 hex chars.", addr_clean));
    }

    let mut output = format!("Token Analysis: {}\n\n", addr_clean);

    // 1. Name
    match eth_call(rpc_url, addr_clean, &encode_name()).await {
        Ok(result) => {
            if let Ok(name) = decode_abi_string(&result) {
                output.push_str(&format!("  Name: {}\n", name));
            }
        }
        Err(_) => { output.push_str("  Name: Could not read (non-standard contract)\n"); }
    }

    // 2. Symbol
    match eth_call(rpc_url, addr_clean, &encode_symbol()).await {
        Ok(result) => {
            if let Ok(symbol) = decode_abi_string(&result) {
                output.push_str(&format!("  Symbol: {}\n", symbol));
            }
        }
        Err(_) => { output.push_str("  Symbol: Could not read\n"); }
    }

    // 3. Decimals
    let mut token_decimals = 18u8;
    match eth_call(rpc_url, addr_clean, &encode_decimals()).await {
        Ok(result) => {
            let bytes = hex_decode(&result).unwrap_or_default();
            if bytes.len() >= 32 {
                token_decimals = bytes[31];
                output.push_str(&format!("  Decimals: {}\n", token_decimals));
            }
        }
        Err(_) => { output.push_str("  Decimals: 18 (assumed)\n"); }
    }

    // 4. Total Supply
    match eth_call(rpc_url, addr_clean, &encode_total_supply()).await {
        Ok(result) => {
            if let Ok(supply) = raw_to_amount(&result, token_decimals) {
                output.push_str(&format!("  Total Supply: {}\n", supply));
            }
        }
        Err(_) => { output.push_str("  Total Supply: Could not read\n"); }
    }

    // 5. Owner (if the contract has an owner function — indicates centralization risk)
    match eth_call(rpc_url, addr_clean, &encode_owner()).await {
        Ok(result) => {
            let bytes = hex_decode(&result).unwrap_or_default();
            if bytes.len() >= 32 {
                let owner_addr = &bytes[12..32];
                let zero_addr = [0u8; 20];
                if owner_addr == zero_addr {
                    output.push_str("  Owner: Renounced (0x0) [SAFE]\n");
                } else {
                    let owner_hex = eip55_checksum(owner_addr);
                    output.push_str(&format!("  Owner: {} [WARNING: not renounced — owner can modify contract]\n", owner_hex));
                }
            }
        }
        Err(_) => { output.push_str("  Owner: No owner() function (may be immutable) [SAFE]\n"); }
    }

    // 6. Contract code size (is it actually a contract?)
    let code_result = rpc_call(rpc_url, "eth_getCode", serde_json::json!([addr_clean, "latest"])).await;
    match code_result {
        Ok(code) => {
            let code_str = code.as_str().unwrap_or("0x");
            let code_len = (code_str.len() - 2) / 2; // subtract "0x", divide by 2 for bytes
            if code_len == 0 {
                output.push_str("  Contract: NO CODE — this is an EOA (wallet), not a token!\n");
            } else {
                output.push_str(&format!("  Contract: {} bytes of bytecode [OK]\n", code_len));
            }
        }
        Err(_) => {}
    }

    // 7. Check ETH balance of the contract
    match eth_get_balance(rpc_url, addr_clean).await {
        Ok(bal_hex) => {
            if let Ok(eth_bal) = raw_to_amount(&bal_hex, 18) {
                if eth_bal != "0" {
                    output.push_str(&format!("  Contract ETH balance: {} ETH\n", eth_bal));
                }
            }
        }
        Err(_) => {}
    }

    // 8. Check if the token can be quoted on Uniswap (basic swap viability)
    output.push_str("\n  Swap Viability:\n");

    let token_bytes = parse_address(addr_clean)?;
    let tiny_amount = parse_u256_decimal("1000000000000000")?; // 0.001 ETH in wei
    let weth_addr_bytes = parse_address(WETH_ADDRESS)?;

    for fee in &[3000u32, 10000, 500, 100] {
        let quote_data = encode_quote_exact_input_single(
            &weth_addr_bytes,
            &token_bytes,
            &tiny_amount,
            *fee,
        );

        match eth_call(rpc_url, UNISWAP_QUOTER_V2, &quote_data).await {
            Ok(result) => {
                let result_bytes = hex_decode(&result).unwrap_or_default();
                if result_bytes.len() >= 32 {
                    let amount_out: [u8; 32] = result_bytes[..32].try_into().unwrap();
                    let out_hex = hex_encode(&amount_out);
                    if let Ok(out_amount) = raw_to_amount(&out_hex, token_decimals) {
                        output.push_str(&format!("    Uniswap V3 pool found ({}% fee tier) [OK]\n", *fee as f64 / 10000.0));
                        output.push_str(&format!("    Quote: 0.001 WETH -> {} tokens\n", out_amount));

                        // Honeypot check: try reverse quote (can you SELL?)
                        if let Ok(sell_raw) = amount_to_raw(&out_amount, token_decimals) {
                            if let Ok(sell_u256) = parse_u256_decimal(&sell_raw) {
                                let reverse_quote = encode_quote_exact_input_single(
                                    &token_bytes,
                                    &weth_addr_bytes,
                                    &sell_u256,
                                    *fee,
                                );
                                match eth_call(rpc_url, UNISWAP_QUOTER_V2, &reverse_quote).await {
                                    Ok(rev_result) => {
                                        let rev_bytes = hex_decode(&rev_result).unwrap_or_default();
                                        if rev_bytes.len() >= 32 {
                                            let rev_out: [u8; 32] = rev_bytes[..32].try_into().unwrap();
                                            let rev_hex = hex_encode(&rev_out);
                                            if let Ok(rev_amount) = raw_to_amount(&rev_hex, 18) {
                                                let rev_f: f64 = rev_amount.parse().unwrap_or(0.0);
                                                let original = 0.001f64;
                                                let round_trip_loss = ((original - rev_f) / original * 100.0).abs();
                                                output.push_str(&format!("    SELL quote works: {} tokens -> {} WETH [OK]\n", out_amount, rev_amount));
                                                output.push_str(&format!("    Round-trip loss: {:.2}% (fees + slippage)\n", round_trip_loss));
                                                if round_trip_loss > 50.0 {
                                                    output.push_str("    [DANGER] HIGH ROUND-TRIP LOSS — possible honeypot or extreme tax\n");
                                                } else if round_trip_loss > 10.0 {
                                                    output.push_str("    [WARNING] Moderate tax detected — check tokenomics\n");
                                                } else {
                                                    output.push_str("    Normal fee range — not a honeypot [OK]\n");
                                                }
                                            }
                                        }
                                    }
                                    Err(_) => {
                                        output.push_str("    [DANGER] SELL BLOCKED — cannot get reverse quote. LIKELY HONEYPOT!\n");
                                    }
                                }
                            }
                        }
                        break; // Found a working pool, done
                    }
                }
            }
            Err(_) => {} // No pool at this fee tier, try next
        }
    }

    // 9. Chain info
    if let Ok(chain_id) = eth_chain_id(rpc_url).await {
        let chain = match chain_id {
            1 => "Ethereum Mainnet",
            8453 => "Base",
            42161 => "Arbitrum One",
            10 => "Optimism",
            137 => "Polygon",
            _ => "Unknown",
        };
        output.push_str(&format!("\n  Network: {} (chain ID {})\n", chain, chain_id));
    }

    Ok(output)
}

/// Perform automated safety checks on a token contract.
/// Simulates buy AND sell to detect honeypots, checks ownership, analyzes on-chain data.
pub async fn execute_dex_check_token(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> Result<String, String> {
    let rpc_url = creds.get("DEX_RPC_URL").ok_or("Missing DEX_RPC_URL")?;
    let token_address = args["token_address"].as_str()
        .ok_or("dex_check_token: missing 'token_address'")?;

    let addr_clean = token_address.trim();
    if !addr_clean.starts_with("0x") || addr_clean.len() != 42 {
        return Err(format!("Invalid address: '{}'", addr_clean));
    }

    let mut output = String::from("Token Safety Report\n\n");
    let mut risk_score = 0u32;
    let mut flags: Vec<String> = Vec::new();

    let token_bytes = parse_address(addr_clean)?;
    let weth_bytes = parse_address(WETH_ADDRESS)?;

    // Check 1: Is it actually a contract?
    let code_result = rpc_call(rpc_url, "eth_getCode", serde_json::json!([addr_clean, "latest"])).await;
    match code_result {
        Ok(code) => {
            let code_str = code.as_str().unwrap_or("0x");
            let code_len = (code_str.len() - 2) / 2;
            if code_len == 0 {
                output.push_str("FATAL: Address has no contract code — this is a wallet address, not a token!\n");
                return Ok(output);
            }
            output.push_str(&format!("[OK] Contract verified ({} bytes)\n", code_len));
        }
        Err(e) => { output.push_str(&format!("[WARNING] Could not check contract code: {}\n", e)); }
    }

    // Check 2: ERC-20 standard compliance
    let has_name = eth_call(rpc_url, addr_clean, &encode_name()).await.is_ok();
    let has_symbol = eth_call(rpc_url, addr_clean, &encode_symbol()).await.is_ok();
    let has_decimals = eth_call(rpc_url, addr_clean, &encode_decimals()).await.is_ok();
    let has_supply = eth_call(rpc_url, addr_clean, &encode_total_supply()).await.is_ok();

    if has_name && has_symbol && has_decimals && has_supply {
        output.push_str("[OK] ERC-20 standard compliant (name, symbol, decimals, totalSupply)\n");
    } else {
        output.push_str("[WARNING] Non-standard ERC-20 — missing some functions\n");
        risk_score += 2;
        flags.push("Non-standard ERC-20".into());
    }

    // Get token decimals
    let mut token_decimals = 18u8;
    if let Ok(result) = eth_call(rpc_url, addr_clean, &encode_decimals()).await {
        let bytes = hex_decode(&result).unwrap_or_default();
        if bytes.len() >= 32 { token_decimals = bytes[31]; }
    }

    // Check 3: Ownership
    match eth_call(rpc_url, addr_clean, &encode_owner()).await {
        Ok(result) => {
            let bytes = hex_decode(&result).unwrap_or_default();
            if bytes.len() >= 32 {
                let owner_addr = &bytes[12..32];
                if owner_addr == [0u8; 20] {
                    output.push_str("[OK] Ownership renounced (owner = 0x0)\n");
                } else {
                    let owner_hex = eip55_checksum(owner_addr);
                    output.push_str(&format!("[WARNING] Owner: {} — can potentially modify contract\n", owner_hex));
                    risk_score += 3;
                    flags.push("Owner not renounced".into());
                }
            }
        }
        Err(_) => {
            output.push_str("[OK] No owner() function — likely immutable\n");
        }
    }

    // Check 4: HONEYPOT TEST
    output.push_str("\nHoneypot Test:\n");
    let tiny_amount = parse_u256_decimal("1000000000000000")?; // 0.001 ETH

    let mut can_buy = false;
    let mut can_sell = false;

    for fee in &[3000u32, 10000, 500, 100] {
        let buy_quote = encode_quote_exact_input_single(&weth_bytes, &token_bytes, &tiny_amount, *fee);
        match eth_call(rpc_url, UNISWAP_QUOTER_V2, &buy_quote).await {
            Ok(result) => {
                let result_bytes = hex_decode(&result).unwrap_or_default();
                if result_bytes.len() >= 32 {
                    let out: [u8; 32] = result_bytes[..32].try_into().unwrap();
                    let out_hex = hex_encode(&out);
                    if let Ok(amount) = raw_to_amount(&out_hex, token_decimals) {
                        can_buy = true;
                        output.push_str(&format!("  [OK] BUY works: 0.001 WETH -> {} tokens ({}% fee)\n", amount, *fee as f64 / 10000.0));

                        // Try to sell
                        if let Ok(sell_raw) = amount_to_raw(&amount, token_decimals) {
                            if let Ok(sell_u256) = parse_u256_decimal(&sell_raw) {
                                let sell_quote = encode_quote_exact_input_single(&token_bytes, &weth_bytes, &sell_u256, *fee);
                                match eth_call(rpc_url, UNISWAP_QUOTER_V2, &sell_quote).await {
                                    Ok(rev) => {
                                        let rev_bytes = hex_decode(&rev).unwrap_or_default();
                                        if rev_bytes.len() >= 32 {
                                            let rev_out: [u8; 32] = rev_bytes[..32].try_into().unwrap();
                                            let rev_hex = hex_encode(&rev_out);
                                            if let Ok(rev_amount) = raw_to_amount(&rev_hex, 18) {
                                                can_sell = true;
                                                let rev_f: f64 = rev_amount.parse().unwrap_or(0.0);
                                                let loss_pct = ((0.001 - rev_f) / 0.001 * 100.0).abs();

                                                output.push_str(&format!("  [OK] SELL works: {} tokens -> {} WETH\n", amount, rev_amount));

                                                if loss_pct > 50.0 {
                                                    output.push_str(&format!("  [DANGER] EXTREME TAX: {:.1}% round-trip loss — PROBABLE HONEYPOT or >25% tax\n", loss_pct));
                                                    risk_score += 10;
                                                    flags.push(format!("Extreme tax: {:.1}%", loss_pct));
                                                } else if loss_pct > 20.0 {
                                                    output.push_str(&format!("  [WARNING] HIGH TAX: {:.1}% round-trip loss — likely 10%+ buy/sell tax\n", loss_pct));
                                                    risk_score += 5;
                                                    flags.push(format!("High tax: {:.1}%", loss_pct));
                                                } else if loss_pct > 5.0 {
                                                    output.push_str(&format!("  [WARNING] Moderate tax: {:.1}% round-trip loss\n", loss_pct));
                                                    risk_score += 2;
                                                    flags.push(format!("Tax: {:.1}%", loss_pct));
                                                } else {
                                                    output.push_str(&format!("  [OK] Normal: {:.1}% round-trip loss (just pool fees)\n", loss_pct));
                                                }
                                            }
                                        }
                                    }
                                    Err(_) => {
                                        output.push_str("  [DANGER] SELL FAILED — quoter reverted. LIKELY HONEYPOT!\n");
                                        risk_score += 15;
                                        flags.push("Sell blocked — honeypot".into());
                                    }
                                }
                            }
                        }
                        break;
                    }
                }
            }
            Err(_) => {} // Try next fee tier
        }
    }

    if !can_buy {
        output.push_str("  No Uniswap V3 liquidity pool found for this token\n");
        risk_score += 5;
        flags.push("No Uniswap V3 pool".into());
    }

    // Total supply
    if let Ok(result) = eth_call(rpc_url, addr_clean, &encode_total_supply()).await {
        if let Ok(total) = raw_to_amount(&result, token_decimals) {
            output.push_str(&format!("\nSupply: {} total tokens\n", total));
        }
    }

    // Final risk assessment
    output.push_str("\n────────────────────────────────────────\n");
    output.push_str(&format!("Risk Score: {}/30\n", risk_score.min(30)));

    if risk_score == 0 {
        output.push_str("LOW RISK — All checks passed\n");
    } else if risk_score <= 5 {
        output.push_str("MODERATE RISK — Some concerns, proceed with caution\n");
    } else if risk_score <= 10 {
        output.push_str("HIGH RISK — Significant red flags detected\n");
    } else {
        output.push_str("CRITICAL RISK — DO NOT TRADE — Multiple severe issues\n");
    }

    if !flags.is_empty() {
        output.push_str(&format!("\nFlags: {}\n", flags.join(", ")));
    }

    if !can_sell && can_buy {
        output.push_str("\nVERDICT: HONEYPOT — You can buy but CANNOT sell. Do NOT trade this token.\n");
    }

    Ok(output)
}

// ── Token Discovery ────────────────────────────────────────────────────

/// Search for tokens by name or symbol using the DexScreener public API.
/// Returns contract addresses, chain, price, volume, liquidity, and pair info.
/// This is a JSON REST API — no web scraping, no bot detection.
pub async fn execute_dex_search_token(
    args: &serde_json::Value,
    _creds: &HashMap<String, String>,
) -> Result<String, String> {
    let query = args["query"].as_str()
        .ok_or("dex_search_token: missing 'query'. Provide a token name or symbol (e.g. 'KIMCHI', 'pepe', 'uniswap').")?;

    let chain_filter = args["chain"].as_str().unwrap_or("");
    let max_results = args["max_results"].as_u64().unwrap_or(10).min(25) as usize;

    // Query DexScreener search API
    let url = format!("https://api.dexscreener.com/latest/dex/search?q={}", urlencoding(query));

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (compatible; PawAgent/1.0)")
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let response = client.get(&url)
        .send()
        .await
        .map_err(|e| format!("DexScreener API request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("DexScreener API returned status {}", response.status()));
    }

    let body: serde_json::Value = response.json().await
        .map_err(|e| format!("Failed to parse DexScreener response: {}", e))?;

    let pairs = body["pairs"].as_array()
        .ok_or("No pairs found in DexScreener response")?;

    if pairs.is_empty() {
        return Ok(format!("No results found for '{}'. Try a different search term.", query));
    }

    let mut output = format!("Search results for '{}'\n\n", query);
    let mut seen_tokens: HashMap<String, bool> = HashMap::new();
    let mut count = 0;

    for pair in pairs {
        if count >= max_results { break; }

        let chain_id = pair["chainId"].as_str().unwrap_or("unknown");

        // Apply chain filter if specified
        if !chain_filter.is_empty() {
            let filter_lower = chain_filter.to_lowercase();
            let chain_lower = chain_id.to_lowercase();
            if !chain_lower.contains(&filter_lower) {
                continue;
            }
        }

        let base_token = &pair["baseToken"];
        let quote_token = &pair["quoteToken"];
        let token_address = base_token["address"].as_str().unwrap_or("?");
        let token_name = base_token["name"].as_str().unwrap_or("?");
        let token_symbol = base_token["symbol"].as_str().unwrap_or("?");
        let quote_symbol = quote_token["symbol"].as_str().unwrap_or("?");

        // Deduplicate by token address + chain
        let dedup_key = format!("{}:{}", chain_id, token_address.to_lowercase());
        if seen_tokens.contains_key(&dedup_key) { continue; }
        seen_tokens.insert(dedup_key, true);

        let price_usd = pair["priceUsd"].as_str().unwrap_or("N/A");
        let pair_address = pair["pairAddress"].as_str().unwrap_or("?");
        let dex_id = pair["dexId"].as_str().unwrap_or("?");
        let url = pair["url"].as_str().unwrap_or("");

        output.push_str(&format!("{}. {} ({}) on {}\n", count + 1, token_name, token_symbol, chain_id));
        output.push_str(&format!("   Contract: {}\n", token_address));
        output.push_str(&format!("   Pair: {}/{} | DEX: {}\n", token_symbol, quote_symbol, dex_id));
        output.push_str(&format!("   Pair Address: {}\n", pair_address));
        output.push_str(&format!("   Price: ${}\n", price_usd));

        // Volume
        if let Some(volume) = pair["volume"].as_object() {
            let h24 = volume.get("h24").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let h6 = volume.get("h6").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let h1 = volume.get("h1").and_then(|v| v.as_f64()).unwrap_or(0.0);
            output.push_str(&format!("   Volume: 24h=${:.0} | 6h=${:.0} | 1h=${:.0}\n", h24, h6, h1));
        }

        // Price changes
        if let Some(price_change) = pair["priceChange"].as_object() {
            let h24 = price_change.get("h24").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let h6 = price_change.get("h6").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let h1 = price_change.get("h1").and_then(|v| v.as_f64()).unwrap_or(0.0);
            output.push_str(&format!("   Price Change: 24h={:+.1}% | 6h={:+.1}% | 1h={:+.1}%\n", h24, h6, h1));
        }

        // Liquidity
        if let Some(liq) = pair["liquidity"].as_object() {
            let usd = liq.get("usd").and_then(|v| v.as_f64()).unwrap_or(0.0);
            output.push_str(&format!("   Liquidity: ${:.0}\n", usd));
        }

        // FDV
        if let Some(fdv) = pair["fdv"].as_f64() {
            output.push_str(&format!("   FDV: ${:.0}\n", fdv));
        }

        // Pair creation time
        if let Some(created) = pair["pairCreatedAt"].as_u64() {
            let secs = created / 1000;
            let age_hrs = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .saturating_sub(secs)) / 3600;
            if age_hrs < 24 {
                output.push_str(&format!("   Age: {} hours old [NEW]\n", age_hrs));
            } else {
                output.push_str(&format!("   Age: {} days old\n", age_hrs / 24));
            }
        }

        if !url.is_empty() {
            output.push_str(&format!("   URL: {}\n", url));
        }

        output.push('\n');
        count += 1;
    }

    output.push_str(&format!("Showing {} of {} total pairs found.\n", count, pairs.len()));
    output.push_str("\nNext step: Use dex_check_token with the contract address to run safety checks before trading.\n");

    Ok(output)
}

/// Simple URL encoding for query parameters
fn urlencoding(s: &str) -> String {
    let mut result = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(b as char);
            }
            b' ' => result.push_str("%20"),
            _ => result.push_str(&format!("%{:02X}", b)),
        }
    }
    result
}

// ── Whale Monitoring ───────────────────────────────────────────────────

/// ERC-20 Transfer event topic: keccak256("Transfer(address,address,uint256)")
const TRANSFER_EVENT_TOPIC: &str = "0xddf252ad1be2c89b69c2b068fc378daa0952e8da11aeba5c4f27ead9083c756cc2";

/// Monitor a wallet address: show ETH balance, recent ERC-20 transfers (in/out),
/// and current holdings of known tokens. Use this to track alpha traders.
pub async fn execute_dex_watch_wallet(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> Result<String, String> {
    let rpc_url = creds.get("DEX_RPC_URL").ok_or("Missing DEX_RPC_URL")?;
    let wallet = args["wallet_address"].as_str()
        .ok_or("dex_watch_wallet: missing 'wallet_address'")?;
    let blocks_back = args["blocks_back"].as_u64().unwrap_or(1000); // ~3.3 hours on mainnet
    let addr_clean = wallet.trim();
    if !addr_clean.starts_with("0x") || addr_clean.len() != 42 {
        return Err(format!("Invalid wallet address: '{}'", addr_clean));
    }

    let mut output = format!("Wallet Monitor: {}\n\n", addr_clean);

    // 1. ETH balance
    match eth_get_balance(rpc_url, addr_clean).await {
        Ok(bal_hex) => {
            if let Ok(eth_bal) = raw_to_amount(&bal_hex, 18) {
                output.push_str(&format!("ETH Balance: {} ETH\n", eth_bal));
            }
        }
        Err(e) => { output.push_str(&format!("ETH Balance: error ({})\n", e)); }
    }

    // 2. Check known token balances
    let wallet_bytes = parse_address(addr_clean)?;
    output.push_str("\nToken Holdings:\n");
    let mut has_tokens = false;
    for (symbol, addr, decimals) in KNOWN_TOKENS {
        if *symbol == "ETH" { continue; }
        let token_bytes = match parse_address(addr) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let call_data = encode_balance_of(&wallet_bytes);
        if let Ok(result) = eth_call(rpc_url, addr, &call_data).await {
            if let Ok(amount) = raw_to_amount(&result, *decimals) {
                let amt_f: f64 = amount.parse().unwrap_or(0.0);
                if amt_f > 0.0 {
                    output.push_str(&format!("  {}: {}\n", symbol, amount));
                    has_tokens = true;
                }
            }
        }
    }

    // Also check user-specified tokens
    if let Some(extra_tokens) = args["tokens"].as_array() {
        for token_val in extra_tokens {
            if let Some(token_addr) = token_val.as_str() {
                let token_addr = token_addr.trim();
                if token_addr.starts_with("0x") && token_addr.len() == 42 {
                    let call_data = encode_balance_of(&wallet_bytes);
                    if let Ok(result) = eth_call(rpc_url, token_addr, &call_data).await {
                        // Try to get symbol
                        let symbol = match eth_call(rpc_url, token_addr, &encode_symbol()).await {
                            Ok(s) => decode_abi_string(&s).unwrap_or_else(|_| token_addr[..10].to_string()),
                            Err(_) => token_addr[..10].to_string(),
                        };
                        let decimals = match eth_call(rpc_url, token_addr, &encode_decimals()).await {
                            Ok(d) => {
                                let b = hex_decode(&d).unwrap_or_default();
                                if b.len() >= 32 { b[31] } else { 18 }
                            }
                            Err(_) => 18,
                        };
                        if let Ok(amount) = raw_to_amount(&result, decimals) {
                            let amt_f: f64 = amount.parse().unwrap_or(0.0);
                            if amt_f > 0.0 {
                                output.push_str(&format!("  {}: {}\n", symbol, amount));
                                has_tokens = true;
                            }
                        }
                    }
                }
            }
        }
    }

    if !has_tokens {
        output.push_str("  (no known token holdings)\n");
    }

    // 3. Get current block number
    let block_num = match rpc_call(rpc_url, "eth_blockNumber", serde_json::json!([])).await {
        Ok(val) => {
            let hex = val.as_str().unwrap_or("0x0");
            u64::from_str_radix(hex.trim_start_matches("0x"), 16).unwrap_or(0)
        }
        Err(e) => return Err(format!("Cannot get block number: {}", e)),
    };
    let from_block = block_num.saturating_sub(blocks_back);

    // 4. Scan Transfer events where this wallet is sender or receiver
    output.push_str(&format!("\nRecent Transfers (last {} blocks, ~{} to #{}):\n",
        blocks_back, from_block, block_num));

    // Pad wallet to 32 bytes for topic filter
    let wallet_topic = format!("0x000000000000000000000000{}", &addr_clean[2..].to_lowercase());

    // Outgoing transfers (wallet is sender = topic[1])
    let outgoing_logs = chunked_get_logs(
        rpc_url,
        None,
        from_block,
        block_num,
        vec![Some(serde_json::json!(TRANSFER_EVENT_TOPIC)), Some(serde_json::json!(wallet_topic))],
        500,
    ).await;

    // Incoming transfers (wallet is receiver = topic[2])
    let incoming_logs = chunked_get_logs(
        rpc_url,
        None,
        from_block,
        block_num,
        vec![Some(serde_json::json!(TRANSFER_EVENT_TOPIC)), None, Some(serde_json::json!(wallet_topic))],
        500,
    ).await;

    let mut transfers: Vec<(u64, String, String, String, String, String)> = Vec::new(); // (block, direction, token_addr, counterparty, amount_raw, symbol)

    // Process outgoing
    if let Ok(logs) = outgoing_logs {
        for log in logs.iter().take(50) {
            if let Some(parsed) = parse_transfer_log(log, "SELL/SEND", rpc_url).await {
                transfers.push(parsed);
            }
        }
    }

    // Process incoming
    if let Ok(logs) = incoming_logs {
        for log in logs.iter().take(50) {
            if let Some(parsed) = parse_transfer_log(log, "BUY/RECV", rpc_url).await {
                transfers.push(parsed);
            }
        }
    }

    // Sort by block number
    transfers.sort_by_key(|t| t.0);

    if transfers.is_empty() {
        output.push_str("  No ERC-20 transfers found in this range.\n");
    } else {
        for (block, direction, token_addr, counterparty, amount, symbol) in &transfers {
            output.push_str(&format!("  Block {} | {} | {} {} | {} | counterparty: {}\n",
                block, direction, amount, symbol, &token_addr[..10], &counterparty[..10]));
        }
        output.push_str(&format!("\n  Total: {} transfers found\n", transfers.len()));
    }

    // Chain info
    if let Ok(chain_id) = eth_chain_id(rpc_url).await {
        let chain = match chain_id {
            1 => "Ethereum Mainnet", 8453 => "Base", 42161 => "Arbitrum",
            10 => "Optimism", 137 => "Polygon", _ => "Unknown",
        };
        output.push_str(&format!("\nNetwork: {} (chain ID {})\n", chain, chain_id));
    }

    Ok(output)
}

/// Scan recent large transfers of a specific token to detect whale activity.
/// Shows accumulation/distribution patterns and identifies major holders moving tokens.
pub async fn execute_dex_whale_transfers(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> Result<String, String> {
    let rpc_url = creds.get("DEX_RPC_URL").ok_or("Missing DEX_RPC_URL")?;
    let token_address = args["token_address"].as_str()
        .ok_or("dex_whale_transfers: missing 'token_address'")?;
    let blocks_back = args["blocks_back"].as_u64().unwrap_or(2000);
    let min_amount_str = args["min_amount"].as_str().unwrap_or("0");

    let addr_clean = token_address.trim();
    if !addr_clean.starts_with("0x") || addr_clean.len() != 42 {
        return Err(format!("Invalid token address: '{}'", addr_clean));
    }

    // Get token info
    let symbol = match eth_call(rpc_url, addr_clean, &encode_symbol()).await {
        Ok(s) => decode_abi_string(&s).unwrap_or_else(|_| "???".into()),
        Err(_) => "???".into(),
    };
    let decimals = match eth_call(rpc_url, addr_clean, &encode_decimals()).await {
        Ok(d) => {
            let b = hex_decode(&d).unwrap_or_default();
            if b.len() >= 32 { b[31] } else { 18 }
        }
        Err(_) => 18,
    };

    let mut output = format!("Whale Transfer Scanner: {} ({})\nContract: {}\n\n", symbol, decimals, addr_clean);

    // Get current block
    let block_num = match rpc_call(rpc_url, "eth_blockNumber", serde_json::json!([])).await {
        Ok(val) => {
            let hex = val.as_str().unwrap_or("0x0");
            u64::from_str_radix(hex.trim_start_matches("0x"), 16).unwrap_or(0)
        }
        Err(e) => return Err(format!("Cannot get block number: {}", e)),
    };
    let from_block = block_num.saturating_sub(blocks_back);

    // Get all Transfer events for this token (chunked to avoid RPC limits)
    let log_arr = chunked_get_logs(
        rpc_url,
        Some(&addr_clean),
        from_block,
        block_num,
        vec![Some(serde_json::json!(TRANSFER_EVENT_TOPIC))],
        500,
    ).await
        .map_err(|e| format!("Failed to get transfer logs: {}", e))?;

    if log_arr.is_empty() {
        output.push_str(&format!("No transfers found in last {} blocks.\n", blocks_back));
        return Ok(output);
    }

    // Parse min_amount filter
    let min_amount_f: f64 = min_amount_str.parse().unwrap_or(0.0);

    // Collect transfers and identify large ones
    let mut transfers: Vec<Transfer> = Vec::new();
    let mut accumulation_map: HashMap<String, f64> = HashMap::new(); // net inflow per address
    let mut total_volume = 0.0f64;

    for log in &log_arr {
        let topics = match log["topics"].as_array() {
            Some(t) if t.len() >= 3 => t,
            _ => continue,
        };

        let from_topic = topics[1].as_str().unwrap_or("");
        let to_topic = topics[2].as_str().unwrap_or("");
        let data = log["data"].as_str().unwrap_or("0x");
        let block_hex = log["blockNumber"].as_str().unwrap_or("0x0");
        let tx_hash = log["transactionHash"].as_str().unwrap_or("").to_string();

        let block = u64::from_str_radix(block_hex.trim_start_matches("0x"), 16).unwrap_or(0);

        // Decode amount from data
        let amount_str = match raw_to_amount(data, decimals) {
            Ok(a) => a,
            Err(_) => continue,
        };
        let amount_f: f64 = amount_str.parse().unwrap_or(0.0);

        if amount_f < min_amount_f { continue; }

        // Decode addresses from topics (last 20 bytes of 32-byte topic)
        let from_addr = if from_topic.len() >= 42 {
            format!("0x{}", &from_topic[from_topic.len()-40..])
        } else { "0x?".into() };
        let to_addr = if to_topic.len() >= 42 {
            format!("0x{}", &to_topic[to_topic.len()-40..])
        } else { "0x?".into() };

        total_volume += amount_f;

        // Track net accumulation
        let from_lower = from_addr.to_lowercase();
        let to_lower = to_addr.to_lowercase();
        *accumulation_map.entry(to_lower.clone()).or_insert(0.0) += amount_f;
        *accumulation_map.entry(from_lower.clone()).or_insert(0.0) -= amount_f;

        transfers.push(Transfer {
            block, from: from_addr, to: to_addr,
            amount: amount_f, amount_str, tx_hash,
        });
    }

    // Sort by amount descending to show largest first
    transfers.sort_by(|a, b| b.amount.partial_cmp(&a.amount).unwrap_or(std::cmp::Ordering::Equal));

    output.push_str(&format!("Scanned blocks {} → {} ({} blocks)\n", from_block, block_num, blocks_back));
    output.push_str(&format!("Total transfers found: {}\n", transfers.len()));
    output.push_str(&format!("Total volume: {} {}\n\n", format_large_number(total_volume), symbol));

    // Show top 20 largest transfers
    output.push_str("Largest Transfers:\n");
    for (i, t) in transfers.iter().take(20).enumerate() {
        output.push_str(&format!("  {}. {} {} | {} → {} | block {} | tx: {}...\n",
            i + 1,
            format_large_number(t.amount), symbol,
            &t.from[..8], &t.to[..8],
            t.block,
            if t.tx_hash.len() > 14 { &t.tx_hash[..14] } else { &t.tx_hash },
        ));
    }

    // Show top accumulators (net buyers) and distributors (net sellers)
    let mut accumulators: Vec<(String, f64)> = Vec::new();
    let mut distributors: Vec<(String, f64)> = Vec::new();
    for (addr, net) in &accumulation_map {
        if addr.contains("000000000000000000000000000000000") { continue; }
        if *net > 0.0 {
            accumulators.push((addr.clone(), *net));
        } else if *net < 0.0 {
            distributors.push((addr.clone(), net.abs()));
        }
    }
    accumulators.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    distributors.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    if !accumulators.is_empty() {
        output.push_str("\nTop Accumulators (net buyers — potential smart money):\n");
        for (i, (addr, net)) in accumulators.iter().take(10).enumerate() {
            output.push_str(&format!("  {}. {} | net +{} {}\n",
                i + 1, addr, format_large_number(*net), symbol));
        }
    }

    if !distributors.is_empty() {
        output.push_str("\nTop Distributors (net sellers — potential exit signals):\n");
        for (i, (addr, net)) in distributors.iter().take(10).enumerate() {
            output.push_str(&format!("  {}. {} | net -{} {}\n",
                i + 1, addr, format_large_number(*net), symbol));
        }
    }

    // Chain info
    if let Ok(chain_id) = eth_chain_id(rpc_url).await {
        let chain = match chain_id {
            1 => "Ethereum Mainnet", 8453 => "Base", 42161 => "Arbitrum",
            10 => "Optimism", 137 => "Polygon", _ => "Unknown",
        };
        output.push_str(&format!("\nNetwork: {} (chain ID {})\n", chain, chain_id));
    }

    output.push_str("\nTip: Use dex_watch_wallet on top accumulator addresses to see their full portfolio and trading history.\n");

    Ok(output)
}

/// Helper: parse a single Transfer event log into a transfer tuple
async fn parse_transfer_log(
    log: &serde_json::Value,
    direction: &str,
    rpc_url: &str,
) -> Option<(u64, String, String, String, String, String)> {
    let topics = log["topics"].as_array()?;
    if topics.len() < 3 { return None; }

    let token_addr = log["address"].as_str()?.to_string();
    let block_hex = log["blockNumber"].as_str()?;
    let block = u64::from_str_radix(block_hex.trim_start_matches("0x"), 16).ok()?;

    // Data field contains the amount
    let data = log["data"].as_str()?;

    // Get counterparty from topics
    let counterparty = if direction.contains("SELL") || direction.contains("SEND") {
        // Outgoing: recipient is topic[2]
        let to_topic = topics[2].as_str()?;
        if to_topic.len() >= 42 {
            format!("0x{}", &to_topic[to_topic.len()-40..])
        } else { "0x?".into() }
    } else {
        // Incoming: sender is topic[1]
        let from_topic = topics[1].as_str()?;
        if from_topic.len() >= 42 {
            format!("0x{}", &from_topic[from_topic.len()-40..])
        } else { "0x?".into() }
    };

    // Try to look up token symbol
    let symbol = match eth_call(rpc_url, &token_addr, &encode_symbol()).await {
        Ok(s) => decode_abi_string(&s).unwrap_or_else(|_| token_addr[..8].to_string()),
        Err(_) => token_addr[..8].to_string(),
    };

    // Try to get decimals
    let decimals = match eth_call(rpc_url, &token_addr, &encode_decimals()).await {
        Ok(d) => {
            let b = hex_decode(&d).unwrap_or_default();
            if b.len() >= 32 { b[31] } else { 18 }
        }
        Err(_) => 18,
    };

    let amount = raw_to_amount(data, decimals).unwrap_or_else(|_| "?".into());

    Some((block, direction.to_string(), token_addr, counterparty, amount, symbol))
}

/// Format a large number with K/M/B suffix for readability
fn format_large_number(n: f64) -> String {
    if n >= 1_000_000_000.0 {
        format!("{:.2}B", n / 1_000_000_000.0)
    } else if n >= 1_000_000.0 {
        format!("{:.2}M", n / 1_000_000.0)
    } else if n >= 1_000.0 {
        format!("{:.2}K", n / 1_000.0)
    } else if n >= 1.0 {
        format!("{:.4}", n)
    } else {
        format!("{:.8}", n)
    }
}

/// Transfer struct used internally by whale scanning
struct Transfer {
    block: u64,
    from: String,
    to: String,
    amount: f64,
    amount_str: String,
    tx_hash: String,
}

// ── Top Traders / Alpha Discovery ──────────────────────────────────────

/// Analyze on-chain Transfer events for a token to identify the most profitable
/// wallets — the "smart DEX traders", rotators, and early movers.
/// Profiles each wallet by: buy amount, sell amount, estimated PnL, trade count,
/// timing (early buyer vs late), and current holding.
pub async fn execute_dex_top_traders(
    args: &serde_json::Value,
    creds: &HashMap<String, String>,
) -> Result<String, String> {
    let rpc_url = creds.get("DEX_RPC_URL").ok_or("Missing DEX_RPC_URL")?;
    let token_address = args["token_address"].as_str()
        .ok_or("dex_top_traders: missing 'token_address'")?;
    let blocks_back = args["blocks_back"].as_u64().unwrap_or(5000);
    let min_trades = args["min_trades"].as_u64().unwrap_or(2) as usize;

    let addr_clean = token_address.trim();
    if !addr_clean.starts_with("0x") || addr_clean.len() != 42 {
        return Err(format!("Invalid token address: '{}'", addr_clean));
    }

    // Get token metadata
    let symbol = match eth_call(rpc_url, addr_clean, &encode_symbol()).await {
        Ok(s) => decode_abi_string(&s).unwrap_or_else(|_| "???".into()),
        Err(_) => "???".into(),
    };
    let decimals = match eth_call(rpc_url, addr_clean, &encode_decimals()).await {
        Ok(d) => {
            let b = hex_decode(&d).unwrap_or_default();
            if b.len() >= 32 { b[31] } else { 18 }
        }
        Err(_) => 18,
    };

    let mut output = format!("Top Traders Analysis: {} ({})\nContract: {}\n\n", symbol, decimals, addr_clean);

    // Get current block
    let block_num = match rpc_call(rpc_url, "eth_blockNumber", serde_json::json!([])).await {
        Ok(val) => {
            let hex = val.as_str().unwrap_or("0x0");
            u64::from_str_radix(hex.trim_start_matches("0x"), 16).unwrap_or(0)
        }
        Err(e) => return Err(format!("Cannot get block number: {}", e)),
    };
    let from_block = block_num.saturating_sub(blocks_back);

    // Fetch all Transfer events for this token (chunked to avoid RPC limits)
    let log_arr = chunked_get_logs(
        rpc_url,
        Some(&addr_clean),
        from_block,
        block_num,
        vec![Some(serde_json::json!(TRANSFER_EVENT_TOPIC))],
        500,
    ).await
        .map_err(|e| format!("Failed to get transfer logs: {}", e))?;

    if log_arr.is_empty() {
        output.push_str(&format!("No transfers found in last {} blocks.\n", blocks_back));
        return Ok(output);
    }

    // Build per-wallet trading profile
    struct WalletProfile {
        address: String,
        total_bought: f64,
        total_sold: f64,
        buy_count: usize,
        sell_count: usize,
        first_seen_block: u64,
        last_seen_block: u64,
    }

    let mut profiles: HashMap<String, WalletProfile> = HashMap::new();
    let zero_addr = "0x0000000000000000000000000000000000000000";

    for log in &log_arr {
        let topics = match log["topics"].as_array() {
            Some(t) if t.len() >= 3 => t,
            _ => continue,
        };

        let from_topic = topics[1].as_str().unwrap_or("");
        let to_topic = topics[2].as_str().unwrap_or("");
        let data = log["data"].as_str().unwrap_or("0x");
        let block_hex = log["blockNumber"].as_str().unwrap_or("0x0");
        let block = u64::from_str_radix(block_hex.trim_start_matches("0x"), 16).unwrap_or(0);

        let amount_str = match raw_to_amount(data, decimals) {
            Ok(a) => a,
            Err(_) => continue,
        };
        let amount_f: f64 = amount_str.parse().unwrap_or(0.0);
        if amount_f == 0.0 { continue; }

        let from_addr = if from_topic.len() >= 42 {
            format!("0x{}", &from_topic[from_topic.len()-40..]).to_lowercase()
        } else { continue };
        let to_addr = if to_topic.len() >= 42 {
            format!("0x{}", &to_topic[to_topic.len()-40..]).to_lowercase()
        } else { continue };

        // Skip mint/burn
        if from_addr == zero_addr || to_addr == zero_addr { continue; }

        // Receiver = buyer
        let buyer = profiles.entry(to_addr.clone()).or_insert(WalletProfile {
            address: to_addr.clone(),
            total_bought: 0.0, total_sold: 0.0,
            buy_count: 0, sell_count: 0,
            first_seen_block: block, last_seen_block: block,
        });
        buyer.total_bought += amount_f;
        buyer.buy_count += 1;
        if block < buyer.first_seen_block { buyer.first_seen_block = block; }
        if block > buyer.last_seen_block { buyer.last_seen_block = block; }

        // Sender = seller
        let seller = profiles.entry(from_addr.clone()).or_insert(WalletProfile {
            address: from_addr.clone(),
            total_bought: 0.0, total_sold: 0.0,
            buy_count: 0, sell_count: 0,
            first_seen_block: block, last_seen_block: block,
        });
        seller.total_sold += amount_f;
        seller.sell_count += 1;
        if block < seller.first_seen_block { seller.first_seen_block = block; }
        if block > seller.last_seen_block { seller.last_seen_block = block; }
    }

    // Filter and rank traders
    struct TraderScore {
        address: String,
        total_bought: f64,
        total_sold: f64,
        net_pnl_tokens: f64,
        trade_count: usize,
        win_indicator: f64,
        first_block: u64,
        #[allow(dead_code)]
        last_block: u64,
        still_holding: f64,
        trader_type: String,
    }

    let mut scored: Vec<TraderScore> = Vec::new();

    for (_, p) in &profiles {
        let total_trades = p.buy_count + p.sell_count;
        if total_trades < min_trades { continue; }

        let net = p.total_sold - p.total_bought;
        let still_holding = p.total_bought - p.total_sold;
        let win_indicator = if p.total_bought > 0.0 { p.total_sold / p.total_bought } else { 0.0 };

        let trader_type = if p.sell_count == 0 && p.buy_count > 0 {
            "Accumulator".to_string()
        } else if win_indicator > 1.5 {
            "Profit Taker".to_string()
        } else if win_indicator > 0.8 && win_indicator <= 1.5 {
            "Rotator".to_string()
        } else if p.buy_count > 0 && p.sell_count > 0 && total_trades > 5 {
            "Active Trader".to_string()
        } else if p.first_seen_block <= from_block + (blocks_back / 10) {
            "Early Buyer".to_string()
        } else {
            "Trader".to_string()
        };

        scored.push(TraderScore {
            address: p.address.clone(),
            total_bought: p.total_bought,
            total_sold: p.total_sold,
            net_pnl_tokens: net,
            trade_count: total_trades,
            win_indicator,
            first_block: p.first_seen_block,
            last_block: p.last_seen_block,
            still_holding,
            trader_type,
        });
    }

    // Sort by profit taken
    scored.sort_by(|a, b| b.net_pnl_tokens.partial_cmp(&a.net_pnl_tokens).unwrap_or(std::cmp::Ordering::Equal));

    output.push_str(&format!("Scanned blocks {} → {} ({} blocks, {} transfers)\n", from_block, block_num, blocks_back, log_arr.len()));
    output.push_str(&format!("Unique traders: {} (min {} trades filter)\n\n", scored.len(), min_trades));

    // Top profit takers
    output.push_str("Top Profit Takers (sold more than bought — realized gains):\n");
    let profit_takers: Vec<&TraderScore> = scored.iter().filter(|s| s.net_pnl_tokens > 0.0).take(15).collect();
    if profit_takers.is_empty() {
        output.push_str("  None found — all traders are still accumulating\n");
    } else {
        for (i, t) in profit_takers.iter().enumerate() {
            output.push_str(&format!("  {}. {} [{}]\n", i + 1, t.address, t.trader_type));
            output.push_str(&format!("     Bought: {} {} | Sold: {} {} | Net: +{} {}\n",
                format_large_number(t.total_bought), symbol,
                format_large_number(t.total_sold), symbol,
                format_large_number(t.net_pnl_tokens), symbol));
            output.push_str(&format!("     Trades: {} | Sell/Buy ratio: {:.2}x | First block: {}\n",
                t.trade_count, t.win_indicator, t.first_block));
        }
    }

    // Top accumulators
    output.push_str("\nTop Accumulators (bought more than sold — still holding):\n");
    let mut accumulators: Vec<&TraderScore> = scored.iter().filter(|s| s.still_holding > 0.0).collect();
    accumulators.sort_by(|a, b| b.still_holding.partial_cmp(&a.still_holding).unwrap_or(std::cmp::Ordering::Equal));

    if accumulators.is_empty() {
        output.push_str("  None found\n");
    } else {
        for (i, t) in accumulators.iter().take(15).enumerate() {
            output.push_str(&format!("  {}. {} [{}]\n", i + 1, t.address, t.trader_type));
            output.push_str(&format!("     Bought: {} {} | Sold: {} {} | Holding: {} {}\n",
                format_large_number(t.total_bought), symbol,
                format_large_number(t.total_sold), symbol,
                format_large_number(t.still_holding), symbol));
            output.push_str(&format!("     Trades: {} | First seen: block {}\n",
                t.trade_count, t.first_block));
        }
    }

    // Early smart money
    output.push_str("\nEarly Smart Money (first 10% of blocks AND took profit):\n");
    let early_cutoff = from_block + (blocks_back / 10);
    let mut early_winners: Vec<&TraderScore> = scored.iter()
        .filter(|s| s.first_block <= early_cutoff && s.net_pnl_tokens > 0.0)
        .collect();
    early_winners.sort_by(|a, b| b.net_pnl_tokens.partial_cmp(&a.net_pnl_tokens).unwrap_or(std::cmp::Ordering::Equal));

    if early_winners.is_empty() {
        output.push_str("  No early profit-takers found in this range\n");
    } else {
        for (i, t) in early_winners.iter().take(10).enumerate() {
            output.push_str(&format!("  {}. {} — in at block {}, net +{} {}, {} trades\n",
                i + 1, t.address, t.first_block,
                format_large_number(t.net_pnl_tokens), symbol, t.trade_count));
        }
        output.push_str("\n  ^ These wallets got in early AND profited. Watch them with dex_watch_wallet.\n");
    }

    // Chain info
    if let Ok(chain_id) = eth_chain_id(rpc_url).await {
        let chain = match chain_id {
            1 => "Ethereum Mainnet", 8453 => "Base", 42161 => "Arbitrum",
            10 => "Optimism", 137 => "Polygon", _ => "Unknown",
        };
        output.push_str(&format!("\nNetwork: {} (chain ID {})\n", chain, chain_id));
    }

    output.push_str("\nNext: Use dex_watch_wallet on promising addresses to see their full portfolio across tokens.\n");

    Ok(output)
}

// ── Trending Token Discovery ───────────────────────────────────────────

/// Get trending / recently boosted tokens from DexScreener.
/// No API key needed — uses public endpoints.
pub async fn execute_dex_trending(
    args: &serde_json::Value,
    _creds: &HashMap<String, String>,
) -> Result<String, String> {
    let chain_filter = args["chain"].as_str().unwrap_or("");
    let max_results = args["max_results"].as_u64().unwrap_or(20).min(50) as usize;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (compatible; PawAgent/1.0)")
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let mut output = String::from("Trending Tokens\n\n");

    // 1. Token Boosts (recently promoted/trending on DexScreener)
    let boosts_url = "https://api.dexscreener.com/token-boosts/latest/v1";
    match client.get(boosts_url).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                if let Ok(boosts) = resp.json::<serde_json::Value>().await {
                    if let Some(arr) = boosts.as_array() {
                        output.push_str(&format!("Recently Boosted ({} tokens):\n", arr.len()));
                        let mut count = 0;
                        for boost in arr {
                            if count >= max_results { break; }

                            let chain = boost["chainId"].as_str().unwrap_or("?");
                            if !chain_filter.is_empty() && !chain.to_lowercase().contains(&chain_filter.to_lowercase()) {
                                continue;
                            }

                            let token_addr = boost["tokenAddress"].as_str().unwrap_or("?");
                            let description = boost["description"].as_str().unwrap_or("");
                            let url = boost["url"].as_str().unwrap_or("");
                            let amount = boost["amount"].as_f64().unwrap_or(0.0);

                            output.push_str(&format!("  {}. {} on {}\n", count + 1, token_addr, chain));
                            if !description.is_empty() {
                                output.push_str(&format!("     {}\n", &description[..description.len().min(100)]));
                            }
                            if amount > 0.0 {
                                output.push_str(&format!("     Boost amount: ${:.0}\n", amount));
                            }
                            if !url.is_empty() {
                                output.push_str(&format!("     {}\n", url));
                            }
                            count += 1;
                        }
                        if count == 0 {
                            output.push_str(&format!("  No boosted tokens found for chain '{}'\n", chain_filter));
                        }
                    }
                }
            }
        }
        Err(e) => { output.push_str(&format!("  Boosts API error: {}\n", e)); }
    }

    // 2. Token Profiles (latest token listings with metadata)
    let profiles_url = "https://api.dexscreener.com/token-profiles/latest/v1";
    match client.get(profiles_url).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                if let Ok(profiles) = resp.json::<serde_json::Value>().await {
                    if let Some(arr) = profiles.as_array() {
                        output.push_str(&format!("\nRecent Token Profiles ({} listings):\n", arr.len()));
                        let mut count = 0;
                        for profile in arr {
                            if count >= max_results { break; }

                            let chain = profile["chainId"].as_str().unwrap_or("?");
                            if !chain_filter.is_empty() && !chain.to_lowercase().contains(&chain_filter.to_lowercase()) {
                                continue;
                            }

                            let token_addr = profile["tokenAddress"].as_str().unwrap_or("?");
                            let description = profile["description"].as_str().unwrap_or("");
                            let url = profile["url"].as_str().unwrap_or("");

                            output.push_str(&format!("  {}. {} on {}\n", count + 1, token_addr, chain));
                            if !description.is_empty() {
                                let desc_trimmed = &description[..description.len().min(120)];
                                output.push_str(&format!("     {}\n", desc_trimmed));
                            }
                            if !url.is_empty() {
                                output.push_str(&format!("     {}\n", url));
                            }
                            count += 1;
                        }
                        if count == 0 {
                            output.push_str(&format!("  No profiles found for chain '{}'\n", chain_filter));
                        }
                    }
                }
            }
        }
        Err(e) => { output.push_str(&format!("  Profiles API error: {}\n", e)); }
    }

    output.push_str("\nNext steps:\n");
    output.push_str("1. Use dex_search_token to get price/volume/liquidity for interesting tokens\n");
    output.push_str("2. Use dex_check_token to run safety audit before trading\n");
    output.push_str("3. Use dex_top_traders to find who's trading them profitably\n");

    Ok(output)
}