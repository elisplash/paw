use crate::OutputFormat;
use openpawz_core::engine::audit;
use openpawz_core::engine::sessions::SessionStore;

pub fn run(store: &SessionStore, format: &OutputFormat) -> Result<(), String> {
    let config_json = store
        .get_config("engine_config")
        .map_err(|e| e.to_string())?;

    let has_config = config_json.is_some();
    let parsed_config = config_json
        .as_ref()
        .and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok());

    let has_provider = parsed_config
        .as_ref()
        .and_then(|v| v.get("providers")?.as_array().map(|a| !a.is_empty()))
        .unwrap_or(false);

    let provider_names: Vec<String> = parsed_config
        .as_ref()
        .and_then(|v| v.get("providers")?.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|p| {
            p.get("kind")
                .and_then(|k| k.as_str())
                .map(|s| s.to_string())
        })
        .collect();

    let default_model = parsed_config
        .as_ref()
        .and_then(|v| v.get("default_model")?.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "-".to_string());

    let daily_budget = parsed_config
        .as_ref()
        .and_then(|v| v.get("daily_budget_usd")?.as_f64())
        .unwrap_or(0.0);

    let memory_config = store
        .get_config("memory_config")
        .map_err(|e| e.to_string())?;
    let has_memory = memory_config.is_some();

    let sessions = store
        .list_sessions_filtered(10000, None)
        .map_err(|e| e.to_string())?;

    let agents = store.list_all_agents().map_err(|e| e.to_string())?;

    let memory_stats = store.memory_stats().ok();

    let audit_stats = audit::stats(store).ok();

    let tasks = store.list_tasks().ok().unwrap_or_default();
    let pending_tasks = tasks.iter().filter(|t| t.status == "pending").count();

    match format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "engine": if has_config { "configured" } else { "not configured" },
                    "provider": if has_provider { "configured" } else { "missing" },
                    "providers": provider_names,
                    "default_model": default_model,
                    "daily_budget_usd": daily_budget,
                    "memory": if has_memory { "configured" } else { "default" },
                    "memory_count": memory_stats.as_ref().map(|s| s.total_memories).unwrap_or(0),
                    "sessions": sessions.len(),
                    "agents": agents.len(),
                    "tasks_pending": pending_tasks,
                    "tasks_total": tasks.len(),
                    "audit_entries": audit_stats.as_ref().map(|s| s.total_entries).unwrap_or(0),
                    "data_dir": openpawz_core::engine::paths::paw_data_dir().to_string_lossy(),
                }))
                .map_err(|e| e.to_string())?
            );
        }
        _ => {
            println!("OpenPawz Engine Status");
            println!("{}", "=".repeat(50));
            println!(
                "  Engine config:  {}",
                if has_config {
                    "\x1b[32mOK\x1b[0m"
                } else {
                    "\x1b[31mNot configured\x1b[0m"
                }
            );
            println!(
                "  AI provider:    {}",
                if has_provider {
                    format!("\x1b[32m{}\x1b[0m", provider_names.join(", "))
                } else {
                    "\x1b[31mMissing — run `openpawz setup`\x1b[0m".to_string()
                }
            );
            println!("  Default model:  {}", default_model);
            if daily_budget > 0.0 {
                println!("  Daily budget:   ${:.2}", daily_budget);
            }
            println!(
                "  Memory config:  {}",
                if has_memory { "Custom" } else { "Default" }
            );
            if let Some(ref ms) = memory_stats {
                println!(
                    "  Memories:       {} (embeddings: {})",
                    ms.total_memories,
                    if ms.has_embeddings { "yes" } else { "no" }
                );
            }
            println!(
                "  Data directory: {}",
                openpawz_core::engine::paths::paw_data_dir().to_string_lossy()
            );
            println!("  Sessions:       {}", sessions.len());
            println!("  Agents:         {}", agents.len());
            if !tasks.is_empty() {
                println!(
                    "  Tasks:          {} ({} pending)",
                    tasks.len(),
                    pending_tasks
                );
            }
            if let Some(ref as_) = audit_stats {
                println!("  Audit entries:  {}", as_.total_entries);
            }
        }
    }
    Ok(())
}
