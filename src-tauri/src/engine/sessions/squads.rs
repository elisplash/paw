// Agent Squads — CRUD operations on the `squads` and `squad_members` tables.
// Squads group agents into named teams that can be assigned goals collectively.

use rusqlite::params;
use crate::engine::types::{Squad, SquadMember};
use super::SessionStore;
use crate::atoms::error::EngineResult;

impl SessionStore {
    /// List all squads with their members.
    pub fn list_squads(&self) -> EngineResult<Vec<Squad>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, name, goal, status, created_at, updated_at
             FROM squads ORDER BY updated_at DESC"
        )?;
        let squads = stmt.query_map([], |row| {
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
            let mut m_stmt = conn.prepare(
                "SELECT agent_id, role FROM squad_members WHERE squad_id = ?1"
            )?;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::sessions::schema_for_testing;
    use rusqlite::Connection;
    use parking_lot::Mutex;

    fn test_store() -> SessionStore {
        let conn = Connection::open_in_memory().unwrap();
        schema_for_testing(&conn);
        SessionStore { conn: Mutex::new(conn) }
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
                SquadMember { agent_id: "alice".into(), role: "coordinator".into() },
                SquadMember { agent_id: "bob".into(), role: "member".into() },
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

        store.add_squad_member("s1", &SquadMember {
            agent_id: "alice".into(),
            role: "coordinator".into(),
        }).unwrap();

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
            members: vec![
                SquadMember { agent_id: "alice".into(), role: "member".into() },
            ],
            created_at: String::new(),
            updated_at: String::new(),
        };
        store.create_squad(&squad).unwrap();
        store.delete_squad("s1").unwrap();

        let squads = store.list_squads().unwrap();
        assert_eq!(squads.len(), 0);
    }
}
