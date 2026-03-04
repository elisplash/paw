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
use std::collections::HashMap;
use std::sync::LazyLock;
use tauri::Manager;

/// Minimum cooldown between re-triggers of the same task (in seconds).
const EVENT_TRIGGER_COOLDOWN_SECS: i64 = 30;

/// Track last trigger time per task_id to enforce cooldown.
static LAST_TRIGGER_TIMES: LazyLock<
    parking_lot::Mutex<HashMap<String, chrono::DateTime<chrono::Utc>>>,
> = LazyLock::new(|| parking_lot::Mutex::new(HashMap::new()));

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
pub async fn dispatch_event(app_handle: &tauri::AppHandle, event: &EngineEvent) -> Vec<String> {
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
        if !task.cron_enabled {
            continue;
        }
        let trigger_json = match &task.event_trigger {
            Some(t) if !t.is_empty() => t,
            _ => continue,
        };

        let trigger: serde_json::Value = match serde_json::from_str(trigger_json) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if matches_event(&trigger, event) {
            // §Security: enforce cooldown to prevent event amplification attacks.
            // An agent spamming agent_send_message cannot re-trigger the same
            // task faster than EVENT_TRIGGER_COOLDOWN_SECS.
            {
                let now = chrono::Utc::now();
                let mut times = LAST_TRIGGER_TIMES.lock();
                if let Some(last) = times.get(&task.id) {
                    let elapsed = (now - *last).num_seconds();
                    if elapsed < EVENT_TRIGGER_COOLDOWN_SECS {
                        info!(
                            "[events] Skipping task '{}' — cooldown ({}/{}s)",
                            task.id, elapsed, EVENT_TRIGGER_COOLDOWN_SECS
                        );
                        continue;
                    }
                }
                times.insert(task.id.clone(), now);
            }

            info!("[events] Event matched task '{}' ({})", task.title, task.id);

            let now = chrono::Utc::now();
            state
                .store
                .update_task_cron_run(
                    &task.id,
                    &now.to_rfc3339(),
                    Some(&(now + chrono::Duration::minutes(1)).to_rfc3339()),
                )
                .ok();

            let aid = uuid::Uuid::new_v4().to_string();
            let event_desc = match event {
                EngineEvent::Webhook { path, .. } => format!("webhook:{}", path),
                EngineEvent::AgentMessage {
                    from_agent,
                    channel,
                    ..
                } => format!("agent_message:{}#{}", from_agent, channel),
            };
            state
                .store
                .add_task_activity(
                    &aid,
                    &task.id,
                    "event_triggered",
                    None,
                    &format!("Event-triggered: {}", event_desc),
                )
                .ok();

            let task_id = task.id.clone();
            let app = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                let state_inner = app.state::<EngineState>();
                if let Err(e) =
                    crate::engine::tasks::execute_task(&app, &state_inner, &task_id).await
                {
                    warn!(
                        "[events] Failed to execute event-triggered task {}: {}",
                        task_id, e
                    );
                }
            });

            triggered.push(task.id.clone());
        }
    }

    // §Security: GC stale entries from cooldown map to prevent unbounded growth.
    // Remove entries for task IDs that no longer exist in the active task list.
    {
        let active_ids: std::collections::HashSet<&str> =
            tasks.iter().map(|t| t.id.as_str()).collect();
        let mut times = LAST_TRIGGER_TIMES.lock();
        times.retain(|k, _| active_ids.contains(k.as_str()));
    }

    triggered
}

/// Check if an event trigger condition matches an incoming event.
fn matches_event(trigger: &serde_json::Value, event: &EngineEvent) -> bool {
    let trigger_type = trigger["type"].as_str().unwrap_or("");

    match (trigger_type, event) {
        ("webhook", EngineEvent::Webhook { path, .. }) => {
            // If trigger has a path filter, check it with prefix matching;
            // otherwise match all webhooks.
            // §Security: use starts_with instead of contains to prevent
            // overly-broad substring matching on webhook paths.
            match trigger["path"].as_str() {
                Some(pattern) => path == pattern || path.starts_with(&format!("{}/", pattern)),
                None => true,
            }
        }
        (
            "agent_message",
            EngineEvent::AgentMessage {
                from_agent,
                channel,
                ..
            },
        ) => {
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
        // Exact match
        let event = EngineEvent::Webhook {
            path: "/deploy".into(),
            agent_id: "default".into(),
            payload: "{}".into(),
        };
        assert!(matches_event(&trigger, &event));

        // Prefix match (path starts with "/deploy/")
        let sub = EngineEvent::Webhook {
            path: "/deploy/prod".into(),
            agent_id: "default".into(),
            payload: "{}".into(),
        };
        assert!(matches_event(&trigger, &sub));

        let other = EngineEvent::Webhook {
            path: "/other".into(),
            agent_id: "default".into(),
            payload: "{}".into(),
        };
        assert!(!matches_event(&trigger, &other));

        // Substring elsewhere should NOT match (security fix)
        let substring = EngineEvent::Webhook {
            path: "/webhook/deploy".into(),
            agent_id: "default".into(),
            payload: "{}".into(),
        };
        assert!(!matches_event(&trigger, &substring));
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

    #[test]
    fn agent_message_trigger_does_not_match_webhook() {
        let trigger = serde_json::json!({"type": "agent_message"});
        let event = EngineEvent::Webhook {
            path: "/deploy".into(),
            agent_id: "default".into(),
            payload: "{}".into(),
        };
        assert!(!matches_event(&trigger, &event));
    }

    #[test]
    fn unknown_trigger_type_no_match() {
        let trigger = serde_json::json!({"type": "cron"});
        let event = EngineEvent::Webhook {
            path: "/test".into(),
            agent_id: "default".into(),
            payload: "{}".into(),
        };
        assert!(!matches_event(&trigger, &event));
    }

    #[test]
    fn missing_trigger_type_no_match() {
        let trigger = serde_json::json!({});
        let event = EngineEvent::Webhook {
            path: "/test".into(),
            agent_id: "default".into(),
            payload: "{}".into(),
        };
        assert!(!matches_event(&trigger, &event));
    }

    #[test]
    fn agent_message_matches_all_no_filters() {
        let trigger = serde_json::json!({"type": "agent_message"});
        let event = EngineEvent::AgentMessage {
            from_agent: "anyone".into(),
            to_agent: "default".into(),
            channel: "any_channel".into(),
            content: "anything".into(),
        };
        assert!(matches_event(&trigger, &event));
    }

    #[test]
    fn agent_message_both_filters_must_match() {
        let trigger =
            serde_json::json!({"type": "agent_message", "channel": "alerts", "from": "monitor"});

        // Both match
        let ok = EngineEvent::AgentMessage {
            from_agent: "monitor".into(),
            to_agent: "default".into(),
            channel: "alerts".into(),
            content: "disk full".into(),
        };
        assert!(matches_event(&trigger, &ok));

        // Channel matches but from doesn't
        let wrong_from = EngineEvent::AgentMessage {
            from_agent: "bob".into(),
            to_agent: "default".into(),
            channel: "alerts".into(),
            content: "hello".into(),
        };
        assert!(!matches_event(&trigger, &wrong_from));

        // From matches but channel doesn't
        let wrong_channel = EngineEvent::AgentMessage {
            from_agent: "monitor".into(),
            to_agent: "default".into(),
            channel: "general".into(),
            content: "hello".into(),
        };
        assert!(!matches_event(&trigger, &wrong_channel));
    }

    #[test]
    fn webhook_path_prefix_match() {
        let trigger = serde_json::json!({"type": "webhook", "path": "/deploy"});
        // Exact match
        let exact = EngineEvent::Webhook {
            path: "/deploy".into(),
            agent_id: "default".into(),
            payload: "{}".into(),
        };
        assert!(matches_event(&trigger, &exact));

        // Prefix match (sub-path)
        let sub = EngineEvent::Webhook {
            path: "/deploy/prod".into(),
            agent_id: "default".into(),
            payload: "{}".into(),
        };
        assert!(matches_event(&trigger, &sub));

        // Substring in the middle should NOT match
        let middle = EngineEvent::Webhook {
            path: "/api/v2/deploy/prod".into(),
            agent_id: "default".into(),
            payload: "{}".into(),
        };
        assert!(!matches_event(&trigger, &middle));
    }

    #[test]
    fn webhook_path_no_partial_match() {
        let trigger = serde_json::json!({"type": "webhook", "path": "deploy"});
        let event = EngineEvent::Webhook {
            path: "/api/v2/status".into(),
            agent_id: "default".into(),
            payload: "{}".into(),
        };
        assert!(!matches_event(&trigger, &event));
    }
}
