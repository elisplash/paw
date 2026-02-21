// Paw Agent Engine — Ollama Lifecycle Management
//
// Auto-start, model discovery, and model pulling for the local Ollama instance.
// Called at startup by `ensure_ollama_ready()` to guarantee the embedding
// model is available before the memory system starts.

use crate::engine::types::*;
use crate::atoms::error::EngineResult;
use log::{info, warn, error};
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};

use super::embedding::EmbeddingClient;

/// Track whether we've already run ensure_ollama_ready this session.
static OLLAMA_INIT_DONE: AtomicBool = AtomicBool::new(false);

/// Status returned by ensure_ollama_ready.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OllamaReadyStatus {
    pub ollama_running: bool,
    pub was_auto_started: bool,
    pub model_available: bool,
    pub was_auto_pulled: bool,
    pub model_name: String,
    pub embedding_dims: usize,
    pub error: Option<String>,
}

/// Ensure Ollama is running and the embedding model is available.
/// This is the "just works" function — call it at startup and it handles everything:
/// 1. Checks if Ollama is reachable at the configured URL
/// 2. If not, tries to start `ollama serve` as a background process
/// 3. Checks if the configured embedding model is available
/// 4. If not, pulls it automatically
/// 5. Does a test embedding to verify everything works
pub async fn ensure_ollama_ready(config: &MemoryConfig) -> OllamaReadyStatus {
    let client = Client::new();
    let base_url = config.embedding_base_url.trim_end_matches('/');
    let model = &config.embedding_model;

    let mut status = OllamaReadyStatus {
        ollama_running: false,
        was_auto_started: false,
        model_available: false,
        was_auto_pulled: false,
        model_name: model.clone(),
        embedding_dims: 0,
        error: None,
    };

    // Skip if base_url isn't localhost (can't auto-start remote Ollama)
    let is_local = base_url.contains("localhost") || base_url.contains("127.0.0.1");

    // ── Step 1: Check if Ollama is reachable ──
    let reachable = check_ollama_reachable(&client, base_url).await;
    if reachable {
        info!("[memory] Ollama is already running at {}", base_url);
        status.ollama_running = true;
    } else if is_local {
        // ── Step 2: Try to start Ollama ──
        info!("[memory] Ollama not reachable at {} — attempting to start...", base_url);
        match start_ollama_process().await {
            Ok(()) => {
                // Wait for it to become reachable (up to 15 seconds)
                let mut started = false;
                for i in 0..30 {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    if check_ollama_reachable(&client, base_url).await {
                        info!("[memory] Ollama started successfully after {}ms", (i + 1) * 500);
                        started = true;
                        break;
                    }
                }
                if started {
                    status.ollama_running = true;
                    status.was_auto_started = true;
                } else {
                    let msg = "Started Ollama process but it didn't become reachable within 15 seconds".to_string();
                    warn!("[memory] {}", msg);
                    status.error = Some(msg);
                    return status;
                }
            }
            Err(e) => {
                let msg = format!("Ollama not running and auto-start failed: {}. Install Ollama from https://ollama.ai", e);
                warn!("[memory] {}", msg);
                status.error = Some(msg);
                return status;
            }
        }
    } else {
        let msg = format!("Ollama not reachable at {} (remote server — cannot auto-start)", base_url);
        warn!("[memory] {}", msg);
        status.error = Some(msg);
        return status;
    }

    // ── Step 3: Check if model is available ──
    match check_model_available_static(&client, base_url, model).await {
        Ok(true) => {
            info!("[memory] Embedding model '{}' is available", model);
            status.model_available = true;
        }
        Ok(false) => {
            // ── Step 4: Pull the model ──
            info!("[memory] Model '{}' not found, pulling...", model);
            match pull_model_static(&client, base_url, model).await {
                Ok(()) => {
                    info!("[memory] Model '{}' pulled successfully", model);
                    status.model_available = true;
                    status.was_auto_pulled = true;
                }
                Err(e) => {
                    let msg = format!("Failed to pull embedding model '{}': {}", model, e);
                    error!("[memory] {}", msg);
                    status.error = Some(msg);
                    return status;
                }
            }
        }
        Err(e) => {
            warn!("[memory] Could not check model availability: {}", e);
            // Try pulling anyway
            info!("[memory] Attempting to pull '{}' anyway...", model);
            match pull_model_static(&client, base_url, model).await {
                Ok(()) => {
                    status.model_available = true;
                    status.was_auto_pulled = true;
                }
                Err(pull_e) => {
                    let msg = format!("Cannot verify or pull model '{}': check={}, pull={}", model, e, pull_e);
                    error!("[memory] {}", msg);
                    status.error = Some(msg);
                    return status;
                }
            }
        }
    }

    // ── Step 5: Test embedding to get dimensions ──
    let emb_client = EmbeddingClient::new(config);
    match emb_client.embed("test").await {
        Ok(vec) => {
            info!("[memory] ✓ Embedding test passed — {} dimensions", vec.len());
            status.embedding_dims = vec.len();
        }
        Err(e) => {
            let msg = format!("Ollama and model ready, but test embedding failed: {}", e);
            warn!("[memory] {}", msg);
            status.error = Some(msg);
        }
    }

    OLLAMA_INIT_DONE.store(true, Ordering::SeqCst);
    status
}

/// Check if Ollama initialization has already been done this session.
pub fn is_ollama_init_done() -> bool {
    OLLAMA_INIT_DONE.load(Ordering::SeqCst)
}

/// Check if Ollama is reachable by hitting the /api/tags endpoint.
async fn check_ollama_reachable(client: &Client, base_url: &str) -> bool {
    match client.get(format!("{}/api/tags", base_url))
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

/// Try to start Ollama by spawning `ollama serve` as a detached background process.
async fn start_ollama_process() -> EngineResult<()> {
    let ollama_path = which_ollama();
    let path = ollama_path.ok_or_else(|| {
        "Ollama binary not found in PATH. Install Ollama from https://ollama.ai".to_string()
    })?;

    info!("[memory] Starting ollama serve from: {}", path);

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        Command::new(&path)
            .arg("serve")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(0x00000008) // DETACHED_PROCESS
            .spawn()?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::process::Command;
        Command::new(&path)
            .arg("serve")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
    }

    Ok(())
}

/// Find the `ollama` binary in PATH.
fn which_ollama() -> Option<String> {
    let candidates = if cfg!(target_os = "windows") {
        vec![
            "ollama".to_string(),
            format!("{}\\AppData\\Local\\Programs\\Ollama\\ollama.exe", std::env::var("USERPROFILE").unwrap_or_default()),
        ]
    } else if cfg!(target_os = "macos") {
        vec![
            "ollama".to_string(),
            "/usr/local/bin/ollama".to_string(),
            "/opt/homebrew/bin/ollama".to_string(),
            format!("{}/bin/ollama", std::env::var("HOME").unwrap_or_default()),
        ]
    } else {
        vec![
            "ollama".to_string(),
            "/usr/local/bin/ollama".to_string(),
            "/usr/bin/ollama".to_string(),
            format!("{}/.local/bin/ollama", std::env::var("HOME").unwrap_or_default()),
        ]
    };

    for candidate in &candidates {
        if let Ok(output) = std::process::Command::new(candidate)
            .arg("--version")
            .output()
        {
            if output.status.success() {
                return Some(candidate.clone());
            }
        }
    }

    let which_cmd = if cfg!(target_os = "windows") { "where" } else { "which" };
    if let Ok(output) = std::process::Command::new(which_cmd)
        .arg("ollama")
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
    }

    None
}

/// Check if a model is available in Ollama (static version, no &self).
pub(crate) async fn check_model_available_static(client: &Client, base_url: &str, model: &str) -> EngineResult<bool> {
    let url = format!("{}/api/tags", base_url);
    let resp = client.get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err("Ollama returned an error".into());
    }

    let v: Value = resp.json().await?;

    if let Some(models) = v["models"].as_array() {
        let model_base = model.split(':').next().unwrap_or(model);
        for m in models {
            for key in &["name", "model"] {
                if let Some(name) = m[key].as_str() {
                    let name_base = name.split(':').next().unwrap_or(name);
                    if name_base == model_base || name == model {
                        return Ok(true);
                    }
                }
            }
        }
    }
    Ok(false)
}

/// Pull a model from Ollama (static version, no &self).
pub(crate) async fn pull_model_static(client: &Client, base_url: &str, model: &str) -> EngineResult<()> {
    let url = format!("{}/api/pull", base_url);
    let body = json!({
        "name": model,
        "stream": false,
    });

    info!("[memory] Pulling model '{}' (this may take a few minutes for first download)...", model);

    let resp = client.post(&url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(600))
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Pull failed {} — {}", status, text).into());
    }

    info!("[memory] Model '{}' pull complete", model);
    Ok(())
}
