// Paw Agent Engine — Community skill tools

use crate::atoms::types::*;
use crate::engine::state::EngineState;
use crate::engine::skills;
use log::info;
use tauri::Emitter;
use tauri::Manager;
use crate::atoms::error::EngineResult;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "skill_search".into(),
                description: "Search the Paw community skills registry (like an app store for AI capabilities). Find skills by name, keyword, or category.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query (keywords, skill name, category)" }
                    },
                    "required": ["query"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "skill_install".into(),
                description: "Install a community skill from a GitHub repository. The skill will be available to you immediately after installation.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "source": { "type": "string", "description": "GitHub repository slug (e.g., 'username/repo-name') or skill registry ID" },
                        "path": { "type": "string", "description": "Path within the repo to the skill directory (optional, defaults to root)" }
                    },
                    "required": ["source"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "skill_list".into(),
                description: "List all community skills installed and available to you or a specific agent.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Agent ID to filter by (optional; defaults to your agent)" }
                    },
                    "required": []
                }),
            },
        },
    ]
}

pub async fn execute(
    name: &str,
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> Option<Result<String, String>> {
    Some(match name {
        "skill_search"  => execute_skill_search(args, app_handle).await.map_err(|e| e.to_string()),
        "skill_install" => execute_skill_install(args, app_handle, agent_id).await.map_err(|e| e.to_string()),
        "skill_list"    => execute_skill_list(app_handle, agent_id).await.map_err(|e| e.to_string()),
        _ => return None,
    })
}

fn format_installs(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M installs", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K installs", n as f64 / 1_000.0)
    } else {
        format!("{} installs", n)
    }
}

async fn execute_skill_search(
    args: &serde_json::Value,
    _app_handle: &tauri::AppHandle,
) -> EngineResult<String> {
    let query = args["query"].as_str().ok_or("Missing 'query' parameter")?;

    info!("[engine] skill_search tool: query={}", query);

    let results = skills::search_community_skills(query).await?;

    if results.is_empty() {
        return Ok(format!(
            "No skills found for query: '{}'\n\nYou can install from GitHub directly with skill_install using 'username/repo' format.",
            query
        ));
    }

    let mut output = format!("# Community Skills: '{}'\n\n", query);
    for (i, result) in results.iter().enumerate() {
        output.push_str(&format!(
            "{}. **{}** ({})\n   {}\n   Source: `{}`\n   {}\n\n",
            i + 1,
            result.name, result.id,
            result.description,
            result.source,
            format_installs(result.installs),
        ));
    }
    output.push_str("\nUse skill_install with the source to install a skill.");
    Ok(output)
}

async fn execute_skill_install(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> EngineResult<String> {
    let source = args["source"].as_str().ok_or("Missing 'source' parameter")?;
    let path = args["path"].as_str().unwrap_or("");

    info!("[engine] skill_install tool: source={}, path={}, agent_id={}", source, path, agent_id);

    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let skill = skills::install_community_skill(&state.store, source, path, Some(agent_id)).await?;

    let _ = app_handle.emit("community-skill-installed", serde_json::json!({ "skill_id": skill.id }));

    Ok(format!(
        "Successfully installed skill **{}** (id: `{}`)\n\n\
        - Source: {}\n\
        - Description: {}\n\
        - Assigned to agent: {}\n\n\
        The skill is now available in your tool list.",
        skill.name, skill.id, source,
        if skill.description.is_empty() { "(no description)" } else { &skill.description },
        agent_id,
    ))
}

async fn execute_skill_list(
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> EngineResult<String> {
    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let all_skills = state.store.list_community_skills()?;
    let my_skills: Vec<_> = all_skills.iter().filter(|s| {
        s.agent_ids.is_empty() || s.agent_ids.contains(&agent_id.to_string())
    }).collect();

    if my_skills.is_empty() {
        return Ok(format!(
            "No community skills installed for agent '{}'.\n\nUse skill_search to find skills, then skill_install to add them.",
            agent_id
        ));
    }

    let mut output = format!("# Installed Skills for '{}'\n\n", agent_id);
    for (i, skill) in my_skills.iter().enumerate() {
        let status = if skill.enabled { "Enabled" } else { "Disabled" };
        let scope = if skill.agent_ids.is_empty() {
            "Global (all agents)".to_string()
        } else {
            format!("Scoped to: {}", skill.agent_ids.join(", "))
        };
        output.push_str(&format!(
            "{}. **{}** [{}]\n   ID: `{}`\n   {}\n   Scope: {}\n\n",
            i + 1, skill.name, status, skill.id,
            if skill.description.is_empty() { "(no description)" } else { &skill.description },
            scope,
        ));
    }
    Ok(output)
}
