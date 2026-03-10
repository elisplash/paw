// pawz-code-app — Tauri desktop control panel backend.
//
// Commands exposed to the frontend:
//   get_status              — fetch daemon /status and return JSON
//   load_config             — read ~/.pawz-code/config.toml, return as string
//   save_config(content)    — write updated TOML back to disk
//   open_config_file        — open config in system default editor
//   start_daemon(path)      — spawn daemon detached
//   stop_daemon             — kill daemon process
//   toggle_start_at_login   — install/remove OS service (macOS launchd / Linux systemd)

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ── Shared config type ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default)]
    pub auth_token: String,
    #[serde(default)]
    pub workspace_root: Option<String>,
}

fn default_port() -> u16 {
    3941
}
fn default_bind() -> String {
    "127.0.0.1".into()
}

fn config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".pawz-code")
        .join("config.toml")
}

// ── get_status ────────────────────────────────────────────────────────────────

/// Fetch http://127.0.0.1:<port>/status using the stored auth token.
/// Returns the raw JSON string so the frontend can parse it.
#[tauri::command]
async fn get_status() -> Result<String, String> {
    // Load config to get port + token
    let cfg = read_parsed_config()?;
    let url = format!("http://{}:{}/status", cfg.bind, cfg.port);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", cfg.auth_token))
        .send()
        .await
        .map_err(|e| format!("Cannot reach daemon at {}: {}", url, e))?;

    if !resp.status().is_success() {
        return Err(format!("Daemon returned HTTP {}", resp.status()));
    }

    resp.text()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))
}

// ── load_config ───────────────────────────────────────────────────────────────

/// Read ~/.pawz-code/config.toml and return its raw content as a string.
#[tauri::command]
fn load_config() -> Result<String, String> {
    let path = config_path();
    std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read config {}: {}", path.display(), e))
}

// ── save_config ───────────────────────────────────────────────────────────────

/// Write updated config TOML back to ~/.pawz-code/config.toml.
#[tauri::command]
fn save_config(content: String) -> Result<String, String> {
    // Validate it parses before writing
    toml::from_str::<toml::Value>(&content)
        .map_err(|e| format!("Invalid TOML: {}", e))?;

    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create config dir: {}", e))?;
    }

    std::fs::write(&path, &content)
        .map_err(|e| format!("Cannot write config {}: {}", path.display(), e))?;

    Ok(format!("Config saved to {}", path.display()))
}

// ── open_config_file ──────────────────────────────────────────────────────────

/// Open ~/.pawz-code/config.toml in the system default editor.
#[tauri::command]
fn open_config_file() -> Result<String, String> {
    let path = config_path();

    // Ensure the file exists with minimal defaults if missing
    if !path.exists() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(
            &path,
            "# Pawz CODE configuration\nauth_token = \"\"\nport = 3941\nbind = \"127.0.0.1\"\n",
        );
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("open: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("xdg-open: {}", e))?;
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        return Err("open_config_file: unsupported platform".into());
    }

    Ok(format!("Opened {}", path.display()))
}

// ── start_daemon ──────────────────────────────────────────────────────────────

/// Spawn the pawz-code daemon detached from the control panel.
/// `binary_path` must be an absolute path to the compiled binary.
#[tauri::command]
async fn start_daemon(binary_path: String) -> Result<String, String> {
    let path = PathBuf::from(&binary_path);
    if !path.exists() {
        return Err(format!("Binary not found: {}", binary_path));
    }

    // Ensure log dir exists
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;
    let log_dir = home.join(".pawz-code").join("logs");
    std::fs::create_dir_all(&log_dir)
        .map_err(|e| format!("Cannot create log dir: {}", e))?;

    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("daemon.log"))
        .map_err(|e| format!("Cannot open log file: {}", e))?;

    let stderr_file = log_file
        .try_clone()
        .map_err(|e| format!("Cannot clone log fd: {}", e))?;

    std::process::Command::new(&path)
        .stdout(std::process::Stdio::from(log_file))
        .stderr(std::process::Stdio::from(stderr_file))
        .current_dir(&home)
        // Detach from this process
        .stdin(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to start daemon: {}", e))?;

    Ok(format!("Daemon started: {}", binary_path))
}

// ── stop_daemon ───────────────────────────────────────────────────────────────

/// Find and kill the running pawz-code daemon process.
#[tauri::command]
async fn stop_daemon() -> Result<String, String> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let output = std::process::Command::new("pkill")
            .args(["-f", "pawz-code"])
            .output()
            .map_err(|e| format!("pkill: {}", e))?;

        if output.status.success() {
            return Ok("Daemon stopped.".into());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        let code = output.status.code().unwrap_or(-1);
        if code == 1 {
            // pkill exit 1 = no process matched — already stopped
            return Ok("Daemon was not running.".into());
        }
        return Err(format!("pkill failed (exit {}): {}", code, stderr.trim()));
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    Err("stop_daemon: unsupported platform".into())
}

// ── toggle_start_at_login ─────────────────────────────────────────────────────

/// Install or remove the OS-level startup service for the daemon.
/// `binary_path` is the absolute path to the compiled `pawz-code` binary.
#[tauri::command]
fn toggle_start_at_login(enable: bool, binary_path: String) -> Result<String, String> {
    let home = dirs::home_dir().ok_or("Cannot determine home directory")?;

    #[cfg(target_os = "macos")]
    {
        toggle_launchd(enable, &home, &binary_path)
    }

    #[cfg(target_os = "linux")]
    {
        toggle_systemd(enable, &home, &binary_path)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (enable, binary_path);
        Err("Start-at-login is only supported on macOS and Linux.".into())
    }
}

// ── macOS launchd ─────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn toggle_launchd(enable: bool, home: &Path, binary_path: &str) -> Result<String, String> {
    let agents_dir = home.join("Library").join("LaunchAgents");
    let plist_path = agents_dir.join("io.pawz.pawzcode.plist");
    let label = "io.pawz.pawzcode";

    if enable {
        let path = PathBuf::from(binary_path);
        if !path.exists() {
            return Err(format!("Binary not found: {}", binary_path));
        }

        let log_dir = home.join(".pawz-code").join("logs");
        std::fs::create_dir_all(&log_dir)
            .map_err(|e| format!("Cannot create log dir: {}", e))?;
        std::fs::create_dir_all(&agents_dir)
            .map_err(|e| format!("Cannot create LaunchAgents dir: {}", e))?;

        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>

    <key>ProgramArguments</key>
    <array>
        <string>{binary}</string>
    </array>

    <key>RunAtLoad</key>
    <true/>

    <key>KeepAlive</key>
    <true/>

    <key>StandardOutPath</key>
    <string>{home}/.pawz-code/logs/daemon.log</string>

    <key>StandardErrorPath</key>
    <string>{home}/.pawz-code/logs/daemon.log</string>

    <key>WorkingDirectory</key>
    <string>{home}</string>
</dict>
</plist>
"#,
            label = label,
            binary = binary_path,
            home = home.display(),
        );

        std::fs::write(&plist_path, &plist)
            .map_err(|e| format!("Cannot write plist: {}", e))?;

        let out = std::process::Command::new("launchctl")
            .args(["load", "-w"])
            .arg(&plist_path)
            .output()
            .map_err(|e| format!("launchctl load: {}", e))?;

        if out.status.success() {
            Ok(format!(
                "Start-at-login enabled via launchd.\nPlist: {}",
                plist_path.display()
            ))
        } else {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(format!("launchctl load failed: {}", stderr.trim()))
        }
    } else {
        if plist_path.exists() {
            let _ = std::process::Command::new("launchctl")
                .args(["unload", "-w"])
                .arg(&plist_path)
                .output();

            std::fs::remove_file(&plist_path)
                .map_err(|e| format!("Cannot remove plist: {}", e))?;
        }
        Ok("Start-at-login disabled. Plist removed.".into())
    }
}

// ── Linux systemd user unit ───────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn toggle_systemd(enable: bool, home: &Path, binary_path: &str) -> Result<String, String> {
    let unit_dir = home.join(".config").join("systemd").join("user");
    let unit_path = unit_dir.join("pawz-code.service");

    if enable {
        let path = PathBuf::from(binary_path);
        if !path.exists() {
            return Err(format!("Binary not found: {}", binary_path));
        }

        let log_dir = home.join(".pawz-code").join("logs");
        std::fs::create_dir_all(&log_dir)
            .map_err(|e| format!("Cannot create log dir: {}", e))?;
        std::fs::create_dir_all(&unit_dir)
            .map_err(|e| format!("Cannot create systemd unit dir: {}", e))?;

        let unit = format!(
            "[Unit]\n\
             Description=Pawz CODE daemon — local AI developer agent\n\
             After=network.target\n\
             \n\
             [Service]\n\
             Type=simple\n\
             ExecStart={binary}\n\
             Restart=on-failure\n\
             RestartSec=3\n\
             StandardOutput=append:{home}/.pawz-code/logs/daemon.log\n\
             StandardError=append:{home}/.pawz-code/logs/daemon.log\n\
             WorkingDirectory={home}\n\
             \n\
             [Install]\n\
             WantedBy=default.target\n",
            binary = binary_path,
            home = home.display(),
        );

        std::fs::write(&unit_path, &unit)
            .map_err(|e| format!("Cannot write unit file: {}", e))?;

        let _ = std::process::Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .output();

        let out = std::process::Command::new("systemctl")
            .args(["--user", "enable", "--now", "pawz-code"])
            .output()
            .map_err(|e| format!("systemctl enable: {}", e))?;

        if out.status.success() {
            Ok(format!(
                "Start-at-login enabled via systemd user unit.\nUnit: {}",
                unit_path.display()
            ))
        } else {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(format!("systemctl enable failed: {}", stderr.trim()))
        }
    } else {
        let out = std::process::Command::new("systemctl")
            .args(["--user", "disable", "--now", "pawz-code"])
            .output()
            .map_err(|e| format!("systemctl disable: {}", e))?;

        if unit_path.exists() {
            let _ = std::fs::remove_file(&unit_path);
        }

        if out.status.success() {
            Ok("Start-at-login disabled. Systemd unit removed.".into())
        } else {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Ok(format!(
                "Unit disabled (note: {}). File removed.",
                stderr.trim()
            ))
        }
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn read_parsed_config() -> Result<ParsedConfig, String> {
    let path = config_path();
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read config {}: {}", path.display(), e))?;
    toml::from_str::<ParsedConfig>(&content)
        .map_err(|e| format!("Cannot parse config: {}", e))
}

// ── Tauri entry point ─────────────────────────────────────────────────────────

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_status,
            load_config,
            save_config,
            open_config_file,
            start_daemon,
            stop_daemon,
            toggle_start_at_login,
        ])
        .run(tauri::generate_context!())
        .expect("error running pawz-code app");
}

fn main() {
    run();
}
