// pawz-code — main.rs
// HTTP server: POST /chat/stream (SSE), GET /health, GET /status,
//              POST /runs/cancel, GET /memory/search
// Auth: bearer token checked on every request except /health.

mod agent;
mod claude_code;
mod config;
mod engram;
mod memory;
mod protocols;
mod provider;
mod reduction;
mod state;
mod tools;
mod types;

use axum::{
    body::Body,
    extract::{Query, State},
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::StreamExt;
use serde::Deserialize;
use std::sync::Arc;
use tokio_stream::wrappers::BroadcastStream;
use types::ChatRequest;

// ── Auth middleware ──────────────────────────────────────────────────────────

async fn require_auth(
    State(state): State<Arc<state::AppState>>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Health endpoint is exempt
    if req.uri().path() == "/health" {
        return Ok(next.run(req).await);
    }

    let token = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    // §Security: constant-time comparison to prevent timing attacks
    if !constant_time_eq(token.as_bytes(), state.config.auth_token.as_bytes()) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(req).await)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

// ── GET /health ──────────────────────────────────────────────────────────────

async fn health(State(state): State<Arc<state::AppState>>) -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "service": "pawz-code",
        "model": state.config.model,
        "provider": state.config.provider,
    }))
}

// ── GET /status ──────────────────────────────────────────────────────────────

async fn status(State(state): State<Arc<state::AppState>>) -> impl IntoResponse {
    let active_runs = state.active_run_count();
    let memory_count = memory::memory_count(&state).unwrap_or(0);
    let engram_count = engram::engram_count(&state).unwrap_or(0);
    let loaded_protocols = protocols::loaded_protocol_names(&state);

    Json(serde_json::json!({
        "status": "ok",
        "service": "pawz-code",
        "version": env!("CARGO_PKG_VERSION"),
        "model": state.config.model,
        "provider": state.config.provider,
        "workspace_root": state.config.workspace_root,
        "active_runs": active_runs,
        "memory_entries": memory_count,
        "engram_entries": engram_count,
        "protocols": loaded_protocols,
        "max_rounds": state.config.max_rounds,
    }))
}

// ── POST /runs/cancel ────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CancelRequest {
    run_id: String,
}

async fn cancel_run(
    State(state): State<Arc<state::AppState>>,
    Json(req): Json<CancelRequest>,
) -> impl IntoResponse {
    let cancelled = state.cancel_run(&req.run_id);
    if cancelled {
        log::info!("[cancel] run_id={} cancelled", req.run_id);
        Json(serde_json::json!({ "cancelled": true, "run_id": req.run_id }))
    } else {
        Json(serde_json::json!({
            "cancelled": false,
            "run_id": req.run_id,
            "reason": "run not found or already complete"
        }))
    }
}

// ── GET /memory/search ───────────────────────────────────────────────────────

#[derive(Deserialize)]
struct MemorySearchQuery {
    q: String,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    20
}

async fn memory_search(
    State(state): State<Arc<state::AppState>>,
    Query(params): Query<MemorySearchQuery>,
) -> impl IntoResponse {
    match memory::recall(&state, &params.q) {
        Ok(results) => {
            let limited: Vec<_> = results.into_iter().take(params.limit).collect();
            let items: Vec<_> = limited
                .into_iter()
                .map(|(k, v)| serde_json::json!({ "key": k, "content": v }))
                .collect();
            Json(serde_json::json!({ "results": items, "query": params.q }))
        }
        Err(e) => Json(serde_json::json!({
            "results": [],
            "query": params.q,
            "error": e.to_string()
        })),
    }
}

// ── GET /engram/search ───────────────────────────────────────────────────────

#[derive(Deserialize)]
struct EngramSearchQuery {
    q: String,
    #[serde(default)]
    scope: Option<String>,
}

async fn engram_search(
    State(state): State<Arc<state::AppState>>,
    Query(params): Query<EngramSearchQuery>,
) -> impl IntoResponse {
    match engram::search(&state, &params.q, params.scope.as_deref()) {
        Ok(results) => Json(serde_json::json!({ "results": results, "query": params.q })),
        Err(e) => Json(serde_json::json!({
            "results": [],
            "query": params.q,
            "error": e.to_string()
        })),
    }
}

// ── POST /chat/stream ────────────────────────────────────────────────────────

async fn chat_stream(
    State(state): State<Arc<state::AppState>>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    let session_id = req
        .session_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let run_id = uuid::Uuid::new_v4().to_string();

    log::info!(
        "[sse] session={} run={} msg={}",
        session_id,
        run_id,
        &req.message[..req.message.len().min(120)]
    );

    // Register run for cancellation tracking
    state.register_run(&run_id);

    // Subscribe BEFORE spawning the agent so we don't miss early events
    let rx = state.sse_tx.subscribe();
    let sid = session_id.clone();

    // Spawn agent in background — it streams events via sse_tx
    let state_clone = state.clone();
    let run_id_clone = run_id.clone();
    tokio::spawn(async move {
        agent::run(state_clone.clone(), req, session_id, run_id_clone.clone()).await;
        state_clone.deregister_run(&run_id_clone);
    });

    // Filter broadcast to events for this session only, then stop after complete/error.
    let stream = BroadcastStream::new(rx)
        .filter_map(move |msg| {
            let sid = sid.clone();
            async move {
                match msg {
                    Ok(json) if json.contains(sid.as_str()) => Some(json),
                    _ => None,
                }
            }
        })
        // scan carries a `done` flag; once we see complete/error we send it then stop
        .scan(false, |done, json| {
            if *done {
                return std::future::ready(None);
            }
            if json.contains("\"kind\":\"complete\"") || json.contains("\"kind\":\"error\"") {
                *done = true;
            }
            std::future::ready(Some(Ok::<Event, anyhow::Error>(Event::default().data(json))))
        });

    let headers = [
        ("Cache-Control", "no-cache"),
        ("X-Accel-Buffering", "no"),
    ];

    (
        headers,
        Sse::new(stream).keep_alive(
            KeepAlive::new()
                .interval(std::time::Duration::from_secs(15))
                .text("keep-alive"),
        ),
    )
}

// ── Main ─────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config = config::Config::load_or_create()?;

    // api_key is not required when using the claude_code provider — the claude
    // CLI handles authentication independently via its own login flow.
    if config.api_key.is_empty() && config.provider != "claude_code" {
        eprintln!(
            "[pawz-code] ERROR: api_key is not set in {}\n\
             [pawz-code] Edit the file and add your {} API key, then restart.\n\
             [pawz-code] TIP: Set provider = \"claude_code\" to use the Claude CLI instead.",
            config::Config::config_path().display(),
            config.provider
        );
        std::process::exit(1);
    }

    if config.provider == "claude_code" {
        let binary = config
            .claude_binary_path
            .as_deref()
            .unwrap_or("claude");
        eprintln!(
            "[pawz-code] Provider: claude_code (binary: {})\n\
             [pawz-code] Make sure you are logged in: run `{} login` first.",
            binary, binary
        );
    }

    let bind = format!("{}:{}", config.bind, config.port);
    let app_state = Arc::new(state::AppState::new(config)?);

    // Load protocols on startup
    protocols::load_protocols(&app_state);

    let app = Router::new()
        .route("/chat/stream", post(chat_stream))
        .route("/health", get(health))
        .route("/status", get(status))
        .route("/runs/cancel", post(cancel_run))
        .route("/memory/search", get(memory_search))
        .route("/engram/search", get(engram_search))
        .layer(middleware::from_fn_with_state(app_state.clone(), require_auth))
        .with_state(app_state);

    log::info!("[pawz-code] Listening on http://{}", bind);
    log::info!("[pawz-code] VS Code: set pawzCode.serverUrl = http://{}", bind);

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
