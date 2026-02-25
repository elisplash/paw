// Pawz Agent Engine — Skills Module
// Modular replacement for the monolithic skills.rs.
//
// Module layout:
//   types      — all skill types (SkillCategory, SkillDefinition, SkillStatus, …)
//   builtins   — the 400+ built-in skill definitions
//   vault      — SessionStore impl: credential CRUD, enabled state, custom instructions
//   crypto     — OS-keychain key, XOR encrypt/decrypt
//   status     — get_all_skill_status, get_skill_credentials
//   prompt     — get_enabled_skill_instructions, inject_credentials_into_instructions
//   community  — SKILL.md parser, GitHub fetcher, skills.sh search, DB CRUD
//   toml       — pawz-skill.toml manifest subsystem (types, parser, scanner, installer)

mod builtins;
pub mod community;
pub(crate) mod crypto;
mod prompt;
mod status;
pub mod toml;
pub(crate) mod types;
mod vault;

// ── Re-exports (keep crate::engine::skills::* API stable) ────────────────────

pub use builtins::builtin_skills;
pub use community::{
    browse_pawzhub_category, fetch_pawzhub_toml, fetch_repo_skills,
    get_community_skill_instructions, install_community_skill, parse_skill_md,
    search_community_skills, search_pawzhub, CommunitySkill, DiscoveredSkill, PawzHubEntry,
};
pub use crypto::{decrypt_credential, encrypt_credential, get_vault_key, is_legacy_encrypted};
pub use prompt::get_enabled_skill_instructions;
pub use status::{get_all_skill_status, get_skill_credentials};
pub use toml::{
    install_toml_skill, parse_manifest, scan_toml_skills, uninstall_toml_skill, SkillManifest,
    TomlSkillEntry,
};
pub use types::{
    CredentialField, SkillCategory, SkillDefinition, SkillRecord, SkillSource, SkillStatus,
    SkillTier,
};

// SkillTier is now defined in types.rs and re-exported above.
