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
        return Ok(());
    }

    info!("Starting gateway (expected port {})...", p);

    let openclaw_bin = get_openclaw_path();
    let node_dir = get_node_bin_dir();

    // Check if we need to inject OPENAI_BASE_URL for Azure/Foundry
    let embedding_base_url = read_paw_settings()
        .and_then(|s| s.get("embeddingBaseUrl").and_then(|v| v.as_str()).map(|s| s.to_string()));

    // On macOS, set the env var for the launchd user session so the
    // LaunchAgent picks it up when gateway is started/restarted by launchd.
    #[cfg(target_os = "macos")]
    if let Some(ref url) = embedding_base_url {
        info!("Setting OPENAI_BASE_URL={} via launchctl setenv", url);
        let _ = Command::new("launchctl")
            .args(["setenv", "OPENAI_BASE_URL", url])
            .output();
    }

    let bin_str: String;
    let path_env: String;

    if openclaw_bin.exists() {
        bin_str = openclaw_bin.to_str().unwrap().to_string();
        path_env = join_path_env(&node_dir);
    } else {
        bin_str = "openclaw".to_string();
        path_env = std::env::var("PATH").unwrap_or_default();
    }

    // Ensure the LaunchAgent plist is installed. After `openclaw gateway stop`
    // unloads the service, `openclaw gateway start` fails with "service not
    // loaded". Running install first re-registers the plist with launchd.
    // Pass OPENAI_BASE_URL so it's embedded in the plist EnvironmentVariables.
    info!("Ensuring gateway LaunchAgent is installed...");
    let mut install_cmd = Command::new(&bin_str);
    install_cmd.args(["gateway", "install"])
        .env("PATH", &path_env);
    if let Some(ref url) = embedding_base_url {
        install_cmd.env("OPENAI_BASE_URL", url);
    }
    let install_result = install_cmd.output();
    match &install_result {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            info!("gateway install exit={} stdout={} stderr={}", out.status, stdout.trim(), stderr.trim());
        }
        Err(e) => info!("gateway install failed to run: {}", e),
    }

    // Now start the service (launchctl kickstart)
    let mut cmd = Command::new(&bin_str);
    cmd.args(["gateway", "start"])
        .env("PATH", &path_env);
    if let Some(ref url) = embedding_base_url {
        cmd.env("OPENAI_BASE_URL", url);
    }
    cmd.spawn()
        .map_err(|e| format!("Failed to start gateway: {}", e))?;

    Ok(())
}

#[tauri::command]
fn stop_gateway() -> Result<(), String> {
    info!("Stopping gateway...");

    let openclaw_bin = get_openclaw_path();
    let node_dir = get_node_bin_dir();

    // Prefer `openclaw gateway stop` which properly handles the LaunchAgent.
    // Raw `pkill` only kills the process — launchd immediately restarts it,
    // causing the subsequent `gateway start` to fail on `launchctl kickstart`.
    let result = if openclaw_bin.exists() {
        Command::new(openclaw_bin.to_str().unwrap())
            .args(["gateway", "stop"])
            .env("PATH", join_path_env(&node_dir))
            .output()
    } else {
        Command::new("openclaw")
            .args(["gateway", "stop"])
            .output()
    };

    match result {
        Ok(output) => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                info!("openclaw gateway stop returned non-zero ({}): {}", output.status, stderr.trim());
                // Fall back to pkill as last resort
                #[cfg(unix)]
                { let _ = Command::new("pkill").args(["-f", "openclaw-gateway"]).output(); }
                #[cfg(windows)]
                { let _ = Command::new("taskkill").args(["/F", "/IM", "openclaw-gateway.exe"]).output(); }
            } else {
                info!("Gateway stopped via openclaw CLI");
            }
        }
        Err(e) => {
            info!("openclaw gateway stop failed ({}), falling back to pkill", e);
            #[cfg(unix)]
            { let _ = Command::new("pkill").args(["-f", "openclaw-gateway"]).output(); }
            #[cfg(windows)]
            { let _ = Command::new("taskkill").args(["/F", "/IM", "openclaw-gateway.exe"]).output(); }
        }
    }

    Ok(())
}

// ═══ Memory (LanceDB Plugin) ══════════════════════════════════════════════
// OpenClaw ships a built-in memory-lancedb plugin that provides:
//   - memory_recall — semantic vector search over stored memories
//   - memory_store — save memories with categories and importance
//   - memory_forget — delete memories
// Configuration: plugins.slots.memory = "memory-lancedb" in openclaw.json.
// Requires: an OpenAI-compatible embedding API key.

/// Check if memory-lancedb is configured in openclaw.json.
#[tauri::command]
fn check_memory_configured() -> bool {
    let config = match parse_openclaw_config() {
        Some(c) => c,
        None => return false,
    };
    // Check plugins.slots.memory == "memory-lancedb"
    let slot = config.pointer("/plugins/slots/memory")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if slot != "memory-lancedb" {
        return false;
    }
    // Check plugins.entries.memory-lancedb.config.embedding.apiKey exists
    let api_key = config.pointer("/plugins/entries/memory-lancedb/config/embedding/apiKey")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    !api_key.is_empty()
}

/// Enable the memory-lancedb plugin by patching openclaw.json.
/// This is a pure config operation — no Python, no venv, no git clone.
#[tauri::command]
fn enable_memory_plugin(api_key: String, base_url: Option<String>, model: Option<String>) -> Result<(), String> {
    let api_key = api_key.trim().to_string();
    if api_key.is_empty() {
        return Err("An embedding API key is required.".to_string());
    }
    if api_key.starts_with("http://") || api_key.starts_with("https://") {
        return Err("The API key looks like a URL. Please enter your actual API key, not the endpoint URL.".to_string());
    }

    let model = model
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "text-embedding-3-small".to_string());

    let base_url = base_url
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty());

    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    let config_path = home.join(".openclaw/openclaw.json");

    if !config_path.exists() {
        return Err("OpenClaw config not found. Install OpenClaw first.".to_string());
    }

    let content = fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read config: {}", e))?;
    let sanitized = sanitize_json5(&content);
    let mut config: serde_json::Value = serde_json::from_str(&sanitized)
        .map_err(|e| format!("Failed to parse config: {}", e))?;

    // Build the embedding config
    let embedding = serde_json::json!({
        "apiKey": api_key,
        "model": model,
    });

    // If a base URL is provided (Azure Foundry), also store it
    // Note: memory-lancedb's strict schema only allows apiKey + model in "embedding",
    // so we store the base URL separately for the gateway env.

    // Ensure plugins object exists
    let obj = config.as_object_mut().ok_or("Config is not an object")?;
    if !obj.contains_key("plugins") {
        obj.insert("plugins".to_string(), serde_json::json!({}));
    }
    let plugins = obj.get_mut("plugins").unwrap().as_object_mut()
        .ok_or("plugins is not an object")?;

    // Set plugins.slots.memory = "memory-lancedb"
    if !plugins.contains_key("slots") {
        plugins.insert("slots".to_string(), serde_json::json!({}));
    }
    let slots = plugins.get_mut("slots").unwrap().as_object_mut()
        .ok_or("plugins.slots is not an object")?;
    slots.insert("memory".to_string(), serde_json::json!("memory-lancedb"));

    // Set plugins.entries.memory-lancedb
    if !plugins.contains_key("entries") {
        plugins.insert("entries".to_string(), serde_json::json!({}));
    }
    let entries = plugins.get_mut("entries").unwrap().as_object_mut()
        .ok_or("plugins.entries is not an object")?;
    entries.insert("memory-lancedb".to_string(), serde_json::json!({
        "enabled": true,
        "config": {
            "embedding": embedding,
            "autoCapture": true,
            "autoRecall": true,
        },
    }));

    // If a base URL is provided (Azure Foundry), write it to env.vars in
    // openclaw.json so that `gateway install` embeds it in the LaunchAgent plist.
    // The OpenAI SDK reads OPENAI_BASE_URL automatically.
    if let Some(ref url) = base_url {
        if !obj.contains_key("env") {
            obj.insert("env".to_string(), serde_json::json!({}));
        }
        let env_obj = obj.get_mut("env").unwrap().as_object_mut()
            .ok_or("env is not an object")?;
        if !env_obj.contains_key("vars") {
            env_obj.insert("vars".to_string(), serde_json::json!({}));
        }
        let vars = env_obj.get_mut("vars").unwrap().as_object_mut()
            .ok_or("env.vars is not an object")?;
        vars.insert("OPENAI_BASE_URL".to_string(), serde_json::json!(url));
        info!("Set env.vars.OPENAI_BASE_URL={} in openclaw.json", url);
    }

    // Write back openclaw.json (no Paw-specific keys — gateway rejects unknown root keys)
    fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap())
        .map_err(|e| format!("Failed to write config: {}", e))?;

    // Also store the base URL in paw-settings.json for CLI commands (memory_stats/memory_search)
    save_paw_settings(&serde_json::json!({
        "embeddingBaseUrl": base_url,
    }))?;

    info!("Enabled memory-lancedb plugin in openclaw.json (model: {})", model);
    Ok(())
}

/// Read Paw's own settings from ~/.openclaw/paw-settings.json.
fn read_paw_settings() -> Option<serde_json::Value> {
    let home = dirs::home_dir()?;
    let path = home.join(".openclaw/paw-settings.json");
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Write Paw's own settings to ~/.openclaw/paw-settings.json.
/// Merges with existing settings.
fn save_paw_settings(new_values: &serde_json::Value) -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    let path = home.join(".openclaw/paw-settings.json");
    let mut settings = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if let (Some(obj), Some(new_obj)) = (settings.as_object_mut(), new_values.as_object()) {
        for (k, v) in new_obj {
            obj.insert(k.clone(), v.clone());
        }
    }
    fs::write(&path, serde_json::to_string_pretty(&settings).unwrap())
        .map_err(|e| format!("Failed to write paw-settings.json: {}", e))?;
    Ok(())
}

/// Read the embedding base URL from Paw settings.
/// Returns the URL if configured, None otherwise.
#[tauri::command]
fn get_embedding_base_url() -> Option<String> {
    let settings = read_paw_settings()?;
    settings.get("embeddingBaseUrl")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Run `openclaw ltm stats` — returns the memory count as a string.
#[tauri::command]
fn memory_stats() -> Result<String, String> {
    let openclaw_bin = get_openclaw_path();
    let node_dir = get_node_bin_dir();
    let base_url = read_paw_settings()
        .and_then(|s| s.get("embeddingBaseUrl").and_then(|v| v.as_str()).map(|s| s.to_string()));

    let mut cmd = if openclaw_bin.exists() {
        let mut c = Command::new(openclaw_bin.to_str().unwrap());
        c.env("PATH", join_path_env(&node_dir));
        c
    } else {
        Command::new("openclaw")
    };
    cmd.args(["ltm", "stats"]);
    if let Some(ref url) = base_url { cmd.env("OPENAI_BASE_URL", url); }

    let output = cmd.output()
        .map_err(|e| format!("Failed to run openclaw ltm stats: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        return Err(format!("ltm stats failed: {}", stderr));
    }
    Ok(stdout.trim().to_string())
}

/// Run `openclaw ltm search <query>` — returns JSON array of memories.
#[tauri::command]
fn memory_search(query: String, limit: Option<u32>) -> Result<String, String> {
    let openclaw_bin = get_openclaw_path();
    let node_dir = get_node_bin_dir();
    let limit_str = limit.unwrap_or(10).to_string();
    let base_url = read_paw_settings()
        .and_then(|s| s.get("embeddingBaseUrl").and_then(|v| v.as_str()).map(|s| s.to_string()));

    let mut cmd = if openclaw_bin.exists() {
        let mut c = Command::new(openclaw_bin.to_str().unwrap());
        c.env("PATH", join_path_env(&node_dir));
        c
    } else {
        Command::new("openclaw")
    };
    cmd.args(["ltm", "search", &query, "--limit", &limit_str]);
    if let Some(ref url) = base_url { cmd.env("OPENAI_BASE_URL", url); }

    let output = cmd.output()
        .map_err(|e| format!("Failed to run openclaw ltm search: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        return Err(format!("ltm search failed: {}", stderr));
    }
    Ok(stdout.trim().to_string())
}

/// Repair openclaw.json by removing any cruft from previous Paw versions.
/// Removes old "skills" key (from v1 palace integration) and ensures
/// the config is valid for the gateway.
#[tauri::command]
fn repair_openclaw_config() -> Result<bool, String> {
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    let config_path = home.join(".openclaw/openclaw.json");

    if !config_path.exists() {
        return Ok(false);
    }

    let content = fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read config: {}", e))?;

    let sanitized = sanitize_json5(&content);
    let mut config: serde_json::Value = serde_json::from_str(&sanitized)
        .map_err(|e| format!("Failed to parse config: {}", e))?;

    let mut repaired = false;
    if let Some(obj) = config.as_object_mut() {
        // Remove the invalid "skills" key if present (legacy palace integration)
        if obj.contains_key("skills") {
            obj.remove("skills");
            repaired = true;
            info!("Removed invalid 'skills' key from openclaw.json");
        }
        // Remove "_paw" key if present (pre-2026.2.14 stored settings in openclaw.json)
        if obj.contains_key("_paw") {
            obj.remove("_paw");
            repaired = true;
            info!("Removed invalid '_paw' key from openclaw.json");
        }

        // Fix memory-lancedb embedding config — strict schema only allows apiKey + model.
        // If the user manually added baseUrl or other properties, strip them and
        // migrate baseUrl to env.vars.OPENAI_BASE_URL where the gateway can use it.
        let mut rescued_base_url: Option<String> = None;
        if let Some(embedding) = obj
            .get_mut("plugins")
            .and_then(|p| p.get_mut("entries"))
            .and_then(|e| e.get_mut("memory-lancedb"))
            .and_then(|m| m.get_mut("config"))
            .and_then(|c| c.get_mut("embedding"))
            .and_then(|e| e.as_object_mut())
        {
            // Rescue baseUrl before removing it
            if let Some(base_url_val) = embedding.remove("baseUrl") {
                repaired = true;
                info!("Removed invalid 'baseUrl' from embedding config");
                rescued_base_url = base_url_val.as_str().map(|s| s.to_string());
            }
            // Remove any other unknown properties (only apiKey and model are valid)
            let allowed: std::collections::HashSet<&str> = ["apiKey", "model"].iter().copied().collect();
            let invalid_keys: Vec<String> = embedding.keys()
                .filter(|k| !allowed.contains(k.as_str()))
                .cloned()
                .collect();
            for key in invalid_keys {
                embedding.remove(&key);
                repaired = true;
                info!("Removed invalid '{}' from embedding config", key);
            }
        }
        // Now migrate rescued baseUrl to env.vars.OPENAI_BASE_URL (separate borrow scope)
        if let Some(url) = rescued_base_url {
            if !obj.contains_key("env") {
                obj.insert("env".to_string(), serde_json::json!({}));
            }
            if let Some(env_obj) = obj.get_mut("env").and_then(|e| e.as_object_mut()) {
                if !env_obj.contains_key("vars") {
                    env_obj.insert("vars".to_string(), serde_json::json!({}));
                }
                if let Some(vars) = env_obj.get_mut("vars").and_then(|v| v.as_object_mut()) {
                    vars.insert("OPENAI_BASE_URL".to_string(), serde_json::json!(&url));
                    info!("Migrated baseUrl to env.vars.OPENAI_BASE_URL: {}", url);
                }
            }
            let _ = save_paw_settings(&serde_json::json!({ "embeddingBaseUrl": &url }));
        }
    }

    if repaired {
        fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap())
            .map_err(|e| format!("Failed to write config: {}", e))?;
        info!("Repaired openclaw.json at {:?}", config_path);
    }

    Ok(repaired)
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
            check_memory_configured,
            enable_memory_plugin,
            get_embedding_base_url,
            memory_stats,
            memory_search,
            repair_openclaw_config
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
