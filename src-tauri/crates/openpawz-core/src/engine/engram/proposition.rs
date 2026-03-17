// ── Engram: Proposition Decomposition (§6.1) ───────────────────────────────
//
// Breaks compound memories into atomic propositions for more precise
// retrieval and contradiction detection.
//
// Example:
//   "The user works at Acme Corp and prefers dark mode"
//   → ["The user works at Acme Corp", "The user prefers dark mode"]
//
// This is a best-effort heuristic decomposition. For high-quality results,
// an LLM-assisted decomposition can be triggered during consolidation.

use crate::atoms::error::EngineResult;
use serde::{Deserialize, Serialize};

/// An atomic proposition extracted from a compound memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposition {
    /// The atomic statement.
    pub content: String,
    /// Confidence that this decomposition is correct (0.0–1.0).
    pub confidence: f32,
    /// Index of this proposition in the original text (for provenance).
    pub source_offset: usize,
}

/// Decompose a compound sentence into atomic propositions.
///
/// Uses heuristic sentence splitting and conjunction detection.
/// Returns the original as a single proposition if no decomposition is possible.
pub fn decompose(text: &str) -> Vec<Proposition> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return vec![];
    }

    let mut propositions = Vec::new();

    // Step 1: Split into sentences
    let sentences = split_sentences(trimmed);

    for sentence in &sentences {
        let sub_props = split_conjunctions(sentence);
        if sub_props.len() > 1 {
            // Multiple propositions from conjunction splitting
            for (i, prop) in sub_props.iter().enumerate() {
                let cleaned = prop.trim();
                if cleaned.len() >= 5 {
                    // Minimum length for a meaningful proposition
                    propositions.push(Proposition {
                        content: cleaned.to_string(),
                        confidence: 0.85, // heuristic split — good but not perfect
                        source_offset: i,
                    });
                }
            }
        } else {
            // Single proposition
            let cleaned = sentence.trim();
            if cleaned.len() >= 5 {
                propositions.push(Proposition {
                    content: cleaned.to_string(),
                    confidence: 0.95, // whole sentence — high confidence
                    source_offset: 0,
                });
            }
        }
    }

    // Fallback: if decomposition yielded nothing, return original
    if propositions.is_empty() {
        propositions.push(Proposition {
            content: trimmed.to_string(),
            confidence: 1.0,
            source_offset: 0,
        });
    }

    propositions
}

/// Build an LLM prompt for proposition decomposition (§6.1).
/// Callers should send this to an LLM and parse the JSON response.
pub fn build_decomposition_prompt(text: &str) -> String {
    format!(
        r#"Break the following text into atomic propositions. Each proposition should be a simple, self-contained statement that can be independently true or false.

Text: "{}"

Return a JSON array of strings, each being one atomic proposition. Example:
["The user works at Acme Corp", "The user prefers dark mode"]

Respond with ONLY the JSON array, no other text."#,
        text.replace('"', r#"\""#)
    )
}

/// Parse an LLM response to the decomposition prompt.
pub fn parse_decomposition_response(response: &str) -> EngineResult<Vec<Proposition>> {
    // Try to find JSON array in the response
    let trimmed = response.trim();
    let json_str = if trimmed.starts_with('[') {
        trimmed
    } else if let Some(start) = trimmed.find('[') {
        let end = trimmed.rfind(']').unwrap_or(trimmed.len() - 1);
        &trimmed[start..=end]
    } else {
        return Ok(vec![Proposition {
            content: trimmed.to_string(),
            confidence: 0.5,
            source_offset: 0,
        }]);
    };

    let items: Vec<String> =
        serde_json::from_str(json_str).unwrap_or_else(|_| vec![trimmed.to_string()]);

    Ok(items
        .into_iter()
        .enumerate()
        .filter(|(_, s)| s.len() >= 5)
        .map(|(i, s)| Proposition {
            content: s,
            confidence: 0.90, // LLM-assisted — high quality
            source_offset: i,
        })
        .collect())
}

// ── Internal: Sentence Splitting ─────────────────────────────────────────

/// Split text into sentences at `. `, `! `, `? ` boundaries.
fn split_sentences(text: &str) -> Vec<&str> {
    let mut sentences = Vec::new();
    let mut start = 0;

    for (i, c) in text.char_indices() {
        if (c == '.' || c == '!' || c == '?') && i + 1 < text.len() {
            let next = text.as_bytes().get(i + 1).copied();
            if next == Some(b' ') || next == Some(b'\n') {
                let end = i + 1;
                let sentence = text[start..end].trim();
                if !sentence.is_empty() {
                    sentences.push(sentence);
                }
                start = end;
            }
        }
    }

    // Last sentence (may not end with punctuation)
    let remaining = text[start..].trim();
    if !remaining.is_empty() {
        sentences.push(remaining);
    }

    sentences
}

/// Split a sentence at conjunctions: " and ", " but ", " while ", " also "
fn split_conjunctions(text: &str) -> Vec<String> {
    let conjunctions = [" and ", " but ", " while ", " also ", " however "];
    let mut parts = vec![text.to_string()];

    for conj in &conjunctions {
        let mut new_parts = Vec::new();
        for part in &parts {
            let lower = part.to_lowercase();
            if let Some(idx) = lower.find(conj) {
                let left = part[..idx].trim();
                let right = part[idx + conj.len()..].trim();
                if left.len() >= 5 && right.len() >= 5 {
                    new_parts.push(left.to_string());
                    new_parts.push(right.to_string());
                } else {
                    new_parts.push(part.clone());
                }
            } else {
                new_parts.push(part.clone());
            }
        }
        parts = new_parts;
    }

    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decompose_simple() {
        let props = decompose("The user prefers dark mode");
        assert_eq!(props.len(), 1);
        assert_eq!(props[0].content, "The user prefers dark mode");
        assert!(props[0].confidence >= 0.95);
    }

    #[test]
    fn test_decompose_conjunction() {
        let props = decompose("The user works at Acme Corp and prefers dark mode");
        assert!(props.len() >= 2);
    }

    #[test]
    fn test_decompose_multiple_sentences() {
        let props =
            decompose("User likes Python. They also use Rust. TypeScript is their third choice.");
        assert!(props.len() >= 3);
    }

    #[test]
    fn test_decompose_empty() {
        let props = decompose("");
        assert!(props.is_empty());
    }

    #[test]
    fn test_parse_llm_response() {
        let response = r#"["The user likes Python", "The user uses Rust"]"#;
        let props = parse_decomposition_response(response).unwrap();
        assert_eq!(props.len(), 2);
        assert_eq!(props[0].content, "The user likes Python");
    }
}
