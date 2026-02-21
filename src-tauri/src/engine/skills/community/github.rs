use log::info;

use crate::engine::sessions::SessionStore;
use super::types::{CommunitySkill, DiscoveredSkill};
use super::parser::parse_skill_md;
use crate::atoms::error::EngineResult;

/// Fetch the list of skills available in a GitHub repo.
/// Uses the GitHub API to list files in skills/ directories.
pub async fn fetch_repo_skills(source: &str) -> EngineResult<Vec<DiscoveredSkill>> {
    let (owner, repo) = parse_github_source(source)?;
    let client = reqwest::Client::new();

    // Try the GitHub API to get the repo tree (try main, then master)
    let tree_url = format!(
        "https://api.github.com/repos/{}/{}/git/trees/main?recursive=1",
        owner, repo
    );

    let resp = client.get(&tree_url)
        .header("User-Agent", "Pawz/1.0")
        .header("Accept", "application/vnd.github.v3+json")
        .send().await?;

    let tree: serde_json::Value = if resp.status().is_success() {
        resp.json().await?
    } else {
        // Fallback to master branch
        let tree_url_master = format!(
            "https://api.github.com/repos/{}/{}/git/trees/master?recursive=1",
            owner, repo
        );
        let resp2 = client.get(&tree_url_master)
            .header("User-Agent", "Pawz/1.0")
            .header("Accept", "application/vnd.github.v3+json")
            .send().await?;

        if !resp2.status().is_success() {
            return Err(format!("GitHub API returned {} (tried both main and master branches)", resp2.status()).into());
        }
        resp2.json().await?
    };

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
        return Err(format!("No SKILL.md files found in {}/{}", owner, repo).into());
    }

    // Fetch each SKILL.md and parse it (try main, then master)
    let mut skills = Vec::new();
    for path in &skill_paths {
        let mut content = String::new();
        for branch in &["main", "master"] {
            let raw_url = format!(
                "https://raw.githubusercontent.com/{}/{}/{}/{}",
                owner, repo, branch, path
            );

            if let Ok(r) = client.get(&raw_url)
                .header("User-Agent", "Pawz/1.0")
                .send().await
            {
                if r.status().is_success() {
                    content = r.text().await.unwrap_or_default();
                    break;
                }
            }
        }

        if content.is_empty() {
            continue;
        }

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
/// If `skill_path` is empty, auto-discovers SKILL.md via the tree API.
pub async fn install_community_skill(
    store: &SessionStore,
    source: &str,
    skill_path: &str,
    agent_id: Option<&str>,
) -> EngineResult<CommunitySkill> {
    let (owner, repo) = parse_github_source(source)?;
    let client = reqwest::Client::new();

    // If no explicit path given, auto-discover SKILL.md in the repo
    let resolved_path = if skill_path.is_empty() {
        discover_skill_path(&client, &owner, &repo).await?
    } else if !skill_path.ends_with("SKILL.md") {
        // Agent may pass a directory like "skills/my-skill" — append SKILL.md
        let trimmed = skill_path.trim_end_matches('/');
        format!("{}/SKILL.md", trimmed)
    } else {
        skill_path.to_string()
    };

    let raw_url = format!(
        "https://raw.githubusercontent.com/{}/{}/main/{}",
        owner, repo, resolved_path
    );
    info!("[skills] Fetching SKILL.md from: {}", raw_url);

    let resp = client.get(&raw_url)
        .header("User-Agent", "Pawz/1.0")
        .send().await?;

    if !resp.status().is_success() {
        // Try the 'master' branch as fallback
        let fallback_url = format!(
            "https://raw.githubusercontent.com/{}/{}/master/{}",
            owner, repo, resolved_path
        );
        info!("[skills] main branch failed (HTTP {}), trying master: {}", resp.status(), fallback_url);

        let resp2 = client.get(&fallback_url)
            .header("User-Agent", "Pawz/1.0")
            .send().await?;

        if !resp2.status().is_success() {
            return Err(format!(
                "Failed to download SKILL.md from {}/{}: HTTP {} (tried both main and master branches, path: '{}')",
                owner, repo, resp2.status(), resolved_path
            ).into());
        }

        let content = resp2.text().await?;
        return finish_install(store, &owner, &repo, source, &content, agent_id);
    }

    let content = resp.text().await?;

    finish_install(store, &owner, &repo, source, &content, agent_id)
}

/// Auto-discover the SKILL.md path in a repo by checking common locations,
/// then falling back to the tree API.
async fn discover_skill_path(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
) -> EngineResult<String> {
    // Try common paths first (fast, no API rate limit)
    let candidates = vec![
        "SKILL.md".to_string(),
        format!("skills/{}/SKILL.md", repo),
    ];

    for candidate in &candidates {
        for branch in &["main", "master"] {
            let url = format!(
                "https://raw.githubusercontent.com/{}/{}/{}/{}",
                owner, repo, branch, candidate
            );
            if let Ok(resp) = client.head(&url)
                .header("User-Agent", "Pawz/1.0")
                .send().await
            {
                if resp.status().is_success() {
                    info!("[skills] Auto-discovered SKILL.md at: {}", candidate);
                    return Ok(candidate.clone());
                }
            }
        }
    }

    // Fall back to tree API to find any SKILL.md
    let tree_url = format!(
        "https://api.github.com/repos/{}/{}/git/trees/main?recursive=1",
        owner, repo
    );
    if let Ok(resp) = client.get(&tree_url)
        .header("User-Agent", "Pawz/1.0")
        .header("Accept", "application/vnd.github.v3+json")
        .send().await
    {
        if resp.status().is_success() {
            if let Ok(tree) = resp.json::<serde_json::Value>().await {
                let skill_paths: Vec<String> = tree["tree"].as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|entry| {
                        let path = entry["path"].as_str()?;
                        if path.ends_with("SKILL.md") {
                            Some(path.to_string())
                        } else {
                            None
                        }
                    })
                    .collect();

                if let Some(first) = skill_paths.first() {
                    info!("[skills] Found SKILL.md via tree API: {}", first);
                    return Ok(first.clone());
                }
            }
        }
    }

    Err(format!(
        "No SKILL.md found in {}/{}. The repository may not contain a valid Paw skill. \
         Try specifying the path explicitly (e.g., 'skills/my-skill/SKILL.md').",
        owner, repo
    ).into())
}

/// Finalize skill installation: parse SKILL.md content, save to DB.
fn finish_install(
    store: &SessionStore,
    owner: &str,
    repo: &str,
    source: &str,
    content: &str,
    agent_id: Option<&str>,
) -> EngineResult<CommunitySkill> {
    let (name, description, instructions) = parse_skill_md(content)
        .ok_or("Invalid SKILL.md format — missing name in frontmatter")?;

    let skill_name = name.to_lowercase().replace(' ', "-");
    let id = format!("{}/{}/{}", owner, repo, skill_name);
    let now = chrono::Utc::now().to_rfc3339();

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
fn parse_github_source(source: &str) -> EngineResult<(String, String)> {
    // Handle full URLs: https://github.com/owner/repo
    let cleaned = source
        .trim()
        .trim_end_matches('/')
        .replace("https://github.com/", "")
        .replace("http://github.com/", "");

    let parts: Vec<&str> = cleaned.split('/').collect();
    if parts.len() < 2 {
        return Err(format!("Invalid source '{}' — expected 'owner/repo' format", source).into());
    }

    Ok((parts[0].to_string(), parts[1].to_string()))
}
