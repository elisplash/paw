use crate::OutputFormat;
use clap::Subcommand;
use openpawz_core::engine::sessions::SessionStore;
use openpawz_core::engine::types::Task;

#[derive(Subcommand)]
pub enum TaskAction {
    /// List all tasks
    List {
        /// Filter by status (pending, running, done, failed)
        #[arg(long)]
        status: Option<String>,
    },
    /// Show task details
    Get {
        /// Task ID
        id: String,
    },
    /// Create a new task
    Create {
        /// Task title
        #[arg(long)]
        title: String,
        /// Task description
        #[arg(long)]
        description: Option<String>,
        /// Priority (low, medium, high, critical)
        #[arg(long, default_value = "medium")]
        priority: String,
        /// Agent to assign
        #[arg(long)]
        agent: Option<String>,
    },
    /// Update task status
    Update {
        /// Task ID
        id: String,
        /// New status (pending, running, done, failed)
        #[arg(long)]
        status: Option<String>,
        /// New priority
        #[arg(long)]
        priority: Option<String>,
        /// New title
        #[arg(long)]
        title: Option<String>,
    },
    /// Delete a task
    Delete {
        /// Task ID
        id: String,
    },
    /// List tasks with upcoming cron schedules
    Due,
}

pub fn run(store: &SessionStore, action: TaskAction, format: &OutputFormat) -> Result<(), String> {
    match action {
        TaskAction::List { status } => {
            let mut tasks = store.list_tasks().map_err(|e| e.to_string())?;
            if let Some(ref s) = status {
                tasks.retain(|t| t.status.eq_ignore_ascii_case(s));
            }
            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&tasks).map_err(|e| e.to_string())?
                    );
                }
                OutputFormat::Quiet => {
                    for t in &tasks {
                        println!("{}", t.id);
                    }
                }
                OutputFormat::Human => {
                    if tasks.is_empty() {
                        println!("No tasks found.");
                    } else {
                        println!(
                            "{:<10} {:<10} {:<10} {:<30} {}",
                            "ID", "STATUS", "PRIORITY", "TITLE", "AGENT"
                        );
                        println!("{}", "-".repeat(80));
                        for t in &tasks {
                            let status_colored = match t.status.as_str() {
                                "done" => format!("\x1b[32m{:<10}\x1b[0m", &t.status),
                                "failed" => format!("\x1b[31m{:<10}\x1b[0m", &t.status),
                                "running" => format!("\x1b[33m{:<10}\x1b[0m", &t.status),
                                _ => format!("{:<10}", &t.status),
                            };
                            let agent = t.assigned_agent.as_deref().unwrap_or("-");
                            println!(
                                "{:<10} {} {:<10} {:<30} {}",
                                truncate(&t.id, 8),
                                status_colored,
                                t.priority,
                                truncate(&t.title, 28),
                                truncate(agent, 20),
                            );
                        }
                        println!("\n{} task(s)", tasks.len());
                    }
                }
            }
            Ok(())
        }
        TaskAction::Get { id } => {
            let tasks = store.list_tasks().map_err(|e| e.to_string())?;
            let task = tasks.iter().find(|t| t.id == id || t.id.starts_with(&id));
            match task {
                Some(t) => {
                    match format {
                        OutputFormat::Json => {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(t).map_err(|e| e.to_string())?
                            );
                        }
                        _ => {
                            println!("Task: {}", t.id);
                            println!("  Title:       {}", t.title);
                            println!(
                                "  Description: {}",
                                if t.description.is_empty() {
                                    "-"
                                } else {
                                    &t.description
                                }
                            );
                            println!("  Status:      {}", t.status);
                            println!("  Priority:    {}", t.priority);
                            println!(
                                "  Agent:       {}",
                                t.assigned_agent.as_deref().unwrap_or("-")
                            );
                            if !t.assigned_agents.is_empty() {
                                println!("  Team:");
                                for a in &t.assigned_agents {
                                    println!("    - {} ({})", a.agent_id, a.role);
                                }
                            }
                            if let Some(ref cron) = t.cron_schedule {
                                println!("  Cron:        {} (enabled: {})", cron, t.cron_enabled);
                            }
                            println!("  Created:     {}", t.created_at);
                            println!("  Updated:     {}", t.updated_at);
                        }
                    }
                    Ok(())
                }
                None => Err(format!("Task '{}' not found", id)),
            }
        }
        TaskAction::Create {
            title,
            description,
            priority,
            agent,
        } => {
            let id = uuid::Uuid::new_v4().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            let task = Task {
                id: id.clone(),
                title: title.clone(),
                description: description.unwrap_or_default(),
                status: "pending".into(),
                priority,
                assigned_agent: agent,
                assigned_agents: Vec::new(),
                session_id: None,
                model: None,
                cron_schedule: None,
                cron_enabled: false,
                last_run_at: None,
                next_run_at: None,
                created_at: now.clone(),
                updated_at: now,
                event_trigger: None,
                persistent: false,
            };
            store.create_task(&task).map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::json!({ "id": id, "title": title, "status": "created" })
                    );
                }
                OutputFormat::Quiet => println!("{}", id),
                OutputFormat::Human => println!("Created task '{}' ({})", title, &id[..8]),
            }
            Ok(())
        }
        TaskAction::Update {
            id,
            status,
            priority,
            title,
        } => {
            let tasks = store.list_tasks().map_err(|e| e.to_string())?;
            let task = tasks
                .iter()
                .find(|t| t.id == id || t.id.starts_with(&id))
                .ok_or_else(|| format!("Task '{}' not found", id))?;

            let mut updated = task.clone();
            if let Some(s) = status {
                updated.status = s;
            }
            if let Some(p) = priority {
                updated.priority = p;
            }
            if let Some(t) = title {
                updated.title = t;
            }
            updated.updated_at = chrono::Utc::now().to_rfc3339();

            store.update_task(&updated).map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Quiet => {}
                _ => println!("Updated task '{}'", updated.id),
            }
            Ok(())
        }
        TaskAction::Delete { id } => {
            store.delete_task(&id).map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Quiet => {}
                _ => println!("Deleted task '{}'", id),
            }
            Ok(())
        }
        TaskAction::Due => {
            let tasks = store.get_due_cron_tasks().map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&tasks).map_err(|e| e.to_string())?
                    );
                }
                OutputFormat::Quiet => {
                    for t in &tasks {
                        println!("{}", t.id);
                    }
                }
                OutputFormat::Human => {
                    if tasks.is_empty() {
                        println!("No tasks are currently due.");
                    } else {
                        for t in &tasks {
                            println!(
                                "[{}] {} (cron: {})",
                                &t.id[..8.min(t.id.len())],
                                t.title,
                                t.cron_schedule.as_deref().unwrap_or("-"),
                            );
                        }
                        println!("\n{} due task(s)", tasks.len());
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
