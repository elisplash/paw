// Paw Agent Engine — DEX RLP Encoding
// Recursive-length prefix encoding as used in Ethereum transactions.

/// RLP-encode a single byte string
pub(crate) fn rlp_encode_bytes(data: &[u8]) -> Vec<u8> {
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
pub(crate) fn rlp_encode_list(items: &[Vec<u8>]) -> Vec<u8> {
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
pub(crate) fn to_minimal_be_bytes(val: usize) -> Vec<u8> {
    if val == 0 { return vec![]; }
    let bytes = val.to_be_bytes();
    let first_nonzero = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len() - 1);
    bytes[first_nonzero..].to_vec()
}

/// Encode a u64 as minimal big-endian bytes (for RLP)
pub(crate) fn u64_to_minimal_be(val: u64) -> Vec<u8> {
    if val == 0 { return vec![]; }
    let bytes = val.to_be_bytes();
    let first_nonzero = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len() - 1);
    bytes[first_nonzero..].to_vec()
}

/// Encode a u256 (big-endian [u8; 32]) as minimal big-endian bytes
pub(crate) fn u256_to_minimal_be(val: &[u8; 32]) -> Vec<u8> {
    let first_nonzero = val.iter().position(|&b| b != 0);
    match first_nonzero {
        Some(pos) => val[pos..].to_vec(),
        None => vec![], // represents zero
    }
}
