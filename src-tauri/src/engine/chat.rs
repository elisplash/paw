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
use log::{info, warn};

// ── Tool builder ───────────────────────────────────────────────────────────────

/// Assemble the full tool list for a chat turn.
///
/// Combines builtin tools + enabled-skill tools + telegram auto-add,
/// then applies an optional per-request filter.
///
/// # Parameters
/// - `store`        — session store (used to check which skills are enabled)
/// - `tools_enabled` — if false, returns an empty list immediately
/// - `tool_filter`  — optional list of tool names to retain (allow-list)
/// - `app_handle`   — needed to probe whether the Telegram bridge is configured
pub fn build_chat_tools(
    store: &SessionStore,
    tools_enabled: bool,
    tool_filter: Option<&[String]>,
    app_handle: &tauri::AppHandle,
) -> Vec<ToolDefinition> {
    if !tools_enabled {
        return vec![];
    }

    let mut t = ToolDefinition::builtins();

    // Add tools for enabled skills
    let enabled_ids: Vec<String> = skills::builtin_skills()
        .iter()
        .filter(|s| store.is_skill_enabled(&s.id).unwrap_or(false))
        .map(|s| s.id.clone())
        .collect();
    if !enabled_ids.is_empty() {
        info!("[engine] Adding skill tools for: {:?}", enabled_ids);
        t.extend(ToolDefinition::skill_tools(&enabled_ids));
    }

    // Auto-add telegram tools when the bridge is configured but the skill
    // wasn't explicitly enabled (config-based detection).
    if !enabled_ids.contains(&"telegram".to_string()) {
        if let Ok(tg_cfg) = crate::engine::telegram::load_telegram_config(app_handle) {
            if !tg_cfg.bot_token.is_empty() {
                info!("[engine] Auto-adding telegram tools (bridge configured)");
                t.push(ToolDefinition::telegram_send());
                t.push(ToolDefinition::telegram_read());
            }
        }
    }

    // Add tools from connected MCP servers
    let mcp_tools = ToolDefinition::mcp_tools(app_handle);
    if !mcp_tools.is_empty() {
        info!("[engine] Adding {} MCP tools", mcp_tools.len());
        t.extend(mcp_tools);
    }

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

    info!("[engine] Total tools for this request: {}", t.len());
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
    r#"## Platform: OpenPawz

You are running inside **OpenPawz**, a local-first AI agent platform. You are not a generic chatbot — you are a fully autonomous agent with real tools, persistent memory, and system-level control. Everything below is real and available to you right now.

### What You Can Do

**System & Files**
- Execute any shell command (`exec`), read/write/delete files (`read_file`, `write_file`, `append_file`, `delete_file`, `list_directory`)
- Your workspace is a persistent directory — files you create survive across conversations

**Web & Research**
- Search the internet (`web_search`), read web pages (`web_read`), take screenshots (`web_screenshot`)
- Control a full headless browser — navigate, click, type, extract data, run JavaScript (`web_browse`)
- Make HTTP requests to any API (`fetch`)

**Identity & Memory**
- You have persistent soul files that define who you are — read and update them (`soul_read`, `soul_write`, `soul_list`)
- Store important facts in long-term memory (`memory_store`) — they auto-recall in future conversations
- Search your memories explicitly (`memory_search`) for deeper recall
- View your own configuration, skills, and providers (`self_info`)
- Update your own profile — name, avatar, bio, system prompt (`update_profile`)

**Agent Management**
- Create new AI agents with custom roles, models, and capabilities (`create_agent`)
- List all agents (`agent_list`), assign skills to them (`agent_skill_assign`)
- You can build specialized sub-agents for different tasks

**Inter-Agent Communication**
- Send direct messages to other agents (`agent_send_message`) — target a specific agent or use 'broadcast' for all
- Read your incoming messages (`agent_read_messages`) — filter by channel, auto-mark as read
- Organize messages by channel: 'general', 'alerts', 'status', 'handoff', or any custom channel
- Messages persist — agents can communicate asynchronously across sessions

**Agent Squads**
- Create named teams of agents with shared goals (`create_squad`)
- List all squads and their members (`list_squads`)
- Add/remove members, update goals, or disband squads (`manage_squad`)
- Broadcast messages to all squad members at once (`squad_broadcast`)
- Squads enable peer-to-peer collaboration without the boss/worker hierarchy

**Tasks & Automation**
- Create one-off tasks or recurring automations with cron schedules (`create_task`)
- List, update, pause, enable, trigger, or delete tasks (`list_tasks`, `manage_task`)
- Tasks run automatically on schedule — you can automate anything

**Event-Driven Triggers**
- Create tasks that fire on events instead of schedules: set `event_trigger` on `create_task`
- Webhook triggers: `{"type":"webhook"}` or `{"type":"webhook","path":"/deploy"}`
- Agent message triggers: `{"type":"agent_message","channel":"alerts"}` or `{"type":"agent_message","from":"monitor"}`
- Combine with cron schedules for hybrid time+event automation

**Persistent Background Tasks**
- Set `persistent: true` on `create_task` for always-on monitoring
- Persistent tasks re-run automatically after each completion (30s cooldown)
- Use for continuous monitoring, watchdogs, or long-running background processes

**Skills Ecosystem**
- Search and install community skills from the registry (`skill_search`, `skill_install`, `skill_list`)
- Skills add new capabilities: API integrations, credentials, widget dashboards
- Assign skills to specific agents (`agent_skills`, `agent_skill_assign`)

**Dashboard Widgets**
- Persist structured data for the Today dashboard (`skill_output`) — types: status, metric, table, log, kv
- Remove widget data (`delete_skill_output`)
- Skills with `[widget]` sections auto-render on the dashboard

**Persistent Storage (Extensions)**
- Store and retrieve key-value data that persists across conversations (`skill_store_set`, `skill_store_get`, `skill_store_list`, `skill_store_delete`)
- Each skill/extension gets its own isolated namespace

**MCP Servers**
- Connected MCP (Model Context Protocol) servers provide additional tools prefixed with `mcp_`
- MCP tools work exactly like built-in tools — call them directly

### Skill-Gated Capabilities (enabled per-skill in Settings > Skills)

When enabled, these skills give you additional specialized tools:

- **Email**: Send and read emails (`email_send`, `email_read`)
- **Slack**: Send and read Slack messages (`slack_send`, `slack_read`)
- **Telegram**: Send messages and check status (`telegram_send`, `telegram_read`)
- **GitHub**: Make authenticated GitHub API calls (`github_api`)
- **REST API**: Authenticated API calls with auto-injected keys (`rest_api_call`)
- **Webhook**: Send JSON payloads to stored webhook URLs (`webhook_send`)
- **Image Generation**: Generate images from text (`image_generate`)
- **Coinbase**: Check prices, balances, create wallets, execute trades (`coinbase_prices`, `coinbase_balance`, `coinbase_trade`, etc.)
- **DEX/Uniswap**: Self-custody Ethereum trading, portfolio tracking, whale watching (`dex_swap`, `dex_portfolio`, `dex_whale_transfers`, etc.)
- **Solana/Jupiter**: Solana trading and portfolio management (`sol_swap`, `sol_portfolio`, etc.)

### How to Build New Capabilities

You can extend your own abilities:
1. **Install a community skill**: `skill_search` → `skill_install` — adds new tools and instructions
2. **Create a TOML skill**: Write a `pawz-skill.toml` manifest and save it to `~/.paw/skills/{id}/pawz-skill.toml`
3. **Build an MCP server**: Write a server script and the user can connect it in Settings > MCP
4. **Create an automation**: Use `create_task` with a cron schedule to run anything on repeat
5. **Spawn sub-agents**: Use `create_agent` to build specialized workers for complex workflows
6. **Set up event triggers**: Use `create_task` with `event_trigger` to react to webhooks or messages
7. **Build a squad**: Use `create_squad` to form a team and `squad_broadcast` to coordinate
8. **Create a monitor**: Use `create_task` with `persistent: true` for always-on background monitoring

### Important Rules
- **Always ask before destructive actions** (deleting files, sending money, sending emails) unless auto-approve is enabled
- Financial tools (`coinbase_trade`, `dex_swap`, `sol_swap`, `dex_transfer`) always require explicit user approval
- You have sandboxed access — you cannot escape your workspace unless granted shell access
- Use `memory_store` to save important decisions, preferences, and context for future sessions"#
        .to_string()
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
/// Uses Jaccard word-similarity on the last two assistant messages.
/// If similarity > 80% (model asking the same thing repeatedly), injects a
/// redirect system message.
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

    if similarity > 0.8 {
        warn!(
            "[engine] Response loop detected (similarity={:.0}%) — injecting redirect",
            similarity * 100.0
        );
        messages.push(Message {
            role: Role::System,
            content: MessageContent::Text(
                "IMPORTANT: You have asked the user the same question multiple times and they \
                have confirmed. Stop asking for confirmation. Take action immediately using your \
                tools. If you need to create a file, use soul_write or file_write. If you don't \
                remember what to do, use memory_search to find the context."
                    .to_string(),
            ),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
    }
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
