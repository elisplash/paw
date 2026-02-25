// src/components/chat-mission-panel.ts — Chat Mission Control Panel
//
// Updates the mission side panel with live session data:
// context gauge, session metrics, active jobs, signal flashes.
// Called from chat_controller when token/state changes occur.

import { progressRing } from './molecules/data-viz';
import { kineticRow, kineticDot } from './kinetic-row';

// ── Shared app state reference ─────────────────────────────────────────
// We read from the global appState rather than importing it to avoid
// circular deps with the chat controller.

function $(id: string): HTMLElement | null {
  return document.getElementById(id);
}

// ── Context Gauge ──────────────────────────────────────────────────────

export function updateMissionGauge(
  tokensUsed: number,
  contextLimit: number,
): void {
  const gauge = $('mission-ctx-gauge');
  const usedEl = $('mission-ctx-used');
  const limitEl = $('mission-ctx-limit');
  if (!gauge) return;

  const pct = contextLimit > 0
    ? Math.min((tokensUsed / contextLimit) * 100, 100)
    : 0;

  const color =
    pct >= 80 ? 'var(--error)' :
    pct >= 60 ? 'var(--warning)' :
    'var(--accent)';

  gauge.innerHTML = progressRing(pct, color, 56);

  if (usedEl) {
    usedEl.textContent = fmtK(tokensUsed);
    usedEl.style.color = pct >= 80 ? 'var(--error)' : pct >= 60 ? 'var(--warning)' : '';
  }
  if (limitEl) {
    limitEl.textContent = fmtK(contextLimit);
  }
}

// ── Session Metrics ────────────────────────────────────────────────────

export function updateMissionMetrics(data: {
  inputTokens: number;
  outputTokens: number;
  cost: number;
  messageCount: number;
}): void {
  const inEl = $('mission-tokens-in');
  const outEl = $('mission-tokens-out');
  const costEl = $('mission-cost');
  const msgsEl = $('mission-msgs');

  if (inEl) inEl.textContent = fmtK(data.inputTokens);
  if (outEl) outEl.textContent = fmtK(data.outputTokens);
  if (costEl) costEl.textContent = data.cost > 0 ? `$${data.cost.toFixed(4)}` : '$0';
  if (msgsEl) msgsEl.textContent = `${data.messageCount}`;
}

// ── Active Jobs ────────────────────────────────────────────────────────

interface ActiveJob {
  name: string;
  startedAt: number; // timestamp ms
}

const _activeJobs: ActiveJob[] = [];

export function addActiveJob(name: string): void {
  _activeJobs.push({ name, startedAt: Date.now() });
  renderActiveJobs();
}

export function clearActiveJobs(): void {
  _activeJobs.length = 0;
  renderActiveJobs();
}

export function removeActiveJob(name: string): void {
  const idx = _activeJobs.findIndex((j) => j.name === name);
  if (idx >= 0) _activeJobs.splice(idx, 1);
  renderActiveJobs();
}

function renderActiveJobs(): void {
  const list = $('mission-jobs-list');
  const badge = $('mission-jobs-count');
  const badgeWrap = $('mission-jobs-badge');
  if (!list) return;

  if (badge) badge.textContent = `${_activeJobs.length}`;

  if (badgeWrap) {
    if (_activeJobs.length > 0) {
      badgeWrap.classList.add('k-breathe', 'k-status-healthy');
      badgeWrap.classList.remove('k-status-idle');
    } else {
      badgeWrap.classList.remove('k-status-healthy');
      badgeWrap.classList.add('k-status-idle');
    }
  }

  if (_activeJobs.length === 0) {
    list.innerHTML = `
      <div class="mission-jobs-empty">
        <span class="ms" style="font-size:16px;color:var(--text-muted)">hourglass_empty</span>
        <span>Waiting for activity…</span>
      </div>`;
    return;
  }

  list.innerHTML = _activeJobs.map((job) => {
    const elapsed = Math.round((Date.now() - job.startedAt) / 1000);
    const timeStr = elapsed < 60 ? `${elapsed}s` : `${Math.floor(elapsed / 60)}m${elapsed % 60}s`;
    return `
      <div class="mission-job-item k-row k-breathe k-status-healthy k-materialise">
        ${kineticDot()}
        <span class="mission-job-name">${escHtml(job.name)}</span>
        <span class="mission-job-time">${timeStr}</span>
      </div>`;
  }).join('');
}

// ── Composite update — call from chat_controller.updateTokenMeter ──

export function refreshMissionPanel(state: {
  tokensUsed: number;
  contextLimit: number;
  inputTokens: number;
  outputTokens: number;
  cost: number;
  messageCount: number;
}): void {
  updateMissionGauge(state.tokensUsed, state.contextLimit);
  updateMissionMetrics({
    inputTokens: state.inputTokens,
    outputTokens: state.outputTokens,
    cost: state.cost,
    messageCount: state.messageCount,
  });

  // Flash signal wave on metrics card when tokens arrive
  if (state.tokensUsed > 0 && _metricsCtrl) {
    _metricsCtrl.signal('accent');
  }
  // Flash gauge card if context is getting high
  if (state.tokensUsed > 0 && _gaugeCtrl) {
    const pct = state.contextLimit > 0 ? (state.tokensUsed / state.contextLimit) * 100 : 0;
    if (pct >= 80) _gaugeCtrl.signal('error');
    else if (pct >= 60) _gaugeCtrl.signal('warning');
  }
}

// ── Init: render initial empty gauge + wire kinetic controllers ────────

let _metricsCtrl: ReturnType<typeof kineticRow> | null = null;
let _gaugeCtrl: ReturnType<typeof kineticRow> | null = null;

export function initMissionPanel(): void {
  updateMissionGauge(0, 128_000);
  renderActiveJobs();

  // Wire kinetic controllers for signal flashes on updates
  const panel = $('chat-mission-panel');
  if (panel) {
    const cards = panel.querySelectorAll('.mission-card');
    if (cards[0] && !_gaugeCtrl) {
      _gaugeCtrl = kineticRow(cards[0] as HTMLElement, {});
    }
    if (cards[1] && !_metricsCtrl) {
      _metricsCtrl = kineticRow(cards[1] as HTMLElement, {});
    }
  }
}

// ── Helpers ────────────────────────────────────────────────────────────

function fmtK(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return `${n}`;
}

function escHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}
