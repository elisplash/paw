use std::process::Command;
use std::path::PathBuf;
use std::fs;
use std::net::TcpStream;
use std::time::{Duration, Instant};
use log::{info, warn, error};
use tauri::{Emitter, Manager};

/// Set restrictive file permissions (owner-only read/write) on Unix.
/// No-op on non-Unix platforms.
#[cfg(unix)]
fn set_owner_only_permissions(path: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let perms = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, perms)
        .map_err(|e| format!("Failed to set permissions on {:?}: {}", path, e))
}

#[cfg(not(unix))]
fn set_owner_only_permissions(_path: &std::path::Path) -> Result<(), String> {
    Ok(()) // Windows ACLs not handled here yet
}

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
    let azure_api_version = get_api_version_or_default();
    let embedding_model = parse_openclaw_config()
        .and_then(|c| c.pointer("/plugins/entries/memory-lancedb/config/embedding/model")
            .and_then(|v| v.as_str()).map(|s| s.to_string()));

    // Ensure memory plugin is compatible with the configured provider
    let is_azure = embedding_base_url.as_ref()
        .map(|u| is_azure_endpoint(u)).unwrap_or(false);
    match ensure_memory_plugin_compatible(is_azure) {
        Ok(()) => info!("Memory plugin compatibility verified (azure={})", is_azure),
        Err(e) => warn!("Failed to ensure plugin compatibility: {}", e),
    }

    // On macOS, set env vars for the launchd user session so the
    // LaunchAgent picks them up when gateway is started/restarted by launchd.
    #[cfg(target_os = "macos")]
    if let Some(ref url) = embedding_base_url {
        info!("Setting OPENAI_BASE_URL={} via launchctl setenv", url);
        let _ = Command::new("launchctl")
            .args(["setenv", "OPENAI_BASE_URL", url])
            .output();
        if is_azure {
            let _ = Command::new("launchctl")
                .args(["setenv", "AZURE_OPENAI_ENDPOINT", url])
                .output();
            let _ = Command::new("launchctl")
                .args(["setenv", "OPENAI_API_VERSION", &azure_api_version])
                .output();
            if let Some(ref model) = embedding_model {
                let _ = Command::new("launchctl")
                    .args(["setenv", "OPENAI_DEPLOYMENT", model])
                    .output();
            }
            if let Some(home) = dirs::home_dir() {
                let shim = home.join(".openclaw/_paw_openai_shim.js");
                if shim.exists() {
                    let _ = Command::new("launchctl")
                        .args(["setenv", "NODE_OPTIONS", &format!("--require {}", shim.display())])
                        .output();
                }
            }
        }
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
    info!("Ensuring gateway LaunchAgent is installed...");
    let mut install_cmd = Command::new(&bin_str);
    install_cmd.args(["gateway", "install"])
        .env("PATH", &path_env);
    if let Some(ref url) = embedding_base_url {
        apply_embedding_env(&mut install_cmd, url);
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
        apply_embedding_env(&mut cmd, url);
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

    // Use `openclaw gateway stop` with a timeout so the UI doesn't freeze.
    let mut cmd = if openclaw_bin.exists() {
        let mut c = Command::new(openclaw_bin.to_str().unwrap());
        c.env("PATH", join_path_env(&node_dir));
        c
    } else {
        Command::new("openclaw")
    };
    cmd.args(["gateway", "stop"]);

    // Spawn with timeout instead of blocking .output()
    match run_with_timeout(cmd, 10) {
        Ok(stdout) => {
            info!("Gateway stopped via openclaw CLI: {}", stdout.trim());
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

/// Returns true if the given base URL looks like an Azure endpoint.
fn is_azure_endpoint(url: &str) -> bool {
    url.contains(".azure.") || url.contains(".cognitiveservices.") || url.contains(".ai.azure.")
}

/// Find the memory-lancedb plugin directory (inside the openclaw npm package).
/// Checks multiple possible install locations including nvm-managed globals.
fn find_bundled_memory_plugin() -> Option<PathBuf> {
    let app_dir = get_app_data_dir();

    // 1. Installed via npm in app data dir
    let candidate = app_dir.join("node_modules/openclaw/extensions/memory-lancedb");
    if candidate.join("index.ts").exists() {
        return Some(candidate);
    }

    // 2. ~/.openclaw/node_modules/openclaw/extensions/memory-lancedb
    let home = dirs::home_dir()?;
    let global = home.join(".openclaw/node_modules/openclaw/extensions/memory-lancedb");
    if global.join("index.ts").exists() {
        return Some(global);
    }

    // 3. Resolve from the `openclaw` binary — handles nvm, volta, etc.
    //    `which openclaw` → /path/to/node/bin/openclaw (symlink)
    //    Real path: /path/to/node/lib/node_modules/openclaw/...
    let openclaw_bin = get_openclaw_path();
    let bin_to_check = if openclaw_bin.exists() {
        openclaw_bin
    } else {
        // Try `which openclaw`
        if let Ok(out) = Command::new("which").arg("openclaw").output() {
            if out.status.success() {
                PathBuf::from(String::from_utf8_lossy(&out.stdout).trim())
            } else {
                return None;
            }
        } else {
            return None;
        }
    };

    // Resolve symlink to find the real package location
    if let Ok(resolved) = fs::canonicalize(&bin_to_check) {
        // resolved is typically: .../lib/node_modules/openclaw/dist/run-main-XXX.js
        // or .../lib/node_modules/openclaw/bin/openclaw.js
        // Walk up to find the openclaw package root containing extensions/
        let mut dir = resolved.as_path();
        for _ in 0..10 {
            if let Some(parent) = dir.parent() {
                let ext_dir = parent.join("extensions/memory-lancedb");
                if ext_dir.join("index.ts").exists() {
                    return Some(ext_dir);
                }
                dir = parent;
            } else {
                break;
            }
        }
    }

    // 4. Also try the bin path's sibling lib directory (nvm pattern)
    //    bin: ~/.nvm/versions/node/vXX/bin/openclaw
    //    lib: ~/.nvm/versions/node/vXX/lib/node_modules/openclaw/extensions/
    if let Some(bin_dir) = bin_to_check.parent() {
        if let Some(version_dir) = bin_dir.parent() {
            let nvm_candidate = version_dir.join("lib/node_modules/openclaw/extensions/memory-lancedb");
            if nvm_candidate.join("index.ts").exists() {
                return Some(nvm_candidate);
            }
        }
    }

    None
}

/// Ensure the memory plugin is compatible with the configured provider.
///
/// **Standard OpenAI / compatible endpoints:** No patching needed — the plugin
/// uses `new OpenAI({ apiKey })` which reads `OPENAI_BASE_URL` automatically.
///
/// **Azure OpenAI:** The plugin must use `new AzureOpenAI(...)` instead of
/// `new OpenAI(...)`.  We handle this in two layers:
///   1. Source patches — rewrite import & constructor **in-place** at the
///      actual bundled plugin location (nvm, volta, local, etc.)
///   2. Runtime shim — monkey-patch the module loader to Proxy the constructor
///      (injected via `NODE_OPTIONS=--require <shim>` in openclaw.json env.vars)
///
/// The shim is the primary mechanism; source patches are best-effort.
fn ensure_memory_plugin_compatible(is_azure: bool) -> Result<(), String> {
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;

    // Clean up old ~/.openclaw/extensions/ copy (no longer used — we patch in-place)
    remove_patched_memory_plugin();

    if !is_azure {
        // Standard OpenAI path — remove stale shim
        let shim = home.join(".openclaw/_paw_openai_shim.js");
        if shim.exists() { let _ = fs::remove_file(&shim); }
        info!("Standard OpenAI routing — no plugin patching needed");
        return Ok(());
    }

    // ── Azure OpenAI path ──────────────────────────────────────────────

    // Layer 1: Patch the plugin source files IN-PLACE at whatever location
    // the gateway actually loads them from (nvm, global npm, local, etc.)
    let plugin_dir = find_bundled_memory_plugin();
    if let Some(ref pdir) = plugin_dir {
        info!("Found memory-lancedb plugin at {:?}", pdir);
        for file in &["index.ts", "index.js", "dist/index.js", "dist/index.mjs"] {
            let path = pdir.join(file);
            if path.exists() {
                let content = fs::read_to_string(&path)
                    .map_err(|e| format!("Failed to read {}: {}", file, e))?;
                // Only patch if not already patched
                if !content.contains("AzureOpenAI") {
                    let patched = apply_openai_to_azure_patch(&content);
                    fs::write(&path, &patched)
                        .map_err(|e| format!("Failed to write {}: {}", file, e))?;
                    info!("Patched {} in-place for Azure routing", file);
                } else {
                    info!("{} already contains Azure routing", file);
                }
            }
        }
    } else {
        warn!("Bundled memory-lancedb plugin not found — relying on shim only");
    }

    // Layer 2: Write the runtime routing shim (primary reliability mechanism)
    // Pass the plugin directory so the shim can resolve `openai` from the
    // correct node_modules (the shim lives in ~/.openclaw/ which has no
    // node_modules of its own).
    write_openai_routing_shim(&home, plugin_dir.as_deref())?;

    Ok(())
}

/// Write the runtime OpenAI → AzureOpenAI routing shim.
///
/// This shim is loaded via `NODE_OPTIONS=--require <path>` before any
/// application code runs.  It eagerly loads the `openai` npm package,
/// replaces the `OpenAI` constructor with a Proxy that creates `AzureOpenAI`
/// instead, and mutates the cached module exports.  This works for both
/// CJS `require('openai')` and ESM `import OpenAI from 'openai'` because
/// Node's ESM loader uses the same underlying CJS module cache for
/// CommonJS packages.
///
/// `openclaw_plugin_dir` is the path to the memory-lancedb plugin directory
/// (e.g. `~/.nvm/versions/node/v24/lib/node_modules/openclaw/extensions/memory-lancedb`).
/// We use it to compute the openclaw package root so `require('openai')` resolves correctly.
fn write_openai_routing_shim(home: &std::path::Path, openclaw_plugin_dir: Option<&std::path::Path>) -> Result<(), String> {
    // Compute the openclaw package root so the shim can resolve `openai`.
    // Plugin dir: .../openclaw/extensions/memory-lancedb → root: .../openclaw/
    let openclaw_root = openclaw_plugin_dir
        .and_then(|p| p.parent())        // extensions/
        .and_then(|p| p.parent());       // openclaw/

    // Build the require-resolution snippet.
    // If we know the openclaw root, use Module.createRequire so require('openai')
    // resolves from openclaw's node_modules rather than the shim's directory.
    let require_snippet = if let Some(root) = openclaw_root {
        let root_str = root.display().to_string().replace('\\', "/");
        format!(
            "const {{ createRequire: _pawCR }} = require('module');\n    \
             const _pawReq = _pawCR('{}/package.json');\n    \
             const openaiMod = _pawReq('openai');",
            root_str
        )
    } else {
        // Fallback: try process.argv[1] (the gateway main script), then bare require
        "let openaiMod;\n    \
         try {\n      \
           const { createRequire: _cr } = require('module');\n      \
           const _r = _cr(process.argv[1] || __filename);\n      \
           openaiMod = _r('openai');\n    \
         } catch (_) {\n      \
           openaiMod = require('openai');\n    \
         }".to_string()
    };

    let shim_content = format!(
r#"// Paw OpenAI → AzureOpenAI routing shim
// Loaded via NODE_OPTIONS=--require before the gateway starts.
//
// Strategy: eagerly load `openai`, replace the OpenAI constructor with a
// Proxy that creates AzureOpenAI, and mutate the module cache in-place.
// Both CJS require() and ESM import see the patched exports because
// ESM resolves CJS packages through the same module cache.
'use strict';

const _pawEndpoint = process.env.AZURE_OPENAI_ENDPOINT || '';
const _pawIsAzure = _pawEndpoint.length > 0 && (
  _pawEndpoint.includes('.azure.') ||
  _pawEndpoint.includes('.cognitiveservices.') ||
  _pawEndpoint.includes('.ai.azure.')
);

if (_pawIsAzure) {{
  try {{
    // Eagerly load the openai package into the CJS module cache.
    // Use createRequire to resolve from the openclaw package root
    // (the shim lives in ~/.openclaw/ which has no node_modules).
    {require_snippet}
    const OrigOpenAI = openaiMod.default || openaiMod.OpenAI;
    const AzureOpenAI = openaiMod.AzureOpenAI;

    if (!OrigOpenAI || !AzureOpenAI || typeof AzureOpenAI !== 'function') {{
      console.warn('[paw] openai package missing OpenAI or AzureOpenAI exports — cannot route');
    }} else {{
      const endpoint   = _pawEndpoint.replace(/\/openai\/?$/, '');
      const apiVersion = process.env.OPENAI_API_VERSION || '2024-08-01-preview';
      const deployment = process.env.OPENAI_DEPLOYMENT || undefined;
      const apiKey     = process.env.OPENAI_API_KEY || undefined;

      const handler = {{
        construct(_target, args) {{
          const opts = args[0] || {{}};
          // If caller explicitly set baseURL, honour it (non-Azure usage)
          if (opts.baseURL) return new OrigOpenAI(opts);
          try {{
            const client = new AzureOpenAI({{
              apiKey:     opts.apiKey || apiKey,
              endpoint:   endpoint,
              deployment: deployment,
              apiVersion: apiVersion,
            }});
            return client;
          }} catch (err) {{
            console.error('[paw] AzureOpenAI constructor failed, falling back:', err.message);
            return new OrigOpenAI(opts);
          }}
        }},
        apply(_target, _this, args) {{
          return handler.construct(_target, args);
        }},
        get(target, prop, receiver) {{
          // Proxy static properties / prototype so instanceof checks etc. still work
          return Reflect.get(target, prop, receiver);
        }}
      }};

      const proxied = new Proxy(OrigOpenAI, handler);

      // Mutate the cached module exports in-place so ALL future imports
      // (both CJS require and ESM import) see the proxied constructor.
      // Use Object.defineProperty because the openai package defines
      // `default` as a getter-only property — direct assignment throws.
      function patchExports(obj) {{
        if (!obj) return;
        for (const key of ['default', 'OpenAI']) {{
          if (obj[key]) {{
            try {{ obj[key] = proxied; }} catch (_) {{
              try {{
                Object.defineProperty(obj, key, {{
                  value: proxied, writable: true, configurable: true, enumerable: true
                }});
              }} catch (_e) {{ /* truly frozen — skip */ }}
            }}
          }}
        }}
      }}
      patchExports(openaiMod);

      // Also patch the CJS cache entry directly for extra safety
      const Module = require('module');
      try {{
        const resolvedPath = Module._resolveFilename('openai', module);
        if (resolvedPath && Module._cache && Module._cache[resolvedPath]) {{
          patchExports(Module._cache[resolvedPath].exports);
        }}
      }} catch (_e) {{ /* _resolveFilename may throw if openai not in default paths */ }}

      console.log('[paw] Azure OpenAI routing active —',
        'endpoint:', endpoint,
        'deployment:', deployment || '(from model)',
        'apiVersion:', apiVersion,
        'apiKey:', apiKey ? apiKey.slice(0, 8) + '...' : '(none)');
    }}
  }} catch (err) {{
    console.error('[paw] Failed to initialise Azure routing shim:', err.message);
    console.error('[paw] Stack:', err.stack);
  }}
}} else if (process.env.OPENAI_BASE_URL) {{
  console.log('[paw] Standard OpenAI routing — baseURL:', process.env.OPENAI_BASE_URL);
}} else {{
  console.log('[paw] No AZURE_OPENAI_ENDPOINT set — shim inactive');
}}
"#,
        require_snippet = require_snippet,
    );

    let shim_path = home.join(".openclaw/_paw_openai_shim.js");
    fs::write(&shim_path, &shim_content)
        .map_err(|e| format!("Failed to write routing shim: {}", e))?;
    info!("Wrote OpenAI routing shim to {:?}", shim_path);
    Ok(())
}

/// Apply source-level patches to rewrite `new OpenAI(…)` → `new AzureOpenAI(…)`
/// in plugin source files.  Works on both TypeScript and JavaScript.
///
/// This is a best-effort layer — the runtime shim (Layer 2) is the reliable fallback.
fn apply_openai_to_azure_patch(content: &str) -> String {
    let mut result = content.to_string();

    // ── Step 1: Add AzureOpenAI import ──

    // ES module: import OpenAI from "openai" → import OpenAI, { AzureOpenAI } from "openai"
    if result.contains("import OpenAI from") && !result.contains("AzureOpenAI") {
        result = result.replace(
            r#"import OpenAI from "openai""#,
            r#"import OpenAI, { AzureOpenAI } from "openai""#,
        );
        result = result.replace(
            "import OpenAI from 'openai'",
            "import OpenAI, { AzureOpenAI } from 'openai'",
        );
    }

    // CommonJS: const OpenAI = require("openai") → also import AzureOpenAI
    if (result.contains("require(\"openai\")") || result.contains("require('openai')"))
        && !result.contains("AzureOpenAI")
    {
        result = result.replace(
            "require(\"openai\")",
            "require(\"openai\"); const { AzureOpenAI } = require(\"openai\")",
        );
        result = result.replace(
            "require('openai')",
            "require('openai'); const { AzureOpenAI } = require('openai')",
        );
    }

    // ── Step 2: Replace `new OpenAI({ apiKey })` with Azure-aware constructor ──

    let azure_constructor = concat!(
        "(() => {\n",
        "      const _url = process.env.AZURE_OPENAI_ENDPOINT || '';\n",
        "      const _isAz = _url.includes('.azure.') || _url.includes('.cognitiveservices.') || _url.includes('.ai.azure.');\n",
        "      if (_isAz && typeof AzureOpenAI !== 'undefined') {\n",
        "        return new AzureOpenAI({\n",
        "          apiKey: apiKey || process.env.OPENAI_API_KEY,\n",
        "          endpoint: _url.replace(/\\/openai\\/?$/, ''),\n",
        "          apiVersion: process.env.OPENAI_API_VERSION || '2024-08-01-preview',\n",
        "          deployment: this?.model || process.env.OPENAI_DEPLOYMENT || undefined,\n",
        "        });\n",
        "      }\n",
        "      return new OpenAI({ apiKey });\n",
        "    })()",
    );

    let patterns = [
        "new OpenAI({ apiKey })",
        "new OpenAI({apiKey})",
        "new OpenAI({ apiKey})",
        "new OpenAI({apiKey })",
    ];
    for pat in patterns {
        if result.contains(pat) {
            result = result.replacen(pat, azure_constructor, 1);
            break;
        }
    }

    result
}

/// Recursively copy a directory.
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            // Skip node_modules — symlink it instead
            if entry.file_name() == "node_modules" {
                #[cfg(unix)]
                { let _ = std::os::unix::fs::symlink(&src_path, &dst_path); }
                #[cfg(windows)]
                { let _ = Command::new("cmd")
                    .args(["/c", "mklink", "/J",
                        dst_path.to_str().unwrap(),
                        src_path.to_str().unwrap()])
                    .output(); }
                continue;
            }
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// Remove the patched memory plugin (revert to bundled version).
fn remove_patched_memory_plugin() {
    if let Some(home) = dirs::home_dir() {
        let target_dir = home.join(".openclaw/extensions/memory-lancedb");
        if target_dir.exists() {
            let _ = fs::remove_dir_all(&target_dir);
            info!("Removed patched memory-lancedb plugin");
        }
    }
}

/// Test the embedding API connection by sending a minimal embedding request.
/// Returns Ok("ok") on success, or an Err with a descriptive message on failure.
/// Works with both standard OpenAI and Azure OpenAI endpoints.
#[tauri::command]
fn test_embedding_connection(
    api_key: String,
    base_url: Option<String>,
    model: Option<String>,
    api_version: Option<String>,
    provider: Option<String>,
) -> Result<String, String> {
    let api_key = api_key.trim().to_string();
    if api_key.is_empty() {
        return Err("API key is required".to_string());
    }

    let model = model
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "text-embedding-3-small".to_string());

    let base_url = base_url
        .map(|u| u.trim().trim_end_matches('/').to_string())
        .filter(|u| !u.is_empty());

    let api_version = api_version
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "2024-08-01-preview".to_string());

    let is_azure = provider.as_deref() == Some("azure")
        || base_url.as_ref().map(|u| is_azure_endpoint(u)).unwrap_or(false);

    // Build the request URL and headers based on provider type
    let (url, auth_header) = if is_azure {
        let endpoint = base_url.as_deref().unwrap_or("")
            .trim_end_matches('/').replace("/openai", "");
        if endpoint.is_empty() {
            return Err("Azure endpoint URL is required".to_string());
        }
        let url = format!(
            "{}/openai/deployments/{}/embeddings?api-version={}",
            endpoint, model, api_version
        );
        (url, format!("api-key: {}", api_key))
    } else if let Some(ref base) = base_url {
        // Custom OpenAI-compatible: POST {base_url}/v1/embeddings
        let url = if base.ends_with("/v1") {
            format!("{}/embeddings", base)
        } else if base.contains("/v1/") {
            format!("{}/embeddings", base.trim_end_matches('/'))
        } else {
            format!("{}/v1/embeddings", base)
        };
        (url, format!("Authorization: Bearer {}", api_key))
    } else {
        // Standard OpenAI
        let url = "https://api.openai.com/v1/embeddings".to_string();
        (url, format!("Authorization: Bearer {}", api_key))
    };

    // Minimal request body — embed a single short string
    let body = serde_json::json!({
        "input": "test",
        "model": model
    });

    info!("Testing embedding connection: POST {}", url);

    // Use curl for the HTTP request (universally available, no extra deps)
    let mut cmd = Command::new("curl");
    cmd.args([
        "-s",                          // silent
        "-w", "\n%{http_code}",        // append HTTP status code
        "-X", "POST",
        &url,
        "-H", "Content-Type: application/json",
        "-H", &auth_header,
        "-d", &body.to_string(),
        "--connect-timeout", "10",
        "--max-time", "15",
    ]);

    let result = run_with_timeout(cmd, 20)?;

    // Parse: last line is the HTTP status code, everything before is the body
    let lines: Vec<&str> = result.trim().rsplitn(2, '\n').collect();
    let (status_str, response_body) = if lines.len() == 2 {
        (lines[0].trim(), lines[1])
    } else {
        (result.trim(), "")
    };

    let status: u16 = status_str.parse().unwrap_or(0);

    match status {
        200 => {
            // Verify we got actual embedding data back
            if let Ok(resp) = serde_json::from_str::<serde_json::Value>(response_body) {
                if resp.get("data").and_then(|d| d.as_array()).map(|a| !a.is_empty()).unwrap_or(false) {
                    info!("Embedding connection test passed (200 OK, got embedding data)");
                    Ok("ok".to_string())
                } else {
                    Err(format!("Got 200 but response missing embedding data: {}", 
                        &response_body[..response_body.len().min(200)]))
                }
            } else {
                Err(format!("Got 200 but could not parse response: {}",
                    &response_body[..response_body.len().min(200)]))
            }
        }
        401 => Err("Authentication failed (401). Check your API key.".to_string()),
        403 => Err("Access denied (403). Your API key may not have permission for this resource.".to_string()),
        404 => {
            if let Some(ref base) = base_url {
                if is_azure_endpoint(base) {
                    Err(format!("Deployment not found (404). Verify that '{}' is a valid deployment name in your Azure resource.", model))
                } else {
                    Err("Endpoint not found (404). Check your base URL.".to_string())
                }
            } else {
                Err("Not found (404). The model may not be available.".to_string())
            }
        }
        429 => Err("Rate limited (429). The connection works but you're being throttled. Try again shortly.".to_string()),
        0 => Err("Could not connect to the endpoint. Check the URL and your network connection.".to_string()),
        _ => {
            let detail = if response_body.len() > 200 {
                &response_body[..200]
            } else {
                response_body
            };
            Err(format!("HTTP {} — {}", status, detail))
        }
    }
}

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
fn enable_memory_plugin(api_key: String, base_url: Option<String>, model: Option<String>, api_version: Option<String>, provider: Option<String>) -> Result<(), String> {
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

    let api_version = api_version
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "2024-08-01-preview".to_string());

    let provider_str = provider.as_deref().unwrap_or("openai");

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

    // If a base URL is provided (Azure Foundry), also store it.
    // Note: memory-lancedb's strict schema only allows apiKey + model in "embedding",
    // so we store the base URL separately for the gateway env.
    // For Azure, the "model" doubles as the deployment name.

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
        // Always provide the API key so the gateway process can authenticate
        vars.insert("OPENAI_API_KEY".to_string(), serde_json::json!(&api_key));
        info!("Set env.vars.OPENAI_BASE_URL={} in openclaw.json", url);

        // For Azure provider, also set AZURE_OPENAI_ENDPOINT and OPENAI_API_VERSION
        if provider_str == "azure" || is_azure_endpoint(url) {
            vars.insert("AZURE_OPENAI_ENDPOINT".to_string(), serde_json::json!(url));
            vars.insert("OPENAI_API_VERSION".to_string(), serde_json::json!(&api_version));
            vars.insert("OPENAI_DEPLOYMENT".to_string(), serde_json::json!(&model));
            // Embed NODE_OPTIONS in the plist so the gateway process loads
            // the routing shim that swaps OpenAI → AzureOpenAI at runtime.
            let home = dirs::home_dir().ok_or("Cannot find home directory")?;
            let shim_path = home.join(".openclaw/_paw_openai_shim.js");
            vars.insert("NODE_OPTIONS".to_string(),
                serde_json::json!(format!("--require {}", shim_path.display())));
            info!("Set env.vars for Azure: AZURE_OPENAI_ENDPOINT, OPENAI_API_VERSION={}, OPENAI_DEPLOYMENT={}, NODE_OPTIONS", api_version, model);
        } else {
            // Non-Azure custom endpoint — remove Azure-specific vars if present
            vars.remove("AZURE_OPENAI_ENDPOINT");
            vars.remove("OPENAI_API_VERSION");
            vars.remove("OPENAI_DEPLOYMENT");
            vars.remove("NODE_OPTIONS");
        }
    } else {
        // No base URL — remove all custom env vars if present
        if let Some(env_obj) = obj.get_mut("env").and_then(|e| e.as_object_mut()) {
            if let Some(vars) = env_obj.get_mut("vars").and_then(|v| v.as_object_mut()) {
                vars.remove("OPENAI_BASE_URL");
                vars.remove("OPENAI_API_KEY");
                vars.remove("AZURE_OPENAI_ENDPOINT");
                vars.remove("OPENAI_API_VERSION");
                vars.remove("OPENAI_DEPLOYMENT");
                vars.remove("NODE_OPTIONS");
            }
        }
    }

    // Write back openclaw.json (no Paw-specific keys — gateway rejects unknown root keys)
    fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap())
        .map_err(|e| format!("Failed to write config: {}", e))?;

    // Also store the base URL, API version, and provider in paw-settings.json
    save_paw_settings(&serde_json::json!({
        "embeddingBaseUrl": base_url,
        "azureApiVersion": &api_version,
        "embeddingProvider": provider_str,
    }))?;

    // Ensure the memory plugin is compatible with the configured provider.
    // Azure endpoints need AzureOpenAI SDK routing; standard endpoints work as-is.
    let is_azure = provider_str == "azure"
        || base_url.as_ref().map(|u| is_azure_endpoint(u)).unwrap_or(false);
    match ensure_memory_plugin_compatible(is_azure) {
        Ok(()) => info!("Memory plugin configured for {} routing", if is_azure { "Azure" } else { "standard OpenAI" }),
        Err(e) => warn!("Failed to configure memory plugin routing: {}", e),
    }

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

/// Read the Azure API version from Paw settings.
/// Returns the version string if configured, None otherwise.
#[tauri::command]
fn get_azure_api_version() -> Option<String> {
    let settings = read_paw_settings()?;
    settings.get("azureApiVersion")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Read the embedding provider from Paw settings ("openai" or "azure").
#[tauri::command]
fn get_embedding_provider() -> Option<String> {
    let settings = read_paw_settings()?;
    settings.get("embeddingProvider")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Helper: read the stored Azure API version, falling back to a default.
fn get_api_version_or_default() -> String {
    read_paw_settings()
        .and_then(|s| s.get("azureApiVersion").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .unwrap_or_else(|| "2024-08-01-preview".to_string())
}

/// Helper: apply embedding-related env vars to a Command.
/// Routes to either standard OpenAI SDK or Azure OpenAI SDK based on the URL.
///
/// Standard OpenAI path:
///   - Sets OPENAI_BASE_URL (SDK reads this automatically)
///   - Sets OPENAI_API_KEY
///   - No patching or shim needed
///
/// Azure OpenAI path:
///   - Sets OPENAI_BASE_URL, AZURE_OPENAI_ENDPOINT, OPENAI_API_VERSION
///   - Sets OPENAI_DEPLOYMENT (model name doubles as deployment name)
///   - Sets OPENAI_API_KEY
///   - Injects NODE_OPTIONS with routing shim that swaps OpenAI → AzureOpenAI
fn apply_embedding_env(cmd: &mut Command, url: &str) {
    // Always set the base URL — OpenAI SDK reads OPENAI_BASE_URL automatically
    cmd.env("OPENAI_BASE_URL", url);

    // Set OPENAI_API_KEY from config so CLI commands authenticate
    if let Some(api_key) = parse_openclaw_config()
        .and_then(|c| c.pointer("/plugins/entries/memory-lancedb/config/embedding/apiKey")
            .and_then(|v| v.as_str()).map(|s| s.to_string()))
    {
        cmd.env("OPENAI_API_KEY", &api_key);
    }

    // Check saved provider preference first, then fall back to URL sniffing
    let saved_provider = read_paw_settings()
        .and_then(|s| s.get("embeddingProvider").and_then(|v| v.as_str()).map(|s| s.to_string()));
    let is_azure = saved_provider.as_deref() == Some("azure") || is_azure_endpoint(url);

    if is_azure {
        // ── Azure OpenAI path ──
        let api_version = get_api_version_or_default();
        cmd.env("AZURE_OPENAI_ENDPOINT", url);
        cmd.env("OPENAI_API_VERSION", &api_version);

        // Pass the deployment/model name
        if let Some(model) = parse_openclaw_config()
            .and_then(|c| c.pointer("/plugins/entries/memory-lancedb/config/embedding/model")
                .and_then(|v| v.as_str()).map(|s| s.to_string()))
        {
            cmd.env("OPENAI_DEPLOYMENT", &model);
        }

        // Inject routing shim via NODE_OPTIONS — intercepts `new OpenAI()` → `new AzureOpenAI()`
        if let Some(home) = dirs::home_dir() {
            let shim = home.join(".openclaw/_paw_openai_shim.js");
            if shim.exists() {
                cmd.env("NODE_OPTIONS", format!("--require {}", shim.display()));
            }
        }
    }
    // Standard OpenAI path: OPENAI_BASE_URL + OPENAI_API_KEY are sufficient.
    // The SDK reads both automatically — no shim or patching needed.
}

/// Run a command with a timeout. Returns stdout on success.
fn run_with_timeout(mut cmd: Command, timeout_secs: u64) -> Result<String, String> {
    let mut child = cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn command: {}", e))?;

    let start = Instant::now();
    let timeout = Duration::from_secs(timeout_secs);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = child.stdout.take()
                    .map(|mut s| { let mut buf = String::new(); std::io::Read::read_to_string(&mut s, &mut buf).ok(); buf })
                    .unwrap_or_default();
                let stderr = child.stderr.take()
                    .map(|mut s| { let mut buf = String::new(); std::io::Read::read_to_string(&mut s, &mut buf).ok(); buf })
                    .unwrap_or_default();
                if !status.success() {
                    return Err(format!("Command failed: {}", stderr.trim()));
                }
                return Ok(stdout.trim().to_string());
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    return Err("Command timed out".to_string());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(format!("Wait failed: {}", e)),
        }
    }
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
    if let Some(ref url) = base_url {
        apply_embedding_env(&mut cmd, url);
    }

    run_with_timeout(cmd, 10)
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
    if let Some(ref url) = base_url {
        apply_embedding_env(&mut cmd, url);
    }

    run_with_timeout(cmd, 15)
}

/// Run `openclaw ltm store <text> --category <cat> --importance <n>` — stores a memory directly.
#[tauri::command]
fn memory_store(content: String, category: Option<String>, importance: Option<u32>) -> Result<String, String> {
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
    cmd.args(["ltm", "store", &content]);
    if let Some(ref cat) = category {
        cmd.args(["--category", cat]);
    }
    if let Some(imp) = importance {
        cmd.args(["--importance", &imp.to_string()]);
    }
    if let Some(ref url) = base_url {
        apply_embedding_env(&mut cmd, url);
    }

    run_with_timeout(cmd, 15)
}

/// Write (or merge) a Himalaya TOML config for an IMAP/SMTP email account.
/// Creates ~/.config/himalaya/config.toml with the account settings.
/// If the file already exists, the account is added/replaced while preserving
/// other accounts.
#[tauri::command]
fn write_himalaya_config(
    account_name: String,
    email: String,
    display_name: Option<String>,
    imap_host: String,
    imap_port: u16,
    smtp_host: String,
    smtp_port: u16,
    password: String,
) -> Result<bool, String> {
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    let config_dir = home.join(".config/himalaya");
    let config_path = config_dir.join("config.toml");

    fs::create_dir_all(&config_dir)
        .map_err(|e| format!("Failed to create config dir: {}", e))?;

    // Build TOML for this account
    // Himalaya v1 format: [accounts.<name>]
    let display = display_name.unwrap_or_else(|| email.clone());
    let escaped_pw = password.replace('\'', "'\\''");
    let account_toml = format!(
        r#"[accounts.{name}]
email = "{email}"
display-name = "{display}"

backend.type = "imap"
backend.host = "{imap_host}"
backend.port = {imap_port}
backend.encryption = "tls"
backend.login = "{email}"
backend.auth.type = "password"
backend.auth.raw = "{escaped_pw}"

message.send.backend.type = "smtp"
message.send.backend.host = "{smtp_host}"
message.send.backend.port = {smtp_port}
message.send.backend.encryption = "tls"
message.send.backend.login = "{email}"
message.send.backend.auth.type = "password"
message.send.backend.auth.raw = "{escaped_pw}"
"#,
        name = account_name,
        email = email,
        display = display,
        imap_host = imap_host,
        imap_port = imap_port,
        smtp_host = smtp_host,
        smtp_port = smtp_port,
        escaped_pw = escaped_pw,
    );

    if config_path.exists() {
        // Read existing config and check if this account already exists
        let existing = fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read existing config: {}", e))?;

        // Simple approach: if [accounts.<name>] exists, replace from that header
        // to the next [accounts.*] header or end of file.
        let marker = format!("[accounts.{}]", account_name);
        if let Some(start) = existing.find(&marker) {
            // Find the next [accounts.*] section after this one
            let rest = &existing[start + marker.len()..];
            let end_offset = if let Some(next) = rest.find("\n[accounts.") {
                start + marker.len() + next
            } else {
                existing.len()
            };
            let mut new_content = String::new();
            new_content.push_str(&existing[..start]);
            new_content.push_str(&account_toml);
            new_content.push('\n');
            if end_offset < existing.len() {
                new_content.push_str(&existing[end_offset..]);
            }
            fs::write(&config_path, new_content.trim_end())
                .map_err(|e| format!("Failed to write config: {}", e))?;
        } else {
            // Append new account
            let mut content = existing;
            if !content.ends_with('\n') {
                content.push('\n');
            }
            content.push('\n');
            content.push_str(&account_toml);
            fs::write(&config_path, content.trim_end())
                .map_err(|e| format!("Failed to write config: {}", e))?;
        }
    } else {
        // New file — write account directly
        fs::write(&config_path, account_toml.trim_end())
            .map_err(|e| format!("Failed to write config: {}", e))?;
    }

    // Set owner-only permissions (chmod 600) so other users can't read the password
    set_owner_only_permissions(&config_path)?;

    info!("Wrote himalaya config for account '{}' at {:?} (mode 600)", account_name, config_path);
    Ok(true)
}

/// Read the current Himalaya config to list configured accounts.
/// Passwords are REDACTED before returning to the frontend — the raw
/// credential never leaves the Rust process.
#[tauri::command]
fn read_himalaya_config() -> Result<String, String> {
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    let config_path = home.join(".config/himalaya/config.toml");
    if !config_path.exists() {
        return Ok(String::new());
    }
    let raw = fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read config: {}", e))?;

    // Redact all password values so they never reach the JS layer.
    // Matches lines like:  backend.auth.raw = "secret123"
    let redacted = raw.lines().map(|line| {
        let trimmed = line.trim();
        if trimmed.starts_with("backend.auth.raw")
            || trimmed.starts_with("message.send.backend.auth.raw")
        {
            // Replace the value after '=' with a redacted placeholder
            if let Some(eq) = line.find('=') {
                format!("{} \"••••••••\"", &line[..eq+1])
            } else {
                line.to_string()
            }
        } else {
            line.to_string()
        }
    }).collect::<Vec<_>>().join("\n");

    Ok(redacted)
}

/// Remove a Himalaya account from the config file.
#[tauri::command]
fn remove_himalaya_account(account_name: String) -> Result<bool, String> {
    let home = dirs::home_dir().ok_or("Cannot find home directory")?;
    let config_path = home.join(".config/himalaya/config.toml");
    if !config_path.exists() {
        return Ok(false);
    }

    let existing = fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read config: {}", e))?;

    let marker = format!("[accounts.{}]", account_name);
    if let Some(start) = existing.find(&marker) {
        let rest = &existing[start + marker.len()..];
        let end_offset = if let Some(next) = rest.find("\n[accounts.") {
            start + marker.len() + next
        } else {
            existing.len()
        };
        let mut new_content = String::new();
        new_content.push_str(existing[..start].trim_end());
        if end_offset < existing.len() {
            new_content.push('\n');
            new_content.push_str(existing[end_offset..].trim_start());
        }
        let final_content = new_content.trim().to_string();
        if final_content.is_empty() {
            fs::remove_file(&config_path)
                .map_err(|e| format!("Failed to remove config: {}", e))?;
        } else {
            fs::write(&config_path, final_content)
                .map_err(|e| format!("Failed to write config: {}", e))?;
            set_owner_only_permissions(&config_path)?;
        }
        Ok(true)
    } else {
        Ok(false)
    }
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
            // Fix invalid embedding model — only text-embedding-3-small and
            // text-embedding-3-large are supported by the memory-lancedb plugin.
            let valid_models: std::collections::HashSet<&str> =
                ["text-embedding-3-small", "text-embedding-3-large"].iter().copied().collect();
            if let Some(model_val) = embedding.get("model") {
                let model_str = model_val.as_str().unwrap_or("");
                if !valid_models.contains(model_str) {
                    info!(
                        "Replacing invalid embedding model '{}' with 'text-embedding-3-small'",
                        model_str
                    );
                    embedding.insert(
                        "model".to_string(),
                        serde_json::json!("text-embedding-3-small"),
                    );
                    repaired = true;
                }
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
                    if is_azure_endpoint(&url) {
                        vars.insert("AZURE_OPENAI_ENDPOINT".to_string(), serde_json::json!(&url));
                        vars.insert("OPENAI_API_VERSION".to_string(), serde_json::json!(get_api_version_or_default()));
                    }
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
            test_embedding_connection,
            get_embedding_base_url,
            get_azure_api_version,
            get_embedding_provider,
            memory_stats,
            memory_search,
            memory_store,
            repair_openclaw_config,
            write_himalaya_config,
            read_himalaya_config,
            remove_himalaya_account
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
