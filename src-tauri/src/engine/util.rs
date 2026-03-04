/// Truncate a string to at most `max_bytes` bytes, rounding down to the
/// nearest UTF-8 character boundary so we never panic on a byte-level slice.
#[inline]
pub fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    &s[..s.floor_char_boundary(max_bytes)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_only() {
        assert_eq!(safe_truncate("hello world", 5), "hello");
    }

    #[test]
    fn exact_len() {
        assert_eq!(safe_truncate("abc", 3), "abc");
    }

    #[test]
    fn under_limit() {
        assert_eq!(safe_truncate("ab", 10), "ab");
    }

    #[test]
    fn emoji_boundary() {
        // 🔴 is 4 bytes (U+1F534)
        let s = "aa🔴bb"; // bytes: a(1) a(1) 🔴(4) b(1) b(1) = 8
        assert_eq!(safe_truncate(s, 3), "aa"); // can't fit the emoji
        assert_eq!(safe_truncate(s, 6), "aa🔴"); // emoji fits fully
        assert_eq!(safe_truncate(s, 5), "aa"); // mid-emoji → back up
        assert_eq!(safe_truncate(s, 4), "aa"); // mid-emoji → back up
    }

    #[test]
    fn empty() {
        assert_eq!(safe_truncate("", 10), "");
    }

    #[test]
    fn multibyte_various() {
        // é is 2 bytes, 中 is 3 bytes
        let s = "aé中b"; // 1+2+3+1 = 7
        assert_eq!(safe_truncate(s, 1), "a");
        assert_eq!(safe_truncate(s, 2), "a"); // mid-é
        assert_eq!(safe_truncate(s, 3), "aé");
        assert_eq!(safe_truncate(s, 5), "aé"); // mid-中
        assert_eq!(safe_truncate(s, 6), "aé中");
    }
}
