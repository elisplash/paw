// Settings View — Logs, Usage, Presence, Nodes
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

// ── Initialize event listeners ─────────────────────────────────────────────
export function initSettings() {
  $('settings-refresh-logs')?.addEventListener('click', () => loadSettingsLogs());
  $('settings-refresh-usage')?.addEventListener('click', () => loadSettingsUsage());
  $('settings-refresh-presence')?.addEventListener('click', () => loadSettingsPresence());
  $('settings-refresh-nodes')?.addEventListener('click', () => loadSettingsNodes());
}

// ── Load all settings data ─────────────────────────────────────────────────
export async function loadSettings() {
  await Promise.all([
    loadSettingsLogs(),
    loadSettingsUsage(),
    loadSettingsPresence(),
    loadSettingsNodes(),
  ]);
}
