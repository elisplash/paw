// Paw Agent Engine — DEX ABI Encoding
// EVM ABI encoding, Uniswap V3 calldata builders, and ERC-20 introspection helpers.

use super::primitives::{keccak256, hex_decode};
use crate::atoms::error::{EngineError, EngineResult};

/// Compute 4-byte function selector from signature
pub(crate) fn function_selector(sig: &str) -> [u8; 4] {
    let hash = keccak256(sig.as_bytes());
    let mut sel = [0u8; 4];
    sel.copy_from_slice(&hash[..4]);
    sel
}

/// ABI-encode an address (left-padded to 32 bytes)
pub(crate) fn abi_encode_address(addr: &[u8; 20]) -> Vec<u8> {
    let mut encoded = vec![0u8; 12]; // 12 zero bytes
    encoded.extend_from_slice(addr);
    encoded
}

/// ABI-encode a uint256 from big-endian bytes
pub(crate) fn abi_encode_uint256(val: &[u8; 32]) -> Vec<u8> {
    val.to_vec()
}

/// ABI-encode a uint24 (fee tier) as uint256
pub(crate) fn abi_encode_uint24_as_uint256(val: u32) -> Vec<u8> {
    let mut encoded = vec![0u8; 32];
    encoded[29] = ((val >> 16) & 0xFF) as u8;
    encoded[30] = ((val >> 8) & 0xFF) as u8;
    encoded[31] = (val & 0xFF) as u8;
    encoded
}

/// Encode ERC-20 balanceOf(address)
pub(crate) fn encode_balance_of(address: &[u8; 20]) -> Vec<u8> {
    let selector = function_selector("balanceOf(address)");
    let mut data = selector.to_vec();
    data.extend_from_slice(&abi_encode_address(address));
    data
}

/// Encode ERC-20 approve(address, uint256)
pub(crate) fn encode_approve(spender: &[u8; 20], amount: &[u8; 32]) -> Vec<u8> {
    let selector = function_selector("approve(address,uint256)");
    let mut data = selector.to_vec();
    data.extend_from_slice(&abi_encode_address(spender));
    data.extend_from_slice(&abi_encode_uint256(amount));
    data
}

/// Encode ERC-20 allowance(owner, spender)
pub(crate) fn encode_allowance(owner: &[u8; 20], spender: &[u8; 20]) -> Vec<u8> {
    let selector = function_selector("allowance(address,address)");
    let mut data = selector.to_vec();
    data.extend_from_slice(&abi_encode_address(owner));
    data.extend_from_slice(&abi_encode_address(spender));
    data
}

/// Encode Uniswap V3 QuoterV2.quoteExactInput for multi-hop paths
/// quoteExactInput(bytes path, uint256 amountIn) → (uint256 amountOut, ...)
pub(crate) fn encode_quote_exact_input(
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
pub(crate) fn encode_exact_input(
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
pub(crate) fn build_multihop_path(tokens: &[&[u8; 20]], fees: &[u32]) -> Vec<u8> {
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
pub(crate) fn strip_leading_zeros(data: &[u8]) -> Vec<u8> {
    let first_nonzero = data.iter().position(|&b| b != 0);
    match first_nonzero {
        Some(pos) => data[pos..].to_vec(),
        None => vec![],
    }
}

/// Convert a u256 (big-endian [u8; 32]) to a quantity hex string ("0x1234", no leading zeros)
pub(crate) fn u256_to_quantity_hex(val: &[u8; 32]) -> String {
    let stripped = strip_leading_zeros(val);
    if stripped.is_empty() {
        "0x0".to_string()
    } else {
        format!("0x{}", stripped.iter().map(|b| format!("{:02x}", b)).collect::<String>())
    }
}

/// Encode Uniswap V3 QuoterV2.quoteExactInputSingle
/// quoteExactInputSingle((address,address,uint256,uint24,uint160))
pub(crate) fn encode_quote_exact_input_single(
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
pub(crate) fn encode_exact_input_single(
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

/// ABI-encode ERC-20 name() call
pub(crate) fn encode_name() -> Vec<u8> {
    function_selector("name()").to_vec()
}

/// ABI-encode ERC-20 symbol() call
pub(crate) fn encode_symbol() -> Vec<u8> {
    function_selector("symbol()").to_vec()
}

/// ABI-encode ERC-20 decimals() call
pub(crate) fn encode_decimals() -> Vec<u8> {
    function_selector("decimals()").to_vec()
}

/// ABI-encode ERC-20 totalSupply() call
pub(crate) fn encode_total_supply() -> Vec<u8> {
    function_selector("totalSupply()").to_vec()
}

/// ABI-encode ERC-20 owner() call (common in tokens)
pub(crate) fn encode_owner() -> Vec<u8> {
    function_selector("owner()").to_vec()
}

/// Decode an ABI-encoded string (dynamic type at offset 0)
pub(crate) fn decode_abi_string(hex_data: &str) -> EngineResult<String> {
    let bytes = hex_decode(hex_data)?;
    if bytes.len() < 64 {
        // Might be a non-standard response — try UTF-8 directly from bytes32
        let trimmed: Vec<u8> = bytes.iter().copied().filter(|&b| b != 0).collect();
        return String::from_utf8(trimmed).map_err(|_| EngineError::Other("Cannot decode string".into()));
    }
    // Standard ABI: offset (32 bytes) + length (32 bytes) + data
    let offset_bytes: [u8; 32] = bytes[..32].try_into().map_err(|_| EngineError::Other("Bad offset".into()))?;
    let offset = u32::from_be_bytes(
        offset_bytes[28..32].try_into().map_err(|_| EngineError::Other("Bad offset u32 slice".into()))?
    ) as usize;

    if offset + 32 > bytes.len() {
        // Try bytes32 fallback
        let trimmed: Vec<u8> = bytes[..32].iter().copied().filter(|&b| b != 0).collect();
        return String::from_utf8(trimmed).map_err(|_| EngineError::Other("Cannot decode string".into()));
    }

    let len_start = offset;
    let len_bytes: [u8; 32] = bytes[len_start..len_start + 32].try_into().map_err(|_| EngineError::Other("Bad length".into()))?;
    let len = u32::from_be_bytes(
        len_bytes[28..32].try_into().map_err(|_| EngineError::Other("Bad length u32 slice".into()))?
    ) as usize;

    let data_start = len_start + 32;
    if data_start + len > bytes.len() {
        return Err(EngineError::Other("String data exceeds response".into()));
    }

    String::from_utf8(bytes[data_start..data_start + len].to_vec())
        .map_err(|_| EngineError::Other("Invalid UTF-8 in string".into()))
}

/// Encode ERC-20 transfer(address, uint256)
pub(crate) fn encode_transfer(to: &[u8; 20], amount: &[u8; 32]) -> Vec<u8> {
    let selector = function_selector("transfer(address,uint256)");
    let mut data = selector.to_vec();
    data.extend_from_slice(&abi_encode_address(to));
    data.extend_from_slice(&abi_encode_uint256(amount));
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::dex::primitives::hex_encode;

    #[test]
    fn function_selector_transfer() {
        // keccak256("transfer(address,uint256)") first 4 bytes = 0xa9059cbb
        let sel = function_selector("transfer(address,uint256)");
        assert_eq!(hex_encode(&sel), "0xa9059cbb");
    }

    #[test]
    fn function_selector_balance_of() {
        // keccak256("balanceOf(address)") first 4 bytes = 0x70a08231
        let sel = function_selector("balanceOf(address)");
        assert_eq!(hex_encode(&sel), "0x70a08231");
    }

    #[test]
    fn function_selector_approve() {
        // keccak256("approve(address,uint256)") first 4 bytes = 0x095ea7b3
        let sel = function_selector("approve(address,uint256)");
        assert_eq!(hex_encode(&sel), "0x095ea7b3");
    }

    #[test]
    fn abi_encode_address_padding() {
        let addr = [0u8; 20];
        let encoded = abi_encode_address(&addr);
        assert_eq!(encoded.len(), 32);
        assert_eq!(&encoded[..12], &[0u8; 12]); // left-padded
    }

    #[test]
    fn abi_encode_uint256_passthrough() {
        let val = [0xFFu8; 32];
        let encoded = abi_encode_uint256(&val);
        assert_eq!(encoded, val.to_vec());
    }

    #[test]
    fn encode_balance_of_length() {
        let addr = [1u8; 20];
        let data = encode_balance_of(&addr);
        assert_eq!(data.len(), 4 + 32); // 4-byte selector + 32-byte address
    }

    #[test]
    fn encode_approve_length() {
        let spender = [2u8; 20];
        let amount = [0u8; 32];
        let data = encode_approve(&spender, &amount);
        assert_eq!(data.len(), 4 + 32 + 32); // selector + address + uint256
    }

    #[test]
    fn u256_to_quantity_hex_zero() {
        assert_eq!(u256_to_quantity_hex(&[0u8; 32]), "0x0");
    }

    #[test]
    fn u256_to_quantity_hex_one() {
        let mut val = [0u8; 32];
        val[31] = 1;
        assert_eq!(u256_to_quantity_hex(&val), "0x01");
    }

    #[test]
    fn build_multihop_path_two_hops() {
        let token_a = &[1u8; 20];
        let token_b = &[2u8; 20];
        let token_c = &[3u8; 20];
        let path = build_multihop_path(&[token_a, token_b, token_c], &[3000, 500]);
        // token_a(20) + fee(3) + token_b(20) + fee(3) + token_c(20) = 66 bytes
        assert_eq!(path.len(), 66);
    }
}
