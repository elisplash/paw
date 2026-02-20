// Paw Agent Engine — Session Manager
// Stores conversation history in SQLite via rusqlite.
// Independent of the Tauri SQL plugin — uses its own connection pool
// for the engine's data, separate from the frontend's paw.db.

use crate::engine::types::*;
use chrono::Utc;
use log::{info, warn};
use rusqlite::{Connection, params};
use std::path::PathBuf;
use parking_lot::Mutex;

/// Get the path to the engine's SQLite database.
pub fn engine_db_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_default();
    let dir = home.join(".paw");
    std::fs::create_dir_all(&dir).ok();
    dir.join("engine.db")
}

/// Thread-safe database wrapper.
pub struct SessionStore {
    pub(crate) conn: Mutex<Connection>,
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

        // ── Pre-migration: detect stale project_agents schema ───────────
        // Older versions created project_agents with (id INTEGER PK, project_id INTEGER,
        // name TEXT, …) which is incompatible with the current (project_id TEXT,
        // agent_id TEXT, …) composite-PK schema.  Detect the old layout by checking
        // for the presence of a `name` column (the new schema has no such column)
        // and DROP + recreate so CREATE TABLE IF NOT EXISTS picks up the new DDL.
        {
            let has_old_schema = conn
                .prepare("SELECT name FROM pragma_table_info('project_agents') WHERE name = 'name'")
                .and_then(|mut stmt| stmt.query_row([], |_row| Ok(true)))
                .unwrap_or(false);

            if has_old_schema {
                warn!("[engine] Detected legacy project_agents schema — migrating to composite-PK layout");
                conn.execute_batch("DROP TABLE IF EXISTS project_agents;").ok();
            }
        }

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

            CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'inbox',
                priority TEXT NOT NULL DEFAULT 'medium',
                assigned_agent TEXT,
                session_id TEXT,
                model TEXT,
                cron_schedule TEXT,
                cron_enabled INTEGER NOT NULL DEFAULT 0,
                last_run_at TEXT,
                next_run_at TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);
            CREATE INDEX IF NOT EXISTS idx_tasks_agent ON tasks(assigned_agent);

            CREATE TABLE IF NOT EXISTS task_activity (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                agent TEXT,
                content TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_task_activity_task ON task_activity(task_id, created_at DESC);

            CREATE TABLE IF NOT EXISTS task_agents (
                task_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                role TEXT NOT NULL DEFAULT 'collaborator',
                added_at TEXT NOT NULL DEFAULT (datetime('now')),
                PRIMARY KEY (task_id, agent_id),
                FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
            );

            -- ═══ Orchestrator: Projects & Message Bus ═══

            CREATE TABLE IF NOT EXISTS projects (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                goal TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'planning',
                boss_agent TEXT NOT NULL DEFAULT 'default',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS project_agents (
                project_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                role TEXT NOT NULL DEFAULT 'worker',
                specialty TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'idle',
                current_task TEXT,
                model TEXT,
                PRIMARY KEY (project_id, agent_id),
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS project_messages (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL,
                from_agent TEXT NOT NULL,
                to_agent TEXT,
                kind TEXT NOT NULL DEFAULT 'message',
                content TEXT NOT NULL DEFAULT '',
                metadata TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_project_messages
                ON project_messages(project_id, created_at);

            -- ═══ Trading: Trade History & Auto-Trade Policy ═══

            CREATE TABLE IF NOT EXISTS trade_history (
                id TEXT PRIMARY KEY,
                trade_type TEXT NOT NULL,
                side TEXT,
                product_id TEXT,
                currency TEXT,
                amount TEXT NOT NULL,
                order_type TEXT,
                order_id TEXT,
                status TEXT NOT NULL DEFAULT 'completed',
                usd_value TEXT,
                to_address TEXT,
                reason TEXT NOT NULL DEFAULT '',
                session_id TEXT,
                agent_id TEXT,
                raw_response TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_trade_history_created
                ON trade_history(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_trade_history_type
                ON trade_history(trade_type, created_at DESC);
        ").map_err(|e| format!("Failed to create tables: {}", e))?;

        // ── Migrations: add columns to existing tables ──────────────────
        // SQLite ignores ALTER TABLE ADD COLUMN if it already exists (we catch the error).
        conn.execute("ALTER TABLE project_agents ADD COLUMN model TEXT", []).ok();
        conn.execute("ALTER TABLE project_agents ADD COLUMN system_prompt TEXT", []).ok();
        conn.execute("ALTER TABLE project_agents ADD COLUMN capabilities TEXT NOT NULL DEFAULT ''", []).ok();

        // Add agent_id column to sessions (for per-agent session isolation)
        conn.execute("ALTER TABLE sessions ADD COLUMN agent_id TEXT", []).ok();
        conn.execute("CREATE INDEX IF NOT EXISTS idx_sessions_agent ON sessions(agent_id)", []).ok();

        // ── Positions table: stop-loss / take-profit tracking ────────────
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS positions (
                id TEXT PRIMARY KEY,
                mint TEXT NOT NULL,
                symbol TEXT NOT NULL,
                entry_price_usd REAL NOT NULL DEFAULT 0.0,
                entry_sol REAL NOT NULL DEFAULT 0.0,
                amount REAL NOT NULL DEFAULT 0.0,
                current_amount REAL NOT NULL DEFAULT 0.0,
                stop_loss_pct REAL NOT NULL DEFAULT 0.30,
                take_profit_pct REAL NOT NULL DEFAULT 2.0,
                status TEXT NOT NULL DEFAULT 'open',
                last_price_usd REAL NOT NULL DEFAULT 0.0,
                last_checked_at TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                closed_at TEXT,
                close_tx TEXT,
                agent_id TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_positions_status ON positions(status);
            CREATE INDEX IF NOT EXISTS idx_positions_mint ON positions(mint);
        ").ok();

        // ── Phase 2: Memory Intelligence migrations ──────────────────────
        // Add agent_id column to memories (for per-agent memory scope)
        conn.execute("ALTER TABLE memories ADD COLUMN agent_id TEXT NOT NULL DEFAULT ''", []).ok();

        // Create FTS5 virtual table for BM25 full-text search
        conn.execute_batch("
            CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                id UNINDEXED,
                content,
                category UNINDEXED,
                agent_id UNINDEXED,
                content_rowid=rowid
            );
        ").ok();

        // Populate FTS index with existing memories that aren't indexed yet
        conn.execute_batch("
            INSERT OR IGNORE INTO memories_fts(id, content, category, agent_id)
            SELECT id, content, category, COALESCE(agent_id, '')
            FROM memories
            WHERE id NOT IN (SELECT id FROM memories_fts);
        ").ok();

        // Ensure the _standalone sentinel project exists so that
        // user-created agents (via create_agent tool) satisfy the FK constraint.
        conn.execute(
            "INSERT OR IGNORE INTO projects (id, title, goal, status, boss_agent)
             VALUES ('_standalone', 'Standalone Agents', 'Container for user-created agents', 'active', 'system')",
            [],
        ).map_err(|e| format!("Failed to seed _standalone project: {}", e))?;

        // One-time dedup (runs once, guarded by flag): remove duplicate messages
        // caused by a historical bug that re-inserted messages on every agent turn.
        // The underlying bug is fixed (pre_loop_msg_count guard in commands.rs).
        let already_deduped: bool = conn.query_row(
            "SELECT COUNT(*) FROM engine_config WHERE key = 'migration_dedup_done'",
            [], |r| r.get::<_, i64>(0),
        ).unwrap_or(0) > 0;

        if !already_deduped {
            let deduped = conn.execute(
                "DELETE FROM messages WHERE id NOT IN (
                    SELECT MIN(id) FROM messages
                    GROUP BY session_id, role, content, tool_call_id
                )",
                [],
            ).unwrap_or(0);
            if deduped > 0 {
                info!("[engine] Deduplication: removed {} duplicate messages", deduped);
                conn.execute_batch(
                    "UPDATE sessions SET message_count = (
                        SELECT COUNT(*) FROM messages WHERE messages.session_id = sessions.id
                    )"
                ).ok();
            }
            // Mark migration as done so it never runs again
            conn.execute(
                "INSERT OR REPLACE INTO engine_config (key, value) VALUES ('migration_dedup_done', '1')",
                [],
            ).ok();
        }

        Ok(SessionStore { conn: Mutex::new(conn) })
    }

    // ── Session CRUD ───────────────────────────────────────────────────

    pub fn create_session(&self, id: &str, model: &str, system_prompt: Option<&str>, agent_id: Option<&str>) -> Result<Session, String> {
        let conn = self.conn.lock();

        conn.execute(
            "INSERT INTO sessions (id, model, system_prompt, agent_id) VALUES (?1, ?2, ?3, ?4)",
            params![id, model, system_prompt, agent_id],
        ).map_err(|e| format!("Failed to create session: {}", e))?;

        Ok(Session {
            id: id.to_string(),
            label: None,
            model: model.to_string(),
            system_prompt: system_prompt.map(|s| s.to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            message_count: 0,
            agent_id: agent_id.map(|s| s.to_string()),
        })
    }

    pub fn list_sessions(&self, limit: i64) -> Result<Vec<Session>, String> {
        self.list_sessions_filtered(limit, None)
    }

    /// List sessions, optionally filtered by agent_id.
    pub fn list_sessions_filtered(&self, limit: i64, agent_id: Option<&str>) -> Result<Vec<Session>, String> {
        let conn = self.conn.lock();

        let (sql, params_vec): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(aid) = agent_id {
            (
                "SELECT id, label, model, system_prompt, created_at, updated_at, message_count, agent_id \
                 FROM sessions WHERE agent_id = ?1 ORDER BY updated_at DESC LIMIT ?2".to_string(),
                vec![Box::new(aid.to_string()) as Box<dyn rusqlite::types::ToSql>, Box::new(limit)],
            )
        } else {
            (
                "SELECT id, label, model, system_prompt, created_at, updated_at, message_count, agent_id \
                 FROM sessions ORDER BY updated_at DESC LIMIT ?1".to_string(),
                vec![Box::new(limit) as Box<dyn rusqlite::types::ToSql>],
            )
        };

        let mut stmt = conn.prepare(&sql).map_err(|e| format!("Prepare error: {}", e))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();

        let sessions = stmt.query_map(param_refs.as_slice(), |row| {
            Ok(Session {
                id: row.get(0)?,
                label: row.get(1)?,
                model: row.get(2)?,
                system_prompt: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
                message_count: row.get(6)?,
                agent_id: row.get(7)?,
            })
        }).map_err(|e| format!("Query error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(sessions)
    }

    pub fn get_session(&self, id: &str) -> Result<Option<Session>, String> {
        let conn = self.conn.lock();

        let result = conn.query_row(
            "SELECT id, label, model, system_prompt, created_at, updated_at, message_count, agent_id
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
                    agent_id: row.get(7)?,
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
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE sessions SET label = ?1, updated_at = datetime('now') WHERE id = ?2",
            params![label, id],
        ).map_err(|e| format!("Update error: {}", e))?;
        Ok(())
    }

    pub fn delete_session(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM messages WHERE session_id = ?1", params![id])
            .map_err(|e| format!("Delete messages error: {}", e))?;
        conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])
            .map_err(|e| format!("Delete session error: {}", e))?;
        Ok(())
    }

    /// Clear all messages for a session but keep the session itself.
    pub fn clear_messages(&self, session_id: &str) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM messages WHERE session_id = ?1", params![session_id])
            .map_err(|e| format!("Delete messages error: {}", e))?;
        conn.execute(
            "UPDATE sessions SET message_count = 0, updated_at = datetime('now') WHERE id = ?1",
            params![session_id],
        ).map_err(|e| format!("Update session error: {}", e))?;
        info!("[engine] Cleared all messages for session {}", session_id);
        Ok(())
    }

    /// Bulk-delete sessions with 0 messages that are older than `max_age_secs`.
    /// Skips the `exclude_id` session (the user's current session).
    /// Returns the number of sessions deleted.
    pub fn cleanup_empty_sessions(&self, max_age_secs: i64, exclude_id: Option<&str>) -> Result<usize, String> {
        let conn = self.conn.lock();
        let deleted = if let Some(eid) = exclude_id {
            conn.execute(
                "DELETE FROM sessions WHERE message_count = 0 \
                 AND updated_at < datetime('now', ?1) \
                 AND id != ?2",
                params![format!("-{} seconds", max_age_secs), eid],
            )
        } else {
            conn.execute(
                "DELETE FROM sessions WHERE message_count = 0 \
                 AND updated_at < datetime('now', ?1)",
                params![format!("-{} seconds", max_age_secs)],
            )
        }.map_err(|e| format!("Cleanup error: {}", e))?;

        if deleted > 0 {
            info!("[engine] Cleaned up {} empty session(s) older than {}s", deleted, max_age_secs);
        }
        Ok(deleted)
    }

    /// Prune a session's message history, keeping only the most recent `keep`
    /// messages.  Used by the cron heartbeat to prevent context accumulation
    /// across recurring task runs — the #1 cause of runaway token costs.
    ///
    /// Returns the number of messages deleted.
    pub fn prune_session_messages(&self, session_id: &str, keep: i64) -> Result<usize, String> {
        let conn = self.conn.lock();

        // Count current messages
        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE session_id = ?1",
            params![session_id],
            |r| r.get(0),
        ).map_err(|e| format!("Count error: {}", e))?;

        if total <= keep {
            return Ok(0);
        }

        // Delete oldest messages, keeping the most recent `keep`
        let deleted = conn.execute(
            "DELETE FROM messages WHERE session_id = ?1 AND id NOT IN (
                SELECT id FROM messages WHERE session_id = ?1
                ORDER BY created_at DESC LIMIT ?2
            )",
            params![session_id, keep],
        ).map_err(|e| format!("Prune error: {}", e))?;

        // Update session message count
        conn.execute(
            "UPDATE sessions SET
                message_count = (SELECT COUNT(*) FROM messages WHERE session_id = ?1),
                updated_at = datetime('now')
             WHERE id = ?1",
            params![session_id],
        ).map_err(|e| format!("Update session error: {}", e))?;

        if deleted > 0 {
            info!("[engine] Pruned {} old messages from session {} (kept {})", deleted, session_id, keep);
        }

        Ok(deleted)
    }

    // ── Message CRUD ───────────────────────────────────────────────────

    pub fn add_message(&self, msg: &StoredMessage) -> Result<(), String> {
        let conn = self.conn.lock();

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
        let conn = self.conn.lock();

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
        // Load recent messages only — lean sessions rely on memory_search for
        // historical context rather than carrying the full conversation.
        let stored = self.get_messages(session_id, 50)?;
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
        // messages that fit within ~16k tokens to leave room for the response.
        // With lean session init (core soul files only + today's memories),
        // 16k tokens is plenty of context. At $3/MTok (Sonnet), 16k = $0.048
        // per round vs $0.09 at 30k. Agent uses memory_search for deeper context.
        // Always keep system prompt (first message).
        const MAX_CONTEXT_TOKENS: usize = 16_000;
        let estimate_tokens = |m: &Message| -> usize {
            let text_len = match &m.content {
                MessageContent::Text(t) => t.len(),
                MessageContent::Blocks(blocks) => blocks.iter().map(|b| match b {
                    ContentBlock::Text { text } => text.len(),
                    ContentBlock::ImageUrl { .. } => 1000, // rough estimate for images
                    ContentBlock::Document { data, .. } => data.len() / 4, // rough: base64 → chars
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

            // Drop from the front (oldest) until we fit, but ALWAYS keep the
            // last user message so the provider gets non-empty contents.
            let running_tokens: usize = system_msg.as_ref().map(|m| estimate_tokens(m)).unwrap_or(0);
            let mut keep_from = 0;
            let msg_tokens: Vec<usize> = messages.iter().map(|m| estimate_tokens(m)).collect();
            let total_msg_tokens: usize = msg_tokens.iter().sum();
            let mut drop_tokens = running_tokens + total_msg_tokens;

            // Find the last user message index — we must never drop past it
            let last_user_idx = messages.iter().rposition(|m| m.role == Role::User)
                .unwrap_or(messages.len().saturating_sub(1));

            for (i, &t) in msg_tokens.iter().enumerate() {
                if drop_tokens <= MAX_CONTEXT_TOKENS {
                    break;
                }
                // Never drop past the last user message
                if i >= last_user_idx {
                    break;
                }
                drop_tokens -= t;
                keep_from = i + 1;
            }

            messages = messages.split_off(keep_from);

            // Re-insert system prompt at the front
            if let Some(sys) = system_msg {
                messages.insert(0, sys);
            }

            log::info!("[engine] Context truncated: kept {} messages (~{} tokens, was ~{})",
                messages.len(), drop_tokens, total_tokens);
        }

        // ── Sanitize tool_use / tool_result pairing ────────────────────
        // After truncation (or corruption from previous crashes), ensure every
        // assistant message with tool_calls has matching tool_result messages.
        // The Anthropic API returns 400 if tool_use IDs appear without a
        // corresponding tool_result immediately after.
        Self::sanitize_tool_pairs(&mut messages);

        Ok(messages)
    }

    /// Ensure every assistant message with tool_calls has matching tool_result
    /// messages immediately after it.  Orphaned tool_use IDs (from context
    /// truncation or prior crashes) cause Anthropic to return HTTP 400.
    ///
    /// Strategy:
    /// 1. Remove leading orphaned tool-result messages that have no preceding
    ///    assistant message with tool_calls.
    /// 2. For each assistant message with tool_calls, collect the set of
    ///    tool_call IDs and check the immediately following messages.  Inject
    ///    a synthetic tool_result for any missing ID.
    fn sanitize_tool_pairs(messages: &mut Vec<Message>) {
        use std::collections::HashSet;

        // ── Pass 1: strip leading orphan tool results ──────────────────
        // After truncation the first non-system messages might be tool results
        // whose parent assistant message was dropped.
        let first_non_system = messages.iter().position(|m| m.role != Role::System).unwrap_or(0);
        let mut strip_end = first_non_system;
        while strip_end < messages.len() && messages[strip_end].role == Role::Tool {
            strip_end += 1;
        }
        if strip_end > first_non_system {
            let removed = strip_end - first_non_system;
            log::warn!("[engine] Removing {} orphaned leading tool_result messages", removed);
            messages.drain(first_non_system..strip_end);
        }

        // ── Pass 2: ensure every assistant+tool_calls has matching results ─
        let mut i = 0;
        while i < messages.len() {
            let has_tc = messages[i].role == Role::Assistant
                && messages[i].tool_calls.as_ref().map(|tc| !tc.is_empty()).unwrap_or(false);

            if !has_tc {
                i += 1;
                continue;
            }

            // Collect expected tool_call IDs from this assistant message
            let expected_ids: Vec<String> = messages[i]
                .tool_calls
                .as_ref()
                .unwrap()
                .iter()
                .map(|tc| tc.id.clone())
                .collect();

            // Scan immediately following tool-result messages
            let mut found_ids = HashSet::new();
            let mut j = i + 1;
            while j < messages.len() && messages[j].role == Role::Tool {
                if let Some(ref tcid) = messages[j].tool_call_id {
                    found_ids.insert(tcid.clone());
                }
                j += 1;
            }

            // Inject synthetic results for any missing tool_call IDs
            let mut injected = 0;
            for expected_id in &expected_ids {
                if !found_ids.contains(expected_id) {
                    let synthetic = Message {
                        role: Role::Tool,
                        content: MessageContent::Text(
                            "[Tool execution was interrupted or result was lost.]".into(),
                        ),
                        tool_calls: None,
                        tool_call_id: Some(expected_id.clone()),
                        name: Some("_synthetic".into()),
                    };
                    // Insert right after the assistant message (at position i+1+injected)
                    messages.insert(i + 1 + injected, synthetic);
                    injected += 1;
                }
            }

            if injected > 0 {
                log::warn!(
                    "[engine] Injected {} synthetic tool_result(s) for orphaned tool_use IDs",
                    injected
                );
            }

            // Advance past this assistant message + all following tool results
            i += 1;
            while i < messages.len() && messages[i].role == Role::Tool {
                i += 1;
            }
        }
    }

    // ── Config storage ─────────────────────────────────────────────────

    pub fn get_config(&self, key: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock();
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
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO engine_config (key, value) VALUES (?1, ?2)",
            params![key, value],
        ).map_err(|e| format!("Config write error: {}", e))?;
        Ok(())
    }

    // ── Trade History ──────────────────────────────────────────────────

    pub fn insert_trade(
        &self,
        trade_type: &str,
        side: Option<&str>,
        product_id: Option<&str>,
        currency: Option<&str>,
        amount: &str,
        order_type: Option<&str>,
        order_id: Option<&str>,
        status: &str,
        usd_value: Option<&str>,
        to_address: Option<&str>,
        reason: &str,
        session_id: Option<&str>,
        agent_id: Option<&str>,
        raw_response: Option<&str>,
    ) -> Result<String, String> {
        let conn = self.conn.lock();
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO trade_history (id, trade_type, side, product_id, currency, amount, order_type, order_id, status, usd_value, to_address, reason, session_id, agent_id, raw_response)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![id, trade_type, side, product_id, currency, amount, order_type, order_id, status, usd_value, to_address, reason, session_id, agent_id, raw_response],
        ).map_err(|e| format!("Insert trade error: {}", e))?;
        Ok(id)
    }

    pub fn list_trades(&self, limit: u32) -> Result<Vec<serde_json::Value>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, trade_type, side, product_id, currency, amount, order_type, order_id, status, usd_value, to_address, reason, session_id, agent_id, created_at
             FROM trade_history ORDER BY created_at DESC LIMIT ?1"
        ).map_err(|e| format!("Prepare error: {}", e))?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "trade_type": row.get::<_, String>(1)?,
                "side": row.get::<_, Option<String>>(2)?,
                "product_id": row.get::<_, Option<String>>(3)?,
                "currency": row.get::<_, Option<String>>(4)?,
                "amount": row.get::<_, String>(5)?,
                "order_type": row.get::<_, Option<String>>(6)?,
                "order_id": row.get::<_, Option<String>>(7)?,
                "status": row.get::<_, String>(8)?,
                "usd_value": row.get::<_, Option<String>>(9)?,
                "to_address": row.get::<_, Option<String>>(10)?,
                "reason": row.get::<_, String>(11)?,
                "session_id": row.get::<_, Option<String>>(12)?,
                "agent_id": row.get::<_, Option<String>>(13)?,
                "created_at": row.get::<_, String>(14)?,
            }))
        }).map_err(|e| format!("Query error: {}", e))?;
        let mut trades = Vec::new();
        for row in rows {
            trades.push(row.map_err(|e| format!("Row error: {}", e))?);
        }
        Ok(trades)
    }

    /// Get daily P&L: sum of all trades today, grouped by side
    pub fn daily_trade_summary(&self) -> Result<serde_json::Value, String> {
        let conn = self.conn.lock();
        let today = Utc::now().format("%Y-%m-%d").to_string();
        // SQLite datetime('now') uses space separator: "2026-02-19 00:00:00"
        // Must match that format, NOT ISO 8601 'T' separator
        let today_start = format!("{} 00:00:00", today);

        // Coinbase trades
        let trade_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM trade_history WHERE trade_type = 'trade' AND created_at >= ?1",
            params![&today_start],
            |row| row.get(0),
        ).unwrap_or(0);

        let transfer_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM trade_history WHERE trade_type = 'transfer' AND created_at >= ?1",
            params![&today_start],
            |row| row.get(0),
        ).unwrap_or(0);

        // DEX swap count
        let dex_swap_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM trade_history WHERE trade_type = 'dex_swap' AND created_at >= ?1",
            params![&today_start],
            |row| row.get(0),
        ).unwrap_or(0);

        // Sum USD values for buys and sells today (Coinbase)
        let buy_total: f64 = conn.query_row(
            "SELECT COALESCE(SUM(CAST(usd_value AS REAL)), 0.0) FROM trade_history WHERE trade_type = 'trade' AND side = 'buy' AND created_at >= ?1",
            params![&today_start],
            |row| row.get(0),
        ).unwrap_or(0.0);

        let sell_total: f64 = conn.query_row(
            "SELECT COALESCE(SUM(CAST(usd_value AS REAL)), 0.0) FROM trade_history WHERE trade_type = 'trade' AND side = 'sell' AND created_at >= ?1",
            params![&today_start],
            |row| row.get(0),
        ).unwrap_or(0.0);

        let transfer_total: f64 = conn.query_row(
            "SELECT COALESCE(SUM(CAST(usd_value AS REAL)), 0.0) FROM trade_history WHERE trade_type = 'transfer' AND created_at >= ?1",
            params![&today_start],
            |row| row.get(0),
        ).unwrap_or(0.0);

        // DEX swap volume (sum of amounts — not USD-denominated, but tracks activity)
        let dex_volume_raw: f64 = conn.query_row(
            "SELECT COALESCE(SUM(CAST(amount AS REAL)), 0.0) FROM trade_history WHERE trade_type = 'dex_swap' AND created_at >= ?1",
            params![&today_start],
            |row| row.get(0),
        ).unwrap_or(0.0);

        // Unique tokens swapped today
        let dex_pairs: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT product_id FROM trade_history WHERE trade_type = 'dex_swap' AND product_id IS NOT NULL AND created_at >= ?1"
            ).unwrap();
            let rows = stmt.query_map(params![&today_start], |row| row.get::<_, String>(0)).unwrap();
            rows.filter_map(|r| r.ok()).collect()
        };

        // Total operations today (buys + transfers out) for daily loss tracking
        let daily_spent = buy_total + transfer_total;

        Ok(serde_json::json!({
            "date": today,
            "trade_count": trade_count,
            "transfer_count": transfer_count,
            "dex_swap_count": dex_swap_count,
            "buy_total_usd": buy_total,
            "sell_total_usd": sell_total,
            "transfer_total_usd": transfer_total,
            "dex_volume_raw": dex_volume_raw,
            "dex_pairs": dex_pairs,
            "net_pnl_usd": sell_total - buy_total,
            "daily_spent_usd": daily_spent,
        }))
    }

    // ── Positions (Stop-Loss / Take-Profit) ────────────────────────────

    /// Insert a new open position.
    pub fn insert_position(
        &self,
        mint: &str,
        symbol: &str,
        entry_price_usd: f64,
        entry_sol: f64,
        amount: f64,
        stop_loss_pct: f64,
        take_profit_pct: f64,
        agent_id: Option<&str>,
    ) -> Result<String, String> {
        let conn = self.conn.lock();
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO positions (id, mint, symbol, entry_price_usd, entry_sol, amount, current_amount, stop_loss_pct, take_profit_pct, status, last_price_usd, agent_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, ?8, 'open', ?4, ?9)",
            params![id, mint, symbol, entry_price_usd, entry_sol, amount, stop_loss_pct, take_profit_pct, agent_id],
        ).map_err(|e| format!("Insert position error: {}", e))?;
        info!("[positions] Opened position {} for {} ({}) — entry ${:.6}, SL {:.0}%, TP {:.0}x",
            id, symbol, &mint[..std::cmp::min(8, mint.len())], entry_price_usd, stop_loss_pct * 100.0, take_profit_pct);
        Ok(id)
    }

    /// List all positions, optionally filtered by status.
    pub fn list_positions(&self, status_filter: Option<&str>) -> Result<Vec<crate::engine::types::Position>, String> {
        let conn = self.conn.lock();
        let sql = if let Some(status) = status_filter {
            format!("SELECT id, mint, symbol, entry_price_usd, entry_sol, amount, current_amount, stop_loss_pct, take_profit_pct, status, last_price_usd, last_checked_at, created_at, closed_at, close_tx, agent_id
                     FROM positions WHERE status = '{}' ORDER BY created_at DESC", status)
        } else {
            "SELECT id, mint, symbol, entry_price_usd, entry_sol, amount, current_amount, stop_loss_pct, take_profit_pct, status, last_price_usd, last_checked_at, created_at, closed_at, close_tx, agent_id
             FROM positions ORDER BY created_at DESC".to_string()
        };
        let mut stmt = conn.prepare(&sql).map_err(|e| format!("Prepare error: {}", e))?;
        let rows = stmt.query_map([], |row| {
            Ok(crate::engine::types::Position {
                id: row.get(0)?,
                mint: row.get(1)?,
                symbol: row.get(2)?,
                entry_price_usd: row.get(3)?,
                entry_sol: row.get(4)?,
                amount: row.get(5)?,
                current_amount: row.get(6)?,
                stop_loss_pct: row.get(7)?,
                take_profit_pct: row.get(8)?,
                status: row.get(9)?,
                last_price_usd: row.get(10)?,
                last_checked_at: row.get(11)?,
                created_at: row.get(12)?,
                closed_at: row.get(13)?,
                close_tx: row.get(14)?,
                agent_id: row.get(15)?,
            })
        }).map_err(|e| format!("Query error: {}", e))?;
        let mut positions = Vec::new();
        for row in rows {
            positions.push(row.map_err(|e| format!("Row error: {}", e))?);
        }
        Ok(positions)
    }

    /// Update a position's last known price.
    pub fn update_position_price(&self, id: &str, price_usd: f64) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE positions SET last_price_usd = ?1, last_checked_at = datetime('now') WHERE id = ?2",
            params![price_usd, id],
        ).map_err(|e| format!("Update position price error: {}", e))?;
        Ok(())
    }

    /// Close a position (stop-loss hit, take-profit hit, or manual).
    pub fn close_position(&self, id: &str, status: &str, close_tx: Option<&str>) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE positions SET status = ?1, closed_at = datetime('now'), close_tx = ?2 WHERE id = ?3",
            params![status, close_tx, id],
        ).map_err(|e| format!("Close position error: {}", e))?;
        Ok(())
    }

    /// Reduce the current_amount of a position (partial take-profit sell).
    pub fn reduce_position(&self, id: &str, new_amount: f64) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE positions SET current_amount = ?1 WHERE id = ?2",
            params![new_amount, id],
        ).map_err(|e| format!("Reduce position error: {}", e))?;
        Ok(())
    }

    /// Update stop-loss and take-profit percentages for a position.
    pub fn update_position_targets(&self, id: &str, stop_loss_pct: f64, take_profit_pct: f64) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE positions SET stop_loss_pct = ?1, take_profit_pct = ?2 WHERE id = ?3",
            params![stop_loss_pct, take_profit_pct, id],
        ).map_err(|e| format!("Update position targets error: {}", e))?;
        Ok(())
    }

    // ── Agent Files (Soul / Persona) ───────────────────────────────────

    pub fn list_agent_files(&self, agent_id: &str) -> Result<Vec<AgentFile>, String> {
        let conn = self.conn.lock();
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
        let conn = self.conn.lock();
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
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO agent_files (agent_id, file_name, content, updated_at)
             VALUES (?1, ?2, ?3, datetime('now'))",
            params![agent_id, file_name, content],
        ).map_err(|e| format!("Write error: {}", e))?;
        Ok(())
    }

    pub fn delete_agent_file(&self, agent_id: &str, file_name: &str) -> Result<(), String> {
        let conn = self.conn.lock();
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

    /// Lean session init — load ONLY the three core soul files.
    /// Everything else (AGENTS.md, TOOLS.md, custom files) is available
    /// on-demand via `soul_read` / `soul_list`.
    pub fn compose_core_context(&self, agent_id: &str) -> Result<Option<String>, String> {
        let core_files = ["IDENTITY.md", "SOUL.md", "USER.md"];
        let mut sections = Vec::new();
        for name in &core_files {
            if let Ok(Some(f)) = self.get_agent_file(agent_id, name) {
                if !f.content.trim().is_empty() {
                    sections.push(f.content.clone());
                }
            }
        }
        if sections.is_empty() {
            return Ok(None);
        }
        Ok(Some(sections.join("\n\n---\n\n")))
    }

    /// Get memories created today — lightweight daily context injection.
    /// Returns a compact summary string (max 10 entries, highest importance first).
    pub fn get_todays_memories(&self, agent_id: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock();
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let today_start = format!("{} 00:00:00", today);
        let mut stmt = conn.prepare(
            "SELECT content, category FROM memories
             WHERE created_at >= ?1 AND (agent_id = ?2 OR agent_id = '')
             ORDER BY importance DESC, created_at DESC
             LIMIT 10"
        ).map_err(|e| format!("Prepare error: {}", e))?;

        let rows: Vec<(String, String)> = stmt.query_map(params![today_start, agent_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }).map_err(|e| format!("Query error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

        if rows.is_empty() {
            return Ok(None);
        }

        let mut lines = Vec::new();
        for (content, category) in &rows {
            // Truncate long entries to keep the block compact
            let short = if content.len() > 200 { format!("{}…", &content[..200]) } else { content.clone() };
            lines.push(format!("- [{}] {}", category, short));
        }
        Ok(Some(format!("## Today's Memory Notes ({})\n{}", today, lines.join("\n"))))
    }

    // ── Memory CRUD ────────────────────────────────────────────────────

    pub fn store_memory(&self, id: &str, content: &str, category: &str, importance: u8, embedding: Option<&[u8]>, agent_id: Option<&str>) -> Result<(), String> {
        let conn = self.conn.lock();
        let aid = agent_id.unwrap_or("");
        conn.execute(
            "INSERT OR REPLACE INTO memories (id, content, category, importance, embedding, agent_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, content, category, importance as i32, embedding, aid],
        ).map_err(|e| format!("Memory store error: {}", e))?;

        // Sync FTS5 index
        conn.execute(
            "INSERT OR REPLACE INTO memories_fts (id, content, category, agent_id) VALUES (?1, ?2, ?3, ?4)",
            params![id, content, category, aid],
        ).ok(); // Best-effort FTS sync
        Ok(())
    }

    pub fn delete_memory(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM memories WHERE id = ?1", params![id])
            .map_err(|e| format!("Memory delete error: {}", e))?;
        // Sync FTS5 index
        conn.execute("DELETE FROM memories_fts WHERE id = ?1", params![id]).ok();
        Ok(())
    }

    pub fn memory_stats(&self) -> Result<MemoryStats, String> {
        let conn = self.conn.lock();

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
    pub fn search_memories_by_embedding(&self, query_embedding: &[f32], limit: usize, threshold: f64, agent_id: Option<&str>) -> Result<Vec<Memory>, String> {
        let conn = self.conn.lock();

        let mut stmt = conn.prepare(
            "SELECT id, content, category, importance, embedding, created_at, agent_id FROM memories WHERE embedding IS NOT NULL"
        ).map_err(|e| format!("Prepare error: {}", e))?;

        let mut scored: Vec<(Memory, f64)> = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let content: String = row.get(1)?;
            let category: String = row.get(2)?;
            let importance: i32 = row.get(3)?;
            let embedding_blob: Vec<u8> = row.get(4)?;
            let created_at: String = row.get(5)?;
            let mem_agent_id: String = row.get::<_, String>(6).unwrap_or_default();
            Ok((id, content, category, importance as u8, embedding_blob, created_at, mem_agent_id))
        }).map_err(|e| format!("Query error: {}", e))?
        .filter_map(|r| r.ok())
        .filter_map(|(id, content, category, importance, blob, created_at, mem_agent_id)| {
            // Filter by agent_id if specified
            if let Some(aid) = agent_id {
                if !mem_agent_id.is_empty() && mem_agent_id != aid {
                    return None;
                }
            }
            let stored_emb = bytes_to_f32_vec(&blob);
            let score = cosine_similarity(query_embedding, &stored_emb);
            if score >= threshold {
                Some((Memory { id, content, category, importance, created_at, score: Some(score), agent_id: if mem_agent_id.is_empty() { None } else { Some(mem_agent_id) } }, score))
            } else {
                None
            }
        })
        .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        Ok(scored.into_iter().map(|(m, _)| m).collect())
    }

    /// BM25 full-text search via FTS5 — much better than LIKE keyword search.
    pub fn search_memories_bm25(&self, query: &str, limit: usize, agent_id: Option<&str>) -> Result<Vec<Memory>, String> {
        let conn = self.conn.lock();

        // FTS5 match query — escape special characters
        let fts_query = query
            .replace('"', "\"\"")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" OR ");

        let sql = if let Some(aid) = agent_id {
            // Filter: memories with matching agent_id OR no agent_id (shared)
            let mut stmt = conn.prepare(
                "SELECT f.id, f.content, f.category, f.agent_id, rank,
                        m.importance, m.created_at
                 FROM memories_fts f
                 JOIN memories m ON m.id = f.id
                 WHERE memories_fts MATCH ?1
                   AND (f.agent_id = '' OR f.agent_id = ?2)
                 ORDER BY rank
                 LIMIT ?3"
            ).map_err(|e| format!("FTS prepare error: {}", e))?;

            let memories: Vec<Memory> = stmt.query_map(params![fts_query, aid, limit as i64], |row| {
                let bm25_rank: f64 = row.get(4)?;
                Ok(Memory {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    category: row.get(2)?,
                    importance: { let i: i32 = row.get(5)?; i as u8 },
                    created_at: row.get(6)?,
                    score: Some(-bm25_rank), // FTS5 rank is negative (lower=better), negate for consistency
                    agent_id: { let a: String = row.get(3)?; if a.is_empty() { None } else { Some(a) } },
                })
            }).map_err(|e| format!("FTS query error: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
            return Ok(memories);
        } else {
            "SELECT f.id, f.content, f.category, f.agent_id, rank,
                    m.importance, m.created_at
             FROM memories_fts f
             JOIN memories m ON m.id = f.id
             WHERE memories_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2"
        };

        let mut stmt = conn.prepare(sql).map_err(|e| format!("FTS prepare error: {}", e))?;
        let memories: Vec<Memory> = stmt.query_map(params![fts_query, limit as i64], |row| {
            let bm25_rank: f64 = row.get(4)?;
            Ok(Memory {
                id: row.get(0)?,
                content: row.get(1)?,
                category: row.get(2)?,
                importance: { let i: i32 = row.get(5)?; i as u8 },
                created_at: row.get(6)?,
                score: Some(-bm25_rank),
                agent_id: { let a: String = row.get(3)?; if a.is_empty() { None } else { Some(a) } },
            })
        }).map_err(|e| format!("FTS query error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(memories)
    }

    /// Keyword-based fallback search (no embeddings needed).
    pub fn search_memories_keyword(&self, query: &str, limit: usize) -> Result<Vec<Memory>, String> {
        let conn = self.conn.lock();

        let pattern = format!("%{}%", query.to_lowercase());
        let mut stmt = conn.prepare(
            "SELECT id, content, category, importance, created_at, agent_id FROM memories
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
                agent_id: { let a: String = row.get::<_, String>(5).unwrap_or_default(); if a.is_empty() { None } else { Some(a) } },
            })
        }).map_err(|e| format!("Query error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(memories)
    }

    /// Get all memories (for export / listing), newest first.
    pub fn list_memories(&self, limit: usize) -> Result<Vec<Memory>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, content, category, importance, created_at, agent_id FROM memories
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
                agent_id: { let a: String = row.get::<_, String>(5).unwrap_or_default(); if a.is_empty() { None } else { Some(a) } },
            })
        }).map_err(|e| format!("Query error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(memories)
    }

    /// List memories that have no embedding vector (for backfill).
    pub fn list_memories_without_embeddings(&self, limit: usize) -> Result<Vec<Memory>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, content, category, importance, created_at, agent_id FROM memories
             WHERE embedding IS NULL
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
                agent_id: { let a: String = row.get::<_, String>(5).unwrap_or_default(); if a.is_empty() { None } else { Some(a) } },
            })
        }).map_err(|e| format!("Query error: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

        Ok(memories)
    }

    /// Update the embedding for an existing memory (used by backfill).
    pub fn update_memory_embedding(&self, id: &str, embedding: &[u8]) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE memories SET embedding = ?2 WHERE id = ?1",
            params![id, embedding],
        ).map_err(|e| format!("Update embedding error: {}", e))?;
        Ok(())
    }
}

// ── Task CRUD ──────────────────────────────────────────────────────────

impl SessionStore {
    /// List all tasks, ordered by updated_at DESC.
    pub fn list_tasks(&self) -> Result<Vec<Task>, String> {
        let conn = self.conn.lock();

        // Auto-migrate: add model column if not present
        let _ = conn.execute("ALTER TABLE tasks ADD COLUMN model TEXT", []);

        let mut stmt = conn.prepare(
            "SELECT id, title, description, status, priority, assigned_agent, session_id,
                    cron_schedule, cron_enabled, last_run_at, next_run_at, created_at, updated_at, model
             FROM tasks ORDER BY updated_at DESC"
        ).map_err(|e| e.to_string())?;

        let mut tasks: Vec<Task> = stmt.query_map([], |row| {
            Ok(Task {
                id: row.get(0)?,
                title: row.get(1)?,
                description: row.get(2)?,
                status: row.get(3)?,
                priority: row.get(4)?,
                assigned_agent: row.get(5)?,
                assigned_agents: Vec::new(), // populated below
                session_id: row.get(6)?,
                model: row.get(13)?,
                cron_schedule: row.get(7)?,
                cron_enabled: row.get::<_, i32>(8)? != 0,
                last_run_at: row.get(9)?,
                next_run_at: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        }).map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

        // Load agents for each task
        let mut agent_stmt = conn.prepare(
            "SELECT agent_id, role FROM task_agents WHERE task_id = ?1 ORDER BY added_at"
        ).map_err(|e| e.to_string())?;

        for task in &mut tasks {
            if let Ok(agents) = agent_stmt.query_map(params![task.id], |row| {
                Ok(TaskAgent {
                    agent_id: row.get(0)?,
                    role: row.get(1)?,
                })
            }) {
                task.assigned_agents = agents.filter_map(|r| r.ok()).collect();
            }
        }

        Ok(tasks)
    }

    /// Create a new task.
    pub fn create_task(&self, task: &Task) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO tasks (id, title, description, status, priority, assigned_agent, session_id,
                               model, cron_schedule, cron_enabled, last_run_at, next_run_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                task.id, task.title, task.description, task.status, task.priority,
                task.assigned_agent, task.session_id, task.model, task.cron_schedule,
                task.cron_enabled as i32, task.last_run_at, task.next_run_at,
            ],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Update a task (all mutable fields).
    pub fn update_task(&self, task: &Task) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE tasks SET title=?2, description=?3, status=?4, priority=?5,
                    assigned_agent=?6, session_id=?7, model=?8, cron_schedule=?9, cron_enabled=?10,
                    last_run_at=?11, next_run_at=?12, updated_at=datetime('now')
             WHERE id=?1",
            params![
                task.id, task.title, task.description, task.status, task.priority,
                task.assigned_agent, task.session_id, task.model, task.cron_schedule,
                task.cron_enabled as i32, task.last_run_at, task.next_run_at,
            ],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Delete a task and its activity.
    pub fn delete_task(&self, task_id: &str) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM task_activity WHERE task_id = ?1", params![task_id])
            .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM tasks WHERE id = ?1", params![task_id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Add an activity entry for a task.
    pub fn add_task_activity(&self, id: &str, task_id: &str, kind: &str, agent: Option<&str>, content: &str) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO task_activity (id, task_id, kind, agent, content)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, task_id, kind, agent, content],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// List activity for a task (most recent first).
    pub fn list_task_activity(&self, task_id: &str, limit: u32) -> Result<Vec<TaskActivity>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, task_id, kind, agent, content, created_at
             FROM task_activity WHERE task_id = ?1
             ORDER BY created_at DESC LIMIT ?2"
        ).map_err(|e| e.to_string())?;

        let entries = stmt.query_map(params![task_id, limit], |row| {
            Ok(TaskActivity {
                id: row.get(0)?,
                task_id: row.get(1)?,
                kind: row.get(2)?,
                agent: row.get(3)?,
                content: row.get(4)?,
                created_at: row.get(5)?,
            })
        }).map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

        Ok(entries)
    }

    /// Get all activity across all tasks for the live feed, most recent first.
    pub fn list_all_activity(&self, limit: u32) -> Result<Vec<TaskActivity>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, task_id, kind, agent, content, created_at
             FROM task_activity ORDER BY created_at DESC LIMIT ?1"
        ).map_err(|e| e.to_string())?;

        let entries = stmt.query_map(params![limit], |row| {
            Ok(TaskActivity {
                id: row.get(0)?,
                task_id: row.get(1)?,
                kind: row.get(2)?,
                agent: row.get(3)?,
                content: row.get(4)?,
                created_at: row.get(5)?,
            })
        }).map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

        Ok(entries)
    }

    /// Get due cron tasks (cron_enabled=1 and next_run_at <= now).
    pub fn get_due_cron_tasks(&self) -> Result<Vec<Task>, String> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().to_rfc3339();
        let mut stmt = conn.prepare(
            "SELECT id, title, description, status, priority, assigned_agent, session_id,
                    cron_schedule, cron_enabled, last_run_at, next_run_at, created_at, updated_at, model
             FROM tasks WHERE cron_enabled = 1 AND next_run_at IS NOT NULL AND next_run_at <= ?1"
        ).map_err(|e| e.to_string())?;

        let mut tasks: Vec<Task> = stmt.query_map(params![now], |row| {
            Ok(Task {
                id: row.get(0)?,
                title: row.get(1)?,
                description: row.get(2)?,
                status: row.get(3)?,
                priority: row.get(4)?,
                assigned_agent: row.get(5)?,
                assigned_agents: Vec::new(),
                session_id: row.get(6)?,
                model: row.get(13)?,
                cron_schedule: row.get(7)?,
                cron_enabled: row.get::<_, i32>(8)? != 0,
                last_run_at: row.get(9)?,
                next_run_at: row.get(10)?,
                created_at: row.get(11)?,
                updated_at: row.get(12)?,
            })
        }).map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();

        // Load agents for each cron task
        let mut agent_stmt = conn.prepare(
            "SELECT agent_id, role FROM task_agents WHERE task_id = ?1 ORDER BY added_at"
        ).map_err(|e| e.to_string())?;
        for task in &mut tasks {
            if let Ok(agents) = agent_stmt.query_map(params![task.id], |row| {
                Ok(TaskAgent {
                    agent_id: row.get(0)?,
                    role: row.get(1)?,
                })
            }) {
                task.assigned_agents = agents.filter_map(|r| r.ok()).collect();
            }
        }

        Ok(tasks)
    }

    /// Update a task's cron run timestamps.
    pub fn update_task_cron_run(&self, task_id: &str, last_run: &str, next_run: Option<&str>) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE tasks SET last_run_at = ?2, next_run_at = ?3, updated_at = datetime('now') WHERE id = ?1",
            params![task_id, last_run, next_run],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    // ── Task Agents (multi-agent assignments) ──────────────────────────

    /// Set the agents for a task (replaces all existing assignments).
    pub fn set_task_agents(&self, task_id: &str, agents: &[TaskAgent]) -> Result<(), String> {
        let conn = self.conn.lock();
        // Clear existing
        conn.execute("DELETE FROM task_agents WHERE task_id = ?1", params![task_id])
            .map_err(|e| e.to_string())?;
        // Insert new
        for ta in agents {
            conn.execute(
                "INSERT INTO task_agents (task_id, agent_id, role) VALUES (?1, ?2, ?3)",
                params![task_id, ta.agent_id, ta.role],
            ).map_err(|e| e.to_string())?;
        }
        // Also update legacy assigned_agent to the first lead (or first agent)
        let primary = agents.iter().find(|a| a.role == "lead").or_else(|| agents.first());
        let primary_id = primary.map(|a| a.agent_id.as_str());
        conn.execute(
            "UPDATE tasks SET assigned_agent = ?2, updated_at = datetime('now') WHERE id = ?1",
            params![task_id, primary_id],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Get agents assigned to a task.
    pub fn get_task_agents(&self, task_id: &str) -> Result<Vec<TaskAgent>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT agent_id, role FROM task_agents WHERE task_id = ?1 ORDER BY added_at"
        ).map_err(|e| e.to_string())?;

        let agents = stmt.query_map(params![task_id], |row| {
            Ok(TaskAgent {
                agent_id: row.get(0)?,
                role: row.get(1)?,
            })
        }).map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
        Ok(agents)
    }

    // ── Orchestrator: Projects ─────────────────────────────────────────

    pub fn list_projects(&self) -> Result<Vec<crate::engine::types::Project>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, title, goal, status, boss_agent, created_at, updated_at FROM projects ORDER BY updated_at DESC"
        ).map_err(|e| e.to_string())?;

        let projects = stmt.query_map([], |row| {
            Ok(crate::engine::types::Project {
                id: row.get(0)?,
                title: row.get(1)?,
                goal: row.get(2)?,
                status: row.get(3)?,
                boss_agent: row.get(4)?,
                agents: vec![],
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        }).map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect::<Vec<_>>();

        // Load agents for each project (inline to avoid double-locking self.conn)
        let mut result = Vec::new();
        for mut p in projects {
            let mut agent_stmt = conn.prepare(
                "SELECT agent_id, role, specialty, status, current_task, model, system_prompt, capabilities FROM project_agents WHERE project_id=?1"
            ).map_err(|e| e.to_string())?;
            p.agents = agent_stmt.query_map(params![p.id], |row| {
                let caps_str: String = row.get::<_, String>(7).unwrap_or_default();
                let capabilities: Vec<String> = serde_json::from_str(&caps_str).unwrap_or_default();
                Ok(crate::engine::types::ProjectAgent {
                    agent_id: row.get(0)?,
                    role: row.get(1)?,
                    specialty: row.get(2)?,
                    status: row.get(3)?,
                    current_task: row.get(4)?,
                    model: row.get(5)?,
                    system_prompt: row.get(6)?,
                    capabilities,
                })
            }).map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();
            result.push(p);
        }
        Ok(result)
    }

    pub fn create_project(&self, project: &crate::engine::types::Project) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO projects (id, title, goal, status, boss_agent) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![project.id, project.title, project.goal, project.status, project.boss_agent],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_project(&self, project: &crate::engine::types::Project) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE projects SET title=?2, goal=?3, status=?4, boss_agent=?5, updated_at=datetime('now') WHERE id=?1",
            params![project.id, project.title, project.goal, project.status, project.boss_agent],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_project(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM projects WHERE id=?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn set_project_agents(&self, project_id: &str, agents: &[crate::engine::types::ProjectAgent]) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM project_agents WHERE project_id=?1", params![project_id])
            .map_err(|e| e.to_string())?;
        for a in agents {
            let caps_json = serde_json::to_string(&a.capabilities).unwrap_or_default();
            conn.execute(
                "INSERT INTO project_agents (project_id, agent_id, role, specialty, status, current_task, model, system_prompt, capabilities) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![project_id, a.agent_id, a.role, a.specialty, a.status, a.current_task, a.model, a.system_prompt, caps_json],
            ).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn add_project_agent(&self, project_id: &str, agent: &crate::engine::types::ProjectAgent) -> Result<(), String> {
        let conn = self.conn.lock();
        let caps_json = serde_json::to_string(&agent.capabilities).unwrap_or_default();
        conn.execute(
            "INSERT OR REPLACE INTO project_agents (project_id, agent_id, role, specialty, status, current_task, model, system_prompt, capabilities) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![project_id, agent.agent_id, agent.role, agent.specialty, agent.status, agent.current_task, agent.model, agent.system_prompt, caps_json],
        ).map_err(|e| format!("Failed to insert agent: {}", e))?;
        Ok(())
    }

    pub fn get_project_agents(&self, project_id: &str) -> Result<Vec<crate::engine::types::ProjectAgent>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT agent_id, role, specialty, status, current_task, model, system_prompt, capabilities FROM project_agents WHERE project_id=?1"
        ).map_err(|e| e.to_string())?;
        let agents = stmt.query_map(params![project_id], |row| {
            let caps_str: String = row.get::<_, String>(7).unwrap_or_default();
            let capabilities: Vec<String> = serde_json::from_str(&caps_str).unwrap_or_default();
            Ok(crate::engine::types::ProjectAgent {
                agent_id: row.get(0)?,
                role: row.get(1)?,
                specialty: row.get(2)?,
                status: row.get(3)?,
                current_task: row.get(4)?,
                model: row.get(5)?,
                system_prompt: row.get(6)?,
                capabilities,
            })
        }).map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
        Ok(agents)
    }

    pub fn update_project_agent_status(&self, project_id: &str, agent_id: &str, status: &str, current_task: Option<&str>) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE project_agents SET status=?3, current_task=?4 WHERE project_id=?1 AND agent_id=?2",
            params![project_id, agent_id, status, current_task],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Delete an agent from a specific project.
    pub fn delete_agent(&self, project_id: &str, agent_id: &str) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM project_agents WHERE project_id=?1 AND agent_id=?2",
            params![project_id, agent_id],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// List all unique agents across all projects (deduped by agent_id).
    /// Filters out rows with empty/NULL agent_id (bad data from manual SQL inserts).
    pub fn list_all_agents(&self) -> Result<Vec<(String, crate::engine::types::ProjectAgent)>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT project_id, agent_id, role, specialty, status, current_task, model, system_prompt, capabilities FROM project_agents WHERE agent_id IS NOT NULL AND agent_id != '' ORDER BY agent_id"
        ).map_err(|e| e.to_string())?;
        let agents = stmt.query_map([], |row| {
            let caps_str: String = row.get::<_, String>(8).unwrap_or_default();
            let capabilities: Vec<String> = serde_json::from_str(&caps_str).unwrap_or_default();
            Ok((row.get::<_, String>(0)?, crate::engine::types::ProjectAgent {
                agent_id: row.get(1)?,
                role: row.get(2)?,
                specialty: row.get(3)?,
                status: row.get(4)?,
                current_task: row.get(5)?,
                model: row.get(6)?,
                system_prompt: row.get(7)?,
                capabilities,
            }))
        }).map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
        Ok(agents)
    }

    // ── Orchestrator: Message Bus ──────────────────────────────────────

    pub fn add_project_message(&self, msg: &crate::engine::types::ProjectMessage) -> Result<(), String> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO project_messages (id, project_id, from_agent, to_agent, kind, content, metadata) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![msg.id, msg.project_id, msg.from_agent, msg.to_agent, msg.kind, msg.content, msg.metadata],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_project_messages(&self, project_id: &str, limit: i64) -> Result<Vec<crate::engine::types::ProjectMessage>, String> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, project_id, from_agent, to_agent, kind, content, metadata, created_at FROM project_messages WHERE project_id=?1 ORDER BY created_at DESC LIMIT ?2"
        ).map_err(|e| e.to_string())?;
        let msgs = stmt.query_map(params![project_id, limit], |row| {
            Ok(crate::engine::types::ProjectMessage {
                id: row.get(0)?,
                project_id: row.get(1)?,
                from_agent: row.get(2)?,
                to_agent: row.get(3)?,
                kind: row.get(4)?,
                content: row.get(5)?,
                metadata: row.get(6)?,
                created_at: row.get(7)?,
            })
        }).map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect::<Vec<_>>();

        // Return in chronological order
        let mut result = msgs;
        result.reverse();
        Ok(result)
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
