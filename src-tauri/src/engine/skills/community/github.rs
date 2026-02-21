use log::info;

use crate::engine::sessions::SessionStore;
use super::types::{CommunitySkill, DiscoveredSkill};
use super::parser::parse_skill_md;

/// Fetch the list of skills available in a GitHub repo.
/// Uses the GitHub API to list files in skills/ directories.
pub async fn fetch_repo_skills(source: &str) -> Result<Vec<DiscoveredSkill>, String> {
    let (owner, repo) = parse_github_source(source)?;
    let client = reqwest::Client::new();

    // Try the GitHub API to get the repo tree
    let tree_url = format!(
        "https://api.github.com/repos/{}/{}/git/trees/main?recursive=1",
        owner, repo
    );

    let resp = client.get(&tree_url)
        .header("User-Agent", "Pawz/1.0")
        .header("Accept", "application/vnd.github.v3+json")
        .send().await
        .map_err(|e| format!("GitHub API error: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("GitHub API returned {}", resp.status()));
    }

    let tree: serde_json::Value = resp.json().await
        .map_err(|e| format!("Failed to parse GitHub response: {}", e))?;

    // Find all SKILL.md files
    let skill_paths: Vec<String> = tree["tree"].as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|entry| {
            let path = entry["path"].as_str()?;
            if path.ends_with("/SKILL.md") || path == "SKILL.md" {
                Some(path.to_string())
            } else {
                None
            }
        })
        .collect();

    if skill_paths.is_empty() {
        return Err(format!("No SKILL.md files found in {}/{}", owner, repo));
    }

    // Fetch each SKILL.md and parse it
    let mut skills = Vec::new();
    for path in &skill_paths {
        let raw_url = format!(
            "https://raw.githubusercontent.com/{}/{}/main/{}",
            owner, repo, path
        );

        let content = match client.get(&raw_url)
            .header("User-Agent", "Pawz/1.0")
            .send().await
        {
            Ok(r) if r.status().is_success() => {
                r.text().await.unwrap_or_default()
            }
            _ => continue,
        };

        if let Some((name, description, _instructions)) = parse_skill_md(&content) {
            let skill_name = name.to_lowercase().replace(' ', "-");
            let id = format!("{}/{}/{}", owner, repo, skill_name);
            skills.push(DiscoveredSkill {
                id,
                name,
                description,
                source: format!("{}/{}", owner, repo),
                path: path.clone(),
                installed: false,
                installs: 0,
            });
        }
    }

    Ok(skills)
}

/// Install a single community skill from a GitHub repo.
pub async fn install_community_skill(
    store: &SessionStore,
    source: &str,
    skill_path: &str,
    agent_id: Option<&str>,
) -> Result<CommunitySkill, String> {
    let (owner, repo) = parse_github_source(source)?;
    let client = reqwest::Client::new();

    let raw_url = format!(
        "https://raw.githubusercontent.com/{}/{}/main/{}",
        owner, repo, skill_path
    );

    let resp = client.get(&raw_url)
        .header("User-Agent", "Pawz/1.0")
        .send().await
        .map_err(|e| format!("Failed to fetch skill: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Failed to download SKILL.md: HTTP {}", resp.status()));
    }

    let content = resp.text().await
        .map_err(|e| format!("Failed to read SKILL.md: {}", e))?;

    let (name, description, instructions) = parse_skill_md(&content)
        .ok_or("Invalid SKILL.md format — missing name in frontmatter")?;

    let skill_name = name.to_lowercase().replace(' ', "-");
    let id = format!("{}/{}/{}", owner, repo, skill_name);
    let now = chrono::Utc::now().to_rfc3339();

    // If installed by a specific agent, scope the skill to that agent.
    // Empty vec = all agents (when installed from the UI with no agent context).
    let agent_ids = match agent_id {
        Some(aid) => vec![aid.to_string()],
        None => vec![],
    };

    let skill = CommunitySkill {
        id: id.clone(),
        name,
        description,
        instructions,
        source: format!("{}/{}", owner, repo),
        enabled: true,
        agent_ids,
        installed_at: now.clone(),
        updated_at: now,
    };

    store.save_community_skill(&skill)?;
    info!("[skills] Installed community skill: {} from {} (agents: {:?})", id, source, &skill.agent_ids);

    Ok(skill)
}

/// Parse "owner/repo" from a GitHub source string.
fn parse_github_source(source: &str) -> Result<(String, String), String> {
    // Handle full URLs: https://github.com/owner/repo
    let cleaned = source
        .trim()
        .trim_end_matches('/')
        .replace("https://github.com/", "")
        .replace("http://github.com/", "");

    let parts: Vec<&str> = cleaned.split('/').collect();
    if parts.len() < 2 {
        return Err(format!("Invalid source '{}' — expected 'owner/repo' format", source));
    }

    Ok((parts[0].to_string(), parts[1].to_string()))
}
