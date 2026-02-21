use rusqlite::params;
use crate::engine::types::AgentFile;
use super::SessionStore;
use crate::atoms::error::EngineResult;

impl SessionStore {
    // ── Agent Files (Soul / Persona) ───────────────────────────────────

    pub fn list_agent_files(&self, agent_id: &str) -> EngineResult<Vec<AgentFile>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT agent_id, file_name, content, updated_at FROM agent_files WHERE agent_id = ?1 ORDER BY file_name"
        )?;

        let files = stmt.query_map(params![agent_id], |row| {
            Ok(AgentFile {
                agent_id: row.get(0)?,
                file_name: row.get(1)?,
                content: row.get(2)?,
                updated_at: row.get(3)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

        Ok(files)
    }

    pub fn get_agent_file(&self, agent_id: &str, file_name: &str) -> EngineResult<Option<AgentFile>> {
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
            Err(e) => Err(e.into()),
        }
    }

    pub fn set_agent_file(&self, agent_id: &str, file_name: &str, content: &str) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO agent_files (agent_id, file_name, content, updated_at)
             VALUES (?1, ?2, ?3, datetime('now'))",
            params![agent_id, file_name, content],
        )?;
        Ok(())
    }

    pub fn delete_agent_file(&self, agent_id: &str, file_name: &str) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM agent_files WHERE agent_id = ?1 AND file_name = ?2",
            params![agent_id, file_name],
        )?;
        Ok(())
    }

    /// Load all agent files for a given agent and compose them into a single system prompt block.
    /// Returns None if no agent files exist.
    pub fn compose_agent_context(&self, agent_id: &str) -> EngineResult<Option<String>> {
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
    pub fn compose_core_context(&self, agent_id: &str) -> EngineResult<Option<String>> {
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
}
