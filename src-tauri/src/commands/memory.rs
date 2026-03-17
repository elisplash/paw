// commands/memory.rs — Thin wrappers for memory & embedding commands.
// Routes through Engram (3-tier memory) for store/search, and the Engram
// session store for CRUD. The old engine::memory module is no longer used
// for core CRUD — only shared utilities (EmbeddingClient) remain.

use crate::commands::state::EngineState;
use crate::engine::engram;
use crate::engine::memory; // Still needed for backfill, embeddings, ensure_ollama_ready
use crate::engine::types::*;
use log::info;
use tauri::State;

/// Convert an EpisodicMemory to the frontend-facing Memory type.
fn episodic_to_memory(mem: crate::atoms::engram_types::EpisodicMemory) -> Memory {
    // Decrypt content for display (per-agent HKDF key)
    let content = if let Ok(key) = engram::encryption::get_agent_encryption_key(&mem.agent_id) {
        engram::encryption::decrypt_memory_content(&mem.content.full, &key)
            .unwrap_or(mem.content.full)
    } else {
        mem.content.full
    };

    Memory {
        id: mem.id,
        content,
        category: mem.category,
        importance: (mem.importance * 10.0).round() as u8,
        created_at: mem.created_at,
        score: None,
        agent_id: Some(mem.agent_id),
    }
}

// ── Memory CRUD ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn engine_memory_store(
    state: State<'_, EngineState>,
    content: String,
    category: Option<String>,
    importance: Option<u8>,
    agent_id: Option<String>,
) -> Result<String, String> {
    let cat = category.unwrap_or_else(|| "general".into());
    let imp_f32 = importance.unwrap_or(5) as f32 / 10.0; // Convert 0-10 → 0.0-1.0
    let emb_client = state.embedding_client();
    engram::bridge::store(
        &state.store,
        &content,
        &cat,
        imp_f32,
        emb_client.as_ref(),
        agent_id.as_deref(),
        None, // no session_id for explicit stores
        Some(&state.hnsw_index),
    )
    .await
    .map(|opt| opt.unwrap_or_default())
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn engine_memory_search(
    state: State<'_, EngineState>,
    query: String,
    limit: Option<usize>,
    agent_id: Option<String>,
    // §11 Frontend SearchConfig wiring — optional override for search parameters
    threshold: Option<f64>,
) -> Result<Vec<Memory>, String> {
    let lim = limit.unwrap_or(10);
    let thresh = threshold.unwrap_or_else(|| state.memory_config.lock().recall_threshold);
    let emb_client = state.embedding_client();
    let results = engram::bridge::search(
        &state.store,
        &query,
        lim,
        thresh,
        emb_client.as_ref(),
        agent_id.as_deref(),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(results
        .into_iter()
        .map(|r| Memory {
            id: r.id,
            content: r.content,
            category: r.category,
            importance: 5, // bridge::search doesn't expose importance
            created_at: String::new(),
            score: Some(r.score),
            agent_id: agent_id.clone(),
        })
        .collect())
}

#[tauri::command]
pub fn engine_memory_stats(state: State<'_, EngineState>) -> Result<MemoryStats, String> {
    state.store.memory_stats().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_memory_get(
    state: State<'_, EngineState>,
    id: String,
) -> Result<Option<Memory>, String> {
    match state.store.engram_get_episodic(&id) {
        Ok(Some(mem)) => Ok(Some(episodic_to_memory(mem))),
        Ok(None) => {
            // Fallback: check old memory table for backward compat
            state.store.get_memory_by_id(&id).map_err(|e| e.to_string())
        }
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn engine_memory_update(
    state: State<'_, EngineState>,
    id: String,
    content: String,
    category: String,
    importance: u8,
) -> Result<(), String> {
    // Try Engram first
    if state
        .store
        .engram_get_episodic(&id)
        .ok()
        .flatten()
        .is_some()
    {
        state
            .store
            .engram_update_episodic_content(&id, &content, None)
            .map(|_| ())
            .map_err(|e| e.to_string())
    } else {
        // Fallback to old memory table
        state
            .store
            .update_memory(&id, &content, &category, importance)
            .map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub fn engine_memory_delete(state: State<'_, EngineState>, id: String) -> Result<(), String> {
    // Try Engram first
    if state
        .store
        .engram_get_episodic(&id)
        .ok()
        .flatten()
        .is_some()
    {
        state
            .store
            .engram_delete_episodic(&id)
            .map_err(|e| e.to_string())
    } else {
        // Fallback to old memory table
        state.store.delete_memory(&id).map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub fn engine_memory_list(
    state: State<'_, EngineState>,
    limit: Option<usize>,
) -> Result<Vec<Memory>, String> {
    let scope = crate::atoms::engram_types::MemoryScope {
        global: true,
        ..Default::default()
    };
    let lim = limit.unwrap_or(100);

    // Collect from both episodic (new) and legacy memories tables
    let mut results: Vec<Memory> = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();

    // New engram episodic memories
    match state.store.engram_list_episodic(&scope, None, lim) {
        Ok(memories) => {
            for mem in memories {
                seen_ids.insert(mem.id.clone());
                results.push(episodic_to_memory(mem));
            }
        }
        Err(e) => {
            info!("[memory] Engram list failed ({}), skipping episodic", e);
        }
    }

    // Old legacy memories (fill remaining slots, skip duplicates)
    let remaining = lim.saturating_sub(results.len());
    if remaining > 0 {
        match state.store.list_memories(remaining) {
            Ok(old_memories) => {
                for mem in old_memories {
                    if !seen_ids.contains(&mem.id) {
                        seen_ids.insert(mem.id.clone());
                        results.push(mem);
                    }
                }
            }
            Err(e) => {
                info!("[memory] Legacy list failed ({})", e);
            }
        }
    }

    // Sort combined results by created_at descending
    results.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(results)
}

#[tauri::command]
pub fn engine_memory_delete_by_session(
    state: State<'_, EngineState>,
    session_id: String,
) -> Result<usize, String> {
    state
        .store
        .engram_delete_episodic_by_session(&session_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_memory_edges(
    state: State<'_, EngineState>,
    limit: Option<usize>,
) -> Result<Vec<crate::atoms::engram_types::MemoryEdge>, String> {
    state
        .store
        .engram_list_all_edges(limit.unwrap_or(500))
        .map_err(|e| e.to_string())
}

// ── Memory config ──────────────────────────────────────────────────────

#[tauri::command]
pub fn engine_get_memory_config(state: State<'_, EngineState>) -> Result<MemoryConfig, String> {
    let cfg = state.memory_config.lock();
    Ok(cfg.clone())
}

#[tauri::command]
pub fn engine_set_memory_config(
    state: State<'_, EngineState>,
    config: MemoryConfig,
) -> Result<(), String> {
    let json = serde_json::to_string(&config).map_err(|e| format!("Serialize error: {}", e))?;
    state.store.set_config("memory_config", &json)?;
    let mut cfg = state.memory_config.lock();
    *cfg = config;
    info!("[engine] Memory config updated");
    Ok(())
}

// ── Embedding / Ollama ─────────────────────────────────────────────────

#[tauri::command]
pub async fn engine_test_embedding(state: State<'_, EngineState>) -> Result<usize, String> {
    let client = state.embedding_client().ok_or_else(|| {
        "No embedding configuration — set base URL and model in memory settings".to_string()
    })?;
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
        None => {
            return Ok(serde_json::json!({
                "ollama_running": false,
                "model_available": false,
                "model_name": "",
                "error": "No embedding configuration"
            }))
        }
    };

    let model_name = {
        let cfg = state.memory_config.lock();
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
pub async fn engine_embedding_pull_model(state: State<'_, EngineState>) -> Result<String, String> {
    let client = state
        .embedding_client()
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
        let cfg = state.memory_config.lock();
        cfg.clone()
    };

    let status = memory::ensure_ollama_ready(&config).await;

    // If we discovered the actual dimensions, update the config
    if status.embedding_dims > 0 {
        let mut cfg = state.memory_config.lock();
        if cfg.embedding_dims != status.embedding_dims {
            info!(
                "[engine] Updating embedding_dims from {} to {} based on actual model output",
                cfg.embedding_dims, status.embedding_dims
            );
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
    let client = state.embedding_client().ok_or_else(|| {
        "No embedding configuration — Ollama must be running with an embedding model".to_string()
    })?;

    let (success, fail) = memory::backfill_embeddings(&state.store, &client).await?;
    Ok(serde_json::json!({
        "success": success,
        "failed": fail,
    }))
}

/// Save working memory snapshot for an agent (called on agent switch).
#[tauri::command]
pub fn engine_working_memory_save(
    state: State<'_, EngineState>,
    agent_id: String,
) -> Result<(), String> {
    use crate::atoms::engram_types::WorkingMemorySnapshot;

    // Build a snapshot with the agent's ID and current timestamp
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let snapshot = WorkingMemorySnapshot {
        agent_id: agent_id.clone(),
        slots: Vec::new(),
        momentum_embeddings: Vec::new(),
        saved_at: now,
    };

    state
        .store
        .engram_save_snapshot(&snapshot)
        .map_err(|e| e.to_string())?;

    log::info!(
        "[engram] Working memory snapshot saved for agent '{}'",
        agent_id
    );
    Ok(())
}

/// Restore working memory snapshot for an agent (called on agent switch).
#[tauri::command]
pub fn engine_working_memory_restore(
    state: State<'_, EngineState>,
    agent_id: String,
) -> Result<serde_json::Value, String> {
    match state.store.engram_load_snapshot(&agent_id) {
        Ok(Some(snapshot)) => {
            log::info!("[engram] Working memory restored for agent '{}'", agent_id);
            let value = serde_json::to_value(&snapshot).unwrap_or(serde_json::json!(null));
            Ok(value)
        }
        Ok(None) => Ok(serde_json::json!(null)),
        Err(e) => Err(e.to_string()),
    }
}

/// GDPR right-to-erasure: purge ALL memories for given user identifiers.
/// This securely erases episodic, semantic, procedural memories, snapshots,
/// and audit log entries. Implements Article 17 right to be forgotten.
#[tauri::command]
pub fn engine_memory_purge_user(
    state: State<'_, EngineState>,
    identifiers: Vec<String>,
) -> Result<serde_json::Value, String> {
    use crate::engine::engram::encryption::{engram_purge_user, UserPurgeRequest};

    let request = UserPurgeRequest { identifiers };
    let result = engram_purge_user(&state.store, &request).map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "records_erased": result.records_erased,
        "identifiers_processed": result.identifiers_processed,
    }))
}

// ── Message Feedback (RLHF) ───────────────────────────────────────────

/// Record user feedback (thumbs up/down) on an assistant message.
/// Updates trust scores on the agent's episodic memories to improve
/// future memory relevance through reinforcement learning.
#[tauri::command]
pub fn engine_message_feedback(
    state: State<'_, EngineState>,
    session_id: String,
    message_id: String,
    agent_id: String,
    helpful: bool,
    context: Option<String>,
) -> Result<serde_json::Value, String> {
    // Store the feedback record
    let feedback_id = state
        .store
        .store_message_feedback(
            &session_id,
            &message_id,
            &agent_id,
            helpful,
            context.as_deref(),
        )
        .map_err(|e| e.to_string())?;

    // Update trust scores on the agent's episodic memories
    let updated = state
        .store
        .update_trust_from_feedback(&agent_id, helpful)
        .unwrap_or(0);

    info!(
        "[engine] Message feedback recorded: {} (helpful={}, trust updated {} memories)",
        &feedback_id[..8.min(feedback_id.len())],
        helpful,
        updated
    );

    // Get cumulative stats
    let (pos, neg) = state.store.get_feedback_stats(&agent_id).unwrap_or((0, 0));

    Ok(serde_json::json!({
        "feedback_id": feedback_id,
        "memories_updated": updated,
        "total_positive": pos,
        "total_negative": neg,
    }))
}
