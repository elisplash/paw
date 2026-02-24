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

use crate::engine::types::*;
use crate::engine::sessions::SessionStore;
use crate::engine::skills;
use crate::engine::tools;
use crate::engine::tool_index;
use log::{info, warn};

// ── Tool builder ───────────────────────────────────────────────────────────────

/// Assemble the tool list for a chat turn using Tool RAG (lazy loading).
///
/// Instead of dumping all 75+ tools, sends only:
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
    let is_policy_allowed = |name: &str| {
        tool_filter.is_some_and(|f| f.iter().any(|n| n == name))
    };

    let mut t: Vec<ToolDefinition> = all_tools.into_iter()
        .filter(|tool| {
            let name = tool.function.name.as_str();
            is_core(name) || is_loaded(name) || is_mcp(name) || is_policy_allowed(name)
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
/// Contains: model, provider, session, agent, current time, workspace path.
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
        format!(
            "{} {}",
            local.format("%Y-%m-%d %H:%M"),
            local.format("%A")
        )
    };

    let ws = tools::agent_workspace(agent_id);

    format!(
        "## Runtime\n\
        Model: {} | Provider: {} | Session: {} | Agent: {}\n\
        Time: {}\n\
        Workspace: {}",
        model,
        provider_name,
        session_id,
        agent_id,
        time_str,
        ws.display(),
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

    format!(
        r#"## Platform: OpenPawz

You are running inside **OpenPawz**, a local-first AI agent platform. You are not a generic chatbot — you are a fully autonomous agent with real tools, persistent memory, and system-level control.

### How Tools Work (Tool RAG)

You have a few core tools always loaded (memory, soul files, file I/O). Your full toolkit has 75+ tools across many domains, but they're loaded **on demand** to keep you fast and focused.

**Your core tools (always available):**
- `memory_store` / `memory_search` — long-term memory (persists across conversations)
- `soul_read` / `soul_write` / `soul_list` — your identity and personality files
- `self_info` — view your configuration, skills, providers
- `read_file` / `write_file` / `list_directory` — file operations in your workspace

**Your skill library (call `request_tools` to load):**
{}

**To load tools:** Call `request_tools` with a description of what you need.
- Example: `request_tools({{"query": "send an email"}})` → loads email_send, email_read
- Example: `request_tools({{"query": "crypto trading on solana"}})` → loads sol_swap, sol_balance, etc.
- Example: `request_tools({{"domain": "web"}})` → loads all web tools
- Tools stay loaded for the rest of this conversation turn.

### How to Build New Capabilities

1. **Install a community skill**: `skill_search` → `skill_install`
2. **Create a TOML integration**: Write `pawz-skill.toml` to `~/.paw/skills/{{id}}/`
3. **Build an MCP server**: Connect in Settings → MCP
4. **Create an automation**: `create_task` with cron schedule
5. **Spawn sub-agents**: `create_agent` for specialized workers
6. **Set up event triggers**: `create_task` with `event_trigger`
7. **Build a squad**: `create_squad` + `squad_broadcast`

### TOML Skill Template

```toml
[skill]
id = "my-tool"
name = "My Tool"
version = "1.0.0"
author = "user"
category = "api"            # api|cli|productivity|media|development|system|communication
icon = "search"             # Material Symbol icon name
description = "What this skill does"
install_hint = "Get your API key at https://example.com/api"
required_binaries = []
required_env_vars = []

[[credentials]]
key = "API_KEY"
label = "API Key"
description = "Your API key from example.com"
required = true
placeholder = "sk-..."

[instructions]
text = """
You have access to the My Tool API.
API Key: {{{{API_KEY}}}}
Base URL: https://api.example.com/v1

To search: `fetch` POST https://api.example.com/v1/search with header Authorization: Bearer {{{{API_KEY}}}}
"""

[widget]
type = "table"
title = "My Tool Results"

[[widget.fields]]
key = "name"
label = "Name"
type = "text"
```

### Conversation Discipline
- **Prefer action over clarification** — When the user gives short directives like "yes", "do it", "both", "go ahead", or "try again", act immediately using your tools instead of asking follow-up questions. Infer intent from conversation context.
- **If a tool fails, try alternatives** — Use `request_tools` to discover dedicated tools instead of retrying the same generic tool. For example, use `google_docs_create` instead of `google_api` for creating documents.
- **Maximum 2 tool attempts per approach** — If a tool fails twice with the same strategy, switch to a completely different approach. Call `request_tools` to find alternative tools.
- **Load tools before using them** — If you need a tool that isn't in your core set, call `request_tools` first.
- **If a tool doesn't exist, call `request_tools` immediately** — Never guess tool names. If you call a tool and get "unknown tool", your very next action must be `request_tools` to find the right one.
- **Always ask before destructive actions** (deleting files, sending money, sending emails) unless auto-approve is enabled
- Financial tools (coinbase_trade, dex_swap, sol_swap) always require explicit user approval
- You have sandboxed access — you cannot escape your workspace unless granted shell access
- Use `memory_store` to save important decisions, preferences, and context for future sessions
- **Be concise** — Keep responses short and action-oriented. Don't pad with filler phrases. Just do it."#,
        domains.join("\n")
    )
}

// ── Code-generation discipline ─────────────────────────────────────────────────

/// Coding guidelines injected into every system prompt so the agent produces
/// code that integrates cleanly with the OpenPawz codebase and the wider
/// TOML-skill / MCP ecosystem.  These are non-negotiable quality gates.
pub fn build_coding_guidelines() -> &'static str {
    r#"## Code-Generation Guidelines

When you write, modify, or install code (skills, MCP servers, scripts, Rust modules, extensions, or any executable artifact), you **must** follow every rule below. Violations will be reverted.

### 1. Repository Hygiene
- **Never commit build artifacts.** `target/`, `node_modules/`, `dist/`, `build/`, `*.o`, `*.so`, `*.dylib`, `*.exe`, `*.wasm` (unless intentional release assets) must never be staged.
- **Keep `.gitignore` current.** If you create a new build pipeline, add its output directory to `.gitignore` before the first commit.
- **Atomic commits.** One logical change per commit. Don't bundle unrelated fixes.
- **No generated files in source.** Lock-files (`Cargo.lock` for binaries, `package-lock.json`) are fine; generated code is not.

### 2. Architecture Compliance
- **Use shared modules.** Before creating new infrastructure, check if `src-tauri/src/engine/channels/`, `src-tauri/src/engine/skills/`, or another existing module already covers the need. Extend, don't duplicate.
- **No standalone binaries.** Never compile a separate sidecar binary when the functionality should integrate with the engine. All Rust code compiles into the single Tauri binary.
- **Follow the layered architecture.** `commands/` (thin system layer) → `engine/` (organisms + molecules + atoms). Organisms contain business logic; atoms are pure helpers. Never import `commands/` from `engine/`.
- **Use engine error types.** Return `EngineResult<T>` from engine functions. Map external errors with `EngineError::Internal(msg)`. Never `unwrap()` in production paths — use `?` or explicit error handling.

### 3. Rust Code Standards
- **Logging.** Use the `log` crate (`info!`, `warn!`, `error!`). Prefix every message with a bracketed tag: `info!("[discord] Connected to gateway")`. Never use `println!` or `eprintln!` in library code.
- **Error handling.** Propagate with `?`. Add context: `.map_err(|e| EngineError::Internal(format!("[skill] load failed: {}", e)))?`.
- **String safety.** When truncating strings, always use `floor_char_boundary()` to avoid slicing inside a multi-byte UTF-8 character.
- **Async discipline.** Use `tokio` for async. Never block the async runtime with `std::thread::sleep` — use `tokio::time::sleep`. Never spawn detached threads for work that should be a `tokio::spawn` task with proper cancellation.
- **Dependencies.** Do not add new crate dependencies without justification. Prefer what's already in `Cargo.toml`. If you must add a crate, verify it's maintained and has a compatible license (MIT/Apache-2.0).

### 4. TypeScript / Frontend Standards
- **State management.** Use Jotai atoms in `src/state/`. Never use global mutable variables.
- **Reactivity.** Components use Lit (`@lit/reactive-element`). Follow existing patterns in `src/components/` and `src/engine/`.
- **No raw DOM manipulation.** Use Lit's reactive properties and templates.
- **Imports.** Relative paths within `src/`. No circular imports.

### 5. Security — Non-Negotiable
- **No hardcoded secrets.** API keys, tokens, passwords, and private keys must **never** appear in source code, commit messages, or logs. Use the TOML `[[credentials]]` system or environment variables.
- **Credential injection.** In TOML skill instructions, reference credentials as `{{KEY_NAME}}`. The engine replaces these at runtime from the encrypted credential store.
- **Sanitize all external input.** Shell arguments via `exec` must be escaped. User-supplied strings must never be interpolated into SQL, shell commands, or file paths without validation.
- **File-system boundaries.** Skills and scripts must operate within the agent workspace (`~/.paw/agents/{id}/workspace/`) or designated config dirs. Never write outside these paths without explicit user approval.

### 6. TOML Skill Authoring
- **Manifest required.** Every skill directory under `~/.paw/skills/{id}/` must contain a valid `pawz-skill.toml` with at minimum `[skill]` (id, name, version, author, category, description) and `[instructions]`.
- **ID format.** Lowercase alphanumeric + hyphens only: `my-cool-skill`. No underscores, spaces, or uppercase.
- **Instruction text.** The `[instructions].text` field tells the agent how to use the skill. Include: base URL, auth header format, available endpoints, and example payloads. Keep under 2000 chars — instructions are subject to compression.
- **Credentials.** Declare every required secret in `[[credentials]]` with clear labels and placeholders. Reference in instructions as `{{KEY_NAME}}`.
- **Category.** Use one of: `vault`, `cli`, `api`, `productivity`, `media`, `smart_home`, `communication`, `development`, `system`.

### 7. MCP Server Authoring
- **Transport.** Default is `stdio`. Only use `sse` or `streamable-http` if the server is remote.
- **Keep it lean.** An MCP server should expose a focused set of tools. Don't build monoliths — split into multiple skills if the domain is broad.
- **Error responses.** Return structured JSON errors with `isError: true` and a human-readable message. Never crash on bad input.
- **Startup.** The MCP `command` must be executable on the user's system. Document `required_binaries` and `install_hint` in the TOML manifest.

### 8. Testing & Validation
- **Tests are mandatory for new logic.** Rust: add `#[cfg(test)] mod tests` in the same file or a dedicated `tests/` module. TypeScript: add `.test.ts` alongside the source file using Vitest.
- **Run tests before committing.** `cargo test` for Rust, `npx vitest run` for TypeScript.
- **Don't break existing tests.** If your change causes test failures, fix them in the same commit.

### 9. Process
- **Read before writing.** Before creating a new file, use `list_directory` and `read_file` to check for existing implementations.
- **Explain the plan first.** Before writing more than 50 lines of code, briefly describe what you're building and which existing modules you'll integrate with.
- **One skill = one PR scope.** Don't mix unrelated skill work. Each skill or feature is a self-contained unit.
- **Clean up after yourself.** Remove temporary files, test artifacts, and debug logging before committing."#
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
        if agent.agent_id == current_agent_id { continue; } // don't list yourself
        if agent.agent_id == "default" { continue; } // skip the default agent entry

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
    parts.push(build_coding_guidelines().to_string());
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

// ── Response loop detector ─────────────────────────────────────────────────────

/// Detect stuck response loops and inject a system nudge to break the cycle.
///
/// Checks:
/// 1. **Repetition**: Jaccard word-similarity > 40% between last two assistant
///    messages — the model is repeating itself with minor rewording.
/// 2. **Question loop**: Both last assistant messages end in `?` — the model
///    keeps asking clarifying questions instead of acting.
/// 3. **Topic-ignoring**: The last user message shares < 25% keywords with the
///    model's response — the model is ignoring what the user asked about.
///
/// In all cases, a system-role redirect is injected telling the model to
/// stop repeating itself and respond to the user's actual request.
pub fn detect_response_loop(messages: &mut Vec<Message>) {
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

    // ── Check 1: assistant repeating itself (> 40% overlap) ────────────
    if similarity > 0.40 {
        warn!(
            "[engine] Response loop detected (similarity={:.0}%) — injecting redirect",
            similarity * 100.0
        );
        inject_loop_break(messages);
        return;
    }

    // ── Check 2: question loop — both responses are questions ──────────
    // When the model asks "Should I do X?" twice in a row, it's stuck
    // asking for confirmation instead of acting.
    let a_is_question = a.trim_end().ends_with('?');
    let b_is_question = b.trim_end().ends_with('?');
    if a_is_question && b_is_question {
        warn!(
            "[engine] Question loop detected — assistant asked two consecutive questions"
        );
        inject_loop_break(messages);
        return;
    }

    // ── Check 3: assistant ignoring the user's topic ───────────────────
    // Find last user message and check if assistant response addresses it.
    let last_user = messages
        .iter()
        .rev()
        .find(|m| m.role == Role::User)
        .map(|m| m.content.as_text_ref().to_lowercase());

    if let Some(user_text) = last_user {
        let stop_words: std::collections::HashSet<&str> = [
            "the", "a", "an", "is", "are", "was", "were", "be", "been", "being",
            "have", "has", "had", "do", "does", "did", "will", "would", "could",
            "should", "may", "might", "can", "shall", "to", "of", "in", "for",
            "on", "with", "at", "by", "from", "as", "into", "about", "like",
            "through", "after", "over", "between", "out", "against", "during",
            "i", "you", "he", "she", "it", "we", "they", "me", "him", "her",
            "us", "them", "my", "your", "his", "its", "our", "their", "this",
            "that", "these", "those", "and", "but", "or", "nor", "not", "so",
            "if", "then", "than", "too", "very", "just", "don't", "im",
            "i'd", "i'm", "i'll", "i've", "you're", "it's", "what", "how",
            "all", "each", "which", "who", "when", "where", "why",
        ].into_iter().collect();

        let user_keywords: std::collections::HashSet<&str> = user_text
            .split_whitespace()
            .filter(|w| w.len() > 2 && !stop_words.contains(w))
            .collect();
        let asst_keywords: std::collections::HashSet<&str> = a
            .split_whitespace()
            .filter(|w| w.len() > 2 && !stop_words.contains(w))
            .collect();

        // Check for short affirmative/directive user messages — "both", "yes",
        // "do it", "go ahead". If the user gives a brief directive and the
        // model responds with another question, that's a loop.
        let short_directive = user_text.split_whitespace().count() <= 4;
        if short_directive && a_is_question && similarity > 0.20 {
            warn!(
                "[engine] Short-directive loop: user said '{}' but model asked another question \
                (similarity={:.0}%) — injecting redirect",
                user_text, similarity * 100.0
            );
            inject_loop_break(messages);
            return;
        }

        if !user_keywords.is_empty() && !asst_keywords.is_empty() {
            let topic_overlap = user_keywords.intersection(&asst_keywords).count();
            let topic_ratio = topic_overlap as f64 / user_keywords.len() as f64;

            // Also check: are the two assistant messages MORE similar to each
            // other than the assistant is to the user? That's a strong loop signal.
            if topic_ratio < 0.15 && similarity > 0.30 {
                warn!(
                    "[engine] Topic-ignoring loop: user keywords overlap={:.0}%, \
                    inter-response similarity={:.0}% — injecting redirect",
                    topic_ratio * 100.0, similarity * 100.0
                );
                inject_loop_break(messages);
            }
        }
    }
}

/// Inject a system message that breaks the agent out of a response loop.
fn inject_loop_break(messages: &mut Vec<Message>) {
    // Find the last user message to echo it back
    let last_user_text = messages
        .iter()
        .rev()
        .find(|m| m.role == Role::User)
        .map(|m| m.content.as_text_ref().to_string())
        .unwrap_or_default();

    let redirect = if last_user_text.is_empty() {
        "IMPORTANT: You are stuck in a response loop — repeating the same topic despite the \
        user's request. Read the user's MOST RECENT message carefully and respond ONLY to \
        what they actually asked. Do NOT ask another question. Take action with your tools NOW."
            .to_string()
    } else {
        format!(
            "CRITICAL: You are stuck asking clarifying questions instead of acting. STOP asking. \
            The user's actual request is: \"{}\"\n\n\
            Take action NOW. Use your tools to do what the user asked. \
            If they said 'yes', 'both', 'do it', 'go ahead', or similar — that means proceed with ALL \
            the options you mentioned. Do NOT ask another question. Call the relevant tools immediately.",
            &last_user_text[..last_user_text.len().min(300)]
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
