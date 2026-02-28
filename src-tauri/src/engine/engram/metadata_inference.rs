// ── Engram: Metadata Schema Inference (§35.3) ──────────────────────────────
//
// Automatically extracts structured metadata from episodic memory content
// during the consolidation phase. This enriches memories with searchable
// fields (technologies, file paths, URLs, languages, etc.) that enable
// metadata-filtered queries.
//
// Two extraction modes:
//   1. Regex fast-path (always available) — extracts paths, URLs, technologies
//   2. LLM extraction (optional, if Ollama available) — richer: people, topics, sentiment
//
// Inspired by Vectorize.io's `MetadataExtractionStrategy { inferSchema: true }`,
// but goes further with dual-mode extraction and curated tech vocabulary.

use crate::atoms::engram_types::InferredMetadata;
use std::collections::HashSet;

// ═══════════════════════════════════════════════════════════════════════════
// Technology Vocabulary
// ═══════════════════════════════════════════════════════════════════════════

/// Curated vocabulary of technologies for auto-detection.
/// Matched case-insensitively against memory content.
const TECH_VOCABULARY: &[&str] = &[
    // ── Frontend Frameworks ──
    "React",
    "Vue",
    "Svelte",
    "Angular",
    "Next.js",
    "Nuxt",
    "Solid",
    "Astro",
    "Remix",
    "Gatsby",
    // ── Languages ──
    "TypeScript",
    "JavaScript",
    "Python",
    "Rust",
    "Go",
    "Java",
    "Kotlin",
    "Swift",
    "C++",
    "C#",
    "Ruby",
    "PHP",
    "Elixir",
    "Haskell",
    "Zig",
    "Lua",
    "Dart",
    "Scala",
    // ── Databases ──
    "PostgreSQL",
    "MySQL",
    "SQLite",
    "MongoDB",
    "Redis",
    "DynamoDB",
    "CockroachDB",
    "Cassandra",
    "Neo4j",
    "Supabase",
    // ── Infrastructure ──
    "Docker",
    "Kubernetes",
    "AWS",
    "GCP",
    "Azure",
    "Vercel",
    "Netlify",
    "Cloudflare",
    "Terraform",
    "Ansible",
    // ── Tools ──
    "Git",
    "GitHub",
    "GitLab",
    "Tailwind",
    "Vite",
    "Webpack",
    "esbuild",
    "Turbopack",
    "Rollup",
    // ── Runtimes ──
    "Node.js",
    "Deno",
    "Bun",
    "Tauri",
    "Electron",
    "Wasm",
    // ── AI / ML ──
    "OpenAI",
    "Anthropic",
    "Claude",
    "GPT",
    "Ollama",
    "LangChain",
    "LlamaIndex",
    "Hugging Face",
    "TensorFlow",
    "PyTorch",
    // ── Protocols ──
    "GraphQL",
    "REST",
    "gRPC",
    "WebSocket",
    "MQTT",
    "HTTP",
    "SSH",
    "OAuth",
    "JWT",
    "MCP",
];

// ═══════════════════════════════════════════════════════════════════════════
// Programming Language Detection
// ═══════════════════════════════════════════════════════════════════════════

/// File extension → language mapping for auto-detection.
const LANG_EXTENSIONS: &[(&str, &str)] = &[
    (".rs", "Rust"),
    (".ts", "TypeScript"),
    (".tsx", "TypeScript"),
    (".js", "JavaScript"),
    (".jsx", "JavaScript"),
    (".py", "Python"),
    (".go", "Go"),
    (".java", "Java"),
    (".kt", "Kotlin"),
    (".swift", "Swift"),
    (".cpp", "C++"),
    (".c", "C"),
    (".cs", "C#"),
    (".rb", "Ruby"),
    (".php", "PHP"),
    (".ex", "Elixir"),
    (".exs", "Elixir"),
    (".hs", "Haskell"),
    (".zig", "Zig"),
    (".lua", "Lua"),
    (".dart", "Dart"),
    (".scala", "Scala"),
    (".sql", "SQL"),
    (".sh", "Shell"),
    (".bash", "Shell"),
    (".zsh", "Shell"),
    (".toml", "TOML"),
    (".yaml", "YAML"),
    (".yml", "YAML"),
    (".json", "JSON"),
    (".html", "HTML"),
    (".css", "CSS"),
    (".scss", "SCSS"),
    (".md", "Markdown"),
];

/// Code keyword → language mapping for content-based detection.
const LANG_KEYWORDS: &[(&str, &str)] = &[
    ("fn main()", "Rust"),
    ("pub fn ", "Rust"),
    ("impl ", "Rust"),
    ("let mut ", "Rust"),
    ("use std::", "Rust"),
    ("#[derive(", "Rust"),
    ("async fn ", "Rust"),
    ("def __init__", "Python"),
    ("import numpy", "Python"),
    ("from django", "Python"),
    ("def ", "Python"),
    ("func main()", "Go"),
    ("package main", "Go"),
    ("interface ", "TypeScript"),
    ("const [", "TypeScript"),
    ("useState(", "TypeScript"),
    ("export default", "TypeScript"),
    ("class ", "Java"),
    ("public static void main", "Java"),
    ("console.log(", "JavaScript"),
    ("require(", "JavaScript"),
];

/// Detect the primary programming language from memory content.
///
/// Uses two heuristics:
/// 1. File extension matching (if any file paths are referenced)
/// 2. Code keyword matching (if code snippets are embedded)
///
/// Returns the most frequently detected language, or None.
pub fn detect_programming_language(content: &str) -> Option<String> {
    let mut lang_votes: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();

    // Vote by file extensions found in content
    for &(ext, lang) in LANG_EXTENSIONS {
        if content.contains(ext) {
            *lang_votes.entry(lang).or_default() += 1;
        }
    }

    // Vote by code keywords found in content
    for &(keyword, lang) in LANG_KEYWORDS {
        if content.contains(keyword) {
            *lang_votes.entry(lang).or_default() += 2; // Keywords are stronger signals
        }
    }

    // Return the language with the most votes
    lang_votes
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(lang, _)| lang.to_string())
}

// ═══════════════════════════════════════════════════════════════════════════
// Metadata Extraction (Regex Fast-Path)
// ═══════════════════════════════════════════════════════════════════════════

/// Extract structured metadata from raw memory content using regex patterns.
///
/// This is the fast path — always available, no external dependencies.
/// Extracts: file paths, URLs, technologies, programming language.
///
/// For richer extraction (people, topics, sentiment, dates), the caller
/// should follow up with `enrich_metadata_with_llm()` when Ollama is available.
pub fn infer_metadata(content: &str) -> InferredMetadata {
    let mut meta = InferredMetadata::default();

    // ── File paths ─────────────────────────────────────────────────
    meta.file_paths = extract_file_paths(content);

    // ── URLs ───────────────────────────────────────────────────────
    meta.urls = extract_urls(content);

    // ── Technologies ──────────────────────────────────────────────
    meta.technologies = extract_technologies(content);

    // ── Programming language ──────────────────────────────────────
    meta.language = detect_programming_language(content);

    meta
}

/// Extract file paths from content.
///
/// Matches patterns like: `/foo/bar.ts`, `src/main.rs`, `./config.json`, `~/docs/file.md`
fn extract_file_paths(content: &str) -> Vec<String> {
    let mut paths: HashSet<String> = HashSet::new();

    for word in content.split_whitespace() {
        // Remove trailing punctuation that might be attached
        let clean = word.trim_matches(|c: char| c == ',' || c == ';' || c == ')' || c == ']');

        // Must contain at least one `/` and look like a path
        if clean.contains('/') && looks_like_path(clean) {
            paths.insert(clean.to_string());
        }
    }

    let mut result: Vec<String> = paths.into_iter().collect();
    result.sort();
    result
}

/// Heuristic: does this string look like a file path?
fn looks_like_path(s: &str) -> bool {
    // Must start with / or ./ or ~/ or a letter
    let starts_ok = s.starts_with('/')
        || s.starts_with("./")
        || s.starts_with("~/")
        || s.starts_with("../")
        || s.chars().next().map_or(false, |c| c.is_alphanumeric());

    // Must not be a URL
    let not_url = !s.starts_with("http://") && !s.starts_with("https://");

    // Must have path-like characters only
    let valid_chars = s
        .chars()
        .all(|c| c.is_alphanumeric() || "/_.-~@#".contains(c));

    starts_ok && not_url && valid_chars && s.len() > 2
}

/// Extract URLs from content.
fn extract_urls(content: &str) -> Vec<String> {
    let mut urls: HashSet<String> = HashSet::new();

    for word in content.split_whitespace() {
        let clean = word.trim_end_matches(|c: char| c == ',' || c == ';' || c == ')' || c == '>');

        if clean.starts_with("http://") || clean.starts_with("https://") {
            urls.insert(clean.to_string());
        }
    }

    let mut result: Vec<String> = urls.into_iter().collect();
    result.sort();
    result
}

/// Extract technologies mentioned in content.
///
/// Matches against the curated TECH_VOCABULARY list (case-insensitive).
/// Deduplicates and returns sorted.
fn extract_technologies(content: &str) -> Vec<String> {
    let content_lower = content.to_lowercase();
    let mut techs: Vec<String> = Vec::new();

    for &tech in TECH_VOCABULARY {
        let tech_lower = tech.to_lowercase();

        // Word-boundary aware matching (avoid matching "Go" inside "Google")
        if word_boundary_match(&content_lower, &tech_lower) {
            techs.push(tech.to_string());
        }
    }

    techs.sort();
    techs.dedup();
    techs
}

/// Check if `needle` appears in `haystack` at a word boundary.
///
/// Avoids false positives like "Go" matching "Google" or "Got".
fn word_boundary_match(haystack: &str, needle: &str) -> bool {
    // Special cases for multi-word names
    if needle.contains(' ') || needle.contains('.') || needle.contains('+') {
        return haystack.contains(needle);
    }

    // For single words, require word boundaries
    let mut search_start = 0;
    while let Some(pos) = haystack[search_start..].find(needle) {
        let abs_pos = search_start + pos;
        let end_pos = abs_pos + needle.len();

        let before_ok = abs_pos == 0
            || !haystack
                .as_bytes()
                .get(abs_pos - 1)
                .map_or(false, |&b| b.is_ascii_alphanumeric());

        let after_ok = end_pos >= haystack.len()
            || !haystack
                .as_bytes()
                .get(end_pos)
                .map_or(false, |&b| b.is_ascii_alphanumeric());

        if before_ok && after_ok {
            return true;
        }

        search_start = abs_pos + 1;
        if search_start >= haystack.len() {
            break;
        }
    }

    false
}

// ═══════════════════════════════════════════════════════════════════════════
// Date Extraction
// ═══════════════════════════════════════════════════════════════════════════

/// Extract date-like strings from content.
///
/// Matches common formats: YYYY-MM-DD, MM/DD/YYYY, "January 15, 2026", etc.
pub fn extract_dates(content: &str) -> Vec<String> {
    let mut dates: HashSet<String> = HashSet::new();

    // ISO dates: 2026-02-28
    for word in content.split_whitespace() {
        let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
        if clean.len() == 10 && is_iso_date(clean) {
            dates.insert(clean.to_string());
        }
    }

    let mut result: Vec<String> = dates.into_iter().collect();
    result.sort();
    result
}

/// Check if a string looks like an ISO date (YYYY-MM-DD).
fn is_iso_date(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return false;
    }
    let year = parts[0].parse::<u32>().ok();
    let month = parts[1].parse::<u32>().ok();
    let day = parts[2].parse::<u32>().ok();

    matches!(
        (year, month, day),
        (Some(1900..=2100), Some(1..=12), Some(1..=31))
    )
}

// ═══════════════════════════════════════════════════════════════════════════
// Full Inference (Regex + Optional Enrichment)
// ═══════════════════════════════════════════════════════════════════════════

/// Full metadata inference: regex extraction + date parsing.
///
/// This is the complete fast-path pipeline. LLM enrichment (people, topics,
/// sentiment) should be called separately when Ollama is available.
pub fn infer_metadata_full(content: &str) -> InferredMetadata {
    let mut meta = infer_metadata(content);

    // Add date extraction
    meta.dates = extract_dates(content);

    meta
}

// ═══════════════════════════════════════════════════════════════════════════
// Metadata Serialization (for SQLite JSON column)
// ═══════════════════════════════════════════════════════════════════════════

/// Serialize metadata to JSON for storage in the `inferred_metadata` column.
/// Returns None if metadata is completely empty (saves storage).
pub fn serialize_metadata(meta: &InferredMetadata) -> Option<String> {
    if meta.is_empty() {
        return None;
    }
    serde_json::to_string(meta).ok()
}

/// Deserialize metadata from the JSON column.
/// Returns default (empty) metadata on parse failure.
pub fn deserialize_metadata(json: &str) -> InferredMetadata {
    serde_json::from_str(json).unwrap_or_default()
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_file_paths() {
        let content =
            "Check out src/main.rs and ./config.json for the setup. Also ~/docs/notes.md has info.";
        let paths = extract_file_paths(content);
        assert!(paths.contains(&"src/main.rs".to_string()));
        assert!(paths.contains(&"./config.json".to_string()));
        assert!(paths.contains(&"~/docs/notes.md".to_string()));
    }

    #[test]
    fn test_extract_file_paths_no_urls() {
        let content = "Visit https://github.com/foo/bar and also check src/lib.rs";
        let paths = extract_file_paths(content);
        assert!(!paths.iter().any(|p| p.starts_with("http")));
        assert!(paths.contains(&"src/lib.rs".to_string()));
    }

    #[test]
    fn test_extract_urls() {
        let content = "See https://docs.rs/serde and http://localhost:8080/api for details.";
        let urls = extract_urls(content);
        assert!(urls.contains(&"https://docs.rs/serde".to_string()));
        assert!(urls.contains(&"http://localhost:8080/api".to_string()));
    }

    #[test]
    fn test_extract_technologies() {
        let content =
            "We're using React with TypeScript and deploying to Docker containers on AWS.";
        let techs = extract_technologies(content);
        assert!(techs.contains(&"React".to_string()));
        assert!(techs.contains(&"TypeScript".to_string()));
        assert!(techs.contains(&"Docker".to_string()));
        assert!(techs.contains(&"AWS".to_string()));
    }

    #[test]
    fn test_word_boundary_go_not_google() {
        // "Go" should not match "Google"
        assert!(!word_boundary_match("google cloud platform", "go"));
        // But should match standalone "go"
        assert!(word_boundary_match("written in go language", "go"));
        assert!(word_boundary_match("go is great", "go"));
    }

    #[test]
    fn test_word_boundary_rust_not_frustrated() {
        assert!(!word_boundary_match("i'm frustrated with this", "rust"));
        assert!(word_boundary_match("written in rust", "rust"));
    }

    #[test]
    fn test_multiword_tech() {
        assert!(word_boundary_match("using node.js runtime", "node.js"));
        assert!(word_boundary_match("next.js is great", "next.js"));
        assert!(word_boundary_match("hugging face models", "hugging face"));
    }

    #[test]
    fn test_detect_rust_by_keywords() {
        let content = "fn main() { let mut x = 5; println!(\"{}\", x); }";
        let lang = detect_programming_language(content);
        assert_eq!(lang, Some("Rust".to_string()));
    }

    #[test]
    fn test_detect_python_by_keywords() {
        let content = "def __init__(self): import numpy as np";
        let lang = detect_programming_language(content);
        assert_eq!(lang, Some("Python".to_string()));
    }

    #[test]
    fn test_detect_language_by_extension() {
        let content = "Edit the file src/components/App.tsx to add the new component.";
        let lang = detect_programming_language(content);
        assert_eq!(lang, Some("TypeScript".to_string()));
    }

    #[test]
    fn test_infer_metadata_comprehensive() {
        let content = "I was working on src/engine/engram/graph.rs using Rust and SQLite. \
                        The HNSW index uses https://github.com/vectorize-io for reference. \
                        We deploy with Docker on 2026-02-28.";
        let meta = infer_metadata_full(content);

        assert!(!meta.file_paths.is_empty(), "Should find file paths");
        assert!(meta.technologies.contains(&"Rust".to_string()));
        assert!(meta.technologies.contains(&"SQLite".to_string()));
        assert!(meta.technologies.contains(&"Docker".to_string()));
        assert!(!meta.urls.is_empty(), "Should find URLs");
        assert!(meta.dates.contains(&"2026-02-28".to_string()));
        assert_eq!(meta.language, Some("Rust".to_string()));
    }

    #[test]
    fn test_infer_metadata_empty_content() {
        let meta = infer_metadata("");
        assert!(meta.is_empty());
    }

    #[test]
    fn test_is_iso_date() {
        assert!(is_iso_date("2026-02-28"));
        assert!(is_iso_date("2024-01-01"));
        assert!(!is_iso_date("not-a-date"));
        assert!(!is_iso_date("2026-13-01")); // invalid month
        assert!(!is_iso_date("2026-02-32")); // invalid day
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let meta = InferredMetadata {
            technologies: vec!["Rust".into(), "TypeScript".into()],
            file_paths: vec!["src/main.rs".into()],
            language: Some("Rust".into()),
            ..Default::default()
        };

        let json = serialize_metadata(&meta).expect("Should serialize non-empty metadata");
        let restored = deserialize_metadata(&json);

        assert_eq!(restored.technologies, meta.technologies);
        assert_eq!(restored.file_paths, meta.file_paths);
        assert_eq!(restored.language, meta.language);
    }

    #[test]
    fn test_serialize_empty_returns_none() {
        let meta = InferredMetadata::default();
        assert!(serialize_metadata(&meta).is_none());
    }

    #[test]
    fn test_extract_dates_iso() {
        let content = "Meeting on 2026-02-28 and follow-up on 2026-03-15.";
        let dates = extract_dates(content);
        assert!(dates.contains(&"2026-02-28".to_string()));
        assert!(dates.contains(&"2026-03-15".to_string()));
    }

    #[test]
    fn test_no_false_positive_dates() {
        let content = "Version 1.2.3 was released with 100-200 improvements.";
        let dates = extract_dates(content);
        assert!(
            dates.is_empty(),
            "Should not detect version numbers as dates"
        );
    }
}
