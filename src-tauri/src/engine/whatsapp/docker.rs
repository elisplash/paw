// WhatsApp Bridge — Docker Container Management
// EVOLUTION_IMAGE, CONTAINER_NAME, discover_colima_socket,
// ensure_docker_ready, ensure_evolution_container

use crate::atoms::error::{EngineResult, EngineError};
use log::{info, warn, error};
use serde_json::json;
use std::sync::atomic::Ordering;
use tauri::Emitter;
use super::bridge::get_stop_signal;
use super::config::WhatsAppConfig;

// ── Constants ──────────────────────────────────────────────────────────

// v1.8.6 works standalone with SQLite — no Redis or PostgreSQL needed.
// v2.x requires PostgreSQL + Redis (multi-container) which is too heavy for local use.
pub(crate) const EVOLUTION_IMAGE: &str = "atendai/evolution-api:v1.8.6";
pub(crate) const CONTAINER_NAME: &str = "paw-whatsapp-evolution";

// ── Colima Socket Discovery ────────────────────────────────────────────

/// On macOS, Colima creates its Docker socket at ~/.colima/default/docker.sock
/// rather than the standard /var/run/docker.sock. This function finds the
/// Colima socket and sets DOCKER_HOST so bollard and Docker CLI can use it.
#[cfg(target_os = "macos")]
pub(crate) fn discover_colima_socket() {
    // If DOCKER_HOST is already set and the socket file exists, respect it
    if let Ok(existing) = std::env::var("DOCKER_HOST") {
        let path = existing.strip_prefix("unix://").unwrap_or(&existing);
        if std::path::Path::new(path).exists() {
            return;
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        format!("{}/.colima/default/docker.sock", home),
        format!("{}/.colima/docker.sock", home),
    ];
    for sock in &candidates {
        if std::path::Path::new(sock).exists() {
            let docker_host = format!("unix://{}", sock);
            info!("[whatsapp] Found Colima socket, setting DOCKER_HOST={}", docker_host);
            std::env::set_var("DOCKER_HOST", &docker_host);
            return;
        }
    }
}

/// No-op on non-macOS — Colima is macOS-only.
#[cfg(not(target_os = "macos"))]
pub(crate) fn discover_colima_socket() {}

// ── Docker Readiness ───────────────────────────────────────────────────

/// Check if the WhatsApp backend service is reachable.
/// If Docker isn't installed, install it automatically.
/// If Docker isn't running, start it automatically.
/// Returns the Docker client on success, or a user-friendly error.
pub(crate) async fn ensure_docker_ready(app_handle: &tauri::AppHandle) -> EngineResult<bollard::Docker> {
    use bollard::Docker;

    let stop = get_stop_signal();

    // On macOS, Colima puts its Docker socket at ~/.colima/default/docker.sock
    // instead of /var/run/docker.sock. Discover it and set DOCKER_HOST so bollard
    // (and any docker CLI calls) can find it.
    discover_colima_socket();

    // First attempt: connect directly — fastest path
    if let Ok(docker) = Docker::connect_with_local_defaults() {
        if docker.ping().await.is_ok() {
            return Ok(docker);
        }
    }

    // Check if we've been told to stop before doing anything slow
    if stop.load(Ordering::Relaxed) {
        return Err("Cancelled".into());
    }

    info!("[whatsapp] Docker not responding, checking installation...");
    let _ = app_handle.emit("whatsapp-status", json!({
        "kind": "docker_starting",
        "message": "Setting up WhatsApp...",
    }));

    // Check if docker CLI exists at all
    let docker_installed = std::process::Command::new("docker")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !docker_installed {
        // Auto-install Docker Engine
        info!("[whatsapp] Docker not found, installing automatically...");
        let _ = app_handle.emit("whatsapp-status", json!({
            "kind": "installing",
            "message": "Installing WhatsApp service (first time only)...",
        }));

        let install_ok = if cfg!(target_os = "macos") {
            // macOS: use Homebrew to install Docker CLI + Colima (lightweight runtime)
            let brew_ok = std::process::Command::new("brew")
                .args(["install", "docker", "colima"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if brew_ok {
                // Start Colima (lightweight Docker VM — no GUI needed)
                std::process::Command::new("colima")
                    .arg("start")
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false)
            } else {
                false
            }
        } else if cfg!(target_os = "windows") {
            // Windows: try winget
            std::process::Command::new("winget")
                .args(["install", "--id", "Docker.DockerCLI", "--accept-source-agreements", "--accept-package-agreements"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        } else {
            // Linux: use official install script
            let curl_ok = std::process::Command::new("sh")
                .args(["-c", "curl -fsSL https://get.docker.com | sh"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if curl_ok {
                // Start Docker daemon
                let _ = std::process::Command::new("sudo")
                    .args(["systemctl", "start", "docker"])
                    .output();
                // Add user to docker group so future runs don't need sudo
                let _ = std::process::Command::new("sudo")
                    .args(["usermod", "-aG", "docker", &std::env::var("USER").unwrap_or_default()])
                    .output();
                true
            } else {
                false
            }
        };

        if !install_ok {
            let _ = app_handle.emit("whatsapp-status", json!({
                "kind": "install_failed",
                "message": "Couldn't set up WhatsApp automatically. Please try again or check your internet connection.",
            }));
            return Err(
                "WhatsApp couldn't be set up automatically. Check your internet connection and try again."
                .into()
            );
        }

        info!("[whatsapp] Docker installed successfully");
        // Discover Colima socket after fresh install
        discover_colima_socket();
    } else {
        // Docker CLI is installed but daemon isn't running — start it
        info!("[whatsapp] Docker installed but not running, starting...");

        if cfg!(target_os = "macos") {
            // macOS needs a VM runtime (Colima or Docker Desktop) to run containers.
            // Check if Colima is installed; if not, install it via Homebrew.
            let colima_exists = std::process::Command::new("which")
                .arg("colima")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);

            if !colima_exists {
                info!("[whatsapp] Colima not found, installing via Homebrew...");
                let _ = app_handle.emit("whatsapp-status", json!({
                    "kind": "installing",
                    "message": "Installing WhatsApp service (first time only — this may take a minute)...",
                }));
                let brew_ok = std::process::Command::new("brew")
                    .args(["install", "colima"])
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);
                if !brew_ok {
                    let _ = app_handle.emit("whatsapp-status", json!({
                        "kind": "install_failed",
                        "message": "Couldn't install the WhatsApp service. Make sure Homebrew (brew.sh) is installed and try again.",
                    }));
                    return Err("Couldn't install required service. Make sure Homebrew is installed (brew.sh) and try again.".into());
                }
                info!("[whatsapp] Colima installed successfully");
            }

            // Start Colima (boots a lightweight Linux VM)
            info!("[whatsapp] Starting Colima...");
            let _ = app_handle.emit("whatsapp-status", json!({
                "kind": "docker_starting",
                "message": "Starting WhatsApp service...",
            }));
            let colima_start = std::process::Command::new("colima")
                .arg("start")
                .output();
            match colima_start {
                Ok(out) if out.status.success() => {
                    info!("[whatsapp] Colima started successfully");
                }
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    // If already running, that's fine
                    if !stderr.contains("already running") {
                        warn!("[whatsapp] Colima start issue: {}", stderr);
                    }
                }
                Err(e) => {
                    warn!("[whatsapp] Failed to start colima: {}", e);
                }
            }
            // Re-discover the socket now that Colima is (hopefully) running
            discover_colima_socket();
        } else if cfg!(target_os = "windows") {
            let _ = std::process::Command::new("cmd")
                .args(["/C", "net", "start", "com.docker.service"])
                .spawn();
        } else {
            let _ = std::process::Command::new("sudo")
                .args(["systemctl", "start", "docker"])
                .output();
        }
    }

    // Poll for Docker to become ready (up to 40 seconds — Colima VM boot can take ~20s)
    info!("[whatsapp] Waiting for backend service to start...");
    for attempt in 1..=20 {
        // Check stop signal each iteration so Stop/Remove can interrupt
        if stop.load(Ordering::Relaxed) {
            info!("[whatsapp] Stop signal received during Docker wait");
            return Err("Cancelled".into());
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        // On macOS, re-check for Colima socket each iteration (it may
        // appear after Colima finishes booting its VM)
        discover_colima_socket();
        if let Ok(docker) = Docker::connect_with_local_defaults() {
            if docker.ping().await.is_ok() {
                info!("[whatsapp] Backend service ready after ~{}s", attempt * 2);
                let _ = app_handle.emit("whatsapp-status", json!({
                    "kind": "docker_ready",
                    "message": "Setting up WhatsApp...",
                }));
                return Ok(docker);
            }
        }
        if attempt % 5 == 0 {
            info!("[whatsapp] Still waiting for backend... ({}s)", attempt * 2);
        }
    }

    let _ = app_handle.emit("whatsapp-status", json!({
        "kind": "docker_timeout",
        "message": "WhatsApp is still loading. Try again in a moment.",
    }));
    Err(
        "WhatsApp service didn't start in time. Give it a moment and try again."
        .into()
    )
}

// ── Docker DRY Helpers ─────────────────────────────────────────────────

/// Poll the Evolution API URL every 2 s for up to 60 s (30 attempts).
/// Returns `true` if the API responds with 2xx or 401; `false` on timeout.
async fn poll_api_ready(client: &reqwest::Client, api_url: &str) -> bool {
    for attempt in 1..=30 {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        match client.get(api_url).send().await {
            Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 401 => {
                info!("[whatsapp] Evolution API ready after {} attempts", attempt);
                return true;
            }
            _ => {
                if attempt % 5 == 0 {
                    info!("[whatsapp] Waiting for Evolution API to start... (attempt {}/30)", attempt);
                }
            }
        }
    }
    false
}

/// Force-stop and remove a Docker container, ignoring errors.
async fn force_remove_container(docker: &bollard::Docker, container_id: &str) {
    let _ = docker.stop_container(container_id, None).await;
    let opts = bollard::container::RemoveContainerOptions { force: true, ..Default::default() };
    let _ = docker.remove_container(container_id, Some(opts)).await;
}

// ── Evolution Container ────────────────────────────────────────────────

/// Ensure the Evolution API Docker container is running.
/// Pulls the image if needed, creates and starts the container.
pub(crate) async fn ensure_evolution_container(app_handle: &tauri::AppHandle, config: &WhatsAppConfig) -> EngineResult<String> {
    use bollard::container::{Config as ContainerConfig, CreateContainerOptions, StartContainerOptions, ListContainersOptions};
    use bollard::models::HostConfig;
    use bollard::image::CreateImageOptions;
    use futures::StreamExt;

    let docker = ensure_docker_ready(app_handle).await?;

    // Check if our container already exists
    let mut filters = std::collections::HashMap::new();
    filters.insert("name".to_string(), vec![CONTAINER_NAME.to_string()]);
    let opts = ListContainersOptions {
        all: true,
        filters,
        ..Default::default()
    };

    let containers = docker.list_containers(Some(opts)).await
        .map_err(|e| EngineError::Other(e.to_string()))?;

    if let Some(existing) = containers.first() {
        let container_id = existing.id.clone().unwrap_or_default();
        let state = existing.state.as_deref().unwrap_or("");
        let image = existing.image.as_deref().unwrap_or("");

        // Check if container needs recreating (wrong image or stale API key)
        let needs_recreate = if image != EVOLUTION_IMAGE {
            info!("[whatsapp] Container uses wrong image ({} vs {}), recreating...", image, EVOLUTION_IMAGE);
            true
        } else {
            // Inspect env vars to check API key matches
            let inspect = docker.inspect_container(&container_id, None).await.ok();
            let env_key = inspect
                .as_ref()
                .and_then(|i| i.config.as_ref())
                .and_then(|c| c.env.as_ref())
                .and_then(|envs| {
                    envs.iter()
                        .find(|e| e.starts_with("AUTHENTICATION_API_KEY="))
                        .map(|e| e.trim_start_matches("AUTHENTICATION_API_KEY=").to_string())
                });
            if let Some(ref existing_key) = env_key {
                if existing_key != &config.api_key {
                    info!("[whatsapp] Container has stale API key, recreating...");
                    true
                } else {
                    false
                }
            } else {
                false
            }
        };

        if needs_recreate {
            force_remove_container(&docker, &container_id).await;
            // Fall through to create below
        } else if state == "restarting" || state == "dead" {
            info!("[whatsapp] Container is {} — removing and recreating", state);
            force_remove_container(&docker, &container_id).await;
            info!("[whatsapp] Old container removed");
            // Fall through to create a new container below
        } else if state == "running" {
            // Verify the API is actually responding (not just "running" in Docker)
            let client = reqwest::Client::new();
            let api_url = format!("http://127.0.0.1:{}", config.api_port);
            match client.get(&api_url).send().await {
                Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 401 => {
                    info!("[whatsapp] Evolution API container already running and healthy: {}", &container_id[..12]);
                    return Ok(container_id);
                }
                _ => {
                    // Container says "running" but API not responding — wait for it
                    info!("[whatsapp] Container running but API not ready, waiting...");
                }
            }
            // Wait for API to be ready (reuses same client/api_url)
            if poll_api_ready(&client, &api_url).await {
                return Ok(container_id);
            }
            // API never came up — remove and recreate
            info!("[whatsapp] Running container never became healthy — removing");
            force_remove_container(&docker, &container_id).await;
            // Fall through to create new container
        } else {
            // Container exists but stopped — start it and wait for API
            info!("[whatsapp] Starting existing Evolution API container");
            docker.start_container(&container_id, None::<StartContainerOptions<String>>).await
                .map_err(|e| EngineError::Other(e.to_string()))?;

            info!("[whatsapp] Waiting for Evolution API to be ready...");
            let client = reqwest::Client::new();
            let api_url = format!("http://127.0.0.1:{}", config.api_port);
            if poll_api_ready(&client, &api_url).await {
                return Ok(container_id);
            }
            return Err("WhatsApp service started but didn't become ready. Try again in a moment.".into());
        }
    }

    // Pull image if not present
    info!("[whatsapp] Pulling Evolution API image (first time setup)...");
    match docker.inspect_image(EVOLUTION_IMAGE).await {
        Ok(_) => info!("[whatsapp] Image already present"),
        Err(_) => {
            let _ = app_handle.emit("whatsapp-status", json!({
                "kind": "downloading",
                "message": "First-time setup — downloading WhatsApp service...",
            }));
            let pull_opts = CreateImageOptions {
                from_image: EVOLUTION_IMAGE,
                ..Default::default()
            };
            let mut stream = docker.create_image(Some(pull_opts), None, None);
            while let Some(result) = stream.next().await {
                if let Err(e) = result {
                    return Err(EngineError::Other(e.to_string()));
                }
            }
            info!("[whatsapp] Image pulled successfully");
        }
    }

    // Create container
    let host_config = HostConfig {
        port_bindings: Some({
            let mut ports = std::collections::HashMap::new();
            ports.insert(
                "8080/tcp".to_string(),
                Some(vec![bollard::models::PortBinding {
                    host_ip: Some("127.0.0.1".to_string()),
                    host_port: Some(config.api_port.to_string()),
                }]),
            );
            ports
        }),
        restart_policy: Some(bollard::models::RestartPolicy {
            name: Some(bollard::models::RestartPolicyNameEnum::UNLESS_STOPPED),
            maximum_retry_count: None,
        }),
        ..Default::default()
    };

    let container_config = ContainerConfig {
        image: Some(EVOLUTION_IMAGE.to_string()),
        env: Some(vec![
            // v1.x defaults to JWT auth; explicitly switch to API key auth
            "AUTHENTICATION_TYPE=apikey".to_string(),
            format!("AUTHENTICATION_API_KEY={}", config.api_key),
            "SERVER_PORT=8080".to_string(),
            // Webhook: point back to Paw's webhook listener
            format!("WEBHOOK_GLOBAL_URL=http://host.docker.internal:{}/webhook/whatsapp", config.webhook_port),
            "WEBHOOK_GLOBAL_ENABLED=true".to_string(),
            "WEBHOOK_GLOBAL_WEBHOOK_BY_EVENTS=true".to_string(),
            // Events we care about
            "WEBHOOK_EVENTS_MESSAGES_UPSERT=true".to_string(),
            "WEBHOOK_EVENTS_QRCODE_UPDATED=true".to_string(),
            "WEBHOOK_EVENTS_CONNECTION_UPDATE=true".to_string(),
            // Disable features we don't need
            "WEBHOOK_EVENTS_MESSAGES_UPDATE=false".to_string(),
            "WEBHOOK_EVENTS_SEND_MESSAGE=false".to_string(),
            // Database: SQLite inside the container (persisted via volume)
            "DATABASE_PROVIDER=sqlite".to_string(),
            "DATABASE_CONNECTION_URI=file:./data/evolution.db".to_string(),
        ]),
        host_config: Some(host_config),
        exposed_ports: Some({
            let mut ports = std::collections::HashMap::new();
            ports.insert("8080/tcp".to_string(), std::collections::HashMap::new());
            ports
        }),
        ..Default::default()
    };

    let create_opts = CreateContainerOptions {
        name: CONTAINER_NAME,
        platform: None,
    };

    let container = docker.create_container(Some(create_opts), container_config).await
        .map_err(|e| EngineError::Other(e.to_string()))?;

    let container_id = container.id.clone();
    info!("[whatsapp] Created Evolution API container: {}", &container_id[..12]);

    // Start it
    docker.start_container(&container_id, None::<StartContainerOptions<String>>).await
        .map_err(|e| EngineError::Other(e.to_string()))?;

    info!("[whatsapp] Evolution API container started on port {}", config.api_port);

    // Wait for the API to be ready
    let client = reqwest::Client::new();
    let api_url = format!("http://127.0.0.1:{}", config.api_port);
    if poll_api_ready(&client, &api_url).await {
        return Ok(container_id);
    }

    // Fetch container logs to diagnose why it's not starting
    let log_opts = bollard::container::LogsOptions::<String> {
        stdout: true,
        stderr: true,
        tail: "20".to_string(),
        ..Default::default()
    };
    let mut log_stream = docker.logs(&container_id, Some(log_opts));
    let mut log_lines = Vec::new();
    while let Some(Ok(line)) = futures::StreamExt::next(&mut log_stream).await {
        log_lines.push(line.to_string());
    }
    if !log_lines.is_empty() {
        error!("[whatsapp] Container logs (last 20 lines):\n{}", log_lines.join(""));
    }
    Err("WhatsApp service didn't start. It may need more time — try again in a moment.".into())
}
