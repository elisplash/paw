// ─────────────────────────────────────────────────────────────────────────────
// Action DAG Planning — Module barrel
//
// Atomic structure:
//   atoms.rs     — Pure types, constants, validation (no I/O)
//   molecules.rs — Parser, validator (reads tool registry)
//   executor.rs  — Parallel execution engine (side effects, IPC)
// ─────────────────────────────────────────────────────────────────────────────

pub mod atoms;
pub mod executor;
pub mod molecules;

// Re-export the key types for ergonomic use
pub use atoms::{ExecutionPlan, NodeResult, NodeStatus, PlanNode, PlanValidationError};
pub use executor::execute_plan;
pub use molecules::{build_results_context, describe_plan, parse_plan, validate_plan};
