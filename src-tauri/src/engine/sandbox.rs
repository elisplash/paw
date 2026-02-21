// Paw Agent Engine — Container Sandboxing
// Runs exec tool calls inside Docker containers for security isolation.
// Uses bollard (Docker API client) to manage ephemeral containers.

use bollard::Docker;
use bollard::container::{Config, CreateContainerOptions, StartContainerOptions, LogsOptions, RemoveContainerOptions, WaitContainerOptions};
use bollard::models::HostConfig;
use futures::StreamExt;
use crate::atoms::error::EngineResult;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::time::Duration;

// ── Types ──────────────────────────────────────────────────────────────

/// Sandbox configuration for exec tool calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Whether sandboxing is enabled (default: false — must be opted in)
    pub enabled: bool,
    /// Docker image to use (default: "alpine:latest")
    pub image: String,
    /// Timeout in seconds for container execution (default: 30)
    pub timeout_secs: u64,
    /// Memory limit in bytes (default: 256MB)
    pub memory_limit: i64,
    /// CPU shares (default: 512, relative weight)
    pub cpu_shares: i64,
    /// Whether to allow network access (default: false)
    pub network_enabled: bool,
    /// Working directory inside the container
    pub workdir: String,
    /// Optional bind mounts (host_path:container_path:ro)
    pub bind_mounts: Vec<String>,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            image: "alpine:latest".to_string(),
            timeout_secs: 30,
            memory_limit: 256 * 1024 * 1024, // 256 MB
            cpu_shares: 512,
            network_enabled: false,
            workdir: "/workspace".to_string(),
            bind_mounts: Vec::new(),
        }
    }
}

/// Result from running a command in a sandbox container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i64,
    pub timed_out: bool,
    pub container_id: String,
}

// ── Docker health check ────────────────────────────────────────────────

/// Check if Docker daemon is reachable.
pub async fn is_docker_available() -> bool {
    match Docker::connect_with_local_defaults() {
        Ok(docker) => {
            match docker.ping().await {
                Ok(_) => {
                    info!("[sandbox] Docker daemon is available");
                    true
                }
                Err(e) => {
                    warn!("[sandbox] Docker ping failed: {}", e);
                    false
                }
            }
        }
        Err(e) => {
            warn!("[sandbox] Cannot connect to Docker: {}", e);
            false
        }
    }
}

/// Pull image if not already present.
async fn ensure_image(docker: &Docker, image: &str) -> EngineResult<()> {
    use bollard::image::CreateImageOptions;

    // Check if image exists locally
    match docker.inspect_image(image).await {
        Ok(_) => return Ok(()),
        Err(_) => {
            info!("[sandbox] Pulling image: {}", image);
        }
    }

    let opts = CreateImageOptions {
        from_image: image,
        ..Default::default()
    };

    let mut stream = docker.create_image(Some(opts), None, None);
    while let Some(result) = stream.next().await {
        match result {
            Ok(_info) => {} // progress updates
            Err(e) => return Err(format!("Failed to pull image '{}': {}", image, e).into()),
        }
    }

    info!("[sandbox] Image pulled: {}", image);
    Ok(())
}

// ── Run command in sandbox ─────────────────────────────────────────────

/// Execute a shell command inside an ephemeral Docker container.
/// The container is created, started, waited on, and removed automatically.
pub async fn run_in_sandbox(command: &str, config: &SandboxConfig) -> EngineResult<SandboxResult> {
    let docker = Docker::connect_with_local_defaults()?;

    // Ensure the image is available
    ensure_image(&docker, &config.image).await?;

    // Build container config
    let container_name = format!("paw-sandbox-{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("x"));

    let host_config = HostConfig {
        memory: Some(config.memory_limit),
        cpu_shares: Some(config.cpu_shares),
        network_mode: if config.network_enabled { None } else { Some("none".to_string()) },
        binds: if config.bind_mounts.is_empty() { None } else { Some(config.bind_mounts.clone()) },
        // Security: drop all capabilities, no privileged mode
        cap_drop: Some(vec!["ALL".to_string()]),
        // Read-only root filesystem (write to /tmp only)
        readonly_rootfs: Some(false), // alpine needs some writes, /tmp is writable
        ..Default::default()
    };

    let container_config = Config {
        image: Some(config.image.clone()),
        cmd: Some(vec!["sh".to_string(), "-c".to_string(), command.to_string()]),
        working_dir: Some(config.workdir.clone()),
        host_config: Some(host_config),
        // No tty, capture stdout/stderr
        tty: Some(false),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        // Environment: minimal
        env: Some(vec![
            "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string(),
            "HOME=/root".to_string(),
        ]),
        ..Default::default()
    };

    let create_opts = CreateContainerOptions {
        name: &container_name,
        platform: None,
    };

    // Create container
    let container = docker.create_container(Some(create_opts), container_config).await?;
    let container_id = container.id.clone();

    info!("[sandbox] Created container {} for command: {}", &container_id[..12], &command[..command.len().min(100)]);

    // Start container
    docker.start_container(&container_id, None::<StartContainerOptions<String>>).await
        .map_err(|e| {
            // Try to clean up on start failure
            let _ = cleanup_container(&docker, &container_id);
            format!("Failed to start sandbox container: {}", e)
        })?;

    // Wait for container to finish (with timeout)
    let timed_out;
    let exit_code;

    let wait_future = async {
        let mut stream = docker.wait_container(&container_id, None::<WaitContainerOptions<String>>);
        if let Some(result) = stream.next().await {
            match result {
                Ok(response) => response.status_code,
                Err(_) => -1,
            }
        } else {
            -1
        }
    };

    match tokio::time::timeout(Duration::from_secs(config.timeout_secs), wait_future).await {
        Ok(code) => {
            exit_code = code;
            timed_out = false;
        }
        Err(_) => {
            warn!("[sandbox] Container {} timed out after {}s", &container_id[..12], config.timeout_secs);
            // Kill the container
            let _ = docker.kill_container::<String>(&container_id, None).await;
            exit_code = -1;
            timed_out = true;
        }
    }

    // Collect logs (stdout + stderr)
    let log_opts = LogsOptions::<String> {
        stdout: true,
        stderr: true,
        follow: false,
        ..Default::default()
    };

    let mut stdout = String::new();
    let mut stderr = String::new();

    let mut log_stream = docker.logs(&container_id, Some(log_opts));
    while let Some(log_result) = log_stream.next().await {
        match log_result {
            Ok(output) => {
                match output {
                    bollard::container::LogOutput::StdOut { message } => {
                        stdout.push_str(&String::from_utf8_lossy(&message));
                    }
                    bollard::container::LogOutput::StdErr { message } => {
                        stderr.push_str(&String::from_utf8_lossy(&message));
                    }
                    _ => {}
                }
            }
            Err(e) => {
                warn!("[sandbox] Error reading container logs: {}", e);
                break;
            }
        }
    }

    // Truncate output to prevent context overflow
    const MAX_OUTPUT: usize = 50_000;
    if stdout.len() > MAX_OUTPUT {
        stdout.truncate(MAX_OUTPUT);
        stdout.push_str("\n... [stdout truncated]");
    }
    if stderr.len() > MAX_OUTPUT {
        stderr.truncate(MAX_OUTPUT);
        stderr.push_str("\n... [stderr truncated]");
    }

    // Remove container
    let remove_opts = RemoveContainerOptions {
        force: true,
        ..Default::default()
    };
    if let Err(e) = docker.remove_container(&container_id, Some(remove_opts)).await {
        warn!("[sandbox] Failed to remove container {}: {}", &container_id[..12], e);
    } else {
        info!("[sandbox] Removed container {}", &container_id[..12]);
    }

    Ok(SandboxResult {
        stdout,
        stderr,
        exit_code,
        timed_out,
        container_id,
    })
}

/// Clean up a container (best-effort, used in error paths).
async fn cleanup_container(docker: &Docker, container_id: &str) {
    let remove_opts = RemoveContainerOptions {
        force: true,
        ..Default::default()
    };
    let _ = docker.remove_container(container_id, Some(remove_opts)).await;
}

// ── Config persistence ─────────────────────────────────────────────────

/// Load sandbox config from engine_config table.
pub fn load_sandbox_config(store: &crate::engine::sessions::SessionStore) -> SandboxConfig {
    match store.get_config("sandbox_config") {
        Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_default(),
        _ => SandboxConfig::default(),
    }
}

/// Save sandbox config to engine_config table.
pub fn save_sandbox_config(store: &crate::engine::sessions::SessionStore, config: &SandboxConfig) -> EngineResult<()> {
    let json = serde_json::to_string(config)?;
    store.set_config("sandbox_config", &json)
}

// ── Format result for AI context ───────────────────────────────────────

/// Format a SandboxResult into a string suitable for inclusion in the AI context.
pub fn format_sandbox_result(result: &SandboxResult) -> String {
    let mut output = String::new();

    if result.timed_out {
        output.push_str("[SANDBOX: Command timed out]\n");
    }

    if !result.stdout.is_empty() {
        output.push_str(&result.stdout);
    }
    if !result.stderr.is_empty() {
        if !output.is_empty() {
            output.push_str("\n--- stderr ---\n");
        }
        output.push_str(&result.stderr);
    }
    if output.is_empty() || (output.starts_with("[SANDBOX") && result.stdout.is_empty()) {
        output.push_str(&format!("(sandbox exit code: {})", result.exit_code));
    }

    output
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = SandboxConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.image, "alpine:latest");
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.memory_limit, 256 * 1024 * 1024);
        assert!(!config.network_enabled);
        assert_eq!(config.workdir, "/workspace");
        assert!(config.bind_mounts.is_empty());
    }

    #[test]
    fn test_format_sandbox_result_stdout() {
        let result = SandboxResult {
            stdout: "hello world\n".to_string(),
            stderr: String::new(),
            exit_code: 0,
            timed_out: false,
            container_id: "abc123".to_string(),
        };
        let formatted = format_sandbox_result(&result);
        assert_eq!(formatted, "hello world\n");
    }

    #[test]
    fn test_format_sandbox_result_stderr() {
        let result = SandboxResult {
            stdout: "output\n".to_string(),
            stderr: "warning: something\n".to_string(),
            exit_code: 0,
            timed_out: false,
            container_id: "abc123".to_string(),
        };
        let formatted = format_sandbox_result(&result);
        assert!(formatted.contains("output"));
        assert!(formatted.contains("--- stderr ---"));
        assert!(formatted.contains("warning: something"));
    }

    #[test]
    fn test_format_sandbox_result_timeout() {
        let result = SandboxResult {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: -1,
            timed_out: true,
            container_id: "abc123".to_string(),
        };
        let formatted = format_sandbox_result(&result);
        assert!(formatted.contains("timed out"));
        assert!(formatted.contains("exit code: -1"));
    }

    #[test]
    fn test_config_serialization() {
        let config = SandboxConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: SandboxConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.image, config.image);
        assert_eq!(deserialized.timeout_secs, config.timeout_secs);
        assert_eq!(deserialized.memory_limit, config.memory_limit);
    }
}
