// src/engine/molecules/inbox_sidebar.ts
// Phase 11.3 — Inbox context sidebar molecule (right panel).
// Shows agent profile, session metrics, quick actions, and collapsible
// mission cards — migrated from the existing chat-mission-panel markup.
// This molecule builds DOM programmatically rather than lifting existing
// elements so the old panel can be removed independently.

import { spriteAvatar } from '../../views/agents/atoms';

// ── Types ────────────────────────────────────────────────────────────────

export interface InboxSidebarController {
  /** Root DOM element */
  el: HTMLElement;
  /** Set the agent displayed in the profile card */
  setAgent(name: string, avatar: string, color: string, bio: string, model: string): void;
  /** Update context gauge (used / limit / percentage) */
  setContext(used: string, limit: string, pct: number): void;
  /** Update session metrics */
  setMetrics(tokensIn: string, tokensOut: string, cost: string, msgs: string): void;
  /** Update active-jobs badge count */
  setJobCount(n: number): void;
  /** Replace the jobs list HTML */
  setJobs(html: string): void;
  /** Show / hide the sidebar */
  toggle(open: boolean): void;
  /** Destroy + cleanup */
  destroy(): void;
}

export interface InboxSidebarCallbacks {
  onRename: () => void;
  onDelete: () => void;
  onClear: () => void;
  onCompact: () => void;
  /** Optional: colour swatch picked */
  onColorPick?: (color: string) => void;
  /** Optional: search inside conversation */
  onSearch?: (query: string) => void;
}

// ── Colour swatches ──────────────────────────────────────────────────────

const SWATCH_COLORS = [
  '#FF4D4D', '#FF8C42', '#FFC93C', '#4ADE80',
  '#38BDF8', '#818CF8', '#E879F9', '#F472B6',
];

// ── Factory ──────────────────────────────────────────────────────────────

export function createInboxSidebar(
  callbacks: InboxSidebarCallbacks,
): InboxSidebarController {
  let destroyed = false;

  const root = document.createElement('div');
  root.className = 'inbox-sidebar';

  // ── Agent profile card ─────────────────────────────────────────────────

  const profileCard = card('', false);
  profileCard.classList.add('inbox-sidebar-profile');

  const avatarEl = document.createElement('div');
  avatarEl.className = 'inbox-sidebar-avatar';
  const nameEl = document.createElement('div');
  nameEl.className = 'inbox-sidebar-name';
  nameEl.textContent = 'Agent';
  const bioEl = document.createElement('div');
  bioEl.className = 'inbox-sidebar-bio';
  bioEl.textContent = '';
  const modelBadge = document.createElement('span');
  modelBadge.className = 'inbox-sidebar-model';
  modelBadge.textContent = '';

  profileCard.appendChild(avatarEl);
  profileCard.appendChild(nameEl);
  profileCard.appendChild(bioEl);
  profileCard.appendChild(modelBadge);
  root.appendChild(profileCard);

  // ── Colour picker ──────────────────────────────────────────────────────

  if (callbacks.onColorPick) {
    const colorCard = card('COLOR', true);
    const swatchRow = document.createElement('div');
    swatchRow.className = 'inbox-sidebar-swatches';
    SWATCH_COLORS.forEach((c) => {
      const btn = document.createElement('button');
      btn.className = 'inbox-sidebar-swatch';
      btn.style.background = c;
      btn.title = c;
      btn.addEventListener('click', () => callbacks.onColorPick!(c));
      swatchRow.appendChild(btn);
    });
    colorCard.appendChild(swatchRow);
    root.appendChild(colorCard);
  }

  // ── Search in conversation ─────────────────────────────────────────────

  if (callbacks.onSearch) {
    const searchCard = card('SEARCH', true);
    const searchInput = document.createElement('input');
    searchInput.type = 'text';
    searchInput.className = 'inbox-sidebar-search-input';
    searchInput.placeholder = 'Search messages…';
    let debounce: ReturnType<typeof setTimeout>;
    searchInput.addEventListener('input', () => {
      clearTimeout(debounce);
      debounce = setTimeout(() => callbacks.onSearch!(searchInput.value), 250);
    });
    searchCard.appendChild(searchInput);
    root.appendChild(searchCard);
  }

  // ── Context gauge ──────────────────────────────────────────────────────

  const ctxCard = card('CONTEXT WINDOW', true, 'speed');
  const gaugeRow = document.createElement('div');
  gaugeRow.className = 'inbox-sidebar-gauge-row';

  const gaugeEl = document.createElement('div');
  gaugeEl.className = 'inbox-sidebar-gauge';
  const gaugeFill = document.createElement('div');
  gaugeFill.className = 'inbox-sidebar-gauge-fill';
  gaugeEl.appendChild(gaugeFill);

  const usedEl = document.createElement('span');
  usedEl.className = 'inbox-sidebar-gauge-label';
  usedEl.textContent = '0 used';
  const limitEl = document.createElement('span');
  limitEl.className = 'inbox-sidebar-gauge-label';
  limitEl.textContent = '128k limit';

  gaugeRow.appendChild(gaugeEl);
  const statsRow = document.createElement('div');
  statsRow.className = 'inbox-sidebar-gauge-stats';
  statsRow.appendChild(usedEl);
  statsRow.appendChild(limitEl);
  ctxCard.appendChild(gaugeRow);
  ctxCard.appendChild(statsRow);
  root.appendChild(ctxCard);

  // ── Session metrics ────────────────────────────────────────────────────

  const metricsCard = card('SESSION METRICS', true, 'monitoring');
  const metricsGrid = document.createElement('div');
  metricsGrid.className = 'inbox-sidebar-metrics';

  const metricInEl = metricCell('0', 'INPUT');
  const metricOutEl = metricCell('0', 'OUTPUT');
  const metricCostEl = metricCell('$0', 'COST');
  const metricMsgsEl = metricCell('0', 'MESSAGES');

  metricsGrid.appendChild(metricInEl.wrap);
  metricsGrid.appendChild(metricOutEl.wrap);
  metricsGrid.appendChild(metricCostEl.wrap);
  metricsGrid.appendChild(metricMsgsEl.wrap);
  metricsCard.appendChild(metricsGrid);
  root.appendChild(metricsCard);

  // ── Active jobs ────────────────────────────────────────────────────────

  const jobsCard = card('ACTIVE JOBS', true, 'engineering');
  const jobsBadge = document.createElement('span');
  jobsBadge.className = 'inbox-sidebar-jobs-badge';
  jobsBadge.textContent = '0';
  (jobsCard.querySelector('.inbox-sidebar-card-title') as HTMLElement).after(jobsBadge);

  const jobsList = document.createElement('div');
  jobsList.className = 'inbox-sidebar-jobs-list';
  jobsList.innerHTML = `<div class="inbox-sidebar-jobs-empty"><span class="ms" style="font-size:14px;color:var(--text-muted)">hourglass_empty</span> Waiting…</div>`;
  jobsCard.appendChild(jobsList);
  root.appendChild(jobsCard);

  // ── Quick actions ──────────────────────────────────────────────────────

  const actionsCard = card('QUICK ACTIONS', true, 'bolt');
  const actionsGrid = document.createElement('div');
  actionsGrid.className = 'inbox-sidebar-actions';

  actionsGrid.appendChild(actionBtn('edit', 'Rename', callbacks.onRename));
  actionsGrid.appendChild(actionBtn('delete', 'Delete', callbacks.onDelete));
  actionsGrid.appendChild(actionBtn('delete_sweep', 'Clear', callbacks.onClear));
  actionsGrid.appendChild(actionBtn('compress', 'Compact', callbacks.onCompact));
  actionsCard.appendChild(actionsGrid);
  root.appendChild(actionsCard);

  // ── Controller ─────────────────────────────────────────────────────────

  const ctrl: InboxSidebarController = {
    el: root,

    setAgent(name, avatar, color, bio, model) {
      avatarEl.innerHTML = spriteAvatar(avatar, 48);
      avatarEl.style.borderColor = color;
      nameEl.textContent = name;
      bioEl.textContent = bio || '';
      modelBadge.textContent = model || '';
    },

    setContext(used, limit, pct) {
      usedEl.textContent = `${used} used`;
      limitEl.textContent = `${limit} limit`;
      gaugeFill.style.width = `${Math.min(100, pct)}%`;
      if (pct > 90) gaugeFill.style.background = 'var(--danger)';
      else if (pct > 70) gaugeFill.style.background = 'var(--warning)';
      else gaugeFill.style.background = 'var(--accent)';
    },

    setMetrics(tokensIn, tokensOut, cost, msgs) {
      metricInEl.val.textContent = tokensIn;
      metricOutEl.val.textContent = tokensOut;
      metricCostEl.val.textContent = cost;
      metricMsgsEl.val.textContent = msgs;
    },

    setJobCount(n) {
      jobsBadge.textContent = String(n);
    },

    setJobs(html) {
      jobsList.innerHTML = html || `<div class="inbox-sidebar-jobs-empty"><span class="ms" style="font-size:14px;color:var(--text-muted)">hourglass_empty</span> Waiting…</div>`;
    },

    toggle(open) {
      root.style.display = open ? '' : 'none';
    },

    destroy() {
      if (destroyed) return;
      destroyed = true;
      root.remove();
    },
  };

  return ctrl;
}

// ── Helpers (private) ────────────────────────────────────────────────────

function card(title: string, withBorder: boolean, icon?: string): HTMLElement {
  const el = document.createElement('div');
  el.className = 'inbox-sidebar-card' + (withBorder ? ' inbox-sidebar-card-bordered' : '');
  if (title) {
    const header = document.createElement('div');
    header.className = 'inbox-sidebar-card-header';
    if (icon) {
      const ic = document.createElement('span');
      ic.className = 'ms inbox-sidebar-card-icon';
      ic.textContent = icon;
      header.appendChild(ic);
    }
    const t = document.createElement('span');
    t.className = 'inbox-sidebar-card-title';
    t.textContent = title;
    header.appendChild(t);
    el.appendChild(header);
  }
  return el;
}

function metricCell(value: string, label: string) {
  const wrap = document.createElement('div');
  wrap.className = 'inbox-sidebar-metric';
  const val = document.createElement('span');
  val.className = 'inbox-sidebar-metric-val';
  val.textContent = value;
  const lbl = document.createElement('span');
  lbl.className = 'inbox-sidebar-metric-lbl';
  lbl.textContent = label;
  wrap.appendChild(val);
  wrap.appendChild(lbl);
  return { wrap, val, lbl };
}

function actionBtn(icon: string, label: string, onClick: () => void): HTMLElement {
  const btn = document.createElement('button');
  btn.className = 'inbox-sidebar-action-btn';
  btn.title = label;
  btn.innerHTML = `<span class="ms">${icon}</span><span>${label}</span>`;
  btn.addEventListener('click', onClick);
  return btn;
}
