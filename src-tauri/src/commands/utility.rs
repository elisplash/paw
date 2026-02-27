// commands/utility.rs — Keyring and weather utility commands.

use crate::atoms::constants::{DB_KEY_SERVICE, DB_KEY_USER};
use log::{error, info};
use serde::Serialize;

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
/// On first call, generates a random key and persists it. Subsequent calls
/// return the same key.
#[tauri::command]
pub fn get_db_encryption_key() -> Result<String, String> {
    let entry = keyring::Entry::new(DB_KEY_SERVICE, DB_KEY_USER).map_err(|e| {
        error!("[keychain] Failed to initialise keyring entry: {}", e);
        format!("Keyring init failed: {}", e)
    })?;
    match entry.get_password() {
        Ok(key) => {
            info!("Retrieved DB encryption key from OS keychain");
            Ok(key)
        }
        Err(keyring::Error::NoEntry) => {
            use rand::Rng;
            let key: String = (0..32)
                .map(|_| format!("{:02x}", rand::thread_rng().gen::<u8>()))
                .collect();
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
#[tauri::command]
pub fn has_db_encryption_key() -> bool {
    keyring::Entry::new(DB_KEY_SERVICE, DB_KEY_USER)
        .ok()
        .and_then(|e| e.get_password().ok())
        .is_some()
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
#[tauri::command]
pub fn check_keychain_health() -> KeychainHealth {
    // Test DB encryption key access
    let db_key_result =
        keyring::Entry::new(DB_KEY_SERVICE, DB_KEY_USER).and_then(|e| e.get_password().map(|_| ()));
    let db_key_ok = match &db_key_result {
        Ok(()) => true,
        Err(keyring::Error::NoEntry) => true, // No entry yet is fine — will be created on first use
        Err(_) => false,
    };

    // Test skill vault key access
    let vault_result = keyring::Entry::new("paw-skill-vault", "encryption-key")
        .and_then(|e| e.get_password().map(|_| ()));
    let vault_key_ok = match &vault_result {
        Ok(()) => true,
        Err(keyring::Error::NoEntry) => true,
        Err(_) => false,
    };

    let (status, message, error) = match (db_key_ok, vault_key_ok) {
        (true, true) => (
            "healthy".to_string(),
            "OS keychain is accessible — all encryption keys protected".to_string(),
            None,
        ),
        (true, false) => {
            let err_msg = format!(
                "Skill vault keychain error: {:?}",
                vault_result.unwrap_err()
            );
            error!("[keychain] {}", err_msg);
            (
                "degraded".to_string(),
                "DB encryption works but skill vault keychain is inaccessible — credential storage blocked".to_string(),
                Some(err_msg),
            )
        }
        (false, true) => {
            let err_msg = format!("DB key keychain error: {:?}", db_key_result.unwrap_err());
            error!("[keychain] {}", err_msg);
            (
                "degraded".to_string(),
                "Skill vault works but DB encryption keychain is inaccessible — field encryption disabled".to_string(),
                Some(err_msg),
            )
        }
        (false, false) => {
            let db_err = format!("{:?}", db_key_result.unwrap_err());
            let vault_err = format!("{:?}", vault_result.unwrap_err());
            let err_msg = format!("DB key: {}; Vault: {}", db_err, vault_err);
            error!("[keychain] OS keychain completely unavailable: {}", err_msg);
            (
                "unavailable".to_string(),
                "OS keychain is completely unavailable — no encryption possible. Install and unlock a keychain provider (GNOME Keyring, KWallet, or macOS Keychain).".to_string(),
                Some(err_msg),
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
        .query(&[("name", query), ("count", "1"), ("language", "en"), ("format", "json")])
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
/// Two-step: geocode the location name → fetch forecast with lat/lon.
#[tauri::command]
pub async fn fetch_weather(location: Option<String>) -> Result<String, String> {
    let loc = location.clone().unwrap_or_default();
    log::info!("[weather] Fetching weather for location: {:?}", location);
    if loc.is_empty() {
        return Err("No location configured — set your city in Settings → Integrations → Weather API".into());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

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

    // Step 2: Fetch current weather from Open-Meteo
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

    // Merge location info into the response
    wx["location"] = serde_json::json!({
        "name": place_name,
        "country": country,
    });

    log::info!("[weather] Successfully fetched weather for {}, {}", place_name, country);
    serde_json::to_string(&wx).map_err(|e| format!("JSON serialization error: {}", e))
}
