// ── THE FORGE: Skill Tree via Memory Edges ──────────────────────────────────
//
// Skill trees are DAGs built from Engram's existing memory_edges table.
// Each procedural memory with a domain/skill_tree_path is a node.
// PartOf edges encode parent→child relationships in the tree.
// DependsOn relationships reuse "CausedBy" edges (prerequisite ordering).
//
// No new tables. No new storage. Just structured queries on what exists.

use crate::atoms::error::EngineResult;
use crate::engine::sessions::SessionStore;
use rusqlite::params;
use serde::{Deserialize, Serialize};

// ═════════════════════════════════════════════════════════════════════════════
// Skill Tree Queries
// ═════════════════════════════════════════════════════════════════════════════

/// A node in the skill tree — lightweight view of a procedural memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillTreeNode {
    pub memory_id: String,
    pub trigger: String,
    pub domain: String,
    pub skill_tree_path: String,
    pub certification_status: String,
    pub success_rate: f64,
    pub children: Vec<String>,
}

/// Get the full skill tree for a domain. Returns all procedural memories
/// with a domain assignment, structured as a flat list with child refs.
pub fn get_domain_tree(
    store: &SessionStore,
    agent_id: &str,
    domain: &str,
) -> EngineResult<Vec<SkillTreeNode>> {
    let conn = store.conn.lock();

    // Fetch all skills in this domain
    let mut stmt = conn.prepare(
        "SELECT id, trigger_pattern, domain, skill_tree_path,
                certification_status, success_count, failure_count
         FROM procedural_memories
         WHERE scope_agent_id = ?1 AND domain = ?2 AND skill_tree_path != ''
         ORDER BY skill_tree_path",
    )?;

    let nodes: Vec<SkillTreeNode> = stmt
        .query_map(params![agent_id, domain], |row| {
            let success: i32 = row.get(5)?;
            let failure: i32 = row.get(6)?;
            let total = (success + failure).max(1) as f64;
            Ok(SkillTreeNode {
                memory_id: row.get(0)?,
                trigger: row.get(1)?,
                domain: row.get(2)?,
                skill_tree_path: row.get(3)?,
                certification_status: row.get(4)?,
                success_rate: success as f64 / total,
                children: Vec::new(),
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    // Resolve children via PartOf edges
    let mut result = nodes;
    for node in &mut result {
        let mut child_stmt = conn.prepare(
            "SELECT source_id FROM memory_edges
             WHERE target_id = ?1 AND edge_type = 'part_of'",
        )?;
        let children: Vec<String> = child_stmt
            .query_map(params![node.memory_id], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        node.children = children;
    }

    Ok(result)
}

/// Link a child skill to a parent skill via PartOf edge.
/// Uses the existing memory_edges table — no new storage.
pub fn link_skill_parent(
    store: &SessionStore,
    child_id: &str,
    parent_id: &str,
) -> EngineResult<()> {
    let conn = store.conn.lock();
    let edge_id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT OR IGNORE INTO memory_edges (id, source_id, target_id, edge_type, weight)
         VALUES (?1, ?2, ?3, 'part_of', 0.8)",
        params![edge_id, child_id, parent_id],
    )?;
    Ok(())
}

/// Link a prerequisite dependency (skill B depends on skill A).
/// Uses CausedBy edge type — "A caused B to be possible."
pub fn link_skill_dependency(
    store: &SessionStore,
    skill_id: &str,
    prerequisite_id: &str,
) -> EngineResult<()> {
    let conn = store.conn.lock();
    let edge_id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT OR IGNORE INTO memory_edges (id, source_id, target_id, edge_type, weight)
         VALUES (?1, ?2, ?3, 'caused_by', 0.9)",
        params![edge_id, skill_id, prerequisite_id],
    )?;
    Ok(())
}

/// Get all prerequisites for a skill (skills it depends on).
pub fn get_prerequisites(store: &SessionStore, skill_id: &str) -> EngineResult<Vec<String>> {
    let conn = store.conn.lock();
    let mut stmt = conn.prepare(
        "SELECT target_id FROM memory_edges
         WHERE source_id = ?1 AND edge_type = 'caused_by'",
    )?;
    let ids = stmt
        .query_map(params![skill_id], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(ids)
}

/// Check if all prerequisites for a skill are certified.
pub fn prerequisites_met(store: &SessionStore, skill_id: &str) -> EngineResult<bool> {
    let prereqs = get_prerequisites(store, skill_id)?;
    if prereqs.is_empty() {
        return Ok(true);
    }

    let conn = store.conn.lock();
    for prereq_id in &prereqs {
        let status: String = conn
            .query_row(
                "SELECT certification_status FROM procedural_memories WHERE id = ?1",
                params![prereq_id],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| "uncertified".to_string());
        if status != "certified" {
            return Ok(false);
        }
    }
    Ok(true)
}

/// List all domains that have at least one FORGE-tagged procedural memory.
pub fn list_domains(store: &SessionStore, agent_id: &str) -> EngineResult<Vec<DomainSummary>> {
    let conn = store.conn.lock();
    let mut stmt = conn.prepare(
        "SELECT domain,
                COUNT(*) as total,
                SUM(CASE WHEN certification_status = 'certified' THEN 1 ELSE 0 END) as certified
         FROM procedural_memories
         WHERE scope_agent_id = ?1 AND domain != ''
         GROUP BY domain
         ORDER BY domain",
    )?;
    let items = stmt
        .query_map(params![agent_id], |row| {
            Ok(DomainSummary {
                domain: row.get(0)?,
                total_skills: row.get::<_, i64>(1)? as usize,
                certified_skills: row.get::<_, i64>(2)? as usize,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(items)
}

/// Summary of a training domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainSummary {
    pub domain: String,
    pub total_skills: usize,
    pub certified_skills: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atoms::engram_types::{MemoryScope, ProceduralMemory};
    use crate::engine::forge::certification;
    use crate::engine::sessions::schema::run_migrations;
    use parking_lot::Mutex;
    use rusqlite::Connection;
    use std::sync::Arc;

    fn test_store() -> SessionStore {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        SessionStore {
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    fn seed_skill(store: &SessionStore, id: &str, trigger: &str) {
        let mem = ProceduralMemory {
            id: id.to_string(),
            trigger: trigger.to_string(),
            steps: vec![],
            success_rate: 0.0,
            execution_count: 0,
            scope: MemoryScope::agent("agent-1"),
            created_at: "2026-03-01T00:00:00Z".to_string(),
            updated_at: None,
        };
        store.engram_store_procedural(&mem).unwrap();
    }

    #[test]
    fn test_domain_tree() {
        let store = test_store();
        seed_skill(&store, "s1", "create hubspot contact");
        seed_skill(&store, "s2", "list hubspot contacts");

        certification::certify_skill(&store, "s1", "hubspot", "hubspot.contacts.create", None)
            .unwrap();
        certification::certify_skill(&store, "s2", "hubspot", "hubspot.contacts.list", None)
            .unwrap();

        let tree = get_domain_tree(&store, "agent-1", "hubspot").unwrap();
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].domain, "hubspot");
    }

    #[test]
    fn test_skill_parent_link() {
        let store = test_store();
        seed_skill(&store, "s1", "contacts module");
        seed_skill(&store, "s2", "create contact");

        certification::certify_skill(&store, "s1", "hubspot", "hubspot.contacts", None).unwrap();
        certification::certify_skill(&store, "s2", "hubspot", "hubspot.contacts.create", None)
            .unwrap();

        link_skill_parent(&store, "s2", "s1").unwrap();

        let tree = get_domain_tree(&store, "agent-1", "hubspot").unwrap();
        let parent = tree.iter().find(|n| n.memory_id == "s1").unwrap();
        assert!(parent.children.contains(&"s2".to_string()));
    }

    #[test]
    fn test_prerequisites() {
        let store = test_store();
        seed_skill(&store, "s1", "deal stages");
        seed_skill(&store, "s2", "deal stage trigger");

        link_skill_dependency(&store, "s2", "s1").unwrap();

        // s1 not certified yet — s2 prerequisites not met
        assert!(!prerequisites_met(&store, "s2").unwrap());

        // Certify s1
        certification::certify_skill(&store, "s1", "hubspot", "hubspot.deals.stages", None)
            .unwrap();

        // Now s2 prerequisites are met
        assert!(prerequisites_met(&store, "s2").unwrap());
    }

    #[test]
    fn test_list_domains() {
        let store = test_store();
        seed_skill(&store, "s1", "create contact");
        seed_skill(&store, "s2", "create deal");
        seed_skill(&store, "s3", "charge card");

        certification::certify_skill(&store, "s1", "hubspot", "hubspot.contacts.create", None)
            .unwrap();
        certification::certify_skill(&store, "s2", "hubspot", "hubspot.deals.create", None)
            .unwrap();
        certification::certify_skill(&store, "s3", "stripe", "stripe.charges.create", None)
            .unwrap();

        let domains = list_domains(&store, "agent-1").unwrap();
        assert_eq!(domains.len(), 2);
        assert_eq!(domains[0].domain, "hubspot");
        assert_eq!(domains[0].total_skills, 2);
        assert_eq!(domains[0].certified_skills, 2);
        assert_eq!(domains[1].domain, "stripe");
    }

    #[test]
    fn test_no_prerequisites() {
        let store = test_store();
        seed_skill(&store, "s1", "simple skill");
        // No dependencies linked — prerequisites should be met
        assert!(prerequisites_met(&store, "s1").unwrap());
    }
}
