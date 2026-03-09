// Agent Squads — CRUD operations on the `squads` and `squad_members` tables.
// Squads group agents into named teams that can be assigned goals collectively.

use super::SessionStore;
use crate::atoms::error::EngineResult;
use crate::engine::types::{Squad, SquadMember};
use rusqlite::params;

impl SessionStore {
    /// List all squads with their members.
    pub fn list_squads(&self) -> EngineResult<Vec<Squad>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, goal, status, created_at, updated_at
             FROM squads ORDER BY updated_at DESC",
        )?;
        let squads = stmt
            .query_map([], |row| {
                Ok(Squad {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    goal: row.get(2)?,
                    status: row.get(3)?,
                    members: vec![],
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect::<Vec<_>>();

        let mut result = Vec::new();
        for mut s in squads {
            let mut m_stmt =
                conn.prepare("SELECT agent_id, role FROM squad_members WHERE squad_id = ?1")?;
            s.members = m_stmt
                .query_map(params![s.id], |row| {
                    Ok(SquadMember {
                        agent_id: row.get(0)?,
                        role: row.get(1)?,
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();
            result.push(s);
        }
        Ok(result)
    }

    /// Create a new squad.
    pub fn create_squad(&self, squad: &Squad) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO squads (id, name, goal, status) VALUES (?1, ?2, ?3, ?4)",
            params![squad.id, squad.name, squad.goal, squad.status],
        )?;
        for m in &squad.members {
            conn.execute(
                "INSERT OR REPLACE INTO squad_members (squad_id, agent_id, role) VALUES (?1, ?2, ?3)",
                params![squad.id, m.agent_id, m.role],
            )?;
        }
        Ok(())
    }

    /// Add a member to a squad.
    pub fn add_squad_member(&self, squad_id: &str, member: &SquadMember) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT OR REPLACE INTO squad_members (squad_id, agent_id, role) VALUES (?1, ?2, ?3)",
            params![squad_id, member.agent_id, member.role],
        )?;
        conn.execute(
            "UPDATE squads SET updated_at = datetime('now') WHERE id = ?1",
            params![squad_id],
        )?;
        Ok(())
    }

    /// Remove a member from a squad.
    pub fn remove_squad_member(&self, squad_id: &str, agent_id: &str) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM squad_members WHERE squad_id = ?1 AND agent_id = ?2",
            params![squad_id, agent_id],
        )?;
        conn.execute(
            "UPDATE squads SET updated_at = datetime('now') WHERE id = ?1",
            params![squad_id],
        )?;
        Ok(())
    }

    /// Update squad goal or status.
    pub fn update_squad(&self, squad: &Squad) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "UPDATE squads SET name = ?2, goal = ?3, status = ?4, updated_at = datetime('now') WHERE id = ?1",
            params![squad.id, squad.name, squad.goal, squad.status],
        )?;
        Ok(())
    }

    /// Delete a squad and its members.
    pub fn delete_squad(&self, squad_id: &str) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM squads WHERE id = ?1", params![squad_id])?;
        Ok(())
    }

    /// Check whether two agents share at least one squad.
    ///
    /// Used by the memory bus to enforce visibility scope: a publication with
    /// `PublicationScope::Squad` should only be delivered to agents that are
    /// co-members of a squad with the publishing agent.
    pub fn agents_share_squad(&self, agent_a: &str, agent_b: &str) -> bool {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM squad_members a
                INNER JOIN squad_members b ON a.squad_id = b.squad_id
                WHERE a.agent_id = ?1 AND b.agent_id = ?2
            )",
            params![agent_a, agent_b],
            |row| row.get::<_, bool>(0),
        )
        .unwrap_or(false)
    }

    /// Check whether a specific agent is a member of a specific squad.
    ///
    /// Used by read-path scope verification to confirm the requesting agent
    /// is authorized to read memories scoped to the given squad.
    pub fn agent_in_squad(&self, agent_id: &str, squad_id: &str) -> bool {
        let conn = self.conn.lock();
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM squad_members WHERE agent_id = ?1 AND squad_id = ?2)",
            params![agent_id, squad_id],
            |row| row.get::<_, bool>(0),
        )
        .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::sessions::schema_for_testing;
    use parking_lot::Mutex;
    use rusqlite::Connection;
    use std::sync::Arc;

    fn test_store() -> SessionStore {
        let conn = Connection::open_in_memory().unwrap();
        schema_for_testing(&conn);
        SessionStore::from_connection(conn)
    }

    #[test]
    fn create_and_list_squads() {
        let store = test_store();
        let squad = Squad {
            id: "s1".into(),
            name: "Alpha Team".into(),
            goal: "Build the thing".into(),
            status: "active".into(),
            members: vec![
                SquadMember {
                    agent_id: "alice".into(),
                    role: "coordinator".into(),
                },
                SquadMember {
                    agent_id: "bob".into(),
                    role: "member".into(),
                },
            ],
            created_at: String::new(),
            updated_at: String::new(),
        };
        store.create_squad(&squad).unwrap();

        let squads = store.list_squads().unwrap();
        assert_eq!(squads.len(), 1);
        assert_eq!(squads[0].name, "Alpha Team");
        assert_eq!(squads[0].members.len(), 2);
    }

    #[test]
    fn add_remove_members() {
        let store = test_store();
        let squad = Squad {
            id: "s1".into(),
            name: "Team".into(),
            goal: "test".into(),
            status: "active".into(),
            members: vec![],
            created_at: String::new(),
            updated_at: String::new(),
        };
        store.create_squad(&squad).unwrap();

        store
            .add_squad_member(
                "s1",
                &SquadMember {
                    agent_id: "alice".into(),
                    role: "coordinator".into(),
                },
            )
            .unwrap();

        let squads = store.list_squads().unwrap();
        assert_eq!(squads[0].members.len(), 1);

        store.remove_squad_member("s1", "alice").unwrap();
        let squads = store.list_squads().unwrap();
        assert_eq!(squads[0].members.len(), 0);
    }

    #[test]
    fn delete_squad_cascades() {
        let store = test_store();
        let squad = Squad {
            id: "s1".into(),
            name: "Team".into(),
            goal: "test".into(),
            status: "active".into(),
            members: vec![SquadMember {
                agent_id: "alice".into(),
                role: "member".into(),
            }],
            created_at: String::new(),
            updated_at: String::new(),
        };
        store.create_squad(&squad).unwrap();
        store.delete_squad("s1").unwrap();

        let squads = store.list_squads().unwrap();
        assert_eq!(squads.len(), 0);
    }
}
