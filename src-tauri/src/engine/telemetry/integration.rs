// Paw Agent Engine — Telemetry Integration (Canvas Phase 5)
// Thin helpers that connect RunCollector to agent_loop + tool execution
// without bloating those files.

use super::{RunCollector, TelemetryTurnSummary};
use crate::engine::sessions::SessionStore;
use log::info;
use std::time::Instant;

/// Capture timing around a tool execution and record a child span.
pub struct ToolTimer {
    tool_name: String,
    start: Instant,
}

impl ToolTimer {
    /// Start timing a tool execution.
    pub fn start(tool_name: &str) -> Self {
        Self {
            tool_name: tool_name.to_string(),
            start: Instant::now(),
        }
    }

    /// Finish timing and record a span into the collector.
    pub fn finish(self, collector: &RunCollector, root_id: &str, success: bool) -> u64 {
        let duration_ms = self.start.elapsed().as_millis() as u64;

        let mut handle = collector.start_span(&self.tool_name, Some(root_id));
        handle.set_attribute("tool.success", if success { "true" } else { "false" });
        handle.set_attribute("tool.duration_ms", &duration_ms.to_string());
        handle.finish_with(success);

        duration_ms
    }
}

/// Record a turn's telemetry summary into the SessionStore.
pub fn persist_summary(store: &SessionStore, summary: &TelemetryTurnSummary) {
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let model_str = summary.model.as_deref().unwrap_or("unknown");
    if let Err(e) = store.record_metric(
        &date,
        &summary.session_id,
        model_str,
        summary.input_tokens,
        summary.output_tokens,
        summary.cost_usd,
        summary.tool_calls,
        summary.tool_duration_ms,
        summary.llm_duration_ms,
        summary.total_duration_ms,
        summary.rounds,
    ) {
        log::warn!("[telemetry] Failed to persist turn metrics: {}", e);
    } else {
        info!(
            "[telemetry] Recorded turn metrics: session={} model={} cost=${:.4}",
            summary.session_id, model_str, summary.cost_usd
        );
    }
}

/// Emit a telemetry summary as a Tauri event for the frontend Inspector.
pub fn emit_summary(app_handle: &tauri::AppHandle, summary: &TelemetryTurnSummary) {
    use tauri::Emitter;
    let _ = app_handle.emit("telemetry-flush", summary);
}
