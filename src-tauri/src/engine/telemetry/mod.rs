// Paw Agent Engine — Telemetry Collector (Canvas Phase 5)
// Lightweight in-process span collector that captures timing data
// during agent turns and emits it to the frontend Inspector via Tauri events.
//
// Architecture:
// - No external OTLP dependency for default usage
// - Span data is collected per-run in a thread-safe buffer
// - On run completion, the buffer is flushed to the frontend as a TelemetryFlush event
// - Optional OTLP export via PAWZ_OTLP_ENDPOINT + PAWZ_OTLP_ENABLED env vars (future)

pub mod integration;

use chrono::Utc;
use log::info;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

// ── Span Data ─────────────────────────────────────────────────────────

/// A completed telemetry span — a timed operation within an agent turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetrySpan {
    /// Unique span identifier
    pub span_id: String,
    /// Parent span id (None for root spans)
    pub parent_id: Option<String>,
    /// Operation name (e.g. "agent_turn", "llm_request", "tool_execution")
    pub name: String,
    /// Key-value attributes (e.g. model, tool name, provider)
    pub attributes: HashMap<String, String>,
    /// Start time as epoch milliseconds
    pub start_ms: i64,
    /// End time as epoch milliseconds (0 if still running)
    pub end_ms: i64,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Whether the operation succeeded
    pub success: bool,
}

/// Summary metrics for a completed agent turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryTurnSummary {
    /// Session ID
    pub session_id: String,
    /// Run ID
    pub run_id: String,
    /// Model used
    pub model: Option<String>,
    /// Total turn duration in ms
    pub total_duration_ms: u64,
    /// Time spent waiting for LLM responses
    pub llm_duration_ms: u64,
    /// Time spent executing tools
    pub tool_duration_ms: u64,
    /// Number of rounds in this turn
    pub rounds: u32,
    /// Number of tool calls
    pub tool_calls: u32,
    /// Input tokens
    pub input_tokens: u64,
    /// Output tokens
    pub output_tokens: u64,
    /// Estimated cost in USD
    pub cost_usd: f64,
    /// All spans collected during this turn
    pub spans: Vec<TelemetrySpan>,
    /// Timestamp (ISO 8601)
    pub timestamp: String,
}

// ── Active Span Handle ────────────────────────────────────────────────

/// A handle to an in-progress span. Drop it or call `.finish()` to complete.
pub struct SpanHandle {
    span_id: String,
    parent_id: Option<String>,
    name: String,
    attributes: HashMap<String, String>,
    start: Instant,
    start_ms: i64,
    collector: Arc<Mutex<SpanBuffer>>,
    finished: bool,
}

impl SpanHandle {
    /// Add an attribute to this span.
    pub fn set_attribute(&mut self, key: &str, value: &str) {
        self.attributes.insert(key.to_string(), value.to_string());
    }

    /// Mark this span as finished with success status.
    pub fn finish(&mut self) {
        self.finish_with(true);
    }

    /// Mark this span as finished with explicit success/failure.
    pub fn finish_with(&mut self, success: bool) {
        if self.finished {
            return;
        }
        self.finished = true;
        let duration = self.start.elapsed();
        let end_ms = Utc::now().timestamp_millis();

        let span = TelemetrySpan {
            span_id: self.span_id.clone(),
            parent_id: self.parent_id.clone(),
            name: self.name.clone(),
            attributes: self.attributes.clone(),
            start_ms: self.start_ms,
            end_ms,
            duration_ms: duration.as_millis() as u64,
            success,
        };

        self.collector.lock().spans.push(span);
    }
}

impl Drop for SpanHandle {
    fn drop(&mut self) {
        if !self.finished {
            self.finish_with(true);
        }
    }
}

// ── Span Buffer ───────────────────────────────────────────────────────

/// In-memory buffer of spans for the current run.
struct SpanBuffer {
    spans: Vec<TelemetrySpan>,
}

impl SpanBuffer {
    fn new() -> Self {
        Self { spans: Vec::new() }
    }

    fn drain(&mut self) -> Vec<TelemetrySpan> {
        std::mem::take(&mut self.spans)
    }
}

// ── Run Collector ─────────────────────────────────────────────────────

/// Collects telemetry spans for a single agent run.
/// Create one per `run_agent_turn` call.
pub struct RunCollector {
    session_id: String,
    run_id: String,
    model: String,
    buffer: Arc<Mutex<SpanBuffer>>,
    root_start: Instant,
    #[allow(dead_code)]
    root_start_ms: i64,
    span_counter: Mutex<u64>,
    root_handle: Option<SpanHandle>,
}

impl RunCollector {
    /// Create a new collector for an agent run.
    pub fn new(session_id: &str, run_id: &str, model: &str) -> Self {
        let now = Utc::now();
        Self {
            session_id: session_id.to_string(),
            run_id: run_id.to_string(),
            model: model.to_string(),
            buffer: Arc::new(Mutex::new(SpanBuffer::new())),
            root_start: Instant::now(),
            root_start_ms: now.timestamp_millis(),
            span_counter: Mutex::new(0),
            root_handle: None,
        }
    }

    /// Start a new span with optional parent.
    pub fn start_span(&self, name: &str, parent_id: Option<&str>) -> SpanHandle {
        let mut counter = self.span_counter.lock();
        *counter += 1;
        let span_id = format!(
            "span-{}-{}",
            self.run_id.get(..8).unwrap_or(&self.run_id),
            counter
        );

        SpanHandle {
            span_id,
            parent_id: parent_id.map(|s| s.to_string()),
            name: name.to_string(),
            attributes: HashMap::new(),
            start: Instant::now(),
            start_ms: Utc::now().timestamp_millis(),
            collector: Arc::clone(&self.buffer),
            finished: false,
        }
    }

    /// Start a root-level span and return its span_id as a String
    /// (so the caller can pass it as parent_id to child spans).
    pub fn root_span(&mut self, name: &str) -> String {
        let handle = self.start_span(name, None);
        let id = handle.span_id.clone();
        // Don't drop/finish yet — store it to finish later
        self.root_handle = Some(handle);
        id
    }

    /// Finish the root span (call at end of turn).
    pub fn finish_root(&mut self) {
        if let Some(mut h) = self.root_handle.take() {
            h.finish();
        }
    }

    /// Build the turn summary from collected spans.
    /// Call `finish_root()` first so the root span is included.
    pub fn build_summary(
        &mut self,
        input_tokens: u64,
        output_tokens: u64,
        rounds: u32,
        tool_calls: u32,
    ) -> TelemetryTurnSummary {
        // Finish root span if still open
        self.finish_root();

        let spans = self.buffer.lock().drain();
        let total_duration = self.root_start.elapsed();

        info!(
            "[telemetry] Turn summary: session={} run={} model={} rounds={} tools={} total={}ms",
            self.session_id,
            &self.run_id[..self.run_id.len().min(12)],
            self.model,
            rounds,
            tool_calls,
            total_duration.as_millis(),
        );

        TelemetryTurnSummary {
            session_id: self.session_id.clone(),
            run_id: self.run_id.clone(),
            model: Some(self.model.clone()),
            total_duration_ms: total_duration.as_millis() as u64,
            llm_duration_ms: 0,  // Caller sets from turn_start - tool_time
            tool_duration_ms: 0, // Caller sets from accumulated tool timers
            rounds,
            tool_calls,
            input_tokens,
            output_tokens,
            cost_usd: 0.0, // Caller sets from pricing
            spans,
            timestamp: Utc::now().to_rfc3339(),
        }
    }
}

// ── OTLP Export Helpers ───────────────────────────────────────────────

/// Check if OTLP export is enabled via environment variables.
pub fn is_otlp_enabled() -> bool {
    std::env::var("PAWZ_OTLP_ENABLED")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
}

/// Get the configured OTLP endpoint, if any.
pub fn otlp_endpoint() -> Option<String> {
    std::env::var("PAWZ_OTLP_ENDPOINT").ok()
}
