use std::process::Command;
use std::path::PathBuf;
use std::fs;
use std::net::TcpStream;
use log::{info, warn, error};
use tauri::{Emitter, Manager};

fn get_app_data_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().unwrap_or_default();
        home.join("Library/Application Support/Claw")
    }
    #[cfg(target_os = "windows")]
    {
        dirs::data_local_dir().unwrap_or_default().join("Claw")
    }
    #[cfg(target_os = "linux")]
    {
        dirs::data_dir().unwrap_or_default().join("claw")
    }
}

fn get_bundled_node_path() -> Option<PathBuf> {
    let app_dir = get_app_data_dir();
    let node_bin = if cfg!(target_os = "windows") {
        app_dir.join("node/node.exe")
    } else {
        app_dir.join("node/bin/node")
    };
    if node_bin.exists() {
        Some(node_bin)
    } else {
        None
    }
}

fn get_npm_path() -> PathBuf {
    let app_dir = get_app_data_dir();
    if cfg!(target_os = "windows") {
        app_dir.join("node/npm.cmd")
    } else {
        app_dir.join("node/bin/npm")
    }
}

fn get_openclaw_path() -> PathBuf {
    let app_dir = get_app_data_dir();
    if cfg!(target_os = "windows") {
        app_dir.join("node_modules/.bin/openclaw.cmd")
    } else {
        app_dir.join("node_modules/.bin/openclaw")
    }
}

fn get_node_bin_dir() -> PathBuf {
    let app_dir = get_app_data_dir();
    if cfg!(target_os = "windows") {
        app_dir.join("node")
    } else {
        app_dir.join("node/bin")
    }
}

fn join_path_env(new_dir: &std::path::Path) -> String {
    let sep = if cfg!(target_os = "windows") { ";" } else { ":" };
    format!("{}{}{}", new_dir.display(), sep, std::env::var("PATH").unwrap_or_default())
}

#[tauri::command]
fn check_node_installed() -> bool {
    // Check bundled node first
    if get_bundled_node_path().is_some() {
        return true;
    }
    // Fall back to system node
    Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[tauri::command]
fn check_openclaw_installed() -> bool {
    let home = dirs::home_dir().unwrap_or_default();
    let openclaw_config = home.join(".openclaw/openclaw.json");
    openclaw_config.exists()
}

/// Strip JSON5 features (comments, trailing commas) so serde_json can parse.
/// OpenClaw config files are JSON5 — they may contain // comments, /* blocks */,
/// trailing commas, and unquoted keys (we don't handle unquoted keys here).
fn sanitize_json5(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut in_string = false;

    while i < len {
        if in_string {
            out.push(chars[i]);
            if chars[i] == '\\' && i + 1 < len {
                i += 1;
                out.push(chars[i]);
            } else if chars[i] == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }

        // Line comment
        if i + 1 < len && chars[i] == '/' && chars[i + 1] == '/' {
            // Skip until newline
            while i < len && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }

        // Block comment
        if i + 1 < len && chars[i] == '/' && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            if i + 1 < len {
                i += 2; // skip */
            }
            continue;
        }

        // Trailing commas: comma followed by } or ]
        if chars[i] == ',' {
            // Peek ahead past whitespace for } or ]
            let mut j = i + 1;
            while j < len && chars[j].is_whitespace() {
                j += 1;
            }
            if j < len && (chars[j] == '}' || chars[j] == ']') {
                i += 1; // skip the trailing comma
                continue;
            }
        }

        if chars[i] == '"' {
            in_string = true;
        }

        out.push(chars[i]);
        i += 1;
    }

    out
}

/// Parse the OpenClaw config file, handling JSON5 features.
fn parse_openclaw_config() -> Option<serde_json::Value> {
    let home = dirs::home_dir()?;
    let config_path = home.join(".openclaw/openclaw.json");
    let content = std::fs::read_to_string(&config_path).map_err(|e| {
        info!("Cannot read {}: {}", config_path.display(), e);
    }).ok()?;

    // Try standard JSON first (fast path)
    if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
        info!("Parsed openclaw.json as standard JSON");
        return Some(config);
    }

    // Fall back to JSON5-sanitized parsing
    let sanitized = sanitize_json5(&content);
    match serde_json::from_str::<serde_json::Value>(&sanitized) {
        Ok(config) => {
            info!("Parsed openclaw.json after JSON5 sanitization");
            Some(config)
        }
        Err(e) => {
            error!("Failed to parse openclaw.json even after sanitization: {}", e);
            info!("First 200 chars: {}", &content.chars().take(200).collect::<String>());
            None
        }
    }
}

/// Read the gateway port from ~/.openclaw/openclaw.json.
/// Checks gateway.port, then falls back to 18789 (OpenClaw default).
fn get_gateway_port() -> u16 {
    let port = (|| -> Option<u16> {
        let config = parse_openclaw_config()?;
        // Try gateway.port first, then top-level port
        let p = config["gateway"]["port"].as_u64()
            .or_else(|| config["port"].as_u64())
            .map(|p| p as u16);
        info!("Config port: {:?}", p);
        p
    })();
    let result = port.unwrap_or(18789);
    info!("Resolved gateway port: {}", result);
    result
}

#[tauri::command]
fn get_gateway_token() -> Option<String> {
    let config = parse_openclaw_config()?;

    // Primary: gateway.auth.token (the correct path per OpenClaw docs)
    if let Some(t) = config["gateway"]["auth"]["token"].as_str() {
        let result = t.to_string();
        if !result.trim().is_empty() {
            let masked = if result.len() > 8 {
                format!("{}...{}", &result[..4], &result[result.len()-4..])
            } else {
                "****".to_string()
            };
            info!("Token from config gateway.auth.token: {} ({} chars)", masked, result.len());
            return Some(result);
        }
    }

    // Fallback: OPENCLAW_GATEWAY_TOKEN env var
    info!("gateway.auth.token not found in config, checking env");
    if let Ok(env_token) = std::env::var("OPENCLAW_GATEWAY_TOKEN") {
        let trimmed = env_token.trim().to_string();
        if !trimmed.is_empty() {
            info!("Token from OPENCLAW_GATEWAY_TOKEN env ({} chars)", trimmed.len());
            return Some(trimmed);
        }
    }

    warn!("No gateway token found in config or environment");
    // Log what keys ARE present for debugging
    if let Some(config) = parse_openclaw_config() {
        let gw = &config["gateway"];
        if gw.is_object() {
            let keys: Vec<&str> = gw.as_object().map(|m| m.keys().map(|k| k.as_str()).collect()).unwrap_or_default();
            info!("gateway config keys: {:?}", keys);
            let auth = &gw["auth"];
            if auth.is_object() {
                let auth_keys: Vec<&str> = auth.as_object().map(|m| m.keys().map(|k| k.as_str()).collect()).unwrap_or_default();
                info!("gateway.auth keys: {:?}", auth_keys);
            } else {
                info!("gateway.auth is not an object (type: {})", if auth.is_null() { "null/missing" } else { "other" });
            }
        } else {
            info!("gateway key is not an object or is missing");
        }
    }
    None
}

#[tauri::command]
fn get_gateway_port_setting() -> u16 {
    get_gateway_port()
}

#[tauri::command]
async fn install_openclaw(window: tauri::Window, app_handle: tauri::AppHandle) -> Result<(), String> {
    info!("Starting OpenClaw installation...");
    let app_dir = get_app_data_dir();
    fs::create_dir_all(&app_dir).map_err(|e| format!("Failed to create app dir: {}", e))?;

    // Step 1: Extract bundled Node.js if not already done
    window.emit("install-progress", serde_json::json!({
        "stage": "extracting",
        "percent": 5,
        "message": "Setting up runtime..."
    })).ok();

    let node_dir = app_dir.join("node");
    if !node_dir.exists() {
        // Determine architecture and platform
        let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
        let os_name = if cfg!(target_os = "macos") {
            "darwin"
        } else if cfg!(target_os = "windows") {
            "win"
        } else {
            "linux"
        };
        let resource_name = format!("node/node-{}-{}.tar.gz", os_name, arch);
        
        // Get the resource path from the app bundle
        let resource_path = app_handle.path()
            .resource_dir()
            .map_err(|e| format!("Failed to get resource dir: {}", e))?
            .join(&resource_name);
        
        if !resource_path.exists() {
            return Err(format!("Node.js bundle not found at {:?}", resource_path));
        }

        window.emit("install-progress", serde_json::json!({
            "stage": "extracting",
            "percent": 15,
            "message": "Extracting Node.js runtime..."
        })).ok();

        // Create node directory and extract
        fs::create_dir_all(&node_dir).map_err(|e| format!("Failed to create node dir: {}", e))?;
        
        let extract_result = Command::new("tar")
            .args(["-xzf", resource_path.to_str().unwrap(), "-C", node_dir.to_str().unwrap(), "--strip-components=1"])
            .output()
            .map_err(|e| format!("Failed to extract Node.js: {}", e))?;

        if !extract_result.status.success() {
            let stderr = String::from_utf8_lossy(&extract_result.stderr);
            return Err(format!("Failed to extract Node.js: {}", stderr));
        }
    }

    window.emit("install-progress", serde_json::json!({
        "stage": "downloading",
        "percent": 30,
        "message": "Installing OpenClaw..."
    })).ok();

    // Step 2: Install OpenClaw using bundled npm
    let npm_path = get_npm_path();
    let _node_modules_dir = app_dir.join("node_modules");
    
    let install_result = Command::new(npm_path.to_str().unwrap())
        .args(["install", "openclaw", "--prefix", app_dir.to_str().unwrap()])
        .env("PATH", join_path_env(&node_dir.join(if cfg!(target_os = "windows") { "" } else { "bin" })))
        .output()
        .map_err(|e| format!("Failed to run npm: {}", e))?;

    if !install_result.status.success() {
        let stderr = String::from_utf8_lossy(&install_result.stderr);
        return Err(format!("npm install failed: {}", stderr));
    }

    window.emit("install-progress", serde_json::json!({
        "stage": "configuring",
        "percent": 60,
        "message": "Configuring..."
    })).ok();

    // Step 3: Create default config if it doesn't exist
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    let openclaw_dir = home.join(".openclaw");
    let config_path = openclaw_dir.join("openclaw.json");

    if !config_path.exists() {
        fs::create_dir_all(&openclaw_dir).map_err(|e| format!("Failed to create .openclaw dir: {}", e))?;
        
        // Generate random token
        let token: String = (0..48)
            .map(|_| {
                let idx = rand::random::<usize>() % 36;
                if idx < 10 { (b'0' + idx as u8) as char } else { (b'a' + (idx - 10) as u8) as char }
            })
            .collect();

        let config = serde_json::json!({
            "meta": {
                "lastTouchedVersion": "2026.2.0",
                "lastTouchedAt": chrono::Utc::now().to_rfc3339()
            },
            "gateway": {
                "mode": "local",
                "auth": {
                    "mode": "token",
                    "token": token
                }
            },
            "agents": {
                "defaults": {
                    "maxConcurrent": 4
                }
            }
        });

        fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap())
            .map_err(|e| format!("Failed to write config: {}", e))?;

        // Create workspace directory
        fs::create_dir_all(openclaw_dir.join("workspace")).ok();
    }

    window.emit("install-progress", serde_json::json!({
        "stage": "starting",
        "percent": 80,
        "message": "Starting gateway..."
    })).ok();

    // Step 4: Start the gateway (skip if already running)
    let gw_port = get_gateway_port();
    info!("Checking if gateway already running on port {}", gw_port);
    if !is_gateway_running(gw_port) {
        let openclaw_bin = get_openclaw_path();
        let node_bin_dir = get_node_bin_dir();
        
        Command::new(openclaw_bin.to_str().unwrap())
            .args(["gateway", "start"])
            .env("PATH", join_path_env(&node_bin_dir))
            .spawn()
            .map_err(|e| format!("Failed to start gateway: {}", e))?;

        // Give it time to start
        std::thread::sleep(std::time::Duration::from_secs(3));
    }

    window.emit("install-progress", serde_json::json!({
        "stage": "done",
        "percent": 100,
        "message": "Installation complete!"
    })).ok();

    Ok(())
}

fn is_gateway_running(port: u16) -> bool {
    TcpStream::connect(format!("127.0.0.1:{}", port)).is_ok()
}

#[tauri::command]
fn check_gateway_health(port: Option<u16>) -> bool {
    let p = port.unwrap_or_else(get_gateway_port);
    let running = is_gateway_running(p);
    info!("Gateway health check on port {}: {}", p, if running { "running" } else { "not running" });
    running
}

#[tauri::command]
fn start_gateway(port: Option<u16>) -> Result<(), String> {
    let p = port.unwrap_or_else(get_gateway_port);
    // Don't start if already running
    if is_gateway_running(p) {
        info!("Gateway already running on port {}, skipping start", p);
    } else {
        info!("Starting gateway (expected port {})...", p);

        let openclaw_bin = get_openclaw_path();
        let node_dir = get_node_bin_dir();
        
        // Try bundled openclaw first
        if openclaw_bin.exists() {
            Command::new(openclaw_bin.to_str().unwrap())
                .args(["gateway", "start"])
                .env("PATH", join_path_env(&node_dir))
                .spawn()
                .map_err(|e| format!("Failed to start gateway: {}", e))?;
        } else {
            // Fall back to system openclaw
            Command::new("openclaw")
                .args(["gateway", "start"])
                .spawn()
                .map_err(|e| format!("Failed to start gateway: {}", e))?;
        }
    }

    // Memory Palace is an MCP skill — the gateway spawns it on demand.
    // No separate server to auto-start.

    Ok(())
}

#[tauri::command]
fn stop_gateway() -> Result<(), String> {
    info!("Stopping gateway...");
    #[cfg(unix)]
    {
        let _ = Command::new("pkill")
            .args(["-f", "openclaw-gateway"])
            .output();
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/F", "/IM", "openclaw-gateway.exe"])
            .output();
    }
    Ok(())
}

// ═══ Memory Palace Lifecycle ══════════════════════════════════════════════
// Memory Palace is an MCP server (stdio transport), NOT an HTTP server.
// Install: git clone → pip install -e . → python -m setup.first_run
// Run: registered as an MCP skill in the OpenClaw gateway config.
// Requires: Python 3.10+, Ollama (for local embeddings).

const PALACE_GIT_URL: &str = "https://github.com/jeffpierce/memory-palace.git";

fn get_palace_dir() -> PathBuf {
    get_app_data_dir().join("memory-palace")
}

fn get_palace_repo() -> PathBuf {
    get_palace_dir().join("repo")
}

fn get_palace_venv() -> PathBuf {
    get_palace_dir().join("venv")
}

fn get_palace_python() -> PathBuf {
    let venv = get_palace_venv();
    if cfg!(target_os = "windows") {
        venv.join("Scripts/python.exe")
    } else {
        venv.join("bin/python3")
    }
}

fn get_palace_pip() -> PathBuf {
    let venv = get_palace_venv();
    if cfg!(target_os = "windows") {
        venv.join("Scripts/pip.exe")
    } else {
        venv.join("bin/pip3")
    }
}

/// Path to the memory-palace-mcp entrypoint in the venv
fn get_palace_mcp_bin() -> PathBuf {
    let venv = get_palace_venv();
    if cfg!(target_os = "windows") {
        venv.join("Scripts/memory-palace-mcp.exe")
    } else {
        venv.join("bin/memory-palace-mcp")
    }
}

fn find_system_python() -> Option<String> {
    for name in &["python3", "python"] {
        if let Ok(output) = Command::new(name).arg("--version").output() {
            if output.status.success() {
                let ver = String::from_utf8_lossy(&output.stdout).to_string();
                let ver_alt = String::from_utf8_lossy(&output.stderr).to_string();
                let version_str = if ver.contains("3.") { ver } else { ver_alt };
                if version_str.contains("3.") {
                    info!("Found Python: {} → {}", name, version_str.trim());
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

fn check_ollama_available() -> bool {
    Command::new("ollama")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[tauri::command]
fn check_python_installed() -> bool {
    find_system_python().is_some()
}

#[tauri::command]
fn check_ollama_installed() -> bool {
    check_ollama_available()
}

#[tauri::command]
fn check_palace_installed() -> bool {
    // Palace is installed if: venv python exists AND memory_palace module imports
    let python = get_palace_python();
    if !python.exists() {
        return false;
    }
    let output = Command::new(python.to_str().unwrap())
        .args(["-c", "import memory_palace; print('ok')"])
        .output();
    match output {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}

#[tauri::command]
fn check_palace_health() -> bool {
    // Palace is "healthy" if it's installed and registered as a gateway skill.
    // Since it's an MCP stdio server, there's no port to check.
    // We check: (1) mcp entrypoint exists, (2) it's in the gateway config.
    let mcp_bin = get_palace_mcp_bin();
    if !mcp_bin.exists() {
        return false;
    }
    // Check if registered in gateway config
    if let Some(config) = parse_openclaw_config() {
        if let Some(skills) = config["skills"].as_object() {
            return skills.contains_key("memory-palace");
        }
    }
    false
}

#[tauri::command]
async fn install_palace(window: tauri::Window) -> Result<(), String> {
    info!("Starting Memory Palace installation...");

    // ── Step 1: Check Python ──────────────────────────────────────────────
    window.emit("palace-install-progress", serde_json::json!({
        "stage": "checking",
        "percent": 5,
        "message": "Checking requirements..."
    })).ok();

    let python_cmd = find_system_python()
        .ok_or("Python 3 not found. Please install Python 3.10+ from python.org")?;
    info!("Using Python: {}", python_cmd);

    // ── Step 2: Check Ollama ──────────────────────────────────────────────
    if !check_ollama_available() {
        return Err("Ollama not found. Please install Ollama from https://ollama.ai — it provides local embedding models for Memory Palace.".to_string());
    }
    info!("Ollama is available");

    // ── Step 3: Create venv ───────────────────────────────────────────────
    window.emit("palace-install-progress", serde_json::json!({
        "stage": "venv",
        "percent": 10,
        "message": "Creating Python virtual environment..."
    })).ok();

    let palace_dir = get_palace_dir();
    let venv_dir = get_palace_venv();
    let repo_dir = get_palace_repo();
    fs::create_dir_all(&palace_dir).map_err(|e| format!("Failed to create palace dir: {}", e))?;

    if !venv_dir.exists() {
        let venv_result = Command::new(&python_cmd)
            .args(["-m", "venv", venv_dir.to_str().unwrap()])
            .output()
            .map_err(|e| format!("Failed to create venv: {}", e))?;

        if !venv_result.status.success() {
            let stderr = String::from_utf8_lossy(&venv_result.stderr);
            return Err(format!("venv creation failed: {}", stderr));
        }
    }

    // ── Step 4: Git clone ─────────────────────────────────────────────────
    window.emit("palace-install-progress", serde_json::json!({
        "stage": "clone",
        "percent": 20,
        "message": "Cloning memory-palace repository..."
    })).ok();

    if repo_dir.exists() {
        // Pull latest
        info!("memory-palace repo exists, pulling latest...");
        let pull = Command::new("git")
            .args(["pull", "--ff-only"])
            .current_dir(&repo_dir)
            .output();
        if let Ok(o) = pull {
            if o.status.success() {
                info!("git pull succeeded");
            } else {
                warn!("git pull failed, continuing with existing code");
            }
        }
    } else {
        let clone_result = Command::new("git")
            .args(["clone", "--depth", "1", PALACE_GIT_URL, repo_dir.to_str().unwrap()])
            .output()
            .map_err(|e| format!("git clone failed: {}", e))?;

        if !clone_result.status.success() {
            let stderr = String::from_utf8_lossy(&clone_result.stderr);
            return Err(format!("git clone failed: {}", stderr));
        }
    }

    // ── Step 5: pip install -e . ──────────────────────────────────────────
    window.emit("palace-install-progress", serde_json::json!({
        "stage": "pip",
        "percent": 35,
        "message": "Installing memory-palace and dependencies..."
    })).ok();

    let pip = get_palace_pip();
    let pip_result = Command::new(pip.to_str().unwrap())
        .args(["install", "-e", repo_dir.to_str().unwrap()])
        .output()
        .map_err(|e| format!("pip install failed: {}", e))?;

    if !pip_result.status.success() {
        let stderr = String::from_utf8_lossy(&pip_result.stderr);
        return Err(format!("pip install -e . failed: {}", stderr));
    }
    info!("pip install -e . succeeded");

    // ── Step 6: Verify module imports ─────────────────────────────────────
    window.emit("palace-install-progress", serde_json::json!({
        "stage": "verify",
        "percent": 50,
        "message": "Verifying installation..."
    })).ok();

    let venv_python = get_palace_python();
    let verify = Command::new(venv_python.to_str().unwrap())
        .args(["-c", "import memory_palace; print(getattr(memory_palace, '__version__', 'ok'))"])
        .output()
        .map_err(|e| format!("Failed to verify: {}", e))?;

    if !verify.status.success() {
        let stderr = String::from_utf8_lossy(&verify.stderr);
        return Err(format!("memory-palace module cannot be imported: {}", stderr));
    }
    let ver = String::from_utf8_lossy(&verify.stdout).trim().to_string();
    info!("memory-palace verified: {}", ver);

    // Verify MCP entrypoint exists
    let mcp_bin = get_palace_mcp_bin();
    if !mcp_bin.exists() {
        // Try to find it
        let find = Command::new("find")
            .args([venv_dir.to_str().unwrap(), "-name", "memory-palace-mcp", "-o", "-name", "memory_palace_mcp"])
            .output();
        let found = find.map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
        warn!("MCP entrypoint not at expected path {:?}. Found: {}", mcp_bin, found.trim());
        // Not fatal — the gateway might use python -m mcp_server.server instead
    }

    // ── Step 7: Run setup wizard (non-interactive) ────────────────────────
    window.emit("palace-install-progress", serde_json::json!({
        "stage": "setup",
        "percent": 60,
        "message": "Running first-time setup (downloading models)..."
    })).ok();

    // Create default config at ~/.memory-palace/config.json
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    let mp_config_dir = home.join(".memory-palace");
    let mp_config_path = mp_config_dir.join("config.json");
    fs::create_dir_all(&mp_config_dir).ok();

    if !mp_config_path.exists() {
        let mp_config = serde_json::json!({
            "database": {
                "type": "sqlite",
                "url": null
            },
            "ollama_url": "http://localhost:11434",
            "embedding_model": null,
            "llm_model": null,
            "synthesis": {
                "enabled": true
            },
            "auto_link": {
                "enabled": true,
                "link_threshold": 0.75,
                "suggest_threshold": 0.675
            },
            "toon_output": true,
            "instances": ["paw"],
            "notify_command": null,
            "instance_routes": {}
        });
        fs::write(&mp_config_path, serde_json::to_string_pretty(&mp_config).unwrap())
            .map_err(|e| format!("Failed to write config: {}", e))?;
        info!("Wrote default Memory Palace config to {:?}", mp_config_path);
    }

    // Try to run setup.first_run — it auto-detects and pulls Ollama models.
    // This script may be interactive so we run it with a timeout and skip if it hangs.
    let setup_result = Command::new(venv_python.to_str().unwrap())
        .args(["-m", "setup.first_run"])
        .current_dir(&repo_dir)
        .env("MEMORY_PALACE_DATA_DIR", mp_config_dir.to_str().unwrap())
        .output();

    match setup_result {
        Ok(ref o) if o.status.success() => {
            info!("setup.first_run completed successfully");
            let stdout = String::from_utf8_lossy(&o.stdout);
            info!("Setup output: {}", stdout.chars().take(500).collect::<String>());
        }
        Ok(ref o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            warn!("setup.first_run exited non-zero: {}", stderr.chars().take(500).collect::<String>());
            // Not fatal — Ollama models can be pulled on first use
        }
        Err(e) => {
            warn!("setup.first_run failed to run: {}", e);
            // Not fatal
        }
    }

    // Pull a lightweight embedding model via Ollama explicitly (in case setup didn't)
    window.emit("palace-install-progress", serde_json::json!({
        "stage": "models",
        "percent": 70,
        "message": "Ensuring embedding model is available..."
    })).ok();

    let model_check = Command::new("ollama")
        .args(["list"])
        .output();
    let has_embed_model = model_check
        .map(|o| {
            let out = String::from_utf8_lossy(&o.stdout).to_lowercase();
            out.contains("nomic-embed-text")
        })
        .unwrap_or(false);

    if !has_embed_model {
        info!("Pulling nomic-embed-text via Ollama...");
        window.emit("palace-install-progress", serde_json::json!({
            "stage": "models",
            "percent": 72,
            "message": "Pulling embedding model (nomic-embed-text)... this may take a minute"
        })).ok();

        let pull = Command::new("ollama")
            .args(["pull", "nomic-embed-text"])
            .output();
        match pull {
            Ok(ref o) if o.status.success() => info!("nomic-embed-text pulled successfully"),
            Ok(ref o) => warn!("ollama pull nomic-embed-text failed: {}", String::from_utf8_lossy(&o.stderr)),
            Err(e) => warn!("Failed to run ollama pull: {}", e),
        }
    }

    // ── Step 8: Register as MCP skill in OpenClaw gateway ─────────────────
    window.emit("palace-install-progress", serde_json::json!({
        "stage": "register",
        "percent": 85,
        "message": "Registering Memory Palace as gateway skill..."
    })).ok();

    register_palace_skill()?;

    // ── Done ──────────────────────────────────────────────────────────────
    window.emit("palace-install-progress", serde_json::json!({
        "stage": "done",
        "percent": 100,
        "message": "Memory Palace installed and registered!"
    })).ok();
    info!("Memory Palace installation complete");

    Ok(())
}

/// Register Memory Palace as an MCP skill in the OpenClaw gateway config.
/// Adds/updates the "memory-palace" entry under the "skills" key in openclaw.json.
fn register_palace_skill() -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    let config_path = home.join(".openclaw/openclaw.json");

    if !config_path.exists() {
        return Err("OpenClaw config not found. Install OpenClaw first.".to_string());
    }

    // Read current config
    let content = fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read config: {}", e))?;

    let sanitized = sanitize_json5(&content);
    let mut config: serde_json::Value = serde_json::from_str(&sanitized)
        .map_err(|e| format!("Failed to parse config: {}", e))?;

    // Build the MCP command for the skill
    let mcp_bin = get_palace_mcp_bin();
    let venv_python = get_palace_python();

    // Prefer the direct entrypoint, fall back to python -m
    let (command, args) = if mcp_bin.exists() {
        (mcp_bin.to_str().unwrap().to_string(), Vec::<String>::new())
    } else {
        (venv_python.to_str().unwrap().to_string(), vec!["-m".to_string(), "mcp_server.server".to_string()])
    };

    // Create skill entry
    let skill_config = serde_json::json!({
        "command": command,
        "args": args,
        "transport": "stdio",
        "enabled": true
    });

    // Ensure "skills" object exists
    if !config["skills"].is_object() {
        config["skills"] = serde_json::json!({});
    }
    config["skills"]["memory-palace"] = skill_config;

    // Write back
    fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap())
        .map_err(|e| format!("Failed to write config: {}", e))?;

    info!("Registered memory-palace as MCP skill in {:?}", config_path);
    Ok(())
}

fn start_palace_server() -> Result<(), String> {
    // Memory Palace is an MCP stdio server — there's no persistent server to start.
    // It's spawned on-demand by the gateway when a tool call is made.
    // This function is a no-op but kept for API compatibility.
    info!("Memory Palace is an MCP stdio server — no persistent server to start.");
    info!("The gateway spawns it on-demand when skills are invoked.");
    Ok(())
}

#[tauri::command]
fn start_palace() -> Result<(), String> {
    start_palace_server()
}

#[tauri::command]
fn stop_palace() -> Result<(), String> {
    // MCP stdio servers are ephemeral — nothing to stop
    info!("Memory Palace uses stdio transport — no persistent process to stop.");
    Ok(())
}

#[tauri::command]
fn get_palace_port() -> u16 {
    // No port — MCP stdio transport. Return 0 to indicate not applicable.
    0
}

#[tauri::command]
fn get_palace_log() -> String {
    let log_path = get_palace_dir().join("palace.log");
    if log_path.exists() {
        fs::read_to_string(&log_path)
            .unwrap_or_else(|e| format!("(failed to read log: {})", e))
            .lines()
            .rev()
            .take(50)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        "(no palace.log found — Memory Palace uses MCP stdio transport)".to_string()
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new()
            .target(tauri_plugin_log::Target::new(
                tauri_plugin_log::TargetKind::LogDir { file_name: Some("claw".into()) },
            ))
            .max_file_size(5_000_000) // 5MB max per log file
            .build())
        .plugin(tauri_plugin_sql::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            check_node_installed,
            check_openclaw_installed,
            check_gateway_health,
            get_gateway_token,
            get_gateway_port_setting,
            install_openclaw,
            start_gateway,
            stop_gateway,
            check_python_installed,
            check_ollama_installed,
            check_palace_installed,
            check_palace_health,
            install_palace,
            start_palace,
            stop_palace,
            get_palace_port,
            get_palace_log
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
