// Paw Agent Engine — Agent squad management tools
//
// Squads are named groups of agents that can collaborate on shared goals.
// Unlike projects (boss/worker hierarchy), squads are peer-to-peer teams.

use crate::atoms::types::*;
use crate::engine::state::EngineState;
use log::info;
use tauri::Emitter;
use tauri::Manager;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "create_squad".into(),
                description: "Create a named squad (team) of agents with a shared goal. Squads enable peer-to-peer collaboration.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Squad name (e.g. 'Research Team', 'DevOps Crew')" },
                        "goal": { "type": "string", "description": "High-level goal for the squad" },
                        "agent_ids": { "type": "array", "items": { "type": "string" }, "description": "Agent IDs to add as members" },
                        "coordinator": { "type": "string", "description": "Agent ID of the coordinator (optional, first member if omitted)" }
                    },
                    "required": ["name", "goal", "agent_ids"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "list_squads".into(),
                description: "List all agent squads with their members and goals.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "manage_squad".into(),
                description: "Add/remove members, update the goal, or disband a squad.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "squad_id": { "type": "string", "description": "Squad ID to manage" },
                        "action": { "type": "string", "enum": ["add_member", "remove_member", "update", "disband"], "description": "Action to perform" },
                        "agent_id": { "type": "string", "description": "Agent ID (for add/remove member)" },
                        "role": { "type": "string", "enum": ["coordinator", "member"], "description": "Role for new member (default: member)" },
                        "goal": { "type": "string", "description": "New goal (for action=update)" },
                        "name": { "type": "string", "description": "New name (for action=update)" }
                    },
                    "required": ["squad_id", "action"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "squad_broadcast".into(),
                description: "Send a message to all members of a squad. Uses the inter-agent messaging system.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "squad_id": { "type": "string", "description": "Squad to broadcast to" },
                        "content": { "type": "string", "description": "Message content" },
                        "channel": { "type": "string", "description": "Channel name (default: squad name)" }
                    },
                    "required": ["squad_id", "content"]
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
        "create_squad"    => exec_create(args, app_handle, agent_id),
        "list_squads"     => exec_list(app_handle),
        "manage_squad"    => exec_manage(args, app_handle),
        "squad_broadcast" => exec_broadcast(args, app_handle, agent_id),
        _ => return None,
    })
}

fn exec_create(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    _agent_id: &str,
) -> Result<String, String> {
    let name = args["name"].as_str().ok_or_else(|| "missing 'name'".to_string())?;
    let goal = args["goal"].as_str().ok_or_else(|| "missing 'goal'".to_string())?;
    let agent_ids: Vec<String> = args["agent_ids"].as_array()
        .ok_or_else(|| "missing 'agent_ids'".to_string())?
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    let coordinator = args["coordinator"].as_str()
        .unwrap_or_else(|| agent_ids.first().map(|s| s.as_str()).unwrap_or("default"));

    let state = app_handle.try_state::<EngineState>()
        .ok_or_else(|| "Engine state not available".to_string())?;

    let id = uuid::Uuid::new_v4().to_string();
    let members: Vec<SquadMember> = agent_ids.iter().map(|aid| {
        SquadMember {
            agent_id: aid.clone(),
            role: if aid == coordinator { "coordinator".into() } else { "member".into() },
        }
    }).collect();

    let squad = Squad {
        id: id.clone(),
        name: name.to_string(),
        goal: goal.to_string(),
        status: "active".into(),
        members: members.clone(),
        created_at: String::new(),
        updated_at: String::new(),
    };

    state.store.create_squad(&squad).map_err(|e| e.to_string())?;

    info!("[engine] create_squad: '{}' with {} members", name, agent_ids.len());
    app_handle.emit("squad-updated", serde_json::json!({ "squad_id": id })).ok();

    let member_list: Vec<String> = members.iter()
        .map(|m| format!("{} ({})", m.agent_id, m.role))
        .collect();

    Ok(format!(
        "Squad created!\n\n- **ID**: {}\n- **Name**: {}\n- **Goal**: {}\n- **Members**: {}",
        id, name, goal, member_list.join(", ")
    ))
}

fn exec_list(
    app_handle: &tauri::AppHandle,
) -> Result<String, String> {
    let state = app_handle.try_state::<EngineState>()
        .ok_or_else(|| "Engine state not available".to_string())?;

    let squads = state.store.list_squads().map_err(|e| e.to_string())?;
    if squads.is_empty() {
        return Ok("No squads found. Use `create_squad` to form a team.".into());
    }

    let mut output = format!("{} squad(s):\n\n", squads.len());
    for s in &squads {
        let members: Vec<String> = s.members.iter()
            .map(|m| format!("{} ({})", m.agent_id, m.role))
            .collect();
        output.push_str(&format!(
            "---\n**{}** (ID: `{}`)\n- Goal: {}\n- Status: {}\n- Members: {}\n\n",
            s.name, s.id, s.goal, s.status, members.join(", ")
        ));
    }
    Ok(output)
}

fn exec_manage(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
) -> Result<String, String> {
    let squad_id = args["squad_id"].as_str().ok_or_else(|| "missing 'squad_id'".to_string())?;
    let action = args["action"].as_str().ok_or_else(|| "missing 'action'".to_string())?;

    let state = app_handle.try_state::<EngineState>()
        .ok_or_else(|| "Engine state not available".to_string())?;

    match action {
        "add_member" => {
            let aid = args["agent_id"].as_str().ok_or_else(|| "missing 'agent_id'".to_string())?;
            let role = args["role"].as_str().unwrap_or("member");
            state.store.add_squad_member(squad_id, &SquadMember {
                agent_id: aid.to_string(),
                role: role.to_string(),
            }).map_err(|e| e.to_string())?;
            app_handle.emit("squad-updated", serde_json::json!({ "squad_id": squad_id })).ok();
            Ok(format!("Added {} to squad as {}", aid, role))
        }
        "remove_member" => {
            let aid = args["agent_id"].as_str().ok_or_else(|| "missing 'agent_id'".to_string())?;
            state.store.remove_squad_member(squad_id, aid).map_err(|e| e.to_string())?;
            app_handle.emit("squad-updated", serde_json::json!({ "squad_id": squad_id })).ok();
            Ok(format!("Removed {} from squad", aid))
        }
        "update" => {
            let squads = state.store.list_squads().map_err(|e| e.to_string())?;
            if let Some(mut s) = squads.into_iter().find(|s| s.id == squad_id) {
                if let Some(g) = args["goal"].as_str() { s.goal = g.to_string(); }
                if let Some(n) = args["name"].as_str() { s.name = n.to_string(); }
                state.store.update_squad(&s).map_err(|e| e.to_string())?;
                app_handle.emit("squad-updated", serde_json::json!({ "squad_id": squad_id })).ok();
                Ok(format!("Squad '{}' updated", s.name))
            } else {
                Err(format!("Squad not found: {}", squad_id))
            }
        }
        "disband" => {
            state.store.delete_squad(squad_id).map_err(|e| e.to_string())?;
            app_handle.emit("squad-updated", serde_json::json!({ "squad_id": squad_id })).ok();
            Ok(format!("Squad {} disbanded", squad_id))
        }
        other => Err(format!("Unknown action: {}", other)),
    }
}

fn exec_broadcast(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> Result<String, String> {
    let squad_id = args["squad_id"].as_str().ok_or_else(|| "missing 'squad_id'".to_string())?;
    let content = args["content"].as_str().ok_or_else(|| "missing 'content'".to_string())?;

    let state = app_handle.try_state::<EngineState>()
        .ok_or_else(|| "Engine state not available".to_string())?;

    let squads = state.store.list_squads().map_err(|e| e.to_string())?;
    let squad = squads.iter().find(|s| s.id == squad_id)
        .ok_or_else(|| format!("Squad not found: {}", squad_id))?;

    let channel = args["channel"].as_str()
        .unwrap_or(&squad.name);

    let mut sent = 0;
    for m in &squad.members {
        if m.agent_id == agent_id { continue; }
        let msg = AgentMessage {
            id: uuid::Uuid::new_v4().to_string(),
            from_agent: agent_id.to_string(),
            to_agent: m.agent_id.clone(),
            channel: channel.to_string(),
            content: content.to_string(),
            metadata: Some(serde_json::json!({ "squad_id": squad_id }).to_string()),
            read: false,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        state.store.send_agent_message(&msg).map_err(|e| e.to_string())?;

        // Emit frontend event so UI can update in real time
        app_handle.emit("agent-message", serde_json::json!({
            "from": agent_id,
            "to": m.agent_id,
            "channel": channel,
            "squad_id": squad_id,
        })).ok();

        // Fire event-driven triggers (same as agent_send_message)
        let event = crate::engine::events::EngineEvent::AgentMessage {
            from_agent: agent_id.to_string(),
            to_agent: m.agent_id.clone(),
            channel: channel.to_string(),
            content: content.to_string(),
        };
        let app_clone = app_handle.clone();
        tauri::async_runtime::spawn(async move {
            crate::engine::events::dispatch_event(&app_clone, &event).await;
        });

        sent += 1;
    }

    // Emit squad-updated so the squad detail view refreshes messages
    app_handle.emit("squad-updated", serde_json::json!({ "squad_id": squad_id })).ok();

    info!("[engine] squad_broadcast: {} → {} members of '{}'", agent_id, sent, squad.name);
    Ok(format!("Broadcast sent to {} members of squad '{}'", sent, squad.name))
}
