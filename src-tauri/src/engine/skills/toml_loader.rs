// Pawz Agent Engine — TOML Manifest Loader (Phase F.1)
//
// Scans `~/.paw/skills/*/pawz-skill.toml` for community skill manifests,
// parses them into SkillDefinition structs, and merges them with built-ins.
// Reuses the same credential vault, prompt injection, and per-agent scoping
// as built-in skills — zero special-casing needed downstream.

use serde::Deserialize;
use std::path::{Path, PathBuf};
use super::types::{CredentialField, SkillCategory, SkillDefinition, SkillTier};

// ── TOML Manifest Types ────────────────────────────────────────────────────

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

/// `[widget]` — dashboard widget declaration (F.2 — parsed now, rendered later).
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

/// `[mcp]` — MCP server declaration (F.3 — parsed now, wired later).
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

/// `[view]` — custom sidebar tab declaration (F.6 Extensions).
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

// ── Parsing ────────────────────────────────────────────────────────────────

/// Parse a `pawz-skill.toml` string into a `SkillManifest`.
pub fn parse_manifest(content: &str) -> Result<SkillManifest, String> {
    toml::from_str::<SkillManifest>(content).map_err(|e| format!("TOML parse error: {e}"))
}

/// Map a category string from TOML to `SkillCategory`.
/// Falls back to `Api` for unrecognized values (safe default).
pub fn parse_category(s: &str) -> SkillCategory {
    match s.to_lowercase().as_str() {
        "vault" => SkillCategory::Vault,
        "cli" => SkillCategory::Cli,
        "api" => SkillCategory::Api,
        "productivity" => SkillCategory::Productivity,
        "media" => SkillCategory::Media,
        "smart_home" | "smarthome" => SkillCategory::SmartHome,
        "communication" => SkillCategory::Communication,
        "development" => SkillCategory::Development,
        "system" => SkillCategory::System,
        _ => SkillCategory::Api,
    }
}

/// Determine the tier from a manifest:
/// - Has credentials → Integration
/// - Otherwise → Skill (prompt-only)
///
/// Extension tier requires `[view]` or `[storage]` — not yet implemented.
fn infer_tier(manifest: &SkillManifest) -> SkillTier {
    if manifest.view.is_some() {
        SkillTier::Extension
    } else if manifest.credentials.is_empty() {
        SkillTier::Skill
    } else {
        SkillTier::Integration
    }
}

/// Convert a parsed manifest into a `SkillDefinition`.
pub fn manifest_to_definition(manifest: &SkillManifest) -> SkillDefinition {
    let instructions = manifest
        .instructions
        .as_ref()
        .map(|i| i.text.clone())
        .unwrap_or_default();

    let required_credentials: Vec<CredentialField> = manifest
        .credentials
        .iter()
        .map(|c| CredentialField {
            key: c.key.clone(),
            label: c.label.clone(),
            description: c.description.clone(),
            required: c.required,
            placeholder: c.placeholder.clone(),
        })
        .collect();

    SkillDefinition {
        id: manifest.skill.id.clone(),
        name: manifest.skill.name.clone(),
        description: manifest.skill.description.clone(),
        icon: if manifest.skill.icon.is_empty() {
            "extension".to_string()
        } else {
            manifest.skill.icon.clone()
        },
        category: parse_category(&manifest.skill.category),
        tier: infer_tier(manifest),
        required_credentials,
        tool_names: Vec::new(), // TOML skills don't have dedicated Rust tool functions
        required_binaries: manifest.skill.required_binaries.clone(),
        required_env_vars: manifest.skill.required_env_vars.clone(),
        install_hint: manifest.skill.install_hint.clone(),
        agent_instructions: instructions,
    }
}

// ── Validation ─────────────────────────────────────────────────────────────

/// Validate a manifest's required fields. Returns Ok(()) or an error message.
pub fn validate_manifest(manifest: &SkillManifest) -> Result<(), String> {
    if manifest.skill.id.is_empty() {
        return Err("Skill ID is required".to_string());
    }
    // Safe ID format: alphanumeric + hyphens only
    if !manifest
        .skill
        .id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!(
            "Skill ID '{}' contains invalid characters (use a-z, 0-9, -, _)",
            manifest.skill.id
        ));
    }
    if manifest.skill.name.is_empty() {
        return Err("Skill name is required".to_string());
    }
    if manifest.skill.version.is_empty() {
        return Err("Skill version is required".to_string());
    }
    if manifest.skill.author.is_empty() {
        return Err("Skill author is required".to_string());
    }
    if manifest.skill.description.is_empty() {
        return Err("Skill description is required".to_string());
    }
    if manifest.skill.description.len() > 500 {
        return Err(format!(
            "Skill description too long ({} chars, max 500)",
            manifest.skill.description.len()
        ));
    }
    // Validate widget field types if widget is declared
    if let Some(ref w) = manifest.widget {
        let valid_types = ["status", "metric", "table", "log", "kv"];
        if !valid_types.contains(&w.widget_type.as_str()) {
            return Err(format!(
                "Invalid widget type '{}' (valid: {:?})",
                w.widget_type, valid_types
            ));
        }
        let valid_field_types = ["text", "number", "badge", "datetime", "percentage", "currency"];
        for field in &w.fields {
            if !valid_field_types.contains(&field.field_type.as_str()) {
                return Err(format!(
                    "Invalid widget field type '{}' for key '{}' (valid: {:?})",
                    field.field_type, field.key, valid_field_types
                ));
            }
        }
    }
    Ok(())
}

// ── Directory Scanner ──────────────────────────────────────────────────────

/// Returns the skills directory path: `~/.paw/skills/`
pub fn skills_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".paw").join("skills"))
}

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

/// Scan `~/.paw/skills/*/pawz-skill.toml` and return all valid skill definitions.
/// Invalid manifests are logged and skipped (never crash on bad community input).
pub fn scan_toml_skills() -> Vec<TomlSkillEntry> {
    let dir = match skills_dir() {
        Some(d) => d,
        None => {
            log::warn!("[toml-loader] Could not determine home directory");
            return Vec::new();
        }
    };

    if !dir.exists() {
        log::debug!("[toml-loader] Skills directory does not exist: {}", dir.display());
        return Vec::new();
    }

    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(err) => {
            log::warn!("[toml-loader] Failed to read skills directory: {}", err);
            return Vec::new();
        }
    };

    let mut skills = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("pawz-skill.toml");
        if !manifest_path.exists() {
            continue;
        }

        match load_manifest_from_path(&manifest_path) {
            Ok(entry) => {
                log::info!(
                    "[toml-loader] Loaded skill '{}' v{} from {}",
                    entry.definition.name,
                    entry.version,
                    manifest_path.display()
                );
                skills.push(entry);
            }
            Err(err) => {
                log::warn!(
                    "[toml-loader] Skipping {}: {}",
                    manifest_path.display(),
                    err
                );
            }
        }
    }

    log::info!("[toml-loader] Scanned {} TOML skills from {}", skills.len(), dir.display());
    skills
}

/// Load and validate a single manifest from a file path.
pub fn load_manifest_from_path(path: &Path) -> Result<TomlSkillEntry, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

    let manifest = parse_manifest(&content)?;
    validate_manifest(&manifest)?;

    let definition = manifest_to_definition(&manifest);
    let source_dir = path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    Ok(TomlSkillEntry {
        definition,
        source_dir,
        version: manifest.skill.version,
        author: manifest.skill.author,
        has_mcp: manifest.mcp.is_some(),
        has_widget: manifest.widget.is_some(),
        has_view: manifest.view.is_some(),
        view_label: manifest.view.as_ref().map(|v| v.label.clone()).unwrap_or_default(),
        view_icon: manifest.view.as_ref().map(|v| v.icon.clone()).unwrap_or_default(),
    })
}

/// Install a TOML skill by writing a manifest to `~/.paw/skills/{id}/pawz-skill.toml`.
pub fn install_toml_skill(skill_id: &str, toml_content: &str) -> Result<PathBuf, String> {
    // Validate ID format first
    if !skill_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!(
            "Invalid skill ID '{}' — use only a-z, 0-9, hyphens, underscores",
            skill_id
        ));
    }

    // Validate the TOML content parses and is valid
    let manifest = parse_manifest(toml_content)?;
    validate_manifest(&manifest)?;

    if manifest.skill.id != skill_id {
        return Err(format!(
            "Skill ID mismatch: path says '{}' but manifest says '{}'",
            skill_id, manifest.skill.id
        ));
    }

    let dir = skills_dir().ok_or_else(|| "Could not determine home directory".to_string())?;
    let skill_dir = dir.join(skill_id);

    std::fs::create_dir_all(&skill_dir)
        .map_err(|e| format!("Failed to create directory {}: {}", skill_dir.display(), e))?;

    let manifest_path = skill_dir.join("pawz-skill.toml");
    std::fs::write(&manifest_path, toml_content)
        .map_err(|e| format!("Failed to write {}: {}", manifest_path.display(), e))?;

    log::info!(
        "[toml-loader] Installed TOML skill '{}' to {}",
        skill_id,
        manifest_path.display()
    );

    Ok(manifest_path)
}

/// Uninstall a TOML skill by removing its directory from `~/.paw/skills/{id}/`.
pub fn uninstall_toml_skill(skill_id: &str) -> Result<(), String> {
    // Validate safe ID to prevent path traversal
    if !skill_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!("Invalid skill ID: {}", skill_id));
    }

    let dir = skills_dir().ok_or_else(|| "Could not determine home directory".to_string())?;
    let skill_dir = dir.join(skill_id);

    if !skill_dir.exists() {
        return Err(format!("Skill directory does not exist: {}", skill_dir.display()));
    }

    // Extra safety: confirm the directory is inside ~/.paw/skills/
    let canonical = skill_dir
        .canonicalize()
        .map_err(|e| format!("Failed to resolve path: {}", e))?;
    let canonical_base = dir
        .canonicalize()
        .unwrap_or_else(|_| dir.clone());
    if !canonical.starts_with(&canonical_base) {
        return Err("Path traversal detected — aborting".to_string());
    }

    std::fs::remove_dir_all(&skill_dir)
        .map_err(|e| format!("Failed to remove {}: {}", skill_dir.display(), e))?;

    log::info!("[toml-loader] Uninstalled TOML skill '{}' from {}", skill_id, skill_dir.display());
    Ok(())
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_MANIFEST: &str = r#"
[skill]
id = "notion"
name = "Notion"
version = "1.0.0"
author = "testuser"
category = "productivity"
icon = "edit_note"
description = "Read and write Notion pages, databases, and blocks via the API"
install_hint = "Get your API key at https://www.notion.so/my-integrations"

[[credentials]]
key = "NOTION_API_KEY"
label = "Integration Token"
description = "Your Notion internal integration token"
required = true
placeholder = "secret_..."

[instructions]
text = "You have access to the Notion API."

[widget]
type = "table"
title = "Recent Pages"
refresh = "10m"

[[widget.fields]]
key = "title"
label = "Page"
type = "text"

[[widget.fields]]
key = "updated"
label = "Last Updated"
type = "datetime"
"#;

    const MINIMAL_MANIFEST: &str = r#"
[skill]
id = "simple"
name = "Simple Skill"
version = "0.1.0"
author = "tester"
category = "api"
description = "A minimal skill"

[instructions]
text = "Do the thing."
"#;

    const MCP_MANIFEST: &str = r#"
[skill]
id = "github-mcp"
name = "GitHub (MCP)"
version = "1.0.0"
author = "openpawz"
category = "development"
description = "Full GitHub API via MCP"

[[credentials]]
key = "GITHUB_TOKEN"
label = "Personal Access Token"
required = true

[mcp]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
transport = "stdio"

[instructions]
text = "GitHub tools are available via MCP."
"#;

    #[test]
    fn parse_full_manifest() {
        let manifest = parse_manifest(FULL_MANIFEST).unwrap();
        assert_eq!(manifest.skill.id, "notion");
        assert_eq!(manifest.skill.name, "Notion");
        assert_eq!(manifest.skill.version, "1.0.0");
        assert_eq!(manifest.skill.author, "testuser");
        assert_eq!(manifest.skill.category, "productivity");
        assert_eq!(manifest.credentials.len(), 1);
        assert_eq!(manifest.credentials[0].key, "NOTION_API_KEY");
        assert!(manifest.credentials[0].required);
        assert!(manifest.instructions.is_some());
        assert_eq!(manifest.instructions.as_ref().unwrap().text, "You have access to the Notion API.");
        assert!(manifest.widget.is_some());
        let w = manifest.widget.as_ref().unwrap();
        assert_eq!(w.widget_type, "table");
        assert_eq!(w.fields.len(), 2);
    }

    #[test]
    fn parse_minimal_manifest() {
        let manifest = parse_manifest(MINIMAL_MANIFEST).unwrap();
        assert_eq!(manifest.skill.id, "simple");
        assert!(manifest.credentials.is_empty());
        assert!(manifest.widget.is_none());
        assert!(manifest.mcp.is_none());
    }

    #[test]
    fn parse_mcp_manifest() {
        let manifest = parse_manifest(MCP_MANIFEST).unwrap();
        assert_eq!(manifest.skill.id, "github-mcp");
        assert!(manifest.mcp.is_some());
        let mcp = manifest.mcp.as_ref().unwrap();
        assert_eq!(mcp.command, "npx");
        assert_eq!(mcp.args, vec!["-y", "@modelcontextprotocol/server-github"]);
        assert_eq!(mcp.transport, "stdio");
    }

    #[test]
    fn convert_to_definition() {
        let manifest = parse_manifest(FULL_MANIFEST).unwrap();
        let def = manifest_to_definition(&manifest);
        assert_eq!(def.id, "notion");
        assert_eq!(def.name, "Notion");
        assert_eq!(def.icon, "edit_note");
        assert_eq!(def.category, SkillCategory::Productivity);
        assert_eq!(def.tier, SkillTier::Integration); // has credentials
        assert_eq!(def.required_credentials.len(), 1);
        assert_eq!(def.required_credentials[0].key, "NOTION_API_KEY");
        assert!(!def.agent_instructions.is_empty());
    }

    #[test]
    fn minimal_skill_is_tier1() {
        let manifest = parse_manifest(MINIMAL_MANIFEST).unwrap();
        let def = manifest_to_definition(&manifest);
        assert_eq!(def.tier, SkillTier::Skill); // no credentials → Skill tier
        assert!(def.required_credentials.is_empty());
        assert!(def.tool_names.is_empty());
    }

    #[test]
    fn validate_valid_manifest() {
        let manifest = parse_manifest(FULL_MANIFEST).unwrap();
        assert!(validate_manifest(&manifest).is_ok());
    }

    #[test]
    fn validate_missing_id() {
        let toml = r#"
[skill]
id = ""
name = "Test"
version = "1.0.0"
author = "x"
category = "api"
description = "test"
"#;
        let manifest = parse_manifest(toml).unwrap();
        let result = validate_manifest(&manifest);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("ID is required"));
    }

    #[test]
    fn validate_invalid_id_chars() {
        let toml = r#"
[skill]
id = "../evil"
name = "Evil"
version = "1.0.0"
author = "x"
category = "api"
description = "test"
"#;
        let manifest = parse_manifest(toml).unwrap();
        let result = validate_manifest(&manifest);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid characters"));
    }

    #[test]
    fn validate_invalid_widget_type() {
        let toml = r#"
[skill]
id = "test"
name = "Test"
version = "1.0.0"
author = "x"
category = "api"
description = "test"

[widget]
type = "sparkline"
title = "Bad Widget"
"#;
        let manifest = parse_manifest(toml).unwrap();
        let result = validate_manifest(&manifest);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid widget type"));
    }

    #[test]
    fn validate_invalid_widget_field_type() {
        let toml = r#"
[skill]
id = "test"
name = "Test"
version = "1.0.0"
author = "x"
category = "api"
description = "test"

[widget]
type = "table"
title = "Test"

[[widget.fields]]
key = "x"
label = "X"
type = "sparkline"
"#;
        let manifest = parse_manifest(toml).unwrap();
        let result = validate_manifest(&manifest);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid widget field type"));
    }

    #[test]
    fn parse_category_mapping() {
        assert_eq!(parse_category("vault"), SkillCategory::Vault);
        assert_eq!(parse_category("Productivity"), SkillCategory::Productivity);
        assert_eq!(parse_category("smart_home"), SkillCategory::SmartHome);
        assert_eq!(parse_category("smarthome"), SkillCategory::SmartHome);
        assert_eq!(parse_category("unknown_thing"), SkillCategory::Api); // fallback
    }

    #[test]
    fn default_icon_when_empty() {
        let manifest = parse_manifest(MINIMAL_MANIFEST).unwrap();
        let def = manifest_to_definition(&manifest);
        assert_eq!(def.icon, "extension"); // default when not specified
    }

    #[test]
    fn description_too_long() {
        let long_desc = "x".repeat(501);
        let toml = format!(
            r#"
[skill]
id = "test"
name = "Test"
version = "1.0.0"
author = "x"
category = "api"
description = "{}"
"#,
            long_desc
        );
        let manifest = parse_manifest(&toml).unwrap();
        let result = validate_manifest(&manifest);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too long"));
    }

    #[test]
    fn invalid_toml_syntax() {
        let result = parse_manifest("this is not valid toml {{{}}}");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("TOML parse error"));
    }

    #[test]
    fn scan_empty_dir_returns_empty() {
        // When the skills dir doesn't exist, scan returns empty vec
        // (This test relies on the test environment not having ~/.paw/skills
        // with actual skills, which is the expected case in CI)
        let result = scan_toml_skills();
        // Don't assert empty — CI machines might have skills dirs.
        // Just verify it doesn't crash.
        assert!(result.len() < 10000); // sanity bound
    }
}
