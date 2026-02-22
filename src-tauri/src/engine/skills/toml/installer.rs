// TOML Manifest — Install / Uninstall
//
// Filesystem operations for managing TOML skill directories under
// `~/.paw/skills/{id}/`. Validates content and checks for path traversal.

use std::path::PathBuf;
use super::scanner::skills_dir;
use super::parser::{parse_manifest, validate_manifest};

/// Install a TOML skill by writing a manifest to `~/.paw/skills/{id}/pawz-skill.toml`.
pub fn install_toml_skill(skill_id: &str, toml_content: &str) -> Result<PathBuf, String> {
    // Validate ID format first
    if !skill_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!(
            "Invalid skill ID '{}' — use only a-z, 0-9, hyphens, underscores",
            skill_id
        ));
    }

    // Validate the TOML content parses and is valid
    let manifest = parse_manifest(toml_content)?;
    validate_manifest(&manifest)?;

    if manifest.skill.id != skill_id {
        return Err(format!(
            "Skill ID mismatch: path says '{}' but manifest says '{}'",
            skill_id, manifest.skill.id
        ));
    }

    let dir = skills_dir().ok_or_else(|| "Could not determine home directory".to_string())?;
    let skill_dir = dir.join(skill_id);

    std::fs::create_dir_all(&skill_dir)
        .map_err(|e| format!("Failed to create directory {}: {}", skill_dir.display(), e))?;

    let manifest_path = skill_dir.join("pawz-skill.toml");
    std::fs::write(&manifest_path, toml_content)
        .map_err(|e| format!("Failed to write {}: {}", manifest_path.display(), e))?;

    log::info!(
        "[toml-loader] Installed TOML skill '{}' to {}",
        skill_id,
        manifest_path.display()
    );

    Ok(manifest_path)
}

/// Uninstall a TOML skill by removing its directory from `~/.paw/skills/{id}/`.
pub fn uninstall_toml_skill(skill_id: &str) -> Result<(), String> {
    // Validate safe ID to prevent path traversal
    if !skill_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!("Invalid skill ID: {}", skill_id));
    }

    let dir = skills_dir().ok_or_else(|| "Could not determine home directory".to_string())?;
    let skill_dir = dir.join(skill_id);

    if !skill_dir.exists() {
        return Err(format!("Skill directory does not exist: {}", skill_dir.display()));
    }

    // Extra safety: confirm the directory is inside ~/.paw/skills/
    let canonical = skill_dir
        .canonicalize()
        .map_err(|e| format!("Failed to resolve path: {}", e))?;
    let canonical_base = dir
        .canonicalize()
        .unwrap_or_else(|_| dir.clone());
    if !canonical.starts_with(&canonical_base) {
        return Err("Path traversal detected — aborting".to_string());
    }

    std::fs::remove_dir_all(&skill_dir)
        .map_err(|e| format!("Failed to remove {}: {}", skill_dir.display(), e))?;

    log::info!("[toml-loader] Uninstalled TOML skill '{}' from {}", skill_id, skill_dir.display());
    Ok(())
}
