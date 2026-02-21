// Paw Agent Engine — DEX Ethereum Primitives
// Core hex, keccak, address, and amount conversion utilities.

use crate::atoms::error::{EngineError, EngineResult};

/// Keccak-256 hash (Ethereum's hash function)
pub(crate) fn keccak256(data: &[u8]) -> [u8; 32] {
    use tiny_keccak::{Hasher, Keccak};
    let mut hasher = Keccak::v256();
    let mut output = [0u8; 32];
    hasher.update(data);
    hasher.finalize(&mut output);
    output
}

/// Hex-encode bytes with 0x prefix
pub(crate) fn hex_encode(data: &[u8]) -> String {
    format!("0x{}", data.iter().map(|b| format!("{:02x}", b)).collect::<String>())
}

/// Hex-decode a 0x-prefixed string
/// Handles Ethereum RPC's minimal hex encoding (e.g. "0x0", "0x1a3")
/// by left-padding to even length.
pub(crate) fn hex_decode(s: &str) -> EngineResult<Vec<u8>> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    // Handle empty hex
    if s.is_empty() {
        return Ok(vec![0]);
    }
    // Left-pad to even length (Ethereum RPC returns minimal hex like "0x0" or "0x1a3")
    let padded;
    let hex_str = if !s.len().is_multiple_of(2) {
        padded = format!("0{}", s);
        &padded
    } else {
        s
    };
    (0..hex_str.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex_str[i..i + 2], 16).map_err(|e| EngineError::Other(format!("Hex decode: {}", e))))
        .collect()
}

/// Derive Ethereum address from secp256k1 public key
pub(crate) fn address_from_pubkey(pubkey_uncompressed: &[u8]) -> String {
    // Skip the 0x04 prefix (uncompressed key marker), hash the 64-byte x||y
    let hash = keccak256(&pubkey_uncompressed[1..]);
    // Address is last 20 bytes
    let addr = &hash[12..];
    // EIP-55 checksum encoding
    eip55_checksum(addr)
}

/// EIP-55 mixed-case checksum address
pub(crate) fn eip55_checksum(addr_bytes: &[u8]) -> String {
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
pub(crate) fn parse_address(addr: &str) -> EngineResult<[u8; 20]> {
    let addr = addr.trim();
    let bytes = hex_decode(addr)?;
    if bytes.len() != 20 {
        return Err(EngineError::Other(format!("Invalid address length: {} bytes (expected 20). Address: '{}'", bytes.len(), addr)));
    }
    let mut arr = [0u8; 20];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

/// Parse a U256 from decimal string
pub(crate) fn parse_u256_decimal(s: &str) -> EngineResult<[u8; 32]> {
    // Simple decimal-to-big-endian conversion
    let mut result = [0u8; 32];

    // Handle scientific notation
    if s.contains('e') || s.contains('E') {
        return Err(EngineError::Other("Scientific notation not supported, use plain decimal".into()));
    }

    // Convert decimal string to bytes
    let mut digits: Vec<u8> = Vec::new();
    for c in s.chars() {
        if !c.is_ascii_digit() {
            return Err(EngineError::Other(format!("Invalid decimal character: {}", c)));
        }
        digits.push(c as u8 - b'0');
    }

    // Convert to big-endian bytes using repeated division by 256
    let mut big = digits;
    let mut byte_pos = 31i32;
    while (big.len() != 1 || big[0] != 0) && byte_pos >= 0 {
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
pub(crate) fn amount_to_raw(amount: &str, decimals: u8) -> EngineResult<String> {
    let parts: Vec<&str> = amount.split('.').collect();
    if parts.len() > 2 {
        return Err(EngineError::Other("Invalid amount format".into()));
    }
    let integer_part = parts[0];
    let decimal_part = if parts.len() == 2 { parts[1] } else { "" };

    if decimal_part.len() > decimals as usize {
        return Err(EngineError::Other(format!("Too many decimal places (max {} for this token)", decimals)));
    }

    let padded_decimals = format!("{:0<width$}", decimal_part, width = decimals as usize);
    let raw = format!("{}{}", integer_part, padded_decimals);
    // Strip leading zeros but keep at least "0"
    let trimmed = raw.trim_start_matches('0');
    if trimmed.is_empty() { Ok("0".into()) } else { Ok(trimmed.into()) }
}

/// Convert raw units to human-readable amount
pub(crate) fn raw_to_amount(raw_hex: &str, decimals: u8) -> EngineResult<String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_encode_empty() {
        assert_eq!(hex_encode(&[]), "0x");
    }

    #[test]
    fn hex_encode_bytes() {
        assert_eq!(hex_encode(&[0xde, 0xad, 0xbe, 0xef]), "0xdeadbeef");
    }

    #[test]
    fn hex_decode_valid() {
        assert_eq!(hex_decode("0xdeadbeef").unwrap(), vec![0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn hex_decode_no_prefix() {
        assert_eq!(hex_decode("ff00").unwrap(), vec![0xff, 0x00]);
    }

    #[test]
    fn hex_decode_odd_length_pads() {
        // Ethereum RPC returns minimal hex like "0x0" or "0x1a3"
        assert_eq!(hex_decode("0x0").unwrap(), vec![0x00]);
        assert_eq!(hex_decode("0x1a3").unwrap(), vec![0x01, 0xa3]);
    }

    #[test]
    fn hex_decode_invalid_chars() {
        assert!(hex_decode("0xGG").is_err());
    }

    #[test]
    fn keccak256_known_vector() {
        // keccak256("") = c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
        let hash = keccak256(b"");
        assert_eq!(hex_encode(&hash), "0xc5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470");
    }

    #[test]
    fn eip55_checksum_known_address() {
        // Known EIP-55 test vector
        let addr_bytes = hex_decode("0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed").unwrap();
        let mut arr = [0u8; 20];
        arr.copy_from_slice(&addr_bytes);
        let checksummed = eip55_checksum(&arr);
        assert_eq!(checksummed, "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed");
    }

    #[test]
    fn parse_address_valid() {
        let addr = parse_address("0x0000000000000000000000000000000000000001").unwrap();
        assert_eq!(addr[19], 1);
        assert_eq!(addr[0], 0);
    }

    #[test]
    fn parse_address_wrong_length() {
        assert!(parse_address("0xdead").is_err());
    }

    #[test]
    fn parse_u256_decimal_zero() {
        let result = parse_u256_decimal("0").unwrap();
        assert_eq!(result, [0u8; 32]);
    }

    #[test]
    fn parse_u256_decimal_one() {
        let result = parse_u256_decimal("1").unwrap();
        let mut expected = [0u8; 32];
        expected[31] = 1;
        assert_eq!(result, expected);
    }

    #[test]
    fn parse_u256_decimal_256() {
        let result = parse_u256_decimal("256").unwrap();
        let mut expected = [0u8; 32];
        expected[30] = 1;
        assert_eq!(result, expected);
    }

    #[test]
    fn amount_to_raw_integer() {
        assert_eq!(amount_to_raw("1", 18).unwrap(), "1000000000000000000");
    }

    #[test]
    fn amount_to_raw_decimal() {
        assert_eq!(amount_to_raw("1.5", 18).unwrap(), "1500000000000000000");
    }

    #[test]
    fn amount_to_raw_zero() {
        assert_eq!(amount_to_raw("0", 18).unwrap(), "0");
    }

    #[test]
    fn amount_to_raw_too_many_decimals() {
        assert!(amount_to_raw("1.123456789", 6).is_err());
    }

    #[test]
    fn raw_to_amount_one_eth() {
        // 1 ETH = 0xDE0B6B3A7640000 in raw units (10^18)
        let result = raw_to_amount("0xDE0B6B3A7640000", 18).unwrap();
        assert_eq!(result, "1");
    }
}
