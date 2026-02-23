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
    ///
    /// `max_context_tokens` caps the total conversation size.  Pass `None` to
    /// use the default (32 000 tokens).
    pub fn load_conversation(&self, session_id: &str, system_prompt: Option<&str>, max_context_tokens: Option<usize>) -> EngineResult<Vec<Message>> {
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
        // Configurable via Settings → Engine → Context Window.
        // Default 32K tokens — leaves ~24K for conversation with a ~8K system prompt.
        // Users with large budgets can push to 64K-128K for full-session memory.
        // Always keep system prompt (first message).
        let context_limit = max_context_tokens.unwrap_or(32_000);
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
        if total_tokens > context_limit && messages.len() > 2 {
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
                if drop_tokens <= context_limit {
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

        // ── Compact repeated tool failures ─────────────────────────────
        // If the same tool failed 2+ times in a row, the model sees a wall of
        // errors and gives up even when the user says "try again."
        // Collapse these runs into a single summary + the last failure,
        // freeing context space and removing the "learned helplessness."
        Self::compact_failed_tool_runs(&mut messages);

        // ── Neutralize "give up" assistant responses ───────────────────
        // After tool failures, the model often writes "I apologize, I'm hitting
        // a wall..." — this text persists and anchors future refusals.
        // Replace it with a neutral note so the model doesn't reinforce its own
        // learned helplessness on reload.
        Self::neutralize_give_up_responses(&mut messages);

        // ── Sanitize tool_use / tool_result pairing ────────────────────
        // After truncation (or corruption from previous crashes), ensure every
        // assistant message with tool_calls has matching tool_result messages.
        // The Anthropic API returns 400 if tool_use IDs appear without a
        // corresponding tool_result immediately after.
        Self::sanitize_tool_pairs(&mut messages);

        Ok(messages)
    }

    /// Compact consecutive failed tool calls of the same tool.
    ///
    /// When the agent retries a tool 3+ times and fails each time, the
    /// conversation history fills with repetitive error messages.  The model
    /// then "learns" the tool is broken and refuses to retry — even when the
    /// user explicitly asks it to try again (and the tool may have been fixed).
    ///
    /// Strategy: detect runs of [assistant(tool_call) → tool(error)] for the
    /// same tool name. Keep only the **last** failure pair and replace all
    /// earlier pairs with a single compact user-visible summary.  This frees
    /// context tokens and prevents the model from anchoring on past failures.
    fn compact_failed_tool_runs(messages: &mut Vec<Message>) {
        // Identify tool result messages that are failures (content contains error markers)
        let is_failed_tool_result = |m: &Message| -> bool {
            if m.role != Role::Tool { return false; }
            let text = m.content.as_text();
            // Heuristic: tool errors contain these patterns
            text.starts_with("Error:")
                || text.starts_with("Google API error")
                || text.contains("error (")
                || text.contains("is not enabled")
                || text.contains("failed:")
                || text.starts_with("Tool execution denied")
                || (text.len() < 200 && text.contains("error"))
        };

        // Build a list of (tool_name, assistant_idx, tool_result_idx) for failed calls
        let mut failed_calls: Vec<(String, usize, usize)> = Vec::new();
        let mut i = 0;
        while i < messages.len() {
            if messages[i].role == Role::Assistant {
                if let Some(ref tcs) = messages[i].tool_calls {
                    if tcs.len() == 1 { // Only compact single-tool calls
                        let tc = &tcs[0];
                        // Look for the matching tool result right after
                        let mut j = i + 1;
                        while j < messages.len() && messages[j].role == Role::Tool {
                            if messages[j].tool_call_id.as_deref() == Some(&tc.id)
                                && is_failed_tool_result(&messages[j])
                            {
                                failed_calls.push((tc.function.name.clone(), i, j));
                            }
                            j += 1;
                        }
                    }
                }
            }
            i += 1;
        }

        // Find consecutive runs of the same tool failing
        if failed_calls.len() < 2 { return; }

        let mut indices_to_remove: Vec<usize> = Vec::new();
        let mut run_start = 0;

        while run_start < failed_calls.len() {
            let tool_name = &failed_calls[run_start].0;
            let mut run_end = run_start;

            // Extend the run while same tool name and roughly consecutive in message order
            while run_end + 1 < failed_calls.len()
                && failed_calls[run_end + 1].0 == *tool_name
            {
                run_end += 1;
            }

            let run_len = run_end - run_start + 1;
            if run_len >= 2 {
                // Keep the LAST failure, remove all earlier ones in this run
                for item in failed_calls.iter().take(run_end).skip(run_start) {
                    let (_, asst_idx, tool_idx) = item;
                    indices_to_remove.push(*asst_idx);
                    indices_to_remove.push(*tool_idx);
                }

                log::info!(
                    "[engine] Compacting {} consecutive '{}' failures → keeping last failure only",
                    run_len, tool_name
                );
            }

            run_start = run_end + 1;
        }

        if indices_to_remove.is_empty() { return; }

        // Collect the summary messages to inject (one per compacted run)
        let mut summaries: Vec<(usize, Message)> = Vec::new();
        {
            let mut run_start = 0;
            while run_start < failed_calls.len() {
                let tool_name = &failed_calls[run_start].0;
                let mut run_end = run_start;
                while run_end + 1 < failed_calls.len()
                    && failed_calls[run_end + 1].0 == *tool_name
                {
                    run_end += 1;
                }
                let run_len = run_end - run_start + 1;
                if run_len >= 2 {
                    let (_, last_asst_idx, _) = &failed_calls[run_end];
                    summaries.push((*last_asst_idx, Message {
                        role: Role::User,
                        content: MessageContent::Text(format!(
                            "[Note: {} earlier attempt(s) to use '{}' failed and were removed. \
                            The tools may have been updated since. Try `request_tools` to discover alternatives.]",
                            run_len - 1, tool_name
                        )),
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                    }));
                }
                run_start = run_end + 1;
            }
        }

        // Remove indices in reverse order to preserve positions
        indices_to_remove.sort_unstable();
        indices_to_remove.dedup();
        for &idx in indices_to_remove.iter().rev() {
            if idx < messages.len() {
                messages.remove(idx);
            }
        }

        // Insert summaries (adjust positions for removed elements)
        for (target_pos, summary_msg) in summaries {
            // Find how many removals happened before this position
            let offset = indices_to_remove.iter().filter(|&&r| r < target_pos).count();
            let adjusted = target_pos.saturating_sub(offset);
            let insert_at = adjusted.min(messages.len());
            messages.insert(insert_at, summary_msg);
        }

        log::info!(
            "[engine] Compacted {} failed tool messages from conversation history",
            indices_to_remove.len()
        );
    }

    /// Detect and neutralize assistant responses where the model "gives up"
    /// after tool failures.
    ///
    /// When tools fail repeatedly, the model writes responses like:
    ///   "I apologize, it seems I'm hitting a wall with the Google API today..."
    ///   "Rather than continue to struggle with this specific tool..."
    ///
    /// These responses are persisted to DB and on reload the model sees its
    /// own prior refusal, anchors on it, and refuses again — even when the
    /// user explicitly asks it to "try again" and the tools may have been
    /// fixed.  This creates a self-reinforcing "learned helplessness" loop.
    ///
    /// Strategy: detect assistant messages (without tool_calls) that match
    /// "give up" patterns and replace their content with a neutral note.
    /// The user's follow-up request ("try again") then faces a clean slate.
    fn neutralize_give_up_responses(messages: &mut [Message]) {
        let give_up_patterns: &[&str] = &[
            "hitting a wall",
            "hitting a brick wall",
            "continue to struggle",
            "continue struggling",
            "rather than continue",
            "rather than keep trying",
            "keep running into errors",
            "keep encountering errors",
            "running into errors with the api",
            "i'm unable to",
            "i am unable to",
            "i cannot seem to",
            "tool is broken",
            "tool isn't working",
            "tool is not working",
            "doesn't seem to be working",
            "does not seem to work",
            "consistently failing",
            "keeps failing",
            "this approach isn't working",
            "this approach is not working",
            "apologize for the difficulty",
            "apologize for the inconvenience",
            "i've tried multiple",
            "i have tried multiple",
        ];

        let mut neutralized = 0;
        for msg in messages.iter_mut() {
            // Only target assistant messages without tool_calls (i.e. final text responses)
            if msg.role != Role::Assistant { continue; }
            if msg.tool_calls.as_ref().map(|tc| !tc.is_empty()).unwrap_or(false) { continue; }

            let text = msg.content.as_text().to_lowercase();
            let is_give_up = give_up_patterns.iter().any(|p| text.contains(p));
            if !is_give_up { continue; }

            // Replace the content with a neutral note
            msg.content = MessageContent::Text(
                "[A previous attempt at this task encountered errors. \
                The tools may have been updated since then. \
                Ready to try again with a fresh approach.]".to_string()
            );
            neutralized += 1;
        }

        if neutralized > 0 {
            log::info!(
                "[engine] Neutralized {} 'give up' assistant responses in conversation history",
                neutralized
            );
        }
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
