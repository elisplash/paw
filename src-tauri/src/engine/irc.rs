// Paw Agent Engine — IRC Bridge
//
// Connects Paw to any IRC server via outbound TCP/TLS.
// The simplest chat protocol — text-based, no special API.
//
// Setup: Pick a server (e.g. irc.libera.chat), set a nick, join channels.
//
// Security:
//   - Allowlist by IRC nick
//   - Optional pairing mode
//   - TLS encryption to server (enabled by default, port 6697)

use crate::engine::channels::{self, PendingUser, ChannelStatus};
use log::{debug, info, error, warn};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, AsyncRead, AsyncWrite, BufReader};
use tokio::net::TcpStream;
use crate::atoms::error::{EngineResult, EngineError};

// ── IRC Config ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrcConfig {
    pub server: String,
    pub port: u16,
    pub tls: bool,
    pub nick: String,
    pub password: Option<String>,
    /// Channels to join (e.g. ["#paw", "#general"])
    pub channels_to_join: Vec<String>,
    pub enabled: bool,
    /// "open" | "allowlist" | "pairing"
    pub dm_policy: String,
    /// IRC nicks allowed to talk privately
    pub allowed_users: Vec<String>,
    #[serde(default)]
    pub pending_users: Vec<PendingUser>,
    pub agent_id: Option<String>,
    /// Whether to respond to messages in channels (not just DMs)
    #[serde(default)]
    pub respond_in_channels: bool,
}

impl Default for IrcConfig {
    fn default() -> Self {
        IrcConfig {
            server: "irc.libera.chat".into(),
            port: 6697,
            tls: true,
            nick: "paw-bot".into(),
            password: None,
            channels_to_join: vec![],
            enabled: false,
            dm_policy: "pairing".into(),
            allowed_users: vec![],
            pending_users: vec![],
            agent_id: None,
            respond_in_channels: false,
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

const CONFIG_KEY: &str = "irc_config";

// ── Bridge Core ────────────────────────────────────────────────────────

pub fn start_bridge(app_handle: tauri::AppHandle) -> EngineResult<()> {
    if BRIDGE_RUNNING.load(Ordering::Relaxed) {
        return Err("IRC bridge is already running".into());
    }

    let config: IrcConfig = channels::load_channel_config(&app_handle, CONFIG_KEY)?;
    if config.server.is_empty() || config.nick.is_empty() {
        return Err("IRC server and nick are required.".into());
    }
    if !config.enabled {
        return Err("IRC bridge is disabled.".into());
    }

    let stop = get_stop_signal();
    stop.store(false, Ordering::Relaxed);
    BRIDGE_RUNNING.store(true, Ordering::Relaxed);

    info!("[irc] Starting bridge to {}:{}", config.server, config.port);

    tauri::async_runtime::spawn(async move {
        if let Err(e) = run_irc_loop(app_handle, config).await {
            error!("[irc] Bridge crashed: {}", e);
        }
        BRIDGE_RUNNING.store(false, Ordering::Relaxed);
        info!("[irc] Bridge stopped");
    });

    Ok(())
}

pub fn stop_bridge() {
    let stop = get_stop_signal();
    stop.store(true, Ordering::Relaxed);
    BRIDGE_RUNNING.store(false, Ordering::Relaxed);
    info!("[irc] Stop signal sent");
}

pub fn get_status(app_handle: &tauri::AppHandle) -> ChannelStatus {
    let config: IrcConfig = channels::load_channel_config(app_handle, CONFIG_KEY).unwrap_or_default();
    ChannelStatus {
        running: BRIDGE_RUNNING.load(Ordering::Relaxed),
        connected: BRIDGE_RUNNING.load(Ordering::Relaxed),
        bot_name: Some(config.nick.clone()),
        bot_id: Some(format!("{}:{}", config.server, config.port)),
        message_count: MESSAGE_COUNT.load(Ordering::Relaxed) as u64,
        allowed_users: config.allowed_users,
        pending_users: config.pending_users,
        dm_policy: config.dm_policy,
    }
}

// ── IRC Connection Loop ────────────────────────────────────────────────

/// Trait alias for TLS or plain TCP streams — both implement AsyncRead + AsyncWrite.
trait IrcStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> IrcStream for T {}

async fn run_irc_loop(app_handle: tauri::AppHandle, config: IrcConfig) -> EngineResult<()> {
    let stop = get_stop_signal();
    let addr = format!("{}:{}", config.server, config.port);

    let tcp = TcpStream::connect(&addr).await
        .map_err(|e| format!("TCP connect to {} failed: {}", addr, e))?;

    // Wrap with TLS if enabled
    let stream: Box<dyn IrcStream> = if config.tls {
        info!("[irc] Upgrading to TLS for {}", addr);

        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        let tls_config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let connector = tokio_rustls::TlsConnector::from(Arc::new(tls_config));

        let server_name = rustls::pki_types::ServerName::try_from(config.server.clone())
            .map_err(|e| EngineError::Channel { channel: "irc".into(), message: format!("Invalid server name: {}", e) })?;

        let tls_stream = connector.connect(server_name, tcp).await
            .map_err(|e| format!("TLS handshake with {} failed: {}", addr, e))?;

        info!("[irc] TLS handshake complete for {}", addr);
        Box::new(tls_stream)
    } else {
        warn!("[irc] Connecting WITHOUT TLS to {} — credentials will be sent in plaintext!", addr);
        Box::new(tcp)
    };

    let (reader, writer) = tokio::io::split(stream);
    let mut lines = BufReader::new(reader).lines();

    // Register with the server
    let write_handle = Arc::new(tokio::sync::Mutex::new(writer));
    {
        let mut w = write_handle.lock().await;
        if let Some(ref pass) = config.password {
            let cmd = format!("PASS {}\r\n", pass);
            w.write_all(cmd.as_bytes()).await?;
        }
        let nick_cmd = format!("NICK {}\r\n", config.nick);
        let user_cmd = format!("USER {} 0 * :Paw Agent\r\n", config.nick);
        w.write_all(nick_cmd.as_bytes()).await?;
        w.write_all(user_cmd.as_bytes()).await?;
    }

    info!("[irc] Sent NICK/USER to {}", addr);

    // Shared writer for sending replies
    let mut current_config = config.clone();
    let mut registered = false;
    let mut last_config_reload = std::time::Instant::now();

    while let Ok(Some(line)) = lines.next_line().await {
        if stop.load(Ordering::Relaxed) { break; }

        let line = line.trim_end().to_string();
        if line.is_empty() { continue; }

        // Handle PING
        if line.starts_with("PING") {
            let pong = line.replace("PING", "PONG");
            let mut w = write_handle.lock().await;
            let _ = w.write_all(format!("{}\r\n", pong).as_bytes()).await;
            continue;
        }

        // Parse IRC message
        let parsed = parse_irc_line(&line);

        // Check for registration complete (RPL_WELCOME = 001)
        if parsed.command == "001" && !registered {
            registered = true;
            info!("[irc] Registered as {}", config.nick);

            let _ = app_handle.emit("irc-status", json!({
                "kind": "connected",
                "nick": &config.nick,
                "server": &config.server,
            }));

            // Join configured channels
            for ch in &config.channels_to_join {
                let mut w = write_handle.lock().await;
                let _ = w.write_all(format!("JOIN {}\r\n", ch).as_bytes()).await;
                info!("[irc] Joining {}", ch);
            }
        }

        // Handle PRIVMSG
        if parsed.command == "PRIVMSG" {
            let sender_nick = parsed.prefix_nick().unwrap_or_default();
            if sender_nick == config.nick { continue; } // Skip own messages

            let target = parsed.params.first().map(|s| s.as_str()).unwrap_or("");
            let text = parsed.trailing.clone().unwrap_or_default();
            if text.is_empty() { continue; }

            let is_dm = target == config.nick; // DM = target is our nick
            let is_channel = target.starts_with('#') || target.starts_with('&');

            // In channels, only respond if enabled or if directly addressed
            if is_channel {
                let addressed = text.starts_with(&format!("{}:", config.nick))
                    || text.starts_with(&format!("{},", config.nick));
                if !current_config.respond_in_channels && !addressed { continue; }
            }

            // Strip nick prefix if addressed
            let content = if is_channel {
                let prefixes = [
                    format!("{}: ", config.nick),
                    format!("{}, ", config.nick),
                    format!("{}:", config.nick),
                    format!("{},", config.nick),
                ];
                let mut c = text.clone();
                for p in &prefixes {
                    if c.starts_with(p) {
                        c = c[p.len()..].trim().to_string();
                        break;
                    }
                }
                c
            } else {
                text.clone()
            };

            if content.is_empty() { continue; }

            debug!("[irc] {} from {}: {}", if is_dm { "DM" } else { "Channel msg" },
                sender_nick, if content.len() > 50 { format!("{}...", &content[..50]) } else { content.clone() });

            // Access control (DMs only)
            if is_dm {
                match channels::check_access(
                    &current_config.dm_policy,
                    &sender_nick,
                    &sender_nick,
                    &sender_nick,
                    &current_config.allowed_users,
                    &mut current_config.pending_users,
                ) {
                    Err(denial_msg) => {
                        let _ = channels::save_channel_config(&app_handle, CONFIG_KEY, &current_config);
                        let _ = app_handle.emit("irc-status", json!({
                            "kind": "pairing_request",
                            "user_id": &sender_nick,
                            "username": &sender_nick,
                        }));
                        let reply_target = if is_dm { &sender_nick } else { target };
                        let mut w = write_handle.lock().await;
                        let _ = w.write_all(format!("PRIVMSG {} :{}\r\n", reply_target, denial_msg).as_bytes()).await;
                        continue;
                    }
                    Ok(()) => {}
                }
            }

            MESSAGE_COUNT.fetch_add(1, Ordering::Relaxed);

            // Route to agent
            let agent_id = current_config.agent_id.as_deref().unwrap_or("default");
            let ctx = "You are chatting via IRC. Keep responses concise and plain-text. \
                       No markdown rendering — use simple text formatting. \
                       IRC messages should ideally be under 400 characters.";

            let response = channels::run_channel_agent(
                &app_handle, "irc", ctx, &content, &sender_nick, agent_id,
            ).await;

            let reply_target = if is_dm { sender_nick.as_str() } else { target };

            match response {
                Ok(reply) if !reply.is_empty() => {
                    // IRC has ~512 byte line limit, split at 400 chars
                    for chunk in channels::split_message(&reply, 400) {
                        // Replace newlines with separate PRIVMSG lines
                        for line in chunk.lines() {
                            if !line.trim().is_empty() {
                                let mut w = write_handle.lock().await;
                                let _ = w.write_all(
                                    format!("PRIVMSG {} :{}\r\n", reply_target, line).as_bytes()
                                ).await;
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("[irc] Agent error for {}: {}", sender_nick, e);
                    let mut w = write_handle.lock().await;
                    let _ = w.write_all(
                        format!("PRIVMSG {} :⚠️ Error: {}\r\n", reply_target, e).as_bytes()
                    ).await;
                }
                _ => {}
            }
        }

        // Reload config periodically
        if last_config_reload.elapsed() > std::time::Duration::from_secs(30) {
            if let Ok(fresh) = channels::load_channel_config::<IrcConfig>(&app_handle, CONFIG_KEY) {
                current_config = fresh;
            }
            last_config_reload = std::time::Instant::now();
        }
    }

    let _ = app_handle.emit("irc-status", json!({
        "kind": "disconnected",
    }));

    Ok(())
}

// ── IRC Message Parser ─────────────────────────────────────────────────

struct IrcParsed {
    prefix: Option<String>,
    command: String,
    params: Vec<String>,
    trailing: Option<String>,
}

impl IrcParsed {
    fn prefix_nick(&self) -> Option<String> {
        self.prefix.as_ref().map(|p| {
            p.split('!').next().unwrap_or(p).to_string()
        })
    }
}

fn parse_irc_line(line: &str) -> IrcParsed {
    let mut remaining = line;
    let prefix = if remaining.starts_with(':') {
        let end = remaining.find(' ').unwrap_or(remaining.len());
        let p = remaining[1..end].to_string();
        remaining = &remaining[end..].trim_start();
        Some(p)
    } else {
        None
    };

    // Split trailing (after ' :')
    let (main, trailing) = if let Some(idx) = remaining.find(" :") {
        (&remaining[..idx], Some(remaining[idx + 2..].to_string()))
    } else {
        (remaining, None)
    };

    let parts: Vec<&str> = main.split_whitespace().collect();
    let command = parts.first().unwrap_or(&"").to_string();
    let params: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();

    IrcParsed { prefix, command, params, trailing }
}

// ── Config Persistence ─────────────────────────────────────────────────

pub fn load_config(app_handle: &tauri::AppHandle) -> EngineResult<IrcConfig> {
    channels::load_channel_config(app_handle, CONFIG_KEY)
}

pub fn save_config(app_handle: &tauri::AppHandle, config: &IrcConfig) -> EngineResult<()> {
    channels::save_channel_config(app_handle, CONFIG_KEY, config)
}

pub fn approve_user(app_handle: &tauri::AppHandle, user_id: &str) -> EngineResult<()> {
    channels::approve_user_generic(app_handle, CONFIG_KEY, user_id)
}

pub fn deny_user(app_handle: &tauri::AppHandle, user_id: &str) -> EngineResult<()> {
    channels::deny_user_generic(app_handle, CONFIG_KEY, user_id)
}

pub fn remove_user(app_handle: &tauri::AppHandle, user_id: &str) -> EngineResult<()> {
    channels::remove_user_generic(app_handle, CONFIG_KEY, user_id)
}
