// n8n_engine/docker.rs — Docker container lifecycle management
//
// Handles connecting to Docker, provisioning/restarting/cleaning up the
// managed n8n container. Mirrors patterns from whatsapp/docker.rs.

use super::health::poll_n8n_ready;
use super::types::*;
use crate::atoms::error::{EngineError, EngineResult};
use bollard::container::{
    Config as ContainerConfig, CreateContainerOptions, ListContainersOptions,
    RemoveContainerOptions, StartContainerOptions,
};
use bollard::models::{HostConfig, PortBinding, RestartPolicy, RestartPolicyNameEnum};
use bollard::Docker;
use std::collections::HashMap;

// ── Docker connection ──────────────────────────────────────────────────

/// Try to connect to Docker daemon, including Colima socket discovery on macOS.
pub async fn connect_docker() -> EngineResult<Docker> {
    discover_colima_socket();
    Docker::connect_with_local_defaults()
        .map_err(|e| EngineError::Other(format!("Docker connection failed: {}", e)))
}

/// Check if the Docker daemon is reachable.
pub async fn is_docker_available() -> bool {
    match connect_docker().await {
        Ok(docker) => docker.ping().await.is_ok(),
        Err(_) => false,
    }
}

/// Discover Colima Docker socket on macOS and set DOCKER_HOST if needed.
#[cfg(target_os = "macos")]
fn discover_colima_socket() {
    if std::env::var("DOCKER_HOST").is_ok() {
        if let Ok(host) = std::env::var("DOCKER_HOST") {
            if let Some(path) = host.strip_prefix("unix://") {
                if std::path::Path::new(path).exists() {
                    return;
                }
            }
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        format!("{}/.colima/default/docker.sock", home),
        format!("{}/.colima/docker.sock", home),
    ];
    for path in &candidates {
        if std::path::Path::new(path).exists() {
            std::env::set_var("DOCKER_HOST", format!("unix://{}", path));
            return;
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn discover_colima_socket() {
    // No-op on non-macOS platforms.
}

// ── Container provisioning ─────────────────────────────────────────────

/// Provision a new n8n Docker container.
pub async fn provision_docker_container(
    app_handle: &tauri::AppHandle,
) -> EngineResult<N8nEndpoint> {
    let docker = connect_docker().await?;

    // Clean up any stale container with our name
    cleanup_stale_container(&docker).await;

    let port = find_available_port(DEFAULT_PORT);

    // Reuse encryption key and API key from previous config if they exist.
    // The n8n data directory is a persistent bind mount — if we generate a
    // new encryption key on re-provision, n8n will see a mismatch between
    // the env var and the key saved in /home/node/.n8n/config and crash.
    let prev_config = super::load_config(app_handle).ok();
    let api_key = prev_config
        .as_ref()
        .filter(|c| !c.api_key.is_empty())
        .map(|c| c.api_key.clone())
        .unwrap_or_else(generate_random_key);
    let encryption_key = prev_config
        .as_ref()
        .and_then(|c| c.encryption_key.clone())
        .filter(|k| !k.is_empty())
        .unwrap_or_else(generate_random_key);

    // Determine data volume path
    let data_dir = super::app_data_dir(app_handle).join("n8n-data");
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| EngineError::Other(format!("Failed to create n8n data dir: {}", e)))?;

    super::emit_status(
        app_handle,
        "downloading",
        "Pulling n8n image (first time only)...",
    );

    // Pull image (skip if already present)
    pull_image_if_needed(&docker, N8N_IMAGE).await?;

    super::emit_status(app_handle, "starting", "Starting integration engine...");

    // Build container config
    let mut port_bindings = HashMap::new();
    port_bindings.insert(
        "5678/tcp".to_string(),
        Some(vec![PortBinding {
            host_ip: Some("127.0.0.1".to_string()),
            host_port: Some(port.to_string()),
        }]),
    );

    let tz = std::env::var("TZ").unwrap_or_else(|_| "UTC".to_string());

    let env_vars = vec![
        "N8N_BASIC_AUTH_ACTIVE=false".to_string(),
        "N8N_SECURE_COOKIE=false".to_string(),
        format!("GENERIC_TIMEZONE={}", tz),
        format!("N8N_ENCRYPTION_KEY={}", encryption_key),
        format!("N8N_API_KEY={}", api_key),
        // Disable telemetry for self-hosted
        "N8N_DIAGNOSTICS_ENABLED=false".to_string(),
        "N8N_PERSONALIZATION_ENABLED=false".to_string(),
        // Enable community node installation (required for 25K+ packages)
        "N8N_COMMUNITY_PACKAGES_ENABLED=true".to_string(),
        // Allow installation of packages not in n8n's verified registry
        "N8N_COMMUNITY_PACKAGES_ALLOW_UNVERIFIED=true".to_string(),
        // Reinstall previously installed packages on startup
        "N8N_REINSTALL_MISSING_PACKAGES=true".to_string(),
        // Allow community packages to be used as tools in workflows
        "N8N_COMMUNITY_PACKAGES_ALLOW_TOOL_USAGE=true".to_string(),
    ];

    let host_config = HostConfig {
        port_bindings: Some(port_bindings),
        binds: Some(vec![format!(
            "{}:/home/node/.n8n",
            data_dir.to_string_lossy()
        )]),
        restart_policy: Some(RestartPolicy {
            name: Some(RestartPolicyNameEnum::UNLESS_STOPPED),
            maximum_retry_count: None,
        }),
        // Explicit DNS servers so npm registry is always reachable inside the container
        dns: Some(vec!["8.8.8.8".to_string(), "1.1.1.1".to_string()]),
        ..Default::default()
    };

    let mut exposed_ports = HashMap::new();
    exposed_ports.insert("5678/tcp".to_string(), HashMap::new());

    let container_config = ContainerConfig {
        image: Some(N8N_IMAGE.to_string()),
        env: Some(env_vars),
        host_config: Some(host_config),
        exposed_ports: Some(exposed_ports),
        ..Default::default()
    };

    let create_opts = CreateContainerOptions {
        name: CONTAINER_NAME,
        platform: None,
    };

    let container = docker
        .create_container(Some(create_opts), container_config)
        .await
        .map_err(|e| EngineError::Other(format!("Failed to create n8n container: {}", e)))?;

    docker
        .start_container(&container.id, None::<StartContainerOptions<String>>)
        .await
        .map_err(|e| EngineError::Other(format!("Failed to start n8n container: {}", e)))?;

    // Poll for readiness
    let url = format!("http://127.0.0.1:{}", port);
    let ready = poll_n8n_ready(&url, &api_key).await;
    if !ready {
        super::emit_status(
            app_handle,
            "error",
            "Integration engine started but isn't responding. Check Docker logs.",
        );
        return Err(EngineError::Other(format!(
            "n8n container started but failed to become healthy within {}s",
            STARTUP_TIMEOUT_SECS
        )));
    }

    // Set up the owner account for headless operation.
    // n8n requires this before MCP and other features are usable.
    if let Err(e) = super::health::setup_owner_if_needed(&url).await {
        log::warn!("[n8n] Owner setup failed (non-fatal): {}", e);
    }

    // Log the n8n version for diagnostics (MCP requires recent versions)
    if let Some(version) = super::health::get_n8n_version(&url, &api_key).await {
        log::info!("[n8n] Docker mode running n8n v{}", version);
    }

    // Enable MCP access (disabled by default even after owner creation)
    if let Err(e) = super::health::enable_mcp_access(&url).await {
        log::warn!("[n8n] MCP access enable failed (non-fatal): {}", e);
    }

    // Persist config
    let new_config = N8nEngineConfig {
        mode: N8nMode::Embedded,
        url: url.clone(),
        api_key: api_key.clone(),
        container_id: Some(container.id),
        container_port: Some(port),
        encryption_key: Some(encryption_key),
        process_pid: None,
        process_port: None,
        mcp_token: None,
        enabled: true,
        auto_discover: true,
        mcp_mode: true,
    };
    super::save_config(app_handle, &new_config)?;

    super::emit_status(app_handle, "ready", "Integration engine ready.");

    Ok(N8nEndpoint {
        url,
        api_key,
        mode: N8nMode::Embedded,
    })
}

/// Pull Docker image if not already present locally.
async fn pull_image_if_needed(docker: &Docker, image: &str) -> EngineResult<()> {
    use bollard::image::CreateImageOptions;
    use futures::StreamExt;

    // Check if image exists locally
    if docker.inspect_image(image).await.is_ok() {
        return Ok(());
    }

    let opts = CreateImageOptions {
        from_image: image,
        ..Default::default()
    };

    let mut stream = docker.create_image(Some(opts), None, None);
    while let Some(result) = stream.next().await {
        result.map_err(|e| EngineError::Other(format!("Image pull failed: {}", e)))?;
    }

    Ok(())
}

/// Try to restart an existing container that was previously provisioned.
pub async fn restart_existing_container(
    app_handle: &tauri::AppHandle,
    config: &N8nEngineConfig,
) -> EngineResult<N8nEndpoint> {
    let docker = connect_docker().await?;
    let container_id = config
        .container_id
        .as_deref()
        .ok_or_else(|| EngineError::Other("No container ID stored".into()))?;

    // Check if the container actually exists before wasting time polling
    let container_info = match docker.inspect_container(container_id, None).await {
        Ok(info) => Some(info),
        Err(_) => {
            // Also check by name in case the ID changed
            match docker.inspect_container(CONTAINER_NAME, None).await {
                Ok(info) => Some(info),
                Err(_) => {
                    return Err(EngineError::Other(
                        "Container no longer exists — will re-provision".into(),
                    ));
                }
            }
        }
    };

    // Detect crash-looping container (e.g. encryption key mismatch).
    // If the container is in "restarting" state, it will never become
    // healthy — fail fast so we can re-provision with correct config.
    if let Some(info) = &container_info {
        if let Some(state) = &info.state {
            if state.restarting == Some(true) {
                return Err(EngineError::Other(
                    "Container is crash-looping — will re-provision".into(),
                ));
            }
        }
    }

    super::emit_status(app_handle, "starting", "Restarting integration engine...");

    // Try to start the container (it may be stopped)
    let _ = docker
        .start_container(container_id, None::<StartContainerOptions<String>>)
        .await;

    let port = config.container_port.unwrap_or(DEFAULT_PORT);
    let url = format!("http://127.0.0.1:{}", port);

    if poll_n8n_ready(&url, &config.api_key).await {
        // Ensure owner account exists (idempotent)
        let _ = super::health::setup_owner_if_needed(&url).await;
        // Ensure MCP access is enabled
        let _ = super::health::enable_mcp_access(&url).await;
        super::emit_status(app_handle, "ready", "Integration engine ready.");
        Ok(N8nEndpoint {
            url,
            api_key: config.api_key.clone(),
            mode: N8nMode::Embedded,
        })
    } else {
        Err(EngineError::Other(
            "Container restart failed — will re-provision".into(),
        ))
    }
}

/// Remove any stale container with our name.
async fn cleanup_stale_container(docker: &Docker) {
    let mut filters = HashMap::new();
    filters.insert("name".to_string(), vec![CONTAINER_NAME.to_string()]);
    let opts = ListContainersOptions {
        all: true,
        filters,
        ..Default::default()
    };
    if let Ok(containers) = docker.list_containers(Some(opts)).await {
        for c in containers {
            if let Some(id) = c.id {
                let _ = docker.stop_container(&id, None).await;
                let _ = docker
                    .remove_container(
                        &id,
                        Some(RemoveContainerOptions {
                            force: true,
                            ..Default::default()
                        }),
                    )
                    .await;
            }
        }
    }
}
