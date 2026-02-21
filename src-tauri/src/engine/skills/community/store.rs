use crate::engine::sessions::SessionStore;
use super::types::CommunitySkill;
use crate::atoms::error::EngineResult;

// ── DB Storage for Community Skills ────────────────────────────────────

impl SessionStore {
    /// Initialize the community skills table.
    pub fn init_community_skills_table(&self) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS community_skills (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                instructions TEXT NOT NULL,
                source TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                agent_ids TEXT NOT NULL DEFAULT '[]',
                installed_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
        ")?;

        // Migration: add agent_ids column if table existed without it
        let _ = conn.execute_batch(
            "ALTER TABLE community_skills ADD COLUMN agent_ids TEXT NOT NULL DEFAULT '[]'"
        );

        Ok(())
    }

    /// Save (upsert) a community skill.
    pub fn save_community_skill(&self, skill: &CommunitySkill) -> EngineResult<()> {
        let conn = self.conn.lock();
        let agent_ids_json = serde_json::to_string(&skill.agent_ids).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "INSERT INTO community_skills (id, name, description, instructions, source, enabled, agent_ids, installed_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET name=?2, description=?3, instructions=?4, source=?5, agent_ids=?7, updated_at=?9",
            rusqlite::params![
                skill.id, skill.name, skill.description, skill.instructions,
                skill.source, skill.enabled as i32, agent_ids_json, skill.installed_at, skill.updated_at
            ],
        )?;
        Ok(())
    }

    /// List all installed community skills.
    pub fn list_community_skills(&self) -> EngineResult<Vec<CommunitySkill>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, instructions, source, enabled, agent_ids, installed_at, updated_at
             FROM community_skills ORDER BY name"
        )?;

        let skills = stmt.query_map([], |row| {
            let agent_ids_json: String = row.get::<_, String>(6).unwrap_or_else(|_| "[]".to_string());
            let agent_ids: Vec<String> = serde_json::from_str(&agent_ids_json).unwrap_or_default();
            Ok(CommunitySkill {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                instructions: row.get(3)?,
                source: row.get(4)?,
                enabled: row.get::<_, i32>(5)? != 0,
                agent_ids,
                installed_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

        Ok(skills)
    }

    /// Set enabled state for a community skill.
    pub fn set_community_skill_enabled(&self, id: &str, enabled: bool) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE community_skills SET enabled = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![enabled as i32, id],
        )?;
        Ok(())
    }

    /// Remove a community skill.
    pub fn remove_community_skill(&self, id: &str) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM community_skills WHERE id = ?1",
            rusqlite::params![id],
        )?;
        Ok(())
    }

    /// Update the agent_ids for a community skill.
    /// Empty vec = all agents. Non-empty = only those specific agents.
    pub fn set_community_skill_agents(&self, id: &str, agent_ids: &[String]) -> EngineResult<()> {
        let conn = self.conn.lock();
        let agent_ids_json = serde_json::to_string(agent_ids).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "UPDATE community_skills SET agent_ids = ?1, updated_at = datetime('now') WHERE id = ?2",
            rusqlite::params![agent_ids_json, id],
        )?;
        Ok(())
    }
}

/// Get instructions from enabled community skills for a specific agent.
/// Skills with empty agent_ids apply to ALL agents.
/// Skills with specific agent_ids only apply to those agents.
pub fn get_community_skill_instructions(store: &SessionStore, agent_id: &str) -> EngineResult<String> {
    let skills = store.list_community_skills()?;
    let mut sections: Vec<String> = Vec::new();

    for skill in &skills {
        if !skill.enabled || skill.instructions.is_empty() {
            continue;
        }
        // Filter: empty agent_ids = all agents, otherwise must contain this agent
        if !skill.agent_ids.is_empty() && !skill.agent_ids.contains(&agent_id.to_string()) {
            continue;
        }
        sections.push(format!(
            "## {} (community)\n{}",
            skill.name, skill.instructions
        ));
    }

    if sections.is_empty() {
        return Ok(String::new());
    }

    Ok(format!(
        "\n\n# Community Skills\nYou have the following community skills installed. Follow their instructions when relevant.\n\n{}\n",
        sections.join("\n\n")
    ))
}
