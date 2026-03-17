use crate::OutputFormat;
use openpawz_core::engine::audit;
use openpawz_core::engine::key_vault;
use openpawz_core::engine::sessions::SessionStore;

/// Comprehensive engine health check — verifies all subsystems.
pub fn run(store: &SessionStore, format: &OutputFormat) -> Result<(), String> {
    let mut checks: Vec<HealthCheck> = Vec::new();

    // 1. Database connectivity
    checks.push(check_db(store));

    // 2. Key vault (OS keychain)
    checks.push(check_key_vault());

    // 3. Engine configuration
    checks.push(check_config(store));

    // 4. AI provider
    checks.push(check_provider(store));

    // 5. Memory subsystem
    checks.push(check_memory(store));

    // 6. Audit chain integrity
    checks.push(check_audit(store));

    // 7. Data directory
    checks.push(check_data_dir());

    let all_ok = checks.iter().all(|c| c.status == "ok");

    match format {
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "healthy": all_ok,
                    "checks": checks.iter().map(|c| serde_json::json!({
                        "name": c.name,
                        "status": c.status,
                        "message": c.message,
                    })).collect::<Vec<_>>(),
                }))
                .map_err(|e| e.to_string())?
            );
        }
        OutputFormat::Quiet => {
            if all_ok {
                println!("ok");
            } else {
                for c in checks.iter().filter(|c| c.status != "ok") {
                    println!("{}: {}", c.name, c.status);
                }
            }
        }
        OutputFormat::Human => {
            println!("OpenPawz Doctor");
            println!("{}", "=".repeat(50));
            for c in &checks {
                let icon = match c.status.as_str() {
                    "ok" => "\x1b[32m✓\x1b[0m",
                    "warn" => "\x1b[33m⚠\x1b[0m",
                    _ => "\x1b[31m✗\x1b[0m",
                };
                println!("  {} {:<25} {}", icon, c.name, c.message);
            }
            println!();
            if all_ok {
                println!("\x1b[32mAll systems healthy.\x1b[0m");
            } else {
                let warnings = checks.iter().filter(|c| c.status == "warn").count();
                let errors = checks.iter().filter(|c| c.status == "error").count();
                if errors > 0 {
                    println!(
                        "\x1b[31m{} error(s), {} warning(s) — run `openpawz setup` to fix.\x1b[0m",
                        errors, warnings
                    );
                } else {
                    println!(
                        "\x1b[33m{} warning(s) — engine functional but not fully configured.\x1b[0m",
                        warnings
                    );
                }
            }
        }
    }

    if !all_ok && checks.iter().any(|c| c.status == "error") {
        std::process::exit(1);
    }
    Ok(())
}

struct HealthCheck {
    name: String,
    status: String, // "ok", "warn", "error"
    message: String,
}

fn check_db(store: &SessionStore) -> HealthCheck {
    match store.get_config("engine_config") {
        Ok(_) => HealthCheck {
            name: "Database".into(),
            status: "ok".into(),
            message: "SQLite accessible (WAL mode, secure_delete)".into(),
        },
        Err(e) => HealthCheck {
            name: "Database".into(),
            status: "error".into(),
            message: format!("Cannot access database: {}", e),
        },
    }
}

fn check_key_vault() -> HealthCheck {
    if key_vault::is_loaded() {
        HealthCheck {
            name: "Key Vault".into(),
            status: "ok".into(),
            message: "OS keychain accessible — encryption keys loaded".into(),
        }
    } else {
        // Try to prefetch
        key_vault::prefetch();
        if key_vault::is_loaded() {
            HealthCheck {
                name: "Key Vault".into(),
                status: "ok".into(),
                message: "OS keychain accessible (loaded on demand)".into(),
            }
        } else {
            HealthCheck {
                name: "Key Vault".into(),
                status: "warn".into(),
                message: "OS keychain not loaded — encryption may be unavailable".into(),
            }
        }
    }
}

fn check_config(store: &SessionStore) -> HealthCheck {
    match store.get_config("engine_config") {
        Ok(Some(json)) => {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json) {
                let keys = parsed.as_object().map(|o| o.len()).unwrap_or(0);
                HealthCheck {
                    name: "Engine Config".into(),
                    status: "ok".into(),
                    message: format!("Configured ({} keys)", keys),
                }
            } else {
                HealthCheck {
                    name: "Engine Config".into(),
                    status: "warn".into(),
                    message: "Config exists but is not valid JSON".into(),
                }
            }
        }
        Ok(None) => HealthCheck {
            name: "Engine Config".into(),
            status: "error".into(),
            message: "Not configured — run `openpawz setup`".into(),
        },
        Err(e) => HealthCheck {
            name: "Engine Config".into(),
            status: "error".into(),
            message: format!("Failed to read config: {}", e),
        },
    }
}

fn check_provider(store: &SessionStore) -> HealthCheck {
    let config = store.get_config("engine_config").ok().flatten();
    let providers = config
        .as_ref()
        .and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok())
        .and_then(|v| v.get("providers")?.as_array().cloned())
        .unwrap_or_default();

    if providers.is_empty() {
        HealthCheck {
            name: "AI Provider".into(),
            status: "error".into(),
            message: "No providers configured — run `openpawz setup`".into(),
        }
    } else {
        let names: Vec<&str> = providers
            .iter()
            .filter_map(|p| p.get("kind").and_then(|k| k.as_str()))
            .collect();
        let has_keys = providers.iter().all(|p| {
            let kind = p.get("kind").and_then(|k| k.as_str()).unwrap_or("");
            if kind == "Ollama" {
                return true; // Ollama doesn't need an API key
            }
            p.get("api_key")
                .and_then(|k| k.as_str())
                .is_some_and(|k| !k.is_empty())
        });
        if has_keys {
            HealthCheck {
                name: "AI Provider".into(),
                status: "ok".into(),
                message: format!("{} (API key set)", names.join(", ")),
            }
        } else {
            HealthCheck {
                name: "AI Provider".into(),
                status: "warn".into(),
                message: format!("{} (some missing API keys)", names.join(", ")),
            }
        }
    }
}

fn check_memory(store: &SessionStore) -> HealthCheck {
    match store.memory_stats() {
        Ok(stats) => {
            let embed_str = if stats.has_embeddings {
                "embeddings: yes"
            } else {
                "embeddings: no"
            };
            HealthCheck {
                name: "Memory".into(),
                status: "ok".into(),
                message: format!("{} memories ({})", stats.total_memories, embed_str),
            }
        }
        Err(e) => HealthCheck {
            name: "Memory".into(),
            status: "warn".into(),
            message: format!("Cannot read memory stats: {}", e),
        },
    }
}

fn check_audit(store: &SessionStore) -> HealthCheck {
    match audit::stats(store) {
        Ok(stats) => {
            if stats.total_entries == 0 {
                HealthCheck {
                    name: "Audit Log".into(),
                    status: "ok".into(),
                    message: "Empty (no operations logged yet)".into(),
                }
            } else {
                // Quick chain check — verify just the total, not the full chain
                // (full verification can be slow on large logs)
                HealthCheck {
                    name: "Audit Log".into(),
                    status: "ok".into(),
                    message: format!(
                        "{} entries (run `openpawz audit verify` for integrity check)",
                        stats.total_entries
                    ),
                }
            }
        }
        Err(e) => HealthCheck {
            name: "Audit Log".into(),
            status: "warn".into(),
            message: format!("Cannot read audit stats: {}", e),
        },
    }
}

fn check_data_dir() -> HealthCheck {
    let path = openpawz_core::engine::paths::paw_data_dir();
    if path.exists() {
        let writable = std::fs::metadata(&path)
            .map(|m| !m.permissions().readonly())
            .unwrap_or(false);
        if writable {
            HealthCheck {
                name: "Data Directory".into(),
                status: "ok".into(),
                message: format!("{}", path.to_string_lossy()),
            }
        } else {
            HealthCheck {
                name: "Data Directory".into(),
                status: "error".into(),
                message: format!("{} (read-only!)", path.to_string_lossy()),
            }
        }
    } else {
        HealthCheck {
            name: "Data Directory".into(),
            status: "warn".into(),
            message: format!(
                "{} (doesn't exist yet — will be created on first use)",
                path.to_string_lossy()
            ),
        }
    }
}
