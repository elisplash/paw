// Settings View — Logs, Usage, Presence, Nodes, Devices, Exec Approvals, Security Policies
// Extracted from main.ts for maintainability

import { gateway } from '../gateway';
import { loadSecuritySettings, saveSecuritySettings, type SecuritySettings } from '../security';
import { getSecurityAuditLog } from '../db';

const $ = (id: string) => document.getElementById(id);

// Shared state — will be passed from main
let wsConnected = false;

export function setWsConnected(connected: boolean) {
  wsConnected = connected;
}

function escHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

// ── Gateway Status ─────────────────────────────────────────────────────────
export async function loadSettingsStatus() {
  if (!wsConnected) return;
  const section = $('settings-status-section');
  const content = $('settings-status-content');
  try {
    const [health, status] = await Promise.all([
      gateway.getHealth().catch(() => null),
      gateway.getStatus().catch(() => null) as Promise<Record<string, unknown> | null>,
    ]);
    if (!health && !status) { if (section) section.style.display = 'none'; return; }
    if (section) section.style.display = '';
    let html = '';
    if (health) {
      html += `<div class="status-card"><div class="status-card-label">Uptime</div><div class="status-card-value">${health.ts ? new Date(health.ts).toLocaleString() : '—'}</div></div>`;
      html += `<div class="status-card"><div class="status-card-label">Sessions</div><div class="status-card-value">${health.sessions?.active ?? 0} active / ${health.sessions?.total ?? 0} total</div></div>`;
      html += `<div class="status-card"><div class="status-card-label">Agents</div><div class="status-card-value">${health.agents?.length ?? 0}</div></div>`;
      const channelCount = Object.keys(health.channels ?? {}).length;
      html += `<div class="status-card"><div class="status-card-label">Channels</div><div class="status-card-value">${channelCount}</div></div>`;
    }
    if (status) {
      const version = (status as Record<string, unknown>).version;
      const nodeVersion = (status as Record<string, unknown>).nodeVersion;
      if (version) html += `<div class="status-card"><div class="status-card-label">Version</div><div class="status-card-value">${escHtml(String(version))}</div></div>`;
      if (nodeVersion) html += `<div class="status-card"><div class="status-card-label">Node.js</div><div class="status-card-value">${escHtml(String(nodeVersion))}</div></div>`;
    }
    if (content) content.innerHTML = html || '<p style="color:var(--text-muted)">No status data</p>';
  } catch (e) {
    console.warn('[settings] Status load failed:', e);
    if (section) section.style.display = 'none';
  }
}

// ── Logs Viewer ────────────────────────────────────────────────────────────
export async function loadSettingsLogs() {
  if (!wsConnected) return;
  const section = $('settings-logs-section');
  const output = $('settings-logs-output');
  const linesSelect = $('settings-logs-lines') as HTMLSelectElement | null;
  try {
    const lines = parseInt(linesSelect?.value ?? '100', 10);
    const result = await gateway.logsTail(lines);
    if (section) section.style.display = '';
    if (output) output.textContent = (result.lines ?? []).join('\n') || '(no logs)';
  } catch (e) {
    console.warn('[settings] Logs load failed:', e);
    if (section) section.style.display = 'none';
  }
}

// ── Usage Dashboard ────────────────────────────────────────────────────────
let _usageRefreshInterval: ReturnType<typeof setInterval> | null = null;

export async function loadSettingsUsage() {
  if (!wsConnected) return;
  const section = $('settings-usage-section');
  const content = $('settings-usage-content');
  try {
    const [status, cost] = await Promise.all([
      gateway.usageStatus().catch(() => null),
      gateway.usageCost().catch(() => null),
    ]);
    // Always show the section — even without gateway data, show helpful state
    if (section) section.style.display = '';
    if (!status && !cost) {
      const emptyHtml = `<div class="usage-empty-state">
        <p style="color:var(--text-secondary);margin:0 0 8px">No usage data from gateway yet.</p>
        <p style="color:var(--text-muted);margin:0;font-size:12px">Token tracking is active in chat — send a message to see per-session estimates in the chat header. Gateway-level usage data (total cost, by-model breakdown) requires OpenClaw's usage tracking to be enabled.</p>
      </div>`;
      if (content) content.innerHTML = emptyHtml;
      return;
    }
    let html = '';
    if (status?.total) {
      html += `<div class="usage-card">
        <div class="usage-card-label">Requests</div>
        <div class="usage-card-value">${status.total.requests?.toLocaleString() ?? '—'}</div>
      </div>
      <div class="usage-card">
        <div class="usage-card-label">Tokens</div>
        <div class="usage-card-value">${status.total.tokens?.toLocaleString() ?? '—'}</div>
        <div class="usage-card-sub">In: ${(status.total.inputTokens ?? 0).toLocaleString()} / Out: ${(status.total.outputTokens ?? 0).toLocaleString()}</div>
      </div>`;
    }
    if (cost?.totalCost != null) {
      html += `<div class="usage-card">
        <div class="usage-card-label">Total Cost</div>
        <div class="usage-card-value">$${cost.totalCost.toFixed(4)} ${cost.currency ?? ''}</div>
      </div>`;

      // Budget check
      checkBudgetAlert(cost.totalCost);
    }
    if (cost?.period) {
      html += `<div class="usage-card">
        <div class="usage-card-label">Period</div>
        <div class="usage-card-value">${escHtml(String(cost.period))}</div>
      </div>`;
    }
    // Per-model breakdown with cost
    if (status?.byModel || cost?.byModel) {
      html += '<div class="usage-models"><h4>By Model</h4>';
      const allModels = new Set([
        ...Object.keys(status?.byModel ?? {}),
        ...Object.keys(cost?.byModel ?? {}),
      ]);
      for (const model of allModels) {
        const s = (status?.byModel?.[model] ?? {}) as { requests?: number; tokens?: number; inputTokens?: number; outputTokens?: number };
        const c = (cost?.byModel?.[model] ?? {}) as { cost?: number; requests?: number };
        const costStr = c.cost != null ? `$${c.cost.toFixed(4)}` : '';
        const tokStr = s.tokens ? `${s.tokens.toLocaleString()} tok` : '';
        const reqStr = s.requests ? `${s.requests.toLocaleString()} req` : (c.requests ? `${c.requests.toLocaleString()} req` : '');
        const parts = [reqStr, tokStr, costStr].filter(Boolean).join(' · ');
        html += `<div class="usage-model-row">
          <span class="usage-model-name">${escHtml(model)}</span>
          <span>${parts}</span>
        </div>`;
      }
      html += '</div>';
    }
    if (content) content.innerHTML = html || '<p style="color:var(--text-muted)">No usage data</p>';
  } catch (e) {
    console.warn('[settings] Usage load failed:', e);
    // Don't hide the section — show a helpful message
    if (section) section.style.display = '';
    if (content) content.innerHTML = `<div class="usage-empty-state">
      <p style="color:var(--text-secondary);margin:0 0 8px">Could not load usage data from gateway.</p>
      <p style="color:var(--text-muted);margin:0;font-size:12px">Token tracking is still active in the chat header. Click Refresh to retry.</p>
    </div>`;
  }
}

/** Start auto-refresh for usage dashboard (every 30s) */
export function startUsageAutoRefresh() {
  stopUsageAutoRefresh();
  _usageRefreshInterval = setInterval(() => {
    if (wsConnected) loadSettingsUsage().catch(() => {});
  }, 30_000);
}

export function stopUsageAutoRefresh() {
  if (_usageRefreshInterval) {
    clearInterval(_usageRefreshInterval);
    _usageRefreshInterval = null;
  }
}

// ── Budget Alert ───────────────────────────────────────────────────────────
const BUDGET_KEY = 'paw-budget-limit';

export function getBudgetLimit(): number | null {
  const saved = localStorage.getItem(BUDGET_KEY);
  if (!saved) return null;
  const n = parseFloat(saved);
  return isNaN(n) || n <= 0 ? null : n;
}

export function setBudgetLimit(limit: number | null) {
  if (limit == null || limit <= 0) {
    localStorage.removeItem(BUDGET_KEY);
  } else {
    localStorage.setItem(BUDGET_KEY, String(limit));
  }
}

function checkBudgetAlert(currentCost: number) {
  const limit = getBudgetLimit();
  if (limit == null) return;
  const alertEl = $('budget-alert');
  if (!alertEl) return;

  if (currentCost >= limit) {
    alertEl.style.display = '';
    const text = $('budget-alert-text');
    if (text) text.textContent = `Budget limit reached: $${currentCost.toFixed(4)} / $${limit.toFixed(2)} — consider switching to a cheaper model or pausing automations`;
  } else if (currentCost >= limit * 0.8) {
    alertEl.style.display = '';
    const text = $('budget-alert-text');
    if (text) text.textContent = `Approaching budget: $${currentCost.toFixed(4)} / $${limit.toFixed(2)} (${((currentCost / limit) * 100).toFixed(0)}%)`;
  } else {
    alertEl.style.display = 'none';
  }
}

export function initBudgetSettings() {
  const input = $('budget-limit-input') as HTMLInputElement | null;
  const saveBtn = $('budget-limit-save');
  const clearBtn = $('budget-limit-clear');

  if (input) {
    const current = getBudgetLimit();
    if (current != null) input.value = current.toFixed(2);
  }

  saveBtn?.addEventListener('click', () => {
    const val = parseFloat((input as HTMLInputElement)?.value ?? '');
    if (isNaN(val) || val <= 0) {
      showSettingsToast('Enter a valid budget amount', 'error');
      return;
    }
    setBudgetLimit(val);
    showSettingsToast(`Budget alert set at $${val.toFixed(2)}`, 'success');
    // Re-check immediately
    loadSettingsUsage().catch(() => {});
  });

  clearBtn?.addEventListener('click', () => {
    setBudgetLimit(null);
    if (input) (input as HTMLInputElement).value = '';
    const alertEl = $('budget-alert');
    if (alertEl) alertEl.style.display = 'none';
    showSettingsToast('Budget alert cleared', 'info');
  });
}

// ── System Presence ────────────────────────────────────────────────────────
export async function loadSettingsPresence() {
  if (!wsConnected) return;
  const section = $('settings-presence-section');
  const list = $('settings-presence-list');
  try {
    const result = await gateway.systemPresence();
    const entries = result.entries ?? [];
    if (!entries.length) { if (section) section.style.display = 'none'; return; }
    if (section) section.style.display = '';
    if (list) {
      list.innerHTML = entries.map(e => {
        const name = e.client?.id ?? e.connId ?? 'Unknown';
        const platform = e.client?.platform ?? '';
        const role = e.role ?? '';
        return `
          <div class="presence-entry">
            <div class="presence-dot online"></div>
            <div class="presence-info">
              <div class="presence-name">${escHtml(name)}</div>
              <div class="presence-meta">${escHtml(role)} · ${escHtml(platform)}${e.connectedAt ? ' · ' + new Date(e.connectedAt).toLocaleString() : ''}</div>
            </div>
          </div>
        `;
      }).join('');
    }
  } catch (e) {
    console.warn('[settings] Presence load failed:', e);
    if (section) section.style.display = 'none';
  }
}

// ── Nodes View ─────────────────────────────────────────────────────────────
export async function loadSettingsNodes() {
  if (!wsConnected) return;
  const section = $('settings-nodes-section');
  const list = $('settings-nodes-list');
  try {
    const result = await gateway.nodeList();
    const nodes = result.nodes ?? [];
    if (!nodes.length) { 
      if (section) section.style.display = 'none'; 
      return; 
    }
    if (section) section.style.display = '';
    if (list) {
      list.innerHTML = nodes.map(n => {
        const status = n.connected ? 'online' : 'offline';
        const caps = n.caps?.join(', ') || 'none';
        return `
          <div class="node-entry">
            <div class="presence-dot ${status}"></div>
            <div class="presence-info">
              <div class="presence-name">${escHtml(n.name || n.id)}</div>
              <div class="presence-meta">${escHtml(n.platform || '')} · ${escHtml(n.deviceFamily || '')} · Caps: ${escHtml(caps)}</div>
            </div>
            ${n.connected ? `<button class="btn btn-ghost btn-sm node-invoke-btn" data-node-id="${escHtml(n.id)}">Invoke</button>` : ''}
          </div>
        `;
      }).join('');
      
      // Wire invoke buttons
      list.querySelectorAll('.node-invoke-btn').forEach(btn => {
        btn.addEventListener('click', async () => {
          const nodeId = btn.getAttribute('data-node-id');
          if (!nodeId) return;
          const command = prompt('Command to invoke (e.g., camera.snap):');
          if (!command) return;
          try {
            const result = await gateway.nodeInvoke(nodeId, command);
            alert(`Result: ${JSON.stringify(result, null, 2)}`);
          } catch (e) {
            alert(`Error: ${e instanceof Error ? e.message : e}`);
          }
        });
      });
    }
  } catch (e) {
    console.warn('[settings] Nodes load failed:', e);
    if (section) section.style.display = 'none';
  }
}

// ── Device Pairing ─────────────────────────────────────────────────────────
export async function loadSettingsDevices() {
  if (!wsConnected) return;
  const section = $('settings-devices-section');
  const list = $('settings-devices-list');
  const emptyEl = $('settings-devices-empty');
  try {
    const result = await gateway.devicePairList();
    const devices = result.devices ?? [];
    if (section) section.style.display = '';
    if (!devices.length) {
      if (list) list.innerHTML = '';
      if (emptyEl) emptyEl.style.display = '';
      return;
    }
    if (emptyEl) emptyEl.style.display = 'none';
    if (list) {
      list.innerHTML = devices.map(d => {
        const name = d.name || d.id;
        const platform = d.platform || 'Unknown';
        const paired = d.pairedAt ? new Date(d.pairedAt).toLocaleDateString() : '—';

        // B3: Token age display + rotation reminder
        let tokenAgeHtml = '';
        if (d.pairedAt) {
          const ageMs = Date.now() - d.pairedAt;
          const ageDays = Math.floor(ageMs / (1000 * 60 * 60 * 24));
          let ageClass = '';
          let ageLabel = `Paired ${ageDays}d ago`;
          if (ageDays > 90) {
            ageClass = 'critical';
            ageLabel = `⚠ Token ${ageDays}d old — rotate now`;
          } else if (ageDays > 30) {
            ageClass = 'stale';
            ageLabel = `Token ${ageDays}d old — consider rotating`;
          }
          tokenAgeHtml = `<div class="device-token-age ${ageClass}">${escHtml(ageLabel)}</div>`;
        }

        return `
          <div class="device-card">
            <div class="device-card-info">
              <div class="device-card-icon">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="5" y="2" width="14" height="20" rx="2" ry="2"/><line x1="12" y1="18" x2="12.01" y2="18"/></svg>
              </div>
              <div>
                <div class="device-card-name">${escHtml(name)}</div>
                <div class="device-card-meta">${escHtml(platform)} · Paired ${escHtml(paired)}</div>
                ${tokenAgeHtml}
              </div>
            </div>
            <div class="device-card-actions">
              <button class="btn btn-ghost btn-sm device-rotate-btn" data-device-id="${escHtml(d.id)}" title="Rotate auth token">
                <svg class="icon-sm" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/></svg>
                Rotate Token
              </button>
              <button class="btn btn-danger btn-sm device-revoke-btn" data-device-id="${escHtml(d.id)}" title="Revoke device access">
                <svg class="icon-sm" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="4.93" y1="4.93" x2="19.07" y2="19.07"/></svg>
                Revoke
              </button>
            </div>
          </div>
        `;
      }).join('');

      // Wire rotate buttons
      list.querySelectorAll('.device-rotate-btn').forEach(btn => {
        btn.addEventListener('click', async () => {
          const deviceId = btn.getAttribute('data-device-id');
          if (!deviceId) return;
          try {
            const result = await gateway.deviceTokenRotate(deviceId);
            showSettingsToast(`Token rotated${result.token ? ' — new token: ' + result.token.slice(0, 8) + '…' : ''}`, 'success');
          } catch (e) {
            showSettingsToast(`Failed to rotate token: ${e instanceof Error ? e.message : e}`, 'error');
          }
        });
      });

      // Wire revoke buttons
      list.querySelectorAll('.device-revoke-btn').forEach(btn => {
        btn.addEventListener('click', async () => {
          const deviceId = btn.getAttribute('data-device-id');
          if (!deviceId) return;
          if (!confirm('Revoke access for this device? It will need to re-pair.')) return;
          try {
            await gateway.deviceTokenRevoke(deviceId);
            showSettingsToast('Device access revoked', 'success');
            loadSettingsDevices(); // Refresh list
          } catch (e) {
            showSettingsToast(`Failed to revoke: ${e instanceof Error ? e.message : e}`, 'error');
          }
        });
      });
    }
  } catch (e) {
    console.warn('[settings] Devices load failed:', e);
    if (section) section.style.display = 'none';
  }
}

// ── Onboarding Wizard ──────────────────────────────────────────────────────
export async function loadSettingsWizard() {
  if (!wsConnected) return;
  const section = $('settings-wizard-section');
  const statusEl = $('settings-wizard-status');
  const stepEl = $('settings-wizard-step');
  const startBtn = $('settings-wizard-start');
  try {
    const result = await gateway.wizardStatus();
    if (section) section.style.display = '';
    if (result.active && result.step) {
      // Wizard is in progress
      if (statusEl) statusEl.innerHTML = `<span class="wizard-badge active">Wizard active — Step: ${escHtml(result.step)}</span>`;
      if (stepEl) { stepEl.style.display = ''; }
      if (startBtn) startBtn.style.display = 'none';
      const content = $('settings-wizard-step-content');
      if (content) content.innerHTML = `<p style="color:var(--text-secondary)">Current step: <strong>${escHtml(result.step)}</strong></p>
        <p style="font-size:12px;color:var(--text-muted)">Click "Next Step" to advance, or "Cancel Wizard" to abort.</p>`;
    } else if (result.completed) {
      if (statusEl) statusEl.innerHTML = '<span class="wizard-badge completed">Setup Complete</span>';
      if (stepEl) stepEl.style.display = 'none';
      if (startBtn) startBtn.style.display = 'none';
    } else {
      if (statusEl) statusEl.innerHTML = '<span class="wizard-badge idle">Not started</span>';
      if (stepEl) stepEl.style.display = 'none';
      if (startBtn) startBtn.style.display = '';
    }
  } catch (e) {
    console.warn('[settings] Wizard status failed:', e);
    // Gateway may not support wizard — hide section
    if (section) section.style.display = 'none';
  }
}

async function startWizard() {
  try {
    const result = await gateway.wizardStart();
    showSettingsToast(`Wizard started — step: ${result.step}`, 'success');
    loadSettingsWizard();
  } catch (e) {
    showSettingsToast(`Failed to start wizard: ${e instanceof Error ? e.message : e}`, 'error');
  }
}

async function wizardNext() {
  try {
    const result = await gateway.wizardNext();
    if (result.completed) {
      showSettingsToast('Wizard completed!', 'success');
    } else if (result.step) {
      showSettingsToast(`Step: ${result.step}`, 'info');
    }
    loadSettingsWizard();
  } catch (e) {
    showSettingsToast(`Wizard step failed: ${e instanceof Error ? e.message : e}`, 'error');
  }
}

async function cancelWizard() {
  try {
    await gateway.wizardCancel();
    showSettingsToast('Wizard cancelled', 'info');
    loadSettingsWizard();
  } catch (e) {
    showSettingsToast(`Failed to cancel wizard: ${e instanceof Error ? e.message : e}`, 'error');
  }
}

// ── Self-Update ────────────────────────────────────────────────────────────
async function runUpdate() {
  const btn = $('settings-update-run') as HTMLButtonElement | null;
  if (btn) { btn.disabled = true; btn.textContent = 'Updating…'; }
  try {
    const result = await gateway.updateRun();
    if (result.updated) {
      showSettingsToast(`Updated to ${result.version ?? 'latest'}! Restart gateway for changes.`, 'success');
    } else {
      showSettingsToast('Already up to date', 'info');
    }
  } catch (e) {
    showSettingsToast(`Update failed: ${e instanceof Error ? e.message : e}`, 'error');
  } finally {
    if (btn) { btn.disabled = false; btn.textContent = 'Update OpenClaw'; }
  }
}

// ── Browser Control ────────────────────────────────────────────────────────
export async function loadSettingsBrowser() {
  if (!wsConnected) return;
  const section = $('settings-browser-section');
  const statusEl = $('settings-browser-status');
  const tabsEl = $('settings-browser-tabs');
  const startBtn = $('settings-browser-start');
  const stopBtn = $('settings-browser-stop');
  try {
    const result = await gateway.browserStatus();
    if (section) section.style.display = '';
    if (result.running) {
      if (statusEl) statusEl.innerHTML = '<span class="browser-badge running">Browser Running</span>';
      if (startBtn) startBtn.style.display = 'none';
      if (stopBtn) stopBtn.style.display = '';
      if (result.tabs?.length && tabsEl) {
        tabsEl.innerHTML = '<div style="font-size:12px;color:var(--text-muted);margin-bottom:4px">Open tabs:</div>' +
          result.tabs.map(t => `<div class="browser-tab-entry"><span class="browser-tab-title">${escHtml(t.title || t.url)}</span><span class="browser-tab-url">${escHtml(t.url)}</span></div>`).join('');
      } else if (tabsEl) {
        tabsEl.innerHTML = '';
      }
    } else {
      if (statusEl) statusEl.innerHTML = '<span class="browser-badge stopped">Browser Stopped</span>';
      if (startBtn) startBtn.style.display = '';
      if (stopBtn) stopBtn.style.display = 'none';
      if (tabsEl) tabsEl.innerHTML = '';
    }
  } catch (e) {
    console.warn('[settings] Browser status failed:', e);
    if (section) section.style.display = 'none';
  }
}

async function startBrowser() {
  const btn = $('settings-browser-start') as HTMLButtonElement | null;
  if (btn) btn.disabled = true;
  try {
    await gateway.browserStart();
    showSettingsToast('Browser started', 'success');
    loadSettingsBrowser();
  } catch (e) {
    showSettingsToast(`Browser start failed: ${e instanceof Error ? e.message : e}`, 'error');
  } finally {
    if (btn) btn.disabled = false;
  }
}

async function stopBrowser() {
  try {
    await gateway.browserStop();
    showSettingsToast('Browser stopped', 'info');
    loadSettingsBrowser();
  } catch (e) {
    showSettingsToast(`Browser stop failed: ${e instanceof Error ? e.message : e}`, 'error');
  }
}

// ── Exec Approvals Config ──────────────────────────────────────────────────
// Each tool gets a 3-way toggle row: Allow | Ask | Deny
// Tools are gathered from the existing allow/deny lists

interface ToolRule {
  name: string;
  state: 'allow' | 'ask' | 'deny';
}

let _toolRules: ToolRule[] = [];

function renderToolRules() {
  const list = $('approvals-tool-list');
  if (!list) return;

  if (_toolRules.length === 0) {
    list.innerHTML = '<div class="approvals-empty">No tool-specific rules yet. Click "Add rule" to create one.</div>';
    return;
  }

  list.innerHTML = _toolRules.map((rule, i) => `
    <div class="approvals-tool-row" data-idx="${i}">
      <div class="approvals-tool-name">${escHtml(rule.name)}</div>
      <div class="approvals-toggle-group">
        <button class="approvals-toggle-btn${rule.state === 'allow' ? ' active allow' : ''}" data-state="allow" data-idx="${i}" title="Always allow">
          <svg class="icon-xs" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="20 6 9 17 4 12"/></svg>
          Allow
        </button>
        <button class="approvals-toggle-btn${rule.state === 'ask' ? ' active ask' : ''}" data-state="ask" data-idx="${i}" title="Ask each time">
          <svg class="icon-xs" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12.01" y2="16"/><line x1="12" y1="8" x2="12" y2="12"/></svg>
          Ask
        </button>
        <button class="approvals-toggle-btn${rule.state === 'deny' ? ' active deny' : ''}" data-state="deny" data-idx="${i}" title="Always block">
          <svg class="icon-xs" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
          Block
        </button>
      </div>
      <button class="approvals-remove-btn" data-idx="${i}" title="Remove rule">
        <svg class="icon-xs" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>
      </button>
    </div>
  `).join('');

  // Wire toggle buttons
  list.querySelectorAll('.approvals-toggle-btn').forEach(btn => {
    btn.addEventListener('click', () => {
      const idx = parseInt(btn.getAttribute('data-idx') ?? '-1', 10);
      const state = btn.getAttribute('data-state') as 'allow' | 'ask' | 'deny';
      if (idx < 0 || idx >= _toolRules.length) return;
      _toolRules[idx].state = state;
      renderToolRules();
    });
  });

  // Wire remove buttons
  list.querySelectorAll('.approvals-remove-btn').forEach(btn => {
    btn.addEventListener('click', () => {
      const idx = parseInt(btn.getAttribute('data-idx') ?? '-1', 10);
      if (idx < 0 || idx >= _toolRules.length) return;
      _toolRules.splice(idx, 1);
      renderToolRules();
    });
  });
}

function addToolRule() {
  // Use the prompt modal if available, otherwise native prompt
  const promptModal = $('prompt-modal');
  const promptInput = $('prompt-modal-input') as HTMLInputElement | null;
  const promptTitle = $('prompt-modal-title');
  const promptOk = $('prompt-modal-ok');
  const promptClose = $('prompt-modal-close');
  const promptCancel = $('prompt-modal-cancel');
  if (promptModal && promptInput && promptOk && promptClose) {
    if (promptTitle) promptTitle.textContent = 'Add Tool Rule';
    promptInput.value = '';
    promptInput.placeholder = 'Tool name, e.g. brave_search';
    promptModal.style.display = 'flex';
    promptInput.focus();

    const cleanup = () => {
      promptModal.style.display = 'none';
      promptOk.removeEventListener('click', onOk);
      promptClose.removeEventListener('click', onCancel);
      if (promptCancel) promptCancel.removeEventListener('click', onCancel);
      promptInput.removeEventListener('keydown', onKey);
    };
    const onOk = () => {
      const name = promptInput.value.trim();
      cleanup();
      if (!name) return;
      if (_toolRules.some(r => r.name === name)) {
        showSettingsToast(`"${name}" already has a rule`, 'info');
        return;
      }
      _toolRules.push({ name, state: 'ask' });
      renderToolRules();
    };
    const onCancel = () => cleanup();
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Enter') onOk(); if (e.key === 'Escape') onCancel(); };
    promptOk.addEventListener('click', onOk);
    promptClose.addEventListener('click', onCancel);
    if (promptCancel) promptCancel.addEventListener('click', onCancel);
    promptInput.addEventListener('keydown', onKey);
  } else {
    const name = prompt('Tool name (e.g. brave_search):');
    if (!name?.trim()) return;
    const trimmed = name.trim();
    if (_toolRules.some(r => r.name === trimmed)) {
      showSettingsToast(`"${trimmed}" already has a rule`, 'info');
      return;
    }
    _toolRules.push({ name: trimmed, state: 'ask' });
    renderToolRules();
  }
}

export async function loadSettingsApprovals() {
  if (!wsConnected) return;
  const section = $('settings-approvals-section');
  try {
    const snapshot = await gateway.execApprovalsGet();
    if (section) section.style.display = '';

    // Build tool rules from allow + deny lists
    const allowSet = new Set(snapshot.gateway?.allow ?? []);
    const denySet = new Set(snapshot.gateway?.deny ?? []);
    const allTools = new Set([...allowSet, ...denySet]);
    _toolRules = [...allTools].map(name => ({
      name,
      state: allowSet.has(name) ? 'allow' as const : denySet.has(name) ? 'deny' as const : 'ask' as const,
    }));

    // Set the ask policy radio
    const policy = snapshot.gateway?.askPolicy ?? 'ask';
    const radio = document.querySelector(`input[name="approvals-policy"][value="${policy}"]`) as HTMLInputElement | null;
    if (radio) radio.checked = true;

    renderToolRules();
  } catch (e) {
    console.warn('[settings] Approvals load failed:', e);
    if (section) section.style.display = 'none';
  }
}

async function saveSettingsApprovals() {
  const policyRadio = document.querySelector('input[name="approvals-policy"]:checked') as HTMLInputElement | null;
  const policy = policyRadio?.value ?? 'ask';

  const allow = _toolRules.filter(r => r.state === 'allow').map(r => r.name);
  const deny = _toolRules.filter(r => r.state === 'deny').map(r => r.name);

  try {
    await gateway.execApprovalsSet({
      gateway: { allow, deny, askPolicy: policy },
    });
    showSettingsToast('Approval rules saved', 'success');
  } catch (e) {
    showSettingsToast(`Failed to save: ${e instanceof Error ? e.message : e}`, 'error');
  }
}

// ── Security Audit Dashboard ───────────────────────────────────────────────

export async function loadSecurityAudit() {
  const section = $('settings-audit-section');
  const tbody = $('audit-log-body');
  const emptyEl = $('audit-empty');
  const tableWrapper = $('audit-table-wrapper');
  if (!section || !tbody) return;

  section.style.display = '';

  const filterType = ($('audit-filter-type') as HTMLSelectElement | null)?.value || undefined;
  const filterRisk = ($('audit-filter-risk') as HTMLSelectElement | null)?.value || '';
  const limit = parseInt(($('audit-filter-limit') as HTMLSelectElement | null)?.value || '100', 10);

  try {
    const entries = await getSecurityAuditLog(limit, filterType);

    // Apply client-side risk filter if set
    const filtered = filterRisk
      ? entries.filter(e => e.risk_level === filterRisk)
      : entries;

    // Update score cards
    const denied = entries.filter(e => !e.was_allowed).length;
    const allowed = entries.filter(e => e.was_allowed).length;
    const critical = entries.filter(e => e.risk_level === 'critical').length;
    const deniedLabel = $('audit-score-denied-label');
    const allowedLabel = $('audit-score-allowed-label');
    const criticalLabel = $('audit-score-critical-label');
    if (deniedLabel) deniedLabel.textContent = `${denied} blocked`;
    if (allowedLabel) allowedLabel.textContent = `${allowed} allowed`;
    if (criticalLabel) criticalLabel.textContent = `${critical} critical`;

    if (filtered.length === 0) {
      tbody.innerHTML = '';
      if (emptyEl) emptyEl.style.display = '';
      if (tableWrapper) tableWrapper.style.display = 'none';
      return;
    }

    if (emptyEl) emptyEl.style.display = 'none';
    if (tableWrapper) tableWrapper.style.display = '';

    tbody.innerHTML = filtered.map(e => {
      const time = e.timestamp ? new Date(e.timestamp + 'Z').toLocaleString() : '—';
      const riskBadge = e.risk_level
        ? `<span class="audit-risk-badge risk-${escHtml(e.risk_level)}">${escHtml(e.risk_level)}</span>`
        : '<span class="audit-risk-badge">—</span>';
      const resultBadge = e.was_allowed
        ? '<span class="audit-result-badge allowed">✓ Allowed</span>'
        : '<span class="audit-result-badge denied">✕ Denied</span>';
      const eventLabel = e.event_type.replace(/_/g, ' ');
      return `<tr class="${e.was_allowed ? '' : 'audit-row-denied'}">
        <td class="audit-cell-time">${escHtml(time)}</td>
        <td class="audit-cell-event">${escHtml(eventLabel)}</td>
        <td>${riskBadge}</td>
        <td class="audit-cell-tool">${escHtml(e.tool_name ?? '—')}</td>
        <td class="audit-cell-detail" title="${escHtml(e.detail ?? '')}">${escHtml((e.detail ?? '').slice(0, 80))}${(e.detail?.length ?? 0) > 80 ? '…' : ''}</td>
        <td>${resultBadge}</td>
      </tr>`;
    }).join('');
  } catch (e) {
    console.warn('[settings] Audit log load failed:', e);
    if (emptyEl) { emptyEl.style.display = ''; emptyEl.textContent = `Failed to load audit log: ${e}`; }
    if (tableWrapper) tableWrapper.style.display = 'none';
  }
}

function exportAuditJSON() {
  const tbody = $('audit-log-body');
  if (!tbody) return;
  // Re-fetch and export
  const filterType = ($('audit-filter-type') as HTMLSelectElement | null)?.value || undefined;
  const limit = parseInt(($('audit-filter-limit') as HTMLSelectElement | null)?.value || '100', 10);
  getSecurityAuditLog(limit, filterType).then(entries => {
    const json = JSON.stringify(entries, null, 2);
    downloadFile('paw-security-audit.json', json, 'application/json');
  }).catch(e => showSettingsToast(`Export failed: ${e}`, 'error'));
}

function exportAuditCSV() {
  const filterType = ($('audit-filter-type') as HTMLSelectElement | null)?.value || undefined;
  const limit = parseInt(($('audit-filter-limit') as HTMLSelectElement | null)?.value || '100', 10);
  getSecurityAuditLog(limit, filterType).then(entries => {
    const headers = ['id', 'timestamp', 'event_type', 'risk_level', 'tool_name', 'command', 'detail', 'session_key', 'was_allowed', 'matched_pattern'];
    const rows = entries.map(e =>
      headers.map(h => {
        const val = (e as unknown as Record<string, unknown>)[h];
        const str = val == null ? '' : String(val);
        return `"${str.replace(/"/g, '""')}"`;
      }).join(',')
    );
    const csv = [headers.join(','), ...rows].join('\n');
    downloadFile('paw-security-audit.csv', csv, 'text/csv');
  }).catch(e => showSettingsToast(`Export failed: ${e}`, 'error'));
}

function downloadFile(filename: string, content: string, mimeType: string) {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

// ── Security Policies (local settings) ─────────────────────────────────────

export function loadSecurityPolicies() {
  const settings = loadSecuritySettings();

  const autoDenyPriv = $('sec-auto-deny-priv') as HTMLInputElement | null;
  const autoDenyCritical = $('sec-auto-deny-critical') as HTMLInputElement | null;
  const requireType = $('sec-require-type') as HTMLInputElement | null;
  const allowlistEl = $('sec-allowlist') as HTMLTextAreaElement | null;
  const denylistEl = $('sec-denylist') as HTMLTextAreaElement | null;

  if (autoDenyPriv) autoDenyPriv.checked = settings.autoDenyPrivilegeEscalation;
  if (autoDenyCritical) autoDenyCritical.checked = settings.autoDenyCritical;
  if (requireType) requireType.checked = settings.requireTypeToCritical;
  if (allowlistEl) allowlistEl.value = settings.commandAllowlist.join('\n');
  if (denylistEl) denylistEl.value = settings.commandDenylist.join('\n');
}

function saveSecurityPolicies() {
  const autoDenyPriv = ($('sec-auto-deny-priv') as HTMLInputElement | null)?.checked ?? false;
  const autoDenyCritical = ($('sec-auto-deny-critical') as HTMLInputElement | null)?.checked ?? false;
  const requireType = ($('sec-require-type') as HTMLInputElement | null)?.checked ?? true;
  const allowlistRaw = ($('sec-allowlist') as HTMLTextAreaElement | null)?.value ?? '';
  const denylistRaw = ($('sec-denylist') as HTMLTextAreaElement | null)?.value ?? '';

  const commandAllowlist = allowlistRaw.split('\n').map(l => l.trim()).filter(Boolean);
  const commandDenylist = denylistRaw.split('\n').map(l => l.trim()).filter(Boolean);

  // Validate regex patterns
  for (const p of [...commandAllowlist, ...commandDenylist]) {
    try { new RegExp(p); }
    catch {
      showSettingsToast(`Invalid regex pattern: ${p}`, 'error');
      return;
    }
  }

  const settings: SecuritySettings = {
    autoDenyPrivilegeEscalation: autoDenyPriv,
    autoDenyCritical: autoDenyCritical,
    requireTypeToCritical: requireType,
    commandAllowlist,
    commandDenylist,
  };
  saveSecuritySettings(settings);
  showSettingsToast('Security policies saved', 'success');
}

function resetSecurityPolicies() {
  localStorage.removeItem('paw_security_settings');
  loadSecurityPolicies();
  showSettingsToast('Security policies reset to defaults', 'info');
}

// ── Settings toast (inline) ────────────────────────────────────────────────
function showSettingsToast(message: string, type: 'success' | 'error' | 'info' = 'info') {
  // Try to use the global toast if available
  const toast = document.getElementById('global-toast');
  if (toast) {
    toast.textContent = message;
    toast.className = `global-toast toast-${type}`;
    toast.style.display = 'block';
    setTimeout(() => { toast.style.display = 'none'; }, 3500);
  }
}

// ── Initialize event listeners ─────────────────────────────────────────────
export function initSettings() {
  $('settings-refresh-status')?.addEventListener('click', () => loadSettingsStatus());
  $('settings-refresh-logs')?.addEventListener('click', () => loadSettingsLogs());
  $('settings-refresh-usage')?.addEventListener('click', () => loadSettingsUsage());
  $('settings-refresh-presence')?.addEventListener('click', () => loadSettingsPresence());
  $('settings-refresh-nodes')?.addEventListener('click', () => loadSettingsNodes());
  $('settings-refresh-devices')?.addEventListener('click', () => loadSettingsDevices());
  $('settings-refresh-approvals')?.addEventListener('click', () => loadSettingsApprovals());
  $('settings-save-approvals')?.addEventListener('click', () => saveSettingsApprovals());
  $('approvals-add-tool')?.addEventListener('click', () => addToolRule());
  // Wizard
  $('settings-wizard-start')?.addEventListener('click', () => startWizard());
  $('settings-wizard-next')?.addEventListener('click', () => wizardNext());
  $('settings-wizard-cancel')?.addEventListener('click', () => cancelWizard());
  // Update
  $('settings-update-run')?.addEventListener('click', () => runUpdate());
  // Browser
  $('settings-browser-start')?.addEventListener('click', () => startBrowser());
  $('settings-browser-stop')?.addEventListener('click', () => stopBrowser());
  $('settings-refresh-browser')?.addEventListener('click', () => loadSettingsBrowser());
  // Security audit
  $('audit-refresh')?.addEventListener('click', () => loadSecurityAudit());
  $('audit-export-json')?.addEventListener('click', () => exportAuditJSON());
  $('audit-export-csv')?.addEventListener('click', () => exportAuditCSV());
  $('audit-filter-type')?.addEventListener('change', () => loadSecurityAudit());
  $('audit-filter-risk')?.addEventListener('change', () => loadSecurityAudit());
  $('audit-filter-limit')?.addEventListener('change', () => loadSecurityAudit());
  // Security policies
  $('settings-save-security')?.addEventListener('click', () => saveSecurityPolicies());
  $('settings-reset-security')?.addEventListener('click', () => resetSecurityPolicies());
  // Budget
  initBudgetSettings();
}

// ── Load all settings data ─────────────────────────────────────────────────
export async function loadSettings() {
  loadSecurityPolicies(); // synchronous — reads from localStorage
  await Promise.all([
    loadSecurityAudit(),
    loadSettingsStatus(),
    loadSettingsLogs(),
    loadSettingsUsage(),
    loadSettingsPresence(),
    loadSettingsNodes(),
    loadSettingsDevices(),
    loadSettingsApprovals(),
    loadSettingsWizard(),
    loadSettingsBrowser(),
  ]);
}
