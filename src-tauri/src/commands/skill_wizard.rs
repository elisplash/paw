// commands/skill_wizard.rs — TOML skill creation wizard (Phase F.5).
// Generates `pawz-skill.toml` content from structured form data.

use serde::{Deserialize, Serialize};

// ── Wizard input types ─────────────────────────────────────────────────

/// A single credential field for the wizard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WizardCredential {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub placeholder: String,
}

/// A single widget field for the wizard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WizardWidgetField {
    pub key: String,
    pub label: String,
    /// One of: text, number, badge, datetime, percentage, currency
    pub field_type: String,
}

/// Widget configuration for the wizard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WizardWidget {
    /// One of: status, metric, table, log, kv
    pub widget_type: String,
    pub title: String,
    #[serde(default)]
    pub refresh: String,
    #[serde(default)]
    pub fields: Vec<WizardWidgetField>,
}

/// MCP server configuration for the wizard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WizardMcp {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub transport: String,
    #[serde(default)]
    pub url: String,
}

/// Full wizard form data submitted from the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WizardFormData {
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
    pub instructions: String,
    #[serde(default)]
    pub credentials: Vec<WizardCredential>,
    pub widget: Option<WizardWidget>,
    pub mcp: Option<WizardMcp>,
}

// ── TOML generation ────────────────────────────────────────────────────

/// Escape a string for TOML (double-quoted).
fn toml_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Generate `pawz-skill.toml` content from wizard form data.
fn generate_toml(data: &WizardFormData) -> Result<String, String> {
    // Validate required fields
    if data.id.is_empty() {
        return Err("Skill ID is required".into());
    }
    if !data
        .id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("Skill ID must contain only letters, numbers, hyphens, and underscores".into());
    }
    if data.name.is_empty() {
        return Err("Skill name is required".into());
    }
    if data.version.is_empty() {
        return Err("Version is required".into());
    }
    if data.author.is_empty() {
        return Err("Author is required".into());
    }
    if data.description.is_empty() {
        return Err("Description is required".into());
    }
    if data.description.len() > 500 {
        return Err("Description must be 500 characters or less".into());
    }

    let mut out = String::with_capacity(1024);

    // [skill] section
    out.push_str("[skill]\n");
    out.push_str(&format!("id = \"{}\"\n", toml_escape(&data.id)));
    out.push_str(&format!("name = \"{}\"\n", toml_escape(&data.name)));
    out.push_str(&format!("version = \"{}\"\n", toml_escape(&data.version)));
    out.push_str(&format!("author = \"{}\"\n", toml_escape(&data.author)));
    out.push_str(&format!("category = \"{}\"\n", toml_escape(&data.category)));
    if !data.icon.is_empty() {
        out.push_str(&format!("icon = \"{}\"\n", toml_escape(&data.icon)));
    }
    out.push_str(&format!(
        "description = \"{}\"\n",
        toml_escape(&data.description)
    ));
    if !data.install_hint.is_empty() {
        out.push_str(&format!(
            "install_hint = \"{}\"\n",
            toml_escape(&data.install_hint)
        ));
    }

    // [[credentials]] sections
    for cred in &data.credentials {
        if cred.key.is_empty() || cred.label.is_empty() {
            continue;
        }
        out.push_str("\n[[credentials]]\n");
        out.push_str(&format!("key = \"{}\"\n", toml_escape(&cred.key)));
        out.push_str(&format!("label = \"{}\"\n", toml_escape(&cred.label)));
        if !cred.description.is_empty() {
            out.push_str(&format!(
                "description = \"{}\"\n",
                toml_escape(&cred.description)
            ));
        }
        if cred.required {
            out.push_str("required = true\n");
        }
        if !cred.placeholder.is_empty() {
            out.push_str(&format!(
                "placeholder = \"{}\"\n",
                toml_escape(&cred.placeholder)
            ));
        }
    }

    // [instructions] section
    if !data.instructions.is_empty() {
        out.push_str("\n[instructions]\n");
        out.push_str(&format!(
            "text = \"{}\"\n",
            toml_escape(&data.instructions)
        ));
    }

    // [widget] section
    if let Some(w) = &data.widget {
        let valid_types = ["status", "metric", "table", "log", "kv"];
        if !valid_types.contains(&w.widget_type.as_str()) {
            return Err(format!(
                "Invalid widget type '{}'. Must be one of: {}",
                w.widget_type,
                valid_types.join(", ")
            ));
        }
        out.push_str("\n[widget]\n");
        out.push_str(&format!("type = \"{}\"\n", toml_escape(&w.widget_type)));
        out.push_str(&format!("title = \"{}\"\n", toml_escape(&w.title)));
        if !w.refresh.is_empty() {
            out.push_str(&format!("refresh = \"{}\"\n", toml_escape(&w.refresh)));
        }
        let valid_field_types = [
            "text",
            "number",
            "badge",
            "datetime",
            "percentage",
            "currency",
        ];
        for f in &w.fields {
            if f.key.is_empty() || f.label.is_empty() {
                continue;
            }
            if !valid_field_types.contains(&f.field_type.as_str()) {
                return Err(format!(
                    "Invalid widget field type '{}'. Must be one of: {}",
                    f.field_type,
                    valid_field_types.join(", ")
                ));
            }
            out.push_str("\n[[widget.fields]]\n");
            out.push_str(&format!("key = \"{}\"\n", toml_escape(&f.key)));
            out.push_str(&format!("label = \"{}\"\n", toml_escape(&f.label)));
            out.push_str(&format!("type = \"{}\"\n", toml_escape(&f.field_type)));
        }
    }

    // [mcp] section
    if let Some(m) = &data.mcp {
        out.push_str("\n[mcp]\n");
        if !m.command.is_empty() {
            out.push_str(&format!("command = \"{}\"\n", toml_escape(&m.command)));
        }
        if !m.args.is_empty() {
            let args_str: Vec<String> =
                m.args.iter().map(|a| format!("\"{}\"", toml_escape(a))).collect();
            out.push_str(&format!("args = [{}]\n", args_str.join(", ")));
        }
        if !m.transport.is_empty() && m.transport != "stdio" {
            out.push_str(&format!("transport = \"{}\"\n", toml_escape(&m.transport)));
        }
        if !m.url.is_empty() {
            out.push_str(&format!("url = \"{}\"\n", toml_escape(&m.url)));
        }
    }

    Ok(out)
}

// ── Tauri commands ─────────────────────────────────────────────────────

/// Generate a `pawz-skill.toml` from wizard form data.
///
/// Returns the TOML content string for preview before install.
#[tauri::command]
pub fn engine_wizard_generate_toml(form: WizardFormData) -> Result<String, String> {
    generate_toml(&form)
}

/// URL-encode a string for use in query parameters.
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

/// Build a GitHub new-file URL pre-filled with skill TOML for PawzHub PR.
#[tauri::command]
pub fn engine_wizard_publish_url(
    skill_id: String,
    toml_content: String,
) -> Result<String, String> {
    if skill_id.is_empty() {
        return Err("Skill ID is required".into());
    }
    let path = format!("skills/{}/pawz-skill.toml", skill_id);
    let url = format!(
        "https://github.com/elisplash/pawzhub/new/main/?filename={}&value={}",
        url_encode(&path),
        url_encode(&toml_content)
    );
    Ok(url)
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_form() -> WizardFormData {
        WizardFormData {
            id: "test-skill".into(),
            name: "Test Skill".into(),
            version: "1.0.0".into(),
            author: "testuser".into(),
            category: "api".into(),
            icon: String::new(),
            description: "A test skill".into(),
            install_hint: String::new(),
            instructions: String::new(),
            credentials: vec![],
            widget: None,
            mcp: None,
        }
    }

    #[test]
    fn test_generate_minimal() {
        let form = minimal_form();
        let toml = generate_toml(&form).unwrap();
        assert!(toml.contains("id = \"test-skill\""));
        assert!(toml.contains("name = \"Test Skill\""));
        assert!(toml.contains("version = \"1.0.0\""));
        assert!(toml.contains("category = \"api\""));
        assert!(!toml.contains("[instructions]"));
        assert!(!toml.contains("[widget]"));
        assert!(!toml.contains("[mcp]"));
    }

    #[test]
    fn test_generate_full() {
        let mut form = minimal_form();
        form.instructions = "You have access to the API.".into();
        form.credentials = vec![WizardCredential {
            key: "API_KEY".into(),
            label: "API Key".into(),
            description: "Your API key".into(),
            required: true,
            placeholder: "sk-...".into(),
        }];
        form.widget = Some(WizardWidget {
            widget_type: "status".into(),
            title: "Status".into(),
            refresh: "5m".into(),
            fields: vec![WizardWidgetField {
                key: "status".into(),
                label: "Current Status".into(),
                field_type: "badge".into(),
            }],
        });
        form.mcp = Some(WizardMcp {
            command: "npx".into(),
            args: vec!["-y".into(), "@test/server".into()],
            transport: "stdio".into(),
            url: String::new(),
        });

        let toml = generate_toml(&form).unwrap();
        assert!(toml.contains("[[credentials]]"));
        assert!(toml.contains("key = \"API_KEY\""));
        assert!(toml.contains("required = true"));
        assert!(toml.contains("[instructions]"));
        assert!(toml.contains("[widget]"));
        assert!(toml.contains("type = \"status\""));
        assert!(toml.contains("[[widget.fields]]"));
        assert!(toml.contains("[mcp]"));
        assert!(toml.contains("command = \"npx\""));
    }

    #[test]
    fn test_validate_empty_id() {
        let mut form = minimal_form();
        form.id = String::new();
        assert!(generate_toml(&form).is_err());
    }

    #[test]
    fn test_validate_bad_id_chars() {
        let mut form = minimal_form();
        form.id = "bad skill!".into();
        assert!(generate_toml(&form).is_err());
    }

    #[test]
    fn test_validate_long_description() {
        let mut form = minimal_form();
        form.description = "x".repeat(501);
        assert!(generate_toml(&form).is_err());
    }

    #[test]
    fn test_validate_bad_widget_type() {
        let mut form = minimal_form();
        form.widget = Some(WizardWidget {
            widget_type: "invalid".into(),
            title: "T".into(),
            refresh: String::new(),
            fields: vec![],
        });
        assert!(generate_toml(&form).is_err());
    }

    #[test]
    fn test_toml_escape() {
        let form = WizardFormData {
            id: "esc-test".into(),
            name: "Has \"quotes\"".into(),
            version: "1.0.0".into(),
            author: "test".into(),
            category: "api".into(),
            icon: String::new(),
            description: "Line1\\nLine2".into(),
            install_hint: String::new(),
            instructions: String::new(),
            credentials: vec![],
            widget: None,
            mcp: None,
        };
        let toml = generate_toml(&form).unwrap();
        assert!(toml.contains("Has \\\"quotes\\\""));
    }

    #[test]
    fn test_roundtrip_parse() {
        let mut form = minimal_form();
        form.instructions = "Do stuff.".into();
        form.credentials = vec![WizardCredential {
            key: "TOKEN".into(),
            label: "Token".into(),
            description: String::new(),
            required: true,
            placeholder: String::new(),
        }];
        let toml_str = generate_toml(&form).unwrap();
        // Parse with the actual TOML crate to verify validity
        let parsed: toml::Value = toml::from_str(&toml_str).expect("generated TOML should parse");
        let skill = parsed.get("skill").expect("should have [skill]");
        assert_eq!(skill.get("id").unwrap().as_str().unwrap(), "test-skill");

        let creds = parsed.get("credentials").expect("should have [[credentials]]");
        assert!(creds.as_array().unwrap().len() == 1);
    }
}
