use rusqlite::params;
use crate::engine::types::{Project, ProjectAgent, ProjectMessage};
use super::SessionStore;
use crate::atoms::error::EngineResult;

impl ProjectAgent {
    /// Map columns starting at `offset` → ProjectAgent.
    ///  offset=0 for agent-only queries (agent_id, role, specialty, status, current_task,
    ///                                    model, system_prompt, capabilities_json)
    ///  offset=1 for project+agent queries (project_id, agent_id, role, specialty, …)
    fn from_row_at(row: &rusqlite::Row<'_>, offset: usize) -> rusqlite::Result<Self> {
        let caps_str: String = row.get::<_, String>(offset + 7).unwrap_or_default();
        let capabilities: Vec<String> = serde_json::from_str(&caps_str).unwrap_or_default();
        Ok(ProjectAgent {
            agent_id: row.get(offset)?,
            role: row.get(offset + 1)?,
            specialty: row.get(offset + 2)?,
            status: row.get(offset + 3)?,
            current_task: row.get(offset + 4)?,
            model: row.get(offset + 5)?,
            system_prompt: row.get(offset + 6)?,
            capabilities,
        })
    }
}

impl SessionStore {
    // ── Orchestrator: Projects ─────────────────────────────────────────

    pub fn list_projects(&self) -> EngineResult<Vec<Project>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, title, goal, status, boss_agent, created_at, updated_at FROM projects ORDER BY updated_at DESC"
        )?;

        let projects = stmt.query_map([], |row| {
            Ok(Project {
                id: row.get(0)?,
                title: row.get(1)?,
                goal: row.get(2)?,
                status: row.get(3)?,
                boss_agent: row.get(4)?,
                agents: vec![],
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect::<Vec<_>>();

        // Load agents for each project (inline to avoid double-locking self.conn)
        let mut result = Vec::new();
        for mut p in projects {
            let mut agent_stmt = conn.prepare(
                "SELECT agent_id, role, specialty, status, current_task, model, system_prompt, capabilities FROM project_agents WHERE project_id=?1"
            )?;
            p.agents = agent_stmt.query_map(params![p.id], |row| ProjectAgent::from_row_at(row, 0))?
                .filter_map(|r| r.ok())
                .collect();
            result.push(p);
        }
        Ok(result)
    }

    pub fn create_project(&self, project: &Project) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO projects (id, title, goal, status, boss_agent) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![project.id, project.title, project.goal, project.status, project.boss_agent],
        )?;
        Ok(())
    }

    pub fn update_project(&self, project: &Project) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE projects SET title=?2, goal=?3, status=?4, boss_agent=?5, updated_at=datetime('now') WHERE id=?1",
            params![project.id, project.title, project.goal, project.status, project.boss_agent],
        )?;
        Ok(())
    }

    pub fn delete_project(&self, id: &str) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM projects WHERE id=?1", params![id])?;
        Ok(())
    }

    pub fn set_project_agents(&self, project_id: &str, agents: &[ProjectAgent]) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM project_agents WHERE project_id=?1", params![project_id])?;
        for a in agents {
            let caps_json = serde_json::to_string(&a.capabilities).unwrap_or_default();
            conn.execute(
                "INSERT INTO project_agents (project_id, agent_id, role, specialty, status, current_task, model, system_prompt, capabilities) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![project_id, a.agent_id, a.role, a.specialty, a.status, a.current_task, a.model, a.system_prompt, caps_json],
            )?;
        }
        Ok(())
    }

    pub fn add_project_agent(&self, project_id: &str, agent: &ProjectAgent) -> EngineResult<()> {
        let conn = self.conn.lock();
        let caps_json = serde_json::to_string(&agent.capabilities).unwrap_or_default();
        conn.execute(
            "INSERT OR REPLACE INTO project_agents (project_id, agent_id, role, specialty, status, current_task, model, system_prompt, capabilities) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![project_id, agent.agent_id, agent.role, agent.specialty, agent.status, agent.current_task, agent.model, agent.system_prompt, caps_json],
        )?;
        Ok(())
    }

    pub fn get_project_agents(&self, project_id: &str) -> EngineResult<Vec<ProjectAgent>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT agent_id, role, specialty, status, current_task, model, system_prompt, capabilities FROM project_agents WHERE project_id=?1"
        )?;
        let agents = stmt.query_map(params![project_id], |row| ProjectAgent::from_row_at(row, 0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(agents)
    }

    pub fn update_project_agent_status(&self, project_id: &str, agent_id: &str, status: &str, current_task: Option<&str>) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE project_agents SET status=?3, current_task=?4 WHERE project_id=?1 AND agent_id=?2",
            params![project_id, agent_id, status, current_task],
        )?;
        Ok(())
    }

    /// Look up a single agent's stored model override (from any project).
    /// Returns `None` if the agent doesn't exist or has no model set.
    pub fn get_agent_model(&self, agent_id: &str) -> Option<String> {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT model FROM project_agents WHERE agent_id = ?1 AND model IS NOT NULL AND model != '' LIMIT 1",
            params![agent_id],
            |row| row.get::<_, String>(0),
        ).ok()
    }

    /// Delete an agent from a specific project.
    pub fn delete_agent(&self, project_id: &str, agent_id: &str) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM project_agents WHERE project_id=?1 AND agent_id=?2",
            params![project_id, agent_id],
        )?;
        Ok(())
    }

    /// List all unique agents across all projects (deduped by agent_id).
    /// Filters out rows with empty/NULL agent_id (bad data from manual SQL inserts).
    pub fn list_all_agents(&self) -> EngineResult<Vec<(String, ProjectAgent)>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT project_id, agent_id, role, specialty, status, current_task, model, system_prompt, capabilities FROM project_agents WHERE agent_id IS NOT NULL AND agent_id != '' ORDER BY agent_id"
        )?;
        let agents = stmt.query_map([], |row| {
            let project_id: String = row.get(0)?;
            let agent = ProjectAgent::from_row_at(row, 1)?;
            Ok((project_id, agent))
        })?
        .filter_map(|r| r.ok())
        .collect();
        Ok(agents)
    }

    // ── Orchestrator: Message Bus ──────────────────────────────────────

    pub fn add_project_message(&self, msg: &ProjectMessage) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO project_messages (id, project_id, from_agent, to_agent, kind, content, metadata) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![msg.id, msg.project_id, msg.from_agent, msg.to_agent, msg.kind, msg.content, msg.metadata],
        )?;
        Ok(())
    }

    pub fn get_project_messages(&self, project_id: &str, limit: i64) -> EngineResult<Vec<ProjectMessage>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, project_id, from_agent, to_agent, kind, content, metadata, created_at FROM project_messages WHERE project_id=?1 ORDER BY created_at DESC LIMIT ?2"
        )?;
        let msgs = stmt.query_map(params![project_id, limit], |row| {
            Ok(ProjectMessage {
                id: row.get(0)?,
                project_id: row.get(1)?,
                from_agent: row.get(2)?,
                to_agent: row.get(3)?,
                kind: row.get(4)?,
                content: row.get(5)?,
                metadata: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect::<Vec<_>>();

        // Return in chronological order
        let mut result = msgs;
        result.reverse();
        Ok(result)
    }
}
