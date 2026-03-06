// Constrained Decoding — Module barrel
//
// Atomic structure:
//   atoms.rs     — Pure types, capability detection, schema enforcement
//   molecules.rs — Body mutation functions per provider

pub mod atoms;
pub mod molecules;

// Re-export the most commonly used items
pub use atoms::{detect_constraints, ConstraintConfig, ConstraintLevel};
pub use molecules::{
    apply_anthropic_tool_choice, apply_google_tool_config, apply_ollama_json_format,
    apply_openai_strict,
};
