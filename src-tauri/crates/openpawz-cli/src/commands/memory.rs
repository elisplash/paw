use crate::OutputFormat;
use clap::Subcommand;
use openpawz_core::engine::sessions::SessionStore;

#[derive(Subcommand)]
pub enum MemoryAction {
    /// List stored memories
    List {
        /// Maximum memories to show
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Filter by agent ID
        #[arg(long)]
        agent: Option<String>,
    },
    /// Search memories by keyword (FTS5)
    Search {
        /// Search query
        query: String,
        /// Maximum results
        #[arg(long, default_value = "10")]
        limit: usize,
    },
    /// Store a new memory
    Store {
        /// Memory content
        content: String,
        /// Category (e.g. "general", "preference", "fact")
        #[arg(long, default_value = "general")]
        category: String,
        /// Importance (0-10)
        #[arg(long, default_value = "5")]
        importance: u8,
        /// Agent ID
        #[arg(long)]
        agent: Option<String>,
    },
    /// Delete a memory
    Delete {
        /// Memory ID
        id: String,
    },
    /// Show memory statistics
    Stats,
    /// Export memories to an encrypted archive
    Export {
        /// Output file path
        #[arg(long, short)]
        output: String,
        /// Agent ID (or "global" for all)
        #[arg(long, default_value = "global")]
        agent: String,
        /// Encryption passphrase
        #[arg(long)]
        passphrase: String,
    },
    /// Import memories from an encrypted archive
    Import {
        /// Input file path
        #[arg(long, short)]
        input: String,
        /// Decryption passphrase
        #[arg(long)]
        passphrase: String,
    },
}

pub async fn run(
    store: &SessionStore,
    action: MemoryAction,
    format: &OutputFormat,
) -> Result<(), String> {
    match action {
        MemoryAction::List { limit, agent: _ } => {
            let memories = store.list_memories(limit).map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&memories).map_err(|e| e.to_string())?
                    );
                }
                OutputFormat::Quiet => {
                    for m in &memories {
                        println!("{}", m.id);
                    }
                }
                OutputFormat::Human => {
                    if memories.is_empty() {
                        println!("No memories stored.");
                    } else {
                        for m in &memories {
                            println!(
                                "[{}] ({}, imp:{}) {}",
                                &m.id[..8.min(m.id.len())],
                                m.category,
                                m.importance,
                                truncate(&m.content, 100)
                            );
                        }
                        println!("\n{} memor(ies)", memories.len());
                    }
                }
            }
            Ok(())
        }
        MemoryAction::Search { query, limit } => {
            let results = store
                .search_memories_keyword(&query, limit)
                .map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&results).map_err(|e| e.to_string())?
                    );
                }
                OutputFormat::Quiet => {
                    for m in &results {
                        println!("{}", m.id);
                    }
                }
                OutputFormat::Human => {
                    if results.is_empty() {
                        println!("No memories matching '{}'.", query);
                    } else {
                        for m in &results {
                            println!(
                                "[{}] ({}, imp:{}) {}",
                                &m.id[..8.min(m.id.len())],
                                m.category,
                                m.importance,
                                truncate(&m.content, 100)
                            );
                        }
                        println!("\n{} result(s) for '{}'", results.len(), query);
                    }
                }
            }
            Ok(())
        }
        MemoryAction::Store {
            content,
            category,
            importance,
            agent,
        } => {
            let id = uuid::Uuid::new_v4().to_string();
            store
                .store_memory(&id, &content, &category, importance, None, agent.as_deref())
                .map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Json => {
                    println!("{}", serde_json::json!({ "id": id, "status": "stored" }));
                }
                OutputFormat::Quiet => println!("{}", id),
                OutputFormat::Human => {
                    println!("Stored memory {} ({})", &id[..8], category);
                }
            }
            Ok(())
        }
        MemoryAction::Delete { id } => {
            store.delete_memory(&id).map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Quiet => {}
                _ => println!("Deleted memory '{}'", id),
            }
            Ok(())
        }
        MemoryAction::Stats => {
            let stats = store.memory_stats().map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&stats).map_err(|e| e.to_string())?
                    );
                }
                _ => {
                    println!("Memory Statistics");
                    println!("{}", "=".repeat(40));
                    println!("  Total memories: {}", stats.total_memories);
                    println!("  Has embeddings: {}", stats.has_embeddings);
                    if !stats.categories.is_empty() {
                        println!("  Categories:");
                        for (cat, count) in &stats.categories {
                            println!("    {:<20} {}", cat, count);
                        }
                    }
                }
            }
            Ok(())
        }
        MemoryAction::Export {
            output,
            agent,
            passphrase,
        } => {
            if passphrase.len() < 8 {
                return Err("Passphrase must be at least 8 characters.".into());
            }
            let archive = openpawz_core::engine::engram::encrypted_export::export_encrypted(
                store,
                &agent,
                &passphrase,
            )
            .map_err(|e| e.to_string())?;
            std::fs::write(&output, &archive)
                .map_err(|e| format!("Failed to write {}: {}", output, e))?;
            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::json!({
                            "status": "exported",
                            "file": output,
                            "bytes": archive.len(),
                            "agent": agent,
                        })
                    );
                }
                OutputFormat::Quiet => println!("{}", output),
                OutputFormat::Human => {
                    println!(
                        "Exported memories for '{}' → {} ({} bytes)",
                        agent,
                        output,
                        archive.len()
                    );
                }
            }
            Ok(())
        }
        MemoryAction::Import { input, passphrase } => {
            let archive =
                std::fs::read(&input).map_err(|e| format!("Failed to read {}: {}", input, e))?;
            let report = openpawz_core::engine::engram::encrypted_export::import_encrypted(
                store,
                &archive,
                &passphrase,
            )
            .map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
                    );
                }
                _ => {
                    println!("Import complete");
                    println!("  Source agent: {}", report.source_agent);
                    println!("  Exported at:  {}", report.exported_at);
                    println!(
                        "  Imported:     {} episodic ({} skipped)",
                        report.imported_episodic, report.skipped
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
