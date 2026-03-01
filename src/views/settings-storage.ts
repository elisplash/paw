// Settings: Storage — Data paths, workspace location, cloud sync guidance
// All data goes through Tauri IPC. No gateway.

import { pawEngine } from '../engine';
import { $, confirmModal } from '../components/helpers';
import { getWorkspacePath, setWorkspacePath } from '../workspace';

// ── Helpers ───────────────────────────────────────────────────────────

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB'];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / Math.pow(1024, i);
  return `${value.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

// ── Load ──────────────────────────────────────────────────────────────

export async function loadStorageSettings() {
  const container = $('settings-storage-content');
  if (!container) return;
  container.innerHTML = '<p style="color:var(--text-muted)">Loading storage paths…</p>';

  try {
    const paths = await pawEngine.storageGetPaths();
    const userWorkspace = await getWorkspacePath();

    container.innerHTML = '';

    // ── Engine Data Root ──────────────────────────────────────────────
    const rootSection = document.createElement('div');
    rootSection.className = 'settings-section';
    rootSection.innerHTML = `
      <h2 class="settings-section-title">Engine Data Root</h2>
      <p class="settings-section-desc">
        Where Paw stores its database, agent workspaces, skills, and browser profiles.
        Changing this requires a restart.
      </p>
      <div class="form-group" style="margin-bottom: 12px">
        <label class="form-label">Current location</label>
        <div style="display: flex; align-items: center; gap: 8px">
          <input type="text" class="form-input" id="storage-data-root"
                 value="${escapeHtml(paths.data_root)}"
                 style="flex: 1; font-family: var(--font-mono, monospace); font-size: 12px" />
          <button class="btn btn-sm btn-primary" id="storage-data-root-save">
            <span class="ms ms-sm" style="margin-right: 2px">save</span> Save
          </button>
          ${
            paths.is_custom
              ? `
          <button class="btn btn-sm btn-ghost" id="storage-data-root-reset" title="Reset to default">
            <span class="ms ms-sm">restart_alt</span>
          </button>`
              : ''
          }
        </div>
        <p style="color: var(--text-muted); font-size: 11px; margin-top: 4px">
          Default: <code style="font-size: 11px">${escapeHtml(paths.default_root)}</code>
          ${paths.is_custom ? ' — <span style="color: var(--accent)">custom path active</span>' : ''}
        </p>
      </div>
    `;
    container.appendChild(rootSection);

    // Wire data root save
    const rootInput = rootSection.querySelector('#storage-data-root') as HTMLInputElement;
    const rootSaveBtn = rootSection.querySelector('#storage-data-root-save') as HTMLButtonElement;
    rootSaveBtn.addEventListener('click', async () => {
      const val = rootInput.value.trim();
      if (!val) return;
      try {
        rootSaveBtn.disabled = true;
        rootSaveBtn.textContent = 'Saving…';
        await pawEngine.storageSetDataRoot(val);
        rootSaveBtn.innerHTML =
          '<span class="ms ms-sm" style="margin-right:2px">check</span> Saved — restart required';
        setTimeout(() => loadStorageSettings(), 2000);
      } catch (e) {
        rootSaveBtn.textContent = 'Error';
        alert(`Failed to set data root: ${e}`);
        setTimeout(() => {
          rootSaveBtn.innerHTML =
            '<span class="ms ms-sm" style="margin-right:2px">save</span> Save';
          rootSaveBtn.disabled = false;
        }, 2000);
      }
    });

    const resetBtn = rootSection.querySelector(
      '#storage-data-root-reset',
    ) as HTMLButtonElement | null;
    if (resetBtn) {
      resetBtn.addEventListener('click', async () => {
        if (
          !(await confirmModal(
            'Reset data root to default (~/.paw/)? Requires a restart.',
            'Reset Data Root',
          ))
        )
          return;
        await pawEngine.storageSetDataRoot(null);
        loadStorageSettings();
      });
    }

    // ── Storage Breakdown ─────────────────────────────────────────────
    const statsSection = document.createElement('div');
    statsSection.className = 'settings-section';
    const totalSize =
      paths.engine_db_size + paths.workspaces_size + paths.skills_size + paths.browser_size;
    statsSection.innerHTML = `
      <h2 class="settings-section-title">Storage Usage</h2>
      <p class="settings-section-desc" style="margin-bottom: 12px">
        Total: <strong>${formatBytes(totalSize)}</strong>
      </p>
      <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 8px; max-width: 480px">
        ${storageCard('database', 'Engine Database', paths.engine_db, paths.engine_db_size)}
        ${storageCard('folder', 'Agent Workspaces', paths.workspaces_dir, paths.workspaces_size)}
        ${storageCard('extension', 'Skills', paths.skills_dir, paths.skills_size)}
        ${storageCard('language', 'Browser Profiles', paths.browser_dir, paths.browser_size)}
      </div>
    `;
    container.appendChild(statsSection);

    // ── User Workspace ────────────────────────────────────────────────
    const wsSection = document.createElement('div');
    wsSection.className = 'settings-section';
    wsSection.innerHTML = `
      <h2 class="settings-section-title">User Workspace</h2>
      <p class="settings-section-desc">
        Where Paw saves user-facing files — research, content, and builds.
        This is separate from the engine data root.
      </p>
      <div class="form-group" style="margin-bottom: 12px">
        <label class="form-label">Workspace path</label>
        <div style="display: flex; align-items: center; gap: 8px">
          <input type="text" class="form-input" id="storage-workspace-path"
                 value="${escapeHtml(userWorkspace)}"
                 style="flex: 1; font-family: var(--font-mono, monospace); font-size: 12px" />
          <button class="btn btn-sm btn-primary" id="storage-workspace-save">
            <span class="ms ms-sm" style="margin-right: 2px">save</span> Save
          </button>
        </div>
        <p style="color: var(--text-muted); font-size: 11px; margin-top: 4px">
          Default: <code style="font-size: 11px">~/Documents/Paw</code>
        </p>
      </div>
    `;
    container.appendChild(wsSection);

    // Wire workspace save
    const wsInput = wsSection.querySelector('#storage-workspace-path') as HTMLInputElement;
    const wsSaveBtn = wsSection.querySelector('#storage-workspace-save') as HTMLButtonElement;
    wsSaveBtn.addEventListener('click', () => {
      const val = wsInput.value.trim();
      if (!val) return;
      setWorkspacePath(val);
      wsSaveBtn.innerHTML = '<span class="ms ms-sm" style="margin-right:2px">check</span> Saved';
      setTimeout(() => {
        wsSaveBtn.innerHTML = '<span class="ms ms-sm" style="margin-right:2px">save</span> Save';
      }, 2000);
    });

    // ── Cloud Sync Tip ────────────────────────────────────────────────
    const syncSection = document.createElement('div');
    syncSection.className = 'settings-section';
    syncSection.innerHTML = `
      <h2 class="settings-section-title">
        <span class="ms ms-sm" style="margin-right: 4px; color: var(--accent)">cloud_sync</span>
        Cloud Sync
      </h2>
      <div style="background: var(--bg-secondary); border: 1px solid var(--border); border-radius: 8px; padding: 16px; max-width: 560px">
        <p style="color: var(--text-secondary); font-size: 13px; line-height: 1.6; margin: 0 0 8px 0">
          <strong>Sync your workspace across devices</strong> by pointing the User Workspace
          path to a folder synced by your cloud provider:
        </p>
        <ul style="color: var(--text-secondary); font-size: 13px; line-height: 1.8; margin: 0; padding-left: 20px">
          <li><strong>Google Drive</strong> — <code style="font-size: 11px">~/Google Drive/Paw</code></li>
          <li><strong>Dropbox</strong> — <code style="font-size: 11px">~/Dropbox/Paw</code></li>
          <li><strong>OneDrive</strong> — <code style="font-size: 11px">~/OneDrive/Paw</code></li>
          <li><strong>iCloud</strong> — <code style="font-size: 11px">~/Library/Mobile Documents/com~apple~CloudDocs/Paw</code></li>
        </ul>
        <p style="color: var(--text-muted); font-size: 12px; margin: 10px 0 0 0">
          <span class="ms ms-sm" style="font-size: 14px; vertical-align: middle">info</span>
          The engine database and browser profiles stay local — only user-facing files sync.
        </p>
      </div>
    `;
    container.appendChild(syncSection);
  } catch (e) {
    container.innerHTML = `<p style="color:var(--error)">Failed to load storage settings: ${e}</p>`;
  }
}

// ── Internal helpers ──────────────────────────────────────────────────

function storageCard(icon: string, label: string, path: string, size: number): string {
  return `
    <div style="background: var(--bg-secondary); border: 1px solid var(--border); border-radius: 6px; padding: 10px 12px">
      <div style="display: flex; align-items: center; gap: 6px; margin-bottom: 4px">
        <span class="ms ms-sm" style="color: var(--accent)">${icon}</span>
        <span style="font-weight: 600; font-size: 13px">${label}</span>
        <span style="margin-left: auto; font-size: 12px; color: var(--text-muted)">${formatBytes(size)}</span>
      </div>
      <code style="font-size: 10px; color: var(--text-muted); word-break: break-all">${escapeHtml(path)}</code>
    </div>
  `;
}

function escapeHtml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
