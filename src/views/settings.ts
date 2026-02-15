// Settings View — Logs, Usage, Presence, Nodes, Devices, Exec Approvals
// Extracted from main.ts for maintainability

import { gateway } from '../gateway';

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
export async function loadSettingsUsage() {
  if (!wsConnected) return;
  const section = $('settings-usage-section');
  const content = $('settings-usage-content');
  try {
    const [status, cost] = await Promise.all([
      gateway.usageStatus().catch(() => null),
      gateway.usageCost().catch(() => null),
    ]);
    if (!status && !cost) { if (section) section.style.display = 'none'; return; }
    if (section) section.style.display = '';
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
        <div class="usage-card-label">Cost</div>
        <div class="usage-card-value">$${cost.totalCost.toFixed(4)} ${cost.currency ?? ''}</div>
      </div>`;
    }
    if (status?.byModel) {
      html += '<div class="usage-models"><h4>By Model</h4>';
      for (const [model, data] of Object.entries(status.byModel)) {
        const d = data as { requests?: number; tokens?: number };
        html += `<div class="usage-model-row"><span class="usage-model-name">${escHtml(model)}</span><span>${(d.requests ?? 0).toLocaleString()} req / ${(d.tokens ?? 0).toLocaleString()} tok</span></div>`;
      }
      html += '</div>';
    }
    if (content) content.innerHTML = html || '<p style="color:var(--text-muted)">No usage data</p>';
  } catch (e) {
    console.warn('[settings] Usage load failed:', e);
    if (section) section.style.display = 'none';
  }
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
        return `
          <div class="device-card">
            <div class="device-card-info">
              <div class="device-card-icon">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><rect x="5" y="2" width="14" height="20" rx="2" ry="2"/><line x1="12" y1="18" x2="12.01" y2="18"/></svg>
              </div>
              <div>
                <div class="device-card-name">${escHtml(name)}</div>
                <div class="device-card-meta">${escHtml(platform)} · Paired ${escHtml(paired)}</div>
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
  $('settings-refresh-logs')?.addEventListener('click', () => loadSettingsLogs());
  $('settings-refresh-usage')?.addEventListener('click', () => loadSettingsUsage());
  $('settings-refresh-presence')?.addEventListener('click', () => loadSettingsPresence());
  $('settings-refresh-nodes')?.addEventListener('click', () => loadSettingsNodes());
  $('settings-refresh-devices')?.addEventListener('click', () => loadSettingsDevices());
  $('settings-refresh-approvals')?.addEventListener('click', () => loadSettingsApprovals());
  $('settings-save-approvals')?.addEventListener('click', () => saveSettingsApprovals());
  $('approvals-add-tool')?.addEventListener('click', () => addToolRule());
}

// ── Load all settings data ─────────────────────────────────────────────────
export async function loadSettings() {
  await Promise.all([
    loadSettingsLogs(),
    loadSettingsUsage(),
    loadSettingsPresence(),
    loadSettingsNodes(),
    loadSettingsDevices(),
    loadSettingsApprovals(),
  ]);
}
