// Paw Agent Engine — Chat Workflow Organism
//
// Pure helper functions extracted from engine_chat_send.
// These contain the "heavy lifting": tool assembly, prompt composition,
// loop detection, and attachment preprocessing.
//
// Dependency rule (one-way):
//   engine/chat.rs → engine/types, engine/skills, engine/tools, engine/telegram
//   engine/chat.rs has NO import from commands/ — EngineState is NEVER referenced here.
//
// Called by: commands/chat.rs (the thin System layer)

use crate::engine::sessions::SessionStore;
use crate::engine::skills;
use crate::engine::tool_index;
use crate::engine::tools;
use crate::engine::types::*;
use crate::engine::util::safe_truncate;
use log::{info, warn};

// ── Tool builder ───────────────────────────────────────────────────────────────

/// Assemble the tool list for a chat turn using Tool RAG (lazy loading).
///
/// Instead of dumping all 400+ tools, sends only:
///   1. Core tools (memory, soul, files, request_tools) — always available
///   2. Previously loaded tools (from request_tools calls this turn)
///   3. MCP tools (always included — they're dynamically registered)
///
/// The agent discovers additional tools by calling `request_tools`.
///
/// # Parameters
/// - `store`         — session store (used to check which skills are enabled)
/// - `tools_enabled` — if false, returns an empty list immediately
/// - `tool_filter`   — optional list of tool names to retain (allow-list)
/// - `app_handle`    — needed to probe whether the Telegram bridge is configured
/// - `loaded_tools`  — tool names previously loaded via request_tools this turn
pub fn build_chat_tools(
    store: &SessionStore,
    tools_enabled: bool,
    tool_filter: Option<&[String]>,
    app_handle: &tauri::AppHandle,
    loaded_tools: &std::collections::HashSet<String>,
) -> Vec<ToolDefinition> {
    if !tools_enabled {
        return vec![];
    }

    // ── Build the full tool registry (same as before) ──────────────────
    let mut all_tools = ToolDefinition::builtins();

    let enabled_ids: Vec<String> = skills::builtin_skills()
        .iter()
        .filter(|s| store.is_skill_enabled(&s.id).unwrap_or(false))
        .map(|s| s.id.clone())
        .collect();
    if !enabled_ids.is_empty() {
        info!("[engine] Skills enabled: {:?}", enabled_ids);
        all_tools.extend(ToolDefinition::skill_tools(&enabled_ids));
    }

    // Auto-add telegram tools when bridge configured but skill not enabled
    if !enabled_ids.contains(&"telegram".to_string()) {
        if let Ok(tg_cfg) = crate::engine::telegram::load_telegram_config(app_handle) {
            if !tg_cfg.bot_token.is_empty() {
                info!("[engine] Auto-adding telegram tools (bridge configured)");
                all_tools.push(ToolDefinition::telegram_send());
                all_tools.push(ToolDefinition::telegram_read());
            }
        }
    }

    // Add MCP tools (always included — they're external servers)
    let mcp_tools = ToolDefinition::mcp_tools(app_handle);
    if !mcp_tools.is_empty() {
        info!("[engine] Adding {} MCP tools", mcp_tools.len());
        all_tools.extend(mcp_tools);
    }

    // ── Tool RAG: filter to core + loaded + policy-allowed tools ─────
    let is_core = |name: &str| tool_index::CORE_TOOLS.contains(&name);
    let is_loaded = |name: &str| loaded_tools.contains(name);
    let is_mcp = |name: &str| name.starts_with("mcp_");
    // If the agent policy explicitly lists skill tools, auto-include them
    // so users don't have to rely on request_tools for tools they manually enabled.
    let is_policy_allowed = |name: &str| tool_filter.is_some_and(|f| f.iter().any(|n| n == name));
    // Auto-include exec and fetch when integration skills are enabled — these
    // are needed for CLI tools (gh, git) and direct HTTP calls that skills rely on.
    let has_integration_skills = !enabled_ids.is_empty();
    let is_skill_required = |name: &str| has_integration_skills && matches!(name, "fetch" | "exec");

    let mut t: Vec<ToolDefinition> = all_tools
        .into_iter()
        .filter(|tool| {
            let name = tool.function.name.as_str();
            is_core(name)
                || is_loaded(name)
                || is_mcp(name)
                || is_policy_allowed(name)
                || is_skill_required(name)
        })
        .collect();

    // Apply per-request tool allow-list (frontend agent policy)
    if let Some(filter) = tool_filter {
        let before = t.len();
        t.retain(|tool| filter.contains(&tool.function.name));
        info!(
            "[engine] Tool policy filter applied: {} → {} tools (filter has {} entries)",
            before,
            t.len(),
            filter.len()
        );
    }

    info!(
        "[engine] Tool RAG: {} tools active ({} core + {} loaded + MCP) [request_tools available for discovery]",
        t.len(),
        t.iter().filter(|tool| is_core(&tool.function.name)).count(),
        t.iter().filter(|tool| is_loaded(&tool.function.name)).count(),
    );
    t
}

// ── Runtime context block builder ─────────────────────────────────────────────

/// Build the compact runtime context block injected into every system prompt.
///
/// Contains: model, provider, session, agent, current time, workspace path,
/// and full environment awareness (OS, arch, shell, hostname, username, version).
/// All inputs are plain strings extracted by the command layer from locked state.
pub fn build_runtime_context(
    model: &str,
    provider_name: &str,
    session_id: &str,
    agent_id: &str,
    user_timezone: &str,
) -> String {
    let now_utc = chrono::Utc::now();
    let time_str = if let Ok(tz) = user_timezone.parse::<chrono_tz::Tz>() {
        let local: chrono::DateTime<chrono_tz::Tz> = now_utc.with_timezone(&tz);
        format!(
            "{} {} ({})",
            local.format("%Y-%m-%d %H:%M"),
            local.format("%A"),
            tz.name()
        )
    } else {
        let local = chrono::Local::now();
        format!("{} {}", local.format("%Y-%m-%d %H:%M"), local.format("%A"))
    };

    let ws = tools::agent_workspace(agent_id);

    // ── Environment self-awareness ──────────────────────────────────────
    let os_name = std::env::consts::OS; // "macos", "linux", "windows"
    let os_arch = std::env::consts::ARCH; // "aarch64", "x86_64"
    let hostname = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .or_else(|_| {
            // macOS/Linux fallback: read /etc/hostname or use scutil
            std::fs::read_to_string("/etc/hostname").map(|s| s.trim().to_string())
        })
        .unwrap_or_else(|_| "localhost".into());
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".into());
    let home_dir = dirs::home_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "~".into());
    let shell = std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(target_os = "windows") {
            "powershell".into()
        } else {
            "/bin/zsh".into()
        }
    });
    let app_version = env!("CARGO_PKG_VERSION");

    format!(
        "## Runtime\n\
        Model: {} | Provider: {} | Session: {} | Agent: {}\n\
        Time: {}\n\
        Workspace: {}\n\
        \n\
        ## Environment\n\
        OS: {} ({}) | Shell: {}\n\
        Host: {} | User: {} | Home: {}\n\
        OpenPawz: v{}",
        model,
        provider_name,
        session_id,
        agent_id,
        time_str,
        ws.display(),
        os_name,
        os_arch,
        shell,
        hostname,
        username,
        home_dir,
        app_version,
    )
}

// ── Platform awareness manifest ────────────────────────────────────────────────

/// Build the platform capabilities block that gives the agent full self-awareness.
///
/// This is injected once into every system prompt so the agent knows exactly
/// what OpenPawz is, what it can do, and how to do it — without guessing.
pub fn build_platform_awareness() -> String {
    // Build dynamic skill domain listing from the tool index
    let domains: Vec<String> = crate::engine::tool_index::domain_summaries()
        .iter()
        .map(|(id, _icon, desc)| format!("- **{}** — {}", id, desc))
        .collect();

    // Template loaded from prompts/platform.md at compile time.
    // Contains a {DOMAINS} placeholder for the dynamic skill listing.
    const TEMPLATE: &str = include_str!("prompts/platform.md");
    TEMPLATE.replace("{DOMAINS}", &domains.join("\n"))
}

// ── Code-generation discipline ─────────────────────────────────────────────────

/// Coding guidelines injected into every system prompt so the agent produces
/// code that integrates cleanly with the OpenPawz codebase and the wider
/// TOML-skill / MCP ecosystem.  These are non-negotiable quality gates.
///
/// Loaded from `prompts/coding.md` at compile time.
pub fn build_coding_guidelines() -> &'static str {
    include_str!("prompts/coding.md")
}

/// Build the MCP/Foreman Protocol awareness block.
///
/// Only injected when MCP tools are actually present — no point cluttering
/// the system prompt when no MCP servers are connected.
///
/// Loaded from `prompts/foreman.md` at compile time.
pub fn build_foreman_awareness() -> &'static str {
    include_str!("prompts/foreman.md")
}

/// Build the Action DAG (execute_plan) awareness block.
///
/// Loaded from `prompts/plan.md` at compile time. Instructs the model
/// to use the execute_plan tool for parallelizable multi-step tasks.
pub fn build_plan_awareness() -> &'static str {
    include_str!("prompts/plan.md")
}

/// Build a lightweight agent roster showing known agents and their specialties.
/// Injected into the system prompt so the agent can delegate tasks to the right agent
/// without needing to call `agent_list` first.
pub fn build_agent_roster(store: &SessionStore, current_agent_id: &str) -> Option<String> {
    let agents = store.list_all_agents().ok()?;
    if agents.is_empty() {
        return None;
    }

    let mut lines: Vec<String> = Vec::new();
    for (_project_id, agent) in &agents {
        if agent.agent_id == current_agent_id {
            continue;
        } // don't list yourself
        if agent.agent_id == "default" {
            continue;
        } // skip the default agent entry

        let model_info = agent.model.as_deref().unwrap_or("default");
        lines.push(format!(
            "- **{}** (id: `{}`) — {} / {} (model: {})",
            agent.agent_id, agent.agent_id, agent.role, agent.specialty, model_info
        ));
    }

    if lines.is_empty() {
        return None;
    }

    Some(format!(
        "## Your Agent Team\n\
        You have {} other agent(s) available. When the user mentions an agent by name \
        or asks you to delegate/assign work, use `request_tools` to load `agent_send_message`, \
        then send the task to the appropriate agent.\n\n\
        {}\n\n\
        **Delegation rules:**\n\
        - If the user says \"get [agent] to do X\" or \"ask [agent] about X\", delegate immediately — do NOT do X yourself.\n\
        - Match agent names loosely (e.g., \"Crypto Cat\" matches agent id containing \"crypto-cat\").\n\
        - After delegating, tell the user you've sent the task to that agent.",
        lines.len(),
        lines.join("\n")
    ))
}

// ── System prompt composer ─────────────────────────────────────────────────────

/// Compose the full multi-section system prompt.
///
/// Sections (all optional, joined with `\n\n---\n\n`):
///   1. Base system prompt (from request or engine config default)
///   2. Platform awareness manifest (what OpenPawz is + all capabilities)
///   3. Runtime context block (model / session / time / workspace)
///   4. Soul-file guidance + core files (IDENTITY.md, SOUL.md, USER.md)
///   5. Today's memory notes
///   6. Skill instructions for enabled skills
///
/// Returns `None` if every section is empty (practically never).
pub fn compose_chat_system_prompt(
    base_system_prompt: Option<&str>,
    runtime_context: String,
    core_context: Option<&str>,
    todays_memories: Option<&str>,
    skill_instructions: &str,
) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();

    if let Some(sp) = base_system_prompt {
        parts.push(sp.to_string());
    }
    parts.push(build_platform_awareness());
    // Foreman Protocol — always injected. This is the ONLY way external
    // services work. The model must always know about MCP delegation.
    parts.push(build_foreman_awareness().to_string());
    // Action DAG planning awareness — always injected so the model knows
    // it can use execute_plan for multi-step parallel tasks.
    parts.push(build_plan_awareness().to_string());
    // Coding guidelines are heavy (~5K chars). Only inject when coding/dev skills
    // are actually enabled, to keep the system prompt lean for everyday tasks.
    if skill_instructions.contains("development") || skill_instructions.contains("## Code") {
        parts.push(build_coding_guidelines().to_string());
    }
    parts.push(runtime_context);

    let soul_hint = if core_context.is_some() {
        "Your core soul files (IDENTITY.md, SOUL.md, USER.md) are loaded below. \
        Use `soul_write` to update them. Use `soul_read` / `soul_list` to access other files \
        (AGENTS.md, TOOLS.md, etc.) on demand."
    } else {
        "You have no soul files yet. Use `soul_write` to create IDENTITY.md (who you are), \
        SOUL.md (your personality), and USER.md (what you know about the user). \
        These persist across conversations and define your identity."
    };

    parts.push(format!(
        "## Soul Files\n{}\n\n\
        ## Memory\n\
        Relevant memories from past conversations are automatically recalled and shown below \
        (if any match this context). Use `memory_search` for deeper or more specific recall. \
        Use `memory_store` to save important information for future sessions.",
        soul_hint,
    ));

    if let Some(cc) = core_context {
        parts.push(cc.to_string());
    }
    if let Some(tm) = todays_memories {
        parts.push(tm.to_string());
    }
    if !skill_instructions.is_empty() {
        parts.push(skill_instructions.to_string());
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n---\n\n"))
    }
}

/// Build a system prompt within a character budget by dropping lowest-priority
/// sections first.
///
/// Priority (highest → lowest — dropped last → first):
///   1. Core: platform awareness + foreman protocol + runtime context
///   2. Soul files (IDENTITY.md, SOUL.md, USER.md)
///   3. Base system prompt (user-configured personality/instructions)
///   4. Agent roster (needed for multi-agent delegation)
///   5. Today's memory notes
///   6. Skill instructions
///   7. Auto-recalled memories (most expendable — `memory_search` is available)
///
/// Each section is tried in reverse priority order. If including it would
/// exceed the budget, it's dropped with a hint telling the model how to
/// access the information on demand.
#[allow(clippy::too_many_arguments)]
pub fn compose_chat_system_prompt_budgeted(
    base_system_prompt: Option<&str>,
    runtime_context: String,
    core_context: Option<&str>,
    todays_memories: Option<&str>,
    skill_instructions: &str,
    agent_roster: Option<&str>,
    recall_block: Option<&str>,
    max_chars: usize,
) -> Option<String> {
    let sep = "\n\n---\n\n";

    // ── Priority 1 (always included): core platform identity ───────────
    let mut parts: Vec<String> = vec![
        build_platform_awareness(),
        build_foreman_awareness().to_string(),
        build_plan_awareness().to_string(),
        runtime_context,
    ];

    // ── Priority 2: soul files ─────────────────────────────────────────
    // Always include the soul hint, soul content is already capped at 3K each
    let soul_hint = if core_context.is_some() {
        "Your core soul files (IDENTITY.md, SOUL.md, USER.md) are loaded below. \
        Use `soul_write` to update them. Use `soul_read` / `soul_list` to access other files."
    } else {
        "You have no soul files yet. Use `soul_write` to create IDENTITY.md, SOUL.md, USER.md."
    };
    parts.push(format!(
        "## Soul Files\n{}\n\n## Memory\n\
        Use `memory_search` for recall. Use `memory_store` to save important info.",
        soul_hint
    ));
    if let Some(cc) = core_context {
        parts.push(cc.to_string());
    }

    // ── Priority 3: base system prompt ─────────────────────────────────
    if let Some(sp) = base_system_prompt {
        parts.push(sp.to_string());
    }

    // Add coding guidelines only when dev skills actually enabled
    if skill_instructions.contains("development") || skill_instructions.contains("## Code") {
        parts.push(build_coding_guidelines().to_string());
    }

    // Check running size — everything above is "essential"
    let essential = parts.join(sep);
    let mut result = essential;

    // ── Priority 4-7: optional sections, added if budget allows ────────
    // Try adding each in priority order; skip any that would bust the budget.
    struct OptionalSection<'a> {
        content: Option<&'a str>,
        label: &'a str,
        fallback_hint: &'a str,
    }

    let optional_sections = [
        OptionalSection {
            content: agent_roster,
            label: "agent_roster",
            fallback_hint:
                "[Agent roster omitted to save context. Use `agent_list` to see available agents.]",
        },
        OptionalSection {
            content: todays_memories,
            label: "todays_memories",
            fallback_hint:
                "[Today's notes omitted to save context. Use `memory_search` for recall.]",
        },
        OptionalSection {
            content: if skill_instructions.is_empty() {
                None
            } else {
                Some(skill_instructions)
            },
            label: "skill_instructions",
            fallback_hint:
                "[Skill instructions omitted. Use `request_tools` to discover available tools.]",
        },
        OptionalSection {
            content: recall_block,
            label: "recalled_memories",
            fallback_hint:
                "[Auto-recalled memories omitted. Use `memory_search` for relevant context.]",
        },
    ];

    let mut dropped: Vec<&str> = Vec::new();

    for section in &optional_sections {
        if let Some(content) = section.content {
            let candidate = format!("{}{}{}", result, sep, content);
            if candidate.len() <= max_chars {
                result = candidate;
            } else {
                // Won't fit — add the fallback hint instead (much smaller)
                let hint_candidate = format!("{}{}{}", result, sep, section.fallback_hint);
                if hint_candidate.len() <= max_chars {
                    result = hint_candidate;
                }
                dropped.push(section.label);
            }
        }
    }

    if !dropped.is_empty() {
        info!(
            "[engine] System prompt budget: dropped {:?} to fit {} char limit (final: {} chars, ~{} tokens)",
            dropped, max_chars, result.len(), result.len() / 4
        );
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

// ── Response loop detector ─────────────────────────────────────────────────────

/// Detect stuck response loops and inject a system nudge to break the cycle.
///
/// Checks:
/// 1. **Repetition**: Jaccard word-similarity > 40% between last two assistant
///    messages — the model is repeating itself with minor rewording.
/// 2. **Question loop**: Both last assistant messages end in `?` — the model
///    keeps asking clarifying questions instead of acting.
/// 3. **Topic fixation**: The user's message has near-zero keyword overlap
///    with the model's response AND the model's last two responses are similar
///    to each other — meaning the model is stuck on an old topic and ignoring
///    the user. This combined check avoids false positives from natural topic
///    shifts (where the model just hasn't responded yet).
/// 4. **Topic-ignoring + repetition**: Low keyword overlap combined with
///    moderate inter-response similarity — subtler fixation.
///
/// **Escalation:** If a previous redirect was already injected and the model
/// STILL isn't addressing the user, injects a progressively stronger redirect.
/// Crucially, **no messages are ever pruned** — the user may return to any
/// earlier topic. The escalation is purely in redirect strength.
///
/// In all cases, a system-role redirect is injected telling the model to
/// stop repeating itself and respond to the user's actual request.
pub fn detect_response_loop(messages: &mut Vec<Message>) {
    let last_user_text = messages
        .iter()
        .rev()
        .find(|m| m.role == Role::User)
        .map(|m| m.content.as_text_ref().to_lowercase());

    let assistant_msgs: Vec<&str> = messages
        .iter()
        .rev()
        .filter(|m| m.role == Role::Assistant)
        .take(3)
        .map(|m| m.content.as_text_ref())
        .collect();

    if assistant_msgs.len() < 2 {
        return;
    }

    let a = assistant_msgs[0].to_lowercase();
    let b = assistant_msgs[1].to_lowercase();

    let words_a: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let words_b: std::collections::HashSet<&str> = b.split_whitespace().collect();
    let intersection = words_a.intersection(&words_b).count();
    let union = words_a.union(&words_b).count();
    let similarity = if union > 0 {
        intersection as f64 / union as f64
    } else {
        0.0
    };

    // Count how many redirects we've already injected — determines escalation level
    let prior_redirect_count = messages
        .iter()
        .filter(|m| {
            m.role == Role::System
                && (m.content.as_text_ref().contains("TOPIC CHANGE")
                    || m.content.as_text_ref().contains("stuck repeating")
                    || m.content.as_text_ref().contains("response loop"))
        })
        .count();

    // ── Check 1: assistant repeating itself (> 40% overlap) ────────────
    if similarity > 0.40 {
        warn!(
            "[engine] Response loop detected (similarity={:.0}%) — injecting redirect",
            similarity * 100.0
        );
        inject_loop_break(messages, prior_redirect_count);
        return;
    }

    // ── Check 2: question loop — both responses are questions ──────────
    // When the model asks "Should I do X?" twice in a row, it's stuck
    // asking for confirmation instead of acting.
    let a_is_question = a.trim_end().ends_with('?');
    let b_is_question = b.trim_end().ends_with('?');
    if a_is_question && b_is_question {
        warn!("[engine] Question loop detected — assistant asked two consecutive questions");
        inject_loop_break(messages, prior_redirect_count);
        return;
    }

    // ── Check 3: topic fixation — model stuck on old topic ─────────────
    // Compare what the user asked about vs what the model responded about.
    // CRITICAL: We only fire when BOTH conditions are true:
    //   1. Near-zero keyword overlap (user and model talking about different things)
    //   2. Model's response is similar to its PREVIOUS response (it's fixated)
    // This avoids false positives from natural topic shifts where the model
    // simply hasn't responded to the new topic yet.
    if let Some(ref user_text) = last_user_text {
        let stop_words = build_stop_words();

        let user_keywords: std::collections::HashSet<&str> = user_text
            .split_whitespace()
            .filter(|w| w.len() > 2 && !stop_words.contains(w))
            .collect();
        let asst_keywords: std::collections::HashSet<&str> = a
            .split_whitespace()
            .filter(|w| w.len() > 2 && !stop_words.contains(w))
            .collect();

        // Short affirmatives: "yes", "both", "do it", "go ahead"
        // If the model asks another question after a directive, that's a loop.
        let short_directive = user_text.split_whitespace().count() <= 4;
        if short_directive && a_is_question && similarity > 0.20 {
            warn!(
                "[engine] Short-directive loop: user said '{}' but model asked another question \
                (similarity={:.0}%) — injecting redirect",
                user_text,
                similarity * 100.0
            );
            inject_loop_break(messages, prior_redirect_count);
            return;
        }

        if user_keywords.len() >= 2 && !asst_keywords.is_empty() {
            let topic_overlap = user_keywords.intersection(&asst_keywords).count();
            let topic_ratio = topic_overlap as f64 / user_keywords.len() as f64;

            // ── Check 3a: zero overlap + model repeating itself = fixation ──
            // The model ignored the user AND is producing similar content to
            // its previous response. This is the "stuck on SerpAPI" pattern.
            // Both conditions must be true to avoid false positives from
            // natural topic shifts (Jira → president → back to Jira).
            if topic_ratio < 0.10 && similarity > 0.15 {
                warn!(
                    "[engine] Topic fixation: user keywords overlap={:.0}%, \
                    inter-response similarity={:.0}% — model is stuck on old topic",
                    topic_ratio * 100.0,
                    similarity * 100.0
                );
                inject_topic_redirect(messages, prior_redirect_count);
                return;
            }

            // ── Check 3b: low overlap + moderate repetition ───────────────
            // Weaker fixation — model is drifting but not completely stuck.
            if topic_ratio < 0.20 && similarity > 0.25 {
                warn!(
                    "[engine] Topic-ignoring loop: user keywords overlap={:.0}%, \
                    inter-response similarity={:.0}% — injecting redirect",
                    topic_ratio * 100.0,
                    similarity * 100.0
                );
                inject_loop_break(messages, prior_redirect_count);
                return;
            }

            // ── Check 3c: unidirectional topic ignorance (§59.3) ──────────
            // The model's response contains NONE of the user's key entities
            // AND the model's response talks about a clearly different domain.
            // This catches the case where the model gives UNIQUE responses
            // (low inter-response similarity) but to the WRONG topic.
            //
            // Example: User says "set up my X developer account", model
            // responds with a fresh (non-repeated) response about building
            // a hit list of Twitter accounts to follow — unique content, wrong topic.
            //
            // Guard: Only fire when there's been at least one prior redirect
            // (either from check 3a/3b or from detect_user_override). This avoids
            // false positives on the FIRST response to a new topic, where the model
            // simply hasn't had a chance to respond yet.
            if prior_redirect_count >= 1 && topic_ratio == 0.0 && user_keywords.len() >= 3 {
                // Extract key entities from the model's last response and the
                // user's message. If the model's entities are completely disjoint
                // from the user's, it's talking about a different topic.
                let asst_words: Vec<&str> = a
                    .split_whitespace()
                    .filter(|w| w.len() > 3 && !stop_words.contains(*w))
                    .collect();
                let user_in_asst = user_keywords
                    .iter()
                    .filter(|uk| {
                        asst_words
                            .iter()
                            .any(|aw| aw.contains(*uk) || uk.contains(aw))
                    })
                    .count();

                if user_in_asst == 0 && asst_words.len() >= 5 {
                    warn!(
                        "[engine] Unidirectional topic ignorance: model response has 0 overlap \
                        with user's {} keywords despite {} prior redirects",
                        user_keywords.len(),
                        prior_redirect_count
                    );
                    inject_topic_redirect(messages, prior_redirect_count);
                }
            }
        }
    }
}

/// Build the common stop-word set used by topic analysis.
fn build_stop_words() -> std::collections::HashSet<&'static str> {
    [
        "the", "a", "an", "is", "are", "was", "were", "be", "been", "being", "have", "has", "had",
        "do", "does", "did", "will", "would", "could", "should", "may", "might", "can", "shall",
        "to", "of", "in", "for", "on", "with", "at", "by", "from", "as", "into", "about", "like",
        "through", "after", "over", "between", "out", "against", "during", "i", "you", "he", "she",
        "it", "we", "they", "me", "him", "her", "us", "them", "my", "your", "his", "its", "our",
        "their", "this", "that", "these", "those", "and", "but", "or", "nor", "not", "so", "if",
        "then", "than", "too", "very", "just", "don't", "im", "i'd", "i'm", "i'll", "i've",
        "you're", "it's", "what", "how", "all", "each", "which", "who", "when", "where", "why",
    ]
    .into_iter()
    .collect()
}

/// Inject a system message that breaks the agent out of a response loop.
///
/// `prior_redirect_count` controls escalation intensity — the more prior
/// redirects the model has ignored, the stronger the language becomes.
/// Messages are NEVER pruned; the model keeps full conversation history
/// so it can naturally return to earlier topics when the user circles back.
fn inject_loop_break(messages: &mut Vec<Message>, prior_redirect_count: usize) {
    let last_user_text = messages
        .iter()
        .rev()
        .find(|m| m.role == Role::User)
        .map(|m| m.content.as_text_ref().to_string())
        .unwrap_or_default();

    let redirect = if last_user_text.is_empty() {
        "IMPORTANT: You are stuck in a response loop — repeating the same content. \
        Read the user's MOST RECENT message carefully and respond ONLY to what they \
        actually asked. Do NOT ask another question. Take action with your tools NOW."
            .to_string()
    } else if prior_redirect_count >= 2 {
        format!(
            "URGENT — REPEATED LOOP (redirected {} times already): You keep repeating \
            yourself and IGNORING the user. This is the user's actual request:\n\n\
            >>> {} <<<\n\n\
            STOP everything else. Respond ONLY to the text above. If the user said \
            'yes', 'do it', 'go ahead', etc. — proceed immediately. Call tools NOW.",
            prior_redirect_count,
            safe_truncate(&last_user_text, 300)
        )
    } else {
        format!(
            "IMPORTANT: You are stuck repeating yourself instead of acting. STOP. \
            The user's actual request is: \"{}\"\n\n\
            Take action NOW. Use your tools to do what the user asked. \
            If they said 'yes', 'both', 'do it', 'go ahead', or similar — proceed with ALL \
            the options you mentioned. Do NOT ask another question. Call the relevant tools immediately.",
            safe_truncate(&last_user_text, 300)
        )
    };

    messages.push(Message {
        role: Role::System,
        content: MessageContent::Text(redirect),
        tool_calls: None,
        tool_call_id: None,
        name: None,
    });
}

/// Inject a system message for topic-fixation situations.
///
/// The model is producing responses that address an *old* topic while the
/// user has moved on. Unlike `inject_loop_break`, the model may not be
/// literally repeating itself — it's giving valid responses to the wrong
/// question.
///
/// **Key design decision:** We tell the model to *prioritize* the new topic
/// but do NOT say "the old topic is DEAD." The user may naturally circle
/// back (e.g. Jira → "who is president?" → back to Jira), so all context
/// must be preserved.
///
/// Escalation is controlled by `prior_redirect_count` — the more prior
/// redirects the model ignored, the more forceful the language.
fn inject_topic_redirect(messages: &mut Vec<Message>, prior_redirect_count: usize) {
    let last_user_text = messages
        .iter()
        .rev()
        .find(|m| m.role == Role::User)
        .map(|m| m.content.as_text_ref().to_string())
        .unwrap_or_default();

    let redirect = if prior_redirect_count >= 2 {
        format!(
            "URGENT — TOPIC CHANGE (redirected {} times): You keep responding to an \
            OLD topic the user has ALREADY moved past. The user's CURRENT message is:\n\n\
            >>> {} <<<\n\n\
            Respond ONLY to this message RIGHT NOW. Do NOT continue the previous thread. \
            Every word of your response must address what the user just said.",
            prior_redirect_count,
            safe_truncate(&last_user_text, 300)
        )
    } else {
        format!(
            "TOPIC CHANGE: The user has moved to a new question or topic. \
            Address their current message FIRST:\n\n\
            \"{}\"\n\n\
            Respond directly to this. Do not continue the previous topic unless \
            the user explicitly asks to return to it.",
            safe_truncate(&last_user_text, 300)
        )
    };

    messages.push(Message {
        role: Role::System,
        content: MessageContent::Text(redirect),
        tool_calls: None,
        tool_call_id: None,
        name: None,
    });
}

// ── Attachment processor ───────────────────────────────────────────────────────

// ── User-override detector (§59.2) ────────────────────────────────────────────

/// Detect explicit user commands to stop the current behavior and refocus.
///
/// Catches phrases like:
///   - "stop", "STOP", "PAWZ stop"
///   - "focus on my question", "focus on what I said"
///   - "I asked you to...", "I'm asking about..."
///   - "that's not what I asked", "not what I said"
///   - "new topic", "change the subject", "switch to..."
///   - "ignore that", "forget about that", "drop it"
///   - "listen to me", "pay attention", "I am in control"
///
/// When detected, injects a HARD system redirect that is stronger than the
/// statistical loop-detection messages. Returns `true` if an override was
/// detected (callers should clear working memory momentum).
///
/// **Design rule:** We only fire on EXPLICIT commands — not inferred intent.
/// This avoids false positives on normal conversation. The user must clearly
/// express that the agent is misbehaving.
pub fn detect_user_override(messages: &mut Vec<Message>) -> bool {
    let last_user_text = match messages
        .iter()
        .rev()
        .find(|m| m.role == Role::User)
        .map(|m| m.content.as_text_ref().to_string())
    {
        Some(t) => t,
        None => return false,
    };

    let lower = last_user_text.to_lowercase();

    // ── Phase 1: explicit stop / control assertions ────────────────────
    let has_stop_command = is_user_override_phrase(&lower);

    if !has_stop_command {
        return false;
    }

    // Count prior override redirects for escalation
    let prior_override_count = messages
        .iter()
        .filter(|m| {
            m.role == Role::System
                && (m.content.as_text_ref().contains("USER OVERRIDE")
                    || m.content.as_text_ref().contains("USER COMMAND"))
        })
        .count();

    let redirect =
        if prior_override_count >= 2 {
            format!(
            "** USER COMMAND (override #{}) — The user has EXPLICITLY told you {times} to stop \
            your current behavior. You MUST comply NOW.\n\n\
            The user said: >>> {} <<<\n\n\
            RULES:\n\
            1. STOP all current task execution immediately\n\
            2. Do NOT continue any previous topic or task\n\
            3. Read ONLY the user's message above\n\
            4. Respond ONLY to what they are asking RIGHT NOW\n\
            5. If they are giving you a new task, start it from scratch\n\
            6. Acknowledge that you heard them before proceeding",
            prior_override_count + 1,
            safe_truncate(&last_user_text, 500),
            times = if prior_override_count == 2 { "multiple times" } else { "repeatedly" },
        )
        } else if prior_override_count == 1 {
            format!(
            "** USER OVERRIDE (2nd time): The user is EXPLICITLY redirecting you. They said:\n\n\
            >>> {} <<<\n\n\
            You ignored them once already. STOP your current task. Address ONLY what the user \
            just said. Do NOT continue the previous topic. Acknowledge the user's control.",
            safe_truncate(&last_user_text, 500)
        )
        } else {
            format!(
            "** USER OVERRIDE: The user is explicitly redirecting the conversation. They said:\n\n\
            >>> {} <<<\n\n\
            STOP your current task and respond ONLY to this message. The user is in control — \
            follow their direction immediately. Do NOT continue the previous topic unless they \
            explicitly ask you to return to it.",
            safe_truncate(&last_user_text, 500)
        )
        };

    warn!(
        "[engine] User override detected (escalation level {}): \"{}\"",
        prior_override_count,
        safe_truncate(&last_user_text, 80)
    );

    messages.push(Message {
        role: Role::System,
        content: MessageContent::Text(redirect),
        tool_calls: None,
        tool_call_id: None,
        name: None,
    });

    true
}

/// Check if a lowercased user message contains an explicit override command.
///
/// Split out as a standalone function for unit testing.
pub fn is_user_override_phrase(lower: &str) -> bool {
    // ── Exact stop commands ────────────────────────────────────────────
    let stop_patterns = [
        "stop",
        "pawz stop",
        "paw stop",
        "pawz, stop",
        "stop it",
        "stop that",
        "cut it out",
        "enough",
        "quit it",
    ];
    // Check if message starts with or IS a stop command
    for pat in &stop_patterns {
        if lower.trim() == *pat
            || lower.starts_with(&format!("{} ", pat))
            || lower.starts_with(&format!("{}!", pat))
        {
            return true;
        }
    }

    // ── Focus / attention commands ─────────────────────────────────────
    let focus_phrases = [
        "focus on my",
        "focus on what i",
        "answer my question",
        "answer my actual",
        "answer what i asked",
        "respond to what i",
        "respond to my",
        "listen to me",
        "pay attention",
        "i am in control",
        "i'm in control",
        "im in control",
    ];
    for phrase in &focus_phrases {
        if lower.contains(phrase) {
            return true;
        }
    }

    // ── Correction / redirection commands ──────────────────────────────
    let correction_phrases = [
        "not what i asked",
        "that's not what i",
        "thats not what i",
        "didn't ask you to",
        "i didn't ask",
        "i did not ask",
        "i asked you to",
        "i'm asking about",
        "i was asking about",
        "i asked about",
        "i'm talking about",
        "i want to talk about",
        "i wanted to talk about",
    ];
    for phrase in &correction_phrases {
        if lower.contains(phrase) {
            return true;
        }
    }

    // ── Topic switch commands ──────────────────────────────────────────
    let switch_phrases = [
        "new topic",
        "change the subject",
        "change the topic",
        "switch to",
        "move on to",
        "let's move on",
        "lets move on",
        "forget about that",
        "forget that",
        "drop it",
        "drop that",
        "never mind that",
        "nevermind that",
        "ignore that",
        "skip that",
    ];
    for phrase in &switch_phrases {
        if lower.contains(phrase) {
            return true;
        }
    }

    // ── Frustration signals with instruction context ───────────────────
    // Only fire on frustration + instruction combo, not standalone frustration
    let has_frustration = lower.contains("not listening")
        || lower.contains("you're ignoring")
        || lower.contains("youre ignoring")
        || lower.contains("you keep")
        || lower.contains("you're not")
        || lower.contains("stop ignoring")
        || lower.contains("why are you")
        || lower.contains("why aren't you");
    let has_instruction_ref = lower.contains("my instruction")
        || lower.contains("my question")
        || lower.contains("my request")
        || lower.contains("what i said")
        || lower.contains("what i asked")
        || lower.contains("the task")
        || lower.contains("focus on");
    if has_frustration && has_instruction_ref {
        return true;
    }

    // ── Explicit control assertion ─────────────────────────────────────
    // "Focus on my instructions not the tasks" pattern from user's screenshot
    if (lower.contains("focus on") || lower.contains("follow"))
        && (lower.contains("instruction")
            || lower.contains("direction")
            || lower.contains("what i"))
    {
        return true;
    }

    false
}

// ── Implicit Topic Shift Detection ─────────────────────────────────────────────

/// Detect an implicit topic shift: the user sends a short message that
/// doesn't look like a continuation of the previous tool-heavy exchange.
///
/// Unlike `detect_user_override` (which catches explicit "stop" / "new topic"),
/// this uses **inverted matching**: after a tool-heavy exchange, ANY short
/// message is treated as a topic break UNLESS it looks like a task continuation
/// (e.g. "yes", "option 1", "go ahead", "do that"). This avoids brittleness
/// from trying to enumerate every possible casual phrase — "I'm hungry",
/// "tell me a joke", "hello", etc. all get caught naturally.
///
/// When a shift is detected, this function:
/// 1. Strips all tool-call assistant messages and tool-result messages from history
///    (these dominate the context and anchor the model to the old task)
/// 2. Injects a strong system nudge at position 1 (right after the main system prompt)
///    so the model sees it BEFORE any remaining conversation history
///
/// Returns `true` if a topic shift was detected and momentum should be cleared.
pub fn detect_implicit_topic_shift(messages: &mut Vec<Message>) -> bool {
    // Need at least 3 messages: system + assistant + user
    if messages.len() < 3 {
        return false;
    }

    // Get the last user message
    let last_user = match messages.iter().rev().find(|m| m.role == Role::User) {
        Some(m) => m.content.as_text_ref().to_string(),
        None => return false,
    };

    let trimmed = last_user.trim();
    let lower = trimmed.to_lowercase();

    // Only consider short messages — long messages carry enough context
    // for the model to figure out the topic on its own.
    if trimmed.len() > 80 {
        return false;
    }

    // ── Check if recent context was tool-heavy ─────────────────────────
    let recent_msgs: Vec<&Message> = messages.iter().rev().take(20).collect();
    let tool_call_count = recent_msgs
        .iter()
        .filter(|m| {
            m.role == Role::Assistant && m.tool_calls.as_ref().is_some_and(|tc| !tc.is_empty())
        })
        .count();
    let tool_result_count = recent_msgs.iter().filter(|m| m.role == Role::Tool).count();
    let total_tool_activity = tool_call_count + tool_result_count;

    // Also check if the last assistant response was very long
    let last_assistant_long = recent_msgs
        .iter()
        .find(|m| m.role == Role::Assistant)
        .is_some_and(|m| m.content.as_text_ref().len() > 500);

    // Only fire if there was significant prior activity
    if total_tool_activity < 2 && !last_assistant_long {
        return false;
    }

    // ── Inverted match: skip if it looks like a task CONTINUATION ───────
    // These are phrases where the user clearly wants the previous task
    // to continue. Everything else is treated as a potential topic break.
    if is_task_continuation(&lower) {
        return false;
    }

    // ── Aggressively strip tool messages from history ──────────────────
    // Tool-call + tool-result pairs dominate the context window and anchor
    // the model to the old task. Remove them so the model sees a clean slate.
    let before_len = messages.len();
    messages.retain(|m| {
        // Keep all user messages (conversation continuity)
        if m.role == Role::User {
            return true;
        }
        // Keep system messages
        if m.role == Role::System {
            return true;
        }
        // Strip tool-result messages entirely
        if m.role == Role::Tool {
            return false;
        }
        // Strip assistant messages that had tool_calls (these are the
        // "I'll now use canvas_create..." messages that anchor the old task)
        if m.role == Role::Assistant && m.tool_calls.as_ref().is_some_and(|tc| !tc.is_empty()) {
            return false;
        }
        // Keep regular assistant text responses (conversation memory)
        true
    });
    let stripped = before_len - messages.len();

    // ── Inject strong system nudge at position 1 ──────────────────────
    // Place it right after the main system prompt so the model sees it
    // BEFORE any remaining conversation history.
    let nudge = format!(
        "** CONTEXT SHIFT: The user's latest message is: \"{}\"\n\n\
         This is NOT a continuation of the previous task. {} tool interaction messages \
         have been cleared from context. Respond ONLY to what the user just said. \
         Do NOT reference, resume, or continue any previous task (canvas, generation, etc.) \
         unless the user explicitly asks you to.",
        safe_truncate(trimmed, 60),
        stripped
    );

    log::info!(
        "[engine] Implicit topic shift: \"{}\" — stripped {} tool messages, {} remain",
        safe_truncate(trimmed, 40),
        stripped,
        messages.len()
    );

    // Insert at position 1 (after system prompt) or 0 if no system prompt
    let insert_pos = if !messages.is_empty() && messages[0].role == Role::System {
        1
    } else {
        0
    };
    messages.insert(
        insert_pos,
        Message {
            role: Role::System,
            content: MessageContent::Text(nudge),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
    );

    true
}

/// Check if a short user message looks like a continuation of the previous task.
/// Returns `true` for affirmative/selection responses that clearly reference
/// the ongoing exchange. Everything not matching here is treated as a topic shift.
fn is_task_continuation(lower: &str) -> bool {
    // Direct affirmatives / go-ahead signals
    let continuations = [
        "yes",
        "yeah",
        "yep",
        "yup",
        "sure",
        "ok",
        "okay",
        "k",
        "kk",
        "go ahead",
        "go for it",
        "do it",
        "do that",
        "proceed",
        "continue",
        "sounds good",
        "looks good",
        "perfect",
        "great",
        "let's go",
        "lets go",
        "make it so",
        "ship it",
        "send it",
        "run it",
        "yes please",
        "yeah do it",
        "yes do it",
        "go",
        "confirmed",
        "approved",
        "accept",
        "yes that",
        "yep that",
    ];
    if continuations.contains(&lower) {
        return true;
    }

    // Numbered/lettered option selections (e.g. "1", "option 2", "#3", "b")
    let s = lower.trim_start_matches('#');
    if s.len() <= 3 && s.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    if lower.starts_with("option ") || lower.starts_with("number ") || lower.starts_with("choice ")
    {
        return true;
    }
    // Single letter a-e (menu selection)
    if lower.len() == 1 && lower.chars().next().is_some_and(|c| c.is_ascii_lowercase()) {
        return true;
    }

    // Explicit resume/continue references
    if lower.contains("keep going")
        || lower.contains("carry on")
        || lower.contains("as i said")
        || lower.contains("like i said")
        || lower.contains("what i asked")
        || lower.contains("resume")
        || lower.contains("finish it")
        || lower.contains("finish that")
        || lower.contains("complete it")
        || lower.contains("complete that")
    {
        return true;
    }

    false
}

// ── Attachment processor ───────────────────────────────────────────────────────

/// Convert chat attachments into multi-modal content blocks on the last user message.
///
/// Replaces the last user message's `Text` content with a `Blocks` list containing:
///   - A `Text` block with the original message text
///   - One block per attachment: `ImageUrl`, `Document`, or inlined `Text`
///
/// No-op if `attachments` is empty or the last message is not a user message.
pub fn process_attachments(
    user_message: &str,
    attachments: &[ChatAttachment],
    messages: &mut [Message],
) {
    if attachments.is_empty() {
        return;
    }
    let Some(last_msg) = messages.last_mut() else {
        return;
    };
    if last_msg.role != Role::User {
        return;
    }

    info!("[engine] Processing {} attachment(s)", attachments.len());

    let mut blocks = vec![ContentBlock::Text {
        text: user_message.to_string(),
    }];

    for att in attachments {
        let label = att.name.as_deref().unwrap_or("attachment");
        info!(
            "[engine] Attachment '{}' type={} size={}B",
            label,
            att.mime_type,
            att.content.len()
        );

        if att.mime_type.starts_with("image/") {
            // Images → native vision content blocks
            let data_url = format!("data:{};base64,{}", att.mime_type, att.content);
            blocks.push(ContentBlock::ImageUrl {
                image_url: ImageUrlData {
                    url: data_url,
                    detail: Some("auto".into()),
                },
            });
        } else if att.mime_type == "application/pdf" {
            // PDFs → native document blocks (Claude, Gemini, OpenAI all support this)
            blocks.push(ContentBlock::Document {
                mime_type: att.mime_type.clone(),
                data: att.content.clone(),
                name: att.name.clone(),
            });
        } else {
            // Text-based files → decode base64 and inline as a fenced code block
            use base64::Engine as _;
            match base64::engine::general_purpose::STANDARD.decode(&att.content) {
                Ok(bytes) => {
                    let text_content = String::from_utf8_lossy(&bytes);
                    blocks.push(ContentBlock::Text {
                        text: format!(
                            "[Attached file: {} ({})]\n```\n{}\n```",
                            label, att.mime_type, text_content
                        ),
                    });
                }
                Err(e) => {
                    warn!("[engine] Failed to decode attachment '{}': {}", label, e);
                    blocks.push(ContentBlock::Text {
                        text: format!(
                            "[Attached file: {} ({}) — could not decode content]",
                            label, att.mime_type
                        ),
                    });
                }
            }
        }
    }

    last_msg.content = MessageContent::Blocks(blocks);
}
