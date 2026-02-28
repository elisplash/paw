// Paw Agent Engine — Session Compaction
// Summarizes long sessions using the AI model, then replaces old messages
// with a compact summary to free context window space.

use crate::atoms::error::EngineResult;
use crate::engine::engram;
use crate::engine::providers::AnyProvider;
use crate::engine::sessions::SessionStore;
use crate::engine::types::*;
use log::{info, warn};
use std::sync::Arc;

/// Statistics returned after a compaction operation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CompactionResult {
    pub session_id: String,
    pub messages_before: usize,
    pub messages_after: usize,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub summary_length: usize,
}

/// Configuration for compaction behaviour.
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// Minimum message count before compaction is triggered.
    pub min_messages: usize,
    /// Estimated token threshold to trigger auto-compaction.
    pub token_threshold: usize,
    /// How many recent messages to keep verbatim (not summarized).
    pub keep_recent: usize,
    /// Max tokens for the summary itself.
    pub max_summary_tokens: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            min_messages: 20,
            token_threshold: 60_000,
            keep_recent: 6,
            max_summary_tokens: 2000,
        }
    }
}

/// Estimate the token count of a stored message (~4 chars per token).
pub fn estimate_message_tokens(msg: &StoredMessage) -> usize {
    let text_len = msg.content.len();
    let tc_len = msg.tool_calls_json.as_ref().map(|j| j.len()).unwrap_or(0);
    (text_len + tc_len) / 4 + 4
}

/// Check whether a session needs compaction.
pub fn needs_compaction(messages: &[StoredMessage], config: &CompactionConfig) -> bool {
    if messages.len() < config.min_messages {
        return false;
    }
    let total_tokens: usize = messages.iter().map(estimate_message_tokens).sum();
    total_tokens > config.token_threshold
}

/// Build the prompt that asks the AI to summarize the conversation.
fn build_summary_prompt(messages: &[StoredMessage]) -> Vec<Message> {
    let mut transcript = String::new();
    for msg in messages {
        let role_label = match msg.role.as_str() {
            "user" => "User",
            "assistant" => "Assistant",
            "tool" => "Tool",
            "system" => "System",
            _ => "Unknown",
        };
        // Skip tool call JSON detail — just note that tools were called
        if msg.role == "tool" {
            let name = msg.name.as_deref().unwrap_or("unknown");
            let output_preview = if msg.content.len() > 200 {
                format!("{}… (truncated)", &msg.content[..200])
            } else {
                msg.content.clone()
            };
            transcript.push_str(&format!(
                "[{}: {} → {}]\n",
                role_label, name, output_preview
            ));
        } else {
            let content_preview = if msg.content.len() > 500 {
                format!("{}… (truncated)", &msg.content[..500])
            } else {
                msg.content.clone()
            };
            transcript.push_str(&format!("{}: {}\n", role_label, content_preview));
        }
    }

    let system = Message {
        role: Role::System,
        content: MessageContent::Text(
            "You are a conversation summarizer. Produce a concise summary that captures:\n\
             1. Key decisions and conclusions\n\
             2. Important context and preferences the user expressed\n\
             3. Any action items or ongoing tasks\n\
             4. Technical details that would be needed to continue the conversation\n\n\
             Keep the summary under 800 words. Use bullet points for clarity. \
             Start with '[Session Summary]' on the first line."
                .to_string(),
        ),
        tool_calls: None,
        tool_call_id: None,
        name: None,
    };

    let user = Message {
        role: Role::User,
        content: MessageContent::Text(format!(
            "Please summarize this conversation:\n\n{}",
            transcript
        )),
        tool_calls: None,
        tool_call_id: None,
        name: None,
    };

    vec![system, user]
}

/// Perform compaction on a session:
/// 1. Load all messages
/// 2. Generate a summary of older messages using the AI
/// 3. Delete old messages from the DB
/// 4. Insert the summary as a system message
/// 5. Return stats
pub async fn compact_session(
    store: &Arc<SessionStore>,
    provider: &AnyProvider,
    model: &str,
    session_id: &str,
    config: &CompactionConfig,
) -> EngineResult<CompactionResult> {
    // 1. Load all messages
    let all_messages = store.get_messages(session_id, 10_000)?;
    let total_before = all_messages.len();

    if total_before < config.min_messages {
        return Err(format!(
            "Session has only {} messages (minimum {} for compaction)",
            total_before, config.min_messages
        )
        .into());
    }

    let tokens_before: usize = all_messages.iter().map(estimate_message_tokens).sum();

    // 2. Split: old messages to summarize vs recent messages to keep
    let keep_count = config.keep_recent.min(total_before);
    let split_point = total_before - keep_count;
    let old_messages = &all_messages[..split_point];
    let _recent_messages = &all_messages[split_point..];

    if old_messages.is_empty() {
        return Err("No old messages to compact.".into());
    }

    info!(
        "[compaction] Session {} — summarizing {} old messages, keeping {} recent",
        session_id,
        old_messages.len(),
        keep_count
    );

    // 3. Generate summary using AI
    let summary_prompt = build_summary_prompt(old_messages);
    let chunks = provider
        .chat_stream(&summary_prompt, &[], model, Some(0.3), None)
        .await?;

    let summary_text: String = chunks
        .iter()
        .filter_map(|c| c.delta_text.as_ref())
        .cloned()
        .collect();

    if summary_text.is_empty() {
        return Err("AI produced empty summary.".into());
    }

    info!(
        "[compaction] Generated summary: {} chars",
        summary_text.len()
    );

    // 4. Delete old messages from DB
    {
        let conn = store.conn.lock();
        for msg in old_messages {
            conn.execute(
                "DELETE FROM messages WHERE id = ?1",
                rusqlite::params![msg.id],
            )?;
        }

        // Update message count
        conn.execute(
            "UPDATE sessions SET
                message_count = (SELECT COUNT(*) FROM messages WHERE session_id = ?1),
                updated_at = datetime('now')
             WHERE id = ?1",
            rusqlite::params![session_id],
        )?;
    }

    // 5. Insert summary as the first message
    let summary_msg = StoredMessage {
        id: format!("compact_{}", uuid::Uuid::new_v4()),
        session_id: session_id.to_string(),
        role: "system".to_string(),
        content: summary_text.clone(),
        tool_calls_json: None,
        tool_call_id: None,
        name: Some("session_compaction".to_string()),
        created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    };
    store.add_message(&summary_msg)?;

    // 5b. Store compaction summary in Engram memory (§13 bridge)
    // This ensures session learnings are preserved for long-term recall
    // even after the original messages are deleted.
    {
        let emb_client = None; // No embedding client in this context — deferred to backfill
        match engram::bridge::store_auto_capture(
            store,
            &summary_text,
            "session",
            emb_client,
            None, // agent_id not available at compaction time
            Some(session_id),
            None, // no channel
            None, // no channel user
        )
        .await
        {
            Ok(Some(id)) => info!(
                "[compaction] Compaction summary stored in Engram (id={})",
                &id[..id.len().min(8)]
            ),
            Ok(None) => info!("[compaction] Compaction summary skipped (near-duplicate)"),
            Err(e) => warn!(
                "[compaction] Failed to store compaction summary in Engram: {}",
                e
            ),
        }
    }

    // 6. Calculate final stats
    let remaining = store.get_messages(session_id, 10_000)?;
    let tokens_after: usize = remaining.iter().map(estimate_message_tokens).sum();

    let result = CompactionResult {
        session_id: session_id.to_string(),
        messages_before: total_before,
        messages_after: remaining.len(),
        tokens_before,
        tokens_after,
        summary_length: summary_text.len(),
    };

    info!(
        "[compaction] Done: {} → {} messages, ~{} → ~{} tokens",
        result.messages_before, result.messages_after, result.tokens_before, result.tokens_after
    );

    Ok(result)
}

/// Auto-compact check: called before sending a new message.
/// If the session exceeds the threshold, compact it first.
pub async fn auto_compact_if_needed(
    store: &Arc<SessionStore>,
    provider: &AnyProvider,
    model: &str,
    session_id: &str,
) -> Option<CompactionResult> {
    let config = CompactionConfig::default();

    match store.get_messages(session_id, 10_000) {
        Ok(messages) => {
            if needs_compaction(&messages, &config) {
                info!(
                    "[compaction] Auto-compacting session {} ({} messages)",
                    session_id,
                    messages.len()
                );
                match compact_session(store, provider, model, session_id, &config).await {
                    Ok(result) => {
                        info!("[compaction] Auto-compact success: {:?}", result);
                        Some(result)
                    }
                    Err(e) => {
                        warn!("[compaction] Auto-compact failed: {}", e);
                        None
                    }
                }
            } else {
                None
            }
        }
        Err(e) => {
            warn!(
                "[compaction] Failed to load messages for auto-compact check: {}",
                e
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens() {
        let msg = StoredMessage {
            id: "1".into(),
            session_id: "s1".into(),
            role: "user".into(),
            content: "Hello world, this is a test message.".into(), // 36 chars
            tool_calls_json: None,
            tool_call_id: None,
            name: None,
            created_at: "2025-01-01".into(),
        };
        let tokens = estimate_message_tokens(&msg);
        assert!(tokens > 0);
        assert_eq!(tokens, 36 / 4 + 4); // 13
    }

    #[test]
    fn test_needs_compaction_below_threshold() {
        let msgs: Vec<StoredMessage> = (0..5)
            .map(|i| StoredMessage {
                id: format!("m{}", i),
                session_id: "s1".into(),
                role: "user".into(),
                content: "Short message".into(),
                tool_calls_json: None,
                tool_call_id: None,
                name: None,
                created_at: "2025-01-01".into(),
            })
            .collect();

        let config = CompactionConfig::default();
        assert!(!needs_compaction(&msgs, &config));
    }

    #[test]
    fn test_needs_compaction_above_threshold() {
        // Create messages totalling > 60k tokens
        let long_content = "x".repeat(12_000); // 3000 tokens per msg
        let msgs: Vec<StoredMessage> = (0..25)
            .map(|i| StoredMessage {
                id: format!("m{}", i),
                session_id: "s1".into(),
                role: "user".into(),
                content: long_content.clone(),
                tool_calls_json: None,
                tool_call_id: None,
                name: None,
                created_at: "2025-01-01".into(),
            })
            .collect();

        let config = CompactionConfig::default();
        assert!(needs_compaction(&msgs, &config)); // 25 * 3004 = 75100 > 60000
    }
}
