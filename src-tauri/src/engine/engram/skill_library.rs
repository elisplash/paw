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
// Built-in Canvas / Visualisation Skills (seeded at startup)
// ═════════════════════════════════════════════════════════════════════════════

/// Seed the skill library with built-in canvas visualisation knowledge.
///
/// Uses deterministic IDs (prefixed `builtin-canvas-*`) and INSERT OR REPLACE,
/// so re-running at startup is idempotent — no duplicates, always up to date.
///
/// Each skill teaches the agent:
///   1. Which CDN URL to put in the `libraries` array
///   2. What HTML/JS pattern to write
///   3. Sensible `height` and styling defaults
///
/// Returns the number of skills that were newly written.
pub fn seed_builtin_canvas_skills(store: &SessionStore) -> EngineResult<usize> {
    let skills = builtin_canvas_skills();
    let mut written = 0;
    for skill in &skills {
        store.engram_store_procedural(skill)?;
        written += 1;
    }
    info!("[skill_library] Seeded {} built-in canvas skills", written);
    Ok(written)
}

/// The catalogue of built-in canvas visualisation procedural memories.
fn builtin_canvas_skills() -> Vec<ProceduralMemory> {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let global_scope = MemoryScope {
        global: true,
        ..Default::default()
    };

    vec![
        // ── Three.js — 3D scenes, WebGL, particle systems ──────────────
        ProceduralMemory {
            id: "builtin-canvas-threejs".into(),
            trigger: "3d scene WebGL canvas visualization particles globe sphere rotating orbit".into(),
            steps: vec![
                ProceduralStep {
                    description: "Use embed type with libraries: [\"https://cdnjs.cloudflare.com/ajax/libs/three.js/r128/three.min.js\"]. Three.js is available as the global `THREE` object.".into(),
                    tool_name: Some("canvas_push".into()),
                    args_pattern: Some("{\"type\":\"embed\",\"data\":{\"libraries\":[\"https://cdnjs.cloudflare.com/ajax/libs/three.js/r128/three.min.js\"]}}".into()),
                    expected_outcome: Some("Three.js WebGL scene rendered inside iframe".into()),
                },
                ProceduralStep {
                    description: "Boilerplate: const scene = new THREE.Scene(); const camera = new THREE.PerspectiveCamera(60, canvas.clientWidth/canvas.clientHeight, 0.1, 1000); const renderer = new THREE.WebGLRenderer({canvas, antialias:true, alpha:true}); renderer.setSize(canvas.clientWidth, canvas.clientHeight); function animate(){ requestAnimationFrame(animate); renderer.render(scene,camera); } animate();".into(),
                    tool_name: None,
                    args_pattern: None,
                    expected_outcome: Some("Render loop running".into()),
                },
                ProceduralStep {
                    description: "Set height 480–800 for 3D scenes. Dark background (#020008 or #0a0a0f) makes glow effects pop. Use THREE.AdditiveBlending for glowing particles. Camera at z=300 for medium-scale scenes. Auto-rotate: scene.rotation.y += 0.0003 each frame.".into(),
                    tool_name: None,
                    args_pattern: None,
                    expected_outcome: Some("Polished 3D presentation".into()),
                },
            ],
            success_rate: 1.0,
            execution_count: 0,
            scope: global_scope.clone(),
            created_at: now.clone(),
            updated_at: None,
        },

        // ── D3.js — data-driven SVG, force graphs, maps, funnels ───────
        ProceduralMemory {
            id: "builtin-canvas-d3".into(),
            trigger: "data visualization graph chart SVG force network funnel bar pie donut map D3".into(),
            steps: vec![
                ProceduralStep {
                    description: "Use embed type with libraries: [\"https://cdnjs.cloudflare.com/ajax/libs/d3/7.8.5/d3.min.js\"]. D3 is available as the global `d3` object.".into(),
                    tool_name: Some("canvas_push".into()),
                    args_pattern: Some("{\"type\":\"embed\",\"data\":{\"libraries\":[\"https://cdnjs.cloudflare.com/ajax/libs/d3/7.8.5/d3.min.js\"]}}".into()),
                    expected_outcome: Some("D3 SVG visualization rendered".into()),
                },
                ProceduralStep {
                    description: "Pattern: const svg = d3.select('#chart').append('svg').attr('width','100%').attr('height', height); const g = svg.append('g').attr('transform','translate(margin.left,margin.top)'); — always set viewBox and use responsive width. For funnels: use trapezoid paths with d3.line(). For force graphs: d3.forceSimulation() + d3.forceLink() + d3.forceManyBody(). For geo maps: d3.geoMercator() + d3.geoPath().".into(),
                    tool_name: None,
                    args_pattern: None,
                    expected_outcome: Some("Responsive D3 chart".into()),
                },
                ProceduralStep {
                    description: "Height 350–520 for charts. Add CSS transitions for data updates. Tooltip pattern: div.style('opacity',0) → on mouseover .style('opacity',1).style('left',(event.pageX+10)+'px'). Use d3.schemeTableau10 or d3.interpolatePlasma for color scales.".into(),
                    tool_name: None,
                    args_pattern: None,
                    expected_outcome: Some("Interactive D3 chart with tooltips".into()),
                },
            ],
            success_rate: 1.0,
            execution_count: 0,
            scope: global_scope.clone(),
            created_at: now.clone(),
            updated_at: None,
        },

        // ── GSAP — animation timeline, tweens, ScrollTrigger ───────────
        ProceduralMemory {
            id: "builtin-canvas-gsap".into(),
            trigger: "animation animate tween smooth transition morph timeline stagger entrance motion".into(),
            steps: vec![
                ProceduralStep {
                    description: "Use embed type with libraries: [\"https://cdnjs.cloudflare.com/ajax/libs/gsap/3.12.2/gsap.min.js\"]. GSAP is available as the global `gsap` object.".into(),
                    tool_name: Some("canvas_push".into()),
                    args_pattern: Some("{\"type\":\"embed\",\"data\":{\"libraries\":[\"https://cdnjs.cloudflare.com/ajax/libs/gsap/3.12.2/gsap.min.js\"]}}".into()),
                    expected_outcome: Some("GSAP animation running".into()),
                },
                ProceduralStep {
                    description: "Key patterns: gsap.from('.card', {opacity:0, y:40, stagger:0.1, duration:0.6, ease:'power2.out'}) for staggered entrance. gsap.to('#counter', {innerText: targetValue, duration:2, snap:{innerText:1}}) for animated counters. gsap.timeline() for sequenced multi-step animations. Combine with D3 or Three.js for data-driven motion.".into(),
                    tool_name: None,
                    args_pattern: None,
                    expected_outcome: Some("Smooth entrance and data animations".into()),
                },
                ProceduralStep {
                    description: "GSAP pairs perfectly with plain CSS elements — no canvas element needed. Use it for counter animations, card entrances, progress bar fills, and morphing SVG shapes.".into(),
                    tool_name: None,
                    args_pattern: None,
                    expected_outcome: Some("Polished animated dashboard elements".into()),
                },
            ],
            success_rate: 1.0,
            execution_count: 0,
            scope: global_scope.clone(),
            created_at: now.clone(),
            updated_at: None,
        },

        // ── Chart.js — declarative charts, fast and clean ──────────────
        ProceduralMemory {
            id: "builtin-canvas-chartjs".into(),
            trigger: "line chart bar chart area chart pie donut revenue trend sparkline quick chart".into(),
            steps: vec![
                ProceduralStep {
                    description: "Use embed type with libraries: [\"https://cdnjs.cloudflare.com/ajax/libs/Chart.js/4.4.0/chart.umd.min.js\"]. Available as global `Chart`.".into(),
                    tool_name: Some("canvas_push".into()),
                    args_pattern: Some("{\"type\":\"embed\",\"data\":{\"libraries\":[\"https://cdnjs.cloudflare.com/ajax/libs/Chart.js/4.4.0/chart.umd.min.js\"]}}".into()),
                    expected_outcome: Some("Chart.js canvas chart rendered".into()),
                },
                ProceduralStep {
                    description: "Pattern: new Chart(document.getElementById('myChart'), { type:'line', data:{labels:[...],datasets:[{label:'Revenue',data:[...],borderColor:'#6366f1',backgroundColor:'rgba(99,102,241,0.1)',fill:true,tension:0.4}]}, options:{responsive:true,plugins:{legend:{display:false}},scales:{y:{beginAtZero:false,grid:{color:'rgba(255,255,255,0.05)'}}}} }).".into(),
                    tool_name: None,
                    args_pattern: None,
                    expected_outcome: Some("Styled Chart.js visualization".into()),
                },
                ProceduralStep {
                    description: "Chart.js is the fastest option for standard line/bar/pie charts. Use D3 when you need custom shapes (funnels, force graphs, geo). For dark backgrounds set Chart.defaults.color='rgba(255,255,255,0.7)' before creating any chart.".into(),
                    tool_name: None,
                    args_pattern: None,
                    expected_outcome: Some("Production-ready chart".into()),
                },
            ],
            success_rate: 1.0,
            execution_count: 0,
            scope: global_scope.clone(),
            created_at: now.clone(),
            updated_at: None,
        },

        // ── Pixi.js — 2D WebGL, particles, sprites, filters ────────────
        ProceduralMemory {
            id: "builtin-canvas-pixijs".into(),
            trigger: "2d game particles sprites pixel effects filter real-time animation WebGL 2D".into(),
            steps: vec![
                ProceduralStep {
                    description: "Use embed type with libraries: [\"https://cdnjs.cloudflare.com/ajax/libs/pixi.js/7.3.2/pixi.min.js\"]. Available as global `PIXI`.".into(),
                    tool_name: Some("canvas_push".into()),
                    args_pattern: Some("{\"type\":\"embed\",\"data\":{\"libraries\":[\"https://cdnjs.cloudflare.com/ajax/libs/pixi.js/7.3.2/pixi.min.js\"]}}".into()),
                    expected_outcome: Some("Pixi.js WebGL renderer initialized".into()),
                },
                ProceduralStep {
                    description: "Pattern: const app = new PIXI.Application({width:800,height:480,backgroundColor:0x0a0a0f,antialias:true}); document.body.appendChild(app.view); app.ticker.add(()=>{ /* update each frame */ }); — use PIXI.Graphics for drawing, PIXI.ParticleContainer for high-performance particles (up to 200k sprites).".into(),
                    tool_name: None,
                    args_pattern: None,
                    expected_outcome: Some("60fps 2D WebGL scene".into()),
                },
                ProceduralStep {
                    description: "Ideal for: particle bursts, animated icons, pixel-art dashboards, real-time signal visualizers, waveform displays. Supports WebGL filters (blur, glow, color matrix).".into(),
                    tool_name: None,
                    args_pattern: None,
                    expected_outcome: Some("Smooth 2D effects".into()),
                },
            ],
            success_rate: 1.0,
            execution_count: 0,
            scope: global_scope.clone(),
            created_at: now.clone(),
            updated_at: None,
        },

        // ── Matter.js — physics simulation ─────────────────────────────
        ProceduralMemory {
            id: "builtin-canvas-matterjs".into(),
            trigger: "physics simulation gravity collision rigid body bounce ball budget bubbles weight".into(),
            steps: vec![
                ProceduralStep {
                    description: "Use embed type with libraries: [\"https://cdnjs.cloudflare.com/ajax/libs/matter-js/0.19.0/matter.min.js\"]. Available as global `Matter`. Destructure: const { Engine, Render, Runner, Bodies, Composite, Mouse, MouseConstraint } = Matter;".into(),
                    tool_name: Some("canvas_push".into()),
                    args_pattern: Some("{\"type\":\"embed\",\"data\":{\"libraries\":[\"https://cdnjs.cloudflare.com/ajax/libs/matter-js/0.19.0/matter.min.js\"]}}".into()),
                    expected_outcome: Some("Matter.js physics world running".into()),
                },
                ProceduralStep {
                    description: "Pattern: const engine = Engine.create(); const render = Render.create({element:document.body,engine,options:{width:800,height:480,wireframes:false,background:'#0a0a0f'}}); Runner.run(Runner.create(), engine); Render.run(render); — Add bodies: Composite.add(engine.world, [Bodies.circle(400,200,40,{restitution:0.8,render:{fillStyle:'#6366f1'}})]); — Add walls for bouncing: Bodies.rectangle with isStatic:true.".into(),
                    tool_name: None,
                    args_pattern: None,
                    expected_outcome: Some("Bouncing physics objects".into()),
                },
                ProceduralStep {
                    description: "Great creative use: budget visualization where each budget item is a bubble with radius proportional to spend, and they bounce around colliding. Or a priority queue where tasks are falling balls landing in labeled bins.".into(),
                    tool_name: None,
                    args_pattern: None,
                    expected_outcome: Some("Engaging physics-based data viz".into()),
                },
            ],
            success_rate: 1.0,
            execution_count: 0,
            scope: global_scope.clone(),
            created_at: now.clone(),
            updated_at: None,
        },

        // ── p5.js — creative coding, generative art ─────────────────────
        ProceduralMemory {
            id: "builtin-canvas-p5js".into(),
            trigger: "generative art creative coding procedural drawing sketch noise Perlin wave abstract".into(),
            steps: vec![
                ProceduralStep {
                    description: "Use embed type with libraries: [\"https://cdnjs.cloudflare.com/ajax/libs/p5.js/1.9.0/p5.min.js\"]. p5 is available as the global `p5` object. Use instance mode to avoid polluting globals: const sketch = (p) => { p.setup = () => {}; p.draw = () => {}; }; new p5(sketch);".into(),
                    tool_name: Some("canvas_push".into()),
                    args_pattern: Some("{\"type\":\"embed\",\"data\":{\"libraries\":[\"https://cdnjs.cloudflare.com/ajax/libs/p5.js/1.9.0/p5.min.js\"]}}".into()),
                    expected_outcome: Some("p5.js generative sketch running".into()),
                },
                ProceduralStep {
                    description: "Key APIs: p.noise(x,y) for Perlin noise (smooth organic movement). p.map(v,lo,hi,outLo,outHi) for value remapping. p.lerpColor(c1,c2,t) for gradient interpolation. p.frameCount for time-based animation. p.createVector() + p.PVector for physics math.".into(),
                    tool_name: None,
                    args_pattern: None,
                    expected_outcome: Some("Organic generative visual".into()),
                },
                ProceduralStep {
                    description: "Ideal for: generative backgrounds, data art where numbers drive organic shapes, Perlin flow fields, real-time audio-reactive visuals, abstract status displays.".into(),
                    tool_name: None,
                    args_pattern: None,
                    expected_outcome: Some("Living generative art embedded in dashboard".into()),
                },
            ],
            success_rate: 1.0,
            execution_count: 0,
            scope: global_scope.clone(),
            created_at: now.clone(),
            updated_at: None,
        },

        // ── Three.js + D3 combo — data-driven 3D ────────────────────────
        ProceduralMemory {
            id: "builtin-canvas-threejs-d3".into(),
            trigger: "3D globe world map network connections arc flight path data-driven 3D".into(),
            steps: vec![
                ProceduralStep {
                    description: "Combine both libraries: libraries: [\"https://cdnjs.cloudflare.com/ajax/libs/three.js/r128/three.min.js\",\"https://cdnjs.cloudflare.com/ajax/libs/d3/7.8.5/d3.min.js\"]. Use D3 for data processing (scales, projections, color) and Three.js for 3D rendering.".into(),
                    tool_name: Some("canvas_push".into()),
                    args_pattern: Some("{\"type\":\"embed\",\"data\":{\"libraries\":[\"https://cdnjs.cloudflare.com/ajax/libs/three.js/r128/three.min.js\",\"https://cdnjs.cloudflare.com/ajax/libs/d3/7.8.5/d3.min.js\"]}}".into()),
                    expected_outcome: Some("3D data visualization with D3 data processing".into()),
                },
                ProceduralStep {
                    description: "Globe pattern: THREE.SphereGeometry for the globe, d3.geoMercator() to convert lat/lon to 3D coordinates via spherical math: x=r*cos(lat)*cos(lon), y=r*sin(lat), z=r*cos(lat)*sin(lon). Draw arc connections with THREE.QuadraticBezierCurve3, extruding control point outward.".into(),
                    tool_name: None,
                    args_pattern: None,
                    expected_outcome: Some("Interactive 3D globe with data arcs".into()),
                },
                ProceduralStep {
                    description: "Set height 560–800 for globe views. Add OrbitControls-equivalent mouse drag: track mousedown delta, apply to scene.rotation. Dot the globe surface with BufferGeometry + Points for a realistic globe texture effect.".into(),
                    tool_name: None,
                    args_pattern: None,
                    expected_outcome: Some("Stunning 3D globe dashboard".into()),
                },
            ],
            success_rate: 1.0,
            execution_count: 0,
            scope: global_scope.clone(),
            created_at: now.clone(),
            updated_at: None,
        },

        // ── Chart.js + GSAP combo — animated business dashboards ────────
        ProceduralMemory {
            id: "builtin-canvas-chartjs-gsap".into(),
            trigger: "animated business dashboard KPI metrics counter entrance sales revenue animated chart".into(),
            steps: vec![
                ProceduralStep {
                    description: "Combine Chart.js for charts with GSAP for counter animations and card entrances: libraries: [\"https://cdnjs.cloudflare.com/ajax/libs/Chart.js/4.4.0/chart.umd.min.js\",\"https://cdnjs.cloudflare.com/ajax/libs/gsap/3.12.2/gsap.min.js\"].".into(),
                    tool_name: Some("canvas_push".into()),
                    args_pattern: Some("{\"type\":\"embed\",\"data\":{\"libraries\":[\"https://cdnjs.cloudflare.com/ajax/libs/Chart.js/4.4.0/chart.umd.min.js\",\"https://cdnjs.cloudflare.com/ajax/libs/gsap/3.12.2/gsap.min.js\"]}}".into()),
                    expected_outcome: Some("Animated dashboard with live charts and counters".into()),
                },
                ProceduralStep {
                    description: "Pattern: on load, gsap.from('.kpi-card',{opacity:0,y:30,stagger:0.08,duration:0.5}) for card entrances. For counter animation: gsap.to(el,{innerText:targetNumber,duration:2,snap:{innerText:1},ease:'power2.out'}). Then initialize Chart.js charts after GSAP entrance completes (use onComplete callback).".into(),
                    tool_name: None,
                    args_pattern: None,
                    expected_outcome: Some("Cards fly in, counters count up, charts draw".into()),
                },
                ProceduralStep {
                    description: "This combo rivals Tableau/PowerBI for business KPI dashboards. Height 480–600. Dark background (#0f172a) with card backgrounds (#1e293b), indigo/emerald accent colors (#6366f1, #10b981).".into(),
                    tool_name: None,
                    args_pattern: None,
                    expected_outcome: Some("Executive-grade animated KPI dashboard".into()),
                },
            ],
            success_rate: 1.0,
            execution_count: 0,
            scope: global_scope.clone(),
            created_at: now.clone(),
            updated_at: None,
        },

        // ── Leaflet — interactive maps with data pins ────────────────────
        ProceduralMemory {
            id: "builtin-canvas-leaflet".into(),
            trigger: "map location geography pins markers heat map customer locations stores offices".into(),
            steps: vec![
                ProceduralStep {
                    description: "Use Leaflet: libraries: [\"https://unpkg.com/leaflet@1.9.4/dist/leaflet.js\"]. Also include the Leaflet CSS in the html field: <link rel='stylesheet' href='https://unpkg.com/leaflet@1.9.4/dist/leaflet.css'/>. The iframe has allow-scripts sandbox so external CSS links work fine.".into(),
                    tool_name: Some("canvas_push".into()),
                    args_pattern: Some("{\"type\":\"embed\",\"data\":{\"libraries\":[\"https://unpkg.com/leaflet@1.9.4/dist/leaflet.js\"]}}".into()),
                    expected_outcome: Some("Interactive Leaflet map rendered".into()),
                },
                ProceduralStep {
                    description: "Pattern: const map = L.map('map').setView([lat,lon],zoom); L.tileLayer('https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png',{attribution:'© OpenStreetMap'}).addTo(map); — Add markers: L.marker([lat,lon]).bindPopup('<b>Name</b><br>details').addTo(map); — For clusters: use Leaflet.markercluster CDN.".into(),
                    tool_name: None,
                    args_pattern: None,
                    expected_outcome: Some("Map with data pins and popups".into()),
                },
                ProceduralStep {
                    description: "Height 420–600 for maps. OpenStreetMap tiles are free, no API key needed. For dark map tiles use: 'https://{s}.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}{r}.png' (CartoDB dark matter — also free, no key).".into(),
                    tool_name: None,
                    args_pattern: None,
                    expected_outcome: Some("No-key-required interactive map".into()),
                },
            ],
            success_rate: 1.0,
            execution_count: 0,
            scope: global_scope.clone(),
            created_at: now.clone(),
            updated_at: None,
        },

        // ── Pure Canvas API — no library needed for simple fx ───────────
        ProceduralMemory {
            id: "builtin-canvas-api".into(),
            trigger: "custom draw hex grid gradient glow scan-line waveform signal oscilloscope raw canvas".into(),
            steps: vec![
                ProceduralStep {
                    description: "For custom drawing effects that don't need a library, use the browser's native Canvas 2D API directly — no libraries array needed. Access: const canvas = document.getElementById('c'); const ctx = canvas.getContext('2d');".into(),
                    tool_name: Some("canvas_push".into()),
                    args_pattern: Some("{\"type\":\"embed\",\"data\":{\"libraries\":[]}}".into()),
                    expected_outcome: Some("Native Canvas 2D drawing".into()),
                },
                ProceduralStep {
                    description: "Glow effect: ctx.shadowBlur=20; ctx.shadowColor='#00f5ff'; then draw. Gradient fill under line: ctx.createLinearGradient(x0,y0,x1,y1) → addColorStop. Hex grid: loop with 6-corner polygon math. Scan-line: CSS repeating-linear-gradient(transparent 0,transparent 1px,rgba(0,0,0,0.15) 1px,rgba(0,0,0,0.15) 2px) over the canvas. requestAnimationFrame loop for animation.".into(),
                    tool_name: None,
                    args_pattern: None,
                    expected_outcome: Some("Custom drawn effects".into()),
                },
                ProceduralStep {
                    description: "For hybrid effects: use native Canvas API for the background/overlay layer (hex grid, scan-lines, glow aura) and Chart.js or D3 for the data layer on top. Layer them with position:absolute in the HTML.".into(),
                    tool_name: None,
                    args_pattern: None,
                    expected_outcome: Some("Layered canvas effect".into()),
                },
            ],
            success_rate: 1.0,
            execution_count: 0,
            scope: global_scope.clone(),
            created_at: now.clone(),
            updated_at: None,
        },
    ]
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
