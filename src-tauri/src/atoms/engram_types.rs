// ── Paw Atoms: Engram Memory System Types ──────────────────────────────────
//
// Type definitions for Project Engram — the biologically-inspired memory system.
// These are pure data types (no logic, no DB access, no I/O).
//
// Follows the project pattern: structs in atoms/, impls in engine/.
// Old `Memory` / `MemoryConfig` types remain in types.rs for backward compat.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════════════════
// SECTION 1: Memory Scoping
// ═══════════════════════════════════════════════════════════════════════════

/// Hierarchical memory scope — controls who can see/write memories.
///
/// Scope resolution order (most specific → least specific):
///   channel_user → channel → agent → squad → project → global
///
/// A memory stored at `agent` scope is visible to that agent and
/// anyone with a broader scope (project, global), but NOT to other
/// agents unless they share the same project/squad scope.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MemoryScope {
    /// If Some, this memory is global (visible to all agents).
    #[serde(default)]
    pub global: bool,
    /// Project ID — memories shared within a project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Squad ID — memories shared within an orchestrator squad.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub squad_id: Option<String>,
    /// Agent ID — agent-scoped memories (the default for most operations).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Channel name — memories scoped to a specific channel (Discord, Slack, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    /// Channel user ID — per-user within a channel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_user_id: Option<String>,
}

impl MemoryScope {
    /// Create a global scope (visible everywhere).
    pub fn global() -> Self {
        Self {
            global: true,
            ..Default::default()
        }
    }

    /// Create an agent-scoped memory.
    pub fn agent(agent_id: &str) -> Self {
        Self {
            agent_id: Some(agent_id.to_string()),
            ..Default::default()
        }
    }

    /// Create a project-scoped memory (visible to all agents in the project).
    pub fn project(project_id: &str) -> Self {
        Self {
            project_id: Some(project_id.to_string()),
            ..Default::default()
        }
    }

    /// Create a squad-scoped memory (orchestrator squad).
    pub fn squad(squad_id: &str, project_id: &str) -> Self {
        Self {
            squad_id: Some(squad_id.to_string()),
            project_id: Some(project_id.to_string()),
            ..Default::default()
        }
    }

    /// Create a channel+user-scoped memory.
    pub fn channel_user(channel: &str, user_id: &str, agent_id: &str) -> Self {
        Self {
            channel: Some(channel.to_string()),
            channel_user_id: Some(user_id.to_string()),
            agent_id: Some(agent_id.to_string()),
            ..Default::default()
        }
    }

    /// Build a SQL WHERE clause fragment for this scope.
    /// Returns (clause, params) where params are positional ($1, $2, etc.).
    pub fn to_sql_where(&self) -> (String, Vec<String>) {
        let mut conditions = Vec::new();
        let mut params = Vec::new();

        if self.global {
            // Global scope sees everything — no WHERE restriction
            return ("1=1".to_string(), vec![]);
        }

        if let Some(ref pid) = self.project_id {
            conditions.push("(scope_project_id = ?{} OR scope_global = 1)".to_string());
            params.push(pid.clone());
        }
        if let Some(ref sid) = self.squad_id {
            conditions.push("(scope_squad_id = ?{} OR scope_squad_id = '')".to_string());
            params.push(sid.clone());
        }
        if let Some(ref aid) = self.agent_id {
            conditions.push("(agent_id = ?{} OR agent_id = '' OR scope_global = 1)".to_string());
            params.push(aid.clone());
        }
        if let Some(ref ch) = self.channel {
            conditions.push("(scope_channel = ?{} OR scope_channel = '')".to_string());
            params.push(ch.clone());
        }
        if let Some(ref uid) = self.channel_user_id {
            conditions
                .push("(scope_channel_user_id = ?{} OR scope_channel_user_id = '')".to_string());
            params.push(uid.clone());
        }

        if conditions.is_empty() {
            // No scope specified = agent-only default (match empty agent_id)
            return ("(scope_global = 1 OR agent_id = '')".to_string(), vec![]);
        }

        // Number the placeholders
        let mut numbered = Vec::new();
        for (i, cond) in conditions.iter().enumerate() {
            numbered.push(cond.replace("?{}", &format!("?{}", i + 1)));
        }

        (numbered.join(" AND "), params)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SECTION 2: Memory Types (Episodic, Semantic, Procedural)
// ═══════════════════════════════════════════════════════════════════════════

/// The source of a memory — how it was created.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum MemorySource {
    /// Extracted from a conversation automatically.
    AutoCapture,
    /// Stored explicitly by the user or agent via tool/command.
    Explicit,
    /// Result of a task or cron job.
    TaskResult { task_id: String },
    /// Discovered during research.
    ResearchDiscovery { urls: Vec<String>, query: String },
    /// Created by consolidation (merging episodic → semantic).
    Consolidation,
    /// Inferred from graph relationships.
    Inference,
    /// Imported from a skill.
    Skill { skill_id: String },
    /// Migrated from the legacy `memories` table.
    LegacyMigration,
}

impl Default for MemorySource {
    fn default() -> Self {
        MemorySource::AutoCapture
    }
}

/// Consolidation state for episodic memories.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ConsolidationState {
    /// Just captured, not yet processed.
    Fresh,
    /// Processed by the consolidation engine.
    Consolidated,
    /// Superseded by a newer version or merged into semantic memory.
    Archived,
}

impl Default for ConsolidationState {
    fn default() -> Self {
        ConsolidationState::Fresh
    }
}

/// Multi-dimensional trust score for retrieved memories.
/// Each dimension is 0.0–1.0.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct TrustScore {
    /// How relevant is this memory to the current query?
    pub relevance: f32,
    /// How accurate/reliable is this memory? (calibrated by feedback)
    pub accuracy: f32,
    /// How fresh is this memory? (decays over time)
    pub freshness: f32,
    /// How useful has this memory been in past retrievals?
    pub utility: f32,
}

impl TrustScore {
    /// Composite score — weighted average of all dimensions.
    pub fn composite(&self) -> f32 {
        // Relevance dominates, but accuracy and utility matter
        self.relevance * 0.4 + self.accuracy * 0.25 + self.freshness * 0.15 + self.utility * 0.2
    }

    /// Create a TrustScore from a single similarity value (for migration).
    pub fn from_similarity(sim: f32) -> Self {
        Self {
            relevance: sim,
            accuracy: 0.5,  // neutral
            freshness: 1.0, // just created
            utility: 0.5,   // unknown
        }
    }

    /// Apply time-based decay to the freshness dimension.
    /// `days_since_creation` — how old this memory is.
    /// `half_life_days` — how many days until freshness halves.
    pub fn apply_freshness_decay(&mut self, days_since_creation: f64, half_life_days: f64) {
        if half_life_days <= 0.0 {
            return;
        }
        let decay = (-days_since_creation * (2.0_f64.ln()) / half_life_days).exp() as f32;
        self.freshness = (self.freshness * decay).clamp(0.0, 1.0);
    }

    /// Apply negative feedback — reduces accuracy and utility.
    /// Called when a user or system marks a retrieved memory as unhelpful.
    pub fn apply_negative_feedback(&mut self) {
        self.accuracy = (self.accuracy - 0.15).max(0.0);
        self.utility = (self.utility - 0.1).max(0.0);
    }

    /// Apply positive feedback — boosts accuracy and utility.
    pub fn apply_positive_feedback(&mut self) {
        self.accuracy = (self.accuracy + 0.1).min(1.0);
        self.utility = (self.utility + 0.08).min(1.0);
    }

    /// Check if this memory should be filtered out due to low trust.
    /// Memories with very low composite scores are noise.
    pub fn should_filter(&self, threshold: f32) -> bool {
        self.composite() < threshold
    }
}

/// Tiered content — a memory exists at multiple compression levels simultaneously.
/// Under token pressure, we use the most compact level that fits.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TieredContent {
    /// Full original content.
    pub full: String,
    /// Summary (~50% of original tokens).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Single key fact (~1 sentence).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_fact: Option<String>,
    /// Tags only (~2-5 words).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
}

impl TieredContent {
    /// Create tiered content from raw text (only full level populated initially).
    pub fn from_text(text: &str) -> Self {
        Self {
            full: text.to_string(),
            summary: None,
            key_fact: None,
            tags: None,
        }
    }

    /// Get the best content at the requested compression level.
    pub fn at_level(&self, level: CompressionLevel) -> &str {
        match level {
            CompressionLevel::Full => &self.full,
            CompressionLevel::Summary => self.summary.as_deref().unwrap_or(&self.full),
            CompressionLevel::KeyFact => self
                .key_fact
                .as_deref()
                .or(self.summary.as_deref())
                .unwrap_or(&self.full),
            CompressionLevel::TagOnly => self
                .tags
                .as_deref()
                .or(self.key_fact.as_deref())
                .or(self.summary.as_deref())
                .unwrap_or(&self.full),
        }
    }

    /// Check if all compression levels are populated.
    pub fn is_fully_tiered(&self) -> bool {
        self.summary.is_some() && self.key_fact.is_some() && self.tags.is_some()
    }
}

/// Compression level for tiered content.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum CompressionLevel {
    Full = 0,
    Summary = 1,
    KeyFact = 2,
    TagOnly = 3,
}

// ── Episodic Memory ─────────────────────────────────────────────────────

/// An episodic memory — a record of a specific event/interaction.
/// The raw material from which semantic memories are distilled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodicMemory {
    pub id: String,
    /// The event content (what happened).
    pub content: TieredContent,
    /// Optional outcome (what resulted from this event).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    /// Category for filtering.
    pub category: String,
    /// Importance score (0.0–1.0). Higher = more important.
    pub importance: f32,
    /// The agent that created this memory.
    pub agent_id: String,
    /// Session in which this memory was created.
    pub session_id: String,
    /// How this memory was created.
    pub source: MemorySource,
    /// Consolidation state.
    pub consolidation_state: ConsolidationState,
    /// Memory strength (Ebbinghaus decay). Starts at 1.0, decays over time.
    /// Strengthened by retrieval (spacing effect).
    pub strength: f32,
    /// Scope — who can see this memory.
    pub scope: MemoryScope,
    /// Embedding vector (None if not yet computed).
    #[serde(skip)]
    pub embedding: Option<Vec<f32>>,
    /// Embedding model used (for migration tracking).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding_model: Option<String>,
    /// Negative feedback contexts — queries where this memory was marked wrong.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub negative_contexts: Vec<String>,
    /// Creation timestamp (ISO 8601).
    pub created_at: String,
    /// Last accessed timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_accessed_at: Option<String>,
    /// Number of times retrieved.
    #[serde(default)]
    pub access_count: u32,
}

impl Default for EpisodicMemory {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            content: TieredContent::default(),
            outcome: None,
            category: "general".into(),
            importance: 0.5,
            agent_id: String::new(),
            session_id: String::new(),
            source: MemorySource::default(),
            consolidation_state: ConsolidationState::default(),
            strength: 1.0,
            scope: MemoryScope::default(),
            embedding: None,
            embedding_model: None,
            negative_contexts: Vec::new(),
            created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            last_accessed_at: None,
            access_count: 0,
        }
    }
}

// ── Semantic Memory ─────────────────────────────────────────────────────

/// A semantic memory — distilled knowledge extracted from one or more
/// episodic memories. Stored as subject-predicate-object triples.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticMemory {
    pub id: String,
    /// Subject of the knowledge triple.
    pub subject: String,
    /// Predicate (relation).
    pub predicate: String,
    /// Object of the knowledge triple.
    pub object: String,
    /// Full text representation for search/display.
    pub full_text: String,
    pub category: String,
    /// Confidence in this knowledge (0.0–1.0).
    pub confidence: f32,
    /// Was this explicitly stated by the user (vs. inferred)?
    pub is_user_explicit: bool,
    /// If this contradicts another semantic memory, link to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contradiction_of: Option<String>,
    /// Scope.
    pub scope: MemoryScope,
    /// Embedding vector.
    #[serde(skip)]
    pub embedding: Option<Vec<f32>>,
    pub embedding_model: Option<String>,
    /// Version for reconsolidation tracking.
    pub version: u32,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl Default for SemanticMemory {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            subject: String::new(),
            predicate: String::new(),
            object: String::new(),
            full_text: String::new(),
            category: "general".into(),
            confidence: 0.5,
            is_user_explicit: false,
            contradiction_of: None,
            scope: MemoryScope::default(),
            embedding: None,
            embedding_model: None,
            version: 1,
            created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            updated_at: None,
        }
    }
}

// ── Procedural Memory ───────────────────────────────────────────────────

/// A procedural memory — a learned pattern of behavior (how to do things).
/// Extracted from repeated successful tool-use sequences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProceduralMemory {
    pub id: String,
    /// What triggers this procedure (e.g., "user asks to deploy").
    pub trigger: String,
    /// Ordered steps in the procedure.
    pub steps: Vec<ProceduralStep>,
    /// Success rate from past executions.
    pub success_rate: f32,
    /// How many times this procedure has been executed.
    pub execution_count: u32,
    pub scope: MemoryScope,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// A single step in a procedural memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProceduralStep {
    /// Description of what this step does.
    pub description: String,
    /// Tool name (if this step involves a tool call).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Typical arguments pattern.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args_pattern: Option<String>,
    /// Expected outcome description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_outcome: Option<String>,
}

impl Default for ProceduralMemory {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            trigger: String::new(),
            steps: Vec::new(),
            success_rate: 0.0,
            execution_count: 0,
            scope: MemoryScope::default(),
            created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            updated_at: None,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SECTION 3: Memory Graph Edges
// ═══════════════════════════════════════════════════════════════════════════

/// Edge type in the memory graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EdgeType {
    /// Source memory was consolidated into target.
    ConsolidatedInto,
    /// Source contradicts target.
    Contradicts,
    /// Source supports / reinforces target.
    SupportedBy,
    /// Source supersedes target (updated version).
    Supersedes,
    /// Source is causally related to target.
    CausedBy,
    /// Source is temporally adjacent to target.
    TemporallyAdjacent,
    /// Source and target share the same topic/entity.
    RelatedTo,
    /// Source was inferred from target (transitive inference).
    InferredFrom,
    /// Procedural memory linked to the episodic event that spawned it.
    LearnedFrom,
    /// Source is an example of target.
    ExampleOf,
    /// Source is a part of target (compositional).
    PartOf,
}

impl std::fmt::Display for EdgeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EdgeType::ConsolidatedInto => write!(f, "consolidated_into"),
            EdgeType::Contradicts => write!(f, "contradicts"),
            EdgeType::SupportedBy => write!(f, "supported_by"),
            EdgeType::Supersedes => write!(f, "supersedes"),
            EdgeType::CausedBy => write!(f, "caused_by"),
            EdgeType::TemporallyAdjacent => write!(f, "temporally_adjacent"),
            EdgeType::RelatedTo => write!(f, "related_to"),
            EdgeType::InferredFrom => write!(f, "inferred_from"),
            EdgeType::LearnedFrom => write!(f, "learned_from"),
            EdgeType::ExampleOf => write!(f, "example_of"),
            EdgeType::PartOf => write!(f, "part_of"),
        }
    }
}

impl std::str::FromStr for EdgeType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "consolidated_into" => Ok(EdgeType::ConsolidatedInto),
            "contradicts" => Ok(EdgeType::Contradicts),
            "supported_by" | "supports" => Ok(EdgeType::SupportedBy),
            "supersedes" => Ok(EdgeType::Supersedes),
            "caused_by" => Ok(EdgeType::CausedBy),
            "temporally_adjacent" => Ok(EdgeType::TemporallyAdjacent),
            "related_to" => Ok(EdgeType::RelatedTo),
            "inferred_from" => Ok(EdgeType::InferredFrom),
            "learned_from" => Ok(EdgeType::LearnedFrom),
            "example_of" => Ok(EdgeType::ExampleOf),
            "part_of" => Ok(EdgeType::PartOf),
            _ => Err(format!("Unknown edge type: {}", s)),
        }
    }
}

/// An edge in the memory graph connecting two memories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEdge {
    pub source_id: String,
    pub target_id: String,
    pub edge_type: EdgeType,
    /// Weight/confidence of this edge (0.0–1.0).
    pub weight: f32,
    pub created_at: String,
}

// ═══════════════════════════════════════════════════════════════════════════
// SECTION 4: Retrieval Types
// ═══════════════════════════════════════════════════════════════════════════

/// A memory retrieved from search, with scoring metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievedMemory {
    /// The memory content (at chosen compression level).
    pub content: String,
    /// Which compression level was used.
    pub compression_level: CompressionLevel,
    /// Original memory ID.
    pub memory_id: String,
    /// Memory type for display.
    pub memory_type: MemoryType,
    /// Multi-dimensional trust score.
    pub trust_score: TrustScore,
    /// Token cost of this retrieval at the current compression level.
    pub token_cost: usize,
    /// Category.
    pub category: String,
    /// When this memory was created.
    pub created_at: String,
}

/// Which type of memory store a retrieved result came from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum MemoryType {
    Episodic,
    Semantic,
    Procedural,
}

impl std::fmt::Display for MemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryType::Episodic => write!(f, "episodic"),
            MemoryType::Semantic => write!(f, "semantic"),
            MemoryType::Procedural => write!(f, "procedural"),
        }
    }
}

/// Search configuration — all tunable search parameters.
/// Sent from frontend to backend; backend no longer hardcodes ANY search values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchConfig {
    /// BM25 text search weight (0.0–1.0).
    pub bm25_weight: f32,
    /// Vector similarity weight (0.0–1.0).
    pub vector_weight: f32,
    /// MMR diversity parameter (0.0 = max diversity, 1.0 = max relevance).
    pub mmr_lambda: f32,
    /// Temporal decay half-life in days.
    pub decay_half_life_days: f32,
    /// Minimum similarity threshold for inclusion.
    pub similarity_threshold: f32,
    /// Whether to apply reranking after initial retrieval.
    pub rerank_enabled: bool,
    /// Which reranking strategy to use.
    pub rerank_strategy: RerankStrategy,
    /// Hybrid search text-boost configuration.
    pub hybrid: HybridSearchConfig,
}

impl Default for MemorySearchConfig {
    fn default() -> Self {
        Self {
            bm25_weight: 0.4,
            vector_weight: 0.6,
            mmr_lambda: 0.7,
            decay_half_life_days: 30.0,
            similarity_threshold: 0.3,
            rerank_enabled: true,
            rerank_strategy: RerankStrategy::default(),
            hybrid: HybridSearchConfig::default(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SECTION 5: Model Capabilities
// ═══════════════════════════════════════════════════════════════════════════

/// Provider type for a model.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ModelProvider {
    OpenAI,
    Anthropic,
    Google,
    DeepSeek,
    Mistral,
    XAI,
    Ollama,
    OpenRouter,
    Custom,
    Unknown,
}

/// Tokenizer type — determines which tokenizer to use for accurate budget calculation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum TokenizerType {
    /// GPT-4, GPT-4o, Claude 3.x family.
    Cl100kBase,
    /// o1, o3, o4, Codex 5.x family.
    O200kBase,
    /// Gemini tokenizer.
    Gemini,
    /// Llama, Mistral, local open models.
    SentencePiece,
    /// Fallback: character-based heuristic.
    Heuristic,
}

/// Per-model capability fingerprint.
/// Eliminates ALL hardcoded model limits throughout the codebase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    /// Maximum input context window (tokens).
    pub context_window: usize,
    /// Maximum output tokens the model can generate.
    pub max_output_tokens: usize,
    /// Can this model call tools/functions?
    pub supports_tools: bool,
    /// Can this model process images?
    pub supports_vision: bool,
    /// Does this model support extended thinking / chain-of-thought?
    pub supports_extended_thinking: bool,
    /// Does this model support streaming responses?
    pub supports_streaming: bool,
    /// Which tokenizer to use for budget calculations.
    pub tokenizer: TokenizerType,
    /// Provider rate limit in requests per minute (None = unknown/unlimited).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit_rpm: Option<u32>,
    /// The provider type.
    pub provider: ModelProvider,
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            context_window: 32_000,
            max_output_tokens: 4_096,
            supports_tools: true,
            supports_vision: false,
            supports_extended_thinking: false,
            supports_streaming: true,
            tokenizer: TokenizerType::Heuristic,
            rate_limit_rpm: None,
            provider: ModelProvider::Unknown,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SECTION 6: Configuration
// ═══════════════════════════════════════════════════════════════════════════

/// Centralized configuration for the entire Engram memory system.
/// Every previously-hardcoded value lives here with a documented default.
/// Frontend can override via IPC; stored in DB for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngramConfig {
    // ── Embedding ─────────────────────────────────────────────────────
    pub embedding_base_url: String,
    pub embedding_model: String,
    pub embedding_dims: usize,

    // ── Auto-recall/capture ───────────────────────────────────────────
    pub auto_recall: bool,
    pub auto_capture: bool,

    // ── Search tuning ─────────────────────────────────────────────────
    pub search: MemorySearchConfig,

    // ── Consolidation ─────────────────────────────────────────────────
    /// Minimum cosine similarity to consider two episodic memories as duplicate.
    pub dedup_threshold: f32,
    /// Minimum cosine similarity to merge episodic → semantic.
    pub consolidation_merge_threshold: f32,
    /// How often to run consolidation (seconds).
    pub consolidation_interval_secs: u64,

    // ── Ebbinghaus decay ──────────────────────────────────────────────
    /// Strength decay half-life in days.
    pub strength_half_life_days: f32,
    /// Minimum strength before a memory becomes GC-eligible.
    pub gc_strength_threshold: f32,

    // ── Context budget ────────────────────────────────────────────────
    /// Percentage of context window allocated to memories (0.0–1.0).
    pub memory_budget_pct: f32,
    /// Percentage allocated to conversation history.
    pub history_budget_pct: f32,
    /// Percentage reserved for system prompt + identity.
    pub system_budget_pct: f32,
    /// Default context window (fallback for unknown models).
    pub default_context_window: usize,

    // ── Working memory ────────────────────────────────────────────────
    /// Max items in working memory.
    pub working_memory_capacity: usize,
    /// Sensory buffer size (number of recent message pairs).
    pub sensory_buffer_size: usize,

    // ── Background tasks ──────────────────────────────────────────────
    /// Embedding backfill batch size.
    pub backfill_batch_size: usize,
    /// Delay between backfill batches (ms).
    pub backfill_delay_ms: u64,

    // ── RAM management ────────────────────────────────────────────────
    /// Max RSS in bytes before triggering RAM pressure.
    pub ram_pressure_threshold_bytes: usize,
    /// Whether to automatically tier HNSW to disk under pressure.
    pub auto_tier_to_disk: bool,
}

impl Default for EngramConfig {
    fn default() -> Self {
        Self {
            embedding_base_url: "http://localhost:11434".into(),
            embedding_model: "nomic-embed-text".into(),
            embedding_dims: 768,
            auto_recall: true,
            auto_capture: true,
            search: MemorySearchConfig::default(),
            dedup_threshold: 0.85,
            consolidation_merge_threshold: 0.8,
            consolidation_interval_secs: 300, // 5 minutes
            strength_half_life_days: 30.0,
            gc_strength_threshold: 0.05,
            memory_budget_pct: 0.35,
            history_budget_pct: 0.40,
            system_budget_pct: 0.25,
            default_context_window: 32_000,
            working_memory_capacity: 7, // Miller's 7±2
            sensory_buffer_size: 5,
            backfill_batch_size: 50,
            backfill_delay_ms: 100,
            ram_pressure_threshold_bytes: 350 * 1024 * 1024, // 350 MB
            auto_tier_to_disk: true,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SECTION 7: Audit Trail
// ═══════════════════════════════════════════════════════════════════════════

/// An entry in the memory audit log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// What operation was performed.
    pub operation: AuditOperation,
    /// Which memory was affected.
    pub memory_id: String,
    /// Who performed the operation.
    pub actor: String,
    /// Additional context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// When this happened.
    pub timestamp: String,
}

/// Types of auditable memory operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditOperation {
    Store,
    Update,
    Delete,
    Search,
    Consolidate,
    Migrate,
    Encrypt,
    Decrypt,
    NegativeFeedback,
    PositiveFeedback,
}

// ═══════════════════════════════════════════════════════════════════════════
// SECTION 8: Working Memory
// ═══════════════════════════════════════════════════════════════════════════

/// A slot in working memory — an active piece of context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkingMemorySlot {
    /// Reference to the source memory (if from LTM).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_id: Option<String>,
    /// The content currently in this slot.
    pub content: String,
    /// How this got into working memory.
    pub source: WorkingMemorySource,
    /// When this was loaded into working memory.
    pub loaded_at: String,
    /// Priority for eviction (higher = keep longer).
    pub priority: f32,
    /// Token cost of this slot.
    pub token_cost: usize,
}

/// How a piece of content entered working memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkingMemorySource {
    /// Auto-recalled from LTM.
    Recall,
    /// Direct user mention.
    UserMention,
    /// From the sensory buffer (recent messages).
    SensoryBuffer,
    /// From a tool result.
    ToolResult,
    /// Restored from a previous session.
    Restored,
}

/// Serializable working memory state — for save/restore across agent switches.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkingMemorySnapshot {
    pub agent_id: String,
    pub slots: Vec<WorkingMemorySlot>,
    /// Momentum vector for trajectory-aware recall (last N query embeddings).
    pub momentum_embeddings: Vec<Vec<f32>>,
    /// Timestamp when this snapshot was taken.
    pub saved_at: String,
}

/// Unified memory category enum — single source of truth across Rust, TypeScript, and SQLite.
/// Covers categories from: backend MemoryCategory, agent tool enum, frontend memory-intelligence,
/// flows UI memory-flow-atoms, and auto-capture session/task categories.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum MemoryCategory {
    General,
    Preference,
    Fact,
    Skill,
    Context,
    Instruction,
    Correction,
    Feedback,
    Project,
    Person,
    Technical,
    Session,
    TaskResult,
    Summary,
    Conversation,
    Insight,
    ErrorLog,
    Procedure,
}

impl std::fmt::Display for MemoryCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryCategory::General => write!(f, "general"),
            MemoryCategory::Preference => write!(f, "preference"),
            MemoryCategory::Fact => write!(f, "fact"),
            MemoryCategory::Skill => write!(f, "skill"),
            MemoryCategory::Context => write!(f, "context"),
            MemoryCategory::Instruction => write!(f, "instruction"),
            MemoryCategory::Correction => write!(f, "correction"),
            MemoryCategory::Feedback => write!(f, "feedback"),
            MemoryCategory::Project => write!(f, "project"),
            MemoryCategory::Person => write!(f, "person"),
            MemoryCategory::Technical => write!(f, "technical"),
            MemoryCategory::Session => write!(f, "session"),
            MemoryCategory::TaskResult => write!(f, "task_result"),
            MemoryCategory::Summary => write!(f, "summary"),
            MemoryCategory::Conversation => write!(f, "conversation"),
            MemoryCategory::Insight => write!(f, "insight"),
            MemoryCategory::ErrorLog => write!(f, "error_log"),
            MemoryCategory::Procedure => write!(f, "procedure"),
        }
    }
}

impl Default for MemoryCategory {
    fn default() -> Self {
        MemoryCategory::General
    }
}

impl std::str::FromStr for MemoryCategory {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "general" => Ok(MemoryCategory::General),
            "preference" | "user_preference" => Ok(MemoryCategory::Preference),
            "fact" => Ok(MemoryCategory::Fact),
            "skill" => Ok(MemoryCategory::Skill),
            "context" => Ok(MemoryCategory::Context),
            "instruction" => Ok(MemoryCategory::Instruction),
            "correction" => Ok(MemoryCategory::Correction),
            "feedback" => Ok(MemoryCategory::Feedback),
            "project" => Ok(MemoryCategory::Project),
            "person" => Ok(MemoryCategory::Person),
            "technical" => Ok(MemoryCategory::Technical),
            "session" => Ok(MemoryCategory::Session),
            "task_result" => Ok(MemoryCategory::TaskResult),
            "summary" => Ok(MemoryCategory::Summary),
            "conversation" => Ok(MemoryCategory::Conversation),
            "insight" => Ok(MemoryCategory::Insight),
            "error_log" => Ok(MemoryCategory::ErrorLog),
            "procedure" => Ok(MemoryCategory::Procedure),
            _ => Ok(MemoryCategory::General), // graceful fallback
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SECTION 9: Reranking Strategies (§35.1)
// ═══════════════════════════════════════════════════════════════════════════

/// Reranking strategy applied after initial retrieval + filtering.
/// Significantly improves precision — the right memories float to the top.
///
/// Applied as step 5 in the recall pipeline (§8.4) when `rerank_enabled = true`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum RerankStrategy {
    /// Reciprocal Rank Fusion — merges vector + FTS5 rankings.
    /// Fast, no model dependency. Default when Ollama is unavailable.
    RRF,

    /// MMR (Maximal Marginal Relevance) — penalizes near-duplicate results.
    /// Use when diversity matters more than pure relevance.
    MMR,

    /// Combined: RRF first, then MMR for diversity. Best overall quality.
    /// Default strategy.
    RRFThenMMR,

    /// Cross-encoder reranking using a lightweight local model.
    /// Most accurate but requires Ollama. Falls back to RRF if unavailable.
    CrossEncoder,
}

impl Default for RerankStrategy {
    fn default() -> Self {
        RerankStrategy::RRFThenMMR
    }
}

impl std::fmt::Display for RerankStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RerankStrategy::RRF => write!(f, "rrf"),
            RerankStrategy::MMR => write!(f, "mmr"),
            RerankStrategy::RRFThenMMR => write!(f, "rrf_then_mmr"),
            RerankStrategy::CrossEncoder => write!(f, "cross_encoder"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SECTION 10: Hybrid Search Configuration (§35.2)
// ═══════════════════════════════════════════════════════════════════════════

/// Hybrid search configuration — controls the balance between vector
/// similarity (semantic) and FTS5 keyword matching (lexical).
///
/// `text_weight = 0.0` → pure vector search
/// `text_weight = 1.0` → pure FTS5 keyword search
/// `text_weight = 0.3` → 70% vector + 30% text (recommended default)
///
/// The optimal weight depends on the query type:
/// - Factual lookups ("what port does the server use?") → higher text weight
/// - Conceptual queries ("how does auth work?") → higher vector weight
/// - The system auto-detects and adjusts per-query when `auto_detect = true`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSearchConfig {
    /// Weight given to FTS5 text matching (0.0–1.0).
    /// The vector weight is implicitly `(1.0 - text_weight)`.
    pub text_weight: f64,

    /// When true, the system analyzes the query and adjusts text_weight
    /// automatically per-query. Factual queries get higher text_weight;
    /// conceptual queries get higher vector_weight.
    pub auto_detect: bool,

    /// Minimum text_weight when auto_detect overrides (floor).
    pub auto_min: f64,

    /// Maximum text_weight when auto_detect overrides (ceiling).
    pub auto_max: f64,
}

impl Default for HybridSearchConfig {
    fn default() -> Self {
        Self {
            text_weight: 0.3, // 70% semantic, 30% lexical
            auto_detect: true,
            auto_min: 0.1,
            auto_max: 0.7,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SECTION 11: Retrieval Quality Metrics (§5.3 / §35)
// ═══════════════════════════════════════════════════════════════════════════

/// Quality metrics computed on every retrieval operation.
/// Returned alongside recalled memories to the context builder.
/// Also fed back into the search tuning pipeline for self-improvement.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RetrievalQualityMetrics {
    /// Average composite trust score of returned memories.
    /// Range: 0.0–1.0. Below 0.3 indicates poor recall quality.
    pub average_relevancy: f64,

    /// Normalized Discounted Cumulative Gain.
    /// Measures whether the most relevant results are ranked first.
    /// Range: 0.0–1.0. NDCG=1.0 means perfect ranking order.
    pub ndcg: f64,

    /// Number of memories that passed all filters (scope, trust, dedup).
    pub candidates_after_filter: usize,

    /// Number of memories actually packed into the budget.
    pub memories_packed: usize,

    /// Total tokens consumed by recalled memories.
    pub tokens_consumed: usize,

    /// Search latency in milliseconds.
    pub search_latency_ms: u64,

    /// Whether reranking was applied (and which strategy).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rerank_applied: Option<RerankStrategy>,

    /// Hybrid search text-boost weight that was used.
    pub hybrid_text_weight: f64,
}

/// Result of a recall operation — memories plus quality metrics.
#[derive(Debug, Clone, Serialize)]
pub struct RecallResult {
    /// The recalled memories, sorted by relevance and budget-trimmed.
    pub memories: Vec<RetrievedMemory>,
    /// Quality metrics for this retrieval (NDCG, relevancy, latency, etc.).
    pub quality: RetrievalQualityMetrics,
}

// ═══════════════════════════════════════════════════════════════════════════
// SECTION 12: Metadata Schema Inference (§35.3)
// ═══════════════════════════════════════════════════════════════════════════

/// Auto-inferred metadata extracted from episodic memory content.
/// This runs during consolidation (§4), enriching memories with structured
/// fields that improve search precision and enable metadata-filtered queries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InferredMetadata {
    /// People mentioned (extracted via NER patterns or LLM).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub people: Vec<String>,

    /// Technologies/tools mentioned (matched against known tech vocabulary).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub technologies: Vec<String>,

    /// File paths referenced.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_paths: Vec<String>,

    /// Date references (parsed where possible).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dates: Vec<String>,

    /// URLs referenced.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub urls: Vec<String>,

    /// Sentiment of the memory content (-1.0 to 1.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sentiment: Option<f64>,

    /// Auto-detected topic categories (from a fixed taxonomy).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub topics: Vec<String>,

    /// Programming language (if code-related).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// Custom key-value pairs extracted by schema templates.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom: HashMap<String, String>,
}

impl InferredMetadata {
    /// Returns true if this metadata is entirely empty (nothing was extracted).
    pub fn is_empty(&self) -> bool {
        self.people.is_empty()
            && self.technologies.is_empty()
            && self.file_paths.is_empty()
            && self.dates.is_empty()
            && self.urls.is_empty()
            && self.sentiment.is_none()
            && self.topics.is_empty()
            && self.language.is_none()
            && self.custom.is_empty()
    }
}

/// Filters for metadata-scoped search queries.
/// If a field is `None`, it is not filtered on.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetadataFilters {
    /// Filter by technologies mentioned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub technologies: Option<Vec<String>>,

    /// Filter by file paths mentioned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_paths: Option<Vec<String>>,

    /// Filter by people mentioned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub people: Option<Vec<String>>,

    /// Filter by programming language.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    /// Filter by topic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topics: Option<Vec<String>>,
}

// ═══════════════════════════════════════════════════════════════════════════
// SECTION 14: Emotional Memory Dimension (§37)
// ═══════════════════════════════════════════════════════════════════════════

/// Affective dimensions attached to memories.
/// Based on the PAD (Pleasure-Arousal-Dominance) model extended with surprise.
/// All values range from -1.0 to 1.0 (bipolar scales).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EmotionalContext {
    /// Pleasure/displeasure dimension. Positive = pleasant, negative = unpleasant.
    pub valence: f32,
    /// Activation/deactivation. High = excited/alert, low = calm/bored.
    pub arousal: f32,
    /// Dominance/submissiveness. High = in control, low = overwhelmed.
    pub dominance: f32,
    /// Surprise factor (0.0 = expected, 1.0 = highly surprising).
    /// Surprising memories get stronger encoding (von Restorff effect).
    pub surprise: f32,
}

impl EmotionalContext {
    /// Compute a single emotional intensity score (0.0–1.0).
    /// Used as a retrieval boost factor — more emotionally intense memories
    /// are recalled more readily (mirroring the biological flashbulb effect).
    pub fn intensity(&self) -> f32 {
        let raw = (self.valence.abs() + self.arousal.abs() + self.dominance.abs() + self.surprise.abs()) / 4.0;
        raw.clamp(0.0, 1.0)
    }

    /// Compute emotional similarity to another context (cosine-like).
    /// Used to find memories with similar emotional tone during retrieval.
    pub fn similarity(&self, other: &EmotionalContext) -> f32 {
        let a = [self.valence, self.arousal, self.dominance, self.surprise];
        let b = [other.valence, other.arousal, other.dominance, other.surprise];
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let mag_a = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let mag_b = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if mag_a < f32::EPSILON || mag_b < f32::EPSILON {
            return 0.0;
        }
        (dot / (mag_a * mag_b)).clamp(-1.0, 1.0)
    }

    /// Modulate Ebbinghaus decay rate based on emotional arousal.
    /// High-arousal memories decay slower (stronger encoding).
    /// Returns a decay multiplier: < 1.0 = slower decay, > 1.0 = faster decay.
    pub fn decay_modulation(&self) -> f32 {
        // Arousal-based modulation: high arousal → slower decay
        // Surprise also strengthens encoding
        let arousal_factor = 1.0 - (self.arousal.abs() * 0.3);
        let surprise_factor = 1.0 - (self.surprise * 0.2);
        (arousal_factor * surprise_factor).clamp(0.4, 1.2)
    }

    /// Check if this is emotionally neutral (all dimensions near zero).
    pub fn is_neutral(&self) -> bool {
        self.intensity() < 0.1
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SECTION 15: Temporal-Axis Retrieval (§39)
// ═══════════════════════════════════════════════════════════════════════════

/// Temporal query types for time-axis retrieval.
/// Allows searching memories by when they occurred, not just what they contain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TemporalQuery {
    /// Find memories within a date range (inclusive).
    Range {
        start: String, // ISO 8601
        end: String,   // ISO 8601
    },
    /// Find memories near a specific point in time (within ±window).
    Proximity {
        anchor: String,      // ISO 8601
        window_hours: f64,
    },
    /// Find memories that repeat at a pattern (daily, weekly, etc.).
    Pattern {
        pattern: TemporalPattern,
    },
    /// Find the most recent N memories for an agent.
    Recent {
        limit: usize,
    },
    /// Find memories from a specific session.
    Session {
        session_id: String,
    },
}

/// Recognized temporal patterns in memory creation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum TemporalPattern {
    /// Memories created around the same time daily.
    Daily,
    /// Memories created on the same day of week.
    Weekly,
    /// Memories created around the same date monthly.
    Monthly,
    /// Burst of memories in a short window (high activity period).
    Burst,
}

/// Result of a temporal search including temporal metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalSearchResult {
    /// The retrieved memories.
    pub memories: Vec<RetrievedMemory>,
    /// Temporal clustering info — groups of memories that are temporally close.
    pub clusters: Vec<TemporalCluster>,
    /// Time range spanned by results.
    pub span_start: String,
    pub span_end: String,
}

/// A temporally co-located cluster of memories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalCluster {
    /// Cluster centroid time (ISO 8601).
    pub centroid: String,
    /// Memory IDs in this cluster.
    pub memory_ids: Vec<String>,
    /// Duration of cluster window (seconds).
    pub window_secs: u64,
}

// ═══════════════════════════════════════════════════════════════════════════
// SECTION 16: Intent-Aware Retrieval (§40)
// ═══════════════════════════════════════════════════════════════════════════

/// Classified query intent — determines how to weight retrieval signals.
/// A single query can have multiple intents with varying confidence.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum QueryIntent {
    /// "What is X?" — factual lookup, weight BM25 heavily.
    Factual,
    /// "How do I X?" — procedural recall, weight procedures heavily.
    Procedural,
    /// "Why did X happen?" — causal reasoning, traverse CausedBy edges.
    Causal,
    /// "What happened when..." — episodic recall, weight temporal + episodic.
    Episodic,
    /// "Tell me about X" — broad exploration, weight diversity via MMR.
    Exploratory,
    /// "Remember when we..." — personal/emotional, weight emotional context.
    Reflective,
}

/// Intent classification result — scores for each intent type.
/// Used to dynamically weight search signals per-query.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IntentClassification {
    pub factual: f32,
    pub procedural: f32,
    pub causal: f32,
    pub episodic: f32,
    pub exploratory: f32,
    pub reflective: f32,
}

impl IntentClassification {
    /// Return the dominant intent (highest score).
    pub fn dominant(&self) -> QueryIntent {
        let scores = [
            (QueryIntent::Factual, self.factual),
            (QueryIntent::Procedural, self.procedural),
            (QueryIntent::Causal, self.causal),
            (QueryIntent::Episodic, self.episodic),
            (QueryIntent::Exploratory, self.exploratory),
            (QueryIntent::Reflective, self.reflective),
        ];
        scores
            .iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(intent, _)| *intent)
            .unwrap_or(QueryIntent::Factual)
    }

    /// Get dynamic signal weights based on intent classification.
    /// Returns (bm25_weight, vector_weight, graph_weight, temporal_weight, emotional_weight).
    pub fn signal_weights(&self) -> (f32, f32, f32, f32, f32) {
        let dom = self.dominant();
        match dom {
            QueryIntent::Factual     => (0.50, 0.30, 0.10, 0.05, 0.05),
            QueryIntent::Procedural  => (0.30, 0.35, 0.15, 0.10, 0.10),
            QueryIntent::Causal      => (0.20, 0.30, 0.35, 0.10, 0.05),
            QueryIntent::Episodic    => (0.25, 0.25, 0.15, 0.25, 0.10),
            QueryIntent::Exploratory => (0.20, 0.40, 0.20, 0.10, 0.10),
            QueryIntent::Reflective  => (0.15, 0.30, 0.15, 0.10, 0.30),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SECTION 17: Entity Lifecycle Tracking (§41)
// ═══════════════════════════════════════════════════════════════════════════

/// A tracked entity — a person, project, tool, or concept that appears
/// across multiple memories and whose lifecycle is tracked over time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityProfile {
    /// Canonical entity ID (lowercase, normalized).
    pub id: String,
    /// Canonical display name.
    pub canonical_name: String,
    /// Known aliases / alternate spellings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// Entity type.
    pub entity_type: EntityType,
    /// First seen timestamp.
    pub first_seen: String,
    /// Last seen timestamp.
    pub last_seen: String,
    /// Number of memories mentioning this entity.
    pub mention_count: u32,
    /// Memory IDs that reference this entity.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub memory_ids: Vec<String>,
    /// Related entity IDs (auto-discovered co-occurrences).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_entities: Vec<String>,
    /// Summary of what we know about this entity (auto-generated).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Sentiment toward this entity (running average).
    pub sentiment: f32,
}

/// Types of tracked entities.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum EntityType {
    Person,
    Project,
    Technology,
    Organization,
    Location,
    Concept,
    Unknown,
}

impl std::fmt::Display for EntityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityType::Person => write!(f, "person"),
            EntityType::Project => write!(f, "project"),
            EntityType::Technology => write!(f, "technology"),
            EntityType::Organization => write!(f, "organization"),
            EntityType::Location => write!(f, "location"),
            EntityType::Concept => write!(f, "concept"),
            EntityType::Unknown => write!(f, "unknown"),
        }
    }
}

impl std::str::FromStr for EntityType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "person" => Ok(EntityType::Person),
            "project" => Ok(EntityType::Project),
            "technology" | "tech" => Ok(EntityType::Technology),
            "organization" | "org" | "company" => Ok(EntityType::Organization),
            "location" | "place" => Ok(EntityType::Location),
            "concept" | "idea" => Ok(EntityType::Concept),
            _ => Ok(EntityType::Unknown),
        }
    }
}

/// Entity mention extracted from memory content.
/// Used during storage to update entity profiles and discover relationships.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityMention {
    /// The raw text as it appeared in content.
    pub surface_form: String,
    /// Resolved entity ID (after canonical name resolution).
    pub entity_id: String,
    /// Entity type (person, project, etc.).
    pub entity_type: EntityType,
    /// Position in the source text (character offset).
    pub offset: usize,
    /// Confidence of the extraction (0.0–1.0).
    pub confidence: f32,
}
