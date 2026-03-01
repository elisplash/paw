// ── Engram: Working Memory ──────────────────────────────────────────────────
//
// Tier 1 of the Engram three-tier architecture:
//   Sensory Buffer (seconds) → *Working Memory* (minutes) → Long-Term Store (weeks+)
//
// Working Memory is the active scratchpad that holds the most relevant context
// for the current conversation. It is:
//   - Slot-based: each piece of context occupies a "slot" with a token cost
//   - Token-budget-aware: total slots are bounded by the model's context window
//   - Priority-ordered: eviction removes lowest-priority slots first
//   - Snapshot-saveable: can be serialized for agent switching
//   - Source-tracked: knows whether each slot came from recall, user, tools, etc.
//
// The WorkingMemory struct does NOT do database I/O directly — it works purely
// in memory. Persistence of snapshots is handled by the schema/store layer.

use crate::atoms::engram_types::{WorkingMemorySlot, WorkingMemorySnapshot, WorkingMemorySource};
use crate::engine::engram::tokenizer::Tokenizer;

/// The core working memory manager.
///
/// Thread-safety: NOT internally synchronized. Callers must wrap in Arc<Mutex<_>>.
pub struct WorkingMemory {
    /// The active slots, sorted by priority (highest first for fast access).
    slots: Vec<WorkingMemorySlot>,
    /// Maximum token budget for all slots combined.
    token_budget: usize,
    /// Current total token cost across all slots.
    current_tokens: usize,
    /// The tokenizer for counting.
    tokenizer: Tokenizer,
    /// Agent ID this working memory belongs to.
    agent_id: String,
    /// Momentum embeddings for trajectory-aware recall.
    /// Stores the last N query embeddings to predict conversation direction.
    momentum_embeddings: Vec<Vec<f32>>,
    /// Maximum number of momentum embeddings to keep.
    max_momentum: usize,
}

impl WorkingMemory {
    /// Create a new working memory with the given token budget.
    pub fn new(agent_id: String, token_budget: usize, tokenizer: Tokenizer) -> Self {
        Self {
            slots: Vec::new(),
            token_budget,
            current_tokens: 0,
            tokenizer,
            agent_id,
            momentum_embeddings: Vec::new(),
            max_momentum: 5,
        }
    }

    /// Insert a slot into working memory. If the budget is exceeded,
    /// low-priority slots are evicted to make room.
    ///
    /// Returns the list of evicted slots (if any).
    pub fn insert(&mut self, slot: WorkingMemorySlot) -> Vec<WorkingMemorySlot> {
        let mut evicted = Vec::new();

        // If this single slot exceeds the total budget, reject it
        if slot.token_cost > self.token_budget {
            log::warn!(
                "Working memory: slot ({} tokens) exceeds total budget ({}), skipping",
                slot.token_cost,
                self.token_budget
            );
            return evicted;
        }

        // Evict lowest-priority slots until we have room
        while self.current_tokens + slot.token_cost > self.token_budget && !self.slots.is_empty() {
            if let Some(removed) = self.evict_lowest() {
                evicted.push(removed);
            }
        }

        self.current_tokens += slot.token_cost;
        self.slots.push(slot);
        // Re-sort by priority descending
        self.slots.sort_by(|a, b| {
            b.priority
                .partial_cmp(&a.priority)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        evicted
    }

    /// Insert content from a recall operation.
    pub fn insert_recall(
        &mut self,
        memory_id: String,
        content: String,
        relevance_score: f32,
    ) -> Vec<WorkingMemorySlot> {
        let token_cost = self.tokenizer.count_tokens(&content);
        let slot = WorkingMemorySlot {
            memory_id: Some(memory_id),
            content,
            source: WorkingMemorySource::Recall,
            loaded_at: chrono::Utc::now().to_rfc3339(),
            priority: relevance_score,
            token_cost,
        };
        self.insert(slot)
    }

    /// Insert content from the sensory buffer (recently evicted messages).
    pub fn insert_sensory(
        &mut self,
        content: String,
        recency_score: f32,
    ) -> Vec<WorkingMemorySlot> {
        let token_cost = self.tokenizer.count_tokens(&content);
        let slot = WorkingMemorySlot {
            memory_id: None,
            content,
            source: WorkingMemorySource::SensoryBuffer,
            loaded_at: chrono::Utc::now().to_rfc3339(),
            priority: recency_score,
            token_cost,
        };
        self.insert(slot)
    }

    /// Insert content from a user mention (highest priority — user explicitly referenced it).
    pub fn insert_user_mention(&mut self, content: String) -> Vec<WorkingMemorySlot> {
        let token_cost = self.tokenizer.count_tokens(&content);
        let slot = WorkingMemorySlot {
            memory_id: None,
            content,
            source: WorkingMemorySource::UserMention,
            loaded_at: chrono::Utc::now().to_rfc3339(),
            priority: 1.0, // Maximum priority
            token_cost,
        };
        self.insert(slot)
    }

    /// Insert a tool result.
    pub fn insert_tool_result(&mut self, content: String, priority: f32) -> Vec<WorkingMemorySlot> {
        let token_cost = self.tokenizer.count_tokens(&content);
        let slot = WorkingMemorySlot {
            memory_id: None,
            content,
            source: WorkingMemorySource::ToolResult,
            loaded_at: chrono::Utc::now().to_rfc3339(),
            priority,
            token_cost,
        };
        self.insert(slot)
    }

    /// Get all slots in priority order (highest first).
    pub fn slots(&self) -> &[WorkingMemorySlot] {
        &self.slots
    }

    /// Get total token usage.
    pub fn token_usage(&self) -> usize {
        self.current_tokens
    }

    /// Get remaining token budget.
    pub fn remaining_budget(&self) -> usize {
        self.token_budget.saturating_sub(self.current_tokens)
    }

    /// Get the token budget.
    pub fn token_budget(&self) -> usize {
        self.token_budget
    }

    /// Update the token budget (e.g., when switching models).
    pub fn set_token_budget(&mut self, new_budget: usize) {
        self.token_budget = new_budget;
        // Evict if we're now over budget
        while self.current_tokens > self.token_budget && !self.slots.is_empty() {
            self.evict_lowest();
        }
    }

    /// Get the number of active slots.
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    /// Check if working memory is empty.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Remove a specific slot by memory_id.
    /// Returns the removed slot if found.
    pub fn remove_by_id(&mut self, memory_id: &str) -> Option<WorkingMemorySlot> {
        if let Some(pos) = self
            .slots
            .iter()
            .position(|s| s.memory_id.as_deref() == Some(memory_id))
        {
            let removed = self.slots.remove(pos);
            self.current_tokens = self.current_tokens.saturating_sub(removed.token_cost);
            Some(removed)
        } else {
            None
        }
    }

    /// Boost priority of a slot by memory_id.
    /// Used when the user references something already in working memory.
    pub fn boost_priority(&mut self, memory_id: &str, boost: f32) {
        if let Some(slot) = self
            .slots
            .iter_mut()
            .find(|s| s.memory_id.as_deref() == Some(memory_id))
        {
            slot.priority = (slot.priority + boost).min(1.0);
        }
        // Re-sort
        self.slots.sort_by(|a, b| {
            b.priority
                .partial_cmp(&a.priority)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    /// Decay all slot priorities by a factor (e.g., 0.95 per turn).
    /// This ensures old unreferenced memories gradually lose priority.
    pub fn decay_priorities(&mut self, factor: f32) {
        for slot in &mut self.slots {
            slot.priority *= factor;
        }
    }

    /// Add a query embedding to the momentum vector.
    pub fn push_momentum(&mut self, embedding: Vec<f32>) {
        self.momentum_embeddings.push(embedding);
        if self.momentum_embeddings.len() > self.max_momentum {
            self.momentum_embeddings.remove(0);
        }
    }

    /// Get the momentum embeddings (for trajectory-aware recall).
    pub fn momentum(&self) -> &[Vec<f32>] {
        &self.momentum_embeddings
    }

    /// Format all working memory slots as context for the model.
    /// Returns content in priority order (highest first).
    pub fn format_for_context(&self) -> String {
        if self.slots.is_empty() {
            return String::new();
        }

        let mut parts = Vec::with_capacity(self.slots.len());
        for slot in &self.slots {
            let source_tag = match slot.source {
                WorkingMemorySource::Recall => "[recalled]",
                WorkingMemorySource::UserMention => "[user-referenced]",
                WorkingMemorySource::SensoryBuffer => "[recent]",
                WorkingMemorySource::ToolResult => "[tool-result]",
                WorkingMemorySource::Restored => "[restored]",
            };
            parts.push(format!("{} {}", source_tag, slot.content));
        }

        parts.join("\n\n")
    }

    /// Create a serializable snapshot of the current state.
    /// Used for saving before agent switches or session end.
    pub fn snapshot(&self) -> WorkingMemorySnapshot {
        WorkingMemorySnapshot {
            agent_id: self.agent_id.clone(),
            slots: self.slots.clone(),
            momentum_embeddings: self.momentum_embeddings.clone(),
            saved_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Restore from a snapshot. This replaces existing state.
    pub fn restore(&mut self, snapshot: WorkingMemorySnapshot) {
        self.slots = snapshot.slots;
        self.momentum_embeddings = snapshot.momentum_embeddings;
        self.current_tokens = self.slots.iter().map(|s| s.token_cost).sum();

        // Mark all restored slots
        for slot in &mut self.slots {
            slot.source = WorkingMemorySource::Restored;
        }

        // Re-sort by priority
        self.slots.sort_by(|a, b| {
            b.priority
                .partial_cmp(&a.priority)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Evict if over budget after restore
        while self.current_tokens > self.token_budget && !self.slots.is_empty() {
            self.evict_lowest();
        }
    }

    /// Clear all slots.
    pub fn clear(&mut self) {
        self.slots.clear();
        self.current_tokens = 0;
    }

    // ── Internal ────────────────────────────────────────────────────────

    /// Evict the lowest-priority slot. Returns it if one was evicted.
    fn evict_lowest(&mut self) -> Option<WorkingMemorySlot> {
        if self.slots.is_empty() {
            return None;
        }
        // Slots are sorted by priority descending, so lowest is last
        let removed = self.slots.pop().unwrap();
        self.current_tokens = self.current_tokens.saturating_sub(removed.token_cost);
        log::debug!(
            "Working memory: evicted slot ({} tokens, priority {:.2})",
            removed.token_cost,
            removed.priority
        );
        Some(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atoms::engram_types::TokenizerType;

    fn make_wm(budget: usize) -> WorkingMemory {
        let tokenizer = Tokenizer::new(TokenizerType::Heuristic);
        WorkingMemory::new("test-agent".into(), budget, tokenizer)
    }

    #[test]
    fn test_insert_and_count() {
        let mut wm = make_wm(10_000);
        assert!(wm.is_empty());

        wm.insert_recall("m1".into(), "Hello world".into(), 0.8);
        assert_eq!(wm.slot_count(), 1);
        assert!(!wm.is_empty());
        assert!(wm.token_usage() > 0);
    }

    #[test]
    fn test_priority_ordering() {
        let mut wm = make_wm(10_000);

        wm.insert_recall("low".into(), "low priority".into(), 0.2);
        wm.insert_recall("high".into(), "high priority".into(), 0.9);
        wm.insert_recall("mid".into(), "mid priority".into(), 0.5);

        let slots = wm.slots();
        assert!(slots[0].priority > slots[1].priority);
        assert!(slots[1].priority > slots[2].priority);
    }

    #[test]
    fn test_eviction_on_budget_overflow() {
        // Very small budget — only room for ~1 slot
        let mut wm = make_wm(10);

        wm.insert_recall("m1".into(), "first memory".into(), 0.3);
        let evicted = wm.insert_recall("m2".into(), "second memory".into(), 0.8);

        // The lower-priority slot should have been evicted
        assert!(!evicted.is_empty());
        assert_eq!(evicted[0].memory_id.as_deref(), Some("m1"));
    }

    #[test]
    fn test_remove_by_id() {
        let mut wm = make_wm(10_000);
        wm.insert_recall("m1".into(), "memory one".into(), 0.5);
        wm.insert_recall("m2".into(), "memory two".into(), 0.7);

        let removed = wm.remove_by_id("m1");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().memory_id.as_deref(), Some("m1"));
        assert_eq!(wm.slot_count(), 1);
    }

    #[test]
    fn test_boost_priority() {
        let mut wm = make_wm(10_000);
        wm.insert_recall("m1".into(), "memory one".into(), 0.3);
        wm.insert_recall("m2".into(), "memory two".into(), 0.7);

        wm.boost_priority("m1", 0.6);

        // m1 should now be higher priority (0.9) than m2 (0.7)
        let slots = wm.slots();
        assert_eq!(slots[0].memory_id.as_deref(), Some("m1"));
    }

    #[test]
    fn test_decay_priorities() {
        let mut wm = make_wm(10_000);
        wm.insert_recall("m1".into(), "memory".into(), 1.0);

        wm.decay_priorities(0.9);

        let slots = wm.slots();
        assert!((slots[0].priority - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_snapshot_and_restore() {
        let mut wm = make_wm(10_000);
        wm.insert_recall("m1".into(), "memory one".into(), 0.8);
        wm.push_momentum(vec![0.1, 0.2, 0.3]);

        let snap = wm.snapshot();
        assert_eq!(snap.agent_id, "test-agent");
        assert_eq!(snap.slots.len(), 1);
        assert_eq!(snap.momentum_embeddings.len(), 1);

        // Create a fresh WM and restore
        let mut wm2 = make_wm(10_000);
        wm2.restore(snap);

        assert_eq!(wm2.slot_count(), 1);
        assert_eq!(wm2.momentum().len(), 1);
        // Restored slots should be marked as Restored
        assert!(matches!(
            wm2.slots()[0].source,
            WorkingMemorySource::Restored
        ));
    }

    #[test]
    fn test_set_budget_shrinks() {
        let mut wm = make_wm(10_000);
        wm.insert_recall("m1".into(), "first".into(), 0.3);
        wm.insert_recall("m2".into(), "second".into(), 0.9);

        // Shrink budget to force eviction
        wm.set_token_budget(5);

        // At least one slot should have been evicted
        assert!(wm.slot_count() <= 1);
    }

    #[test]
    fn test_format_for_context() {
        let mut wm = make_wm(10_000);
        wm.insert_recall("m1".into(), "The capital of France is Paris".into(), 0.8);
        wm.insert_user_mention("Remember I prefer dark mode".into());

        let ctx = wm.format_for_context();
        assert!(ctx.contains("[user-referenced]"));
        assert!(ctx.contains("[recalled]"));
        assert!(ctx.contains("Paris"));
    }

    #[test]
    fn test_remaining_budget() {
        let mut wm = make_wm(1000);
        let before = wm.remaining_budget();
        assert_eq!(before, 1000);

        wm.insert_recall("m1".into(), "some content".into(), 0.5);
        assert!(wm.remaining_budget() < before);
    }

    #[test]
    fn test_insert_sensory() {
        let mut wm = make_wm(10_000);
        wm.insert_sensory(
            "User asked about weather\nAssistant: It's sunny".into(),
            0.6,
        );
        assert_eq!(wm.slot_count(), 1);
        assert!(matches!(
            wm.slots()[0].source,
            WorkingMemorySource::SensoryBuffer
        ));
    }

    #[test]
    fn test_momentum() {
        let mut wm = make_wm(10_000);
        assert!(wm.momentum().is_empty());

        for i in 0..7 {
            wm.push_momentum(vec![i as f32]);
        }

        // Should cap at max_momentum (5)
        assert_eq!(wm.momentum().len(), 5);
        // Latest should be the last pushed
        assert_eq!(wm.momentum()[4], vec![6.0]);
    }

    #[test]
    fn test_clear() {
        let mut wm = make_wm(10_000);
        wm.insert_recall("m1".into(), "data".into(), 0.5);
        wm.clear();
        assert!(wm.is_empty());
        assert_eq!(wm.token_usage(), 0);
    }
}
