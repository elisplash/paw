// Paw Commands — Canvas (Visual Workspace)
//
// Agent-driven visual canvas for spatial note-taking, diagramming,
// and collaborative ideation.  Nodes live in SQLite; the frontend
// renders them on an infinite pannable/zoomable surface.

use crate::engine::state::EngineState;
use log::info;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::State;

// ── Types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Canvas {
    pub id: String,
    pub name: String,
    pub description: String,
    pub created_at: String,
    pub updated_at: String,
    pub viewport: CanvasViewport,
    pub node_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasViewport {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

impl Default for CanvasViewport {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0, zoom: 1.0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasNode {
    pub id: String,
    pub canvas_id: String,
    pub kind: String,
    pub title: String,
    pub content: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub color: String,
    pub z_index: i32,
    pub collapsed: bool,
    pub metadata: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasEdge {
    pub id: String,
    pub canvas_id: String,
    pub from_node: String,
    pub to_node: String,
    pub label: String,
    pub color: String,
    pub style: String,
}

// ── Table init ─────────────────────────────────────────────────────────

fn ensure_canvas_tables(state: &EngineState) -> Result<(), String> {
    let conn = state.store.conn.lock();
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS canvases (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            viewport    TEXT NOT NULL DEFAULT '{\"x\":0,\"y\":0,\"zoom\":1}',
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS canvas_nodes (
            id          TEXT PRIMARY KEY,
            canvas_id   TEXT NOT NULL,
            kind        TEXT NOT NULL DEFAULT 'text',
            title       TEXT NOT NULL DEFAULT '',
            content     TEXT NOT NULL DEFAULT '',
            x           REAL NOT NULL DEFAULT 0,
            y           REAL NOT NULL DEFAULT 0,
            width       REAL NOT NULL DEFAULT 240,
            height      REAL NOT NULL DEFAULT 160,
            color       TEXT NOT NULL DEFAULT '#ff00ff',
            z_index     INTEGER NOT NULL DEFAULT 0,
            collapsed   INTEGER NOT NULL DEFAULT 0,
            metadata    TEXT NOT NULL DEFAULT '{}',
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS canvas_edges (
            id          TEXT PRIMARY KEY,
            canvas_id   TEXT NOT NULL,
            from_node   TEXT NOT NULL,
            to_node     TEXT NOT NULL,
            label       TEXT NOT NULL DEFAULT '',
            color       TEXT NOT NULL DEFAULT '#888888',
            style       TEXT NOT NULL DEFAULT 'solid'
        );
        CREATE INDEX IF NOT EXISTS idx_canvas_nodes_canvas ON canvas_nodes(canvas_id);
        CREATE INDEX IF NOT EXISTS idx_canvas_edges_canvas ON canvas_edges(canvas_id);
    ").map_err(|e| format!("Canvas table init failed: {}", e))
}

// ── Canvas CRUD ────────────────────────────────────────────────────────

#[tauri::command]
pub fn engine_canvas_list(state: State<'_, EngineState>) -> Result<Vec<Canvas>, String> {
    ensure_canvas_tables(&state)?;
    let conn = state.store.conn.lock();

    let mut stmt = conn.prepare(
        "SELECT id, name, description, viewport, created_at, updated_at,
                (SELECT COUNT(*) FROM canvas_nodes WHERE canvas_id = canvases.id) as node_count
         FROM canvases ORDER BY updated_at DESC"
    ).map_err(|e| format!("Canvas list query failed: {}", e))?;

    let canvases = stmt.query_map([], |row| {
        let viewport_json: String = row.get(3)?;
        let viewport: CanvasViewport = serde_json::from_str(&viewport_json).unwrap_or_default();
        Ok(Canvas {
            id: row.get(0)?,
            name: row.get(1)?,
            description: row.get(2)?,
            viewport,
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
            node_count: row.get::<_, i64>(6)? as usize,
        })
    }).map_err(|e| format!("Canvas list failed: {}", e))?
    .filter_map(|r| r.ok())
    .collect();

    Ok(canvases)
}

#[tauri::command]
pub fn engine_canvas_create(
    state: State<'_, EngineState>,
    name: String,
    description: Option<String>,
) -> Result<Canvas, String> {
    ensure_canvas_tables(&state)?;
    let conn = state.store.conn.lock();
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let desc = description.unwrap_or_default();
    let viewport = CanvasViewport::default();
    let viewport_json = serde_json::to_string(&viewport).unwrap_or_default();

    conn.execute(
        "INSERT INTO canvases (id, name, description, viewport, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, name, desc, viewport_json, now, now],
    ).map_err(|e| format!("Canvas create failed: {}", e))?;

    info!("[canvas] Created canvas '{}' ({})", name, id);
    Ok(Canvas { id, name, description: desc, viewport, created_at: now.clone(), updated_at: now, node_count: 0 })
}

#[tauri::command]
pub fn engine_canvas_update(
    state: State<'_, EngineState>,
    id: String,
    name: Option<String>,
    description: Option<String>,
    viewport: Option<CanvasViewport>,
) -> Result<(), String> {
    ensure_canvas_tables(&state)?;
    let conn = state.store.conn.lock();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    if let Some(n) = &name {
        conn.execute(
            "UPDATE canvases SET name = ?1, updated_at = ?2 WHERE id = ?3",
            params![n, now, id],
        ).map_err(|e| format!("Canvas update name failed: {}", e))?;
    }
    if let Some(d) = &description {
        conn.execute(
            "UPDATE canvases SET description = ?1, updated_at = ?2 WHERE id = ?3",
            params![d, now, id],
        ).map_err(|e| format!("Canvas update desc failed: {}", e))?;
    }
    if let Some(v) = &viewport {
        let vj = serde_json::to_string(v).unwrap_or_default();
        conn.execute(
            "UPDATE canvases SET viewport = ?1, updated_at = ?2 WHERE id = ?3",
            params![vj, now, id],
        ).map_err(|e| format!("Canvas update viewport failed: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
pub fn engine_canvas_delete(state: State<'_, EngineState>, id: String) -> Result<(), String> {
    ensure_canvas_tables(&state)?;
    let conn = state.store.conn.lock();
    conn.execute("DELETE FROM canvas_edges WHERE canvas_id = ?1", params![id])
        .map_err(|e| format!("Canvas edge cleanup failed: {}", e))?;
    conn.execute("DELETE FROM canvas_nodes WHERE canvas_id = ?1", params![id])
        .map_err(|e| format!("Canvas node cleanup failed: {}", e))?;
    conn.execute("DELETE FROM canvases WHERE id = ?1", params![id])
        .map_err(|e| format!("Canvas delete failed: {}", e))?;
    info!("[canvas] Deleted canvas {}", id);
    Ok(())
}

// ── Node CRUD ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn engine_canvas_nodes(
    state: State<'_, EngineState>,
    canvas_id: String,
) -> Result<Vec<CanvasNode>, String> {
    ensure_canvas_tables(&state)?;
    let conn = state.store.conn.lock();

    let mut stmt = conn.prepare(
        "SELECT id, canvas_id, kind, title, content, x, y, width, height,
                color, z_index, collapsed, metadata, created_at, updated_at
         FROM canvas_nodes WHERE canvas_id = ?1 ORDER BY z_index ASC"
    ).map_err(|e| format!("Canvas nodes query failed: {}", e))?;

    let nodes = stmt.query_map(params![canvas_id], |row| {
        Ok(CanvasNode {
            id: row.get(0)?,
            canvas_id: row.get(1)?,
            kind: row.get(2)?,
            title: row.get(3)?,
            content: row.get(4)?,
            x: row.get(5)?,
            y: row.get(6)?,
            width: row.get(7)?,
            height: row.get(8)?,
            color: row.get(9)?,
            z_index: row.get(10)?,
            collapsed: row.get::<_, i32>(11)? != 0,
            metadata: row.get(12)?,
            created_at: row.get(13)?,
            updated_at: row.get(14)?,
        })
    }).map_err(|e| format!("Canvas nodes failed: {}", e))?
    .filter_map(|r| r.ok())
    .collect();

    Ok(nodes)
}

#[tauri::command]
pub fn engine_canvas_node_create(
    state: State<'_, EngineState>,
    canvas_id: String,
    kind: String,
    title: Option<String>,
    content: Option<String>,
    x: f64,
    y: f64,
    width: Option<f64>,
    height: Option<f64>,
    color: Option<String>,
    metadata: Option<String>,
) -> Result<CanvasNode, String> {
    ensure_canvas_tables(&state)?;
    let conn = state.store.conn.lock();
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let t = title.unwrap_or_default();
    let c = content.unwrap_or_default();
    let w = width.unwrap_or(240.0);
    let h = height.unwrap_or(160.0);
    let col = color.unwrap_or_else(|| "#ff00ff".to_string());
    let meta = metadata.unwrap_or_else(|| "{}".to_string());

    let z: i32 = conn.query_row(
        "SELECT COALESCE(MAX(z_index), 0) FROM canvas_nodes WHERE canvas_id = ?1",
        params![canvas_id],
        |row| row.get::<_, i32>(0),
    ).unwrap_or(0) + 1;

    conn.execute(
        "INSERT INTO canvas_nodes (id, canvas_id, kind, title, content, x, y, width, height, color, z_index, collapsed, metadata, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0, ?12, ?13, ?14)",
        params![id, canvas_id, kind, t, c, x, y, w, h, col, z, meta, now, now],
    ).map_err(|e| format!("Canvas node create failed: {}", e))?;

    conn.execute(
        "UPDATE canvases SET updated_at = ?1 WHERE id = ?2",
        params![now, canvas_id],
    ).ok();

    info!("[canvas] Created {} node '{}' on canvas {}", kind, t, canvas_id);
    Ok(CanvasNode {
        id, canvas_id, kind, title: t, content: c,
        x, y, width: w, height: h, color: col,
        z_index: z, collapsed: false, metadata: meta,
        created_at: now.clone(), updated_at: now,
    })
}

#[tauri::command]
pub fn engine_canvas_node_update(
    state: State<'_, EngineState>,
    id: String,
    title: Option<String>,
    content: Option<String>,
    x: Option<f64>,
    y: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
    color: Option<String>,
    z_index: Option<i32>,
    collapsed: Option<bool>,
    metadata: Option<String>,
) -> Result<(), String> {
    ensure_canvas_tables(&state)?;
    let conn = state.store.conn.lock();
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // Position updates (most frequent — drag/resize)
    if x.is_some() || y.is_some() || width.is_some() || height.is_some() {
        conn.execute(
            "UPDATE canvas_nodes SET
                x = COALESCE(?1, x), y = COALESCE(?2, y),
                width = COALESCE(?3, width), height = COALESCE(?4, height),
                updated_at = ?5
             WHERE id = ?6",
            params![x, y, width, height, now, id],
        ).map_err(|e| format!("Node position update failed: {}", e))?;
    }

    if let Some(t) = &title {
        conn.execute("UPDATE canvas_nodes SET title = ?1, updated_at = ?2 WHERE id = ?3", params![t, now, id])
            .map_err(|e| format!("Node title update failed: {}", e))?;
    }
    if let Some(c) = &content {
        conn.execute("UPDATE canvas_nodes SET content = ?1, updated_at = ?2 WHERE id = ?3", params![c, now, id])
            .map_err(|e| format!("Node content update failed: {}", e))?;
    }
    if let Some(c) = &color {
        conn.execute("UPDATE canvas_nodes SET color = ?1, updated_at = ?2 WHERE id = ?3", params![c, now, id])
            .map_err(|e| format!("Node color update failed: {}", e))?;
    }
    if let Some(z) = z_index {
        conn.execute("UPDATE canvas_nodes SET z_index = ?1, updated_at = ?2 WHERE id = ?3", params![z, now, id])
            .map_err(|e| format!("Node z_index update failed: {}", e))?;
    }
    if let Some(col) = collapsed {
        let val: i32 = if col { 1 } else { 0 };
        conn.execute("UPDATE canvas_nodes SET collapsed = ?1, updated_at = ?2 WHERE id = ?3", params![val, now, id])
            .map_err(|e| format!("Node collapsed update failed: {}", e))?;
    }
    if let Some(m) = &metadata {
        conn.execute("UPDATE canvas_nodes SET metadata = ?1, updated_at = ?2 WHERE id = ?3", params![m, now, id])
            .map_err(|e| format!("Node metadata update failed: {}", e))?;
    }

    conn.execute(
        "UPDATE canvases SET updated_at = ?1 WHERE id = (SELECT canvas_id FROM canvas_nodes WHERE id = ?2)",
        params![now, id],
    ).ok();

    Ok(())
}

#[tauri::command]
pub fn engine_canvas_node_delete(state: State<'_, EngineState>, id: String) -> Result<(), String> {
    ensure_canvas_tables(&state)?;
    let conn = state.store.conn.lock();
    conn.execute("DELETE FROM canvas_edges WHERE from_node = ?1 OR to_node = ?1", params![id])
        .map_err(|e| format!("Edge cleanup failed: {}", e))?;
    conn.execute("DELETE FROM canvas_nodes WHERE id = ?1", params![id])
        .map_err(|e| format!("Canvas node delete failed: {}", e))?;
    info!("[canvas] Deleted node {}", id);
    Ok(())
}

// ── Edge CRUD ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn engine_canvas_edges(
    state: State<'_, EngineState>,
    canvas_id: String,
) -> Result<Vec<CanvasEdge>, String> {
    ensure_canvas_tables(&state)?;
    let conn = state.store.conn.lock();

    let mut stmt = conn.prepare(
        "SELECT id, canvas_id, from_node, to_node, label, color, style
         FROM canvas_edges WHERE canvas_id = ?1"
    ).map_err(|e| format!("Canvas edges query failed: {}", e))?;

    let edges = stmt.query_map(params![canvas_id], |row| {
        Ok(CanvasEdge {
            id: row.get(0)?,
            canvas_id: row.get(1)?,
            from_node: row.get(2)?,
            to_node: row.get(3)?,
            label: row.get(4)?,
            color: row.get(5)?,
            style: row.get(6)?,
        })
    }).map_err(|e| format!("Canvas edges failed: {}", e))?
    .filter_map(|r| r.ok())
    .collect();

    Ok(edges)
}

#[tauri::command]
pub fn engine_canvas_edge_create(
    state: State<'_, EngineState>,
    canvas_id: String,
    from_node: String,
    to_node: String,
    label: Option<String>,
    color: Option<String>,
    style: Option<String>,
) -> Result<CanvasEdge, String> {
    ensure_canvas_tables(&state)?;
    let conn = state.store.conn.lock();
    let id = uuid::Uuid::new_v4().to_string();
    let lbl = label.unwrap_or_default();
    let col = color.unwrap_or_else(|| "#888888".to_string());
    let sty = style.unwrap_or_else(|| "solid".to_string());

    conn.execute(
        "INSERT INTO canvas_edges (id, canvas_id, from_node, to_node, label, color, style)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, canvas_id, from_node, to_node, lbl, col, sty],
    ).map_err(|e| format!("Canvas edge create failed: {}", e))?;

    info!("[canvas] Created edge {} → {} on canvas {}", from_node, to_node, canvas_id);
    Ok(CanvasEdge { id, canvas_id, from_node, to_node, label: lbl, color: col, style: sty })
}

#[tauri::command]
pub fn engine_canvas_edge_delete(state: State<'_, EngineState>, id: String) -> Result<(), String> {
    ensure_canvas_tables(&state)?;
    let conn = state.store.conn.lock();
    conn.execute("DELETE FROM canvas_edges WHERE id = ?1", params![id])
        .map_err(|e| format!("Canvas edge delete failed: {}", e))?;
    Ok(())
}
