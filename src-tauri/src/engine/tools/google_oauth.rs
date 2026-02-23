// Paw Agent Engine — Google OAuth2 (Desktop App Flow)
//
// One-click "Connect with Google" for personal Gmail and Workspace accounts.
//
// Flow:
//   1. Pawz spins up an ephemeral localhost HTTP listener on a random port
//   2. Opens the user's browser to Google's consent screen
//   3. User signs in and clicks "Allow"
//   4. Google redirects to http://127.0.0.1:{port}?code=...
//   5. Pawz captures the auth code, exchanges it for access + refresh tokens
//   6. Tokens are encrypted and stored in the skill vault
//   7. Listener shuts down
//
// Token refresh:
//   - Access tokens expire after 1 hour
//   - Pawz auto-refreshes using the stored refresh_token
//   - No user interaction needed after initial connect

use crate::atoms::error::EngineResult;
use crate::commands::state::EngineState;
use crate::engine::skills;
use log::info;
use tauri::Manager;
use tauri_plugin_opener::OpenerExt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Google OAuth2 scopes — same as the service account flow.
const SCOPES: &str = "https://www.googleapis.com/auth/gmail.modify \
    https://www.googleapis.com/auth/gmail.send \
    https://www.googleapis.com/auth/calendar \
    https://www.googleapis.com/auth/drive \
    https://www.googleapis.com/auth/spreadsheets \
    https://www.googleapis.com/auth/documents \
    https://www.googleapis.com/auth/userinfo.email";

const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const USERINFO_ENDPOINT: &str = "https://www.googleapis.com/oauth2/v3/userinfo";

/// HTML that gets shown in the browser after successful auth.
const SUCCESS_HTML: &str = r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"><title>Pawz Connected!</title>
<style>
  body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
    display: flex; justify-content: center; align-items: center; min-height: 100vh;
    margin: 0; background: #1a1a2e; color: #e0e0e0; }
  .card { text-align: center; padding: 3rem; border-radius: 16px;
    background: #16213e; box-shadow: 0 8px 32px rgba(0,0,0,0.3); max-width: 420px; }
  h1 { font-size: 2.5rem; margin: 0 0 0.5rem; }
  p { font-size: 1.1rem; color: #a0a0b0; margin: 0.5rem 0; }
  .check { font-size: 4rem; margin-bottom: 1rem; }
  .hint { font-size: 0.9rem; color: #707080; margin-top: 1.5rem; }
</style></head>
<body><div class="card">
  <div class="check">✅</div>
  <h1>Connected!</h1>
  <p>Pawz now has access to your Google account.</p>
  <p class="hint">You can close this tab and return to Pawz.</p>
</div></body></html>"#;

/// HTML for error state.
const ERROR_HTML: &str = r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"><title>Connection Failed</title>
<style>
  body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
    display: flex; justify-content: center; align-items: center; min-height: 100vh;
    margin: 0; background: #1a1a2e; color: #e0e0e0; }
  .card { text-align: center; padding: 3rem; border-radius: 16px;
    background: #16213e; box-shadow: 0 8px 32px rgba(0,0,0,0.3); max-width: 420px; }
  h1 { font-size: 2rem; margin: 0 0 0.5rem; }
  p { font-size: 1.1rem; color: #a0a0b0; }
  .icon { font-size: 4rem; margin-bottom: 1rem; }
</style></head>
<body><div class="card">
  <div class="icon">❌</div>
  <h1>Connection Failed</h1>
  <p>Something went wrong. Please try again from Pawz.</p>
  <p>ERROR_DETAILS</p>
</div></body></html>"#;

/// Run the full OAuth2 flow: open browser → capture code → exchange → store.
/// Returns the connected email address on success.
pub async fn run_oauth_flow(app_handle: &tauri::AppHandle) -> EngineResult<String> {
    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    // Get client ID/secret from credentials (user pastes these from Google Cloud Console)
    let vault_key = skills::get_vault_key().map_err(|e| format!("Vault key error: {}", e))?;

    let client_id = get_decrypted_cred(&state, &vault_key, "GOOGLE_CLIENT_ID")
        .ok_or("Missing GOOGLE_CLIENT_ID. Paste your OAuth Client ID in the Google Workspace skill settings.")?;
    let client_secret = get_decrypted_cred(&state, &vault_key, "GOOGLE_CLIENT_SECRET")
        .ok_or("Missing GOOGLE_CLIENT_SECRET. Paste your OAuth Client Secret in the Google Workspace skill settings.")?;

    // 1. Spin up ephemeral localhost listener
    let listener = TcpListener::bind("127.0.0.1:0").await
        .map_err(|e| format!("Failed to bind localhost listener: {}", e))?;
    let port = listener.local_addr()
        .map_err(|e| format!("Failed to get listener port: {}", e))?
        .port();

    let redirect_uri = format!("http://127.0.0.1:{}", port);
    info!("[google-oauth] Listening on {}", redirect_uri);

    // 2. Build the authorization URL
    let auth_url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent",
        AUTH_ENDPOINT,
        url_encode(&client_id),
        url_encode(&redirect_uri),
        url_encode(SCOPES),
    );

    // 3. Open the user's browser
    info!("[google-oauth] Opening browser for consent");
    app_handle.opener().open_url(&auth_url, None::<&str>)
        .map_err(|e| format!("Failed to open browser: {}", e))?;

    // 4. Wait for the callback (with timeout)
    let code = wait_for_callback(listener).await?;
    info!("[google-oauth] Received authorization code");

    // 5. Exchange the code for tokens
    let tokens = exchange_code(&code, &client_id, &client_secret, &redirect_uri).await?;

    let access_token = tokens["access_token"].as_str()
        .ok_or("Token response missing 'access_token'")?;
    let refresh_token = tokens["refresh_token"].as_str()
        .ok_or("Token response missing 'refresh_token'. Try again — make sure to click 'Allow' on the consent screen.")?;
    let expires_in = tokens["expires_in"].as_u64().unwrap_or(3600);

    // 6. Get the user's email address
    let email = get_user_email(access_token).await.unwrap_or_else(|_| "unknown".to_string());
    info!("[google-oauth] Connected as: {}", email);

    // 7. Store tokens in skill vault (encrypted)
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let expires_at = (now + expires_in).to_string();

    store_cred(&state, &vault_key, "GOOGLE_ACCESS_TOKEN", access_token)?;
    store_cred(&state, &vault_key, "GOOGLE_REFRESH_TOKEN", refresh_token)?;
    store_cred(&state, &vault_key, "GOOGLE_TOKEN_EXPIRES_AT", &expires_at)?;
    store_cred(&state, &vault_key, "GOOGLE_USER_EMAIL", &email)?;

    // Auto-enable the skill
    state.store.set_skill_enabled("google_workspace", true)
        .map_err(|e| e.to_string())?;

    info!("[google-oauth] Tokens stored, skill enabled");
    Ok(email)
}

/// Wait for Google's redirect to hit our localhost listener.
/// Extracts the `code` parameter from the query string.
async fn wait_for_callback(listener: TcpListener) -> EngineResult<String> {
    // 2-minute timeout for the user to complete the consent flow
    let timeout = tokio::time::timeout(Duration::from_secs(120), async {
        let (mut stream, _addr) = listener.accept().await
            .map_err(|e| format!("Accept failed: {}", e))?;

        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await
            .map_err(|e| format!("Read failed: {}", e))?;
        let request = String::from_utf8_lossy(&buf[..n]).to_string();

        // Parse the GET request line: "GET /?code=XXXX&scope=... HTTP/1.1"
        let path = request.lines().next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("");

        // Check for error
        if let Some(error) = extract_param(path, "error") {
            let error_html = ERROR_HTML.replace("ERROR_DETAILS", &error);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                error_html.len(), error_html
            );
            let _ = stream.write_all(response.as_bytes()).await;
            return Err(format!("Google auth error: {}", error).into());
        }

        // Extract the authorization code
        let code = extract_param(path, "code")
            .ok_or("No authorization code in callback. The user may have denied access.")?;

        // Send success page
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            SUCCESS_HTML.len(), SUCCESS_HTML
        );
        let _ = stream.write_all(response.as_bytes()).await;

        Ok(code)
    });

    timeout.await
        .map_err(|_| "Timed out waiting for Google authorization (2 minutes). Please try again.")?
}

/// Extract a query parameter from a URL path.
fn extract_param(path: &str, key: &str) -> Option<String> {
    let query = path.split('?').nth(1)?;
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        if parts.next() == Some(key) {
            return parts.next().map(url_decode);
        }
    }
    None
}

fn url_encode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

fn url_decode(s: &str) -> String {
    url::form_urlencoded::parse(s.as_bytes())
        .map(|(k, v)| if v.is_empty() { k.to_string() } else { format!("{}={}", k, v) })
        .next()
        .unwrap_or_else(|| s.to_string())
}

/// Exchange an authorization code for access and refresh tokens.
async fn exchange_code(
    code: &str,
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
) -> EngineResult<serde_json::Value> {
    let client = reqwest::Client::new();
    let resp = client.post(TOKEN_ENDPOINT)
        .form(&[
            ("code", code),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
        ])
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Token exchange failed: {}", e))?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(format!("Token exchange failed ({}): {}", status, body).into());
    }

    serde_json::from_str(&body)
        .map_err(|e| format!("Invalid token response: {}", e).into())
}

/// Refresh an access token using a refresh token.
pub async fn refresh_access_token(
    refresh_token: &str,
    client_id: &str,
    client_secret: &str,
) -> EngineResult<(String, u64)> {
    let client = reqwest::Client::new();
    let resp = client.post(TOKEN_ENDPOINT)
        .form(&[
            ("refresh_token", refresh_token),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("grant_type", "refresh_token"),
        ])
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Token refresh failed: {}", e))?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(format!("Token refresh failed ({}): {}", status, body).into());
    }

    let json: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("Invalid refresh response: {}", e))?;

    let access_token = json["access_token"].as_str()
        .ok_or("Refresh response missing 'access_token'")?
        .to_string();
    let expires_in = json["expires_in"].as_u64().unwrap_or(3600);
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

    Ok((access_token, now + expires_in))
}

/// Get the user's email from the userinfo endpoint.
async fn get_user_email(access_token: &str) -> EngineResult<String> {
    let client = reqwest::Client::new();
    let resp = client.get(USERINFO_ENDPOINT)
        .header("Authorization", format!("Bearer {}", access_token))
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Userinfo request failed: {}", e))?;

    let json: serde_json::Value = resp.json().await
        .map_err(|e| format!("Userinfo parse failed: {}", e))?;

    json["email"].as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Userinfo response missing 'email'".into())
}

/// Check if Google OAuth is connected (has a refresh token stored).
pub fn get_connection_status(app_handle: &tauri::AppHandle) -> Option<String> {
    let state = app_handle.try_state::<EngineState>()?;
    let vault_key = skills::get_vault_key().ok()?;
    get_decrypted_cred(&state, &vault_key, "GOOGLE_USER_EMAIL")
}

/// Disconnect Google OAuth — remove all stored tokens.
pub fn disconnect(app_handle: &tauri::AppHandle) -> EngineResult<()> {
    let state = app_handle.try_state::<EngineState>()
        .ok_or("Engine state not available")?;

    for key in &["GOOGLE_ACCESS_TOKEN", "GOOGLE_REFRESH_TOKEN", "GOOGLE_TOKEN_EXPIRES_AT", "GOOGLE_USER_EMAIL"] {
        let _ = state.store.delete_skill_credential("google_workspace", key);
    }

    // Clear cached token
    super::google::clear_token_cache();

    info!("[google-oauth] Disconnected");
    Ok(())
}

// ── Credential helpers ─────────────────────────────────────────────────────

fn get_decrypted_cred(state: &EngineState, vault_key: &[u8], key: &str) -> Option<String> {
    let encrypted = state.store.get_skill_credential("google_workspace", key).ok()??;
    skills::decrypt_credential(&encrypted, vault_key).ok()
}

fn store_cred(state: &EngineState, vault_key: &[u8], key: &str, value: &str) -> EngineResult<()> {
    let encrypted = skills::encrypt_credential(value, vault_key);
    state.store.set_skill_credential("google_workspace", key, &encrypted)
        .map_err(|e| e.to_string())?;
    Ok(())
}
