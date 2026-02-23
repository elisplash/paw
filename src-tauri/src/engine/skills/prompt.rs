// Pawz Agent Engine — Skill Prompt Injection
// Assembles skill instructions to inject into agent system prompts.
// Includes built-in skills, TOML manifest skills, and community skills.

use crate::engine::sessions::SessionStore;
use super::builtins::builtin_skills;
use super::toml::scan_toml_skills;
use super::types::CredentialField;
use super::status::get_skill_credentials;
use super::community::get_community_skill_instructions;
use crate::atoms::error::EngineResult;

/// Collect agent instructions from all enabled skills.
/// Returns a combined string to be injected into the system prompt.
/// - Prefers custom instructions over defaults (if user edited them).
/// - For skills with credentials, injects actual decrypted values into placeholders.
/// - `agent_id` filters community skills to only those assigned to this agent.
pub fn get_enabled_skill_instructions(store: &SessionStore, agent_id: &str) -> EngineResult<String> {
    let definitions = builtin_skills();
    let mut sections: Vec<String> = Vec::new();

    // ── Built-in skills ────────────────────────────────────────────────
    for def in &definitions {
        if !store.is_skill_enabled(&def.id)? { continue; }

        // Use custom instructions if set, otherwise fall back to defaults
        let base_instructions = store.get_skill_custom_instructions(&def.id)?
            .unwrap_or_else(|| def.agent_instructions.clone());

        if base_instructions.is_empty() { continue; }

        // For skills with credentials, inject actual values into the instructions
        // UNLESS the skill has built-in tool_executor auth (credentials stay server-side)
        let hidden_credential_skills = ["coinbase", "dex"];
        let instructions = if !def.required_credentials.is_empty() && !hidden_credential_skills.contains(&def.id.as_str()) {
            inject_credentials_into_instructions(store, &def.id, &def.required_credentials, &base_instructions)
        } else {
            base_instructions
        };

        sections.push(format!(
            "## {} Skill ({})\n{}",
            def.name, def.id, instructions
        ));
    }

    // ── TOML manifest skills from ~/.paw/skills/ ───────────────────────
    let builtin_ids: std::collections::HashSet<&str> = definitions.iter().map(|d| d.id.as_str()).collect();
    let toml_skills = scan_toml_skills();

    for entry in &toml_skills {
        let def = &entry.definition;
        // Skip collisions with built-ins
        if builtin_ids.contains(def.id.as_str()) { continue; }
        if !store.is_skill_enabled(&def.id).unwrap_or(false) { continue; }

        let base_instructions = store.get_skill_custom_instructions(&def.id)
            .ok()
            .flatten()
            .unwrap_or_else(|| def.agent_instructions.clone());

        if base_instructions.is_empty() { continue; }

        // TOML skills always get credential injection (no hidden-credential exceptions)
        let instructions = if !def.required_credentials.is_empty() {
            inject_credentials_into_instructions(store, &def.id, &def.required_credentials, &base_instructions)
        } else {
            base_instructions
        };

        sections.push(format!(
            "## {} Skill ({})\n{}",
            def.name, def.id, instructions
        ));
    }

    // Also include enabled community skills scoped to this agent
    let community_instructions = get_community_skill_instructions(store, agent_id).unwrap_or_default();

    let mut result = String::new();

    if !sections.is_empty() {
        result.push_str(&format!(
            "\n\n# Enabled Skills\nYou have the following skills available. Use exec, fetch, read_file, write_file, and other built-in tools to leverage them.\n\n{}\n",
            sections.join("\n\n")
        ));
    }

    if !community_instructions.is_empty() {
        result.push_str(&community_instructions);
    }

    // Guard: cap total skill instructions to ~3000 tokens (~12K chars).
    // Beyond this, skills eat too much context window and degrade conversation
    // quality — the agent loses track of recent messages and gets stuck in loops.
    // At 12K chars the system prompt stays under ~8K tokens, leaving room for
    // ~8K tokens of actual conversation history (enough for ~10 exchanges).
    const MAX_SKILL_CHARS: usize = 12_000;
    if result.len() > MAX_SKILL_CHARS {
        log::warn!(
            "[skills] Skill instructions too large ({} chars, ~{} tokens). Truncating to {} chars. \
            Consider reducing the number of enabled skills for this agent.",
            result.len(), result.len() / 4, MAX_SKILL_CHARS
        );
        // Truncate at a line boundary to avoid breaking mid-instruction
        let truncated = &result[..MAX_SKILL_CHARS];
        let last_newline = truncated.rfind('\n').unwrap_or(MAX_SKILL_CHARS);
        result = result[..last_newline].to_string();
        result.push_str("\n\n⚠️ Some skill instructions were truncated because too many skills are enabled. Consider disabling unused skills.");
    }

    Ok(result)
}

/// Inject decrypted credential values into instruction text.
/// Adds a "Credentials available:" block at the end of the instructions
/// so the agent knows the actual API keys/tokens to use.
fn inject_credentials_into_instructions(
    store: &SessionStore,
    skill_id: &str,
    required_credentials: &[CredentialField],
    instructions: &str,
) -> String {
    match get_skill_credentials(store, skill_id) {
        Ok(creds) if !creds.is_empty() => {
            let cred_lines: Vec<String> = required_credentials.iter()
                .filter_map(|field| {
                    creds.get(&field.key).map(|val| {
                        format!("- {} = {}", field.key, val)
                    })
                })
                .collect();

            if cred_lines.is_empty() {
                return instructions.to_string();
            }

            format!(
                "{}\n\nCredentials (use these values directly — do NOT ask the user for them):\n{}",
                instructions,
                cred_lines.join("\n")
            )
        }
        _ => instructions.to_string(),
    }
}
