// commands/utility.rs — Keyring and weather utility commands.

use crate::atoms::constants::{DB_KEY_SERVICE, DB_KEY_USER};
use log::info;

/// Check whether the OS keychain has a stored password for the given account.
#[tauri::command]
pub fn keyring_has_password(account_name: String, email: String) -> Result<bool, String> {
    let service = format!("paw-mail-{}", account_name);
    let entry = keyring::Entry::new(&service, &email)
        .map_err(|e| format!("Keyring init failed: {}", e))?;
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
    let entry = keyring::Entry::new(&service, &email)
        .map_err(|e| format!("Keyring init failed: {}", e))?;
    match entry.delete_credential() {
        Ok(()) => {
            info!("Deleted keychain entry for '{}' (service={})", email, service);
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
    let entry = keyring::Entry::new(DB_KEY_SERVICE, DB_KEY_USER)
        .map_err(|e| format!("Keyring init failed: {}", e))?;
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
            entry.set_password(&key)
                .map_err(|e| format!("Failed to store DB key: {}", e))?;
            info!("Generated and stored new DB encryption key in OS keychain");
            Ok(key)
        }
        Err(e) => Err(format!("Keyring error: {}", e)),
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

/// Fetch weather data via wttr.in (bypasses the browser CSP for the frontend).
#[tauri::command]
pub async fn fetch_weather(location: Option<String>) -> Result<String, String> {
    let loc = location.unwrap_or_default();
    let url = format!("https://wttr.in/{}?format=j1", loc);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;
    let resp = client
        .get(&url)
        .header("User-Agent", "curl")
        .send()
        .await
        .map_err(|e| format!("Weather fetch failed: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("Weather API returned {}", resp.status()));
    }
    resp.text()
        .await
        .map_err(|e| format!("Failed to read weather response: {}", e))
}
