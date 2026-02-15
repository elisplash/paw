// Nodes View — Device Management
// Controls paired iOS/Android/macOS nodes (camera, screen, location, etc.)

import { gateway } from '../gateway';
import type { GatewayNode } from '../types';

const $ = (id: string) => document.getElementById(id);

// ── Module state ───────────────────────────────────────────────────────────
let wsConnected = false;
let _nodes: GatewayNode[] = [];
let _selectedNodeId: string | null = null;
let _pendingPairRequests: Array<{ id: string; nodeId: string; name?: string; requestedAt: number }> = [];

export function setWsConnected(connected: boolean) {
  wsConnected = connected;
}

// ── Helpers ────────────────────────────────────────────────────────────────
function escHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

let _toastTimer: number | null = null;
function showToast(message: string, type: 'success' | 'error' | 'info') {
  const toast = $('nodes-toast');
  if (!toast) return;
  toast.className = `nodes-toast ${type}`;
  toast.textContent = message;
  toast.style.display = 'flex';

  if (_toastTimer) clearTimeout(_toastTimer);
  _toastTimer = window.setTimeout(() => {
    toast.style.display = 'none';
    _toastTimer = null;
  }, type === 'error' ? 8000 : 4000);
}

function formatTimestamp(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleString();
}

// ── Getters ────────────────────────────────────────────────────────────────
export function getNodes(): GatewayNode[] {
  return _nodes;
}

export function getSelectedNode(): GatewayNode | null {
  return _nodes.find(n => n.id === _selectedNodeId) ?? null;
}

// ── Main loader ────────────────────────────────────────────────────────────
export async function loadNodes() {
  const list = $('nodes-list');
  const empty = $('nodes-empty');
  const loading = $('nodes-loading');
  const detail = $('nodes-detail');

  if (!wsConnected) return;

  if (loading) loading.style.display = '';
  if (empty) empty.style.display = 'none';
  if (list) list.innerHTML = '';

  try {
    const result = await gateway.nodeList();
    _nodes = result.nodes ?? [];

    if (loading) loading.style.display = 'none';

    if (!_nodes.length) {
      if (empty) empty.style.display = 'flex';
      if (detail) detail.style.display = 'none';
      return;
    }

    renderNodeList();

    // Auto-select first node if none selected
    if (!_selectedNodeId && _nodes.length) {
      selectNode(_nodes[0].id);
    } else if (_selectedNodeId) {
      // Refresh detail for currently selected
      selectNode(_selectedNodeId);
    }

  } catch (e) {
    if (loading) loading.style.display = 'none';
    console.error('[nodes] Failed to load:', e);
    showToast(`Failed to load nodes: ${e instanceof Error ? e.message : e}`, 'error');
  }
}

function renderNodeList() {
  const list = $('nodes-list');
  if (!list) return;

  list.innerHTML = '';

  for (const node of _nodes) {
    const item = document.createElement('div');
    item.className = `nodes-item${node.id === _selectedNodeId ? ' active' : ''}${node.connected ? '' : ' disconnected'}`;
    item.dataset.nodeId = node.id;

    const icon = getNodeIcon(node);
    const statusDot = node.connected ? '🟢' : '🔴';

    item.innerHTML = `
      <div class="nodes-item-icon">${icon}</div>
      <div class="nodes-item-info">
        <div class="nodes-item-name">${escHtml(node.name || node.id)}</div>
        <div class="nodes-item-status">${statusDot} ${node.connected ? 'Connected' : 'Disconnected'}</div>
      </div>
    `;

    item.addEventListener('click', () => selectNode(node.id));
    list.appendChild(item);
  }
}

function getNodeIcon(node: GatewayNode): string {
  const platform = (node.platform || node.deviceFamily || '').toLowerCase();
  if (platform.includes('ios') || platform.includes('iphone')) return '📱';
  if (platform.includes('ipad')) return '📱';
  if (platform.includes('android')) return '🤖';
  if (platform.includes('mac')) return '💻';
  if (platform.includes('windows')) return '🖥️';
  if (platform.includes('linux')) return '🐧';
  return '📟';
}

// ── Node selection & detail ────────────────────────────────────────────────
async function selectNode(nodeId: string) {
  _selectedNodeId = nodeId;

  // Update list selection
  const items = document.querySelectorAll('.nodes-item');
  items.forEach(item => {
    item.classList.toggle('active', (item as HTMLElement).dataset.nodeId === nodeId);
  });

  const detail = $('nodes-detail');
  const empty = $('nodes-empty');
  const node = _nodes.find(n => n.id === nodeId);

  if (!node) {
    if (detail) detail.style.display = 'none';
    return;
  }

  if (empty) empty.style.display = 'none';
  if (detail) detail.style.display = '';

  // Fetch detailed info
  try {
    const info = await gateway.nodeDescribe(nodeId);
    renderNodeDetail(node, info);
  } catch (e) {
    // Fall back to basic info
    renderNodeDetail(node, null);
  }
}

function renderNodeDetail(node: GatewayNode, info: { node: GatewayNode; caps?: string[]; commands?: string[] } | null) {
  const detail = $('nodes-detail');
  if (!detail) return;

  const caps = info?.caps ?? node.caps ?? [];
  const commands = info?.commands ?? node.commands ?? [];
  const statusDot = node.connected ? '🟢' : '🔴';

  detail.innerHTML = `
    <div class="nodes-detail-header">
      <div class="nodes-detail-icon">${getNodeIcon(node)}</div>
      <div class="nodes-detail-title">
        <h3>${escHtml(node.name || node.id)}</h3>
        <div class="nodes-detail-status">${statusDot} ${node.connected ? 'Connected' : 'Disconnected'}</div>
      </div>
      <button class="nodes-rename-btn" data-node-id="${node.id}" title="Rename">✏️</button>
    </div>

    <div class="nodes-detail-meta">
      <div class="nodes-meta-item">
        <span class="nodes-meta-label">ID</span>
        <span class="nodes-meta-value">${escHtml(node.id)}</span>
      </div>
      ${node.platform ? `
      <div class="nodes-meta-item">
        <span class="nodes-meta-label">Platform</span>
        <span class="nodes-meta-value">${escHtml(node.platform)}</span>
      </div>
      ` : ''}
      ${node.deviceFamily ? `
      <div class="nodes-meta-item">
        <span class="nodes-meta-label">Device</span>
        <span class="nodes-meta-value">${escHtml(node.deviceFamily)}</span>
      </div>
      ` : ''}
      ${node.modelIdentifier ? `
      <div class="nodes-meta-item">
        <span class="nodes-meta-label">Model</span>
        <span class="nodes-meta-value">${escHtml(node.modelIdentifier)}</span>
      </div>
      ` : ''}
    </div>

    ${caps.length ? `
    <div class="nodes-detail-section">
      <h4>Capabilities</h4>
      <div class="nodes-caps-list">
        ${caps.map(c => `<span class="nodes-cap-badge">${escHtml(c)}</span>`).join('')}
      </div>
    </div>
    ` : ''}

    ${commands.length ? `
    <div class="nodes-detail-section">
      <h4>Commands</h4>
      <div class="nodes-commands-grid">
        ${renderCommandButtons(node, commands)}
      </div>
    </div>
    ` : ''}

    ${!node.connected ? `
    <div class="nodes-offline-notice">
      <p>This node is currently offline. Commands will be unavailable until it reconnects.</p>
    </div>
    ` : ''}
  `;

  // Wire up rename button
  detail.querySelector('.nodes-rename-btn')?.addEventListener('click', () => promptRenameNode(node.id));

  // Wire up command buttons
  detail.querySelectorAll('.nodes-cmd-btn').forEach(btn => {
    btn.addEventListener('click', () => {
      const cmd = (btn as HTMLElement).dataset.command;
      if (cmd) invokeCommand(node.id, cmd);
    });
  });
}

function renderCommandButtons(node: GatewayNode, commands: string[]): string {
  const commandMeta: Record<string, { icon: string; label: string }> = {
    'camera.snap': { icon: '📷', label: 'Take Photo' },
    'camera.list': { icon: '📹', label: 'List Cameras' },
    'camera.clip': { icon: '🎬', label: 'Record Clip' },
    'screen.record': { icon: '🖥️', label: 'Record Screen' },
    'location.get': { icon: '📍', label: 'Get Location' },
    'notify': { icon: '🔔', label: 'Send Notification' },
    'clipboard.get': { icon: '📋', label: 'Get Clipboard' },
    'clipboard.set': { icon: '📝', label: 'Set Clipboard' },
  };

  return commands.map(cmd => {
    const meta = commandMeta[cmd] ?? { icon: '⚡', label: cmd };
    const disabled = !node.connected ? 'disabled' : '';
    return `<button class="nodes-cmd-btn" data-command="${escHtml(cmd)}" ${disabled}>
      <span class="nodes-cmd-icon">${meta.icon}</span>
      <span class="nodes-cmd-label">${escHtml(meta.label)}</span>
    </button>`;
  }).join('');
}

// ── Commands ───────────────────────────────────────────────────────────────
async function invokeCommand(nodeId: string, command: string) {
  const node = _nodes.find(n => n.id === nodeId);
  if (!node?.connected) {
    showToast('Node is offline', 'error');
    return;
  }

  showToast(`Running ${command}...`, 'info');

  try {
    const result = await gateway.nodeInvoke(nodeId, command);
    console.log(`[nodes] ${command} result:`, result);

    // Handle specific command results
    if (command === 'camera.snap' && result) {
      handleCameraSnapResult(result);
    } else if (command === 'location.get' && result) {
      handleLocationResult(result);
    } else {
      showToast(`${command} completed`, 'success');
    }
  } catch (e) {
    console.error(`[nodes] ${command} failed:`, e);
    showToast(`${command} failed: ${e instanceof Error ? e.message : e}`, 'error');
  }
}

function handleCameraSnapResult(result: unknown) {
  const r = result as { path?: string; url?: string; data?: string };
  if (r.path || r.url || r.data) {
    showToast('Photo captured!', 'success');
    // Could show a preview modal here
    if (_onCommandResult) _onCommandResult('camera.snap', result);
  } else {
    showToast('Photo captured', 'success');
  }
}

function handleLocationResult(result: unknown) {
  const r = result as { latitude?: number; longitude?: number; accuracy?: number };
  if (r.latitude != null && r.longitude != null) {
    showToast(`Location: ${r.latitude.toFixed(4)}, ${r.longitude.toFixed(4)}`, 'success');
    if (_onCommandResult) _onCommandResult('location.get', result);
  } else {
    showToast('Location retrieved', 'success');
  }
}

// ── Rename ─────────────────────────────────────────────────────────────────
async function promptRenameNode(nodeId: string) {
  const node = _nodes.find(n => n.id === nodeId);
  if (!node) return;

  const newName = prompt('Enter new name for this node:', node.name || '');
  if (newName === null) return; // cancelled
  if (newName === node.name) return; // no change

  try {
    await gateway.nodeRename(nodeId, newName);
    showToast('Node renamed', 'success');
    await loadNodes(); // refresh
  } catch (e) {
    showToast(`Rename failed: ${e instanceof Error ? e.message : e}`, 'error');
  }
}

// ── Pairing requests ───────────────────────────────────────────────────────
export async function loadPairingRequests() {
  try {
    const result = await gateway.nodePairList();
    _pendingPairRequests = result.requests ?? [];
    renderPairingRequests();
  } catch (e) {
    console.error('[nodes] Failed to load pairing requests:', e);
  }
}

function renderPairingRequests() {
  const container = $('nodes-pairing-requests');
  if (!container) return;

  if (!_pendingPairRequests.length) {
    container.style.display = 'none';
    return;
  }

  container.style.display = '';
  container.innerHTML = `
    <h4>Pending Pairing Requests</h4>
    <div class="nodes-pairing-list">
      ${_pendingPairRequests.map(req => `
        <div class="nodes-pairing-item" data-request-id="${req.id}">
          <div class="nodes-pairing-info">
            <div class="nodes-pairing-name">${escHtml(req.name || req.nodeId)}</div>
            <div class="nodes-pairing-time">Requested ${formatTimestamp(req.requestedAt)}</div>
          </div>
          <div class="nodes-pairing-actions">
            <button class="nodes-pair-approve" data-request-id="${req.id}">✓ Approve</button>
            <button class="nodes-pair-reject" data-request-id="${req.id}">✕ Reject</button>
          </div>
        </div>
      `).join('')}
    </div>
  `;

  // Wire up buttons
  container.querySelectorAll('.nodes-pair-approve').forEach(btn => {
    btn.addEventListener('click', () => approvePairing((btn as HTMLElement).dataset.requestId!));
  });
  container.querySelectorAll('.nodes-pair-reject').forEach(btn => {
    btn.addEventListener('click', () => rejectPairing((btn as HTMLElement).dataset.requestId!));
  });
}

async function approvePairing(requestId: string) {
  try {
    await gateway.nodePairApprove(requestId);
    showToast('Pairing approved', 'success');
    await loadPairingRequests();
    await loadNodes();
  } catch (e) {
    showToast(`Approve failed: ${e instanceof Error ? e.message : e}`, 'error');
  }
}

async function rejectPairing(requestId: string) {
  try {
    await gateway.nodePairReject(requestId);
    showToast('Pairing rejected', 'info');
    await loadPairingRequests();
  } catch (e) {
    showToast(`Reject failed: ${e instanceof Error ? e.message : e}`, 'error');
  }
}

// ── Event handlers ─────────────────────────────────────────────────────────
export function initNodesEvents() {
  // Refresh button
  $('nodes-refresh')?.addEventListener('click', () => {
    loadNodes();
    loadPairingRequests();
  });
}

// ── Gateway event listeners (called from main.ts) ──────────────────────────
export function handleNodePairRequested(payload: unknown) {
  const p = payload as { id: string; nodeId: string; name?: string };
  showToast(`New pairing request from ${p.name || p.nodeId}`, 'info');
  loadPairingRequests();
}

export function handleNodePairResolved(payload: unknown) {
  const p = payload as { nodeId: string; approved: boolean };
  if (p.approved) {
    showToast('Node paired successfully', 'success');
    loadNodes();
  }
  loadPairingRequests();
}

// ── Callbacks ──────────────────────────────────────────────────────────────
let _onCommandResult: ((command: string, result: unknown) => void) | null = null;

export function configureCallbacks(opts: {
  onCommandResult?: (command: string, result: unknown) => void;
}) {
  if (opts.onCommandResult) _onCommandResult = opts.onCommandResult;
}
