// Paw Agent Engine — Memory System
//
// Provides long-term semantic memory using SQLite + embedding vectors.
// Uses Ollama (local) for embeddings by default — works out of the box.
// Also supports OpenAI-compatible embedding APIs.
//
// Module layout:
//   ollama.rs    — Ollama lifecycle (auto-start, model discovery/pull)
//   embedding.rs — EmbeddingClient (Ollama + OpenAI-compatible API calls)
//   mod.rs       — store, search (hybrid BM25+vector), MMR, fact extraction

pub mod embedding;
pub mod ollama;

// Re-export public API at the module level
pub use embedding::EmbeddingClient;
pub use ollama::{ensure_ollama_ready, is_ollama_init_done, OllamaReadyStatus};

use crate::atoms::error::EngineResult;
use crate::engine::sessions::{f32_vec_to_bytes, SessionStore};
use crate::engine::types::*;
use log::{error, info, warn};

// ── Store ──────────────────────────────────────────────────────────────

/// Store a memory with embedding.
/// If embedding_client is provided, computes embedding automatically.
/// Logs clearly when embeddings succeed or fail.
pub async fn store_memory(
    store: &SessionStore,
    content: &str,
    category: &str,
    importance: u8,
    embedding_client: Option<&EmbeddingClient>,
    agent_id: Option<&str>,
) -> EngineResult<String> {
    let id = uuid::Uuid::new_v4().to_string();

    let embedding_bytes = if let Some(client) = embedding_client {
        match client.embed(content).await {
            Ok(vec) => {
                info!(
                    "[memory] ✓ Embedded {} dims for memory {}",
                    vec.len(),
                    &id[..8]
                );
                Some(f32_vec_to_bytes(&vec))
            }
            Err(e) => {
                error!(
                    "[memory] ✗ Embedding failed for memory {} — storing without vector: {}",
                    &id[..8],
                    e
                );
                None
            }
        }
    } else {
        warn!("[memory] No embedding client — storing memory {} without vector (semantic search won't find this)", &id[..8]);
        None
    };

    store.store_memory(
        &id,
        content,
        category,
        importance,
        embedding_bytes.as_deref(),
        agent_id,
    )?;
    info!(
        "[memory] Stored memory {} cat={} imp={} agent={:?} has_embedding={}",
        &id[..8],
        category,
        importance,
        agent_id,
        embedding_bytes.is_some()
    );
    Ok(id)
}

/// Jaccard overlap threshold: memories above this are considered near-duplicates.
pub const DEDUP_OVERLAP_THRESHOLD: f64 = 0.6;

/// Store a memory with near-duplicate detection.
///
/// Before storing, checks if any memory *in the same category* created in the
/// last hour has > 60% word overlap with the new content.  If so, skips the
/// store and returns `None`.  This prevents memory loops where auto-capture
/// and session summaries pile up near-identical entries on every context
/// switch or model change.
pub async fn store_memory_dedup(
    store: &SessionStore,
    content: &str,
    category: &str,
    importance: u8,
    embedding_client: Option<&EmbeddingClient>,
    agent_id: Option<&str>,
) -> EngineResult<Option<String>> {
    // Check recent memories *in the same category* for near-duplicates (last 1 hour)
    let recent = store.get_recent_memory_contents_by_category(3600, category, agent_id)?;
    for existing in &recent {
        if content_overlap(content, existing) > DEDUP_OVERLAP_THRESHOLD {
            let preview = &content[..content.floor_char_boundary(80)];
            info!(
                "[memory] Skipping near-duplicate {} memory (overlap > {:.0}%): {}",
                category,
                DEDUP_OVERLAP_THRESHOLD * 100.0,
                preview
            );
            return Ok(None);
        }
    }

    let id = store_memory(
        store,
        content,
        category,
        importance,
        embedding_client,
        agent_id,
    )
    .await?;
    Ok(Some(id))
}

/// Compute word-level Jaccard overlap between two texts.
/// Returns a value between 0.0 (no overlap) and 1.0 (identical word sets).
/// Exported for use by the dedup logic in commands/chat.rs.
pub fn content_overlap(a: &str, b: &str) -> f64 {
    let words_a: std::collections::HashSet<&str> = a
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.len() > 2)
        .collect();
    let words_b: std::collections::HashSet<&str> = b
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.len() > 2)
        .collect();
    if words_a.is_empty() && words_b.is_empty() {
        return 1.0;
    }
    let intersection = words_a.intersection(&words_b).count() as f64;
    let union = words_a.union(&words_b).count() as f64;
    if union < 1.0 {
        0.0
    } else {
        intersection / union
    }
}

// ── Search (hybrid BM25 + vector + temporal decay + MMR) ───────────────

/// Search memories using hybrid strategy (BM25 + vector + temporal decay + MMR).
///
/// Strategy:
/// 1. BM25 full-text search via FTS5 (fast, exact-match aware)
/// 2. Vector semantic search via embeddings (meaning-aware)
/// 3. Merge results with weighted scoring (0.4 BM25 + 0.6 vector)
/// 4. Apply temporal decay (newer memories score higher)
/// 5. Apply MMR re-ranking (maximize diversity in top results)
/// 6. Optionally filter by agent_id
pub async fn search_memories(
    store: &SessionStore,
    query: &str,
    limit: usize,
    threshold: f64,
    embedding_client: Option<&EmbeddingClient>,
    agent_id: Option<&str>,
) -> EngineResult<Vec<Memory>> {
    // Truncate long queries — embedding models have limited context windows
    // (nomic-embed-text: 8192 tokens ≈ 6K chars). For search, first 2K chars
    // is more than enough to capture intent.
    // Use floor_char_boundary to avoid panicking on multi-byte chars (e.g. em dash —)
    let truncated_query: &str = &query[..query.floor_char_boundary(2000)];
    let query_preview = &truncated_query[..truncated_query.floor_char_boundary(80)];
    let fetch_limit = limit * 3; // Fetch extra for MMR re-ranking

    // ── Step 1: BM25 full-text search ──────────────────────────────
    let bm25_results = match store.search_memories_bm25(truncated_query, fetch_limit, agent_id) {
        Ok(r) => {
            info!(
                "[memory] BM25 search: {} results for '{}'",
                r.len(),
                query_preview
            );
            r
        }
        Err(e) => {
            warn!(
                "[memory] BM25 search failed: {} — continuing with vector only",
                e
            );
            Vec::new()
        }
    };

    // ── Step 2: Vector semantic search ─────────────────────────────
    let mut vector_results = Vec::new();
    let mut query_embedding: Option<Vec<f32>> = None;
    if let Some(client) = embedding_client {
        match client.embed(truncated_query).await {
            Ok(query_vec) => {
                info!(
                    "[memory] Query embedded ({} dims), searching...",
                    query_vec.len()
                );
                match store.search_memories_by_embedding(
                    &query_vec,
                    fetch_limit,
                    threshold,
                    agent_id,
                ) {
                    Ok(results) => {
                        info!(
                            "[memory] Vector search: {} results (top score: {:.3})",
                            results.len(),
                            results.first().and_then(|r| r.score).unwrap_or(0.0)
                        );
                        vector_results = results;
                    }
                    Err(e) => warn!("[memory] Vector search failed: {}", e),
                }
                query_embedding = Some(query_vec);
            }
            Err(e) => {
                warn!("[memory] Embedding query failed: {}", e);
            }
        }
    }

    // ── Step 3: Merge with weighted scoring ────────────────────────
    let mut merged = merge_search_results(&bm25_results, &vector_results, 0.4, 0.6);

    if merged.is_empty() {
        // Final fallback: keyword LIKE search
        info!("[memory] No BM25/vector results, falling back to keyword search");
        let results = store.search_memories_keyword(truncated_query, limit)?;
        info!(
            "[memory] Keyword fallback: {} results for '{}'",
            results.len(),
            query_preview
        );
        return Ok(results);
    }

    // ── Step 4: Apply temporal decay ───────────────────────────────
    apply_temporal_decay(&mut merged);

    // ── Step 5: MMR re-ranking for diversity ───────────────────────
    let merged_count = merged.len();
    let final_results = if query_embedding.is_some() && merged.len() > limit {
        mmr_rerank(&merged, limit, 0.7) // lambda=0.7 (70% relevance, 30% diversity)
    } else {
        merged.sort_by(|a, b| {
            b.score
                .unwrap_or(0.0)
                .partial_cmp(&a.score.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        merged.truncate(limit);
        merged
    };

    info!(
        "[memory] Hybrid search: returning {} results for '{}' (BM25={}, vector={}, merged={})",
        final_results.len(),
        query_preview,
        bm25_results.len(),
        vector_results.len(),
        merged_count
    );

    Ok(final_results)
}

// ── Search internals ───────────────────────────────────────────────────

/// Merge BM25 and vector search results with weighted scoring.
/// Normalizes scores from each source to [0,1] range before combining.
fn merge_search_results(
    bm25: &[Memory],
    vector: &[Memory],
    bm25_weight: f64,
    vector_weight: f64,
) -> Vec<Memory> {
    use std::collections::HashMap;

    let mut score_map: HashMap<String, (Option<f64>, Option<f64>, Memory)> = HashMap::new();

    // Normalize BM25 scores to [0,1]
    let bm25_max = bm25.iter().filter_map(|m| m.score).fold(0.0f64, f64::max);
    let bm25_min = bm25.iter().filter_map(|m| m.score).fold(f64::MAX, f64::min);
    let bm25_range = if (bm25_max - bm25_min).abs() < 1e-12 {
        1.0
    } else {
        bm25_max - bm25_min
    };

    for mem in bm25 {
        let normalized = mem.score.map(|s| (s - bm25_min) / bm25_range);
        score_map.insert(mem.id.clone(), (normalized, None, mem.clone()));
    }

    // Vector scores are already cosine similarity [0,1]
    for mem in vector {
        if let Some(entry) = score_map.get_mut(&mem.id) {
            entry.1 = mem.score;
        } else {
            score_map.insert(mem.id.clone(), (None, mem.score, mem.clone()));
        }
    }

    // Combine scores
    score_map
        .into_values()
        .map(|(bm25_score, vec_score, mut mem)| {
            let b = bm25_score.unwrap_or(0.0) * bm25_weight;
            let v = vec_score.unwrap_or(0.0) * vector_weight;
            mem.score = Some(b + v);
            mem
        })
        .collect()
}

/// Apply temporal decay: boost newer memories, penalize old ones.
/// Uses exponential decay with a half-life of 30 days.
fn apply_temporal_decay(memories: &mut [Memory]) {
    let now = chrono::Utc::now();
    let half_life_days: f64 = 30.0;
    let decay_constant = (2.0f64).ln() / half_life_days;

    for mem in memories.iter_mut() {
        if let Ok(created) =
            chrono::NaiveDateTime::parse_from_str(&mem.created_at, "%Y-%m-%d %H:%M:%S")
        {
            let created_utc = created.and_utc();
            let age_days = (now - created_utc).num_hours() as f64 / 24.0;
            let decay_factor = (-decay_constant * age_days).exp();
            if let Some(ref mut score) = mem.score {
                *score *= decay_factor;
            }
        }
    }
}

/// Maximal Marginal Relevance re-ranking.
/// Selects diverse results by penalizing redundancy.
/// lambda: 1.0 = pure relevance, 0.0 = pure diversity. 0.7 is a good default.
fn mmr_rerank(candidates: &[Memory], k: usize, lambda: f64) -> Vec<Memory> {
    if candidates.is_empty() || k == 0 {
        return Vec::new();
    }

    let mut selected: Vec<Memory> = Vec::with_capacity(k);
    let mut remaining: Vec<&Memory> = candidates.iter().collect();

    // Pick the highest-scored item first
    remaining.sort_by(|a, b| {
        b.score
            .unwrap_or(0.0)
            .partial_cmp(&a.score.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if let Some(first) = remaining.first() {
        selected.push((*first).clone());
        remaining.remove(0);
    }

    // Greedily select remaining items using MMR
    while selected.len() < k && !remaining.is_empty() {
        let mut best_idx = 0;
        let mut best_mmr = f64::NEG_INFINITY;

        for (i, candidate) in remaining.iter().enumerate() {
            let relevance = candidate.score.unwrap_or(0.0);
            let max_similarity = selected
                .iter()
                .map(|s| content_similarity(&candidate.content, &s.content))
                .fold(0.0f64, f64::max);

            let mmr_score = lambda * relevance - (1.0 - lambda) * max_similarity;

            if mmr_score > best_mmr {
                best_mmr = mmr_score;
                best_idx = i;
            }
        }

        selected.push(remaining[best_idx].clone());
        remaining.remove(best_idx);
    }

    selected
}

/// Simple content similarity (Jaccard on word sets) for MMR diversity.
/// Delegates to the public `content_overlap` function.
fn content_similarity(a: &str, b: &str) -> f64 {
    content_overlap(a, b)
}

// ── Backfill ───────────────────────────────────────────────────────────

/// Backfill embeddings for memories that were stored without vectors.
pub async fn backfill_embeddings(
    store: &SessionStore,
    client: &EmbeddingClient,
) -> EngineResult<(usize, usize)> {
    let memories = store.list_memories_without_embeddings(500)?;
    if memories.is_empty() {
        info!("[memory] Backfill: all memories already have embeddings");
        return Ok((0, 0));
    }

    info!(
        "[memory] Backfill: embedding {} memories...",
        memories.len()
    );
    let mut success = 0usize;
    let mut fail = 0usize;

    for mem in &memories {
        match client.embed(&mem.content).await {
            Ok(vec) => {
                let bytes = f32_vec_to_bytes(&vec);
                if let Err(e) = store.update_memory_embedding(&mem.id, &bytes) {
                    warn!(
                        "[memory] Backfill: failed to update {} — {}",
                        &mem.id[..8],
                        e
                    );
                    fail += 1;
                } else {
                    success += 1;
                }
            }
            Err(e) => {
                warn!(
                    "[memory] Backfill: embed failed for {} — {}",
                    &mem.id[..8],
                    e
                );
                fail += 1;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    info!(
        "[memory] Backfill complete: {} succeeded, {} failed",
        success, fail
    );
    Ok((success, fail))
}

// ── Fact Extraction ────────────────────────────────────────────────────

/// System prompt for LLM-powered fact extraction.
/// Structured JSON output for reliable parsing.
const FACT_EXTRACTION_PROMPT: &str = r#"You are a memory extraction system. Analyze the conversation and extract memorable facts worth remembering for future conversations.

Extract facts in these categories:
- "preference": User likes, dislikes, preferred tools, languages, styles
- "context": Technical environment, project details, team info, codebase facts
- "instruction": Standing orders ("always", "never", "remember to")
- "skill": User expertise, experience level, domains of knowledge
- "decision": Architectural decisions, technology choices, agreed-upon approaches
- "finding": Key discoveries, root causes, solutions found during tool work

Rules:
- Only extract facts worth remembering across sessions (skip ephemeral chitchat)
- Each fact should be a self-contained statement (understandable without context)
- Maximum 5 facts per exchange
- If nothing is worth remembering, return an empty array
- Keep facts concise (under 200 chars each)

Respond with ONLY a JSON array, no markdown fencing:
[{"fact": "User prefers TypeScript over JavaScript", "category": "preference"}, ...]
Or [] if nothing is memorable."#;

/// LLM-powered fact extraction — uses the chat provider to extract structured
/// knowledge from conversation turns. Falls back to heuristic extraction on failure.
pub async fn extract_memorable_facts_llm(
    user_message: &str,
    assistant_response: &str,
    provider: &crate::engine::providers::AnyProvider,
    model: &str,
) -> Vec<(String, String)> {
    use crate::engine::types::{Message, MessageContent, Role};

    // Skip trivially short exchanges — not worth an LLM call
    if user_message.len() < 10 && assistant_response.len() < 50 {
        return Vec::new();
    }

    // Truncate to avoid wasting tokens on extraction
    let user_trunc = if user_message.len() > 1000 {
        format!(
            "{}…",
            &user_message[..user_message.floor_char_boundary(1000)]
        )
    } else {
        user_message.to_string()
    };
    let asst_trunc = if assistant_response.len() > 2000 {
        format!(
            "{}…",
            &assistant_response[..assistant_response.floor_char_boundary(2000)]
        )
    } else {
        assistant_response.to_string()
    };

    let messages = vec![
        Message {
            role: Role::System,
            content: MessageContent::Text(FACT_EXTRACTION_PROMPT.to_string()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
        Message {
            role: Role::User,
            content: MessageContent::Text(format!(
                "User: {}\n\nAssistant: {}",
                user_trunc, asst_trunc
            )),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
    ];

    match provider
        .chat_stream(&messages, &[], model, Some(0.0), None)
        .await
    {
        Ok(chunks) => {
            let raw: String = chunks
                .iter()
                .filter_map(|c| c.delta_text.as_deref())
                .collect();

            parse_extracted_facts(&raw)
        }
        Err(e) => {
            warn!(
                "[memory] LLM fact extraction failed, falling back to heuristic: {}",
                e
            );
            extract_memorable_facts_heuristic(user_message, assistant_response)
        }
    }
}

/// Parse JSON array of extracted facts from LLM response.
fn parse_extracted_facts(raw: &str) -> Vec<(String, String)> {
    // Strip markdown code fences if present
    let trimmed = raw.trim();
    let json_str = if trimmed.starts_with("```") {
        trimmed
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
    } else {
        trimmed
    };

    // Find the JSON array boundaries
    let start = match json_str.find('[') {
        Some(i) => i,
        None => return Vec::new(),
    };
    let end = match json_str.rfind(']') {
        Some(i) => i + 1,
        None => return Vec::new(),
    };

    match serde_json::from_str::<Vec<serde_json::Value>>(&json_str[start..end]) {
        Ok(arr) => {
            arr.iter()
                .filter_map(|v| {
                    let fact = v.get("fact")?.as_str()?.to_string();
                    let category = v
                        .get("category")
                        .and_then(|c| c.as_str())
                        .unwrap_or("context")
                        .to_string();
                    // Validate category
                    let valid_cat = match category.as_str() {
                        "preference" | "context" | "instruction" | "skill" | "decision"
                        | "finding" => category,
                        _ => "context".to_string(),
                    };
                    if fact.len() < 5 || fact.len() > 500 {
                        return None;
                    }
                    // Strip HTML tags as defense-in-depth against stored XSS
                    let sanitized = fact.replace('<', "&lt;").replace('>', "&gt;");
                    Some((sanitized, valid_cat))
                })
                .take(5)
                .collect()
        }
        Err(e) => {
            warn!("[memory] Failed to parse LLM extraction JSON: {}", e);
            Vec::new()
        }
    }
}

/// Legacy heuristic fact extraction — fast, no LLM call, but only catches ~20%
/// of memorable facts. Used as fallback when LLM extraction fails.
pub fn extract_memorable_facts_heuristic(
    user_message: &str,
    assistant_response: &str,
) -> Vec<(String, String)> {
    let mut facts: Vec<(String, String)> = Vec::new();
    let user_lower = user_message.to_lowercase();

    // User preference patterns
    let preference_patterns = [
        "i like ",
        "i love ",
        "i prefer ",
        "i use ",
        "i work with ",
        "my favorite ",
        "my name is ",
        "i'm ",
        "i am ",
        "i live ",
        "my job ",
        "i work at ",
        "i work as ",
    ];
    for pattern in &preference_patterns {
        if user_lower.contains(pattern) {
            facts.push((user_message.to_string(), "preference".into()));
            break;
        }
    }

    // Factual statements about the user's environment
    let fact_patterns = [
        "my project ",
        "my repo ",
        "my app ",
        "the codebase ",
        "we use ",
        "our stack ",
        "our team ",
        "the database ",
    ];
    for pattern in &fact_patterns {
        if user_lower.contains(pattern) {
            facts.push((user_message.to_string(), "context".into()));
            break;
        }
    }

    // Instructions: "always...", "never...", "remember that..."
    let instruction_patterns = [
        "always ",
        "never ",
        "remember that ",
        "remember to ",
        "don't forget ",
        "make sure to ",
        "keep in mind ",
    ];
    for pattern in &instruction_patterns {
        if user_lower.contains(pattern) {
            facts.push((user_message.to_string(), "instruction".into()));
            break;
        }
    }

    // Extract facts from assistant response — capture key findings
    if assistant_response.len() > 100 {
        let resp_lower = assistant_response.to_lowercase();
        let assistant_fact_patterns = [
            "i found that ",
            "i discovered ",
            "the issue is ",
            "the problem is ",
            "the solution is ",
            "i've set up ",
            "i configured ",
            "i created ",
            "the root cause ",
            "i installed ",
            "i fixed ",
        ];
        for pattern in &assistant_fact_patterns {
            if resp_lower.contains(pattern) {
                let condensed = if assistant_response.len() > 300 {
                    format!(
                        "Agent finding: {}…",
                        &assistant_response[..assistant_response.floor_char_boundary(300)]
                    )
                } else {
                    format!("Agent finding: {}", assistant_response)
                };
                facts.push((condensed, "context".into()));
                break;
            }
        }
    }

    facts
}

// ── Session Summaries ──────────────────────────────────────────────────

/// LLM-powered session summary — generates a concise summary of the exchange
/// for long-term memory. Falls back to naive truncation on failure.
pub async fn generate_session_summary(
    user_message: &str,
    assistant_response: &str,
    provider: &crate::engine::providers::AnyProvider,
    model: &str,
) -> String {
    use crate::engine::types::{Message, MessageContent, Role};

    let user_trunc = if user_message.len() > 500 {
        format!(
            "{}…",
            &user_message[..user_message.floor_char_boundary(500)]
        )
    } else {
        user_message.to_string()
    };
    let asst_trunc = if assistant_response.len() > 3000 {
        format!(
            "{}…",
            &assistant_response[..assistant_response.floor_char_boundary(3000)]
        )
    } else {
        assistant_response.to_string()
    };

    let messages = vec![
        Message {
            role: Role::System,
            content: MessageContent::Text(
                "Summarize this exchange in 1-2 sentences for future reference. \
                 Focus on what was accomplished, decisions made, and any important outcomes. \
                 Be specific and concise. Respond with ONLY the summary text, no formatting."
                    .to_string(),
            ),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
        Message {
            role: Role::User,
            content: MessageContent::Text(format!(
                "User: {}\n\nAssistant: {}",
                user_trunc, asst_trunc
            )),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
    ];

    match provider
        .chat_stream(&messages, &[], model, Some(0.0), None)
        .await
    {
        Ok(chunks) => {
            let summary: String = chunks
                .iter()
                .filter_map(|c| c.delta_text.as_deref())
                .collect();
            let trimmed = summary.trim().to_string();
            if trimmed.is_empty() {
                fallback_session_summary(user_message, assistant_response)
            } else {
                trimmed
            }
        }
        Err(e) => {
            warn!("[memory] LLM session summary failed, using fallback: {}", e);
            fallback_session_summary(user_message, assistant_response)
        }
    }
}

/// Fallback summary when LLM is unavailable — naive truncation.
fn fallback_session_summary(user_message: &str, assistant_response: &str) -> String {
    let summary = if assistant_response.len() > 300 {
        format!(
            "{}…",
            &assistant_response[..assistant_response.floor_char_boundary(300)]
        )
    } else {
        assistant_response.to_string()
    };
    format!(
        "Session work: User asked: \"{}\". Agent responded: {}",
        crate::engine::types::truncate_utf8(user_message, 150),
        summary,
    )
}

// ── History Compression ────────────────────────────────────────────────

/// Compress a batch of old messages into a single summary message.
/// Used during context window truncation to preserve information from
/// dropped messages instead of losing them entirely.
///
/// Note: Currently available for future integration from the async agent loop.
/// The sync truncation path in messages.rs uses a lightweight inline recap instead.
#[allow(dead_code)]
pub async fn compress_history(
    messages_to_compress: &[crate::engine::types::Message],
    provider: &crate::engine::providers::AnyProvider,
    model: &str,
) -> Option<String> {
    use crate::engine::types::{Message, MessageContent, Role};

    if messages_to_compress.is_empty() {
        return None;
    }

    // Build a condensed view of the messages to compress
    let mut history_text = String::with_capacity(4000);
    for msg in messages_to_compress {
        let role_label = match msg.role {
            Role::User => "User",
            Role::Assistant => "Assistant",
            Role::Tool => "Tool",
            Role::System => continue,
        };
        let text = msg.content.as_text();
        if text.is_empty() {
            continue;
        }
        let truncated = if text.len() > 500 {
            format!("{}…", &text[..text.floor_char_boundary(500)])
        } else {
            text
        };
        history_text.push_str(&format!("{}: {}\n", role_label, truncated));
        if history_text.len() > 4000 {
            break;
        }
    }

    if history_text.is_empty() {
        return None;
    }

    let messages = vec![
        Message {
            role: Role::System,
            content: MessageContent::Text(
                "Summarize the following conversation history into a brief recap (3-5 sentences). \
                 Preserve key facts, decisions, file paths, error messages, and action items. \
                 This summary will replace the original messages in the context window. \
                 Respond with ONLY the summary text."
                    .to_string(),
            ),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
        Message {
            role: Role::User,
            content: MessageContent::Text(history_text),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        },
    ];

    match provider
        .chat_stream(&messages, &[], model, Some(0.0), None)
        .await
    {
        Ok(chunks) => {
            let summary: String = chunks
                .iter()
                .filter_map(|c| c.delta_text.as_deref())
                .collect();
            let trimmed = summary.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(format!("[Previous conversation summary: {}]", trimmed))
            }
        }
        Err(e) => {
            warn!("[memory] History compression failed: {}", e);
            None
        }
    }
}
