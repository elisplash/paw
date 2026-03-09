// ── Engram: Context Builder ──────────────────────────────────────────────────
//
// Budget-aware context assembly for LLM prompts.
//
// Replaces the ad-hoc system-prompt composition in commands/chat.rs with a
// unified, token-precise pipeline backed by:
//   - Tokenizer (accurate token counting per model)
//   - ModelCapabilities (context window, max output tokens)
//   - WorkingMemory (priority-evicting slot store)
//   - Engram graph search (BM25 + vector + spreading activation)
//
// Budget allocation:
//   ┌──────────────────────────────────────────────┐
//   │  Context Window (model-specific)             │
//   │                                              │
//   │  ┌──────────────────────────────────────────┐│
//   │  │ Max Output Tokens (reserved for reply)   ││
//   │  └──────────────────────────────────────────┘│
//   │  ┌──────────────────────────────────────────┐│
//   │  │ System Prompt     (priority-ordered)     ││
//   │  │  - Platform awareness                    ││
//   │  │  - Foreman protocol                      ││
//   │  │  - Runtime context                       ││
//   │  │  - Soul files (identity / personality)   ││
//   │  │  - Agent roster                          ││
//   │  │  - Working memory slots                  ││
//   │  │  - Auto-recalled memories                ││
//   │  │  - Skill instructions                    ││
//   │  └──────────────────────────────────────────┘│
//   │  ┌──────────────────────────────────────────┐│
//   │  │ Conversation History (recent → old trim) ││
//   │  └──────────────────────────────────────────┘│
//   └──────────────────────────────────────────────┘

use crate::atoms::engram_types::{MemoryScope, MemorySearchConfig, RetrievedMemory};
use crate::atoms::error::EngineResult;
use crate::engine::engram::encryption;
use crate::engine::engram::model_caps::{resolve_injection_resistance, resolve_model_capabilities};
use crate::engine::engram::tokenizer::Tokenizer;
use crate::engine::engram::working_memory::WorkingMemory;
use crate::engine::memory::EmbeddingClient;
use crate::engine::sessions::SessionStore;
use log::{info, warn};
use std::borrow::Cow;

// ═════════════════════════════════════════════════════════════════════════════
// Configuration
// ═════════════════════════════════════════════════════════════════════════════

/// Minimum fraction of context window reserved for conversation history.
const MIN_HISTORY_FRACTION: f32 = 0.35;

/// Maximum fraction of context window for system prompt (including memories).
const MAX_SYSTEM_FRACTION: f32 = 0.45;

/// Minimum tokens always reserved for the model's reply.
const MIN_REPLY_TOKENS: usize = 1024;

/// Default recall BM25+vector similarity threshold.
/// Now superseded at runtime by the self-tuning recall_tuner module (§5),
/// which adapts the threshold based on rolling NDCG quality metrics.
/// Kept as a fallback for the tuner's initial state.
const DEFAULT_RECALL_THRESHOLD: f64 = 0.3;

// ═════════════════════════════════════════════════════════════════════════════
// Public Types
// ═════════════════════════════════════════════════════════════════════════════

/// The assembled context, ready to be sent to the LLM.
#[derive(Debug, Clone)]
pub struct AssembledContext {
    /// The final system prompt (all sections assembled).
    pub system_prompt: Option<String>,
    /// Conversation messages, trimmed to fit budget.
    /// Each entry is (role, content).
    pub messages: Vec<(String, String)>,
    /// Token accounting.
    pub budget: BudgetReport,
    /// Memories that were injected into the system prompt.
    pub recalled_memories: Vec<RetrievedMemory>,
    /// §8.6 Raw query embedding from recall — push into WorkingMemory.push_momentum()
    /// to enable trajectory-aware recall in subsequent turns.
    pub query_embedding: Option<Vec<f32>>,
}

/// Token budget breakdown.
#[derive(Debug, Clone, Default)]
pub struct BudgetReport {
    pub context_window: usize,
    pub max_output_tokens: usize,
    pub system_prompt_tokens: usize,
    pub history_tokens: usize,
    pub available_for_reply: usize,
    pub memories_injected: usize,
    pub messages_included: usize,
    pub messages_trimmed: usize,
}

/// A named section of the system prompt with priority.
/// Lower priority number = higher importance (never dropped first).
#[derive(Debug, Clone)]
struct PromptSection {
    #[allow(dead_code)] // used in debug logging and future prompt-inspector UI
    name: Cow<'static, str>,
    content: String,
    priority: u8, // 0 = highest (never drop), 10 = lowest
    tokens: usize,
    /// Fallback text if this section is dropped for budget.
    fallback: Option<String>,
}

// ═════════════════════════════════════════════════════════════════════════════
// Builder
// ═════════════════════════════════════════════════════════════════════════════

/// Fluent builder for assembling LLM context with precise token budgeting.
pub struct ContextBuilder<'a> {
    #[allow(dead_code)] // reserved for model-specific context strategies
    model: String,
    tokenizer: Tokenizer,
    context_window: usize,
    max_output_tokens: usize,

    // System prompt pieces
    base_prompt: Option<String>,
    runtime_context: Option<String>,
    core_context: Option<String>,
    platform_awareness: Option<String>,
    foreman_protocol: Option<String>,
    skill_instructions: Option<String>,
    agent_roster: Option<String>,
    todays_memories: Option<String>,

    // Consumer-specific custom sections (task context, swarm context, etc.)
    custom_sections: Vec<(String, String, u8)>, // (name, content, priority)

    // Memory retrieval
    store: Option<&'a SessionStore>,
    embedding_client: Option<&'a EmbeddingClient>,
    scope: MemoryScope,
    user_query: Option<String>,
    recall_config: Option<MemorySearchConfig>,
    hnsw_index: Option<&'a super::hnsw::SharedHnswIndex>,

    // Working memory
    working_memory: Option<&'a WorkingMemory>,

    // Conversation history
    messages: Vec<(String, String)>,

    // Override budgets
    context_window_override: Option<usize>,
}

impl<'a> ContextBuilder<'a> {
    /// Create a new context builder for a given model.
    pub fn new(model: &str) -> Self {
        let caps = resolve_model_capabilities(model);
        let tokenizer = Tokenizer::new(caps.tokenizer);
        Self {
            model: model.to_string(),
            tokenizer,
            context_window: caps.context_window,
            max_output_tokens: caps.max_output_tokens,
            base_prompt: None,
            runtime_context: None,
            core_context: None,
            platform_awareness: None,
            foreman_protocol: None,
            skill_instructions: None,
            agent_roster: None,
            todays_memories: None,
            custom_sections: Vec::new(),
            store: None,
            embedding_client: None,
            scope: MemoryScope::default(),
            user_query: None,
            recall_config: None,
            hnsw_index: None,
            working_memory: None,
            messages: Vec::new(),
            context_window_override: None,
        }
    }

    /// Override context window (e.g., from user config).
    pub fn context_window(mut self, tokens: usize) -> Self {
        self.context_window_override = Some(tokens);
        self
    }

    /// Set the base system prompt (agent personality / instructions).
    pub fn base_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.base_prompt = Some(prompt.into());
        self
    }

    /// Set runtime context (model, session, time info).
    pub fn runtime_context(mut self, ctx: impl Into<String>) -> Self {
        self.runtime_context = Some(ctx.into());
        self
    }

    /// Set core context (soul files: IDENTITY.md, USER.md, etc.).
    pub fn core_context(mut self, ctx: impl Into<String>) -> Self {
        self.core_context = Some(ctx.into());
        self
    }

    /// Set platform awareness block.
    pub fn platform_awareness(mut self, text: impl Into<String>) -> Self {
        self.platform_awareness = Some(text.into());
        self
    }

    /// Set Foreman protocol instructions.
    pub fn foreman_protocol(mut self, text: impl Into<String>) -> Self {
        self.foreman_protocol = Some(text.into());
        self
    }

    /// Set skill instructions.
    pub fn skill_instructions(mut self, text: impl Into<String>) -> Self {
        self.skill_instructions = Some(text.into());
        self
    }

    /// Set agent roster.
    pub fn agent_roster(mut self, text: impl Into<String>) -> Self {
        self.agent_roster = Some(text.into());
        self
    }

    /// Set today's memory notes.
    pub fn todays_memories(mut self, text: impl Into<String>) -> Self {
        self.todays_memories = Some(text.into());
        self
    }

    /// Add a custom section with a given name and priority.
    /// Lower priority number = higher importance (never dropped first).
    /// Use this for consumer-specific context like task instructions,
    /// swarm collaboration context, or orchestrator mode.
    pub fn custom_section(
        mut self,
        name: impl Into<String>,
        content: impl Into<String>,
        priority: u8,
    ) -> Self {
        self.custom_sections
            .push((name.into(), content.into(), priority));
        self
    }

    /// Enable memory auto-recall from the Engram store.
    pub fn recall_from(
        mut self,
        store: &'a SessionStore,
        embedding_client: Option<&'a EmbeddingClient>,
        scope: MemoryScope,
        query: impl Into<String>,
    ) -> Self {
        self.store = Some(store);
        self.embedding_client = embedding_client;
        self.scope = scope;
        self.user_query = Some(query.into());
        self
    }

    /// Set custom recall configuration.
    pub fn recall_config(mut self, config: MemorySearchConfig) -> Self {
        self.recall_config = Some(config);
        self
    }

    /// Set the HNSW vector index for O(log n) approximate nearest-neighbor search.
    pub fn hnsw_index(mut self, index: &'a super::hnsw::SharedHnswIndex) -> Self {
        self.hnsw_index = Some(index);
        self
    }

    /// Inject working memory slots.
    pub fn working_memory(mut self, wm: &'a WorkingMemory) -> Self {
        self.working_memory = Some(wm);
        self
    }

    /// Set conversation history. Order: oldest → newest.
    pub fn messages(mut self, messages: Vec<(String, String)>) -> Self {
        self.messages = messages;
        self
    }

    /// Build the assembled context.
    pub async fn build(self) -> EngineResult<AssembledContext> {
        let effective_window = self.context_window_override.unwrap_or(self.context_window);
        let reply_reserve = self.max_output_tokens.max(MIN_REPLY_TOKENS);
        let usable_tokens = effective_window.saturating_sub(reply_reserve);

        // ── Budget partitions ────────────────────────────────────────────
        let max_system =
            ((effective_window as f32 * MAX_SYSTEM_FRACTION) as usize).min(usable_tokens);
        let min_history = (effective_window as f32 * MIN_HISTORY_FRACTION) as usize;

        // ── 1. Collect system prompt sections ────────────────────────────
        let mut sections = self.collect_sections();

        // ── 2. Auto-recall memories via gated_search (§7) ────────────────
        // Route through the full gated_search pipeline to preserve:
        //   - Gate decision (Skip/Retrieve/DeepRetrieve/Refuse/Defer)
        //   - CRAG 3-tier quality checking (Correct/Ambiguous/Incorrect)
        //   - Intent classification & signal weighting
        //   - Query decomposition and escalating recovery
        //   - Capability token verification (§43.4)
        let mut recalled_memories = Vec::new();
        let mut recall_query_embedding: Option<Vec<f32>> = None;
        if let (Some(store), Some(query)) = (self.store, &self.user_query) {
            let mut config = self.recall_config.clone().unwrap_or_default();

            // §5 Self-tuning: override the static similarity_threshold with the
            // recall tuner's adapted value if it differs from the default.
            let adapted_threshold = super::recall_tuner::current_threshold();
            if (adapted_threshold - DEFAULT_RECALL_THRESHOLD).abs() > 0.001 {
                config.similarity_threshold = adapted_threshold as f32;
            }

            // §8.6 Pass momentum embeddings from working memory for trajectory-aware recall
            let momentum: Option<Vec<Vec<f32>>> = self
                .working_memory
                .filter(|wm| !wm.momentum().is_empty())
                .map(|wm| wm.momentum().to_vec());
            let mom_ref: Option<&[Vec<f32>]> = momentum.as_deref();

            // Issue a read-path capability token for scope verification (§43.4)
            let agent_id = self.scope.agent_id.as_deref().unwrap_or("default");
            let read_cap = super::memory_bus::issue_read_capability(agent_id).ok();

            match super::gated_search::gated_search(
                store,
                &super::gated_search::GatedSearchRequest {
                    query,
                    scope: &self.scope,
                    config: &config,
                    embedding_client: self.embedding_client,
                    budget_tokens: max_system,
                    momentum: mom_ref,
                    model: Some(&self.model),
                    capability: read_cap.as_ref(),
                    hnsw_index: self.hnsw_index,
                },
            )
            .await
            {
                Ok(result) => {
                    // §8.6 Extract query embedding from the recalled memories for
                    // momentum tracking (gated_search doesn't expose the raw embedding
                    // directly, so we re-embed if needed for trajectory recall)
                    if let Some(emb_client) = self.embedding_client {
                        if let Ok(emb) = emb_client.embed(query).await {
                            recall_query_embedding = Some(emb);
                        }
                    }

                    match result.gate {
                        super::gated_search::GateDecision::Skip => {
                            // No memories needed for this query — intentional empty
                        }
                        super::gated_search::GateDecision::Refuse => {
                            warn!(
                                "[engram:context] CRAG quality gate refused results for '{}'",
                                encryption::safe_log_preview(query, 50)
                            );
                            // Don't inject bad memories — leave recalled_memories empty
                        }
                        super::gated_search::GateDecision::Defer(ref reason) => {
                            info!(
                                "[engram:context] Gate deferred for '{}': {:?}",
                                encryption::safe_log_preview(query, 50),
                                reason
                            );
                            // Don't inject — caller can surface disambiguation_hint if needed
                        }
                        _ => {
                            // Retrieve / DeepRetrieve — use the memories
                            let resistance = resolve_injection_resistance(&self.model);
                            recalled_memories = result
                                .memories
                                .into_iter()
                                .map(|mut m| {
                                    // Decrypt with per-agent derived key (HKDF isolation)
                                    if let Ok(key) =
                                        encryption::get_agent_encryption_key(&m.agent_id)
                                    {
                                        m.content =
                                            encryption::decrypt_memory_content(&m.content, &key)
                                                .unwrap_or(m.content);
                                    }
                                    // Level-aware sanitization (Standard/Strict/Paranoid per model tier)
                                    m.content = encryption::sanitize_recalled_memory_at_level(
                                        &m.content,
                                        resistance.sanitization_level,
                                    );
                                    // Per-model content length cap (§58.5)
                                    if m.content.len() > resistance.max_memory_content_chars {
                                        let mut end = resistance.max_memory_content_chars;
                                        while end > 0 && !m.content.is_char_boundary(end) {
                                            end -= 1;
                                        }
                                        m.content = format!("{}…[truncated]", &m.content[..end]);
                                    }
                                    m
                                })
                                .collect();
                            recalled_memories.truncate(resistance.max_recalled_memories);
                        }
                    }
                }
                Err(e) => {
                    warn!("[engram:context] Gated memory recall failed: {}", e);
                }
            }
        }

        // Format recalled memories into a section
        if !recalled_memories.is_empty() {
            let recall_text = format_recalled_memories(&recalled_memories);
            let recall_tokens = self.tokenizer.count_tokens(&recall_text);
            sections.push(PromptSection {
                name: Cow::Borrowed("recalled_memories"),
                content: recall_text,
                priority: 7,
                tokens: recall_tokens,
                fallback: Some("Use memory_search to recall relevant information.".to_string()),
            });
        }

        // ── 3. Working memory slots ──────────────────────────────────────
        if let Some(wm) = self.working_memory {
            let wm_text = wm.format_for_context();
            if !wm_text.is_empty() {
                let wm_tokens = self.tokenizer.count_tokens(&wm_text);
                sections.push(PromptSection {
                    name: Cow::Borrowed("working_memory"),
                    content: wm_text,
                    priority: 5, // higher priority than recalled memories
                    tokens: wm_tokens,
                    fallback: None,
                });
            }
        }

        // ── 4. Assemble system prompt within budget ──────────────────────
        sections.sort_by_key(|s| s.priority);
        let (system_prompt, system_tokens) =
            assemble_sections(&sections, max_system, &self.tokenizer);

        // ── 5. Budget conversation history ───────────────────────────────
        let history_budget = usable_tokens.saturating_sub(system_tokens).max(min_history);
        let (trimmed_messages, history_tokens, trimmed_count) =
            trim_history(&self.messages, history_budget, &self.tokenizer);

        let available_for_reply = effective_window
            .saturating_sub(system_tokens)
            .saturating_sub(history_tokens);

        let budget = BudgetReport {
            context_window: effective_window,
            max_output_tokens: self.max_output_tokens,
            system_prompt_tokens: system_tokens,
            history_tokens,
            available_for_reply,
            memories_injected: recalled_memories.len(),
            messages_included: trimmed_messages.len(),
            messages_trimmed: trimmed_count,
        };

        info!(
            "[engram:context] Budget: sys={}tok hist={}tok reply={}tok mem={} msgs={}/{}",
            budget.system_prompt_tokens,
            budget.history_tokens,
            budget.available_for_reply,
            budget.memories_injected,
            budget.messages_included,
            budget.messages_included + budget.messages_trimmed,
        );

        Ok(AssembledContext {
            system_prompt,
            messages: trimmed_messages,
            budget,
            recalled_memories,
            query_embedding: recall_query_embedding,
        })
    }

    /// Collect all configured sections into PromptSection entries.
    fn collect_sections(&self) -> Vec<PromptSection> {
        let mut sections = Vec::new();

        if let Some(ref text) = self.platform_awareness {
            sections.push(PromptSection {
                name: Cow::Borrowed("platform_awareness"),
                content: text.clone(),
                priority: 0,
                tokens: self.tokenizer.count_tokens(text),
                fallback: None,
            });
        }

        if let Some(ref text) = self.foreman_protocol {
            sections.push(PromptSection {
                name: Cow::Borrowed("foreman_protocol"),
                content: text.clone(),
                priority: 0,
                tokens: self.tokenizer.count_tokens(text),
                fallback: None,
            });
        }

        if let Some(ref text) = self.runtime_context {
            sections.push(PromptSection {
                name: Cow::Borrowed("runtime_context"),
                content: text.clone(),
                priority: 1,
                tokens: self.tokenizer.count_tokens(text),
                fallback: None,
            });
        }

        if let Some(ref text) = self.core_context {
            sections.push(PromptSection {
                name: Cow::Borrowed("soul_files"),
                content: text.clone(),
                priority: 2,
                tokens: self.tokenizer.count_tokens(text),
                fallback: None,
            });
        }

        if let Some(ref text) = self.base_prompt {
            sections.push(PromptSection {
                name: Cow::Borrowed("base_prompt"),
                content: text.clone(),
                priority: 3,
                tokens: self.tokenizer.count_tokens(text),
                fallback: None,
            });
        }

        if let Some(ref text) = self.agent_roster {
            sections.push(PromptSection {
                name: Cow::Borrowed("agent_roster"),
                content: text.clone(),
                priority: 4,
                tokens: self.tokenizer.count_tokens(text),
                fallback: Some("Use agent_list to see available agents.".to_string()),
            });
        }

        if let Some(ref text) = self.todays_memories {
            sections.push(PromptSection {
                name: Cow::Borrowed("todays_memories"),
                content: format!("## Today's Memory Notes\n{}", text),
                priority: 6,
                tokens: self.tokenizer.count_tokens(text) + 10,
                fallback: Some("Use memory_search to find stored information.".to_string()),
            });
        }

        if let Some(ref text) = self.skill_instructions {
            sections.push(PromptSection {
                name: Cow::Borrowed("skill_instructions"),
                content: text.clone(),
                priority: 8,
                tokens: self.tokenizer.count_tokens(text),
                fallback: Some("Use request_tools to get relevant skill instructions.".to_string()),
            });
        }

        // Custom sections from consumers (task context, swarm context, etc.)
        for (name, content, priority) in &self.custom_sections {
            sections.push(PromptSection {
                name: Cow::Owned(name.clone()),
                content: content.clone(),
                priority: *priority,
                tokens: self.tokenizer.count_tokens(content),
                fallback: None,
            });
        }

        sections
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Internal: Section Assembly
// ═════════════════════════════════════════════════════════════════════════════

/// Assemble prompt sections into a single string, respecting token budget.
/// Sections should be pre-sorted by priority (ascending = most important first).
/// Returns (assembled_prompt, total_tokens).
fn assemble_sections(
    sections: &[PromptSection],
    budget: usize,
    tokenizer: &Tokenizer,
) -> (Option<String>, usize) {
    if sections.is_empty() {
        return (None, 0);
    }

    let separator = "\n\n---\n\n";
    let separator_tokens = tokenizer.count_tokens(separator);

    let mut included: Vec<&str> = Vec::new();
    let mut fallbacks: Vec<String> = Vec::new();
    let mut used_tokens = 0usize;

    for section in sections {
        let cost = section.tokens
            + if included.is_empty() {
                0
            } else {
                separator_tokens
            };
        if used_tokens + cost <= budget {
            included.push(&section.content);
            used_tokens += cost;
        } else if let Some(ref fb) = section.fallback {
            // Section doesn't fit — use fallback if available
            let fb_cost = tokenizer.count_tokens(fb)
                + if included.is_empty() && fallbacks.is_empty() {
                    0
                } else {
                    separator_tokens
                };
            if used_tokens + fb_cost <= budget {
                fallbacks.push(fb.clone());
                used_tokens += fb_cost;
            }
        }
        // else: section dropped entirely
    }

    if included.is_empty() && fallbacks.is_empty() {
        return (None, 0);
    }

    let mut parts: Vec<&str> = included;
    let fallback_refs: Vec<&str> = fallbacks.iter().map(|s| s.as_str()).collect();
    parts.extend(fallback_refs);

    let assembled = parts.join(separator);
    let total = tokenizer.count_tokens(&assembled);

    (Some(assembled), total)
}

// ═════════════════════════════════════════════════════════════════════════════
// Internal: Memory Formatting
// ═════════════════════════════════════════════════════════════════════════════

/// Format recalled memories into a readable section.
fn format_recalled_memories(memories: &[RetrievedMemory]) -> String {
    let mut lines = vec!["## Relevant Memories".to_string()];
    for mem in memories {
        let category_tag = if mem.category.is_empty() {
            String::new()
        } else {
            format!("[{}] ", mem.category)
        };
        let content = truncate_str(&mem.content, 300);
        let score_tag = format!(" (trust: {:.2})", mem.trust_score.composite());
        lines.push(format!("- {}{}{}", category_tag, content, score_tag));
    }
    lines.join("\n")
}

fn truncate_str(s: &str, max_chars: usize) -> &str {
    if s.len() <= max_chars {
        s
    } else {
        // Find a safe char boundary
        let mut end = max_chars;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Internal: History Trimming
// ═════════════════════════════════════════════════════════════════════════════

/// Trim conversation history to fit within a token budget.
/// Keeps recent messages, drops oldest first.
/// Returns (trimmed_messages, total_tokens, dropped_count).
fn trim_history(
    messages: &[(String, String)],
    budget: usize,
    tokenizer: &Tokenizer,
) -> (Vec<(String, String)>, usize, usize) {
    if messages.is_empty() {
        return (Vec::new(), 0, 0);
    }

    // Count tokens per message (role overhead + content)
    let per_message_overhead = 4; // role tokens + formatting
    let costs: Vec<usize> = messages
        .iter()
        .map(|(role, content)| {
            tokenizer.count_tokens(role) + tokenizer.count_tokens(content) + per_message_overhead
        })
        .collect();

    let total: usize = costs.iter().sum();
    if total <= budget {
        return (messages.to_vec(), total, 0);
    }

    // Drop oldest messages until we fit
    let mut start = 0;
    let mut running = total;
    while running > budget && start < messages.len() {
        running -= costs[start];
        start += 1;
    }

    let kept = messages[start..].to_vec();
    let kept_tokens: usize = costs[start..].iter().sum();

    (kept, kept_tokens, start)
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    fn make_tokenizer() -> Tokenizer {
        Tokenizer::heuristic()
    }

    #[test]
    fn test_assemble_sections_empty() {
        let tok = make_tokenizer();
        let (result, tokens) = assemble_sections(&[], 1000, &tok);
        assert!(result.is_none());
        assert_eq!(tokens, 0);
    }

    #[test]
    fn test_assemble_sections_fits() {
        let tok = make_tokenizer();
        let sections = vec![PromptSection {
            name: Cow::Borrowed("test"),
            content: "Hello world".to_string(),
            priority: 0,
            tokens: tok.count_tokens("Hello world"),
            fallback: None,
        }];
        let (result, tokens) = assemble_sections(&sections, 1000, &tok);
        assert_eq!(result.as_deref(), Some("Hello world"));
        assert!(tokens > 0);
    }

    #[test]
    fn test_assemble_sections_drops_low_priority() {
        let tok = make_tokenizer();
        let long_text = "x".repeat(2000);
        let sections = vec![
            PromptSection {
                name: Cow::Borrowed("critical"),
                content: "Important".to_string(),
                priority: 0,
                tokens: tok.count_tokens("Important"),
                fallback: None,
            },
            PromptSection {
                name: Cow::Borrowed("optional"),
                content: long_text,
                priority: 10,
                tokens: 600, // won't fit
                fallback: Some("Use search.".to_string()),
            },
        ];
        // Budget of 20 tokens — only "Important" fits
        let (result, _) = assemble_sections(&sections, 20, &tok);
        let text = result.unwrap();
        assert!(text.contains("Important"));
        // The fallback might or might not fit at 20 tokens
    }

    #[test]
    fn test_trim_history_all_fit() {
        let tok = make_tokenizer();
        let messages = vec![
            ("user".to_string(), "Hello".to_string()),
            ("assistant".to_string(), "Hi there!".to_string()),
        ];
        let (kept, tokens, dropped) = trim_history(&messages, 10000, &tok);
        assert_eq!(kept.len(), 2);
        assert_eq!(dropped, 0);
        assert!(tokens > 0);
    }

    #[test]
    fn test_trim_history_drops_oldest() {
        let tok = make_tokenizer();
        let messages = vec![
            (
                "user".to_string(),
                "First message that is somewhat long".to_string(),
            ),
            ("assistant".to_string(), "Second".to_string()),
            ("user".to_string(), "Third".to_string()),
        ];
        // Very tight budget — should drop the first message
        let (kept, _, dropped) = trim_history(&messages, 15, &tok);
        assert!(dropped > 0, "Should have dropped at least one message");
        assert!(kept.len() < 3);
    }

    #[test]
    fn test_trim_history_empty() {
        let tok = make_tokenizer();
        let (kept, tokens, dropped) = trim_history(&[], 1000, &tok);
        assert!(kept.is_empty());
        assert_eq!(tokens, 0);
        assert_eq!(dropped, 0);
    }

    #[test]
    fn test_format_recalled_memories() {
        use crate::atoms::engram_types::{CompressionLevel, MemoryType, TrustScore};
        let memories = vec![RetrievedMemory {
            memory_id: "mem-1".to_string(),
            content: "The user prefers dark mode".to_string(),
            compression_level: CompressionLevel::Full,
            category: "preference".to_string(),
            memory_type: MemoryType::Episodic,
            trust_score: TrustScore {
                relevance: 0.85,
                accuracy: 0.9,
                freshness: 0.8,
                utility: 0.7,
            },
            token_cost: 10,
            created_at: "2025-01-01T00:00:00Z".to_string(),
            agent_id: String::new(),
        }];
        let formatted = format_recalled_memories(&memories);
        assert!(formatted.contains("## Relevant Memories"));
        assert!(formatted.contains("[preference]"));
        assert!(formatted.contains("dark mode"));
    }

    #[test]
    fn test_truncate_str() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world", 5), "hello");
    }

    #[test]
    fn test_budget_report_defaults() {
        let report = BudgetReport::default();
        assert_eq!(report.context_window, 0);
        assert_eq!(report.memories_injected, 0);
    }
}
