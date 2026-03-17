use crate::OutputFormat;
use clap::Subcommand;
use openpawz_core::engine::sessions::SessionStore;
use openpawz_core::engine::types::{Project, ProjectAgent};

#[derive(Subcommand)]
pub enum ProjectAction {
    /// List all projects
    List,
    /// Create a new project
    Create {
        /// Project title
        #[arg(long)]
        title: String,
        /// Project goal
        #[arg(long)]
        goal: String,
        /// Boss (orchestrator) agent ID
        #[arg(long)]
        boss: String,
    },
    /// Show project details and team
    Get {
        /// Project ID
        id: String,
    },
    /// Add an agent to a project
    AddAgent {
        /// Project ID
        #[arg(long)]
        project: String,
        /// Agent ID
        #[arg(long)]
        agent: String,
        /// Role: boss, worker
        #[arg(long, default_value = "worker")]
        role: String,
        /// Specialty: coder, researcher, designer, communicator, security, general
        #[arg(long, default_value = "general")]
        specialty: String,
    },
    /// Show project messages (delegation log)
    Messages {
        /// Project ID
        id: String,
        /// Maximum messages
        #[arg(long, default_value = "50")]
        limit: usize,
    },
    /// Update project status
    Update {
        /// Project ID
        id: String,
        /// New status (planning, running, paused, completed, failed)
        #[arg(long)]
        status: Option<String>,
        /// New title
        #[arg(long)]
        title: Option<String>,
    },
    /// Delete a project
    Delete {
        /// Project ID
        id: String,
    },
}

pub fn run(
    store: &SessionStore,
    action: ProjectAction,
    format: &OutputFormat,
) -> Result<(), String> {
    match action {
        ProjectAction::List => {
            let projects = store.list_projects().map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&projects).map_err(|e| e.to_string())?
                    );
                }
                OutputFormat::Quiet => {
                    for p in &projects {
                        println!("{}", p.id);
                    }
                }
                OutputFormat::Human => {
                    if projects.is_empty() {
                        println!("No projects found.");
                    } else {
                        println!(
                            "{:<12} {:<12} {:<30} {:<15} {}",
                            "ID", "STATUS", "TITLE", "BOSS", "AGENTS"
                        );
                        println!("{}", "-".repeat(85));
                        for p in &projects {
                            println!(
                                "{:<12} {:<12} {:<30} {:<15} {}",
                                truncate(&p.id, 10),
                                color_status(&p.status),
                                truncate(&p.title, 28),
                                truncate(&p.boss_agent, 13),
                                p.agents.len(),
                            );
                        }
                        println!("\n{} project(s)", projects.len());
                    }
                }
            }
            Ok(())
        }
        ProjectAction::Create { title, goal, boss } => {
            let id = uuid::Uuid::new_v4()
                .to_string()
                .split('-')
                .next()
                .unwrap_or("proj")
                .to_string();
            let now = chrono::Utc::now().to_rfc3339();
            let project = Project {
                id: id.clone(),
                title: title.clone(),
                goal,
                status: "planning".into(),
                boss_agent: boss.clone(),
                agents: vec![ProjectAgent {
                    agent_id: boss,
                    role: "boss".into(),
                    specialty: "general".into(),
                    status: "idle".into(),
                    current_task: None,
                    model: None,
                    system_prompt: None,
                    capabilities: Vec::new(),
                }],
                created_at: now.clone(),
                updated_at: now,
            };
            store.create_project(&project).map_err(|e| e.to_string())?;
            // Also persist the boss agent to project_agents table
            store
                .set_project_agents(&id, &project.agents)
                .map_err(|e| e.to_string())?;

            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&project).map_err(|e| e.to_string())?
                    );
                }
                OutputFormat::Quiet => println!("{}", id),
                OutputFormat::Human => {
                    println!("Created project '{}' ({})", title, id);
                }
            }
            Ok(())
        }
        ProjectAction::Get { id } => {
            let projects = store.list_projects().map_err(|e| e.to_string())?;
            let project = projects
                .iter()
                .find(|p| p.id == id || p.id.starts_with(&id))
                .ok_or_else(|| format!("Project '{}' not found", id))?;

            let agents = store
                .get_project_agents(&project.id)
                .map_err(|e| e.to_string())?;

            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(project).map_err(|e| e.to_string())?
                    );
                }
                _ => {
                    println!("Project: {}", project.id);
                    println!("  Title:   {}", project.title);
                    println!("  Goal:    {}", project.goal);
                    println!("  Status:  {}", project.status);
                    println!("  Boss:    {}", project.boss_agent);
                    println!("  Created: {}", project.created_at);
                    println!("  Updated: {}", project.updated_at);
                    if !agents.is_empty() {
                        println!("  Team:");
                        for a in &agents {
                            let task_str = a
                                .current_task
                                .as_deref()
                                .map(|t| format!(" → {}", truncate(t, 30)))
                                .unwrap_or_default();
                            println!(
                                "    {} ({}/{}) [{}]{}",
                                a.agent_id, a.role, a.specialty, a.status, task_str
                            );
                        }
                    }
                }
            }
            Ok(())
        }
        ProjectAction::AddAgent {
            project,
            agent,
            role,
            specialty,
        } => {
            let new_agent = ProjectAgent {
                agent_id: agent.clone(),
                role,
                specialty,
                status: "idle".into(),
                current_task: None,
                model: None,
                system_prompt: None,
                capabilities: Vec::new(),
            };
            store
                .add_project_agent(&project, &new_agent)
                .map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::json!({
                            "project_id": project,
                            "agent_id": agent,
                            "status": "added",
                        })
                    );
                }
                OutputFormat::Quiet => {}
                OutputFormat::Human => {
                    println!("Added agent '{}' to project '{}'", agent, project);
                }
            }
            Ok(())
        }
        ProjectAction::Messages { id, limit } => {
            let messages = store
                .get_project_messages(&id, limit as i64)
                .map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&messages).map_err(|e| e.to_string())?
                    );
                }
                OutputFormat::Quiet => {
                    for m in &messages {
                        println!("{}", m.id);
                    }
                }
                OutputFormat::Human => {
                    if messages.is_empty() {
                        println!("No messages in project '{}'.", id);
                    } else {
                        for m in &messages {
                            let to = m.to_agent.as_deref().unwrap_or("broadcast");
                            let kind_colored = match m.kind.as_str() {
                                "delegation" => format!("\x1b[33m{}\x1b[0m", m.kind),
                                "result" => format!("\x1b[32m{}\x1b[0m", m.kind),
                                "error" => format!("\x1b[31m{}\x1b[0m", m.kind),
                                _ => m.kind.clone(),
                            };
                            println!(
                                "[{}] {} → {} ({}) {}",
                                truncate(&m.created_at, 19),
                                m.from_agent,
                                to,
                                kind_colored,
                                truncate(&m.content, 80),
                            );
                        }
                        println!("\n{} message(s)", messages.len());
                    }
                }
            }
            Ok(())
        }
        ProjectAction::Update { id, status, title } => {
            let projects = store.list_projects().map_err(|e| e.to_string())?;
            let project = projects
                .iter()
                .find(|p| p.id == id || p.id.starts_with(&id))
                .ok_or_else(|| format!("Project '{}' not found", id))?;

            let mut updated = project.clone();
            if let Some(s) = status {
                updated.status = s;
            }
            if let Some(t) = title {
                updated.title = t;
            }
            updated.updated_at = chrono::Utc::now().to_rfc3339();

            store.update_project(&updated).map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Quiet => {}
                _ => println!("Updated project '{}'", updated.id),
            }
            Ok(())
        }
        ProjectAction::Delete { id } => {
            store.delete_project(&id).map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Quiet => {}
                _ => println!("Deleted project '{}'", id),
            }
            Ok(())
        }
    }
}

fn color_status(status: &str) -> String {
    match status {
        "running" => format!("\x1b[33m{:<12}\x1b[0m", status),
        "completed" => format!("\x1b[32m{:<12}\x1b[0m", status),
        "failed" => format!("\x1b[31m{:<12}\x1b[0m", status),
        "paused" => format!("\x1b[90m{:<12}\x1b[0m", status),
        _ => format!("{:<12}", status),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..s.floor_char_boundary(max - 1)])
    }
}
