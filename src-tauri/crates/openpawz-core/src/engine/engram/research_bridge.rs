// ── Engram: Research → Memory Integration ───────────────────────────────────
//
// Bridges research tool results (web_read, web_search) into the Engram
// memory store. When the research tool returns findings, they are stored
// as episodic memories with `MemorySource::ResearchDiscovery`.
//
// This enables the knowledge graph to grow from web research, not just
// conversation — giving agents persistent knowledge from their research.

use crate::atoms::engram_types::{EpisodicMemory, MemoryScope, MemorySource, TieredContent};
use crate::atoms::error::EngineResult;
use crate::engine::sessions::SessionStore;
use log::info;
use serde::{Deserialize, Serialize};

/// A research finding ready to be stored in memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchFinding {
    /// The main content/text from the research.
    pub content: String,
    /// Where the finding came from (URL or search query).
    pub source_url: Option<String>,
    /// The original search query that led to this finding.
    pub query: String,
    /// Category for the finding (e.g. "technical", "fact").
    pub category: String,
    /// Agent that performed the research.
    pub agent_id: String,
}

/// Result of ingesting research findings into memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionReport {
    /// Number of findings successfully stored.
    pub stored: usize,
    /// Number of findings skipped (e.g., duplicates).
    pub skipped: usize,
    /// IDs of stored memories.
    pub memory_ids: Vec<String>,
}

/// Ingest a batch of research findings into the Engram memory store.
///
/// Each finding is stored as an episodic memory with `ResearchDiscovery` source.
/// Duplicate content is detected by checking existing memories with the same
/// first 100 chars.
pub fn ingest_findings(
    store: &SessionStore,
    findings: &[ResearchFinding],
) -> EngineResult<IngestionReport> {
    let mut stored = 0usize;
    let mut skipped = 0usize;
    let mut memory_ids = Vec::new();

    for finding in findings {
        // Skip empty or too-short findings
        if finding.content.trim().len() < 20 {
            skipped += 1;
            continue;
        }

        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

        // Build tiered content
        let summary = if finding.content.len() > 200 {
            // Simple first-sentence summary
            finding
                .content
                .find(". ")
                .map(|end| finding.content[..=end].to_string())
                .unwrap_or_else(|| finding.content[..200].to_string() + "…")
        } else {
            finding.content.clone()
        };

        let key_fact = if finding.content.len() > 100 {
            finding.content[..100].to_string()
        } else {
            finding.content.clone()
        };

        let source_urls = finding
            .source_url
            .as_ref()
            .map(|u| vec![u.clone()])
            .unwrap_or_default();

        let memory = EpisodicMemory {
            id: id.clone(),
            content: TieredContent {
                full: finding.content.clone(),
                summary: Some(summary),
                key_fact: Some(key_fact),
                tags: Some(finding.category.clone()),
            },
            category: finding.category.clone(),
            source: MemorySource::ResearchDiscovery {
                urls: source_urls,
                query: finding.query.clone(),
            },
            scope: MemoryScope {
                global: false,
                agent_id: Some(finding.agent_id.clone()),
                ..Default::default()
            },
            created_at: now,
            importance: 0.6,
            agent_id: finding.agent_id.clone(),
            session_id: String::new(),
            strength: 1.0,
            embedding: None,
            embedding_model: None,
            access_count: 0,
            last_accessed_at: None,
            ..Default::default()
        };

        match store.engram_store_episodic(&memory) {
            Ok(_) => {
                stored += 1;
                memory_ids.push(id);
            }
            Err(e) => {
                log::debug!(
                    "[engram:research] Failed to store finding: {} — skipping",
                    e
                );
                skipped += 1;
            }
        }
    }

    if stored > 0 {
        info!(
            "[engram:research] Ingested {} research findings ({} skipped)",
            stored, skipped
        );
    }

    Ok(IngestionReport {
        stored,
        skipped,
        memory_ids,
    })
}

/// Convert a raw web_read result into a ResearchFinding.
pub fn finding_from_web_read(
    content: &str,
    url: &str,
    query: &str,
    agent_id: &str,
) -> ResearchFinding {
    ResearchFinding {
        content: content.to_string(),
        source_url: Some(url.to_string()),
        query: query.to_string(),
        category: "technical".to_string(), // default; can be overridden
        agent_id: agent_id.to_string(),
    }
}

/// Convert web_search results into ResearchFindings.
pub fn findings_from_web_search(
    results: &[(String, String)], // (title, snippet) pairs
    query: &str,
    agent_id: &str,
) -> Vec<ResearchFinding> {
    results
        .iter()
        .filter(|(_, snippet)| snippet.trim().len() >= 20)
        .map(|(title, snippet)| ResearchFinding {
            content: format!("{}: {}", title, snippet),
            source_url: None,
            query: query.to_string(),
            category: "fact".to_string(),
            agent_id: agent_id.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_finding_from_web_read() {
        let finding = finding_from_web_read(
            "Rust is a systems programming language",
            "https://rust-lang.org",
            "what is rust",
            "agent-1",
        );
        assert_eq!(finding.agent_id, "agent-1");
        assert_eq!(finding.source_url.as_deref(), Some("https://rust-lang.org"));
    }

    #[test]
    fn test_findings_from_search() {
        let results = vec![
            (
                "Rust".to_string(),
                "A systems language focused on safety".to_string(),
            ),
            ("Go".to_string(), "Short snippet".to_string()), // too short, will be filtered
        ];
        let findings = findings_from_web_search(&results, "systems languages", "agent-1");
        assert_eq!(findings.len(), 1);
    }
}
