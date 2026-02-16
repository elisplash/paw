// Paw Agent Engine — Session Manager
// Stores conversation history in SQLite via rusqlite.
// Independent of the Tauri SQL plugin — uses its own connection pool
// for the engine's data, separate from the frontend's paw.db.

use crate::engine::types::*;
use log::{info, warn, error};
use rusqlite::{Connection, params};
use std::path::PathBuf;
use std::sync::Mutex;

/// Get the path to the engine's SQLite database.
fn engine_db_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_default();
    let dir = home.join(".paw");
    std::fs::create_dir_all(&dir).ok();
    dir.join("engine.db")
}

/// Thread-safe database wrapper.
pub struct SessionStore {
    conn: Mutex<Connection>,
}

impl SessionStore {
    /// Open (or create) the engine database and initialize tables.
    pub fn open() -> Result<Self, String> {
        let path = engine_db_path();
        info!("[engine] Opening session store at {:?}", path);

        let conn = Connection::open(&path)
            .map_err(|e| format!("Failed to open engine DB: {}", e))?;

        // Enable WAL mode for better concurrent read performance
        conn.execute_batch("PRAGMA journal_mode=WAL;").ok();

        // Create tables
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                label TEXT,
                model TEXT NOT NULL DEFAULT '',
                system_prompt TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                message_count INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL DEFAULT '',
                tool_calls_json TEXT,
                tool_call_id TEXT,
                name TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_messages_session
                ON messages(session_id, created_at);

            CREATE TABLE IF NOT EXISTS engine_config (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS agent_files (
                agent_id TEXT NOT NULL,
                file_name TEXT NOT NULL,
                content TEXT NOT NULL DEFAULT '',
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (agent_id, file_name)
            );

            CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                category TEXT NOT NULL DEFAULT 'general',
                importance INTEGER NOT NULL DEFAULT 5,
                embedding BLOB,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_memories_category
                ON memories(category);
        ").map_err(|e| format!("Failed to create tables: {}", e))?;

        Ok(SessionStore { conn: Mutex::new(conn) })
    }

    // ── Session CRUD ───────────────────────────────────────────────────

    pub fn create_session(&self, id: &str, model: &str, system_prompt: Option<&str>) -> Result<Session, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;

        conn.execute(
            "INSERT INTO sessions (id, model, system_prompt) VALUES (?1, ?2, ?3)",
            params![id, model, system_prompt],
        ).map_err(|e| format!("Failed to create session: {}", e))?;

        Ok(Session {
            id: id.to_string(),
            label: None,
            model: model.to_string(),
            system_prompt: system_prompt.map(|s| s.to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            message_count: 0,
        })
    }

    pub fn list_sessions(&self, limit: i64) -> Result<Vec<Session>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;

        let mut stmt = conn.prepare(
            "SELECT id, label, model, system_prompt, created_at, updated_at, message_count
             FROM sessions ORDER BY updated_at DESC LIMIT ?1"
        ).map_err(|e| format!("Prepare error: {}", e))?;

        let sessions = stmt.query_map(params![limit], |row| {
            Ok(Session {
                id: row.get(0)?,
                label: row.get(1)?,
                model: row.get(2)?,
                system_prompt: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                message_count: row.get(6)?,
            })
        }).map_err(|e| format!("Query error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(sessions)
    }

    pub fn get_session(&self, id: &str) -> Result<Option<Session>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;

        let result = conn.query_row(
            "SELECT id, label, model, system_prompt, created_at, updated_at, message_count
             FROM sessions WHERE id = ?1",
            params![id],
            |row| {
                Ok(Session {
                    id: row.get(0)?,
                    label: row.get(1)?,
                    model: row.get(2)?,
                    system_prompt: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                    message_count: row.get(6)?,
                })
            },
        );

        match result {
            Ok(session) => Ok(Some(session)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("Query error: {}", e)),
        }
    }

    pub fn rename_session(&self, id: &str, label: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        conn.execute(
            "UPDATE sessions SET label = ?1, updated_at = datetime('now') WHERE id = ?2",
            params![label, id],
        ).map_err(|e| format!("Update error: {}", e))?;
        Ok(())
    }

    pub fn delete_session(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        conn.execute("DELETE FROM messages WHERE session_id = ?1", params![id])
            .map_err(|e| format!("Delete messages error: {}", e))?;
        conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])
            .map_err(|e| format!("Delete session error: {}", e))?;
        Ok(())
    }

    // ── Message CRUD ───────────────────────────────────────────────────

    pub fn add_message(&self, msg: &StoredMessage) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;

        conn.execute(
            "INSERT INTO messages (id, session_id, role, content, tool_calls_json, tool_call_id, name)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                msg.id,
                msg.session_id,
                msg.role,
                msg.content,
                msg.tool_calls_json,
                msg.tool_call_id,
                msg.name,
            ],
        ).map_err(|e| format!("Insert message error: {}", e))?;

        // Update session stats
        conn.execute(
            "UPDATE sessions SET
                message_count = (SELECT COUNT(*) FROM messages WHERE session_id = ?1),
                updated_at = datetime('now')
             WHERE id = ?1",
            params![msg.session_id],
        ).map_err(|e| format!("Update session error: {}", e))?;

        Ok(())
    }

    pub fn get_messages(&self, session_id: &str, limit: i64) -> Result<Vec<StoredMessage>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;

        let mut stmt = conn.prepare(
            "SELECT id, session_id, role, content, tool_calls_json, tool_call_id, name, created_at
             FROM messages WHERE session_id = ?1 ORDER BY created_at ASC LIMIT ?2"
        ).map_err(|e| format!("Prepare error: {}", e))?;

        let messages = stmt.query_map(params![session_id, limit], |row| {
            Ok(StoredMessage {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                tool_calls_json: row.get(4)?,
                tool_call_id: row.get(5)?,
                name: row.get(6)?,
                created_at: row.get(7)?,
            })
        }).map_err(|e| format!("Query error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(messages)
    }

    /// Convert stored messages to engine Message types for sending to AI provider.
    pub fn load_conversation(&self, session_id: &str, system_prompt: Option<&str>) -> Result<Vec<Message>, String> {
        // Load messages with a reasonable limit. We'll further truncate by
        // estimated token count below to avoid exceeding model context windows.
        let stored = self.get_messages(session_id, 500)?;
        let mut messages = Vec::new();

        // Add system prompt if provided
        if let Some(prompt) = system_prompt {
            messages.push(Message {
                role: Role::System,
                content: MessageContent::Text(prompt.to_string()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }

        for sm in &stored {
            let role = match sm.role.as_str() {
                "system" => Role::System,
                "user" => Role::User,
                "assistant" => Role::Assistant,
                "tool" => Role::Tool,
                _ => Role::User,
            };

            let tool_calls: Option<Vec<ToolCall>> = sm.tool_calls_json.as_ref()
                .and_then(|json| serde_json::from_str(json).ok());

            messages.push(Message {
                role,
                content: MessageContent::Text(sm.content.clone()),
                tool_calls,
                tool_call_id: sm.tool_call_id.clone(),
                name: sm.name.clone(),
            });
        }

        // ── Context window truncation ──────────────────────────────────
        // Estimate tokens (~4 chars per token) and keep only the most recent
        // messages that fit within ~100k tokens to leave room for the response.
        // Always keep the system prompt (first message) and the last user message.
        const MAX_CONTEXT_TOKENS: usize = 100_000;
        let estimate_tokens = |m: &Message| -> usize {
            let text_len = match &m.content {
                MessageContent::Text(t) => t.len(),
                MessageContent::Blocks(blocks) => blocks.iter().map(|b| match b {
                    ContentBlock::Text { text } => text.len(),
                    ContentBlock::ImageUrl { .. } => 1000, // rough estimate for images
                }).sum(),
            };
            let tc_len = m.tool_calls.as_ref().map(|tcs| {
                tcs.iter().map(|tc| tc.function.arguments.len() + tc.function.name.len() + 20).sum::<usize>()
            }).unwrap_or(0);
            (text_len + tc_len) / 4 + 4 // +4 for role/overhead tokens
        };

        let total_tokens: usize = messages.iter().map(|m| estimate_tokens(m)).sum();
        if total_tokens > MAX_CONTEXT_TOKENS && messages.len() > 2 {
            // Keep system prompt (index 0) and trim oldest non-system messages
            let system_msg = if !messages.is_empty() && messages[0].role == Role::System {
                Some(messages.remove(0))
            } else {
                None
            };

            // Drop from the front (oldest) until we fit
            let mut running_tokens: usize = system_msg.as_ref().map(|m| estimate_tokens(m)).unwrap_or(0);
            let mut keep_from = 0;
            let msg_tokens: Vec<usize> = messages.iter().map(|m| estimate_tokens(m)).collect();
            let total_msg_tokens: usize = msg_tokens.iter().sum();
            let mut drop_tokens = running_tokens + total_msg_tokens;

            for (i, &t) in msg_tokens.iter().enumerate() {
                if drop_tokens <= MAX_CONTEXT_TOKENS {
                    break;
                }
                drop_tokens -= t;
                keep_from = i + 1;
            }
            let _ = running_tokens; // suppress unused warning

            messages = messages.split_off(keep_from);

            // Re-insert system prompt at the front
            if let Some(sys) = system_msg {
                messages.insert(0, sys);
            }

            log::info!("[engine] Context truncated: kept {} messages (~{} tokens, was ~{})",
                messages.len(), drop_tokens, total_tokens);
        }

        Ok(messages)
    }

    // ── Config storage ─────────────────────────────────────────────────

    pub fn get_config(&self, key: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        let result = conn.query_row(
            "SELECT value FROM engine_config WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("Config read error: {}", e)),
        }
    }

    pub fn set_config(&self, key: &str, value: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        conn.execute(
            "INSERT OR REPLACE INTO engine_config (key, value) VALUES (?1, ?2)",
            params![key, value],
        ).map_err(|e| format!("Config write error: {}", e))?;
        Ok(())
    }

    // ── Agent Files (Soul / Persona) ───────────────────────────────────

    pub fn list_agent_files(&self, agent_id: &str) -> Result<Vec<AgentFile>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        let mut stmt = conn.prepare(
            "SELECT agent_id, file_name, content, updated_at FROM agent_files WHERE agent_id = ?1 ORDER BY file_name"
        ).map_err(|e| format!("Prepare error: {}", e))?;

        let files = stmt.query_map(params![agent_id], |row| {
            Ok(AgentFile {
                agent_id: row.get(0)?,
                file_name: row.get(1)?,
                content: row.get(2)?,
                updated_at: row.get(3)?,
            })
        }).map_err(|e| format!("Query error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(files)
    }

    pub fn get_agent_file(&self, agent_id: &str, file_name: &str) -> Result<Option<AgentFile>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        let result = conn.query_row(
            "SELECT agent_id, file_name, content, updated_at FROM agent_files WHERE agent_id = ?1 AND file_name = ?2",
            params![agent_id, file_name],
            |row| Ok(AgentFile {
                agent_id: row.get(0)?,
                file_name: row.get(1)?,
                content: row.get(2)?,
                updated_at: row.get(3)?,
            }),
        );
        match result {
            Ok(f) => Ok(Some(f)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(format!("Query error: {}", e)),
        }
    }

    pub fn set_agent_file(&self, agent_id: &str, file_name: &str, content: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        conn.execute(
            "INSERT OR REPLACE INTO agent_files (agent_id, file_name, content, updated_at)
             VALUES (?1, ?2, ?3, datetime('now'))",
            params![agent_id, file_name, content],
        ).map_err(|e| format!("Write error: {}", e))?;
        Ok(())
    }

    pub fn delete_agent_file(&self, agent_id: &str, file_name: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        conn.execute(
            "DELETE FROM agent_files WHERE agent_id = ?1 AND file_name = ?2",
            params![agent_id, file_name],
        ).map_err(|e| format!("Delete error: {}", e))?;
        Ok(())
    }

    /// Load all agent files for a given agent and compose them into a single system prompt block.
    /// Returns None if no agent files exist.
    pub fn compose_agent_context(&self, agent_id: &str) -> Result<Option<String>, String> {
        let files = self.list_agent_files(agent_id)?;
        if files.is_empty() {
            return Ok(None);
        }
        // Compose in a specific order: IDENTITY → SOUL → USER → AGENTS → TOOLS
        let order = ["IDENTITY.md", "SOUL.md", "USER.md", "AGENTS.md", "TOOLS.md"];
        let mut sections = Vec::new();
        for name in &order {
            if let Some(f) = files.iter().find(|f| f.file_name == *name) {
                if !f.content.trim().is_empty() {
                    sections.push(f.content.clone());
                }
            }
        }
        // Also include any non-standard files
        for f in &files {
            if !order.contains(&f.file_name.as_str()) && !f.content.trim().is_empty() {
                sections.push(f.content.clone());
            }
        }
        if sections.is_empty() {
            return Ok(None);
        }
        Ok(Some(sections.join("\n\n---\n\n")))
    }

    // ── Memory CRUD ────────────────────────────────────────────────────

    pub fn store_memory(&self, id: &str, content: &str, category: &str, importance: u8, embedding: Option<&[u8]>) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        conn.execute(
            "INSERT OR REPLACE INTO memories (id, content, category, importance, embedding)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, content, category, importance as i32, embedding],
        ).map_err(|e| format!("Memory store error: {}", e))?;
        Ok(())
    }

    pub fn delete_memory(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        conn.execute("DELETE FROM memories WHERE id = ?1", params![id])
            .map_err(|e| format!("Memory delete error: {}", e))?;
        Ok(())
    }

    pub fn memory_stats(&self) -> Result<MemoryStats, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;

        let total: i64 = conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))
            .map_err(|e| format!("Count error: {}", e))?;

        let has_embeddings: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM memories WHERE embedding IS NOT NULL", [], |r| r.get(0)
        ).unwrap_or(false);

        let mut stmt = conn.prepare(
            "SELECT category, COUNT(*) FROM memories GROUP BY category ORDER BY COUNT(*) DESC"
        ).map_err(|e| format!("Prepare error: {}", e))?;

        let categories: Vec<(String, i64)> = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        }).map_err(|e| format!("Query error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(MemoryStats { total_memories: total, categories, has_embeddings })
    }

    /// Search memories by cosine similarity against a query embedding.
    /// Falls back to keyword search if no embeddings are stored.
    pub fn search_memories_by_embedding(&self, query_embedding: &[f32], limit: usize, threshold: f64) -> Result<Vec<Memory>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;

        let mut stmt = conn.prepare(
            "SELECT id, content, category, importance, embedding, created_at FROM memories WHERE embedding IS NOT NULL"
        ).map_err(|e| format!("Prepare error: {}", e))?;

        let mut scored: Vec<(Memory, f64)> = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let content: String = row.get(1)?;
            let category: String = row.get(2)?;
            let importance: i32 = row.get(3)?;
            let embedding_blob: Vec<u8> = row.get(4)?;
            let created_at: String = row.get(5)?;
            Ok((id, content, category, importance as u8, embedding_blob, created_at))
        }).map_err(|e| format!("Query error: {}", e))?
        .filter_map(|r| r.ok())
        .filter_map(|(id, content, category, importance, blob, created_at)| {
            let stored_emb = bytes_to_f32_vec(&blob);
            let score = cosine_similarity(query_embedding, &stored_emb);
            if score >= threshold {
                Some((Memory { id, content, category, importance, created_at, score: Some(score) }, score))
            } else {
                None
            }
        })
        .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        Ok(scored.into_iter().map(|(m, _)| m).collect())
    }

    /// Keyword-based fallback search (no embeddings needed).
    pub fn search_memories_keyword(&self, query: &str, limit: usize) -> Result<Vec<Memory>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;

        let pattern = format!("%{}%", query.to_lowercase());
        let mut stmt = conn.prepare(
            "SELECT id, content, category, importance, created_at FROM memories
             WHERE LOWER(content) LIKE ?1
             ORDER BY importance DESC, created_at DESC
             LIMIT ?2"
        ).map_err(|e| format!("Prepare error: {}", e))?;

        let memories = stmt.query_map(params![pattern, limit as i64], |row| {
            Ok(Memory {
                id: row.get(0)?,
                content: row.get(1)?,
                category: row.get(2)?,
                importance: {
                    let i: i32 = row.get(3)?;
                    i as u8
                },
                created_at: row.get(4)?,
                score: None,
            })
        }).map_err(|e| format!("Query error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(memories)
    }

    /// Get all memories (for export / listing), newest first.
    pub fn list_memories(&self, limit: usize) -> Result<Vec<Memory>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        let mut stmt = conn.prepare(
            "SELECT id, content, category, importance, created_at FROM memories
             ORDER BY created_at DESC LIMIT ?1"
        ).map_err(|e| format!("Prepare error: {}", e))?;

        let memories = stmt.query_map(params![limit as i64], |row| {
            Ok(Memory {
                id: row.get(0)?,
                content: row.get(1)?,
                category: row.get(2)?,
                importance: {
                    let i: i32 = row.get(3)?;
                    i as u8
                },
                created_at: row.get(4)?,
                score: None,
            })
        }).map_err(|e| format!("Query error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(memories)
    }
}

// ── Vector math utilities ──────────────────────────────────────────────

/// Convert a byte slice (from SQLite BLOB) to a Vec<f32>.
fn bytes_to_f32_vec(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

/// Convert a Vec<f32> to bytes for SQLite BLOB storage.
pub fn f32_vec_to_bytes(vec: &[f32]) -> Vec<u8> {
    vec.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Cosine similarity between two vectors. Returns 0.0 if either is zero-length.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;
    for (x, y) in a.iter().zip(b.iter()) {
        let x = *x as f64;
        let y = *y as f64;
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom < 1e-12 {
        0.0
    } else {
        dot / denom
    }
}
