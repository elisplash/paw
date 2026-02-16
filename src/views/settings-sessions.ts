// Settings: Sessions
// List sessions, preview, patch overrides, reset/delete, compact
// ~250 lines

import { gateway } from '../gateway';
import { showToast } from '../components/toast';
import {
  getConfig, patchConfig, getVal, isConnected,
  esc, formRow, selectInput, saveReloadButtons
} from './settings-config';
import type { Session } from '../types';

const $ = (id: string) => document.getElementById(id);

// ── Option enums ────────────────────────────────────────────────────────────

const THINKING_OPTS = [
  { value: '', label: '(inherit default)' },
  { value: 'off', label: 'Off' }, { value: 'minimal', label: 'Minimal' },
  { value: 'low', label: 'Low' }, { value: 'medium', label: 'Medium' },
  { value: 'high', label: 'High' }, { value: 'xhigh', label: 'Extra High' },
];

const VERBOSE_OPTS = [
  { value: '', label: '(inherit default)' },
  { value: 'off', label: 'Off' }, { value: 'on', label: 'On' }, { value: 'full', label: 'Full' },
];

const ELEVATED_OPTS = [
  { value: '', label: '(inherit default)' },
  { value: 'off', label: 'Off' }, { value: 'on', label: 'On' },
  { value: 'ask', label: 'Ask' }, { value: 'full', label: 'Full' },
];

// ── State ───────────────────────────────────────────────────────────────────

let _sessions: Session[] = [];

// ── Render session list ─────────────────────────────────────────────────────

export async function loadSessionsSettings() {
  if (!isConnected()) return;
  const container = $('settings-sessions-content');
  if (!container) return;
  container.innerHTML = '<p style="color:var(--text-muted)">Loading sessions…</p>';

  try {
    const result = await gateway.listSessions({
      limit: 100,
      includeGlobal: true,
      includeUnknown: true,
      includeDerivedTitles: true,
      includeLastMessage: true,
    });
    _sessions = result.sessions ?? [];

    container.innerHTML = '';

    // ── Toolbar ──────────────────────────────────────────────────────────
    const toolbar = document.createElement('div');
    toolbar.style.cssText = 'display:flex; justify-content:space-between; align-items:center; margin-bottom:12px; flex-wrap:wrap; gap:8px';

    const info = document.createElement('span');
    info.style.color = 'var(--text-muted)';
    info.textContent = `${_sessions.length} session${_sessions.length !== 1 ? 's' : ''}`;
    toolbar.appendChild(info);

    const acts = document.createElement('div');
    acts.style.cssText = 'display:flex; gap:6px';

    const compactAllBtn = document.createElement('button');
    compactAllBtn.className = 'btn btn-sm';
    compactAllBtn.textContent = 'Compact All';
    compactAllBtn.title = 'Remove redundant messages from all sessions';
    compactAllBtn.onclick = async () => {
      try {
        await gateway.sessionsCompact();
        showToast('All sessions compacted', 'success');
        loadSessionsSettings();
      } catch (e: any) { showToast(e.message || String(e), 'error'); }
    };
    acts.appendChild(compactAllBtn);

    const refreshBtn = document.createElement('button');
    refreshBtn.className = 'btn btn-sm';
    refreshBtn.textContent = 'Refresh';
    refreshBtn.onclick = () => loadSessionsSettings();
    acts.appendChild(refreshBtn);

    toolbar.appendChild(acts);
    container.appendChild(toolbar);

    // ── Global session config ────────────────────────────────────────────
    try {
      const config = await getConfig();
      const sessConf = (getVal(config, 'sessions') ?? {}) as Record<string, any>;
      const globalSection = document.createElement('div');
      globalSection.className = 'settings-card';
      globalSection.style.cssText = 'margin-bottom:16px; padding:12px; border:1px solid var(--border-color); border-radius:8px';
      globalSection.innerHTML = '<h4 style="margin:0 0 8px">Session Defaults</h4>';

      const maxRow = formRow('Max Session Turns', 'Auto-compact sessions exceeding this turn count');
      const maxInp = document.createElement('input');
      maxInp.type = 'number'; maxInp.className = 'input'; maxInp.min = '0';
      maxInp.value = String(sessConf.maxTurns ?? '');
      maxInp.placeholder = '50'; maxInp.style.maxWidth = '120px';
      maxRow.appendChild(maxInp);
      globalSection.appendChild(maxRow);

      const ttlRow = formRow('Session TTL (minutes)', 'Auto-expire inactive sessions after N minutes');
      const ttlInp = document.createElement('input');
      ttlInp.type = 'number'; ttlInp.className = 'input'; ttlInp.min = '0';
      ttlInp.value = String(sessConf.ttlMinutes ?? '');
      ttlInp.placeholder = '0 (no expiry)'; ttlInp.style.maxWidth = '160px';
      ttlRow.appendChild(ttlInp);
      globalSection.appendChild(ttlRow);

      globalSection.appendChild(saveReloadButtons(
        async () => {
          const patch: Record<string, unknown> = {};
          if (maxInp.value) patch.maxTurns = parseInt(maxInp.value);
          if (ttlInp.value) patch.ttlMinutes = parseInt(ttlInp.value);
          await patchConfig({ sessions: patch });
        },
        () => loadSessionsSettings()
      ));
      container.appendChild(globalSection);
    } catch (_) { /* config read may fail — skip global section */ }

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

  } catch (e: any) {
    container.innerHTML = `<p style="color:var(--danger)">Failed to load sessions: ${esc(String(e))}</p>`;
  }
}

// ── Session card ────────────────────────────────────────────────────────────

function buildSessionCard(sess: Session): HTMLElement {
  const card = document.createElement('div');
  card.className = 'settings-card';
  card.style.cssText = 'margin-bottom:10px; padding:12px; border:1px solid var(--border-color); border-radius:8px';

  const ts = sess.updatedAt ? new Date(sess.updatedAt * 1000).toLocaleString() : '—';
  const label = sess.displayName || sess.label || sess.key;

  // Header row
  const header = document.createElement('div');
  header.style.cssText = 'display:flex; justify-content:space-between; align-items:start; gap:8px; flex-wrap:wrap';
  header.innerHTML = `
    <div style="flex:1; min-width:200px">
      <strong style="font-size:14px">${esc(label)}</strong>
      <div style="color:var(--text-muted); font-size:12px; margin-top:2px">
        <span class="badge" style="font-size:11px">${esc(sess.kind)}</span>
        ${sess.channel ? `<span class="badge" style="font-size:11px; margin-left:4px">${esc(sess.channel)}</span>` : ''}
        <span style="margin-left:6px">${esc(ts)}</span>
      </div>
      <div style="color:var(--text-muted); font-size:11px; margin-top:2px; font-family:monospace">${esc(sess.key)}</div>
    </div>
  `;
  card.appendChild(header);

  // ── Overrides (expandable) ─────────────────────────────────────────────
  const details = document.createElement('details');
  details.style.marginTop = '8px';
  const summary = document.createElement('summary');
  summary.style.cssText = 'cursor:pointer; color:var(--text-muted); font-size:13px';
  summary.textContent = 'Session Overrides & Actions';
  details.appendChild(summary);

  const overridesBody = document.createElement('div');
  overridesBody.style.cssText = 'margin-top:8px; display:flex; flex-direction:column; gap:6px';

  // Thinking override
  const thinkRow = formRow('Thinking');
  const thinkSel = selectInput(THINKING_OPTS, sess.thinkingLevel ?? '');
  thinkSel.style.maxWidth = '180px';
  thinkRow.appendChild(thinkSel);
  overridesBody.appendChild(thinkRow);

  // Verbose override
  const verbRow = formRow('Verbose');
  const verbSel = selectInput(VERBOSE_OPTS, '');
  verbSel.style.maxWidth = '180px';
  verbRow.appendChild(verbSel);
  overridesBody.appendChild(verbRow);

  // Elevated override
  const elevRow = formRow('Elevated');
  const elevSel = selectInput(ELEVATED_OPTS, '');
  elevSel.style.maxWidth = '180px';
  elevRow.appendChild(elevSel);
  overridesBody.appendChild(elevRow);

  // Actions row
  const actRow = document.createElement('div');
  actRow.style.cssText = 'display:flex; gap:6px; margin-top:8px; flex-wrap:wrap';

  const patchBtn = document.createElement('button');
  patchBtn.className = 'btn btn-sm btn-primary';
  patchBtn.textContent = 'Save Overrides';
  patchBtn.onclick = async () => {
    try {
      const patch: Record<string, unknown> = {};
      if (thinkSel.value) patch.thinkingLevel = thinkSel.value;
      if (verbSel.value) patch.verbose = verbSel.value;
      if (elevSel.value) patch.elevated = elevSel.value;
      await gateway.patchSession(sess.key, patch);
      showToast(`Session "${label}" updated`, 'success');
    } catch (e: any) { showToast(e.message || String(e), 'error'); }
  };
  actRow.appendChild(patchBtn);

  const compactBtn = document.createElement('button');
  compactBtn.className = 'btn btn-sm';
  compactBtn.textContent = 'Compact';
  compactBtn.onclick = async () => {
    try {
      await gateway.sessionsCompact(sess.key);
      showToast(`Session "${label}" compacted`, 'success');
    } catch (e: any) { showToast(e.message || String(e), 'error'); }
  };
  actRow.appendChild(compactBtn);

  const resetBtn = document.createElement('button');
  resetBtn.className = 'btn btn-sm';
  resetBtn.textContent = 'Reset';
  resetBtn.style.color = 'var(--warning)';
  resetBtn.onclick = async () => {
    if (!confirm(`Reset session "${label}"? This clears history but keeps the session.`)) return;
    try {
      await gateway.resetSession(sess.key);
      showToast(`Session "${label}" reset`, 'success');
      loadSessionsSettings();
    } catch (e: any) { showToast(e.message || String(e), 'error'); }
  };
  actRow.appendChild(resetBtn);

  const deleteBtn = document.createElement('button');
  deleteBtn.className = 'btn btn-sm';
  deleteBtn.textContent = 'Delete';
  deleteBtn.style.color = 'var(--danger)';
  deleteBtn.onclick = async () => {
    if (!confirm(`Delete session "${label}"? This is permanent.`)) return;
    try {
      await gateway.deleteSession(sess.key);
      showToast(`Session "${label}" deleted`, 'success');
      card.remove();
      _sessions = _sessions.filter(s => s.key !== sess.key);
    } catch (e: any) { showToast(e.message || String(e), 'error'); }
  };
  actRow.appendChild(deleteBtn);

  overridesBody.appendChild(actRow);
  details.appendChild(overridesBody);
  card.appendChild(details);
  return card;
}

export function initSessionsSettings() {
  // All dynamic — no static elements to bind
}
