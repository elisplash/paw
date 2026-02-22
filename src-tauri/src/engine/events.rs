// Paw Agent Engine — Event-driven task triggers
//
// Matches incoming events against task `event_trigger` conditions and
// fires matching tasks. Integrated into:
//   - Webhook server (on inbound request)
//   - Agent messaging (on message delivery)
//   - Cron heartbeat (periodic event check)
//
// Event trigger JSON format:
//   {"type": "webhook"}                           — any inbound webhook
//   {"type": "webhook", "path": "/deploy"}        — specific webhook path
//   {"type": "agent_message", "channel": "alerts"} — message on alerts channel
//   {"type": "agent_message", "from": "monitor"}   — message from specific agent

use crate::engine::state::EngineState;
use log::{info, warn};
use tauri::Manager;

/// An event that can trigger task execution.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    /// An inbound webhook request was received.
    Webhook {
        path: String,
        agent_id: String,
        payload: String,
    },
    /// An inter-agent message was delivered.
    AgentMessage {
        from_agent: String,
        to_agent: String,
        channel: String,
        content: String,
    },
}

/// Check all event-triggered tasks and execute those that match the given event.
/// Returns the IDs of tasks that were triggered.
pub async fn dispatch_event(
    app_handle: &tauri::AppHandle,
    event: &EngineEvent,
) -> Vec<String> {
    let state = match app_handle.try_state::<EngineState>() {
        Some(s) => s,
        None => return vec![],
    };

    let tasks = match state.store.list_tasks() {
        Ok(t) => t,
        Err(e) => {
            warn!("[events] Failed to list tasks: {}", e);
            return vec![];
        }
    };

    let mut triggered = Vec::new();

    for task in &tasks {
        if !task.cron_enabled { continue; }
        let trigger_json = match &task.event_trigger {
            Some(t) if !t.is_empty() => t,
            _ => continue,
        };

        let trigger: serde_json::Value = match serde_json::from_str(trigger_json) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if matches_event(&trigger, event) {
            info!("[events] Event matched task '{}' ({})", task.title, task.id);

            let now = chrono::Utc::now();
            state.store.update_task_cron_run(
                &task.id,
                &now.to_rfc3339(),
                Some(&(now + chrono::Duration::minutes(1)).to_rfc3339()),
            ).ok();

            let aid = uuid::Uuid::new_v4().to_string();
            let event_desc = match event {
                EngineEvent::Webhook { path, .. } => format!("webhook:{}", path),
                EngineEvent::AgentMessage { from_agent, channel, .. } =>
                    format!("agent_message:{}#{}", from_agent, channel),
            };
            state.store.add_task_activity(
                &aid, &task.id, "event_triggered", None,
                &format!("Event-triggered: {}", event_desc),
            ).ok();

            let task_id = task.id.clone();
            let app = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                let state_inner = app.state::<EngineState>();
                if let Err(e) = crate::commands::task::execute_task(&app, &state_inner, &task_id).await {
                    warn!("[events] Failed to execute event-triggered task {}: {}", task_id, e);
                }
            });

            triggered.push(task.id.clone());
        }
    }

    triggered
}

/// Check if an event trigger condition matches an incoming event.
fn matches_event(trigger: &serde_json::Value, event: &EngineEvent) -> bool {
    let trigger_type = trigger["type"].as_str().unwrap_or("");

    match (trigger_type, event) {
        ("webhook", EngineEvent::Webhook { path, .. }) => {
            // If trigger has a path filter, check it; otherwise match all webhooks
            match trigger["path"].as_str() {
                Some(pattern) => path.contains(pattern),
                None => true,
            }
        }
        ("agent_message", EngineEvent::AgentMessage { from_agent, channel, .. }) => {
            let channel_match = match trigger["channel"].as_str() {
                Some(ch) => ch == channel,
                None => true,
            };
            let from_match = match trigger["from"].as_str() {
                Some(f) => f == from_agent,
                None => true,
            };
            channel_match && from_match
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_matches_all() {
        let trigger: serde_json::Value = serde_json::json!({"type": "webhook"});
        let event = EngineEvent::Webhook {
            path: "/deploy".into(),
            agent_id: "default".into(),
            payload: "{}".into(),
        };
        assert!(matches_event(&trigger, &event));
    }

    #[test]
    fn webhook_matches_path() {
        let trigger = serde_json::json!({"type": "webhook", "path": "/deploy"});
        let event = EngineEvent::Webhook {
            path: "/webhook/deploy".into(),
            agent_id: "default".into(),
            payload: "{}".into(),
        };
        assert!(matches_event(&trigger, &event));

        let other = EngineEvent::Webhook {
            path: "/other".into(),
            agent_id: "default".into(),
            payload: "{}".into(),
        };
        assert!(!matches_event(&trigger, &other));
    }

    #[test]
    fn agent_message_matches_channel() {
        let trigger = serde_json::json!({"type": "agent_message", "channel": "alerts"});
        let event = EngineEvent::AgentMessage {
            from_agent: "monitor".into(),
            to_agent: "default".into(),
            channel: "alerts".into(),
            content: "disk full".into(),
        };
        assert!(matches_event(&trigger, &event));

        let other = EngineEvent::AgentMessage {
            from_agent: "monitor".into(),
            to_agent: "default".into(),
            channel: "general".into(),
            content: "hello".into(),
        };
        assert!(!matches_event(&trigger, &other));
    }

    #[test]
    fn agent_message_matches_from() {
        let trigger = serde_json::json!({"type": "agent_message", "from": "monitor"});
        let event = EngineEvent::AgentMessage {
            from_agent: "monitor".into(),
            to_agent: "default".into(),
            channel: "general".into(),
            content: "alert".into(),
        };
        assert!(matches_event(&trigger, &event));

        let other = EngineEvent::AgentMessage {
            from_agent: "bob".into(),
            to_agent: "default".into(),
            channel: "general".into(),
            content: "hello".into(),
        };
        assert!(!matches_event(&trigger, &other));
    }

    #[test]
    fn mismatched_types_dont_match() {
        let trigger = serde_json::json!({"type": "webhook"});
        let event = EngineEvent::AgentMessage {
            from_agent: "a".into(),
            to_agent: "b".into(),
            channel: "c".into(),
            content: "d".into(),
        };
        assert!(!matches_event(&trigger, &event));
    }
}
