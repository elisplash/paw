// commands/skills.rs — Thin wrappers for skill vault commands.
// Credential encryption lives in engine/skills.rs.
// TOML manifest commands (Phase F.1) + MCP server sharing (Phase F.3).

use crate::commands::state::EngineState;
use crate::engine::skills;
use crate::engine::channels;
use crate::engine::mcp::types::{McpServerConfig, McpTransport};
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

// ── TOML Manifest Skills (Phase F.1) + MCP Sharing (Phase F.3) ─────

/// Scan `~/.paw/skills/*/pawz-skill.toml` and return all valid entries.
#[tauri::command]
pub fn engine_toml_skills_scan() -> Result<Vec<skills::TomlSkillEntry>, String> {
    Ok(skills::scan_toml_skills())
}

/// MCP server config key — shared with commands/mcp.rs.
const MCP_CONFIG_KEY: &str = "mcp_servers";

/// Install a TOML skill and auto-register its MCP server if declared.
#[tauri::command]
pub async fn engine_toml_skill_install(
    app_handle: tauri::AppHandle,
    state: State<'_, EngineState>,
    skill_id: String,
    toml_content: String,
) -> Result<String, String> {
    info!("[engine] Installing TOML skill '{}'", skill_id);

    // 1. Write files to disk
    let path = skills::install_toml_skill(&skill_id, &toml_content)?;

    // 2. If manifest has [mcp], auto-register the MCP server
    if let Ok(manifest) = skills::parse_manifest(&toml_content) {
        if let Some(mcp) = &manifest.mcp {
            info!("[engine] Skill '{}' declares MCP server — auto-registering", skill_id);

            // Build env: manifest env + decrypted credentials
            let mut env = mcp.env.clone();
            if let Ok(creds) = skills::get_skill_credentials(&state.store, &skill_id) {
                for (k, v) in creds {
                    env.insert(k, v);
                }
            }

            let transport = match mcp.transport.as_str() {
                "sse" => McpTransport::Sse,
                _ => McpTransport::Stdio,
            };

            let config = McpServerConfig {
                id: format!("skill-{}", skill_id),
                name: manifest.skill.name.clone(),
                transport,
                command: mcp.command.clone(),
                args: mcp.args.clone(),
                env,
                url: mcp.url.clone(),
                enabled: true,
            };

            // Persist to MCP server list
            let mut servers: Vec<McpServerConfig> =
                channels::load_channel_config(&app_handle, MCP_CONFIG_KEY).unwrap_or_default();
            if let Some(pos) = servers.iter().position(|s| s.id == config.id) {
                servers[pos] = config.clone();
            } else {
                servers.push(config.clone());
            }
            if let Err(e) = channels::save_channel_config(&app_handle, MCP_CONFIG_KEY, &servers) {
                log::warn!("[engine] Failed to persist MCP config for skill '{}': {}", skill_id, e);
            }

            // Connect the MCP server
            let mut reg = state.mcp_registry.lock().await;
            if let Err(e) = reg.connect(config).await {
                // Non-fatal: skill is installed, MCP connection can be retried
                log::warn!("[engine] MCP server for skill '{}' failed to connect: {}", skill_id, e);
            }
        }
    }

    Ok(path.to_string_lossy().to_string())
}

/// Uninstall a TOML skill and disconnect its MCP server if one was registered.
#[tauri::command]
pub async fn engine_toml_skill_uninstall(
    app_handle: tauri::AppHandle,
    state: State<'_, EngineState>,
    skill_id: String,
) -> Result<(), String> {
    info!("[engine] Uninstalling TOML skill '{}'", skill_id);

    let mcp_id = format!("skill-{}", skill_id);

    // 1. Disconnect MCP server if running
    {
        let mut reg = state.mcp_registry.lock().await;
        reg.disconnect(&mcp_id).await;
    }

    // 2. Remove from persisted MCP config
    let mut servers: Vec<McpServerConfig> =
        channels::load_channel_config(&app_handle, MCP_CONFIG_KEY).unwrap_or_default();
    let before = servers.len();
    servers.retain(|s| s.id != mcp_id);
    if servers.len() < before {
        info!("[engine] Removed MCP server '{}' for skill '{}'", mcp_id, skill_id);
        if let Err(e) = channels::save_channel_config(&app_handle, MCP_CONFIG_KEY, &servers) {
            log::warn!("[engine] Failed to update MCP config after uninstall: {}", e);
        }
    }

    // 3. Remove skill files from disk
    skills::uninstall_toml_skill(&skill_id)
}

// ── PawzHub Registry (Phase F.4) ───────────────────────────────────

/// Search the PawzHub registry by query. Returns all entries if query is empty.
#[tauri::command]
pub async fn engine_pawzhub_search(
    query: String,
) -> Result<Vec<skills::PawzHubEntry>, String> {
    info!("[engine] PawzHub search: '{}'", query);
    let mut entries = skills::search_pawzhub(&query).await.map_err(|e| e.to_string())?;

    // Mark installed entries
    let installed_ids: std::collections::HashSet<String> =
        skills::scan_toml_skills().into_iter().map(|s| s.definition.id).collect();
    for entry in &mut entries {
        entry.installed = installed_ids.contains(&entry.id);
    }

    Ok(entries)
}

/// Browse PawzHub by category.
#[tauri::command]
pub async fn engine_pawzhub_browse(
    category: String,
) -> Result<Vec<skills::PawzHubEntry>, String> {
    info!("[engine] PawzHub browse category: '{}'", category);
    let mut entries = skills::browse_pawzhub_category(&category).await.map_err(|e| e.to_string())?;

    let installed_ids: std::collections::HashSet<String> =
        skills::scan_toml_skills().into_iter().map(|s| s.definition.id).collect();
    for entry in &mut entries {
        entry.installed = installed_ids.contains(&entry.id);
    }

    Ok(entries)
}

/// Fetch a pawz-skill.toml from PawzHub and install it.
#[tauri::command]
pub async fn engine_pawzhub_install(
    app_handle: tauri::AppHandle,
    state: State<'_, EngineState>,
    skill_id: String,
    source_repo: String,
) -> Result<String, String> {
    info!("[engine] PawzHub install: '{}' from {}", skill_id, source_repo);

    // Fetch the TOML manifest from GitHub
    let toml_content = skills::fetch_pawzhub_toml(&source_repo, &skill_id)
        .await
        .map_err(|e| e.to_string())?;

    // Delegate to the existing TOML install (which handles MCP wiring via F.3)
    engine_toml_skill_install(app_handle, state, skill_id, toml_content).await
}

// ── Skill Outputs (Phase F.2 — Dashboard Widgets) ──────────────────

/// List all skill outputs for dashboard widget rendering.
#[tauri::command]
pub fn engine_list_skill_outputs(
    state: State<'_, EngineState>,
    skill_id: Option<String>,
    agent_id: Option<String>,
) -> Result<Vec<crate::engine::sessions::SkillOutput>, String> {
    state
        .store
        .list_skill_outputs(
            skill_id.as_deref(),
            agent_id.as_deref(),
        )
        .map_err(|e| e.to_string())
}

/// List all key-value pairs in a skill's persistent storage.
#[tauri::command]
pub fn engine_skill_store_list(
    state: State<'_, EngineState>,
    skill_id: String,
) -> Result<Vec<crate::engine::sessions::SkillStorageItem>, String> {
    state.store.skill_store_list(&skill_id).map_err(|e| e.to_string())
}
