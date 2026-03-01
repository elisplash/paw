// Database schema and migrations for the Paw engine store.
// Called once at startup by SessionStore::open() after WAL is enabled.
// Adding a new table or column: append an idempotent CREATE TABLE IF NOT EXISTS
// or ALTER TABLE … ADD COLUMN (errors are silently swallowed) at the end of
// run_migrations() — never modify existing SQL to keep upgrade paths clean.

use crate::atoms::error::EngineResult;
use log::{info, warn};
use rusqlite::Connection;

pub(crate) fn run_migrations(conn: &Connection) -> EngineResult<()> {
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
            warn!(
                "[engine] Detected legacy project_agents schema — migrating to composite-PK layout"
            );
            conn.execute_batch("DROP TABLE IF EXISTS project_agents;")
                .ok();
        }
    }

    // ── Core tables ──────────────────────────────────────────────────
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
    ")?;

    // ── Migrations: add columns to existing tables ──────────────────
    // SQLite silently ignores ALTER TABLE ADD COLUMN errors if the column exists.
    conn.execute("ALTER TABLE project_agents ADD COLUMN model TEXT", [])
        .ok();
    conn.execute(
        "ALTER TABLE project_agents ADD COLUMN system_prompt TEXT",
        [],
    )
    .ok();
    conn.execute(
        "ALTER TABLE project_agents ADD COLUMN capabilities TEXT NOT NULL DEFAULT ''",
        [],
    )
    .ok();

    // Add agent_id column to sessions (for per-agent session isolation)
    conn.execute("ALTER TABLE sessions ADD COLUMN agent_id TEXT", [])
        .ok();
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_sessions_agent ON sessions(agent_id)",
        [],
    )
    .ok();

    // ── Positions table: stop-loss / take-profit tracking ────────────
    conn.execute_batch(
        "
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
    ",
    )
    .ok();

    // ── Phase F.2: Skill Outputs (Dashboard Widgets) ────────────────
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS skill_outputs (
            id TEXT PRIMARY KEY,
            skill_id TEXT NOT NULL,
            agent_id TEXT NOT NULL DEFAULT 'default',
            widget_type TEXT NOT NULL,
            title TEXT NOT NULL,
            data TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_skill_outputs_skill ON skill_outputs(skill_id);
    ",
    )
    .ok();

    // ── Phase F.6: Skill Storage (Extension KV Store) ───────────────
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS skill_storage (
            skill_id TEXT NOT NULL,
            key TEXT NOT NULL,
            value TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (skill_id, key)
        );
        CREATE INDEX IF NOT EXISTS idx_skill_storage_skill ON skill_storage(skill_id);
    ",
    )
    .ok();

    // ── Phase 2: Memory Intelligence migrations ──────────────────────
    // Add agent_id column to memories (for per-agent memory scope)
    conn.execute(
        "ALTER TABLE memories ADD COLUMN agent_id TEXT NOT NULL DEFAULT ''",
        [],
    )
    .ok();

    // Create FTS5 virtual table for BM25 full-text search
    conn.execute_batch(
        "
        CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
            id UNINDEXED,
            content,
            category UNINDEXED,
            agent_id UNINDEXED,
            content_rowid=rowid
        );
    ",
    )
    .ok();

    // Populate FTS index with existing memories that aren't indexed yet
    conn.execute_batch(
        "
        INSERT OR IGNORE INTO memories_fts(id, content, category, agent_id)
        SELECT id, content, category, COALESCE(agent_id, '')
        FROM memories
        WHERE id NOT IN (SELECT id FROM memories_fts);
    ",
    )
    .ok();

    // ── Event-Driven Triggers & Persistent Tasks ──────────────────
    conn.execute("ALTER TABLE tasks ADD COLUMN event_trigger TEXT", [])
        .ok();
    conn.execute(
        "ALTER TABLE tasks ADD COLUMN persistent INTEGER NOT NULL DEFAULT 0",
        [],
    )
    .ok();

    // ── Inter-Agent Communication ───────────────────────────────────
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS agent_messages (
            id TEXT PRIMARY KEY,
            from_agent TEXT NOT NULL,
            to_agent TEXT NOT NULL,
            channel TEXT NOT NULL DEFAULT 'general',
            content TEXT NOT NULL DEFAULT '',
            metadata TEXT,
            read INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_agent_messages_to ON agent_messages(to_agent, read, created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_agent_messages_channel ON agent_messages(channel, created_at DESC);
    ").ok();

    // ── Agent Squads ────────────────────────────────────────────────
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS squads (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            goal TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'active',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS squad_members (
            squad_id TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT 'member',
            added_at TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (squad_id, agent_id),
            FOREIGN KEY (squad_id) REFERENCES squads(id) ON DELETE CASCADE
        );
    ",
    )
    .ok();

    // ── Flows (Visual Pipelines) ────────────────────────────────────
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS flows (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            folder TEXT,
            graph_json TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_flows_name ON flows(name);
        CREATE INDEX IF NOT EXISTS idx_flows_folder ON flows(folder);

        CREATE TABLE IF NOT EXISTS flow_runs (
            id TEXT PRIMARY KEY,
            flow_id TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'running',
            duration_ms INTEGER,
            events_json TEXT,
            error TEXT,
            started_at TEXT NOT NULL DEFAULT (datetime('now')),
            finished_at TEXT,
            FOREIGN KEY (flow_id) REFERENCES flows(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_flow_runs_flow ON flow_runs(flow_id, started_at DESC);
        CREATE INDEX IF NOT EXISTS idx_flow_runs_status ON flow_runs(status);
    ",
    )
    .ok();

    // ── Seed _standalone sentinel project ───────────────────────────
    // Ensures user-created agents (via create_agent tool) satisfy the FK constraint.
    conn.execute(
        "INSERT OR IGNORE INTO projects (id, title, goal, status, boss_agent)
         VALUES ('_standalone', 'Standalone Agents', 'Container for user-created agents', 'active', 'system')",
        [],
    )?;

    // ── One-time dedup migration ─────────────────────────────────────
    // Removes duplicate messages caused by a historical bug that re-inserted
    // messages on every agent turn.  Guarded by a config flag so it only runs once.
    let already_deduped: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM engine_config WHERE key = 'migration_dedup_done'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;

    if !already_deduped {
        let deduped = conn
            .execute(
                "DELETE FROM messages WHERE id NOT IN (
                SELECT MIN(id) FROM messages
                GROUP BY session_id, role, content, tool_call_id
            )",
                [],
            )
            .unwrap_or(0);
        if deduped > 0 {
            info!(
                "[engine] Deduplication: removed {} duplicate messages",
                deduped
            );
            conn.execute_batch(
                "UPDATE sessions SET message_count = (
                    SELECT COUNT(*) FROM messages WHERE messages.session_id = sessions.id
                )",
            )
            .ok();
        }
        conn.execute(
            "INSERT OR REPLACE INTO engine_config (key, value) VALUES ('migration_dedup_done', '1')",
            [],
        ).ok();
    }

    // ── Engram: Three-tier memory system tables ─────────────────────
    crate::engine::engram::schema::run_engram_migrations(conn)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn in_memory_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA journal_mode = WAL;").unwrap();
        conn
    }

    #[test]
    fn migrations_run_cleanly() {
        let conn = in_memory_db();
        let result = run_migrations(&conn);
        assert!(result.is_ok(), "First migration run failed: {:?}", result);
    }

    #[test]
    fn migrations_idempotent() {
        let conn = in_memory_db();
        run_migrations(&conn).unwrap();
        let result = run_migrations(&conn);
        assert!(result.is_ok(), "Second migration run failed: {:?}", result);
    }

    #[test]
    fn core_tables_created() {
        let conn = in_memory_db();
        run_migrations(&conn).unwrap();

        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert!(tables.contains(&"sessions".to_string()));
        assert!(tables.contains(&"messages".to_string()));
        assert!(tables.contains(&"memories".to_string()));
        assert!(tables.contains(&"tasks".to_string()));
        assert!(tables.contains(&"engine_config".to_string()));
    }
}
