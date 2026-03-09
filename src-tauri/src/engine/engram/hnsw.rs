// Paw Agent Engine — HNSW Vector Index
//
// Hierarchical Navigable Small World graph for approximate nearest-neighbor
// search on episodic memory embeddings. Replaces the O(n) brute-force scan
// in `sessions/engram.rs::engram_search_episodic_vector` with O(log n)
// approximate search.
//
// Architecture:
//   - Multi-layer graph where each layer is a navigable small-world graph
//   - Bottom layer (0) contains ALL nodes; higher layers contain progressively
//     fewer nodes (exponential decay controlled by `ml`)
//   - Insert: probabilistic level assignment, greedy search for nearest
//     neighbors at each layer, bidirectional connections
//   - Search: begin at top layer entry point, greedy descend, then
//     ef-bounded beam search on layer 0
//
// Thread safety: Arc<RwLock<HnswIndex>> — reads are concurrent, writes exclusive.
// The index lives in EngineState and is rebuilt from the DB on startup.

use log::{info, warn};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::{Arc, RwLock};

use crate::atoms::error::EngineResult;
use crate::engine::sessions::SessionStore;

// ═══════════════════════════════════════════════════════════════════════════
// Parameters
// ═══════════════════════════════════════════════════════════════════════════

/// Max bidirectional connections per node per layer.
const M: usize = 16;

/// Max connections at layer 0 (typically 2 × M).
const M_MAX0: usize = 32;

/// Search width during construction (larger = better recall, slower build).
const EF_CONSTRUCTION: usize = 200;

/// Default search width during query.
const DEFAULT_EF_SEARCH: usize = 64;

/// Level multiplier: 1 / ln(M).
fn ml() -> f64 {
    1.0 / (M as f64).ln()
}

// ═══════════════════════════════════════════════════════════════════════════
// Data structures
// ═══════════════════════════════════════════════════════════════════════════

/// A node in the HNSW graph.
#[allow(dead_code)]
struct HnswNode {
    /// Memory ID (maps back to episodic_memories.id).
    id: String,
    /// Embedding vector.
    vector: Vec<f32>,
    /// Neighbors per layer: layer → Vec<node_index>.
    neighbors: Vec<Vec<usize>>,
    /// Max layer this node exists on.
    level: usize,
}

/// In-memory HNSW index.
pub struct HnswIndex {
    nodes: Vec<HnswNode>,
    /// Index of node IDs for fast lookup.
    id_to_idx: HashMap<String, usize>,
    /// Entry point (index of the node with highest level).
    entry_point: Option<usize>,
    /// Maximum level in the graph.
    max_level: usize,
    /// Embedding dimensionality (set on first insert).
    dims: usize,
    /// Search width for queries.
    ef_search: usize,
}

/// A search result from the HNSW index.
#[derive(Debug, Clone)]
pub struct HnswResult {
    pub memory_id: String,
    pub similarity: f64,
}

/// Thread-safe shared HNSW index.
pub type SharedHnswIndex = Arc<RwLock<HnswIndex>>;

// ═══════════════════════════════════════════════════════════════════════════
// Distance
// ═══════════════════════════════════════════════════════════════════════════

#[inline]
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() {
        return 0.0;
    }
    let (mut dot, mut na, mut nb) = (0.0f64, 0.0f64, 0.0f64);
    for (x, y) in a.iter().zip(b.iter()) {
        let (xf, yf) = (*x as f64, *y as f64);
        dot += xf * yf;
        na += xf * xf;
        nb += yf * yf;
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom < 1e-12 {
        0.0
    } else {
        dot / denom
    }
}

/// Distance = 1 - cosine_similarity (lower is closer).
#[inline]
fn cosine_distance(a: &[f32], b: &[f32]) -> f64 {
    1.0 - cosine_similarity(a, b)
}

// ═══════════════════════════════════════════════════════════════════════════
// Implementation
// ═══════════════════════════════════════════════════════════════════════════

impl Default for HnswIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl HnswIndex {
    /// Create a new empty HNSW index.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            id_to_idx: HashMap::new(),
            entry_point: None,
            max_level: 0,
            dims: 0,
            ef_search: DEFAULT_EF_SEARCH,
        }
    }

    /// Set search width (higher = better recall, slower search).
    pub fn set_ef_search(&mut self, ef: usize) {
        self.ef_search = ef.max(1);
    }

    /// Number of indexed vectors.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Assign a random level for a new node using exponential decay.
    fn random_level(&self) -> usize {
        let r: f64 = rand::random::<f64>().max(1e-12);
        let level = (-r.ln() * ml()).floor() as usize;
        level.min(32) // safety cap
    }

    /// Insert a vector with its memory ID.
    /// If the ID already exists, the vector is updated.
    pub fn insert(&mut self, id: &str, vector: Vec<f32>) {
        if vector.is_empty() {
            return;
        }

        // Set dimensionality on first insert
        if self.dims == 0 {
            self.dims = vector.len();
        } else if vector.len() != self.dims {
            warn!(
                "[hnsw] Dimension mismatch: expected {}, got {} for '{}'. Skipping.",
                self.dims,
                vector.len(),
                id
            );
            return;
        }

        // If ID exists, update its vector (don't rebuild connections — incremental update)
        if let Some(&idx) = self.id_to_idx.get(id) {
            self.nodes[idx].vector = vector;
            return;
        }

        let new_level = self.random_level();
        let new_idx = self.nodes.len();

        let mut node = HnswNode {
            id: id.to_string(),
            vector,
            neighbors: Vec::with_capacity(new_level + 1),
            level: new_level,
        };
        for _ in 0..=new_level {
            node.neighbors.push(Vec::new());
        }

        self.nodes.push(node);
        self.id_to_idx.insert(id.to_string(), new_idx);

        // First node — becomes entry point
        if self.nodes.len() == 1 {
            self.entry_point = Some(0);
            self.max_level = new_level;
            return;
        }

        let ep = self.entry_point.unwrap();
        let mut current_ep = ep;

        // Phase 1: Greedy descent from top layer to new_level + 1
        let top_layer = self.max_level;
        for layer in (new_level + 1..=top_layer).rev() {
            let nearest = self.search_layer_greedy(current_ep, &self.nodes[new_idx].vector, layer);
            current_ep = nearest;
        }

        // Phase 2: Insert at each layer from new_level down to 0
        for layer in (0..=new_level.min(top_layer)).rev() {
            let m_max = if layer == 0 { M_MAX0 } else { M };

            // Find ef_construction nearest neighbors at this layer
            let neighbors = self.search_layer_ef(
                current_ep,
                &self.nodes[new_idx].vector,
                layer,
                EF_CONSTRUCTION,
            );

            // Select M closest neighbors
            let selected: Vec<usize> = neighbors
                .into_iter()
                .take(m_max)
                .map(|(idx, _dist)| idx)
                .collect();

            // Set this node's neighbors at this layer
            self.nodes[new_idx].neighbors[layer] = selected.clone();

            // Add bidirectional connections
            for &neighbor_idx in &selected {
                if self.nodes[neighbor_idx].neighbors.len() > layer {
                    self.nodes[neighbor_idx].neighbors[layer].push(new_idx);

                    // Prune if over capacity
                    if self.nodes[neighbor_idx].neighbors[layer].len() > m_max {
                        self.prune_connections(neighbor_idx, layer, m_max);
                    }
                }
            }

            // Update current entry point for next layer
            if !selected.is_empty() {
                current_ep = selected[0];
            }
        }

        // Update entry point if new node has higher level
        if new_level > self.max_level {
            self.entry_point = Some(new_idx);
            self.max_level = new_level;
        }
    }

    /// Remove a vector by memory ID.
    pub fn remove(&mut self, id: &str) -> bool {
        let Some(&idx) = self.id_to_idx.get(id) else {
            return false;
        };

        // Remove this node from all neighbors' connection lists
        for layer in 0..self.nodes[idx].neighbors.len() {
            let neighbors = self.nodes[idx].neighbors[layer].clone();
            for &neighbor_idx in &neighbors {
                if neighbor_idx < self.nodes.len()
                    && self.nodes[neighbor_idx].neighbors.len() > layer
                {
                    self.nodes[neighbor_idx].neighbors[layer].retain(|&n| n != idx);
                }
            }
        }

        // Mark as removed (don't actually remove to keep indices stable)
        self.nodes[idx].vector.clear();
        self.nodes[idx].neighbors.clear();
        self.id_to_idx.remove(id);

        // Update entry point if needed
        if self.entry_point == Some(idx) {
            self.entry_point = self.id_to_idx.values().copied().next();
        }

        true
    }

    /// Search for the k nearest neighbors of a query vector.
    pub fn search(&self, query: &[f32], k: usize, threshold: f64) -> Vec<HnswResult> {
        if self.is_empty() || query.len() != self.dims {
            return Vec::new();
        }

        let ep = match self.entry_point {
            Some(ep) => ep,
            None => return Vec::new(),
        };

        // Phase 1: Greedy descent from top layer to layer 1
        let mut current_ep = ep;
        for layer in (1..=self.max_level).rev() {
            current_ep = self.search_layer_greedy(current_ep, query, layer);
        }

        // Phase 2: Beam search at layer 0
        let candidates = self.search_layer_ef(current_ep, query, 0, self.ef_search.max(k));

        candidates
            .into_iter()
            .take(k)
            .filter(|(_idx, dist)| (1.0 - dist) >= threshold)
            .map(|(idx, dist)| HnswResult {
                memory_id: self.nodes[idx].id.clone(),
                similarity: 1.0 - dist, // convert distance back to similarity
            })
            .collect()
    }

    // ─── Internal helpers ────────────────────────────────────────────

    /// Greedy search: follow closest neighbor at a single layer.
    fn search_layer_greedy(&self, entry: usize, query: &[f32], layer: usize) -> usize {
        let mut current = entry;
        let mut best_dist = cosine_distance(&self.nodes[entry].vector, query);

        loop {
            let neighbors = if layer < self.nodes[current].neighbors.len() {
                &self.nodes[current].neighbors[layer]
            } else {
                break;
            };

            let mut improved = false;
            for &neighbor in neighbors {
                if neighbor >= self.nodes.len() || self.nodes[neighbor].vector.is_empty() {
                    continue;
                }
                let d = cosine_distance(&self.nodes[neighbor].vector, query);
                if d < best_dist {
                    best_dist = d;
                    current = neighbor;
                    improved = true;
                }
            }

            if !improved {
                break;
            }
        }

        current
    }

    /// Beam search at a single layer with ef width.
    /// Returns up to ef results sorted by distance (ascending = closest first).
    fn search_layer_ef(
        &self,
        entry: usize,
        query: &[f32],
        layer: usize,
        ef: usize,
    ) -> Vec<(usize, f64)> {
        let entry_dist = cosine_distance(&self.nodes[entry].vector, query);

        // Min-heap of candidates (closest first)
        let mut candidates: BinaryHeap<Reverse<(OrderedFloat, usize)>> = BinaryHeap::new();
        // Max-heap of results (furthest first for easy pruning)
        let mut results: BinaryHeap<(OrderedFloat, usize)> = BinaryHeap::new();
        let mut visited: HashSet<usize> = HashSet::new();

        candidates.push(Reverse((OrderedFloat(entry_dist), entry)));
        results.push((OrderedFloat(entry_dist), entry));
        visited.insert(entry);

        while let Some(Reverse((OrderedFloat(c_dist), c_idx))) = candidates.pop() {
            // If the closest candidate is further than the furthest result, stop
            let furthest = results
                .peek()
                .map(|(OrderedFloat(d), _)| *d)
                .unwrap_or(f64::MAX);
            if c_dist > furthest && results.len() >= ef {
                break;
            }

            let neighbors = if layer < self.nodes[c_idx].neighbors.len() {
                &self.nodes[c_idx].neighbors[layer]
            } else {
                continue;
            };

            for &neighbor in neighbors {
                if visited.contains(&neighbor)
                    || neighbor >= self.nodes.len()
                    || self.nodes[neighbor].vector.is_empty()
                {
                    continue;
                }
                visited.insert(neighbor);

                let n_dist = cosine_distance(&self.nodes[neighbor].vector, query);
                let furthest = results
                    .peek()
                    .map(|(OrderedFloat(d), _)| *d)
                    .unwrap_or(f64::MAX);

                if n_dist < furthest || results.len() < ef {
                    candidates.push(Reverse((OrderedFloat(n_dist), neighbor)));
                    results.push((OrderedFloat(n_dist), neighbor));
                    if results.len() > ef {
                        results.pop(); // remove furthest
                    }
                }
            }
        }

        // Collect and sort by distance
        let mut out: Vec<(usize, f64)> = results
            .into_iter()
            .map(|(OrderedFloat(d), idx)| (idx, d))
            .collect();
        out.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        out
    }

    /// Prune a node's connections at a layer to at most max_connections.
    fn prune_connections(&mut self, node_idx: usize, layer: usize, max_connections: usize) {
        if self.nodes[node_idx].neighbors.len() <= layer {
            return;
        }

        // Score each neighbor by distance to node
        let node_vec = self.nodes[node_idx].vector.clone();
        let mut scored: Vec<(usize, f64)> = self.nodes[node_idx].neighbors[layer]
            .iter()
            .filter(|&&n| n < self.nodes.len() && !self.nodes[n].vector.is_empty())
            .map(|&n| {
                let d = cosine_distance(&node_vec, &self.nodes[n].vector);
                (n, d)
            })
            .collect();

        scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(max_connections);

        self.nodes[node_idx].neighbors[layer] = scored.into_iter().map(|(idx, _)| idx).collect();
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Ordered float wrapper for BinaryHeap
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Clone, Copy)]
struct OrderedFloat(f64);

impl PartialEq for OrderedFloat {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}
impl Eq for OrderedFloat {}

impl PartialOrd for OrderedFloat {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for OrderedFloat {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0
            .partial_cmp(&other.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Build from DB
// ═══════════════════════════════════════════════════════════════════════════

/// Build the HNSW index from all episodic memories that have embeddings.
pub fn build_from_store(store: &SessionStore) -> EngineResult<HnswIndex> {
    let scope = crate::atoms::engram_types::MemoryScope {
        global: true,
        ..Default::default()
    };
    let all_memories = store.engram_list_episodic(&scope, None, 100_000)?;

    let mut index = HnswIndex::new();
    let mut inserted = 0usize;

    for mem in all_memories {
        if let Some(embedding) = mem.embedding {
            index.insert(&mem.id, embedding);
            inserted += 1;
        }
    }

    info!(
        "[hnsw] Built index: {} vectors, {} layers, dims={}",
        inserted,
        index.max_level + 1,
        index.dims
    );

    Ok(index)
}

/// Create a new thread-safe shared HNSW index.
pub fn new_shared() -> SharedHnswIndex {
    Arc::new(RwLock::new(HnswIndex::new()))
}

/// Rebuild the shared index from the database.
pub fn rebuild_shared(shared: &SharedHnswIndex, store: &SessionStore) -> EngineResult<()> {
    let new_index = build_from_store(store)?;
    let mut guard = shared
        .write()
        .map_err(|e| format!("HNSW lock poisoned: {}", e))?;
    *guard = new_index;
    Ok(())
}

/// Search the shared index.
pub fn search_shared(
    shared: &SharedHnswIndex,
    query: &[f32],
    k: usize,
    threshold: f64,
) -> Vec<HnswResult> {
    match shared.read() {
        Ok(guard) => guard.search(query, k, threshold),
        Err(e) => {
            warn!("[hnsw] Read lock poisoned: {}", e);
            Vec::new()
        }
    }
}

/// Insert a single vector into the shared index.
pub fn insert_shared(shared: &SharedHnswIndex, id: &str, vector: Vec<f32>) {
    match shared.write() {
        Ok(mut guard) => guard.insert(id, vector),
        Err(e) => warn!("[hnsw] Write lock poisoned: {}", e),
    }
}

/// Remove a vector from the shared index.
pub fn remove_shared(shared: &SharedHnswIndex, id: &str) {
    match shared.write() {
        Ok(mut guard) => {
            guard.remove(id);
        }
        Err(e) => warn!("[hnsw] Write lock poisoned: {}", e),
    }
}

/// O(1) check whether the shared index has any vectors.
pub fn is_empty_shared(shared: &SharedHnswIndex) -> bool {
    match shared.read() {
        Ok(guard) => guard.is_empty(),
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn random_vec(dims: usize) -> Vec<f32> {
        (0..dims).map(|_| rand::random::<f32>() - 0.5).collect()
    }

    #[test]
    fn test_insert_and_search() {
        let mut index = HnswIndex::new();

        // Insert 100 random vectors
        for i in 0..100 {
            index.insert(&format!("mem-{}", i), random_vec(128));
        }

        assert_eq!(index.len(), 100);

        // Search for the first vector (should find itself as nearest)
        let query = index.nodes[0].vector.clone();
        let results = index.search(&query, 5, 0.0);

        assert!(!results.is_empty());
        assert_eq!(results[0].memory_id, "mem-0");
        assert!((results[0].similarity - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_remove() {
        let mut index = HnswIndex::new();
        index.insert("a", vec![1.0, 0.0, 0.0]);
        index.insert("b", vec![0.0, 1.0, 0.0]);
        index.insert("c", vec![0.0, 0.0, 1.0]);

        assert_eq!(index.len(), 3);
        assert!(index.remove("b"));
        assert_eq!(index.id_to_idx.len(), 2);

        let results = index.search(&[1.0, 0.0, 0.0], 5, 0.0);
        assert!(results.iter().all(|r| r.memory_id != "b"));
    }

    #[test]
    fn test_empty_search() {
        let index = HnswIndex::new();
        let results = index.search(&[1.0, 0.0], 5, 0.0);
        assert!(results.is_empty());
    }

    #[test]
    fn test_threshold_filter() {
        let mut index = HnswIndex::new();
        index.insert("a", vec![1.0, 0.0]);
        index.insert("b", vec![-1.0, 0.0]); // opposite direction

        let results = index.search(&[1.0, 0.0], 5, 0.9);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].memory_id, "a");
    }
}
