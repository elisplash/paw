use rusqlite::params;
use crate::engine::types::{StoredMessage, Message, MessageContent, Role, ToolCall, ContentBlock};
use crate::atoms::error::EngineResult;
use super::SessionStore;

impl SessionStore {
    // ── Message CRUD ───────────────────────────────────────────────────

    pub fn add_message(&self, msg: &StoredMessage) -> EngineResult<()> {
        let conn = self.conn.lock();

        conn.execute(
            "INSERT INTO messages (id, session_id, role, content, tool_calls_json, tool_call_id, name)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                msg.id,
                msg.session_id,
                msg.role,
                msg.content,
                msg.tool_calls_json,
                msg.tool_call_id,
                msg.name,
            ],
        )?;

        // Update session stats
        conn.execute(
            "UPDATE sessions SET
                message_count = (SELECT COUNT(*) FROM messages WHERE session_id = ?1),
                updated_at = datetime('now')
             WHERE id = ?1",
            params![msg.session_id],
        )?;

        Ok(())
    }

    pub fn get_messages(&self, session_id: &str, limit: i64) -> EngineResult<Vec<StoredMessage>> {
        let conn = self.conn.lock();

        let mut stmt = conn.prepare(
            "SELECT id, session_id, role, content, tool_calls_json, tool_call_id, name, created_at
             FROM messages WHERE session_id = ?1 ORDER BY created_at ASC LIMIT ?2"
        )?;

        let messages = stmt.query_map(params![session_id, limit], |row| {
            Ok(StoredMessage {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                tool_calls_json: row.get(4)?,
                tool_call_id: row.get(5)?,
                name: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

        Ok(messages)
    }

    /// Convert stored messages to engine Message types for sending to AI provider.
    pub fn load_conversation(&self, session_id: &str, system_prompt: Option<&str>) -> EngineResult<Vec<Message>> {
        // Load recent messages only — lean sessions rely on memory_search for
        // historical context rather than carrying the full conversation.
        let stored = self.get_messages(session_id, 50)?;
        let mut messages = Vec::new();

        // Add system prompt if provided
        if let Some(prompt) = system_prompt {
            messages.push(Message {
                role: Role::System,
                content: MessageContent::Text(prompt.to_string()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }

        for sm in &stored {
            let role = match sm.role.as_str() {
                "system" => Role::System,
                "user" => Role::User,
                "assistant" => Role::Assistant,
                "tool" => Role::Tool,
                _ => Role::User,
            };

            let tool_calls: Option<Vec<ToolCall>> = sm.tool_calls_json.as_ref()
                .and_then(|json| serde_json::from_str(json).ok());

            messages.push(Message {
                role,
                content: MessageContent::Text(sm.content.clone()),
                tool_calls,
                tool_call_id: sm.tool_call_id.clone(),
                name: sm.name.clone(),
            });
        }

        // ── Context window truncation ──────────────────────────────────
        // Estimate tokens (~4 chars per token) and keep only the most recent
        // messages that fit within ~16k tokens to leave room for the response.
        // With lean session init (core soul files only + today's memories),
        // 16k tokens is plenty of context. At $3/MTok (Sonnet), 16k = $0.048
        // per round vs $0.09 at 30k. Agent uses memory_search for deeper context.
        // Always keep system prompt (first message).
        const MAX_CONTEXT_TOKENS: usize = 16_000;
        let estimate_tokens = |m: &Message| -> usize {
            let text_len = match &m.content {
                MessageContent::Text(t) => t.len(),
                MessageContent::Blocks(blocks) => blocks.iter().map(|b| match b {
                    ContentBlock::Text { text } => text.len(),
                    ContentBlock::ImageUrl { .. } => 1000, // rough estimate for images
                    ContentBlock::Document { data, .. } => data.len() / 4, // rough: base64 → chars
                }).sum(),
            };
            let tc_len = m.tool_calls.as_ref().map(|tcs| {
                tcs.iter().map(|tc| tc.function.arguments.len() + tc.function.name.len() + 20).sum::<usize>()
            }).unwrap_or(0);
            (text_len + tc_len) / 4 + 4 // +4 for role/overhead tokens
        };

        let total_tokens: usize = messages.iter().map(&estimate_tokens).sum();
        if total_tokens > MAX_CONTEXT_TOKENS && messages.len() > 2 {
            // Keep system prompt (index 0) and trim oldest non-system messages
            let system_msg = if !messages.is_empty() && messages[0].role == Role::System {
                Some(messages.remove(0))
            } else {
                None
            };

            // Drop from the front (oldest) until we fit, but ALWAYS keep the
            // last user message so the provider gets non-empty contents.
            let running_tokens: usize = system_msg.as_ref().map(&estimate_tokens).unwrap_or(0);
            let mut keep_from = 0;
            let msg_tokens: Vec<usize> = messages.iter().map(&estimate_tokens).collect();
            let total_msg_tokens: usize = msg_tokens.iter().sum();
            let mut drop_tokens = running_tokens + total_msg_tokens;

            // Find the last user message index — we must never drop past it
            let last_user_idx = messages.iter().rposition(|m| m.role == Role::User)
                .unwrap_or(messages.len().saturating_sub(1));

            for (i, &t) in msg_tokens.iter().enumerate() {
                if drop_tokens <= MAX_CONTEXT_TOKENS {
                    break;
                }
                // Never drop past the last user message
                if i >= last_user_idx {
                    break;
                }
                drop_tokens -= t;
                keep_from = i + 1;
            }

            messages = messages.split_off(keep_from);

            // Re-insert system prompt at the front
            if let Some(sys) = system_msg {
                messages.insert(0, sys);
            }

            log::info!("[engine] Context truncated: kept {} messages (~{} tokens, was ~{})",
                messages.len(), drop_tokens, total_tokens);
        }

        // ── Sanitize tool_use / tool_result pairing ────────────────────
        // After truncation (or corruption from previous crashes), ensure every
        // assistant message with tool_calls has matching tool_result messages.
        // The Anthropic API returns 400 if tool_use IDs appear without a
        // corresponding tool_result immediately after.
        Self::sanitize_tool_pairs(&mut messages);

        Ok(messages)
    }

    /// Ensure every assistant message with tool_calls has matching tool_result
    /// messages immediately after it.  Orphaned tool_use IDs (from context
    /// truncation or prior crashes) cause Anthropic to return HTTP 400.
    ///
    /// Strategy:
    /// 1. Remove leading orphaned tool-result messages that have no preceding
    ///    assistant message with tool_calls.
    /// 2. For each assistant message with tool_calls, collect the set of
    ///    tool_call IDs and check the immediately following messages.  Inject
    ///    a synthetic tool_result for any missing ID.
    fn sanitize_tool_pairs(messages: &mut Vec<Message>) {
        use std::collections::HashSet;

        // ── Pass 1: strip leading orphan tool results ──────────────────
        // After truncation the first non-system messages might be tool results
        // whose parent assistant message was dropped.
        let first_non_system = messages.iter().position(|m| m.role != Role::System).unwrap_or(0);
        let mut strip_end = first_non_system;
        while strip_end < messages.len() && messages[strip_end].role == Role::Tool {
            strip_end += 1;
        }
        if strip_end > first_non_system {
            let removed = strip_end - first_non_system;
            log::warn!("[engine] Removing {} orphaned leading tool_result messages", removed);
            messages.drain(first_non_system..strip_end);
        }

        // ── Pass 2: ensure every assistant+tool_calls has matching results ─
        let mut i = 0;
        while i < messages.len() {
            let has_tc = messages[i].role == Role::Assistant
                && messages[i].tool_calls.as_ref().map(|tc| !tc.is_empty()).unwrap_or(false);

            if !has_tc {
                i += 1;
                continue;
            }

            // Collect expected tool_call IDs from this assistant message
            let expected_ids: Vec<String> = messages[i]
                .tool_calls
                .as_ref()
                .unwrap()
                .iter()
                .map(|tc| tc.id.clone())
                .collect();

            // Scan immediately following tool-result messages
            let mut found_ids = HashSet::new();
            let mut j = i + 1;
            while j < messages.len() && messages[j].role == Role::Tool {
                if let Some(ref tcid) = messages[j].tool_call_id {
                    found_ids.insert(tcid.clone());
                }
                j += 1;
            }

            // Inject synthetic results for any missing tool_call IDs
            let mut injected = 0;
            for expected_id in &expected_ids {
                if !found_ids.contains(expected_id) {
                    let synthetic = Message {
                        role: Role::Tool,
                        content: MessageContent::Text(
                            "[Tool execution was interrupted or result was lost.]".into(),
                        ),
                        tool_calls: None,
                        tool_call_id: Some(expected_id.clone()),
                        name: Some("_synthetic".into()),
                    };
                    // Insert right after the assistant message (at position i+1+injected)
                    messages.insert(i + 1 + injected, synthetic);
                    injected += 1;
                }
            }

            if injected > 0 {
                log::warn!(
                    "[engine] Injected {} synthetic tool_result(s) for orphaned tool_use IDs",
                    injected
                );
            }

            // Advance past this assistant message + all following tool results
            i += 1;
            while i < messages.len() && messages[i].role == Role::Tool {
                i += 1;
            }
        }
    }
}
