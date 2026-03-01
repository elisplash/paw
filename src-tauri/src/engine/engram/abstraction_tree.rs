// ── Engram: Hierarchical Semantic Compression — Abstraction Tree (§42) ──────
//
// Multi-level hierarchical compression of the memory store.
//
// 4-Level Architecture:
//   L0: Individual memories (episodic + semantic) — full detail
//   L1: Cluster summaries — groups of related memories compressed into
//       single paragraphs (typically 5-15 memories per cluster)
//   L2: Domain summaries — aggregation of L1 clusters into topic-level
//       overviews (using domain labels from meta_cognition)
//   L3: Global summary — single paragraph capturing the agent's overall
//       knowledge state
//
// Token Budget Selection:
//   Given a token budget, the tree chooses the most informative level:
//     - Budget ≥ L0 total tokens → use L0 (full detail)
//     - Budget ≥ L1 total tokens → use L1 (cluster summaries)
//     - Budget ≥ L2 total tokens → use L2 (domain summaries)
//     - Otherwise → use L3 (global summary)
//
// Fallback Packing:
//   When budget is between two levels, the tree uses the lower level
//   and fills remaining budget with the highest-priority items from the
//   level above (greedy by recency × importance).
//
// Rebuild is triggered during consolidation or when memory count changes
// significantly (>20% since last rebuild).

use crate::atoms::engram_types::{
    AbstractionLevel, AbstractionNode, AbstractionTree, MemoryScope, RetrievedMemory,
};
use crate::atoms::error::EngineResult;
use crate::engine::engram::tokenizer::Tokenizer;
use crate::engine::sessions::SessionStore;
use chrono::Utc;
use log::info;
use std::collections::HashMap;

// ═════════════════════════════════════════════════════════════════════════════
// Constants
// ═════════════════════════════════════════════════════════════════════════════

/// Cluster size target for L1 compression.
const L1_CLUSTER_SIZE: usize = 8;

/// Maximum L1 nodes (prevents runaway trees).
const MAX_L1_NODES: usize = 100;

/// Maximum L2 domain nodes.
const MAX_L2_NODES: usize = 20;

/// Maximum summary length (characters) for L1 cluster summaries.
const L1_SUMMARY_MAX_CHARS: usize = 500;

/// Maximum summary length (characters) for L2 domain summaries.
const L2_SUMMARY_MAX_CHARS: usize = 300;

/// Maximum summary length (characters) for L3 global summary.
const L3_SUMMARY_MAX_CHARS: usize = 200;

// ═════════════════════════════════════════════════════════════════════════════
// Tree Builder
// ═════════════════════════════════════════════════════════════════════════════

/// Build a full AbstractionTree from a set of retrieved memories.
///
/// This is a pure function — it does not touch the database.
/// The caller provides the memories to compress.
///
/// Compression is heuristic (extractive): summaries are built by
/// taking the highest-importance sentences from each cluster.
/// When an LLM is available, these should be replaced with abstractive summaries.
pub fn build_tree(memories: &[RetrievedMemory], tokenizer: &Tokenizer) -> AbstractionTree {
    if memories.is_empty() {
        return AbstractionTree {
            levels: Vec::new(),
            last_rebuilt: Utc::now().to_rfc3339(),
        };
    }

    // ── L0: Individual memories ──────────────────────────────────────
    let l0_nodes: Vec<AbstractionNode> = memories
        .iter()
        .map(|m| AbstractionNode {
            id: m.memory_id.clone(),
            summary: m.content.clone(),
            token_count: m.token_cost,
            children: vec![],
        })
        .collect();
    let l0_tokens: usize = l0_nodes.iter().map(|n| n.token_count).sum();

    // ── L1: Cluster summaries ────────────────────────────────────────
    let clusters = cluster_by_category(memories, L1_CLUSTER_SIZE);
    let l1_nodes: Vec<AbstractionNode> = clusters
        .iter()
        .take(MAX_L1_NODES)
        .enumerate()
        .map(|(idx, cluster)| {
            let summary = extractive_summarize(cluster, L1_SUMMARY_MAX_CHARS);
            let token_count = tokenizer.count_tokens(&summary);
            let children: Vec<String> = cluster.iter().map(|m| m.memory_id.clone()).collect();
            AbstractionNode {
                id: format!("L1-{}", idx),
                summary,
                token_count,
                children,
            }
        })
        .collect();
    let l1_tokens: usize = l1_nodes.iter().map(|n| n.token_count).sum();

    // ── L2: Domain summaries ─────────────────────────────────────────
    let domain_groups = group_l1_by_domain(&l1_nodes);
    let l2_nodes: Vec<AbstractionNode> = domain_groups
        .iter()
        .take(MAX_L2_NODES)
        .map(|(domain_label, l1_ids)| {
            // Find L1 summaries and compress further
            let l1_summaries: Vec<&str> = l1_ids
                .iter()
                .filter_map(|id| l1_nodes.iter().find(|n| &n.id == id))
                .map(|n| n.summary.as_str())
                .collect();
            let summary = compress_summaries(&l1_summaries, L2_SUMMARY_MAX_CHARS);
            let token_count = tokenizer.count_tokens(&summary);
            AbstractionNode {
                id: format!("L2-{}", domain_label),
                summary,
                token_count,
                children: l1_ids.clone(),
            }
        })
        .collect();
    let l2_tokens: usize = l2_nodes.iter().map(|n| n.token_count).sum();

    // ── L3: Global summary ───────────────────────────────────────────
    let all_l2_summaries: Vec<&str> = l2_nodes.iter().map(|n| n.summary.as_str()).collect();
    let global_summary = compress_summaries(&all_l2_summaries, L3_SUMMARY_MAX_CHARS);
    let l3_token_count = tokenizer.count_tokens(&global_summary);
    let l3_children: Vec<String> = l2_nodes.iter().map(|n| n.id.clone()).collect();
    let l3_nodes = vec![AbstractionNode {
        id: "L3-global".to_string(),
        summary: global_summary,
        token_count: l3_token_count,
        children: l3_children,
    }];

    let levels = vec![
        AbstractionLevel {
            level: 0,
            nodes: l0_nodes,
            total_tokens: l0_tokens,
        },
        AbstractionLevel {
            level: 1,
            nodes: l1_nodes,
            total_tokens: l1_tokens,
        },
        AbstractionLevel {
            level: 2,
            nodes: l2_nodes,
            total_tokens: l2_tokens,
        },
        AbstractionLevel {
            level: 3,
            nodes: l3_nodes,
            total_tokens: l3_token_count,
        },
    ];

    info!(
        "[engram::abstraction] Built tree: L0={} tok, L1={} tok, L2={} tok, L3={} tok",
        l0_tokens, l1_tokens, l2_tokens, l3_token_count
    );

    AbstractionTree {
        levels,
        last_rebuilt: Utc::now().to_rfc3339(),
    }
}

/// Select the best abstraction level for a given token budget.
///
/// Returns the level index (0-3) and the nodes at that level.
/// If the budget fits between two levels, returns the lower level
/// (more compressed) — the caller should use `pack_with_fallback`
/// to fill remaining space with detail from the level above.
pub fn select_level(tree: &AbstractionTree, budget_tokens: usize) -> Option<usize> {
    if tree.levels.is_empty() {
        return None;
    }

    // Try from most detailed (L0) to most compressed (L3)
    for level in &tree.levels {
        if level.total_tokens <= budget_tokens {
            return Some(level.level);
        }
    }

    // Even L3 doesn't fit — use L3 anyway (truncated)
    Some(tree.levels.len() - 1)
}

/// Pack context using abstraction fallback.
///
/// Strategy:
///   1. Select the most compressed level that fits in budget
///   2. Include all nodes from that level
///   3. Fill remaining budget with highest-priority detail from one level up
///
/// Returns a list of (node_id, summary_text) pairs.
pub fn pack_with_fallback(tree: &AbstractionTree, budget_tokens: usize) -> Vec<(String, String)> {
    if tree.levels.is_empty() {
        return Vec::new();
    }

    let level_idx = select_level(tree, budget_tokens).unwrap_or(tree.levels.len() - 1);
    let base_level = &tree.levels[level_idx];

    let mut packed: Vec<(String, String)> = Vec::new();
    let mut used_tokens = 0;

    // ── Pack base level ──────────────────────────────────────────────
    for node in &base_level.nodes {
        if used_tokens + node.token_count <= budget_tokens {
            packed.push((node.id.clone(), node.summary.clone()));
            used_tokens += node.token_count;
        }
    }

    // ── Fill with finer detail from level above ─────────────────────
    if level_idx > 0 {
        let detail_level = &tree.levels[level_idx - 1];
        let remaining = budget_tokens.saturating_sub(used_tokens);

        if remaining > 10 {
            // Sort detail nodes by token count (smallest first → pack more)
            let mut detail_nodes: Vec<&AbstractionNode> = detail_level.nodes.iter().collect();
            detail_nodes.sort_by_key(|n| n.token_count);

            for node in detail_nodes {
                if used_tokens + node.token_count <= budget_tokens {
                    packed.push((node.id.clone(), node.summary.clone()));
                    used_tokens += node.token_count;
                }
            }
        }
    }

    packed
}

/// Rebuild the abstraction tree from the memory store.
///
/// Convenience wrapper that fetches memories and builds the tree.
pub async fn rebuild_tree(
    store: &SessionStore,
    scope: &MemoryScope,
    tokenizer: &Tokenizer,
) -> EngineResult<AbstractionTree> {
    use crate::atoms::engram_types::{CompressionLevel, MemoryType, TrustScore};

    info!("[engram::abstraction] Rebuilding abstraction tree");

    // Fetch episodic memories
    let episodics = store.engram_search_episodic_bm25("", scope, 500)?;

    // Convert to RetrievedMemory for uniform processing
    let memories: Vec<RetrievedMemory> = episodics
        .iter()
        .map(|(mem, score)| {
            let content = mem.content.full.clone();
            RetrievedMemory {
                token_cost: tokenizer.count_tokens(&content),
                content,
                compression_level: CompressionLevel::Full,
                memory_id: mem.id.clone(),
                memory_type: MemoryType::Episodic,
                trust_score: TrustScore {
                    relevance: *score as f32,
                    accuracy: 0.5,
                    freshness: 0.5,
                    utility: 0.5,
                },
                category: mem.category.clone(),
                created_at: mem.created_at.clone(),
            }
        })
        .collect();

    Ok(build_tree(&memories, tokenizer))
}

// ═════════════════════════════════════════════════════════════════════════════
// Clustering & Compression Helpers
// ═════════════════════════════════════════════════════════════════════════════

/// Cluster memories by category, falling back to sequential grouping.
fn cluster_by_category(
    memories: &[RetrievedMemory],
    target_size: usize,
) -> Vec<Vec<&RetrievedMemory>> {
    if memories.is_empty() {
        return Vec::new();
    }

    // Group by category
    let mut by_category: HashMap<String, Vec<&RetrievedMemory>> = HashMap::new();
    for mem in memories {
        let cat = if mem.category.is_empty() {
            "uncategorized".to_string()
        } else {
            mem.category.clone()
        };
        by_category.entry(cat).or_default().push(mem);
    }

    // Sub-divide large groups into target_size chunks
    let mut clusters = Vec::new();
    for (_cat, group) in by_category {
        for chunk in group.chunks(target_size) {
            clusters.push(chunk.to_vec());
        }
    }

    clusters
}

/// Extractive summarization: take the first N characters of joined content.
/// In production, this would be replaced by an LLM abstractive summary.
fn extractive_summarize(memories: &[&RetrievedMemory], max_chars: usize) -> String {
    let mut combined = String::new();
    for mem in memories {
        if !combined.is_empty() {
            combined.push_str("; ");
        }
        combined.push_str(&mem.content);
        if combined.len() >= max_chars {
            break;
        }
    }
    if combined.len() > max_chars {
        combined.truncate(max_chars);
        combined.push('…');
    }
    combined
}

/// Compress multiple summaries into a shorter text.
fn compress_summaries(summaries: &[&str], max_chars: usize) -> String {
    let mut combined = String::new();
    for s in summaries {
        if !combined.is_empty() {
            combined.push_str("; ");
        }
        combined.push_str(s);
        if combined.len() >= max_chars {
            break;
        }
    }
    if combined.len() > max_chars {
        combined.truncate(max_chars);
        combined.push('…');
    }
    combined
}

/// Group L1 nodes by the first word of their ID or first content word as domain proxy.
fn group_l1_by_domain(l1_nodes: &[AbstractionNode]) -> Vec<(String, Vec<String>)> {
    let mut groups: HashMap<String, Vec<String>> = HashMap::new();

    for node in l1_nodes {
        // Use first significant word of summary as domain label
        let domain = node
            .summary
            .split_whitespace()
            .find(|w| w.len() > 3)
            .unwrap_or("general")
            .to_lowercase();
        groups.entry(domain).or_default().push(node.id.clone());
    }

    groups.into_iter().collect()
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atoms::engram_types::{CompressionLevel, MemoryType, TrustScore};

    fn make_memory(id: &str, content: &str, category: Option<&str>) -> RetrievedMemory {
        let tokenizer = Tokenizer::heuristic();
        RetrievedMemory {
            token_cost: tokenizer.count_tokens(content),
            content: content.to_string(),
            compression_level: CompressionLevel::Full,
            memory_id: id.to_string(),
            memory_type: MemoryType::Episodic,
            trust_score: TrustScore {
                relevance: 0.8,
                accuracy: 0.7,
                freshness: 0.6,
                utility: 0.5,
            },
            category: category
                .map(|s| s.to_string())
                .unwrap_or_else(|| "general".to_string()),
            created_at: "2025-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn empty_tree() {
        let tokenizer = Tokenizer::heuristic();
        let tree = build_tree(&[], &tokenizer);
        assert!(tree.levels.is_empty());
    }

    #[test]
    fn tree_has_four_levels() {
        let tokenizer = Tokenizer::heuristic();
        let memories: Vec<RetrievedMemory> = (0..20)
            .map(|i| make_memory(
                &format!("m{}", i),
                &format!(
                    "Memory number {} describes a comprehensive exploration of Rust programming concepts \
                     including ownership, borrowing, lifetimes, trait implementations, and async runtime \
                     patterns that are essential for building safe concurrent systems, along with various \
                     practical examples and code snippets that demonstrate real-world usage in production \
                     applications deployed to cloud infrastructure environments number {}",
                    i, i
                ),
                Some("rust"),
            ))
            .collect();
        let tree = build_tree(&memories, &tokenizer);
        assert_eq!(tree.levels.len(), 4);
        // L0 (individual) should have more tokens than L1 (cluster summaries)
        assert!(
            tree.levels[0].total_tokens >= tree.levels[1].total_tokens,
            "L0={} should >= L1={}",
            tree.levels[0].total_tokens,
            tree.levels[1].total_tokens,
        );
    }

    #[test]
    fn select_level_prefers_detail() {
        let tokenizer = Tokenizer::heuristic();
        let memories: Vec<RetrievedMemory> = (0..10)
            .map(|i| make_memory(&format!("m{}", i), "Short memo", Some("general")))
            .collect();
        let tree = build_tree(&memories, &tokenizer);

        // Large budget → L0 (most detail)
        let level = select_level(&tree, 100_000);
        assert_eq!(level, Some(0));
    }

    #[test]
    fn select_level_falls_back_to_compressed() {
        let tokenizer = Tokenizer::heuristic();
        let memories: Vec<RetrievedMemory> = (0..10)
            .map(|i| make_memory(&format!("m{}", i), "A reasonably long memory about various programming topics and techniques that uses many tokens", Some("general")))
            .collect();
        let tree = build_tree(&memories, &tokenizer);

        // Tiny budget → L3 (most compressed)
        let level = select_level(&tree, 5);
        assert_eq!(level, Some(3));
    }

    #[test]
    fn pack_with_fallback_returns_content() {
        let tokenizer = Tokenizer::heuristic();
        let memories: Vec<RetrievedMemory> = (0..5)
            .map(|i| make_memory(&format!("m{}", i), "Test memory", Some("test")))
            .collect();
        let tree = build_tree(&memories, &tokenizer);
        let packed = pack_with_fallback(&tree, 100_000);
        assert!(!packed.is_empty());
    }
}
