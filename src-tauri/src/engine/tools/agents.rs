// Paw Agent Engine — Agent management tools

use crate::atoms::types::*;
use crate::engine::state::EngineState;
use crate::engine::memory;
use log::info;
use tauri::Emitter;
use tauri::Manager;
use crate::atoms::error::EngineResult;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "self_info".into(),
                description: "Get information about yourself: your configuration, enabled skills, available tools, memory settings, and current context.".into(),
                parameters: serde_json::json!({ "type": "object", "properties": {}, "required": [] }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "update_profile".into(),
                description: "Update your own profile: name, avatar, bio, or system prompt. Changes take effect immediately.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Agent ID to update (use 'default' for the main agent)" },
                        "name": { "type": "string", "description": "New display name" },
                        "avatar": { "type": "string", "description": "New avatar URL or emoji" },
                        "bio": { "type": "string", "description": "Short bio / tagline" },
                        "system_prompt": { "type": "string", "description": "Updated system prompt / persona" }
                    },
                    "required": ["agent_id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "create_agent".into(),
                description: "Create a new AI agent with a name, role, and system prompt. The agent will appear in the Agents view.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Name for the new agent" },
                        "role": { "type": "string", "description": "Agent's role (e.g. 'researcher', 'writer', 'coder')" },
                        "system_prompt": { "type": "string", "description": "Full system prompt / persona for the agent" },
                        "specialty": { "type": "string", "description": "Agent's specialty (e.g. 'crypto', 'marketing', 'general')" },
                        "model": { "type": "string", "description": "Model to use (optional, defaults to project default)" },
                        "capabilities": { "type": "array", "items": { "type": "string" }, "description": "List of tool names this agent can use (empty = all tools)" }
                    },
                    "required": ["name", "role", "system_prompt"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "agent_list".into(),
                description: "List all agents in the system with their roles, models, and skill counts. Only available to orchestrator/boss agents.".into(),
                parameters: serde_json::json!({ "type": "object", "properties": {}, "required": [] }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "agent_skills".into(),
                description: "View community skills assigned to a specific agent.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "The agent ID to inspect" }
                    },
                    "required": ["agent_id"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "agent_skill_assign".into(),
                description: "Add or remove a community skill from a specific agent.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "skill_id": { "type": "string", "description": "The community skill ID" },
                        "agent_id": { "type": "string", "description": "The agent ID to assign/unassign the skill to/from" },
                        "action": { "type": "string", "enum": ["add", "remove"], "description": "'add' to give the agent this skill, 'remove' to take it away" }
                    },
                    "required": ["skill_id", "agent_id", "action"]
                }),
            },
        },
    ]
}

pub async fn execute(
    name: &str,
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    _agent_id: &str,
) -> Option<Result<String, String>> {
    Some(match name {
        "self_info"          => execute_self_info(app_handle).await.map_err(|e| e.to_string()),
        "update_profile"     => execute_update_profile(args, app_handle).await.map_err(|e| e.to_string()),
        "create_agent"       => execute_create_agent(args, app_handle).await.map_err(|e| e.to_string()),
        "agent_list"         => execute_agent_list(app_handle).await.map_err(|e| e.to_string()),
        "agent_skills"       => execute_agent_skills(args, app_handle).await.map_err(|e| e.to_string()),
        "agent_skill_assign" => execute_agent_skill_assign(args, app_handle).await.map_err(|e| e.to_string()),
        _ => return None,
    })
}

async fn execute_self_info(app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let cfg = state.config.lock();
    let mcfg = state.memory_config.lock();

    let providers_info: Vec<String> = cfg.providers.iter().map(|p| {
        let is_default = cfg.default_provider.as_ref() == Some(&p.id);
        format!("  - {} ({:?}){}", p.id, p.kind, if is_default { " <- DEFAULT" } else { "" })
    }).collect();

    let routing = &cfg.model_routing;
    let routing_info = format!(
        "  Boss model: {}\n  Worker model: {}\n  Specialties: {}\n  Per-agent overrides: {}",
        routing.boss_model.as_deref().unwrap_or("(default)"),
        routing.worker_model.as_deref().unwrap_or("(default)"),
        if routing.specialty_models.is_empty() { "none".into() }
        else { routing.specialty_models.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>().join(", ") },
        if routing.agent_models.is_empty() { "none".into() }
        else { routing.agent_models.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>().join(", ") },
    );

    let memory_info = format!(
        "  Embedding provider: {}\n  Embedding model: {}\n  Auto-recall: {}\n  Auto-capture: {}\n  Recall limit: {}",
        mcfg.embedding_base_url,
        if mcfg.embedding_model.is_empty() { "(not configured)" } else { &mcfg.embedding_model },
        mcfg.auto_recall,
        mcfg.auto_capture,
        mcfg.recall_limit,
    );

    let skills_list = crate::engine::skills::builtin_skills();
    let enabled_skills: Vec<String> = skills_list.iter()
        .filter(|s| state.store.is_skill_enabled(&s.id).unwrap_or(false))
        .map(|s| format!("  - {} ({})", s.name, s.id))
        .collect();

    Ok(format!(
        "# Paw Engine Self-Info\n\n\
        ## Current Configuration\n\
        - Default model: {}\n\
        - Default provider: {}\n\
        - Max tool rounds: {}\n\
        - Tool timeout: {}s\n\n\
        ## Configured Providers\n{}\n\n\
        ## Model Routing (Orchestrator)\n{}\n\n\
        ## Memory Configuration\n{}\n\n\
        ## Enabled Skills\n{}\n\n\
        ## Data Location\n\
        - Config stored in: SQLite database (engine_config key)\n\
        - Soul files: stored in SQLite (agent_files table)\n\
        - Memories: stored in SQLite (memories table)\n\
        - Sessions: stored in SQLite (sessions + messages tables)",
        cfg.default_model.as_deref().unwrap_or("(not set)"),
        cfg.default_provider.as_deref().unwrap_or("(not set)"),
        cfg.max_tool_rounds,
        cfg.tool_timeout_secs,
        if providers_info.is_empty() { "  (none configured)".into() } else { providers_info.join("\n") },
        routing_info,
        memory_info,
        if enabled_skills.is_empty() { "  (none enabled)".into() } else { enabled_skills.join("\n") },
    ))
}

async fn execute_update_profile(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> EngineResult<String> {
    let agent_id = args["agent_id"].as_str()
        .ok_or("update_profile: missing 'agent_id' argument (use 'default' for the main agent)")?;

    let name = args["name"].as_str();
    let avatar = args["avatar"].as_str();
    let bio = args["bio"].as_str();
    let system_prompt = args["system_prompt"].as_str();

    if name.is_none() && avatar.is_none() && bio.is_none() && system_prompt.is_none() {
        return Err("update_profile: provide at least one field to update (name, avatar, bio, system_prompt)".into());
    }

    let mut updates = serde_json::Map::new();
    updates.insert("agent_id".into(), serde_json::json!(agent_id));
    if let Some(v) = name { updates.insert("name".into(), serde_json::json!(v)); }
    if let Some(v) = avatar { updates.insert("avatar".into(), serde_json::json!(v)); }
    if let Some(v) = bio { updates.insert("bio".into(), serde_json::json!(v)); }
    if let Some(v) = system_prompt { updates.insert("system_prompt".into(), serde_json::json!(v)); }

    info!("[engine] update_profile tool: updating agent '{}' with fields: {:?}",
        agent_id, updates.keys().collect::<Vec<_>>());

    let _ = app_handle.emit("agent-profile-updated", serde_json::Value::Object(updates));

    let mut desc_parts = vec![format!("Updated profile for agent '{}':", agent_id)];
    if let Some(v) = name { desc_parts.push(format!("name -> {}", v)); }
    if let Some(v) = avatar { desc_parts.push(format!("avatar -> {}", v)); }
    if let Some(v) = bio { desc_parts.push(format!("bio -> {}", v)); }
    if system_prompt.is_some() { desc_parts.push("system_prompt updated".into()); }
    let memory_content = desc_parts.join(" ");

    let state = app_handle.try_state::<EngineState>();
    if let Some(state) = state {
        let emb_client = state.embedding_client();
        let _ = memory::store_memory(&state.store, &memory_content, "fact", 5, emb_client.as_ref(), None).await;
    }

    let mut result_parts = vec![format!("Successfully updated agent profile for '{}':", agent_id)];
    if let Some(v) = name { result_parts.push(format!("- **Name**: {}", v)); }
    if let Some(v) = avatar { result_parts.push(format!("- **Avatar**: {}", v)); }
    if let Some(v) = bio { result_parts.push(format!("- **Bio**: {}", v)); }
    if system_prompt.is_some() { result_parts.push("- **System Prompt**: updated".into()); }
    result_parts.push("\nThe UI has been updated in real-time.".into());

    Ok(result_parts.join("\n"))
}

async fn execute_create_agent(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> EngineResult<String> {
    let name = args["name"].as_str().ok_or("create_agent: missing 'name'")?;
    let role = args["role"].as_str().ok_or("create_agent: missing 'role'")?;
    let system_prompt = args["system_prompt"].as_str().ok_or("create_agent: missing 'system_prompt'")?;
    let specialty = args["specialty"].as_str().unwrap_or("general");
    let model = args["model"].as_str().filter(|s| !s.is_empty());
    let capabilities: Vec<String> = args["capabilities"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let slug: String = name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let agent_id = format!("agent-{}-{}", slug, timestamp);

    info!("[engine] create_agent tool: creating '{}' as {}", name, agent_id);

    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let agent = crate::engine::types::ProjectAgent {
        agent_id: agent_id.clone(),
        role: role.to_string(),
        specialty: specialty.to_string(),
        status: "idle".into(),
        current_task: None,
        model: model.map(String::from),
        system_prompt: Some(system_prompt.to_string()),
        capabilities: capabilities.clone(),
    };

    state.store.add_project_agent("_standalone", &agent)?;

    let memory_content = format!(
        "Created agent '{}' (id: {}, role: {}, specialty: {})",
        name, agent_id, role, specialty
    );
    let emb_client = state.embedding_client();
    let _ = memory::store_memory(&state.store, &memory_content, "fact", 5, emb_client.as_ref(), None).await;

    Ok(format!(
        "Successfully created agent '{}'!\n\n\
        - **Agent ID**: {}\n\
        - **Role**: {}\n\
        - **Specialty**: {}\n\
        - **Model**: {}\n\
        - **Capabilities**: {}\n\n\
        The agent is now available in the Agents view.",
        name, agent_id, role, specialty,
        model.unwrap_or("(uses default)"),
        if capabilities.is_empty() { "all tools".to_string() } else { capabilities.join(", ") }
    ))
}

async fn execute_agent_list(app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let backend_agents = state.store.list_all_agents().unwrap_or_default();
    let all_skills = state.store.list_community_skills().unwrap_or_default();

    let mut skill_counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    let global_count = all_skills.iter().filter(|s| s.agent_ids.is_empty() && s.enabled).count();

    for skill in &all_skills {
        if skill.enabled {
            for aid in &skill.agent_ids {
                *skill_counts.entry(aid.clone()).or_insert(0u64) += 1;
            }
        }
    }

    let mut output = String::from("# Agents in System\n\n");

    let global_count64 = global_count as u64;
    let default_skills = skill_counts.get("default").cloned().unwrap_or(0) + global_count64;
    output.push_str(&format!(
        "1. **Default Agent** (id: `default`)\n   Role: Boss / Main Agent\n   Community skills: {} ({} agent-specific + {} global)\n\n",
        default_skills, skill_counts.get("default").cloned().unwrap_or(0), global_count
    ));

    let mut idx = 2;
    for (_project_id, agent) in &backend_agents {
        let agent_skills = skill_counts.get(&agent.agent_id).cloned().unwrap_or(0) + global_count as u64;
        output.push_str(&format!(
            "{}. **{}** (id: `{}`)\n   Role: {} | Specialty: {}\n   Model: {}\n   Capabilities: {}\n   Community skills: {}\n\n",
            idx,
            agent.agent_id,
            agent.agent_id,
            agent.role,
            agent.specialty,
            agent.model.as_deref().unwrap_or("default"),
            if agent.capabilities.is_empty() { "all".into() } else { agent.capabilities.join(", ") },
            agent_skills,
        ));
        idx += 1;
    }

    output.push_str("**Note**: Some agents may be configured in the frontend only.");
    Ok(output)
}

async fn execute_agent_skills(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> EngineResult<String> {
    let agent_id = args["agent_id"].as_str().ok_or("Missing 'agent_id' parameter")?;

    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let all_skills = state.store.list_community_skills()?;
    let agent_skills: Vec<_> = all_skills.iter().filter(|s| {
        s.agent_ids.is_empty() || s.agent_ids.contains(&agent_id.to_string())
    }).collect();

    if agent_skills.is_empty() {
        return Ok(format!("Agent '{}' has no community skills assigned.\n\nUse skill_search to find skills, then agent_skill_assign to give them to this agent.", agent_id));
    }

    let mut output = format!("# Community Skills for agent '{}'\n\n", agent_id);
    for (i, skill) in agent_skills.iter().enumerate() {
        let status = if skill.enabled { "Enabled" } else { "Disabled" };
        let scope = if skill.agent_ids.is_empty() {
            "Global (all agents)".to_string()
        } else {
            format!("Scoped to: {}", skill.agent_ids.join(", "))
        };
        output.push_str(&format!(
            "{}. **{}** [{}]\n   ID: `{}`\n   {}\n   Scope: {}\n   Source: {}\n\n",
            i + 1, skill.name, status, skill.id,
            if skill.description.is_empty() { "(no description)" } else { &skill.description },
            scope, skill.source,
        ));
    }
    output.push_str("Use agent_skill_assign to add or remove skills from this agent.");
    Ok(output)
}

async fn execute_agent_skill_assign(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> EngineResult<String> {
    let skill_id = args["skill_id"].as_str().ok_or("Missing 'skill_id' parameter")?;
    let agent_id = args["agent_id"].as_str().ok_or("Missing 'agent_id' parameter")?;
    let action = args["action"].as_str().ok_or("Missing 'action' parameter (must be 'add' or 'remove')")?;

    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    let all_skills = state.store.list_community_skills()?;
    let skill = all_skills.iter().find(|s| s.id == skill_id)
        .ok_or_else(|| format!("Skill '{}' not found. Use skill_list to see installed skills.", skill_id))?;

    let mut agent_ids = skill.agent_ids.clone();

    match action {
        "add" => {
            if agent_ids.is_empty() {
                return Ok(format!(
                    "Skill '{}' is already global (available to all agents including '{}').",
                    skill.name, agent_id
                ));
            }
            if agent_ids.contains(&agent_id.to_string()) {
                return Ok(format!("Skill '{}' is already assigned to agent '{}'.", skill.name, agent_id));
            }
            agent_ids.push(agent_id.to_string());
            state.store.set_community_skill_agents(skill_id, &agent_ids)?;
            let _ = app_handle.emit("community-skill-updated", serde_json::json!({ "skill_id": skill_id }));
            Ok(format!("Assigned skill '{}' to agent '{}'.", skill.name, agent_id))
        }
        "remove" => {
            if agent_ids.is_empty() {
                return Ok(format!("Skill '{}' is currently global. Cannot remove from individual agents.", skill.name));
            }
            if !agent_ids.contains(&agent_id.to_string()) {
                return Ok(format!("Skill '{}' is not assigned to agent '{}'.", skill.name, agent_id));
            }
            agent_ids.retain(|id| id != agent_id);
            state.store.set_community_skill_agents(skill_id, &agent_ids)?;
            let _ = app_handle.emit("community-skill-updated", serde_json::json!({ "skill_id": skill_id }));
            Ok(format!("Removed skill '{}' from agent '{}'.", skill.name, agent_id))
        }
        _ => Err(format!("Invalid action '{}'. Must be 'add' or 'remove'.", action).into()),
    }
}
