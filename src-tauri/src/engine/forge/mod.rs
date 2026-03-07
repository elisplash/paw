// ── THE FORGE — Structured Agent Training & Certification ──────────────────
//
// THE FORGE extends Engram's procedural memory system with formal training,
// testing, and certification. Instead of building parallel storage, FORGE
// operates directly on procedural_memories with added certification columns.
//
// Design principles:
//   1. Zero new tables — certification metadata lives on procedural_memories
//   2. Reuse Engram infrastructure — TrustScore, memory_edges (PartOf/LearnedFrom),
//      meta_cognition KnowledgeConfidenceMap, Ebbinghaus decay
//   3. Skill trees are memory_edges DAGs, not a separate data model
//   4. The forge module is training LOGIC, not storage
//
// Sub-modules:
//   - certification: certify/expire/query certified procedural memories
//   - skill_tree: build & query skill-tree DAGs via memory_edges

pub mod certification;
pub mod skill_tree;
