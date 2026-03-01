// src/components/chat-mission-panel.ts — Chat Mission Control Panel
//
// Updates the mission side panel with live session data:
// context gauge, session metrics, active jobs, signal flashes.
// Called from chat_controller when token/state changes occur.

import { progressRing } from './molecules/data-viz';
import { kineticRow, kineticDot } from './kinetic-row';
import { getPopularTemplates } from '../views/integrations/automations/templates';
import { getPopularQueries, QUERY_CATALOG } from '../views/integrations/queries/catalog';
import type { AutomationTemplate } from '../views/integrations/automations/atoms';
import type { ServiceQuery } from '../views/integrations/queries/atoms';

// ── Shared app state reference ─────────────────────────────────────────
// We read from the global appState rather than importing it to avoid
// circular deps with the chat controller.

function $(id: string): HTMLElement | null {
  return document.getElementById(id);
}

// ── Context Gauge ──────────────────────────────────────────────────────

export function updateMissionGauge(tokensUsed: number, contextLimit: number): void {
  const gauge = $('mission-ctx-gauge');
  const usedEl = $('mission-ctx-used');
  const limitEl = $('mission-ctx-limit');
  if (!gauge) return;

  const pct = contextLimit > 0 ? Math.min((tokensUsed / contextLimit) * 100, 100) : 0;

  const color = pct >= 80 ? 'var(--error)' : pct >= 60 ? 'var(--warning)' : 'var(--accent)';

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

  list.innerHTML = _activeJobs
    .map((job) => {
      const elapsed = Math.round((Date.now() - job.startedAt) / 1000);
      const timeStr = elapsed < 60 ? `${elapsed}s` : `${Math.floor(elapsed / 60)}m${elapsed % 60}s`;
      return `
      <div class="mission-job-item k-row k-breathe k-status-healthy k-materialise">
        ${kineticDot()}
        <span class="mission-job-name">${escHtml(job.name)}</span>
        <span class="mission-job-time">${timeStr}</span>
      </div>`;
    })
    .join('');
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
let _eventsWired = false;

export function initMissionPanel(): void {
  updateMissionGauge(0, 128_000);
  renderActiveJobs();
  renderQuickPrompts();
  renderApprovals();
  renderAutomations();
  renderQueries();

  // Wire collapsible toggles + click-to-chat (only once)
  if (!_eventsWired) {
    wireMissionEvents();
    _eventsWired = true;
  }

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

// ── Quick Prompts — pill buttons that inject into chat ──────────────────

// ── Tool Approval Categories ───────────────────────────────────────────

interface ApprovalCategory {
  id: string;
  label: string;
  icon: string;
  description: string;
  /** Tool names belonging to this category */
  tools: string[];
}

const APPROVAL_CATEGORIES: ApprovalCategory[] = [
  {
    id: 'browser',
    label: 'Browser',
    icon: 'travel_explore',
    description: 'Web search, read pages, screenshots',
    tools: ['web_search', 'web_read', 'web_browse', 'web_screenshot', 'fetch'],
  },
  {
    id: 'files',
    label: 'Files',
    icon: 'folder',
    description: 'Read / write / list files',
    tools: ['read_file', 'write_file', 'list_directory'],
  },
  {
    id: 'shell',
    label: 'Shell',
    icon: 'terminal',
    description: 'Run terminal commands',
    tools: ['exec', 'run_command'],
  },
  {
    id: 'memory',
    label: 'Memory',
    icon: 'psychology',
    description: 'Read / write agent memory',
    tools: ['soul_read', 'soul_write', 'soul_list', 'memory_store', 'memory_search'],
  },
  {
    id: 'tasks',
    label: 'Tasks',
    icon: 'task_alt',
    description: 'Create and manage tasks',
    tools: ['create_task', 'manage_task', 'list_tasks'],
  },
  {
    id: 'messaging',
    label: 'Messaging',
    icon: 'forum',
    description: 'Slack, Telegram, Discord, inter-agent',
    tools: ['telegram_read', 'telegram_send', 'agent_send_message', 'agent_read_messages'],
  },
  {
    id: 'trading',
    label: 'Trading',
    icon: 'candlestick_chart',
    description: 'Crypto swap, transfer, wallets',
    tools: [
      'sol_swap',
      'sol_transfer',
      'dex_swap',
      'dex_transfer',
      'coinbase_trade',
      'coinbase_transfer',
    ],
  },
  {
    id: 'media',
    label: 'Media',
    icon: 'image',
    description: 'Image generation',
    tools: ['image_generate'],
  },
];

const APPROVAL_STORAGE_KEY = 'paw-tool-approvals';

/** Load user's approval preferences from localStorage */
export function getApprovalPrefs(): Record<string, boolean> {
  try {
    const raw = localStorage.getItem(APPROVAL_STORAGE_KEY);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

/** Save approval preferences */
function saveApprovalPrefs(prefs: Record<string, boolean>): void {
  localStorage.setItem(APPROVAL_STORAGE_KEY, JSON.stringify(prefs));
}

/** Check if a specific tool is user-approved via the sidebar toggles */
export function isToolUserApproved(toolName: string): boolean {
  const prefs = getApprovalPrefs();
  for (const cat of APPROVAL_CATEGORIES) {
    if (cat.tools.includes(toolName)) {
      return !!prefs[cat.id];
    }
  }
  return false;
}

/** Get all tool names that are currently user-approved */
export function getUserApprovedTools(): string[] {
  const prefs = getApprovalPrefs();
  const approved: string[] = [];
  for (const cat of APPROVAL_CATEGORIES) {
    if (prefs[cat.id]) {
      approved.push(...cat.tools);
    }
  }
  return approved;
}

function renderApprovals(): void {
  const container = $('mission-approvals-list');
  if (!container) return;

  const prefs = getApprovalPrefs();

  container.innerHTML = APPROVAL_CATEGORIES.map((cat) => {
    const checked = prefs[cat.id] ? 'checked' : '';
    return `
      <label class="mission-approval-row" title="${escHtml(cat.description)}">
        <span class="ms mission-approval-icon">${cat.icon}</span>
        <span class="mission-approval-label">${escHtml(cat.label)}</span>
        <input type="checkbox" class="mission-approval-toggle" data-category="${cat.id}" ${checked} />
      </label>`;
  }).join('');

  // Wire toggle events
  container.addEventListener('change', (e) => {
    const input = e.target as HTMLInputElement;
    if (!input.classList.contains('mission-approval-toggle')) return;
    const catId = input.dataset.category;
    if (!catId) return;
    const currentPrefs = getApprovalPrefs();
    currentPrefs[catId] = input.checked;
    saveApprovalPrefs(currentPrefs);
  });
}

// ── Quick Prompts — pill buttons that inject into chat ──────────────────

interface QuickPrompt {
  label: string;
  icon: string;
  prompt: string;
  /** If true, this is a slash command (insert into input, don't auto-send) */
  isSlash?: boolean;
}

const QUICK_PROMPTS: QuickPrompt[] = [
  {
    label: 'Briefing',
    icon: 'wb_sunny',
    prompt:
      'Give me a morning briefing: weather, any calendar events today, and summarize my unread emails.',
  },
  {
    label: 'Inbox',
    icon: 'mail',
    prompt: 'Check my email inbox and summarize the important unread messages.',
  },
  {
    label: 'Schedule',
    icon: 'event',
    prompt: 'What do I have scheduled for today? Check my calendar.',
  },
  { label: 'Status', icon: 'info', prompt: '/status', isSlash: true },
  { label: 'Web search', icon: 'travel_explore', prompt: '/web ', isSlash: true },
  { label: 'Memory', icon: 'psychology', prompt: '/recall ', isSlash: true },
  { label: 'Generate', icon: 'image', prompt: '/img ', isSlash: true },
  { label: 'Help', icon: 'help', prompt: '/help', isSlash: true },
];

function renderQuickPrompts(): void {
  const container = $('mission-prompt-pills');
  if (!container) return;

  container.innerHTML = QUICK_PROMPTS.map(
    (p) => `
    <button class="mission-pill" data-prompt="${escHtml(p.prompt)}" data-slash="${p.isSlash ? '1' : ''}" title="${escHtml(p.prompt.slice(0, 80))}">
      <span class="ms" style="font-size:12px">${p.icon}</span>
      <span>${escHtml(p.label)}</span>
    </button>
  `,
  ).join('');
}

// ── Automations — popular templates, compact list ──────────────────────

function renderAutomations(): void {
  const container = $('mission-automations-list');
  const badge = $('mission-auto-badge');
  if (!container) return;

  const popular = getPopularTemplates(6);
  if (badge) badge.textContent = `${popular.length}`;

  if (!popular.length) {
    container.innerHTML = '<div class="mission-compact-empty">No templates available</div>';
    return;
  }

  container.innerHTML = popular
    .map((t: AutomationTemplate) => {
      const trigLabel = t.trigger.label;
      return `
      <button class="mission-compact-item k-row k-spring" data-auto-prompt="${escHtml(t.name)}" data-auto-desc="${escHtml(t.description)}" title="${escHtml(t.description)}">
        <span class="ms mission-compact-icon">${categoryIcon(t.category)}</span>
        <div class="mission-compact-text">
          <span class="mission-compact-name">${escHtml(t.name)}</span>
          <span class="mission-compact-sub">${escHtml(trigLabel)}</span>
        </div>
      </button>`;
    })
    .join('');
}

function categoryIcon(cat: string): string {
  const map: Record<string, string> = {
    alerts: 'notifications_active',
    reporting: 'assessment',
    sync: 'sync',
    onboarding: 'waving_hand',
    productivity: 'rocket_launch',
    devops: 'terminal',
    marketing: 'campaign',
    support: 'support_agent',
  };
  return map[cat] ?? 'bolt';
}

// ── Queries — popular service questions, compact list ──────────────────

function renderQueries(): void {
  const container = $('mission-queries-list');
  if (!container) return;

  // Show popular cross-service queries + a sample from general catalog
  const popular = getPopularQueries();
  const sample = QUERY_CATALOG.filter((q) => q.category !== 'cross-service').slice(0, 4);
  const queries = [...popular.slice(0, 4), ...sample].slice(0, 8);

  if (!queries.length) {
    container.innerHTML = '<div class="mission-compact-empty">No queries available</div>';
    return;
  }

  container.innerHTML = queries
    .map(
      (q: ServiceQuery) => `
    <button class="mission-compact-item k-row k-spring" data-query-prompt="${escHtml(q.question)}" title="${escHtml(q.resultHint)}">
      <span class="ms mission-compact-icon">${q.icon}</span>
      <div class="mission-compact-text">
        <span class="mission-compact-name">${escHtml(q.question)}</span>
        <span class="mission-compact-sub">${escHtml(q.resultHint)}</span>
      </div>
    </button>
  `,
    )
    .join('');
}

// ── Collapsible card toggle + click-to-chat wiring ─────────────────────

function wireMissionEvents(): void {
  const panel = $('chat-mission-panel');
  if (!panel) return;

  panel.addEventListener('click', (e) => {
    const target = e.target as HTMLElement;

    // Collapsible toggle
    const toggle = target.closest('.mission-card-header-toggle') as HTMLElement | null;
    if (toggle) {
      const bodyId = toggle.dataset.toggle;
      if (bodyId) {
        const body = document.getElementById(bodyId);
        if (body) {
          const isOpen = !body.classList.contains('mission-collapsed');
          body.classList.toggle('mission-collapsed', isOpen);
          const chevron = toggle.querySelector('.mission-card-chevron');
          if (chevron) chevron.textContent = isOpen ? 'expand_more' : 'expand_less';
        }
      }
      return;
    }

    // Quick prompt pill
    const pill = target.closest('.mission-pill') as HTMLElement | null;
    if (pill) {
      const prompt = pill.dataset.prompt ?? '';
      const isSlash = pill.dataset.slash === '1';
      sendToChat(prompt, !isSlash);
      return;
    }

    // Automation template → ask agent to set it up
    const autoItem = target.closest('[data-auto-prompt]') as HTMLElement | null;
    if (autoItem) {
      const name = autoItem.dataset.autoPrompt ?? '';
      const desc = autoItem.dataset.autoDesc ?? '';
      sendToChat(`Set up the "${name}" automation: ${desc}`, true);
      return;
    }

    // Query → send question to chat
    const queryItem = target.closest('[data-query-prompt]') as HTMLElement | null;
    if (queryItem) {
      const question = queryItem.dataset.queryPrompt ?? '';
      sendToChat(question, true);
      return;
    }
  });
}

function sendToChat(text: string, autoSend: boolean): void {
  const chatInput = document.getElementById('chat-input') as HTMLTextAreaElement | null;
  if (!chatInput) return;

  chatInput.value = text;
  chatInput.dispatchEvent(new Event('input', { bubbles: true }));
  chatInput.focus();

  if (autoSend) {
    // Trigger send via the send button click
    const sendBtn = document.getElementById('chat-send') as HTMLButtonElement | null;
    if (sendBtn && !sendBtn.disabled) {
      sendBtn.click();
    }
  }
}

function fmtK(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return `${n}`;
}

function escHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
