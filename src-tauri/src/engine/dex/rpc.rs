// Paw Agent Engine — DEX JSON-RPC Helpers

use super::primitives::hex_encode;
use crate::atoms::error::{EngineError, EngineResult};
use std::time::Duration;

/// Low-level JSON-RPC call
pub(crate) async fn rpc_call(
    rpc_url: &str,
    method: &str,
    params: serde_json::Value,
) -> EngineResult<serde_json::Value> {
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
        .await?;

    let result: serde_json::Value = resp
        .json()
        .await?;

    if let Some(error) = result.get("error") {
        return Err(EngineError::Other(format!("RPC error: {}", error)));
    }

    result.get("result")
        .cloned()
        .ok_or_else(|| EngineError::Other("RPC response missing 'result' field".into()))
}

/// Get ETH balance of an address
pub(crate) async fn eth_get_balance(rpc_url: &str, address: &str) -> EngineResult<String> {
    let result = rpc_call(rpc_url, "eth_getBalance", serde_json::json!([address, "latest"])).await?;
    result.as_str().map(String::from).ok_or(EngineError::Other("Invalid balance result".into()))
}

/// Call a contract (read-only)
pub(crate) async fn eth_call(rpc_url: &str, to: &str, data: &[u8]) -> EngineResult<String> {
    let result = rpc_call(rpc_url, "eth_call", serde_json::json!([
        { "to": to, "data": hex_encode(data) },
        "latest"
    ])).await?;
    result.as_str().map(String::from).ok_or(EngineError::Other("Invalid eth_call result".into()))
}

/// Get the next nonce for an address
pub(crate) async fn eth_get_transaction_count(rpc_url: &str, address: &str) -> EngineResult<u64> {
    let result = rpc_call(rpc_url, "eth_getTransactionCount", serde_json::json!([address, "latest"])).await?;
    let hex = result.as_str().ok_or(EngineError::Other("Invalid nonce result".into()))?;
    u64::from_str_radix(hex.strip_prefix("0x").unwrap_or(hex), 16)
        .map_err(|e| EngineError::Other(format!("Parse nonce: {}", e)))
}

/// Get current gas fees (EIP-1559) — returns (max_priority_fee_per_gas, max_fee_per_gas)
pub(crate) async fn get_gas_fees(rpc_url: &str) -> EngineResult<(u64, u64)> {
    // Get base fee from latest block
    let block = rpc_call(rpc_url, "eth_getBlockByNumber", serde_json::json!(["latest", false])).await?;
    let base_fee_hex = block.get("baseFeePerGas")
        .and_then(|v| v.as_str())
        .ok_or(EngineError::Other("Missing baseFeePerGas".into()))?;
    let base_fee = u64::from_str_radix(base_fee_hex.strip_prefix("0x").unwrap_or(base_fee_hex), 16)
        .map_err(|e| EngineError::Other(format!("Parse base fee: {}", e)))?;

    // Priority fee: reasonable default of 1.5 gwei
    let max_priority_fee = 1_500_000_000u64; // 1.5 gwei

    // Max fee = 2 * base_fee + priority fee (gives room for next block)
    let max_fee = base_fee * 2 + max_priority_fee;

    Ok((max_priority_fee, max_fee))
}

/// Estimate gas for a transaction
pub(crate) async fn eth_estimate_gas(
    rpc_url: &str,
    from: &str,
    to: &str,
    data: &[u8],
    value: &str,
) -> EngineResult<u64> {
    let result = rpc_call(rpc_url, "eth_estimateGas", serde_json::json!([{
        "from": from,
        "to": to,
        "data": hex_encode(data),
        "value": value
    }])).await?;
    let hex = result.as_str().ok_or(EngineError::Other("Invalid gas estimate".into()))?;
    let estimate = u64::from_str_radix(hex.strip_prefix("0x").unwrap_or(hex), 16)
        .map_err(|e| EngineError::Other(format!("Parse gas estimate: {}", e)))?;
    // Add 20% buffer
    Ok(estimate * 120 / 100)
}

/// Broadcast a signed transaction
pub(crate) async fn eth_send_raw_transaction(rpc_url: &str, signed_tx: &[u8]) -> EngineResult<String> {
    let result = rpc_call(rpc_url, "eth_sendRawTransaction", serde_json::json!([hex_encode(signed_tx)])).await?;
    result.as_str().map(String::from).ok_or(EngineError::Other("Invalid tx hash result".into()))
}

/// Get chain ID
pub(crate) async fn eth_chain_id(rpc_url: &str) -> EngineResult<u64> {
    let result = rpc_call(rpc_url, "eth_chainId", serde_json::json!([])).await?;
    let hex = result.as_str().ok_or(EngineError::Other("Invalid chain ID".into()))?;
    u64::from_str_radix(hex.strip_prefix("0x").unwrap_or(hex), 16)
        .map_err(|e| EngineError::Other(format!("Parse chain ID: {}", e)))
}

/// Get transaction receipt (to check if tx was mined)
pub(crate) async fn eth_get_transaction_receipt(rpc_url: &str, tx_hash: &str) -> EngineResult<Option<serde_json::Value>> {
    let result = rpc_call(rpc_url, "eth_getTransactionReceipt", serde_json::json!([tx_hash])).await?;
    if result.is_null() { Ok(None) } else { Ok(Some(result)) }
}

/// Chunked eth_getLogs — splits large block ranges into smaller chunks to avoid
/// RPC provider limits (many free tiers limit to 500-2000 blocks per request).
/// Returns all matching logs combined from all chunks.
pub(crate) async fn chunked_get_logs(
    rpc_url: &str,
    address: Option<&str>,
    from_block: u64,
    to_block: u64,
    topics: Vec<Option<serde_json::Value>>,
    chunk_size: u64,
) -> EngineResult<Vec<serde_json::Value>> {
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
