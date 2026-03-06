// ─────────────────────────────────────────────────────────────────────────────
// Action DAG Planning — Atoms
//
// Pure types, constants, and validation functions for execution plans.
// No side effects. No I/O. No async. No state.
//
// An execution plan is a DAG (directed acyclic graph) of tool calls that the
// model emits in a single inference call. The engine validates the DAG,
// groups nodes into parallel execution phases, and runs them concurrently.
// ─────────────────────────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// ── Constants ──────────────────────────────────────────────────────────────

/// Maximum number of nodes allowed in a single plan.
/// Prevents unbounded plans from consuming resources.
pub const MAX_PLAN_NODES: usize = 20;

/// Default timeout per node in milliseconds (30 seconds).
pub const DEFAULT_NODE_TIMEOUT_MS: u64 = 30_000;

/// Timeout for MCP tool nodes in milliseconds (60 seconds).
pub const MCP_NODE_TIMEOUT_MS: u64 = 60_000;

/// Overall plan timeout in milliseconds (5 minutes).
pub const PLAN_TIMEOUT_MS: u64 = 300_000;

/// Maximum retry attempts for retryable failures.
pub const MAX_RETRIES: u32 = 2;

/// Base delay for exponential backoff in milliseconds.
pub const RETRY_BASE_DELAY_MS: u64 = 1_000;

// ── Types ──────────────────────────────────────────────────────────────────

/// A single node in the execution plan DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanNode {
    /// Unique identifier for this node within the plan (e.g., "a", "b", "step_1").
    pub id: String,

    /// Tool name to execute (must be a valid registered tool).
    pub tool: String,

    /// Tool arguments as a JSON value.
    pub args: serde_json::Value,

    /// IDs of nodes that must complete before this node can execute.
    /// Empty means the node has no dependencies and can run immediately.
    #[serde(default)]
    pub depends_on: Vec<String>,
}

/// The full execution plan emitted by the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    /// Human-readable description of the plan's goal.
    #[serde(default)]
    pub description: String,

    /// The nodes to execute.
    pub nodes: Vec<PlanNode>,
}

/// Result status for a single node execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    /// Node executed successfully.
    Success,
    /// Node failed after all retries.
    Error,
    /// Node was skipped because a dependency failed.
    Skipped,
}

/// The result of executing a single node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResult {
    /// The node ID.
    pub node_id: String,

    /// The tool that was called.
    pub tool: String,

    /// Execution status.
    pub status: NodeStatus,

    /// Output text (tool result, error message, or skip reason).
    pub output: String,

    /// Whether the error was retryable (only meaningful for Error status).
    #[serde(default)]
    pub retryable: bool,

    /// Number of retries attempted.
    #[serde(default)]
    pub retries: u32,

    /// Execution duration in milliseconds.
    #[serde(default)]
    pub duration_ms: u64,
}

/// A phase of execution — all nodes in a phase run concurrently.
#[derive(Debug, Clone)]
pub struct ExecutionPhase {
    /// Phase index (0-based).
    pub index: usize,

    /// Node IDs to execute in this phase (concurrently).
    pub node_ids: Vec<String>,
}

/// Validation errors for an execution plan.
#[derive(Debug, Clone, PartialEq)]
pub enum PlanValidationError {
    /// Plan has no nodes.
    EmptyPlan,
    /// Plan exceeds the maximum node count.
    TooManyNodes(usize),
    /// A node has a duplicate ID.
    DuplicateNodeId(String),
    /// A node depends on a non-existent node.
    MissingDependency { node_id: String, missing: String },
    /// The plan contains a cycle (not a DAG).
    CycleDetected(Vec<String>),
    /// A node references an unknown tool.
    UnknownTool { node_id: String, tool: String },
    /// A node's arguments contain a raw secret.
    SecretInArguments { node_id: String, field: String },
}

impl std::fmt::Display for PlanValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyPlan => write!(f, "Plan has no nodes"),
            Self::TooManyNodes(n) => {
                write!(f, "Plan has {} nodes (max: {})", n, MAX_PLAN_NODES)
            }
            Self::DuplicateNodeId(id) => write!(f, "Duplicate node ID: '{}'", id),
            Self::MissingDependency { node_id, missing } => {
                write!(
                    f,
                    "Node '{}' depends on '{}' which does not exist",
                    node_id, missing
                )
            }
            Self::CycleDetected(cycle) => {
                write!(f, "Cycle detected: {}", cycle.join(" → "))
            }
            Self::UnknownTool { node_id, tool } => {
                write!(f, "Node '{}' references unknown tool '{}'", node_id, tool)
            }
            Self::SecretInArguments { node_id, field } => {
                write!(
                    f,
                    "Security: argument '{}' in node '{}' contains a secret. Use vault reference instead",
                    field, node_id
                )
            }
        }
    }
}

// ── Pure validation functions ──────────────────────────────────────────────

/// Validate an execution plan structurally (no I/O, no tool registry lookup).
///
/// Checks:
/// - Non-empty
/// - Node count within limits
/// - No duplicate IDs
/// - All dependencies reference existing nodes
/// - No cycles (topological sort)
///
/// Does NOT check tool validity (that requires the tool registry).
pub fn validate_plan_structure(plan: &ExecutionPlan) -> Vec<PlanValidationError> {
    let mut errors = Vec::new();

    // Empty check
    if plan.nodes.is_empty() {
        errors.push(PlanValidationError::EmptyPlan);
        return errors;
    }

    // Size check
    if plan.nodes.len() > MAX_PLAN_NODES {
        errors.push(PlanValidationError::TooManyNodes(plan.nodes.len()));
    }

    // Duplicate ID check
    let mut seen_ids = HashSet::new();
    for node in &plan.nodes {
        if !seen_ids.insert(&node.id) {
            errors.push(PlanValidationError::DuplicateNodeId(node.id.clone()));
        }
    }

    // Missing dependency check
    let all_ids: HashSet<&str> = plan.nodes.iter().map(|n| n.id.as_str()).collect();
    for node in &plan.nodes {
        for dep in &node.depends_on {
            if !all_ids.contains(dep.as_str()) {
                errors.push(PlanValidationError::MissingDependency {
                    node_id: node.id.clone(),
                    missing: dep.clone(),
                });
            }
        }
    }

    // Cycle detection via Kahn's algorithm (topological sort)
    if let Some(cycle) = detect_cycle(plan) {
        errors.push(PlanValidationError::CycleDetected(cycle));
    }

    // Secret detection in arguments
    for node in &plan.nodes {
        if let Some(field) = detect_secret_in_args(&node.args) {
            errors.push(PlanValidationError::SecretInArguments {
                node_id: node.id.clone(),
                field,
            });
        }
    }

    errors
}

/// Detect cycles in the plan DAG using Kahn's algorithm.
/// Returns `Some(cycle_path)` if a cycle exists, `None` if the graph is acyclic.
fn detect_cycle(plan: &ExecutionPlan) -> Option<Vec<String>> {
    let node_ids: HashSet<&str> = plan.nodes.iter().map(|n| n.id.as_str()).collect();

    // Build adjacency list and in-degree map
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();

    for node in &plan.nodes {
        in_degree.entry(node.id.as_str()).or_insert(0);
        adjacency.entry(node.id.as_str()).or_default();
        for dep in &node.depends_on {
            if node_ids.contains(dep.as_str()) {
                adjacency.entry(dep.as_str()).or_default().push(&node.id);
                *in_degree.entry(node.id.as_str()).or_insert(0) += 1;
            }
        }
    }

    // Kahn's algorithm
    let mut queue: Vec<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&id, _)| id)
        .collect();
    let mut sorted_count = 0;

    while let Some(node) = queue.pop() {
        sorted_count += 1;
        if let Some(neighbors) = adjacency.get(node) {
            for &next in neighbors {
                if let Some(deg) = in_degree.get_mut(next) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push(next);
                    }
                }
            }
        }
    }

    if sorted_count < plan.nodes.len() {
        // Cycle exists — return the remaining nodes (they're in the cycle)
        let remaining: Vec<String> = in_degree
            .iter()
            .filter(|(_, &deg)| deg > 0)
            .map(|(&id, _)| id.to_string())
            .collect();
        Some(remaining)
    } else {
        None
    }
}

/// Check if a JSON value contains strings that look like secrets.
/// Returns the first field name that contains a secret pattern, or None.
fn detect_secret_in_args(args: &serde_json::Value) -> Option<String> {
    /// Known secret prefixes and patterns
    const SECRET_PREFIXES: &[&str] = &[
        "sk-",
        "GOCSPX-",
        "xoxb-",
        "xoxp-",
        "ghp_",
        "gho_",
        "glpat-",
        "ya29.",
        "AIza",
        "-----BEGIN",
    ];

    match args {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                if let Some(s) = value.as_str() {
                    for prefix in SECRET_PREFIXES {
                        if s.contains(prefix) {
                            return Some(key.clone());
                        }
                    }
                    // Check for JWT pattern (eyJ...)
                    if s.len() > 50 && s.starts_with("eyJ") {
                        return Some(key.clone());
                    }
                }
                // Recursively check nested objects
                if let Some(field) = detect_secret_in_args(value) {
                    return Some(format!("{}.{}", key, field));
                }
            }
            None
        }
        serde_json::Value::Array(arr) => {
            for (i, item) in arr.iter().enumerate() {
                if let Some(field) = detect_secret_in_args(item) {
                    return Some(format!("[{}].{}", i, field));
                }
            }
            None
        }
        _ => None,
    }
}

/// Compute the topological depth of each node (longest path from any root).
/// Used to group nodes into parallel execution phases.
pub fn compute_node_depths(plan: &ExecutionPlan) -> HashMap<String, usize> {
    let mut depths: HashMap<String, usize> = HashMap::new();

    // Build a lookup for fast dependency resolution
    let node_map: HashMap<&str, &PlanNode> =
        plan.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    fn resolve_depth(
        node_id: &str,
        node_map: &HashMap<&str, &PlanNode>,
        depths: &mut HashMap<String, usize>,
        visiting: &mut HashSet<String>,
    ) -> usize {
        if let Some(&d) = depths.get(node_id) {
            return d;
        }

        // Guard against cycles (should be caught by validation, but defensive)
        if !visiting.insert(node_id.to_string()) {
            return 0;
        }

        let node = match node_map.get(node_id) {
            Some(n) => n,
            None => return 0,
        };

        let depth = if node.depends_on.is_empty() {
            0
        } else {
            node.depends_on
                .iter()
                .map(|dep| resolve_depth(dep, node_map, depths, visiting) + 1)
                .max()
                .unwrap_or(0)
        };

        visiting.remove(node_id);
        depths.insert(node_id.to_string(), depth);
        depth
    }

    let mut visiting = HashSet::new();
    for node in &plan.nodes {
        resolve_depth(&node.id, &node_map, &mut depths, &mut visiting);
    }

    depths
}

/// Group plan nodes into sequential execution phases based on their depth.
/// Nodes at the same depth level have no mutual dependencies and can run
/// concurrently within the same phase.
pub fn build_execution_phases(plan: &ExecutionPlan) -> Vec<ExecutionPhase> {
    let depths = compute_node_depths(plan);

    // Group by depth
    let max_depth = depths.values().copied().max().unwrap_or(0);
    let mut phases: Vec<ExecutionPhase> = Vec::with_capacity(max_depth + 1);

    for depth in 0..=max_depth {
        let node_ids: Vec<String> = plan
            .nodes
            .iter()
            .filter(|n| depths.get(&n.id).copied().unwrap_or(0) == depth)
            .map(|n| n.id.clone())
            .collect();

        if !node_ids.is_empty() {
            phases.push(ExecutionPhase {
                index: depth,
                node_ids,
            });
        }
    }

    phases
}

/// Determine timeout for a node based on its tool type.
pub fn node_timeout_ms(tool_name: &str) -> u64 {
    if tool_name.starts_with("mcp_") {
        MCP_NODE_TIMEOUT_MS
    } else {
        DEFAULT_NODE_TIMEOUT_MS
    }
}

/// Determine if an error is retryable based on the error message.
pub fn is_retryable_error(error: &str) -> bool {
    let retryable_patterns = [
        "timeout",
        "timed out",
        "connection refused",
        "connection reset",
        "429",
        "rate limit",
        "too many requests",
        "500",
        "502",
        "503",
        "504",
        "internal server error",
        "bad gateway",
        "service unavailable",
        "gateway timeout",
        "ECONNRESET",
        "ECONNREFUSED",
        "ETIMEDOUT",
    ];

    let lower = error.to_lowercase();
    retryable_patterns
        .iter()
        .any(|pattern| lower.contains(&pattern.to_lowercase()))
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_plan(nodes: Vec<PlanNode>) -> ExecutionPlan {
        ExecutionPlan {
            description: "test plan".into(),
            nodes,
        }
    }

    fn make_node(id: &str, tool: &str, depends_on: Vec<&str>) -> PlanNode {
        PlanNode {
            id: id.into(),
            tool: tool.into(),
            args: serde_json::json!({}),
            depends_on: depends_on.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn test_validate_empty_plan() {
        let plan = make_plan(vec![]);
        let errors = validate_plan_structure(&plan);
        assert_eq!(errors, vec![PlanValidationError::EmptyPlan]);
    }

    #[test]
    fn test_validate_valid_plan() {
        let plan = make_plan(vec![
            make_node("a", "gmail_search", vec![]),
            make_node("b", "google_calendar_list", vec![]),
            make_node("c", "gmail_send", vec!["a", "b"]),
        ]);
        let errors = validate_plan_structure(&plan);
        assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_validate_duplicate_ids() {
        let plan = make_plan(vec![
            make_node("a", "fetch", vec![]),
            make_node("a", "exec", vec![]),
        ]);
        let errors = validate_plan_structure(&plan);
        assert!(errors
            .iter()
            .any(|e| matches!(e, PlanValidationError::DuplicateNodeId(id) if id == "a")));
    }

    #[test]
    fn test_validate_missing_dependency() {
        let plan = make_plan(vec![make_node("a", "fetch", vec!["z"])]);
        let errors = validate_plan_structure(&plan);
        assert!(errors.iter().any(
            |e| matches!(e, PlanValidationError::MissingDependency { node_id, missing } if node_id == "a" && missing == "z")
        ));
    }

    #[test]
    fn test_validate_cycle_detection() {
        let plan = make_plan(vec![
            make_node("a", "fetch", vec!["b"]),
            make_node("b", "exec", vec!["a"]),
        ]);
        let errors = validate_plan_structure(&plan);
        assert!(errors
            .iter()
            .any(|e| matches!(e, PlanValidationError::CycleDetected(_))));
    }

    #[test]
    fn test_validate_secret_in_args() {
        let plan = make_plan(vec![PlanNode {
            id: "a".into(),
            tool: "fetch".into(),
            args: serde_json::json!({"api_key": "sk-1234567890abcdef"}),
            depends_on: vec![],
        }]);
        let errors = validate_plan_structure(&plan);
        assert!(errors.iter().any(
            |e| matches!(e, PlanValidationError::SecretInArguments { node_id, field } if node_id == "a" && field == "api_key")
        ));
    }

    #[test]
    fn test_compute_depths_linear() {
        let plan = make_plan(vec![
            make_node("a", "fetch", vec![]),
            make_node("b", "exec", vec!["a"]),
            make_node("c", "write_file", vec!["b"]),
        ]);
        let depths = compute_node_depths(&plan);
        assert_eq!(depths["a"], 0);
        assert_eq!(depths["b"], 1);
        assert_eq!(depths["c"], 2);
    }

    #[test]
    fn test_compute_depths_parallel() {
        let plan = make_plan(vec![
            make_node("a", "gmail_search", vec![]),
            make_node("b", "google_calendar_list", vec![]),
            make_node("c", "gmail_send", vec!["a", "b"]),
        ]);
        let depths = compute_node_depths(&plan);
        assert_eq!(depths["a"], 0);
        assert_eq!(depths["b"], 0);
        assert_eq!(depths["c"], 1);
    }

    #[test]
    fn test_build_phases_parallel() {
        let plan = make_plan(vec![
            make_node("a", "gmail_search", vec![]),
            make_node("b", "google_calendar_list", vec![]),
            make_node("c", "gmail_send", vec!["a", "b"]),
        ]);
        let phases = build_execution_phases(&plan);
        assert_eq!(phases.len(), 2);
        assert_eq!(phases[0].node_ids.len(), 2); // a, b in parallel
        assert_eq!(phases[1].node_ids.len(), 1); // c sequential after
    }

    #[test]
    fn test_build_phases_diamond() {
        // Diamond: a -> b, a -> c, b -> d, c -> d
        let plan = make_plan(vec![
            make_node("a", "fetch", vec![]),
            make_node("b", "exec", vec!["a"]),
            make_node("c", "read_file", vec!["a"]),
            make_node("d", "write_file", vec!["b", "c"]),
        ]);
        let phases = build_execution_phases(&plan);
        assert_eq!(phases.len(), 3); // [a], [b, c], [d]
        assert_eq!(phases[0].node_ids, vec!["a"]);
        assert!(phases[1].node_ids.contains(&"b".to_string()));
        assert!(phases[1].node_ids.contains(&"c".to_string()));
        assert_eq!(phases[2].node_ids, vec!["d"]);
    }

    #[test]
    fn test_is_retryable_error() {
        assert!(is_retryable_error("connection timeout after 30s"));
        assert!(is_retryable_error("HTTP 429 Too Many Requests"));
        assert!(is_retryable_error("502 Bad Gateway"));
        assert!(!is_retryable_error("401 Unauthorized"));
        assert!(!is_retryable_error("404 Not Found"));
        assert!(!is_retryable_error("Invalid JSON schema"));
    }

    #[test]
    fn test_node_timeout() {
        assert_eq!(node_timeout_ms("gmail_send"), DEFAULT_NODE_TIMEOUT_MS);
        assert_eq!(node_timeout_ms("mcp_execute_workflow"), MCP_NODE_TIMEOUT_MS);
    }

    #[test]
    fn test_too_many_nodes() {
        let nodes: Vec<PlanNode> = (0..25)
            .map(|i| make_node(&format!("n{}", i), "fetch", vec![]))
            .collect();
        let plan = make_plan(nodes);
        let errors = validate_plan_structure(&plan);
        assert!(errors
            .iter()
            .any(|e| matches!(e, PlanValidationError::TooManyNodes(25))));
    }
}
