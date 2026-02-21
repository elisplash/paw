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

#[cfg(test)]
mod tests {
    use super::*;
use crate::atoms::error::EngineResult;

    #[test]
    fn rlp_single_byte_below_0x80() {
        assert_eq!(rlp_encode_bytes(&[0x42]), vec![0x42]);
    }

    #[test]
    fn rlp_empty_bytes() {
        assert_eq!(rlp_encode_bytes(&[]), vec![0x80]);
    }

    #[test]
    fn rlp_short_string() {
        // "dog" = [0x64, 0x6f, 0x67] → [0x83, 0x64, 0x6f, 0x67]
        let result = rlp_encode_bytes(b"dog");
        assert_eq!(result, vec![0x83, 0x64, 0x6f, 0x67]);
    }

    #[test]
    fn rlp_empty_list() {
        assert_eq!(rlp_encode_list(&[]), vec![0xc0]);
    }

    #[test]
    fn rlp_encode_list_of_strings() {
        // RLP(["cat", "dog"]) — known Ethereum test vector
        let cat = rlp_encode_bytes(b"cat");
        let dog = rlp_encode_bytes(b"dog");
        let result = rlp_encode_list(&[cat, dog]);
        assert_eq!(result[0], 0xc8); // 0xc0 + 8 bytes payload
    }

    #[test]
    fn to_minimal_be_bytes_zero() {
        assert_eq!(to_minimal_be_bytes(0), Vec::<u8>::new());
    }

    #[test]
    fn to_minimal_be_bytes_one() {
        assert_eq!(to_minimal_be_bytes(1), vec![1]);
    }

    #[test]
    fn to_minimal_be_bytes_256() {
        assert_eq!(to_minimal_be_bytes(256), vec![1, 0]);
    }

    #[test]
    fn u64_to_minimal_be_zero() {
        assert_eq!(u64_to_minimal_be(0), Vec::<u8>::new());
    }

    #[test]
    fn u64_to_minimal_be_values() {
        assert_eq!(u64_to_minimal_be(1), vec![1]);
        assert_eq!(u64_to_minimal_be(255), vec![255]);
        assert_eq!(u64_to_minimal_be(256), vec![1, 0]);
    }

    #[test]
    fn u256_to_minimal_be_zero() {
        assert_eq!(u256_to_minimal_be(&[0u8; 32]), Vec::<u8>::new());
    }

    #[test]
    fn u256_to_minimal_be_one() {
        let mut val = [0u8; 32];
        val[31] = 1;
        assert_eq!(u256_to_minimal_be(&val), vec![1]);
    }
}
