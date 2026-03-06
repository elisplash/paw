// ─────────────────────────────────────────────────────────────────────────────
// Action DAG Planning — Molecules
//
// Stateful logic: parse plan from model output, validate against live tool
// registry, build execution strategy. These functions have side effects
// (they read from EngineState for tool validation).
// ─────────────────────────────────────────────────────────────────────────────

use super::atoms::*;
use crate::atoms::types::ToolDefinition;
use log::{info, warn};
use std::collections::HashSet;

/// Parse an execution plan from the model's tool call arguments.
///
/// The model calls `execute_plan` with a JSON object containing:
/// ```json
/// {
///   "description": "Set up weekly standup",
///   "nodes": [
///     { "id": "a", "tool": "google_calendar_create", "args": {...} },
///     { "id": "b", "tool": "gmail_send", "args": {...}, "depends_on": ["a"] }
///   ]
/// }
/// ```
///
/// Returns `Ok(ExecutionPlan)` on success, or `Err(message)` with a
/// human-readable error that the model can use to self-correct.
pub fn parse_plan(args: &serde_json::Value) -> Result<ExecutionPlan, String> {
    // Try parsing the full plan directly
    let plan: ExecutionPlan = serde_json::from_value(args.clone()).map_err(|e| {
        format!(
            "Failed to parse execution plan: {}. Expected format: \
            {{\"description\": \"...\", \"nodes\": [{{\"id\": \"a\", \"tool\": \"tool_name\", \
            \"args\": {{}}, \"depends_on\": []}}]}}",
            e
        )
    })?;

    Ok(plan)
}

/// Validate a parsed plan against the live tool registry.
///
/// Combines structural validation (from atoms) with tool existence checks
/// against the currently available tools.
///
/// Returns a list of errors. Empty list means the plan is valid.
pub fn validate_plan(
    plan: &ExecutionPlan,
    available_tools: &[ToolDefinition],
) -> Vec<PlanValidationError> {
    let mut errors = validate_plan_structure(plan);

    // Build the set of known tool names
    let known_tools: HashSet<&str> = available_tools
        .iter()
        .map(|t| t.function.name.as_str())
        .collect();

    // Check each node references a real tool
    for node in &plan.nodes {
        if !known_tools.contains(node.tool.as_str()) {
            errors.push(PlanValidationError::UnknownTool {
                node_id: node.id.clone(),
                tool: node.tool.clone(),
            });
        }
    }

    errors
}

/// Build a human-readable summary of the execution plan for logging and
/// the frontend status display.
pub fn describe_plan(plan: &ExecutionPlan) -> String {
    let phases = build_execution_phases(plan);
    let mut lines = Vec::new();

    lines.push(format!(
        "Execution Plan: {} nodes in {} phases",
        plan.nodes.len(),
        phases.len()
    ));

    if !plan.description.is_empty() {
        lines.push(format!("Goal: {}", plan.description));
    }

    for phase in &phases {
        let tools: Vec<String> = phase
            .node_ids
            .iter()
            .filter_map(|id| plan.nodes.iter().find(|n| &n.id == id))
            .map(|n| format!("{}({})", n.tool, n.id))
            .collect();

        if tools.len() == 1 {
            lines.push(format!("  Phase {}: {}", phase.index, tools[0]));
        } else {
            lines.push(format!(
                "  Phase {}: {} (parallel)",
                phase.index,
                tools.join(" ‖ ")
            ));
        }
    }

    lines.join("\n")
}

/// Build a summary of plan results for injection into the model's next
/// context. This gives the model all results (success, error, skip) so it
/// can synthesize a user-facing response.
pub fn build_results_context(plan: &ExecutionPlan, results: &[NodeResult]) -> String {
    let mut parts = Vec::new();

    parts.push("[Plan Execution Results]".to_string());

    let success_count = results
        .iter()
        .filter(|r| r.status == NodeStatus::Success)
        .count();
    let error_count = results
        .iter()
        .filter(|r| r.status == NodeStatus::Error)
        .count();
    let skip_count = results
        .iter()
        .filter(|r| r.status == NodeStatus::Skipped)
        .count();

    parts.push(format!(
        "Completed: {}/{} nodes ({} success, {} failed, {} skipped)",
        success_count + error_count,
        plan.nodes.len(),
        success_count,
        error_count,
        skip_count
    ));

    for result in results {
        let status_icon = match result.status {
            NodeStatus::Success => "✓",
            NodeStatus::Error => "✗",
            NodeStatus::Skipped => "⊘",
        };

        let duration = if result.duration_ms > 0 {
            format!(" ({}ms)", result.duration_ms)
        } else {
            String::new()
        };

        parts.push(format!(
            "\n[{} {} — {}{}]",
            status_icon, result.node_id, result.tool, duration
        ));

        // Truncate very long outputs to avoid context bloat
        let output = if result.output.len() > 2000 {
            format!(
                "{}… (truncated, {} chars total)",
                &result.output[..2000],
                result.output.len()
            )
        } else {
            result.output.clone()
        };

        parts.push(output);
    }

    parts.join("\n")
}

/// Log plan execution details for observability.
pub fn log_plan_summary(plan: &ExecutionPlan, results: &[NodeResult]) {
    let success = results
        .iter()
        .filter(|r| r.status == NodeStatus::Success)
        .count();
    let errors = results
        .iter()
        .filter(|r| r.status == NodeStatus::Error)
        .count();
    let skipped = results
        .iter()
        .filter(|r| r.status == NodeStatus::Skipped)
        .count();
    let total_ms: u64 = results.iter().map(|r| r.duration_ms).max().unwrap_or(0);

    info!(
        "[plan] Plan '{}' completed: {}/{} success, {} errors, {} skipped, ~{}ms",
        plan.description,
        success,
        plan.nodes.len(),
        errors,
        skipped,
        total_ms
    );

    for result in results {
        match result.status {
            NodeStatus::Success => {
                info!(
                    "[plan]   ✓ {} ({}) — {}ms",
                    result.node_id, result.tool, result.duration_ms
                );
            }
            NodeStatus::Error => {
                warn!(
                    "[plan]   ✗ {} ({}) — {} (retries: {})",
                    result.node_id, result.tool, result.output, result.retries
                );
            }
            NodeStatus::Skipped => {
                info!(
                    "[plan]   ⊘ {} ({}) — skipped: {}",
                    result.node_id, result.tool, result.output
                );
            }
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atoms::types::FunctionDefinition;

    fn make_tool_def(name: &str) -> ToolDefinition {
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: name.into(),
                description: format!("Test tool {}", name),
                parameters: serde_json::json!({"type": "object", "properties": {}}),
            },
        }
    }

    #[test]
    fn test_parse_valid_plan() {
        let args = serde_json::json!({
            "description": "Search and send email",
            "nodes": [
                {"id": "a", "tool": "gmail_search", "args": {"query": "action items"}},
                {"id": "b", "tool": "gmail_send", "args": {"to": "team"}, "depends_on": ["a"]}
            ]
        });
        let plan = parse_plan(&args).unwrap();
        assert_eq!(plan.nodes.len(), 2);
        assert_eq!(plan.description, "Search and send email");
    }

    #[test]
    fn test_parse_invalid_plan() {
        let args = serde_json::json!({"bad": "format"});
        assert!(parse_plan(&args).is_err());
    }

    #[test]
    fn test_validate_with_tool_registry() {
        let plan = ExecutionPlan {
            description: "test".into(),
            nodes: vec![
                PlanNode {
                    id: "a".into(),
                    tool: "gmail_search".into(),
                    args: serde_json::json!({}),
                    depends_on: vec![],
                },
                PlanNode {
                    id: "b".into(),
                    tool: "nonexistent_tool".into(),
                    args: serde_json::json!({}),
                    depends_on: vec!["a".into()],
                },
            ],
        };

        let tools = vec![make_tool_def("gmail_search"), make_tool_def("gmail_send")];
        let errors = validate_plan(&plan, &tools);
        assert!(errors.iter().any(
            |e| matches!(e, PlanValidationError::UnknownTool { tool, .. } if tool == "nonexistent_tool")
        ));
    }

    #[test]
    fn test_describe_plan() {
        let plan = ExecutionPlan {
            description: "Set up standup".into(),
            nodes: vec![
                PlanNode {
                    id: "a".into(),
                    tool: "google_calendar_create".into(),
                    args: serde_json::json!({}),
                    depends_on: vec![],
                },
                PlanNode {
                    id: "b".into(),
                    tool: "gmail_search".into(),
                    args: serde_json::json!({}),
                    depends_on: vec![],
                },
                PlanNode {
                    id: "c".into(),
                    tool: "gmail_send".into(),
                    args: serde_json::json!({}),
                    depends_on: vec!["a".into(), "b".into()],
                },
            ],
        };
        let desc = describe_plan(&plan);
        assert!(desc.contains("3 nodes in 2 phases"));
        assert!(desc.contains("parallel"));
    }

    #[test]
    fn test_build_results_context() {
        let plan = ExecutionPlan {
            description: "test".into(),
            nodes: vec![
                PlanNode {
                    id: "a".into(),
                    tool: "fetch".into(),
                    args: serde_json::json!({}),
                    depends_on: vec![],
                },
                PlanNode {
                    id: "b".into(),
                    tool: "exec".into(),
                    args: serde_json::json!({}),
                    depends_on: vec![],
                },
            ],
        };
        let results = vec![
            NodeResult {
                node_id: "a".into(),
                tool: "fetch".into(),
                status: NodeStatus::Success,
                output: "fetched data".into(),
                retryable: false,
                retries: 0,
                duration_ms: 150,
            },
            NodeResult {
                node_id: "b".into(),
                tool: "exec".into(),
                status: NodeStatus::Error,
                output: "command failed".into(),
                retryable: true,
                retries: 2,
                duration_ms: 3200,
            },
        ];
        let ctx = build_results_context(&plan, &results);
        assert!(ctx.contains("1 success"));
        assert!(ctx.contains("1 failed"));
        assert!(ctx.contains("fetched data"));
        assert!(ctx.contains("command failed"));
    }
}
