// ─────────────────────────────────────────────────────────────────────────────
// Constrained Decoding — Molecules
//
// Side-effectful functions that apply constrained decoding configuration
// to request bodies. These transform the JSON body that gets sent to each
// provider's API endpoint.
// ─────────────────────────────────────────────────────────────────────────────

use super::atoms::{
    enforce_additional_properties_false, enforce_required_from_properties, ConstraintConfig,
};
use serde_json::Value;

// ── OpenAI Schema Normalization ─────────────────────────────────────────────

/// Normalize tool schemas for OpenAI-compatible APIs.
///
/// OpenAI (including Azure) rejects tool schemas where:
/// 1. Properties exist but aren't all listed in `required`
/// 2. `additionalProperties` is anything other than `false` (e.g. `true`, `{}`)
///
/// This must run for ALL OpenAI-compatible providers, regardless of strict mode.
pub fn normalize_tool_required(tools: &mut [Value]) {
    for tool in tools.iter_mut() {
        if let Some(func) = tool.get_mut("function") {
            if let Some(params) = func.get_mut("parameters") {
                enforce_required_from_properties(params);
                enforce_additional_properties_false(params);
            }
        }
    }
}

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
            // Skip strict mode for MCP tools — external schemas use JSON Schema
            // features (anyOf, $schema, exclusiveMinimum, default, etc.) that
            // OpenAI strict mode does not support.
            let is_mcp = func
                .get("name")
                .and_then(|n| n.as_str())
                .map(|n| n.starts_with("mcp_"))
                .unwrap_or(false);
            if is_mcp {
                continue;
            }

            // Set strict: true on the function object
            func["strict"] = Value::Bool(true);

            // Ensure additionalProperties: false on parameters schema
            if config.add_additional_properties_false {
                if let Some(params) = func.get_mut("parameters") {
                    enforce_additional_properties_false(params);
                    enforce_required_from_properties(params);
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

    #[test]
    fn test_normalize_tool_required_fills_missing() {
        let mut tools = vec![json!({
            "type": "function",
            "function": {
                "name": "list_directory",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"},
                        "recursive": {"type": "boolean"},
                        "max_depth": {"type": "integer"}
                    }
                }
            }
        })];

        normalize_tool_required(&mut tools);

        let req = tools[0]["function"]["parameters"]["required"]
            .as_array()
            .unwrap();
        assert_eq!(req.len(), 3);
        assert!(req.contains(&json!("path")));
        assert!(req.contains(&json!("recursive")));
        assert!(req.contains(&json!("max_depth")));
    }

    #[test]
    fn test_normalize_tool_required_completes_partial() {
        let mut tools = vec![json!({
            "type": "function",
            "function": {
                "name": "exec",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": {"type": "string"},
                        "timeout": {"type": "integer"}
                    },
                    "required": ["command"]
                }
            }
        })];

        normalize_tool_required(&mut tools);

        let req = tools[0]["function"]["parameters"]["required"]
            .as_array()
            .unwrap();
        assert_eq!(req.len(), 2);
        assert!(req.contains(&json!("command")));
        assert!(req.contains(&json!("timeout")));
    }

    #[test]
    fn test_normalize_tool_required_empty_properties() {
        let mut tools = vec![json!({
            "type": "function",
            "function": {
                "name": "no_args",
                "parameters": {
                    "type": "object",
                    "properties": {}
                }
            }
        })];

        normalize_tool_required(&mut tools);

        // No required field should be added for empty properties
        assert!(tools[0]["function"]["parameters"].get("required").is_none());
        // additionalProperties: false is added (object with properties key)
        assert_eq!(
            tools[0]["function"]["parameters"]["additionalProperties"],
            json!(false)
        );
    }

    #[test]
    fn test_normalize_mcp_nested_anyof_without_type() {
        // Simulates n8n MCP schema: deeply nested anyOf with objects
        // missing "type": "object" and additionalProperties as sub-schemas
        let mut tools = vec![json!({
            "type": "function",
            "function": {
                "name": "mcp_n8n_execute_workflow",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "workflowId": {"type": "string"},
                        "inputs": {
                            "anyOf": [
                                {"type": "string"},
                                {
                                    "properties": {
                                        "formData": {
                                            "additionalProperties": {
                                                "properties": {
                                                    "key": {"type": "string"}
                                                }
                                            }
                                        }
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        })];

        normalize_tool_required(&mut tools);

        let params = &tools[0]["function"]["parameters"];
        // Top-level required
        let req = params["required"].as_array().unwrap();
        assert!(req.contains(&json!("workflowId")));
        assert!(req.contains(&json!("inputs")));

        // anyOf[1] should now have type: "object", required, and additionalProperties: false
        let any1 = &params["properties"]["inputs"]["anyOf"][1];
        assert_eq!(any1["type"], "object");
        assert_eq!(any1["additionalProperties"], false);
        let any1_req = any1["required"].as_array().unwrap();
        assert!(any1_req.contains(&json!("formData")));

        // formData's additionalProperties should be forced to false
        let form = &any1["properties"]["formData"];
        assert_eq!(form["additionalProperties"], false);
    }

    #[test]
    fn test_normalize_strips_stale_required_without_properties() {
        // MCP schemas may have required: ["formData"] at a level that has
        // no "properties" field (e.g. anyOf variant with additionalProperties)
        let mut tools = vec![json!({
            "type": "function",
            "function": {
                "name": "mcp_n8n_execute_workflow",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "workflowId": {"type": "string"},
                        "inputs": {
                            "anyOf": [
                                {"type": "string"},
                                {
                                    "type": "object",
                                    "required": ["formData"],
                                    "additionalProperties": {
                                        "type": "string"
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        })];

        normalize_tool_required(&mut tools);

        let any1 = &tools[0]["function"]["parameters"]["properties"]["inputs"]["anyOf"][1];
        // Stale required should be removed since anyOf[1] has no properties
        assert!(any1.get("required").is_none());
        // additionalProperties forced to false
        assert_eq!(any1["additionalProperties"], false);
    }
}
