// Paw Agent Engine — DEX EIP-1559 Transaction Signing

use super::abi::strip_leading_zeros;
use super::primitives::keccak256;
use super::rlp::{rlp_encode_bytes, rlp_encode_list, u64_to_minimal_be, u256_to_minimal_be};
use crate::atoms::error::{EngineResult, EngineError};

/// Sign an EIP-1559 transaction and return the raw serialized bytes.
pub(crate) fn sign_eip1559_transaction(
    chain_id: u64,
    nonce: u64,
    max_priority_fee_per_gas: u64,
    max_fee_per_gas: u64,
    gas_limit: u64,
    to: &[u8; 20],
    value: &[u8; 32],
    data: &[u8],
    private_key: &k256::ecdsa::SigningKey,
) -> EngineResult<Vec<u8>> {
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
        .map_err(|e| EngineError::Other(e.to_string()))?;

    let sig_bytes = signature.to_bytes();
    let r = &sig_bytes[..32];
    let s = &sig_bytes[32..];
    let v = recovery_id.to_byte(); // 0 or 1

    // Signed tx: 0x02 || RLP([chain_id, nonce, max_priority_fee, max_fee, gas, to, value, data, access_list, v, r, s])
    let mut signed_items = items;
    signed_items.push(rlp_encode_bytes(&u64_to_minimal_be(v as u64)));
    signed_items.push(rlp_encode_bytes(&strip_leading_zeros(r)));
    signed_items.push(rlp_encode_bytes(&strip_leading_zeros(s)));

    let signed_rlp = rlp_encode_list(&signed_items);

    let mut result = vec![0x02u8];
    result.extend_from_slice(&signed_rlp);
    Ok(result)
}
