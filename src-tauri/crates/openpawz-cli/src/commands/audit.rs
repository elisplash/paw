use crate::OutputFormat;
use clap::Subcommand;
use openpawz_core::engine::audit;
use openpawz_core::engine::sessions::SessionStore;

#[derive(Subcommand)]
pub enum AuditAction {
    /// Show recent audit log entries
    Log {
        /// Maximum entries to show
        #[arg(long, default_value = "25")]
        limit: usize,
        /// Filter by category (tool_call, memory, credential, api_request, security, cognitive, flow)
        #[arg(long)]
        category: Option<String>,
        /// Filter by agent ID
        #[arg(long)]
        agent: Option<String>,
    },
    /// Verify the HMAC chain integrity of the audit log
    Verify,
    /// Show audit log statistics
    Stats,
}

pub fn run(store: &SessionStore, action: AuditAction, format: &OutputFormat) -> Result<(), String> {
    match action {
        AuditAction::Log {
            limit,
            category,
            agent,
        } => {
            let entries = audit::query_recent(store, limit, category.as_deref(), agent.as_deref())
                .map_err(|e| e.to_string())?;

            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&entries).map_err(|e| e.to_string())?
                    );
                }
                OutputFormat::Quiet => {
                    for e in &entries {
                        println!("{}", e.id);
                    }
                }
                OutputFormat::Human => {
                    if entries.is_empty() {
                        println!("No audit entries found.");
                    } else {
                        println!(
                            "{:<6} {:<20} {:<12} {:<20} {:<10} {}",
                            "ID", "TIMESTAMP", "CATEGORY", "ACTION", "OK", "SUBJECT"
                        );
                        println!("{}", "-".repeat(90));
                        for e in &entries {
                            let ok_str = if e.success {
                                "\x1b[32m✓\x1b[0m"
                            } else {
                                "\x1b[31m✗\x1b[0m"
                            };
                            println!(
                                "{:<6} {:<20} {:<12} {:<20} {:<10} {}",
                                e.id,
                                truncate(&e.timestamp, 19),
                                e.category,
                                truncate(&e.action, 18),
                                ok_str,
                                truncate(&e.subject, 30),
                            );
                        }
                        println!("\n{} entr(ies)", entries.len());
                    }
                }
            }
            Ok(())
        }
        AuditAction::Verify => {
            let result = audit::verify_chain(store).map_err(|e| e.to_string())?;
            match result {
                Ok(count) => {
                    match format {
                        OutputFormat::Json => {
                            println!(
                                "{}",
                                serde_json::json!({
                                    "status": "intact",
                                    "entries_verified": count,
                                })
                            );
                        }
                        OutputFormat::Quiet => println!("ok {}", count),
                        OutputFormat::Human => {
                            println!(
                                "\x1b[32m✓\x1b[0m Audit chain intact — {} entries verified",
                                count
                            );
                        }
                    }
                    Ok(())
                }
                Err(broken_id) => {
                    match format {
                        OutputFormat::Json => {
                            println!(
                                "{}",
                                serde_json::json!({
                                    "status": "broken",
                                    "broken_at_id": broken_id,
                                })
                            );
                        }
                        OutputFormat::Quiet => println!("broken {}", broken_id),
                        OutputFormat::Human => {
                            eprintln!(
                                "\x1b[31m✗\x1b[0m Audit chain BROKEN at entry {} — possible tampering detected!",
                                broken_id
                            );
                        }
                    }
                    // Return error exit code for scripting
                    Err(format!("Audit chain broken at entry {}", broken_id))
                }
            }
        }
        AuditAction::Stats => {
            let stats = audit::stats(store).map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&stats).map_err(|e| e.to_string())?
                    );
                }
                _ => {
                    println!("Audit Log Statistics");
                    println!("{}", "=".repeat(40));
                    println!("  Total entries: {}", stats.total_entries);
                    if let Some(ref oldest) = stats.oldest_entry {
                        println!("  Oldest entry: {}", oldest);
                    }
                    if let Some(ref newest) = stats.newest_entry {
                        println!("  Newest entry: {}", newest);
                    }
                    if !stats.categories.is_empty() {
                        println!("  By category:");
                        for (cat, count) in &stats.categories {
                            println!("    {:<20} {}", cat, count);
                        }
                    }
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
