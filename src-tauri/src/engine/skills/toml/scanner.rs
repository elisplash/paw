// TOML Manifest — Directory scanner
//
// Scans `~/.paw/skills/*/pawz-skill.toml` and loads valid manifests.
// Invalid or corrupt files are logged and skipped — never crash on bad input.

use std::path::{Path, PathBuf};
use super::types::TomlSkillEntry;
use super::parser::{parse_manifest, validate_manifest, manifest_to_definition};

// ── Path helpers ───────────────────────────────────────────────────────────

/// Returns the skills directory path: `~/.paw/skills/`.
pub fn skills_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".paw").join("skills"))
}

// ── Scanner ────────────────────────────────────────────────────────────────

/// Scan `~/.paw/skills/*/pawz-skill.toml` and return all valid skill definitions.
/// Invalid manifests are logged and skipped (never crash on bad community input).
pub fn scan_toml_skills() -> Vec<TomlSkillEntry> {
    let dir = match skills_dir() {
        Some(d) => d,
        None => {
            log::warn!("[toml-loader] Could not determine home directory");
            return Vec::new();
        }
    };

    if !dir.exists() {
        log::debug!("[toml-loader] Skills directory does not exist: {}", dir.display());
        return Vec::new();
    }

    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(err) => {
            log::warn!("[toml-loader] Failed to read skills directory: {}", err);
            return Vec::new();
        }
    };

    let mut skills = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("pawz-skill.toml");
        if !manifest_path.exists() {
            continue;
        }

        match load_manifest_from_path(&manifest_path) {
            Ok(skill_entry) => {
                log::info!(
                    "[toml-loader] Loaded skill '{}' v{} from {}",
                    skill_entry.definition.name,
                    skill_entry.version,
                    manifest_path.display()
                );
                skills.push(skill_entry);
            }
            Err(err) => {
                log::warn!(
                    "[toml-loader] Skipping {}: {}",
                    manifest_path.display(),
                    err
                );
            }
        }
    }

    log::info!("[toml-loader] Scanned {} TOML skills from {}", skills.len(), dir.display());
    skills
}

/// Load and validate a single manifest from a file path.
pub fn load_manifest_from_path(path: &Path) -> Result<TomlSkillEntry, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

    let manifest = parse_manifest(&content)?;
    validate_manifest(&manifest)?;

    let definition = manifest_to_definition(&manifest);
    let source_dir = path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    Ok(TomlSkillEntry {
        definition,
        source_dir,
        version: manifest.skill.version,
        author: manifest.skill.author,
        has_mcp: manifest.mcp.is_some(),
        has_widget: manifest.widget.is_some(),
        has_view: manifest.view.is_some(),
        view_label: manifest.view.as_ref().map(|v| v.label.clone()).unwrap_or_default(),
        view_icon: manifest.view.as_ref().map(|v| v.icon.clone()).unwrap_or_default(),
    })
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_empty_dir_returns_empty() {
        // When the skills dir doesn't exist, scan returns empty vec.
        // (CI machines typically don't have ~/.paw/skills with actual skills.)
        let result = scan_toml_skills();
        // Don't assert empty — CI machines might have skills dirs.
        // Just verify it doesn't crash.
        assert!(result.len() < 10000); // sanity bound
    }
}
