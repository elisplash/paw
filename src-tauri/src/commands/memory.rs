// commands/memory.rs — Thin wrappers for memory & embedding commands.
// All business logic lives in engine/memory.rs.

use crate::commands::state::EngineState;
use crate::engine::memory;
use crate::engine::types::*;
use log::info;
use tauri::State;

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
    let imp = importance.unwrap_or(5);
    let emb_client = state.embedding_client();
    memory::store_memory(
        &state.store,
        &content,
        &cat,
        imp,
        emb_client.as_ref(),
        agent_id.as_deref(),
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn engine_memory_search(
    state: State<'_, EngineState>,
    query: String,
    limit: Option<usize>,
    agent_id: Option<String>,
) -> Result<Vec<Memory>, String> {
    let lim = limit.unwrap_or(10);
    let threshold = state.memory_config.lock().recall_threshold;
    let emb_client = state.embedding_client();
    memory::search_memories(
        &state.store,
        &query,
        lim,
        threshold,
        emb_client.as_ref(),
        agent_id.as_deref(),
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_memory_stats(state: State<'_, EngineState>) -> Result<MemoryStats, String> {
    state.store.memory_stats().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_memory_delete(state: State<'_, EngineState>, id: String) -> Result<(), String> {
    state.store.delete_memory(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_memory_list(
    state: State<'_, EngineState>,
    limit: Option<usize>,
) -> Result<Vec<Memory>, String> {
    state
        .store
        .list_memories(limit.unwrap_or(100))
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
