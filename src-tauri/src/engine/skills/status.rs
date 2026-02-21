// Pawz Agent Engine — Skill Status
// Aggregates builtin skill definitions with stored DB state.

use log::warn;
use crate::engine::sessions::SessionStore;
use super::builtins::builtin_skills;
use super::types::SkillStatus;
use super::crypto::{decrypt_credential, get_vault_key};

/// Get the combined status of all skills (definition + stored state).
pub fn get_all_skill_status(store: &SessionStore) -> Result<Vec<SkillStatus>, String> {
    let definitions = builtin_skills();
    let mut statuses = Vec::new();

    for def in &definitions {
        let enabled = store.is_skill_enabled(&def.id)?;
        let configured_keys = store.list_skill_credential_keys(&def.id)?;
        let missing_creds: Vec<String> = def.required_credentials.iter()
            .filter(|c| c.required && !configured_keys.contains(&c.key))
            .map(|c| c.key.clone())
            .collect();

        // Check which required binaries are missing from PATH
        let missing_bins: Vec<String> = def.required_binaries.iter()
            .filter(|bin| {
                std::process::Command::new("which")
                    .arg(bin)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map(|s| !s.success())
                    .unwrap_or(true)
            })
            .cloned()
            .collect();

        // Check which required env vars are missing
        let missing_envs: Vec<String> = def.required_env_vars.iter()
            .filter(|v| std::env::var(v).is_err())
            .cloned()
            .collect();

        let is_ready = enabled && missing_creds.is_empty() && missing_bins.is_empty() && missing_envs.is_empty();

        let custom_instr = store.get_skill_custom_instructions(&def.id)?.unwrap_or_default();

        statuses.push(SkillStatus {
            id: def.id.clone(),
            name: def.name.clone(),
            description: def.description.clone(),
            icon: def.icon.clone(),
            category: def.category.clone(),
            enabled,
            required_credentials: def.required_credentials.clone(),
            configured_credentials: configured_keys,
            missing_credentials: missing_creds,
            required_binaries: def.required_binaries.clone(),
            missing_binaries: missing_bins,
            required_env_vars: def.required_env_vars.clone(),
            missing_env_vars: missing_envs,
            install_hint: def.install_hint.clone(),
            has_instructions: !def.agent_instructions.is_empty() || !custom_instr.is_empty(),
            is_ready,
            tool_names: def.tool_names.clone(),
            default_instructions: def.agent_instructions.clone(),
            custom_instructions: custom_instr,
        });
    }

    Ok(statuses)
}

/// Get credential values for a skill (decrypted). Used by tool executor at runtime.
pub fn get_skill_credentials(store: &SessionStore, skill_id: &str) -> Result<std::collections::HashMap<String, String>, String> {
    let vault_key = get_vault_key()?;
    let keys = store.list_skill_credential_keys(skill_id)?;
    let mut creds = std::collections::HashMap::new();

    for key in keys {
        if let Some(encrypted) = store.get_skill_credential(skill_id, &key)? {
            match decrypt_credential(&encrypted, &vault_key) {
                Ok(value) => { creds.insert(key, value); }
                Err(e) => {
                    warn!("[vault] Failed to decrypt {}:{}: {}", skill_id, key, e);
                }
            }
        }
    }

    Ok(creds)
}
