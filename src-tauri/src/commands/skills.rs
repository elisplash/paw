// commands/skills.rs — Thin wrappers for skill vault commands.
// Credential encryption lives in engine/skills.rs.

use crate::commands::state::EngineState;
use crate::engine::skills;
use log::info;
use tauri::State;

#[tauri::command]
pub fn engine_skills_list(
    state: State<'_, EngineState>,
) -> Result<Vec<skills::SkillStatus>, String> {
    skills::get_all_skill_status(&state.store).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_skill_set_enabled(
    state: State<'_, EngineState>,
    skill_id: String,
    enabled: bool,
) -> Result<(), String> {
    info!("[engine] Skill {} → enabled={}", skill_id, enabled);
    state.store.set_skill_enabled(&skill_id, enabled).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_skill_set_credential(
    state: State<'_, EngineState>,
    skill_id: String,
    key: String,
    value: String,
) -> Result<(), String> {
    let vault_key = skills::get_vault_key()?;
    let encrypted = skills::encrypt_credential(&value, &vault_key);
    info!("[engine] Setting credential {}:{} ({} chars)", skill_id, key, value.len());
    state.store.set_skill_credential(&skill_id, &key, &encrypted).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_skill_delete_credential(
    state: State<'_, EngineState>,
    skill_id: String,
    key: String,
) -> Result<(), String> {
    info!("[engine] Deleting credential {}:{}", skill_id, key);
    state.store.delete_skill_credential(&skill_id, &key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_skill_revoke_all(
    state: State<'_, EngineState>,
    skill_id: String,
) -> Result<(), String> {
    info!("[engine] Revoking all credentials for skill {}", skill_id);
    state.store.delete_all_skill_credentials(&skill_id)?;
    state.store.set_skill_enabled(&skill_id, false).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_skill_get_instructions(
    state: State<'_, EngineState>,
    skill_id: String,
) -> Result<Option<String>, String> {
    state.store.get_skill_custom_instructions(&skill_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_skill_set_instructions(
    state: State<'_, EngineState>,
    skill_id: String,
    instructions: String,
) -> Result<(), String> {
    info!("[engine] Setting custom instructions for skill {} ({} chars)", skill_id, instructions.len());
    state.store.set_skill_custom_instructions(&skill_id, &instructions).map_err(|e| e.to_string())
}

// ── Community Skills (skills.sh) ───────────────────────────────────────

#[tauri::command]
pub fn engine_community_skills_list(
    state: State<'_, EngineState>,
) -> Result<Vec<skills::CommunitySkill>, String> {
    state.store.list_community_skills().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn engine_community_skills_browse(
    source: String,
    state: State<'_, EngineState>,
) -> Result<Vec<skills::DiscoveredSkill>, String> {
    let mut discovered = skills::fetch_repo_skills(&source).await?;

    // Mark which ones are already installed
    let installed = state.store.list_community_skills()?;
    let installed_ids: std::collections::HashSet<String> = installed.iter().map(|s| s.id.clone()).collect();
    for skill in &mut discovered {
        skill.installed = installed_ids.contains(&skill.id);
    }

    Ok(discovered)
}

#[tauri::command]
pub async fn engine_community_skills_search(
    query: String,
    state: State<'_, EngineState>,
) -> Result<Vec<skills::DiscoveredSkill>, String> {
    let mut discovered = skills::search_community_skills(&query).await?;

    // Mark which ones are already installed
    let installed = state.store.list_community_skills()?;
    let installed_ids: std::collections::HashSet<String> = installed.iter().map(|s| s.id.clone()).collect();
    for skill in &mut discovered {
        skill.installed = installed_ids.contains(&skill.id);
    }

    Ok(discovered)
}

#[tauri::command]
pub async fn engine_community_skill_install(
    source: String,
    skill_path: String,
    state: State<'_, EngineState>,
) -> Result<skills::CommunitySkill, String> {
    info!("[engine] Installing community skill from {} path {} (UI — all agents)", source, skill_path);
    skills::install_community_skill(&state.store, &source, &skill_path, None).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_community_skill_remove(
    state: State<'_, EngineState>,
    skill_id: String,
) -> Result<(), String> {
    info!("[engine] Removing community skill: {}", skill_id);
    state.store.remove_community_skill(&skill_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_community_skill_set_enabled(
    state: State<'_, EngineState>,
    skill_id: String,
    enabled: bool,
) -> Result<(), String> {
    info!("[engine] Community skill {} → enabled={}", skill_id, enabled);
    state.store.set_community_skill_enabled(&skill_id, enabled).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn engine_community_skill_set_agents(
    state: State<'_, EngineState>,
    skill_id: String,
    agent_ids: Vec<String>,
) -> Result<(), String> {
    info!("[engine] Community skill {} → agent_ids={:?}", skill_id, agent_ids);
    state.store.set_community_skill_agents(&skill_id, &agent_ids).map_err(|e| e.to_string())
}
