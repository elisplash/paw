use crate::OutputFormat;
use clap::Subcommand;
use openpawz_core::engine::sessions::SessionStore;

/// Known configuration keys — validated before writing.
const ALLOWED_CONFIG_KEYS: &[&str] = &[
    "default_model",
    "default_provider",
    "daily_budget_usd",
    "max_tool_rounds",
    "max_tokens",
    "temperature",
    "system_prompt",
    "memory_enabled",
    "memory_auto_store",
    "memory_recall_threshold",
    "sandbox_enabled",
    "sandbox_image",
    "tool_timeout_secs",
    "max_concurrent_runs",
];

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Show current engine configuration
    Get,
    /// Set a configuration value
    Set {
        /// Configuration key (e.g. "default_model", "daily_budget_usd")
        key: String,
        /// New value
        value: String,
    },
    /// List all valid configuration keys
    Keys,
    /// Show configured AI providers
    Providers,
    /// Get the data directory path
    Path,
}

pub fn run(
    store: &SessionStore,
    action: ConfigAction,
    format: &OutputFormat,
) -> Result<(), String> {
    match action {
        ConfigAction::Get => {
            let config_json = store
                .get_config("engine_config")
                .map_err(|e| e.to_string())?;
            match config_json {
                Some(json) => {
                    match format {
                        OutputFormat::Json | OutputFormat::Human => {
                            // Pretty-print the JSON
                            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json) {
                                println!(
                                    "{}",
                                    serde_json::to_string_pretty(&parsed)
                                        .map_err(|e| e.to_string())?
                                );
                            } else {
                                println!("{}", json);
                            }
                        }
                        OutputFormat::Quiet => println!("{}", json),
                    }
                }
                None => match format {
                    OutputFormat::Quiet => {}
                    _ => println!("No engine configuration found. Run `openpawz setup` first."),
                },
            }
            Ok(())
        }
        ConfigAction::Set { key, value } => {
            // Validate key against allowlist to prevent arbitrary key injection
            if !ALLOWED_CONFIG_KEYS.contains(&key.as_str()) {
                return Err(format!(
                    "Unknown config key '{}'. Use `openpawz config keys` to list valid keys.",
                    key
                ));
            }

            // Reject null bytes in value
            if value.contains('\0') {
                return Err("Config value must not contain null bytes.".into());
            }

            // Load existing config, patch the key, save back
            let config_json = store
                .get_config("engine_config")
                .map_err(|e| e.to_string())?
                .unwrap_or_else(|| "{}".to_string());

            let mut config: serde_json::Value =
                serde_json::from_str(&config_json).map_err(|e| e.to_string())?;

            // Try to parse value as JSON first, fall back to string
            let parsed_value: serde_json::Value =
                serde_json::from_str(&value).unwrap_or(serde_json::Value::String(value.clone()));

            config[&key] = parsed_value;

            let updated = serde_json::to_string(&config).map_err(|e| e.to_string())?;
            store
                .set_config("engine_config", &updated)
                .map_err(|e| e.to_string())?;

            match format {
                OutputFormat::Quiet => {}
                _ => println!("Set {} = {}", key, value),
            }
            Ok(())
        }
        ConfigAction::Keys => {
            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&ALLOWED_CONFIG_KEYS)
                            .map_err(|e| e.to_string())?
                    );
                }
                OutputFormat::Quiet => {
                    for k in ALLOWED_CONFIG_KEYS {
                        println!("{}", k);
                    }
                }
                OutputFormat::Human => {
                    println!("Valid configuration keys:");
                    println!("{}", "-".repeat(30));
                    for k in ALLOWED_CONFIG_KEYS {
                        println!("  {}", k);
                    }
                }
            }
            Ok(())
        }
        ConfigAction::Providers => {
            let config_json = store
                .get_config("engine_config")
                .map_err(|e| e.to_string())?;
            let providers = config_json
                .as_ref()
                .and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok())
                .and_then(|v| v.get("providers")?.as_array().cloned())
                .unwrap_or_default();

            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&providers).map_err(|e| e.to_string())?
                    );
                }
                OutputFormat::Quiet => {
                    for p in &providers {
                        if let Some(id) = p.get("id").and_then(|v| v.as_str()) {
                            println!("{}", id);
                        }
                    }
                }
                OutputFormat::Human => {
                    if providers.is_empty() {
                        println!("No providers configured. Run `openpawz setup`.");
                    } else {
                        println!(
                            "{:<15} {:<15} {:<30} {}",
                            "ID", "KIND", "DEFAULT MODEL", "HAS KEY"
                        );
                        println!("{}", "-".repeat(70));
                        for p in &providers {
                            let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("-");
                            let kind = p.get("kind").and_then(|v| v.as_str()).unwrap_or("-");
                            let model = p
                                .get("default_model")
                                .and_then(|v| v.as_str())
                                .unwrap_or("-");
                            let has_key = p
                                .get("api_key")
                                .and_then(|v| v.as_str())
                                .map(|k| if k.is_empty() { "no" } else { "yes" })
                                .unwrap_or("no");
                            println!("{:<15} {:<15} {:<30} {}", id, kind, model, has_key);
                        }
                    }
                }
            }
            Ok(())
        }
        ConfigAction::Path => {
            let path = openpawz_core::engine::paths::paw_data_dir();
            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::json!({ "data_dir": path.to_string_lossy() })
                    );
                }
                _ => println!("{}", path.to_string_lossy()),
            }
            Ok(())
        }
    }
}
