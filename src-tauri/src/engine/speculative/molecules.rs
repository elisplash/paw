// ─────────────────────────────────────────────────────────────────────────────
// Speculative Tool Execution — Molecules
//
// Side-effectful components: SQLite persistence for transition patterns,
// in-memory caching of speculative results, connection pre-warming via TCP,
// cancellation sessions, and observability logging.
//
// All pure logic (classification, probability math, thresholding) lives in
// atoms.rs.  This module provides the stateful wrappers that interact with
// the database, network, and system clock.
// ─────────────────────────────────────────────────────────────────────────────

use super::atoms::*;
use log::{debug, info, warn};
use rusqlite::Connection;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ── Time Helpers (side effects: system clock) ──────────────────────────────

/// Current time in epoch milliseconds.
fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Current time as Unix epoch seconds (for SQLite integer timestamps).
fn current_epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ── Transition Store (SQLite-backed) ───────────────────────────────────────

/// SQLite-backed store for tool-call transition sequences.
///
/// Records which tool follows which other tool, building a frequency-based
/// transition probability matrix.  The `tool_sequences` table has schema:
///
/// ```sql
/// CREATE TABLE tool_sequences (
///     from_tool  TEXT NOT NULL,
///     to_tool    TEXT NOT NULL,
///     count      INTEGER NOT NULL DEFAULT 1,
///     last_seen  INTEGER NOT NULL DEFAULT 0,
///     PRIMARY KEY (from_tool, to_tool)
/// );
/// ```
pub struct TransitionStore;

impl TransitionStore {
    /// Record a tool transition: `from_tool` was just followed by `to_tool`.
    ///
    /// Uses `INSERT ... ON CONFLICT DO UPDATE` to upsert — first occurrence
    /// inserts with `count=1`, subsequent occurrences increment `count`.
    pub fn record_transition(
        conn: &Connection,
        from_tool: &str,
        to_tool: &str,
    ) -> Result<(), String> {
        let now = current_epoch_secs();
        conn.execute(
            "INSERT INTO tool_sequences (from_tool, to_tool, count, last_seen)
             VALUES (?1, ?2, 1, ?3)
             ON CONFLICT(from_tool, to_tool) DO UPDATE SET
                count = count + 1,
                last_seen = ?3",
            rusqlite::params![from_tool, to_tool, now],
        )
        .map_err(|e| format!("Failed to record transition: {e}"))?;
        Ok(())
    }

    /// Load the full transition matrix from SQLite into memory.
    pub fn load_matrix(conn: &Connection) -> Result<TransitionMatrix, String> {
        let mut stmt = conn
            .prepare(
                "SELECT from_tool, to_tool, count FROM tool_sequences
                 ORDER BY from_tool, count DESC",
            )
            .map_err(|e| format!("Failed to prepare matrix query: {e}"))?;

        let mut matrix = TransitionMatrix::new();
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u64>(2)?,
                ))
            })
            .map_err(|e| format!("Failed to load matrix: {e}"))?;

        for row in rows {
            let (from, to, count) = row.map_err(|e| format!("Row error: {e}"))?;
            matrix.entry(from).or_default().insert(to, count);
        }

        Ok(matrix)
    }

    /// Get the top-N most likely successor tools for a given tool,
    /// with normalized probabilities.
    pub fn top_successors(
        conn: &Connection,
        from_tool: &str,
        limit: usize,
    ) -> Result<Vec<(String, f64)>, String> {
        let total: u64 = conn
            .query_row(
                "SELECT COALESCE(SUM(count), 0) FROM tool_sequences
                 WHERE from_tool = ?1",
                [from_tool],
                |row| row.get(0),
            )
            .map_err(|e| format!("Failed to compute total: {e}"))?;

        if total == 0 {
            return Ok(Vec::new());
        }

        let mut stmt = conn
            .prepare(
                "SELECT to_tool, count FROM tool_sequences
                 WHERE from_tool = ?1
                 ORDER BY count DESC
                 LIMIT ?2",
            )
            .map_err(|e| format!("Failed to prepare top_successors: {e}"))?;

        let results = stmt
            .query_map(rusqlite::params![from_tool, limit as i64], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
            })
            .map_err(|e| format!("Failed to query top_successors: {e}"))?;

        let mut successors = Vec::new();
        for r in results {
            let (tool, count) = r.map_err(|e| format!("Row error: {e}"))?;
            successors.push((tool, count as f64 / total as f64));
        }
        Ok(successors)
    }

    /// Remove transitions not seen since `before_ts` (epoch seconds).
    pub fn prune_stale(conn: &Connection, before_ts: i64) -> Result<usize, String> {
        conn.execute(
            "DELETE FROM tool_sequences WHERE last_seen < ?1",
            [before_ts],
        )
        .map_err(|e| format!("Failed to prune stale transitions: {e}"))
    }

    /// Count total distinct transition pairs in the database.
    pub fn transition_count(conn: &Connection) -> Result<usize, String> {
        conn.query_row("SELECT COUNT(*) FROM tool_sequences", [], |row| {
            row.get::<_, usize>(0)
        })
        .map_err(|e| format!("Failed to count transitions: {e}"))
    }
}

// ── Speculative Cache (In-Memory) ──────────────────────────────────────────

/// In-memory cache for speculative tool execution results.
///
/// Stores pre-computed results keyed by `(tool_name, args_hash)`.
/// Entries auto-expire after their TTL.  Bounded by max capacity with
/// oldest-first eviction.
pub struct SpeculativeCache {
    entries: Vec<CacheEntry>,
    max_entries: usize,
    default_ttl_ms: u64,
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl SpeculativeCache {
    /// Create a new cache with the given configuration.
    pub fn new(config: &SpeculationConfig) -> Self {
        Self {
            entries: Vec::new(),
            max_entries: config.max_cache_entries,
            default_ttl_ms: config.cache_ttl_ms,
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }

    /// Insert a speculative result into the cache.
    ///
    /// If an entry with the same `(tool_name, args_hash)` already exists,
    /// it is replaced.  If the cache is at capacity, the oldest entry is evicted.
    pub fn put(&mut self, tool_name: String, args_hash: u64, output: String, success: bool) {
        // Remove existing entry for the same key
        self.entries
            .retain(|e| !(e.tool_name == tool_name && e.args_hash == args_hash));

        // Evict oldest entries if at capacity
        while self.entries.len() >= self.max_entries {
            self.entries.remove(0);
            self.evictions += 1;
        }

        let now = current_time_ms();
        self.entries.push(CacheEntry {
            tool_name: tool_name.clone(),
            args_hash,
            result: SpeculativeResult {
                tool_name,
                args_hash,
                output,
                success,
                ttl_ms: self.default_ttl_ms,
            },
            created_at_ms: now,
        });
    }

    /// Look up a speculative result by tool name and argument hash.
    ///
    /// Returns the result if found and not expired.  Updates hit/miss counters.
    pub fn get(&mut self, tool_name: &str, args_hash: u64) -> Option<&SpeculativeResult> {
        let now = current_time_ms();
        let found = self.entries.iter().position(|e| {
            e.tool_name == tool_name && e.args_hash == args_hash && is_cache_valid(e, now)
        });

        if let Some(idx) = found {
            self.hits += 1;
            Some(&self.entries[idx].result)
        } else {
            self.misses += 1;
            None
        }
    }

    /// Remove all cached entries for a specific tool.
    pub fn invalidate_tool(&mut self, tool_name: &str) {
        let before = self.entries.len();
        self.entries.retain(|e| e.tool_name != tool_name);
        let removed = before - self.entries.len();
        self.evictions += removed as u64;
    }

    /// Remove all expired entries from the cache.
    pub fn evict_expired(&mut self) {
        let now = current_time_ms();
        let before = self.entries.len();
        self.entries.retain(|e| is_cache_valid(e, now));
        self.evictions += (before - self.entries.len()) as u64;
    }

    /// Get cache performance statistics.
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.entries.len(),
            hits: self.hits,
            misses: self.misses,
            evictions: self.evictions,
        }
    }

    /// Clear all entries and reset counters.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.hits = 0;
        self.misses = 0;
        self.evictions = 0;
    }

    /// Number of current entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ── Speculation Session ────────────────────────────────────────────────────

/// Tracks an in-flight speculative execution with cancellation support.
///
/// When the model calls a different tool than predicted, the session is
/// cancelled via an `AtomicBool` flag that can be shared across threads.
pub struct SpeculationSession {
    candidate: SpeculationCandidate,
    cancelled: Arc<AtomicBool>,
    created_at_ms: u64,
}

impl SpeculationSession {
    /// Create a new session for the given predicted candidate.
    pub fn new(candidate: SpeculationCandidate) -> Self {
        Self {
            candidate,
            cancelled: Arc::new(AtomicBool::new(false)),
            created_at_ms: current_time_ms(),
        }
    }

    /// Cancel this speculation (thread-safe, lock-free).
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Check if this speculation has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Get a clone of the cancellation flag for passing to spawned tasks.
    pub fn cancellation_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancelled)
    }

    /// The predicted tool candidate.
    pub fn candidate(&self) -> &SpeculationCandidate {
        &self.candidate
    }

    /// Milliseconds since this session was created.
    pub fn age_ms(&self) -> u64 {
        current_time_ms().saturating_sub(self.created_at_ms)
    }
}

// ── Connection Warming ─────────────────────────────────────────────────────

/// Pre-establish a TCP connection to a warm target.
///
/// This primes the OS DNS cache and verifies connectivity.  The connection
/// is immediately dropped — the goal is to warm DNS and routing tables,
/// not maintain a persistent connection.  Saves 100–300ms on the subsequent
/// real API call.
pub fn warm_connection(target: &WarmTarget) -> Result<Duration, String> {
    use std::net::ToSocketAddrs;

    let addr = format!("{}:{}", target.host, target.port);
    let socket_addr = addr
        .to_socket_addrs()
        .map_err(|e| format!("DNS resolution failed for {addr}: {e}"))?
        .next()
        .ok_or_else(|| format!("No addresses resolved for {addr}"))?;

    let timeout = Duration::from_millis(target.timeout_ms);
    let start = Instant::now();
    TcpStream::connect_timeout(&socket_addr, timeout)
        .map_err(|e| format!("TCP connect to {addr} failed: {e}"))?;
    let elapsed = start.elapsed();

    debug!(
        "Warmed connection to {} in {:.1}ms",
        addr,
        elapsed.as_secs_f64() * 1000.0
    );
    Ok(elapsed)
}

/// Attempt to warm connections for a list of targets (best-effort).
///
/// Failures are logged but not fatal — warming is purely an optimization.
pub fn warm_connections_batch(targets: &[WarmTarget]) -> Vec<(String, Result<Duration, String>)> {
    targets
        .iter()
        .map(|t| {
            let addr = format!("{}:{}", t.host, t.port);
            let result = warm_connection(t);
            if let Err(ref e) = result {
                debug!("Connection warming failed for {}: {}", addr, e);
            }
            (addr, result)
        })
        .collect()
}

// ── Orchestration ──────────────────────────────────────────────────────────

/// After a tool completes, record the transition and predict the next tool.
///
/// This is the main entry point called from the agent loop.  It:
/// 1. Records the `previous_tool → completed_tool` transition in SQLite
/// 2. Queries historical successors of `completed_tool`
/// 3. Returns the top candidate if it's above threshold and read-only
pub fn predict_and_record(
    conn: &Connection,
    previous_tool: Option<&str>,
    completed_tool: &str,
    config: &SpeculationConfig,
) -> Option<SpeculationCandidate> {
    if !config.enabled {
        return None;
    }

    // Step 1: Record the transition (if we have a predecessor)
    if let Some(prev) = previous_tool {
        if let Err(e) = TransitionStore::record_transition(conn, prev, completed_tool) {
            warn!("Failed to record tool transition: {e}");
        }
    }

    // Step 2: Check minimum observation count before querying
    let total_count: u64 = conn
        .query_row(
            "SELECT COALESCE(SUM(count), 0) FROM tool_sequences
             WHERE from_tool = ?1",
            [completed_tool],
            |row| row.get::<_, u64>(0),
        )
        .unwrap_or(0);

    if total_count < config.min_observations {
        return None;
    }

    // Step 3: Get top successors and find the best read-only candidate
    match TransitionStore::top_successors(conn, completed_tool, config.top_k) {
        Ok(successors) => {
            for (tool, prob) in successors {
                if prob >= config.threshold && is_read_only_tool(&tool) {
                    debug!(
                        "Speculation candidate: {} → {} (p={:.2})",
                        completed_tool, tool, prob
                    );
                    return Some(SpeculationCandidate {
                        tool_name: tool,
                        probability: prob,
                    });
                }
            }
            None
        }
        Err(e) => {
            warn!("Failed to predict next tool: {e}");
            None
        }
    }
}

/// Log the outcome of a speculation for observability.
pub fn log_speculation_outcome(
    from_tool: &str,
    predicted: &str,
    actual: &str,
    outcome: &SpeculationOutcome,
) {
    match outcome {
        SpeculationOutcome::Hit => {
            info!(
                "Speculation HIT: {} → predicted {} (actual {})",
                from_tool, predicted, actual
            );
        }
        SpeculationOutcome::Miss => {
            debug!(
                "Speculation MISS: {} → predicted {} but got {}",
                from_tool, predicted, actual
            );
        }
        SpeculationOutcome::Cancelled => {
            debug!(
                "Speculation CANCELLED: {} → predicted {}",
                from_tool, predicted
            );
        }
        SpeculationOutcome::Expired => {
            debug!(
                "Speculation EXPIRED: {} → predicted {} (TTL exceeded before use)",
                from_tool, predicted
            );
        }
    }
}

/// Log session-level speculation statistics.
pub fn log_session_speculation_stats(stats: &SpeculationStats) {
    let summary = format_speculation_summary(stats);
    if stats.predictions > 0 {
        info!("{}", summary);
    } else {
        debug!("No speculations attempted this session");
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::sessions::schema::run_migrations;
    use rusqlite::Connection;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA journal_mode = WAL;").unwrap();
        run_migrations(&conn).unwrap();
        conn
    }

    fn test_config() -> SpeculationConfig {
        SpeculationConfig {
            threshold: 0.5,
            top_k: 3,
            cache_ttl_ms: 30_000,
            max_cache_entries: 64,
            min_observations: 1, // low for tests
            enabled: true,
            warm_connections: true,
        }
    }

    // ── TransitionStore ────────────────────────────────────────────────

    #[test]
    fn transition_store_record_and_load() {
        let conn = test_db();
        TransitionStore::record_transition(&conn, "a", "b").unwrap();
        TransitionStore::record_transition(&conn, "a", "c").unwrap();
        TransitionStore::record_transition(&conn, "b", "c").unwrap();

        let matrix = TransitionStore::load_matrix(&conn).unwrap();
        assert_eq!(matrix["a"]["b"], 1);
        assert_eq!(matrix["a"]["c"], 1);
        assert_eq!(matrix["b"]["c"], 1);
    }

    #[test]
    fn transition_store_increment_existing() {
        let conn = test_db();
        TransitionStore::record_transition(&conn, "a", "b").unwrap();
        TransitionStore::record_transition(&conn, "a", "b").unwrap();
        TransitionStore::record_transition(&conn, "a", "b").unwrap();

        let matrix = TransitionStore::load_matrix(&conn).unwrap();
        assert_eq!(matrix["a"]["b"], 3);
    }

    #[test]
    fn transition_store_top_successors() {
        let conn = test_db();
        // A → B: 5 times
        for _ in 0..5 {
            TransitionStore::record_transition(&conn, "a", "b").unwrap();
        }
        // A → C: 3 times
        for _ in 0..3 {
            TransitionStore::record_transition(&conn, "a", "c").unwrap();
        }
        // A → D: 2 times
        for _ in 0..2 {
            TransitionStore::record_transition(&conn, "a", "d").unwrap();
        }

        let successors = TransitionStore::top_successors(&conn, "a", 2).unwrap();
        assert_eq!(successors.len(), 2);
        assert_eq!(successors[0].0, "b");
        assert!((successors[0].1 - 0.5).abs() < 0.01); // 5/10
        assert_eq!(successors[1].0, "c");
        assert!((successors[1].1 - 0.3).abs() < 0.01); // 3/10
    }

    #[test]
    fn transition_store_top_successors_empty() {
        let conn = test_db();
        let s = TransitionStore::top_successors(&conn, "nonexistent", 5).unwrap();
        assert!(s.is_empty());
    }

    #[test]
    fn transition_store_prune_stale() {
        let conn = test_db();
        TransitionStore::record_transition(&conn, "a", "b").unwrap();
        TransitionStore::record_transition(&conn, "c", "d").unwrap();

        // Prune with timestamp in the past — should keep all
        let past = current_epoch_secs() - 3600;
        let pruned = TransitionStore::prune_stale(&conn, past).unwrap();
        assert_eq!(pruned, 0);

        // Prune with timestamp in the future — removes everything
        let future = current_epoch_secs() + 3600;
        let pruned = TransitionStore::prune_stale(&conn, future).unwrap();
        assert_eq!(pruned, 2);
        assert_eq!(TransitionStore::transition_count(&conn).unwrap(), 0);
    }

    #[test]
    fn transition_store_count() {
        let conn = test_db();
        assert_eq!(TransitionStore::transition_count(&conn).unwrap(), 0);

        TransitionStore::record_transition(&conn, "a", "b").unwrap();
        TransitionStore::record_transition(&conn, "c", "d").unwrap();
        assert_eq!(TransitionStore::transition_count(&conn).unwrap(), 2);

        // Same pair again: increments count, doesn't add a new row
        TransitionStore::record_transition(&conn, "a", "b").unwrap();
        assert_eq!(TransitionStore::transition_count(&conn).unwrap(), 2);
    }

    // ── SpeculativeCache ───────────────────────────────────────────────

    #[test]
    fn cache_put_and_get() {
        let config = test_config();
        let mut cache = SpeculativeCache::new(&config);

        cache.put("memory_search".into(), 123, "result data".into(), true);
        let result = cache.get("memory_search", 123);
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.output, "result data");
        assert!(r.success);
    }

    #[test]
    fn cache_miss_wrong_args() {
        let config = test_config();
        let mut cache = SpeculativeCache::new(&config);

        cache.put("memory_search".into(), 123, "result data".into(), true);
        let result = cache.get("memory_search", 456);
        assert!(result.is_none());
    }

    #[test]
    fn cache_miss_wrong_tool() {
        let config = test_config();
        let mut cache = SpeculativeCache::new(&config);

        cache.put("memory_search".into(), 123, "result data".into(), true);
        let result = cache.get("email_read", 123);
        assert!(result.is_none());
    }

    #[test]
    fn cache_invalidate_tool() {
        let config = test_config();
        let mut cache = SpeculativeCache::new(&config);

        cache.put("memory_search".into(), 1, "r1".into(), true);
        cache.put("memory_search".into(), 2, "r2".into(), true);
        cache.put("email_read".into(), 3, "r3".into(), true);
        assert_eq!(cache.len(), 3);

        cache.invalidate_tool("memory_search");
        assert_eq!(cache.len(), 1);
        assert!(cache.get("email_read", 3).is_some());
    }

    #[test]
    fn cache_max_entries_eviction() {
        let config = SpeculationConfig {
            max_cache_entries: 3,
            ..test_config()
        };
        let mut cache = SpeculativeCache::new(&config);

        cache.put("a".into(), 1, "r1".into(), true);
        cache.put("b".into(), 2, "r2".into(), true);
        cache.put("c".into(), 3, "r3".into(), true);
        assert_eq!(cache.len(), 3);

        // Adding a 4th evicts the oldest (a)
        cache.put("d".into(), 4, "r4".into(), true);
        assert_eq!(cache.len(), 3);
        assert!(cache.get("a", 1).is_none()); // evicted
        assert!(cache.get("d", 4).is_some()); // present
    }

    #[test]
    fn cache_replaces_same_key() {
        let config = test_config();
        let mut cache = SpeculativeCache::new(&config);

        cache.put("tool".into(), 42, "old result".into(), true);
        cache.put("tool".into(), 42, "new result".into(), true);
        assert_eq!(cache.len(), 1);

        let r = cache.get("tool", 42).unwrap();
        assert_eq!(r.output, "new result");
    }

    #[test]
    fn cache_stats_tracking() {
        let config = test_config();
        let mut cache = SpeculativeCache::new(&config);

        cache.put("a".into(), 1, "r1".into(), true);
        let _ = cache.get("a", 1); // hit
        let _ = cache.get("b", 2); // miss
        let _ = cache.get("c", 3); // miss

        let stats = cache.stats();
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 2);
    }

    #[test]
    fn cache_clear_resets() {
        let config = test_config();
        let mut cache = SpeculativeCache::new(&config);

        cache.put("a".into(), 1, "r1".into(), true);
        let _ = cache.get("a", 1);
        cache.clear();

        assert!(cache.is_empty());
        let stats = cache.stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
    }

    // ── SpeculationSession ─────────────────────────────────────────────

    #[test]
    fn session_cancel() {
        let session = SpeculationSession::new(SpeculationCandidate {
            tool_name: "memory_search".into(),
            probability: 0.7,
        });

        assert!(!session.is_cancelled());
        session.cancel();
        assert!(session.is_cancelled());
    }

    #[test]
    fn session_cancellation_flag_shared() {
        let session = SpeculationSession::new(SpeculationCandidate {
            tool_name: "memory_search".into(),
            probability: 0.7,
        });
        let flag = session.cancellation_flag();

        assert!(!flag.load(Ordering::Acquire));
        session.cancel();
        assert!(flag.load(Ordering::Acquire));
    }

    #[test]
    fn session_candidate_access() {
        let session = SpeculationSession::new(SpeculationCandidate {
            tool_name: "email_read".into(),
            probability: 0.65,
        });

        assert_eq!(session.candidate().tool_name, "email_read");
        assert!((session.candidate().probability - 0.65).abs() < 0.001);
    }

    // ── Orchestration ──────────────────────────────────────────────────

    #[test]
    fn predict_and_record_above_threshold() {
        let conn = test_db();
        let config = test_config();

        // Build pattern: soul_read → memory_search (8×), soul_read → email_send (2×)
        for _ in 0..8 {
            TransitionStore::record_transition(&conn, "soul_read", "memory_search").unwrap();
        }
        for _ in 0..2 {
            TransitionStore::record_transition(&conn, "soul_read", "email_send").unwrap();
        }

        // Predict after soul_read
        let candidate = predict_and_record(&conn, None, "soul_read", &config);
        assert!(candidate.is_some());
        let c = candidate.unwrap();
        assert_eq!(c.tool_name, "memory_search");
        assert!((c.probability - 0.8).abs() < 0.01);
    }

    #[test]
    fn predict_and_record_below_threshold() {
        let conn = test_db();
        let config = test_config();

        // Uniform distribution — each P ≈ 0.33, below 0.5
        for _ in 0..3 {
            TransitionStore::record_transition(&conn, "a", "memory_search").unwrap();
            TransitionStore::record_transition(&conn, "a", "email_read").unwrap();
            TransitionStore::record_transition(&conn, "a", "list_tasks").unwrap();
        }

        let candidate = predict_and_record(&conn, None, "a", &config);
        assert!(candidate.is_none());
    }

    #[test]
    fn predict_and_record_skips_write_tools() {
        let conn = test_db();
        let config = test_config();

        // Strong pattern but to a write tool
        for _ in 0..10 {
            TransitionStore::record_transition(&conn, "gmail_search", "email_send").unwrap();
        }

        // email_send is Write — never speculate
        let candidate = predict_and_record(&conn, None, "gmail_search", &config);
        assert!(candidate.is_none());
    }

    #[test]
    fn predict_and_record_records_transition() {
        let conn = test_db();
        let config = test_config();

        // Call with previous_tool set to verify it records
        predict_and_record(&conn, Some("tool_a"), "tool_b", &config);

        assert_eq!(TransitionStore::transition_count(&conn).unwrap(), 1);
        let matrix = TransitionStore::load_matrix(&conn).unwrap();
        assert_eq!(matrix["tool_a"]["tool_b"], 1);
    }

    #[test]
    fn predict_disabled_returns_none() {
        let conn = test_db();
        let config = SpeculationConfig {
            enabled: false,
            ..test_config()
        };

        for _ in 0..10 {
            TransitionStore::record_transition(&conn, "a", "memory_search").unwrap();
        }

        // Even with strong pattern, disabled returns None
        let candidate = predict_and_record(&conn, None, "a", &config);
        assert!(candidate.is_none());
    }

    #[test]
    fn predict_respects_min_observations() {
        let conn = test_db();
        let config = SpeculationConfig {
            min_observations: 5,
            ..test_config()
        };

        // Only 3 observations — below min_observations=5
        for _ in 0..3 {
            TransitionStore::record_transition(&conn, "a", "memory_search").unwrap();
        }

        let candidate = predict_and_record(&conn, None, "a", &config);
        assert!(candidate.is_none());
    }

    // ── Logging ────────────────────────────────────────────────────────

    #[test]
    fn log_outcome_doesnt_panic() {
        log_speculation_outcome("a", "b", "c", &SpeculationOutcome::Hit);
        log_speculation_outcome("a", "b", "c", &SpeculationOutcome::Miss);
        log_speculation_outcome("a", "b", "c", &SpeculationOutcome::Cancelled);
        log_speculation_outcome("a", "b", "c", &SpeculationOutcome::Expired);
    }

    #[test]
    fn log_session_stats_doesnt_panic() {
        log_session_speculation_stats(&SpeculationStats::default());
        log_session_speculation_stats(&SpeculationStats {
            predictions: 10,
            hits: 7,
            misses: 3,
            ..Default::default()
        });
    }
}
