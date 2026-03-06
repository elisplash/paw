// ─────────────────────────────────────────────────────────────────────────────
// Binary IPC — Molecules
//
// Side-effectful components for binary internal communication:
//   1. EventBatcher — accumulates streaming deltas, flushes as batches
//   2. ResultAccumulator — binary buffer for plan DAG results
//   3. AgentMessageCodec — encode/decode structured agent envelopes
//
// These molecules integrate with the engine's existing Tauri IPC and plan
// execution infrastructure. They don't replace existing paths — they provide
// an optimized parallel path that can be activated per use site.
// ─────────────────────────────────────────────────────────────────────────────

use super::atoms::*;
use crate::engine::plan::atoms::{NodeResult, NodeStatus};
use log::info;
use std::time::Instant;

// ── Event Batcher ──────────────────────────────────────────────────────────

/// Accumulates streaming delta events and flushes them as batches.
///
/// Instead of emitting one Tauri IPC event per token (high overhead for
/// fast models generating 50-100 tokens/second), the batcher collects
/// deltas and flushes when either:
///   - The batch reaches MAX_BATCH_SIZE tokens, or
///   - BATCH_FLUSH_INTERVAL_MS milliseconds have passed since last flush
///
/// The caller checks `push_delta()` return value — if Some, emit the batch.
/// When streaming ends, call `flush()` to emit any remaining deltas.
pub struct EventBatcher {
    /// Accumulated deltas waiting to be flushed.
    pending: Vec<CompactDelta>,
    /// Session ID for this batch run.
    session_id: String,
    /// Run ID for this batch run.
    run_id: String,
    /// Sequence counter (monotonically increasing per batcher lifetime).
    seq: u32,
    /// When the last flush occurred (or batcher creation).
    last_flush: Instant,
    /// Batching configuration.
    config: BatchConfig,
    /// Cumulative statistics.
    total_deltas: u64,
    total_batches: u64,
}

impl EventBatcher {
    /// Create a new batcher for a specific session + run.
    pub fn new(session_id: &str, run_id: &str, config: BatchConfig) -> Self {
        EventBatcher {
            pending: Vec::with_capacity(config.max_size),
            session_id: session_id.to_string(),
            run_id: run_id.to_string(),
            seq: 0,
            last_flush: Instant::now(),
            config,
            total_deltas: 0,
            total_batches: 0,
        }
    }

    /// Push a delta token into the batcher.
    ///
    /// Returns Some(DeltaBatch) if the batch should be flushed now
    /// (either full or timed out). Returns None if more deltas can
    /// be accumulated.
    pub fn push_delta(&mut self, text: &str) -> Option<DeltaBatch> {
        if !self.config.enabled {
            // Batching disabled — return single-delta batch immediately
            self.seq += 1;
            self.total_deltas += 1;
            self.total_batches += 1;
            return Some(DeltaBatch {
                session_id: self.session_id.clone(),
                run_id: self.run_id.clone(),
                deltas: vec![CompactDelta {
                    text: text.to_string(),
                    seq: self.seq - 1,
                }],
                combined_text: text.to_string(),
            });
        }

        self.pending.push(CompactDelta {
            text: text.to_string(),
            seq: self.seq,
        });
        self.seq += 1;
        self.total_deltas += 1;

        // Flush if batch is full or timer expired
        if self.pending.len() >= self.config.max_size || self.timer_expired() {
            return self.flush();
        }

        None
    }

    /// Flush any pending deltas into a batch.
    ///
    /// Returns None if there are no pending deltas.
    /// Call this when streaming ends to emit the final partial batch.
    pub fn flush(&mut self) -> Option<DeltaBatch> {
        if self.pending.is_empty() {
            return None;
        }

        let deltas = std::mem::take(&mut self.pending);
        self.pending = Vec::with_capacity(self.config.max_size);
        self.last_flush = Instant::now();
        self.total_batches += 1;

        let combined_text: String = deltas.iter().map(|d| d.text.as_str()).collect();

        Some(DeltaBatch {
            session_id: self.session_id.clone(),
            run_id: self.run_id.clone(),
            deltas,
            combined_text,
        })
    }

    /// Check if the flush interval has expired.
    pub fn timer_expired(&self) -> bool {
        self.last_flush.elapsed().as_millis() as u64 >= self.config.flush_interval_ms
    }

    /// Number of deltas currently pending.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    /// Cumulative statistics.
    pub fn stats(&self) -> BatcherStats {
        BatcherStats {
            total_deltas: self.total_deltas,
            total_batches: self.total_batches,
            avg_batch_size: if self.total_batches > 0 {
                self.total_deltas as f64 / self.total_batches as f64
            } else {
                0.0
            },
            pending: self.pending.len(),
        }
    }
}

/// Batcher performance statistics.
#[derive(Debug, Clone)]
pub struct BatcherStats {
    /// Total deltas processed.
    pub total_deltas: u64,
    /// Total batches emitted.
    pub total_batches: u64,
    /// Average deltas per batch.
    pub avg_batch_size: f64,
    /// Currently pending deltas.
    pub pending: usize,
}

// ── Result Accumulator ─────────────────────────────────────────────────────

/// Accumulates plan DAG node results in a compact binary buffer.
///
/// Instead of building intermediate JSON objects during parallel plan
/// execution, results are serialized to MessagePack as they arrive.
/// When all nodes complete, the accumulated buffer can be:
///   - Deserialized to Vec<CompactNodeResult> for structured access
///   - Converted to a text context string for model injection
///
/// This avoids N separate JSON serialize+deserialize cycles during
/// plan execution, replacing them with N msgpack appends + 1 decode.
pub struct ResultAccumulator {
    /// Accumulated results (each individually serialized).
    results: Vec<CompactNodeResult>,
    /// Total byte size of all results if they were msgpack-encoded.
    estimated_bytes: usize,
}

impl Default for ResultAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

impl ResultAccumulator {
    /// Create an empty accumulator.
    pub fn new() -> Self {
        ResultAccumulator {
            results: Vec::new(),
            estimated_bytes: 0,
        }
    }

    /// Push a plan NodeResult into the accumulator, converting to compact form.
    pub fn push_result(&mut self, result: &NodeResult) {
        let status = match result.status {
            NodeStatus::Success => STATUS_SUCCESS,
            NodeStatus::Error => STATUS_ERROR,
            NodeStatus::Skipped => STATUS_SKIPPED,
        };

        let compact = CompactNodeResult {
            node_id: result.node_id.clone(),
            tool: result.tool.clone(),
            status,
            output: result.output.as_bytes().to_vec(),
            retryable: result.retryable,
            retries: result.retries,
            duration_ms: result.duration_ms,
        };

        // Estimate msgpack size (rough: header + field sizes)
        self.estimated_bytes +=
            compact.node_id.len() + compact.tool.len() + compact.output.len() + 32;

        self.results.push(compact);
    }

    /// Number of accumulated results.
    pub fn count(&self) -> usize {
        self.results.len()
    }

    /// Estimated total size in bytes.
    pub fn estimated_size(&self) -> usize {
        self.estimated_bytes
    }

    /// Get all accumulated results.
    pub fn results(&self) -> &[CompactNodeResult] {
        &self.results
    }

    /// Serialize all accumulated results to a single MessagePack buffer.
    pub fn to_msgpack(&self) -> Result<Vec<u8>, String> {
        msgpack_encode(&self.results)
    }

    /// Build a text context string for model injection.
    ///
    /// Matches the format of `plan::molecules::build_results_context()` but
    /// built from the compact binary representation.
    pub fn to_context_string(&self) -> String {
        let mut parts = Vec::new();
        parts.push("[Plan Execution Results]".to_string());

        let success_count = self
            .results
            .iter()
            .filter(|r| r.status == STATUS_SUCCESS)
            .count();
        let error_count = self
            .results
            .iter()
            .filter(|r| r.status == STATUS_ERROR)
            .count();
        let skip_count = self
            .results
            .iter()
            .filter(|r| r.status == STATUS_SKIPPED)
            .count();

        parts.push(format!(
            "Completed: {}/{} nodes ({} success, {} failed, {} skipped)",
            success_count + error_count,
            self.results.len(),
            success_count,
            error_count,
            skip_count
        ));

        for result in &self.results {
            let status_icon = match result.status {
                STATUS_SUCCESS => "\u{2713}",
                STATUS_ERROR => "\u{2717}",
                _ => "\u{2298}",
            };

            let duration = if result.duration_ms > 0 {
                format!(" ({}ms)", result.duration_ms)
            } else {
                String::new()
            };

            parts.push(format!(
                "\n[{} {} \u{2014} {}{}]",
                status_icon, result.node_id, result.tool, duration
            ));

            // Output as UTF-8 text, truncated to avoid context bloat
            let output_text = String::from_utf8_lossy(&result.output);
            let max_output = 2000;
            if output_text.len() > max_output {
                parts.push(format!(
                    "{}\n... (truncated, {} total chars)",
                    &output_text[..max_output],
                    output_text.len()
                ));
            } else if !output_text.is_empty() {
                parts.push(output_text.to_string());
            }
        }

        parts.join("\n")
    }
}

// ── Agent Message Codec ────────────────────────────────────────────────────

/// Codec for structured inter-agent messages.
///
/// Provides conversion between:
///   - AgentEnvelope ↔ MessagePack bytes (binary transport)
///   - AgentEnvelope ↔ legacy format (content string + metadata JSON)
///
/// The codec enables gradual migration: existing agent_comms.rs can
/// continue using (content, metadata) pairs, while new code uses
/// AgentEnvelope for richer structured communication.
pub struct AgentMessageCodec;

impl AgentMessageCodec {
    /// Encode an agent envelope to MessagePack bytes.
    ///
    /// Uses named-field encoding to preserve map keys, which is required
    /// for internally-tagged enums like TypedPayload to round-trip correctly.
    pub fn encode(envelope: &AgentEnvelope) -> Result<Vec<u8>, String> {
        msgpack_encode_named(envelope)
    }

    /// Decode an agent envelope from MessagePack bytes.
    pub fn decode(bytes: &[u8]) -> Result<AgentEnvelope, String> {
        msgpack_decode(bytes)
    }

    /// Convert a legacy (content, metadata) pair into a typed AgentEnvelope.
    ///
    /// Attempts to parse metadata as JSON to extract typed_payload.
    /// If metadata is not valid JSON or doesn't match a known payload type,
    /// it's stored as the content with msg_type = Direct.
    pub fn from_legacy(
        from: &str,
        to: &str,
        channel: &str,
        content: &str,
        metadata: Option<&str>,
    ) -> AgentEnvelope {
        let msg_type = if to == "broadcast" {
            AgentMsgType::Broadcast
        } else {
            AgentMsgType::Direct
        };

        let typed_payload = metadata.and_then(|meta| {
            let parsed: serde_json::Value = serde_json::from_str(meta).ok()?;
            // Check for known payload types in metadata
            if parsed.get("tool_name").is_some() {
                Some(TypedPayload::ToolResult {
                    tool_name: parsed["tool_name"].as_str().unwrap_or("").to_string(),
                    output: parsed["output"].as_str().unwrap_or("").to_string(),
                    success: parsed["success"].as_bool().unwrap_or(false),
                })
            } else if parsed.get("columns").is_some() && parsed.get("rows").is_some() {
                let columns: Vec<String> = parsed["columns"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let rows: Vec<Vec<String>> = parsed["rows"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|row| {
                                row.as_array().map(|r| {
                                    r.iter()
                                        .filter_map(|v| v.as_str().map(String::from))
                                        .collect()
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Some(TypedPayload::DataTable { columns, rows })
            } else {
                None
            }
        });

        AgentEnvelope {
            msg_type,
            from: from.to_string(),
            to: to.to_string(),
            channel: channel.to_string(),
            content: content.to_string(),
            typed_payload,
            timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// Convert a typed AgentEnvelope back to legacy (content, metadata) format.
    ///
    /// This allows new AgentEnvelope structures to be stored/displayed
    /// using the existing agent_comms infrastructure.
    pub fn to_legacy(envelope: &AgentEnvelope) -> (String, Option<String>) {
        let metadata = envelope.typed_payload.as_ref().map(|payload| {
            let json = match payload {
                TypedPayload::ToolResult {
                    tool_name,
                    output,
                    success,
                } => serde_json::json!({
                    "type": "tool_result",
                    "tool_name": tool_name,
                    "output": output,
                    "success": success,
                }),
                TypedPayload::PlanFragment { nodes, description } => {
                    let node_refs: Vec<serde_json::Value> = nodes
                        .iter()
                        .map(|n| {
                            serde_json::json!({
                                "id": n.id,
                                "tool": n.tool,
                                "description": n.description,
                            })
                        })
                        .collect();
                    serde_json::json!({
                        "type": "plan_fragment",
                        "nodes": node_refs,
                        "description": description,
                    })
                }
                TypedPayload::DataTable { columns, rows } => serde_json::json!({
                    "type": "data_table",
                    "columns": columns,
                    "rows": rows,
                }),
                TypedPayload::Raw { content_type, .. } => serde_json::json!({
                    "type": "raw",
                    "content_type": content_type,
                    "note": "Binary data omitted in legacy format",
                }),
            };
            json.to_string()
        });

        (envelope.content.clone(), metadata)
    }

    /// Estimate the wire size savings of using MessagePack vs JSON
    /// for a given envelope.
    pub fn measure(envelope: &AgentEnvelope) -> WireStats {
        measure_wire_format(envelope)
    }
}

// ── Format Negotiation ─────────────────────────────────────────────────────

/// Determine the best wire format for a given event type.
///
/// Hot-path events (Delta, ThinkingDelta) benefit most from binary.
/// Infrequent events (Complete, Error) keep JSON for debuggability.
pub fn recommended_format(event_kind: &str) -> WireFormat {
    match event_kind {
        // High-frequency: binary saves the most
        "delta" | "thinking_delta" | "tool_result" | "plan_node_start" => WireFormat::MessagePack,
        // Low-frequency or debug-critical: keep JSON
        _ => WireFormat::Json,
    }
}

/// Log a summary of binary IPC performance for the current session.
pub fn log_session_stats(batcher_stats: &BatcherStats, accumulator_bytes: usize) {
    info!(
        "[binary-ipc] Session stats: {} deltas in {} batches (avg {:.1}/batch), \
         {} bytes accumulated in plan results",
        batcher_stats.total_deltas,
        batcher_stats.total_batches,
        batcher_stats.avg_batch_size,
        accumulator_bytes,
    );
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── EventBatcher ───────────────────────────────────────────────

    #[test]
    fn batcher_accumulates_and_flushes_on_max() {
        let config = BatchConfig {
            max_size: 3,
            flush_interval_ms: 60_000, // won't expire in test
            enabled: true,
        };
        let mut batcher = EventBatcher::new("s1", "r1", config);

        assert!(batcher.push_delta("Hello").is_none());
        assert_eq!(batcher.pending_count(), 1);

        assert!(batcher.push_delta(" ").is_none());
        assert_eq!(batcher.pending_count(), 2);

        // Third push should trigger flush (max_size=3)
        let batch = batcher
            .push_delta("world")
            .expect("Should flush at max_size");
        assert_eq!(batch.deltas.len(), 3);
        assert_eq!(batch.combined_text, "Hello world");
        assert_eq!(batch.session_id, "s1");
        assert_eq!(batch.run_id, "r1");
        assert_eq!(batcher.pending_count(), 0);
    }

    #[test]
    fn batcher_flush_partial() {
        let config = BatchConfig {
            max_size: 10,
            flush_interval_ms: 60_000,
            enabled: true,
        };
        let mut batcher = EventBatcher::new("s1", "r1", config);

        batcher.push_delta("Hello");
        batcher.push_delta(" world");

        let batch = batcher.flush().expect("Should flush pending");
        assert_eq!(batch.deltas.len(), 2);
        assert_eq!(batch.combined_text, "Hello world");
    }

    #[test]
    fn batcher_flush_empty() {
        let config = BatchConfig::default();
        let mut batcher = EventBatcher::new("s1", "r1", config);
        assert!(batcher.flush().is_none());
    }

    #[test]
    fn batcher_disabled_returns_immediately() {
        let config = BatchConfig {
            enabled: false,
            ..Default::default()
        };
        let mut batcher = EventBatcher::new("s1", "r1", config);

        let batch = batcher
            .push_delta("token")
            .expect("Disabled should always return");
        assert_eq!(batch.deltas.len(), 1);
        assert_eq!(batch.combined_text, "token");
        assert_eq!(batcher.pending_count(), 0);
    }

    #[test]
    fn batcher_sequence_numbers() {
        let config = BatchConfig {
            max_size: 100,
            flush_interval_ms: 60_000,
            enabled: true,
        };
        let mut batcher = EventBatcher::new("s1", "r1", config);

        batcher.push_delta("a");
        batcher.push_delta("b");
        batcher.push_delta("c");

        let batch = batcher.flush().unwrap();
        assert_eq!(batch.deltas[0].seq, 0);
        assert_eq!(batch.deltas[1].seq, 1);
        assert_eq!(batch.deltas[2].seq, 2);
    }

    #[test]
    fn batcher_stats() {
        let config = BatchConfig {
            max_size: 2,
            flush_interval_ms: 60_000,
            enabled: true,
        };
        let mut batcher = EventBatcher::new("s1", "r1", config);

        batcher.push_delta("a"); // pending
        batcher.push_delta("b"); // flush → batch 1
        batcher.push_delta("c"); // pending
        batcher.push_delta("d"); // flush → batch 2

        let stats = batcher.stats();
        assert_eq!(stats.total_deltas, 4);
        assert_eq!(stats.total_batches, 2);
        assert!((stats.avg_batch_size - 2.0).abs() < 0.01);
        assert_eq!(stats.pending, 0);
    }

    // ── ResultAccumulator ──────────────────────────────────────────

    #[test]
    fn accumulator_push_and_count() {
        let mut acc = ResultAccumulator::new();
        assert_eq!(acc.count(), 0);

        acc.push_result(&NodeResult {
            node_id: "a".to_string(),
            tool: "gmail_search".to_string(),
            status: NodeStatus::Success,
            output: "Found emails".to_string(),
            retryable: false,
            retries: 0,
            duration_ms: 150,
        });

        assert_eq!(acc.count(), 1);
        assert!(acc.estimated_size() > 0);
    }

    #[test]
    fn accumulator_to_msgpack_roundtrip() {
        let mut acc = ResultAccumulator::new();

        acc.push_result(&NodeResult {
            node_id: "a".to_string(),
            tool: "gmail_search".to_string(),
            status: NodeStatus::Success,
            output: "Found 3 emails".to_string(),
            retryable: false,
            retries: 0,
            duration_ms: 100,
        });

        acc.push_result(&NodeResult {
            node_id: "b".to_string(),
            tool: "calendar_list".to_string(),
            status: NodeStatus::Error,
            output: "401 Unauthorized".to_string(),
            retryable: true,
            retries: 2,
            duration_ms: 3200,
        });

        let bytes = acc.to_msgpack().unwrap();
        let decoded: Vec<CompactNodeResult> = msgpack_decode(&bytes).unwrap();
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].status, STATUS_SUCCESS);
        assert_eq!(decoded[1].status, STATUS_ERROR);
        assert!(decoded[1].retryable);
    }

    #[test]
    fn accumulator_context_string_format() {
        let mut acc = ResultAccumulator::new();

        acc.push_result(&NodeResult {
            node_id: "a".to_string(),
            tool: "gmail_search".to_string(),
            status: NodeStatus::Success,
            output: "Found 3 emails".to_string(),
            retryable: false,
            retries: 0,
            duration_ms: 100,
        });

        acc.push_result(&NodeResult {
            node_id: "b".to_string(),
            tool: "calendar_list".to_string(),
            status: NodeStatus::Skipped,
            output: "dependency 'a' failed".to_string(),
            retryable: false,
            retries: 0,
            duration_ms: 0,
        });

        let ctx = acc.to_context_string();
        assert!(ctx.contains("[Plan Execution Results]"));
        assert!(ctx.contains("1 success"));
        assert!(ctx.contains("1 skipped"));
        assert!(ctx.contains("gmail_search"));
        assert!(ctx.contains("Found 3 emails"));
    }

    #[test]
    fn accumulator_truncates_long_output() {
        let mut acc = ResultAccumulator::new();

        let long_output = "x".repeat(3000);
        acc.push_result(&NodeResult {
            node_id: "a".to_string(),
            tool: "web_search".to_string(),
            status: NodeStatus::Success,
            output: long_output,
            retryable: false,
            retries: 0,
            duration_ms: 500,
        });

        let ctx = acc.to_context_string();
        assert!(ctx.contains("truncated"));
        assert!(ctx.contains("3000 total chars"));
    }

    // ── AgentMessageCodec ──────────────────────────────────────────

    #[test]
    fn codec_encode_decode_roundtrip() {
        let envelope = AgentEnvelope {
            msg_type: AgentMsgType::Direct,
            from: "agent-a".to_string(),
            to: "agent-b".to_string(),
            channel: "general".to_string(),
            content: "Handle this task".to_string(),
            typed_payload: None,
            timestamp: 1709740800,
        };

        let bytes = AgentMessageCodec::encode(&envelope).unwrap();
        let decoded = AgentMessageCodec::decode(&bytes).unwrap();
        assert_eq!(decoded.from, "agent-a");
        assert_eq!(decoded.to, "agent-b");
        assert_eq!(decoded.content, "Handle this task");
    }

    #[test]
    fn codec_from_legacy_broadcast() {
        let env = AgentMessageCodec::from_legacy(
            "orchestrator",
            "broadcast",
            "alerts",
            "System update completed",
            None,
        );
        assert_eq!(env.msg_type, AgentMsgType::Broadcast);
        assert_eq!(env.channel, "alerts");
        assert!(env.typed_payload.is_none());
    }

    #[test]
    fn codec_from_legacy_with_tool_metadata() {
        let metadata = r#"{"tool_name":"gmail_search","output":"3 results","success":true}"#;
        let env = AgentMessageCodec::from_legacy(
            "worker",
            "orchestrator",
            "results",
            "Search completed",
            Some(metadata),
        );
        assert_eq!(env.msg_type, AgentMsgType::Direct);
        match &env.typed_payload {
            Some(TypedPayload::ToolResult {
                tool_name, success, ..
            }) => {
                assert_eq!(tool_name, "gmail_search");
                assert!(success);
            }
            _ => panic!("Expected ToolResult payload"),
        }
    }

    #[test]
    fn codec_from_legacy_with_data_table() {
        let metadata = r#"{"columns":["name","score"],"rows":[["alpha","0.9"],["beta","0.8"]]}"#;
        let env = AgentMessageCodec::from_legacy(
            "research",
            "analyst",
            "data",
            "Results",
            Some(metadata),
        );
        match &env.typed_payload {
            Some(TypedPayload::DataTable { columns, rows }) => {
                assert_eq!(columns.len(), 2);
                assert_eq!(rows.len(), 2);
            }
            _ => panic!("Expected DataTable payload"),
        }
    }

    #[test]
    fn codec_to_legacy_roundtrip() {
        let envelope = AgentEnvelope {
            msg_type: AgentMsgType::Direct,
            from: "a".to_string(),
            to: "b".to_string(),
            channel: "general".to_string(),
            content: "Hello".to_string(),
            typed_payload: Some(TypedPayload::ToolResult {
                tool_name: "web_search".to_string(),
                output: "Found 5 results".to_string(),
                success: true,
            }),
            timestamp: 1709740800,
        };

        let (content, metadata) = AgentMessageCodec::to_legacy(&envelope);
        assert_eq!(content, "Hello");
        assert!(metadata.is_some());
        let meta_json: serde_json::Value = serde_json::from_str(&metadata.unwrap()).unwrap();
        assert_eq!(meta_json["type"], "tool_result");
        assert_eq!(meta_json["tool_name"], "web_search");
    }

    #[test]
    fn codec_to_legacy_no_payload() {
        let envelope = AgentEnvelope {
            msg_type: AgentMsgType::Direct,
            from: "a".to_string(),
            to: "b".to_string(),
            channel: "general".to_string(),
            content: "Simple message".to_string(),
            typed_payload: None,
            timestamp: 1709740800,
        };

        let (content, metadata) = AgentMessageCodec::to_legacy(&envelope);
        assert_eq!(content, "Simple message");
        assert!(metadata.is_none());
    }

    #[test]
    fn codec_measure_shows_savings() {
        let envelope = AgentEnvelope {
            msg_type: AgentMsgType::DataExchange,
            from: "research-agent-with-long-name".to_string(),
            to: "analysis-agent-with-long-name".to_string(),
            channel: "data-exchange-channel".to_string(),
            content: "Here is a substantial message with real content that has enough text to show meaningful compression gains with MessagePack binary encoding versus JSON text encoding".to_string(),
            typed_payload: Some(TypedPayload::DataTable {
                columns: vec!["Name".into(), "Score".into(), "Category".into()],
                rows: (0..10)
                    .map(|i| vec![format!("Item {}", i), format!("0.{}", i), "category_a".into()])
                    .collect(),
            }),
            timestamp: 1709740800,
        };

        let stats = AgentMessageCodec::measure(&envelope);
        assert!(stats.json_bytes > 0);
        assert!(stats.msgpack_bytes > 0);
        // MessagePack should be meaningfully smaller for this payload
        assert!(
            stats.msgpack_bytes < stats.json_bytes,
            "MessagePack ({}) should be smaller than JSON ({})",
            stats.msgpack_bytes,
            stats.json_bytes
        );
    }

    // ── Format Negotiation ─────────────────────────────────────────

    #[test]
    fn recommended_format_hot_path() {
        assert_eq!(recommended_format("delta"), WireFormat::MessagePack);
        assert_eq!(
            recommended_format("thinking_delta"),
            WireFormat::MessagePack
        );
        assert_eq!(recommended_format("tool_result"), WireFormat::MessagePack);
    }

    #[test]
    fn recommended_format_cold_path() {
        assert_eq!(recommended_format("complete"), WireFormat::Json);
        assert_eq!(recommended_format("error"), WireFormat::Json);
        assert_eq!(recommended_format("plan_start"), WireFormat::Json);
    }

    // ── Log session stats (smoke test) ─────────────────────────────

    #[test]
    fn log_stats_does_not_panic() {
        let stats = BatcherStats {
            total_deltas: 100,
            total_batches: 10,
            avg_batch_size: 10.0,
            pending: 0,
        };
        log_session_stats(&stats, 4096);
    }
}
