// Paw Agent Engine — exec tool
// Execute shell commands on the user's machine.

use crate::atoms::error::EngineResult;
use crate::atoms::types::*;
use crate::engine::sandbox;
use crate::engine::state::EngineState;
use crate::engine::util::safe_truncate;
use log::{info, warn};
use tauri::Manager;

pub fn definitions() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        tool_type: "function".into(),
        function: FunctionDefinition {
            name: "exec".into(),
            description: "Execute a shell command on the user's machine. Returns stdout and stderr. Use for file operations, git, build tools, package managers, CLI tools (gh, docker, kubectl, etc.), and any local or remote command.".into(),
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
        "exec" => Some(
            execute_exec(args, app_handle, agent_id)
                .await
                .map_err(|e| e.to_string()),
        ),
        _ => None,
    }
}

async fn execute_exec(
    args: &serde_json::Value,
    app_handle: &tauri::AppHandle,
    agent_id: &str,
) -> EngineResult<String> {
    let command = args["command"]
        .as_str()
        .ok_or("exec: missing 'command' argument")?;

    info!("[engine] exec: {}", safe_truncate(command, 200));

    // §Security: Block dangerous command patterns that attempt to exfiltrate
    // credentials, open reverse shells, or access sensitive files.
    // These are blocked even when the user approves the tool call.
    let cmd_lower = command.to_lowercase();
    {
        const EXFIL_PATTERNS: &[&str] = &[
            // Credential exfiltration
            "cat.*id_rsa",
            "cat.*id_ed25519",
            "cat.*/etc/shadow",
            "base64.*\\.ssh",
            "base64.*\\.gnupg",
            "tar.*\\.ssh",
            "zip.*\\.ssh",
            "cp.*\\.ssh",
            "scp.*\\.ssh",
            // Reverse shells
            "nc -e",
            "nc -c",
            "ncat -e",
            "bash -i >& /dev/tcp",
            "python.*-c.*import.*socket",
            "python.*-c.*import.*subprocess",
            "perl.*socket.*connect",
            "ruby.*tcpsocket",
            "php.*fsockopen",
            // Credential harvesting
            "cat.*\\.aws/credentials",
            "cat.*\\.npmrc",
            "cat.*\\.env",
            "printenv.*secret",
            "printenv.*token",
            "printenv.*password",
            "echo.*\\$.*secret",
            "echo.*\\$.*token",
            "echo.*\\$.*password",
        ];

        for pattern in EXFIL_PATTERNS {
            if cmd_lower.contains(pattern) {
                warn!(
                    "[engine] exec: BLOCKED dangerous command pattern '{}' (agent={}): {}",
                    pattern,
                    agent_id,
                    safe_truncate(command, 100)
                );
                return Err(format!(
                    "exec: command blocked by security policy — matches dangerous pattern '{}'. \
                     Credential access and reverse shells are not permitted.",
                    pattern
                )
                .into());
            }
        }
    }

    // Block installing packages that duplicate built-in skill tools
    let blocked_packages = [
        "cdp-sdk",
        "coinbase-sdk",
        "coinbase-advanced-py",
        "cbpro",
        "coinbase",
    ];
    if cmd_lower.contains("pip") || cmd_lower.contains("npm") {
        for pkg in &blocked_packages {
            if cmd_lower.contains(pkg) {
                return Err(format!(
                    "Do not install '{}'. Coinbase access is handled by built-in tools: \
                     coinbase_balance, coinbase_prices, coinbase_trade, coinbase_transfer. \
                     Call those tools directly.",
                    pkg
                )
                .into());
            }
        }
    }

    // Check sandbox config — if enabled, route through Docker container
    let sandbox_config = {
        let state = app_handle.state::<EngineState>();
        sandbox::load_sandbox_config(&state.store)
    };

    if sandbox_config.enabled {
        info!(
            "[engine] exec: routing through sandbox (image={})",
            sandbox_config.image
        );
        match sandbox::run_in_sandbox(command, &sandbox_config).await {
            Ok(result) => return Ok(sandbox::format_sandbox_result(&result)),
            Err(e) => {
                warn!(
                    "[engine] Sandbox execution failed — refusing to fall back to host: {}",
                    e
                );
                return Err(format!(
                    "Sandbox execution failed (host fallback is disabled for security): {}",
                    e
                )
                .into());
            }
        }
    }

    // Set working directory to agent's workspace
    let workspace = super::ensure_workspace(agent_id)?;

    // Parse optional timeout (default 120s, max 600s)
    let timeout_secs = args["timeout"].as_u64().unwrap_or(120).min(600);

    // Run via sh -c (Unix) or cmd /C (Windows) with timeout
    use std::time::Duration;
    use tokio::process::Command as TokioCommand;

    let child = if cfg!(target_os = "windows") {
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
    }
    .map_err(|e| {
        crate::atoms::error::EngineError::Other(format!("Failed to spawn process: {}", e))
    })?;

    let output =
        match tokio::time::timeout(Duration::from_secs(timeout_secs), child.wait_with_output())
            .await
        {
            Ok(result) => result,
            Err(_) => {
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
