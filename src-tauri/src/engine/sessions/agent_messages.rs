// Agent-to-agent direct messaging — CRUD operations on the `agent_messages` table.
// Independent of the project/orchestrator message bus: any agent can message any other.

use rusqlite::params;
use crate::engine::types::AgentMessage;
use super::SessionStore;
use crate::atoms::error::EngineResult;

impl SessionStore {
    /// Send a direct message between agents.
    pub fn send_agent_message(&self, msg: &AgentMessage) -> EngineResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO agent_messages (id, from_agent, to_agent, channel, content, metadata, read)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![msg.id, msg.from_agent, msg.to_agent, msg.channel, msg.content, msg.metadata, msg.read],
        )?;
        Ok(())
    }

    /// Read messages for an agent, optionally filtered by channel.
    /// Returns unread messages first, then recent read messages up to `limit`.
    pub fn get_agent_messages(
        &self,
        agent_id: &str,
        channel: Option<&str>,
        limit: i64,
    ) -> EngineResult<Vec<AgentMessage>> {
        let conn = self.conn.lock();
        let (sql, p): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(ch) = channel {
            (
                "SELECT id, from_agent, to_agent, channel, content, metadata, read, created_at
                 FROM agent_messages
                 WHERE (to_agent = ?1 OR to_agent = 'broadcast') AND channel = ?2
                 ORDER BY read ASC, created_at DESC LIMIT ?3",
                vec![Box::new(agent_id.to_string()), Box::new(ch.to_string()), Box::new(limit)],
            )
        } else {
            (
                "SELECT id, from_agent, to_agent, channel, content, metadata, read, created_at
                 FROM agent_messages
                 WHERE to_agent = ?1 OR to_agent = 'broadcast'
                 ORDER BY read ASC, created_at DESC LIMIT ?2",
                vec![Box::new(agent_id.to_string()), Box::new(limit)],
            )
        };
        let mut stmt = conn.prepare(sql)?;
        let msgs = stmt.query_map(rusqlite::params_from_iter(p.iter()), |row| {
            Ok(AgentMessage {
                id: row.get(0)?,
                from_agent: row.get(1)?,
                to_agent: row.get(2)?,
                channel: row.get(3)?,
                content: row.get(4)?,
                metadata: row.get(5)?,
                read: row.get::<_, i64>(6)? != 0,
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

    /// Read ALL messages on a given channel (for squad message boards).
    /// Not filtered by recipient — shows the full channel conversation.
    pub fn get_channel_messages(
        &self,
        channel: &str,
        limit: i64,
    ) -> EngineResult<Vec<AgentMessage>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, from_agent, to_agent, channel, content, metadata, read, created_at
             FROM agent_messages
             WHERE channel = ?1
             ORDER BY created_at DESC LIMIT ?2",
        )?;
        let msgs = stmt.query_map(params![channel, limit], |row| {
            Ok(AgentMessage {
                id: row.get(0)?,
                from_agent: row.get(1)?,
                to_agent: row.get(2)?,
                channel: row.get(3)?,
                content: row.get(4)?,
                metadata: row.get(5)?,
                read: row.get::<_, i64>(6)? != 0,
                created_at: row.get(7)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect::<Vec<_>>();

        let mut result = msgs;
        result.reverse();
        Ok(result)
    }

    /// Mark all messages for an agent as read.
    pub fn mark_agent_messages_read(&self, agent_id: &str) -> EngineResult<u64> {
        let conn = self.conn.lock();
        let count = conn.execute(
            "UPDATE agent_messages SET read = 1 WHERE (to_agent = ?1 OR to_agent = 'broadcast') AND read = 0",
            params![agent_id],
        )?;
        Ok(count as u64)
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
    fn send_and_read_messages() {
        let store = test_store();
        let msg = AgentMessage {
            id: "m1".into(),
            from_agent: "alice".into(),
            to_agent: "bob".into(),
            channel: "general".into(),
            content: "Hello Bob!".into(),
            metadata: None,
            read: false,
            created_at: "2025-01-01T00:00:00Z".into(),
        };
        store.send_agent_message(&msg).unwrap();

        let msgs = store.get_agent_messages("bob", None, 50).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "Hello Bob!");
        assert!(!msgs[0].read);
    }

    #[test]
    fn channel_filter() {
        let store = test_store();
        for (i, ch) in ["alerts", "general"].iter().enumerate() {
            let msg = AgentMessage {
                id: format!("m{}", i),
                from_agent: "system".into(),
                to_agent: "bob".into(),
                channel: ch.to_string(),
                content: format!("msg on {}", ch),
                metadata: None,
                read: false,
                created_at: "2025-01-01T00:00:00Z".into(),
            };
            store.send_agent_message(&msg).unwrap();
        }
        let alerts = store.get_agent_messages("bob", Some("alerts"), 50).unwrap();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].channel, "alerts");
    }

    #[test]
    fn mark_read() {
        let store = test_store();
        let msg = AgentMessage {
            id: "m1".into(),
            from_agent: "alice".into(),
            to_agent: "bob".into(),
            channel: "general".into(),
            content: "ping".into(),
            metadata: None,
            read: false,
            created_at: "2025-01-01T00:00:00Z".into(),
        };
        store.send_agent_message(&msg).unwrap();

        let count = store.mark_agent_messages_read("bob").unwrap();
        assert_eq!(count, 1);

        let msgs = store.get_agent_messages("bob", None, 50).unwrap();
        assert!(msgs[0].read);
    }

    #[test]
    fn broadcast_messages_visible_to_all() {
        let store = test_store();
        let msg = AgentMessage {
            id: "m1".into(),
            from_agent: "system".into(),
            to_agent: "broadcast".into(),
            channel: "alerts".into(),
            content: "system alert".into(),
            metadata: None,
            read: false,
            created_at: "2025-01-01T00:00:00Z".into(),
        };
        store.send_agent_message(&msg).unwrap();

        let a = store.get_agent_messages("alice", None, 50).unwrap();
        let b = store.get_agent_messages("bob", None, 50).unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(b.len(), 1);
    }
}
