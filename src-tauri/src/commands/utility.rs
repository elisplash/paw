// commands/utility.rs — Keyring, lock screen, and weather utility commands.
//
// All long-lived keychain values (DB encryption key, lock passphrase hash)
// are cached in-memory after the first read so the macOS Keychain is only
// accessed once per process session, eliminating repeated system prompts.
//
// Security properties:
//   - `Zeroizing<String>` wraps all cached secrets so they are securely
//     zeroed when dropped or replaced (prevents key material lingering in
//     freed heap memory).
//   - `RwLock` allows concurrent readers (UI + engine threads) without
//     serialisation, while still protecting writes.
//   - Poison recovery via `unwrap_or_else(|e| e.into_inner())` — a panicked
//     thread will not permanently brick the cache.
//   - Passphrase hash comparison uses `subtle::ConstantTimeEq` to resist
//     timing side-channel attacks.

use crate::atoms::constants::{DB_KEY_SERVICE, DB_KEY_USER, LOCK_SERVICE, LOCK_USER};
use log::{error, info};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::sync::RwLock;
use subtle::ConstantTimeEq;
use tauri::Manager;
use zeroize::Zeroizing;

/// In-memory cache for the DB encryption key (hex string).
/// Wrapped in `Zeroizing` so the key material is securely zeroed on drop.
static DB_KEY_CACHE: RwLock<Option<Zeroizing<String>>> = RwLock::new(None);

/// In-memory cache for the lock screen passphrase hash.
/// Updated on set/remove so verify never needs to hit the keychain twice.
/// Wrapped in `Zeroizing` so the hash is securely zeroed on drop.
static LOCK_HASH_CACHE: RwLock<Option<Zeroizing<String>>> = RwLock::new(None);

/// Check whether the OS keychain has a stored password for the given account.
#[tauri::command]
pub fn keyring_has_password(account_name: String, email: String) -> Result<bool, String> {
    let service = format!("paw-mail-{}", account_name);
    let entry =
        keyring::Entry::new(&service, &email).map_err(|e| format!("Keyring init failed: {}", e))?;
    match entry.get_password() {
        Ok(_) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(format!("Keyring error: {}", e)),
    }
}

/// Delete a password from the OS keychain.
#[tauri::command]
pub fn keyring_delete_password(account_name: String, email: String) -> Result<bool, String> {
    let service = format!("paw-mail-{}", account_name);
    let entry =
        keyring::Entry::new(&service, &email).map_err(|e| format!("Keyring init failed: {}", e))?;
    match entry.delete_credential() {
        Ok(()) => {
            info!(
                "Deleted keychain entry for '{}' (service={})",
                email, service
            );
            Ok(true)
        }
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(format!("Keyring delete failed: {}", e)),
    }
}

/// Get or create a 256-bit database encryption key stored in the OS keychain.
/// The result is cached in-memory so the keychain is only accessed once per
/// process session.
#[tauri::command]
pub fn get_db_encryption_key() -> Result<String, String> {
    // Fast path: return cached key (read lock — many readers allowed)
    {
        let guard = DB_KEY_CACHE.read().unwrap_or_else(|e| e.into_inner());
        if let Some(ref key) = *guard {
            return Ok(key.to_string());
        }
    }
    // Slow path: acquire write lock and double-check (prevents TOCTOU race)
    let mut guard = DB_KEY_CACHE.write().unwrap_or_else(|e| e.into_inner());
    if let Some(ref key) = *guard {
        return Ok(key.to_string());
    }
    let key = load_db_key_from_keychain()?;
    if key.len() < 32 {
        error!("[keychain] DB key too short: {} chars (min 32)", key.len());
        return Err("DB encryption key from keychain is too short".into());
    }
    let result = key.to_string();
    *guard = Some(key);
    info!("[keychain] DB encryption key loaded and cached");
    Ok(result)
}

/// Read (or create) the DB encryption key directly from the OS keychain.
/// Returns `Zeroizing<String>` so the key is securely zeroed when dropped.
fn load_db_key_from_keychain() -> Result<Zeroizing<String>, String> {
    let entry = keyring::Entry::new(DB_KEY_SERVICE, DB_KEY_USER).map_err(|e| {
        error!("[keychain] Failed to initialise keyring entry: {}", e);
        format!("Keyring init failed: {}", e)
    })?;
    match entry.get_password() {
        Ok(key) => {
            info!("Retrieved DB encryption key from OS keychain");
            Ok(Zeroizing::new(key))
        }
        Err(keyring::Error::NoEntry) => {
            // Generate a new random key using OS CSPRNG (not thread_rng)
            use rand::rngs::OsRng;
            use rand::RngCore;
            let mut bytes = [0u8; 32];
            OsRng.fill_bytes(&mut bytes);
            let key = Zeroizing::new(
                bytes
                    .iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<String>(),
            );
            // Zero the raw bytes immediately
            zeroize::Zeroize::zeroize(&mut bytes);
            entry.set_password(&key).map_err(|e| {
                error!("[keychain] Failed to store DB encryption key: {}", e);
                format!("Failed to store DB key: {}", e)
            })?;
            info!("Generated and stored new DB encryption key in OS keychain");
            Ok(key)
        }
        Err(e) => {
            error!("[keychain] Failed to retrieve DB encryption key: {}", e);
            Err(format!("Keyring error: {}", e))
        }
    }
}

/// Check if a DB encryption key exists (for UI indicators).
/// Uses the in-memory cache when available to avoid keychain access.
#[tauri::command]
pub fn has_db_encryption_key() -> bool {
    // Check cache first (read lock, poison-safe)
    if DB_KEY_CACHE
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .is_some()
    {
        return true;
    }
    keyring::Entry::new(DB_KEY_SERVICE, DB_KEY_USER)
        .ok()
        .and_then(|e| e.get_password().ok())
        .is_some()
}

// ── Lock Screen Passphrase ─────────────────────────────────────────────────
// The passphrase is SHA-256 hashed before storing in the keychain. The raw
// passphrase never leaves the WebView→Rust boundary. On verify, we hash the
// input and compare with the stored hash.

fn hash_passphrase(passphrase: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(passphrase.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Check if a lock screen passphrase has been configured.
/// Uses the in-memory cache when available.
#[tauri::command]
pub fn lock_screen_has_passphrase() -> bool {
    // Check cache first (read lock, poison-safe)
    if LOCK_HASH_CACHE
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .is_some()
    {
        return true;
    }
    // Fall through to keychain (populates the cache on success)
    let result = keyring::Entry::new(LOCK_SERVICE, LOCK_USER)
        .ok()
        .and_then(|e| e.get_password().ok());
    if let Some(ref hash) = result {
        let mut guard = LOCK_HASH_CACHE.write().unwrap_or_else(|e| e.into_inner());
        *guard = Some(Zeroizing::new(hash.clone()));
    }
    result.is_some()
}

/// Set (or replace) the lock screen passphrase. Stores SHA-256 hash in keychain.
/// Also updates the in-memory cache.
#[tauri::command]
pub fn lock_screen_set_passphrase(passphrase: String) -> Result<(), String> {
    if passphrase.len() < 4 {
        return Err("Passphrase must be at least 4 characters".into());
    }
    let hash = hash_passphrase(&passphrase);
    let entry = keyring::Entry::new(LOCK_SERVICE, LOCK_USER)
        .map_err(|e| format!("Keyring init failed: {}", e))?;
    entry
        .set_password(&hash)
        .map_err(|e| format!("Failed to store passphrase: {}", e))?;
    // Update cache (write lock, poison-safe, zeroized)
    *LOCK_HASH_CACHE.write().unwrap_or_else(|e| e.into_inner()) = Some(Zeroizing::new(hash));
    info!("[lock] Passphrase set in OS keychain");
    Ok(())
}

/// Verify a passphrase against the stored hash.
/// Uses the in-memory cache when available — the keychain is only read once
/// per session.
#[tauri::command]
pub fn lock_screen_verify_passphrase(passphrase: String) -> Result<bool, String> {
    let input_hash = hash_passphrase(&passphrase);

    // Try cache first (read lock, poison-safe)
    {
        let guard = LOCK_HASH_CACHE.read().unwrap_or_else(|e| e.into_inner());
        if let Some(ref stored_hash) = *guard {
            // Constant-time comparison to prevent timing side-channel attacks
            return Ok(stored_hash.as_bytes().ct_eq(input_hash.as_bytes()).into());
        }
    }

    // Fall through to keychain
    let entry = keyring::Entry::new(LOCK_SERVICE, LOCK_USER)
        .map_err(|e| format!("Keyring init failed: {}", e))?;
    match entry.get_password() {
        Ok(stored_hash) => {
            // Constant-time comparison
            let matches: bool = stored_hash.as_bytes().ct_eq(input_hash.as_bytes()).into();
            // Populate cache (write lock, poison-safe, zeroized)
            *LOCK_HASH_CACHE.write().unwrap_or_else(|e| e.into_inner()) =
                Some(Zeroizing::new(stored_hash));
            Ok(matches)
        }
        Err(keyring::Error::NoEntry) => Err("No passphrase configured".into()),
        Err(e) => Err(format!("Keyring error: {}", e)),
    }
}

/// Remove the lock screen passphrase (disable lock screen).
/// Also clears the in-memory cache.
#[tauri::command]
pub fn lock_screen_remove_passphrase() -> Result<(), String> {
    let entry = keyring::Entry::new(LOCK_SERVICE, LOCK_USER)
        .map_err(|e| format!("Keyring init failed: {}", e))?;
    let result = match entry.delete_credential() {
        Ok(()) => {
            info!("[lock] Passphrase removed from OS keychain");
            Ok(())
        }
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Failed to remove passphrase: {}", e)),
    };
    // Clear cache regardless of outcome (write lock, poison-safe)
    // The old Zeroizing<String> is dropped here, securely zeroing the hash.
    *LOCK_HASH_CACHE.write().unwrap_or_else(|e| e.into_inner()) = None;
    result
}

// ── System Authentication (macOS Touch ID / device password) ───────────────
// Uses the LocalAuthentication framework via osascript/JXA. This triggers
// the native Touch ID dialog or falls back to the Mac login password.
// Policy 1 = LAPolicyDeviceOwnerAuthentication (biometric + passcode fallback).

/// Trigger macOS system authentication (Touch ID / login password).
/// Returns true on success, false if the user cancelled or failed.
#[tauri::command]
pub async fn lock_screen_system_auth() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        let script = r#"
ObjC.import('LocalAuthentication');
ObjC.import('Foundation');
var ctx = $.LAContext.alloc.init;
var error = Ref();
var canEval = ctx.canEvaluatePolicyError(2, error);
if (!canEval) {
    'unavailable';
} else {
    var done = false;
    var success = false;
    ctx.evaluatePolicyLocalizedReasonReply(
        2,
        'Verify your identity to open OpenPawz',
        function(s, e) { success = s; done = true; }
    );
    while (!done) {
        $.NSRunLoop.currentRunLoop.runUntilDate(
            $.NSDate.dateWithTimeIntervalSinceNow(0.1)
        );
    }
    success ? 'ok' : 'denied';
}"#;
        let output = tokio::process::Command::new("osascript")
            .args(["-l", "JavaScript", "-e", script])
            .output()
            .await
            .map_err(|e| format!("Failed to run system auth: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        match stdout.as_str() {
            "ok" => {
                info!("[lock] System authentication successful");
                Ok(true)
            }
            "denied" => Ok(false),
            "unavailable" => Err("System authentication not available on this device".into()),
            _ => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                error!(
                    "[lock] System auth unexpected result: {} {}",
                    stdout, stderr
                );
                Err(format!("Authentication error: {}", stderr.trim()))
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err("System authentication is only available on macOS".into())
    }
}

/// Check if system authentication (Touch ID / device password) is available.
#[tauri::command]
pub fn lock_screen_system_available() -> bool {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("osascript")
            .args([
                "-l",
                "JavaScript",
                "-e",
                r#"ObjC.import('LocalAuthentication');var c=$.LAContext.alloc.init;c.canEvaluatePolicyError(2,Ref())?'yes':'no'"#,
            ])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "yes")
            .unwrap_or(false)
    }

    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Detailed keychain health status for the Settings → Security panel.
#[derive(Serialize, Clone)]
pub struct KeychainHealth {
    /// Overall status: "healthy", "degraded", or "unavailable"
    pub status: String,
    /// Whether the DB encryption keychain entry is accessible
    pub db_key_ok: bool,
    /// Whether the skill vault keychain entry is accessible
    pub vault_key_ok: bool,
    /// Human-readable summary
    pub message: String,
    /// Error detail (if any)
    pub error: Option<String>,
}

/// Check health of all keychain entries used by Paw.
/// Tests both the DB encryption key and the skill vault key.
/// Uses in-memory caches when available to avoid extra keychain prompts.
#[tauri::command]
pub fn check_keychain_health() -> KeychainHealth {
    // Test DB encryption key access — use cache when available (read lock, poison-safe)
    let db_key_ok = if DB_KEY_CACHE
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .is_some()
    {
        true
    } else {
        match keyring::Entry::new(DB_KEY_SERVICE, DB_KEY_USER)
            .and_then(|e| e.get_password().map(|_| ()))
        {
            Ok(()) => true,
            Err(keyring::Error::NoEntry) => true, // No entry yet is fine
            Err(_) => false,
        }
    };

    // Test skill vault key access — check if get_vault_key() has been cached
    let vault_key_ok = if crate::engine::skills::crypto::vault_key_cached() {
        true
    } else {
        match keyring::Entry::new("paw-skill-vault", "encryption-key")
            .and_then(|e| e.get_password().map(|_| ()))
        {
            Ok(()) => true,
            Err(keyring::Error::NoEntry) => true,
            Err(_) => false,
        }
    };

    let (status, message, error) = match (db_key_ok, vault_key_ok) {
        (true, true) => (
            "healthy".to_string(),
            "OS keychain is accessible — all encryption keys protected".to_string(),
            None,
        ),
        (true, false) => {
            error!("[keychain] Skill vault keychain not accessible");
            (
                "degraded".to_string(),
                "DB encryption works but skill vault keychain is inaccessible — credential storage blocked".to_string(),
                Some("Skill vault keychain error".to_string()),
            )
        }
        (false, true) => {
            error!("[keychain] DB key keychain not accessible");
            (
                "degraded".to_string(),
                "Skill vault works but DB encryption keychain is inaccessible — field encryption disabled".to_string(),
                Some("DB key keychain error".to_string()),
            )
        }
        (false, false) => {
            error!("[keychain] OS keychain completely unavailable");
            (
                "unavailable".to_string(),
                "OS keychain is completely unavailable — no encryption possible. Install and unlock a keychain provider (GNOME Keyring, KWallet, or macOS Keychain).".to_string(),
                Some("Both DB key and vault keychain inaccessible".to_string()),
            )
        }
    };

    KeychainHealth {
        status,
        db_key_ok,
        vault_key_ok,
        message,
        error,
    }
}

/// Geocode a location string via Open-Meteo. Tries the full input first,
/// then falls back to just the city name (before the first comma) since
/// Open-Meteo doesn't understand "City, State" format well.
pub async fn geocode_location(
    client: &reqwest::Client,
    location: &str,
) -> Result<serde_json::Value, String> {
    // Try full input first
    let geo = _geocode_query(client, location).await?;
    if let Some(place) = geo["results"].get(0) {
        return Ok(place.clone());
    }

    // Fallback: try just the city name (before the comma)
    if let Some(city) = location.split(',').next() {
        let city = city.trim();
        if city != location {
            log::info!("[weather] Retrying geocode with city-only: {}", city);
            let geo2 = _geocode_query(client, city).await?;
            if let Some(place) = geo2["results"].get(0) {
                return Ok(place.clone());
            }
        }
    }

    Err(format!("Location not found: {}", location))
}

pub async fn _geocode_query(
    client: &reqwest::Client,
    query: &str,
) -> Result<serde_json::Value, String> {
    let resp = client
        .get("https://geocoding-api.open-meteo.com/v1/search")
        .query(&[
            ("name", query),
            ("count", "1"),
            ("language", "en"),
            ("format", "json"),
        ])
        .send()
        .await
        .map_err(|e| format!("Geocoding failed: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("Geocoding API returned {}", resp.status()));
    }
    let text = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read geocoding response: {}", e))?;
    serde_json::from_str(&text).map_err(|e| format!("Invalid geocoding JSON: {}", e))
}

/// Fetch weather data via Open-Meteo (free, no API key, reliable).
///
/// Location priority:
///   1. `config.weather_location` (user-configured)
///   2. Legacy integration credentials (`weather-api`)
///   3. IP geolocation auto-detect
///
/// Two-step: geocode location → fetch forecast with lat/lon.
#[tauri::command]
pub async fn fetch_weather(app_handle: tauri::AppHandle) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    // 1. Try config.weather_location
    let mut loc = String::new();
    if let Some(state) = app_handle.try_state::<crate::engine::state::EngineState>() {
        let cfg = state.config.lock();
        if let Some(ref wl) = cfg.weather_location {
            if !wl.is_empty() {
                loc = wl.clone();
            }
        }
    }

    // 2. Legacy: try integration credentials
    if loc.is_empty() {
        if let Ok(creds) = crate::engine::channels::load_channel_config::<
            std::collections::HashMap<String, String>,
        >(&app_handle, "integration_creds_weather-api")
        {
            if let Some(l) = creds.get("location") {
                if !l.is_empty() {
                    loc = l.clone();
                    log::info!(
                        "[weather] Using legacy integration credential location: {}",
                        loc
                    );
                    // Migrate to config.weather_location for next time
                    if let Some(state) = app_handle.try_state::<crate::engine::state::EngineState>()
                    {
                        let mut cfg = state.config.lock();
                        cfg.weather_location = Some(loc.clone());
                        drop(cfg);
                    }
                }
            }
        }
    }

    // 3. Auto-detect via IP geolocation
    if loc.is_empty() {
        log::info!("[weather] No location configured — auto-detecting via IP");
        match auto_detect_location(&client).await {
            Ok((lat, lon, city, country)) => {
                log::info!(
                    "[weather] Auto-detected location: {}, {} ({}, {})",
                    city,
                    country,
                    lat,
                    lon
                );
                // Go straight to weather fetch with known coords
                return fetch_weather_by_coords(&client, lat, lon, &city, &country).await;
            }
            Err(e) => {
                log::warn!("[weather] IP geolocation failed: {}", e);
                return Err(
                    "No location set. Click the location on your dashboard to set your city."
                        .into(),
                );
            }
        }
    }

    log::info!("[weather] Fetching weather for location: {}", loc);

    // Step 1: Geocode the location name to lat/lon
    let place = geocode_location(&client, &loc).await?;
    let lat = place["latitude"]
        .as_f64()
        .ok_or("Missing latitude in geocoding result")?;
    let lon = place["longitude"]
        .as_f64()
        .ok_or("Missing longitude in geocoding result")?;
    let place_name = place["name"].as_str().unwrap_or("");
    let country = place["country"].as_str().unwrap_or("");

    fetch_weather_by_coords(&client, lat, lon, place_name, country).await
}

/// Fetch weather from Open-Meteo using known lat/lon coordinates.
async fn fetch_weather_by_coords(
    client: &reqwest::Client,
    lat: f64,
    lon: f64,
    place_name: &str,
    country: &str,
) -> Result<String, String> {
    let weather_url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&current=temperature_2m,apparent_temperature,weather_code,wind_speed_10m,relative_humidity_2m&wind_speed_unit=kmh",
        lat, lon
    );
    let wx_resp = client
        .get(&weather_url)
        .send()
        .await
        .map_err(|e| format!("Weather fetch failed: {}", e))?;
    if !wx_resp.status().is_success() {
        return Err(format!("Weather API returned {}", wx_resp.status()));
    }
    let wx_text = wx_resp
        .text()
        .await
        .map_err(|e| format!("Failed to read weather response: {}", e))?;
    let mut wx: serde_json::Value =
        serde_json::from_str(&wx_text).map_err(|e| format!("Invalid weather JSON: {}", e))?;

    wx["location"] = serde_json::json!({
        "name": place_name,
        "country": country,
    });

    log::info!(
        "[weather] Successfully fetched weather for {}, {}",
        place_name,
        country
    );
    serde_json::to_string(&wx).map_err(|e| format!("JSON serialization error: {}", e))
}

/// Auto-detect user location via IP geolocation (ipapi.co — free, no key).
/// Returns (lat, lon, city, country).
async fn auto_detect_location(
    client: &reqwest::Client,
) -> Result<(f64, f64, String, String), String> {
    let resp = client
        .get("https://ipapi.co/json/")
        .header("User-Agent", "OpenPawz/1.0")
        .send()
        .await
        .map_err(|e| format!("IP geolocation failed: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("IP geolocation returned {}", resp.status()));
    }
    let text = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read IP geo response: {}", e))?;
    let data: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Invalid IP geo JSON: {}", e))?;

    let lat = data["latitude"]
        .as_f64()
        .ok_or("IP geolocation missing latitude")?;
    let lon = data["longitude"]
        .as_f64()
        .ok_or("IP geolocation missing longitude")?;
    let city = data["city"].as_str().unwrap_or("Unknown").to_string();
    let country = data["country_name"].as_str().unwrap_or("").to_string();
    Ok((lat, lon, city, country))
}
