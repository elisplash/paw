// ── Engram: Temporal-Axis Retrieval (§39) ────────────────────────────────────
//
// Time-aware memory search — find memories by WHEN they occurred,
// not just what they contain. Enables queries like:
//   "What happened last Tuesday?"
//   "Show me everything from the first week of the project"
//   "What were we discussing around 3pm yesterday?"
//
// Provides:
//   - Range queries (start..end)
//   - Proximity queries (near a point in time ± window)
//   - Pattern detection (recurring daily/weekly events)
//   - Burst detection (high-activity periods)
//   - Temporal clustering (group co-located memories)
//   - Recency-weighted RRF fusion signal

use crate::atoms::engram_types::{
    EpisodicMemory, MemoryScope, RetrievedMemory, TemporalCluster, TemporalPattern,
    TemporalQuery, TemporalSearchResult, TrustScore,
};
use crate::atoms::error::EngineResult;
use crate::engine::sessions::SessionStore;
use log::info;

// ═══════════════════════════════════════════════════════════════════════════
// Temporal Search
// ═══════════════════════════════════════════════════════════════════════════

/// Execute a temporal query — retrieve memories based on time criteria.
pub fn temporal_search(
    store: &SessionStore,
    query: &TemporalQuery,
    scope: &MemoryScope,
    limit: usize,
) -> EngineResult<TemporalSearchResult> {
    match query {
        TemporalQuery::Range { start, end } => search_range(store, start, end, scope, limit),
        TemporalQuery::Proximity {
            anchor,
            window_hours,
        } => search_proximity(store, anchor, *window_hours, scope, limit),
        TemporalQuery::Pattern { pattern } => search_pattern(store, *pattern, scope, limit),
        TemporalQuery::Recent { limit: n } => search_recent(store, scope, *n),
        TemporalQuery::Session { session_id } => search_session(store, session_id, scope, limit),
    }
}

/// Compute a recency score for RRF fusion.
/// Returns 0.0–1.0 where 1.0 = just now, decays over days.
/// This is used as an additional signal in the main search pipeline.
pub fn recency_score(created_at: &str, half_life_hours: f64) -> f64 {
    let now = chrono::Utc::now();
    let created = chrono::DateTime::parse_from_rfc3339(created_at)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(created_at, "%Y-%m-%dT%H:%M:%SZ")
                .map(|ndt| ndt.and_utc())
        })
        .unwrap_or(now);

    let hours_ago: f64 = (now - created).num_minutes() as f64 / 60.0;
    if hours_ago <= 0.0 {
        return 1.0;
    }
    let decay = (-hours_ago * (2.0_f64.ln()) / half_life_hours).exp();
    decay.clamp(0.0, 1.0)
}

/// Cluster temporally close memories into groups.
/// Uses a simple gap-based algorithm: memories within `gap_secs` of each other
/// are placed in the same cluster.
pub fn cluster_temporal(memories: &[RetrievedMemory], gap_secs: u64) -> Vec<TemporalCluster> {
    if memories.is_empty() {
        return vec![];
    }

    // Parse timestamps and sort
    let mut timestamped: Vec<(usize, i64)> = memories
        .iter()
        .enumerate()
        .filter_map(|(i, m)| {
            parse_timestamp(&m.created_at).map(|ts| (i, ts))
        })
        .collect();

    timestamped.sort_by_key(|(_, ts)| *ts);

    let mut clusters: Vec<TemporalCluster> = Vec::new();
    let mut current_ids: Vec<String> = Vec::new();
    let mut cluster_start = 0i64;
    let mut prev_ts = 0i64;

    for (idx, (i, ts)) in timestamped.iter().enumerate() {
        if idx == 0 || (*ts - prev_ts) > gap_secs as i64 {
            // Start a new cluster (flush previous if non-empty)
            if !current_ids.is_empty() {
                clusters.push(TemporalCluster {
                    centroid: format_timestamp((cluster_start + prev_ts) / 2),
                    memory_ids: current_ids.clone(),
                    window_secs: (prev_ts - cluster_start).unsigned_abs(),
                });
                current_ids.clear();
            }
            cluster_start = *ts;
        }
        current_ids.push(memories[*i].memory_id.clone());
        prev_ts = *ts;
    }

    // Flush last cluster
    if !current_ids.is_empty() {
        clusters.push(TemporalCluster {
            centroid: format_timestamp((cluster_start + prev_ts) / 2),
            memory_ids: current_ids,
            window_secs: (prev_ts - cluster_start).unsigned_abs(),
        });
    }

    clusters
}

// ═══════════════════════════════════════════════════════════════════════════
// Internal search implementations
// ═══════════════════════════════════════════════════════════════════════════

fn search_range(
    store: &SessionStore,
    start: &str,
    end: &str,
    scope: &MemoryScope,
    limit: usize,
) -> EngineResult<TemporalSearchResult> {
    let memories = store.engram_search_episodic_temporal_range(start, end, scope, limit)?;
    let retrieved = memories_to_retrieved(&memories);
    let clusters = cluster_temporal(&retrieved, 3600); // 1-hour gap
    let (span_start, span_end) = span_bounds(&retrieved, start, end);

    info!(
        "[engram:temporal] Range {start}..{end} → {} memories, {} clusters",
        retrieved.len(),
        clusters.len()
    );

    Ok(TemporalSearchResult {
        memories: retrieved,
        clusters,
        span_start,
        span_end,
    })
}

fn search_proximity(
    store: &SessionStore,
    anchor: &str,
    window_hours: f64,
    scope: &MemoryScope,
    limit: usize,
) -> EngineResult<TemporalSearchResult> {
    let anchor_dt = parse_datetime(anchor);
    let window = chrono::Duration::minutes((window_hours * 60.0) as i64);
    let start = (anchor_dt - window).format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let end = (anchor_dt + window).format("%Y-%m-%dT%H:%M:%SZ").to_string();

    search_range(store, &start, &end, scope, limit)
}

fn search_pattern(
    store: &SessionStore,
    pattern: TemporalPattern,
    scope: &MemoryScope,
    limit: usize,
) -> EngineResult<TemporalSearchResult> {
    // Get recent memories to analyze patterns
    let recent = store.engram_search_episodic_temporal_range(
        &(chrono::Utc::now() - chrono::Duration::days(90))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string(),
        &chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        scope,
        500,
    )?;

    let pattern_memories = detect_temporal_pattern(&recent, pattern);
    let retrieved: Vec<RetrievedMemory> = pattern_memories
        .into_iter()
        .take(limit)
        .collect();
    let clusters = cluster_temporal(&retrieved, 3600);
    let (span_start, span_end) = span_bounds_default(&retrieved);

    Ok(TemporalSearchResult {
        memories: retrieved,
        clusters,
        span_start,
        span_end,
    })
}

fn search_recent(
    store: &SessionStore,
    scope: &MemoryScope,
    limit: usize,
) -> EngineResult<TemporalSearchResult> {
    let end = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let start = (chrono::Utc::now() - chrono::Duration::days(365))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();

    search_range(store, &start, &end, scope, limit)
}

fn search_session(
    store: &SessionStore,
    session_id: &str,
    scope: &MemoryScope,
    limit: usize,
) -> EngineResult<TemporalSearchResult> {
    let memories = store.engram_search_episodic_by_session(session_id, scope, limit)?;
    let retrieved = memories_to_retrieved(&memories);
    let clusters = cluster_temporal(&retrieved, 300); // 5-min gap within session
    let (span_start, span_end) = span_bounds_default(&retrieved);

    Ok(TemporalSearchResult {
        memories: retrieved,
        clusters,
        span_start,
        span_end,
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// Pattern Detection
// ═══════════════════════════════════════════════════════════════════════════

fn detect_temporal_pattern(
    memories: &[EpisodicMemory],
    pattern: TemporalPattern,
) -> Vec<RetrievedMemory> {
    let mut timestamped: Vec<(&EpisodicMemory, chrono::DateTime<chrono::Utc>)> = memories
        .iter()
        .filter_map(|m| {
            parse_datetime_opt(&m.created_at).map(|dt| (m, dt))
        })
        .collect();

    timestamped.sort_by_key(|(_, dt)| *dt);

    match pattern {
        TemporalPattern::Daily => {
            // Find memories that occur at similar times of day (±1 hour)
            find_recurring_by_hour(&timestamped)
        }
        TemporalPattern::Weekly => {
            // Find memories on the same weekday
            find_recurring_by_weekday(&timestamped)
        }
        TemporalPattern::Monthly => {
            // Find memories around the same day of month
            find_recurring_by_monthday(&timestamped)
        }
        TemporalPattern::Burst => {
            // Find high-density time windows (> 5 memories in 1 hour)
            find_bursts(&timestamped, 3600, 5)
        }
    }
}

fn find_recurring_by_hour(
    memories: &[(&EpisodicMemory, chrono::DateTime<chrono::Utc>)],
) -> Vec<RetrievedMemory> {
    use std::collections::HashMap;
    let mut hour_buckets: HashMap<u32, Vec<&EpisodicMemory>> = HashMap::new();

    for (mem, dt) in memories {
        let hour = dt.format("%H").to_string().parse::<u32>().unwrap_or(0);
        hour_buckets.entry(hour).or_default().push(mem);
    }

    // Return memories from the most populated hour buckets that have ≥3 entries
    let mut results: Vec<RetrievedMemory> = Vec::new();
    let mut buckets: Vec<_> = hour_buckets.into_iter().filter(|(_, v)| v.len() >= 3).collect();
    buckets.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    for (_, mems) in buckets.into_iter().take(3) {
        for mem in mems {
            results.push(episodic_to_retrieved(mem, 0.6));
        }
    }
    results
}

fn find_recurring_by_weekday(
    memories: &[(&EpisodicMemory, chrono::DateTime<chrono::Utc>)],
) -> Vec<RetrievedMemory> {
    use chrono::Datelike;
    use std::collections::HashMap;
    let mut day_buckets: HashMap<u32, Vec<&EpisodicMemory>> = HashMap::new();

    for (mem, dt) in memories {
        let weekday = dt.weekday().num_days_from_monday();
        day_buckets.entry(weekday).or_default().push(mem);
    }

    let mut results: Vec<RetrievedMemory> = Vec::new();
    let mut buckets: Vec<_> = day_buckets.into_iter().filter(|(_, v)| v.len() >= 3).collect();
    buckets.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    for (_, mems) in buckets.into_iter().take(2) {
        for mem in mems {
            results.push(episodic_to_retrieved(mem, 0.5));
        }
    }
    results
}

fn find_recurring_by_monthday(
    memories: &[(&EpisodicMemory, chrono::DateTime<chrono::Utc>)],
) -> Vec<RetrievedMemory> {
    use chrono::Datelike;
    use std::collections::HashMap;
    let mut day_buckets: HashMap<u32, Vec<&EpisodicMemory>> = HashMap::new();

    for (mem, dt) in memories {
        let day = dt.day();
        day_buckets.entry(day).or_default().push(mem);
    }

    let mut results: Vec<RetrievedMemory> = Vec::new();
    let mut buckets: Vec<_> = day_buckets.into_iter().filter(|(_, v)| v.len() >= 2).collect();
    buckets.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    for (_, mems) in buckets.into_iter().take(3) {
        for mem in mems {
            results.push(episodic_to_retrieved(mem, 0.4));
        }
    }
    results
}

fn find_bursts(
    memories: &[(&EpisodicMemory, chrono::DateTime<chrono::Utc>)],
    window_secs: i64,
    min_count: usize,
) -> Vec<RetrievedMemory> {
    let mut results: Vec<RetrievedMemory> = Vec::new();

    // Sliding window approach
    let mut i = 0;
    while i < memories.len() {
        let (_, window_start) = memories[i];
        let mut j = i;
        while j < memories.len()
            && (memories[j].1 - window_start).num_seconds() <= window_secs
        {
            j += 1;
        }
        if j - i >= min_count {
            // Found a burst
            for k in i..j {
                results.push(episodic_to_retrieved(memories[k].0, 0.7));
            }
            i = j; // Skip past this burst
        } else {
            i += 1;
        }
    }
    results
}

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

fn episodic_to_retrieved(mem: &EpisodicMemory, relevance: f32) -> RetrievedMemory {
    use crate::atoms::engram_types::{CompressionLevel, MemoryType};
    use crate::engine::engram::tokenizer::Tokenizer;

    let content = mem.content.full.clone();
    RetrievedMemory {
        token_cost: Tokenizer::heuristic().count_tokens(&content),
        content,
        compression_level: CompressionLevel::Full,
        memory_id: mem.id.clone(),
        memory_type: MemoryType::Episodic,
        trust_score: TrustScore {
            relevance,
            accuracy: 0.5,
            freshness: recency_score(&mem.created_at, 168.0) as f32, // 1-week half-life
            utility: 0.5,
        },
        category: mem.category.clone(),
        created_at: mem.created_at.clone(),
    }
}

fn memories_to_retrieved(memories: &[EpisodicMemory]) -> Vec<RetrievedMemory> {
    memories.iter().map(|m| episodic_to_retrieved(m, 0.5)).collect()
}

fn parse_timestamp(s: &str) -> Option<i64> {
    parse_datetime_opt(s).map(|dt| dt.timestamp())
}

fn parse_datetime(s: &str) -> chrono::DateTime<chrono::Utc> {
    parse_datetime_opt(s).unwrap_or_else(|| chrono::Utc::now())
}

fn parse_datetime_opt(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .ok()
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%SZ")
                .map(|ndt| ndt.and_utc())
                .ok()
        })
}

fn format_timestamp(epoch_secs: i64) -> String {
    chrono::DateTime::from_timestamp(epoch_secs, 0)
        .unwrap_or_else(|| chrono::Utc::now())
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string()
}

fn span_bounds(retrieved: &[RetrievedMemory], default_start: &str, default_end: &str) -> (String, String) {
    let start = retrieved
        .iter()
        .map(|r| r.created_at.as_str())
        .min()
        .unwrap_or(default_start)
        .to_string();
    let end = retrieved
        .iter()
        .map(|r| r.created_at.as_str())
        .max()
        .unwrap_or(default_end)
        .to_string();
    (start, end)
}

fn span_bounds_default(retrieved: &[RetrievedMemory]) -> (String, String) {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    span_bounds(retrieved, &now, &now)
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recency_score() {
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        assert!((recency_score(&now, 24.0) - 1.0).abs() < 0.01);

        let old = (chrono::Utc::now() - chrono::Duration::hours(24))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let score = recency_score(&old, 24.0);
        assert!((score - 0.5).abs() < 0.05); // Should be ~0.5 at half-life
    }

    #[test]
    fn test_cluster_temporal_empty() {
        let clusters = cluster_temporal(&[], 3600);
        assert!(clusters.is_empty());
    }
}
