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
    let agent_id_owned = request.agent_id.clone().unwrap_or_else(|| "default".to_string());
    let agent_context = state.store.compose_agent_context(&agent_id_owned).unwrap_or(None);
    if let Some(ref ac) = agent_context {
        info!("[engine] Agent context loaded ({} chars) for agent '{}'", ac.len(), agent_id_owned);
    } else {
        info!("[engine] No agent context files found for agent '{}'", agent_id_owned);
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
            &state.store, &request.message, recall_limit, recall_threshold, emb_client.as_ref(), Some(&agent_id_owned)
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

    // Collect skill instructions from enabled skills
    let skill_instructions = skills::get_enabled_skill_instructions(&state.store).unwrap_or_default();
    if !skill_instructions.is_empty() {
        info!("[engine] Skill instructions injected ({} chars)", skill_instructions.len());
    }

    // Build self-awareness context: tell the agent what model/provider it's running on
    let self_awareness = {
        let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
        let provider_name = cfg.providers.iter()
            .find(|p| Some(p.id.clone()) == cfg.default_provider)
            .or_else(|| cfg.providers.first())
            .map(|p| format!("{} ({:?})", p.id, p.kind))
            .unwrap_or_else(|| "unknown".into());
        format!(
            "## Your Runtime Identity\n\
            - **Current model**: {}\n\
            - **Provider**: {}\n\
            - **Session**: {}\n\
            - **Max tool rounds**: {}\n\
            - **Tool timeout**: {}s\n\
            You know exactly which model you are and can tell the user. \
            If you need detailed config info, use the `self_info` tool.",
            model, provider_name, session_id, cfg.max_tool_rounds, cfg.tool_timeout_secs
        )
    };

    // Compose the full system prompt: base + self-awareness + agent context + memory context + skill instructions
    let full_system_prompt = {
        let mut parts: Vec<String> = Vec::new();
        if let Some(sp) = &system_prompt {
            parts.push(sp.clone());
        }
        parts.push(self_awareness);
        if let Some(ac) = &agent_context {
            parts.push(ac.clone());
        }
        if let Some(mc) = &memory_context {
            parts.push(mc.clone());
        }
        if !skill_instructions.is_empty() {
            parts.push(skill_instructions.clone());
        }
        if parts.is_empty() { None } else { Some(parts.join("\n\n---\n\n")) }
    };

    info!("[engine] System prompt: {} parts, total {} chars",
        [&system_prompt, &agent_context, &memory_context].iter().filter(|p| p.is_some()).count()
            + if skill_instructions.is_empty() { 0 } else { 1 },
        full_system_prompt.as_ref().map(|s| s.len()).unwrap_or(0));

    let mut messages = state.store.load_conversation(
        &session_id,
        full_system_prompt.as_deref(),
    )?;

    // If the user message has attachments, replace the last (user) message with
    // content blocks so the provider can see images and text files.
    if !request.attachments.is_empty() {
        info!("[engine] Processing {} attachment(s)", request.attachments.len());
        if let Some(last_msg) = messages.last_mut() {
            if last_msg.role == Role::User {
                let mut blocks = vec![ContentBlock::Text { text: request.message.clone() }];
                for att in &request.attachments {
                    let label = att.name.as_deref().unwrap_or("attachment");
                    info!("[engine] Attachment '{}' type={} size={}B", label, att.mime_type, att.content.len());
                    if att.mime_type.starts_with("image/") {
                        // Images → send as vision content blocks (native LLM vision)
                        let data_url = format!("data:{};base64,{}", att.mime_type, att.content);
                        blocks.push(ContentBlock::ImageUrl {
                            image_url: ImageUrlData {
                                url: data_url,
                                detail: Some("auto".into()),
                            },
                        });
                    } else if att.mime_type == "application/pdf" {
                        // PDFs → send as native document blocks (Claude, Gemini, OpenAI all support this)
                        blocks.push(ContentBlock::Document {
                            mime_type: att.mime_type.clone(),
                            data: att.content.clone(),
                            name: att.name.clone(),
                        });
                    } else {
                        // Text-based files → decode base64 to text and inline
                        use base64::Engine as _;
                        match base64::engine::general_purpose::STANDARD.decode(&att.content) {
                            Ok(bytes) => {
                                let text_content = String::from_utf8_lossy(&bytes);
                                blocks.push(ContentBlock::Text {
                                    text: format!(
                                        "[Attached file: {} ({})]\n```\n{}\n```",
                                        label, att.mime_type, text_content
                                    ),
                                });
                            }
                            Err(e) => {
                                warn!("[engine] Failed to decode attachment '{}': {}", label, e);
                                blocks.push(ContentBlock::Text {
                                    text: format!(
                                        "[Attached file: {} ({}) — could not decode content]",
                                        label, att.mime_type
                                    ),
                                });
                            }
                        }
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
        // Apply per-agent tool filter (if provided by frontend policy)
        if let Some(ref filter) = request.tool_filter {
            let before = t.len();
            t.retain(|tool| filter.contains(&tool.function.name));
            info!("[engine] Tool policy filter applied: {} → {} tools (filter has {} entries)",
                before, t.len(), filter.len());
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
    let agent_id_for_spawn = agent_id_owned.clone();
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
                                    &engine_state.store, content, category, 5, emb_client.as_ref(), Some(&agent_id_for_spawn)
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

#[tauri::command]
pub async fn engine_session_compact(
    state: State<'_, EngineState>,
    session_id: String,
) -> Result<crate::engine::compaction::CompactionResult, String> {
    info!("[engine] Manual compaction requested for session {}", session_id);

    // Resolve provider and model from config
    let (provider_config, model) = {
        let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
        let model = cfg.default_model.clone().unwrap_or_else(|| "gpt-4o".to_string());
        let provider = cfg.default_provider.as_ref()
            .and_then(|dp| cfg.providers.iter().find(|p| p.id == *dp).cloned())
            .or_else(|| cfg.providers.first().cloned())
            .ok_or("No AI provider configured.")?;
        (provider, model)
    };

    let provider = crate::engine::providers::AnyProvider::from_config(&provider_config);
    let compact_config = crate::engine::compaction::CompactionConfig::default();

    // Wrap the existing store in an Arc for the async call
    // Note: we use a new SessionStore connection since the state's store is behind a State ref
    let store_arc = std::sync::Arc::new(
        crate::engine::sessions::SessionStore::open()?
    );

    crate::engine::compaction::compact_session(
        &store_arc,
        &provider,
        &model,
        &session_id,
        &compact_config,
    ).await
}

// ── Sandbox commands ───────────────────────────────────────────────────

#[tauri::command]
pub async fn engine_sandbox_check() -> Result<bool, String> {
    Ok(crate::engine::sandbox::is_docker_available().await)
}

#[tauri::command]
pub fn engine_sandbox_get_config(
    state: State<'_, EngineState>,
) -> Result<crate::engine::sandbox::SandboxConfig, String> {
    Ok(crate::engine::sandbox::load_sandbox_config(&state.store))
}

#[tauri::command]
pub fn engine_sandbox_set_config(
    state: State<'_, EngineState>,
    config: crate::engine::sandbox::SandboxConfig,
) -> Result<(), String> {
    crate::engine::sandbox::save_sandbox_config(&state.store, &config)
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

/// Auto-setup: detect Ollama on first run and add it as a provider.
/// Returns what was done so the frontend can show a toast.
#[tauri::command]
pub async fn engine_auto_setup(
    state: State<'_, EngineState>,
) -> Result<serde_json::Value, String> {
    // Only run if no providers are configured yet
    {
        let cfg = state.config.lock().map_err(|e| format!("Lock: {}", e))?;
        if !cfg.providers.is_empty() {
            return Ok(serde_json::json!({ "action": "none", "reason": "providers_exist" }));
        }
    }

    info!("[engine] First run — no providers configured, attempting Ollama auto-detect");

    // Try to reach Ollama
    let base_url = "http://localhost:11434";
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    // Check if Ollama is reachable
    let ollama_up = match client.get(format!("{}/api/tags", base_url)).send().await {
        Ok(resp) if resp.status().is_success() => true,
        _ => {
            // Try to start Ollama if the binary exists
            if let Ok(_child) = std::process::Command::new("ollama").arg("serve").spawn() {
                info!("[engine] Attempting to auto-start Ollama...");
                // Wait for it to come up
                let mut up = false;
                for _ in 0..10 {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    if client.get(format!("{}/api/tags", base_url)).send().await.is_ok() {
                        up = true;
                        break;
                    }
                }
                up
            } else {
                false
            }
        }
    };

    if !ollama_up {
        info!("[engine] Ollama not found — user will need to configure a provider manually");
        return Ok(serde_json::json!({
            "action": "none",
            "reason": "ollama_not_found",
            "message": "No AI provider configured. Install Ollama from ollama.ai for free local AI, or add an API key in Settings → Engine."
        }));
    }

    // Ollama is up — check what models are available
    let models: Vec<String> = match client.get(format!("{}/api/tags", base_url)).send().await {
        Ok(resp) => {
            if let Ok(data) = resp.json::<serde_json::Value>().await {
                data["models"].as_array()
                    .map(|arr| arr.iter().filter_map(|m| m["name"].as_str().map(String::from)).collect())
                    .unwrap_or_default()
            } else {
                vec![]
            }
        }
        _ => vec![],
    };

    // Pick the best available model, or pull a small one
    let preferred = ["llama3.2:3b", "llama3.2:1b", "llama3.1:8b", "llama3:8b", "mistral:7b", "gemma2:2b", "phi3:mini", "qwen2.5:3b"];
    let chosen_model = models.iter()
        .find(|m| preferred.iter().any(|p| m.starts_with(p.split(':').next().unwrap_or(""))))
        .cloned()
        .or_else(|| models.first().cloned());

    let model_name = if let Some(m) = chosen_model {
        m
    } else {
        // No models at all — pull a small one
        info!("[engine] Ollama has no models — pulling llama3.2:3b");
        let pull_body = serde_json::json!({ "name": "llama3.2:3b", "stream": false });
        match client.post(format!("{}/api/pull", base_url))
            .json(&pull_body)
            .timeout(std::time::Duration::from_secs(300))
            .send().await
        {
            Ok(resp) if resp.status().is_success() => {
                info!("[engine] Successfully pulled llama3.2:3b");
                "llama3.2:3b".to_string()
            }
            Ok(resp) => {
                let status = resp.status();
                warn!("[engine] Model pull returned {}", status);
                "llama3.2:3b".to_string() // set it anyway, user can fix
            }
            Err(e) => {
                warn!("[engine] Model pull failed: {}", e);
                "llama3.2:3b".to_string()
            }
        }
    };

    // Add Ollama as a provider
    let provider = ProviderConfig {
        id: "ollama".to_string(),
        kind: ProviderKind::Ollama,
        api_key: String::new(),
        base_url: Some(base_url.to_string()),
        default_model: Some(model_name.clone()),
    };

    {
        let mut cfg = state.config.lock().map_err(|e| format!("Lock: {}", e))?;
        cfg.providers.push(provider);
        cfg.default_provider = Some("ollama".to_string());
        cfg.default_model = Some(model_name.clone());

        let json = serde_json::to_string(&*cfg)
            .map_err(|e| format!("Serialize: {}", e))?;
        state.store.set_config("engine_config", &json)?;
    }

    info!("[engine] Auto-setup complete: Ollama added as default provider with model '{}'", model_name);

    Ok(serde_json::json!({
        "action": "ollama_added",
        "model": model_name,
        "available_models": models,
        "message": format!("Ollama detected! Set up with model '{}' — ready to chat.", model_name)
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
    agent_id: Option<String>,
) -> Result<String, String> {
    let cat = category.unwrap_or_else(|| "general".into());
    let imp = importance.unwrap_or(5);
    let emb_client = state.embedding_client();
    memory::store_memory(&state.store, &content, &cat, imp, emb_client.as_ref(), agent_id.as_deref()).await
}

#[tauri::command]
pub async fn engine_memory_search(
    state: State<'_, EngineState>,
    query: String,
    limit: Option<usize>,
    agent_id: Option<String>,
) -> Result<Vec<Memory>, String> {
    let lim = limit.unwrap_or(10);
    let threshold = {
        let mcfg = state.memory_config.lock().ok();
        mcfg.map(|c| c.recall_threshold).unwrap_or(0.3)
    };
    let emb_client = state.embedding_client();
    memory::search_memories(&state.store, &query, lim, threshold, emb_client.as_ref(), agent_id.as_deref()).await
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

/// Check Ollama status and model availability.
/// Returns { ollama_running: bool, model_available: bool, model_name: String }
#[tauri::command]
pub async fn engine_embedding_status(
    state: State<'_, EngineState>,
) -> Result<serde_json::Value, String> {
    let client = match state.embedding_client() {
        Some(c) => c,
        None => return Ok(serde_json::json!({
            "ollama_running": false,
            "model_available": false,
            "model_name": "",
            "error": "No embedding configuration"
        })),
    };

    let model_name = {
        let cfg = state.memory_config.lock().map_err(|e| format!("Lock error: {}", e))?;
        cfg.embedding_model.clone()
    };

    let ollama_running = client.check_ollama_running().await.unwrap_or(false);
    let model_available = if ollama_running {
        client.check_model_available().await.unwrap_or(false)
    } else {
        false
    };

    Ok(serde_json::json!({
        "ollama_running": ollama_running,
        "model_available": model_available,
        "model_name": model_name,
    }))
}

/// Pull the embedding model from Ollama.
#[tauri::command]
pub async fn engine_embedding_pull_model(
    state: State<'_, EngineState>,
) -> Result<String, String> {
    let client = state.embedding_client()
        .ok_or_else(|| "No embedding configuration".to_string())?;

    // Check Ollama running first
    let running = client.check_ollama_running().await.unwrap_or(false);
    if !running {
        return Err("Ollama is not running. Start Ollama first, then try again.".into());
    }

    // Check if already available
    if client.check_model_available().await.unwrap_or(false) {
        return Ok("Model already available".into());
    }

    // Pull the model (blocking)
    client.pull_model().await?;
    Ok("Model pulled successfully".into())
}

/// Ensure Ollama is running and the embedding model is available.
/// This is the "just works" function — automatically starts Ollama if needed
/// and pulls the embedding model if it's not present.
#[tauri::command]
pub async fn engine_ensure_embedding_ready(
    state: State<'_, EngineState>,
) -> Result<memory::OllamaReadyStatus, String> {
    let config = {
        let cfg = state.memory_config.lock().map_err(|e| format!("Lock error: {}", e))?;
        cfg.clone()
    };

    let status = memory::ensure_ollama_ready(&config).await;

    // If we discovered the actual dimensions, update the config
    if status.embedding_dims > 0 {
        let mut cfg = state.memory_config.lock().map_err(|e| format!("Lock error: {}", e))?;
        if cfg.embedding_dims != status.embedding_dims {
            info!("[engine] Updating embedding_dims from {} to {} based on actual model output", cfg.embedding_dims, status.embedding_dims);
            cfg.embedding_dims = status.embedding_dims;
            // Save to DB
            if let Ok(json) = serde_json::to_string(&*cfg) {
                let _ = state.store.set_config("memory_config", &json);
            }
        }
    }

    // If we auto-pulled the model, backfill any existing memories that lack embeddings
    if status.was_auto_pulled && status.error.is_none() {
        if let Some(client) = state.embedding_client() {
            let _ = memory::backfill_embeddings(&state.store, &client).await;
        }
    }

    Ok(status)
}

/// Backfill embeddings for memories that don't have them.
#[tauri::command]
pub async fn engine_memory_backfill(
    state: State<'_, EngineState>,
) -> Result<serde_json::Value, String> {
    let client = state.embedding_client()
        .ok_or_else(|| "No embedding configuration — Ollama must be running with an embedding model".to_string())?;

    let (success, fail) = memory::backfill_embeddings(&state.store, &client).await?;
    Ok(serde_json::json!({
        "success": success,
        "failed": fail,
    }))
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

#[tauri::command]
pub fn engine_skill_get_instructions(
    state: State<'_, EngineState>,
    skill_id: String,
) -> Result<Option<String>, String> {
    state.store.get_skill_custom_instructions(&skill_id)
}

#[tauri::command]
pub fn engine_skill_set_instructions(
    state: State<'_, EngineState>,
    skill_id: String,
    instructions: String,
) -> Result<(), String> {
    info!("[engine] Setting custom instructions for skill {} ({} chars)", skill_id, instructions.len());
    state.store.set_skill_custom_instructions(&skill_id, &instructions)
}

// ── Task commands ──────────────────────────────────────────────────────

#[tauri::command]
pub fn engine_tasks_list(
    state: State<'_, EngineState>,
) -> Result<Vec<Task>, String> {
    state.store.list_tasks()
}

#[tauri::command]
pub fn engine_task_create(
    state: State<'_, EngineState>,
    task: Task,
) -> Result<(), String> {
    info!("[engine] Creating task: {} ({})", task.title, task.id);
    state.store.create_task(&task)?;
    // Log activity
    let aid = uuid::Uuid::new_v4().to_string();
    state.store.add_task_activity(&aid, &task.id, "created", None, &format!("Task created: {}", task.title))?;
    Ok(())
}

#[tauri::command]
pub fn engine_task_update(
    state: State<'_, EngineState>,
    task: Task,
) -> Result<(), String> {
    info!("[engine] Updating task: {} status={}", task.id, task.status);
    state.store.update_task(&task)
}

#[tauri::command]
pub fn engine_task_delete(
    state: State<'_, EngineState>,
    task_id: String,
) -> Result<(), String> {
    info!("[engine] Deleting task: {}", task_id);
    state.store.delete_task(&task_id)
}

#[tauri::command]
pub fn engine_task_move(
    state: State<'_, EngineState>,
    task_id: String,
    new_status: String,
) -> Result<(), String> {
    info!("[engine] Moving task {} → {}", task_id, new_status);
    // Load, update status, save
    let tasks = state.store.list_tasks()?;
    if let Some(mut task) = tasks.into_iter().find(|t| t.id == task_id) {
        let old_status = task.status.clone();
        task.status = new_status.clone();
        state.store.update_task(&task)?;
        // Log activity
        let aid = uuid::Uuid::new_v4().to_string();
        state.store.add_task_activity(
            &aid, &task_id, "status_change", None,
            &format!("Moved from {} to {}", old_status, new_status),
        )?;
        Ok(())
    } else {
        Err(format!("Task not found: {}", task_id))
    }
}

#[tauri::command]
pub fn engine_task_activity(
    state: State<'_, EngineState>,
    task_id: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<TaskActivity>, String> {
    let limit = limit.unwrap_or(50);
    match task_id {
        Some(id) => state.store.list_task_activity(&id, limit),
        None => state.store.list_all_activity(limit),
    }
}

/// Set agents assigned to a task (multi-agent support).
#[tauri::command]
pub fn engine_task_set_agents(
    state: State<'_, EngineState>,
    task_id: String,
    agents: Vec<TaskAgent>,
) -> Result<(), String> {
    info!("[engine] Setting {} agent(s) for task {}", agents.len(), task_id);
    state.store.set_task_agents(&task_id, &agents)?;

    // Log activity
    let agent_names: Vec<&str> = agents.iter().map(|a| a.agent_id.as_str()).collect();
    let aid = uuid::Uuid::new_v4().to_string();
    state.store.add_task_activity(
        &aid, &task_id, "assigned", None,
        &format!("Agents assigned: {}", agent_names.join(", ")),
    )?;

    Ok(())
}

/// Run a task: send it to agents and stream the results.
/// Multi-agent: spawns parallel agent loops for all assigned agents.
/// Each agent gets its own session (`eng-task-{task_id}-{agent_id}`)
/// and its own soul context.
#[tauri::command]
pub async fn engine_task_run(
    app_handle: tauri::AppHandle,
    state: State<'_, EngineState>,
    task_id: String,
) -> Result<String, String> {
    let run_id = uuid::Uuid::new_v4().to_string();

    // Load the task
    let tasks = state.store.list_tasks()?;
    let task = tasks.into_iter().find(|t| t.id == task_id)
        .ok_or_else(|| format!("Task not found: {}", task_id))?;

    // Determine which agents to run
    // Multi-agent: use task_agents table; fallback to legacy assigned_agent
    let agent_ids: Vec<String> = if !task.assigned_agents.is_empty() {
        task.assigned_agents.iter().map(|a| a.agent_id.clone()).collect()
    } else if let Some(ref agent) = task.assigned_agent {
        vec![agent.clone()]
    } else {
        vec!["default".to_string()]
    };

    info!("[engine] Running task '{}' with {} agent(s): {:?}", task.title, agent_ids.len(), agent_ids);

    // Update task status to in_progress
    {
        let mut t = task.clone();
        t.status = "in_progress".to_string();
        state.store.update_task(&t)?;
    }

    // Log activity for each agent
    for agent_id in &agent_ids {
        let aid = uuid::Uuid::new_v4().to_string();
        state.store.add_task_activity(
            &aid, &task_id, "agent_started", Some(agent_id),
            &format!("Agent {} started working on: {}", agent_id, task.title),
        )?;
    }

    // Compose task prompt
    let task_prompt = if task.description.is_empty() {
        task.title.clone()
    } else {
        format!("{}\n\n{}", task.title, task.description)
    };

    // Get global config values
    let (base_system_prompt, max_rounds, tool_timeout) = {
        let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
        (
            cfg.default_system_prompt.clone(),
            cfg.max_tool_rounds,
            cfg.tool_timeout_secs,
        )
    };

    // Get skill instructions (shared across agents)
    let skill_instructions = skills::get_enabled_skill_instructions(&state.store).unwrap_or_default();

    // Build tools (shared across agents)
    let mut all_tools = ToolDefinition::builtins();
    let enabled_ids: Vec<String> = skills::builtin_skills().iter()
        .filter(|s| state.store.is_skill_enabled(&s.id).unwrap_or(false))
        .map(|s| s.id.clone())
        .collect();
    if !enabled_ids.is_empty() {
        all_tools.extend(ToolDefinition::skill_tools(&enabled_ids));
    }

    let pending = state.pending_approvals.clone();
    let store_path = crate::engine::sessions::engine_db_path();
    let task_id_for_spawn = task_id.clone();
    let agent_count = agent_ids.len();

    // For each agent, spawn a parallel agent loop
    let mut handles = Vec::new();

    for agent_id in agent_ids.clone() {
        // Per-agent session: eng-task-{task_id}-{agent_id}
        let session_id = format!("eng-task-{}-{}", task.id, agent_id);

        // Create session if needed
        let (provider_config, model) = {
            let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
            let model = cfg.default_model.clone().unwrap_or_else(|| "gpt-4o".to_string());
            let provider = cfg.providers.iter()
                .find(|p| {
                    if model.starts_with("claude") || model.starts_with("anthropic") {
                        p.kind == ProviderKind::Anthropic
                    } else if model.starts_with("gemini") || model.starts_with("google") {
                        p.kind == ProviderKind::Google
                    } else if model.starts_with("gpt") || model.starts_with("o1") || model.starts_with("o3") {
                        p.kind == ProviderKind::OpenAI
                    } else {
                        false
                    }
                })
                .or_else(|| {
                    cfg.default_provider.as_ref()
                        .and_then(|dp| cfg.providers.iter().find(|p| p.id == *dp))
                })
                .or_else(|| cfg.providers.first())
                .cloned();
            match provider {
                Some(p) => (p, model),
                None => return Err("No AI provider configured".into()),
            }
        };

        if state.store.get_session(&session_id).ok().flatten().is_none() {
            state.store.create_session(&session_id, &model, None)?;
        }

        // Compose system prompt with agent-specific soul context
        let agent_context = state.store.compose_agent_context(&agent_id).unwrap_or(None);

        let mut parts: Vec<String> = Vec::new();
        if let Some(sp) = &base_system_prompt { parts.push(sp.clone()); }
        if let Some(ac) = agent_context { parts.push(ac); }
        if !skill_instructions.is_empty() { parts.push(skill_instructions.clone()); }

        // Multi-agent context
        let agent_count_note = if agent_count > 1 {
            format!("\n\nYou are agent '{}', one of {} agents working on this task collaboratively. Focus on your area of expertise. Be thorough but avoid duplicating work other agents may do.", agent_id, agent_count)
        } else {
            String::new()
        };

        parts.push(format!(
            "## Current Task\nYou are working on a task from the task board.\n- **Title:** {}\n- **Priority:** {}{}\n\nComplete this task thoroughly. When done, summarize what you accomplished.",
            task.title, task.priority, agent_count_note
        ));

        let full_system_prompt = parts.join("\n\n---\n\n");

        // Store user message for this agent's session
        let user_msg = StoredMessage {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.clone(),
            role: "user".into(),
            content: task_prompt.clone(),
            tool_calls_json: None,
            tool_call_id: None,
            name: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        state.store.add_message(&user_msg)?;

        // Load conversation
        let mut messages = state.store.load_conversation(&session_id, Some(&full_system_prompt))?;

        let provider = AnyProvider::from_config(&provider_config);
        let pending_clone = pending.clone();
        let task_id_clone = task_id.clone();
        let store_path_clone = store_path.clone();
        let run_id_clone = run_id.clone();
        let app_handle_clone = app_handle.clone();
        let all_tools_clone = all_tools.clone();
        let model_clone = model.clone();

        let handle = tauri::async_runtime::spawn(async move {
            let result = agent_loop::run_agent_turn(
                &app_handle_clone,
                &provider,
                &model_clone,
                &mut messages,
                &all_tools_clone,
                &session_id,
                &run_id_clone,
                max_rounds,
                None,
                &pending_clone,
                tool_timeout,
            ).await;

            // Store agent result
            if let Ok(conn) = rusqlite::Connection::open(&store_path_clone) {
                match &result {
                    Ok(text) => {
                        let msg_id = uuid::Uuid::new_v4().to_string();
                        conn.execute(
                            "INSERT INTO messages (id, session_id, role, content) VALUES (?1, ?2, 'assistant', ?3)",
                            rusqlite::params![msg_id, session_id, text],
                        ).ok();
                        // Activity log
                        let aid = uuid::Uuid::new_v4().to_string();
                        conn.execute(
                            "INSERT INTO task_activity (id, task_id, kind, agent, content) VALUES (?1, ?2, 'agent_completed', ?3, ?4)",
                            rusqlite::params![aid, task_id_clone, agent_id, format!("Agent {} completed. Summary: {}", agent_id, &text[..text.len().min(200)])],
                        ).ok();
                    }
                    Err(err) => {
                        let aid = uuid::Uuid::new_v4().to_string();
                        conn.execute(
                            "INSERT INTO task_activity (id, task_id, kind, agent, content) VALUES (?1, ?2, 'agent_error', ?3, ?4)",
                            rusqlite::params![aid, task_id_clone, agent_id, format!("Agent {} error: {}", agent_id, err)],
                        ).ok();
                    }
                }
            }

            result
        });

        handles.push(handle);
    }

    // Spawn a coordinator that waits for all agents to finish
    let app_handle_final = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        let mut all_ok = true;
        let mut any_ok = false;
        for handle in handles {
            match handle.await {
                Ok(Ok(_)) => { any_ok = true; }
                _ => { all_ok = false; }
            }
        }

        // Update task status based on results
        if let Ok(conn) = rusqlite::Connection::open(&store_path) {
            let new_status = if all_ok { "review" } else if any_ok { "review" } else { "blocked" };
            conn.execute(
                "UPDATE tasks SET status = ?2, updated_at = datetime('now') WHERE id = ?1",
                rusqlite::params![task_id_for_spawn, new_status],
            ).ok();

            // Final summary activity
            let aid = uuid::Uuid::new_v4().to_string();
            let summary = if agent_count > 1 {
                format!("All {} agents finished. Status: {}", agent_count, new_status)
            } else {
                format!("Task completed. Status: {}", new_status)
            };
            conn.execute(
                "INSERT INTO task_activity (id, task_id, kind, content) VALUES (?1, ?2, 'status_change', ?3)",
                rusqlite::params![aid, task_id_for_spawn, summary],
            ).ok();
        }

        // Emit task-updated event
        app_handle_final.emit("task-updated", serde_json::json!({
            "task_id": task_id_for_spawn,
            "status": if any_ok { "review" } else { "blocked" },
        })).ok();
    });

    Ok(run_id)
}

/// Check for due cron tasks and process them.
/// Returns the IDs of tasks that were due and triggered.
#[tauri::command]
pub fn engine_tasks_cron_tick(
    state: State<'_, EngineState>,
) -> Result<Vec<String>, String> {
    let due = state.store.get_due_cron_tasks()?;
    let mut triggered_ids = Vec::new();

    for task in due {
        info!("[engine] Cron task due: {} ({})", task.title, task.id);

        // Update last_run_at and compute next_run_at
        let now = chrono::Utc::now();
        let next = compute_next_run(&task.cron_schedule, &now);
        state.store.update_task_cron_run(&task.id, &now.to_rfc3339(), next.as_deref())?;

        // Log activity
        let aid = uuid::Uuid::new_v4().to_string();
        state.store.add_task_activity(
            &aid, &task.id, "cron_triggered", None,
            &format!("Cron triggered: {}", task.cron_schedule.as_deref().unwrap_or("unknown")),
        )?;

        triggered_ids.push(task.id);
    }

    Ok(triggered_ids)
}

/// Simple schedule parser: "every Xm", "every Xh", "daily HH:MM"
fn compute_next_run(schedule: &Option<String>, from: &chrono::DateTime<chrono::Utc>) -> Option<String> {
    let s = schedule.as_deref()?;
    let s = s.trim().to_lowercase();

    if s.starts_with("every ") {
        let rest = s.strip_prefix("every ")?.trim();
        if rest.ends_with('m') {
            let mins: i64 = rest.trim_end_matches('m').trim().parse().ok()?;
            return Some((*from + chrono::Duration::minutes(mins)).to_rfc3339());
        } else if rest.ends_with('h') {
            let hours: i64 = rest.trim_end_matches('h').trim().parse().ok()?;
            return Some((*from + chrono::Duration::hours(hours)).to_rfc3339());
        }
    } else if s.starts_with("daily ") {
        let time_str = s.strip_prefix("daily ")?.trim();
        let parts: Vec<&str> = time_str.split(':').collect();
        if parts.len() == 2 {
            let hour: u32 = parts[0].parse().ok()?;
            let minute: u32 = parts[1].parse().ok()?;
            let today = from.date_naive();
            let target_time = today.and_hms_opt(hour, minute, 0)?;
            let target = target_time.and_utc();
            if target > *from {
                return Some(target.to_rfc3339());
            } else {
                let tomorrow = today.succ_opt()?;
                let next = tomorrow.and_hms_opt(hour, minute, 0)?.and_utc();
                return Some(next.to_rfc3339());
            }
        }
    }

    // Default: 1 hour from now
    Some((*from + chrono::Duration::hours(1)).to_rfc3339())
}

// ── Telegram Bridge Commands ───────────────────────────────────────────

#[tauri::command]
pub async fn engine_telegram_start(
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    crate::engine::telegram::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_telegram_stop() -> Result<(), String> {
    crate::engine::telegram::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_telegram_status(
    app_handle: tauri::AppHandle,
) -> Result<crate::engine::telegram::TelegramStatus, String> {
    Ok(crate::engine::telegram::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_telegram_get_config(
    app_handle: tauri::AppHandle,
) -> Result<crate::engine::telegram::TelegramConfig, String> {
    crate::engine::telegram::load_telegram_config(&app_handle)
}

#[tauri::command]
pub fn engine_telegram_set_config(
    app_handle: tauri::AppHandle,
    config: crate::engine::telegram::TelegramConfig,
) -> Result<(), String> {
    crate::engine::telegram::save_telegram_config(&app_handle, &config)
}

#[tauri::command]
pub async fn engine_telegram_approve_user(
    app_handle: tauri::AppHandle,
    user_id: i64,
) -> Result<(), String> {
    crate::engine::telegram::approve_user(&app_handle, user_id).await
}

#[tauri::command]
pub async fn engine_telegram_deny_user(
    app_handle: tauri::AppHandle,
    user_id: i64,
) -> Result<(), String> {
    crate::engine::telegram::deny_user(&app_handle, user_id).await
}

#[tauri::command]
pub fn engine_telegram_remove_user(
    app_handle: tauri::AppHandle,
    user_id: i64,
) -> Result<(), String> {
    crate::engine::telegram::remove_user(&app_handle, user_id)
}

// ── Discord Bridge Commands ────────────────────────────────────────────

#[tauri::command]
pub async fn engine_discord_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::discord::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_discord_stop() -> Result<(), String> {
    crate::engine::discord::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_discord_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::discord::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_discord_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::discord::DiscordConfig, String> {
    crate::engine::discord::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_discord_set_config(app_handle: tauri::AppHandle, config: crate::engine::discord::DiscordConfig) -> Result<(), String> {
    crate::engine::discord::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_discord_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::discord::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_discord_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::discord::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_discord_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::discord::remove_user(&app_handle, &user_id)
}

// ── IRC Bridge Commands ────────────────────────────────────────────────

#[tauri::command]
pub async fn engine_irc_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::irc::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_irc_stop() -> Result<(), String> {
    crate::engine::irc::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_irc_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::irc::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_irc_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::irc::IrcConfig, String> {
    crate::engine::irc::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_irc_set_config(app_handle: tauri::AppHandle, config: crate::engine::irc::IrcConfig) -> Result<(), String> {
    crate::engine::irc::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_irc_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::irc::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_irc_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::irc::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_irc_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::irc::remove_user(&app_handle, &user_id)
}

// ── Slack Bridge Commands ──────────────────────────────────────────────

#[tauri::command]
pub async fn engine_slack_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::slack::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_slack_stop() -> Result<(), String> {
    crate::engine::slack::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_slack_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::slack::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_slack_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::slack::SlackConfig, String> {
    crate::engine::slack::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_slack_set_config(app_handle: tauri::AppHandle, config: crate::engine::slack::SlackConfig) -> Result<(), String> {
    crate::engine::slack::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_slack_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::slack::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_slack_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::slack::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_slack_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::slack::remove_user(&app_handle, &user_id)
}

// ── Matrix Bridge Commands ─────────────────────────────────────────────

#[tauri::command]
pub async fn engine_matrix_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::matrix::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_matrix_stop() -> Result<(), String> {
    crate::engine::matrix::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_matrix_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::matrix::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_matrix_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::matrix::MatrixConfig, String> {
    crate::engine::matrix::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_matrix_set_config(app_handle: tauri::AppHandle, config: crate::engine::matrix::MatrixConfig) -> Result<(), String> {
    crate::engine::matrix::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_matrix_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::matrix::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_matrix_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::matrix::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_matrix_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::matrix::remove_user(&app_handle, &user_id)
}

// ── Mattermost Bridge Commands ─────────────────────────────────────────

#[tauri::command]
pub async fn engine_mattermost_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::mattermost::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_mattermost_stop() -> Result<(), String> {
    crate::engine::mattermost::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_mattermost_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::mattermost::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_mattermost_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::mattermost::MattermostConfig, String> {
    crate::engine::mattermost::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_mattermost_set_config(app_handle: tauri::AppHandle, config: crate::engine::mattermost::MattermostConfig) -> Result<(), String> {
    crate::engine::mattermost::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_mattermost_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::mattermost::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_mattermost_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::mattermost::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_mattermost_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::mattermost::remove_user(&app_handle, &user_id)
}

// ── Nextcloud Talk Bridge Commands ─────────────────────────────────────

#[tauri::command]
pub async fn engine_nextcloud_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::nextcloud::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_nextcloud_stop() -> Result<(), String> {
    crate::engine::nextcloud::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_nextcloud_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::nextcloud::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_nextcloud_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::nextcloud::NextcloudConfig, String> {
    crate::engine::nextcloud::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_nextcloud_set_config(app_handle: tauri::AppHandle, config: crate::engine::nextcloud::NextcloudConfig) -> Result<(), String> {
    crate::engine::nextcloud::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_nextcloud_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::nextcloud::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_nextcloud_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::nextcloud::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_nextcloud_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::nextcloud::remove_user(&app_handle, &user_id)
}

// ── Nostr Bridge Commands ──────────────────────────────────────────────

#[tauri::command]
pub async fn engine_nostr_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::nostr::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_nostr_stop() -> Result<(), String> {
    crate::engine::nostr::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_nostr_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::nostr::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_nostr_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::nostr::NostrConfig, String> {
    crate::engine::nostr::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_nostr_set_config(app_handle: tauri::AppHandle, config: crate::engine::nostr::NostrConfig) -> Result<(), String> {
    crate::engine::nostr::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_nostr_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::nostr::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_nostr_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::nostr::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_nostr_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::nostr::remove_user(&app_handle, &user_id)
}

// ── Twitch Bridge Commands ─────────────────────────────────────────────

#[tauri::command]
pub async fn engine_twitch_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::twitch::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_twitch_stop() -> Result<(), String> {
    crate::engine::twitch::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_twitch_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::twitch::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_twitch_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::twitch::TwitchConfig, String> {
    crate::engine::twitch::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_twitch_set_config(app_handle: tauri::AppHandle, config: crate::engine::twitch::TwitchConfig) -> Result<(), String> {
    crate::engine::twitch::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_twitch_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::twitch::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_twitch_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::twitch::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_twitch_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::twitch::remove_user(&app_handle, &user_id)
}

// ── Web Chat Bridge Commands ───────────────────────────────────────────

#[tauri::command]
pub fn engine_webchat_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::webchat::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_webchat_stop() -> Result<(), String> {
    crate::engine::webchat::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_webchat_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::webchat::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_webchat_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::webchat::WebChatConfig, String> {
    crate::engine::webchat::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_webchat_set_config(app_handle: tauri::AppHandle, config: crate::engine::webchat::WebChatConfig) -> Result<(), String> {
    crate::engine::webchat::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_webchat_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::webchat::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_webchat_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::webchat::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_webchat_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::webchat::remove_user(&app_handle, &user_id)
}

// ── Orchestrator: Projects ─────────────────────────────────────────────

#[tauri::command]
pub fn engine_projects_list(state: State<'_, EngineState>) -> Result<Vec<crate::engine::types::Project>, String> {
    state.store.list_projects()
}

#[tauri::command]
pub fn engine_project_create(state: State<'_, EngineState>, project: crate::engine::types::Project) -> Result<(), String> {
    state.store.create_project(&project)
}

#[tauri::command]
pub fn engine_project_update(state: State<'_, EngineState>, project: crate::engine::types::Project) -> Result<(), String> {
    state.store.update_project(&project)
}

#[tauri::command]
pub fn engine_project_delete(state: State<'_, EngineState>, project_id: String) -> Result<(), String> {
    state.store.delete_project(&project_id)
}

#[tauri::command]
pub fn engine_project_set_agents(
    state: State<'_, EngineState>,
    project_id: String,
    agents: Vec<crate::engine::types::ProjectAgent>,
) -> Result<(), String> {
    state.store.set_project_agents(&project_id, &agents)
}

#[tauri::command]
pub fn engine_list_all_agents(
    state: State<'_, EngineState>,
) -> Result<Vec<serde_json::Value>, String> {
    let agents = state.store.list_all_agents()?;
    Ok(agents
        .into_iter()
        .map(|(project_id, agent)| {
            serde_json::json!({
                "project_id": project_id,
                "agent_id": agent.agent_id,
                "role": agent.role,
                "specialty": agent.specialty,
                "status": agent.status,
                "current_task": agent.current_task,
                "model": agent.model,
                "system_prompt": agent.system_prompt,
                "capabilities": agent.capabilities,
            })
        })
        .collect())
}

#[tauri::command]
pub fn engine_project_messages(
    state: State<'_, EngineState>,
    project_id: String,
    limit: Option<i64>,
) -> Result<Vec<crate::engine::types::ProjectMessage>, String> {
    state.store.get_project_messages(&project_id, limit.unwrap_or(100))
}

#[tauri::command]
pub async fn engine_project_run(
    app_handle: tauri::AppHandle,
    project_id: String,
) -> Result<String, String> {
    let run_id = uuid::Uuid::new_v4().to_string();
    let app = app_handle.clone();
    let pid = project_id.clone();

    // Spawn the orchestrator in background
    tauri::async_runtime::spawn(async move {
        match crate::engine::orchestrator::run_project(&app, &pid).await {
            Ok(text) => info!("[orchestrator] Project {} completed: {}...", pid, &text[..text.len().min(200)]),
            Err(e) => error!("[orchestrator] Project {} failed: {}", pid, e),
        }
    });

    Ok(run_id)
}
