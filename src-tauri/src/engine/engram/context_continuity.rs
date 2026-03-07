// ── Engram: Context Continuity — Checkpoint System (§24) ────────────────────
//
// Preserves cognitive state across context boundaries. When context limits
// are reached, the system can save a checkpoint and resume later with
// full cognitive state intact.
//
// Two modes:
//   - Automatic: summarize and continue (agent loops, tasks, orchestration)
//   - Manual: present options to the user (interactive chat)
//
// A checkpoint captures:
//   - Conversation state (full message history)
//   - Working memory snapshot (active slots + momentum)
//   - File state (hashes of read/modified files)
//   - Task progress (pending/completed/failed items + key decisions)
//
// Integration points:
//   - Before side-effect operations → capture_checkpoint()
//   - Context limit reached → summarize_for_continuation() or offer_choices()
//   - User chooses "revert" → restore_checkpoint()

use crate::atoms::engram_types::{
    CheckpointMessage, ContinuationMode, TaskCheckpoint, TaskCheckpointStatus,
    WorkingMemorySnapshot, WorkspaceCheckpoint,
};
use crate::atoms::error::EngineResult;
use crate::engine::sessions::SessionStore;
use log::info;

// ═════════════════════════════════════════════════════════════════════════════
// Constants
// ═════════════════════════════════════════════════════════════════════════════

/// Maximum checkpoints kept per session (oldest are pruned).
const MAX_CHECKPOINTS_PER_SESSION: usize = 10;

// ═════════════════════════════════════════════════════════════════════════════
// Checkpoint Capture
// ═════════════════════════════════════════════════════════════════════════════

/// Bundled parameters for [`capture_checkpoint`].
pub struct CaptureCheckpointRequest<'a> {
    pub agent_id: &'a str,
    pub session_id: &'a str,
    pub messages: &'a [CheckpointMessage],
    pub working_memory: &'a WorkingMemorySnapshot,
    pub file_hashes: &'a std::collections::HashMap<String, String>,
    pub tasks: &'a [TaskCheckpoint],
    pub key_decisions: &'a [String],
}

/// Capture a workspace checkpoint at the current point in time.
///
/// Should be called before any side-effect operation (file write, tool
/// execution, memory mutation) to enable recovery.
pub fn capture_checkpoint(
    store: &SessionStore,
    req: &CaptureCheckpointRequest<'_>,
) -> EngineResult<String> {
    let checkpoint = WorkspaceCheckpoint {
        id: uuid::Uuid::new_v4().to_string(),
        agent_id: req.agent_id.to_string(),
        session_id: req.session_id.to_string(),
        conversation_snapshot: req.messages.to_vec(),
        working_memory: req.working_memory.clone(),
        file_hashes: req.file_hashes.clone(),
        task_progress: req.tasks.to_vec(),
        key_decisions: req.key_decisions.to_vec(),
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    };

    let checkpoint_id = checkpoint.id.clone();
    save_checkpoint(store, &checkpoint)?;
    prune_old_checkpoints(store, req.session_id)?;

    info!(
        "[context_continuity] ✓ Checkpoint {} captured for session {} ({} messages, {} tasks)",
        checkpoint_id,
        req.session_id,
        req.messages.len(),
        req.tasks.len(),
    );

    Ok(checkpoint_id)
}

// ═════════════════════════════════════════════════════════════════════════════
// Checkpoint Restoration
// ═════════════════════════════════════════════════════════════════════════════

/// Restore a checkpoint, returning the full cognitive state.
pub fn restore_checkpoint(
    store: &SessionStore,
    checkpoint_id: &str,
) -> EngineResult<Option<WorkspaceCheckpoint>> {
    load_checkpoint(store, checkpoint_id)
}

/// List available checkpoints for a session, ordered by creation time (newest first).
pub fn list_checkpoints(
    store: &SessionStore,
    session_id: &str,
) -> EngineResult<Vec<CheckpointSummary>> {
    ensure_checkpoint_table(store)?;

    let conn = store.conn.lock();
    let mut stmt = conn.prepare(
        "SELECT id, agent_id, created_at, message_count, task_count
         FROM workspace_checkpoints
         WHERE session_id = ?1
         ORDER BY created_at DESC",
    )?;

    let summaries: Vec<CheckpointSummary> = stmt
        .query_map(rusqlite::params![session_id], |row| {
            Ok(CheckpointSummary {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                created_at: row.get(2)?,
                message_count: row.get::<_, u32>(3).unwrap_or(0),
                task_count: row.get::<_, u32>(4).unwrap_or(0),
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(summaries)
}

/// Summary of a checkpoint (for listing without loading full data).
#[derive(Debug, Clone)]
pub struct CheckpointSummary {
    pub id: String,
    pub agent_id: String,
    pub created_at: String,
    pub message_count: u32,
    pub task_count: u32,
}

// ═════════════════════════════════════════════════════════════════════════════
// Continuation
// ═════════════════════════════════════════════════════════════════════════════

/// Generate a task-aware summary for automatic continuation when context
/// limits are reached.
///
/// Extracts:
///   - Pending work items (incomplete tasks)
///   - Key decisions made so far
///   - Critical context from working memory
///   - Recent conversation highlights
pub fn summarize_for_continuation(checkpoint: &WorkspaceCheckpoint) -> String {
    let mut summary = String::with_capacity(2048);

    summary.push_str("## Context Continuation Summary\n\n");

    // Key decisions
    if !checkpoint.key_decisions.is_empty() {
        summary.push_str("### Key Decisions\n");
        for decision in &checkpoint.key_decisions {
            summary.push_str("- ");
            summary.push_str(decision);
            summary.push('\n');
        }
        summary.push('\n');
    }

    // Task progress
    let pending: Vec<_> = checkpoint
        .task_progress
        .iter()
        .filter(|t| t.status != TaskCheckpointStatus::Completed)
        .collect();
    let completed: Vec<_> = checkpoint
        .task_progress
        .iter()
        .filter(|t| t.status == TaskCheckpointStatus::Completed)
        .collect();

    if !pending.is_empty() {
        summary.push_str("### Pending Work\n");
        for task in &pending {
            let status = match task.status {
                TaskCheckpointStatus::InProgress => "[IN PROGRESS]",
                TaskCheckpointStatus::Failed => "[FAILED]",
                _ => "[PENDING]",
            };
            summary.push_str(&format!("- {} {}\n", status, task.description));
        }
        summary.push('\n');
    }

    if !completed.is_empty() {
        summary.push_str("### Completed\n");
        for task in completed.iter().rev().take(5) {
            summary.push_str(&format!("- ✓ {}\n", task.description));
        }
        summary.push('\n');
    }

    // Working memory highlights
    if !checkpoint.working_memory.slots.is_empty() {
        summary.push_str("### Active Context\n");
        for slot in checkpoint.working_memory.slots.iter().take(5) {
            let preview: String = slot.content.chars().take(200).collect();
            summary.push_str(&format!("- {}\n", preview));
        }
        summary.push('\n');
    }

    // Recent conversation (last few messages)
    let recent_msgs: Vec<_> = checkpoint
        .conversation_snapshot
        .iter()
        .rev()
        .take(6)
        .collect();
    if !recent_msgs.is_empty() {
        summary.push_str("### Recent Conversation\n");
        for msg in recent_msgs.iter().rev() {
            let preview: String = msg.content.chars().take(300).collect();
            summary.push_str(&format!("**{}**: {}\n", msg.role, preview));
        }
    }

    summary
}

/// Determine the appropriate continuation mode for a given context.
pub fn select_continuation_mode(is_interactive: bool) -> ContinuationMode {
    if is_interactive {
        ContinuationMode::Manual
    } else {
        ContinuationMode::Automatic
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Persistence (SQLite)
// ═════════════════════════════════════════════════════════════════════════════

/// Create the checkpoints table if it doesn't exist.
pub fn ensure_checkpoint_table(store: &SessionStore) -> EngineResult<()> {
    let conn = store.conn.lock();
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspace_checkpoints (
            id TEXT PRIMARY KEY,
            agent_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            checkpoint_json TEXT NOT NULL,
            message_count INTEGER DEFAULT 0,
            task_count INTEGER DEFAULT 0,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_checkpoints_session
            ON workspace_checkpoints(session_id, created_at DESC);",
    )?;
    Ok(())
}

fn save_checkpoint(store: &SessionStore, checkpoint: &WorkspaceCheckpoint) -> EngineResult<()> {
    ensure_checkpoint_table(store)?;

    let json = serde_json::to_string(checkpoint)?;

    let conn = store.conn.lock();
    conn.execute(
        "INSERT OR REPLACE INTO workspace_checkpoints
         (id, agent_id, session_id, checkpoint_json, message_count, task_count, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            checkpoint.id,
            checkpoint.agent_id,
            checkpoint.session_id,
            json,
            checkpoint.conversation_snapshot.len() as u32,
            checkpoint.task_progress.len() as u32,
            checkpoint.created_at,
        ],
    )?;

    Ok(())
}

fn load_checkpoint(
    store: &SessionStore,
    checkpoint_id: &str,
) -> EngineResult<Option<WorkspaceCheckpoint>> {
    ensure_checkpoint_table(store)?;

    let conn = store.conn.lock();
    let result = conn.query_row(
        "SELECT checkpoint_json FROM workspace_checkpoints WHERE id = ?1",
        rusqlite::params![checkpoint_id],
        |row| row.get::<_, String>(0),
    );

    match result {
        Ok(json) => {
            let checkpoint: WorkspaceCheckpoint = serde_json::from_str(&json)?;
            Ok(Some(checkpoint))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Remove old checkpoints beyond the retention limit.
fn prune_old_checkpoints(store: &SessionStore, session_id: &str) -> EngineResult<()> {
    let conn = store.conn.lock();
    conn.execute(
        "DELETE FROM workspace_checkpoints
         WHERE session_id = ?1
           AND id NOT IN (
               SELECT id FROM workspace_checkpoints
               WHERE session_id = ?1
               ORDER BY created_at DESC
               LIMIT ?2
           )",
        rusqlite::params![session_id, MAX_CHECKPOINTS_PER_SESSION as u32],
    )?;
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atoms::engram_types::WorkingMemorySlot;

    #[test]
    fn test_summarize_for_continuation() {
        let checkpoint = WorkspaceCheckpoint {
            id: "test-1".into(),
            agent_id: "agent-1".into(),
            session_id: "session-1".into(),
            conversation_snapshot: vec![
                CheckpointMessage {
                    role: "user".into(),
                    content: "Deploy to staging".into(),
                    timestamp: "2025-01-01T00:00:00Z".into(),
                },
                CheckpointMessage {
                    role: "assistant".into(),
                    content: "Starting deployment...".into(),
                    timestamp: "2025-01-01T00:01:00Z".into(),
                },
            ],
            working_memory: WorkingMemorySnapshot {
                agent_id: "agent-1".into(),
                slots: vec![WorkingMemorySlot {
                    memory_id: None,
                    content: "Server is running on port 8080".into(),
                    source: crate::atoms::engram_types::WorkingMemorySource::Recall,
                    loaded_at: "2025-01-01T00:00:00Z".into(),
                    priority: 0.8,
                    token_cost: 10,
                }],
                momentum_embeddings: vec![],
                saved_at: "2025-01-01T00:00:00Z".into(),
            },
            file_hashes: std::collections::HashMap::new(),
            task_progress: vec![
                TaskCheckpoint {
                    task_id: "t1".into(),
                    description: "Build Docker image".into(),
                    status: TaskCheckpointStatus::Completed,
                },
                TaskCheckpoint {
                    task_id: "t2".into(),
                    description: "Push to registry".into(),
                    status: TaskCheckpointStatus::InProgress,
                },
                TaskCheckpoint {
                    task_id: "t3".into(),
                    description: "Run health checks".into(),
                    status: TaskCheckpointStatus::Pending,
                },
            ],
            key_decisions: vec!["Using Docker Compose for orchestration".into()],
            created_at: "2025-01-01T00:00:00Z".into(),
        };

        let summary = summarize_for_continuation(&checkpoint);
        assert!(summary.contains("Key Decisions"));
        assert!(summary.contains("Docker Compose"));
        assert!(summary.contains("Pending Work"));
        assert!(summary.contains("Push to registry"));
        assert!(summary.contains("Completed"));
        assert!(summary.contains("Build Docker image"));
        assert!(summary.contains("Active Context"));
        assert!(summary.contains("port 8080"));
    }

    #[test]
    fn test_select_continuation_mode() {
        assert_eq!(select_continuation_mode(true), ContinuationMode::Manual);
        assert_eq!(select_continuation_mode(false), ContinuationMode::Automatic);
    }
}
