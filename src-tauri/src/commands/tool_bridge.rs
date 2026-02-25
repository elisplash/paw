// commands/tool_bridge.rs — Service-native tool name remapping & MCP bridge
//
// Phase 5: Remaps raw n8n/MCP tool names to service-native names;
// manages tool discovery lists for agent tool pickers.

use crate::engine::channels;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemappedTool {
    pub name: String,
    #[serde(rename = "originalName")]
    pub original_name: String,
    pub description: String,
    pub service: String,
    #[serde(rename = "serviceName")]
    pub service_name: String,
    pub action: String,
    #[serde(default)]
    pub parameters: Option<serde_json::Value>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceToolSet {
    pub service: String,
    #[serde(rename = "serviceName")]
    pub service_name: String,
    pub tools: Vec<RemappedTool>,
    pub connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolAssignment {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    /// Map of service id → tool names (* for all)
    pub services: HashMap<String, Vec<String>>,
}

// ── Remap rules ────────────────────────────────────────────────────────

struct RemapEntry {
    pattern: &'static str,
    service: &'static str,
    service_name: &'static str,
    action: &'static str,
    description: &'static str,
}

const REMAP_TABLE: &[RemapEntry] = &[
    // Slack
    RemapEntry { pattern: "sendslackmessage", service: "slack", service_name: "Slack", action: "send_message", description: "Send a message to a Slack channel or DM a user" },
    RemapEntry { pattern: "slack_post_message", service: "slack", service_name: "Slack", action: "send_message", description: "Send a message to a Slack channel or DM a user" },
    RemapEntry { pattern: "slack_list_channels", service: "slack", service_name: "Slack", action: "list_channels", description: "List all channels in the Slack workspace" },
    RemapEntry { pattern: "slack_list_users", service: "slack", service_name: "Slack", action: "list_users", description: "List all users in the Slack workspace" },
    // GitHub
    RemapEntry { pattern: "github_create_issue", service: "github", service_name: "GitHub", action: "create_issue", description: "Create a new issue in a GitHub repository" },
    RemapEntry { pattern: "github_list_issues", service: "github", service_name: "GitHub", action: "list_issues", description: "List issues in a GitHub repository" },
    RemapEntry { pattern: "github_create_pr", service: "github", service_name: "GitHub", action: "create_pr", description: "Create a new pull request" },
    RemapEntry { pattern: "github_list_repos", service: "github", service_name: "GitHub", action: "list_repos", description: "List GitHub repositories" },
    RemapEntry { pattern: "github_search_code", service: "github", service_name: "GitHub", action: "search_code", description: "Search code across GitHub repositories" },
    // Gmail
    RemapEntry { pattern: "gmail_send", service: "gmail", service_name: "Gmail", action: "send_email", description: "Send an email via Gmail" },
    RemapEntry { pattern: "gmail_search", service: "gmail", service_name: "Gmail", action: "search_inbox", description: "Search emails in Gmail" },
    RemapEntry { pattern: "gmail_draft", service: "gmail", service_name: "Gmail", action: "create_draft", description: "Create a draft email in Gmail" },
    // HubSpot
    RemapEntry { pattern: "hubspot_list_deals", service: "hubspot", service_name: "HubSpot", action: "list_deals", description: "List deals from HubSpot CRM" },
    RemapEntry { pattern: "hubspot_create_deal", service: "hubspot", service_name: "HubSpot", action: "create_deal", description: "Create a deal in HubSpot CRM" },
    RemapEntry { pattern: "hubspot_list_contacts", service: "hubspot", service_name: "HubSpot", action: "list_contacts", description: "List contacts from HubSpot CRM" },
    // Jira
    RemapEntry { pattern: "jira_create_issue", service: "jira", service_name: "Jira", action: "create_issue", description: "Create a Jira issue/ticket" },
    RemapEntry { pattern: "jira_search", service: "jira", service_name: "Jira", action: "search_issues", description: "Search Jira issues using JQL" },
    // Notion
    RemapEntry { pattern: "notion_create_page", service: "notion", service_name: "Notion", action: "create_page", description: "Create a new page in Notion" },
    RemapEntry { pattern: "notion_search", service: "notion", service_name: "Notion", action: "search", description: "Search pages and databases in Notion" },
    // Trello
    RemapEntry { pattern: "trello_create_card", service: "trello", service_name: "Trello", action: "create_card", description: "Create a card on a Trello board" },
    RemapEntry { pattern: "trello_list_cards", service: "trello", service_name: "Trello", action: "list_cards", description: "List cards from a Trello board" },
    // Stripe
    RemapEntry { pattern: "stripe_list_payments", service: "stripe", service_name: "Stripe", action: "list_payments", description: "List recent Stripe payments" },
    RemapEntry { pattern: "stripe_list_customers", service: "stripe", service_name: "Stripe", action: "list_customers", description: "List Stripe customers" },
    // Shopify
    RemapEntry { pattern: "shopify_list_orders", service: "shopify", service_name: "Shopify", action: "list_orders", description: "List Shopify orders" },
    RemapEntry { pattern: "shopify_list_products", service: "shopify", service_name: "Shopify", action: "list_products", description: "List Shopify products" },
];

fn normalize_name(name: &str) -> String {
    name.to_lowercase()
        .replace("n8n_", "")
        .replace('-', "_")
        .replace(' ', "_")
}

fn remap_tool_name(raw_name: &str) -> Option<&'static RemapEntry> {
    let norm = normalize_name(raw_name);
    REMAP_TABLE.iter().find(|e| norm.contains(e.pattern))
}

// ── Storage ────────────────────────────────────────────────────────────

const ASSIGNMENTS_KEY: &str = "agent_tool_assignments";

fn load_assignments(app: &tauri::AppHandle) -> Vec<AgentToolAssignment> {
    channels::load_channel_config::<Vec<AgentToolAssignment>>(app, ASSIGNMENTS_KEY)
        .unwrap_or_default()
}

fn save_assignments(
    app: &tauri::AppHandle,
    assignments: &[AgentToolAssignment],
) -> Result<(), String> {
    channels::save_channel_config(app, ASSIGNMENTS_KEY, &assignments.to_vec())
        .map_err(|e| e.to_string())
}

// ── Commands ───────────────────────────────────────────────────────────

/// Remap a list of raw MCP/n8n tool names to service-native tools.
#[tauri::command]
pub fn engine_tools_remap(
    raw_names: Vec<String>,
) -> Result<Vec<RemappedTool>, String> {
    let mapped: Vec<RemappedTool> = raw_names
        .iter()
        .map(|name| {
            if let Some(entry) = remap_tool_name(name) {
                RemappedTool {
                    name: format!("{}_{}", entry.service, entry.action),
                    original_name: name.clone(),
                    description: entry.description.to_string(),
                    service: entry.service.to_string(),
                    service_name: entry.service_name.to_string(),
                    action: entry.action.to_string(),
                    parameters: None,
                    source: format!("{} (via Integrations)", entry.service_name),
                }
            } else {
                // Fallback: keep original name
                let service = name
                    .to_lowercase()
                    .replace("n8n_", "")
                    .split('_')
                    .next()
                    .unwrap_or("unknown")
                    .to_string();
                RemappedTool {
                    name: name.clone(),
                    original_name: name.clone(),
                    description: format!("Integration tool: {}", name),
                    service: service.clone(),
                    service_name: capitalize(&service),
                    action: name.clone(),
                    parameters: None,
                    source: "Integrations".to_string(),
                }
            }
        })
        .collect();

    Ok(mapped)
}

/// Group remapped tools by service.
#[tauri::command]
pub fn engine_tools_by_service(
    raw_names: Vec<String>,
    connected_services: Vec<String>,
) -> Result<Vec<ServiceToolSet>, String> {
    let tools = engine_tools_remap(raw_names)?;
    let mut groups: HashMap<String, ServiceToolSet> = HashMap::new();

    for tool in tools {
        let entry = groups.entry(tool.service.clone()).or_insert_with(|| {
            ServiceToolSet {
                service: tool.service.clone(),
                service_name: tool.service_name.clone(),
                tools: vec![],
                connected: connected_services.contains(&tool.service),
            }
        });
        entry.tools.push(tool);
    }

    let mut result: Vec<ServiceToolSet> = groups.into_values().collect();
    result.sort_by(|a, b| {
        b.connected.cmp(&a.connected)
            .then_with(|| a.service_name.cmp(&b.service_name))
    });

    Ok(result)
}

/// Get agent tool assignments.
#[tauri::command]
pub fn engine_tools_get_agent_assignment(
    app_handle: tauri::AppHandle,
    agent_id: String,
) -> Result<AgentToolAssignment, String> {
    let assignments = load_assignments(&app_handle);
    Ok(assignments
        .into_iter()
        .find(|a| a.agent_id == agent_id)
        .unwrap_or_else(|| AgentToolAssignment {
            agent_id,
            services: HashMap::new(),
        }))
}

/// Set agent tool assignment for a specific service.
#[tauri::command]
pub fn engine_tools_set_agent_service(
    app_handle: tauri::AppHandle,
    agent_id: String,
    service: String,
    tool_names: Vec<String>,
) -> Result<(), String> {
    let mut assignments = load_assignments(&app_handle);

    if let Some(existing) = assignments.iter_mut().find(|a| a.agent_id == agent_id) {
        if tool_names.is_empty() {
            existing.services.remove(&service);
        } else {
            existing.services.insert(service, tool_names);
        }
    } else {
        let mut services = HashMap::new();
        if !tool_names.is_empty() {
            services.insert(service, tool_names);
        }
        assignments.push(AgentToolAssignment {
            agent_id,
            services,
        });
    }

    save_assignments(&app_handle, &assignments)
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}
