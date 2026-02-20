// Paw Agent Engine — WhatsApp Bridge (via Evolution API)
//
// Connects Paw to WhatsApp via a local Evolution API Docker container.
// Evolution API wraps the Baileys library (WhatsApp Web multi-device protocol)
// and exposes a clean REST + Webhook interface — zero WebSocket code in Paw.
//
// Architecture:
//   1. Paw auto-manages an Evolution API Docker container (pull + run)
//   2. User scans a QR code to link their WhatsApp account
//   3. Inbound messages arrive via webhook (Evolution → Paw's local HTTP listener)
//   4. Paw routes through the agent loop and replies via Evolution's REST API
//
// Setup: Enable WhatsApp in Channels → Paw handles Docker + QR code automatically.
//
// Security:
//   - Allowlist by phone number or WhatsApp JID
//   - Optional pairing mode (first message from unknown → pending approval)
//   - All WhatsApp traffic stays on your machine (Docker container)
//   - Evolution API bound to localhost only

use crate::engine::channels::{self, PendingUser, ChannelStatus};
use log::{info, warn, error};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use tauri::Emitter;

// ── WhatsApp Config ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WhatsAppConfig {
    pub enabled: bool,
    /// Instance name for Evolution API (default: "paw")
    pub instance_name: String,
    /// Evolution API base URL (auto-set when Docker container starts)
    pub api_url: String,
    /// Evolution API key (auto-generated on first run)
    pub api_key: String,
    /// Port for the Evolution API container (default: 8085)
    pub api_port: u16,
    /// Port for the local webhook listener (default: 8086)
    pub webhook_port: u16,
    /// "open" | "allowlist" | "pairing"
    pub dm_policy: String,
    /// Allowed phone numbers or WhatsApp JIDs (e.g. "1234567890" or "1234567890@s.whatsapp.net")
    pub allowed_users: Vec<String>,
    #[serde(default)]
    pub pending_users: Vec<PendingUser>,
    /// Which agent to route messages to
    pub agent_id: Option<String>,
    /// Whether to respond in group chats (when mentioned)
    #[serde(default)]
    pub respond_in_groups: bool,
    /// Docker container ID (managed internally)
    #[serde(default)]
    pub container_id: Option<String>,
    /// Whether the WhatsApp session is connected (QR scanned)
    #[serde(default)]
    pub session_connected: bool,
    /// QR code data (base64) for the frontend to display
    #[serde(default)]
    pub qr_code: Option<String>,
}

impl Default for WhatsAppConfig {
    fn default() -> Self {
        // Generate a random API key on first creation
        let api_key = format!("paw-wa-{}", uuid::Uuid::new_v4().to_string().replace('-', "")[..16].to_string());
        WhatsAppConfig {
            enabled: false,
            instance_name: "paw".into(),
            api_url: "http://127.0.0.1:8085".into(),
            api_key,
            api_port: 8085,
            webhook_port: 8086,
            dm_policy: "pairing".into(),
            allowed_users: vec![],
            pending_users: vec![],
            agent_id: None,
            respond_in_groups: false,
            container_id: None,
            session_connected: false,
            qr_code: None,
        }
    }
}

// ── Global State ───────────────────────────────────────────────────────

static BRIDGE_RUNNING: AtomicBool = AtomicBool::new(false);
static MESSAGE_COUNT: AtomicI64 = AtomicI64::new(0);
static STOP_SIGNAL: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();

fn get_stop_signal() -> Arc<AtomicBool> {
    STOP_SIGNAL.get_or_init(|| Arc::new(AtomicBool::new(false))).clone()
}

const CONFIG_KEY: &str = "whatsapp_config";
// v1.8.6 works standalone with SQLite — no Redis or PostgreSQL needed.
// v2.x requires PostgreSQL + Redis (multi-container) which is too heavy for local use.
const EVOLUTION_IMAGE: &str = "atendai/evolution-api:v1.8.6";
const CONTAINER_NAME: &str = "paw-whatsapp-evolution";

// ── Bridge Core ────────────────────────────────────────────────────────

pub fn start_bridge(app_handle: tauri::AppHandle) -> Result<(), String> {
    if BRIDGE_RUNNING.load(Ordering::Relaxed) {
        return Err("WhatsApp bridge is already running".into());
    }

    let mut config: WhatsAppConfig = channels::load_channel_config(&app_handle, CONFIG_KEY)?;
    if !config.enabled {
        return Err("WhatsApp bridge is disabled. Enable it in Channels settings.".into());
    }
    // Ensure API key is never empty (old configs may have been saved without one)
    if config.api_key.is_empty() {
        config.api_key = format!("paw-wa-{}", uuid::Uuid::new_v4().to_string().replace('-', "")[..16].to_string());
        let _ = channels::save_channel_config(&app_handle, CONFIG_KEY, &config);
    }

    let stop = get_stop_signal();
    stop.store(false, Ordering::Relaxed);
    BRIDGE_RUNNING.store(true, Ordering::Relaxed);

    info!("[whatsapp] Starting bridge via Evolution API");

    let app = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run_whatsapp_bridge(app.clone(), config).await {
            error!("[whatsapp] Bridge crashed: {}", e);
            let _ = app.emit("whatsapp-status", json!({
                "kind": "error",
                "message": format!("{}", e),
            }));
        }
        BRIDGE_RUNNING.store(false, Ordering::Relaxed);
        info!("[whatsapp] Bridge stopped");
    });

    Ok(())
}

pub fn stop_bridge() {
    let stop = get_stop_signal();
    stop.store(true, Ordering::Relaxed);
    BRIDGE_RUNNING.store(false, Ordering::Relaxed);
    info!("[whatsapp] Stop signal sent");
}

pub fn get_status(app_handle: &tauri::AppHandle) -> ChannelStatus {
    let config: WhatsAppConfig = channels::load_channel_config(app_handle, CONFIG_KEY).unwrap_or_default();
    ChannelStatus {
        running: BRIDGE_RUNNING.load(Ordering::Relaxed),
        connected: config.session_connected && BRIDGE_RUNNING.load(Ordering::Relaxed),
        bot_name: Some("WhatsApp".into()),
        bot_id: config.container_id.clone(),
        message_count: MESSAGE_COUNT.load(Ordering::Relaxed) as u64,
        allowed_users: config.allowed_users,
        pending_users: config.pending_users,
        dm_policy: config.dm_policy,
    }
}

// ── Docker Container Management ────────────────────────────────────────

/// On macOS, Colima creates its Docker socket at ~/.colima/default/docker.sock
/// rather than the standard /var/run/docker.sock. This function finds the
/// Colima socket and sets DOCKER_HOST so bollard and Docker CLI can use it.
#[cfg(target_os = "macos")]
fn discover_colima_socket() {
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
fn discover_colima_socket() {}

/// Check if the WhatsApp backend service is reachable.
/// If Docker isn't installed, install it automatically.
/// If Docker isn't running, start it automatically.
/// Returns the Docker client on success, or a user-friendly error.
async fn ensure_docker_ready(app_handle: &tauri::AppHandle) -> Result<bollard::Docker, String> {
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

/// Ensure the Evolution API Docker container is running.
/// Pulls the image if needed, creates and starts the container.
async fn ensure_evolution_container(app_handle: &tauri::AppHandle, config: &WhatsAppConfig) -> Result<String, String> {
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
        .map_err(|e| format!("Failed to list containers: {}", e))?;

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
            let _ = docker.stop_container(&container_id, None).await;
            let remove_opts = bollard::container::RemoveContainerOptions { force: true, ..Default::default() };
            let _ = docker.remove_container(&container_id, Some(remove_opts)).await;
            // Fall through to create below
        } else if state == "restarting" || state == "dead" {
            info!("[whatsapp] Container is {} — removing and recreating", state);
            // Force stop + remove
            let _ = docker.stop_container(&container_id, None).await;
            let remove_opts = bollard::container::RemoveContainerOptions { force: true, ..Default::default() };
            let _ = docker.remove_container(&container_id, Some(remove_opts)).await;
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
            // Wait for API to be ready
            let client = reqwest::Client::new();
            let api_url = format!("http://127.0.0.1:{}", config.api_port);
            for attempt in 1..=30 {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                match client.get(&api_url).send().await {
                    Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 401 => {
                        info!("[whatsapp] Evolution API ready after {} attempts", attempt);
                        return Ok(container_id);
                    }
                    _ => {
                        if attempt % 5 == 0 {
                            info!("[whatsapp] Waiting for Evolution API to start... (attempt {}/30)", attempt);
                        }
                    }
                }
            }
            // API never came up — remove and recreate
            info!("[whatsapp] Running container never became healthy — removing");
            let _ = docker.stop_container(&container_id, None).await;
            let remove_opts = bollard::container::RemoveContainerOptions { force: true, ..Default::default() };
            let _ = docker.remove_container(&container_id, Some(remove_opts)).await;
            // Fall through to create new container
        } else {
            // Container exists but stopped — start it and wait for API
            info!("[whatsapp] Starting existing Evolution API container");
            docker.start_container(&container_id, None::<StartContainerOptions<String>>).await
                .map_err(|e| format!("Failed to start container: {}", e))?;

            info!("[whatsapp] Waiting for Evolution API to be ready...");
            let client = reqwest::Client::new();
            let api_url = format!("http://127.0.0.1:{}", config.api_port);
            for attempt in 1..=30 {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                match client.get(&api_url).send().await {
                    Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 401 => {
                        info!("[whatsapp] Evolution API ready after {} attempts", attempt);
                        return Ok(container_id);
                    }
                    _ => {
                        if attempt % 5 == 0 {
                            info!("[whatsapp] Waiting for Evolution API to start... (attempt {}/30)", attempt);
                        }
                    }
                }
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
                    return Err(format!("Failed to pull Evolution API image: {}", e));
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
        .map_err(|e| format!("Failed to create Evolution API container: {}", e))?;

    let container_id = container.id.clone();
    info!("[whatsapp] Created Evolution API container: {}", &container_id[..12]);

    // Start it
    docker.start_container(&container_id, None::<StartContainerOptions<String>>).await
        .map_err(|e| format!("Failed to start Evolution API container: {}", e))?;

    info!("[whatsapp] Evolution API container started on port {}", config.api_port);

    // Wait for the API to be ready
    let client = reqwest::Client::new();
    let api_url = format!("http://127.0.0.1:{}", config.api_port);
    for attempt in 1..=30 {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        match client.get(&api_url).send().await {
            Ok(resp) if resp.status().is_success() || resp.status().as_u16() == 401 => {
                info!("[whatsapp] Evolution API ready after {} attempts", attempt);
                return Ok(container_id);
            }
            _ => {
                if attempt % 5 == 0 {
                    info!("[whatsapp] Waiting for Evolution API to start... (attempt {}/30)", attempt);
                }
            }
        }
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

/// Create a WhatsApp instance in Evolution API and get the QR code.
async fn create_evolution_instance(config: &WhatsAppConfig) -> Result<String, String> {
    let client = reqwest::Client::new();
    let url = format!("{}/instance/create", config.api_url);

    // v1.x format: webhook is a plain string URL, no "integration" field.
    // Webhook events are configured via container env vars (WEBHOOK_GLOBAL_*).
    // Provide a unique token per instance to avoid "Token already exists" collisions.
    let instance_token = format!("paw-{}", uuid::Uuid::new_v4().to_string().replace('-', "")[..12].to_string());
    let body = json!({
        "instanceName": config.instance_name,
        "token": instance_token,
        "qrcode": true,
        "webhook": format!("http://host.docker.internal:{}/webhook/whatsapp", config.webhook_port),
    });

    info!("[whatsapp] Creating instance '{}' with token '{}'", config.instance_name, instance_token);

    let resp = client.post(&url)
        .header("apikey", &config.api_key)
        .json(&body)
        .send().await
        .map_err(|e| format!("Failed to create instance: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    info!("[whatsapp] Instance create response [{}]: {}", status, &text[..text.len().min(500)]);

    if !status.is_success() {
        // Instance with this name already exists — delete it and retry
        let lower = text.to_lowercase();
        let is_instance_exists = lower.contains("instance") && (lower.contains("already") || lower.contains("exists"));
        let is_token_exists = lower.contains("token") && lower.contains("already");

        if is_instance_exists || is_token_exists {
            info!("[whatsapp] Instance/token conflict, deleting instance and recreating...");
            delete_evolution_instance(config).await;

            // Generate a fresh token for the retry
            let retry_token = format!("paw-{}", uuid::Uuid::new_v4().to_string().replace('-', "")[..12].to_string());
            let retry_body = json!({
                "instanceName": config.instance_name,
                "token": retry_token,
                "qrcode": true,
                "webhook": format!("http://host.docker.internal:{}/webhook/whatsapp", config.webhook_port),
            });

            // Retry create after delete
            let resp2 = client.post(&url)
                .header("apikey", &config.api_key)
                .json(&retry_body)
                .send().await
                .map_err(|e| format!("Failed to create instance (retry): {}", e))?;

            let status2 = resp2.status();
            let text2 = resp2.text().await.unwrap_or_default();
            info!("[whatsapp] Instance create (retry) response [{}]: {}", status2, &text2[..text2.len().min(500)]);

            if !status2.is_success() {
                return Err(format!("Create instance failed after delete ({}): {}", status2, text2));
            }

            let resp_json: serde_json::Value = serde_json::from_str(&text2).unwrap_or_default();
            return Ok(extract_qr_from_response(&resp_json));
        }
        return Err(format!("Create instance failed ({}): {}", status, text));
    }

    info!("[whatsapp] Instance create response [{}]: {}", status, &text[..text.len().min(500)]);

    // Parse QR code from response
    let resp_json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("Parse create response: {}", e))?;

    Ok(extract_qr_from_response(&resp_json))
}

/// Extract QR code base64 from various Evolution API response formats.
fn extract_qr_from_response(resp: &serde_json::Value) -> String {
    // v1.x create: { "qrcode": { "base64": "data:image/..." } }
    // v1.x connect: { "base64": "data:image/..." }
    // Also try nested: { "qrcode": "data:image/..." }
    resp["qrcode"]["base64"].as_str()
        .or_else(|| resp["base64"].as_str())
        .or_else(|| resp["qrcode"].as_str().filter(|s| s.starts_with("data:")))
        .unwrap_or("")
        .to_string()
}

/// Delete an existing Evolution API instance.
async fn delete_evolution_instance(config: &WhatsAppConfig) {
    let client = reqwest::Client::new();
    let url = format!("{}/instance/delete/{}", config.api_url, config.instance_name);

    match client.delete(&url)
        .header("apikey", &config.api_key)
        .send().await
    {
        Ok(resp) => {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            info!("[whatsapp] Delete instance response [{}]: {}", status, &text[..text.len().min(200)]);
        }
        Err(e) => {
            warn!("[whatsapp] Delete instance failed: {}", e);
        }
    }
    // Brief pause to let the API settle
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
}

/// Connect an existing Evolution API instance (fallback if delete+create fails).
#[allow(dead_code)]
async fn connect_evolution_instance(config: &WhatsAppConfig) -> Result<String, String> {
    let client = reqwest::Client::new();
    let url = format!("{}/instance/connect/{}", config.api_url, config.instance_name);

    let resp = client.get(&url)
        .header("apikey", &config.api_key)
        .send().await
        .map_err(|e| format!("Failed to connect instance: {}", e))?;

    let text = resp.text().await.unwrap_or_default();
    info!("[whatsapp] Connect instance response: {}", &text[..text.len().min(500)]);
    let resp_json: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();

    Ok(extract_qr_from_response(&resp_json))
}

// ── Main Bridge Loop ───────────────────────────────────────────────────

/// The main bridge loop:
/// 1. Ensure Docker container is running
/// 2. Create/connect WhatsApp instance (get QR code)
/// 3. Start local webhook HTTP listener
/// 4. Route inbound messages through the agent loop
async fn run_whatsapp_bridge(app_handle: tauri::AppHandle, mut config: WhatsAppConfig) -> Result<(), String> {
    let stop = get_stop_signal();

    // Step 1: Ensure backend service is available and container is running
    let _ = app_handle.emit("whatsapp-status", json!({
        "kind": "starting",
        "message": "Setting up WhatsApp...",
    }));

    let container_id = ensure_evolution_container(&app_handle, &config).await?;
    config.container_id = Some(container_id.clone());
    let _ = channels::save_channel_config(&app_handle, CONFIG_KEY, &config);

    // Step 2: Create/connect instance and get QR code
    let _ = app_handle.emit("whatsapp-status", json!({
        "kind": "connecting",
        "message": "Creating WhatsApp instance...",
    }));

    let qr_code = create_evolution_instance(&config).await?;
    if !qr_code.is_empty() {
        config.qr_code = Some(qr_code.clone());
        let _ = channels::save_channel_config(&app_handle, CONFIG_KEY, &config);

        let _ = app_handle.emit("whatsapp-status", json!({
            "kind": "qr_code",
            "qr": &qr_code,
            "message": "Scan this QR code with WhatsApp on your phone",
        }));

        info!("[whatsapp] QR code generated — waiting for scan");
    }

    // Step 3: Start webhook listener
    let webhook_port = config.webhook_port;
    let app_for_webhook = app_handle.clone();
    let stop_for_webhook = stop.clone();

    let webhook_handle = tauri::async_runtime::spawn(async move {
        if let Err(e) = run_webhook_listener(app_for_webhook, webhook_port, stop_for_webhook).await {
            error!("[whatsapp] Webhook listener error: {}", e);
        }
    });

    // Step 4: Poll for connection status until connected or stopped
    let client = reqwest::Client::new();
    let mut check_interval = tokio::time::interval(std::time::Duration::from_secs(5));
    let mut connected = config.session_connected;

    while !stop.load(Ordering::Relaxed) {
        check_interval.tick().await;

        if !connected {
            // Check connection status
            let url = format!("{}/instance/connectionState/{}", config.api_url, config.instance_name);
            match client.get(&url).header("apikey", &config.api_key).send().await {
                Ok(resp) => {
                    if let Ok(body) = resp.json::<serde_json::Value>().await {
                        let state = body["instance"]["state"].as_str()
                            .or_else(|| body["state"].as_str())
                            .unwrap_or("");

                        if state == "open" || state == "connected" {
                            connected = true;
                            config.session_connected = true;
                            config.qr_code = None;
                            let _ = channels::save_channel_config(&app_handle, CONFIG_KEY, &config);

                            let _ = app_handle.emit("whatsapp-status", json!({
                                "kind": "connected",
                                "message": "WhatsApp connected successfully",
                            }));

                            info!("[whatsapp] Session connected — ready to receive messages");
                        }
                    }
                }
                Err(e) => {
                    warn!("[whatsapp] Connection check failed: {}", e);
                }
            }
        }
    }

    // Cleanup
    webhook_handle.abort();

    let _ = app_handle.emit("whatsapp-status", json!({
        "kind": "disconnected",
    }));

    Ok(())
}

// ── Webhook HTTP Listener ──────────────────────────────────────────────

/// Minimal HTTP listener that receives webhooks from Evolution API.
/// Runs on `webhook_port` (default 8086), bound to 127.0.0.1.
async fn run_webhook_listener(
    app_handle: tauri::AppHandle,
    port: u16,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    use tokio::net::TcpListener;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr).await
        .map_err(|e| format!("Failed to bind webhook listener on {}: {}", addr, e))?;

    info!("[whatsapp] Webhook listener started on {}", addr);

    loop {
        if stop.load(Ordering::Relaxed) { break; }

        let accept_result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            listener.accept(),
        ).await;

        let (mut stream, _peer) = match accept_result {
            Ok(Ok(conn)) => conn,
            Ok(Err(e)) => {
                warn!("[whatsapp] Accept error: {}", e);
                continue;
            }
            Err(_) => continue, // Timeout — check stop signal
        };

        // Read the full HTTP request
        let mut buf = vec![0u8; 65536];
        let n = match stream.read(&mut buf).await {
            Ok(n) => n,
            Err(_) => continue,
        };
        let request = String::from_utf8_lossy(&buf[..n]).to_string();

        // Send 200 OK immediately (Evolution expects quick response)
        let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK";
        let _ = stream.write_all(response.as_bytes()).await;
        drop(stream);

        // Parse the JSON body (after the blank line)
        let body = if let Some(idx) = request.find("\r\n\r\n") {
            &request[idx + 4..]
        } else {
            continue;
        };

        let payload: serde_json::Value = match serde_json::from_str(body) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Handle different webhook events
        let event = payload["event"].as_str().unwrap_or("");

        match event {
            "qrcode.updated" => {
                let qr = payload["data"]["qrcode"]["base64"].as_str()
                    .or_else(|| payload["data"]["qrcode"].as_str())
                    .unwrap_or("");
                if !qr.is_empty() {
                    let _ = app_handle.emit("whatsapp-status", json!({
                        "kind": "qr_code",
                        "qr": qr,
                        "message": "Scan this QR code with WhatsApp",
                    }));
                }
            }
            "connection.update" => {
                let state = payload["data"]["state"].as_str().unwrap_or("");
                if state == "open" || state == "connected" {
                    let _ = app_handle.emit("whatsapp-status", json!({
                        "kind": "connected",
                        "message": "WhatsApp connected",
                    }));
                    info!("[whatsapp] Connection confirmed via webhook");
                }
            }
            "messages.upsert" => {
                // Process inbound message
                let app = app_handle.clone();
                let msg_payload = payload.clone();
                tauri::async_runtime::spawn(async move {
                    handle_inbound_message(app, msg_payload).await;
                });
            }
            _ => {
                // Ignore other events
            }
        }
    }

    Ok(())
}

// ── Message Handling ───────────────────────────────────────────────────

/// Process an inbound WhatsApp message from the Evolution API webhook.
async fn handle_inbound_message(app_handle: tauri::AppHandle, payload: serde_json::Value) {
    let data = &payload["data"];

    // Extract message details
    let messages = match data.as_array() {
        Some(arr) => arr.clone(),
        None => vec![data.clone()],
    };

    for msg in messages {
        // Skip status messages, reactions, etc.
        let key = &msg["key"];
        let from_me = key["fromMe"].as_bool().unwrap_or(false);
        if from_me { continue; }

        // Extract text content
        let text = msg["message"]["conversation"].as_str()
            .or_else(|| msg["message"]["extendedTextMessage"]["text"].as_str())
            .unwrap_or("");
        if text.is_empty() { continue; }

        // Extract sender info
        let remote_jid = key["remoteJid"].as_str().unwrap_or("");
        let participant = key["participant"].as_str().unwrap_or(remote_jid);
        let is_group = remote_jid.contains("@g.us");

        // Normalize sender ID (strip @s.whatsapp.net)
        let sender_id = participant.split('@').next().unwrap_or(participant).to_string();
        let push_name = msg["pushName"].as_str().unwrap_or(&sender_id).to_string();

        info!("[whatsapp] Message from {} ({}): {}", push_name, sender_id,
            if text.len() > 50 { format!("{}...", &text[..50]) } else { text.to_string() });

        // Load config for access control
        let mut config: WhatsAppConfig = match channels::load_channel_config(&app_handle, CONFIG_KEY) {
            Ok(c) => c,
            Err(e) => { error!("[whatsapp] Load config: {}", e); continue; }
        };

        // Skip groups unless configured
        if is_group && !config.respond_in_groups {
            continue;
        }

        // Access control
        match channels::check_access(
            &config.dm_policy,
            &sender_id,
            &push_name,
            &push_name,
            &config.allowed_users,
            &mut config.pending_users,
        ) {
            Err(denial_msg) => {
                let _ = channels::save_channel_config(&app_handle, CONFIG_KEY, &config);
                let _ = app_handle.emit("whatsapp-status", json!({
                    "kind": "pairing_request",
                    "user_id": &sender_id,
                    "user_name": &push_name,
                }));
                // Send denial message back
                let _ = send_whatsapp_message(&app_handle, remote_jid, &denial_msg).await;
                continue;
            }
            Ok(()) => {}
        }

        MESSAGE_COUNT.fetch_add(1, Ordering::Relaxed);

        // Route to agent
        let agent_id = config.agent_id.as_deref().unwrap_or("default");
        let ctx = "You are chatting via WhatsApp. Keep responses concise and mobile-friendly. \
                   Use WhatsApp formatting: *bold*, _italic_, ~strikethrough~, ```code```. \
                   Avoid very long responses — WhatsApp truncates long messages.";

        let response = channels::run_channel_agent(
            &app_handle, "whatsapp", ctx, text, &sender_id, agent_id,
        ).await;

        match response {
            Ok(reply) if !reply.is_empty() => {
                if let Err(e) = send_whatsapp_message(&app_handle, remote_jid, &reply).await {
                    error!("[whatsapp] Send failed: {}", e);
                }
            }
            Err(e) => {
                error!("[whatsapp] Agent error for {}: {}", sender_id, e);
                let _ = send_whatsapp_message(&app_handle, remote_jid,
                    &format!("Error: {}", e)).await;
            }
            _ => {}
        }
    }
}

// ── Evolution API Helpers ──────────────────────────────────────────────

/// Send a text message via Evolution API.
async fn send_whatsapp_message(
    app_handle: &tauri::AppHandle,
    to_jid: &str,
    text: &str,
) -> Result<(), String> {
    let config: WhatsAppConfig = channels::load_channel_config(app_handle, CONFIG_KEY)?;
    let client = reqwest::Client::new();

    let url = format!("{}/message/sendText/{}", config.api_url, config.instance_name);

    // WhatsApp has no hard character limit, but split very long messages
    let chunks = split_message(text, 4000);

    for chunk in &chunks {
        let body = json!({
            "number": to_jid,
            "text": chunk,
        });

        let resp = client.post(&url)
            .header("apikey", &config.api_key)
            .json(&body)
            .send().await;

        match resp {
            Ok(r) => {
                if !r.status().is_success() {
                    let err_text = r.text().await.unwrap_or_default();
                    warn!("[whatsapp] sendText error: {}", err_text);
                }
            }
            Err(e) => warn!("[whatsapp] sendText failed: {}", e),
        }
    }

    Ok(())
}

fn split_message(text: &str, max_len: usize) -> Vec<String> {
    if text.len() <= max_len {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut remaining = text;
    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining.to_string());
            break;
        }
        let split_at = remaining[..max_len]
            .rfind('\n')
            .or_else(|| remaining[..max_len].rfind(' '))
            .unwrap_or(max_len);
        chunks.push(remaining[..split_at].to_string());
        remaining = remaining[split_at..].trim_start();
    }
    chunks
}

// ── Config Persistence ─────────────────────────────────────────────────

pub fn load_config(app_handle: &tauri::AppHandle) -> Result<WhatsAppConfig, String> {
    channels::load_channel_config(app_handle, CONFIG_KEY)
}

pub fn save_config(app_handle: &tauri::AppHandle, config: &WhatsAppConfig) -> Result<(), String> {
    channels::save_channel_config(app_handle, CONFIG_KEY, config)
}

pub fn approve_user(app_handle: &tauri::AppHandle, user_id: &str) -> Result<(), String> {
    channels::approve_user_generic(app_handle, CONFIG_KEY, user_id)
}

pub fn deny_user(app_handle: &tauri::AppHandle, user_id: &str) -> Result<(), String> {
    channels::deny_user_generic(app_handle, CONFIG_KEY, user_id)
}

pub fn remove_user(app_handle: &tauri::AppHandle, user_id: &str) -> Result<(), String> {
    channels::remove_user_generic(app_handle, CONFIG_KEY, user_id)
}
