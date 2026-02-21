use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunitySkill {
    /// Unique ID: "owner/repo/skill-name"
    pub id: String,
    /// Human name from SKILL.md frontmatter
    pub name: String,
    /// Description from SKILL.md frontmatter
    pub description: String,
    /// Full markdown instructions (the SKILL.md body after frontmatter)
    pub instructions: String,
    /// Source: "owner/repo" or full GitHub URL
    pub source: String,
    /// Whether this skill is enabled (injected into agent prompts)
    pub enabled: bool,
    /// JSON array of agent IDs this skill applies to. Empty array [] = all agents.
    pub agent_ids: Vec<String>,
    /// When it was installed
    pub installed_at: String,
    /// When it was last updated
    pub updated_at: String,
}

/// A skill discovered from a GitHub repo (not yet installed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredSkill {
    /// Derived ID: "owner/repo/skill-name"
    pub id: String,
    /// Human name from SKILL.md frontmatter
    pub name: String,
    /// Description from SKILL.md frontmatter
    pub description: String,
    /// Source repo: "owner/repo"
    pub source: String,
    /// Path within the repo (e.g. "skills/my-skill/SKILL.md")
    pub path: String,
    /// Whether this skill is already installed locally
    pub installed: bool,
    /// Install count from skills.sh (0 if unknown)
    pub installs: u64,
}
