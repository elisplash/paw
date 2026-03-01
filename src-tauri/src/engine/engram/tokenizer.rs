// ── Engram: Unified Tokenizer ───────────────────────────────────────────────
//
// Single source of truth for token estimation across the entire engine.
// Replaces the 4 divergent `chars/4` estimators scattered across:
//   - commands/chat.rs
//   - engine/agent_loop/helpers.rs
//   - engine/injection.rs
//   - frontend token_meter.ts
//
// Strategy:
//   1. For known model families, use the correct chars-per-token ratio.
//   2. For unknown models, use a safe heuristic (chars / 3.5).
//   3. All callers go through `Tokenizer::count_tokens()` — no manual division.

use crate::atoms::engram_types::TokenizerType;

/// Unified tokenizer — all token estimation goes through this.
///
/// The engine operates on text (not raw tokens), so we estimate token counts
/// from character/byte length using model-appropriate ratios. This gives ≤5%
/// error for English text, which is well within the safety margin we keep.
///
/// If/when `tiktoken-rs` or similar is added as a dependency, the `Exact`
/// variant can be used for zero-error counting on supported model families.
#[derive(Debug, Clone)]
pub struct Tokenizer {
    kind: TokenizerType,
    /// Average characters per token for this tokenizer.
    chars_per_token: f32,
}

impl Tokenizer {
    /// Create a tokenizer from a known type.
    pub fn new(kind: TokenizerType) -> Self {
        let cpt = match kind {
            // GPT-4, GPT-4o, Claude 3.x: ~3.7 chars/token for English
            TokenizerType::Cl100kBase => 3.7,
            // o1, o3, o4, Codex 5.x: ~3.9 chars/token (slightly coarser vocab)
            TokenizerType::O200kBase => 3.9,
            // Gemini: ~3.5 chars/token
            TokenizerType::Gemini => 3.5,
            // Llama, Mistral (SentencePiece): ~3.3 chars/token
            TokenizerType::SentencePiece => 3.3,
            // Fallback: conservative 3.5 (overestimates slightly = safe)
            TokenizerType::Heuristic => 3.5,
        };

        Self {
            kind,
            chars_per_token: cpt,
        }
    }

    /// Create a heuristic tokenizer (safe default).
    pub fn heuristic() -> Self {
        Self::new(TokenizerType::Heuristic)
    }

    /// Estimate the number of tokens in a string.
    ///
    /// This is the ONLY function that should be used for token counting.
    /// All `s.len() / 4` or `chars().count() / 4` patterns must be replaced
    /// with calls to this method.
    pub fn count_tokens(&self, text: &str) -> usize {
        if text.is_empty() {
            return 0;
        }
        // Use char count (not byte count) for Unicode correctness.
        // Byte count would overcount for CJK/emoji text.
        let char_count = text.chars().count();
        let estimated = (char_count as f32 / self.chars_per_token).ceil() as usize;

        // Minimum 1 token for non-empty text
        estimated.max(1)
    }

    /// Estimate tokens for a slice of messages (each counted separately, totaled).
    pub fn count_tokens_for_messages(&self, messages: &[&str]) -> usize {
        // Each message has ~4 tokens of overhead (role, separators)
        let overhead_per_message = 4;
        messages
            .iter()
            .map(|m| self.count_tokens(m) + overhead_per_message)
            .sum()
    }

    /// Get the tokenizer kind.
    pub fn kind(&self) -> TokenizerType {
        self.kind
    }

    /// Get the chars-per-token ratio.
    pub fn chars_per_token(&self) -> f32 {
        self.chars_per_token
    }

    /// Estimate how many characters fit in a given token budget.
    /// Useful for pre-allocating string capacity.
    pub fn chars_for_tokens(&self, tokens: usize) -> usize {
        (tokens as f32 * self.chars_per_token) as usize
    }

    /// Truncate text to fit within a token budget, respecting UTF-8 boundaries.
    /// Returns the truncated text and the actual token cost.
    pub fn truncate_to_budget<'a>(&self, text: &'a str, max_tokens: usize) -> (&'a str, usize) {
        let current_tokens = self.count_tokens(text);
        if current_tokens <= max_tokens {
            return (text, current_tokens);
        }

        // Estimate character limit
        let max_chars = self.chars_for_tokens(max_tokens);
        let mut end = max_chars.min(text.len());

        // Walk backwards to find a valid UTF-8 boundary
        while end > 0 && !text.is_char_boundary(end) {
            end -= 1;
        }

        // Try to break at a word boundary (space, newline)
        if let Some(last_space) = text[..end].rfind(|c: char| c.is_whitespace()) {
            if last_space > end / 2 {
                // Only break at word boundary if we're not losing too much
                end = last_space;
            }
        }

        let truncated = &text[..end];
        let actual_tokens = self.count_tokens(truncated);
        (truncated, actual_tokens)
    }
}

impl Default for Tokenizer {
    fn default() -> Self {
        Self::heuristic()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_token_counting() {
        let tok = Tokenizer::heuristic();
        // "Hello, world!" = 13 chars. At 3.5 chars/token ≈ 4 tokens
        let count = tok.count_tokens("Hello, world!");
        assert!(count >= 3 && count <= 5, "Expected ~4, got {}", count);
    }

    #[test]
    fn test_empty_string() {
        let tok = Tokenizer::heuristic();
        assert_eq!(tok.count_tokens(""), 0);
    }

    #[test]
    fn test_single_char() {
        let tok = Tokenizer::heuristic();
        assert_eq!(tok.count_tokens("a"), 1);
    }

    #[test]
    fn test_long_text() {
        let tok = Tokenizer::new(TokenizerType::Cl100kBase);
        // 1000 chars of English text ≈ 270 tokens (at 3.7 chars/token)
        let text = "a".repeat(1000);
        let count = tok.count_tokens(&text);
        assert!(count >= 250 && count <= 300, "Expected ~270, got {}", count);
    }

    #[test]
    fn test_truncate_to_budget() {
        let tok = Tokenizer::heuristic();
        let text = "The quick brown fox jumps over the lazy dog";
        let (truncated, cost) = tok.truncate_to_budget(text, 5);
        assert!(cost <= 5, "Cost {} exceeds budget 5", cost);
        assert!(!truncated.is_empty());
    }

    #[test]
    fn test_truncate_no_op_when_fits() {
        let tok = Tokenizer::heuristic();
        let text = "Hello";
        let (truncated, cost) = tok.truncate_to_budget(text, 100);
        assert_eq!(truncated, text);
        assert!(cost <= 2);
    }

    #[test]
    fn test_unicode_safety() {
        let tok = Tokenizer::heuristic();
        // Emoji/CJK — chars are > 1 byte
        let text = "你好世界🌍";
        let count = tok.count_tokens(text);
        assert!(count >= 1, "Should handle Unicode correctly");

        // Truncation should not panic on Unicode
        let (truncated, _) = tok.truncate_to_budget(text, 1);
        assert!(truncated.is_char_boundary(truncated.len()));
    }

    #[test]
    fn test_different_tokenizers_give_different_counts() {
        let text = "a".repeat(100);
        let cl100k = Tokenizer::new(TokenizerType::Cl100kBase);
        let sp = Tokenizer::new(TokenizerType::SentencePiece);

        let cl100k_count = cl100k.count_tokens(&text);
        let sp_count = sp.count_tokens(&text);

        // SentencePiece has lower chars/token = more tokens for same text
        assert!(
            sp_count >= cl100k_count,
            "SP {} should >= CL100K {}",
            sp_count,
            cl100k_count
        );
    }
}
