// PawzHub Registry — In-App Skill Browser (Phase F.4)
//
// Fetches the PawzHub registry (registry.json) from GitHub and provides
// search + browse capabilities. Entries are richer than DiscoveredSkill —
// they carry tier, MCP, and widget metadata.

use crate::atoms::error::EngineResult;
use serde::{Deserialize, Serialize};

/// Registry URL: raw JSON from the PawzHub GitHub repo.
const REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/OpenPawz/pawzhub/main/registry.json";

/// A single entry in the PawzHub registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PawzHubEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub category: String,
    pub version: String,
    /// Skill tier: "skill", "integration", "extension", "mcp"
    pub tier: String,
    /// GitHub repo: "owner/repo"
    pub source_repo: String,
    /// Whether the skill bundles an MCP server
    #[serde(default)]
    pub has_mcp: bool,
    /// Whether the skill declares a dashboard widget
    #[serde(default)]
    pub has_widget: bool,
    /// Whether the skill is verified by the PawzHub team
    #[serde(default)]
    pub verified: bool,
    /// Whether this skill is already installed locally (set at query time)
    #[serde(default)]
    pub installed: bool,
}

/// Fetch the full PawzHub registry from GitHub.
pub async fn fetch_pawzhub_registry() -> EngineResult<Vec<PawzHubEntry>> {
    let client = reqwest::Client::new();

    let resp = client
        .get(REGISTRY_URL)
        .header("User-Agent", "Pawz/1.0")
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(
            format!("PawzHub registry returned HTTP {}: {}", status, body).into(),
        );
    }

    let entries: Vec<PawzHubEntry> = resp.json().await.map_err(|e| {
        format!("Failed to parse PawzHub registry: {}", e)
    })?;

    log::info!("[pawzhub] Fetched registry with {} entries", entries.len());
    Ok(entries)
}

/// Search the PawzHub registry by query string.
/// Client-side filtering: matches name, description, category, author, or ID.
pub async fn search_pawzhub(query: &str) -> EngineResult<Vec<PawzHubEntry>> {
    let mut entries = fetch_pawzhub_registry().await?;

    if query.is_empty() {
        return Ok(entries);
    }

    let q = query.to_lowercase();
    entries.retain(|e| {
        e.name.to_lowercase().contains(&q)
            || e.description.to_lowercase().contains(&q)
            || e.category.to_lowercase().contains(&q)
            || e.author.to_lowercase().contains(&q)
            || e.id.to_lowercase().contains(&q)
    });

    Ok(entries)
}

/// Browse the PawzHub registry filtered by category.
pub async fn browse_pawzhub_category(category: &str) -> EngineResult<Vec<PawzHubEntry>> {
    let mut entries = fetch_pawzhub_registry().await?;

    if !category.is_empty() {
        let cat = category.to_lowercase();
        entries.retain(|e| e.category.to_lowercase() == cat);
    }

    Ok(entries)
}

/// Fetch a `pawz-skill.toml` manifest from a PawzHub skill's source repo.
/// Looks at `skills/{skill_id}/pawz-skill.toml` in the source repo.
pub async fn fetch_pawzhub_toml(
    source_repo: &str,
    skill_id: &str,
) -> EngineResult<String> {
    let client = reqwest::Client::new();

    // Try main branch first, then master
    for branch in &["main", "master"] {
        let url = format!(
            "https://raw.githubusercontent.com/{}/{}/skills/{}/pawz-skill.toml",
            source_repo, branch, skill_id
        );

        let resp = client
            .get(&url)
            .header("User-Agent", "Pawz/1.0")
            .send()
            .await?;

        if resp.status().is_success() {
            let toml = resp.text().await?;
            log::info!(
                "[pawzhub] Fetched pawz-skill.toml for '{}' from {} ({})",
                skill_id, source_repo, branch
            );
            return Ok(toml);
        }
    }

    Err(format!(
        "Could not find pawz-skill.toml for '{}' in {}",
        skill_id, source_repo
    ).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pawzhub_entry_deserialize() {
        let json = r#"{
            "id": "weather-dash",
            "name": "Weather Dashboard",
            "description": "Live weather on your dashboard",
            "author": "openpawz",
            "category": "productivity",
            "version": "1.0.0",
            "tier": "skill",
            "source_repo": "OpenPawz/pawzhub",
            "has_mcp": false,
            "has_widget": true,
            "verified": true
        }"#;
        let entry: PawzHubEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.id, "weather-dash");
        assert_eq!(entry.tier, "skill");
        assert!(entry.has_widget);
        assert!(entry.verified);
        assert!(!entry.installed);
    }

    #[test]
    fn pawzhub_entry_defaults() {
        let json = r#"{
            "id": "minimal",
            "name": "Minimal",
            "description": "",
            "author": "",
            "category": "general",
            "version": "0.1.0",
            "tier": "skill",
            "source_repo": "user/repo"
        }"#;
        let entry: PawzHubEntry = serde_json::from_str(json).unwrap();
        assert!(!entry.has_mcp);
        assert!(!entry.has_widget);
        assert!(!entry.verified);
        assert!(!entry.installed);
    }

    #[test]
    fn pawzhub_registry_parse() {
        let json = r#"[
            {
                "id": "a",
                "name": "A",
                "description": "desc-a",
                "author": "auth",
                "category": "cat",
                "version": "1.0.0",
                "tier": "skill",
                "source_repo": "o/r"
            },
            {
                "id": "b",
                "name": "B",
                "description": "desc-b",
                "author": "auth",
                "category": "cat",
                "version": "2.0.0",
                "tier": "mcp",
                "source_repo": "o/r",
                "has_mcp": true
            }
        ]"#;
        let entries: Vec<PawzHubEntry> = serde_json::from_str(json).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "a");
        assert_eq!(entries[1].tier, "mcp");
        assert!(entries[1].has_mcp);
    }
}
