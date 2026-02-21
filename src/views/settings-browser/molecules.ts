// Settings: Browser & Sandbox — DOM rendering + IPC

import {
  pawEngine,
  type BrowserConfig,
  type ScreenshotEntry,
  type WorkspaceInfo,
  type NetworkPolicy,
} from '../../engine';
import { showToast } from '../../components/toast';
import { formatBytes, timeAgo, isValidDomain } from '../../features/browser-sandbox';
import { $, escHtml, confirmModal } from '../../components/helpers';

// ── State bridge ──────────────────────────────────────────────────────

interface MoleculesState {
  getLoadBrowserSettings: () => () => Promise<void>;
}

let _state: MoleculesState;

export function initMoleculesState() {
  return {
    setMoleculesState(s: MoleculesState) {
      _state = s;
    },
  };
}

// ── Render Profiles ───────────────────────────────────────────────────

export function renderProfiles(container: HTMLElement, browserConfig: BrowserConfig) {
  const profileSection = document.createElement('div');
  profileSection.className = 'settings-subsection';
  profileSection.innerHTML = `
    <h3 class="settings-subsection-title">
      <span class="ms ms-sm">language</span>
      Browser Profiles
    </h3>
    <p class="form-hint" style="margin:0 0 12px;font-size:12px;color:var(--text-muted)">
      Managed Chrome profiles with persistent state (cookies, sessions, storage).
      Each profile gets its own user-data directory.
    </p>
  `;

  const profilesList = document.createElement('div');
  profilesList.style.cssText = 'display:flex;flex-direction:column;gap:8px;margin-bottom:12px';

  for (const profile of browserConfig.profiles) {
    const row = document.createElement('div');
    row.style.cssText =
      'display:flex;align-items:center;gap:12px;padding:10px 14px;background:var(--surface-elevated);border-radius:8px;border:1px solid var(--border-color)';
    const isDefault = profile.id === browserConfig.default_profile;
    row.innerHTML = `
      <div style="flex:1;min-width:0">
        <div style="font-weight:600;font-size:13px;color:var(--text-primary)">
          ${escHtml(profile.name)}
          ${isDefault ? '<span style="font-size:11px;color:var(--accent);margin-left:6px">★ default</span>' : ''}
        </div>
        <div style="font-size:11px;color:var(--text-muted);margin-top:2px">
          ${formatBytes(profile.size_bytes)} · Created ${timeAgo(profile.created_at)}
        </div>
      </div>
    `;

    const actions = document.createElement('div');
    actions.style.cssText = 'display:flex;gap:4px';

    if (!isDefault) {
      const setDefaultBtn = document.createElement('button');
      setDefaultBtn.className = 'btn btn-ghost btn-sm';
      setDefaultBtn.textContent = 'Set Default';
      setDefaultBtn.addEventListener('click', async () => {
        browserConfig.default_profile = profile.id;
        await pawEngine.browserSetConfig(browserConfig);
        showToast(`Default profile set to "${profile.name}"`, 'success');
        _state.getLoadBrowserSettings()();
      });
      actions.appendChild(setDefaultBtn);

      const deleteBtn = document.createElement('button');
      deleteBtn.className = 'btn btn-ghost btn-sm';
      deleteBtn.style.color = 'var(--error)';
      deleteBtn.textContent = 'Delete';
      deleteBtn.addEventListener('click', async () => {
        if (!(await confirmModal(`Delete profile "${profile.name}" and all its data?`))) return;
        await pawEngine.browserDeleteProfile(profile.id);
        showToast(`Profile "${profile.name}" deleted`, 'success');
        _state.getLoadBrowserSettings()();
      });
      actions.appendChild(deleteBtn);
    }

    row.appendChild(actions);
    profilesList.appendChild(row);
  }

  profileSection.appendChild(profilesList);

  // New profile button
  const newProfileRow = document.createElement('div');
  newProfileRow.style.cssText = 'display:flex;gap:8px;align-items:center';
  const newProfileInput = document.createElement('input');
  newProfileInput.type = 'text';
  newProfileInput.className = 'form-input';
  newProfileInput.placeholder = 'New profile name…';
  newProfileInput.style.cssText = 'max-width:220px;font-size:13px';
  const newProfileBtn = document.createElement('button');
  newProfileBtn.className = 'btn btn-sm';
  newProfileBtn.textContent = '+ Create Profile';
  newProfileBtn.addEventListener('click', async () => {
    const name = newProfileInput.value.trim();
    if (!name) return;
    try {
      await pawEngine.browserCreateProfile(name);
      newProfileInput.value = '';
      showToast(`Profile "${name}" created`, 'success');
      _state.getLoadBrowserSettings()();
    } catch (e) {
      showToast(`Failed to create profile: ${e}`, 'error');
    }
  });
  newProfileRow.appendChild(newProfileInput);
  newProfileRow.appendChild(newProfileBtn);
  profileSection.appendChild(newProfileRow);

  // Browser options
  const optsRow = document.createElement('div');
  optsRow.style.cssText = 'display:flex;gap:16px;margin-top:16px;flex-wrap:wrap';
  optsRow.innerHTML = `
    <label style="display:flex;align-items:center;gap:6px;font-size:13px;color:var(--text-secondary)">
      <input type="checkbox" id="browser-headless" ${browserConfig.headless ? 'checked' : ''} />
      Headless mode
    </label>
    <label style="display:flex;align-items:center;gap:6px;font-size:13px;color:var(--text-secondary)">
      <input type="checkbox" id="browser-auto-close" ${browserConfig.auto_close_tabs ? 'checked' : ''} />
      Auto-close tabs
    </label>
    <div style="display:flex;align-items:center;gap:6px;font-size:13px;color:var(--text-secondary)">
      <span>Idle timeout:</span>
      <input type="number" class="form-input" id="browser-idle-timeout" value="${browserConfig.idle_timeout_secs}" min="30" max="3600" style="width:80px;font-size:13px" />
      <span>sec</span>
    </div>
  `;
  profileSection.appendChild(optsRow);

  // Save browser options
  const saveBrowserBtn = document.createElement('button');
  saveBrowserBtn.className = 'btn btn-primary btn-sm';
  saveBrowserBtn.textContent = 'Save Browser Settings';
  saveBrowserBtn.style.marginTop = '12px';
  saveBrowserBtn.addEventListener('click', async () => {
    browserConfig.headless = ($('browser-headless') as HTMLInputElement)?.checked ?? true;
    browserConfig.auto_close_tabs = ($('browser-auto-close') as HTMLInputElement)?.checked ?? true;
    browserConfig.idle_timeout_secs =
      parseInt(($('browser-idle-timeout') as HTMLInputElement)?.value ?? '300', 10) || 300;
    await pawEngine.browserSetConfig(browserConfig);
    showToast('Browser settings saved', 'success');
  });
  profileSection.appendChild(saveBrowserBtn);
  container.appendChild(profileSection);
}

// ── Render Screenshots ────────────────────────────────────────────────

export function renderScreenshots(container: HTMLElement, screenshots: ScreenshotEntry[]) {
  const screenshotSection = document.createElement('div');
  screenshotSection.className = 'settings-subsection';
  screenshotSection.style.marginTop = '24px';
  screenshotSection.innerHTML = `
    <h3 class="settings-subsection-title">
      <span class="ms ms-sm">screenshot_monitor</span>
      Screenshot Gallery
    </h3>
    <p class="form-hint" style="margin:0 0 12px;font-size:12px;color:var(--text-muted)">
      Screenshots captured by agents via <code>web_screenshot</code>.
      Click to view full-size, or use the camera icon in chat to insert inline.
    </p>
  `;

  if (screenshots.length === 0) {
    const empty = document.createElement('div');
    empty.style.cssText =
      'padding:24px;text-align:center;color:var(--text-muted);font-size:13px;background:var(--surface-elevated);border-radius:8px;border:1px solid var(--border-color)';
    empty.textContent = 'No screenshots yet. Agents will save them here when using web_screenshot.';
    screenshotSection.appendChild(empty);
  } else {
    const grid = document.createElement('div');
    grid.style.cssText =
      'display:grid;grid-template-columns:repeat(auto-fill,minmax(180px,1fr));gap:10px';

    for (const ss of screenshots.slice(0, 50)) {
      const card = document.createElement('div');
      card.style.cssText =
        'position:relative;border-radius:8px;overflow:hidden;border:1px solid var(--border-color);background:var(--surface-elevated);cursor:pointer';

      const thumb = document.createElement('div');
      thumb.style.cssText =
        'height:120px;background:var(--bg-primary);display:flex;align-items:center;justify-content:center;overflow:hidden';
      thumb.innerHTML =
        '<span class="ms" style="font-size:40px;color:var(--text-muted)">image</span>';

      const loadThumb = async () => {
        try {
          const full = await pawEngine.screenshotGet(ss.filename);
          if (full.base64_png) {
            thumb.innerHTML = '';
            const img = document.createElement('img');
            img.src = `data:image/png;base64,${full.base64_png}`;
            img.style.cssText = 'width:100%;height:100%;object-fit:cover';
            img.alt = ss.filename;
            thumb.appendChild(img);
          }
        } catch {
          /* ignore failed loads */
        }
      };

      const observer = new IntersectionObserver((entries) => {
        if (entries[0]?.isIntersecting) {
          loadThumb();
          observer.disconnect();
        }
      });

      card.appendChild(thumb);

      const info = document.createElement('div');
      info.style.cssText = 'padding:6px 8px;font-size:11px;color:var(--text-muted)';
      info.innerHTML = `
        <div style="font-weight:600;color:var(--text-secondary);white-space:nowrap;overflow:hidden;text-overflow:ellipsis">${escHtml(ss.filename)}</div>
        <div>${formatBytes(ss.size_bytes)} · ${timeAgo(ss.created_at)}</div>
      `;
      card.appendChild(info);

      card.addEventListener('click', async () => {
        try {
          const full = await pawEngine.screenshotGet(ss.filename);
          if (full.base64_png) {
            const win = window.open('', '_blank');
            if (win) {
              win.document.title = ss.filename;
              win.document.body.style.cssText =
                'margin:0;background:#111;display:flex;align-items:center;justify-content:center;min-height:100vh';
              const img = win.document.createElement('img');
              img.src = `data:image/png;base64,${full.base64_png}`;
              img.style.maxWidth = '100%';
              win.document.body.appendChild(img);
            }
          }
        } catch (e) {
          showToast(`Failed to load screenshot: ${e}`, 'error');
        }
      });

      const delBtn = document.createElement('button');
      delBtn.style.cssText =
        'position:absolute;top:4px;right:4px;background:rgba(0,0,0,0.7);border:none;border-radius:4px;color:#ff6666;cursor:pointer;padding:2px 6px;font-size:12px';
      delBtn.textContent = '✕';
      delBtn.addEventListener('click', async (e) => {
        e.stopPropagation();
        await pawEngine.screenshotDelete(ss.filename);
        showToast('Screenshot deleted', 'success');
        _state.getLoadBrowserSettings()();
      });
      card.appendChild(delBtn);

      screenshotSection.appendChild(grid);
      grid.appendChild(card);

      requestAnimationFrame(() => observer.observe(card));
    }
  }

  container.appendChild(screenshotSection);
}

// ── Render Workspaces ─────────────────────────────────────────────────

export function renderWorkspaces(container: HTMLElement, workspaces: WorkspaceInfo[]) {
  const workspaceSection = document.createElement('div');
  workspaceSection.className = 'settings-subsection';
  workspaceSection.style.marginTop = '24px';
  workspaceSection.innerHTML = `
    <h3 class="settings-subsection-title">
      <span class="ms ms-sm">folder_open</span>
      Agent Workspaces
    </h3>
    <p class="form-hint" style="margin:0 0 12px;font-size:12px;color:var(--text-muted)">
      Each agent gets an isolated filesystem workspace at <code>~/.paw/workspaces/{agent_id}/</code>.
      Files created by exec, write_file, etc. are scoped to the agent's directory.
    </p>
  `;

  if (workspaces.length === 0) {
    const empty = document.createElement('div');
    empty.style.cssText =
      'padding:24px;text-align:center;color:var(--text-muted);font-size:13px;background:var(--surface-elevated);border-radius:8px;border:1px solid var(--border-color)';
    empty.textContent =
      "No agent workspaces created yet. They're auto-created when an agent writes files.";
    workspaceSection.appendChild(empty);
  } else {
    const wsList = document.createElement('div');
    wsList.style.cssText = 'display:flex;flex-direction:column;gap:8px';

    for (const ws of workspaces) {
      const row = document.createElement('div');
      row.style.cssText =
        'display:flex;align-items:center;gap:12px;padding:10px 14px;background:var(--surface-elevated);border-radius:8px;border:1px solid var(--border-color)';
      row.innerHTML = `
        <span class="ms ms-sm" style="color:var(--accent)">folder</span>
        <div style="flex:1;min-width:0">
          <div style="font-weight:600;font-size:13px;color:var(--text-primary)">${escHtml(ws.agent_id)}</div>
          <div style="font-size:11px;color:var(--text-muted)">${ws.total_files} files · ${formatBytes(ws.total_size_bytes)}</div>
        </div>
      `;

      const browseBtn = document.createElement('button');
      browseBtn.className = 'btn btn-ghost btn-sm';
      browseBtn.textContent = 'Browse';
      browseBtn.addEventListener('click', () => browseWorkspace(ws.agent_id, workspaceSection));
      row.appendChild(browseBtn);

      const deleteBtn = document.createElement('button');
      deleteBtn.className = 'btn btn-ghost btn-sm';
      deleteBtn.style.color = 'var(--error)';
      deleteBtn.textContent = 'Delete';
      deleteBtn.addEventListener('click', async () => {
        if (
          !(await confirmModal(
            `Delete entire workspace for agent "${ws.agent_id}"? This cannot be undone.`,
          ))
        )
          return;
        await pawEngine.workspaceDelete(ws.agent_id);
        showToast(`Workspace for "${ws.agent_id}" deleted`, 'success');
        _state.getLoadBrowserSettings()();
      });
      row.appendChild(deleteBtn);

      wsList.appendChild(row);
    }

    workspaceSection.appendChild(wsList);
  }

  container.appendChild(workspaceSection);
}

// ── Render Network Policy ─────────────────────────────────────────────

export function renderNetworkPolicy(container: HTMLElement, networkPolicy: NetworkPolicy) {
  const networkSection = document.createElement('div');
  networkSection.className = 'settings-subsection';
  networkSection.style.marginTop = '24px';
  networkSection.innerHTML = `
    <h3 class="settings-subsection-title">
      <span class="ms ms-sm">shield</span>
      Outbound Network Policy
    </h3>
    <p class="form-hint" style="margin:0 0 12px;font-size:12px;color:var(--text-muted)">
      Control which domains agents can access. When the allowlist is enabled,
      only listed domains are reachable. Blocked domains are always blocked.
    </p>
  `;

  // Enable toggle
  const enableRow = document.createElement('label');
  enableRow.style.cssText =
    'display:flex;align-items:center;gap:8px;margin-bottom:16px;cursor:pointer';
  enableRow.innerHTML = `
    <input type="checkbox" id="network-allowlist-enabled" ${networkPolicy.enabled ? 'checked' : ''} />
    <div>
      <div style="font-weight:600;font-size:13px;color:var(--text-primary)">Enable domain allowlist</div>
      <div style="font-size:11px;color:var(--text-muted)">When enabled, agents can only fetch from listed domains</div>
    </div>
  `;
  networkSection.appendChild(enableRow);

  // Allowed domains
  const allowedLabel = document.createElement('h4');
  allowedLabel.style.cssText =
    'font-size:13px;font-weight:600;color:var(--text-secondary);margin:12px 0 6px';
  allowedLabel.textContent = 'Allowed Domains';
  networkSection.appendChild(allowedLabel);

  const allowedTextarea = document.createElement('textarea');
  allowedTextarea.id = 'network-allowed-domains';
  allowedTextarea.className = 'form-input';
  allowedTextarea.rows = 6;
  allowedTextarea.style.cssText =
    'font-family:var(--font-mono);font-size:12px;width:100%;resize:vertical';
  allowedTextarea.placeholder = 'api.openai.com\napi.anthropic.com\n*.example.com';
  allowedTextarea.value = networkPolicy.allowed_domains.join('\n');
  networkSection.appendChild(allowedTextarea);

  // Blocked domains
  const blockedLabel = document.createElement('h4');
  blockedLabel.style.cssText =
    'font-size:13px;font-weight:600;color:var(--text-secondary);margin:12px 0 6px';
  blockedLabel.textContent = 'Blocked Domains (always blocked)';
  networkSection.appendChild(blockedLabel);

  const blockedTextarea = document.createElement('textarea');
  blockedTextarea.id = 'network-blocked-domains';
  blockedTextarea.className = 'form-input';
  blockedTextarea.rows = 4;
  blockedTextarea.style.cssText =
    'font-family:var(--font-mono);font-size:12px;width:100%;resize:vertical';
  blockedTextarea.placeholder = 'pastebin.com\ntransfer.sh';
  blockedTextarea.value = networkPolicy.blocked_domains.join('\n');
  networkSection.appendChild(blockedTextarea);

  // Log toggle
  const logRow = document.createElement('label');
  logRow.style.cssText = 'display:flex;align-items:center;gap:8px;margin:12px 0;cursor:pointer';
  logRow.innerHTML = `
    <input type="checkbox" id="network-log-requests" ${networkPolicy.log_requests ? 'checked' : ''} />
    <span style="font-size:13px;color:var(--text-secondary)">Log all outbound requests</span>
  `;
  networkSection.appendChild(logRow);

  // Test URL
  const testRow = document.createElement('div');
  testRow.style.cssText = 'display:flex;gap:8px;align-items:center;margin:12px 0';
  const testInput = document.createElement('input');
  testInput.type = 'text';
  testInput.className = 'form-input';
  testInput.placeholder = 'https://example.com/path';
  testInput.style.cssText = 'max-width:280px;font-size:13px';
  const testBtn = document.createElement('button');
  testBtn.className = 'btn btn-sm';
  testBtn.textContent = 'Test URL';
  const testResult = document.createElement('span');
  testResult.style.cssText = 'font-size:12px;color:var(--text-muted)';
  testBtn.addEventListener('click', async () => {
    const url = testInput.value.trim();
    if (!url) return;
    try {
      const [allowed, domain] = await pawEngine.networkCheckUrl(url);
      testResult.textContent = allowed ? `${domain} — allowed` : `${domain} — blocked`;
      testResult.style.color = allowed ? 'var(--success)' : 'var(--error)';
    } catch (e) {
      testResult.textContent = `Error: ${e}`;
      testResult.style.color = 'var(--error)';
    }
  });
  testRow.appendChild(testInput);
  testRow.appendChild(testBtn);
  testRow.appendChild(testResult);
  networkSection.appendChild(testRow);

  // Save button
  const saveRow = document.createElement('div');
  saveRow.style.cssText = 'display:flex;gap:8px;margin-top:12px';
  const saveNetworkBtn = document.createElement('button');
  saveNetworkBtn.className = 'btn btn-primary btn-sm';
  saveNetworkBtn.textContent = 'Save Network Policy';
  saveNetworkBtn.addEventListener('click', async () => {
    const allowedDomains = (($('network-allowed-domains') as HTMLTextAreaElement)?.value ?? '')
      .split('\n')
      .map((d) => d.trim())
      .filter((d) => d.length > 0);
    const blockedDomains = (($('network-blocked-domains') as HTMLTextAreaElement)?.value ?? '')
      .split('\n')
      .map((d) => d.trim())
      .filter((d) => d.length > 0);

    const invalidAllowed = allowedDomains.filter((d) => !isValidDomain(d));
    const invalidBlocked = blockedDomains.filter((d) => !isValidDomain(d));
    if (invalidAllowed.length > 0) {
      showToast(`Invalid allowed domains: ${invalidAllowed.join(', ')}`, 'error');
      return;
    }
    if (invalidBlocked.length > 0) {
      showToast(`Invalid blocked domains: ${invalidBlocked.join(', ')}`, 'error');
      return;
    }

    const policy = {
      enabled: ($('network-allowlist-enabled') as HTMLInputElement)?.checked ?? false,
      allowed_domains: allowedDomains,
      blocked_domains: blockedDomains,
      log_requests: ($('network-log-requests') as HTMLInputElement)?.checked ?? true,
      recent_requests: [],
    };

    await pawEngine.networkSetPolicy(policy);
    showToast('Network policy saved', 'success');
  });

  const resetNetworkBtn = document.createElement('button');
  resetNetworkBtn.className = 'btn btn-ghost btn-sm';
  resetNetworkBtn.textContent = 'Reset to Defaults';
  resetNetworkBtn.addEventListener('click', async () => {
    const { DEFAULT_ALLOWED_DOMAINS, DEFAULT_BLOCKED_DOMAINS } =
      await import('../../features/browser-sandbox');
    await pawEngine.networkSetPolicy({
      enabled: false,
      allowed_domains: [...DEFAULT_ALLOWED_DOMAINS],
      blocked_domains: [...DEFAULT_BLOCKED_DOMAINS],
      log_requests: true,
      recent_requests: [],
    });
    showToast('Network policy reset to defaults', 'success');
    _state.getLoadBrowserSettings()();
  });

  saveRow.appendChild(saveNetworkBtn);
  saveRow.appendChild(resetNetworkBtn);
  networkSection.appendChild(saveRow);
  container.appendChild(networkSection);
}

// ── Browse Workspace Files ────────────────────────────────────────────

export async function browseWorkspace(agentId: string, parentSection: HTMLElement) {
  try {
    const files = await pawEngine.workspaceFiles(agentId);

    const existing = parentSection.querySelector('.workspace-browser');
    if (existing) existing.remove();

    const panel = document.createElement('div');
    panel.className = 'workspace-browser';
    panel.style.cssText =
      'margin-top:12px;padding:14px;background:var(--bg-primary);border-radius:8px;border:1px solid var(--border-color)';

    const header = document.createElement('div');
    header.style.cssText =
      'display:flex;justify-content:space-between;align-items:center;margin-bottom:10px';
    header.innerHTML = `
      <div style="font-weight:600;font-size:13px;color:var(--text-primary)">
        <span class="ms ms-sm" style="color:var(--accent)">folder_open</span>
        ${escHtml(agentId)}
      </div>
    `;
    const closeBtn = document.createElement('button');
    closeBtn.className = 'btn btn-ghost btn-sm';
    closeBtn.textContent = 'Close';
    closeBtn.addEventListener('click', () => panel.remove());
    header.appendChild(closeBtn);
    panel.appendChild(header);

    if (files.length === 0) {
      panel.innerHTML +=
        '<p style="color:var(--text-muted);font-size:12px;text-align:center;padding:12px">Workspace is empty</p>';
    } else {
      const table = document.createElement('table');
      table.style.cssText = 'width:100%;font-size:12px;border-collapse:collapse';
      table.innerHTML = `
        <thead>
          <tr style="text-align:left;color:var(--text-muted);border-bottom:1px solid var(--border-color)">
            <th style="padding:4px 8px">Name</th>
            <th style="padding:4px 8px">Size</th>
            <th style="padding:4px 8px">Modified</th>
          </tr>
        </thead>
      `;
      const tbody = document.createElement('tbody');

      for (const file of files) {
        const tr = document.createElement('tr');
        tr.style.cssText =
          'border-bottom:1px solid var(--border-subtle);color:var(--text-secondary)';
        const icon = file.is_dir ? 'folder' : 'description';
        const iconColor = file.is_dir ? 'var(--accent)' : 'var(--text-muted)';
        tr.innerHTML = `
          <td style="padding:4px 8px"><span class="ms ms-sm" style="color:${iconColor};margin-right:4px">${icon}</span>${escHtml(file.name)}</td>
          <td style="padding:4px 8px">${formatBytes(file.size_bytes)}</td>
          <td style="padding:4px 8px">${file.modified_at ? timeAgo(file.modified_at) : '—'}</td>
        `;
        tbody.appendChild(tr);
      }

      table.appendChild(tbody);
      panel.appendChild(table);
    }

    parentSection.appendChild(panel);
  } catch (e) {
    showToast(`Failed to browse workspace: ${e}`, 'error');
  }
}
