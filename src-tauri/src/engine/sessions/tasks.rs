use rusqlite::params;
use crate::engine::types::{Task, TaskActivity, TaskAgent};
use super::SessionStore;

impl Task {
    /// Map a row with columns (id, title, description, status, priority, assigned_agent,
    /// session_id, cron_schedule, cron_enabled, last_run_at, next_run_at, created_at,
    /// updated_at, model) → Task (assigned_agents populated separately).
    fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
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
    }
}

/// Populate `task.assigned_agents` for every task in `tasks`.
/// Uses a single prepared statement to avoid N round-trips.
fn load_task_agents(conn: &rusqlite::Connection, tasks: &mut Vec<Task>) -> Result<(), String> {
    let mut agent_stmt = conn.prepare(
        "SELECT agent_id, role FROM task_agents WHERE task_id = ?1 ORDER BY added_at"
    ).map_err(|e| e.to_string())?;
    for task in tasks.iter_mut() {
        if let Ok(agents) = agent_stmt.query_map(params![task.id], |row| {
            Ok(TaskAgent { agent_id: row.get(0)?, role: row.get(1)? })
        }) {
            task.assigned_agents = agents.filter_map(|r| r.ok()).collect();
        }
    }
    Ok(())
}

impl SessionStore {
    // ── Task CRUD ──────────────────────────────────────────────────────

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

        let mut tasks: Vec<Task> = stmt.query_map([], |row| Task::from_row(row))
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        load_task_agents(&conn, &mut tasks)?;

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

        let mut tasks: Vec<Task> = stmt.query_map(params![now], |row| Task::from_row(row))
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        load_task_agents(&conn, &mut tasks)?;

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
}
