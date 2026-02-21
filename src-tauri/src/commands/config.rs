// commands/config.rs — Thin wrappers for engine config, sandbox, and auto-setup.

use crate::commands::state::EngineState;
use crate::engine::types::*;
use log::info;
use std::sync::atomic::Ordering;
use tauri::State;

// ── Sandbox ────────────────────────────────────────────────────────────

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
    crate::engine::sandbox::save_sandbox_config(&state.store, &config).map_err(|e| e.to_string())
}

// ── Engine configuration ───────────────────────────────────────────────

#[tauri::command]
pub fn engine_get_config(
    state: State<'_, EngineState>,
) -> Result<EngineConfig, String> {
    let cfg = state.config.lock();
    Ok(cfg.clone())
}

/// Get the current daily token spend and budget status.
#[tauri::command]
pub fn engine_get_daily_spend(
    state: State<'_, EngineState>,
) -> Result<serde_json::Value, String> {
    let (input_tokens, output_tokens, estimated_usd) = state.daily_tokens.estimated_spend_usd();
    let cache_read = state.daily_tokens.cache_read_tokens.load(Ordering::Relaxed);
    let cache_create = state.daily_tokens.cache_create_tokens.load(Ordering::Relaxed);
    let budget = {
        let cfg = state.config.lock();
        cfg.daily_budget_usd
    };
    let budget_pct = if budget > 0.0 { (estimated_usd / budget * 100.0).min(100.0) } else { 0.0 };
    Ok(serde_json::json!({
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "cache_read_tokens": cache_read,
        "cache_create_tokens": cache_create,
        "estimated_usd": format!("{:.2}", estimated_usd),
        "budget_usd": budget,
        "budget_pct": format!("{:.0}", budget_pct),
        "over_budget": budget > 0.0 && estimated_usd >= budget,
    }))
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
    let mut cfg = state.config.lock();
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
    let mut cfg = state.config.lock();

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
    let mut cfg = state.config.lock();

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
    let cfg = state.config.lock();

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
        let cfg = state.config.lock();
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
                log::warn!("[engine] Model pull returned {}", status);
                "llama3.2:3b".to_string() // set it anyway, user can fix
            }
            Err(e) => {
                log::warn!("[engine] Model pull failed: {}", e);
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
        let mut cfg = state.config.lock();
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
