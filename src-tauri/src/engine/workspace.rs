// Paw Agent Engine — Per-Agent Workspaces, Screenshot Viewer & Domain Allowlist
//
// Provides:
//   - Isolated filesystem workspaces per agent (~/.paw/agent-workspaces/{id}/)
//   - Path validation to prevent workspace escapes
//   - Screenshot listing and base64 reading
//   - Outbound domain allowlist / denylist enforcement

use crate::engine::types::{AgentWorkspace, DomainPolicy, DomainPolicyMode, ScreenshotInfo};
use log::info;
use std::path::{Path, PathBuf};

// ── Per-Agent Workspaces ───────────────────────────────────────────────

/// Base directory for all agent workspaces: ~/.paw/agent-workspaces/
fn workspaces_base_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".paw")
        .join("agent-workspaces")
}

/// Get or create the workspace directory for an agent.
pub fn ensure_workspace(agent_id: &str) -> Result<PathBuf, String> {
    let safe_id = agent_id.replace(
        |c: char| !c.is_alphanumeric() && c != '-' && c != '_',
        "_",
    );
    let workspace = workspaces_base_dir().join(safe_id);
    std::fs::create_dir_all(&workspace)
        .map_err(|e| format!("Failed to create workspace for agent '{}': {}", agent_id, e))?;
    Ok(workspace)
}

/// Validate that a filesystem path is within the allowed workspace.
/// Returns Ok(canonical_path) if valid, Err if path escapes the workspace.
pub fn validate_path(requested_path: &str, workspace_root: &str) -> Result<PathBuf, String> {
    let path = Path::new(requested_path);
    let workspace = Path::new(workspace_root);

    // If the path is relative, resolve it against the workspace root
    let absolute_path = if path.is_relative() {
        workspace.join(path)
    } else {
        path.to_path_buf()
    };

    // Canonicalize both paths to resolve symlinks, .., etc.
    let canonical = if absolute_path.exists() {
        absolute_path
            .canonicalize()
            .map_err(|e| format!("Cannot resolve path '{}': {}", requested_path, e))?
    } else {
        // For new files, canonicalize the parent dir and append the filename
        let parent = absolute_path.parent().unwrap_or(workspace);
        if !parent.exists() {
            // Create parent dirs within workspace only
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent directories: {}", e))?;
        }
        let parent_canonical = parent
            .canonicalize()
            .map_err(|e| format!("Cannot resolve parent dir: {}", e))?;
        parent_canonical.join(
            absolute_path
                .file_name()
                .unwrap_or_default(),
        )
    };

    let workspace_canonical = workspace
        .canonicalize()
        .map_err(|e| format!("Cannot resolve workspace root '{}': {}", workspace_root, e))?;

    if canonical.starts_with(&workspace_canonical) {
        Ok(canonical)
    } else {
        Err(format!(
            "Path '{}' escapes agent workspace '{}'. Access denied.",
            requested_path, workspace_root
        ))
    }
}

/// List all agent workspaces on disk.
pub fn list_workspaces() -> Vec<AgentWorkspace> {
    let base = workspaces_base_dir();
    let mut workspaces = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&base) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let agent_id = entry.file_name().to_string_lossy().to_string();
                let workspace_path = entry.path().to_string_lossy().to_string();
                let created_at = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.created().ok())
                    .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
                    .unwrap_or_default();
                let size_bytes = dir_size(&entry.path()).ok();

                workspaces.push(AgentWorkspace {
                    agent_id,
                    workspace_path,
                    created_at,
                    size_bytes,
                });
            }
        }
    }

    workspaces
}

/// Delete an agent workspace entirely.
pub fn delete_workspace(agent_id: &str) -> Result<(), String> {
    let safe_id = agent_id.replace(
        |c: char| !c.is_alphanumeric() && c != '-' && c != '_',
        "_",
    );
    let workspace = workspaces_base_dir().join(safe_id);
    if workspace.exists() {
        std::fs::remove_dir_all(&workspace)
            .map_err(|e| format!("Failed to delete workspace: {}", e))?;
    }
    info!("[workspace] Deleted workspace for agent '{}'", agent_id);
    Ok(())
}

/// Calculate directory size recursively.
fn dir_size(path: &Path) -> Result<u64, std::io::Error> {
    let mut total = 0u64;
    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let p = entry.path();
            if p.is_dir() {
                total += dir_size(&p)?;
            } else {
                total += entry.metadata()?.len();
            }
        }
    }
    Ok(total)
}

// ── Screenshot Viewer ──────────────────────────────────────────────────

/// List all screenshots in the temp directory (newest first).
pub fn list_screenshots() -> Vec<ScreenshotInfo> {
    let dir = std::env::temp_dir().join("paw-screenshots");
    let mut screenshots = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "png").unwrap_or(false) {
                let filename = entry.file_name().to_string_lossy().to_string();
                let filepath = path.to_string_lossy().to_string();
                let metadata = entry.metadata().ok();
                let size_bytes = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
                let created_at = metadata
                    .and_then(|m| m.created().ok())
                    .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
                    .unwrap_or_default();

                screenshots.push(ScreenshotInfo {
                    filename,
                    filepath,
                    size_bytes,
                    created_at,
                });
            }
        }
    }

    // Sort by filename descending (most recent first — filenames contain timestamps)
    screenshots.sort_by(|a, b| b.filename.cmp(&a.filename));
    screenshots
}

/// Read a screenshot file as base64-encoded PNG.
/// Security: only allows reading from the paw-screenshots temp directory.
pub fn read_screenshot_base64(filepath: &str) -> Result<String, String> {
    use base64::Engine;

    let path = Path::new(filepath);

    // Only allow reading from the screenshots directory
    let screenshots_dir = std::env::temp_dir().join("paw-screenshots");
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("Cannot resolve screenshot path: {}", e))?;
    let dir_canonical = screenshots_dir
        .canonicalize()
        .map_err(|_| "Screenshots directory does not exist".to_string())?;

    if !canonical.starts_with(&dir_canonical) {
        return Err("Access denied: path is outside screenshots directory".into());
    }

    let data =
        std::fs::read(path).map_err(|e| format!("Failed to read screenshot: {}", e))?;

    Ok(base64::engine::general_purpose::STANDARD.encode(&data))
}

/// Delete a specific screenshot.
pub fn delete_screenshot(filepath: &str) -> Result<(), String> {
    let path = Path::new(filepath);

    // Security: same check as read
    let screenshots_dir = std::env::temp_dir().join("paw-screenshots");
    if let (Ok(canonical), Ok(dir_canonical)) =
        (path.canonicalize(), screenshots_dir.canonicalize())
    {
        if !canonical.starts_with(&dir_canonical) {
            return Err("Access denied: path is outside screenshots directory".into());
        }
    }

    std::fs::remove_file(path).map_err(|e| format!("Failed to delete screenshot: {}", e))?;
    Ok(())
}

// ── Outbound Domain Allowlist ──────────────────────────────────────────

/// Check if a URL is allowed by the agent's domain policy.
/// Returns Ok(()) if allowed, Err(reason) if blocked.
pub fn check_domain(url_str: &str, policy: &DomainPolicy) -> Result<(), String> {
    match policy.mode {
        DomainPolicyMode::AllowAll => Ok(()),
        DomainPolicyMode::Allowlist => {
            let domain = extract_domain(url_str)?;
            if policy.domains.iter().any(|d| domain_matches(&domain, d)) {
                Ok(())
            } else {
                Err(format!(
                    "Domain '{}' is not in the allowlist. Allowed: {:?}",
                    domain, policy.domains
                ))
            }
        }
        DomainPolicyMode::Denylist => {
            let domain = extract_domain(url_str)?;
            if policy.domains.iter().any(|d| domain_matches(&domain, d)) {
                Err(format!("Domain '{}' is blocked by deny policy", domain))
            } else {
                Ok(())
            }
        }
    }
}

/// Extract the host/domain from a URL string.
fn extract_domain(url_str: &str) -> Result<String, String> {
    let parsed = url::Url::parse(url_str)
        .map_err(|e| format!("Invalid URL '{}': {}", url_str, e))?;
    parsed
        .host_str()
        .map(|h| h.to_lowercase())
        .ok_or_else(|| format!("No host in URL '{}'", url_str))
}

/// Match a domain against a pattern. Supports wildcard patterns:
///   *.example.com → matches sub.example.com AND example.com
///   example.com   → exact match only
fn domain_matches(domain: &str, pattern: &str) -> bool {
    let pattern = pattern.to_lowercase();
    let domain = domain.to_lowercase();

    if pattern.starts_with("*.") {
        let suffix = &pattern[2..]; // Remove "*."
        domain == suffix || domain.ends_with(&format!(".{}", suffix))
    } else {
        domain == pattern
    }
}

// ── Domain Policy Persistence (SQLite) ─────────────────────────────────

/// Load domain policy for an agent from the session store.
pub fn load_domain_policy(
    store: &crate::engine::sessions::SessionStore,
    agent_id: &str,
) -> DomainPolicy {
    let key = format!("domain_policy_{}", agent_id);
    match store.get_config(&key) {
        Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_default(),
        _ => DomainPolicy::default(),
    }
}

/// Save domain policy for an agent to the session store.
pub fn save_domain_policy(
    store: &crate::engine::sessions::SessionStore,
    agent_id: &str,
    policy: &DomainPolicy,
) -> Result<(), String> {
    let key = format!("domain_policy_{}", agent_id);
    let json = serde_json::to_string(policy).map_err(|e| format!("Serialize error: {}", e))?;
    store.set_config(&key, &json)
}

// ── Workspace Config Persistence ───────────────────────────────────────

/// Check whether workspace isolation is enabled for an agent.
pub fn is_workspace_enabled(
    store: &crate::engine::sessions::SessionStore,
    agent_id: &str,
) -> bool {
    let key = format!("workspace_enabled_{}", agent_id);
    match store.get_config(&key) {
        Ok(Some(v)) => v == "true",
        _ => false,
    }
}

/// Enable or disable workspace isolation for an agent.
pub fn set_workspace_enabled(
    store: &crate::engine::sessions::SessionStore,
    agent_id: &str,
    enabled: bool,
) -> Result<(), String> {
    let key = format!("workspace_enabled_{}", agent_id);
    store.set_config(&key, if enabled { "true" } else { "false" })
}
