// TOML Manifest — Type definitions
//
// All serde-deserializable structs for `pawz-skill.toml` manifest files,
// plus the `TomlSkillEntry` output struct used by the rest of the codebase.

use serde::Deserialize;
use crate::engine::skills::types::SkillDefinition;

// ── Manifest Structs ───────────────────────────────────────────────────────

/// Root of a `pawz-skill.toml` manifest file.
#[derive(Debug, Clone, Deserialize)]
pub struct SkillManifest {
    pub skill: SkillMeta,
    #[serde(default)]
    pub credentials: Vec<ManifestCredential>,
    pub instructions: Option<ManifestInstructions>,
    pub widget: Option<ManifestWidget>,
    pub mcp: Option<ManifestMcp>,
    pub view: Option<ManifestView>,
}

/// `[skill]` — required metadata section.
#[derive(Debug, Clone, Deserialize)]
pub struct SkillMeta {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub category: String,
    #[serde(default)]
    pub icon: String,
    pub description: String,
    #[serde(default)]
    pub install_hint: String,
    #[serde(default)]
    pub required_binaries: Vec<String>,
    #[serde(default)]
    pub required_env_vars: Vec<String>,
}

/// `[[credentials]]` — repeatable credential field declaration.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestCredential {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub placeholder: String,
}

/// `[instructions]` — agent instructions section.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestInstructions {
    pub text: String,
}

/// `[widget]` — dashboard widget declaration.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestWidget {
    #[serde(rename = "type")]
    pub widget_type: String,
    pub title: String,
    #[serde(default)]
    pub refresh: String,
    #[serde(default)]
    pub fields: Vec<ManifestWidgetField>,
}

/// `[[widget.fields]]` — widget field definition.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestWidgetField {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: String,
}

/// `[mcp]` — MCP server declaration.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestMcp {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(default)]
    pub url: String,
}

fn default_transport() -> String {
    "stdio".to_string()
}

/// `[view]` — custom sidebar tab declaration (Extension tier).
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestView {
    /// Display label for the sidebar tab.
    pub label: String,
    /// Material Symbol icon name for the tab.
    #[serde(default = "default_view_icon")]
    pub icon: String,
    /// Layout mode: "widget" (render skill_output as full tab) or "storage" (show KV table).
    #[serde(default = "default_view_layout")]
    pub layout: String,
}

fn default_view_icon() -> String {
    "extension".to_string()
}

fn default_view_layout() -> String {
    "widget".to_string()
}

// ── Output Struct ──────────────────────────────────────────────────────────

/// A loaded TOML skill with its source path for management purposes.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TomlSkillEntry {
    /// The SkillDefinition converted from the manifest.
    pub definition: SkillDefinition,
    /// The directory containing the manifest (e.g. `~/.paw/skills/notion/`).
    pub source_dir: String,
    /// The manifest version string.
    pub version: String,
    /// The manifest author.
    pub author: String,
    /// Whether the manifest has an `[mcp]` section.
    pub has_mcp: bool,
    /// Whether the manifest has a `[widget]` section.
    pub has_widget: bool,
    /// Whether the manifest has a `[view]` section (Extension tier).
    #[serde(default)]
    pub has_view: bool,
    /// View label for sidebar tab (if has_view is true).
    #[serde(default)]
    pub view_label: String,
    /// View icon for sidebar tab (if has_view is true).
    #[serde(default)]
    pub view_icon: String,
}
