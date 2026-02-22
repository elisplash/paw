// TOML Manifest — Parsing, validation, and conversion
//
// Pure functions: no filesystem I/O, no state. Takes strings/structs in,
// returns structs/results out.

use super::types::*;
use crate::engine::skills::types::{CredentialField, SkillCategory, SkillDefinition, SkillTier};

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
/// - Has `[view]` → Extension
/// - Has credentials → Integration
/// - Otherwise → Skill (prompt-only)
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

/// Validate a manifest's required fields. Returns `Ok(())` or an error message.
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
        let toml_str = r#"
[skill]
id = ""
name = "Test"
version = "1.0.0"
author = "x"
category = "api"
description = "test"
"#;
        let manifest = parse_manifest(toml_str).unwrap();
        let result = validate_manifest(&manifest);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("ID is required"));
    }

    #[test]
    fn validate_invalid_id_chars() {
        let toml_str = r#"
[skill]
id = "../evil"
name = "Evil"
version = "1.0.0"
author = "x"
category = "api"
description = "test"
"#;
        let manifest = parse_manifest(toml_str).unwrap();
        let result = validate_manifest(&manifest);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid characters"));
    }

    #[test]
    fn validate_invalid_widget_type() {
        let toml_str = r#"
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
        let manifest = parse_manifest(toml_str).unwrap();
        let result = validate_manifest(&manifest);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid widget type"));
    }

    #[test]
    fn validate_invalid_widget_field_type() {
        let toml_str = r#"
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
        let manifest = parse_manifest(toml_str).unwrap();
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
        let toml_str = format!(
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
        let manifest = parse_manifest(&toml_str).unwrap();
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
}
