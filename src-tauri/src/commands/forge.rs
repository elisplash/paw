// commands/forge.rs — Thin Tauri wrappers for FORGE certification & skill tree queries.
// Business logic lives in engine/forge/. These just deserialise, delegate, serialise.

use crate::commands::state::EngineState;
use crate::engine::forge::{certification, skill_tree};
use tauri::State;

// ── Certification Queries ──────────────────────────────────────────────────

#[tauri::command]
pub fn engine_forge_cert_summary(
    state: State<'_, EngineState>,
    agent_id: String,
) -> Result<certification::CertificationSummary, String> {
    certification::certification_summary(&state.store, &agent_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_forge_list_certified(
    state: State<'_, EngineState>,
    agent_id: String,
    domain: Option<String>,
) -> Result<Vec<ForgeSkillRow>, String> {
    let items = certification::list_certified_skills(&state.store, &agent_id, domain.as_deref())
        .map_err(|e| e.to_string())?;

    Ok(items
        .into_iter()
        .map(|(mem, meta)| ForgeSkillRow {
            memory_id: mem.id,
            trigger: mem.trigger,
            domain: meta.domain,
            skill_tree_path: meta.skill_tree_path,
            certification_status: format!("{:?}", meta.certification_status),
            success_rate: mem.success_rate,
            certified_at: meta.certified_at,
        })
        .collect())
}

#[tauri::command]
pub fn engine_forge_metadata(
    state: State<'_, EngineState>,
    memory_id: String,
) -> Result<Option<ForgeMetadataRow>, String> {
    let meta =
        certification::get_forge_metadata(&state.store, &memory_id).map_err(|e| e.to_string())?;

    Ok(meta.map(|m| ForgeMetadataRow {
        certification_status: format!("{:?}", m.certification_status),
        domain: m.domain,
        skill_tree_path: m.skill_tree_path,
        curriculum_source: m.curriculum_source,
        certified_at: m.certified_at,
    }))
}

// ── Skill Tree Queries ─────────────────────────────────────────────────────

#[tauri::command]
pub fn engine_forge_domain_tree(
    state: State<'_, EngineState>,
    agent_id: String,
    domain: String,
) -> Result<Vec<skill_tree::SkillTreeNode>, String> {
    skill_tree::get_domain_tree(&state.store, &agent_id, &domain).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_forge_list_domains(
    state: State<'_, EngineState>,
    agent_id: String,
) -> Result<Vec<skill_tree::DomainSummary>, String> {
    skill_tree::list_domains(&state.store, &agent_id).map_err(|e| e.to_string())
}

// ── Serialisation DTOs ─────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ForgeSkillRow {
    pub memory_id: String,
    pub trigger: String,
    pub domain: String,
    pub skill_tree_path: String,
    pub certification_status: String,
    pub success_rate: f32,
    pub certified_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ForgeMetadataRow {
    pub certification_status: String,
    pub domain: String,
    pub skill_tree_path: String,
    pub curriculum_source: Option<String>,
    pub certified_at: Option<String>,
}
