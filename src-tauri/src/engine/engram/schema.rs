// ── Engram: Database Schema ─────────────────────────────────────────────────
//
// New tables for the Engram three-tier memory system.
// These live alongside the existing `memories` table (not replacing it).
//
// Tables:
//   - episodic_memories: conversation-grounded facts with full metadata
//   - semantic_memories: subject-predicate-object knowledge graph triples
//   - procedural_memories: trigger→action rules and learned procedures
//   - memory_edges: graph edges between any memory types
//   - working_memory_snapshots: serialized working memory for agent switching
//   - memory_audit_log: append-only audit trail
//
// Called from run_migrations() in sessions/schema.rs.
// All statements are idempotent (CREATE IF NOT EXISTS / ADD COLUMN with silent error).

use crate::atoms::error::EngineResult;
use log::info;
use rusqlite::Connection;

/// Run Engram-specific migrations. Called from sessions/schema.rs run_migrations().
pub fn run_engram_migrations(conn: &Connection) -> EngineResult<()> {
    info!("[engram] Running Engram schema migrations");

    conn.execute_batch(ENGRAM_SCHEMA)?;

    // ── Idempotent column additions for future migrations ────────────
    // Pattern: try ADD COLUMN, swallow error if already exists.
    // Add new migrations below as needed.

    // §35.3: Metadata inference column (JSON blob, populated during consolidation)
    let _ = conn.execute(
        "ALTER TABLE episodic_memories ADD COLUMN inferred_metadata TEXT",
        [],
    );

    // §7: Negative contexts for context-aware suppression (JSON array of strings)
    let _ = conn.execute(
        "ALTER TABLE episodic_memories ADD COLUMN negative_contexts TEXT DEFAULT '[]'",
        [],
    );

    // §34.2: Embedding model tracking for version migration
    let _ = conn.execute(
        "ALTER TABLE episodic_memories ADD COLUMN embedding_model TEXT DEFAULT ''",
        [],
    );

    // §41: Entity lifecycle tracking — entity_profiles table
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS entity_profiles (
            id TEXT PRIMARY KEY,
            canonical_name TEXT NOT NULL,
            aliases TEXT NOT NULL DEFAULT '[]',
            entity_type TEXT NOT NULL DEFAULT 'unknown',
            first_seen TEXT NOT NULL,
            last_seen TEXT NOT NULL,
            mention_count INTEGER NOT NULL DEFAULT 1,
            memory_ids TEXT NOT NULL DEFAULT '[]',
            related_entities TEXT NOT NULL DEFAULT '[]',
            summary TEXT,
            sentiment REAL
        );
        CREATE INDEX IF NOT EXISTS idx_entity_canonical ON entity_profiles(canonical_name);
        CREATE INDEX IF NOT EXISTS idx_entity_type ON entity_profiles(entity_type);",
    )?;

    // §39: Temporal index on episodic memories created_at
    let _ = conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_episodic_created_at ON episodic_memories(created_at)",
        [],
    );

    // ── Anti-forensic padding (KDBX-equivalent vault-size quantization) ──
    // Inflate the database to the next PADDING_BUCKET boundary so the
    // file size only reveals a coarse bucket, not the exact row count.
    pad_to_bucket(conn)?;

    info!("[engram] Schema migrations complete");
    Ok(())
}

/// Minimum bucket size for vault-size quantization (512 KB).
/// The database file size will always be a multiple of this value,
/// preventing an attacker from inferring the exact number of memories
/// from the file size alone. Equivalent to KDBX inner-content padding.
const PADDING_BUCKET_BYTES: u64 = 512 * 1024;

/// Ensure `_engram_padding` table inflates the DB to the next bucket boundary.
pub fn pad_to_bucket(conn: &Connection) -> EngineResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _engram_padding (
            id INTEGER PRIMARY KEY,
            pad BLOB NOT NULL
        );",
    )?;

    let page_size: u64 = conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
    let page_count: u64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
    let current_bytes = page_size * page_count;

    let target_bytes = ((current_bytes / PADDING_BUCKET_BYTES) + 1) * PADDING_BUCKET_BYTES;
    let deficit = target_bytes.saturating_sub(current_bytes);

    if deficit > 0 {
        // Each row ≈ content + 20 bytes overhead. We use 4KB blobs so
        // we only need a handful of rows to reach the target.
        let blob_size: usize = 4096;
        let rows_needed = (deficit as usize / blob_size).max(1);

        // Clear existing padding and write fresh (random-length would leak
        // timing info, so we use fixed-size blobs of zeroed bytes).
        conn.execute("DELETE FROM _engram_padding", [])?;
        let mut stmt = conn.prepare("INSERT INTO _engram_padding (pad) VALUES (zeroblob(?1))")?;
        for _ in 0..rows_needed {
            stmt.execute(rusqlite::params![blob_size as i64])?;
        }
    }

    Ok(())
}

const ENGRAM_SCHEMA: &str = "
    -- ═══════════════════════════════════════════════════════════════
    -- Episodic Memories (Tier 2: Long-Term Store)
    -- Conversation-grounded facts with full provenance.
    -- ═══════════════════════════════════════════════════════════════
    CREATE TABLE IF NOT EXISTS episodic_memories (
        id TEXT PRIMARY KEY,

        -- Content tiers (tiered compression)
        content_full TEXT NOT NULL,
        content_summary TEXT,
        content_key_fact TEXT,
        content_tags TEXT,

        -- Classification
        category TEXT NOT NULL DEFAULT 'general',
        source TEXT NOT NULL DEFAULT 'conversation',

        -- Provenance
        session_id TEXT,
        turn_index INTEGER,
        agent_id TEXT NOT NULL DEFAULT '',

        -- Scoping (hierarchical)
        scope_global INTEGER NOT NULL DEFAULT 0,
        scope_project_id TEXT,
        scope_squad_id TEXT,
        scope_agent_id TEXT,
        scope_channel TEXT,
        scope_channel_user_id TEXT,

        -- Trust scoring (4-dimensional)
        trust_source REAL NOT NULL DEFAULT 0.5,
        trust_consistency REAL NOT NULL DEFAULT 0.5,
        trust_recency REAL NOT NULL DEFAULT 1.0,
        trust_user_feedback REAL NOT NULL DEFAULT 0.5,

        -- Consolidation state
        consolidation_state TEXT NOT NULL DEFAULT 'raw',
        consolidation_count INTEGER NOT NULL DEFAULT 0,
        last_consolidated_at TEXT,

        -- Embedding (f32 array serialized as BLOB)
        embedding BLOB,
        embedding_model TEXT,

        -- Temporal
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now')),
        last_accessed_at TEXT NOT NULL DEFAULT (datetime('now')),
        access_count INTEGER NOT NULL DEFAULT 0,

        -- Importance (0-10 scale, used for decay calculations)
        importance INTEGER NOT NULL DEFAULT 5
    );

    CREATE INDEX IF NOT EXISTS idx_episodic_agent
        ON episodic_memories(scope_agent_id);
    CREATE INDEX IF NOT EXISTS idx_episodic_category
        ON episodic_memories(category);
    CREATE INDEX IF NOT EXISTS idx_episodic_consolidation
        ON episodic_memories(consolidation_state);
    CREATE INDEX IF NOT EXISTS idx_episodic_importance
        ON episodic_memories(importance DESC);
    CREATE INDEX IF NOT EXISTS idx_episodic_accessed
        ON episodic_memories(last_accessed_at);
    CREATE INDEX IF NOT EXISTS idx_episodic_session
        ON episodic_memories(session_id);
    CREATE INDEX IF NOT EXISTS idx_episodic_channel
        ON episodic_memories(scope_channel, scope_channel_user_id);

    -- FTS5 index for keyword search (BM25 ranking)
    CREATE VIRTUAL TABLE IF NOT EXISTS episodic_memories_fts USING fts5(
        id UNINDEXED,
        content_full,
        content_summary,
        content_key_fact,
        content_tags,
        category,
        scope_agent_id UNINDEXED,
        content=episodic_memories,
        content_rowid=rowid,
        tokenize='porter unicode61'
    );

    -- Triggers for FTS sync (keep FTS in sync with main table)
    CREATE TRIGGER IF NOT EXISTS episodic_fts_insert AFTER INSERT ON episodic_memories
    BEGIN
        INSERT INTO episodic_memories_fts(
            rowid, id, content_full, content_summary, content_key_fact, content_tags, category, scope_agent_id
        ) VALUES (
            NEW.rowid, NEW.id, NEW.content_full, NEW.content_summary, NEW.content_key_fact, NEW.content_tags, NEW.category, NEW.scope_agent_id
        );
    END;

    CREATE TRIGGER IF NOT EXISTS episodic_fts_delete AFTER DELETE ON episodic_memories
    BEGIN
        INSERT INTO episodic_memories_fts(
            episodic_memories_fts, rowid, id, content_full, content_summary, content_key_fact, content_tags, category, scope_agent_id
        ) VALUES (
            'delete', OLD.rowid, OLD.id, OLD.content_full, OLD.content_summary, OLD.content_key_fact, OLD.content_tags, OLD.category, OLD.scope_agent_id
        );
    END;

    CREATE TRIGGER IF NOT EXISTS episodic_fts_update AFTER UPDATE ON episodic_memories
    BEGIN
        INSERT INTO episodic_memories_fts(
            episodic_memories_fts, rowid, id, content_full, content_summary, content_key_fact, content_tags, category, scope_agent_id
        ) VALUES (
            'delete', OLD.rowid, OLD.id, OLD.content_full, OLD.content_summary, OLD.content_key_fact, OLD.content_tags, OLD.category, OLD.scope_agent_id
        );
        INSERT INTO episodic_memories_fts(
            rowid, id, content_full, content_summary, content_key_fact, content_tags, category, scope_agent_id
        ) VALUES (
            NEW.rowid, NEW.id, NEW.content_full, NEW.content_summary, NEW.content_key_fact, NEW.content_tags, NEW.category, NEW.scope_agent_id
        );
    END;

    -- ═══════════════════════════════════════════════════════════════
    -- Semantic Memories (Knowledge Graph Triples)
    -- Subject → Predicate → Object with version tracking.
    -- ═══════════════════════════════════════════════════════════════
    CREATE TABLE IF NOT EXISTS semantic_memories (
        id TEXT PRIMARY KEY,

        -- Triple
        subject TEXT NOT NULL,
        predicate TEXT NOT NULL,
        object TEXT NOT NULL,

        -- Confidence and versioning
        confidence REAL NOT NULL DEFAULT 0.5,
        version INTEGER NOT NULL DEFAULT 1,
        supersedes_id TEXT,

        -- Scoping (same as episodic)
        scope_agent_id TEXT NOT NULL DEFAULT '',
        scope_project_id TEXT,
        scope_channel TEXT,

        -- Source provenance
        source_memory_id TEXT,
        source TEXT NOT NULL DEFAULT 'extraction',

        -- Embedding (for the full triple as text)
        embedding BLOB,

        -- Temporal
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now')),
        last_accessed_at TEXT NOT NULL DEFAULT (datetime('now')),
        access_count INTEGER NOT NULL DEFAULT 0
    );

    CREATE INDEX IF NOT EXISTS idx_semantic_subject
        ON semantic_memories(subject);
    CREATE INDEX IF NOT EXISTS idx_semantic_predicate
        ON semantic_memories(predicate);
    CREATE INDEX IF NOT EXISTS idx_semantic_object
        ON semantic_memories(object);
    CREATE INDEX IF NOT EXISTS idx_semantic_agent
        ON semantic_memories(scope_agent_id);
    CREATE INDEX IF NOT EXISTS idx_semantic_supersedes
        ON semantic_memories(supersedes_id);

    -- FTS5 for semantic search
    CREATE VIRTUAL TABLE IF NOT EXISTS semantic_memories_fts USING fts5(
        id UNINDEXED,
        subject,
        predicate,
        object,
        content=semantic_memories,
        content_rowid=rowid,
        tokenize='porter unicode61'
    );

    CREATE TRIGGER IF NOT EXISTS semantic_fts_insert AFTER INSERT ON semantic_memories
    BEGIN
        INSERT INTO semantic_memories_fts(rowid, id, subject, predicate, object)
        VALUES (NEW.rowid, NEW.id, NEW.subject, NEW.predicate, NEW.object);
    END;

    CREATE TRIGGER IF NOT EXISTS semantic_fts_delete AFTER DELETE ON semantic_memories
    BEGIN
        INSERT INTO semantic_memories_fts(semantic_memories_fts, rowid, id, subject, predicate, object)
        VALUES ('delete', OLD.rowid, OLD.id, OLD.subject, OLD.predicate, OLD.object);
    END;

    CREATE TRIGGER IF NOT EXISTS semantic_fts_update AFTER UPDATE ON semantic_memories
    BEGIN
        INSERT INTO semantic_memories_fts(semantic_memories_fts, rowid, id, subject, predicate, object)
        VALUES ('delete', OLD.rowid, OLD.id, OLD.subject, OLD.predicate, OLD.object);
        INSERT INTO semantic_memories_fts(rowid, id, subject, predicate, object)
        VALUES (NEW.rowid, NEW.id, NEW.subject, NEW.predicate, NEW.object);
    END;

    -- ═══════════════════════════════════════════════════════════════
    -- Procedural Memories (Trigger → Action Rules)
    -- Learned behaviors and multi-step procedures.
    -- ═══════════════════════════════════════════════════════════════
    CREATE TABLE IF NOT EXISTS procedural_memories (
        id TEXT PRIMARY KEY,

        -- The trigger condition (natural language or pattern)
        trigger_pattern TEXT NOT NULL,

        -- Steps as JSON array of strings
        steps_json TEXT NOT NULL DEFAULT '[]',

        -- Success tracking
        success_count INTEGER NOT NULL DEFAULT 0,
        failure_count INTEGER NOT NULL DEFAULT 0,

        -- Scoping
        scope_agent_id TEXT NOT NULL DEFAULT '',
        scope_project_id TEXT,

        -- Source
        source_memory_id TEXT,

        -- Temporal
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now')),
        last_used_at TEXT
    );

    CREATE INDEX IF NOT EXISTS idx_procedural_agent
        ON procedural_memories(scope_agent_id);

    -- ═══════════════════════════════════════════════════════════════
    -- Memory Edges (Graph Connections)
    -- Links between any memory types forming the knowledge graph.
    -- ═══════════════════════════════════════════════════════════════
    CREATE TABLE IF NOT EXISTS memory_edges (
        id TEXT PRIMARY KEY,

        -- Source and target memory IDs (can be any memory type)
        source_id TEXT NOT NULL,
        target_id TEXT NOT NULL,

        -- Edge type (causal, temporal, semantic, etc.)
        edge_type TEXT NOT NULL DEFAULT 'related',

        -- Edge strength (0.0 - 1.0, used for spreading activation)
        weight REAL NOT NULL DEFAULT 0.5,

        -- Temporal
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    CREATE INDEX IF NOT EXISTS idx_edges_source
        ON memory_edges(source_id);
    CREATE INDEX IF NOT EXISTS idx_edges_target
        ON memory_edges(target_id);
    CREATE INDEX IF NOT EXISTS idx_edges_type
        ON memory_edges(edge_type);
    CREATE UNIQUE INDEX IF NOT EXISTS idx_edges_pair
        ON memory_edges(source_id, target_id, edge_type);

    -- ═══════════════════════════════════════════════════════════════
    -- Working Memory Snapshots
    -- Serialized working memory state for agent switching.
    -- ═══════════════════════════════════════════════════════════════
    CREATE TABLE IF NOT EXISTS working_memory_snapshots (
        agent_id TEXT PRIMARY KEY,

        -- Serialized WorkingMemorySnapshot as JSON
        snapshot_json TEXT NOT NULL,

        -- Metadata
        slot_count INTEGER NOT NULL DEFAULT 0,
        total_tokens INTEGER NOT NULL DEFAULT 0,

        -- Temporal
        saved_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    -- ═══════════════════════════════════════════════════════════════
    -- Memory Audit Log (Append-Only)
    -- Every memory operation is logged for debugging and compliance.
    -- ═══════════════════════════════════════════════════════════════
    CREATE TABLE IF NOT EXISTS memory_audit_log (
        id INTEGER PRIMARY KEY AUTOINCREMENT,

        -- What happened
        operation TEXT NOT NULL,
        memory_id TEXT,
        memory_type TEXT,

        -- Who / where
        agent_id TEXT,
        session_id TEXT,

        -- Details (JSON for flexibility)
        details_json TEXT,

        -- Temporal
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    CREATE INDEX IF NOT EXISTS idx_audit_memory
        ON memory_audit_log(memory_id);
    CREATE INDEX IF NOT EXISTS idx_audit_agent
        ON memory_audit_log(agent_id);
    CREATE INDEX IF NOT EXISTS idx_audit_time
        ON memory_audit_log(created_at);
";
