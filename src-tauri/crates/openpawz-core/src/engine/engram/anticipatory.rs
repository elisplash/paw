// ── Engram: Anticipatory Pre-loading (§8.8) ─────────────────────────────────
//
// Predictive memory pre-fetching based on conversation patterns.
// After each user message, we predict the next likely topic and pre-load
// relevant memories into working memory's sensory buffer.
//
// Strategy:
//   1. Track topic transitions (A→B, B→C) in a lightweight Markov chain
//   2. On topic detection, look up the most likely next topic
//   3. Pre-fetch top-k memories for predicted topic
//   4. Store in sensory buffer with low priority (will be evicted if wrong)

use log::info;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Maximum number of topic transitions to track.
const MAX_TRANSITIONS: usize = 500;

/// Number of memories to pre-fetch for predicted topics.
const PREFETCH_LIMIT: usize = 3;

/// Lightweight topic transition tracker (Markov chain).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TopicPredictor {
    /// Transition counts: "from\0to" → count.
    /// (Tuple keys don't serialize cleanly in JSON, so we use a delimited string key.)
    transitions: HashMap<String, u32>,
    /// Current topic (last observed).
    current_topic: Option<String>,
    /// Total transitions tracked.
    total_transitions: usize,
}

/// Separator for transition map keys (null byte — won't appear in topic names).
const KEY_SEP: char = '\0';

fn transition_key(from: &str, to: &str) -> String {
    format!("{}{}{}", from, KEY_SEP, to)
}

fn parse_transition_key(key: &str) -> Option<(&str, &str)> {
    key.split_once(KEY_SEP)
}

impl TopicPredictor {
    /// Create a new predictor.
    pub fn new() -> Self {
        Self::default()
    }

    /// Observe a topic transition.
    pub fn observe(&mut self, topic: &str) {
        let topic_str = topic.to_string();

        if let Some(ref prev) = self.current_topic {
            if prev != &topic_str {
                let key = transition_key(prev, &topic_str);
                *self.transitions.entry(key).or_insert(0) += 1;
                self.total_transitions += 1;

                // Prune if too large
                if self.total_transitions > MAX_TRANSITIONS {
                    self.prune();
                }
            }
        }

        self.current_topic = Some(topic_str);
    }

    /// Predict the most likely next topics given the current topic.
    /// Returns (topic, probability) pairs sorted by probability.
    pub fn predict_next(&self, limit: usize) -> Vec<(String, f32)> {
        let current = match &self.current_topic {
            Some(t) => t,
            None => return vec![],
        };

        let mut candidates: Vec<(String, u32)> = self
            .transitions
            .iter()
            .filter_map(|(key, count)| {
                let (from, to) = parse_transition_key(key)?;
                if from == current {
                    Some((to.to_string(), *count))
                } else {
                    None
                }
            })
            .collect();

        if candidates.is_empty() {
            return vec![];
        }

        let total: u32 = candidates.iter().map(|(_, c)| c).sum();
        candidates.sort_by(|a, b| b.1.cmp(&a.1));
        candidates.truncate(limit);

        candidates
            .into_iter()
            .map(|(topic, count)| (topic, count as f32 / total as f32))
            .collect()
    }

    /// Get the number of pre-fetch candidates for the current topic.
    pub fn prefetch_limit(&self) -> usize {
        PREFETCH_LIMIT
    }

    /// Prune old transitions to keep memory bounded.
    fn prune(&mut self) {
        // Keep only transitions with count >= 2 (eliminate singletons)
        self.transitions.retain(|_, count| *count >= 2);
        self.total_transitions = self.transitions.values().map(|c| *c as usize).sum();
    }

    /// Serialize the predictor state for persistence.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Restore from serialized state.
    pub fn from_json(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }
}

/// Build pre-fetch queries for predicted next topics.
/// Returns queries that callers should use to pre-load memories.
pub fn build_prefetch_queries(predictor: &TopicPredictor) -> Vec<PrefetchRequest> {
    let predictions = predictor.predict_next(2); // top 2 likely next topics
    let mut requests = Vec::new();

    for (topic, probability) in predictions {
        if probability >= 0.15 {
            // Only pre-fetch if ≥15% likely
            requests.push(PrefetchRequest {
                query: topic.clone(),
                limit: predictor.prefetch_limit(),
                priority: probability * 0.3, // Low priority — speculative
                reason: format!(
                    "Predicted topic shift to '{}' ({:.0}%)",
                    topic,
                    probability * 100.0
                ),
            });
        }
    }

    if !requests.is_empty() {
        info!(
            "[engram:prefetch] {} anticipatory pre-fetch queries queued",
            requests.len()
        );
    }

    requests
}

/// A request to pre-fetch memories for a predicted topic.
#[derive(Debug, Clone)]
pub struct PrefetchRequest {
    /// The search query (predicted next topic).
    pub query: String,
    /// Maximum memories to fetch.
    pub limit: usize,
    /// Priority to assign pre-fetched memories in working memory (low, speculative).
    pub priority: f32,
    /// Human-readable reason for the pre-fetch.
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_predictor_basic() {
        let mut p = TopicPredictor::new();
        p.observe("python");
        p.observe("rust");
        p.observe("python");
        p.observe("rust"); // python→rust seen twice
        p.observe("typescript");

        let predictions = p.predict_next(3);
        // Currently at "typescript", no transitions from typescript
        assert!(predictions.is_empty());

        // Go back to rust
        p.observe("rust");
        p.observe("python");
        let predictions = p.predict_next(3);
        // From python → rust (2 times), python → should not include python→typescript (pruned if only once)
        assert!(!predictions.is_empty());
    }

    #[test]
    fn test_predictor_serialization() {
        let mut p = TopicPredictor::new();
        p.observe("A");
        p.observe("B");
        p.observe("C");

        let json = p.to_json();
        let restored = TopicPredictor::from_json(&json);
        assert_eq!(restored.current_topic, Some("C".to_string()));
    }

    #[test]
    fn test_build_prefetch_empty() {
        let p = TopicPredictor::new();
        let requests = build_prefetch_queries(&p);
        assert!(requests.is_empty());
    }
}
