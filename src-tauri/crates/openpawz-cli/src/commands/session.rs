use crate::OutputFormat;
use clap::Subcommand;
use openpawz_core::engine::sessions::SessionStore;

#[derive(Subcommand)]
pub enum SessionAction {
    /// List all chat sessions
    List {
        /// Maximum number of sessions to show
        #[arg(long, default_value = "50")]
        limit: usize,
    },
    /// Show chat history for a session
    History {
        /// Session ID
        id: String,
        /// Maximum messages to show
        #[arg(long, default_value = "50")]
        limit: usize,
    },
    /// Delete a session
    Delete {
        /// Session ID
        id: String,
    },
    /// Rename a session
    Rename {
        /// Session ID
        id: String,
        /// New name
        name: String,
    },
    /// Clean up empty/stale sessions
    Cleanup,
    /// Prune old messages from a session (keep most recent N)
    Prune {
        /// Session ID
        id: String,
        /// Number of recent messages to keep
        #[arg(long, default_value = "50")]
        keep: usize,
    },
    /// Export a session's history to a file
    Export {
        /// Session ID
        id: String,
        /// Output file (JSON)
        #[arg(long, short)]
        output: String,
    },
}

pub fn run(
    store: &SessionStore,
    action: SessionAction,
    format: &OutputFormat,
) -> Result<(), String> {
    match action {
        SessionAction::List { limit } => {
            let sessions = store
                .list_sessions_filtered(limit as i64, None)
                .map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&sessions).map_err(|e| e.to_string())?
                    );
                }
                OutputFormat::Quiet => {
                    for s in &sessions {
                        println!("{}", s.id);
                    }
                }
                OutputFormat::Human => {
                    if sessions.is_empty() {
                        println!("No sessions found.");
                    } else {
                        println!(
                            "{:<40} {:<30} {:>5} {:<20}",
                            "ID", "MODEL", "MSGS", "UPDATED"
                        );
                        println!("{}", "-".repeat(95));
                        for s in &sessions {
                            println!(
                                "{:<40} {:<30} {:>5} {:<20}",
                                truncate(&s.id, 38),
                                truncate(&s.model, 28),
                                s.message_count,
                                truncate(&s.updated_at, 19),
                            );
                        }
                        println!("\n{} session(s)", sessions.len());
                    }
                }
            }
            Ok(())
        }
        SessionAction::History { id, limit } => {
            let messages = store
                .get_messages(&id, limit as i64)
                .map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&messages).map_err(|e| e.to_string())?
                    );
                }
                _ => {
                    if messages.is_empty() {
                        println!("No messages in session '{}'.", id);
                    } else {
                        for m in &messages {
                            let role_label = match m.role.as_str() {
                                "user" => "\x1b[36mYou\x1b[0m",
                                "assistant" => "\x1b[33mAssistant\x1b[0m",
                                "system" => "\x1b[90mSystem\x1b[0m",
                                "tool" => "\x1b[35mTool\x1b[0m",
                                other => other,
                            };
                            println!("[{}] {}", role_label, truncate(&m.content, 200));
                            println!();
                        }
                    }
                }
            }
            Ok(())
        }
        SessionAction::Delete { id } => {
            store.delete_session(&id).map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Quiet => {}
                _ => println!("Deleted session '{}'", id),
            }
            Ok(())
        }
        SessionAction::Rename { id, name } => {
            store
                .rename_session(&id, &name)
                .map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Quiet => {}
                _ => println!("Renamed session '{}' → '{}'", id, name),
            }
            Ok(())
        }
        SessionAction::Cleanup => {
            let removed = store
                .cleanup_empty_sessions(3600, None)
                .map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::json!({ "removed": removed }));
                }
                OutputFormat::Quiet => println!("{}", removed),
                OutputFormat::Human => {
                    println!("Cleaned up {} empty session(s).", removed);
                }
            }
            Ok(())
        }
        SessionAction::Prune { id, keep } => {
            let removed = store
                .prune_session_messages(&id, keep as i64)
                .map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::json!({
                            "session_id": id,
                            "messages_removed": removed,
                            "messages_kept": keep,
                        })
                    );
                }
                OutputFormat::Quiet => println!("{}", removed),
                OutputFormat::Human => {
                    println!(
                        "Pruned {} message(s) from session '{}' (kept {})",
                        removed,
                        truncate(&id, 12),
                        keep,
                    );
                }
            }
            Ok(())
        }
        SessionAction::Export { id, output } => {
            let messages = store
                .get_messages(&id, 100_000)
                .map_err(|e| e.to_string())?;
            let session = store.get_session(&id).map_err(|e| e.to_string())?;

            let export = serde_json::json!({
                "session": session,
                "messages": messages,
                "exported_at": chrono::Utc::now().to_rfc3339(),
            });

            let json = serde_json::to_string_pretty(&export).map_err(|e| e.to_string())?;
            std::fs::write(&output, &json)
                .map_err(|e| format!("Failed to write {}: {}", output, e))?;

            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::json!({
                            "session_id": id,
                            "messages": messages.len(),
                            "file": output,
                            "bytes": json.len(),
                        })
                    );
                }
                OutputFormat::Quiet => println!("{}", output),
                OutputFormat::Human => {
                    println!(
                        "Exported {} messages from '{}' → {}",
                        messages.len(),
                        truncate(&id, 12),
                        output,
                    );
                }
            }
            Ok(())
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..s.floor_char_boundary(max - 1)])
    }
}
