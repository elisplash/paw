// Paw Agent Engine — Tauri Commands
// These are the invoke() targets for the frontend.
// They replace the WebSocket gateway methods with direct Rust calls.

use crate::engine::types::*;
use crate::engine::providers::AnyProvider;
use crate::engine::sessions::SessionStore;
use crate::engine::agent_loop;
use log::{info, warn, error};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State};

/// Pending tool approvals: maps tool_call_id → oneshot sender.
/// The agent loop registers a sender before emitting ToolRequest,
/// then awaits the receiver. The `engine_approve_tool` command
/// resolves it from the frontend.
pub type PendingApprovals = Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>>>;

/// Engine state managed by Tauri.
pub struct EngineState {
    pub store: SessionStore,
    pub config: Mutex<EngineConfig>,
    pub pending_approvals: PendingApprovals,
}

impl EngineState {
    pub fn new() -> Result<Self, String> {
        let store = SessionStore::open()?;

        // Load config from DB or use defaults
        let config = match store.get_config("engine_config") {
            Ok(Some(json)) => {
                serde_json::from_str::<EngineConfig>(&json).unwrap_or_default()
            }
            _ => EngineConfig::default(),
        };

        Ok(EngineState {
            store,
            config: Mutex::new(config),
            pending_approvals: Arc::new(Mutex::new(HashMap::new())),
        })
    }
}

// ── Chat commands ──────────────────────────────────────────────────────

/// Send a chat message and run the agent loop.
/// Returns immediately with a run_id; results stream via `engine-event` Tauri events.
#[tauri::command]
pub async fn engine_chat_send(
    app_handle: tauri::AppHandle,
    state: State<'_, EngineState>,
    request: ChatRequest,
) -> Result<ChatResponse, String> {
    let run_id = uuid::Uuid::new_v4().to_string();

    // Resolve or create session
    let session_id = match &request.session_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => {
            let new_id = format!("eng-{}", uuid::Uuid::new_v4());
            let model = request.model.clone().unwrap_or_else(|| {
                let cfg = state.config.lock().unwrap();
                cfg.default_model.clone().unwrap_or_else(|| "gpt-4o".to_string())
            });
            state.store.create_session(&new_id, &model, request.system_prompt.as_deref())?;
            new_id
        }
    };

    // Resolve model and provider
    let (provider_config, model) = {
        let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;

        let model = request.model.unwrap_or_else(|| {
            cfg.default_model.clone().unwrap_or_else(|| "gpt-4o".to_string())
        });

        // Find provider by ID or use the one that matches the model prefix
        let provider = if let Some(pid) = &request.provider_id {
            cfg.providers.iter().find(|p| p.id == *pid).cloned()
        } else {
            // Smart provider resolution: check model prefix
            let provider = if model.starts_with("claude") || model.starts_with("anthropic") {
                cfg.providers.iter().find(|p| p.kind == ProviderKind::Anthropic).cloned()
            } else if model.starts_with("gemini") || model.starts_with("google") {
                cfg.providers.iter().find(|p| p.kind == ProviderKind::Google).cloned()
            } else if model.starts_with("gpt") || model.starts_with("o1") || model.starts_with("o3") {
                cfg.providers.iter().find(|p| p.kind == ProviderKind::OpenAI).cloned()
            } else {
                None
            };
            // Fallback to default provider or first available
            provider
                .or_else(|| {
                    cfg.default_provider.as_ref()
                        .and_then(|dp| cfg.providers.iter().find(|p| p.id == *dp).cloned())
                })
                .or_else(|| cfg.providers.first().cloned())
        };

        match provider {
            Some(p) => (p, model),
            None => return Err("No AI provider configured. Go to Settings → Engine to add an API key.".into()),
        }
    };

    // Store the user message
    let user_msg = StoredMessage {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        role: "user".into(),
        content: request.message.clone(),
        tool_calls_json: None,
        tool_call_id: None,
        name: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    state.store.add_message(&user_msg)?;

    // Load conversation history
    let system_prompt = request.system_prompt.or_else(|| {
        let cfg = state.config.lock().ok()?;
        cfg.default_system_prompt.clone()
    });

    let mut messages = state.store.load_conversation(
        &session_id,
        system_prompt.as_deref(),
    )?;

    // If the user message has attachments, replace the last (user) message with
    // one that includes image content blocks, so the provider can see the images.
    if !request.attachments.is_empty() {
        // Pop the plain-text user message we just loaded from DB
        if let Some(last_msg) = messages.last_mut() {
            if last_msg.role == Role::User {
                let mut blocks = vec![ContentBlock::Text { text: request.message.clone() }];
                for att in &request.attachments {
                    if att.mime_type.starts_with("image/") {
                        let data_url = format!("data:{};base64,{}", att.mime_type, att.content);
                        blocks.push(ContentBlock::ImageUrl {
                            image_url: ImageUrlData {
                                url: data_url,
                                detail: Some("auto".into()),
                            },
                        });
                    }
                }
                last_msg.content = MessageContent::Blocks(blocks);
            }
        }
    }

    // Build tools
    let tools = if request.tools_enabled.unwrap_or(true) {
        ToolDefinition::builtins()
    } else {
        vec![]
    };

    // Get engine config values
    let (max_rounds, temperature) = {
        let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
        (cfg.max_tool_rounds, request.temperature)
    };

    let session_id_clone = session_id.clone();
    let run_id_clone = run_id.clone();
    let approvals = state.pending_approvals.clone();
    let tool_timeout = {
        let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
        cfg.tool_timeout_secs
    };

    // Spawn the agent loop in a background task
    let app = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        let provider = AnyProvider::from_config(&provider_config);

        match agent_loop::run_agent_turn(
            &app,
            &provider,
            &model,
            &mut messages,
            &tools,
            &session_id_clone,
            &run_id_clone,
            max_rounds,
            temperature,
            &approvals,
            tool_timeout,
        ).await {
            Ok(final_text) => {
                info!("[engine] Agent turn complete: {} chars", final_text.len());

                // Store assistant messages from the conversation
                // (the loop may have added multiple assistant + tool messages)
                // We store the final text as the main response
                if let Some(engine_state) = app.try_state::<EngineState>() {
                    for msg in &messages {
                        if msg.role == Role::Assistant || msg.role == Role::Tool {
                            let stored = StoredMessage {
                                id: uuid::Uuid::new_v4().to_string(),
                                session_id: session_id_clone.clone(),
                                role: match msg.role {
                                    Role::Assistant => "assistant".into(),
                                    Role::Tool => "tool".into(),
                                    _ => "user".into(),
                                },
                                content: msg.content.as_text(),
                                tool_calls_json: msg.tool_calls.as_ref()
                                    .map(|tc| serde_json::to_string(tc).unwrap_or_default()),
                                tool_call_id: msg.tool_call_id.clone(),
                                name: msg.name.clone(),
                                created_at: chrono::Utc::now().to_rfc3339(),
                            };
                            if let Err(e) = engine_state.store.add_message(&stored) {
                                error!("[engine] Failed to store message: {}", e);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("[engine] Agent turn failed: {}", e);
                let _ = app.emit("engine-event", EngineEvent::Error {
                    session_id: session_id_clone,
                    run_id: run_id_clone,
                    message: e,
                });
            }
        }
    });

    Ok(ChatResponse {
        run_id,
        session_id,
    })
}

/// Get chat history for a session.
#[tauri::command]
pub fn engine_chat_history(
    state: State<'_, EngineState>,
    session_id: String,
    limit: Option<i64>,
) -> Result<Vec<StoredMessage>, String> {
    state.store.get_messages(&session_id, limit.unwrap_or(200))
}

// ── Session commands ───────────────────────────────────────────────────

#[tauri::command]
pub fn engine_sessions_list(
    state: State<'_, EngineState>,
    limit: Option<i64>,
) -> Result<Vec<Session>, String> {
    state.store.list_sessions(limit.unwrap_or(50))
}

#[tauri::command]
pub fn engine_session_rename(
    state: State<'_, EngineState>,
    session_id: String,
    label: String,
) -> Result<(), String> {
    state.store.rename_session(&session_id, &label)
}

#[tauri::command]
pub fn engine_session_delete(
    state: State<'_, EngineState>,
    session_id: String,
) -> Result<(), String> {
    state.store.delete_session(&session_id)
}

// ── Engine configuration commands ──────────────────────────────────────

#[tauri::command]
pub fn engine_get_config(
    state: State<'_, EngineState>,
) -> Result<EngineConfig, String> {
    let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
    Ok(cfg.clone())
}

#[tauri::command]
pub fn engine_set_config(
    state: State<'_, EngineState>,
    config: EngineConfig,
) -> Result<(), String> {
    let json = serde_json::to_string(&config)
        .map_err(|e| format!("Serialize error: {}", e))?;

    // Persist to DB
    state.store.set_config("engine_config", &json)?;

    // Update in-memory config
    let mut cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
    *cfg = config;

    info!("[engine] Config updated, {} providers configured", cfg.providers.len());
    Ok(())
}

/// Add or update a single provider without replacing the entire config.
#[tauri::command]
pub fn engine_upsert_provider(
    state: State<'_, EngineState>,
    provider: ProviderConfig,
) -> Result<(), String> {
    let mut cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;

    // Update existing or add new
    if let Some(existing) = cfg.providers.iter_mut().find(|p| p.id == provider.id) {
        *existing = provider;
    } else {
        cfg.providers.push(provider);
    }

    // Set as default if it's the first provider
    if cfg.default_provider.is_none() && !cfg.providers.is_empty() {
        cfg.default_provider = Some(cfg.providers[0].id.clone());
    }

    // Persist
    let json = serde_json::to_string(&*cfg)
        .map_err(|e| format!("Serialize error: {}", e))?;
    state.store.set_config("engine_config", &json)?;

    info!("[engine] Provider upserted, {} total providers", cfg.providers.len());
    Ok(())
}

/// Remove a provider by ID.
#[tauri::command]
pub fn engine_remove_provider(
    state: State<'_, EngineState>,
    provider_id: String,
) -> Result<(), String> {
    let mut cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;

    cfg.providers.retain(|p| p.id != provider_id);

    // Clear default if it was the removed provider
    if cfg.default_provider.as_deref() == Some(&provider_id) {
        cfg.default_provider = cfg.providers.first().map(|p| p.id.clone());
    }

    let json = serde_json::to_string(&*cfg)
        .map_err(|e| format!("Serialize error: {}", e))?;
    state.store.set_config("engine_config", &json)?;

    info!("[engine] Provider removed, {} remaining", cfg.providers.len());
    Ok(())
}

/// Check if the engine is configured and ready to use.
#[tauri::command]
pub fn engine_status(
    state: State<'_, EngineState>,
) -> Result<serde_json::Value, String> {
    let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;

    let has_providers = !cfg.providers.is_empty();
    let has_api_key = cfg.providers.iter().any(|p| !p.api_key.is_empty());

    Ok(serde_json::json!({
        "ready": has_providers && has_api_key,
        "providers": cfg.providers.len(),
        "has_api_key": has_api_key,
        "default_model": cfg.default_model,
        "default_provider": cfg.default_provider,
    }))
}

/// Resolve a pending tool approval from the frontend.
/// Called by the approval modal when the user clicks Allow or Deny.
#[tauri::command]
pub fn engine_approve_tool(
    state: State<'_, EngineState>,
    tool_call_id: String,
    approved: bool,
) -> Result<(), String> {
    let mut map = state.pending_approvals.lock()
        .map_err(|e| format!("Lock error: {}", e))?;

    if let Some(sender) = map.remove(&tool_call_id) {
        info!("[engine] Tool approval resolved: {} → {}", tool_call_id, if approved { "ALLOWED" } else { "DENIED" });
        let _ = sender.send(approved);
        Ok(())
    } else {
        warn!("[engine] No pending approval found for tool_call_id={}", tool_call_id);
        Err(format!("No pending approval for {}", tool_call_id))
    }
}
