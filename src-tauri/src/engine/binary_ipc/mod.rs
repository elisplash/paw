// Binary IPC — Module barrel
//
// Binary internal communication infrastructure for Phase 3 of the
// Agent Execution Roadmap. Provides MessagePack encoding/decoding,
// delta event batching, structured agent envelopes, and compact plan
// result accumulation.
//
// Atomic structure:
//   atoms.rs     — Wire format types, msgpack encode/decode, benchmarks
//   molecules.rs — Event batcher, result accumulator, agent message codec

pub mod atoms;
pub mod molecules;

// Re-export primary types
pub use atoms::{
    envelope_pack, envelope_unpack, measure_wire_format, msgpack_decode, msgpack_encode,
    msgpack_encode_named, should_use_binary,
};
pub use atoms::{
    AgentEnvelope, AgentMsgType, BatchConfig, BinaryEnvelope, CompactDelta, CompactNodeResult,
    DeltaBatch, PlanNodeRef, TypedPayload, WireFormat, WireStats,
};
pub use molecules::{
    log_session_stats, recommended_format, AgentMessageCodec, BatcherStats, EventBatcher,
    ResultAccumulator,
};
