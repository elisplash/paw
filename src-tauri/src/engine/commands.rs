// Paw Agent Engine — Tauri Commands
// These are the invoke() targets for the frontend.
// They replace the WebSocket gateway methods with direct Rust calls.

use crate::engine::types::*;
use crate::engine::providers::AnyProvider;
use crate::engine::sessions::SessionStore;
use crate::engine::agent_loop;
use crate::engine::memory::{self, EmbeddingClient};
use crate::engine::skills;
use crate::engine::tool_executor;
use log::{info, warn, error};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{Emitter, Manager, State};

/// Pending tool approvals: maps tool_call_id → oneshot sender.
/// The agent loop registers a sender before emitting ToolRequest,
/// then awaits the receiver. The `engine_approve_tool` command
/// resolves it from the frontend.
pub type PendingApprovals = Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>>>;

/// Daily token spend tracker.  Tracks cumulative input & output tokens
/// for the current UTC date.  Resets automatically on new day.
/// All fields are atomic so the tracker can be shared across tasks cheaply.
pub struct DailyTokenTracker {
    /// UTC date string "YYYY-MM-DD" of the current tracking day
    pub date: Mutex<String>,
    /// Cumulative input tokens today
    pub input_tokens: AtomicU64,
    /// Cumulative output tokens today
    pub output_tokens: AtomicU64,
    /// Cumulative cache read tokens today (Anthropic — 90% cheaper)
    pub cache_read_tokens: AtomicU64,
    /// Cumulative cache creation tokens today (Anthropic — 25% cheaper)
    pub cache_create_tokens: AtomicU64,
    /// Accumulated USD cost today (stored as micro-dollars for atomic ops)
    pub cost_microdollars: AtomicU64,
    /// Last model name used (for fallback pricing when model unknown)
    pub last_model: Mutex<String>,
    /// Budget warning thresholds already emitted (50, 75, 90)
    pub warnings_emitted: Mutex<Vec<u8>>,
}

impl DailyTokenTracker {
    pub fn new() -> Self {
        DailyTokenTracker {
            date: Mutex::new(chrono::Utc::now().format("%Y-%m-%d").to_string()),
            input_tokens: AtomicU64::new(0),
            output_tokens: AtomicU64::new(0),
            cache_read_tokens: AtomicU64::new(0),
            cache_create_tokens: AtomicU64::new(0),
            cost_microdollars: AtomicU64::new(0),
            last_model: Mutex::new("unknown".into()),
            warnings_emitted: Mutex::new(Vec::new()),
        }
    }

    fn maybe_reset(&self) {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        if let Ok(mut d) = self.date.lock() {
            if *d != today {
                *d = today;
                self.input_tokens.store(0, Ordering::Relaxed);
                self.output_tokens.store(0, Ordering::Relaxed);
                self.cache_read_tokens.store(0, Ordering::Relaxed);
                self.cache_create_tokens.store(0, Ordering::Relaxed);
                self.cost_microdollars.store(0, Ordering::Relaxed);
                if let Ok(mut w) = self.warnings_emitted.lock() {
                    w.clear();
                }
            }
        }
    }

    /// Add tokens from a completed round with model-aware pricing.
    pub fn record(&self, model: &str, input: u64, output: u64, cache_read: u64, cache_create: u64) {
        self.maybe_reset();
        self.input_tokens.fetch_add(input, Ordering::Relaxed);
        self.output_tokens.fetch_add(output, Ordering::Relaxed);
        self.cache_read_tokens.fetch_add(cache_read, Ordering::Relaxed);
        self.cache_create_tokens.fetch_add(cache_create, Ordering::Relaxed);
        // Calculate cost for this round using per-model pricing
        let cost = crate::engine::types::estimate_cost_usd(model, input, output, cache_read, cache_create);
        let micro = (cost * 1_000_000.0) as u64;
        self.cost_microdollars.fetch_add(micro, Ordering::Relaxed);
        if let Ok(mut m) = self.last_model.lock() {
            *m = model.to_string();
        }
    }

    /// Estimate today's USD spend using accumulated per-model costs.
    /// Returns (input_tokens, output_tokens, estimated_usd).
    pub fn estimated_spend_usd(&self) -> (u64, u64, f64) {
        self.maybe_reset();
        let inp = self.input_tokens.load(Ordering::Relaxed);
        let out = self.output_tokens.load(Ordering::Relaxed);
        let micro = self.cost_microdollars.load(Ordering::Relaxed);
        (inp, out, micro as f64 / 1_000_000.0)
    }

    /// Check if today's spend exceeds the budget.  Returns Some(spend_usd) if over budget.
    pub fn check_budget(&self, budget_usd: f64) -> Option<f64> {
        let (_, _, usd) = self.estimated_spend_usd();
        if usd >= budget_usd { Some(usd) } else { None }
    }

    /// Check budget warning thresholds (50%, 75%, 90%).
    /// Returns the threshold percentage if a NEW warning should be emitted.
    pub fn check_budget_warning(&self, budget_usd: f64) -> Option<u8> {
        if budget_usd <= 0.0 { return None; }
        let (_, _, usd) = self.estimated_spend_usd();
        let pct = (usd / budget_usd * 100.0) as u8;
        let thresholds = [90u8, 75, 50]; // check highest first
        if let Ok(mut emitted) = self.warnings_emitted.lock() {
            for &t in &thresholds {
                if pct >= t && !emitted.contains(&t) {
                    emitted.push(t);
                    return Some(t);
                }
            }
        }
        None
    }
}
/// Map retired / renamed model IDs to their current replacements.
/// This lets old task configs and agent overrides stored in the DB keep working.
pub fn normalize_model_name(model: &str) -> &str {
    match model {
        // Anthropic retired 3.5 model IDs — remap to cheapest available
        // Haiku 3.5 ($0.80/$4) retired → Haiku 3 ($0.25/$1.25) is cheapest
        "claude-3-5-haiku-20241022" => "claude-3-haiku-20240307",
        // Sonnet 3.5 retired → Sonnet 4.6 (same price tier $3/$15)
        "claude-3-5-sonnet-20241022" => "claude-sonnet-4-6",
        "claude-3-5-sonnet-20240620" => "claude-sonnet-4-6",
        // OpenRouter prefixed variants
        "anthropic/claude-3-5-haiku-20241022" => "anthropic/claude-3-haiku-20240307",
        "anthropic/claude-3-5-sonnet-20241022" => "anthropic/claude-sonnet-4-6",
        _ => model,
    }
}

/// Resolve the correct provider for a given model name.
/// First checks if the model's default_model matches any provider exactly,
/// then matches by model prefix (claude→Anthropic, gemini→Google, gpt→OpenAI)
/// and by base URL or provider ID for OpenAI-compatible providers.
pub fn resolve_provider_for_model(model: &str, providers: &[ProviderConfig]) -> Option<ProviderConfig> {
    let model = normalize_model_name(model);
    // 1. Exact match: a provider whose default_model matches exactly
    if let Some(p) = providers.iter().find(|p| p.default_model.as_deref() == Some(model)) {
        return Some(p.clone());
    }

    // 2. Match by model name prefix → well-known provider kind
    if model.starts_with("claude") || model.starts_with("anthropic") {
        providers.iter().find(|p| p.kind == ProviderKind::Anthropic).cloned()
    } else if model.starts_with("gemini") || model.starts_with("google") {
        providers.iter().find(|p| p.kind == ProviderKind::Google).cloned()
    } else if model.starts_with("gpt") || model.starts_with("o1") || model.starts_with("o3") || model.starts_with("o4") {
        providers.iter().find(|p| p.kind == ProviderKind::OpenAI).cloned()
    } else if model.starts_with("moonshot") || model.starts_with("kimi") {
        providers.iter().find(|p| p.id == "moonshot" || p.base_url.as_deref().map_or(false, |u| u.contains("moonshot"))).cloned()
    } else if model.starts_with("deepseek") {
        providers.iter().find(|p| p.id == "deepseek" || p.base_url.as_deref().map_or(false, |u| u.contains("deepseek"))).cloned()
    } else if model.starts_with("grok") {
        providers.iter().find(|p| p.id == "xai" || p.base_url.as_deref().map_or(false, |u| u.contains("x.ai"))).cloned()
    } else if model.starts_with("mistral") || model.starts_with("codestral") || model.starts_with("pixtral") {
        providers.iter().find(|p| p.id == "mistral" || p.base_url.as_deref().map_or(false, |u| u.contains("mistral"))).cloned()
    } else {
        None
    }
}

/// Engine state managed by Tauri.
pub struct EngineState {
    pub store: SessionStore,
    pub config: Mutex<EngineConfig>,
    pub memory_config: Mutex<MemoryConfig>,
    pub pending_approvals: PendingApprovals,
    /// Semaphore limiting concurrent agent runs (chat + cron + manual tasks).
    /// Chat gets a reserved slot; background tasks share the rest.
    pub run_semaphore: Arc<tokio::sync::Semaphore>,
    /// Track task IDs currently being executed to prevent duplicate cron fires.
    pub inflight_tasks: Arc<Mutex<HashSet<String>>>,
    /// Daily token spend tracker — shared across all agent runs.
    pub daily_tokens: Arc<DailyTokenTracker>,
}

impl EngineState {
    pub fn new() -> Result<Self, String> {
        let store = SessionStore::open()?;

        // Initialize skill vault tables
        store.init_skill_tables()?;

        // Load config from DB or use defaults
        let mut config = match store.get_config("engine_config") {
            Ok(Some(json)) => {
                serde_json::from_str::<EngineConfig>(&json).unwrap_or_default()
            }
            _ => EngineConfig::default(),
        };

        // ── Auto-patch system prompt for new tools ──────────────────────
        // If the saved system prompt doesn't mention create_agent, inject it
        // so the LLM knows the tool exists (otherwise it falls back to exec+sqlite3).
        if let Some(ref mut prompt) = config.default_system_prompt {
            if !prompt.contains("create_agent") {
                // Insert the create_agent line after self_info
                if let Some(pos) = prompt.find("- **self_info**") {
                    if let Some(newline) = prompt[pos..].find('\n') {
                        let insert_at = pos + newline;
                        prompt.insert_str(insert_at, "\n- **create_agent**: Create new agent personas that appear in the Agents view. When the user asks you to create an agent, use this tool — don't just describe how to do it.");
                        // Persist the patched prompt back to DB
                        if let Ok(json) = serde_json::to_string(&config) {
                            store.set_config("engine_config", &json).ok();
                        }
                        info!("[engine] Auto-patched system prompt to include create_agent tool");
                    }
                }
            }
        }

        // Load memory config from DB or use defaults
        let memory_config = match store.get_config("memory_config") {
            Ok(Some(json)) => {
                serde_json::from_str::<MemoryConfig>(&json).unwrap_or_default()
            }
            _ => MemoryConfig::default(),
        };

        // Read max_concurrent_runs from config (default 4)
        let max_concurrent = config.max_concurrent_runs;

        Ok(EngineState {
            store,
            config: Mutex::new(config),
            memory_config: Mutex::new(memory_config),
            pending_approvals: Arc::new(Mutex::new(HashMap::new())),
            run_semaphore: Arc::new(tokio::sync::Semaphore::new(max_concurrent as usize)),
            inflight_tasks: Arc::new(Mutex::new(HashSet::new())),
            daily_tokens: Arc::new(DailyTokenTracker::new()),
        })
    }

    /// Get an EmbeddingClient from the current memory config, if configured.
    pub fn embedding_client(&self) -> Option<EmbeddingClient> {
        let cfg = self.memory_config.lock().ok()?;
        if cfg.embedding_base_url.is_empty() || cfg.embedding_model.is_empty() {
            return None;
        }
        Some(EmbeddingClient::new(&cfg))
    }
}

// ── Chat commands ──────────────────────────────────────────────────────

/// Send a chat message and run the agent loop.
/// Returns immediately with a run_id; results stream via `engine-event` Tauri events.
#[tauri::command]
pub async fn engine_chat_send(
    app_handle: tauri::AppHandle,
    state: State<'_, EngineState>,
    request: ChatRequest,
) -> Result<ChatResponse, String> {
    let run_id = uuid::Uuid::new_v4().to_string();

    // Resolve or create session
    let session_id = match &request.session_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => {
            let new_id = format!("eng-{}", uuid::Uuid::new_v4());
            let raw = request.model.clone().unwrap_or_default();
            let model = if raw.is_empty() || raw.eq_ignore_ascii_case("default") {
                let cfg = state.config.lock().unwrap();
                cfg.default_model.clone().unwrap_or_else(|| "gpt-4o".to_string())
            } else {
                raw
            };
            state.store.create_session(&new_id, &model, request.system_prompt.as_deref(), request.agent_id.as_deref())?;
            new_id
        }
    };

    // Resolve model and provider
    let (provider_config, model) = {
        let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;

        let raw_model = request.model.clone().unwrap_or_default();
        // Treat empty string or "default" as "use the configured default"
        let base_model = if raw_model.is_empty() || raw_model.eq_ignore_ascii_case("default") {
            cfg.default_model.clone().unwrap_or_else(|| "gpt-4o".to_string())
        } else {
            raw_model
        };

        // ── Smart model tier routing ──
        // When auto_tier is enabled and no explicit model was requested,
        // classify the user message and use cheap_model for simple tasks.
        let user_explicitly_chose_model = request.model.as_ref().map_or(false, |m| {
            !m.is_empty() && !m.eq_ignore_ascii_case("default")
        });
        let (model, was_downgraded) = if !user_explicitly_chose_model {
            cfg.model_routing.resolve_auto_tier(&request.message, &base_model)
        } else {
            (base_model, false)
        };
        if was_downgraded {
            info!("[engine] Auto-tier: simple task → using cheap model '{}' instead of default", model);
        }

        // Remap retired model IDs to current equivalents
        let model = normalize_model_name(&model).to_string();

        // Find provider by ID or use the one that matches the model prefix
        let provider = if let Some(pid) = &request.provider_id {
            cfg.providers.iter().find(|p| p.id == *pid).cloned()
        } else {
            let provider = resolve_provider_for_model(&model, &cfg.providers);
            // Fallback: match by provider whose default_model matches, then default provider, then first
            provider
                .or_else(|| {
                    cfg.providers.iter().find(|p| p.default_model.as_deref() == Some(model.as_str())).cloned()
                })
                .or_else(|| {
                    cfg.default_provider.as_ref()
                        .and_then(|dp| cfg.providers.iter().find(|p| p.id == *dp).cloned())
                })
                .or_else(|| cfg.providers.first().cloned())
        };

        match provider {
            Some(p) => (p, model),
            None => return Err("No AI provider configured. Go to Settings → Engine to add an API key.".into()),
        }
    };

    // Store the user message
    let user_msg = StoredMessage {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.clone(),
        role: "user".into(),
        content: request.message.clone(),
        tool_calls_json: None,
        tool_call_id: None,
        name: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    state.store.add_message(&user_msg)?;

    // Load conversation history
    let system_prompt = request.system_prompt.or_else(|| {
        let cfg = state.config.lock().ok()?;
        cfg.default_system_prompt.clone()
    });

    // ── Lean session init: load only core soul files ───────────────
    // Only IDENTITY.md, SOUL.md, USER.md get auto-injected. All other
    // soul files (AGENTS.md, TOOLS.md, custom) remain accessible via
    // `soul_read` / `soul_list` tools on demand.
    let agent_id_owned = request.agent_id.clone().unwrap_or_else(|| "default".to_string());
    let core_context = state.store.compose_core_context(&agent_id_owned).unwrap_or(None);
    if let Some(ref cc) = core_context {
        info!("[engine] Core soul context loaded ({} chars) for agent '{}'", cc.len(), agent_id_owned);
    } else {
        info!("[engine] No core soul files found for agent '{}'", agent_id_owned);
    }

    // ── Today's memory notes (lightweight daily context) ───────────
    // Instead of auto-recall searching the full memory store on every
    // message, inject only memories created today. For deeper recall,
    // the agent calls `memory_search` explicitly.
    let todays_memories = state.store.get_todays_memories(&agent_id_owned).unwrap_or(None);
    if let Some(ref tm) = todays_memories {
        info!("[engine] Today's memory notes injected ({} chars)", tm.len());
    }

    // Auto-capture flag (still used at end of turn)
    let auto_capture_on = {
        let mcfg = state.memory_config.lock().ok();
        mcfg.as_ref().map(|c| c.auto_capture).unwrap_or(false)
    };

    // Collect skill instructions from enabled skills
    let skill_instructions = skills::get_enabled_skill_instructions(&state.store).unwrap_or_default();
    if !skill_instructions.is_empty() {
        info!("[engine] Skill instructions injected ({} chars)", skill_instructions.len());
    }

    // Build compact runtime context (model, session, time — all in one block)
    let runtime_context = {
        let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
        let provider_name = cfg.providers.iter()
            .find(|p| Some(p.id.clone()) == cfg.default_provider)
            .or_else(|| cfg.providers.first())
            .map(|p| format!("{} ({:?})", p.id, p.kind))
            .unwrap_or_else(|| "unknown".into());

        let user_tz = cfg.user_timezone.clone();
        let now_utc = chrono::Utc::now();
        let time_str = if let Ok(tz) = user_tz.parse::<chrono_tz::Tz>() {
            let local: chrono::DateTime<chrono_tz::Tz> = now_utc.with_timezone(&tz);
            format!("{} {} ({})", local.format("%Y-%m-%d %H:%M"), local.format("%A"), tz.name())
        } else {
            let local = chrono::Local::now();
            format!("{} {}", local.format("%Y-%m-%d %H:%M"), local.format("%A"))
        };

        let ws = tool_executor::agent_workspace(&agent_id_owned);

        format!(
            "## Runtime\n\
            Model: {} | Provider: {} | Session: {} | Agent: {}\n\
            Time: {}\n\
            Workspace: {}",
            model, provider_name, session_id, agent_id_owned,
            time_str,
            ws.display(),
        )
    };

    // Compose the full system prompt — lean init: base + runtime + core files + today's memories + skills
    let full_system_prompt = {
        let mut parts: Vec<String> = Vec::new();
        if let Some(sp) = &system_prompt {
            parts.push(sp.clone());
        }
        parts.push(runtime_context);

        // Soul file guidance
        let has_soul_files = core_context.is_some();
        let soul_hint = if has_soul_files {
            "Your core soul files (IDENTITY.md, SOUL.md, USER.md) are loaded below. \
            Use `soul_write` to update them. Use `soul_read` / `soul_list` to access other files (AGENTS.md, TOOLS.md, etc.) on demand."
        } else {
            "You have no soul files yet. Use `soul_write` to create IDENTITY.md (who you are), \
            SOUL.md (your personality), and USER.md (what you know about the user). \
            These persist across conversations and define your identity."
        };
        parts.push(format!(
            "## Soul Files\n{}\n\n\
            ## Memory\n\
            Use `memory_search` to recall past conversations, facts, and context. \
            Use `memory_store` to save important information for future sessions. \
            Your memory is NOT pre-loaded — search explicitly when you need historical context.",
            soul_hint,
        ));

        if let Some(cc) = &core_context {
            parts.push(cc.clone());
        }
        if let Some(tm) = &todays_memories {
            parts.push(tm.clone());
        }
        if !skill_instructions.is_empty() {
            parts.push(skill_instructions.clone());
        }
        if parts.is_empty() { None } else { Some(parts.join("\n\n---\n\n")) }
    };

    info!("[engine] System prompt: {} chars (core_ctx={}, today_mem={}, skills={})",
        full_system_prompt.as_ref().map(|s| s.len()).unwrap_or(0),
        core_context.is_some(), todays_memories.is_some(), !skill_instructions.is_empty());

    let mut messages = state.store.load_conversation(
        &session_id,
        full_system_prompt.as_deref(),
    )?;

    // If the user message has attachments, replace the last (user) message with
    // content blocks so the provider can see images and text files.
    if !request.attachments.is_empty() {
        info!("[engine] Processing {} attachment(s)", request.attachments.len());
        if let Some(last_msg) = messages.last_mut() {
            if last_msg.role == Role::User {
                let mut blocks = vec![ContentBlock::Text { text: request.message.clone() }];
                for att in &request.attachments {
                    let label = att.name.as_deref().unwrap_or("attachment");
                    info!("[engine] Attachment '{}' type={} size={}B", label, att.mime_type, att.content.len());
                    if att.mime_type.starts_with("image/") {
                        // Images → send as vision content blocks (native LLM vision)
                        let data_url = format!("data:{};base64,{}", att.mime_type, att.content);
                        blocks.push(ContentBlock::ImageUrl {
                            image_url: ImageUrlData {
                                url: data_url,
                                detail: Some("auto".into()),
                            },
                        });
                    } else if att.mime_type == "application/pdf" {
                        // PDFs → send as native document blocks (Claude, Gemini, OpenAI all support this)
                        blocks.push(ContentBlock::Document {
                            mime_type: att.mime_type.clone(),
                            data: att.content.clone(),
                            name: att.name.clone(),
                        });
                    } else {
                        // Text-based files → decode base64 to text and inline
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
        }
    }

    // Build tools
    let tools = if request.tools_enabled.unwrap_or(true) {
        let mut t = ToolDefinition::builtins();
        // Add tools for enabled skills
        let enabled_ids: Vec<String> = skills::builtin_skills().iter()
            .filter(|s| state.store.is_skill_enabled(&s.id).unwrap_or(false))
            .map(|s| s.id.clone())
            .collect();
        if !enabled_ids.is_empty() {
            info!("[engine] Adding skill tools for: {:?}", enabled_ids);
            t.extend(ToolDefinition::skill_tools(&enabled_ids));
        }
        // Auto-add telegram tools if bridge is configured (even without explicit skill enable)
        if !enabled_ids.contains(&"telegram".into()) {
            if let Ok(tg_cfg) = crate::engine::telegram::load_telegram_config(&app_handle) {
                if !tg_cfg.bot_token.is_empty() {
                    info!("[engine] Auto-adding telegram tools (bridge configured)");
                    t.push(ToolDefinition::telegram_send());
                    t.push(ToolDefinition::telegram_read());
                }
            }
        }
        // Apply per-agent tool filter (if provided by frontend policy)
        if let Some(ref filter) = request.tool_filter {
            let before = t.len();
            t.retain(|tool| filter.contains(&tool.function.name));
            info!("[engine] Tool policy filter applied: {} → {} tools (filter has {} entries)",
                before, t.len(), filter.len());
        }
        t
    } else {
        vec![]
    };

    // ── Response loop detection ─────────────────────────────────
    // If the last N assistant messages are near-identical (model stuck asking
    // the same question), inject a system nudge to break the cycle.
    {
        let assistant_msgs: Vec<&str> = messages.iter().rev()
            .filter(|m| m.role == Role::Assistant)
            .take(3)
            .map(|m| m.content.as_text_ref())
            .collect();
        if assistant_msgs.len() >= 2 {
            let a = assistant_msgs[0].to_lowercase();
            let b = assistant_msgs[1].to_lowercase();
            // Simple similarity: if the last two assistant messages share >80% of words, it's a loop
            let words_a: std::collections::HashSet<&str> = a.split_whitespace().collect();
            let words_b: std::collections::HashSet<&str> = b.split_whitespace().collect();
            let intersection = words_a.intersection(&words_b).count();
            let union = words_a.union(&words_b).count();
            let similarity = if union > 0 { intersection as f64 / union as f64 } else { 0.0 };
            if similarity > 0.8 {
                warn!("[engine] Response loop detected (similarity={:.0}%) — injecting redirect", similarity * 100.0);
                messages.push(Message {
                    role: Role::System,
                    content: MessageContent::Text(
                        "IMPORTANT: You have asked the user the same question multiple times and they have confirmed. \
                        Stop asking for confirmation. Take action immediately using your tools. \
                        If you need to create a file, use soul_write or file_write. \
                        If you don't remember what to do, use memory_search to find the context.".to_string()
                    ),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                });
            }
        }
    }

    // Get engine config values
    let (max_rounds, temperature) = {
        let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
        (cfg.max_tool_rounds, request.temperature)
    };

    let session_id_clone = session_id.clone();
    let run_id_clone = run_id.clone();
    let approvals = state.pending_approvals.clone();
    let tool_timeout = {
        let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
        cfg.tool_timeout_secs
    };
    let user_message_for_capture = request.message.clone();
    let _memory_cfg_for_capture = state.memory_config.lock().ok().map(|c| c.clone());

    // Track how many messages exist BEFORE the agent loop so we only store
    // NEW messages afterward (avoids re-inserting historical messages on every turn).
    let pre_loop_msg_count = messages.len();

    // Spawn the agent loop in a background task.
    // Chat always gets a semaphore permit (priority lane) — we use try_acquire first,
    // and if all slots are busy we still proceed (chat should never be blocked by cron).
    let app = app_handle.clone();
    let agent_id_for_spawn = agent_id_owned.clone();
    let sem = state.run_semaphore.clone();

    // Clone session_id and run_id for the panic-safety wrapper
    let panic_session_id = session_id.clone();
    let panic_run_id = run_id.clone();
    let panic_app = app_handle.clone();

    // Daily budget tracking
    let daily_budget = {
        let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
        cfg.daily_budget_usd
    };
    let daily_tokens = state.daily_tokens.clone();

    let handle = tauri::async_runtime::spawn(async move {
        // Acquire semaphore — chat gets priority so use a short timeout then proceed anyway
        let _permit = match tokio::time::timeout(
            std::time::Duration::from_secs(2),
            sem.acquire_owned()
        ).await {
            Ok(Ok(permit)) => Some(permit),
            _ => {
                info!("[engine] Chat bypassing concurrency limit (all slots busy)");
                None
            }
        };

        let provider = AnyProvider::from_config(&provider_config);

        match agent_loop::run_agent_turn(
            &app,
            &provider,
            &model,
            &mut messages,
            &tools,
            &session_id_clone,
            &run_id_clone,
            max_rounds,
            temperature,
            &approvals,
            tool_timeout,
            &agent_id_for_spawn,
            daily_budget,
            Some(&daily_tokens),
        ).await {
            Ok(final_text) => {
                info!("[engine] Agent turn complete: {} chars", final_text.len());

                // Store only NEW messages generated during this agent turn.
                // Messages before pre_loop_msg_count were loaded from DB and must not be re-inserted.
                if let Some(engine_state) = app.try_state::<EngineState>() {
                    for msg in messages.iter().skip(pre_loop_msg_count) {
                        if msg.role == Role::Assistant || msg.role == Role::Tool {
                            let stored = StoredMessage {
                                id: uuid::Uuid::new_v4().to_string(),
                                session_id: session_id_clone.clone(),
                                role: match msg.role {
                                    Role::Assistant => "assistant".into(),
                                    Role::Tool => "tool".into(),
                                    _ => "user".into(),
                                },
                                content: msg.content.as_text(),
                                tool_calls_json: msg.tool_calls.as_ref()
                                    .map(|tc| serde_json::to_string(tc).unwrap_or_default()),
                                tool_call_id: msg.tool_call_id.clone(),
                                name: msg.name.clone(),
                                created_at: chrono::Utc::now().to_rfc3339(),
                            };
                            if let Err(e) = engine_state.store.add_message(&stored) {
                                error!("[engine] Failed to store message: {}", e);
                            }
                        }
                    }

                    // Auto-capture: extract memorable facts and store them
                    if auto_capture_on && !final_text.is_empty() {
                        let facts = memory::extract_memorable_facts(&user_message_for_capture, &final_text);
                        if !facts.is_empty() {
                            let emb_client = engine_state.embedding_client();
                            for (content, category) in &facts {
                                match memory::store_memory(
                                    &engine_state.store, content, category, 5, emb_client.as_ref(), Some(&agent_id_for_spawn)
                                ).await {
                                    Ok(id) => info!("[engine] Auto-captured memory: {}", crate::engine::types::truncate_utf8(&id, 8)),
                                    Err(e) => warn!("[engine] Auto-capture failed: {}", e),
                                }
                            }
                        }
                    }

                    // Session-end summary: store a compact memory of what was worked on.
                    // This powers the "Today's Memory Notes" injection in future sessions.
                    // Skip trivial responses: short text-only replies (e.g. "should I do X?")
                    // pollute memory and can cause feedback loops when re-injected.
                    let had_tool_calls = messages.iter().skip(pre_loop_msg_count)
                        .any(|m| m.role == Role::Tool || m.tool_calls.as_ref().map(|tc| !tc.is_empty()).unwrap_or(false));
                    let is_substantial = final_text.len() > 200 || had_tool_calls;
                    if is_substantial && !final_text.is_empty() {
                        let summary = if final_text.len() > 300 {
                            format!("{}…", &final_text[..300])
                        } else {
                            final_text.clone()
                        };
                        let session_summary = format!(
                            "Session work: User asked: \"{}\". Agent responded: {}",
                            crate::engine::types::truncate_utf8(&user_message_for_capture, 150),
                            summary,
                        );
                        let emb_client = engine_state.embedding_client();
                        match memory::store_memory(
                            &engine_state.store, &session_summary, "session", 3, emb_client.as_ref(), Some(&agent_id_for_spawn)
                        ).await {
                            Ok(_) => info!("[engine] Session summary stored ({} chars)", session_summary.len()),
                            Err(e) => warn!("[engine] Session summary store failed: {}", e),
                        }
                    }
                }
            }
            Err(e) => {
                error!("[engine] Agent turn failed: {}", e);
                let _ = app.emit("engine-event", EngineEvent::Error {
                    session_id: session_id_clone,
                    run_id: run_id_clone,
                    message: e,
                });
            }
        }
    });

    // ── Panic safety: if the spawned task panics (e.g. from a bug in tool
    // execution), the frontend would hang forever because no Complete or Error
    // event ever fires. Spawn a monitor that awaits the JoinHandle and emits
    // an Error event if the task panicked.
    tauri::async_runtime::spawn(async move {
        if let Err(join_err) = handle.await {
            let msg = format!("Internal error: agent task crashed — {}", join_err);
            error!("[engine] {}", msg);
            let _ = panic_app.emit("engine-event", EngineEvent::Error {
                session_id: panic_session_id,
                run_id: panic_run_id,
                message: msg,
            });
        }
    });

    Ok(ChatResponse {
        run_id,
        session_id,
    })
}

/// Get chat history for a session.
#[tauri::command]
pub fn engine_chat_history(
    state: State<'_, EngineState>,
    session_id: String,
    limit: Option<i64>,
) -> Result<Vec<StoredMessage>, String> {
    state.store.get_messages(&session_id, limit.unwrap_or(200))
}

// ── Session commands ───────────────────────────────────────────────────

#[tauri::command]
pub fn engine_sessions_list(
    state: State<'_, EngineState>,
    limit: Option<i64>,
    agent_id: Option<String>,
) -> Result<Vec<Session>, String> {
    state.store.list_sessions_filtered(limit.unwrap_or(50), agent_id.as_deref())
}

#[tauri::command]
pub fn engine_session_rename(
    state: State<'_, EngineState>,
    session_id: String,
    label: String,
) -> Result<(), String> {
    state.store.rename_session(&session_id, &label)
}

#[tauri::command]
pub fn engine_session_delete(
    state: State<'_, EngineState>,
    session_id: String,
) -> Result<(), String> {
    state.store.delete_session(&session_id)
}

#[tauri::command]
pub fn engine_session_clear(
    state: State<'_, EngineState>,
    session_id: String,
) -> Result<(), String> {
    info!("[engine] Clearing messages for session {}", session_id);
    state.store.clear_messages(&session_id)
}

#[tauri::command]
pub async fn engine_session_compact(
    state: State<'_, EngineState>,
    session_id: String,
) -> Result<crate::engine::compaction::CompactionResult, String> {
    info!("[engine] Manual compaction requested for session {}", session_id);

    // Resolve provider and model from config
    let (provider_config, model) = {
        let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
        let model = cfg.default_model.clone().unwrap_or_else(|| "gpt-4o".to_string());
        let provider = cfg.default_provider.as_ref()
            .and_then(|dp| cfg.providers.iter().find(|p| p.id == *dp).cloned())
            .or_else(|| cfg.providers.first().cloned())
            .ok_or("No AI provider configured.")?;
        (provider, model)
    };

    let provider = crate::engine::providers::AnyProvider::from_config(&provider_config);
    let compact_config = crate::engine::compaction::CompactionConfig::default();

    // Wrap the existing store in an Arc for the async call
    // Note: we use a new SessionStore connection since the state's store is behind a State ref
    let store_arc = std::sync::Arc::new(
        crate::engine::sessions::SessionStore::open()?
    );

    crate::engine::compaction::compact_session(
        &store_arc,
        &provider,
        &model,
        &session_id,
        &compact_config,
    ).await
}

// ── Sandbox commands ───────────────────────────────────────────────────

#[tauri::command]
pub async fn engine_sandbox_check() -> Result<bool, String> {
    Ok(crate::engine::sandbox::is_docker_available().await)
}

#[tauri::command]
pub fn engine_sandbox_get_config(
    state: State<'_, EngineState>,
) -> Result<crate::engine::sandbox::SandboxConfig, String> {
    Ok(crate::engine::sandbox::load_sandbox_config(&state.store))
}

#[tauri::command]
pub fn engine_sandbox_set_config(
    state: State<'_, EngineState>,
    config: crate::engine::sandbox::SandboxConfig,
) -> Result<(), String> {
    crate::engine::sandbox::save_sandbox_config(&state.store, &config)
}

// ── Engine configuration commands ──────────────────────────────────────

#[tauri::command]
pub fn engine_get_config(
    state: State<'_, EngineState>,
) -> Result<EngineConfig, String> {
    let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
    Ok(cfg.clone())
}

/// Get the current daily token spend and budget status.
#[tauri::command]
pub fn engine_get_daily_spend(
    state: State<'_, EngineState>,
) -> Result<serde_json::Value, String> {
    let (input_tokens, output_tokens, estimated_usd) = state.daily_tokens.estimated_spend_usd();
    let cache_read = state.daily_tokens.cache_read_tokens.load(Ordering::Relaxed);
    let cache_create = state.daily_tokens.cache_create_tokens.load(Ordering::Relaxed);
    let budget = {
        let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
        cfg.daily_budget_usd
    };
    let budget_pct = if budget > 0.0 { (estimated_usd / budget * 100.0).min(100.0) } else { 0.0 };
    Ok(serde_json::json!({
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "cache_read_tokens": cache_read,
        "cache_create_tokens": cache_create,
        "estimated_usd": format!("{:.2}", estimated_usd),
        "budget_usd": budget,
        "budget_pct": format!("{:.0}", budget_pct),
        "over_budget": budget > 0.0 && estimated_usd >= budget,
    }))
}

#[tauri::command]
pub fn engine_set_config(
    state: State<'_, EngineState>,
    config: EngineConfig,
) -> Result<(), String> {
    let json = serde_json::to_string(&config)
        .map_err(|e| format!("Serialize error: {}", e))?;

    // Persist to DB
    state.store.set_config("engine_config", &json)?;

    // Update in-memory config
    let mut cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
    *cfg = config;

    info!("[engine] Config updated, {} providers configured", cfg.providers.len());
    Ok(())
}

/// Add or update a single provider without replacing the entire config.
#[tauri::command]
pub fn engine_upsert_provider(
    state: State<'_, EngineState>,
    provider: ProviderConfig,
) -> Result<(), String> {
    let mut cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;

    // Update existing or add new
    if let Some(existing) = cfg.providers.iter_mut().find(|p| p.id == provider.id) {
        *existing = provider;
    } else {
        cfg.providers.push(provider);
    }

    // Set as default if it's the first provider
    if cfg.default_provider.is_none() && !cfg.providers.is_empty() {
        cfg.default_provider = Some(cfg.providers[0].id.clone());
    }

    // Persist
    let json = serde_json::to_string(&*cfg)
        .map_err(|e| format!("Serialize error: {}", e))?;
    state.store.set_config("engine_config", &json)?;

    info!("[engine] Provider upserted, {} total providers", cfg.providers.len());
    Ok(())
}

/// Remove a provider by ID.
#[tauri::command]
pub fn engine_remove_provider(
    state: State<'_, EngineState>,
    provider_id: String,
) -> Result<(), String> {
    let mut cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;

    cfg.providers.retain(|p| p.id != provider_id);

    // Clear default if it was the removed provider
    if cfg.default_provider.as_deref() == Some(&provider_id) {
        cfg.default_provider = cfg.providers.first().map(|p| p.id.clone());
    }

    let json = serde_json::to_string(&*cfg)
        .map_err(|e| format!("Serialize error: {}", e))?;
    state.store.set_config("engine_config", &json)?;

    info!("[engine] Provider removed, {} remaining", cfg.providers.len());
    Ok(())
}

/// Check if the engine is configured and ready to use.
#[tauri::command]
pub fn engine_status(
    state: State<'_, EngineState>,
) -> Result<serde_json::Value, String> {
    let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;

    let has_providers = !cfg.providers.is_empty();
    let has_api_key = cfg.providers.iter().any(|p| !p.api_key.is_empty());

    Ok(serde_json::json!({
        "ready": has_providers && has_api_key,
        "providers": cfg.providers.len(),
        "has_api_key": has_api_key,
        "default_model": cfg.default_model,
        "default_provider": cfg.default_provider,
    }))
}

/// Auto-setup: detect Ollama on first run and add it as a provider.
/// Returns what was done so the frontend can show a toast.
#[tauri::command]
pub async fn engine_auto_setup(
    state: State<'_, EngineState>,
) -> Result<serde_json::Value, String> {
    // Only run if no providers are configured yet
    {
        let cfg = state.config.lock().map_err(|e| format!("Lock: {}", e))?;
        if !cfg.providers.is_empty() {
            return Ok(serde_json::json!({ "action": "none", "reason": "providers_exist" }));
        }
    }

    info!("[engine] First run — no providers configured, attempting Ollama auto-detect");

    // Try to reach Ollama
    let base_url = "http://localhost:11434";
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    // Check if Ollama is reachable
    let ollama_up = match client.get(format!("{}/api/tags", base_url)).send().await {
        Ok(resp) if resp.status().is_success() => true,
        _ => {
            // Try to start Ollama if the binary exists
            if let Ok(_child) = std::process::Command::new("ollama").arg("serve").spawn() {
                info!("[engine] Attempting to auto-start Ollama...");
                // Wait for it to come up
                let mut up = false;
                for _ in 0..10 {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    if client.get(format!("{}/api/tags", base_url)).send().await.is_ok() {
                        up = true;
                        break;
                    }
                }
                up
            } else {
                false
            }
        }
    };

    if !ollama_up {
        info!("[engine] Ollama not found — user will need to configure a provider manually");
        return Ok(serde_json::json!({
            "action": "none",
            "reason": "ollama_not_found",
            "message": "No AI provider configured. Install Ollama from ollama.ai for free local AI, or add an API key in Settings → Engine."
        }));
    }

    // Ollama is up — check what models are available
    let models: Vec<String> = match client.get(format!("{}/api/tags", base_url)).send().await {
        Ok(resp) => {
            if let Ok(data) = resp.json::<serde_json::Value>().await {
                data["models"].as_array()
                    .map(|arr| arr.iter().filter_map(|m| m["name"].as_str().map(String::from)).collect())
                    .unwrap_or_default()
            } else {
                vec![]
            }
        }
        _ => vec![],
    };

    // Pick the best available model, or pull a small one
    let preferred = ["llama3.2:3b", "llama3.2:1b", "llama3.1:8b", "llama3:8b", "mistral:7b", "gemma2:2b", "phi3:mini", "qwen2.5:3b"];
    let chosen_model = models.iter()
        .find(|m| preferred.iter().any(|p| m.starts_with(p.split(':').next().unwrap_or(""))))
        .cloned()
        .or_else(|| models.first().cloned());

    let model_name = if let Some(m) = chosen_model {
        m
    } else {
        // No models at all — pull a small one
        info!("[engine] Ollama has no models — pulling llama3.2:3b");
        let pull_body = serde_json::json!({ "name": "llama3.2:3b", "stream": false });
        match client.post(format!("{}/api/pull", base_url))
            .json(&pull_body)
            .timeout(std::time::Duration::from_secs(300))
            .send().await
        {
            Ok(resp) if resp.status().is_success() => {
                info!("[engine] Successfully pulled llama3.2:3b");
                "llama3.2:3b".to_string()
            }
            Ok(resp) => {
                let status = resp.status();
                warn!("[engine] Model pull returned {}", status);
                "llama3.2:3b".to_string() // set it anyway, user can fix
            }
            Err(e) => {
                warn!("[engine] Model pull failed: {}", e);
                "llama3.2:3b".to_string()
            }
        }
    };

    // Add Ollama as a provider
    let provider = ProviderConfig {
        id: "ollama".to_string(),
        kind: ProviderKind::Ollama,
        api_key: String::new(),
        base_url: Some(base_url.to_string()),
        default_model: Some(model_name.clone()),
    };

    {
        let mut cfg = state.config.lock().map_err(|e| format!("Lock: {}", e))?;
        cfg.providers.push(provider);
        cfg.default_provider = Some("ollama".to_string());
        cfg.default_model = Some(model_name.clone());

        let json = serde_json::to_string(&*cfg)
            .map_err(|e| format!("Serialize: {}", e))?;
        state.store.set_config("engine_config", &json)?;
    }

    info!("[engine] Auto-setup complete: Ollama added as default provider with model '{}'", model_name);

    Ok(serde_json::json!({
        "action": "ollama_added",
        "model": model_name,
        "available_models": models,
        "message": format!("Ollama detected! Set up with model '{}' — ready to chat.", model_name)
    }))
}

/// Resolve a pending tool approval from the frontend.
/// Called by the approval modal when the user clicks Allow or Deny.
#[tauri::command]
pub fn engine_approve_tool(
    state: State<'_, EngineState>,
    tool_call_id: String,
    approved: bool,
) -> Result<(), String> {
    let mut map = state.pending_approvals.lock()
        .map_err(|e| format!("Lock error: {}", e))?;

    if let Some(sender) = map.remove(&tool_call_id) {
        info!("[engine] Tool approval resolved: {} → {}", tool_call_id, if approved { "ALLOWED" } else { "DENIED" });
        let _ = sender.send(approved);
        Ok(())
    } else {
        warn!("[engine] No pending approval found for tool_call_id={}", tool_call_id);
        Err(format!("No pending approval for {}", tool_call_id))
    }
}

// ── Agent Files (Soul / Persona) commands ──────────────────────────────

#[tauri::command]
pub fn engine_agent_file_list(
    state: State<'_, EngineState>,
    agent_id: Option<String>,
) -> Result<Vec<AgentFile>, String> {
    let aid = agent_id.unwrap_or_else(|| "default".into());
    state.store.list_agent_files(&aid)
}

#[tauri::command]
pub fn engine_agent_file_get(
    state: State<'_, EngineState>,
    agent_id: Option<String>,
    file_name: String,
) -> Result<Option<AgentFile>, String> {
    let aid = agent_id.unwrap_or_else(|| "default".into());
    state.store.get_agent_file(&aid, &file_name)
}

#[tauri::command]
pub fn engine_agent_file_set(
    state: State<'_, EngineState>,
    agent_id: Option<String>,
    file_name: String,
    content: String,
) -> Result<(), String> {
    let aid = agent_id.unwrap_or_else(|| "default".into());
    info!("[engine] Setting agent file {}/{} ({} bytes)", aid, file_name, content.len());
    state.store.set_agent_file(&aid, &file_name, &content)
}

#[tauri::command]
pub fn engine_agent_file_delete(
    state: State<'_, EngineState>,
    agent_id: Option<String>,
    file_name: String,
) -> Result<(), String> {
    let aid = agent_id.unwrap_or_else(|| "default".into());
    state.store.delete_agent_file(&aid, &file_name)
}

// ── Memory commands ────────────────────────────────────────────────────

#[tauri::command]
pub async fn engine_memory_store(
    state: State<'_, EngineState>,
    content: String,
    category: Option<String>,
    importance: Option<u8>,
    agent_id: Option<String>,
) -> Result<String, String> {
    let cat = category.unwrap_or_else(|| "general".into());
    let imp = importance.unwrap_or(5);
    let emb_client = state.embedding_client();
    memory::store_memory(&state.store, &content, &cat, imp, emb_client.as_ref(), agent_id.as_deref()).await
}

#[tauri::command]
pub async fn engine_memory_search(
    state: State<'_, EngineState>,
    query: String,
    limit: Option<usize>,
    agent_id: Option<String>,
) -> Result<Vec<Memory>, String> {
    let lim = limit.unwrap_or(10);
    let threshold = {
        let mcfg = state.memory_config.lock().ok();
        mcfg.map(|c| c.recall_threshold).unwrap_or(0.3)
    };
    let emb_client = state.embedding_client();
    memory::search_memories(&state.store, &query, lim, threshold, emb_client.as_ref(), agent_id.as_deref()).await
}

#[tauri::command]
pub fn engine_memory_stats(
    state: State<'_, EngineState>,
) -> Result<MemoryStats, String> {
    state.store.memory_stats()
}

#[tauri::command]
pub fn engine_memory_delete(
    state: State<'_, EngineState>,
    id: String,
) -> Result<(), String> {
    state.store.delete_memory(&id)
}

#[tauri::command]
pub fn engine_memory_list(
    state: State<'_, EngineState>,
    limit: Option<usize>,
) -> Result<Vec<Memory>, String> {
    state.store.list_memories(limit.unwrap_or(100))
}

#[tauri::command]
pub fn engine_get_memory_config(
    state: State<'_, EngineState>,
) -> Result<MemoryConfig, String> {
    let cfg = state.memory_config.lock().map_err(|e| format!("Lock error: {}", e))?;
    Ok(cfg.clone())
}

#[tauri::command]
pub fn engine_set_memory_config(
    state: State<'_, EngineState>,
    config: MemoryConfig,
) -> Result<(), String> {
    let json = serde_json::to_string(&config)
        .map_err(|e| format!("Serialize error: {}", e))?;
    state.store.set_config("memory_config", &json)?;
    let mut cfg = state.memory_config.lock().map_err(|e| format!("Lock error: {}", e))?;
    *cfg = config;
    info!("[engine] Memory config updated");
    Ok(())
}

#[tauri::command]
pub async fn engine_test_embedding(
    state: State<'_, EngineState>,
) -> Result<usize, String> {
    let client = state.embedding_client()
        .ok_or_else(|| "No embedding configuration — set base URL and model in memory settings".to_string())?;
    let dims = client.test_connection().await?;
    info!("[engine] Embedding test passed: {} dimensions", dims);
    Ok(dims)
}

/// Check Ollama status and model availability.
/// Returns { ollama_running: bool, model_available: bool, model_name: String }
#[tauri::command]
pub async fn engine_embedding_status(
    state: State<'_, EngineState>,
) -> Result<serde_json::Value, String> {
    let client = match state.embedding_client() {
        Some(c) => c,
        None => return Ok(serde_json::json!({
            "ollama_running": false,
            "model_available": false,
            "model_name": "",
            "error": "No embedding configuration"
        })),
    };

    let model_name = {
        let cfg = state.memory_config.lock().map_err(|e| format!("Lock error: {}", e))?;
        cfg.embedding_model.clone()
    };

    let ollama_running = client.check_ollama_running().await.unwrap_or(false);
    let model_available = if ollama_running {
        client.check_model_available().await.unwrap_or(false)
    } else {
        false
    };

    Ok(serde_json::json!({
        "ollama_running": ollama_running,
        "model_available": model_available,
        "model_name": model_name,
    }))
}

/// Pull the embedding model from Ollama.
#[tauri::command]
pub async fn engine_embedding_pull_model(
    state: State<'_, EngineState>,
) -> Result<String, String> {
    let client = state.embedding_client()
        .ok_or_else(|| "No embedding configuration".to_string())?;

    // Check Ollama running first
    let running = client.check_ollama_running().await.unwrap_or(false);
    if !running {
        return Err("Ollama is not running. Start Ollama first, then try again.".into());
    }

    // Check if already available
    if client.check_model_available().await.unwrap_or(false) {
        return Ok("Model already available".into());
    }

    // Pull the model (blocking)
    client.pull_model().await?;
    Ok("Model pulled successfully".into())
}

/// Ensure Ollama is running and the embedding model is available.
/// This is the "just works" function — automatically starts Ollama if needed
/// and pulls the embedding model if it's not present.
#[tauri::command]
pub async fn engine_ensure_embedding_ready(
    state: State<'_, EngineState>,
) -> Result<memory::OllamaReadyStatus, String> {
    let config = {
        let cfg = state.memory_config.lock().map_err(|e| format!("Lock error: {}", e))?;
        cfg.clone()
    };

    let status = memory::ensure_ollama_ready(&config).await;

    // If we discovered the actual dimensions, update the config
    if status.embedding_dims > 0 {
        let mut cfg = state.memory_config.lock().map_err(|e| format!("Lock error: {}", e))?;
        if cfg.embedding_dims != status.embedding_dims {
            info!("[engine] Updating embedding_dims from {} to {} based on actual model output", cfg.embedding_dims, status.embedding_dims);
            cfg.embedding_dims = status.embedding_dims;
            // Save to DB
            if let Ok(json) = serde_json::to_string(&*cfg) {
                let _ = state.store.set_config("memory_config", &json);
            }
        }
    }

    // If we auto-pulled the model, backfill any existing memories that lack embeddings
    if status.was_auto_pulled && status.error.is_none() {
        if let Some(client) = state.embedding_client() {
            let _ = memory::backfill_embeddings(&state.store, &client).await;
        }
    }

    Ok(status)
}

/// Backfill embeddings for memories that don't have them.
#[tauri::command]
pub async fn engine_memory_backfill(
    state: State<'_, EngineState>,
) -> Result<serde_json::Value, String> {
    let client = state.embedding_client()
        .ok_or_else(|| "No embedding configuration — Ollama must be running with an embedding model".to_string())?;

    let (success, fail) = memory::backfill_embeddings(&state.store, &client).await?;
    Ok(serde_json::json!({
        "success": success,
        "failed": fail,
    }))
}

// ── Skill Vault commands ───────────────────────────────────────────────

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

// ── Trading commands ───────────────────────────────────────────────────

#[tauri::command]
pub fn engine_trading_history(
    state: State<'_, EngineState>,
    limit: Option<u32>,
) -> Result<Vec<serde_json::Value>, String> {
    state.store.list_trades(limit.unwrap_or(100))
}

#[tauri::command]
pub fn engine_trading_summary(
    state: State<'_, EngineState>,
) -> Result<serde_json::Value, String> {
    state.store.daily_trade_summary()
}

#[tauri::command]
pub fn engine_trading_policy_get(
    state: State<'_, EngineState>,
) -> Result<TradingPolicy, String> {
    match state.store.get_config("trading_policy") {
        Ok(Some(json)) => {
            serde_json::from_str(&json).map_err(|e| format!("Parse error: {}", e))
        }
        Ok(None) => Ok(TradingPolicy::default()),
        Err(e) => Err(e),
    }
}

#[tauri::command]
pub fn engine_trading_policy_set(
    state: State<'_, EngineState>,
    policy: TradingPolicy,
) -> Result<(), String> {
    info!("[engine] Updating trading policy: auto_approve={}, max_trade=${}, max_daily=${}, pairs={:?}, transfers={}",
        policy.auto_approve, policy.max_trade_usd, policy.max_daily_loss_usd,
        policy.allowed_pairs, policy.allow_transfers);
    let json = serde_json::to_string(&policy).map_err(|e| format!("Serialize error: {}", e))?;
    state.store.set_config("trading_policy", &json)
}

// ── Position commands (Stop-Loss / Take-Profit) ────────────────────────

#[tauri::command]
pub fn engine_positions_list(
    state: State<'_, EngineState>,
    status: Option<String>,
) -> Result<Vec<Position>, String> {
    state.store.list_positions(status.as_deref())
}

#[tauri::command]
pub fn engine_position_close(
    state: State<'_, EngineState>,
    id: String,
) -> Result<(), String> {
    info!("[engine] Manually closing position {}", id);
    state.store.close_position(&id, "closed_manual", None)
}

#[tauri::command]
pub fn engine_position_update_targets(
    state: State<'_, EngineState>,
    id: String,
    stop_loss_pct: f64,
    take_profit_pct: f64,
) -> Result<(), String> {
    info!("[engine] Updating position {} targets: SL={:.0}%, TP={:.1}x", id, stop_loss_pct * 100.0, take_profit_pct);
    state.store.update_position_targets(&id, stop_loss_pct, take_profit_pct)
}

// ── Task commands ──────────────────────────────────────────────────────

#[tauri::command]
pub fn engine_tasks_list(
    state: State<'_, EngineState>,
) -> Result<Vec<Task>, String> {
    state.store.list_tasks()
}

#[tauri::command]
pub fn engine_task_create(
    state: State<'_, EngineState>,
    task: Task,
) -> Result<(), String> {
    info!("[engine] Creating task: {} ({})", task.title, task.id);
    state.store.create_task(&task)?;
    // Log activity
    let aid = uuid::Uuid::new_v4().to_string();
    state.store.add_task_activity(&aid, &task.id, "created", None, &format!("Task created: {}", task.title))?;
    Ok(())
}

#[tauri::command]
pub fn engine_task_update(
    state: State<'_, EngineState>,
    task: Task,
) -> Result<(), String> {
    info!("[engine] Updating task: {} status={}", task.id, task.status);
    state.store.update_task(&task)
}

#[tauri::command]
pub fn engine_task_delete(
    state: State<'_, EngineState>,
    task_id: String,
) -> Result<(), String> {
    info!("[engine] Deleting task: {}", task_id);
    state.store.delete_task(&task_id)
}

#[tauri::command]
pub fn engine_task_move(
    state: State<'_, EngineState>,
    task_id: String,
    new_status: String,
) -> Result<(), String> {
    info!("[engine] Moving task {} → {}", task_id, new_status);
    // Load, update status, save
    let tasks = state.store.list_tasks()?;
    if let Some(mut task) = tasks.into_iter().find(|t| t.id == task_id) {
        let old_status = task.status.clone();
        task.status = new_status.clone();
        state.store.update_task(&task)?;
        // Log activity
        let aid = uuid::Uuid::new_v4().to_string();
        state.store.add_task_activity(
            &aid, &task_id, "status_change", None,
            &format!("Moved from {} to {}", old_status, new_status),
        )?;
        Ok(())
    } else {
        Err(format!("Task not found: {}", task_id))
    }
}

#[tauri::command]
pub fn engine_task_activity(
    state: State<'_, EngineState>,
    task_id: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<TaskActivity>, String> {
    let limit = limit.unwrap_or(50);
    match task_id {
        Some(id) => state.store.list_task_activity(&id, limit),
        None => state.store.list_all_activity(limit),
    }
}

/// Set agents assigned to a task (multi-agent support).
#[tauri::command]
pub fn engine_task_set_agents(
    state: State<'_, EngineState>,
    task_id: String,
    agents: Vec<TaskAgent>,
) -> Result<(), String> {
    info!("[engine] Setting {} agent(s) for task {}", agents.len(), task_id);
    state.store.set_task_agents(&task_id, &agents)?;

    // Log activity
    let agent_names: Vec<&str> = agents.iter().map(|a| a.agent_id.as_str()).collect();
    let aid = uuid::Uuid::new_v4().to_string();
    state.store.add_task_activity(
        &aid, &task_id, "assigned", None,
        &format!("Agents assigned: {}", agent_names.join(", ")),
    )?;

    Ok(())
}

/// Run a task: send it to agents and stream the results.
/// Multi-agent: spawns parallel agent loops for all assigned agents.
/// Each agent gets its own session (`eng-task-{task_id}-{agent_id}`)
/// and its own soul context.
#[tauri::command]
pub async fn engine_task_run(
    app_handle: tauri::AppHandle,
    state: State<'_, EngineState>,
    task_id: String,
) -> Result<String, String> {
    // Delegate to the standalone function (usable from heartbeat too)
    execute_task(&app_handle, &state, &task_id).await
}

/// Standalone task execution — callable from both the Tauri command and the
/// background cron heartbeat. This is the core logic for running a task.
pub async fn execute_task(
    app_handle: &tauri::AppHandle,
    state: &EngineState,
    task_id: &str,
) -> Result<String, String> {
    // ── Dedup guard: skip if this task is already running ──
    {
        let mut inflight = state.inflight_tasks.lock().map_err(|e| format!("Lock: {}", e))?;
        if inflight.contains(task_id) {
            info!("[engine] Task '{}' already in flight — skipping duplicate", task_id);
            return Err(format!("Task {} is already running", task_id));
        }
        inflight.insert(task_id.to_string());
    }

    let run_id = uuid::Uuid::new_v4().to_string();

    // Load the task
    let tasks = state.store.list_tasks()?;
    let task = tasks.into_iter().find(|t| t.id == task_id)
        .ok_or_else(|| format!("Task not found: {}", task_id))?;

    // Determine which agents to run
    // Multi-agent: use task_agents table; fallback to legacy assigned_agent
    let agent_ids: Vec<String> = if !task.assigned_agents.is_empty() {
        task.assigned_agents.iter().map(|a| a.agent_id.clone()).collect()
    } else if let Some(ref agent) = task.assigned_agent {
        vec![agent.clone()]
    } else {
        vec!["default".to_string()]
    };

    info!("[engine] Running task '{}' with {} agent(s): {:?}", task.title, agent_ids.len(), agent_ids);

    // Update task status to in_progress
    {
        let mut t = task.clone();
        t.status = "in_progress".to_string();
        state.store.update_task(&t)?;
    }

    // Log activity for each agent
    for agent_id in &agent_ids {
        let aid = uuid::Uuid::new_v4().to_string();
        state.store.add_task_activity(
            &aid, task_id, "agent_started", Some(agent_id),
            &format!("Agent {} started working on: {}", agent_id, task.title),
        )?;
    }

    // Compose task prompt
    let task_prompt = if task.description.is_empty() {
        task.title.clone()
    } else {
        format!("{}\n\n{}", task.title, task.description)
    };

    // Get global config values
    let (base_system_prompt, max_rounds, tool_timeout, model_routing, default_model) = {
        let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
        (
            cfg.default_system_prompt.clone(),
            cfg.max_tool_rounds,
            cfg.tool_timeout_secs,
            cfg.model_routing.clone(),
            cfg.default_model.clone().unwrap_or_else(|| "gpt-4o".to_string()),
        )
    };

    // Get skill instructions (shared across agents)
    let skill_instructions = skills::get_enabled_skill_instructions(&state.store).unwrap_or_default();

    // Build tools (shared across agents)
    let mut all_tools = ToolDefinition::builtins();
    let enabled_ids: Vec<String> = skills::builtin_skills().iter()
        .filter(|s| state.store.is_skill_enabled(&s.id).unwrap_or(false))
        .map(|s| s.id.clone())
        .collect();
    if !enabled_ids.is_empty() {
        all_tools.extend(ToolDefinition::skill_tools(&enabled_ids));
    }
    // Auto-add telegram tools if bridge is configured
    if !enabled_ids.contains(&"telegram".into()) {
        if let Ok(tg_cfg) = crate::engine::telegram::load_telegram_config(app_handle) {
            if !tg_cfg.bot_token.is_empty() {
                all_tools.push(ToolDefinition::telegram_send());
                all_tools.push(ToolDefinition::telegram_read());
            }
        }
    }

    let pending = state.pending_approvals.clone();
    let store_path = crate::engine::sessions::engine_db_path();
    let task_id_for_spawn = task_id.to_string();
    let agent_count = agent_ids.len();
    let is_recurring = task.cron_schedule.as_ref().map_or(false, |s| !s.is_empty());
    let sem = state.run_semaphore.clone();
    let inflight = state.inflight_tasks.clone();

    // Daily budget tracking for tasks
    let task_daily_budget = {
        let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
        cfg.daily_budget_usd
    };
    let task_daily_tokens = state.daily_tokens.clone();

    // ── Cost control constants for cron/background tasks ──
    // Cron sessions reuse the same session_id across runs, causing context to
    // grow unboundedly (up to 500 messages / 100k tokens). This is the #1
    // driver of runaway API costs. We prune old messages before each run and
    // cap the number of tool rounds to keep costs predictable.
    const CRON_SESSION_KEEP_MESSAGES: i64 = 20;   // keep ~2-3 runs of context
    const CRON_MAX_TOOL_ROUNDS: u32 = 10;         // prevent runaway tool loops

    // For each agent, spawn a parallel agent loop
    let mut handles = Vec::new();

    for agent_id in agent_ids.clone() {
        // Per-agent session: eng-task-{task_id}-{agent_id}
        let session_id = format!("eng-task-{}-{}", task.id, agent_id);

        // ── Cron cost control: prune accumulated history ──
        // Recurring cron tasks reuse the same session, so context grows every
        // run. Prune to the last CRON_SESSION_KEEP_MESSAGES messages before
        // adding the new user prompt, keeping costs bounded.
        if is_recurring {
            match state.store.prune_session_messages(&session_id, CRON_SESSION_KEEP_MESSAGES) {
                Ok(pruned) if pruned > 0 => {
                    info!("[engine] Pruned {} old messages from cron session {} (kept {})",
                        pruned, session_id, CRON_SESSION_KEEP_MESSAGES);
                }
                Err(e) => {
                    warn!("[engine] Failed to prune cron session {}: {}", session_id, e);
                }
                _ => {}
            }
        }

        // ── Per-task / per-agent model routing ──
        // Priority: task.model (highest) > agent_models > worker_model > default_model
        // This lets you assign different models to different tasks,
        // cheap/fast models to cron agents, and keep the best for chat.
        let agent_model = if let Some(ref task_model) = task.model {
            if !task_model.is_empty() {
                let normalized = normalize_model_name(task_model).to_string();
                if normalized != *task_model {
                    info!("[engine] Task '{}' model remapped: {} → {}", task.title, task_model, normalized);
                }
                info!("[engine] Task '{}' has explicit model override: {}", task.title, normalized);
                normalized
            } else {
                model_routing.resolve(&agent_id, "worker", "", &default_model)
            }
        } else {
            model_routing.resolve(&agent_id, "worker", "", &default_model)
        };
        info!("[engine] Agent '{}' resolved model: {} (task_override: {:?}, default: {})", agent_id, agent_model, task.model, default_model);

        // Create session if needed
        let (provider_config, model) = {
            let cfg = state.config.lock().map_err(|e| format!("Lock error: {}", e))?;
            let model = agent_model;
            let provider = resolve_provider_for_model(&model, &cfg.providers)
                .or_else(|| {
                    cfg.providers.iter().find(|p| p.default_model.as_deref() == Some(model.as_str())).cloned()
                })
                .or_else(|| {
                    cfg.default_provider.as_ref()
                        .and_then(|dp| cfg.providers.iter().find(|p| p.id == *dp).cloned())
                })
                .or_else(|| cfg.providers.first().cloned());
            match provider {
                Some(p) => (p, model),
                None => return Err("No AI provider configured".into()),
            }
        };

        if state.store.get_session(&session_id).ok().flatten().is_none() {
            state.store.create_session(&session_id, &model, None, Some(&agent_id))?;
        }

        // Compose system prompt with agent-specific soul context (core files only)
        let agent_context = state.store.compose_core_context(&agent_id).unwrap_or(None);

        let mut parts: Vec<String> = Vec::new();
        if let Some(sp) = &base_system_prompt { parts.push(sp.clone()); }
        if let Some(ac) = agent_context { parts.push(ac); }
        if !skill_instructions.is_empty() { parts.push(skill_instructions.clone()); }

        // Local time context for task agents
        {
            let user_tz = {
                let cfg = state.config.lock().map_err(|e| format!("Lock: {}", e))?;
                cfg.user_timezone.clone()
            };
            let now_utc = chrono::Utc::now();
            if let Ok(tz) = user_tz.parse::<chrono_tz::Tz>() {
                let local: chrono::DateTime<chrono_tz::Tz> = now_utc.with_timezone(&tz);
                parts.push(format!(
                    "## Local Time\n\
                    - **Current time**: {}\n\
                    - **Timezone**: {} (UTC{})\n\
                    - **Day of week**: {}",
                    local.format("%Y-%m-%d %H:%M:%S"),
                    tz.name(),
                    local.format("%:z"),
                    local.format("%A"),
                ));
            } else {
                let local = chrono::Local::now();
                parts.push(format!(
                    "## Local Time\n\
                    - **Current time**: {}\n\
                    - **Timezone**: {} (UTC{})\n\
                    - **Day of week**: {}",
                    local.format("%Y-%m-%d %H:%M:%S"),
                    local.format("%Z"),
                    local.format("%:z"),
                    local.format("%A"),
                ));
            }
        }

        // Multi-agent context
        let agent_count_note = if agent_count > 1 {
            format!("\n\nYou are agent '{}', one of {} agents working on this task collaboratively. Focus on your area of expertise. Be thorough but avoid duplicating work other agents may do.", agent_id, agent_count)
        } else {
            String::new()
        };

        // Cron-awareness: let the agent know if this is a scheduled recurring task
        let cron_context = if let Some(ref sched) = task.cron_schedule {
            if !sched.is_empty() {
                format!(
                    "\n\n**Execution mode:** This task was triggered automatically by a scheduled cron job (schedule: `{}`).\n\
                    - You are running autonomously — there is no human operator watching.\n\
                    - Complete your work, produce a clear summary, and exit cleanly.\n\
                    - Do NOT ask questions or wait for user input.\n\
                    - If you encounter errors, log them clearly and move on.\n\
                    - This task will run again on schedule, so focus on the current cycle only.",
                    sched
                )
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        parts.push(format!(
            "## Current Task\nYou are working on a task from the task board.\n- **Title:** {}\n- **Priority:** {}{}{}\n\nComplete this task thoroughly. When done, summarize what you accomplished.",
            task.title, task.priority, agent_count_note, cron_context
        ));

        let full_system_prompt = parts.join("\n\n---\n\n");

        // Store user message for this agent's session
        let user_msg = StoredMessage {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.clone(),
            role: "user".into(),
            content: task_prompt.clone(),
            tool_calls_json: None,
            tool_call_id: None,
            name: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        state.store.add_message(&user_msg)?;

        // Load conversation
        let mut messages = state.store.load_conversation(&session_id, Some(&full_system_prompt))?;

        let provider = AnyProvider::from_config(&provider_config);
        let pending_clone = pending.clone();
        let task_id_clone = task_id.to_string();
        let store_path_clone = store_path.clone();
        let run_id_clone = run_id.clone();
        let app_handle_clone = app_handle.clone();
        let all_tools_clone = all_tools.clone();
        let model_clone = model.clone();
        let sem_clone = sem.clone();

        // Clone Arc for this task's agent
        let task_daily_tokens_clone = task_daily_tokens.clone();
        let task_daily_budget_clone = task_daily_budget;

        // Cap tool rounds for cron tasks to prevent runaway agent loops
        let effective_max_rounds = if is_recurring {
            max_rounds.min(CRON_MAX_TOOL_ROUNDS)
        } else {
            max_rounds
        };

        let handle = tauri::async_runtime::spawn(async move {
            // ── Semaphore gate: wait for a concurrency slot ──
            // Background tasks (cron/manual) respect the limit.
            // If the semaphore is full, this agent waits its turn.
            let _permit = sem_clone.acquire_owned().await.ok();
            info!("[engine] Task agent '{}' acquired run slot", agent_id);

            let result = agent_loop::run_agent_turn(
                &app_handle_clone,
                &provider,
                &model_clone,
                &mut messages,
                &all_tools_clone,
                &session_id,
                &run_id_clone,
                effective_max_rounds,
                None,
                &pending_clone,
                tool_timeout,
                &agent_id,
                task_daily_budget_clone,
                Some(&task_daily_tokens_clone),
            ).await;

            // Store agent result
            if let Ok(conn) = rusqlite::Connection::open(&store_path_clone) {
                match &result {
                    Ok(text) => {
                        let msg_id = uuid::Uuid::new_v4().to_string();
                        conn.execute(
                            "INSERT INTO messages (id, session_id, role, content) VALUES (?1, ?2, 'assistant', ?3)",
                            rusqlite::params![msg_id, session_id, text],
                        ).ok();
                        // Activity log
                        let aid = uuid::Uuid::new_v4().to_string();
                        conn.execute(
                            "INSERT INTO task_activity (id, task_id, kind, agent, content) VALUES (?1, ?2, 'agent_completed', ?3, ?4)",
                            rusqlite::params![aid, task_id_clone, agent_id, format!("Agent {} completed. Summary: {}", agent_id, crate::engine::types::truncate_utf8(&text, 200))],
                        ).ok();
                    }
                    Err(err) => {
                        let aid = uuid::Uuid::new_v4().to_string();
                        conn.execute(
                            "INSERT INTO task_activity (id, task_id, kind, agent, content) VALUES (?1, ?2, 'agent_error', ?3, ?4)",
                            rusqlite::params![aid, task_id_clone, agent_id, format!("Agent {} error: {}", agent_id, err)],
                        ).ok();
                    }
                }
            }

            result
        });

        handles.push(handle);
    }

    // Spawn a coordinator that waits for all agents to finish
    let app_handle_final = app_handle.clone();
    let inflight_clone = inflight.clone();
    let task_id_for_cleanup = task_id.to_string();
    tauri::async_runtime::spawn(async move {
        let mut all_ok = true;
        let mut any_ok = false;
        for handle in handles {
            match handle.await {
                Ok(Ok(_)) => { any_ok = true; }
                _ => { all_ok = false; }
            }
        }

        // ── Remove from in-flight set so the task can be triggered again ──
        if let Ok(mut set) = inflight_clone.lock() {
            set.remove(&task_id_for_cleanup);
        }

        // Update task status based on results.
        // Recurring (cron) tasks stay "in_progress" so they remain in the
        // correct kanban column and will be picked up on the next schedule.
        // One-shot tasks transition to "review" (success) or "blocked" (all failed).
        if let Ok(conn) = rusqlite::Connection::open(&store_path) {
            let new_status = if is_recurring {
                "in_progress"
            } else if any_ok {
                "review"
            } else {
                "blocked"
            };
            conn.execute(
                "UPDATE tasks SET status = ?2, updated_at = datetime('now') WHERE id = ?1",
                rusqlite::params![task_id_for_spawn, new_status],
            ).ok();

            // Final summary activity
            let aid = uuid::Uuid::new_v4().to_string();
            let summary = if is_recurring {
                if agent_count > 1 {
                    format!("All {} agents finished (recurring). Staying in_progress for next run.", agent_count)
                } else {
                    format!("Cron cycle completed. Staying in_progress for next run.")
                }
            } else if agent_count > 1 {
                format!("All {} agents finished. Status: {}", agent_count, new_status)
            } else {
                format!("Task completed. Status: {}", new_status)
            };
            conn.execute(
                "INSERT INTO task_activity (id, task_id, kind, content) VALUES (?1, ?2, 'status_change', ?3)",
                rusqlite::params![aid, task_id_for_spawn, summary],
            ).ok();
        }

        // Emit task-updated event
        app_handle_final.emit("task-updated", serde_json::json!({
            "task_id": task_id_for_spawn,
            "status": if is_recurring { "in_progress" } else if any_ok { "review" } else { "blocked" },
        })).ok();
    });

    Ok(run_id)
}

/// Check for due cron tasks and process them.
/// Returns the IDs of tasks that were due and triggered.
#[tauri::command]
pub fn engine_tasks_cron_tick(
    state: State<'_, EngineState>,
) -> Result<Vec<String>, String> {
    let due = state.store.get_due_cron_tasks()?;
    let mut triggered_ids = Vec::new();

    for task in due {
        info!("[engine] Cron task due: {} ({})", task.title, task.id);

        // Update last_run_at and compute next_run_at
        let now = chrono::Utc::now();
        let next = compute_next_run(&task.cron_schedule, &now);
        state.store.update_task_cron_run(&task.id, &now.to_rfc3339(), next.as_deref())?;

        // Log activity
        let aid = uuid::Uuid::new_v4().to_string();
        state.store.add_task_activity(
            &aid, &task.id, "cron_triggered", None,
            &format!("Cron triggered: {}", task.cron_schedule.as_deref().unwrap_or("unknown")),
        )?;

        triggered_ids.push(task.id);
    }

    Ok(triggered_ids)
}

/// Simple schedule parser: "every Xm", "every Xh", "daily HH:MM"
fn compute_next_run(schedule: &Option<String>, from: &chrono::DateTime<chrono::Utc>) -> Option<String> {
    let s = schedule.as_deref()?;
    let s = s.trim().to_lowercase();

    if s.starts_with("every ") {
        let rest = s.strip_prefix("every ")?.trim();
        if rest.ends_with('m') {
            let mins: i64 = rest.trim_end_matches('m').trim().parse().ok()?;
            return Some((*from + chrono::Duration::minutes(mins)).to_rfc3339());
        } else if rest.ends_with('h') {
            let hours: i64 = rest.trim_end_matches('h').trim().parse().ok()?;
            return Some((*from + chrono::Duration::hours(hours)).to_rfc3339());
        }
    } else if s.starts_with("daily ") {
        let time_str = s.strip_prefix("daily ")?.trim();
        let parts: Vec<&str> = time_str.split(':').collect();
        if parts.len() == 2 {
            let hour: u32 = parts[0].parse().ok()?;
            let minute: u32 = parts[1].parse().ok()?;
            let today = from.date_naive();
            let target_time = today.and_hms_opt(hour, minute, 0)?;
            let target = target_time.and_utc();
            if target > *from {
                return Some(target.to_rfc3339());
            } else {
                let tomorrow = today.succ_opt()?;
                let next = tomorrow.and_hms_opt(hour, minute, 0)?.and_utc();
                return Some(next.to_rfc3339());
            }
        }
    }

    // Default: 1 hour from now
    Some((*from + chrono::Duration::hours(1)).to_rfc3339())
}

// ── Cron Heartbeat (background autonomous execution) ───────────────────

/// Check all open positions against current prices.
/// If stop-loss or take-profit thresholds are crossed, auto-sell.
async fn check_positions(app_handle: &tauri::AppHandle) {
    let state = app_handle.state::<EngineState>();

    let positions = match state.store.list_positions(Some("open")) {
        Ok(p) => p,
        Err(e) => {
            warn!("[positions] Failed to load open positions: {}", e);
            return;
        }
    };

    if positions.is_empty() {
        return;
    }

    // Get Solana credentials for selling
    let creds = match skills::get_skill_credentials(&state.store, "solana") {
        Ok(c) => c,
        Err(e) => {
            warn!("[positions] Cannot load Solana credentials: {}", e);
            return;
        }
    };

    if !creds.contains_key("SOLANA_WALLET_ADDRESS") || !creds.contains_key("SOLANA_PRIVATE_KEY") {
        return; // No wallet configured — skip silently
    }

    info!("[positions] Checking {} open position(s)", positions.len());

    for pos in &positions {
        // Rate-limit: skip if checked within the last 55 seconds
        if let Some(ref last) = pos.last_checked_at {
            if let Ok(last_dt) = chrono::NaiveDateTime::parse_from_str(last, "%Y-%m-%d %H:%M:%S") {
                let now = chrono::Utc::now().naive_utc();
                if (now - last_dt).num_seconds() < 55 {
                    continue;
                }
            }
        }

        // Fetch current price from DexScreener
        let current_price = match crate::engine::sol_dex::get_token_price_usd(&pos.mint).await {
            Ok(p) => p,
            Err(e) => {
                warn!("[positions] Price lookup failed for {} ({}): {}", pos.symbol, &pos.mint[..std::cmp::min(8, pos.mint.len())], e);
                continue;
            }
        };

        // Update tracked price
        let _ = state.store.update_position_price(&pos.id, current_price);

        // Calculate price change ratio
        if pos.entry_price_usd <= 0.0 {
            continue;
        }
        let ratio = current_price / pos.entry_price_usd;

        // ── Stop-loss: price dropped below threshold ──
        if ratio <= (1.0 - pos.stop_loss_pct) {
            info!("[positions] 🛑 STOP-LOSS triggered for {} — entry ${:.8}, now ${:.8} ({:.1}% loss)",
                pos.symbol, pos.entry_price_usd, current_price, (1.0 - ratio) * 100.0);

            let sell_result = execute_position_sell(app_handle, &creds, &pos.mint, &pos.symbol, pos.current_amount).await;

            match sell_result {
                Ok(tx) => {
                    let _ = state.store.close_position(&pos.id, "closed_sl", Some(&tx));
                    let _ = state.store.insert_trade(
                        "sol_swap", Some("sell"), Some(&format!("{} → SOL", pos.symbol)),
                        Some(&pos.mint), &pos.current_amount.to_string(),
                        None, None, "completed", None, Some("SOL"),
                        &format!("Auto stop-loss at {:.1}% loss", (1.0 - ratio) * 100.0),
                        None, None, Some(&tx),
                    );
                    app_handle.emit("position-closed", serde_json::json!({
                        "id": pos.id, "symbol": pos.symbol, "reason": "stop_loss",
                        "entry_price": pos.entry_price_usd, "exit_price": current_price,
                    })).ok();
                    info!("[positions] ✅ Stop-loss sell executed for {} — tx: {}", pos.symbol, &tx[..std::cmp::min(16, tx.len())]);
                }
                Err(e) => {
                    error!("[positions] ❌ Stop-loss sell FAILED for {}: {}", pos.symbol, e);
                }
            }
        }
        // ── Take-profit: price rose above threshold ──
        else if ratio >= pos.take_profit_pct {
            info!("[positions] 🎯 TAKE-PROFIT triggered for {} — entry ${:.8}, now ${:.8} ({:.1}x)",
                pos.symbol, pos.entry_price_usd, current_price, ratio);

            // Sell half on take-profit (lock in gains, let the rest ride)
            let sell_amount = pos.current_amount / 2.0;
            let sell_result = execute_position_sell(app_handle, &creds, &pos.mint, &pos.symbol, sell_amount).await;

            match sell_result {
                Ok(tx) => {
                    let remaining = pos.current_amount - sell_amount;
                    if remaining < 1.0 {
                        // Effectively closed
                        let _ = state.store.close_position(&pos.id, "closed_tp", Some(&tx));
                    } else {
                        // Partial sell — reduce position, raise stop-loss to break-even
                        let _ = state.store.reduce_position(&pos.id, remaining);
                        let _ = state.store.update_position_targets(&pos.id, 0.05, pos.take_profit_pct * 1.5);
                    }
                    let _ = state.store.insert_trade(
                        "sol_swap", Some("sell"), Some(&format!("{} → SOL", pos.symbol)),
                        Some(&pos.mint), &sell_amount.to_string(),
                        None, None, "completed", None, Some("SOL"),
                        &format!("Auto take-profit at {:.1}x", ratio),
                        None, None, Some(&tx),
                    );
                    app_handle.emit("position-closed", serde_json::json!({
                        "id": pos.id, "symbol": pos.symbol, "reason": "take_profit",
                        "entry_price": pos.entry_price_usd, "exit_price": current_price,
                    })).ok();
                    info!("[positions] ✅ Take-profit sell executed for {} — tx: {}", pos.symbol, &tx[..std::cmp::min(16, tx.len())]);
                }
                Err(e) => {
                    error!("[positions] ❌ Take-profit sell FAILED for {}: {}", pos.symbol, e);
                }
            }
        }
    }
}

/// Execute a sell of `amount` tokens of `mint` for SOL via the existing swap infrastructure.
/// Returns the transaction signature on success.
async fn execute_position_sell(
    app_handle: &tauri::AppHandle,
    creds: &HashMap<String, String>,
    mint: &str,
    symbol: &str,
    amount: f64,
) -> Result<String, String> {
    // Build args in the same format as agent tool calls
    let args = serde_json::json!({
        "token_in": mint,
        "token_out": "SOL",
        "amount": format!("{}", amount as u64), // raw token amount for sells
        "reason": format!("Auto position management for {}", symbol),
        "slippage_bps": 300  // 3% slippage for automated sells
    });

    let result = crate::engine::sol_dex::execute_sol_swap(&args, creds).await?;

    // Extract tx hash from the markdown result
    // Format: "| Transaction | [hash](https://solscan.io/tx/FULL_HASH) |"
    if let Some(start) = result.find("solscan.io/tx/") {
        let after = &result[start + 14..];
        if let Some(end) = after.find(')') {
            return Ok(after[..end].to_string());
        }
    }

    // Fallback: return the full result (swap succeeded but couldn't parse tx)
    Ok(format!("swap_ok_{}", chrono::Utc::now().timestamp()))
}

/// Background cron heartbeat — called every 60 seconds from the Tauri
/// setup hook. Finds due cron tasks, updates their timestamps, and
/// auto-executes each one via `execute_task`.
pub async fn run_cron_heartbeat(app_handle: &tauri::AppHandle) {
    let state = app_handle.state::<EngineState>();

    // 0) Check open positions (stop-loss / take-profit)
    check_positions(app_handle).await;

    // 1) Find all due cron tasks
    let due_tasks = match state.store.get_due_cron_tasks() {
        Ok(tasks) => tasks,
        Err(e) => {
            warn!("[heartbeat] Failed to query due cron tasks: {}", e);
            return;
        }
    };

    if due_tasks.is_empty() {
        return;
    }

    info!("[heartbeat] {} cron task(s) due", due_tasks.len());

    for task in due_tasks {
        let task_id = task.id.clone();
        let task_title = task.title.clone();

        // 2) Update cron timestamps (last_run_at → now, compute next_run_at)
        let now = chrono::Utc::now();
        let next = compute_next_run(&task.cron_schedule, &now);
        if let Err(e) = state.store.update_task_cron_run(&task_id, &now.to_rfc3339(), next.as_deref()) {
            error!("[heartbeat] Failed to update cron timestamps for '{}': {}", task_title, e);
            continue;
        }

        // 3) Log cron trigger activity
        let aid = uuid::Uuid::new_v4().to_string();
        state.store.add_task_activity(
            &aid, &task_id, "cron_triggered", None,
            &format!("Cron triggered: {}", task.cron_schedule.as_deref().unwrap_or("unknown")),
        ).ok();

        // 4) Execute the task (spawns agent loops)
        let app = app_handle.clone();
        tauri::async_runtime::spawn(async move {
            let st = app.state::<EngineState>();
            match execute_task(&app, &st, &task_id).await {
                Ok(run_id) => {
                    info!("[heartbeat] Cron task '{}' started, run_id={}", task_title, run_id);
                }
                Err(e) => {
                    error!("[heartbeat] Cron task '{}' failed to start: {}", task_title, e);
                    // Log the failure
                    if let Ok(conn) = rusqlite::Connection::open(crate::engine::sessions::engine_db_path()) {
                        let aid = uuid::Uuid::new_v4().to_string();
                        conn.execute(
                            "INSERT INTO task_activity (id, task_id, kind, content) VALUES (?1, ?2, 'cron_error', ?3)",
                            rusqlite::params![aid, task_id, format!("Cron execution failed: {}", e)],
                        ).ok();
                    }
                }
            }
        });
    }

    // Emit event so frontend knows automations ran
    app_handle.emit("cron-heartbeat", serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
    })).ok();
}

// ── Telegram Bridge Commands ───────────────────────────────────────────

#[tauri::command]
pub async fn engine_telegram_start(
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    crate::engine::telegram::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_telegram_stop() -> Result<(), String> {
    crate::engine::telegram::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_telegram_status(
    app_handle: tauri::AppHandle,
) -> Result<crate::engine::telegram::TelegramStatus, String> {
    Ok(crate::engine::telegram::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_telegram_get_config(
    app_handle: tauri::AppHandle,
) -> Result<crate::engine::telegram::TelegramConfig, String> {
    crate::engine::telegram::load_telegram_config(&app_handle)
}

#[tauri::command]
pub fn engine_telegram_set_config(
    app_handle: tauri::AppHandle,
    config: crate::engine::telegram::TelegramConfig,
) -> Result<(), String> {
    crate::engine::telegram::save_telegram_config(&app_handle, &config)
}

#[tauri::command]
pub async fn engine_telegram_approve_user(
    app_handle: tauri::AppHandle,
    user_id: i64,
) -> Result<(), String> {
    crate::engine::telegram::approve_user(&app_handle, user_id).await
}

#[tauri::command]
pub async fn engine_telegram_deny_user(
    app_handle: tauri::AppHandle,
    user_id: i64,
) -> Result<(), String> {
    crate::engine::telegram::deny_user(&app_handle, user_id).await
}

#[tauri::command]
pub fn engine_telegram_remove_user(
    app_handle: tauri::AppHandle,
    user_id: i64,
) -> Result<(), String> {
    crate::engine::telegram::remove_user(&app_handle, user_id)
}

// ── Discord Bridge Commands ────────────────────────────────────────────

#[tauri::command]
pub async fn engine_discord_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::discord::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_discord_stop() -> Result<(), String> {
    crate::engine::discord::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_discord_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::discord::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_discord_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::discord::DiscordConfig, String> {
    crate::engine::discord::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_discord_set_config(app_handle: tauri::AppHandle, config: crate::engine::discord::DiscordConfig) -> Result<(), String> {
    crate::engine::discord::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_discord_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::discord::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_discord_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::discord::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_discord_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::discord::remove_user(&app_handle, &user_id)
}

// ── IRC Bridge Commands ────────────────────────────────────────────────

#[tauri::command]
pub async fn engine_irc_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::irc::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_irc_stop() -> Result<(), String> {
    crate::engine::irc::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_irc_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::irc::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_irc_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::irc::IrcConfig, String> {
    crate::engine::irc::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_irc_set_config(app_handle: tauri::AppHandle, config: crate::engine::irc::IrcConfig) -> Result<(), String> {
    crate::engine::irc::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_irc_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::irc::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_irc_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::irc::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_irc_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::irc::remove_user(&app_handle, &user_id)
}

// ── Slack Bridge Commands ──────────────────────────────────────────────

#[tauri::command]
pub async fn engine_slack_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::slack::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_slack_stop() -> Result<(), String> {
    crate::engine::slack::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_slack_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::slack::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_slack_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::slack::SlackConfig, String> {
    crate::engine::slack::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_slack_set_config(app_handle: tauri::AppHandle, config: crate::engine::slack::SlackConfig) -> Result<(), String> {
    crate::engine::slack::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_slack_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::slack::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_slack_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::slack::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_slack_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::slack::remove_user(&app_handle, &user_id)
}

// ── Matrix Bridge Commands ─────────────────────────────────────────────

#[tauri::command]
pub async fn engine_matrix_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::matrix::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_matrix_stop() -> Result<(), String> {
    crate::engine::matrix::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_matrix_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::matrix::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_matrix_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::matrix::MatrixConfig, String> {
    crate::engine::matrix::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_matrix_set_config(app_handle: tauri::AppHandle, config: crate::engine::matrix::MatrixConfig) -> Result<(), String> {
    crate::engine::matrix::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_matrix_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::matrix::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_matrix_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::matrix::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_matrix_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::matrix::remove_user(&app_handle, &user_id)
}

// ── Mattermost Bridge Commands ─────────────────────────────────────────

#[tauri::command]
pub async fn engine_mattermost_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::mattermost::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_mattermost_stop() -> Result<(), String> {
    crate::engine::mattermost::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_mattermost_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::mattermost::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_mattermost_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::mattermost::MattermostConfig, String> {
    crate::engine::mattermost::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_mattermost_set_config(app_handle: tauri::AppHandle, config: crate::engine::mattermost::MattermostConfig) -> Result<(), String> {
    crate::engine::mattermost::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_mattermost_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::mattermost::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_mattermost_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::mattermost::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_mattermost_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::mattermost::remove_user(&app_handle, &user_id)
}

// ── Nextcloud Talk Bridge Commands ─────────────────────────────────────

#[tauri::command]
pub async fn engine_nextcloud_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::nextcloud::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_nextcloud_stop() -> Result<(), String> {
    crate::engine::nextcloud::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_nextcloud_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::nextcloud::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_nextcloud_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::nextcloud::NextcloudConfig, String> {
    crate::engine::nextcloud::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_nextcloud_set_config(app_handle: tauri::AppHandle, config: crate::engine::nextcloud::NextcloudConfig) -> Result<(), String> {
    crate::engine::nextcloud::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_nextcloud_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::nextcloud::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_nextcloud_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::nextcloud::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_nextcloud_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::nextcloud::remove_user(&app_handle, &user_id)
}

// ── Nostr Bridge Commands ──────────────────────────────────────────────

#[tauri::command]
pub async fn engine_nostr_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::nostr::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_nostr_stop() -> Result<(), String> {
    crate::engine::nostr::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_nostr_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::nostr::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_nostr_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::nostr::NostrConfig, String> {
    crate::engine::nostr::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_nostr_set_config(app_handle: tauri::AppHandle, config: crate::engine::nostr::NostrConfig) -> Result<(), String> {
    crate::engine::nostr::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_nostr_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::nostr::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_nostr_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::nostr::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_nostr_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::nostr::remove_user(&app_handle, &user_id)
}

// ── Twitch Bridge Commands ─────────────────────────────────────────────

#[tauri::command]
pub async fn engine_twitch_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::twitch::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_twitch_stop() -> Result<(), String> {
    crate::engine::twitch::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_twitch_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::twitch::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_twitch_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::twitch::TwitchConfig, String> {
    crate::engine::twitch::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_twitch_set_config(app_handle: tauri::AppHandle, config: crate::engine::twitch::TwitchConfig) -> Result<(), String> {
    crate::engine::twitch::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_twitch_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::twitch::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_twitch_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::twitch::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_twitch_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::twitch::remove_user(&app_handle, &user_id)
}

// ── Web Chat Bridge Commands ───────────────────────────────────────────

#[tauri::command]
pub fn engine_webchat_start(app_handle: tauri::AppHandle) -> Result<(), String> {
    crate::engine::webchat::start_bridge(app_handle)
}

#[tauri::command]
pub fn engine_webchat_stop() -> Result<(), String> {
    crate::engine::webchat::stop_bridge();
    Ok(())
}

#[tauri::command]
pub fn engine_webchat_status(app_handle: tauri::AppHandle) -> Result<crate::engine::channels::ChannelStatus, String> {
    Ok(crate::engine::webchat::get_status(&app_handle))
}

#[tauri::command]
pub fn engine_webchat_get_config(app_handle: tauri::AppHandle) -> Result<crate::engine::webchat::WebChatConfig, String> {
    crate::engine::webchat::load_config(&app_handle)
}

#[tauri::command]
pub fn engine_webchat_set_config(app_handle: tauri::AppHandle, config: crate::engine::webchat::WebChatConfig) -> Result<(), String> {
    crate::engine::webchat::save_config(&app_handle, &config)
}

#[tauri::command]
pub fn engine_webchat_approve_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::webchat::approve_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_webchat_deny_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::webchat::deny_user(&app_handle, &user_id)
}

#[tauri::command]
pub fn engine_webchat_remove_user(app_handle: tauri::AppHandle, user_id: String) -> Result<(), String> {
    crate::engine::webchat::remove_user(&app_handle, &user_id)
}

// ── Orchestrator: Projects ─────────────────────────────────────────────

#[tauri::command]
pub fn engine_projects_list(state: State<'_, EngineState>) -> Result<Vec<crate::engine::types::Project>, String> {
    state.store.list_projects()
}

#[tauri::command]
pub fn engine_project_create(state: State<'_, EngineState>, project: crate::engine::types::Project) -> Result<(), String> {
    state.store.create_project(&project)
}

#[tauri::command]
pub fn engine_project_update(state: State<'_, EngineState>, project: crate::engine::types::Project) -> Result<(), String> {
    state.store.update_project(&project)
}

#[tauri::command]
pub fn engine_project_delete(state: State<'_, EngineState>, project_id: String) -> Result<(), String> {
    state.store.delete_project(&project_id)
}

#[tauri::command]
pub fn engine_project_set_agents(
    state: State<'_, EngineState>,
    project_id: String,
    agents: Vec<crate::engine::types::ProjectAgent>,
) -> Result<(), String> {
    state.store.set_project_agents(&project_id, &agents)
}

#[tauri::command]
pub fn engine_list_all_agents(
    state: State<'_, EngineState>,
) -> Result<Vec<serde_json::Value>, String> {
    let agents = state.store.list_all_agents()?;
    Ok(agents
        .into_iter()
        .map(|(project_id, agent)| {
            serde_json::json!({
                "project_id": project_id,
                "agent_id": agent.agent_id,
                "role": agent.role,
                "specialty": agent.specialty,
                "status": agent.status,
                "current_task": agent.current_task,
                "model": agent.model,
                "system_prompt": agent.system_prompt,
                "capabilities": agent.capabilities,
            })
        })
        .collect())
}

/// Create a standalone agent (user-created, not from orchestrator).
/// Uses project_id="_standalone" as a sentinel so it lives alongside project agents
/// but is clearly user-created.
#[tauri::command]
pub fn engine_create_agent(
    state: State<'_, EngineState>,
    agent_id: String,
    role: String,
    specialty: Option<String>,
    model: Option<String>,
    system_prompt: Option<String>,
    capabilities: Option<Vec<String>>,
) -> Result<(), String> {
    let agent = crate::engine::types::ProjectAgent {
        agent_id: agent_id.clone(),
        role,
        specialty: specialty.unwrap_or_else(|| "general".into()),
        status: "idle".into(),
        current_task: None,
        model,
        system_prompt,
        capabilities: capabilities.unwrap_or_default(),
    };
    state.store.add_project_agent("_standalone", &agent)?;
    info!("[engine] Created standalone agent: {}", agent_id);
    Ok(())
}

/// Delete a standalone agent by agent_id.
#[tauri::command]
pub fn engine_delete_agent(
    state: State<'_, EngineState>,
    agent_id: String,
) -> Result<(), String> {
    state.store.delete_agent("_standalone", &agent_id)?;
    info!("[engine] Deleted standalone agent: {}", agent_id);
    Ok(())
}

#[tauri::command]
pub fn engine_project_messages(
    state: State<'_, EngineState>,
    project_id: String,
    limit: Option<i64>,
) -> Result<Vec<crate::engine::types::ProjectMessage>, String> {
    state.store.get_project_messages(&project_id, limit.unwrap_or(100))
}

#[tauri::command]
pub async fn engine_project_run(
    app_handle: tauri::AppHandle,
    project_id: String,
) -> Result<String, String> {
    let run_id = uuid::Uuid::new_v4().to_string();
    let app = app_handle.clone();
    let pid = project_id.clone();

    // Spawn the orchestrator in background
    tauri::async_runtime::spawn(async move {
        match crate::engine::orchestrator::run_project(&app, &pid).await {
            Ok(text) => info!("[orchestrator] Project {} completed: {}...", pid, crate::engine::types::truncate_utf8(&text, 200)),
            Err(e) => error!("[orchestrator] Project {} failed: {}", pid, e),
        }
    });

    Ok(run_id)
}

// ═══ Text-to-Speech (Google Cloud TTS + OpenAI TTS) ═════════════════════════

/// TTS configuration stored in DB as JSON under key "tts_config"
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TtsConfig {
    pub provider: String,        // "google" | "openai"
    pub voice: String,           // e.g. "en-US-Chirp3-HD-Achernar" or "alloy"
    pub speed: f64,              // 0.25–4.0
    pub language_code: String,   // e.g. "en-US"
    pub auto_speak: bool,        // automatically speak new responses
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            provider: "google".into(),
            voice: "en-US-Chirp3-HD-Achernar".into(),
            speed: 1.0,
            language_code: "en-US".into(),
            auto_speak: false,
        }
    }
}

/// Synthesize speech from text. Returns base64-encoded MP3 audio.
#[tauri::command]
pub async fn engine_tts_speak(
    state: State<'_, EngineState>,
    text: String,
) -> Result<String, String> {
    if text.trim().is_empty() {
        return Err("No text to speak".into());
    }

    // Load TTS config from DB
    let tts_config: TtsConfig = {
        let store = &state.store;
        match store.get_config("tts_config") {
            Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_default(),
            _ => TtsConfig::default(),
        }
    };

    // Find the provider's API key from engine config
    // Extract needed values before any async calls (MutexGuard is !Send)
    let (openai_provider_info, google_key) = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        let openai = config.providers.iter().find(|p| p.kind == ProviderKind::OpenAI);
        let google = config.providers.iter().find(|p| p.kind == ProviderKind::Google);
        (
            openai.map(|p| (p.api_key.clone(), p.base_url.clone().unwrap_or_else(|| "https://api.openai.com/v1".into()))),
            google.map(|p| p.api_key.clone()),
        )
    };

    match tts_config.provider.as_str() {
        "openai" => {
            let (api_key, base_url) = openai_provider_info
                .ok_or("No OpenAI provider configured — add one in Settings → Models")?;
            tts_openai(&api_key, &base_url, &text, &tts_config).await
        }
        _ => {
            // Default: Google Cloud TTS
            let api_key = google_key
                .ok_or("No Google provider configured — add one in Settings → Models")?;
            tts_google(&api_key, &text, &tts_config).await
        }
    }
}

/// Get TTS config
#[tauri::command]
pub fn engine_tts_get_config(
    state: State<'_, EngineState>,
) -> Result<TtsConfig, String> {
    match state.store.get_config("tts_config") {
        Ok(Some(json)) => serde_json::from_str(&json).map_err(|e| e.to_string()),
        _ => Ok(TtsConfig::default()),
    }
}

/// Save TTS config
#[tauri::command]
pub fn engine_tts_set_config(
    state: State<'_, EngineState>,
    config: TtsConfig,
) -> Result<(), String> {
    let json = serde_json::to_string(&config).map_err(|e| e.to_string())?;
    state.store.set_config("tts_config", &json)?;
    info!("[tts] Config saved: provider={}, voice={}", config.provider, config.voice);
    Ok(())
}

/// Google Cloud TTS — calls texttospeech.googleapis.com/v1/text:synthesize
async fn tts_google(api_key: &str, text: &str, config: &TtsConfig) -> Result<String, String> {
    // Strip markdown for cleaner speech
    let clean = strip_markdown(text);
    if clean.trim().is_empty() {
        return Err("No speakable text after stripping markdown".into());
    }

    // Google TTS has a 5000 byte limit per request — chunk if needed
    let chunks = chunk_text(&clean, 4800);
    let client = reqwest::Client::new();
    let mut all_audio = Vec::new();

    for chunk in &chunks {
        let body = serde_json::json!({
            "input": { "text": chunk },
            "voice": {
                "languageCode": config.language_code,
                "name": config.voice
            },
            "audioConfig": {
                "audioEncoding": "MP3",
                "speakingRate": config.speed,
                "effectsProfileId": ["headphone-class-device"]
            }
        });

        let resp = client
            .post(format!(
                "https://texttospeech.googleapis.com/v1/text:synthesize?key={}",
                api_key
            ))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Google TTS request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Google TTS error ({}): {}", status, body));
        }

        let result: serde_json::Value = resp.json().await
            .map_err(|e| format!("Google TTS JSON parse error: {}", e))?;

        if let Some(audio) = result["audioContent"].as_str() {
            // Decode and accumulate raw audio bytes
            let bytes = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                audio,
            ).map_err(|e| format!("Base64 decode error: {}", e))?;
            all_audio.extend_from_slice(&bytes);
        } else {
            return Err("Google TTS: no audioContent in response".into());
        }
    }

    // Re-encode combined audio as base64
    Ok(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &all_audio,
    ))
}

/// OpenAI TTS — calls /v1/audio/speech
async fn tts_openai(api_key: &str, base_url: &str, text: &str, config: &TtsConfig) -> Result<String, String> {
    let clean = strip_markdown(text);
    if clean.trim().is_empty() {
        return Err("No speakable text after stripping markdown".into());
    }

    // OpenAI TTS has a 4096 char limit
    let chunks = chunk_text(&clean, 4000);
    let client = reqwest::Client::new();
    let mut all_audio = Vec::new();

    for chunk in &chunks {
        let body = serde_json::json!({
            "model": "tts-1",
            "input": chunk,
            "voice": config.voice,
            "speed": config.speed,
            "response_format": "mp3"
        });

        let resp = client
            .post(format!("{}/audio/speech", base_url.trim_end_matches('/')))
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("OpenAI TTS request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("OpenAI TTS error ({}): {}", status, body));
        }

        let bytes = resp.bytes().await
            .map_err(|e| format!("OpenAI TTS read error: {}", e))?;
        all_audio.extend_from_slice(&bytes);
    }

    Ok(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &all_audio,
    ))
}

/// Strip markdown formatting for cleaner TTS output
fn strip_markdown(text: &str) -> String {
    let mut out = text.to_string();
    // Remove code blocks
    while let Some(start) = out.find("```") {
        if let Some(end) = out[start + 3..].find("```") {
            out.replace_range(start..start + 3 + end + 3, " ");
        } else {
            break;
        }
    }
    // Remove inline code
    out = out.replace('`', "");
    // Remove bold/italic markers
    out = out.replace("**", "").replace("__", "").replace('*', "").replace('_', " ");
    // Remove headers
    out = out.lines().map(|l| {
        let trimmed = l.trim_start();
        if trimmed.starts_with('#') {
            trimmed.trim_start_matches('#').trim_start()
        } else {
            l
        }
    }).collect::<Vec<_>>().join("\n");
    // Remove links [text](url) → text
    let mut result = String::new();
    let mut chars = out.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' {
            let mut link_text = String::new();
            let mut found_close = false;
            for cc in chars.by_ref() {
                if cc == ']' {
                    found_close = true;
                    break;
                }
                link_text.push(cc);
            }
            if found_close {
                // Skip (url) part
                if chars.peek() == Some(&'(') {
                    chars.next();
                    for cc in chars.by_ref() {
                        if cc == ')' { break; }
                    }
                }
                result.push_str(&link_text);
            } else {
                result.push('[');
                result.push_str(&link_text);
            }
        } else {
            result.push(c);
        }
    }
    // Remove bullet points
    result = result.lines().map(|l| {
        let trimmed = l.trim_start();
        if trimmed.starts_with("- ") || trimmed.starts_with("• ") {
            &trimmed[2..]
        } else {
            l
        }
    }).collect::<Vec<_>>().join("\n");
    // Collapse whitespace
    while result.contains("  ") {
        result = result.replace("  ", " ");
    }
    result.trim().to_string()
}

/// Split text into chunks of max `max_bytes` length, breaking at sentence boundaries
fn chunk_text(text: &str, max_bytes: usize) -> Vec<String> {
    if text.len() <= max_bytes {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for sentence in text.split_inclusive(|c: char| c == '.' || c == '!' || c == '?' || c == '\n') {
        if current.len() + sentence.len() > max_bytes && !current.is_empty() {
            chunks.push(current.clone());
            current.clear();
        }
        current.push_str(sentence);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}
