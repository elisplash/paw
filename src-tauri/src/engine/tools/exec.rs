// Paw Agent Engine — exec tool
// Execute shell commands on the user's machine.

use crate::atoms::types::*;
use crate::engine::state::EngineState;
use crate::engine::sandbox;
use log::{info, warn};
use tauri::Manager;
use crate::atoms::error::EngineResult;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        tool_type: "function".into(),
        function: FunctionDefinition {
            name: "exec".into(),
            description: "Execute a shell command on the user's machine. Returns stdout and stderr. Use this for file operations, git, package managers, CLI tools, etc.".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute"
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Timeout in seconds (default: 120, max: 600)"
                    }
                },
                "required": ["command"]
            }),
        },
    }]
}

pub async fn execute(
    name: &str,
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> Option<Result<String, String>> {
    match name {
        "exec" => Some(execute_exec(args, app_handle, agent_id).await.map_err(|e| e.to_string())),
        _ => None,
    }
}

async fn execute_exec(args: &serde_json::Value, app_handle: &tauri::AppHandle, agent_id: &str) -> EngineResult<String> {
    let command = args["command"].as_str()
        .ok_or("exec: missing 'command' argument")?;

    info!("[engine] exec: {}", &command[..command.len().min(200)]);

    // Block installing packages that duplicate built-in skill tools
    let cmd_lower = command.to_lowercase();
    let blocked_packages = ["cdp-sdk", "coinbase-sdk", "coinbase-advanced-py", "cbpro", "coinbase"];
    if cmd_lower.contains("pip") || cmd_lower.contains("npm") {
        for pkg in &blocked_packages {
            if cmd_lower.contains(pkg) {
                return Err(format!(
                    "Do not install '{}'. Coinbase access is handled by built-in tools: \
                     coinbase_balance, coinbase_prices, coinbase_trade, coinbase_transfer. \
                     Call those tools directly.",
                    pkg
                ).into());
            }
        }
    }

    // Check sandbox config — if enabled, route through Docker container
    let sandbox_config = {
        let state = app_handle.state::<EngineState>();
        sandbox::load_sandbox_config(&state.store)
    };

    if sandbox_config.enabled {
        info!("[engine] exec: routing through sandbox (image={})", sandbox_config.image);
        match sandbox::run_in_sandbox(command, &sandbox_config).await {
            Ok(result) => return Ok(sandbox::format_sandbox_result(&result)),
            Err(e) => {
                warn!("[engine] Sandbox execution failed, falling back to host: {}", e);
                // Fall through to host execution
            }
        }
    }

    // Set working directory to agent's workspace
    let workspace = super::ensure_workspace(agent_id)?;

    // Parse optional timeout (default 120s, max 600s)
    let timeout_secs = args["timeout"].as_u64().unwrap_or(120).min(600);

    // Run via sh -c (Unix) or cmd /C (Windows) with timeout
    use tokio::process::Command as TokioCommand;
    use std::time::Duration;

    let mut child = if cfg!(target_os = "windows") {
        TokioCommand::new("cmd")
            .args(["C", command])
            .current_dir(&workspace)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
    } else {
        TokioCommand::new("sh")
            .args(["-c", command])
            .current_dir(&workspace)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
    }.map_err(|e| crate::atoms::error::EngineError::Other(format!("Failed to spawn process: {}", e)))?;

    child.kill_on_drop(true);

    let output = match tokio::time::timeout(Duration::from_secs(timeout_secs), child.wait_with_output()).await {
        Ok(result) => result,
        Err(_) => {
            // Timeout — child is killed on drop via kill_on_drop(true)
            return Err(format!("exec: command timed out after {}s", timeout_secs).into());
        }
    };

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();

            let mut result = String::new();
            if !stdout.is_empty() {
                result.push_str(&stdout);
            }
            if !stderr.is_empty() {
                if !result.is_empty() {
                    result.push_str("\n--- stderr ---\n");
                }
                result.push_str(&stderr);
            }
            if result.is_empty() {
                result = format!("(exit code: {})", out.status.code().unwrap_or(-1));
            }

            const MAX_OUTPUT: usize = 50_000;
            if result.len() > MAX_OUTPUT {
                result.truncate(MAX_OUTPUT);
                result.push_str("\n\n... [output truncated]");
            }

            Ok(result)
        }
        Err(e) => Err(e.into()),
    }
}
