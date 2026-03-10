// pawz-code — engram.rs
// Engram: compressed higher-order understanding of a codebase.
// Separate from memory.rs which stores facts — engram stores compressed understanding:
//   - architecture maps
//   - module relationships
//   - key entrypoints
//   - high-value patterns
//   - stable summaries useful across sessions
//
// Storage: engram table in ~/.pawz-code/memory.db (scoped by workspace/repo)

use crate::state::AppState;
use anyhow::Result;

// ── Write ────────────────────────────────────────────────────────────────────

/// Upsert an engram entry. `scope` is typically a workspace path or repo name.
/// `kind` is one of: "architecture", "module", "pattern", "summary", "entrypoint"
pub fn store(
    state: &AppState,
    scope: &str,
    key: &str,
    content: &str,
    kind: &str,
) -> Result<()> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    db.execute(
        r#"INSERT INTO engram (scope, key, content, kind)
           VALUES (?1, ?2, ?3, ?4)
           ON CONFLICT(scope, key) DO UPDATE SET
               content    = excluded.content,
               kind       = excluded.kind,
               updated_at = datetime('now')"#,
        rusqlite::params![scope, key, content, kind],
    )?;
    Ok(())
}

// ── Read ─────────────────────────────────────────────────────────────────────

/// Search engram entries by keyword. Optionally filter by scope.
pub fn search(
    state: &AppState,
    query: &str,
    scope: Option<&str>,
) -> Result<Vec<serde_json::Value>> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let pattern = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));

    let mut out = Vec::new();

    if let Some(sc) = scope {
        let mut stmt = db.prepare(
            "SELECT scope, key, content, kind, updated_at FROM engram \
             WHERE scope = ?1 AND (key LIKE ?2 ESCAPE '\\' OR content LIKE ?2 ESCAPE '\\') \
             ORDER BY updated_at DESC LIMIT 30",
        )?;
        let rows = stmt.query_map(rusqlite::params![sc, pattern], row_to_json)?;
        for row in rows {
            out.push(row?);
        }
    } else {
        let mut stmt = db.prepare(
            "SELECT scope, key, content, kind, updated_at FROM engram \
             WHERE key LIKE ?1 ESCAPE '\\' OR content LIKE ?1 ESCAPE '\\' \
             ORDER BY updated_at DESC LIMIT 30",
        )?;
        let rows = stmt.query_map(rusqlite::params![pattern], row_to_json)?;
        for row in rows {
            out.push(row?);
        }
    }

    Ok(out)
}

fn row_to_json(row: &rusqlite::Row) -> rusqlite::Result<serde_json::Value> {
    Ok(serde_json::json!({
        "scope":      row.get::<_, String>(0)?,
        "key":        row.get::<_, String>(1)?,
        "content":    row.get::<_, String>(2)?,
        "kind":       row.get::<_, String>(3)?,
        "updated_at": row.get::<_, String>(4)?,
    }))
}

/// Load all engram entries for a scope as a context block.
pub fn scope_context(state: &AppState, scope: &str) -> String {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let mut stmt = match db.prepare(
        "SELECT key, content, kind FROM engram WHERE scope = ?1 ORDER BY kind, updated_at DESC LIMIT 50",
    ) {
        Ok(s) => s,
        Err(_) => return String::new(),
    };
    let rows = stmt
        .query_map(rusqlite::params![scope], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .ok();

    let mut out = String::new();
    if let Some(rows) = rows {
        for row in rows.flatten() {
            out.push_str(&format!("[{}] **{}**: {}\n", row.2, row.0, row.1));
        }
    }
    out
}

/// Count total engram entries.
pub fn engram_count(state: &AppState) -> Result<i64> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    let count: i64 = db.query_row("SELECT COUNT(*) FROM engram", [], |r| r.get(0))?;
    Ok(count)
}

/// Delete an engram entry.
pub fn delete(state: &AppState, scope: &str, key: &str) -> Result<()> {
    let db = state.db.lock().unwrap_or_else(|e| e.into_inner());
    db.execute(
        "DELETE FROM engram WHERE scope = ?1 AND key = ?2",
        rusqlite::params![scope, key],
    )?;
    Ok(())
}
