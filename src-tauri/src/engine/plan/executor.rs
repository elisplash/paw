// ─────────────────────────────────────────────────────────────────────────────
// Action DAG Planning — Executor
//
// Parallel execution engine for action DAGs. Runs phases concurrently via
// tokio::spawn, with retry, per-node timeout, dependency-aware degradation,
// and partial result synthesis.
//
// This is the "hot" module — it has side effects (tool execution, IPC events).
// ─────────────────────────────────────────────────────────────────────────────

use super::atoms::*;
use super::molecules;
use crate::atoms::types::*;
use crate::engine::tools;
use crate::engine::types::ToolCall;
use log::{info, warn};
use std::collections::HashMap;
use std::time::Instant;
use tauri::Emitter;

/// Execute an action DAG plan with parallel phase execution.
///
/// For each phase (group of independent nodes at the same DAG depth),
/// all nodes are spawned concurrently. Within a node, retries are attempted
/// for retryable errors with exponential backoff.
///
/// Returns all node results — success, error, or skipped — for context
/// synthesis by the model.
pub async fn execute_plan(
    plan: &ExecutionPlan,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
    session_id: &str,
    run_id: &str,
) -> Vec<NodeResult> {
    let plan_start = Instant::now();
    let phases = build_execution_phases(plan);
    let node_map: HashMap<&str, &PlanNode> =
        plan.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    // Emit plan start event
    let description = molecules::describe_plan(plan);
    info!("[plan] Starting execution: {}", description);
    let _ = app_handle.emit(
        "engine-event",
        EngineEvent::PlanStart {
            session_id: session_id.to_string(),
            run_id: run_id.to_string(),
            description: description.clone(),
            node_count: plan.nodes.len(),
        },
    );

    let mut all_results: Vec<NodeResult> = Vec::new();
    let mut failed_nodes: std::collections::HashSet<String> = std::collections::HashSet::new();

    for phase in &phases {
        // Check overall plan timeout
        if plan_start.elapsed().as_millis() as u64 > PLAN_TIMEOUT_MS {
            warn!(
                "[plan] Plan timeout exceeded ({}ms), cancelling remaining phases",
                PLAN_TIMEOUT_MS
            );
            // Mark all remaining nodes as skipped
            for remaining_phase in phases.iter().skip(phase.index) {
                for node_id in &remaining_phase.node_ids {
                    if !all_results.iter().any(|r| r.node_id == *node_id) {
                        all_results.push(NodeResult {
                            node_id: node_id.clone(),
                            tool: node_map
                                .get(node_id.as_str())
                                .map(|n| n.tool.clone())
                                .unwrap_or_default(),
                            status: NodeStatus::Skipped,
                            output: "Plan timeout exceeded".to_string(),
                            retryable: false,
                            retries: 0,
                            duration_ms: 0,
                        });
                    }
                }
            }
            break;
        }

        // For each node in this phase, check if its dependencies have failed
        let mut phase_tasks = Vec::new();

        for node_id in &phase.node_ids {
            let node = match node_map.get(node_id.as_str()) {
                Some(n) => *n,
                None => continue,
            };

            // Dependency-aware degradation: skip if any dependency failed
            let failed_deps: Vec<String> = node
                .depends_on
                .iter()
                .filter(|dep| failed_nodes.contains(dep.as_str()))
                .cloned()
                .collect();

            if !failed_deps.is_empty() {
                let skip_reason =
                    format!("Skipped: dependencies failed: {}", failed_deps.join(", "));
                info!("[plan] Skipping node '{}': {}", node_id, skip_reason);
                all_results.push(NodeResult {
                    node_id: node_id.clone(),
                    tool: node.tool.clone(),
                    status: NodeStatus::Skipped,
                    output: skip_reason,
                    retryable: false,
                    retries: 0,
                    duration_ms: 0,
                });
                failed_nodes.insert(node_id.clone());
                continue;
            }

            // Clone data for the spawned task
            let node_owned = node.clone();
            let app_handle_clone = app_handle.clone();
            let agent_id_owned = agent_id.to_string();
            let session_id_owned = session_id.to_string();
            let run_id_owned = run_id.to_string();

            let task = tokio::spawn(async move {
                execute_node_with_retry(
                    &node_owned,
                    &app_handle_clone,
                    &agent_id_owned,
                    &session_id_owned,
                    &run_id_owned,
                )
                .await
            });

            phase_tasks.push((node_id.clone(), task));
        }

        // Await all phase tasks concurrently
        for (node_id, task) in phase_tasks {
            match task.await {
                Ok(result) => {
                    if result.status == NodeStatus::Error {
                        failed_nodes.insert(node_id);
                    }
                    all_results.push(result);
                }
                Err(join_err) => {
                    warn!("[plan] Node '{}' panicked: {}", node_id, join_err);
                    failed_nodes.insert(node_id.clone());
                    all_results.push(NodeResult {
                        node_id: node_id.clone(),
                        tool: node_map
                            .get(node_id.as_str())
                            .map(|n| n.tool.clone())
                            .unwrap_or_default(),
                        status: NodeStatus::Error,
                        output: format!("Internal error: task panicked: {}", join_err),
                        retryable: false,
                        retries: 0,
                        duration_ms: 0,
                    });
                }
            }
        }
    }

    // Log summary and emit completion event
    molecules::log_plan_summary(plan, &all_results);

    let success_count = all_results
        .iter()
        .filter(|r| r.status == NodeStatus::Success)
        .count();

    let _ = app_handle.emit(
        "engine-event",
        EngineEvent::PlanComplete {
            session_id: session_id.to_string(),
            run_id: run_id.to_string(),
            success_count,
            total_count: plan.nodes.len(),
            duration_ms: plan_start.elapsed().as_millis() as u64,
        },
    );

    all_results
}

/// Execute a single node with retry logic and per-node timeout.
async fn execute_node_with_retry(
    node: &PlanNode,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
    session_id: &str,
    run_id: &str,
) -> NodeResult {
    let node_start = Instant::now();
    let timeout = node_timeout_ms(&node.tool);
    let mut last_error: String;
    let mut retries: u32 = 0;

    // Emit node start
    let _ = app_handle.emit(
        "engine-event",
        EngineEvent::PlanNodeStart {
            session_id: session_id.to_string(),
            run_id: run_id.to_string(),
            node_id: node.id.clone(),
            tool: node.tool.clone(),
        },
    );

    loop {
        // Build a synthetic ToolCall for the existing execute_tool interface
        let tool_call = ToolCall {
            id: format!("plan_{}_{}", node.id, retries),
            call_type: "function".to_string(),
            function: crate::atoms::types::FunctionCall {
                name: node.tool.clone(),
                arguments: node.args.to_string(),
            },
            thought_signature: None,
            thought_parts: vec![],
        };

        // Execute with per-node timeout
        let execute_result = tokio::time::timeout(
            std::time::Duration::from_millis(timeout),
            tools::execute_tool(&tool_call, app_handle, agent_id),
        )
        .await;

        match execute_result {
            Ok(result) => {
                let duration_ms = node_start.elapsed().as_millis() as u64;

                // Emit node result event
                let _ = app_handle.emit(
                    "engine-event",
                    EngineEvent::ToolResultEvent {
                        session_id: session_id.to_string(),
                        run_id: run_id.to_string(),
                        tool_call_id: tool_call.id.clone(),
                        output: result.output.clone(),
                        success: result.success,
                        duration_ms: Some(duration_ms),
                    },
                );

                if result.success {
                    return NodeResult {
                        node_id: node.id.clone(),
                        tool: node.tool.clone(),
                        status: NodeStatus::Success,
                        output: result.output,
                        retryable: false,
                        retries,
                        duration_ms,
                    };
                }

                // Check if retryable
                last_error = result.output.clone();
                if is_retryable_error(&result.output) && retries < MAX_RETRIES {
                    retries += 1;
                    let delay = RETRY_BASE_DELAY_MS * (1 << retries.min(4));
                    warn!(
                        "[plan] Node '{}' failed (retryable), retry {}/{} in {}ms: {}",
                        node.id, retries, MAX_RETRIES, delay, last_error
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                    continue;
                }

                // Non-retryable or out of retries
                return NodeResult {
                    node_id: node.id.clone(),
                    tool: node.tool.clone(),
                    status: NodeStatus::Error,
                    output: result.output,
                    retryable: is_retryable_error(&last_error),
                    retries,
                    duration_ms,
                };
            }
            Err(_timeout) => {
                let duration_ms = node_start.elapsed().as_millis() as u64;
                last_error = format!("Timeout after {}ms", timeout);

                if retries < MAX_RETRIES {
                    retries += 1;
                    let delay = RETRY_BASE_DELAY_MS * (1 << retries.min(4));
                    warn!(
                        "[plan] Node '{}' timed out, retry {}/{} in {}ms",
                        node.id, retries, MAX_RETRIES, delay
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                    continue;
                }

                return NodeResult {
                    node_id: node.id.clone(),
                    tool: node.tool.clone(),
                    status: NodeStatus::Error,
                    output: last_error,
                    retryable: true,
                    retries,
                    duration_ms,
                };
            }
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full integration tests for execute_plan require a running Tauri app
    // and mock tools. These are covered by the plan integration test suite.
    // Unit tests here validate the helper logic.

    #[test]
    fn test_retry_backoff_values() {
        // Verify exponential backoff formula: base * 2^retry
        let retry_1: u32 = 1;
        let retry_2: u32 = 2;
        assert_eq!(RETRY_BASE_DELAY_MS * (1 << retry_1.min(4)), 2000); // retry 1: 2s
        assert_eq!(RETRY_BASE_DELAY_MS * (1 << retry_2.min(4)), 4000); // retry 2: 4s
    }

    #[test]
    fn test_plan_timeout_sanity() {
        // Plan timeout should be >= 5 * node timeout (for a fully sequential 5-node plan)
        let plan_t = PLAN_TIMEOUT_MS;
        let node_t = DEFAULT_NODE_TIMEOUT_MS;
        assert!(plan_t >= 5 * node_t);
    }
}
