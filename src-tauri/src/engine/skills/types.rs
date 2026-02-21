// Pawz Agent Engine — Skill Types
use serde::{Deserialize, Serialize};

/// The extensibility tier of a skill.
/// Distinguishes prompt-only skills from credential-bearing integrations
/// and future storage-backed extensions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SkillTier {
    /// Tier 1 — instruction-only, no credentials, no dedicated tools.
    Skill,
    /// Tier 2 — credential vault, optional tool gating, optional binaries.
    Integration,
    /// Tier 3 — integration + custom sidebar view + persistent storage (future).
    Extension,
}

/// Skill categories for organization in the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SkillCategory {
    /// Core skills with dedicated tool functions and credential vault
    Vault,
    /// CLI tools the agent can use via exec
    Cli,
    /// API integrations the agent can use via fetch/exec
    Api,
    /// Productivity: notes, reminders, project management
    Productivity,
    /// Media: audio, video, images
    Media,
    /// Smart home and IoT
    SmartHome,
    /// Communication: messaging, calls
    Communication,
    /// Development: coding, CI/CD
    Development,
    /// System: security, monitoring
    System,
}

/// A skill definition — describes what the skill does and what it needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub category: SkillCategory,
    /// The extensibility tier (Skill, Integration, or Extension).
    pub tier: SkillTier,
    /// Credentials this skill requires (name → description). Empty for instruction-only skills.
    pub required_credentials: Vec<CredentialField>,
    /// The dedicated tool names this skill provides (vault skills only).
    pub tool_names: Vec<String>,
    /// CLI binaries required for this skill (checked on PATH).
    pub required_binaries: Vec<String>,
    /// Environment variables required for this skill.
    pub required_env_vars: Vec<String>,
    /// How to install missing dependencies (shown to user).
    pub install_hint: String,
    /// Instructions injected into the agent's system prompt when enabled.
    /// This teaches the agent HOW to use the skill's CLI/API.
    pub agent_instructions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialField {
    pub key: String,
    pub label: String,
    pub description: String,
    /// If true, this is a required field. If false, optional.
    pub required: bool,
    /// Hint text for the input field.
    pub placeholder: String,
}

/// A stored skill record (from DB) — tracks enabled state and credential status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRecord {
    pub skill_id: String,
    pub enabled: bool,
    /// Which credential keys have been set (not the values — just the key names).
    pub configured_keys: Vec<String>,
    pub updated_at: String,
}

/// Skill status for the frontend — combines definition + stored state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillStatus {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub category: SkillCategory,
    /// The extensibility tier (Skill, Integration, or Extension).
    pub tier: SkillTier,
    pub enabled: bool,
    pub required_credentials: Vec<CredentialField>,
    pub configured_credentials: Vec<String>,
    pub missing_credentials: Vec<String>,
    pub required_binaries: Vec<String>,
    pub missing_binaries: Vec<String>,
    pub required_env_vars: Vec<String>,
    pub missing_env_vars: Vec<String>,
    pub install_hint: String,
    pub is_ready: bool,
    pub tool_names: Vec<String>,
    pub has_instructions: bool,
    /// Default agent instructions (from builtin definition).
    pub default_instructions: String,
    /// Custom user-edited instructions (if any). Empty string = using defaults.
    pub custom_instructions: String,
}
