// Paw Commands — Tailscale Remote Access
//
// Provides Tailscale Serve / Funnel integration so agents can
// be reached from anywhere on your tailnet or the public internet.
// The CLI is invoked via std::process::Command — no external crates needed.

use crate::engine::state::EngineState;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::process::Command;
use tauri::State;

// ── Types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TailscaleStatus {
    pub installed: bool,
    pub running: bool,
    pub hostname: String,
    pub tailnet: String,
    pub ip: String,
    pub version: String,
    pub serve_active: bool,
    pub funnel_active: bool,
    pub serve_url: String,
    pub funnel_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TailscaleConfig {
    pub enabled: bool,
    pub serve_port: u16,
    pub funnel_enabled: bool,
    pub auth_key: String,
    pub hostname_override: String,
}

impl Default for TailscaleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            serve_port: 3100,
            funnel_enabled: false,
            auth_key: String::new(),
            hostname_override: String::new(),
        }
    }
}

// ── Commands ───────────────────────────────────────────────────────────

/// Get Tailscale status: installed, running, IPs, serve/funnel state.
#[tauri::command]
pub fn engine_tailscale_status() -> Result<TailscaleStatus, String> {
    let installed = which_tailscale().is_some();
    if !installed {
        return Ok(TailscaleStatus {
            installed: false, running: false,
            hostname: String::new(), tailnet: String::new(),
            ip: String::new(), version: String::new(),
            serve_active: false, funnel_active: false,
            serve_url: String::new(), funnel_url: String::new(),
        });
    }

    // tailscale status --json
    let status_json = run_ts_cmd(&["status", "--json"]).unwrap_or_default();
    let status: serde_json::Value = serde_json::from_str(&status_json).unwrap_or_default();

    let running = status.get("BackendState")
        .and_then(|v| v.as_str())
        .map(|s| s == "Running")
        .unwrap_or(false);

    let hostname = status.get("Self")
        .and_then(|s| s.get("HostName"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let tailnet = status.get("CurrentTailnet")
        .and_then(|t| t.get("Name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let ip = status.get("TailscaleIPs")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let version = run_ts_cmd(&["version"]).unwrap_or_default().trim().to_string();

    // Check serve/funnel state
    let serve_json = run_ts_cmd(&["serve", "status", "--json"]).unwrap_or_default();
    let serve_status: serde_json::Value = serde_json::from_str(&serve_json).unwrap_or_default();

    let serve_active = serve_status.get("TCP").or(serve_status.get("Web"))
        .map(|v| !v.is_null() && v.as_object().map_or(false, |o| !o.is_empty()))
        .unwrap_or(false);

    let funnel_active = serve_status.get("AllowFunnel")
        .and_then(|v| v.as_object())
        .map(|m| !m.is_empty())
        .unwrap_or(false);

    let dns_name = status.get("Self")
        .and_then(|s| s.get("DNSName"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim_end_matches('.')
        .to_string();

    let serve_url = if serve_active && !dns_name.is_empty() {
        format!("https://{}", dns_name)
    } else { String::new() };

    let funnel_url = if funnel_active && !dns_name.is_empty() {
        format!("https://{} (public)", dns_name)
    } else { String::new() };

    Ok(TailscaleStatus {
        installed, running, hostname, tailnet, ip, version,
        serve_active, funnel_active, serve_url, funnel_url,
    })
}

/// Get persisted Tailscale config.
#[tauri::command]
pub fn engine_tailscale_get_config(state: State<'_, EngineState>) -> Result<TailscaleConfig, String> {
    match state.store.get_config("tailscale_config") {
        Ok(Some(json)) => serde_json::from_str(&json).map_err(|e| format!("Parse error: {}", e)),
        _ => Ok(TailscaleConfig::default()),
    }
}

/// Persist Tailscale config.
#[tauri::command]
pub fn engine_tailscale_set_config(
    state: State<'_, EngineState>,
    config: TailscaleConfig,
) -> Result<(), String> {
    let json = serde_json::to_string(&config).map_err(|e| format!("Serialize error: {}", e))?;
    state.store.set_config("tailscale_config", &json).map_err(|e| e.to_string())
}

/// Start Tailscale Serve on the configured port.
#[tauri::command]
pub fn engine_tailscale_serve_start(
    state: State<'_, EngineState>,
    port: Option<u16>,
) -> Result<String, String> {
    let config = engine_tailscale_get_config_inner(&state)?;
    let p = port.unwrap_or(config.serve_port);

    info!("[tailscale] Starting serve on port {}", p);
    let target = format!("http://localhost:{}", p);
    let output = run_ts_cmd(&["serve", "--bg", &target])
        .map_err(|e| format!("Tailscale serve failed: {}", e))?;

    Ok(output)
}

/// Stop Tailscale Serve.
#[tauri::command]
pub fn engine_tailscale_serve_stop() -> Result<String, String> {
    info!("[tailscale] Stopping serve");
    run_ts_cmd(&["serve", "--bg", "off"])
        .map_err(|e| format!("Tailscale serve stop failed: {}", e))
}

/// Enable Tailscale Funnel (public internet access).
#[tauri::command]
pub fn engine_tailscale_funnel_start(
    state: State<'_, EngineState>,
    port: Option<u16>,
) -> Result<String, String> {
    let config = engine_tailscale_get_config_inner(&state)?;
    let p = port.unwrap_or(config.serve_port);

    info!("[tailscale] Starting funnel on port {}", p);
    let target = format!("http://localhost:{}", p);
    let output = run_ts_cmd(&["funnel", "--bg", &target])
        .map_err(|e| format!("Tailscale funnel failed: {}", e))?;

    Ok(output)
}

/// Disable Tailscale Funnel.
#[tauri::command]
pub fn engine_tailscale_funnel_stop() -> Result<String, String> {
    info!("[tailscale] Stopping funnel");
    run_ts_cmd(&["funnel", "--bg", "off"])
        .map_err(|e| format!("Tailscale funnel stop failed: {}", e))
}

/// Connect Tailscale (start the daemon with optional auth key).
#[tauri::command]
pub fn engine_tailscale_connect(auth_key: Option<String>) -> Result<String, String> {
    info!("[tailscale] Connecting...");
    let mut args = vec!["up"];
    let key_flag;
    if let Some(ref key) = auth_key {
        if !key.is_empty() {
            key_flag = format!("--authkey={}", key);
            args.push(&key_flag);
        }
    }
    run_ts_cmd(&args).map_err(|e| format!("Tailscale connect failed: {}", e))
}

/// Disconnect Tailscale.
#[tauri::command]
pub fn engine_tailscale_disconnect() -> Result<String, String> {
    info!("[tailscale] Disconnecting...");
    run_ts_cmd(&["down"]).map_err(|e| format!("Tailscale disconnect failed: {}", e))
}

// ── Helpers ────────────────────────────────────────────────────────────

fn engine_tailscale_get_config_inner(state: &EngineState) -> Result<TailscaleConfig, String> {
    match state.store.get_config("tailscale_config") {
        Ok(Some(json)) => serde_json::from_str(&json).map_err(|e| format!("Parse error: {}", e)),
        _ => Ok(TailscaleConfig::default()),
    }
}

/// Find the tailscale binary.
fn which_tailscale() -> Option<String> {
    // Common locations
    for path in &[
        "/usr/bin/tailscale",
        "/usr/local/bin/tailscale",
        "/usr/sbin/tailscale",
        "/opt/homebrew/bin/tailscale",
    ] {
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
    }
    // Try PATH via `which`
    Command::new("which")
        .arg("tailscale")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Run a tailscale CLI subcommand.
fn run_ts_cmd(args: &[&str]) -> Result<String, String> {
    let bin = which_tailscale().ok_or("Tailscale not found. Install from https://tailscale.com/download")?;
    let output = Command::new(&bin)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run tailscale: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        warn!("[tailscale] Command failed: tailscale {} → {}", args.join(" "), stderr);
        Err(stderr)
    }
}
