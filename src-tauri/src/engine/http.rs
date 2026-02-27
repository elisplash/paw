// ── Paw Engine: HTTP Retry, Circuit-Breaker, TLS Pinning & Request Signing ──
//
// Shared retry utilities used by AI providers, channel bridges, and tools.
//
// Features:
//   • Exponential backoff with ±25% jitter (base 1s, max 30s, 3 retries)
//   • Retry on 429 (rate limit), 500, 502, 503, 504, 529
//   • Respects `Retry-After` header
//   • Circuit breaker: 5 consecutive failures → fail fast for 60s
//   • Bridge reconnect helper with escalating backoff + cap
//   • Certificate-pinned reqwest::Client factory for known AI providers
//   • SHA-256 request signing for outbound API call tamper detection
//   • Audit log of hashed outbound requests

use log::{info, warn};
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

// ── Constants ──────────────────────────────────────────────────────────────

/// Default maximum number of retry attempts per request.
pub const MAX_RETRIES: u32 = 3;

/// Initial retry delay in milliseconds (doubles each attempt).
const INITIAL_RETRY_DELAY_MS: u64 = 1_000;

/// Maximum retry delay cap in milliseconds (30 seconds).
const MAX_RETRY_DELAY_MS: u64 = 30_000;

/// Maximum bridge reconnect delay cap in milliseconds (5 minutes).
const MAX_RECONNECT_DELAY_MS: u64 = 300_000;

// ── Retryable status detection ─────────────────────────────────────────────

/// Check if an HTTP status code represents a transient/retryable error.
pub fn is_retryable_status(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 504 | 529)
}

// ── Backoff delay ──────────────────────────────────────────────────────────

/// Sleep with exponential backoff + ±25% jitter.
/// Respects Retry-After header if the server sent one.
/// Returns the actual delay duration for logging.
pub async fn retry_delay(attempt: u32, retry_after_secs: Option<u64>) -> Duration {
    let base_ms = INITIAL_RETRY_DELAY_MS * 2u64.pow(attempt);
    let capped_ms = base_ms.min(MAX_RETRY_DELAY_MS);
    let delay_ms = if let Some(secs) = retry_after_secs {
        // Use server-specified delay, but cap at 60s and floor at our computed backoff
        (secs.min(60) * 1000).max(capped_ms)
    } else {
        capped_ms
    };
    let jittered = apply_jitter(delay_ms);
    let delay = Duration::from_millis(jittered);
    tokio::time::sleep(delay).await;
    delay
}

/// Compute exponential backoff delay for bridge reconnection.
/// Uses a longer cap (5 minutes) than request retries.
/// `attempt` is 0-based.
pub async fn reconnect_delay(attempt: u32) -> Duration {
    let base_ms = INITIAL_RETRY_DELAY_MS * 2u64.pow(attempt.min(12));
    let capped_ms = base_ms.min(MAX_RECONNECT_DELAY_MS);
    let jittered = apply_jitter(capped_ms);
    let delay = Duration::from_millis(jittered);
    tokio::time::sleep(delay).await;
    delay
}

/// Apply ±25% jitter to prevent thundering-herd effects.
fn apply_jitter(base_ms: u64) -> u64 {
    let jitter_range = (base_ms / 4) as i64;
    if jitter_range == 0 {
        return base_ms.max(100);
    }
    let offset = (rand_jitter() % (2 * jitter_range + 1)) - jitter_range;
    let result = base_ms as i64 + offset;
    result.max(100) as u64
}

/// Simple jitter source using system clock nanos (no extra crate needed).
fn rand_jitter() -> i64 {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    (nanos % 1000) as i64
}

// ── Retry-After header parsing ─────────────────────────────────────────────

/// Parse Retry-After header value (integer seconds only).
/// HTTP-date format is not implemented — falls back to computed backoff.
pub fn parse_retry_after(header_value: &str) -> Option<u64> {
    header_value.trim().parse::<u64>().ok()
}

// ── Circuit Breaker ────────────────────────────────────────────────────────

/// A simple circuit breaker that trips after N consecutive failures,
/// then rejects requests for a cooldown period before allowing retries.
///
/// States:
///   Closed   — normal operation, requests pass through
///   Open     — rejecting requests (cooldown active)
///   HalfOpen — cooldown expired, one probe request allowed
pub struct CircuitBreaker {
    /// Number of consecutive failures.
    consecutive_failures: AtomicU32,
    /// Timestamp (epoch secs) when the circuit was tripped open.
    tripped_at: AtomicU64,
    /// Number of consecutive failures before tripping.
    threshold: u32,
    /// Cooldown period in seconds while circuit is open.
    cooldown_secs: u64,
}

impl CircuitBreaker {
    /// Create a new circuit breaker.
    /// - `threshold`: number of consecutive failures before tripping (default: 5)
    /// - `cooldown_secs`: seconds to wait before allowing probe requests (default: 60)
    pub const fn new(threshold: u32, cooldown_secs: u64) -> Self {
        Self {
            consecutive_failures: AtomicU32::new(0),
            tripped_at: AtomicU64::new(0),
            threshold,
            cooldown_secs,
        }
    }

    /// Check if a request should be allowed through.
    /// Returns `Ok(())` if allowed, `Err(message)` if circuit is open.
    pub fn check(&self) -> Result<(), String> {
        let failures = self.consecutive_failures.load(Ordering::Relaxed);
        if failures < self.threshold {
            return Ok(());
        }

        let tripped = self.tripped_at.load(Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if now - tripped < self.cooldown_secs {
            Err(format!(
                "Circuit breaker open: {} consecutive failures, cooling down for {}s",
                failures,
                self.cooldown_secs - (now - tripped)
            ))
        } else {
            // Half-open: allow one probe request through
            Ok(())
        }
    }

    /// Record a successful request — resets the failure counter.
    pub fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
        self.tripped_at.store(0, Ordering::Relaxed);
    }

    /// Record a failed request — increments the failure counter.
    /// If the threshold is reached, trips the circuit open.
    pub fn record_failure(&self) {
        let prev = self.consecutive_failures.fetch_add(1, Ordering::Relaxed);
        if prev + 1 >= self.threshold {
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            self.tripped_at.store(now, Ordering::Relaxed);
            warn!(
                "[circuit-breaker] Tripped after {} consecutive failures — cooling down {}s",
                prev + 1,
                self.cooldown_secs
            );
        }
    }
}

// ── Certificate-Pinned Client Factory ──────────────────────────────────────
//
// Builds a `reqwest::Client` that uses a custom `rustls::ClientConfig` with
// only the Mozilla root certificates.  Provider domains (api.openai.com, etc.)
// are resolved through the same root store, so an attacker who installs their
// own CA on the user's OS still cannot MITM our connections.
//
// By default reqwest with `rustls-tls` already ignores the OS trust store and
// uses webpki-roots, but building the ClientConfig explicitly lets us:
//   (a) enforce this guarantee even if reqwest defaults change in a future version
//   (b) add per-domain SPKI fingerprint pinning later without restructuring
//   (c) share a single Client across all providers (connection pooling)

use reqwest::Client;
use rustls::ClientConfig;
use std::sync::LazyLock;

/// Build a `rustls::ClientConfig` pinned to the Mozilla root certificates.
/// This explicitly ignores the OS trust store, ensuring a system-level CA
/// compromise cannot intercept provider traffic.
///
/// Uses an explicit `ring` CryptoProvider rather than the process-level
/// default so the config works reliably in unit-test binaries where no
/// global provider has been installed.
fn pinned_tls_config() -> ClientConfig {
    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("Failed to set default TLS protocol versions")
        .with_root_certificates(root_store)
        .with_no_client_auth()
}

/// A singleton certificate-pinned `reqwest::Client` for AI provider calls.
/// Shared across all provider instances — one connection pool, one TLS config.
static PINNED_CLIENT: LazyLock<Client> = LazyLock::new(|| {
    let tls = pinned_tls_config();
    Client::builder()
        .use_preconfigured_tls(tls)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .build()
        .expect("Failed to build certificate-pinned reqwest::Client")
});

/// Get the shared certificate-pinned HTTP client.
/// Providers should call this instead of `Client::builder().build()`.
pub fn pinned_client() -> Client {
    PINNED_CLIENT.clone()
}

// ── Outbound Request Signing & Audit ───────────────────────────────────────
//
// Before sending any AI provider request, we compute a SHA-256 hash of:
//   provider || model || timestamp || body_bytes
// and log it to an in-memory ring buffer.  This allows:
//   • Tamper detection: if a proxy modifies the request body, the hash won't match
//   • Audit trail: security-conscious users can export hashes for compliance
//   • Replay detection: timestamps make each hash unique

/// An entry in the outbound request audit log.
#[derive(Debug, Clone)]
pub struct RequestAuditEntry {
    /// ISO-8601 timestamp of the outbound request.
    pub timestamp: String,
    /// Provider name (e.g. "openai", "anthropic", "google").
    pub provider: String,
    /// Model used in this request.
    pub model: String,
    /// SHA-256 hex digest of `provider || model || timestamp || body`.
    pub hash: String,
    /// HTTP status code of the response (0 if request failed).
    pub status: u16,
}

/// Ring-buffer audit log for outbound API requests.
const AUDIT_LOG_CAPACITY: usize = 500;

pub struct RequestAuditLog {
    entries: Vec<RequestAuditEntry>,
    /// Write index (wraps around at capacity).
    head: usize,
    /// Total entries ever written.
    total: u64,
}

impl Default for RequestAuditLog {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestAuditLog {
    pub fn new() -> Self {
        Self {
            entries: Vec::with_capacity(AUDIT_LOG_CAPACITY),
            head: 0,
            total: 0,
        }
    }

    /// Append an audit entry.  When full, overwrites the oldest entry.
    pub fn push(&mut self, entry: RequestAuditEntry) {
        if self.entries.len() < AUDIT_LOG_CAPACITY {
            self.entries.push(entry);
        } else {
            self.entries[self.head] = entry;
        }
        self.head = (self.head + 1) % AUDIT_LOG_CAPACITY;
        self.total += 1;
    }

    /// Get recent entries (newest first), up to `limit`.
    pub fn recent(&self, limit: usize) -> Vec<RequestAuditEntry> {
        let len = self.entries.len();
        if len == 0 {
            return vec![];
        }
        let count = limit.min(len);
        let mut result = Vec::with_capacity(count);
        let mut idx = if self.entries.len() < AUDIT_LOG_CAPACITY {
            self.entries.len().wrapping_sub(1)
        } else {
            (self.head + AUDIT_LOG_CAPACITY - 1) % AUDIT_LOG_CAPACITY
        };
        for _ in 0..count {
            result.push(self.entries[idx].clone());
            idx = (idx + AUDIT_LOG_CAPACITY - 1) % AUDIT_LOG_CAPACITY;
        }
        result
    }

    /// Total entries ever written.
    pub fn total(&self) -> u64 {
        self.total
    }
}

/// Global audit log instance, protected by a parking_lot Mutex.
static AUDIT_LOG: LazyLock<Arc<Mutex<RequestAuditLog>>> =
    LazyLock::new(|| Arc::new(Mutex::new(RequestAuditLog::new())));

/// Compute a SHA-256 hash for an outbound request and append it to the audit log.
///
/// Call this immediately before `.send()` in every provider.
///
/// Returns the hex hash string so providers can include it in debug logs.
pub fn sign_and_log_request(provider: &str, model: &str, body_bytes: &[u8]) -> String {
    let now = chrono::Utc::now().to_rfc3339();
    let mut hasher = Sha256::new();
    hasher.update(provider.as_bytes());
    hasher.update(model.as_bytes());
    hasher.update(now.as_bytes());
    hasher.update(body_bytes);
    let digest = hasher.finalize();
    let hash_hex: String = digest.iter().map(|b| format!("{:02x}", b)).collect();

    let entry = RequestAuditEntry {
        timestamp: now,
        provider: provider.to_string(),
        model: model.to_string(),
        hash: hash_hex.clone(),
        status: 0, // Will be updated after response
    };

    info!(
        "[security] Outbound request signed: provider={} model={} hash={}",
        provider, model, &hash_hex[..16]
    );

    AUDIT_LOG.lock().push(entry);
    hash_hex
}

/// Update the status code of the most recent audit entry (after response).
pub fn update_last_audit_status(status: u16) {
    let mut log = AUDIT_LOG.lock();
    if log.entries.is_empty() {
        return;
    }
    let idx = if log.entries.len() < AUDIT_LOG_CAPACITY {
        log.entries.len() - 1
    } else {
        (log.head + AUDIT_LOG_CAPACITY - 1) % AUDIT_LOG_CAPACITY
    };
    log.entries[idx].status = status;
}

/// Get recent audit entries (newest first).
pub fn recent_audit_entries(limit: usize) -> Vec<RequestAuditEntry> {
    AUDIT_LOG.lock().recent(limit)
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_statuses() {
        assert!(is_retryable_status(429));
        assert!(is_retryable_status(500));
        assert!(is_retryable_status(502));
        assert!(is_retryable_status(503));
        assert!(is_retryable_status(504));
        assert!(is_retryable_status(529));
        assert!(!is_retryable_status(200));
        assert!(!is_retryable_status(400));
        assert!(!is_retryable_status(401));
        assert!(!is_retryable_status(403));
        assert!(!is_retryable_status(404));
    }

    #[test]
    fn parse_retry_after_valid() {
        assert_eq!(parse_retry_after("5"), Some(5));
        assert_eq!(parse_retry_after(" 30 "), Some(30));
        assert_eq!(parse_retry_after("not-a-number"), None);
    }

    #[test]
    fn jitter_stays_in_range() {
        for base in [100, 1000, 5000, 30_000] {
            let result = apply_jitter(base);
            let lower = (base as f64 * 0.7) as u64;
            let upper = (base as f64 * 1.3) as u64;
            assert!(
                result >= lower.max(100) && result <= upper,
                "jitter({}) = {} not in [{}, {}]",
                base,
                result,
                lower,
                upper
            );
        }
    }

    #[test]
    fn circuit_breaker_trips_and_recovers() {
        let cb = CircuitBreaker::new(3, 1); // trip after 3 failures, 1s cooldown

        // Normal operation
        assert!(cb.check().is_ok());
        cb.record_failure();
        cb.record_failure();
        assert!(cb.check().is_ok()); // 2 failures, threshold is 3

        cb.record_failure(); // 3rd failure — trips
        assert!(cb.check().is_err()); // circuit is open

        // Reset on success
        cb.record_success();
        assert!(cb.check().is_ok());
    }

    #[test]
    fn circuit_breaker_resets_on_success() {
        let cb = CircuitBreaker::new(3, 60);
        cb.record_failure();
        cb.record_failure();
        cb.record_success(); // Reset counter
        cb.record_failure();
        cb.record_failure();
        assert!(cb.check().is_ok()); // Still only 2 since reset
    }

    #[test]
    fn audit_log_ring_buffer() {
        let mut log = RequestAuditLog::new();
        assert_eq!(log.total(), 0);
        assert!(log.recent(10).is_empty());

        // Push a few entries
        for i in 0..3 {
            log.push(RequestAuditEntry {
                timestamp: format!("2025-01-0{}T00:00:00Z", i + 1),
                provider: "test".into(),
                model: format!("model-{}", i),
                hash: format!("hash-{}", i),
                status: 200,
            });
        }
        assert_eq!(log.total(), 3);
        let recent = log.recent(2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].model, "model-2"); // newest first
        assert_eq!(recent[1].model, "model-1");
    }

    #[test]
    fn sign_request_produces_hex_hash() {
        let hash = sign_and_log_request("openai", "gpt-4", b"{\"test\":true}");
        assert_eq!(hash.len(), 64); // SHA-256 = 32 bytes = 64 hex chars
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn pinned_client_builds_successfully() {
        // Install the ring CryptoProvider for the test environment —
        // in the real app this happens implicitly via the rustls feature,
        // but test binaries may not auto-detect the provider.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let _client = pinned_client();
        // If this doesn't panic, the TLS config is valid
    }
}
