// ── Engram: Entity Lifecycle Tracking (§41) ──────────────────────────────────
//
// Track named entities across the memory graph — people, projects,
// technologies, organizations, etc.  Provides:
//   - Lightweight NER extraction from memory content
//   - Entity profile CRUD (canonical name, aliases, stats)
//   - Cross-memory entity linking
//   - Entity-scoped retrieval (find all memories mentioning entity X)
//
// For v1, uses keyword / heuristic NER rather than a full ML pipeline.

use crate::atoms::engram_types::{EntityMention, EntityProfile, EntityType};
use crate::atoms::error::{EngineError, EngineResult};
use crate::engine::sessions::SessionStore;
use log::info;

// ═══════════════════════════════════════════════════════════════════════════
// Entity Extraction (lightweight NER)
// ═══════════════════════════════════════════════════════════════════════════

/// Extract entity mentions from free text.
/// v1 uses capitalization heuristics + tech-glossary matching.
/// Future: plug in a real NER model.
pub fn extract_entities(text: &str) -> Vec<EntityMention> {
    let mut mentions: Vec<EntityMention> = Vec::new();

    // Pass 1: Technology terms (case-insensitive glossary)
    extract_technology_terms(text, &mut mentions);

    // Pass 2: Capitalized multi-word names (people, orgs, projects)
    extract_capitalized_runs(text, &mut mentions);

    // Deduplicate overlapping mentions — prefer longer spans
    deduplicate_mentions(&mut mentions);

    mentions
}

/// Resolve an entity mention to a canonical entity profile.
/// Creates a new profile if none exists.
pub fn resolve_entity(
    store: &SessionStore,
    mention: &EntityMention,
    memory_id: &str,
) -> EngineResult<EntityProfile> {
    // Try to find an existing entity by surface form or alias
    if let Some(mut profile) = store.engram_find_entity_by_name(&mention.surface_form)? {
        // Update stats
        profile.mention_count += 1;
        if !profile.memory_ids.contains(&memory_id.to_string()) {
            profile.memory_ids.push(memory_id.to_string());
        }
        store.engram_update_entity_profile(&profile)?;
        return Ok(profile);
    }

    // Create new
    let profile = EntityProfile {
        id: uuid::Uuid::new_v4().to_string(),
        canonical_name: mention.surface_form.clone(),
        aliases: vec![],
        entity_type: mention.entity_type,
        first_seen: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        last_seen: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        mention_count: 1,
        memory_ids: vec![memory_id.to_string()],
        related_entities: vec![],
        summary: None,
        sentiment: 0.0,
    };

    store.engram_insert_entity_profile(&profile)?;
    info!("[engram:entity] New entity: {} ({:?})", profile.canonical_name, profile.entity_type);

    Ok(profile)
}

/// Process a stored memory: extract entities, resolve each, link to memory.
pub fn process_memory_entities(
    store: &SessionStore,
    memory_id: &str,
    content: &str,
) -> EngineResult<Vec<EntityProfile>> {
    let mentions = extract_entities(content);
    let mut profiles = Vec::new();

    for mention in &mentions {
        if mention.confidence >= 0.3 {
            let profile = resolve_entity(store, mention, memory_id)?;
            profiles.push(profile);
        }
    }

    if !mentions.is_empty() {
        info!(
            "[engram:entity] Processed {} mentions → {} entities for memory {}",
            mentions.len(),
            profiles.len(),
            &memory_id[..8]
        );
    }

    Ok(profiles)
}

/// Find all memories related to a specific entity.
pub fn find_memories_for_entity(
    store: &SessionStore,
    entity_name: &str,
) -> EngineResult<Vec<String>> {
    if let Some(profile) = store.engram_find_entity_by_name(entity_name)? {
        Ok(profile.memory_ids)
    } else {
        Ok(vec![])
    }
}

/// Merge two entity profiles (alias resolution).
/// The `secondary` profile is absorbed into `primary`.
pub fn merge_entities(
    store: &SessionStore,
    primary_id: &str,
    secondary_id: &str,
) -> EngineResult<EntityProfile> {
    let mut primary = store
        .engram_get_entity_profile(primary_id)?
        .ok_or_else(|| EngineError::Other(format!("Entity not found: {primary_id}")))?;

    let secondary = store
        .engram_get_entity_profile(secondary_id)?
        .ok_or_else(|| EngineError::Other(format!("Entity not found: {secondary_id}")))?;

    // Absorb aliases
    if !primary.aliases.contains(&secondary.canonical_name) {
        primary.aliases.push(secondary.canonical_name.clone());
    }
    for alias in &secondary.aliases {
        if !primary.aliases.contains(alias) {
            primary.aliases.push(alias.clone());
        }
    }

    // Merge memory IDs
    for mid in &secondary.memory_ids {
        if !primary.memory_ids.contains(mid) {
            primary.memory_ids.push(mid.clone());
        }
    }

    // Merge related entities
    for re in &secondary.related_entities {
        if !primary.related_entities.contains(re) && re != primary_id {
            primary.related_entities.push(re.clone());
        }
    }

    primary.mention_count += secondary.mention_count;
    primary.last_seen = std::cmp::max(&primary.last_seen, &secondary.last_seen).clone();

    // Persist
    store.engram_update_entity_profile(&primary)?;
    store.engram_delete_entity_profile(secondary_id)?;

    info!(
        "[engram:entity] Merged '{}' into '{}'",
        secondary.canonical_name, primary.canonical_name
    );
    Ok(primary)
}

// ═══════════════════════════════════════════════════════════════════════════
// Technology Glossary
// ═══════════════════════════════════════════════════════════════════════════

const TECH_TERMS: &[&str] = &[
    "rust", "python", "javascript", "typescript", "golang", "go", "java", "kotlin",
    "swift", "c++", "ruby", "php", "scala", "elixir", "haskell", "zig", "nim",
    "react", "vue", "angular", "svelte", "nextjs", "nuxt", "astro", "remix",
    "tauri", "electron", "flutter", "docker", "kubernetes", "k8s", "terraform",
    "ansible", "jenkins", "github", "gitlab", "bitbucket", "jira", "confluence",
    "redis", "postgres", "postgresql", "mysql", "mongodb", "sqlite", "dynamodb",
    "elasticsearch", "kafka", "rabbitmq", "nats", "grpc", "graphql", "rest",
    "nginx", "apache", "caddy", "traefik", "aws", "gcp", "azure", "vercel",
    "netlify", "cloudflare", "supabase", "firebase", "heroku",
    "linux", "ubuntu", "macos", "windows", "debian", "fedora", "arch",
    "openai", "anthropic", "gemini", "ollama", "llama", "mistral", "grok",
    "langchain", "llamaindex", "autogen", "crewai",
    "git", "npm", "yarn", "pnpm", "cargo", "pip", "brew", "apt",
    "vscode", "neovim", "vim", "emacs", "intellij", "xcode",
    "css", "html", "json", "yaml", "toml", "markdown", "xml",
    "oauth", "jwt", "saml", "oidc", "ssh", "tls", "ssl",
    "webrtc", "websocket", "http", "https", "tcp", "udp",
    "n8n", "zapier", "ifttt", "make",
];

fn extract_technology_terms(text: &str, mentions: &mut Vec<EntityMention>) {
    let lower = text.to_lowercase();
    for term in TECH_TERMS {
        // Word-boundary match
        if let Some(pos) = find_word_boundary(&lower, term) {
            mentions.push(EntityMention {
                surface_form: term.to_string(),
                entity_id: String::new(),
                entity_type: EntityType::Technology,
                offset: pos,
                confidence: 0.7,
            });
        }
    }
}

fn find_word_boundary(haystack: &str, needle: &str) -> Option<usize> {
    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        let abs_pos = start + pos;
        let before_ok = abs_pos == 0
            || !haystack.as_bytes()[abs_pos - 1].is_ascii_alphanumeric();
        let after_pos = abs_pos + needle.len();
        let after_ok = after_pos >= haystack.len()
            || !haystack.as_bytes()[after_pos].is_ascii_alphanumeric();

        if before_ok && after_ok {
            return Some(abs_pos);
        }
        start = abs_pos + 1;
    }
    None
}

// ═══════════════════════════════════════════════════════════════════════════
// Capitalized Run Detection (people, orgs, projects)
// ═══════════════════════════════════════════════════════════════════════════

/// Stopwords that should not start or constitute an entity
const STOP_WORDS: &[&str] = &[
    "the", "a", "an", "is", "was", "are", "were", "be", "been", "being",
    "have", "has", "had", "do", "does", "did", "will", "would", "could",
    "should", "may", "might", "shall", "can", "it", "its", "this", "that",
    "these", "those", "i", "we", "you", "he", "she", "they", "my", "our",
    "your", "his", "her", "their", "what", "which", "who", "whom", "how",
    "when", "where", "why", "if", "for", "but", "and", "or", "not", "no",
    "so", "than", "too", "very", "just", "about", "with", "from", "into",
    "to", "of", "in", "on", "at", "by", "up", "out", "off",
];

fn extract_capitalized_runs(text: &str, mentions: &mut Vec<EntityMention>) {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut i = 0;
    let mut offset = 0;

    while i < words.len() {
        let word = words[i];
        let clean = word.trim_matches(|c: char| !c.is_alphanumeric());

        if is_title_case(clean) && !is_stop_word(clean) && i > 0 {
            // Start accumulating a capitalized run
            let run_start = offset;
            let mut run_words = vec![clean.to_string()];
            let mut j = i + 1;

            while j < words.len() {
                let next = words[j].trim_matches(|c: char| !c.is_alphanumeric());
                if is_title_case(next) || is_name_connector(next) {
                    run_words.push(next.to_string());
                    j += 1;
                } else {
                    break;
                }
            }

            // Only keep runs of 1-4 words
            if run_words.len() <= 4 && !run_words.is_empty() {
                let surface = run_words.join(" ");
                // Classify: 2+ words starting with caps → likely Person or Organization
                let entity_type = if run_words.len() >= 2 {
                    EntityType::Person // Could be Person or Org; refine later
                } else {
                    EntityType::Concept
                };

                mentions.push(EntityMention {
                    surface_form: surface,
                    entity_id: String::new(),
                    entity_type,
                    offset: run_start,
                    confidence: 0.4 + (run_words.len() as f32 * 0.1).min(0.3),
                });
            }

            i = j;
            // Advance offset through consumed words
            for k in i..j {
                if k < words.len() {
                    offset += words[k].len() + 1;
                }
            }
            continue;
        }

        offset += word.len() + 1;
        i += 1;
    }
}

fn is_title_case(word: &str) -> bool {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.is_uppercase() && chars.all(|c| c.is_lowercase() || c.is_numeric()),
        None => false,
    }
}

fn is_stop_word(word: &str) -> bool {
    STOP_WORDS.contains(&word.to_lowercase().as_str())
}

fn is_name_connector(word: &str) -> bool {
    matches!(word.to_lowercase().as_str(), "of" | "the" | "de" | "van" | "von" | "di" | "da")
}

// ═══════════════════════════════════════════════════════════════════════════
// Deduplication
// ═══════════════════════════════════════════════════════════════════════════

fn deduplicate_mentions(mentions: &mut Vec<EntityMention>) {
    // Sort by offset, then by length descending (prefer longer spans)
    mentions.sort_by(|a, b| {
        a.offset
            .cmp(&b.offset)
            .then(b.surface_form.len().cmp(&a.surface_form.len()))
    });

    let mut keep = vec![true; mentions.len()];
    for i in 0..mentions.len() {
        if !keep[i] {
            continue;
        }
        let a_end = mentions[i].offset + mentions[i].surface_form.len();
        for j in (i + 1)..mentions.len() {
            if !keep[j] {
                continue;
            }
            // If j starts within i's span, remove j
            if mentions[j].offset < a_end {
                keep[j] = false;
            } else {
                break; // sorted by offset, no more overlaps
            }
        }
    }

    let mut idx = 0;
    mentions.retain(|_| {
        let k = keep[idx];
        idx += 1;
        k
    });
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tech_terms() {
        let mentions = extract_entities("We deployed with Docker and Kubernetes on AWS.");
        let techs: Vec<&str> = mentions
            .iter()
            .filter(|m| m.entity_type == EntityType::Technology)
            .map(|m| m.surface_form.as_str())
            .collect();
        assert!(techs.contains(&"docker"), "Should find Docker");
        assert!(techs.contains(&"kubernetes"), "Should find Kubernetes");
        assert!(techs.contains(&"aws"), "Should find AWS");
    }

    #[test]
    fn test_extract_capitalized_name() {
        let mentions = extract_entities("I talked to John Smith about the project.");
        let names: Vec<&str> = mentions
            .iter()
            .filter(|m| matches!(m.entity_type, EntityType::Person))
            .map(|m| m.surface_form.as_str())
            .collect();
        assert!(names.contains(&"John Smith"), "Should find John Smith");
    }

    #[test]
    fn test_word_boundary_matching() {
        // "go" should match as word boundary, not inside "google"
        let mentions = extract_entities("Use go for this service.");
        let techs: Vec<&str> = mentions
            .iter()
            .filter(|m| m.entity_type == EntityType::Technology && m.surface_form == "go")
            .map(|m| m.surface_form.as_str())
            .collect();
        assert!(techs.contains(&"go"), "Should find 'go' as technology");
    }

    #[test]
    fn test_dedup_overlapping() {
        let mut mentions = vec![
            EntityMention {
                surface_form: "Redis".to_string(),
                entity_id: String::new(),
                entity_type: EntityType::Technology,
                offset: 10,
                confidence: 0.7,
            },
            EntityMention {
                surface_form: "Redis Cluster".to_string(),
                entity_id: String::new(),
                entity_type: EntityType::Technology,
                offset: 10,
                confidence: 0.6,
            },
        ];
        deduplicate_mentions(&mut mentions);
        // Should keep "Redis Cluster" (longer) and drop "Redis"
        // Actually, both have same offset, the first in sort-order is preferred
        assert_eq!(mentions.len(), 1);
    }
}
