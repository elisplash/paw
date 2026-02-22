// Pawz Agent Engine — TOML Manifest Module
//
// Modular replacement for the former monolithic `toml_loader.rs`.
//
// Module layout:
//   types     — serde structs (SkillManifest, ManifestMcp, TomlSkillEntry, …)
//   parser    — parse_manifest, validate_manifest, manifest_to_definition
//   scanner   — skills_dir, scan_toml_skills, load_manifest_from_path
//   installer — install_toml_skill, uninstall_toml_skill

pub(crate) mod types;
mod parser;
mod scanner;
mod installer;

// ── Re-exports (keep crate::engine::skills::toml::* API stable) ────────────

pub use types::{SkillManifest, TomlSkillEntry};
pub use parser::{parse_manifest, validate_manifest, manifest_to_definition, parse_category};
pub use scanner::{skills_dir, scan_toml_skills, load_manifest_from_path};
pub use installer::{install_toml_skill, uninstall_toml_skill};
