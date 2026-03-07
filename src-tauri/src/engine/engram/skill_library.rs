// ── Engram: Compounding Skill Library (§13) ─────────────────────────────────
//
// Implements a self-improving procedural memory library inspired by
// Voyager, Reflexion, and HELPER. Unlike static skill registries, this
// module auto-extracts reusable skills from successful multi-step tasks,
// verifies them, tracks execution outcomes, and learns from failures.
//
// Pipeline:
//   1. Auto-extraction: successful multi-step task → reusable skill
//   2. Verification: referenced tools exist, no hallucinated steps
//   3. Storage: stored as ProceduralMemory with metadata
//   4. Suggestion: proactive pattern-matching on conversation context
//   5. Execution tracking: success → boost strength, failure → reflexion
//   6. Composition: skills reference sub-skills via PartOf edges
//
// Integration points:
//   - Task completion → auto_extract_skill()
//   - Agent turn → suggest_skills()
//   - Skill execution result → record_outcome()

use crate::atoms::engram_types::{EdgeType, MemoryScope, ProceduralMemory, ProceduralStep};
use crate::atoms::error::EngineResult;
use crate::engine::sessions::SessionStore;
use log::info;

// ═════════════════════════════════════════════════════════════════════════════
// Types
// ═════════════════════════════════════════════════════════════════════════════

/// Outcome of a skill extraction attempt.
#[derive(Debug, Clone)]
pub struct ExtractionResult {
    /// The extracted skill (None if extraction failed).
    pub skill: Option<ProceduralMemory>,
    /// Why extraction failed (if it did).
    pub rejection_reason: Option<String>,
}

/// A skill suggestion for the current context.
#[derive(Debug, Clone)]
pub struct SkillSuggestion {
    /// The suggested skill.
    pub skill_id: String,
    /// Trigger that matched.
    pub trigger: String,
    /// Confidence in the match (0.0–1.0).
    pub confidence: f32,
    /// Human-readable description.
    pub description: String,
    /// Number of successful past executions.
    pub success_count: u32,
}

/// Outcome of executing a skill.
#[derive(Debug, Clone)]
pub enum SkillOutcome {
    Success,
    Failure {
        error: String,
        failed_step: Option<usize>,
    },
}

/// Report from a failure analysis (Reflexion-style).
#[derive(Debug, Clone)]
pub struct FailureAnalysis {
    /// What went wrong.
    pub error_description: String,
    /// Which step failed (0-indexed).
    pub failed_step_index: Option<usize>,
    /// Auto-generated guard condition to prevent recurrence.
    pub guard_condition: String,
}

// ═════════════════════════════════════════════════════════════════════════════
// Skill Extraction
// ═════════════════════════════════════════════════════════════════════════════

/// Auto-extract a reusable skill from a successful multi-step interaction.
///
/// Analyzes the tool calls and descriptions to create a ProceduralMemory.
/// The skill is verified before storage — hallucinated or dangerous steps
/// are rejected.
///
/// `steps` — ordered descriptions of what was done (tool calls + outcomes).
/// `trigger` — the user request that initiated this task.
/// `agent_id` — which agent performed the task.
/// `available_tools` — set of tool names this agent can actually call.
pub fn auto_extract_skill(
    store: &SessionStore,
    trigger: &str,
    steps: &[ProceduralStep],
    agent_id: &str,
    available_tools: &[&str],
) -> EngineResult<ExtractionResult> {
    // Require at least 2 steps for a meaningful skill
    if steps.len() < 2 {
        return Ok(ExtractionResult {
            skill: None,
            rejection_reason: Some("Too few steps for a reusable skill".into()),
        });
    }

    // ── Verification ─────────────────────────────────────────────────────
    if let Some(reason) = verify_steps(steps, available_tools) {
        info!(
            "[skill_library] Skill extraction rejected for '{}': {}",
            trigger, reason
        );
        return Ok(ExtractionResult {
            skill: None,
            rejection_reason: Some(reason),
        });
    }

    // ── Check for duplicates ─────────────────────────────────────────────
    if let Some(existing_id) = find_similar_skill(store, trigger)? {
        // Boost existing skill instead of creating duplicate
        boost_skill(store, &existing_id)?;
        info!(
            "[skill_library] Boosted existing skill {} instead of creating duplicate",
            existing_id
        );
        return Ok(ExtractionResult {
            skill: None,
            rejection_reason: Some(format!("Similar skill already exists: {}", existing_id)),
        });
    }

    // ── Create the skill ─────────────────────────────────────────────────
    let skill = ProceduralMemory {
        id: uuid::Uuid::new_v4().to_string(),
        trigger: trigger.to_string(),
        steps: steps.to_vec(),
        success_rate: 1.0, // first extraction = first success
        execution_count: 1,
        scope: MemoryScope {
            agent_id: Some(agent_id.to_string()),
            ..Default::default()
        },
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        updated_at: None,
    };

    store.engram_store_procedural(&skill)?;
    info!(
        "[skill_library] ✓ Extracted skill '{}' ({} steps) → {}",
        trigger,
        steps.len(),
        skill.id
    );

    Ok(ExtractionResult {
        skill: Some(skill),
        rejection_reason: None,
    })
}

/// Verify that skill steps are safe and valid.
/// Returns None if valid, or Some(reason) if invalid.
fn verify_steps(steps: &[ProceduralStep], available_tools: &[&str]) -> Option<String> {
    for (i, step) in steps.iter().enumerate() {
        // Check that referenced tools exist
        if let Some(ref tool) = step.tool_name {
            if !available_tools.iter().any(|t| t == tool) {
                return Some(format!("Step {} references unknown tool '{}'", i + 1, tool));
            }
        }

        // Check for dangerous operations without confirmation
        let desc_lower = step.description.to_lowercase();
        if is_dangerous_operation(&desc_lower) && !has_confirmation_step(steps, i) {
            return Some(format!(
                "Step {} contains dangerous operation without confirmation: {}",
                i + 1,
                step.description
            ));
        }
    }

    None
}

/// Check if a step description contains a dangerous operation.
fn is_dangerous_operation(desc: &str) -> bool {
    const DANGEROUS_PATTERNS: &[&str] = &[
        "rm -rf",
        "drop table",
        "drop database",
        "delete all",
        "format disk",
        "force push",
        "--force",
        "--no-verify",
        "truncate table",
    ];
    DANGEROUS_PATTERNS.iter().any(|p| desc.contains(p))
}

/// Check if there's a confirmation step before a dangerous operation.
fn has_confirmation_step(steps: &[ProceduralStep], dangerous_idx: usize) -> bool {
    if dangerous_idx == 0 {
        return false;
    }
    // Check the previous step for confirmation patterns
    let prev = &steps[dangerous_idx - 1].description.to_lowercase();
    prev.contains("confirm") || prev.contains("verify") || prev.contains("backup")
}

// ═════════════════════════════════════════════════════════════════════════════
// Skill Suggestion
// ═════════════════════════════════════════════════════════════════════════════

/// Search for skills that match the current conversation context.
///
/// Returns relevant skill suggestions, ordered by confidence.
/// The agent can present these to the user: "I have a verified procedure
/// for this from a previous session."
pub fn suggest_skills(
    store: &SessionStore,
    context: &str,
    scope: &MemoryScope,
    limit: usize,
) -> EngineResult<Vec<SkillSuggestion>> {
    let skills = store.engram_search_procedural(context, scope, limit.max(10))?;

    let context_words: Vec<&str> = context.split_whitespace().collect();
    let mut suggestions: Vec<SkillSuggestion> = Vec::new();

    for skill in skills {
        let confidence = compute_trigger_match(&skill.trigger, &context_words);
        if confidence < 0.3 {
            continue;
        }

        let description = format!(
            "{} ({} steps, {}% success rate, {} executions)",
            skill.trigger,
            skill.steps.len(),
            (skill.success_rate * 100.0) as u32,
            skill.execution_count,
        );

        suggestions.push(SkillSuggestion {
            skill_id: skill.id,
            trigger: skill.trigger,
            confidence,
            description,
            success_count: skill.execution_count,
        });
    }

    suggestions.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    suggestions.truncate(limit);

    Ok(suggestions)
}

/// Compute how well a skill trigger matches the current context (0.0–1.0).
fn compute_trigger_match(trigger: &str, context_words: &[&str]) -> f32 {
    let trigger_words: Vec<&str> = trigger.split_whitespace().collect();
    if trigger_words.is_empty() {
        return 0.0;
    }

    let matched = trigger_words
        .iter()
        .filter(|tw| {
            let tw_lower = tw.to_lowercase();
            context_words.iter().any(|cw| cw.to_lowercase() == tw_lower)
        })
        .count();

    matched as f32 / trigger_words.len() as f32
}

// ═════════════════════════════════════════════════════════════════════════════
// Outcome Recording & Reflexion
// ═════════════════════════════════════════════════════════════════════════════

/// Record the outcome of executing a skill. Implements Reflexion-style
/// verbal reinforcement learning.
///
/// - Success: increment success_count, boost strength
/// - Failure: analyze what went wrong, store failure variant as guard condition
pub fn record_outcome(
    store: &SessionStore,
    skill_id: &str,
    outcome: &SkillOutcome,
) -> EngineResult<Option<FailureAnalysis>> {
    match outcome {
        SkillOutcome::Success => {
            boost_skill(store, skill_id)?;
            info!("[skill_library] ✓ Skill {} succeeded, boosted", skill_id);
            Ok(None)
        }
        SkillOutcome::Failure { error, failed_step } => {
            // Record failure
            decrement_skill(store, skill_id)?;

            // Generate failure analysis
            let analysis = FailureAnalysis {
                error_description: error.clone(),
                failed_step_index: *failed_step,
                guard_condition: generate_guard_condition(error, *failed_step),
            };

            // Store the failure as a negative example — create a new procedural
            // memory that acts as a guard variant
            store_failure_variant(store, skill_id, &analysis)?;

            info!(
                "[skill_library] ✗ Skill {} failed at step {:?}: {}",
                skill_id, failed_step, error
            );

            Ok(Some(analysis))
        }
    }
}

/// Generate a guard condition from a failure.
fn generate_guard_condition(error: &str, failed_step: Option<usize>) -> String {
    let step_info = failed_step
        .map(|s| format!(" at step {}", s + 1))
        .unwrap_or_default();

    format!(
        "WARNING: This procedure previously failed{} with error: '{}'. Verify prerequisites before proceeding.",
        step_info, error
    )
}

/// Store a failure variant linked to the original skill.
fn store_failure_variant(
    store: &SessionStore,
    skill_id: &str,
    analysis: &FailureAnalysis,
) -> EngineResult<()> {
    let guard_step = ProceduralStep {
        description: analysis.guard_condition.clone(),
        tool_name: None,
        args_pattern: None,
        expected_outcome: Some("Verify conditions are met before proceeding".into()),
    };

    let variant = ProceduralMemory {
        id: uuid::Uuid::new_v4().to_string(),
        trigger: format!("[GUARD] {}", analysis.error_description),
        steps: vec![guard_step],
        success_rate: 0.0,
        execution_count: 0,
        scope: MemoryScope::default(),
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        updated_at: None,
    };

    store.engram_store_procedural(&variant)?;

    // Link the guard variant to the original skill
    super::graph::relate(store, &variant.id, skill_id, EdgeType::LearnedFrom, 0.8)?;

    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// Compositional Hierarchy
// ═════════════════════════════════════════════════════════════════════════════

/// Link a skill as a sub-skill of a parent skill (compositional hierarchy).
/// Creates a PartOf edge from child to parent.
pub fn link_sub_skill(
    store: &SessionStore,
    parent_skill_id: &str,
    child_skill_id: &str,
) -> EngineResult<()> {
    super::graph::relate(
        store,
        child_skill_id,
        parent_skill_id,
        EdgeType::PartOf,
        1.0,
    )?;
    info!(
        "[skill_library] Linked sub-skill {} → parent {}",
        child_skill_id, parent_skill_id
    );

    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// Internal helpers
// ═════════════════════════════════════════════════════════════════════════════

/// Find a similar skill by trigger text (simple word-overlap check).
fn find_similar_skill(store: &SessionStore, trigger: &str) -> EngineResult<Option<String>> {
    let scope = MemoryScope::default();
    let existing = store.engram_search_procedural(trigger, &scope, 5)?;

    let trigger_words: Vec<String> = trigger
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .collect();

    for skill in &existing {
        let skill_words: Vec<String> = skill
            .trigger
            .split_whitespace()
            .map(|w| w.to_lowercase())
            .collect();

        if skill_words.is_empty() || trigger_words.is_empty() {
            continue;
        }

        let matched = trigger_words
            .iter()
            .filter(|w| skill_words.contains(w))
            .count();
        let overlap = matched as f32 / trigger_words.len().max(skill_words.len()) as f32;

        if overlap >= 0.75 {
            return Ok(Some(skill.id.clone()));
        }
    }

    Ok(None)
}

/// Boost a skill's success count and strength after successful execution.
fn boost_skill(store: &SessionStore, skill_id: &str) -> EngineResult<()> {
    let conn = store.conn.lock();
    conn.execute(
        "UPDATE procedural_memories
         SET success_count = COALESCE(success_count, 0) + 1,
             updated_at = ?2
         WHERE id = ?1",
        rusqlite::params![
            skill_id,
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
        ],
    )?;
    // Also boost fast-path strength for recall priority
    drop(conn);
    super::graph::boost_fast_strength(store, skill_id).ok();
    Ok(())
}

/// Record a failure against a skill.
fn decrement_skill(store: &SessionStore, skill_id: &str) -> EngineResult<()> {
    let conn = store.conn.lock();
    conn.execute(
        "UPDATE procedural_memories
         SET failure_count = COALESCE(failure_count, 0) + 1,
             updated_at = ?2
         WHERE id = ?1",
        rusqlite::params![
            skill_id,
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
        ],
    )?;
    Ok(())
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_match() {
        let context = ["deploy", "to", "staging", "server"];
        assert!(compute_trigger_match("deploy to staging", &context) > 0.9);
        assert!(compute_trigger_match("deploy to production", &context) > 0.5);
        assert!(compute_trigger_match("unrelated database query", &context) < 0.3);
    }

    #[test]
    fn test_dangerous_operation_detection() {
        assert!(is_dangerous_operation("rm -rf /tmp/build"));
        assert!(is_dangerous_operation("drop table users"));
        assert!(is_dangerous_operation("git push --force origin main"));
        assert!(!is_dangerous_operation("create new file"));
        assert!(!is_dangerous_operation("run unit tests"));
    }

    #[test]
    fn test_verify_steps_rejects_unknown_tools() {
        let steps = vec![
            ProceduralStep {
                description: "Build the image".into(),
                tool_name: Some("docker_build".into()),
                args_pattern: None,
                expected_outcome: None,
            },
            ProceduralStep {
                description: "Push to registry".into(),
                tool_name: Some("nonexistent_tool".into()),
                args_pattern: None,
                expected_outcome: None,
            },
        ];

        let available = vec!["docker_build", "docker_push"];
        let result = verify_steps(&steps, &available);
        assert!(result.is_some());
        assert!(result.unwrap().contains("unknown tool"));
    }

    #[test]
    fn test_verify_steps_rejects_dangerous_without_confirmation() {
        let steps = vec![
            ProceduralStep {
                description: "Navigate to directory".into(),
                tool_name: None,
                args_pattern: None,
                expected_outcome: None,
            },
            ProceduralStep {
                description: "rm -rf /build".into(),
                tool_name: None,
                args_pattern: None,
                expected_outcome: None,
            },
        ];

        let available: Vec<&str> = vec![];
        let result = verify_steps(&steps, &available);
        assert!(result.is_some());
        assert!(result.unwrap().contains("dangerous operation"));
    }

    #[test]
    fn test_verify_steps_allows_dangerous_with_confirmation() {
        let steps = vec![
            ProceduralStep {
                description: "Backup the current directory".into(),
                tool_name: None,
                args_pattern: None,
                expected_outcome: None,
            },
            ProceduralStep {
                description: "rm -rf /build".into(),
                tool_name: None,
                args_pattern: None,
                expected_outcome: None,
            },
        ];

        let available: Vec<&str> = vec![];
        let result = verify_steps(&steps, &available);
        assert!(result.is_none());
    }

    #[test]
    fn test_guard_condition_generation() {
        let guard = generate_guard_condition("Connection refused", Some(2));
        assert!(guard.contains("step 3"));
        assert!(guard.contains("Connection refused"));
    }

    #[test]
    fn test_failure_analysis() {
        let analysis = FailureAnalysis {
            error_description: "Timeout connecting to database".into(),
            failed_step_index: Some(1),
            guard_condition: generate_guard_condition("Timeout connecting to database", Some(1)),
        };
        assert!(analysis.guard_condition.contains("step 2"));
        assert!(analysis.guard_condition.contains("Timeout"));
    }
}
