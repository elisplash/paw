// ── Paw Atoms Layer ────────────────────────────────────────────────────────
// Pure constants and error types — zero side effects, no I/O.
// Dependency rule: atoms may only depend on std and external pure crates.
// Nothing here may import from engine/, commands.rs, or lib.rs.

pub mod constants;
pub mod engram_types;
pub mod error;
pub mod traits;
pub mod types;
