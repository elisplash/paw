// Pawz Agent Engine — Skill Prompt Injection
// Assembles skill instructions to inject into agent system prompts.
// Includes built-in skills, TOML manifest skills, and community skills.

use super::builtins::builtin_skills;
use super::community::get_community_skill_instructions;
use super::status::get_skill_credentials;
use super::toml::scan_toml_skills;
use super::types::CredentialField;
use crate::atoms::error::EngineResult;
use crate::engine::sessions::SessionStore;
use crate::engine::util::safe_truncate;

/// Collect agent instructions from all enabled skills.
/// Returns a combined string to be injected into the system prompt.
/// - Prefers custom instructions over defaults (if user edited them).
/// - For skills with credentials, injects actual decrypted values into placeholders.
/// - `agent_id` filters community skills to only those assigned to this agent.
pub fn get_enabled_skill_instructions(
    store: &SessionStore,
    agent_id: &str,
) -> EngineResult<String> {
    let definitions = builtin_skills();
    let mut sections: Vec<String> = Vec::new();

    // ── Built-in skills ────────────────────────────────────────────────
    for def in &definitions {
        // Use explicit user choice if set, otherwise fall back to definition default
        let enabled = store
            .get_skill_enabled_state(&def.id)?
            .unwrap_or(def.default_enabled);
        if !enabled {
            continue;
        }

        // Use custom instructions if set, otherwise fall back to defaults
        let base_instructions = store
            .get_skill_custom_instructions(&def.id)?
            .unwrap_or_else(|| def.agent_instructions.clone());

        if base_instructions.is_empty() {
            continue;
        }

        // For skills with credentials, inject actual values into the instructions
        // UNLESS the skill has built-in tool_executor auth (credentials stay server-side)
        let hidden_credential_skills = ["coinbase", "dex"];
        let instructions = if !def.required_credentials.is_empty()
            && !hidden_credential_skills.contains(&def.id.as_str())
        {
            inject_credentials_into_instructions(
                store,
                &def.id,
                &def.required_credentials,
                &base_instructions,
            )
        } else {
            base_instructions
        };

        sections.push(format!(
            "## {} Skill ({})\n{}",
            def.name, def.id, instructions
        ));
    }

    // ── TOML manifest skills from ~/.paw/skills/ ───────────────────────
    let builtin_ids: std::collections::HashSet<&str> =
        definitions.iter().map(|d| d.id.as_str()).collect();
    let toml_skills = scan_toml_skills();

    for entry in &toml_skills {
        let def = &entry.definition;
        // Skip collisions with built-ins
        if builtin_ids.contains(def.id.as_str()) {
            continue;
        }
        if !store.is_skill_enabled(&def.id).unwrap_or(false) {
            continue;
        }

        let base_instructions = store
            .get_skill_custom_instructions(&def.id)
            .ok()
            .flatten()
            .unwrap_or_else(|| def.agent_instructions.clone());

        if base_instructions.is_empty() {
            continue;
        }

        // TOML skills always get credential injection (no hidden-credential exceptions)
        let instructions = if !def.required_credentials.is_empty() {
            inject_credentials_into_instructions(
                store,
                &def.id,
                &def.required_credentials,
                &base_instructions,
            )
        } else {
            base_instructions
        };

        sections.push(format!(
            "## {} Skill ({})\n{}",
            def.name, def.id, instructions
        ));
    }

    // Also include enabled community skills scoped to this agent.
    // Community skills are treated as additional sections so they participate
    // in the same compression/budget logic as built-in + TOML skills.
    let community_instructions =
        get_community_skill_instructions(store, agent_id).unwrap_or_default();
    if !community_instructions.is_empty() {
        // Parse community instructions into individual sections so they can
        // be individually compressed just like built-in skills.
        for section in parse_community_sections(&community_instructions) {
            sections.push(section);
        }
    }

    // ── Dynamic per-service integration skills ─────────────────────────
    // Services like Linear, Stripe, Jira get their own skill vault (not the
    // generic rest_api).  Generate instructions so the AI knows what services
    // are connected and how to call them via rest_api_call(service: "...").
    let integration_section = build_integration_awareness(store);
    if !integration_section.is_empty() {
        sections.push(integration_section);
    }

    let mut result = String::new();

    if !sections.is_empty() {
        result.push_str(&format!(
            "\n\n# Enabled Skills\nYou have the following skills available. Use exec, fetch, read_file, write_file, and other built-in tools to leverage them.\n\n{}\n",
            sections.join("\n\n")
        ));
    }

    // Guard: cap total skill instructions.
    // When instructions are too large, intelligently compress them instead of
    // blind truncation that chops off entire skills silently.
    //
    // Strategy:
    //   1. If under budget → return as-is
    //   2. If over budget → compress each section in priority order:
    //      a) Skills matching agent's enabled skills with credentials → keep full
    //      b) Skills with credentials → keep full
    //      c) Other skills → compress to name + first ~300 chars
    //      d) If still over → keep only top sections that fit
    const MAX_SKILL_CHARS: usize = 16_000;
    if result.len() > MAX_SKILL_CHARS {
        log::warn!(
            "[skills] Skill instructions large ({} chars, ~{} tokens). Compressing to fit {} char budget.",
            result.len(), result.len() / 4, MAX_SKILL_CHARS
        );
        // Community sections are already merged into `sections`, pass empty community
        result = compress_skill_sections(&sections, "", MAX_SKILL_CHARS);
    }

    Ok(result)
}

/// Parse community instruction blob into individual sections.
/// The blob format is a header followed by `## Name (community)\n...` sections.
fn parse_community_sections(raw: &str) -> Vec<String> {
    let mut sections = Vec::new();
    let mut current = String::new();
    for line in raw.lines() {
        if line.starts_with("## ") && !current.is_empty() {
            let trimmed = current.trim().to_string();
            // Skip the header paragraph ("# Community Skills\nYou have...")
            if !trimmed.starts_with("# Community Skills") && !trimmed.is_empty() {
                sections.push(trimmed);
            }
            current = line.to_string();
            current.push('\n');
        } else if line.starts_with("# Community Skills") {
            // Flush any accumulated content and skip this header
            if !current.trim().is_empty() {
                let trimmed = current.trim().to_string();
                if !trimmed.starts_with("# Community Skills") {
                    sections.push(trimmed);
                }
            }
            current = String::new();
        } else {
            current.push_str(line);
            current.push('\n');
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() && !trimmed.starts_with("# Community Skills") {
        sections.push(trimmed);
    }
    sections
}

/// Compress skill instruction sections to fit a character budget.
/// Priority: sections with credential markers ("API Key", "Bearer", "token")
/// are kept full; others get truncated to a compact reference format.
fn compress_skill_sections(sections: &[String], community: &str, budget: usize) -> String {
    // Header overhead
    let header = "\n\n# Enabled Skills\nYou have the following skills available. Use exec, fetch, read_file, write_file, and other built-in tools to leverage them.\n\n";
    let footer = "\n\nNote: Some skill instructions were compressed to save context. Use `soul_read` on the skill's documentation or `request_tools` to discover full tool schemas.\n";
    let overhead = header.len() + footer.len();
    // If community text is passed, it must fit inside the budget too
    let community_reserve = if community.is_empty() {
        0
    } else {
        community.len().min(2000) + 2
    };
    let section_budget = budget.saturating_sub(overhead + community_reserve);

    // Classify: sections with credentials are "priority" (they have actual API keys/URLs)
    let has_credentials = |s: &str| -> bool {
        let sl = s.to_lowercase();
        sl.contains("api key")
            || sl.contains("api_key")
            || sl.contains("bearer ")
            || sl.contains("token:")
            || sl.contains("credentials available")
            || sl.contains("base url:")
            || sl.contains("endpoint:")
    };

    let mut priority_sections: Vec<(usize, &String)> = Vec::new();
    let mut normal_sections: Vec<(usize, &String)> = Vec::new();

    for (i, section) in sections.iter().enumerate() {
        if has_credentials(section) {
            priority_sections.push((i, section));
        } else {
            normal_sections.push((i, section));
        }
    }

    let mut used = 0usize;
    let mut output_parts: Vec<(usize, String)> = Vec::new();

    // Phase 1: Add priority sections in full
    for (idx, section) in &priority_sections {
        if used + section.len() < section_budget {
            output_parts.push((*idx, (*section).clone()));
            used += section.len() + 2; // +2 for \n\n joiner
        } else {
            // Even priority skill gets compressed if it would bust the budget
            let compressed = compress_one_section(section, 600);
            if used + compressed.len() < section_budget {
                output_parts.push((*idx, compressed.clone()));
                used += compressed.len() + 2;
            }
        }
    }

    // Phase 2: Add normal sections (compressed to 300 chars if needed)
    for (idx, section) in &normal_sections {
        if used + section.len() < section_budget {
            // Fits in full
            output_parts.push((*idx, (*section).clone()));
            used += section.len() + 2;
        } else if used + 350 < section_budget {
            // Compress to compact reference
            let compressed = compress_one_section(section, 300);
            output_parts.push((*idx, compressed.clone()));
            used += compressed.len() + 2;
        }
        // else: skip entirely — budget exhausted
    }

    // Sort by original index so ordering is preserved
    output_parts.sort_by_key(|(idx, _)| *idx);

    let joined: String = output_parts
        .into_iter()
        .map(|(_, s)| s)
        .collect::<Vec<_>>()
        .join("\n\n");

    let mut result = String::with_capacity(budget);
    result.push_str(header);
    result.push_str(&joined);
    result.push_str(footer);
    // Community text (if any) is truncated to stay within budget
    if !community.is_empty() {
        let remaining = budget.saturating_sub(result.len());
        if remaining > 100 {
            let truncated = safe_truncate(community, remaining);
            result.push_str(truncated);
        }
    }

    log::info!(
        "[skills] Compressed skill instructions: {} chars ({} sections kept, {} priority)",
        result.len(),
        sections.len(),
        priority_sections.len()
    );

    result
}

/// Compress a single skill section to at most `max_chars`.
/// Keeps the header line and truncates the body at a line boundary.
fn compress_one_section(section: &str, max_chars: usize) -> String {
    if section.len() <= max_chars {
        return section.to_string();
    }
    // Keep the "## Name Skill (id)" header line
    let first_line_end = section.find('\n').unwrap_or(section.len());
    let header = &section[..first_line_end];

    let body_budget = max_chars.saturating_sub(header.len() + 30); // room for truncation note
    let body = &section[first_line_end..];
    let truncated_body = if body.len() > body_budget {
        let slice = &body[..body_budget];
        let last_nl = slice.rfind('\n').unwrap_or(body_budget);
        &body[..last_nl]
    } else {
        body
    };

    format!(
        "{}{}\n[... truncated — use `request_tools` for full tool details]",
        header, truncated_body
    )
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
            let cred_lines: Vec<String> = required_credentials
                .iter()
                .filter_map(|field| {
                    creds
                        .get(&field.key)
                        .map(|val| format!("- {} = {}", field.key, val))
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

/// Per-service REST API skills that may have been provisioned via integrations.
/// These get their own vault (not the old shared `rest_api` slot).
const INTEGRATION_SERVICES: &[(&str, &str)] = &[
    ("linear", "Linear"),
    ("stripe", "Stripe"),
    ("todoist", "Todoist"),
    ("clickup", "ClickUp"),
    ("airtable", "Airtable"),
    ("sendgrid", "SendGrid"),
    ("jira", "Jira"),
    ("zendesk", "Zendesk"),
    ("hubspot", "HubSpot"),
    ("twilio", "Twilio"),
    ("shopify", "Shopify"),
    ("pagerduty", "PagerDuty"),
    ("microsoft_teams", "Microsoft Teams"),
];

/// Build a prompt section listing all connected REST API integration services.
/// This tells the AI exactly which services it can call with `rest_api_call(service: "...")`.
fn build_integration_awareness(store: &SessionStore) -> String {
    let mut connected: Vec<(&str, String, String)> = Vec::new();

    for &(skill_id, default_name) in INTEGRATION_SERVICES {
        if !store.is_skill_enabled(skill_id).unwrap_or(false) {
            continue;
        }
        // Read stored service metadata
        let creds = super::status::get_skill_credentials(store, skill_id).unwrap_or_default();
        let name = creds
            .get("SERVICE_NAME")
            .cloned()
            .unwrap_or_else(|| default_name.to_string());
        let hint = creds.get("SERVICE_HINT").cloned().unwrap_or_default();
        let base_url = creds.get("API_BASE_URL").cloned().unwrap_or_default();
        let description = if !hint.is_empty() {
            format!("{} ({})", base_url, hint)
        } else if !base_url.is_empty() {
            base_url
        } else {
            String::new()
        };
        connected.push((skill_id, name, description));
    }

    if connected.is_empty() {
        return String::new();
    }

    let mut section = String::from("## Connected REST API Services (integration_awareness)\n\n");
    section.push_str(
        "You have the following services connected. Use `rest_api_call` with the `service` \
         parameter to call any of them. Auth credentials are injected automatically.\n\n",
    );

    for (skill_id, name, desc) in &connected {
        if desc.is_empty() {
            section.push_str(&format!("- **{}** → `rest_api_call({{\"service\": \"{}\", \"path\": \"/...\", \"method\": \"GET\"}})`\n", name, skill_id));
        } else {
            section.push_str(&format!("- **{}** — {} → `rest_api_call({{\"service\": \"{}\", \"path\": \"/...\", \"method\": \"GET\"}})`\n", name, desc, skill_id));
        }
    }

    section.push_str("\nDo NOT ask the user for API keys or base URLs — they are stored securely. Just use `rest_api_call` with the `service` name.\n");

    section
}
