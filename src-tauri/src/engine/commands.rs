// Paw Agent Engine — Tauri Commands
// These are the invoke() targets for the frontend.
// They replace the WebSocket gateway methods with direct Rust calls.

use crate::engine::types::*;
use crate::engine::providers::AnyProvider;
use crate::engine::sessions::SessionStore;
use crate::engine::agent_loop;
use crate::engine::memory::{self, EmbeddingClient};
use crate::engine::skills;
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
    pub memory_config: Mutex<MemoryConfig>,
    pub pending_approvals: PendingApprovals,
}

impl EngineState {
    pub fn new() -> Result<Self, String> {
        let store = SessionStore::open()?;

        // Initialize skill vault tables
        store.init_skill_tables()?;

        // Load config from DB or use defaults
        let config = match store.get_config("engine_config") {
            Ok(Some(json)) => {
                serde_json::from_str::<EngineConfig>(&json).unwrap_or_default()
            }
            _ => EngineConfig::default(),
        };

        // Load memory config from DB or use defaults
        let memory_config = match store.get_config("memory_config") {
            Ok(Some(json)) => {
                serde_json::from_str::<MemoryConfig>(&json).unwrap_or_default()
            }
            _ => MemoryConfig::default(),
        };

        Ok(EngineState {
            store,
            config: Mutex::new(config),
            memory_config: Mutex::new(memory_config),
            pending_approvals: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Get an EmbeddingClient from the current memory config, if configured.
    pub fn embedding_client(&self) -> Option<EmbeddingClient> {
        let cfg = self.memory_config.lock().ok()?;
        if cfg.embedding_base_url.is_empty() || cfg.embedding_model.is_empty() {
            return None;
        }
        Some(EmbeddingClient::new(&cfg))
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
            let raw = request.model.clone().unwrap_or_default();
            let model = if raw.is_empty() || raw.eq_ignore_ascii_case("default") {
                let cfg = state.config.lock().unwrap();
                cfg.default_model.clone().unwrap_or_else(|| "gpt-4o".to_string())
            } else {
                raw
            };
            state.store.create_session(&new_id, &model, request.system_prompt.as_deref())?;
            new_id
        }
    };

    // Resolve model and provider
    let (provider_config, model) = {
        let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;

        let raw_model = request.model.clone().unwrap_or_default();
        // Treat empty string or "default" as "use the configured default"
        let model = if raw_model.is_empty() || raw_model.eq_ignore_ascii_case("default") {
            cfg.default_model.clone().unwrap_or_else(|| "gpt-4o".to_string())
        } else {
            raw_model
        };

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

    // Compose agent context (soul files) into the system prompt
    let agent_id = "default"; // TODO: support multi-agent selection from frontend
    let agent_context = state.store.compose_agent_context(agent_id).unwrap_or(None);
    if let Some(ref ac) = agent_context {
        info!("[engine] Agent context loaded ({} chars) for agent '{}'", ac.len(), agent_id);
    } else {
        info!("[engine] No agent context files found for agent '{}'", agent_id);
    }

    // Auto-recall: search memory for context relevant to the user's message
    let (auto_recall_on, auto_capture_on, recall_limit, recall_threshold) = {
        let mcfg = state.memory_config.lock().ok();
        (
            mcfg.as_ref().map(|c| c.auto_recall).unwrap_or(false),
            mcfg.as_ref().map(|c| c.auto_capture).unwrap_or(false),
            mcfg.as_ref().map(|c| c.recall_limit).unwrap_or(5),
            mcfg.as_ref().map(|c| c.recall_threshold).unwrap_or(0.3),
        )
    };

    let memory_context = if auto_recall_on {
        let emb_client = state.embedding_client();
        match memory::search_memories(
            &state.store, &request.message, recall_limit, recall_threshold, emb_client.as_ref()
        ).await {
            Ok(mems) if !mems.is_empty() => {
                let ctx: Vec<String> = mems.iter().map(|m| {
                    format!("- [{}] {}", m.category, m.content)
                }).collect();
                info!("[engine] Auto-recall: {} memories injected", mems.len());
                Some(format!("## Relevant Memories\n{}", ctx.join("\n")))
            }
            Ok(_) => None,
            Err(e) => {
                warn!("[engine] Auto-recall failed: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Compose the full system prompt: base + agent context + memory context
    let full_system_prompt = {
        let mut parts: Vec<String> = Vec::new();
        if let Some(sp) = &system_prompt {
            parts.push(sp.clone());
        }
        if let Some(ac) = &agent_context {
            parts.push(ac.clone());
        }
        if let Some(mc) = &memory_context {
            parts.push(mc.clone());
        }
        if parts.is_empty() { None } else { Some(parts.join("\n\n---\n\n")) }
    };

    info!("[engine] System prompt: {} parts, total {} chars",
        [&system_prompt, &agent_context, &memory_context].iter().filter(|p| p.is_some()).count(),
        full_system_prompt.as_ref().map(|s| s.len()).unwrap_or(0));

    let mut messages = state.store.load_conversation(
        &session_id,
        full_system_prompt.as_deref(),
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
        let mut t = ToolDefinition::builtins();
        // Add tools for enabled skills
        let enabled_ids: Vec<String> = skills::builtin_skills().iter()
            .filter(|s| state.store.is_skill_enabled(&s.id).unwrap_or(false))
            .map(|s| s.id.clone())
            .collect();
        if !enabled_ids.is_empty() {
            info!("[engine] Adding skill tools for: {:?}", enabled_ids);
            t.extend(ToolDefinition::skill_tools(&enabled_ids));
        }
        t
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
    let user_message_for_capture = request.message.clone();
    let _memory_cfg_for_capture = state.memory_config.lock().ok().map(|c| c.clone());

    // Track how many messages exist BEFORE the agent loop so we only store
    // NEW messages afterward (avoids re-inserting historical messages on every turn).
    let pre_loop_msg_count = messages.len();

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

                // Store only NEW messages generated during this agent turn.
                // Messages before pre_loop_msg_count were loaded from DB and must not be re-inserted.
                if let Some(engine_state) = app.try_state::<EngineState>() {
                    for msg in messages.iter().skip(pre_loop_msg_count) {
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

                    // Auto-capture: extract memorable facts and store them
                    if auto_capture_on && !final_text.is_empty() {
                        let facts = memory::extract_memorable_facts(&user_message_for_capture, &final_text);
                        if !facts.is_empty() {
                            let emb_client = engine_state.embedding_client();
                            for (content, category) in &facts {
                                match memory::store_memory(
                                    &engine_state.store, content, category, 5, emb_client.as_ref()
                                ).await {
                                    Ok(id) => info!("[engine] Auto-captured memory: {}", &id[..8]),
                                    Err(e) => warn!("[engine] Auto-capture failed: {}", e),
                                }
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

#[tauri::command]
pub fn engine_session_clear(
    state: State<'_, EngineState>,
    session_id: String,
) -> Result<(), String> {
    info!("[engine] Clearing messages for session {}", session_id);
    state.store.clear_messages(&session_id)
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

// ── Agent Files (Soul / Persona) commands ──────────────────────────────

#[tauri::command]
pub fn engine_agent_file_list(
    state: State<'_, EngineState>,
    agent_id: Option<String>,
) -> Result<Vec<AgentFile>, String> {
    let aid = agent_id.unwrap_or_else(|| "default".into());
    state.store.list_agent_files(&aid)
}

#[tauri::command]
pub fn engine_agent_file_get(
    state: State<'_, EngineState>,
    agent_id: Option<String>,
    file_name: String,
) -> Result<Option<AgentFile>, String> {
    let aid = agent_id.unwrap_or_else(|| "default".into());
    state.store.get_agent_file(&aid, &file_name)
}

#[tauri::command]
pub fn engine_agent_file_set(
    state: State<'_, EngineState>,
    agent_id: Option<String>,
    file_name: String,
    content: String,
) -> Result<(), String> {
    let aid = agent_id.unwrap_or_else(|| "default".into());
    info!("[engine] Setting agent file {}/{} ({} bytes)", aid, file_name, content.len());
    state.store.set_agent_file(&aid, &file_name, &content)
}

#[tauri::command]
pub fn engine_agent_file_delete(
    state: State<'_, EngineState>,
    agent_id: Option<String>,
    file_name: String,
) -> Result<(), String> {
    let aid = agent_id.unwrap_or_else(|| "default".into());
    state.store.delete_agent_file(&aid, &file_name)
}

// ── Memory commands ────────────────────────────────────────────────────

#[tauri::command]
pub async fn engine_memory_store(
    state: State<'_, EngineState>,
    content: String,
    category: Option<String>,
    importance: Option<u8>,
) -> Result<String, String> {
    let cat = category.unwrap_or_else(|| "general".into());
    let imp = importance.unwrap_or(5);
    let emb_client = state.embedding_client();
    memory::store_memory(&state.store, &content, &cat, imp, emb_client.as_ref()).await
}

#[tauri::command]
pub async fn engine_memory_search(
    state: State<'_, EngineState>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<Memory>, String> {
    let lim = limit.unwrap_or(10);
    let threshold = {
        let mcfg = state.memory_config.lock().ok();
        mcfg.map(|c| c.recall_threshold).unwrap_or(0.3)
    };
    let emb_client = state.embedding_client();
    memory::search_memories(&state.store, &query, lim, threshold, emb_client.as_ref()).await
}

#[tauri::command]
pub fn engine_memory_stats(
    state: State<'_, EngineState>,
) -> Result<MemoryStats, String> {
    state.store.memory_stats()
}

#[tauri::command]
pub fn engine_memory_delete(
    state: State<'_, EngineState>,
    id: String,
) -> Result<(), String> {
    state.store.delete_memory(&id)
}

#[tauri::command]
pub fn engine_memory_list(
    state: State<'_, EngineState>,
    limit: Option<usize>,
) -> Result<Vec<Memory>, String> {
    state.store.list_memories(limit.unwrap_or(100))
}

#[tauri::command]
pub fn engine_get_memory_config(
    state: State<'_, EngineState>,
) -> Result<MemoryConfig, String> {
    let cfg = state.memory_config.lock().map_err(|e| format!("Lock error: {}", e))?;
    Ok(cfg.clone())
}

#[tauri::command]
pub fn engine_set_memory_config(
    state: State<'_, EngineState>,
    config: MemoryConfig,
) -> Result<(), String> {
    let json = serde_json::to_string(&config)
        .map_err(|e| format!("Serialize error: {}", e))?;
    state.store.set_config("memory_config", &json)?;
    let mut cfg = state.memory_config.lock().map_err(|e| format!("Lock error: {}", e))?;
    *cfg = config;
    info!("[engine] Memory config updated");
    Ok(())
}

#[tauri::command]
pub async fn engine_test_embedding(
    state: State<'_, EngineState>,
) -> Result<usize, String> {
    let client = state.embedding_client()
        .ok_or_else(|| "No embedding configuration — set base URL and model in memory settings".to_string())?;
    let dims = client.test_connection().await?;
    info!("[engine] Embedding test passed: {} dimensions", dims);
    Ok(dims)
}

// ── Skill Vault commands ───────────────────────────────────────────────

#[tauri::command]
pub fn engine_skills_list(
    state: State<'_, EngineState>,
) -> Result<Vec<skills::SkillStatus>, String> {
    skills::get_all_skill_status(&state.store)
}

#[tauri::command]
pub fn engine_skill_set_enabled(
    state: State<'_, EngineState>,
    skill_id: String,
    enabled: bool,
) -> Result<(), String> {
    info!("[engine] Skill {} → enabled={}", skill_id, enabled);
    state.store.set_skill_enabled(&skill_id, enabled)
}

#[tauri::command]
pub fn engine_skill_set_credential(
    state: State<'_, EngineState>,
    skill_id: String,
    key: String,
    value: String,
) -> Result<(), String> {
    let vault_key = skills::get_vault_key()?;
    let encrypted = skills::encrypt_credential(&value, &vault_key);
    info!("[engine] Setting credential {}:{} ({} chars)", skill_id, key, value.len());
    state.store.set_skill_credential(&skill_id, &key, &encrypted)
}

#[tauri::command]
pub fn engine_skill_delete_credential(
    state: State<'_, EngineState>,
    skill_id: String,
    key: String,
) -> Result<(), String> {
    info!("[engine] Deleting credential {}:{}", skill_id, key);
    state.store.delete_skill_credential(&skill_id, &key)
}

#[tauri::command]
pub fn engine_skill_revoke_all(
    state: State<'_, EngineState>,
    skill_id: String,
) -> Result<(), String> {
    info!("[engine] Revoking all credentials for skill {}", skill_id);
    state.store.delete_all_skill_credentials(&skill_id)?;
    state.store.set_skill_enabled(&skill_id, false)
}
