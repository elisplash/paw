// n8n_engine/node_provision.rs — Auto-download Node.js for .exe / .dmg users
//
// When neither Docker nor a system Node.js ≥ 18 is available, we download
// a standalone Node.js binary from nodejs.org and extract it to
// `~/.openpawz/node/`.  This gives .exe / .dmg users a zero-setup
// experience — they never have to install Node.js manually.
//
// The binary is **not** added to PATH; we use the full path internally
// when spawning `npx n8n`.

use crate::atoms::error::{EngineError, EngineResult};
use std::path::{Path, PathBuf};

/// The pinned Node.js version we download.  Using the Active LTS release
/// for stability — n8n's native dependencies (better-sqlite3 etc.) ship
/// prebuilt binaries only for LTS lines, and node-gyp fallback would
/// require Python + C++ toolchains that desktop users rarely have.
///
/// **Why LTS over Current (odd-numbered)?**  Current releases (e.g. 25.x)
/// drop out of support quickly and lack guaranteed prebuilt native addons.
/// Node 24 LTS is supported until April 2028.
const NODE_VERSION: &str = "24.14.0";

/// Subdirectory under `~/.openpawz/` where the Node.js tarball is extracted.
const NODE_DIR_NAME: &str = "node";

// ── Public API ─────────────────────────────────────────────────────────

/// Return the path to a usable `node` binary.
///
/// 1. Check system `node` (version ≥ 18) → return `PathBuf::from("node")`
/// 2. Check local `~/.openpawz/node/bin/node` → return full path
/// 3. Neither → `None`
pub fn local_node_binary() -> Option<PathBuf> {
    // Prefer system node if it meets version requirement
    if super::process::is_node_available() {
        return Some(PathBuf::from("node"));
    }
    let node_bin = node_bin_path();
    if node_bin.exists() && validate_local_node(&node_bin) {
        return Some(node_bin);
    }
    None
}

/// Return the directory containing the local `npx` binary (for PATH injection).
///
/// When we use our bundled Node.js, `npx` lives in the same `bin/` dir.
/// We prepend this to `PATH` so `npx n8n@latest` works.
pub fn local_node_bin_dir() -> Option<PathBuf> {
    let dir = node_dir().join(extracted_dir_name()).join(bin_subdir());
    if dir.exists() {
        Some(dir)
    } else {
        None
    }
}

/// Download and extract Node.js if not already present.
/// If an older auto-provisioned version exists, upgrades it in-place.
///
/// Emits status events so the frontend can show progress.
pub async fn ensure_node_available(app_handle: &tauri::AppHandle) -> EngineResult<PathBuf> {
    // Already have a usable node?
    if let Some(bin) = local_node_binary() {
        // If system node → no upgrade needed (we don't touch the user's install)
        if bin == Path::new("node") {
            return Ok(bin);
        }
        // Local auto-provisioned node — check if it's the current pinned version
        if is_current_version(&bin) {
            return Ok(bin);
        }
        // Stale version — fall through to re-download
        log::info!(
            "[n8n] Local Node.js is outdated — upgrading to v{}",
            NODE_VERSION
        );
        cleanup_old_versions();
    }

    log::info!(
        "[n8n] No usable Node.js found — downloading Node.js v{} for {}",
        NODE_VERSION,
        platform_arch()
    );

    super::emit_status(
        app_handle,
        "provisioning",
        &format!("Downloading Node.js v{}…", NODE_VERSION),
    );

    let archive_url = download_url();
    log::info!("[n8n] Downloading Node.js from {}", archive_url);

    // Download the archive
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| EngineError::Other(format!("HTTP client error: {}", e)))?;

    let resp = client
        .get(&archive_url)
        .send()
        .await
        .map_err(|e| EngineError::Other(format!("Failed to download Node.js: {}", e)))?;

    if !resp.status().is_success() {
        return Err(EngineError::Other(format!(
            "Node.js download failed: HTTP {}",
            resp.status()
        )));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| EngineError::Other(format!("Failed to read Node.js archive: {}", e)))?;

    log::info!(
        "[n8n] Downloaded {:.1} MB — extracting…",
        bytes.len() as f64 / 1_048_576.0
    );

    super::emit_status(app_handle, "provisioning", "Extracting Node.js…");

    // Extract
    let dest = node_dir();
    std::fs::create_dir_all(&dest)
        .map_err(|e| EngineError::Other(format!("Failed to create node dir: {}", e)))?;

    extract_archive(&bytes, &dest)?;

    // Verify the binary works
    let node_bin = node_bin_path();
    if !node_bin.exists() {
        return Err(EngineError::Other(format!(
            "Extraction succeeded but node binary not found at {}",
            node_bin.display()
        )));
    }

    // Make binary executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&node_bin, std::fs::Permissions::from_mode(0o755));
        // Also make npx executable
        let npx_bin = node_bin
            .parent()
            .ok_or_else(|| EngineError::Other("node binary has no parent directory".into()))?
            .join("npx");
        let _ = std::fs::set_permissions(&npx_bin, std::fs::Permissions::from_mode(0o755));
    }

    if !validate_local_node(&node_bin) {
        return Err(EngineError::Other(
            "Downloaded Node.js binary failed validation".into(),
        ));
    }

    log::info!(
        "[n8n] Node.js v{} installed to {}",
        NODE_VERSION,
        dest.display()
    );

    super::emit_status(
        app_handle,
        "provisioning",
        "Node.js installed — starting integration engine…",
    );

    Ok(node_bin)
}

// ── Upgrade helpers ────────────────────────────────────────────────────

/// Check whether the local auto-provisioned node matches the current
/// pinned `NODE_VERSION`.
fn is_current_version(node_bin: &Path) -> bool {
    let output = std::process::Command::new(node_bin)
        .arg("--version")
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let ver = String::from_utf8_lossy(&o.stdout);
            let ver = ver.trim().trim_start_matches('v');
            if ver == NODE_VERSION {
                return true;
            }
            log::info!(
                "[n8n] Local Node.js is v{}, pinned is v{}",
                ver,
                NODE_VERSION
            );
            false
        }
        _ => false,
    }
}

/// Remove previously auto-provisioned Node.js directories so the new
/// version can be extracted cleanly.  Only deletes directories whose
/// name matches the known old extracted-dir pattern.
fn cleanup_old_versions() {
    let base = node_dir();
    if !base.exists() {
        return;
    }
    // Remove any node-v* directories that aren't the current version
    let current = extracted_dir_name();
    if let Ok(entries) = std::fs::read_dir(&base) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("node-v") && name != current {
                log::info!("[n8n] Removing old Node.js directory: {}", name);
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
}

// ── Path helpers ───────────────────────────────────────────────────────

/// Root directory: `~/.openpawz/node/`
fn node_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".openpawz")
        .join(NODE_DIR_NAME)
}

/// The directory name inside the tarball/zip, e.g. `node-v22.14.0-darwin-arm64`.
fn extracted_dir_name() -> String {
    if cfg!(target_os = "windows") {
        format!("node-v{}-win-x64", NODE_VERSION)
    } else {
        let (os, arch) = platform_pair();
        format!("node-v{}-{}-{}", NODE_VERSION, os, arch)
    }
}

/// Subdirectory where binaries live inside the extracted Node.js tree.
fn bin_subdir() -> &'static str {
    if cfg!(target_os = "windows") {
        // Windows zip has node.exe at the root of the extracted dir
        ""
    } else {
        "bin"
    }
}

/// Full path to the `node` binary.
fn node_bin_path() -> PathBuf {
    let mut p = node_dir().join(extracted_dir_name()).join(bin_subdir());
    if cfg!(target_os = "windows") {
        p = p.join("node.exe");
    } else {
        p = p.join("node");
    }
    p
}

// ── Platform detection ─────────────────────────────────────────────────

/// Return `(os, arch)` for the nodejs.org download filename.
fn platform_pair() -> (&'static str, &'static str) {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "win"
    };

    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    };

    (os, arch)
}

/// Human-readable label for log messages.
fn platform_arch() -> String {
    let (os, arch) = platform_pair();
    format!("{}-{}", os, arch)
}

/// Build the nodejs.org download URL.
///
/// macOS/Linux: `.tar.gz` (e.g. `node-v22.14.0-darwin-arm64.tar.gz`)
/// Windows:     `.zip`     (e.g. `node-v22.14.0-win-x64.zip`)
fn download_url() -> String {
    let (os, arch) = platform_pair();
    if cfg!(target_os = "windows") {
        format!(
            "https://nodejs.org/dist/v{}/node-v{}-{}-{}.zip",
            NODE_VERSION, NODE_VERSION, os, arch
        )
    } else {
        format!(
            "https://nodejs.org/dist/v{}/node-v{}-{}-{}.tar.gz",
            NODE_VERSION, NODE_VERSION, os, arch
        )
    }
}

// ── Extraction ─────────────────────────────────────────────────────────

/// Extract a `.tar.gz` or `.zip` archive into `dest`.
fn extract_archive(bytes: &[u8], dest: &Path) -> EngineResult<()> {
    if cfg!(target_os = "windows") {
        extract_zip(bytes, dest)
    } else {
        extract_tar_gz(bytes, dest)
    }
}

/// Extract a `.tar.gz` archive (macOS, Linux).
fn extract_tar_gz(bytes: &[u8], dest: &Path) -> EngineResult<()> {
    // Shell out to `tar` which is always available on macOS/Linux.
    let archive_path = dest.join("node-download.tar.gz");
    std::fs::write(&archive_path, bytes)
        .map_err(|e| EngineError::Other(format!("Failed to write archive: {}", e)))?;

    let output = std::process::Command::new("tar")
        .args([
            "xzf",
            &archive_path.to_string_lossy(),
            "-C",
            &dest.to_string_lossy(),
        ])
        .output()
        .map_err(|e| EngineError::Other(format!("tar extraction failed: {}", e)))?;

    // Clean up the archive file
    let _ = std::fs::remove_file(&archive_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(EngineError::Other(format!(
            "tar extraction failed: {}",
            stderr
        )));
    }

    Ok(())
}

/// Extract a `.zip` archive (Windows).
fn extract_zip(bytes: &[u8], dest: &Path) -> EngineResult<()> {
    // Shell out to PowerShell's Expand-Archive on Windows
    let archive_path = dest.join("node-download.zip");
    std::fs::write(&archive_path, bytes)
        .map_err(|e| EngineError::Other(format!("Failed to write archive: {}", e)))?;

    #[cfg(target_os = "windows")]
    {
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Expand-Archive -Force -Path '{}' -DestinationPath '{}'",
                    archive_path.to_string_lossy(),
                    dest.to_string_lossy()
                ),
            ])
            .output()
            .map_err(|e| EngineError::Other(format!("zip extraction failed: {}", e)))?;

        let _ = std::fs::remove_file(&archive_path);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(EngineError::Other(format!(
                "zip extraction failed: {}",
                stderr
            )));
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Fallback: use `unzip` command (shouldn't happen, but safety net)
        let output = std::process::Command::new("unzip")
            .args([
                "-o",
                &archive_path.to_string_lossy(),
                "-d",
                &dest.to_string_lossy(),
            ])
            .output()
            .map_err(|e| EngineError::Other(format!("unzip failed: {}", e)))?;

        let _ = std::fs::remove_file(&archive_path);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(EngineError::Other(format!("unzip failed: {}", stderr)));
        }
    }

    Ok(())
}

// ── Validation ─────────────────────────────────────────────────────────

/// Run `node --version` on the downloaded binary and check it meets the minimum.
fn validate_local_node(node_bin: &Path) -> bool {
    let output = std::process::Command::new(node_bin)
        .arg("--version")
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let ver = String::from_utf8_lossy(&o.stdout);
            let major = super::process::parse_node_major(&ver);
            if major >= super::types::MIN_NODE_MAJOR {
                log::info!("[n8n] Local Node.js validated: {}", ver.trim());
                true
            } else {
                log::warn!(
                    "[n8n] Local Node.js version {} is below minimum {}",
                    ver.trim(),
                    super::types::MIN_NODE_MAJOR
                );
                false
            }
        }
        Ok(o) => {
            log::warn!(
                "[n8n] Local node binary exited with status {} — {:?}",
                o.status,
                String::from_utf8_lossy(&o.stderr)
            );
            false
        }
        Err(e) => {
            log::warn!("[n8n] Failed to validate local node binary: {}", e);
            false
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_url_format() {
        let url = download_url();
        assert!(url.starts_with("https://nodejs.org/dist/v"));
        assert!(url.contains(NODE_VERSION));
        if cfg!(target_os = "windows") {
            assert!(url.ends_with(".zip"));
        } else {
            assert!(url.ends_with(".tar.gz"));
        }
    }

    #[test]
    fn extracted_dir_name_format() {
        let dir = extracted_dir_name();
        assert!(dir.starts_with("node-v"));
        assert!(dir.contains(NODE_VERSION));
    }

    #[test]
    fn node_bin_path_has_correct_suffix() {
        let p = node_bin_path();
        let name = p.file_name().unwrap().to_str().unwrap();
        if cfg!(target_os = "windows") {
            assert_eq!(name, "node.exe");
        } else {
            assert_eq!(name, "node");
        }
    }

    #[test]
    fn platform_pair_is_valid() {
        let (os, arch) = platform_pair();
        assert!(["darwin", "linux", "win"].contains(&os));
        assert!(["x64", "arm64"].contains(&arch));
    }

    #[test]
    fn node_dir_under_openpawz() {
        let dir = node_dir();
        assert!(dir.to_string_lossy().contains(".openpawz"));
        assert!(dir.to_string_lossy().ends_with("node"));
    }

    #[test]
    fn pinned_version_is_lts() {
        // Node.js LTS releases are even-numbered (22, 24, 26…).
        // Current/non-LTS releases are odd (23, 25, 27…).
        let major: u32 = NODE_VERSION.split('.').next().unwrap().parse().unwrap();
        assert!(
            major % 2 == 0,
            "NODE_VERSION should be an even (LTS) release"
        );
        assert!(major >= 24, "NODE_VERSION should be at least 24");
    }
}
