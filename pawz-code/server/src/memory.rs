// pawz-code — memory.rs
// SQLite-backed persistence: conversation history + agent-pinned notes.

use crate::state::AppState;
use crate::types::{ContentBlock, Message};
use anyhow::Result;

const HISTORY_LIMIT: usize = 40; // max messages loaded as context per turn

// ── Conversation history ─────────────────────────────────────────────────────

/// Append a message to a session's conversation history.
pub fn save_message(state: &AppState, session_id: &str, msg: &Message) -> Result<()> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let content_json = serde_json::to_string(&msg.blocks)?;
    db.execute(
        "INSERT INTO messages (session_id, role, content_json) VALUES (?1, ?2, ?3)",
        rusqlite::params![session_id, msg.role, content_json],
    )?;
    Ok(())
}

/// Load the last HISTORY_LIMIT messages for a session.
pub fn load_history(state: &AppState, session_id: &str) -> Result<Vec<Message>> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let mut stmt = db.prepare(
        "SELECT role, content_json FROM (
            SELECT role, content_json, id FROM messages WHERE session_id = ?1 ORDER BY id DESC LIMIT ?2
         ) ORDER BY id ASC",
    )?;
    let rows = stmt.query_map(
        rusqlite::params![session_id, HISTORY_LIMIT as i64],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
            ))
        },
    )?;

    let mut msgs = Vec::new();
    for row in rows {
        let (role, content_json) = row?;
        let blocks: Vec<ContentBlock> = serde_json::from_str(&content_json).unwrap_or_default();
        msgs.push(Message { role, blocks });
    }
    Ok(msgs)
}

// ── Pinned notes (agent memory) ──────────────────────────────────────────────

/// Upsert a named memory note.
pub fn remember(state: &AppState, key: &str, content: &str, tags: Option<&str>) -> Result<()> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    db.execute(
        r#"INSERT INTO memories (key, content, tags)
           VALUES (?1, ?2, ?3)
           ON CONFLICT(key) DO UPDATE SET
               content    = excluded.content,
               tags       = excluded.tags,
               updated_at = datetime('now')"#,
        rusqlite::params![key, content, tags],
    )?;
    Ok(())
}

/// Count total memory entries.
pub fn memory_count(state: &AppState) -> anyhow::Result<i64> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let count: i64 = db.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))?;
    Ok(count)
}

/// Full-text search across all memory notes (key + content).
pub fn recall(state: &AppState, query: &str) -> Result<Vec<(String, String)>> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let pattern = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));
    let mut stmt = db.prepare(
        "SELECT key, content FROM memories WHERE key LIKE ?1 ESCAPE '\\' OR content LIKE ?1 ESCAPE '\\' ORDER BY updated_at DESC LIMIT 20",
    )?;
    let rows = stmt.query_map(rusqlite::params![pattern], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

/// Load ALL memories as a formatted context block for the system prompt.
pub fn all_memories_context(state: &AppState) -> String {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let mut stmt = match db.prepare(
        "SELECT key, content FROM memories ORDER BY updated_at DESC LIMIT 50",
    ) {
        Ok(s) => s,
        Err(_) => return String::new(),
    };
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .ok();

    let mut out = String::new();
    if let Some(rows) = rows {
        for row in rows.flatten() {
            out.push_str(&format!("- **{}**: {}\n", row.0, row.1));
        }
    }
    out
}
