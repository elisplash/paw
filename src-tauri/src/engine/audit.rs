// ── Unified Signed Audit Log ─────────────────────────────────────────────────
//
// Tamper-evident, append-only audit log for ALL engine operations.
// Every entry is chained via HMAC-SHA256: each row's signature covers
// its own content + the previous row's signature, forming a hash chain.
// Breaking the chain at any point is detectable by verify_chain().
//
// Unified sources (previously fragmented across 5+ systems):
//   - Tool calls (agent loop → execute_tool)
//   - Memory operations (engram store/search/delete/consolidate)
//   - Credential usage (guardrails)
//   - Outbound API requests (http.rs ring buffer → now persisted)
//   - Cognitive events (event bus → now persisted)
//   - MCP tool invocations
//   - Flow executions
//
// Signing: HMAC-SHA256 with a per-install key derived from the OS keychain.
//          Not public-key signing (no need for third-party verification),
//          but tamper-evident within the installation.
//
// Chain: Each entry includes `prev_hash` (the signature of the previous entry).
//        The genesis entry uses prev_hash = "0" * 64.

use chrono::Utc;
use hmac::{Hmac, Mac};
use log::{info, warn};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::sync::LazyLock;

use crate::atoms::error::{EngineError, EngineResult};
use crate::engine::key_vault;
use crate::engine::sessions::SessionStore;

// ═════════════════════════════════════════════════════════════════════════════
// Types
// ═════════════════════════════════════════════════════════════════════════════

/// Categories of auditable events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditCategory {
    /// Tool executed by agent (approve/deny/result).
    ToolCall,
    /// Memory stored, searched, deleted, consolidated, encrypted.
    Memory,
    /// Credential accessed or used.
    Credential,
    /// Outbound API request to an AI provider.
    ApiRequest,
    /// Cognitive pipeline event (gate, recall, injection, etc.).
    Cognitive,
    /// MCP tool invocation.
    Mcp,
    /// Flow execution (start, complete, error).
    Flow,
    /// Security event (injection detected, command blocked, sandbox).
    Security,
}

impl std::fmt::Display for AuditCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ToolCall => write!(f, "tool_call"),
            Self::Memory => write!(f, "memory"),
            Self::Credential => write!(f, "credential"),
            Self::ApiRequest => write!(f, "api_request"),
            Self::Cognitive => write!(f, "cognitive"),
            Self::Mcp => write!(f, "mcp"),
            Self::Flow => write!(f, "flow"),
            Self::Security => write!(f, "security"),
        }
    }
}

impl std::str::FromStr for AuditCategory {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tool_call" => Ok(Self::ToolCall),
            "memory" => Ok(Self::Memory),
            "credential" => Ok(Self::Credential),
            "api_request" => Ok(Self::ApiRequest),
            "cognitive" => Ok(Self::Cognitive),
            "mcp" => Ok(Self::Mcp),
            "flow" => Ok(Self::Flow),
            "security" => Ok(Self::Security),
            _ => Err(format!("Unknown audit category: {}", s)),
        }
    }
}

/// A single entry in the unified audit log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedAuditEntry {
    /// Auto-incrementing row ID.
    pub id: i64,
    /// ISO-8601 UTC timestamp.
    pub timestamp: String,
    /// Event category.
    pub category: String,
    /// What happened (e.g. "execute", "store", "deny", "recall").
    pub action: String,
    /// Which agent triggered this (empty for system-wide events).
    pub agent_id: String,
    /// Session ID (if applicable).
    pub session_id: String,
    /// Primary subject (tool name, memory ID, service name, model, etc.).
    pub subject: String,
    /// Structured details as JSON.
    pub details_json: Option<String>,
    /// Whether the operation succeeded.
    pub success: bool,
    /// HMAC-SHA256 of the previous entry's signature (chain link).
    pub prev_hash: String,
    /// HMAC-SHA256 signature of this entry (covers all fields + prev_hash).
    pub signature: String,
}

// ═════════════════════════════════════════════════════════════════════════════
// Schema
// ═════════════════════════════════════════════════════════════════════════════

/// SQL to create the unified audit log table. Called from run_migrations().
pub const UNIFIED_AUDIT_SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS unified_audit_log (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        timestamp TEXT NOT NULL,
        category TEXT NOT NULL,
        action TEXT NOT NULL,
        agent_id TEXT NOT NULL DEFAULT '',
        session_id TEXT NOT NULL DEFAULT '',
        subject TEXT NOT NULL DEFAULT '',
        details_json TEXT,
        success INTEGER NOT NULL DEFAULT 1,
        prev_hash TEXT NOT NULL,
        signature TEXT NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_unified_audit_time
        ON unified_audit_log(timestamp);
    CREATE INDEX IF NOT EXISTS idx_unified_audit_category
        ON unified_audit_log(category);
    CREATE INDEX IF NOT EXISTS idx_unified_audit_agent
        ON unified_audit_log(agent_id);
    CREATE INDEX IF NOT EXISTS idx_unified_audit_subject
        ON unified_audit_log(subject);
";

// ═════════════════════════════════════════════════════════════════════════════
// HMAC Key Management
// ═════════════════════════════════════════════════════════════════════════════

/// Genesis hash — the prev_hash of the very first entry.
const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Get or create the HMAC signing key from the unified key vault.
/// Separate from memory encryption key and skill vault key.
/// Returns `Zeroizing<Vec<u8>>` so intermediate copies are zeroed on drop.
fn get_audit_signing_key() -> EngineResult<zeroize::Zeroizing<Vec<u8>>> {
    if let Some(key_b64) = key_vault::get(key_vault::PURPOSE_AUDIT_CHAIN) {
        let decoded = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &key_b64)
            .map_err(|e| {
                EngineError::Other(format!("Failed to decode audit signing key: {}", e))
            })?;
        return Ok(zeroize::Zeroizing::new(decoded));
    }
    // No key exists — generate a new random key using OS CSPRNG
    use rand::rngs::OsRng;
    use rand::RngCore;
    let mut key = zeroize::Zeroizing::new(vec![0u8; 32]);
    OsRng.fill_bytes(&mut key);
    let key_b64 = zeroize::Zeroizing::new(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        key.as_slice(),
    ));
    key_vault::set(key_vault::PURPOSE_AUDIT_CHAIN, &key_b64);
    info!("[audit] Created new HMAC signing key in unified vault");
    Ok(key)
}

/// Cached signing key (loaded once per process lifetime).
/// Wrapped in `Zeroizing` to securely zero key material on drop.
static SIGNING_KEY: LazyLock<Option<zeroize::Zeroizing<Vec<u8>>>> =
    LazyLock::new(|| match get_audit_signing_key() {
        Ok(key) => Some(key),
        Err(e) => {
            warn!(
                "[audit] Could not load signing key: {}. Audit signatures will be empty.",
                e
            );
            None
        }
    });

// ═════════════════════════════════════════════════════════════════════════════
// Signing
// ═════════════════════════════════════════════════════════════════════════════

type HmacSha256 = Hmac<Sha256>;

/// Compute HMAC-SHA256 over the audit entry fields + prev_hash.
/// The message is: timestamp|category|action|agent_id|session_id|subject|details|success|prev_hash
#[allow(clippy::too_many_arguments)]
fn compute_signature(
    key: &[u8],
    timestamp: &str,
    category: &str,
    action: &str,
    agent_id: &str,
    session_id: &str,
    subject: &str,
    details: &str,
    success: bool,
    prev_hash: &str,
) -> String {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
    let msg = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}",
        timestamp,
        category,
        action,
        agent_id,
        session_id,
        subject,
        details,
        if success { "1" } else { "0" },
        prev_hash
    );
    mac.update(msg.as_bytes());
    let result = mac.finalize();
    result
        .into_bytes()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

// ═════════════════════════════════════════════════════════════════════════════
// Core API
// ═════════════════════════════════════════════════════════════════════════════

/// Append an entry to the unified audit log with HMAC chain signing.
/// This is the single entry point for ALL audit events across the engine.
#[allow(clippy::too_many_arguments)]
pub fn append(
    store: &SessionStore,
    category: AuditCategory,
    action: &str,
    agent_id: &str,
    session_id: &str,
    subject: &str,
    details: Option<&str>,
    success: bool,
) -> EngineResult<i64> {
    let conn = store.conn.lock();
    let timestamp = Utc::now().to_rfc3339();
    let category_str = category.to_string();
    let details_str = details.unwrap_or("");

    // Get the previous entry's signature (or genesis hash)
    let prev_hash: String = conn
        .query_row(
            "SELECT signature FROM unified_audit_log ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| GENESIS_HASH.to_string());

    // Sign the entry
    let signature = match SIGNING_KEY.as_ref() {
        Some(key) => compute_signature(
            key,
            &timestamp,
            &category_str,
            action,
            agent_id,
            session_id,
            subject,
            details_str,
            success,
            &prev_hash,
        ),
        None => String::new(), // No key available — unsigned but still logged
    };

    // Insert
    conn.execute(
        "INSERT INTO unified_audit_log
         (timestamp, category, action, agent_id, session_id, subject, details_json, success, prev_hash, signature)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            timestamp,
            category_str,
            action,
            agent_id,
            session_id,
            subject,
            if details_str.is_empty() {
                None
            } else {
                Some(details_str)
            },
            success as i32,
            prev_hash,
            signature,
        ],
    )?;

    let id = conn.last_insert_rowid();
    Ok(id)
}

/// Query recent audit entries, optionally filtering by category and/or agent.
pub fn query_recent(
    store: &SessionStore,
    limit: usize,
    category: Option<&str>,
    agent_id: Option<&str>,
) -> EngineResult<Vec<UnifiedAuditEntry>> {
    let conn = store.conn.lock();

    let (sql, param_values) = match (category, agent_id) {
        (Some(cat), Some(aid)) => (
            "SELECT id, timestamp, category, action, agent_id, session_id, subject,
                    details_json, success, prev_hash, signature
             FROM unified_audit_log
             WHERE category = ?1 AND agent_id = ?2
             ORDER BY id DESC LIMIT ?3"
                .to_string(),
            vec![cat.to_string(), aid.to_string(), limit.to_string()],
        ),
        (Some(cat), None) => (
            "SELECT id, timestamp, category, action, agent_id, session_id, subject,
                    details_json, success, prev_hash, signature
             FROM unified_audit_log
             WHERE category = ?1
             ORDER BY id DESC LIMIT ?2"
                .to_string(),
            vec![cat.to_string(), limit.to_string()],
        ),
        (None, Some(aid)) => (
            "SELECT id, timestamp, category, action, agent_id, session_id, subject,
                    details_json, success, prev_hash, signature
             FROM unified_audit_log
             WHERE agent_id = ?1
             ORDER BY id DESC LIMIT ?2"
                .to_string(),
            vec![aid.to_string(), limit.to_string()],
        ),
        (None, None) => (
            "SELECT id, timestamp, category, action, agent_id, session_id, subject,
                    details_json, success, prev_hash, signature
             FROM unified_audit_log
             ORDER BY id DESC LIMIT ?1"
                .to_string(),
            vec![limit.to_string()],
        ),
    };

    let mut stmt = conn.prepare(&sql)?;
    let rows = match param_values.len() {
        1 => stmt.query_map(params![param_values[0]], map_audit_row)?,
        2 => stmt.query_map(params![param_values[0], param_values[1]], map_audit_row)?,
        3 => stmt.query_map(
            params![param_values[0], param_values[1], param_values[2]],
            map_audit_row,
        )?,
        _ => unreachable!(),
    };

    let entries: Vec<UnifiedAuditEntry> = rows.filter_map(|r| r.ok()).collect();
    Ok(entries)
}

fn map_audit_row(row: &rusqlite::Row) -> rusqlite::Result<UnifiedAuditEntry> {
    Ok(UnifiedAuditEntry {
        id: row.get(0)?,
        timestamp: row.get(1)?,
        category: row.get(2)?,
        action: row.get(3)?,
        agent_id: row.get(4)?,
        session_id: row.get(5)?,
        subject: row.get(6)?,
        details_json: row.get(7)?,
        success: row.get::<_, i32>(8)? != 0,
        prev_hash: row.get(9)?,
        signature: row.get(10)?,
    })
}

/// Verify the HMAC chain integrity of the entire audit log.
/// Returns Ok(count) if the chain is intact, Err with the broken row ID if tampered.
pub fn verify_chain(store: &SessionStore) -> EngineResult<Result<u64, i64>> {
    let key = match SIGNING_KEY.as_ref() {
        Some(k) => k,
        None => {
            return Err(EngineError::Other(
                "No signing key available — cannot verify chain".into(),
            ))
        }
    };

    let conn = store.conn.lock();
    let mut stmt = conn.prepare(
        "SELECT id, timestamp, category, action, agent_id, session_id, subject,
                details_json, success, prev_hash, signature
         FROM unified_audit_log ORDER BY id ASC",
    )?;

    let mut expected_prev = GENESIS_HASH.to_string();
    let mut count = 0u64;

    let rows = stmt.query_map([], map_audit_row)?;
    for row_result in rows {
        let entry = row_result?;

        // Verify prev_hash matches what we expect
        if entry.prev_hash != expected_prev {
            warn!(
                "[audit] Chain broken at row {}: expected prev_hash={}, got={}",
                entry.id, expected_prev, entry.prev_hash
            );
            return Ok(Err(entry.id));
        }

        // Verify signature
        let expected_sig = compute_signature(
            key,
            &entry.timestamp,
            &entry.category,
            &entry.action,
            &entry.agent_id,
            &entry.session_id,
            &entry.subject,
            entry.details_json.as_deref().unwrap_or(""),
            entry.success,
            &entry.prev_hash,
        );

        if entry.signature != expected_sig {
            warn!(
                "[audit] Signature mismatch at row {}: expected={}, got={}",
                entry.id, expected_sig, entry.signature
            );
            return Ok(Err(entry.id));
        }

        expected_prev = entry.signature;
        count += 1;
    }

    info!("[audit] Chain verified: {} entries, integrity OK", count);
    Ok(Ok(count))
}

/// Get audit log statistics.
pub fn stats(store: &SessionStore) -> EngineResult<AuditStats> {
    let conn = store.conn.lock();

    let total: i64 = conn.query_row("SELECT COUNT(*) FROM unified_audit_log", [], |r| r.get(0))?;

    let categories: Vec<(String, i64)> = {
        let mut stmt = conn.prepare(
            "SELECT category, COUNT(*) FROM unified_audit_log GROUP BY category ORDER BY COUNT(*) DESC",
        )?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        rows
    };

    let oldest: Option<String> = conn
        .query_row(
            "SELECT timestamp FROM unified_audit_log ORDER BY id ASC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .ok();

    let newest: Option<String> = conn
        .query_row(
            "SELECT timestamp FROM unified_audit_log ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .ok();

    Ok(AuditStats {
        total_entries: total as u64,
        categories,
        oldest_entry: oldest,
        newest_entry: newest,
    })
}

/// Audit log statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditStats {
    pub total_entries: u64,
    pub categories: Vec<(String, i64)>,
    pub oldest_entry: Option<String>,
    pub newest_entry: Option<String>,
}

// ═════════════════════════════════════════════════════════════════════════════
// Convenience: typed log helpers for common operations
// ═════════════════════════════════════════════════════════════════════════════

/// Log a tool call execution.
#[allow(clippy::too_many_arguments)]
pub fn log_tool_call(
    store: &SessionStore,
    agent_id: &str,
    session_id: &str,
    tool_name: &str,
    tool_call_id: &str,
    arguments: &str,
    success: bool,
    output_preview: &str,
) {
    let details = serde_json::json!({
        "tool_call_id": tool_call_id,
        "arguments": arguments,
        "output_preview": truncate(output_preview, 500),
    });
    if let Err(e) = append(
        store,
        AuditCategory::ToolCall,
        "execute",
        agent_id,
        session_id,
        tool_name,
        Some(&details.to_string()),
        success,
    ) {
        warn!("[audit] Failed to log tool call: {}", e);
    }
}

/// Log a tool call denial (user rejected).
pub fn log_tool_denied(
    store: &SessionStore,
    agent_id: &str,
    session_id: &str,
    tool_name: &str,
    tool_call_id: &str,
) {
    let details = serde_json::json!({ "tool_call_id": tool_call_id });
    if let Err(e) = append(
        store,
        AuditCategory::ToolCall,
        "denied",
        agent_id,
        session_id,
        tool_name,
        Some(&details.to_string()),
        false,
    ) {
        warn!("[audit] Failed to log tool denial: {}", e);
    }
}

/// Log an outbound API request.
pub fn log_api_request(
    store: &SessionStore,
    provider: &str,
    model: &str,
    request_hash: &str,
    status: u16,
) {
    let details = serde_json::json!({
        "model": model,
        "request_hash": request_hash,
        "http_status": status,
    });
    if let Err(e) = append(
        store,
        AuditCategory::ApiRequest,
        "request",
        "",
        "",
        provider,
        Some(&details.to_string()),
        (200..400).contains(&status),
    ) {
        warn!("[audit] Failed to log API request: {}", e);
    }
}

/// Log a credential usage event.
pub fn log_credential_use(
    store: &SessionStore,
    agent_id: &str,
    service: &str,
    action: &str,
    approved: bool,
    result: &str,
) {
    let details = serde_json::json!({
        "action": action,
        "approved": approved,
        "result": result,
    });
    if let Err(e) = append(
        store,
        AuditCategory::Credential,
        if approved { "use" } else { "deny" },
        agent_id,
        "",
        service,
        Some(&details.to_string()),
        approved && result == "success",
    ) {
        warn!("[audit] Failed to log credential use: {}", e);
    }
}

/// Log a security event (injection detected, command blocked, etc.).
pub fn log_security_event(
    store: &SessionStore,
    agent_id: &str,
    event_type: &str,
    subject: &str,
    details: &str,
) {
    if let Err(e) = append(
        store,
        AuditCategory::Security,
        event_type,
        agent_id,
        "",
        subject,
        Some(details),
        true, // security events themselves always "succeed" (detection worked)
    ) {
        warn!("[audit] Failed to log security event: {}", e);
    }
}

/// Log a cognitive pipeline event.
pub fn log_cognitive_event(store: &SessionStore, agent_id: &str, event_type: &str, details: &str) {
    if let Err(e) = append(
        store,
        AuditCategory::Cognitive,
        event_type,
        agent_id,
        "",
        "engram",
        Some(details),
        true,
    ) {
        warn!("[audit] Failed to log cognitive event: {}", e);
    }
}

/// Log a flow execution event.
pub fn log_flow_event(
    store: &SessionStore,
    flow_id: &str,
    action: &str,
    details: Option<&str>,
    success: bool,
) {
    if let Err(e) = append(
        store,
        AuditCategory::Flow,
        action,
        "",
        "",
        flow_id,
        details,
        success,
    ) {
        warn!("[audit] Failed to log flow event: {}", e);
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &s[..end])
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> SessionStore {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;").ok();
        crate::engine::sessions::schema_for_testing(&conn);
        conn.execute_batch(UNIFIED_AUDIT_SCHEMA).unwrap();
        SessionStore::from_connection(conn)
    }

    #[test]
    fn test_append_and_query() {
        let store = test_store();

        let id = append(
            &store,
            AuditCategory::ToolCall,
            "execute",
            "agent-1",
            "session-1",
            "exec",
            Some(r#"{"cmd":"ls"}"#),
            true,
        )
        .unwrap();
        assert_eq!(id, 1);

        let entries = query_recent(&store, 10, None, None).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].category, "tool_call");
        assert_eq!(entries[0].action, "execute");
        assert_eq!(entries[0].subject, "exec");
        assert!(entries[0].success);
        assert_eq!(entries[0].prev_hash, GENESIS_HASH);
    }

    #[test]
    fn test_chain_integrity() {
        let store = test_store();

        for i in 0..5 {
            append(
                &store,
                AuditCategory::Memory,
                "store",
                "agent-1",
                "",
                &format!("mem-{}", i),
                None,
                true,
            )
            .unwrap();
        }

        let entries = query_recent(&store, 10, None, None).unwrap();
        assert_eq!(entries.len(), 5);

        // Each entry's prev_hash should equal the previous entry's signature
        let mut sorted = entries.clone();
        sorted.sort_by_key(|e| e.id);

        assert_eq!(sorted[0].prev_hash, GENESIS_HASH);
        for i in 1..sorted.len() {
            assert_eq!(
                sorted[i].prev_hash,
                sorted[i - 1].signature,
                "Chain broken at index {}",
                i
            );
        }
    }

    #[test]
    fn test_query_filter_category() {
        let store = test_store();

        append(
            &store,
            AuditCategory::ToolCall,
            "execute",
            "",
            "",
            "tool1",
            None,
            true,
        )
        .unwrap();
        append(
            &store,
            AuditCategory::Memory,
            "store",
            "",
            "",
            "mem1",
            None,
            true,
        )
        .unwrap();
        append(
            &store,
            AuditCategory::ToolCall,
            "execute",
            "",
            "",
            "tool2",
            None,
            true,
        )
        .unwrap();

        let tools = query_recent(&store, 10, Some("tool_call"), None).unwrap();
        assert_eq!(tools.len(), 2);

        let memory = query_recent(&store, 10, Some("memory"), None).unwrap();
        assert_eq!(memory.len(), 1);
    }

    #[test]
    fn test_query_filter_agent() {
        let store = test_store();

        append(
            &store,
            AuditCategory::ToolCall,
            "execute",
            "agent-1",
            "",
            "t1",
            None,
            true,
        )
        .unwrap();
        append(
            &store,
            AuditCategory::ToolCall,
            "execute",
            "agent-2",
            "",
            "t2",
            None,
            true,
        )
        .unwrap();

        let a1 = query_recent(&store, 10, None, Some("agent-1")).unwrap();
        assert_eq!(a1.len(), 1);
        assert_eq!(a1[0].subject, "t1");
    }

    #[test]
    fn test_stats() {
        let store = test_store();

        append(
            &store,
            AuditCategory::ToolCall,
            "execute",
            "",
            "",
            "t1",
            None,
            true,
        )
        .unwrap();
        append(
            &store,
            AuditCategory::Memory,
            "store",
            "",
            "",
            "m1",
            None,
            true,
        )
        .unwrap();
        append(
            &store,
            AuditCategory::ToolCall,
            "denied",
            "",
            "",
            "t2",
            None,
            false,
        )
        .unwrap();

        let s = stats(&store).unwrap();
        assert_eq!(s.total_entries, 3);
        assert!(s.oldest_entry.is_some());
        assert!(s.newest_entry.is_some());
    }

    #[test]
    fn test_compute_signature_deterministic() {
        let key = b"test-key-32-bytes-long-xxxxxxxx";
        let sig1 = compute_signature(key, "t1", "cat", "act", "a", "s", "sub", "", true, "prev");
        let sig2 = compute_signature(key, "t1", "cat", "act", "a", "s", "sub", "", true, "prev");
        assert_eq!(sig1, sig2);

        // Different input → different signature
        let sig3 = compute_signature(key, "t2", "cat", "act", "a", "s", "sub", "", true, "prev");
        assert_ne!(sig1, sig3);
    }
}
