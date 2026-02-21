/// Parse a SKILL.md file into (name, description, instructions).
/// Format: YAML frontmatter between --- delimiters, then markdown body.
pub fn parse_skill_md(content: &str) -> Option<(String, String, String)> {
    let trimmed = content.trim();
    if !trimmed.starts_with("---") {
        return None;
    }

    // Find the closing --- delimiter
    let after_first = &trimmed[3..];
    let end_idx = after_first.find("\n---")?;
    let frontmatter = &after_first[..end_idx];
    let body = after_first[end_idx + 4..].trim();

    // Parse YAML frontmatter (just name + description — no serde_yaml needed)
    let mut name = String::new();
    let mut description = String::new();

    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("name:") {
            name = val.trim().trim_matches('"').trim_matches('\'').to_string();
        } else if let Some(val) = line.strip_prefix("description:") {
            description = val.trim().trim_matches('"').trim_matches('\'').to_string();
        }
    }

    if name.is_empty() {
        return None;
    }

    Some((name, description, body.to_string()))
}
