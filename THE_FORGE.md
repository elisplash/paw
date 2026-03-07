# THE FORGE — AI Agents That Earn Expertise

**Status:** Proposal  
**Author:** Team Lead  
**Date:** March 2026  
**Target:** OpenPaws Platform  

---

## Executive Summary

Every AI platform today creates "specialists" the same way: paste a markdown file into the system prompt. That's not expertise — it's a cheat sheet. THE FORGE is a training system where a Master Craftsman agent teaches specialist agents through structured curriculum ingestion, iterative testing, and verified skill certification. All earned knowledge persists in Engram, our existing agent memory system.

**The moat:** You can copy a prompt file. You can't copy thousands of verified training cycles stored in Engram.

**Why now:** No one in the market is doing this well. Every competitor (AutoGPT, CrewAI, LangGraph, OpenAI Assistants) relies on static instructions. None have a system that verifies competence, tracks skill confidence, detects knowledge gaps, or triggers re-training when domains change. We already have Engram (biologically-inspired memory with skill storage, meta-cognition, and decay) and the Orchestrator (boss/worker multi-agent coordination). THE FORGE is the next logical layer on top of both.

---

## Table of Contents

1. [The Problem](#the-problem)
2. [The Solution](#the-solution)
3. [Architecture Overview](#architecture-overview)
4. [Component Deep Dive](#component-deep-dive)
5. [Integration With Existing Systems](#integration-with-existing-systems)
6. [Implementation Plan](#implementation-plan)
7. [Design Decisions](#design-decisions)
8. [Risk Assessment](#risk-assessment)
9. [First Target: HubSpot Specialist](#first-target-hubspot-specialist)
10. [Success Metrics](#success-metrics)
11. [Competitive Landscape](#competitive-landscape)
12. [Open Questions for Team Discussion](#open-questions-for-team-discussion)

---

## The Problem

### How Everyone Builds AI "Specialists" Today

```
System Prompt = "You are an expert in HubSpot. Here are the key concepts..."
+ 5,000 tokens of documentation
= "Specialist"
```

This approach has fundamental flaws:

| Problem | Impact |
|---------|--------|
| **No verification** | The agent claims expertise but has never been tested. It might hallucinate confidently about deprecated features. |
| **No knowledge boundaries** | The agent doesn't know what it doesn't know. It will answer questions about HubSpot features it's never encountered with the same confidence as features covered in its prompt. |
| **No evolution** | When HubSpot releases a new API version, the "specialist" is instantly outdated. No one notices until a customer hits a failure. |
| **No failure learning** | When the agent gives a wrong answer, there's no feedback loop. It will give the same wrong answer next time. |
| **Trivially copyable** | Your competitor copies the prompt file and has the same "specialist." Zero moat. |

### What Real Expertise Looks Like

A human HubSpot expert:
- Studied the platform systematically (curriculum)
- Was tested on their knowledge (certification)
- Knows their weak areas and says "I need to check that" (metacognition)
- Learns from mistakes in the field (failure feedback)
- Stays current as the platform evolves (continuous learning)
- Can prove their expertise with a track record (verifiable credentials)

THE FORGE gives agents all six properties.

---

## The Solution

### Core Concept: AI Training AI

```
┌─────────────────────────────────────────────────────────┐
│                    THE FORGE                             │
│                                                         │
│  ┌─────────────┐    ┌──────────────┐    ┌────────────┐  │
│  │   MASTER     │    │  CURRICULUM   │    │  TESTING   │  │
│  │  CRAFTSMAN   │───▶│  PIPELINE     │───▶│  ENGINE    │  │
│  │  (teacher)   │    │  (syllabus)   │    │  (exams)   │  │
│  └──────┬───────┘    └──────────────┘    └─────┬──────┘  │
│         │                                       │        │
│         │         ┌──────────────┐              │        │
│         │         │    ENGRAM     │              │        │
│         └────────▶│  (memory +   │◀─────────────┘        │
│                   │   skills)    │                        │
│                   └──────┬───────┘                        │
│                          │                               │
│                   ┌──────▼───────┐                        │
│                   │   FORGED     │                        │
│                   │  SPECIALIST  │                        │
│                   │  (graduate)  │                        │
│                   └──────────────┘                        │
└─────────────────────────────────────────────────────────┘
```

1. **Craftsman** ingests domain knowledge (docs, courses, APIs)
2. **Curriculum Pipeline** decomposes it into a skill tree of testable atomic competencies
3. **Training Loop** teaches and tests each skill iteratively
4. **Engram** stores certified knowledge with confidence scores
5. **Forged Specialist** queries its verified knowledge before responding, flags uncertainty, and reports failures back for re-training

---

## Architecture Overview

```
                         ┌──────────────────┐
                         │  Domain Sources   │
                         │  (docs, APIs,     │
                         │   courses, wikis) │
                         └────────┬─────────┘
                                  │
                    ┌─────────────▼──────────────┐
                    │      CURRICULUM ENGINE      │
                    │  ┌──────────────────────┐   │
                    │  │ Source Ingestion      │   │
                    │  │ Skill Decomposition   │   │
                    │  │ Dependency Mapping    │   │
                    │  │ Lineage Tracking      │   │
                    │  └──────────────────────┘   │
                    └─────────────┬───────────────┘
                                  │
                         Skill Tree (DAG)
                                  │
              ┌───────────────────▼────────────────────┐
              │          MASTER CRAFTSMAN               │
              │                                        │
              │   For each uncertified skill:          │
              │   ┌────────────────────────────────┐   │
              │   │  1. Teach (inject material)    │   │
              │   │  2. Test (run scenario)         │   │
              │   │  3. Evaluate (score result)     │   │
              │   │  4a. Pass → Certify in Engram   │   │
              │   │  4b. Fail → Analyze gap →       │   │
              │   │         Reinforce → Re-test     │   │
              │   └────────────────────────────────┘   │
              └───────────────────┬────────────────────┘
                                  │
                    ┌─────────────▼──────────────┐
                    │         ENGRAM              │
                    │  ┌──────────────────────┐   │
                    │  │ Certified Skills     │   │
                    │  │ Confidence Scores    │   │
                    │  │ Test History         │   │
                    │  │ Failure Patterns     │   │
                    │  │ Dependency Graph     │   │
                    │  │ Curriculum Lineage   │   │
                    │  └──────────────────────┘   │
                    └─────────────┬───────────────┘
                                  │
              ┌───────────────────▼────────────────────┐
              │         FORGED SPECIALIST               │
              │                                        │
              │   On every query:                      │
              │   1. Check Engram for certified skill  │
              │   2. If certified + high confidence    │
              │      → Respond with authority          │
              │   3. If certified + low confidence     │
              │      → Respond with caveat             │
              │   4. If uncertified                    │
              │      → "I don't have verified          │
              │         knowledge on this"             │
              │   5. Log failures → feed back to       │
              │      Craftsman for re-training         │
              └────────────────────────────────────────┘
                                  │
                    ┌─────────────▼──────────────┐
                    │   CONTINUOUS EVOLUTION      │
                    │                            │
                    │   External: Domain changes │
                    │   → Re-certify affected    │
                    │     skills                 │
                    │                            │
                    │   Internal: Prod failures  │
                    │   → Gap analysis           │
                    │   → Re-training cycle      │
                    │                            │
                    │   Temporal: Skill decay    │
                    │   → Periodic re-tests      │
                    └────────────────────────────┘
```

---

## Component Deep Dive

### 1. Master Craftsman

The Craftsman is not a new binary or service. It's a regular OpenPaws agent configured as a **boss** in the Orchestrator, with Specialists as workers. This means we reuse our entire existing multi-agent infrastructure.

**Responsibilities:**
- Discover and ingest domain knowledge (docs, courses, APIs, real workflows)
- Decompose domains into atomic, testable skills
- Design tests that prove comprehension (not just recall)
- Coach specialists through failures with targeted reinforcement
- Trigger re-training when domains update or production failures occur

**Implementation:** A Craftsman agent profile with a FORGE-specific system prompt, plus a set of FORGE-specific tools registered in the tool executor:

| Tool | Purpose |
|------|---------|
| `forge_ingest_curriculum` | Ingest source material and generate skill tree |
| `forge_run_test` | Execute a test scenario against a Specialist |
| `forge_certify_skill` | Mark a skill as certified with confidence score |
| `forge_analyze_failure` | Root-cause a test failure and generate reinforcement |
| `forge_check_domain_updates` | Check curriculum sources for changes |

### 2. Curriculum Pipeline

**Input:** Raw domain sources — URLs, documents, API specs, video transcripts, course modules.

**Output:** A skill tree — a DAG of atomic competencies with dependencies.

```
hubspot (domain)
├── contacts (module)
│   ├── contact_properties (concept)
│   │   ├── create_property          [atomic skill]
│   │   ├── property_types           [atomic skill]
│   │   └── calculated_properties    [atomic skill]
│   ├── contact_lists (concept)
│   │   ├── static_lists             [atomic skill]
│   │   └── active_lists             [atomic skill]
│   └── contact_lifecycle (concept)
│       ├── lifecycle_stages         [atomic skill]
│       └── lead_status              [atomic skill]
├── workflows (module)
│   ├── triggers (concept)
│   │   ├── deal_stage_trigger       [atomic skill]  ← depends on: deals.stages
│   │   ├── form_submission_trigger  [atomic skill]  ← depends on: forms.basics
│   │   └── re_enrollment_triggers   [atomic skill]  ← depends on: triggers.*
│   ├── actions (concept)
│   │   ├── send_email_action        [atomic skill]
│   │   ├── delay_action             [atomic skill]
│   │   └── webhook_action           [atomic skill]
│   └── branching (concept)
│       ├── if_then_branching        [atomic skill]
│       └── value_equals_branching   [atomic skill]
└── deals (module)
    └── stages (concept)
        ├── pipeline_setup           [atomic skill]
        └── stage_properties         [atomic skill]
```

**Boundary rule:** If you can't write a test for it in isolation, it's not atomic enough. If two skills always pass or fail together, they're actually one skill.

**Lineage tracking:** Every atomic skill links back to its source material. When a source changes, we know exactly which skills need re-verification.

### 3. Training Loop

```
for each skill in topological_order(skill_tree):
    # Respect dependencies — prerequisites must be certified first
    assert all(dependency.certified for dependency in skill.dependencies)

    attempts = 0
    while not skill.certified and attempts < MAX_ATTEMPTS:
        attempts += 1

        # TEACH: Inject learning material into Specialist's context
        teach(specialist, skill.learning_material)

        # TEST: Run practical scenario(s)
        for scenario in skill.test_scenarios:
            result = run_test(specialist, scenario)

            if result.passed:
                # CERTIFY: Store in Engram with confidence score
                engram.certify(skill, compute_confidence(skill))
            else:
                # ANALYZE: Identify specific gap
                gap = analyze_failure(result)
                # REINFORCE: Generate targeted material for the gap
                skill.learning_material.add(reinforcement_for(gap))
                # LOG: Store failure pattern for future reference
                engram.log_failure(skill, gap)

    if not skill.certified:
        flag_for_human_review(skill)
```

**Key design choices:**
- Skills are trained in dependency order (topological sort of the skill DAG)
- Maximum retry attempts prevent infinite token burn
- Failures are analyzed and stored — not just retried blindly
- Uncertifiable skills escalate to humans rather than silently failing

### 4. Engram Storage (Extended)

We extend Engram's existing skill schema — we do NOT build parallel storage. This means FORGE-trained skills benefit from all existing Engram capabilities: hybrid search, graph edges, spreading activation, decay, meta-cognition maps, and memory bus sync.

**New fields on existing skill records:**

| Field | Type | Purpose |
|-------|------|---------|
| `provenance` | enum | `usage_extracted` (existing) or `forge_certified` (new) |
| `certification_status` | enum | `uncertified`, `in_training`, `certified`, `expired`, `failed` |
| `domain` | string | Top-level domain (e.g., `hubspot`) |
| `skill_tree_path` | string | Full path (e.g., `hubspot.workflows.triggers.deal_stage`) |
| `test_history` | JSON | Array of `{ timestamp, scenario_id, passed, score, failure_reason }` |
| `dependencies` | JSON | Array of prerequisite `skill_tree_path` values |
| `curriculum_source` | string | URL or document reference (lineage tracking) |
| `certified_at` | timestamp | When last certified |
| `expires_at` | timestamp | When certification lapses |
| `confidence` | float | Composite confidence score (0.0–1.0) |

**Example Engram record for a FORGE-certified skill:**

```json
{
  "skill_id": "hubspot.workflows.triggers.deal_stage",
  "provenance": "forge_certified",
  "certification_status": "certified",
  "confidence": 0.94,
  "domain": "hubspot",
  "skill_tree_path": "hubspot.workflows.triggers.deal_stage",
  "certified_at": "2026-03-15T10:30:00Z",
  "expires_at": "2026-06-15T10:30:00Z",
  "test_history": [
    { "timestamp": "2026-03-15T10:00:00Z", "passed": false, "score": 0.4, "failure_reason": "Confused enrollment triggers with re-enrollment triggers" },
    { "timestamp": "2026-03-15T10:15:00Z", "passed": false, "score": 0.7, "failure_reason": "Correct concept but wrong API field name" },
    { "timestamp": "2026-03-15T10:30:00Z", "passed": true, "score": 0.94 }
  ],
  "dependencies": ["hubspot.workflows.basics", "hubspot.deals.stages"],
  "curriculum_source": "https://academy.hubspot.com/courses/workflows",
  "learning_material": "...",
  "test_scenarios": ["..."]
}
```

### 5. Forged Specialist (Production Behavior)

The Specialist is also a regular OpenPaws agent. What makes it different is its behavioral contract:

1. **Before responding**, query Engram for certified skills matching the query domain
2. **If certified + high confidence** → respond authoritatively, citing the verified knowledge
3. **If certified + low confidence** → respond with a caveat: "I have some training on this but my confidence is moderate — please verify"
4. **If uncertified** → explicitly state: "I don't have verified knowledge about this specific topic"
5. **On failure** → log the failure and notify the Craftsman for re-training

This behavior is enforced through the Specialist's system prompt + meta-cognition integration (which already exists in Engram's `meta_cognition.rs`).

### 6. Confidence Scoring

Confidence is a composite score, not a simple pass rate:

$$C = w_p \cdot P + w_v \cdot V + w_r \cdot R + w_d \cdot D$$

| Signal | Symbol | What it measures | Weight (API domains) | Weight (knowledge domains) |
|--------|--------|------------------|---------------------|---------------------------|
| Pass rate | $P$ | % of test variations passed | 0.45 | 0.30 |
| Consistency | $V$ | Same answer to equivalent questions | 0.20 | 0.35 |
| Recency | $R$ | Time since last verification ($e^{-\lambda \Delta t}$) | 0.20 | 0.15 |
| Dependency health | $D$ | Average confidence of prerequisites | 0.15 | 0.20 |

### 7. Test Generation (Layered)

Not all skills need the same testing rigor:

| Tier | Method | Use when | Example |
|------|--------|----------|---------|
| **L0 — Deterministic** | API sandbox, schema validation, output diff | Skill has a verifiable external artifact | "Create a HubSpot workflow that triggers on deal stage change" → actually call the API, check if the workflow was created correctly |
| **L1 — Template** | Fill-in scenario structures with randomized parameters | Conceptual skills with structured answers | "What property type should you use for [X]?" with 5 different X values |
| **L2 — LLM-as-Judge** | Structured rubric with required elements, scored per-element | Reasoning and synthesis skills | "Explain when to use active lists vs static lists" → rubric checks for 4 required points |
| **L3 — Adversarial** | Craftsman generates plausible-but-wrong scenarios | Deep comprehension | "A user reports their workflow isn't triggering. The enrollment trigger is set to [subtly wrong config]. What's the issue?" |

**For HubSpot (API domain):** L0 dominates. HubSpot offers developer test portals — workflows either trigger correctly or they don't. That's ground truth.

### 8. Continuous Evolution

Two feedback triggers ensure the Specialist never goes stale:

**External (domain changes):**
```
Craftsman periodically checks curriculum sources (API changelogs, docs)
  → Detects change (content hash differs)
  → Identifies affected skills via curriculum lineage DAG
  → Queues affected skills for re-certification
  → Runs re-test cycle
  → Updates Engram records
```

**Internal (production failures):**
```
Specialist encounters failure in production
  → Downstream task error (API returned 400)
  → OR user gives negative feedback
  → OR self-reported low confidence
  → Failure logged in Engram with context
  → Craftsman runs gap analysis
  → Targeted re-training on specific failure pattern
  → Re-certification
```

**Temporal (skill decay):**
```
Confidence decays: C(t) = C₀ · e^(-λt)
  → λ is domain-specific (APIs change fast → high λ)
  → When C(t) drops below re-certification threshold → queue re-test
  → Aligns with Engram's existing Ebbinghaus decay curves
```

---

## Integration With Existing Systems

THE FORGE is not a greenfield build. It layers directly on top of existing OpenPaws infrastructure:

| Existing System | How FORGE Uses It |
|----------------|-------------------|
| **Engram Memory** | Stores all certified skills, test history, confidence scores. Uses existing hybrid search, graph edges, decay, and meta-cognition. Extended with new fields, not replaced. |
| **Engram Skill Library** | FORGE-certified skills coexist with usage-extracted skills. Both feed into the same retrieval pipeline. Distinguished by `provenance` field. |
| **Engram Meta-Cognition** | Existing "I know / I don't know" maps per domain. FORGE enriches these with formal certification data. Specialist's "knows its boundaries" behavior is already infrastructure-supported. |
| **Orchestrator (Boss/Worker)** | Craftsman = Boss agent. Specialist = Worker agent. Training sessions use the existing `sub_agent.rs` spawning mechanism. No new orchestration infrastructure needed. |
| **Tool Executor** | FORGE-specific tools (`forge_ingest_curriculum`, `forge_run_test`, etc.) register in the existing tool system alongside all other tools. |
| **Provider Abstraction** | Craftsman and Specialist can use any configured LLM provider. No provider coupling. |
| **Container Sandbox** | API-testable skills (L0 tier) can execute real API calls inside the existing Docker sandbox. |
| **Reflexion Failure Path** | Existing in Engram's Skill Library — failed skill executions trigger analysis and store negative examples. FORGE extends this with re-certification queuing. |
| **Memory Bus** | Multi-agent memory sync. If multiple Specialists are forged, they can share certified knowledge via the existing CRDT-inspired protocol. |

**Estimated new code vs. reused code:**

| Component | New Code | Reused Infrastructure |
|-----------|----------|----------------------|
| Curriculum engine | ~1,500 lines Rust | Fetch tools, LLM provider calls |
| Test engine | ~2,000 lines Rust | Container sandbox, tool executor |
| Craftsman tools | ~800 lines Rust | Tool registration system |
| Engram schema extension | ~200 lines SQL/Rust | Entire Engram memory system |
| Specialist behavior | ~300 lines (system prompt + config) | Agent loop, meta-cognition |
| Background re-certification | ~500 lines Rust | Consolidation engine pattern |
| Frontend (training dashboard) | ~1,500 lines TypeScript | Existing view system |
| **Total new** | **~6,800 lines** | **~50,000+ lines reused** |

---

## Implementation Plan

### Phase 1: Schema & Storage (Week 1-2)

**What:** Extend Engram's skill schema with FORGE-specific fields.

**Work:**
- [ ] Add migration in `schema.rs`: new columns on `skills` table (`provenance`, `certification_status`, `domain`, `skill_tree_path`, `test_history`, `dependencies`, `curriculum_source`, `certified_at`, `expires_at`)
- [ ] Update `store.rs` CRUD operations to handle new fields
- [ ] Update `bridge.rs` with new FORGE-related Engram bridge functions
- [ ] Add `ForgeSkillRecord` type definitions
- [ ] Unit tests for schema migration and CRUD

**Risk:** Low. This is additive — existing functionality is untouched.

**Validates:** We can store and retrieve FORGE-specific skill data through Engram.

---

### Phase 2: Curriculum Engine (Week 3-5)

**What:** Build the ingestion pipeline that takes raw domain sources and produces a skill tree.

**Work:**
- [ ] New module: `src-tauri/src/engine/forge/mod.rs` (module root)
- [ ] New module: `src-tauri/src/engine/forge/curriculum.rs`
  - Source ingestion (URLs via `fetch.rs`, local docs via `filesystem.rs`)
  - LLM-powered skill decomposition (call via provider abstraction)
  - Dependency detection and DAG construction
  - Skill tree storage in Engram
- [ ] New module: `src-tauri/src/engine/forge/types.rs`
  - `SkillTree`, `SkillNode`, `CurriculumSource`, `Dependency` types
- [ ] Lineage tracking: source → derived concepts → atomic skills
- [ ] Tauri IPC commands: `forge_ingest_source`, `forge_get_skill_tree`
- [ ] Tests: verify skill tree generation, dependency ordering, lineage queries

**Risk:** Medium. Skill decomposition quality depends on the LLM. Need human review checkpoint.

**Validates:** We can go from "here's the HubSpot Academy URL" to a structured skill tree.

---

### Phase 3: Test Engine (Week 5-7)

**What:** Build the test generation and execution system.

**Work:**
- [ ] New module: `src-tauri/src/engine/forge/testing.rs`
  - Test scenario generation (L0–L3 tiers)
  - Test execution harness (spawn Specialist as sub-agent, capture response)
  - Result evaluation (deterministic check for L0, LLM-as-judge for L2-L3)
  - Confidence score computation
- [ ] New module: `src-tauri/src/engine/forge/scoring.rs`
  - Multi-signal confidence computation
  - Consistency analysis across test variations
  - Dependency health aggregation
- [ ] Integration with container sandbox for L0 API tests
- [ ] Tauri IPC commands: `forge_run_test`, `forge_get_test_results`
- [ ] Tests: verify scoring, test execution, failure analysis

**Risk:** Medium-High. L0 (deterministic) tests are reliable. L2-L3 (LLM-judged) need careful rubric design to avoid circular validation.

**Validates:** We can test a Specialist, score the result, and identify specific failure patterns.

---

### Phase 4: Craftsman Agent & Training Loop (Week 7-9)

**What:** Wire it all together — the Craftsman agent that orchestrates the full train→test→certify loop.

**Work:**
- [ ] Craftsman agent profile (system prompt, model config, tool access)
- [ ] New module: `src-tauri/src/engine/forge/craftsman.rs`
  - Orchestration logic: teach → test → evaluate → certify/reinforce
  - Failure analysis and reinforcement generation
  - Training session management (save/resume progress)
- [ ] Register FORGE tools in tool executor (`tools/forge.rs`)
  - `forge_ingest_curriculum` — trigger curriculum pipeline
  - `forge_run_test` — execute test scenario
  - `forge_certify_skill` — write certified skill to Engram
  - `forge_analyze_failure` — root-cause analysis
  - `forge_check_domain_updates` — poll sources for changes
- [ ] Human-in-the-loop approval gates (skill tree review, test spot-checks)
- [ ] Cost accounting: token budget per skill, per training session
- [ ] Integration tests: full pipeline from source → certified skill

**Risk:** Medium. Main risk is the Craftsman infinite-looping on hard-to-certify skills. MAX_ATTEMPTS and token budgets mitigate.

**Validates:** End-to-end: raw docs in → certified specialist out.

---

### Phase 5: Specialist Production Behavior (Week 9-10)

**What:** Make Forged Specialists behave differently in production — query certified knowledge, flag uncertainty, report failures.

**Work:**
- [ ] Specialist agent profile template (system prompt enforcing FORGE behavioral contract)
- [ ] Integration with `meta_cognition.rs` — enrich confidence maps with FORGE certification data
- [ ] Failure reporting pipeline: production failures → Craftsman notification
- [ ] Tauri IPC commands: `forge_specialist_status`, `forge_report_failure`
- [ ] Namespace isolation: Specialist queries only its domain's certified skills

**Risk:** Low-Medium. Behavioral enforcement is prompt-level, which has some leakage risk. Mitigated by meta-cognition integration providing hard data on what's certified vs. not.

**Validates:** Specialist says "I know this" for certified topics and "I'm not sure about this" for uncertified ones.

---

### Phase 6: Continuous Evolution (Week 10-12)

**What:** Background processes for skill decay, domain change detection, and production failure re-training.

**Work:**
- [ ] New module: `src-tauri/src/engine/forge/evolution.rs`
  - Periodic re-certification check (modeled on Engram consolidation engine)
  - Curriculum source change detection (content hash comparison)
  - Skill decay application (confidence degradation over time)
  - Re-training queue management
- [ ] Wire production failure reports into re-training pipeline
- [ ] Alerting: notify user when skills are expiring or failing
- [ ] Tests: verify decay, re-certification triggers, source change detection

**Risk:** Low. Follows established patterns from Engram's consolidation engine.

**Validates:** System stays current without manual intervention.

---

### Phase 7: Frontend Dashboard (Week 12-14)

**What:** UI for managing FORGE — viewing skill trees, training progress, certification status, and triggering training runs.

**Work:**
- [ ] New view: `src/views/forge.ts`
  - Skill tree visualization (expandable tree with certification status badges)
  - Training progress (current skill, attempts, pass/fail history)
  - Confidence dashboard (domain-level and skill-level confidence scores)
  - Curriculum source management (add/remove/check sources)
  - Training controls (start training, pause, resume, force re-certification)
- [ ] Tauri IPC commands for frontend data fetching
- [ ] Integration with sidebar navigation

**Risk:** Low. Standard UI work using existing patterns.

**Validates:** Team and end-users can see and manage FORGE without touching code.

---

### Total Timeline Estimate

| Phase | Duration | Dependencies |
|-------|----------|-------------|
| 1. Schema & Storage | 2 weeks | None |
| 2. Curriculum Engine | 3 weeks | Phase 1 |
| 3. Test Engine | 2-3 weeks | Phase 1 |
| 4. Craftsman & Training Loop | 2-3 weeks | Phases 2 + 3 |
| 5. Specialist Behavior | 1-2 weeks | Phase 4 |
| 6. Continuous Evolution | 2 weeks | Phase 4 |
| 7. Frontend Dashboard | 2 weeks | Phase 4 |

Phases 2 and 3 can run in parallel. Phases 5, 6, and 7 can run in parallel after Phase 4.

**Critical path:** Phase 1 → Phase 2+3 (parallel) → Phase 4 → Phase 5+6+7 (parallel)

**Minimum viable FORGE (demo-ready):** Phases 1–4 ≈ 8–10 weeks.

---

## Design Decisions

### Decision 1: Extend Engram vs. Build Separate Storage

**Decision: Extend Engram.**

Rationale: Engram already has hybrid search, graph edges, decay, meta-cognition, encryption, and memory bus sync. Building separate storage would duplicate all of this and create conflicting skill records. By adding fields to the existing schema, FORGE-certified skills automatically benefit from the entire Engram stack.

### Decision 2: Craftsman as Agent vs. Craftsman as Service

**Decision: Craftsman as Agent (using Orchestrator).**

Rationale: The Craftsman is an LLM-powered orchestrator — exactly what our existing boss/worker model supports. Making it a separate service would require new infrastructure, deployment, and monitoring. As an agent, it uses the existing tool executor, provider abstraction, and session management.

### Decision 3: Cold Start Strategy

**Decision: Bootstrap + Verify.**

The Specialist starts with the base LLM's existing knowledge. The Craftsman runs a verification pass — not full training — testing what the LLM already knows. Skills that pass are certified. Skills that fail enter the full training loop. This avoids both extremes: an empty (useless) Specialist and a redundant full training run on knowledge the LLM already has.

The key insight: the base LLM might "know" HubSpot workflows, but until that knowledge is verified through FORGE tests, it's uncertified. The certification — the proof — is what competitors can't copy.

### Decision 4: Skill Decay

**Decision: Yes, certifications expire.**

Confidence decays via Engram's existing Ebbinghaus curves. Decay rate (λ) is domain-specific:
- API domains (HubSpot, Stripe): high λ — APIs change frequently
- Stable domains (math, logic): near-zero λ — fundamentals don't change
- Regulatory domains (compliance, tax): medium λ — laws change annually

When confidence drops below re-certification threshold, the skill enters a re-test queue.

### Decision 5: Curriculum Discovery

**Decision: Manual-first, autonomous-later.**

V1: Point the Craftsman at specific sources (URLs, documents). This is a list of inputs, nothing autonomous.

V2: Craftsman uses existing web fetch tools to crawl linked resources, discover changelogs, and monitor API version endpoints. But only after the manual pipeline is proven.

---

## Risk Assessment

### Technical Risks

| Risk | Severity | Likelihood | Mitigation |
|------|----------|------------|------------|
| **Circular validation** — Craftsman (LLM) can't reliably judge Specialist (LLM) | High | High for non-API domains | Ground truth anchors: L0 deterministic tests for API domains. LLM-as-judge only as supplement. Human spot-checks. |
| **Infinite training loops** — Hard-to-certify skills burn unlimited tokens | Medium | Medium | MAX_ATTEMPTS per skill. Token budget per training session. Escalate to human after limit. |
| **Skill decomposition quality** — LLM decomposes domain poorly | Medium | Medium | Human review gate on skill tree before training begins. Iterative refinement. |
| **Test isolation failure** — Specialist answers from parametric knowledge, not Engram | High | Medium | Prompt-level enforcement ("Answer ONLY from verified knowledge"). Meta-cognition integration flags when answer isn't backed by certified memory. |
| **Engram schema migration** — Breaking existing skill records | Low | Low | Additive-only migration. New columns with defaults. Existing records unaffected. |

### Strategic Risks

| Risk | Severity | Likelihood | Mitigation |
|------|----------|------------|------------|
| **Over-engineering** — Building too much before validating the concept | Medium | Medium | HubSpot-first. Prove the loop works on one bounded domain before generalizing. |
| **Market timing** — Competitors release similar systems | Low | Low | No one is doing this currently. Even if they start, the earned knowledge (thousands of verified cycles in Engram) is the moat, not the code. |
| **Cost** — Training is expensive (LLM token usage) | Medium | High | Cost accounting built into Phase 4. Budget limits per skill and per domain. Show ROI: training cost vs. cost of wrong answers in production. |

---

## First Target: HubSpot Specialist

### Why HubSpot

| Factor | Rating | Reason |
|--------|--------|--------|
| **Bounded domain** | ★★★★★ | Clear boundaries — HubSpot is one product with defined features |
| **Existing curriculum** | ★★★★★ | HubSpot Academy offers free, structured courses |
| **Testable via API** | ★★★★★ | Developer test portals available. Workflows either work or they don't. |
| **Market demand** | ★★★★☆ | Large SMB market uses HubSpot. CRM/marketing automation help is in demand. |
| **Skill tree clarity** | ★★★★☆ | HubSpot modules (contacts, deals, workflows, reports) map cleanly to skill tree |

### Scope for V1

| Module | Skills | Test Tier |
|--------|--------|-----------|
| Contacts (properties, lists, lifecycle) | ~15 atomic skills | L0 (API) + L1 (template) |
| Deals (pipelines, stages, properties) | ~10 atomic skills | L0 (API) + L1 (template) |
| Workflows (triggers, actions, branching) | ~20 atomic skills | L0 (API) + L2 (LLM-judge) |
| Forms (creation, submission, integration) | ~8 atomic skills | L0 (API) |
| Reporting (dashboards, custom reports) | ~10 atomic skills | L1 (template) + L2 (LLM-judge) |
| **Total** | **~63 atomic skills** | |

### Success Criteria for HubSpot V1

- [ ] Full skill tree generated from HubSpot Academy + API docs
- [ ] ≥90% of API-testable skills certified via L0 deterministic tests
- [ ] Specialist correctly refuses to answer on topics outside its certified skill tree
- [ ] Specialist correctly identifies and flags low-confidence areas
- [ ] At least one production failure triggers successful re-training cycle
- [ ] Training cost per skill is tracked and within budget

---

## Success Metrics

### Training Quality Metrics

| Metric | Target | How measured |
|--------|--------|-------------|
| Certification rate | ≥85% of skills certified within MAX_ATTEMPTS | Pass/fail ratio across all training runs |
| Average confidence | ≥0.80 across certified skills | Mean confidence score |
| Test reliability | ≥95% agreement between L0 tests and human evaluation | Human audit of random sample |
| Training efficiency | ≤5 attempts average per skill | Mean attempts to certification |

### Production Quality Metrics

| Metric | Target | How measured |
|--------|--------|-------------|
| Accuracy on certified topics | ≥90% correct responses | Human evaluation + automated checks |
| Appropriate uncertainty | ≥95% of uncertified queries flagged | Track "I don't know" rate on out-of-domain queries |
| Failure detection rate | ≥80% of failures caught | Compare detected failures vs. user-reported issues |
| Re-training success | ≥90% of failed skills re-certified after re-training | Track re-certification outcomes |

### Business Metrics

| Metric | Target | How measured |
|--------|--------|-------------|
| Time to first specialist | ≤10 weeks from start | Calendar |
| Cost per certified skill | Track and optimize | Token usage per skill |
| User satisfaction delta | Measurable improvement vs. prompt-only specialist | A/B test or before/after comparison |

---

## Competitive Landscape

| Platform | Approach to Expertise | FORGE Advantage |
|----------|----------------------|-----------------|
| **OpenAI Assistants** | File upload + retrieval | No verification, no skill boundaries, no evolution |
| **AutoGPT / AgentGPT** | Static system prompt | No training loop, no failure learning |
| **CrewAI** | Role-based prompts | Roles are labels, not earned competencies |
| **LangGraph** | Graph-based workflows | Workflow != knowledge. No skill verification. |
| **Custom RAG solutions** | Retrieval over docs | Retrieval != comprehension. No testing, no gap detection. |
| **Fine-tuning** | Weight updates on training data | Expensive, slow, opaque, no granular skill tracking |
| **THE FORGE** | **Structured training → verified certification → continuous evolution** | **Granular skill tracking, confidence-aware responses, self-healing knowledge, exportable proof of competence** |

**Key differentiator:** Every competitor's approach produces an agent that *claims* expertise. FORGE produces an agent that can *prove* expertise — with test history, confidence scores, and failure records. And it keeps proving it as the domain evolves.

---

## Open Questions for Team Discussion

### 1. Scope of V1
Should we build the full continuous evolution loop (Phase 6) for V1, or stop at manual-triggered training and add evolution in V2?

**Recommendation:** Stop at Phase 5 for V1. Prove the core training loop works before automating evolution.

### 2. Human-in-the-Loop Granularity
How much human oversight do we want in V1?

**Options:**
- A) Full autopilot — Craftsman runs unsupervised
- B) Approval gates — Human approves skill tree, spot-checks tests, reviews certifications
- C) Human designs curriculum, Craftsman only executes tests

**Recommendation:** Option B. Enough automation to be useful, enough oversight to catch problems.

### 3. Multi-Specialist Priority
After HubSpot, what's the second domain?

**Candidates:**
- Stripe (API-testable, high demand)
- Salesforce (large market, complex domain)
- AWS (massive scope, strong demand)
- Internal platform expertise (our own OpenPaws platform)

### 4. Pricing / Monetization
FORGE-trained specialists are significantly more valuable than prompt-only agents. How do we capture that value?

**Options:**
- Premium tier feature (FORGE training available on paid plans)
- Marketplace model (pre-forged specialists available for purchase)
- Training-as-a-service (users pay for training compute)

### 5. Open Source Scope
THE FORGE training infrastructure is MIT-licensed with the rest of OpenPaws. But should pre-trained FORGE knowledge (the Engram records) be open-sourced?

**Consideration:** The thesis is that earned knowledge is the moat. Open-sourcing the training system is fine (anyone can build their own forge). Open-sourcing the trained output is giving away the moat.

---

## Appendix: Why This Is Hard to Replicate

Even if a competitor reads this document and builds the same pipeline:

1. **Training cycles are expensive.** Each certified specialist represents hundreds of LLM calls, test executions, and failure analyses. You can't shortcut this.

2. **Knowledge compounds.** A specialist with 6 months of production failures fed back through re-training has knowledge that a freshly-trained specialist doesn't. Time is a factor.

3. **Domain expertise is specific.** Our HubSpot specialist's failure patterns ("users commonly confuse enrollment vs. re-enrollment triggers") come from real production interactions. Generic training can't produce this.

4. **Engram integration is deep.** FORGE doesn't just store a JSON file of skills. It stores them in a living memory graph with decay, consolidation, meta-cognition, and multi-agent sync. Replicating this requires replicating Engram.

5. **The training infrastructure is the smaller part.** The training loop is ~6,800 lines of new code. The memory system it depends on is 50,000+ lines of mature, tested infrastructure. FORGE is the last mile on top of years of memory architecture work.

---

*This document is intended for internal team review. It covers the vision, architecture, implementation plan, risks, and open questions for THE FORGE system. Feedback and discussion welcome.*
