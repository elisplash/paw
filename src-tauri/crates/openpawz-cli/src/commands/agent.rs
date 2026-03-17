use crate::OutputFormat;
use clap::Subcommand;
use openpawz_core::engine::sessions::SessionStore;

#[derive(Subcommand)]
pub enum AgentAction {
    /// List all agents
    List,
    /// Show details of a specific agent
    Get {
        /// Agent ID
        id: String,
    },
    /// Create a new agent
    Create {
        /// Agent name
        #[arg(long)]
        name: String,
        /// Default model for this agent
        #[arg(long)]
        model: Option<String>,
    },
    /// Delete an agent
    Delete {
        /// Agent ID
        id: String,
    },
    /// Read an agent file (e.g. SOUL.md, IDENTITY.md, persona.md)
    FileGet {
        /// Agent ID
        #[arg(long)]
        id: String,
        /// File name
        #[arg(long)]
        file: String,
    },
    /// Write/update an agent file
    FileSet {
        /// Agent ID
        #[arg(long)]
        id: String,
        /// File name
        #[arg(long)]
        file: String,
        /// Content (or use --from-file)
        #[arg(long)]
        content: Option<String>,
        /// Read content from a local file instead
        #[arg(long, conflicts_with = "content")]
        from_file: Option<String>,
    },
    /// Show the composed agent context (soul + persona + identity)
    Context {
        /// Agent ID
        id: String,
    },
    /// Export an agent's files to a directory
    Export {
        /// Agent ID
        id: String,
        /// Output directory (created if needed)
        #[arg(long, short)]
        output: String,
    },
    /// Import agent files from a directory
    Import {
        /// Agent ID (created if it doesn't exist)
        #[arg(long)]
        id: String,
        /// Directory containing agent files (*.md, *.json)
        #[arg(long, short)]
        input: String,
    },
}

pub fn run(store: &SessionStore, action: AgentAction, format: &OutputFormat) -> Result<(), String> {
    match action {
        AgentAction::List => {
            let agents = store.list_all_agents().map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Json => {
                    let json: Vec<_> = agents
                        .iter()
                        .map(|(pid, a)| {
                            serde_json::json!({
                                "project_id": pid,
                                "agent_id": a.agent_id,
                                "role": a.role,
                            })
                        })
                        .collect();
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?
                    );
                }
                OutputFormat::Quiet => {
                    for (_, a) in &agents {
                        println!("{}", a.agent_id);
                    }
                }
                OutputFormat::Human => {
                    if agents.is_empty() {
                        println!("No agents found.");
                    } else {
                        println!("{:<20} {:<20} {:<20}", "AGENT ID", "PROJECT", "ROLE");
                        println!("{}", "-".repeat(60));
                        for (pid, a) in &agents {
                            println!("{:<20} {:<20} {:<20}", a.agent_id, pid, &a.role);
                        }
                    }
                }
            }
            Ok(())
        }
        AgentAction::Get { id } => {
            let files = store.list_agent_files(&id).map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&files).map_err(|e| e.to_string())?
                    );
                }
                _ => {
                    println!("Agent: {}", id);
                    println!("Files: {}", files.len());
                    for f in &files {
                        println!("  - {} ({} bytes)", f.file_name, f.content.len());
                    }
                }
            }
            Ok(())
        }
        AgentAction::Create { name, model } => {
            let id = format!(
                "agent-{}",
                uuid::Uuid::new_v4()
                    .to_string()
                    .split('-')
                    .next()
                    .unwrap_or("x")
            );
            let agent_json = serde_json::json!({
                "id": id,
                "name": name,
                "default_model": model,
            });
            store
                .set_agent_file(&id, "identity.json", &agent_json.to_string())
                .map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&agent_json).map_err(|e| e.to_string())?
                    );
                }
                _ => {
                    println!("Created agent '{}' ({})", name, id);
                }
            }
            Ok(())
        }
        AgentAction::Delete { id } => {
            let files = store.list_agent_files(&id).map_err(|e| e.to_string())?;
            for f in &files {
                store
                    .delete_agent_file(&id, &f.file_name)
                    .map_err(|e| e.to_string())?;
            }
            match format {
                OutputFormat::Quiet => {}
                _ => println!("Deleted agent '{}' ({} files removed)", id, files.len()),
            }
            Ok(())
        }
        AgentAction::FileGet { id, file } => {
            let content = store
                .get_agent_file(&id, &file)
                .map_err(|e| e.to_string())?;
            match content {
                Some(f) => {
                    match format {
                        OutputFormat::Json => {
                            println!(
                                "{}",
                                serde_json::json!({
                                    "agent_id": id,
                                    "file_name": file,
                                    "content": f.content,
                                })
                            );
                        }
                        _ => print!("{}", f.content),
                    }
                    Ok(())
                }
                None => Err(format!("File '{}' not found for agent '{}'", file, id)),
            }
        }
        AgentAction::FileSet {
            id,
            file,
            content,
            from_file,
        } => {
            let body = match (content, from_file) {
                (Some(c), _) => c,
                (_, Some(path)) => std::fs::read_to_string(&path)
                    .map_err(|e| format!("Failed to read {}: {}", path, e))?,
                _ => return Err("Provide --content or --from-file".into()),
            };
            store
                .set_agent_file(&id, &file, &body)
                .map_err(|e| e.to_string())?;
            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::json!({
                            "agent_id": id,
                            "file_name": file,
                            "bytes": body.len(),
                            "status": "written",
                        })
                    );
                }
                OutputFormat::Quiet => {}
                OutputFormat::Human => {
                    println!("Wrote '{}' for agent '{}' ({} bytes)", file, id, body.len());
                }
            }
            Ok(())
        }
        AgentAction::Context { id } => {
            let context = store
                .compose_agent_context(&id)
                .map_err(|e| e.to_string())?;
            match context {
                Some(ctx) => {
                    match format {
                        OutputFormat::Json => {
                            println!("{}", serde_json::json!({ "agent_id": id, "context": ctx }));
                        }
                        _ => print!("{}", ctx),
                    }
                    Ok(())
                }
                None => {
                    match format {
                        OutputFormat::Quiet => {}
                        _ => println!("No context files for agent '{}'.", id),
                    }
                    Ok(())
                }
            }
        }
        AgentAction::Export { id, output } => {
            let files = store.list_agent_files(&id).map_err(|e| e.to_string())?;
            if files.is_empty() {
                return Err(format!("No files found for agent '{}'", id));
            }
            std::fs::create_dir_all(&output)
                .map_err(|e| format!("Failed to create directory '{}': {}", output, e))?;
            for f in &files {
                let path = std::path::Path::new(&output).join(&f.file_name);
                std::fs::write(&path, &f.content)
                    .map_err(|e| format!("Failed to write '{}': {}", path.display(), e))?;
            }
            match format {
                OutputFormat::Json => {
                    let exported: Vec<&str> = files.iter().map(|f| f.file_name.as_str()).collect();
                    println!(
                        "{}",
                        serde_json::json!({
                            "agent_id": id,
                            "output": output,
                            "files": exported,
                        })
                    );
                }
                OutputFormat::Quiet => {}
                OutputFormat::Human => {
                    println!(
                        "Exported {} file(s) for agent '{}' → {}",
                        files.len(),
                        id,
                        output
                    );
                    for f in &files {
                        println!("  - {}", f.file_name);
                    }
                }
            }
            Ok(())
        }
        AgentAction::Import { id, input } => {
            let dir = std::path::Path::new(&input);
            if !dir.is_dir() {
                return Err(format!("'{}' is not a directory", input));
            }
            // Ensure agent exists — create identity.json if no files yet
            let existing = store.list_agent_files(&id).map_err(|e| e.to_string())?;
            if existing.is_empty() {
                let identity = serde_json::json!({ "id": id, "name": id });
                store
                    .set_agent_file(&id, "identity.json", &identity.to_string())
                    .map_err(|e| e.to_string())?;
            }
            let mut imported = Vec::new();
            let entries = std::fs::read_dir(dir)
                .map_err(|e| format!("Failed to read directory '{}': {}", input, e))?;
            for entry in entries {
                let entry = entry.map_err(|e| e.to_string())?;
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let name = match path.file_name().and_then(|n| n.to_str()) {
                    Some(n) => n.to_string(),
                    None => continue,
                };
                // Only import known agent file types
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if !matches!(ext, "md" | "json" | "txt" | "yaml" | "yml" | "toml") {
                    continue;
                }
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| format!("Failed to read '{}': {}", path.display(), e))?;
                store
                    .set_agent_file(&id, &name, &content)
                    .map_err(|e| e.to_string())?;
                imported.push(name);
            }
            match format {
                OutputFormat::Json => {
                    println!(
                        "{}",
                        serde_json::json!({
                            "agent_id": id,
                            "input": input,
                            "imported": imported,
                        })
                    );
                }
                OutputFormat::Quiet => {}
                OutputFormat::Human => {
                    println!(
                        "Imported {} file(s) for agent '{}' from {}",
                        imported.len(),
                        id,
                        input
                    );
                    for name in &imported {
                        println!("  - {}", name);
                    }
                }
            }
            Ok(())
        }
    }
}
