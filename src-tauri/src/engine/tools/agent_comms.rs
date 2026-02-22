// Paw Agent Engine — Inter-agent communication tools
//
// Lets any agent send direct messages to other agents, check their inbox,
// and broadcast to all agents. Independent of the project/orchestrator system.

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
                name: "agent_send_message".into(),
                description: "Send a direct message to another agent. Use 'broadcast' as to_agent to message all agents. Messages persist and can be read by the recipient later.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "to_agent": { "type": "string", "description": "Target agent ID, or 'broadcast' for all agents" },
                        "content": { "type": "string", "description": "Message content" },
                        "channel": { "type": "string", "description": "Topic channel (default: 'general'). Use channels like 'alerts', 'status', 'handoff' to organize messages." },
                        "metadata": { "type": "string", "description": "Optional JSON metadata for structured data" }
                    },
                    "required": ["to_agent", "content"]
                }),
            },
        },
        ToolDefinition {
            tool_type: "function".into(),
            function: FunctionDefinition {
                name: "agent_read_messages".into(),
                description: "Read your incoming messages from other agents. Returns unread messages first. Optionally filter by channel.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "channel": { "type": "string", "description": "Filter by channel (e.g. 'alerts'). Omit to see all channels." },
                        "limit": { "type": "integer", "description": "Max messages to return (default: 20)" },
                        "mark_read": { "type": "boolean", "description": "Mark messages as read after retrieval (default: true)" }
                    }
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
        "agent_send_message" => execute_send(args, app_handle, agent_id),
        "agent_read_messages" => execute_read(args, app_handle, agent_id),
        _ => return None,
    })
}

fn execute_send(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> Result<String, String> {
    let to = args["to_agent"].as_str().ok_or_else(|| "missing 'to_agent'".to_string())?;
    let content = args["content"].as_str().ok_or_else(|| "missing 'content'".to_string())?;
    let channel = args["channel"].as_str().unwrap_or("general");
    let metadata = args["metadata"].as_str().map(String::from);

    let state = app_handle.try_state::<EngineState>()
        .ok_or_else(|| "Engine state not available".to_string())?;

    let msg = AgentMessage {
        id: uuid::Uuid::new_v4().to_string(),
        from_agent: agent_id.to_string(),
        to_agent: to.to_string(),
        channel: channel.to_string(),
        content: content.to_string(),
        metadata,
        read: false,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    state.store.send_agent_message(&msg).map_err(|e| e.to_string())?;

    info!(
        "[engine] agent_send_message: {} → {} on #{} ({} chars)",
        agent_id, to, channel, content.len()
    );

    app_handle.emit("agent-message", serde_json::json!({
        "from": agent_id,
        "to": to,
        "channel": channel,
    })).ok();

    // Fire event-driven triggers for agent messages
    let event = crate::engine::events::EngineEvent::AgentMessage {
        from_agent: agent_id.to_string(),
        to_agent: to.to_string(),
        channel: channel.to_string(),
        content: content.to_string(),
    };
    let app_clone = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        crate::engine::events::dispatch_event(&app_clone, &event).await;
    });

    Ok(format!("Message sent to {} on #{}", to, channel))
}

fn execute_read(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> Result<String, String> {
    let channel = args["channel"].as_str();
    let limit = args["limit"].as_i64().unwrap_or(20);
    let mark_read = args["mark_read"].as_bool().unwrap_or(true);

    let state = app_handle.try_state::<EngineState>()
        .ok_or_else(|| "Engine state not available".to_string())?;

    let msgs = state.store.get_agent_messages(agent_id, channel, limit).map_err(|e| e.to_string())?;

    if mark_read {
        state.store.mark_agent_messages_read(agent_id).map_err(|e| e.to_string())?;
    }

    if msgs.is_empty() {
        let ch_info = channel.map(|c| format!(" on #{}", c)).unwrap_or_default();
        return Ok(format!("No messages{}", ch_info));
    }

    let mut output = format!("{} message(s):\n\n", msgs.len());
    for m in &msgs {
        let read_marker = if m.read { "" } else { " [NEW]" };
        output.push_str(&format!(
            "**From**: {} | **Channel**: #{} | {}{}\n{}\n\n",
            m.from_agent, m.channel, m.created_at, read_marker, m.content
        ));
    }

    Ok(output)
}
