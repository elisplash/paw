// engine/state.rs — Shared engine state, type aliases, and model-routing helpers.
// Canonical home for EngineState and related types.
// commands/state.rs re-exports everything from here for backward compatibility.

use crate::engine::memory::EmbeddingClient;
use crate::engine::sessions::SessionStore;
use crate::engine::tool_index::ToolIndex;
use crate::engine::types::*;

use crate::engine::mcp::McpRegistry;

use crate::atoms::error::EngineResult;
use log::info;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

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

impl Default for DailyTokenTracker {
    fn default() -> Self {
        Self::new()
    }
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
        let mut d = self.date.lock();
        if *d != today {
            *d = today;
            self.input_tokens.store(0, Ordering::Relaxed);
            self.output_tokens.store(0, Ordering::Relaxed);
            self.cache_read_tokens.store(0, Ordering::Relaxed);
            self.cache_create_tokens.store(0, Ordering::Relaxed);
            self.cost_microdollars.store(0, Ordering::Relaxed);
            self.warnings_emitted.lock().clear();
        }
    }

    /// Add tokens from a completed round with model-aware pricing.
    pub fn record(&self, model: &str, input: u64, output: u64, cache_read: u64, cache_create: u64) {
        self.maybe_reset();
        self.input_tokens.fetch_add(input, Ordering::Relaxed);
        self.output_tokens.fetch_add(output, Ordering::Relaxed);
        self.cache_read_tokens
            .fetch_add(cache_read, Ordering::Relaxed);
        self.cache_create_tokens
            .fetch_add(cache_create, Ordering::Relaxed);
        // Calculate cost for this round using per-model pricing
        let cost =
            crate::engine::types::estimate_cost_usd(model, input, output, cache_read, cache_create);
        let micro = (cost * 1_000_000.0) as u64;
        self.cost_microdollars.fetch_add(micro, Ordering::Relaxed);
        *self.last_model.lock() = model.to_string();
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
        if usd >= budget_usd {
            Some(usd)
        } else {
            None
        }
    }

    /// Check budget warning thresholds (50%, 75%, 90%).
    /// Returns the threshold percentage if a NEW warning should be emitted.
    pub fn check_budget_warning(&self, budget_usd: f64) -> Option<u8> {
        if budget_usd <= 0.0 {
            return None;
        }
        let (_, _, usd) = self.estimated_spend_usd();
        let pct = (usd / budget_usd * 100.0) as u8;
        let thresholds = [90u8, 75, 50]; // check highest first
        let mut emitted = self.warnings_emitted.lock();
        for &t in &thresholds {
            if pct >= t && !emitted.contains(&t) {
                emitted.push(t);
                return Some(t);
            }
        }
        None
    }
}

/// Signal that the current agent turn should wrap up gracefully.
/// VS Code pattern: when a new user message arrives while one is in progress,
/// the new message is queued and `yield_requested` is set on the active run.
/// The agent loop checks this flag each round and stops if set.
#[derive(Clone)]
pub struct YieldSignal(Arc<AtomicBool>);

impl YieldSignal {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    /// Request the agent to yield (wrap up and stop).
    pub fn request_yield(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Check if yield has been requested.
    pub fn is_yield_requested(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    /// Reset the yield signal (called when starting a new request).
    pub fn reset(&self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl Default for YieldSignal {
    fn default() -> Self {
        Self::new()
    }
}

/// A queued chat request waiting to be processed after the current one completes.
#[derive(Clone)]
pub struct QueuedRequest {
    pub request: ChatRequest,
    pub provider_config: ProviderConfig,
    pub model: String,
    pub system_prompt: Option<String>,
}

/// Per-session request queue.
/// When a user sends a message while one is in progress, it goes here.
/// After the current request completes, the next queued request is dequeued and processed.
pub type RequestQueue = Arc<Mutex<HashMap<String, Vec<QueuedRequest>>>>;

/// Per-session yield signals.
/// When a steering request is queued, yield is requested on the active run.
pub type YieldSignals = Arc<Mutex<HashMap<String, YieldSignal>>>;

/// Map retired / renamed / shorthand model IDs to their current API names.
/// This lets old task configs, agent overrides, and user-entered short names keep working.
pub fn normalize_model_name(model: &str) -> &str {
    match model {
        // ── Google shorthand aliases ────────────────────────────────────
        "gemini-3.1" | "gemini-3.1-pro" => "gemini-3.1-pro-preview",
        "gemini-3" | "gemini-3-pro" => "gemini-3-pro-preview",
        "gemini-3-flash" => "gemini-3-flash-preview",
        // ── Anthropic retired 3.5 model IDs — remap to cheapest available
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
pub fn resolve_provider_for_model(
    model: &str,
    providers: &[ProviderConfig],
) -> Option<ProviderConfig> {
    let model = normalize_model_name(model);
    // 1. Exact match: a provider whose default_model matches exactly
    if let Some(p) = providers
        .iter()
        .find(|p| p.default_model.as_deref() == Some(model))
    {
        return Some(p.clone());
    }

    // 2. Match by model name prefix → well-known provider kind
    if model.starts_with("claude") || model.starts_with("anthropic") {
        providers
            .iter()
            .find(|p| p.kind == ProviderKind::Anthropic)
            .cloned()
    } else if model.starts_with("gemini") || model.starts_with("google") {
        providers
            .iter()
            .find(|p| p.kind == ProviderKind::Google)
            .cloned()
    } else if model.starts_with("gpt")
        || model.starts_with("o1")
        || model.starts_with("o3")
        || model.starts_with("o4")
    {
        providers
            .iter()
            .find(|p| p.kind == ProviderKind::OpenAI)
            .cloned()
    } else if model.starts_with("moonshot") || model.starts_with("kimi") {
        providers
            .iter()
            .find(|p| {
                p.id == "moonshot"
                    || p.base_url
                        .as_deref()
                        .is_some_and(|u| u.contains("moonshot"))
            })
            .cloned()
    } else if model.starts_with("deepseek") {
        providers
            .iter()
            .find(|p| {
                p.id == "deepseek"
                    || p.base_url
                        .as_deref()
                        .is_some_and(|u| u.contains("deepseek"))
            })
            .cloned()
    } else if model.starts_with("grok") {
        providers
            .iter()
            .find(|p| p.id == "xai" || p.base_url.as_deref().is_some_and(|u| u.contains("x.ai")))
            .cloned()
    } else if model.starts_with("mistral")
        || model.starts_with("codestral")
        || model.starts_with("pixtral")
    {
        providers
            .iter()
            .find(|p| {
                p.id == "mistral" || p.base_url.as_deref().is_some_and(|u| u.contains("mistral"))
            })
            .cloned()
    } else if model.starts_with("worker-")
        || model.contains(':')
        || model.starts_with("llama")
        || model.starts_with("qwen")
        || model.starts_with("phi")
        || model.starts_with("gemma")
        || model.starts_with("nomic")
        || model.starts_with("starcoder")
        || model.starts_with("codellama")
        || model.starts_with("codegemma")
        || model.starts_with("yi-")
        || model.starts_with("orca")
        || model.starts_with("neural-")
        || model.starts_with("wizard")
        || model.starts_with("solar")
        || model.starts_with("nous-")
        || model.starts_with("falcon")
        || model.starts_with("vicuna")
    {
        // Ollama models: local model names that don't match cloud provider prefixes.
        // Detected by Ollama-style name:tag format (contains ':') or known model families.
        providers
            .iter()
            .find(|p| p.kind == ProviderKind::Ollama)
            .cloned()
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
    /// Abort handles for active agent runs, keyed by session_id.
    /// Used by engine_chat_abort to cancel in-flight agent loops.
    pub active_runs: Arc<Mutex<HashMap<String, tokio::task::AbortHandle>>>,
    /// MCP server registry — manages connected MCP servers and their tools.
    pub mcp_registry: Arc<tokio::sync::Mutex<McpRegistry>>,
    /// Tool RAG index — semantic search over tool definitions ("the librarian").
    pub tool_index: Arc<tokio::sync::Mutex<ToolIndex>>,
    /// Tools loaded via request_tools in the current chat turn.
    /// Cleared at the start of each new chat message.
    pub loaded_tools: Arc<Mutex<std::collections::HashSet<String>>>,
    /// Per-session request queue (VS Code pattern).
    /// Messages sent while a request is in progress are queued here.
    pub request_queue: RequestQueue,
    /// Per-session yield signals (VS Code pattern).
    /// When a queued request arrives, the active agent is asked to wrap up.
    pub yield_signals: YieldSignals,
}

impl EngineState {
    pub fn new() -> EngineResult<Self> {
        let store = SessionStore::open()?;

        // Initialize skill vault tables
        store.init_skill_tables()?;

        // Initialize community skills table (skills.sh ecosystem)
        store.init_community_skills_table()?;

        // Load config from DB or use defaults
        let mut config = match store.get_config("engine_config") {
            Ok(Some(json)) => serde_json::from_str::<EngineConfig>(&json).unwrap_or_default(),
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
            Ok(Some(json)) => serde_json::from_str::<MemoryConfig>(&json).unwrap_or_default(),
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
            active_runs: Arc::new(Mutex::new(HashMap::new())),
            mcp_registry: Arc::new(tokio::sync::Mutex::new(McpRegistry::new())),
            tool_index: Arc::new(tokio::sync::Mutex::new(ToolIndex::new())),
            loaded_tools: Arc::new(Mutex::new(HashSet::new())),
            request_queue: Arc::new(Mutex::new(HashMap::new())),
            yield_signals: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Get an EmbeddingClient from the current memory config, if configured.
    pub fn embedding_client(&self) -> Option<EmbeddingClient> {
        let cfg = self.memory_config.lock();
        if cfg.embedding_base_url.is_empty() || cfg.embedding_model.is_empty() {
            return None;
        }
        Some(EmbeddingClient::new(&cfg))
    }
}
