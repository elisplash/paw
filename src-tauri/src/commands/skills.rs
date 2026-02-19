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
    skills::get_all_skill_status(&state.store)
}

#[tauri::command]
pub fn engine_skill_set_enabled(
    state: State<'_, EngineState>,
    skill_id: String,
    enabled: bool,
) -> Result<(), String> {
    info!("[engine] Skill {} → enabled={}", skill_id, enabled);
    state.store.set_skill_enabled(&skill_id, enabled)
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
    state.store.set_skill_credential(&skill_id, &key, &encrypted)
}

#[tauri::command]
pub fn engine_skill_delete_credential(
    state: State<'_, EngineState>,
    skill_id: String,
    key: String,
) -> Result<(), String> {
    info!("[engine] Deleting credential {}:{}", skill_id, key);
    state.store.delete_skill_credential(&skill_id, &key)
}

#[tauri::command]
pub fn engine_skill_revoke_all(
    state: State<'_, EngineState>,
    skill_id: String,
) -> Result<(), String> {
    info!("[engine] Revoking all credentials for skill {}", skill_id);
    state.store.delete_all_skill_credentials(&skill_id)?;
    state.store.set_skill_enabled(&skill_id, false)
}

#[tauri::command]
pub fn engine_skill_get_instructions(
    state: State<'_, EngineState>,
    skill_id: String,
) -> Result<Option<String>, String> {
    state.store.get_skill_custom_instructions(&skill_id)
}

#[tauri::command]
pub fn engine_skill_set_instructions(
    state: State<'_, EngineState>,
    skill_id: String,
    instructions: String,
) -> Result<(), String> {
    info!("[engine] Setting custom instructions for skill {} ({} chars)", skill_id, instructions.len());
    state.store.set_skill_custom_instructions(&skill_id, &instructions)
}
