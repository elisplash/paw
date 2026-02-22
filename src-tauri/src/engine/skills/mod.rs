// Pawz Agent Engine — Skills Module
// Modular replacement for the monolithic skills.rs.
// 
// Module layout:
//   types      — all skill types (SkillCategory, SkillDefinition, SkillStatus, …)
//   builtins   — the 40+ built-in skill definitions
//   vault      — SessionStore impl: credential CRUD, enabled state, custom instructions
//   crypto     — OS-keychain key, XOR encrypt/decrypt
//   status     — get_all_skill_status, get_skill_credentials
//   prompt     — get_enabled_skill_instructions, inject_credentials_into_instructions
//   community  — SKILL.md parser, GitHub fetcher, skills.sh search, DB CRUD
//   toml       — pawz-skill.toml manifest subsystem (types, parser, scanner, installer)

pub(crate) mod types;
mod builtins;
mod vault;
pub(crate) mod crypto;
mod status;
mod prompt;
pub mod community;
pub mod toml;

// ── Re-exports (keep crate::engine::skills::* API stable) ────────────────────

pub use types::{SkillCategory, SkillDefinition, SkillTier, SkillSource, CredentialField, SkillRecord, SkillStatus};
pub use builtins::builtin_skills;
pub use crypto::{decrypt_credential, encrypt_credential, get_vault_key, is_legacy_encrypted};
pub use status::{get_all_skill_status, get_skill_credentials};
pub use prompt::get_enabled_skill_instructions;
pub use community::{
    CommunitySkill, DiscoveredSkill,
    fetch_repo_skills, install_community_skill,
    search_community_skills, get_community_skill_instructions,
    parse_skill_md,
    PawzHubEntry, search_pawzhub, browse_pawzhub_category, fetch_pawzhub_toml,
};
pub use toml::{
    scan_toml_skills, install_toml_skill, uninstall_toml_skill,
    TomlSkillEntry, SkillManifest, parse_manifest,
};

// SkillTier is now defined in types.rs and re-exported above.
