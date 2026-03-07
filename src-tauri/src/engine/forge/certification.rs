// ── THE FORGE: Certification Operations ─────────────────────────────────────
//
// Business logic for certifying, expiring, and querying FORGE-trained
// procedural memories. All DB access goes through SessionStore.
//
// This module does NOT create new tables — it reads/writes the
// certification_status, domain, skill_tree_path, curriculum_source,
// and certified_at columns added to procedural_memories by FORGE.

use crate::atoms::engram_types::{CertificationStatus, ForgeMetadata, ProceduralMemory};
use crate::atoms::error::EngineResult;
use crate::engine::sessions::SessionStore;
use log::info;
use rusqlite::params;

// ═════════════════════════════════════════════════════════════════════════════
// Certification
// ═════════════════════════════════════════════════════════════════════════════

/// Certify a procedural memory — mark it as verified by FORGE.
pub fn certify_skill(
    store: &SessionStore,
    memory_id: &str,
    domain: &str,
    skill_tree_path: &str,
    curriculum_source: Option<&str>,
) -> EngineResult<()> {
    let conn = store.conn.lock();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    conn.execute(
        "UPDATE procedural_memories SET
            certification_status = 'certified',
            domain = ?2,
            skill_tree_path = ?3,
            curriculum_source = ?4,
            certified_at = ?5,
            updated_at = ?5
         WHERE id = ?1",
        params![memory_id, domain, skill_tree_path, curriculum_source, now],
    )?;
    info!(
        "[forge] Certified skill: {} → {}.{}",
        memory_id, domain, skill_tree_path
    );
    Ok(())
}

/// Mark a skill as in-training (currently being evaluated).
pub fn begin_training(
    store: &SessionStore,
    memory_id: &str,
    domain: &str,
    skill_tree_path: &str,
) -> EngineResult<()> {
    let conn = store.conn.lock();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    conn.execute(
        "UPDATE procedural_memories SET
            certification_status = 'in_training',
            domain = ?2,
            skill_tree_path = ?3,
            updated_at = ?4
         WHERE id = ?1",
        params![memory_id, domain, skill_tree_path, now],
    )?;
    Ok(())
}

/// Mark a certified skill as expired (trust decayed or time-based).
pub fn expire_skill(store: &SessionStore, memory_id: &str) -> EngineResult<()> {
    let conn = store.conn.lock();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    conn.execute(
        "UPDATE procedural_memories SET
            certification_status = 'expired',
            updated_at = ?2
         WHERE id = ?1",
        params![memory_id, now],
    )?;
    info!("[forge] Expired skill: {}", memory_id);
    Ok(())
}

/// Mark a skill as failed (exceeded max training attempts).
pub fn fail_skill(store: &SessionStore, memory_id: &str) -> EngineResult<()> {
    let conn = store.conn.lock();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    conn.execute(
        "UPDATE procedural_memories SET
            certification_status = 'failed',
            updated_at = ?2
         WHERE id = ?1",
        params![memory_id, now],
    )?;
    info!("[forge] Failed skill: {}", memory_id);
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// Queries
// ═════════════════════════════════════════════════════════════════════════════

/// Get forge metadata for a procedural memory.
pub fn get_forge_metadata(
    store: &SessionStore,
    memory_id: &str,
) -> EngineResult<Option<ForgeMetadata>> {
    let conn = store.conn.lock();
    let result = conn.query_row(
        "SELECT certification_status, domain, skill_tree_path,
                curriculum_source, certified_at
         FROM procedural_memories WHERE id = ?1",
        params![memory_id],
        |row| {
            Ok(ForgeMetadata {
                certification_status: CertificationStatus::parse(&row.get::<_, String>(0)?),
                domain: row.get(1)?,
                skill_tree_path: row.get(2)?,
                curriculum_source: row.get(3)?,
                certified_at: row.get(4)?,
            })
        },
    );
    match result {
        Ok(meta) => Ok(Some(meta)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// List all certified procedural memories for an agent in a domain.
pub fn list_certified_skills(
    store: &SessionStore,
    agent_id: &str,
    domain: Option<&str>,
) -> EngineResult<Vec<(ProceduralMemory, ForgeMetadata)>> {
    let conn = store.conn.lock();

    let (sql, param_count) = if domain.is_some() {
        (
            "SELECT id, trigger_pattern, steps_json, success_count, failure_count,
                    scope_agent_id, scope_project_id, created_at, updated_at,
                    certification_status, domain, skill_tree_path,
                    curriculum_source, certified_at
             FROM procedural_memories
             WHERE scope_agent_id = ?1 AND certification_status = 'certified' AND domain = ?2
             ORDER BY skill_tree_path",
            2,
        )
    } else {
        (
            "SELECT id, trigger_pattern, steps_json, success_count, failure_count,
                    scope_agent_id, scope_project_id, created_at, updated_at,
                    certification_status, domain, skill_tree_path,
                    curriculum_source, certified_at
             FROM procedural_memories
             WHERE scope_agent_id = ?1 AND certification_status = 'certified'
             ORDER BY domain, skill_tree_path",
            1,
        )
    };

    let mut stmt = conn.prepare(sql)?;

    let rows = if param_count == 2 {
        let d = domain.unwrap_or("");
        stmt.query_map(params![agent_id, d], row_to_skill_pair)?
    } else {
        stmt.query_map(params![agent_id], row_to_skill_pair)?
    };

    let items: Vec<(ProceduralMemory, ForgeMetadata)> = rows.filter_map(|r| r.ok()).collect();
    Ok(items)
}

/// List skills needing re-certification (expired or low success rate).
pub fn list_stale_skills(
    store: &SessionStore,
    agent_id: &str,
    min_success_rate: f64,
) -> EngineResult<Vec<(ProceduralMemory, ForgeMetadata)>> {
    let conn = store.conn.lock();
    let mut stmt = conn.prepare(
        "SELECT id, trigger_pattern, steps_json, success_count, failure_count,
                scope_agent_id, scope_project_id, created_at, updated_at,
                certification_status, domain, skill_tree_path,
                curriculum_source, certified_at
         FROM procedural_memories
         WHERE scope_agent_id = ?1
           AND certification_status IN ('certified', 'expired')
           AND (certification_status = 'expired'
                OR CAST(success_count AS REAL) / MAX(success_count + failure_count, 1) < ?2)
         ORDER BY CAST(success_count AS REAL) / MAX(success_count + failure_count, 1) ASC",
    )?;
    let items = stmt
        .query_map(params![agent_id, min_success_rate], row_to_skill_pair)?
        .filter_map(|r| r.ok())
        .collect();
    Ok(items)
}

/// Count skills by certification status for an agent.
pub fn certification_summary(
    store: &SessionStore,
    agent_id: &str,
) -> EngineResult<CertificationSummary> {
    let conn = store.conn.lock();
    let mut summary = CertificationSummary::default();

    let mut stmt = conn.prepare(
        "SELECT certification_status, COUNT(*)
         FROM procedural_memories
         WHERE scope_agent_id = ?1
         GROUP BY certification_status",
    )?;
    let rows = stmt.query_map(params![agent_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;

    for row in rows.flatten() {
        match row.0.as_str() {
            "uncertified" => summary.uncertified = row.1 as usize,
            "in_training" => summary.in_training = row.1 as usize,
            "certified" => summary.certified = row.1 as usize,
            "expired" => summary.expired = row.1 as usize,
            "failed" => summary.failed = row.1 as usize,
            _ => {}
        }
    }
    Ok(summary)
}

/// Summary of certification counts for an agent.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CertificationSummary {
    pub uncertified: usize,
    pub in_training: usize,
    pub certified: usize,
    pub expired: usize,
    pub failed: usize,
}

// ═════════════════════════════════════════════════════════════════════════════
// Internal Helpers
// ═════════════════════════════════════════════════════════════════════════════

/// Parse a row into (ProceduralMemory, ForgeMetadata) pair.
fn row_to_skill_pair(row: &rusqlite::Row) -> rusqlite::Result<(ProceduralMemory, ForgeMetadata)> {
    let steps_json: String = row.get(2)?;
    let success: i32 = row.get(3)?;
    let failure: i32 = row.get(4)?;
    let total = (success + failure).max(1) as u32;
    let rate = success as f32 / total as f32;

    let scope_agent: String = row.get(5)?;
    let scope_project: Option<String> = row.get(6)?;

    let mem = ProceduralMemory {
        id: row.get(0)?,
        trigger: row.get(1)?,
        steps: serde_json::from_str(&steps_json).unwrap_or_default(),
        success_rate: rate,
        execution_count: total,
        scope: crate::atoms::engram_types::MemoryScope {
            agent_id: Some(scope_agent),
            project_id: scope_project,
            ..Default::default()
        },
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    };

    let meta = ForgeMetadata {
        certification_status: CertificationStatus::parse(&row.get::<_, String>(9)?),
        domain: row.get(10)?,
        skill_tree_path: row.get(11)?,
        curriculum_source: row.get(12)?,
        certified_at: row.get(13)?,
    };

    Ok((mem, meta))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atoms::engram_types::{MemoryScope, ProceduralMemory};
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
    fn test_certify_and_query() {
        let store = test_store();
        seed_skill(&store, "s1", "create hubspot contact");

        certify_skill(
            &store,
            "s1",
            "hubspot",
            "hubspot.contacts.create",
            Some("https://academy.hubspot.com"),
        )
        .unwrap();

        let meta = get_forge_metadata(&store, "s1").unwrap().unwrap();
        assert_eq!(meta.certification_status, CertificationStatus::Certified);
        assert_eq!(meta.domain, "hubspot");
        assert_eq!(meta.skill_tree_path, "hubspot.contacts.create");
        assert!(meta.certified_at.is_some());
    }

    #[test]
    fn test_list_certified() {
        let store = test_store();
        seed_skill(&store, "s1", "create contact");
        seed_skill(&store, "s2", "create deal");
        seed_skill(&store, "s3", "send email");

        certify_skill(&store, "s1", "hubspot", "hubspot.contacts.create", None).unwrap();
        certify_skill(&store, "s2", "hubspot", "hubspot.deals.create", None).unwrap();
        // s3 stays uncertified

        let certified = list_certified_skills(&store, "agent-1", Some("hubspot")).unwrap();
        assert_eq!(certified.len(), 2);

        let all_certified = list_certified_skills(&store, "agent-1", None).unwrap();
        assert_eq!(all_certified.len(), 2);
    }

    #[test]
    fn test_expire_skill() {
        let store = test_store();
        seed_skill(&store, "s1", "create contact");
        certify_skill(&store, "s1", "hubspot", "hubspot.contacts.create", None).unwrap();

        expire_skill(&store, "s1").unwrap();

        let meta = get_forge_metadata(&store, "s1").unwrap().unwrap();
        assert_eq!(meta.certification_status, CertificationStatus::Expired);
    }

    #[test]
    fn test_fail_skill() {
        let store = test_store();
        seed_skill(&store, "s1", "create contact");
        begin_training(&store, "s1", "hubspot", "hubspot.contacts.create").unwrap();

        let meta = get_forge_metadata(&store, "s1").unwrap().unwrap();
        assert_eq!(meta.certification_status, CertificationStatus::InTraining);

        fail_skill(&store, "s1").unwrap();

        let meta = get_forge_metadata(&store, "s1").unwrap().unwrap();
        assert_eq!(meta.certification_status, CertificationStatus::Failed);
    }

    #[test]
    fn test_certification_summary() {
        let store = test_store();
        seed_skill(&store, "s1", "create contact");
        seed_skill(&store, "s2", "create deal");
        seed_skill(&store, "s3", "send email");

        certify_skill(&store, "s1", "hubspot", "hubspot.contacts.create", None).unwrap();
        fail_skill(&store, "s2").unwrap();
        // s3 stays uncertified

        let summary = certification_summary(&store, "agent-1").unwrap();
        assert_eq!(summary.certified, 1);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.uncertified, 1);
    }

    #[test]
    fn test_missing_memory() {
        let store = test_store();
        let meta = get_forge_metadata(&store, "nonexistent").unwrap();
        assert!(meta.is_none());
    }
}
