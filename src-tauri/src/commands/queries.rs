// commands/queries.rs — Tauri IPC commands for agent queries
//
// Phase 2.9: execute read-only queries against connected services via n8n.

use crate::engine::channels;
use serde::{Deserialize, Serialize};

// ── Types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRequest {
    /// Natural language query from the user.
    pub question: String,
    /// Service IDs to query (empty = auto-detect).
    #[serde(rename = "serviceIds", default)]
    pub service_ids: Vec<String>,
    /// Optional query category for routing.
    #[serde(default)]
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryHighlight {
    pub severity: String,
    pub icon: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryKpi {
    pub label: String,
    pub value: String,
    #[serde(default)]
    pub trend: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryData {
    #[serde(rename = "type")]
    pub data_type: String,
    #[serde(default)]
    pub columns: Option<Vec<String>>,
    #[serde(default)]
    pub rows: Option<Vec<Vec<String>>>,
    #[serde(default)]
    pub items: Option<Vec<String>>,
    #[serde(default)]
    pub kpis: Option<Vec<QueryKpi>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    #[serde(rename = "queryId")]
    pub query_id: String,
    pub status: String,
    pub formatted: String,
    #[serde(default)]
    pub data: Option<QueryData>,
    #[serde(default)]
    pub highlights: Option<Vec<QueryHighlight>>,
    #[serde(rename = "executedAt")]
    pub executed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryHistoryEntry {
    pub id: String,
    pub question: String,
    #[serde(rename = "serviceIds")]
    pub service_ids: Vec<String>,
    pub status: String,
    pub formatted: String,
    #[serde(rename = "executedAt")]
    pub executed_at: String,
}

// ── Storage helpers ─────────────────────────────────────────────────────

const STORAGE_KEY: &str = "query_history";

fn load_history(app_handle: &tauri::AppHandle) -> Vec<QueryHistoryEntry> {
    channels::load_channel_config::<Vec<QueryHistoryEntry>>(app_handle, STORAGE_KEY)
        .unwrap_or_default()
}

fn save_history(
    app_handle: &tauri::AppHandle,
    history: &[QueryHistoryEntry],
) -> Result<(), String> {
    channels::save_channel_config(app_handle, STORAGE_KEY, &history.to_vec())
        .map_err(|e| e.to_string())
}

// ── Commands ───────────────────────────────────────────────────────────

/// Execute a query against connected services.
/// Routes the query through available integration tools (REST API, n8n, native skills).
#[tauri::command]
pub async fn engine_queries_execute(
    app_handle: tauri::AppHandle,
    request: QueryRequest,
) -> Result<QueryResult, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let query_id = format!("q-{}", now.replace([':', '-', '+'], ""));

    // Detect target services from the question if not provided
    let target_services = if request.service_ids.is_empty() {
        _detect_services(&request.question)
    } else {
        request.service_ids.clone()
    };

    // Try to execute the query by making direct API calls to connected services
    let result_text = execute_query_direct(&app_handle, &request.question, &target_services).await;

    // Store in history
    let entry = QueryHistoryEntry {
        id: query_id.clone(),
        question: request.question.clone(),
        service_ids: target_services.clone(),
        status: "success".to_string(),
        formatted: result_text.clone(),
        executed_at: now.clone(),
    };

    let mut history = load_history(&app_handle);
    history.push(entry);

    // Keep last 100 entries
    if history.len() > 100 {
        history = history.split_off(history.len() - 100);
    }

    save_history(&app_handle, &history)?;

    Ok(QueryResult {
        query_id,
        status: "success".to_string(),
        formatted: result_text,
        data: None,
        highlights: None,
        executed_at: now,
    })
}

/// Execute a query by making direct REST API calls to connected services.
async fn execute_query_direct(
    app_handle: &tauri::AppHandle,
    question: &str,
    target_services: &[String],
) -> String {
    use crate::engine::tools;

    let mut results = Vec::new();

    for service_id in target_services {
        // Build a tool call to execute the query
        let tool_call = build_query_tool_call(service_id, question);

        if let Some(tc) = tool_call {
            let result = tools::execute_tool(&tc, app_handle, "query-agent").await;
            if result.success {
                results.push(format!("📡 {} result:\n{}", service_id, result.output));
            } else {
                results.push(format!(
                    "⚠️ {} error: {}\nEnsure the service is connected in Integrations.",
                    service_id, result.output
                ));
            }
        } else {
            results.push(format!(
                "ℹ️ {} — No direct query tool available. \
                 Connect the service in Integrations to enable queries.",
                service_id,
            ));
        }
    }

    if results.is_empty() {
        format!(
            "🔍 Query: \"{}\"\n\n\
             No target services detected. Try asking about a specific service \
             (e.g., \"How many GitHub issues are open?\").",
            question,
        )
    } else {
        format!(
            "🔍 Query: \"{}\"\n\n{}",
            question,
            results.join("\n\n"),
        )
    }
}

/// Map a query to the appropriate tool call for a given service.
fn build_query_tool_call(
    service_id: &str,
    question: &str,
) -> Option<crate::engine::types::ToolCall> {
    let (tool_name, args) = match service_id {
        "github" => {
            // Map common GitHub queries to API endpoints
            let endpoint = if question.to_lowercase().contains("issue") {
                "/repos/{owner}/{repo}/issues?state=open&per_page=10"
            } else if question.to_lowercase().contains("pr") || question.to_lowercase().contains("pull request") {
                "/repos/{owner}/{repo}/pulls?state=open&per_page=10"
            } else {
                "/user/repos?sort=updated&per_page=5"
            };
            ("github_api", serde_json::json!({"endpoint": endpoint, "method": "GET"}))
        }
        "slack" => {
            ("slack_read", serde_json::json!({"channel": "general", "limit": 10}))
        }
        "trello" => {
            ("rest_api_call", serde_json::json!({"path": "/1/members/me/boards", "method": "GET"}))
        }
        // For services that map to rest_api, use rest_api_call
        "notion" | "linear" | "todoist" | "clickup" | "airtable"
        | "sendgrid" | "hubspot" | "stripe" => {
            let path = match service_id {
                "notion" => "/search",
                "linear" => "/graphql",
                "todoist" => "/tasks",
                "clickup" => "/team",
                "airtable" => "/meta/bases",
                "sendgrid" => "/user/profile",
                "hubspot" => "/crm/v3/objects/contacts?limit=5",
                "stripe" => "/balance",
                _ => "/",
            };
            let method = if service_id == "notion" || service_id == "linear" { "POST" } else { "GET" };
            ("rest_api_call", serde_json::json!({"path": path, "method": method}))
        }
        // Generic n8n execution for everything else
        _ => {
            ("n8n_execute_action", serde_json::json!({
                "service": service_id,
                "action": "list",
                "params": {}
            }))
        }
    };

    Some(crate::engine::types::ToolCall {
        id: format!("query-{}-{}", service_id, chrono::Utc::now().timestamp_millis()),
        call_type: "function".into(),
        function: crate::engine::types::FunctionCall {
            name: tool_name.into(),
            arguments: serde_json::to_string(&args).unwrap_or_default(),
        },
        thought_signature: None,
        thought_parts: vec![],
    })
}

/// List recent query history.
#[tauri::command]
pub async fn engine_queries_history(
    app_handle: tauri::AppHandle,
) -> Result<Vec<QueryHistoryEntry>, String> {
    Ok(load_history(&app_handle))
}

/// Clear query history.
#[tauri::command]
pub async fn engine_queries_clear_history(
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    save_history(&app_handle, &[])?;
    Ok(())
}

// ── Service detection ──────────────────────────────────────────────────

/// Naive keyword-based service detection from natural language queries.
fn _detect_services(question: &str) -> Vec<String> {
    let q = question.to_lowercase();
    let mut services = Vec::new();

    let patterns: &[(&str, &[&str])] = &[
        ("hubspot",   &["hubspot", "deal", "pipeline", "contacts", "companies"]),
        ("salesforce", &["salesforce", "opportunity", "leads", "accounts"]),
        ("trello",    &["trello", "board", "card", "kanban"]),
        ("jira",      &["jira", "sprint", "epic", "story"]),
        ("linear",    &["linear", "cycle", "project"]),
        ("slack",     &["slack", "channel", "message", "dm", "mention"]),
        ("discord",   &["discord", "server", "guild"]),
        ("telegram",  &["telegram", "bot", "group chat"]),
        ("github",    &["github", "repo", "pull request", "pr", "commit", "issue"]),
        ("notion",    &["notion", "page", "database", "wiki"]),
        ("google-sheets", &["sheet", "spreadsheet", "google sheets"]),
        ("gmail",     &["gmail", "email", "inbox", "mail"]),
        ("shopify",   &["shopify", "order", "product", "inventory"]),
        ("stripe",    &["stripe", "payment", "charge", "balance"]),
        ("zendesk",   &["zendesk", "ticket", "support"]),
        ("asana",     &["asana"]),
        ("clickup",   &["clickup"]),
        ("monday",    &["monday"]),
        ("todoist",   &["todoist"]),
        ("airtable",  &["airtable", "base"]),
        ("sendgrid",  &["sendgrid"]),
        ("twilio",    &["twilio", "sms", "call log"]),
    ];

    for (service_id, keywords) in patterns {
        if keywords.iter().any(|kw| q.contains(kw)) {
            services.push(service_id.to_string());
        }
    }

    // Default to a general query if no service detected
    if services.is_empty() {
        services.push("general".to_string());
    }

    services
}
