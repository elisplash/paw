// Paw Agent Engine — Web Chat Server Core
//
// TCP/TLS listener, HTTP routing, auth endpoint, and stream utilities.

use super::html::build_chat_html;
use super::session::{create_session, validate_session, extract_cookie};
use super::{WebChatConfig, handle_websocket, get_stop_signal};

use log::{info, warn};
use serde_json::json;
use std::io::BufReader as StdBufReader;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use tauri::Emitter;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::TcpListener;
use crate::atoms::error::EngineResult;

// ── Prefixed Stream (replays buffered bytes then delegates) ────────────

pub(crate) struct PrefixedStream<S> {
    prefix: Vec<u8>,
    pos: usize,
    inner: S,
}

impl<S> PrefixedStream<S> {
    pub fn new(prefix: Vec<u8>, inner: S) -> Self {
        Self { prefix, pos: 0, inner }
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for PrefixedStream<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        if this.pos < this.prefix.len() {
            let remaining = &this.prefix[this.pos..];
            let n = remaining.len().min(buf.remaining());
            buf.put_slice(&remaining[..n]);
            this.pos += n;
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut this.inner).poll_read(cx, buf)
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for PrefixedStream<S> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

// ── Stream Abstraction ─────────────────────────────────────────────────

pub(crate) trait ChatStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> ChatStream for T {}

/// Build a TLS acceptor from PEM cert+key files, or `None` if not configured.
pub(crate) fn build_tls_acceptor(config: &WebChatConfig) -> EngineResult<Option<tokio_rustls::TlsAcceptor>> {
    let (Some(cert_path), Some(key_path)) = (&config.tls_cert_path, &config.tls_key_path) else {
        return Ok(None);
    };

    let cert_file = std::fs::File::open(cert_path)
        .map_err(|e| format!("Open TLS cert {cert_path}: {e}"))?;
    let certs: Vec<_> = rustls_pemfile::certs(&mut StdBufReader::new(cert_file))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Parse TLS cert: {e}"))?;

    let key_file = std::fs::File::open(key_path)
        .map_err(|e| format!("Open TLS key {key_path}: {e}"))?;
    let key = rustls_pemfile::private_key(&mut StdBufReader::new(key_file))
        .map_err(|e| format!("Parse TLS key: {e}"))?
        .ok_or_else(|| "No private key found in PEM file".to_string())?;

    let tls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| format!("TLS config: {e}"))?;

    Ok(Some(tokio_rustls::TlsAcceptor::from(Arc::new(tls_config))))
}

// ── Server Core ────────────────────────────────────────────────────────

pub(crate) async fn run_server(app_handle: tauri::AppHandle, config: WebChatConfig) -> EngineResult<()> {
    let stop = get_stop_signal();
    let addr = format!("{}:{}", config.bind_address, config.port);

    let listener = TcpListener::bind(&addr).await
        .map_err(|e| format!("Bind {}:{} failed: {}", config.bind_address, config.port, e))?;

    // Build optional TLS acceptor
    let tls_acceptor = build_tls_acceptor(&config)?;

    if config.bind_address != "127.0.0.1" && config.bind_address != "localhost" && tls_acceptor.is_none() {
        warn!("[webchat] Binding to {} without TLS — credentials sent in plaintext over the network", config.bind_address);
    }

    let scheme = if tls_acceptor.is_some() { "https" } else { "http" };
    info!("[webchat] Listening on {}://{}", scheme, addr);

    let _ = app_handle.emit("webchat-status", json!({
        "kind": "connected",
        "address": &addr,
        "title": &config.page_title,
        "tls": tls_acceptor.is_some(),
    }));

    let config = Arc::new(config);
    let tls_acceptor = tls_acceptor.map(Arc::new);

    loop {
        if stop.load(Ordering::Relaxed) { break; }

        // Accept with timeout so we can check stop signal
        let accept = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            listener.accept()
        ).await;

        match accept {
            Ok(Ok((tcp_stream, peer))) => {
                let app = app_handle.clone();
                let cfg = config.clone();
                let stop_clone = stop.clone();
                let tls = tls_acceptor.clone();
                tokio::spawn(async move {
                    // Wrap in TLS if configured, then box for type erasure
                    let stream: Box<dyn ChatStream> = if let Some(acceptor) = tls {
                        match acceptor.accept(tcp_stream).await {
                            Ok(tls_stream) => Box::new(tls_stream),
                            Err(e) => {
                                warn!("[webchat] TLS handshake failed from {}: {}", peer, e);
                                return;
                            }
                        }
                    } else {
                        Box::new(tcp_stream)
                    };

                    if let Err(e) = handle_connection(stream, peer, app, cfg, stop_clone).await {
                        warn!("[webchat] Connection error from {}: {}", peer, e);
                    }
                });
            }
            Ok(Err(e)) => {
                warn!("[webchat] Accept error: {}", e);
            }
            Err(_) => { /* timeout — loop to check stop signal */ }
        }
    }

    Ok(())
}

// ── Connection Handler ─────────────────────────────────────────────────

async fn handle_connection(
    mut stream: Box<dyn ChatStream>,
    peer: std::net::SocketAddr,
    app_handle: tauri::AppHandle,
    config: Arc<WebChatConfig>,
    _stop: Arc<AtomicBool>,
) -> EngineResult<()> {
    // Read the HTTP request (consumed — PrefixedStream replays it for WS)
    let mut buf = vec![0u8; 8192];
    let n = stream.read(&mut buf).await.map_err(|e| format!("Read: {e}"))?;
    if n == 0 { return Ok(()); }
    buf.truncate(n);

    let request_str = String::from_utf8_lossy(&buf);
    let first_line = request_str.lines().next().unwrap_or("");
    let is_websocket = request_str.contains("Upgrade: websocket")
        || request_str.contains("upgrade: websocket");

    if is_websocket && first_line.contains("/ws") {
        // Validate session cookie (token is never in the URL)
        let session_id = extract_cookie(&request_str, "paw_session").unwrap_or("");
        let username = match validate_session(session_id) {
            Some(name) => name,
            None => {
                let resp = "HTTP/1.1 403 Forbidden\r\nContent-Length: 16\r\n\r\nSession invalid.";
                let _ = stream.write_all(resp.as_bytes()).await;
                return Ok(());
            }
        };

        info!("[webchat] WebSocket connection from {} ({})", peer, username);

        // Replay the buffered bytes so tungstenite can read the HTTP upgrade
        let prefixed = PrefixedStream::new(buf, stream);
        handle_websocket(prefixed, peer, app_handle, config, username).await
    } else if first_line.starts_with("POST") && first_line.contains("/auth") {
        handle_auth(stream, &buf, &config).await
    } else if first_line.starts_with("GET /") {
        serve_html(stream, &config).await
    } else {
        Ok(())
    }
}

// ── Auth Endpoint ──────────────────────────────────────────────────────

/// POST /auth — validates access token, returns a session cookie.
async fn handle_auth(
    mut stream: Box<dyn ChatStream>,
    request_bytes: &[u8],
    config: &WebChatConfig,
) -> EngineResult<()> {
    let request_str = String::from_utf8_lossy(request_bytes);

    // Extract JSON body (after \r\n\r\n)
    let body = request_str.split("\r\n\r\n").nth(1).unwrap_or("");
    let parsed: serde_json::Value = serde_json::from_str(body).unwrap_or(json!({}));

    let token = parsed["token"].as_str().unwrap_or("");
    let name = parsed["name"].as_str().unwrap_or("").trim();

    if token != config.access_token || name.is_empty() {
        let resp = "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: 24\r\nConnection: close\r\n\r\n{\"error\":\"access denied\"}";
        stream.write_all(resp.as_bytes()).await
            .map_err(|e| format!("Write auth 403: {e}"))?;
        return Ok(());
    }

    let session_id = create_session(name.to_string());
    info!("[webchat] Session created for '{}'", name);

    let resp_body = json!({"ok": true}).to_string();
    let cookie = format!(
        "paw_session={}; HttpOnly; SameSite=Strict; Path=/; Max-Age=86400",
        session_id,
    );
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nSet-Cookie: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        cookie, resp_body.len(), resp_body
    );

    stream.write_all(response.as_bytes()).await
        .map_err(|e| format!("Write auth 200: {e}"))?;
    Ok(())
}

// ── HTML Chat Page ─────────────────────────────────────────────────────

async fn serve_html(
    mut stream: Box<dyn ChatStream>,
    config: &WebChatConfig,
) -> EngineResult<()> {
    let html = build_chat_html(&config.page_title);
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(), html
    );

    stream.write_all(response.as_bytes()).await
        .map_err(|e| format!("Write HTML: {e}"))?;

    Ok(())
}
