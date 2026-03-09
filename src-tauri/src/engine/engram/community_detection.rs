// ── Engram: GraphRAG Community Detection (Louvain Method) ───────────────────
//
// Discovers topic communities in the memory graph using Louvain modularity
// optimization. Communities represent clusters of densely-connected memories
// that share a topic, entity, or semantic domain.
//
// Integration points:
//   - Consolidation pipeline: rebuild communities after semantic extraction
//   - Search: boost results from the same community as query context
//   - Meta-cognition: populate KnowledgeDomain with community info
//
// Algorithm: Louvain (phase 1 only — local modularity optimization)
//   1. Load all edges from memory_edges table
//   2. Initialize each node in its own community
//   3. For each node, try moving to neighbor's community → pick best ΔQ
//   4. Repeat until no moves improve modularity
//   5. Assign community labels and generate summaries

use crate::atoms::engram_types::KnowledgeDomain;
use crate::atoms::error::EngineResult;
use crate::engine::sessions::SessionStore;
use log::info;
use std::collections::HashMap;

// ═════════════════════════════════════════════════════════════════════════════
// Constants
// ═════════════════════════════════════════════════════════════════════════════

/// Minimum community size to report (singletons are noise).
const MIN_COMMUNITY_SIZE: usize = 2;

/// Minimum modularity gain to continue iterating (convergence threshold).
const MIN_MODULARITY_GAIN: f64 = 1e-6;

/// Maximum iterations for Louvain (prevent infinite loops).
const MAX_ITERATIONS: usize = 50;

// ═════════════════════════════════════════════════════════════════════════════
// Types
// ═════════════════════════════════════════════════════════════════════════════

/// A detected community in the memory graph.
#[derive(Debug, Clone)]
pub struct Community {
    /// Auto-generated community ID.
    pub id: usize,
    /// Memory IDs in this community.
    pub member_ids: Vec<String>,
    /// Total internal edge weight.
    pub internal_weight: f64,
    /// Auto-derived label from member content.
    pub label: String,
    /// Hierarchical summary built from member key_facts and content.
    /// Richer than the label — used by GraphRAG search for context injection.
    pub summary: String,
}

/// Report from a community detection run.
#[derive(Debug, Clone, Default)]
pub struct CommunityDetectionReport {
    /// Number of nodes (memories) in the graph.
    pub nodes: usize,
    /// Number of edges loaded.
    pub edges: usize,
    /// Number of communities discovered.
    pub communities_found: usize,
    /// Final modularity score (0.0 = random, 1.0 = perfect partition).
    pub modularity: f64,
    /// Number of Louvain iterations performed.
    pub iterations: usize,
}

// ═════════════════════════════════════════════════════════════════════════════
// Louvain Algorithm
// ═════════════════════════════════════════════════════════════════════════════

/// Edge representation for the Louvain graph.
struct Edge {
    source: usize,
    target: usize,
    weight: f64,
}

/// Run Louvain community detection on the memory graph.
///
/// Loads all edges from the DB, runs modularity optimization, and returns
/// the discovered communities along with a detection report.
///
/// Results are also stored in the `memory_communities` table and the
/// `community_id` column on episodic/semantic memories.
pub fn detect_communities(
    store: &SessionStore,
) -> EngineResult<(Vec<Community>, CommunityDetectionReport)> {
    let mut report = CommunityDetectionReport::default();

    // ── 1. Load graph ────────────────────────────────────────────────────
    let (edges, node_index, reverse_index) = load_graph(store)?;
    report.nodes = node_index.len();
    report.edges = edges.len();

    if report.nodes < MIN_COMMUNITY_SIZE || edges.is_empty() {
        return Ok((vec![], report));
    }

    let n = node_index.len();

    // ── 2. Build adjacency + degree structures ───────────────────────────
    // Adjacency list: node_idx -> [(neighbor_idx, weight)]
    let mut adj: Vec<Vec<(usize, f64)>> = vec![vec![]; n];
    let mut degree: Vec<f64> = vec![0.0; n]; // weighted degree (= sum of incident edge weights)
    let mut total_weight = 0.0f64;

    for edge in &edges {
        adj[edge.source].push((edge.target, edge.weight));
        adj[edge.target].push((edge.source, edge.weight));
        degree[edge.source] += edge.weight;
        degree[edge.target] += edge.weight;
        total_weight += edge.weight;
    }

    if total_weight < 1e-12 {
        return Ok((vec![], report));
    }

    let m2 = 2.0 * total_weight; // 2*m for modularity formula

    // ── 3. Initialize: each node in its own community ────────────────────
    let mut community: Vec<usize> = (0..n).collect();
    // Sum of degrees of nodes in each community
    let mut sigma_tot: Vec<f64> = degree.clone();
    // Sum of internal edges in each community
    let mut sigma_in: Vec<f64> = vec![0.0; n];

    // ── 4. Louvain iterative optimization ────────────────────────────────
    let mut improved = true;
    let mut iterations = 0usize;

    while improved && iterations < MAX_ITERATIONS {
        improved = false;
        iterations += 1;

        for i in 0..n {
            let current_comm = community[i];
            let ki = degree[i];

            // Compute edge weight from i to each neighboring community
            let mut comm_weights: HashMap<usize, f64> = HashMap::new();
            for &(j, w) in &adj[i] {
                *comm_weights.entry(community[j]).or_default() += w;
            }

            // Weight from i to its own community
            let ki_in = *comm_weights.get(&current_comm).unwrap_or(&0.0);

            // ΔQ for removing i from its current community
            let remove_cost = ki_in - sigma_tot[current_comm] * ki / m2;

            // Find the best community to move i into
            let mut best_comm = current_comm;
            let mut best_gain = 0.0f64;

            for (&candidate_comm, &ki_to_c) in &comm_weights {
                if candidate_comm == current_comm {
                    continue;
                }
                // ΔQ for adding i to candidate community
                let gain = ki_to_c - sigma_tot[candidate_comm] * ki / m2 - remove_cost;
                if gain > best_gain + MIN_MODULARITY_GAIN {
                    best_gain = gain;
                    best_comm = candidate_comm;
                }
            }

            // Move node if beneficial
            if best_comm != current_comm {
                // Remove from current community
                sigma_tot[current_comm] -= ki;
                sigma_in[current_comm] -= 2.0 * ki_in; // remove internal contribution

                // Add to best community
                let ki_to_best = *comm_weights.get(&best_comm).unwrap_or(&0.0);
                sigma_tot[best_comm] += ki;
                sigma_in[best_comm] += 2.0 * ki_to_best;

                community[i] = best_comm;
                improved = true;
            }
        }
    }

    report.iterations = iterations;

    // ── 5. Compute final modularity ──────────────────────────────────────
    report.modularity = compute_modularity(&community, &adj, &degree, total_weight);

    // ── 6. Collect communities ───────────────────────────────────────────
    let mut comm_members: HashMap<usize, Vec<usize>> = HashMap::new();
    for (node_idx, &comm_id) in community.iter().enumerate() {
        comm_members.entry(comm_id).or_default().push(node_idx);
    }

    let communities: Vec<Community> = comm_members
        .into_iter()
        .filter(|(_, members)| members.len() >= MIN_COMMUNITY_SIZE)
        .enumerate()
        .map(|(seq_id, (_, members))| {
            let member_ids: Vec<String> = members
                .iter()
                .map(|&idx| reverse_index[idx].clone())
                .collect();

            // Compute internal weight
            let member_set: std::collections::HashSet<usize> = members.iter().copied().collect();
            let mut internal_w = 0.0f64;
            for &node in &members {
                for &(neighbor, w) in &adj[node] {
                    if member_set.contains(&neighbor) {
                        internal_w += w;
                    }
                }
            }
            internal_w /= 2.0; // each internal edge counted twice

            Community {
                id: seq_id,
                member_ids,
                internal_weight: internal_w,
                label: String::new(),   // resolved below
                summary: String::new(), // resolved below
            }
        })
        .collect();

    report.communities_found = communities.len();

    // ── 7. Derive labels ─────────────────────────────────────────────────
    let communities = derive_labels(store, communities);

    // ── 8. Persist community assignments ─────────────────────────────────
    persist_communities(store, &communities)?;

    if report.communities_found > 0 {
        info!(
            "[engram:community] Detected {} communities (modularity={:.3}, {} iterations, {} nodes, {} edges)",
            report.communities_found, report.modularity, report.iterations, report.nodes, report.edges
        );
    }

    Ok((communities, report))
}

/// Compute modularity Q for a given partition.
/// Q = (1/2m) * Σ_ij [A_ij - k_i*k_j/(2m)] * δ(c_i, c_j)
fn compute_modularity(
    community: &[usize],
    adj: &[Vec<(usize, f64)>],
    degree: &[f64],
    total_weight: f64,
) -> f64 {
    if total_weight < 1e-12 {
        return 0.0;
    }
    let m2 = 2.0 * total_weight;
    let mut q = 0.0f64;

    for (i, neighbors) in adj.iter().enumerate() {
        for &(j, w) in neighbors {
            if community[i] == community[j] {
                q += w - degree[i] * degree[j] / m2;
            }
        }
    }

    q / m2
}

/// Load the memory graph from the DB into indexed edge list.
/// Returns (edges, node_id->index map, index->node_id vec).
#[allow(clippy::type_complexity)]
fn load_graph(
    store: &SessionStore,
) -> EngineResult<(Vec<Edge>, HashMap<String, usize>, Vec<String>)> {
    let conn = store.conn.lock();
    let mut node_index: HashMap<String, usize> = HashMap::new();
    let mut reverse_index: Vec<String> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();

    let mut stmt = conn.prepare("SELECT source_id, target_id, weight FROM memory_edges")?;

    let rows: Vec<(String, String, f64)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
            ))
        })?
        .filter_map(|r| r.ok())
        .collect();

    for (src, tgt, weight) in rows {
        let src_idx = *node_index.entry(src.clone()).or_insert_with(|| {
            let idx = reverse_index.len();
            reverse_index.push(src.clone());
            idx
        });
        let tgt_idx = *node_index.entry(tgt.clone()).or_insert_with(|| {
            let idx = reverse_index.len();
            reverse_index.push(tgt.clone());
            idx
        });
        edges.push(Edge {
            source: src_idx,
            target: tgt_idx,
            weight,
        });
    }

    Ok((edges, node_index, reverse_index))
}

/// Derive human-readable labels and hierarchical summaries for communities
/// by extracting common categories, top keywords, and aggregating member content.
///
/// Summary format (GraphRAG §7 hierarchical):
///   "Topic: {label}\n\nKey facts:\n- fact1\n- fact2\n...\n\nThemes: kw1, kw2, kw3"
///
/// This summary is used by community-augmented search to inject cluster context
/// instead of individual memories — enabling "global" queries over the knowledge graph.
fn derive_labels(store: &SessionStore, mut communities: Vec<Community>) -> Vec<Community> {
    let conn = store.conn.lock();

    for community in &mut communities {
        let mut category_counts: HashMap<String, usize> = HashMap::new();
        let mut word_counts: HashMap<String, usize> = HashMap::new();
        let mut key_facts: Vec<String> = Vec::new();

        // Sample up to 20 members for label derivation
        let sample: Vec<&String> = community.member_ids.iter().take(20).collect();

        for id in &sample {
            let ok = conn.query_row(
                "SELECT category, content_key_fact, content_summary FROM episodic_memories WHERE id = ?1",
                rusqlite::params![id],
                |row| {
                    let cat: String = row.get(0)?;
                    let kf: Option<String> = row.get(1)?;
                    let summary: Option<String> = row.get(2)?;
                    Ok((cat, kf, summary))
                },
            );
            if let Ok((category, key_fact, summary)) = ok {
                *category_counts.entry(category).or_default() += 1;

                // Collect key facts for hierarchical summary
                if let Some(ref kf) = key_fact {
                    if !kf.is_empty() && key_facts.len() < 10 {
                        key_facts.push(kf.clone());
                    }
                    for word in kf.split_whitespace() {
                        let w = word
                            .trim_matches(|c: char| !c.is_alphanumeric())
                            .to_lowercase();
                        if w.len() > 3 && !is_stop_word(&w) {
                            *word_counts.entry(w).or_default() += 1;
                        }
                    }
                }

                // Fall back to summary if no key_fact
                if key_fact.is_none() || key_fact.as_deref() == Some("") {
                    if let Some(ref s) = summary {
                        if !s.is_empty() && key_facts.len() < 10 {
                            key_facts.push(s.clone());
                        }
                    }
                }
            }
        }

        // Label = top category + top 2 keywords
        let top_category = category_counts
            .iter()
            .max_by_key(|(_, c)| *c)
            .map(|(k, _)| k.clone())
            .unwrap_or_else(|| "misc".to_string());

        let mut top_words: Vec<(String, usize)> = word_counts.into_iter().collect();
        top_words.sort_by(|a, b| b.1.cmp(&a.1));
        let keyword_part: Vec<&str> = top_words.iter().take(2).map(|(w, _)| w.as_str()).collect();

        community.label = if keyword_part.is_empty() {
            top_category.clone()
        } else {
            format!("{}: {}", top_category, keyword_part.join(", "))
        };

        // Build hierarchical summary (GraphRAG-style)
        let themes: Vec<&str> = top_words.iter().take(5).map(|(w, _)| w.as_str()).collect();
        let mut summary_parts = Vec::new();
        summary_parts.push(format!("Topic: {}", community.label));
        summary_parts.push(format!("Members: {} memories", community.member_ids.len()));
        if !key_facts.is_empty() {
            let facts_section: Vec<String> = key_facts.iter().map(|f| format!("- {}", f)).collect();
            summary_parts.push(format!("Key facts:\n{}", facts_section.join("\n")));
        }
        if !themes.is_empty() {
            summary_parts.push(format!("Themes: {}", themes.join(", ")));
        }
        community.summary = summary_parts.join("\n\n");
    }

    communities
}

/// Persist community assignments and summaries to the database.
/// Updates episodic_memories.community_id and stores community metadata
/// in the memory_communities table.
fn persist_communities(store: &SessionStore, communities: &[Community]) -> EngineResult<()> {
    let conn = store.conn.lock();

    // Clear existing assignments
    let _ = conn.execute("UPDATE episodic_memories SET community_id = NULL", []);
    let _ = conn.execute("DELETE FROM memory_communities", []);

    for community in communities {
        let comm_id_str = community.id.to_string();

        // Persist community metadata + summary
        conn.execute(
            "INSERT OR REPLACE INTO memory_communities (id, label, summary, member_count, internal_weight)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                comm_id_str,
                community.label,
                community.summary,
                community.member_ids.len(),
                community.internal_weight,
            ],
        )?;

        // Assign community to members
        for member_id in &community.member_ids {
            let _ = conn.execute(
                "UPDATE episodic_memories SET community_id = ?2 WHERE id = ?1",
                rusqlite::params![member_id, comm_id_str],
            );
        }
    }

    Ok(())
}

/// Convert communities to KnowledgeDomain entries for meta-cognition.
pub fn communities_to_domains(
    store: &SessionStore,
    communities: &[Community],
) -> Vec<KnowledgeDomain> {
    let now = chrono::Utc::now();
    let thirty_days_ago = now - chrono::Duration::days(30);
    let cutoff = thirty_days_ago.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    communities
        .iter()
        .map(|c| {
            let depth = c.member_ids.len();

            // Compute freshness: fraction of members created in last 30 days
            let fresh_count = {
                let conn = store.conn.lock();
                let placeholders: String = c.member_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                if placeholders.is_empty() {
                    0usize
                } else {
                    let query = format!(
                        "SELECT COUNT(*) FROM episodic_memories WHERE id IN ({}) AND created_at >= ?",
                        placeholders
                    );
                    let mut stmt = match conn.prepare(&query) {
                        Ok(s) => s,
                        Err(_) => return KnowledgeDomain {
                            label: c.label.clone(),
                            depth,
                            freshness: 0.5,
                            uncertainty: 0.0,
                            confidence: 0.5,
                            memory_ids: c.member_ids.clone(),
                        },
                    };
                    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = c.member_ids
                        .iter()
                        .map(|id| Box::new(id.clone()) as Box<dyn rusqlite::types::ToSql>)
                        .collect();
                    params.push(Box::new(cutoff.clone()));
                    stmt.query_row(rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())), |row| row.get::<_, usize>(0))
                        .unwrap_or(0)
                }
            };

            let freshness = if depth > 0 { fresh_count as f64 / depth as f64 } else { 0.0 };

            // Compute uncertainty: ratio of Contradicts edges within the community
            let contradiction_count = {
                let conn = store.conn.lock();
                let placeholders: String = c.member_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                if placeholders.is_empty() {
                    0usize
                } else {
                    let contradicts_token =
                        super::encryption::tokenize_edge_type("contradicts");
                    // Extra placeholder at end for the tokenized edge_type
                    let query = format!(
                        "SELECT COUNT(*) FROM memory_edges
                         WHERE edge_type = ?{extra}
                           AND source_id IN ({0}) AND target_id IN ({0})",
                        placeholders,
                        extra = c.member_ids.len() * 2 + 1
                    );
                    let mut stmt = match conn.prepare(&query) {
                        Ok(s) => s,
                        Err(_) => return KnowledgeDomain {
                            label: c.label.clone(),
                            depth,
                            freshness,
                            uncertainty: 0.0,
                            confidence: freshness * depth as f64,
                            memory_ids: c.member_ids.clone(),
                        },
                    };
                    // Need to bind member_ids twice (for both IN clauses), plus the token
                    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
                    for id in &c.member_ids {
                        params.push(Box::new(id.clone()));
                    }
                    for id in &c.member_ids {
                        params.push(Box::new(id.clone()));
                    }
                    params.push(Box::new(contradicts_token));
                    stmt.query_row(rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())), |row| row.get::<_, usize>(0))
                        .unwrap_or(0)
                }
            };

            let uncertainty = if depth > 1 {
                (contradiction_count as f64 / (depth as f64 - 1.0)).min(1.0)
            } else {
                0.0
            };

            let confidence = (depth as f64).ln().max(0.0) * freshness * (1.0 - uncertainty);

            KnowledgeDomain {
                label: c.label.clone(),
                depth,
                freshness,
                uncertainty,
                confidence,
                memory_ids: c.member_ids.clone(),
            }
        })
        .collect()
}

/// Simple stop word check for label generation.
fn is_stop_word(w: &str) -> bool {
    matches!(
        w,
        "the"
            | "and"
            | "for"
            | "that"
            | "this"
            | "with"
            | "from"
            | "have"
            | "been"
            | "will"
            | "would"
            | "could"
            | "should"
            | "about"
            | "into"
            | "they"
            | "their"
            | "there"
            | "what"
            | "when"
            | "where"
            | "which"
            | "while"
            | "also"
            | "just"
            | "like"
            | "more"
            | "than"
            | "then"
            | "very"
            | "some"
            | "other"
            | "each"
            | "every"
    )
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_modularity_single_community() {
        // All nodes in one community, complete triangle graph
        // For a complete graph K3 with all in one community:
        //   Q = (1/2m) * Σ [A_ij - ki*kj/2m] * δ(ci,cj)
        // With 3 edges (undirected), m=3, 2m=6, each node has degree 2
        // This gives Q = (1/6) * 3 * (1 - 4/6) = 1/6 ≈ 0.167... for directed
        // But our adjacency list is symmetric, so Q ≈ 1/3
        let community = vec![0, 0, 0];
        let adj = vec![
            vec![(1, 1.0), (2, 1.0)],
            vec![(0, 1.0), (2, 1.0)],
            vec![(0, 1.0), (1, 1.0)],
        ];
        let degree = vec![2.0, 2.0, 2.0];
        let q = compute_modularity(&community, &adj, &degree, 3.0);
        // Modularity is defined for the partitioning, not absolute;
        // a single community with dense connections has positive Q
        assert!(
            q >= 0.0 && q <= 0.5,
            "Single community Q should be modest, got {}",
            q
        );
    }

    #[test]
    fn test_compute_modularity_perfect_partition() {
        // Two disconnected cliques → modularity > 0
        let community = vec![0, 0, 1, 1];
        let adj = vec![
            vec![(1, 1.0)], // 0-1 connected
            vec![(0, 1.0)], // 1-0 connected
            vec![(3, 1.0)], // 2-3 connected
            vec![(2, 1.0)], // 3-2 connected
        ];
        let degree = vec![1.0, 1.0, 1.0, 1.0];
        let q = compute_modularity(&community, &adj, &degree, 2.0);
        assert!(
            q > 0.0,
            "Perfect partition should have positive modularity, got {}",
            q
        );
    }

    #[test]
    fn test_stop_words() {
        assert!(is_stop_word("the"));
        assert!(is_stop_word("with"));
        assert!(!is_stop_word("rust"));
        assert!(!is_stop_word("memory"));
    }
}
