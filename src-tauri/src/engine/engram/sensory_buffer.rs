// ── Engram: Sensory Buffer ──────────────────────────────────────────────────
//
// Tier 0 of the Engram three-tier architecture:
//   Sensory Buffer (seconds) → Working Memory (minutes) → Long-Term Store (weeks+)
//
// The Sensory Buffer is a bounded, in-memory ring buffer that holds the most
// recent raw message pairs BEFORE any consolidation or storage. It serves as
// the immediate context window that the model can always see.
//
// Key properties:
//   - Pure in-memory: survives only within a single session
//   - O(1) push, O(n) drain (n = buffer size, typically small)
//   - Per-agent isolation: each agent gets its own buffer
//   - Token-budget-aware: can report its total token footprint
//   - NOT persisted to SQLite (that's Working Memory's job)

use crate::atoms::engram_types::EngramConfig;
use crate::engine::engram::tokenizer::Tokenizer;
use std::collections::VecDeque;

/// A single message pair in the sensory buffer.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SensoryEntry {
    /// The user's message (or system event)
    pub input: String,
    /// The assistant's response
    pub output: String,
    /// Timestamp when this entry was created
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Approximate token count (input + output combined)
    pub token_count: usize,
    /// Optional metadata tag (e.g., "tool_call", "thinking", "image")
    pub tag: Option<String>,
}

/// A bounded ring buffer for the most recent message exchanges.
///
/// Thread-safety: This struct is NOT internally synchronized.
/// The caller (usually behind an Arc<Mutex<_>> in EngramEngine) must handle locking.
pub struct SensoryBuffer {
    /// The bounded buffer
    entries: VecDeque<SensoryEntry>,
    /// Maximum number of entries (from EngramConfig.sensory_buffer_size)
    capacity: usize,
    /// Running total of tokens in the buffer (kept in sync with push/evict)
    total_tokens: usize,
    /// The tokenizer used for counting
    tokenizer: Tokenizer,
}

impl SensoryBuffer {
    /// Create a new sensory buffer with the given capacity and tokenizer.
    pub fn new(capacity: usize, tokenizer: Tokenizer) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
            total_tokens: 0,
            tokenizer,
        }
    }

    /// Create from an EngramConfig.
    pub fn from_config(config: &EngramConfig, tokenizer: Tokenizer) -> Self {
        Self::new(config.sensory_buffer_size, tokenizer)
    }

    /// Push a new message pair into the buffer.
    /// If the buffer is full, the oldest entry is evicted first.
    /// Returns the evicted entry (if any) for potential promotion to working memory.
    pub fn push(
        &mut self,
        input: String,
        output: String,
        tag: Option<String>,
    ) -> Option<SensoryEntry> {
        let combined = format!("{}\n{}", input, output);
        let token_count = self.tokenizer.count_tokens(&combined);

        let entry = SensoryEntry {
            input,
            output,
            timestamp: chrono::Utc::now(),
            token_count,
            tag,
        };

        let evicted = if self.entries.len() >= self.capacity {
            let old = self.entries.pop_front();
            if let Some(ref e) = old {
                self.total_tokens = self.total_tokens.saturating_sub(e.token_count);
            }
            old
        } else {
            None
        };

        self.total_tokens += entry.token_count;
        self.entries.push_back(entry);

        evicted
    }

    /// Get all entries in chronological order (oldest first).
    pub fn entries(&self) -> impl Iterator<Item = &SensoryEntry> {
        self.entries.iter()
    }

    /// Get the most recent N entries.
    pub fn recent(&self, n: usize) -> impl Iterator<Item = &SensoryEntry> {
        let skip = self.entries.len().saturating_sub(n);
        self.entries.iter().skip(skip)
    }

    /// Get the number of entries currently in the buffer.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get the total token count across all entries.
    pub fn total_tokens(&self) -> usize {
        self.total_tokens
    }

    /// Drain all entries, returning them in chronological order.
    /// This empties the buffer completely.
    pub fn drain_all(&mut self) -> Vec<SensoryEntry> {
        self.total_tokens = 0;
        self.entries.drain(..).collect()
    }

    /// Drain entries that fit within a token budget (oldest first).
    /// Returns entries up to the budget. Remaining entries stay in the buffer.
    pub fn drain_within_budget(&mut self, max_tokens: usize) -> Vec<SensoryEntry> {
        let mut result = Vec::new();
        let mut budget = max_tokens;

        while let Some(front) = self.entries.front() {
            if front.token_count > budget {
                break;
            }
            budget -= front.token_count;
            let entry = self.entries.pop_front().unwrap();
            self.total_tokens = self.total_tokens.saturating_sub(entry.token_count);
            result.push(entry);
        }

        result
    }

    /// Format the buffer contents as a context string for the model.
    /// This is the primary way the sensory buffer feeds into the prompt.
    pub fn format_for_context(&self, max_tokens: usize) -> String {
        let mut parts = Vec::new();
        let mut remaining = max_tokens;

        // Work backwards from most recent, then reverse for chronological order
        let entries: Vec<&SensoryEntry> = self.entries.iter().rev().collect();

        for entry in &entries {
            if entry.token_count > remaining {
                break;
            }
            remaining -= entry.token_count;
            parts.push(format!(
                "User: {}\nAssistant: {}",
                entry.input, entry.output
            ));
        }

        parts.reverse();
        parts.join("\n\n")
    }

    /// Clear the buffer entirely.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.total_tokens = 0;
    }

    /// Resize the buffer capacity. If the new capacity is smaller,
    /// the oldest entries are evicted.
    pub fn resize(&mut self, new_capacity: usize) {
        self.capacity = new_capacity;
        while self.entries.len() > self.capacity {
            if let Some(evicted) = self.entries.pop_front() {
                self.total_tokens = self.total_tokens.saturating_sub(evicted.token_count);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atoms::engram_types::TokenizerType;

    fn make_buffer(capacity: usize) -> SensoryBuffer {
        let tokenizer = Tokenizer::new(TokenizerType::Heuristic);
        SensoryBuffer::new(capacity, tokenizer)
    }

    #[test]
    fn test_push_and_len() {
        let mut buf = make_buffer(3);
        assert!(buf.is_empty());

        buf.push("hello".into(), "world".into(), None);
        assert_eq!(buf.len(), 1);
        assert!(!buf.is_empty());

        buf.push("foo".into(), "bar".into(), None);
        assert_eq!(buf.len(), 2);
    }

    #[test]
    fn test_eviction_on_overflow() {
        let mut buf = make_buffer(2);

        buf.push("a".into(), "1".into(), None);
        buf.push("b".into(), "2".into(), None);
        let evicted = buf.push("c".into(), "3".into(), None);

        assert_eq!(buf.len(), 2);
        assert!(evicted.is_some());
        assert_eq!(evicted.unwrap().input, "a");

        // Check remaining entries are b and c
        let entries: Vec<&SensoryEntry> = buf.entries().collect();
        assert_eq!(entries[0].input, "b");
        assert_eq!(entries[1].input, "c");
    }

    #[test]
    fn test_total_tokens_tracking() {
        let mut buf = make_buffer(10);

        buf.push("hello world".into(), "goodbye world".into(), None);
        let t1 = buf.total_tokens();
        assert!(t1 > 0);

        buf.push("another".into(), "message".into(), None);
        assert!(buf.total_tokens() > t1);
    }

    #[test]
    fn test_drain_all() {
        let mut buf = make_buffer(5);
        buf.push("a".into(), "1".into(), None);
        buf.push("b".into(), "2".into(), None);

        let drained = buf.drain_all();
        assert_eq!(drained.len(), 2);
        assert!(buf.is_empty());
        assert_eq!(buf.total_tokens(), 0);
    }

    #[test]
    fn test_recent() {
        let mut buf = make_buffer(5);
        buf.push("a".into(), "1".into(), None);
        buf.push("b".into(), "2".into(), None);
        buf.push("c".into(), "3".into(), None);

        let recent: Vec<&SensoryEntry> = buf.recent(2).collect();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].input, "b");
        assert_eq!(recent[1].input, "c");
    }

    #[test]
    fn test_resize_shrink() {
        let mut buf = make_buffer(5);
        buf.push("a".into(), "1".into(), None);
        buf.push("b".into(), "2".into(), None);
        buf.push("c".into(), "3".into(), None);

        buf.resize(2);
        assert_eq!(buf.len(), 2);

        let entries: Vec<&SensoryEntry> = buf.entries().collect();
        assert_eq!(entries[0].input, "b"); // oldest evicted
        assert_eq!(entries[1].input, "c");
    }

    #[test]
    fn test_format_for_context() {
        let mut buf = make_buffer(5);
        buf.push("question 1".into(), "answer 1".into(), None);
        buf.push("question 2".into(), "answer 2".into(), None);

        let ctx = buf.format_for_context(100_000);
        assert!(ctx.contains("question 1"));
        assert!(ctx.contains("answer 2"));
    }

    #[test]
    fn test_clear() {
        let mut buf = make_buffer(5);
        buf.push("a".into(), "1".into(), None);
        buf.clear();
        assert!(buf.is_empty());
        assert_eq!(buf.total_tokens(), 0);
    }
}
