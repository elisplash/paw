// ─────────────────────────────────────────────────────────────────────────────
// Constrained Decoding — Molecules
//
// Side-effectful functions that apply constrained decoding configuration
// to request bodies. These transform the JSON body that gets sent to each
// provider's API endpoint.
// ─────────────────────────────────────────────────────────────────────────────

use super::atoms::{enforce_additional_properties_false, ConstraintConfig};
use serde_json::Value;

// ── OpenAI Strict Mode ─────────────────────────────────────────────────────

/// Apply OpenAI `strict: true` to formatted tool definitions.
///
/// OpenAI Structured Outputs require:
/// 1. `strict: true` on each function definition
/// 2. `additionalProperties: false` on all object schemas
///
/// This mutates the tools array in-place. Only call when
/// `config.strict_tools` is true.
pub fn apply_openai_strict(tools: &mut [Value], config: &ConstraintConfig) {
    if !config.strict_tools {
        return;
    }

    for tool in tools.iter_mut() {
        if let Some(func) = tool.get_mut("function") {
            // Set strict: true on the function object
            func["strict"] = Value::Bool(true);

            // Ensure additionalProperties: false on parameters schema
            if config.add_additional_properties_false {
                if let Some(params) = func.get_mut("parameters") {
                    enforce_additional_properties_false(params);
                }
            }
        }
    }
}

// ── Ollama JSON Format ──────────────────────────────────────────────────────

/// Apply Ollama's `format: "json"` to the request body.
///
/// This tells Ollama to constrain the model's output to valid JSON,
/// which prevents malformed tool call arguments. Only call when
/// `config.json_format` is true.
pub fn apply_ollama_json_format(body: &mut Value, config: &ConstraintConfig) {
    if !config.json_format {
        return;
    }

    body["format"] = Value::String("json".to_string());
}

// ── Anthropic Tool Choice ───────────────────────────────────────────────────

/// Apply explicit `tool_choice` to Anthropic request body.
///
/// `tool_choice: { type: "auto" }` makes Claude automatically decide
/// whether to call a tool or produce text, which is the default behavior
/// but being explicit prevents any future API default changes.
///
/// Only call when `config.explicit_tool_choice` is true AND tools are present.
pub fn apply_anthropic_tool_choice(body: &mut Value, config: &ConstraintConfig) {
    if !config.explicit_tool_choice {
        return;
    }

    body["tool_choice"] = serde_json::json!({"type": "auto"});
}

// ── Google Function Calling Config ──────────────────────────────────────────

/// Apply Google Gemini `tool_config` to the request body.
///
/// `function_calling_config: { mode: "AUTO" }` tells Gemini to automatically
/// decide between calling functions and generating text. This is the recommended
/// configuration and enables structured function call output.
///
/// Only call when `config.explicit_tool_choice` is true AND tools are present.
pub fn apply_google_tool_config(body: &mut Value, config: &ConstraintConfig) {
    if !config.explicit_tool_choice {
        return;
    }

    body["tool_config"] = serde_json::json!({
        "function_calling_config": {
            "mode": "AUTO"
        }
    });
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::constrained::atoms::detect_constraints;
    use crate::engine::types::ProviderKind;
    use serde_json::json;

    #[test]
    fn test_apply_openai_strict_adds_strict_and_additional_properties() {
        let config = detect_constraints(ProviderKind::OpenAI, "gpt-4o");
        let mut tools = vec![json!({
            "type": "function",
            "function": {
                "name": "test_tool",
                "description": "A test",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {"type": "string"},
                        "nested": {
                            "type": "object",
                            "properties": {
                                "x": {"type": "number"}
                            }
                        }
                    },
                    "required": ["query"]
                }
            }
        })];

        apply_openai_strict(&mut tools, &config);

        // strict: true added
        assert_eq!(tools[0]["function"]["strict"], json!(true));
        // additionalProperties: false on top-level parameters
        assert_eq!(
            tools[0]["function"]["parameters"]["additionalProperties"],
            json!(false)
        );
        // additionalProperties: false on nested object
        assert_eq!(
            tools[0]["function"]["parameters"]["properties"]["nested"]["additionalProperties"],
            json!(false)
        );
    }

    #[test]
    fn test_apply_openai_strict_noop_for_legacy() {
        let config = detect_constraints(ProviderKind::OpenAI, "gpt-3.5-turbo");
        let mut tools = vec![json!({
            "type": "function",
            "function": {
                "name": "test_tool",
                "description": "A test",
                "parameters": {"type": "object", "properties": {}}
            }
        })];

        let original = tools.clone();
        apply_openai_strict(&mut tools, &config);

        // Should not modify anything for legacy models
        assert_eq!(tools, original);
    }

    #[test]
    fn test_apply_ollama_json_format() {
        let config = detect_constraints(ProviderKind::Ollama, "llama3.2:latest");
        let mut body = json!({"model": "llama3.2", "messages": []});

        apply_ollama_json_format(&mut body, &config);

        assert_eq!(body["format"], json!("json"));
    }

    #[test]
    fn test_apply_anthropic_tool_choice() {
        let config = detect_constraints(ProviderKind::Anthropic, "claude-opus-4-6");
        let mut body = json!({"model": "claude-opus-4-6", "messages": []});

        apply_anthropic_tool_choice(&mut body, &config);

        assert_eq!(body["tool_choice"], json!({"type": "auto"}));
    }

    #[test]
    fn test_apply_google_tool_config() {
        let config = detect_constraints(ProviderKind::Google, "gemini-2.5-flash");
        let mut body = json!({"contents": []});

        apply_google_tool_config(&mut body, &config);

        assert_eq!(
            body["tool_config"],
            json!({"function_calling_config": {"mode": "AUTO"}})
        );
    }

    #[test]
    fn test_no_explicit_choice_for_deepseek() {
        let config = detect_constraints(ProviderKind::DeepSeek, "deepseek-chat");
        let mut body = json!({"model": "deepseek-chat", "messages": []});

        apply_anthropic_tool_choice(&mut body, &config);
        apply_google_tool_config(&mut body, &config);

        // No tool_choice or tool_config should be added
        assert!(body.get("tool_choice").is_none());
        assert!(body.get("tool_config").is_none());
    }
}
