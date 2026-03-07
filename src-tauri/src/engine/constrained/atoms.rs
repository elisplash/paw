// ─────────────────────────────────────────────────────────────────────────────
// Constrained Decoding — Atoms
//
// Pure types and capability detection for constrained decoding support
// across AI providers. No I/O, no side effects.
//
// Constrained decoding forces the model to only produce valid JSON that
// conforms to the tool's schema, eliminating parse failures entirely.
// Each provider exposes this differently:
//   - OpenAI: `strict: true` on function definitions (Structured Outputs)
//   - Anthropic: function calling already structured; tool_choice modes
//   - Google Gemini: function_calling_config with mode AUTO/ANY/NONE
//   - Ollama: `format: "json"` on the request body
// ─────────────────────────────────────────────────────────────────────────────

use crate::engine::types::ProviderKind;
use serde::{Deserialize, Serialize};

// ── Types ──────────────────────────────────────────────────────────────────

/// Level of constrained decoding support for a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintLevel {
    /// Full schema enforcement — model literally cannot produce invalid JSON.
    /// OpenAI Structured Outputs, Ollama format:"json".
    Full,

    /// Structured tool calling — provider natively handles function calls
    /// with structured input/output, but no grammar-level enforcement.
    /// Anthropic (tool_use blocks), Google (functionCall parts).
    Structured,

    /// No constrained decoding — parse and retry on failure.
    None,
}

/// Provider-specific constrained decoding configuration.
#[derive(Debug, Clone)]
pub struct ConstraintConfig {
    /// What level of control we have.
    pub level: ConstraintLevel,

    /// Whether `strict: true` should be added to tool function definitions.
    /// Only OpenAI and compatible endpoints support this.
    pub strict_tools: bool,

    /// Whether the request body should include `format: "json"`.
    /// Only Ollama uses this.
    pub json_format: bool,

    /// Whether to add explicit `tool_choice` configuration.
    /// Anthropic and Google benefit from this.
    pub explicit_tool_choice: bool,

    /// Whether schemas need `additionalProperties: false` added for strict mode.
    /// OpenAI Structured Outputs requires this.
    pub add_additional_properties_false: bool,
}

// ── Pure Functions ─────────────────────────────────────────────────────────

/// Determine constrained decoding capabilities for a provider/model pair.
///
/// This is a pure function — no I/O, no network calls. It uses the
/// provider kind and model name to determine what constraints are available.
pub fn detect_constraints(provider: ProviderKind, model: &str) -> ConstraintConfig {
    match provider {
        // ── OpenAI and compatible ──────────────────────────────────────
        // OpenAI Structured Outputs: strict: true on function definitions.
        // Supported by GPT-4o, GPT-4o-mini, o1, o3, o4-mini, and newer.
        // NOT supported by: gpt-3.5-turbo, older gpt-4 variants.
        ProviderKind::OpenAI => {
            let supports_strict = supports_openai_strict(model);
            ConstraintConfig {
                level: if supports_strict {
                    ConstraintLevel::Full
                } else {
                    ConstraintLevel::Structured
                },
                strict_tools: supports_strict,
                json_format: false,
                explicit_tool_choice: false,
                add_additional_properties_false: supports_strict,
            }
        }

        // ── Ollama (local models) ──────────────────────────────────────
        // Ollama supports `format: "json"` for all models, which constrains
        // the output to valid JSON. Combined with function calling support,
        // this provides strong guarantees.
        ProviderKind::Ollama => ConstraintConfig {
            level: ConstraintLevel::Full,
            strict_tools: false, // Ollama doesn't support strict on function defs
            json_format: true,   // Ollama-specific: add `format: "json"` to body
            explicit_tool_choice: false,
            add_additional_properties_false: false,
        },

        // ── Anthropic ────────────────────────────────────────────────
        // Claude uses structured tool_use content blocks (not freeform JSON).
        // tool_choice can be set to auto/any/tool to control behavior.
        // No grammar-level enforcement, but tool calls are already structured.
        ProviderKind::Anthropic => ConstraintConfig {
            level: ConstraintLevel::Structured,
            strict_tools: false,
            json_format: false,
            explicit_tool_choice: true,
            add_additional_properties_false: false,
        },

        // ── Google Gemini ──────────────────────────────────────────────
        // Gemini uses functionDeclarations in tools. Supports
        // function_calling_config to control tool usage behavior.
        // No strict schema enforcement, but calls are structured.
        ProviderKind::Google => ConstraintConfig {
            level: ConstraintLevel::Structured,
            strict_tools: false,
            json_format: false,
            explicit_tool_choice: true,
            add_additional_properties_false: false,
        },

        // ── OpenRouter ─────────────────────────────────────────────────
        // OpenRouter proxies to many providers. strict: true is forwarded
        // to OpenAI models but may not work for all models behind the proxy.
        // Use structured level as safe default.
        ProviderKind::OpenRouter => {
            let is_openai_model = model.starts_with("openai/") || model.starts_with("gpt-");
            ConstraintConfig {
                level: if is_openai_model {
                    ConstraintLevel::Full
                } else {
                    ConstraintLevel::Structured
                },
                strict_tools: is_openai_model,
                json_format: false,
                explicit_tool_choice: false,
                add_additional_properties_false: is_openai_model,
            }
        }

        // ── DeepSeek ──────────────────────────────────────────────────
        // DeepSeek's API is OpenAI-compatible but doesn't support strict mode.
        ProviderKind::DeepSeek => ConstraintConfig {
            level: ConstraintLevel::Structured,
            strict_tools: false,
            json_format: false,
            explicit_tool_choice: false,
            add_additional_properties_false: false,
        },

        // ── Grok, Mistral, Moonshot ───────────────────────────────
        // OpenAI-compatible APIs that don't support strict mode.
        ProviderKind::Grok | ProviderKind::Mistral | ProviderKind::Moonshot => ConstraintConfig {
            level: ConstraintLevel::Structured,
            strict_tools: false,
            json_format: false,
            explicit_tool_choice: false,
            add_additional_properties_false: false,
        },

        // ── Azure AI Foundry ────────────────────────────────────────
        // Hosts OpenAI models (gpt-*, o1, o3, o4) alongside others.
        // OpenAI models on Azure enforce the same strict schema
        // validation as the native OpenAI API.
        ProviderKind::AzureFoundry => {
            let supports_strict = supports_openai_strict(model);
            ConstraintConfig {
                level: if supports_strict {
                    ConstraintLevel::Full
                } else {
                    ConstraintLevel::Structured
                },
                strict_tools: supports_strict,
                json_format: false,
                explicit_tool_choice: false,
                add_additional_properties_false: supports_strict,
            }
        }

        // ── Custom ────────────────────────────────────────────────────
        // Unknown provider — no constraints, rely on parse + retry.
        ProviderKind::Custom => ConstraintConfig {
            level: ConstraintLevel::None,
            strict_tools: false,
            json_format: false,
            explicit_tool_choice: false,
            add_additional_properties_false: false,
        },
    }
}

/// Check whether an OpenAI model supports `strict: true` Structured Outputs.
///
/// Structured Outputs are supported by:
/// - gpt-4o and variants (gpt-4o-mini, gpt-4o-2024-08-06+)
/// - o1, o3, o4-mini (reasoning models)
/// - gpt-4.1, gpt-4.1-mini, gpt-4.1-nano
/// - gpt-5, gpt-5.1-chat, etc.
///
/// NOT supported by:
/// - gpt-3.5-turbo (legacy)
/// - gpt-4 (original, non-o variants)
/// - gpt-4-turbo (early 2024 variants)
fn supports_openai_strict(model: &str) -> bool {
    let m = model.to_lowercase();

    // Reasoning models — always support strict
    if m.starts_with("o1") || m.starts_with("o3") || m.starts_with("o4") {
        return true;
    }

    // GPT-4o variants — support strict from 2024-08-06 onwards
    if m.starts_with("gpt-4o") {
        return true;
    }

    // GPT-4.1 variants — support strict
    if m.starts_with("gpt-4.1") {
        return true;
    }

    // GPT-5+ variants — support strict
    if m.starts_with("gpt-5") {
        return true;
    }

    // Explicit legacy models that do NOT support strict
    if m.starts_with("gpt-3.5") || m == "gpt-4" || m.starts_with("gpt-4-turbo") {
        return false;
    }

    // Default: assume newer models support it
    // This is a safe default — if the model doesn't support it,
    // the API will return an error and we'll fall back
    m.starts_with("gpt-")
}

/// Ensure a JSON schema has `additionalProperties: false` at every object level.
///
/// OpenAI (including Azure) rejects schemas with `additionalProperties: true`
/// or `additionalProperties: {}` (object without `type`). This recursively
/// forces `additionalProperties: false` on all object schemas — both adding
/// it when missing AND overwriting non-false values.
///
/// Handles MCP schemas that may omit `type: "object"` but still have `properties`.
pub fn enforce_additional_properties_false(schema: &mut serde_json::Value) {
    if let Some(obj) = schema.as_object_mut() {
        let is_object = obj.get("type").and_then(|v| v.as_str()) == Some("object")
            || obj.contains_key("properties");

        if is_object {
            // Force additionalProperties: false (overwrite true, {}, or sub-schemas)
            obj.insert(
                "additionalProperties".to_string(),
                serde_json::Value::Bool(false),
            );
            // Ensure type: "object" is present (OpenAI requires it)
            if !obj.contains_key("type") {
                obj.insert(
                    "type".to_string(),
                    serde_json::Value::String("object".into()),
                );
            }
        } else if let Some(ap) = obj.get("additionalProperties") {
            // additionalProperties exists without properties — if it's not false, force it
            if ap != &serde_json::Value::Bool(false) {
                obj.insert(
                    "additionalProperties".to_string(),
                    serde_json::Value::Bool(false),
                );
            }
        }

        // Recurse into properties
        if let Some(props) = obj.get_mut("properties") {
            if let Some(props_obj) = props.as_object_mut() {
                for (_, v) in props_obj.iter_mut() {
                    enforce_additional_properties_false(v);
                }
            }
        }

        // Recurse into items (array schemas)
        if let Some(items) = obj.get_mut("items") {
            enforce_additional_properties_false(items);
        }

        // Recurse into anyOf / oneOf / allOf
        for key in &["anyOf", "oneOf", "allOf"] {
            if let Some(arr) = obj.get_mut(*key) {
                if let Some(arr_items) = arr.as_array_mut() {
                    for item in arr_items.iter_mut() {
                        enforce_additional_properties_false(item);
                    }
                }
            }
        }
    }
}

/// Ensure every object schema has a `required` array listing all property keys.
///
/// OpenAI strict mode rejects schemas where properties exist but aren't
/// listed in `required`. This recursively patches all object schemas,
/// adding any missing property keys to the `required` array.
///
/// Handles MCP schemas that may omit `type: "object"` but still have `properties`.
pub fn enforce_required_from_properties(schema: &mut serde_json::Value) {
    if let Some(obj) = schema.as_object_mut() {
        // Act on any schema with properties (not just type: "object")
        if let Some(props) = obj.get("properties").and_then(|p| p.as_object()) {
            let all_keys: Vec<serde_json::Value> = props
                .keys()
                .map(|k| serde_json::Value::String(k.clone()))
                .collect();
            // OpenAI strict mode requires `required` to be present and list
            // ALL property keys — even as an empty array for empty properties.
            obj.insert("required".to_string(), serde_json::Value::Array(all_keys));
        } else {
            // No properties at all — remove any stale required array
            // (MCP schemas may have required at levels without properties)
            obj.remove("required");
        }

        // Recurse into properties
        if let Some(props) = obj.get_mut("properties") {
            if let Some(props_obj) = props.as_object_mut() {
                for (_, v) in props_obj.iter_mut() {
                    enforce_required_from_properties(v);
                }
            }
        }

        // Recurse into items (array schemas)
        if let Some(items) = obj.get_mut("items") {
            enforce_required_from_properties(items);
        }

        // Recurse into anyOf / oneOf / allOf
        for key in &["anyOf", "oneOf", "allOf"] {
            if let Some(arr) = obj.get_mut(*key) {
                if let Some(arr_items) = arr.as_array_mut() {
                    for item in arr_items.iter_mut() {
                        enforce_required_from_properties(item);
                    }
                }
            }
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_gpt4o_has_full_constraints() {
        let cfg = detect_constraints(ProviderKind::OpenAI, "gpt-4o");
        assert_eq!(cfg.level, ConstraintLevel::Full);
        assert!(cfg.strict_tools);
        assert!(cfg.add_additional_properties_false);
    }

    #[test]
    fn test_openai_o3_has_full_constraints() {
        let cfg = detect_constraints(ProviderKind::OpenAI, "o3-mini");
        assert_eq!(cfg.level, ConstraintLevel::Full);
        assert!(cfg.strict_tools);
    }

    #[test]
    fn test_openai_gpt4_1_has_full_constraints() {
        let cfg = detect_constraints(ProviderKind::OpenAI, "gpt-4.1-mini");
        assert_eq!(cfg.level, ConstraintLevel::Full);
        assert!(cfg.strict_tools);
    }

    #[test]
    fn test_openai_gpt35_no_strict() {
        let cfg = detect_constraints(ProviderKind::OpenAI, "gpt-3.5-turbo");
        assert_eq!(cfg.level, ConstraintLevel::Structured);
        assert!(!cfg.strict_tools);
    }

    #[test]
    fn test_anthropic_structured() {
        let cfg = detect_constraints(ProviderKind::Anthropic, "claude-opus-4-6");
        assert_eq!(cfg.level, ConstraintLevel::Structured);
        assert!(!cfg.strict_tools);
        assert!(cfg.explicit_tool_choice);
    }

    #[test]
    fn test_google_structured() {
        let cfg = detect_constraints(ProviderKind::Google, "gemini-2.5-flash");
        assert_eq!(cfg.level, ConstraintLevel::Structured);
        assert!(cfg.explicit_tool_choice);
    }

    #[test]
    fn test_ollama_full_json() {
        let cfg = detect_constraints(ProviderKind::Ollama, "llama3.2:latest");
        assert_eq!(cfg.level, ConstraintLevel::Full);
        assert!(cfg.json_format);
        assert!(!cfg.strict_tools);
    }

    #[test]
    fn test_openrouter_openai_model() {
        let cfg = detect_constraints(ProviderKind::OpenRouter, "openai/gpt-4o");
        assert_eq!(cfg.level, ConstraintLevel::Full);
        assert!(cfg.strict_tools);
    }

    #[test]
    fn test_openrouter_non_openai_model() {
        let cfg = detect_constraints(ProviderKind::OpenRouter, "anthropic/claude-3-opus");
        assert_eq!(cfg.level, ConstraintLevel::Structured);
        assert!(!cfg.strict_tools);
    }

    #[test]
    fn test_custom_no_constraints() {
        let cfg = detect_constraints(ProviderKind::Custom, "my-model");
        assert_eq!(cfg.level, ConstraintLevel::None);
    }

    #[test]
    fn test_deepseek_structured() {
        let cfg = detect_constraints(ProviderKind::DeepSeek, "deepseek-chat");
        assert_eq!(cfg.level, ConstraintLevel::Structured);
    }

    #[test]
    fn test_enforce_additional_properties() {
        let mut schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "address": {
                    "type": "object",
                    "properties": {
                        "city": {"type": "string"},
                        "zip": {"type": "string"}
                    }
                },
                "tags": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "key": {"type": "string"}
                        }
                    }
                }
            }
        });

        enforce_additional_properties_false(&mut schema);

        // Top-level object
        assert_eq!(schema["additionalProperties"], serde_json::json!(false));
        // Nested object in properties
        assert_eq!(
            schema["properties"]["address"]["additionalProperties"],
            serde_json::json!(false)
        );
        // Object inside array items
        assert_eq!(
            schema["properties"]["tags"]["items"]["additionalProperties"],
            serde_json::json!(false)
        );
    }

    #[test]
    fn test_enforce_overwrites_non_false() {
        let mut schema = serde_json::json!({
            "type": "object",
            "additionalProperties": true,
            "properties": {"x": {"type": "string"}}
        });

        enforce_additional_properties_false(&mut schema);

        // Should overwrite any non-false additionalProperties
        assert_eq!(schema["additionalProperties"], serde_json::json!(false));
    }
}
