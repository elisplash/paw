// Settings: Sessions — DOM rendering + IPC

import { pawEngine, type EngineSession } from '../../engine';
import { showToast } from '../../components/toast';
import { isConnected } from '../../state/connection';
import { esc } from '../settings-config';
import { $ } from '../../components/helpers';

// ── Internal state ──────────────────────────────────────────────────────────

let _sessions: EngineSession[] = [];

// ── Render session list ─────────────────────────────────────────────────────

export async function loadSessionsSettings() {
  if (!isConnected()) return;
  const container = $('settings-sessions-content');
  if (!container) return;
  container.innerHTML = '<p style="color:var(--text-muted)">Loading sessions…</p>';

  try {
    _sessions = await pawEngine.sessionsList(100);

    container.innerHTML = '';

    // ── Toolbar ──────────────────────────────────────────────────────────
    const toolbar = document.createElement('div');
    toolbar.style.cssText =
      'display:flex; justify-content:space-between; align-items:center; margin-bottom:12px; flex-wrap:wrap; gap:8px';

    const info = document.createElement('span');
    info.style.color = 'var(--text-muted)';
    info.textContent = `${_sessions.length} session${_sessions.length !== 1 ? 's' : ''}`;
    toolbar.appendChild(info);

    const acts = document.createElement('div');
    acts.style.cssText = 'display:flex; gap:6px';

    const refreshBtn = document.createElement('button');
    refreshBtn.className = 'btn btn-sm';
    refreshBtn.textContent = 'Refresh';
    refreshBtn.onclick = () => loadSessionsSettings();
    acts.appendChild(refreshBtn);

    toolbar.appendChild(acts);
    container.appendChild(toolbar);

    // ── Session cards ────────────────────────────────────────────────────
    if (_sessions.length === 0) {
      const empty = document.createElement('p');
      empty.style.color = 'var(--text-muted)';
      empty.textContent = 'No sessions found.';
      container.appendChild(empty);
      return;
    }

    for (const sess of _sessions) {
      container.appendChild(buildSessionCard(sess));
    }
  } catch (e: unknown) {
    container.innerHTML = `<p style="color:var(--danger)">Failed to load sessions: ${esc(e instanceof Error ? e.message : String(e))}</p>`;
  }
}

// ── Session card ────────────────────────────────────────────────────────────

function buildSessionCard(sess: EngineSession): HTMLElement {
  const card = document.createElement('div');
  card.className = 'settings-card';
  card.style.cssText =
    'margin-bottom:10px; padding:12px; border:1px solid var(--border-color); border-radius:8px';

  const ts = sess.updated_at ? new Date(sess.updated_at).toLocaleString() : '—';
  const label = sess.label || (sess.message_count > 0 ? 'Untitled chat' : 'Empty session');

  // Header row
  const header = document.createElement('div');
  header.style.cssText =
    'display:flex; justify-content:space-between; align-items:start; gap:8px; flex-wrap:wrap';
  header.innerHTML = `
    <div style="flex:1; min-width:200px">
      <strong style="font-size:14px">${esc(label)}</strong>
      <div style="color:var(--text-muted); font-size:12px; margin-top:2px">
        <span class="badge" style="font-size:11px">${esc(sess.model)}</span>
        <span style="margin-left:6px">${sess.message_count} messages</span>
        <span style="margin-left:6px">${esc(ts)}</span>
      </div>
      <div style="color:var(--text-muted); font-size:11px; margin-top:2px; font-family:monospace">${esc(sess.id)}</div>
    </div>
  `;
  card.appendChild(header);

  // ── Actions (expandable) ───────────────────────────────────────────────
  const details = document.createElement('details');
  details.style.marginTop = '8px';
  const summary = document.createElement('summary');
  summary.style.cssText = 'cursor:pointer; color:var(--text-muted); font-size:13px';
  summary.textContent = 'Actions';
  details.appendChild(summary);

  const actionsBody = document.createElement('div');
  actionsBody.style.cssText = 'margin-top:8px; display:flex; flex-direction:column; gap:6px';

  // Rename
  const renameRow = document.createElement('div');
  renameRow.style.cssText = 'display:flex;gap:6px;align-items:center';
  const renameInp = document.createElement('input');
  renameInp.type = 'text';
  renameInp.className = 'form-input';
  renameInp.value = sess.label || '';
  renameInp.placeholder = 'Session label';
  renameInp.style.cssText = 'flex:1;max-width:280px;font-size:13px';
  const renameBtn = document.createElement('button');
  renameBtn.className = 'btn btn-sm btn-primary';
  renameBtn.textContent = 'Rename';
  renameBtn.onclick = async () => {
    try {
      await pawEngine.sessionRename(sess.id, renameInp.value.trim());
      showToast(`Session renamed`, 'success');
      loadSessionsSettings();
    } catch (e: unknown) {
      showToast(e instanceof Error ? e.message : String(e), 'error');
    }
  };
  renameRow.appendChild(renameInp);
  renameRow.appendChild(renameBtn);
  actionsBody.appendChild(renameRow);

  // Action buttons row
  const actRow = document.createElement('div');
  actRow.style.cssText = 'display:flex; gap:6px; margin-top:4px; flex-wrap:wrap';

  const clearBtn = document.createElement('button');
  clearBtn.className = 'btn btn-sm';
  clearBtn.textContent = 'Clear Messages';
  clearBtn.style.color = 'var(--warning)';
  let clearPending = false;
  clearBtn.onclick = async () => {
    if (!clearPending) {
      clearPending = true;
      clearBtn.textContent = 'Confirm Clear?';
      setTimeout(() => {
        if (clearPending) {
          clearPending = false;
          clearBtn.textContent = 'Clear Messages';
        }
      }, 4000);
      return;
    }
    clearPending = false;
    try {
      await pawEngine.sessionClear(sess.id);
      showToast(`Session "${label}" cleared`, 'success');
      loadSessionsSettings();
    } catch (e: unknown) {
      showToast(e instanceof Error ? e.message : String(e), 'error');
    }
  };
  actRow.appendChild(clearBtn);

  const deleteBtn = document.createElement('button');
  deleteBtn.className = 'btn btn-sm';
  deleteBtn.textContent = 'Delete';
  deleteBtn.style.color = 'var(--danger)';
  let deletePending = false;
  deleteBtn.onclick = async () => {
    if (!deletePending) {
      deletePending = true;
      deleteBtn.textContent = 'Confirm Delete?';
      deleteBtn.style.fontWeight = 'bold';
      setTimeout(() => {
        if (deletePending) {
          deletePending = false;
          deleteBtn.textContent = 'Delete';
          deleteBtn.style.fontWeight = '';
        }
      }, 4000);
      return;
    }
    deletePending = false;
    try {
      await pawEngine.sessionDelete(sess.id);
      showToast(`Session "${label}" deleted`, 'success');
      card.remove();
      _sessions = _sessions.filter((s) => s.id !== sess.id);
    } catch (e: unknown) {
      showToast(e instanceof Error ? e.message : String(e), 'error');
    }
  };
  actRow.appendChild(deleteBtn);

  actionsBody.appendChild(actRow);
  details.appendChild(actionsBody);
  card.appendChild(details);
  return card;
}
