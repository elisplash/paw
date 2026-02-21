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

pub(crate) mod types;
mod builtins;
mod vault;
pub(crate) mod crypto;
mod status;
mod prompt;
pub mod community;

// ── Re-exports (keep crate::engine::skills::* API stable) ────────────────────

pub use types::{SkillCategory, SkillDefinition, CredentialField, SkillRecord, SkillStatus};
pub use builtins::builtin_skills;
pub use crypto::{decrypt_credential, encrypt_credential, get_vault_key};
pub use status::{get_all_skill_status, get_skill_credentials};
pub use prompt::get_enabled_skill_instructions;
pub use community::{
    CommunitySkill, DiscoveredSkill,
    fetch_repo_skills, install_community_skill,
    search_community_skills, get_community_skill_instructions,
    parse_skill_md,
};

// ── SkillTier — extensibility tier classification (Phase 2 addition) ─────────

use serde::{Deserialize, Serialize};

/// The extensibility tier of a skill.
/// Introduced during the skills.rs refactor to distinguish prompt-only skills
/// from credential-bearing integrations and future storage-backed extensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillTier {
    /// Tier 1 — instruction-only SKILL.md, no credentials, no tools.
    Skill,
    /// Tier 2 — credential vault, optional tool gating, optional binaries.
    Integration,
    /// Tier 3 — integration + custom sidebar view + persistent storage (future).
    Extension,
}
