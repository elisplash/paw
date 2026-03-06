// ─────────────────────────────────────────────────────────────────────────────
// Binary IPC — Atoms
//
// Pure types, constants, and deterministic functions for binary internal
// communication. No I/O, no Tauri handles, no SQLite.
//
// Phase 3 of the Agent Execution Roadmap:
//   1. MessagePack encoding/decoding (rmp-serde) for compact binary wire format
//   2. Delta event batching types (reduce IPC frequency from per-token to timed)
//   3. Structured agent envelopes (typed inter-agent messages, not free text)
//   4. Compact plan result types (binary accumulation during DAG execution)
//   5. Wire format benchmarking (measure JSON vs MessagePack sizes and speeds)
// ─────────────────────────────────────────────────────────────────────────────

use serde::{de::DeserializeOwned, Deserialize, Serialize};

// ── Wire Format ────────────────────────────────────────────────────────────

/// Supported wire formats for internal communication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WireFormat {
    /// JSON — human-readable, universally supported, higher overhead.
    Json,
    /// MessagePack — binary, compact (60-80% of JSON), 10-50× faster encoding.
    MessagePack,
}

/// Current binary envelope format version. Increment on breaking changes.
pub const FORMAT_VERSION: u8 = 1;

/// A format-tagged binary envelope for any serializable payload.
///
/// The envelope wraps data with a format tag and version so receivers
/// can detect and decode without out-of-band knowledge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryEnvelope {
    /// Wire format of the payload bytes.
    pub format: WireFormat,
    /// Envelope format version.
    pub version: u8,
    /// Serialized payload bytes.
    pub payload: Vec<u8>,
}

// ── MessagePack Encoding ───────────────────────────────────────────────────

/// Encode a value to MessagePack bytes.
///
/// Returns compact binary representation — typically 60-80% the size of JSON
/// for the same data, with 10-50× faster encode/decode.
pub fn msgpack_encode<T: Serialize>(val: &T) -> Result<Vec<u8>, String> {
    rmp_serde::to_vec(val).map_err(|e| format!("msgpack encode error: {}", e))
}

/// Decode a value from MessagePack bytes.
pub fn msgpack_decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    rmp_serde::from_slice(bytes).map_err(|e| format!("msgpack decode error: {}", e))
}

/// Encode a value as a named-field MessagePack map (compatible with JSON-like structure).
///
/// Uses `rmp_serde::to_vec_named` which preserves field names as map keys,
/// making the output debuggable and compatible with schema evolution.
pub fn msgpack_encode_named<T: Serialize>(val: &T) -> Result<Vec<u8>, String> {
    rmp_serde::to_vec_named(val).map_err(|e| format!("msgpack named encode error: {}", e))
}

/// Wrap a serializable value in a BinaryEnvelope with MessagePack encoding.
pub fn envelope_pack<T: Serialize>(val: &T) -> Result<BinaryEnvelope, String> {
    let payload = msgpack_encode(val)?;
    Ok(BinaryEnvelope {
        format: WireFormat::MessagePack,
        version: FORMAT_VERSION,
        payload,
    })
}

/// Unwrap a BinaryEnvelope, decoding the payload based on its format tag.
pub fn envelope_unpack<T: DeserializeOwned>(envelope: &BinaryEnvelope) -> Result<T, String> {
    match envelope.format {
        WireFormat::MessagePack => msgpack_decode(&envelope.payload),
        WireFormat::Json => serde_json::from_slice(&envelope.payload)
            .map_err(|e| format!("json decode error: {}", e)),
    }
}

// ── Delta Event Batching ───────────────────────────────────────────────────

/// Maximum number of deltas to accumulate before forcing a flush.
pub const MAX_BATCH_SIZE: usize = 32;

/// Time interval (ms) after which accumulated deltas are flushed even if
/// the batch isn't full. Balances latency vs. IPC overhead.
pub const BATCH_FLUSH_INTERVAL_MS: u64 = 50;

/// A single streaming text delta — minimal representation for batching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactDelta {
    /// The token text chunk.
    pub text: String,
    /// Sequence number within the current response (for ordering).
    pub seq: u32,
}

/// A batch of delta events, sent as a single IPC emission instead of
/// one emission per token. Reduces IPC overhead by up to MAX_BATCH_SIZE×.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaBatch {
    /// Session this batch belongs to.
    pub session_id: String,
    /// Run this batch belongs to.
    pub run_id: String,
    /// Ordered delta chunks in this batch.
    pub deltas: Vec<CompactDelta>,
    /// Total accumulated text (pre-joined for convenience).
    pub combined_text: String,
}

/// Configuration for delta batching behavior.
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Max deltas before forced flush.
    pub max_size: usize,
    /// Max ms between flushes.
    pub flush_interval_ms: u64,
    /// Whether batching is enabled (false = emit every delta immediately).
    pub enabled: bool,
}

impl Default for BatchConfig {
    fn default() -> Self {
        BatchConfig {
            max_size: MAX_BATCH_SIZE,
            flush_interval_ms: BATCH_FLUSH_INTERVAL_MS,
            enabled: true,
        }
    }
}

// ── Structured Agent Envelopes ─────────────────────────────────────────────

/// Message type for structured inter-agent communication.
///
/// Replaces untyped text strings with a discriminated message type,
/// enabling efficient routing and processing without parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMsgType {
    /// Standard direct message.
    Direct,
    /// Broadcast to all agents.
    Broadcast,
    /// Task delegation / handoff with structured context.
    Handoff,
    /// Status update (progress, completion, error).
    StatusUpdate,
    /// Structured data exchange (tables, results, embeddings).
    DataExchange,
}

/// A typed payload attached to an agent envelope.
///
/// Instead of free-text metadata, payloads carry structured data
/// that agents can process without JSON parsing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TypedPayload {
    /// Tool execution result forwarded to another agent.
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_name: String,
        output: String,
        success: bool,
    },
    /// A fragment of an execution plan for delegation.
    #[serde(rename = "plan_fragment")]
    PlanFragment {
        nodes: Vec<PlanNodeRef>,
        description: String,
    },
    /// Key-value data table (e.g., search results, records).
    #[serde(rename = "data_table")]
    DataTable {
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    /// Raw binary payload (opaque bytes, base64 in JSON mode).
    #[serde(rename = "raw")]
    Raw {
        #[serde(with = "base64_bytes")]
        data: Vec<u8>,
        content_type: String,
    },
}

/// A lightweight reference to a plan node for inter-agent delegation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanNodeRef {
    pub id: String,
    pub tool: String,
    pub description: String,
}

/// Structured envelope for inter-agent messages.
///
/// Replaces the current pattern of free-text `content` + optional JSON
/// `metadata` with a typed structure that can be serialized to MessagePack
/// for binary transport or JSON for backward compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEnvelope {
    /// Message type — determines routing and processing behavior.
    pub msg_type: AgentMsgType,
    /// Sender agent ID.
    pub from: String,
    /// Receiver agent ID (or "broadcast" for all agents).
    pub to: String,
    /// Topic channel (e.g., "general", "alerts", "handoff").
    pub channel: String,
    /// Human-readable content (still present for backward compat).
    pub content: String,
    /// Optional typed payload — structured data instead of free text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typed_payload: Option<TypedPayload>,
    /// Timestamp (Unix seconds).
    pub timestamp: i64,
}

// ── Compact Plan Results ───────────────────────────────────────────────────

/// Compact binary representation of a plan node result.
///
/// Mirrors `plan::atoms::NodeResult` but optimized for binary accumulation:
/// - Status is a u8 instead of an enum string
/// - Output stored as bytes (can be msgpack-encoded structured data)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactNodeResult {
    /// Node ID.
    pub node_id: String,
    /// Tool name.
    pub tool: String,
    /// 0 = Success, 1 = Error, 2 = Skipped.
    pub status: u8,
    /// Output bytes (UTF-8 text or msgpack-encoded structure).
    pub output: Vec<u8>,
    /// Whether error was retryable.
    pub retryable: bool,
    /// Retry count.
    pub retries: u32,
    /// Execution duration in milliseconds.
    pub duration_ms: u64,
}

/// Status code constants for CompactNodeResult.
pub const STATUS_SUCCESS: u8 = 0;
pub const STATUS_ERROR: u8 = 1;
pub const STATUS_SKIPPED: u8 = 2;

// ── Wire Format Benchmarking ───────────────────────────────────────────────

/// Wire format comparison statistics.
///
/// Used to measure and compare JSON vs MessagePack for real payloads,
/// enabling data-driven decisions about which paths to optimize.
#[derive(Debug, Clone, Default)]
pub struct WireStats {
    /// JSON-encoded byte count.
    pub json_bytes: usize,
    /// MessagePack-encoded byte count.
    pub msgpack_bytes: usize,
    /// JSON encoding duration in nanoseconds.
    pub json_encode_ns: u64,
    /// MessagePack encoding duration in nanoseconds.
    pub msgpack_encode_ns: u64,
    /// Compression ratio (msgpack_bytes / json_bytes). Lower = better for msgpack.
    pub compression_ratio: f64,
}

/// Measure and compare JSON vs MessagePack encoding for a value.
///
/// Returns wire statistics including byte sizes and encoding durations.
/// Use this to decide whether to apply binary optimization to a given path.
pub fn measure_wire_format<T: Serialize>(val: &T) -> WireStats {
    // JSON
    let json_start = std::time::Instant::now();
    let json_bytes = serde_json::to_vec(val).map(|v| v.len()).unwrap_or(0);
    let json_ns = json_start.elapsed().as_nanos() as u64;

    // MessagePack
    let msgpack_start = std::time::Instant::now();
    let msgpack_bytes = rmp_serde::to_vec(val).map(|v| v.len()).unwrap_or(0);
    let msgpack_ns = msgpack_start.elapsed().as_nanos() as u64;

    let compression_ratio = if json_bytes > 0 {
        msgpack_bytes as f64 / json_bytes as f64
    } else {
        1.0
    };

    WireStats {
        json_bytes,
        msgpack_bytes,
        json_encode_ns: json_ns,
        msgpack_encode_ns: msgpack_ns,
        compression_ratio,
    }
}

/// Check if MessagePack provides meaningful savings for a given path.
/// Returns true if compression ratio is below threshold (default: 0.85).
pub fn should_use_binary(stats: &WireStats, threshold: f64) -> bool {
    stats.compression_ratio < threshold && stats.msgpack_bytes > 0
}

// ── Base64 Serde Helper ────────────────────────────────────────────────────

/// Custom serde module for base64-encoding Vec<u8> in JSON mode.
mod base64_bytes {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(data: &[u8], s: S) -> Result<S::Ok, S::Error> {
        STANDARD.encode(data).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        STANDARD
            .decode(s)
            .map_err(|e| serde::de::Error::custom(format!("base64 decode: {}", e)))
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── MessagePack roundtrip ──────────────────────────────────────

    #[test]
    fn msgpack_roundtrip_string() {
        let val = "hello world".to_string();
        let bytes = msgpack_encode(&val).unwrap();
        let decoded: String = msgpack_decode(&bytes).unwrap();
        assert_eq!(val, decoded);
    }

    #[test]
    fn msgpack_roundtrip_struct() {
        let delta = CompactDelta {
            text: "Hello".to_string(),
            seq: 42,
        };
        let bytes = msgpack_encode(&delta).unwrap();
        let decoded: CompactDelta = msgpack_decode(&bytes).unwrap();
        assert_eq!(decoded.text, "Hello");
        assert_eq!(decoded.seq, 42);
    }

    #[test]
    fn msgpack_roundtrip_batch() {
        let batch = DeltaBatch {
            session_id: "s1".to_string(),
            run_id: "r1".to_string(),
            deltas: vec![
                CompactDelta {
                    text: "Hel".to_string(),
                    seq: 0,
                },
                CompactDelta {
                    text: "lo ".to_string(),
                    seq: 1,
                },
                CompactDelta {
                    text: "world".to_string(),
                    seq: 2,
                },
            ],
            combined_text: "Hello world".to_string(),
        };
        let bytes = msgpack_encode(&batch).unwrap();
        let decoded: DeltaBatch = msgpack_decode(&bytes).unwrap();
        assert_eq!(decoded.deltas.len(), 3);
        assert_eq!(decoded.combined_text, "Hello world");
    }

    #[test]
    fn msgpack_smaller_than_json() {
        let batch = DeltaBatch {
            session_id: "session-abc-123-def-456".to_string(),
            run_id: "run-789-ghi-012".to_string(),
            deltas: (0..10)
                .map(|i| CompactDelta {
                    text: format!("token_{}", i),
                    seq: i,
                })
                .collect(),
            combined_text: "token_0token_1token_2token_3token_4token_5token_6token_7token_8token_9"
                .to_string(),
        };

        let json_bytes = serde_json::to_vec(&batch).unwrap();
        let msgpack_bytes = msgpack_encode(&batch).unwrap();

        assert!(
            msgpack_bytes.len() < json_bytes.len(),
            "MessagePack ({} bytes) should be smaller than JSON ({} bytes)",
            msgpack_bytes.len(),
            json_bytes.len()
        );
    }

    // ── Envelope ───────────────────────────────────────────────────

    #[test]
    fn envelope_roundtrip() {
        let data = CompactDelta {
            text: "test".to_string(),
            seq: 1,
        };
        let env = envelope_pack(&data).unwrap();
        assert_eq!(env.format, WireFormat::MessagePack);
        assert_eq!(env.version, FORMAT_VERSION);

        let decoded: CompactDelta = envelope_unpack(&env).unwrap();
        assert_eq!(decoded.text, "test");
        assert_eq!(decoded.seq, 1);
    }

    #[test]
    fn envelope_json_decode() {
        let data = CompactDelta {
            text: "json".to_string(),
            seq: 5,
        };
        let json_payload = serde_json::to_vec(&data).unwrap();
        let env = BinaryEnvelope {
            format: WireFormat::Json,
            version: FORMAT_VERSION,
            payload: json_payload,
        };
        let decoded: CompactDelta = envelope_unpack(&env).unwrap();
        assert_eq!(decoded.text, "json");
    }

    // ── Named encoding ─────────────────────────────────────────────

    #[test]
    fn msgpack_named_roundtrip() {
        let delta = CompactDelta {
            text: "named".to_string(),
            seq: 99,
        };
        let bytes = msgpack_encode_named(&delta).unwrap();
        let decoded: CompactDelta = msgpack_decode(&bytes).unwrap();
        assert_eq!(decoded.text, "named");
        assert_eq!(decoded.seq, 99);
    }

    // ── Agent Envelope ─────────────────────────────────────────────

    #[test]
    fn agent_envelope_roundtrip() {
        let env = AgentEnvelope {
            msg_type: AgentMsgType::Direct,
            from: "agent-a".to_string(),
            to: "agent-b".to_string(),
            channel: "general".to_string(),
            content: "Please handle the email task".to_string(),
            typed_payload: Some(TypedPayload::ToolResult {
                tool_name: "gmail_search".to_string(),
                output: "Found 3 emails".to_string(),
                success: true,
            }),
            timestamp: 1709740800,
        };
        let bytes = msgpack_encode(&env).unwrap();
        let decoded: AgentEnvelope = msgpack_decode(&bytes).unwrap();
        assert_eq!(decoded.msg_type, AgentMsgType::Direct);
        assert_eq!(decoded.from, "agent-a");
        assert_eq!(decoded.content, "Please handle the email task");
        assert!(decoded.typed_payload.is_some());
    }

    #[test]
    fn agent_envelope_data_table() {
        let env = AgentEnvelope {
            msg_type: AgentMsgType::DataExchange,
            from: "research-agent".to_string(),
            to: "analysis-agent".to_string(),
            channel: "data".to_string(),
            content: "Here are the search results".to_string(),
            typed_payload: Some(TypedPayload::DataTable {
                columns: vec!["Name".into(), "Score".into()],
                rows: vec![
                    vec!["Alpha".into(), "0.95".into()],
                    vec!["Beta".into(), "0.87".into()],
                ],
            }),
            timestamp: 1709740800,
        };
        let bytes = msgpack_encode(&env).unwrap();
        let decoded: AgentEnvelope = msgpack_decode(&bytes).unwrap();
        if let Some(TypedPayload::DataTable { columns, rows }) = decoded.typed_payload {
            assert_eq!(columns.len(), 2);
            assert_eq!(rows.len(), 2);
        } else {
            panic!("Expected DataTable payload");
        }
    }

    #[test]
    fn agent_envelope_handoff() {
        let env = AgentEnvelope {
            msg_type: AgentMsgType::Handoff,
            from: "orchestrator".to_string(),
            to: "worker-1".to_string(),
            channel: "handoff".to_string(),
            content: "Execute these steps".to_string(),
            typed_payload: Some(TypedPayload::PlanFragment {
                nodes: vec![PlanNodeRef {
                    id: "a".to_string(),
                    tool: "gmail_search".to_string(),
                    description: "Search for action items".to_string(),
                }],
                description: "Email search sub-plan".to_string(),
            }),
            timestamp: 1709740800,
        };
        let bytes = msgpack_encode(&env).unwrap();
        assert!(msgpack_decode::<AgentEnvelope>(&bytes).is_ok());
    }

    // ── Compact Node Result ────────────────────────────────────────

    #[test]
    fn compact_node_result_roundtrip() {
        let result = CompactNodeResult {
            node_id: "a".to_string(),
            tool: "gmail_search".to_string(),
            status: STATUS_SUCCESS,
            output: b"Found 3 emails about action items".to_vec(),
            retryable: false,
            retries: 0,
            duration_ms: 245,
        };
        let bytes = msgpack_encode(&result).unwrap();
        let decoded: CompactNodeResult = msgpack_decode(&bytes).unwrap();
        assert_eq!(decoded.node_id, "a");
        assert_eq!(decoded.status, STATUS_SUCCESS);
        assert_eq!(decoded.duration_ms, 245);
    }

    #[test]
    fn compact_node_result_vec_roundtrip() {
        let results = vec![
            CompactNodeResult {
                node_id: "a".to_string(),
                tool: "gmail_search".to_string(),
                status: STATUS_SUCCESS,
                output: b"result_a".to_vec(),
                retryable: false,
                retries: 0,
                duration_ms: 100,
            },
            CompactNodeResult {
                node_id: "b".to_string(),
                tool: "calendar_list".to_string(),
                status: STATUS_ERROR,
                output: b"401 Unauthorized".to_vec(),
                retryable: true,
                retries: 2,
                duration_ms: 3500,
            },
            CompactNodeResult {
                node_id: "c".to_string(),
                tool: "gmail_send".to_string(),
                status: STATUS_SKIPPED,
                output: b"dependency 'b' failed".to_vec(),
                retryable: false,
                retries: 0,
                duration_ms: 0,
            },
        ];
        let bytes = msgpack_encode(&results).unwrap();
        let decoded: Vec<CompactNodeResult> = msgpack_decode(&bytes).unwrap();
        assert_eq!(decoded.len(), 3);
        assert_eq!(decoded[2].status, STATUS_SKIPPED);
    }

    // ── Wire Stats ─────────────────────────────────────────────────

    #[test]
    fn measure_wire_format_produces_valid_stats() {
        let batch = DeltaBatch {
            session_id: "s1".to_string(),
            run_id: "r1".to_string(),
            deltas: vec![CompactDelta {
                text: "hello".to_string(),
                seq: 0,
            }],
            combined_text: "hello".to_string(),
        };
        let stats = measure_wire_format(&batch);
        assert!(stats.json_bytes > 0);
        assert!(stats.msgpack_bytes > 0);
        assert!(stats.compression_ratio > 0.0);
        assert!(stats.compression_ratio < 1.0); // msgpack should be smaller
    }

    #[test]
    fn should_use_binary_threshold() {
        let good_stats = WireStats {
            json_bytes: 1000,
            msgpack_bytes: 600,
            compression_ratio: 0.6,
            ..Default::default()
        };
        assert!(should_use_binary(&good_stats, 0.85));

        let bad_stats = WireStats {
            json_bytes: 100,
            msgpack_bytes: 95,
            compression_ratio: 0.95,
            ..Default::default()
        };
        assert!(!should_use_binary(&bad_stats, 0.85));
    }

    // ── Batch Config ───────────────────────────────────────────────

    #[test]
    fn batch_config_default() {
        let cfg = BatchConfig::default();
        assert_eq!(cfg.max_size, MAX_BATCH_SIZE);
        assert_eq!(cfg.flush_interval_ms, BATCH_FLUSH_INTERVAL_MS);
        assert!(cfg.enabled);
    }

    // ── WireFormat serde ───────────────────────────────────────────

    #[test]
    fn wire_format_serde() {
        let json_val = serde_json::to_string(&WireFormat::Json).unwrap();
        assert_eq!(json_val, "\"json\"");
        let msgpack_val = serde_json::to_string(&WireFormat::MessagePack).unwrap();
        assert_eq!(msgpack_val, "\"messagepack\"");

        let decoded: WireFormat = serde_json::from_str("\"json\"").unwrap();
        assert_eq!(decoded, WireFormat::Json);
    }

    // ── AgentMsgType coverage ──────────────────────────────────────

    #[test]
    fn agent_msg_type_all_variants() {
        let types = [
            AgentMsgType::Direct,
            AgentMsgType::Broadcast,
            AgentMsgType::Handoff,
            AgentMsgType::StatusUpdate,
            AgentMsgType::DataExchange,
        ];
        for t in &types {
            let bytes = msgpack_encode(t).unwrap();
            let decoded: AgentMsgType = msgpack_decode(&bytes).unwrap();
            assert_eq!(*t, decoded);
        }
    }

    // ── Status constants ───────────────────────────────────────────

    #[test]
    fn status_constants_distinct() {
        assert_ne!(STATUS_SUCCESS, STATUS_ERROR);
        assert_ne!(STATUS_ERROR, STATUS_SKIPPED);
        assert_ne!(STATUS_SUCCESS, STATUS_SKIPPED);
    }
}
