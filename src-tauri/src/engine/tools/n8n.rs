// Paw Agent Engine — n8n Workflow Tools
// n8n_list_workflows, n8n_trigger_workflow, n8n_execute_action
//
// Allows agents to use n8n as a universal integration engine.
// For services without dedicated tool modules, agents can trigger
// n8n workflows to perform actions across 400+ services.

use crate::atoms::types::*;
use crate::engine::channels;
use log::info;
use std::time::Duration;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "n8n_list_workflows".into(),
                description: "List all available n8n workflows. Use this to discover what automations are available before triggering them.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {},
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "n8n_trigger_workflow".into(),
                description: "Trigger an n8n workflow by ID with an optional JSON payload. The workflow will execute in n8n and return the result. Use n8n_list_workflows first to find workflow IDs.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "workflow_id": { "type": "string", "description": "The n8n workflow ID to trigger" },
                        "payload": { "type": "object", "description": "Optional JSON payload to pass to the workflow" }
                    },
                    "required": ["workflow_id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "n8n_execute_action".into(),
                description: "Execute a service action via n8n's workflow engine. For services like Notion, Linear, Jira, HubSpot, Stripe, and 400+ others. Specify the service, action, and parameters. n8n handles authentication using stored credentials.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "service": { "type": "string", "description": "Service name (e.g. 'notion', 'linear', 'jira', 'hubspot', 'stripe', 'airtable')" },
                        "action": { "type": "string", "description": "Action to perform (e.g. 'create_page', 'list_issues', 'send_email', 'get_contacts')" },
                        "params": { "type": "object", "description": "Parameters for the action (service-specific)" }
                    },
                    "required": ["service", "action"]
                }),
            },
        },
    ]
}

pub async fn execute(
    name: &str,
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> Option<Result<String, String>> {
    match name {
        "n8n_list_workflows" | "n8n_trigger_workflow" | "n8n_execute_action" => {}
        _ => return None,
    }

    let config = match load_n8n_config(app_handle) {
        Ok(c) => c,
        Err(e) => return Some(Err(e)),
    };

    Some(match name {
        "n8n_list_workflows" => execute_list_workflows(&config).await,
        "n8n_trigger_workflow" => execute_trigger_workflow(args, &config).await,
        "n8n_execute_action" => execute_action(args, &config, app_handle).await,
        _ => unreachable!(),
    })
}

// ── Helpers ────────────────────────────────────────────────────────────

struct N8nConnection {
    url: String,
    api_key: String,
}

fn load_n8n_config(app_handle: &tauri::AppHandle) -> Result<N8nConnection, String> {
    #[derive(serde::Deserialize, Default)]
    struct Cfg {
        #[serde(default)]
        url: String,
        #[serde(default)]
        api_key: String,
    }
    let cfg: Cfg = channels::load_channel_config(app_handle, "n8n_config")
        .map_err(|_| "n8n is not configured. Set up n8n in Settings → Integrations first.".to_string())?;

    if cfg.url.is_empty() || cfg.api_key.is_empty() {
        return Err("n8n URL or API key is empty. Configure n8n in Settings → Integrations.".into());
    }

    Ok(N8nConnection {
        url: cfg.url.trim_end_matches('/').to_string(),
        api_key: cfg.api_key,
    })
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))
}

// ── Executors ──────────────────────────────────────────────────────────

async fn execute_list_workflows(config: &N8nConnection) -> Result<String, String> {
    info!("[tool:n8n] Listing workflows");

    let endpoint = format!("{}/api/v1/workflows", config.url);
    let resp = client()?
        .get(&endpoint)
        .header("X-N8N-API-KEY", &config.api_key)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| format!("n8n request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("n8n API error (HTTP {}): {}", status, body));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let data = body["data"].as_array();

    match data {
        Some(workflows) if !workflows.is_empty() => {
            let mut output = format!("Found {} workflow(s):\n\n", workflows.len());
            for wf in workflows {
                let id = wf["id"].as_str().or(wf["id"].as_u64().map(|_| "")).unwrap_or("?");
                let name = wf["name"].as_str().unwrap_or("Untitled");
                let active = wf["active"].as_bool().unwrap_or(false);
                let status = if active { "✅ active" } else { "⏸ inactive" };
                output.push_str(&format!("• {} — {} ({})\n", id, name, status));
            }
            Ok(output)
        }
        _ => Ok("No workflows found. Create workflows in the n8n editor first.".into()),
    }
}

async fn execute_trigger_workflow(
    args: &serde_json::Value,
    config: &N8nConnection,
) -> Result<String, String> {
    let workflow_id = args["workflow_id"]
        .as_str()
        .ok_or("n8n_trigger_workflow: missing 'workflow_id'")?;
    let payload = args.get("payload").cloned().unwrap_or(serde_json::json!({}));

    info!("[tool:n8n] Triggering workflow {} with payload", workflow_id);

    let endpoint = format!("{}/api/v1/workflows/{}/execute", config.url, workflow_id);
    let resp = client()?
        .post(&endpoint)
        .header("X-N8N-API-KEY", &config.api_key)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("n8n trigger failed: {}", e))?;

    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();

    if status >= 400 {
        return Err(format!("n8n trigger error (HTTP {}): {}", status, body));
    }

    let truncated = if body.len() > 30_000 {
        format!(
            "{}...\n[truncated, {} total bytes]",
            &body[..30_000],
            body.len()
        )
    } else {
        body
    };

    Ok(format!(
        "Workflow {} executed successfully (HTTP {})\n\n{}",
        workflow_id, status, truncated
    ))
}

async fn execute_action(
    args: &serde_json::Value,
    config: &N8nConnection,
    app_handle: &tauri::AppHandle,
) -> Result<String, String> {
    let service = args["service"]
        .as_str()
        .ok_or("n8n_execute_action: missing 'service'")?;
    let action = args["action"]
        .as_str()
        .ok_or("n8n_execute_action: missing 'action'")?;
    let params = args.get("params").cloned().unwrap_or(serde_json::json!({}));

    info!(
        "[tool:n8n] Executing action {}.{} via n8n",
        service, action
    );

    // Load stored credentials for this service
    let cred_key = format!("integration_creds_{}", service);
    let creds: std::collections::HashMap<String, String> =
        channels::load_channel_config(app_handle, &cred_key).unwrap_or_default();

    // Map service + action to a mini n8n workflow execution
    // We use n8n's /api/v1/workflows/run endpoint to execute a dynamic workflow
    let node_type = map_service_to_node_type(service);

    // Build a minimal single-node workflow for execution
    let workflow_payload = serde_json::json!({
        "workflowData": {
            "nodes": [
                {
                    "parameters": build_node_params(service, action, &params, &creds),
                    "name": format!("{} - {}", service, action),
                    "type": node_type,
                    "typeVersion": 1,
                    "position": [250, 300]
                }
            ],
            "connections": {}
        }
    });

    let endpoint = format!("{}/api/v1/workflows/run", config.url);
    let resp = client()?
        .post(&endpoint)
        .header("X-N8N-API-KEY", &config.api_key)
        .header("Content-Type", "application/json")
        .json(&workflow_payload)
        .send()
        .await
        .map_err(|e| format!("n8n execution failed: {}", e))?;

    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();

    // If the dynamic workflow execution fails (e.g. old n8n version),
    // fall back to REST API tool if credentials are available
    if status >= 400 {
        if !creds.is_empty() {
            return fallback_rest_call(service, action, &params, &creds).await;
        }
        return Err(format!(
            "n8n action {}.{} failed (HTTP {}): {}",
            service, action, status, body
        ));
    }

    let truncated = if body.len() > 30_000 {
        format!(
            "{}...\n[truncated, {} total bytes]",
            &body[..30_000],
            body.len()
        )
    } else {
        body
    };

    // Touch the service's last_used timestamp
    let _ = crate::commands::integrations::engine_integrations_touch(
        app_handle.clone(),
        service.to_string(),
    );

    Ok(format!(
        "n8n {}.{} executed (HTTP {})\n\n{}",
        service, action, status, truncated
    ))
}

// ── Service → n8n node type mapping ────────────────────────────────────

fn map_service_to_node_type(service: &str) -> &'static str {
    match service {
        "slack" => "n8n-nodes-base.slack",
        "discord" => "n8n-nodes-base.discord",
        "github" => "n8n-nodes-base.github",
        "gmail" | "email" => "n8n-nodes-base.gmail",
        "notion" => "n8n-nodes-base.notion",
        "linear" => "n8n-nodes-base.linear",
        "jira" => "n8n-nodes-base.jira",
        "stripe" => "n8n-nodes-base.stripe",
        "todoist" => "n8n-nodes-base.todoist",
        "clickup" => "n8n-nodes-base.clickUp",
        "airtable" => "n8n-nodes-base.airtable",
        "trello" => "n8n-nodes-base.trello",
        "telegram" => "n8n-nodes-base.telegram",
        "sendgrid" => "n8n-nodes-base.sendGrid",
        "hubspot" => "n8n-nodes-base.hubspot",
        "zendesk" => "n8n-nodes-base.zendesk",
        "twilio" => "n8n-nodes-base.twilio",
        "salesforce" => "n8n-nodes-base.salesforce",
        "asana" => "n8n-nodes-base.asana",
        "monday" => "n8n-nodes-base.mondayCom",
        "google-sheets" => "n8n-nodes-base.googleSheets",
        "google-drive" => "n8n-nodes-base.googleDrive",
        "google-calendar" => "n8n-nodes-base.googleCalendar",
        "dropbox" => "n8n-nodes-base.dropbox",
        "mailchimp" => "n8n-nodes-base.mailchimp",
        "intercom" => "n8n-nodes-base.intercom",
        "freshdesk" => "n8n-nodes-base.freshdesk",
        "shopify" => "n8n-nodes-base.shopify",
        "woocommerce" => "n8n-nodes-base.wooCommerce",
        "wordpress" => "n8n-nodes-base.wordpress",
        "confluence" => "n8n-nodes-base.confluence",
        "bitbucket" => "n8n-nodes-base.bitbucket",
        "gitlab" => "n8n-nodes-base.gitlab",
        "mattermost" => "n8n-nodes-base.mattermost",
        "pagerduty" => "n8n-nodes-base.pagerDuty",
        "datadog" => "n8n-nodes-base.datadog",
        "sentry" => "n8n-nodes-base.sentry",
        _ => "n8n-nodes-base.httpRequest",
    }
}

fn build_node_params(
    _service: &str,
    action: &str,
    params: &serde_json::Value,
    _creds: &std::collections::HashMap<String, String>,
) -> serde_json::Value {
    // Build generic node parameters from the action + params
    let mut node_params = serde_json::json!({
        "operation": action,
    });

    // Merge user-provided params into node parameters
    if let Some(obj) = params.as_object() {
        if let Some(map) = node_params.as_object_mut() {
            for (k, v) in obj {
                map.insert(k.clone(), v.clone());
            }
        }
    }

    node_params
}

/// Fallback: use direct REST API calls when n8n workflow execution isn't available
async fn fallback_rest_call(
    service: &str,
    action: &str,
    params: &serde_json::Value,
    creds: &std::collections::HashMap<String, String>,
) -> Result<String, String> {
    info!(
        "[tool:n8n] Falling back to direct REST for {}.{}",
        service, action
    );

    let (base_url, auth_header, auth_value) = match service {
        "notion" => {
            let token = creds.get("api_key").or(creds.get("access_token")).cloned().unwrap_or_default();
            ("https://api.notion.com/v1".to_string(), "Authorization", format!("Bearer {}", token))
        }
        "linear" => {
            let token = creds.get("api_key").cloned().unwrap_or_default();
            ("https://api.linear.app".to_string(), "Authorization", format!("Bearer {}", token))
        }
        "todoist" => {
            let token = creds.get("api_token").or(creds.get("api_key")).cloned().unwrap_or_default();
            ("https://api.todoist.com/rest/v2".to_string(), "Authorization", format!("Bearer {}", token))
        }
        "clickup" => {
            let token = creds.get("api_key").cloned().unwrap_or_default();
            ("https://api.clickup.com/api/v2".to_string(), "Authorization", format!("Bearer {}", token))
        }
        "airtable" => {
            let token = creds.get("api_key").cloned().unwrap_or_default();
            ("https://api.airtable.com/v0".to_string(), "Authorization", format!("Bearer {}", token))
        }
        "sendgrid" => {
            let token = creds.get("api_key").cloned().unwrap_or_default();
            ("https://api.sendgrid.com/v3".to_string(), "Authorization", format!("Bearer {}", token))
        }
        "hubspot" => {
            let token = creds.get("access_token").or(creds.get("api_key")).cloned().unwrap_or_default();
            ("https://api.hubapi.com".to_string(), "Authorization", format!("Bearer {}", token))
        }
        _ => {
            return Err(format!(
                "No REST API fallback available for service '{}'. Configure n8n for full support.",
                service
            ));
        }
    };

    // Map action to HTTP method + path
    let (method, path) = map_action_to_rest(service, action, params);
    let url = format!("{}{}", base_url, path);

    info!("[tool:n8n:fallback] {} {}", method, url);

    let c = client()?;
    let mut request = match method {
        "POST" => c.post(&url),
        "PUT" => c.put(&url),
        "PATCH" => c.patch(&url),
        "DELETE" => c.delete(&url),
        _ => c.get(&url),
    };

    request = request
        .header(auth_header, &auth_value)
        .header("Accept", "application/json");

    // Add Notion-specific headers
    if service == "notion" {
        request = request.header("Notion-Version", "2022-06-28");
    }

    // Add body for write operations
    if matches!(method, "POST" | "PUT" | "PATCH") && !params.is_null() {
        request = request
            .header("Content-Type", "application/json")
            .json(params);
    }

    let resp = request
        .send()
        .await
        .map_err(|e| format!("REST call failed: {}", e))?;

    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();

    let truncated = if body.len() > 30_000 {
        format!(
            "{}...\n[truncated, {} total bytes]",
            &body[..30_000],
            body.len()
        )
    } else {
        body
    };

    if status >= 400 {
        Err(format!(
            "{} {} → HTTP {}\n{}",
            method, path, status, truncated
        ))
    } else {
        Ok(format!(
            "{}.{} → HTTP {}\n\n{}",
            service, action, status, truncated
        ))
    }
}

/// Map a semantic action name to (HTTP_METHOD, path)
fn map_action_to_rest(service: &str, action: &str, params: &serde_json::Value) -> (&'static str, String) {
    let id = params.get("id").and_then(|v| v.as_str()).unwrap_or("");

    match (service, action) {
        // Notion
        ("notion", "search") => ("POST", "/search".into()),
        ("notion", "list_databases") => ("POST", "/search".into()),
        ("notion", "get_page") => ("GET", format!("/pages/{}", id)),
        ("notion", "create_page") => ("POST", "/pages".into()),
        ("notion", "update_page") => ("PATCH", format!("/pages/{}", id)),
        ("notion", "get_database") => ("GET", format!("/databases/{}", id)),
        ("notion", "query_database") => ("POST", format!("/databases/{}/query", id)),

        // Linear
        ("linear", "list_issues") | ("linear", "get_issues") => ("POST", "/graphql".into()),
        ("linear", "create_issue") => ("POST", "/graphql".into()),

        // Todoist
        ("todoist", "list_projects") => ("GET", "/projects".into()),
        ("todoist", "list_tasks") | ("todoist", "get_tasks") => ("GET", "/tasks".into()),
        ("todoist", "create_task") => ("POST", "/tasks".into()),
        ("todoist", "close_task") | ("todoist", "complete_task") => ("POST", format!("/tasks/{}/close", id)),

        // ClickUp
        ("clickup", "get_teams") | ("clickup", "list_teams") => ("GET", "/team".into()),
        ("clickup", "list_tasks") => {
            let list_id = params.get("list_id").and_then(|v| v.as_str()).unwrap_or("");
            ("GET", format!("/list/{}/task", list_id))
        }

        // Airtable
        ("airtable", "list_records") => {
            let base_id = params.get("base_id").and_then(|v| v.as_str()).unwrap_or("");
            let table = params.get("table").and_then(|v| v.as_str()).unwrap_or("");
            ("GET", format!("/{}/{}", base_id, table))
        }

        // HubSpot
        ("hubspot", "list_contacts") | ("hubspot", "get_contacts") => ("GET", "/crm/v3/objects/contacts".into()),
        ("hubspot", "list_deals") | ("hubspot", "get_deals") => ("GET", "/crm/v3/objects/deals".into()),
        ("hubspot", "create_contact") => ("POST", "/crm/v3/objects/contacts".into()),
        ("hubspot", "create_deal") => ("POST", "/crm/v3/objects/deals".into()),

        // SendGrid
        ("sendgrid", "send_email") | ("sendgrid", "send") => ("POST", "/mail/send".into()),

        // Generic fallback
        (_, a) if a.starts_with("list") || a.starts_with("get") => {
            let resource = a.trim_start_matches("list_").trim_start_matches("get_");
            ("GET", format!("/{}", resource))
        }
        (_, a) if a.starts_with("create") => {
            let resource = a.trim_start_matches("create_");
            ("POST", format!("/{}", resource))
        }
        (_, a) if a.starts_with("update") => {
            let resource = a.trim_start_matches("update_");
            ("PATCH", format!("/{}/{}", resource, id))
        }
        (_, a) if a.starts_with("delete") => {
            let resource = a.trim_start_matches("delete_");
            ("DELETE", format!("/{}/{}", resource, id))
        }
        _ => ("GET", format!("/{}", action)),
    }
}
