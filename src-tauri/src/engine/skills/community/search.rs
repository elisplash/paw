use super::types::DiscoveredSkill;
use crate::atoms::error::EngineResult;

/// Search for skills via the skills.sh directory API.
/// Uses https://skills.sh/api/search?q={query} to find skills across the ecosystem.
pub async fn search_community_skills(query: &str) -> EngineResult<Vec<DiscoveredSkill>> {
    let client = reqwest::Client::new();

    let encoded_query = query.replace(' ', "+");
    let search_url = format!(
        "https://skills.sh/api/search?q={}",
        encoded_query
    );

    let resp = client.get(&search_url)
        .header("User-Agent", "Pawz/1.0")
        .send().await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("skills.sh returned HTTP {}: {}", status, body).into());
    }

    let data: serde_json::Value = resp.json().await?;

    let empty_vec = vec![];
    let items = data["skills"].as_array()
        .unwrap_or(&empty_vec);

    let mut skills = Vec::new();

    for item in items {
        let skill_id = item["skillId"].as_str().unwrap_or_default();
        let name = item["name"].as_str().unwrap_or_default();
        let source = item["source"].as_str().unwrap_or_default();
        let full_id = item["id"].as_str().unwrap_or_default();
        let installs = item["installs"].as_u64().unwrap_or(0);

        if skill_id.is_empty() || source.is_empty() {
            continue;
        }

        // Construct the SKILL.md path: skills/{skillId}/SKILL.md
        let path = format!("skills/{}/SKILL.md", skill_id);

        skills.push(DiscoveredSkill {
            id: full_id.to_string(),
            name: name.to_string(),
            description: String::new(), // skills.sh doesn't return descriptions in search
            source: source.to_string(),
            path,
            installed: false,
            installs,
        });
    }

    Ok(skills)
}
