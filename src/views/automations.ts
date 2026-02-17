// Automations / Cron View
// Extracted from main.ts for maintainability
// NOTE: Cron/automation requires engine API (not yet implemented)

import type { CronJob, CronRunLogEntry } from '../types';

const $ = (id: string) => document.getElementById(id);

let wsConnected = false;

export function setWsConnected(connected: boolean) {
  wsConnected = connected;
}

function escHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

function escAttr(s: string): string {
  return escHtml(s).replace(/\n/g, '&#10;');
}

/** Currently editing job id (null = creating new) */
let _editingJobId: string | null = null;

// ── Load Cron Jobs ─────────────────────────────────────────────────────────
export async function loadCron() {
  const activeCards = $('cron-active-cards');
  const pausedCards = $('cron-paused-cards');
  const historyCards = $('cron-history-cards');
  const empty = $('cron-empty');
  const loading = $('cron-loading');
  const activeCount = $('cron-active-count');
  const pausedCount = $('cron-paused-count');
  const board = document.querySelector('.auto-board') as HTMLElement | null;
  const statusEl = $('cron-service-status');
  if (!wsConnected) return;

  // Automations/Cron not yet available in engine mode
  if (loading) loading.style.display = 'none';
  if (board) board.style.display = 'none';
  if (empty) {
    empty.style.display = 'flex';
    empty.innerHTML = '<div class="empty-title">Automations</div><div class="empty-subtitle">Cron job management coming soon to the Paw engine</div>';
  }
  if (statusEl) {
    statusEl.className = 'cron-service-status';
    statusEl.textContent = 'Coming soon';
  }
  return;

  if (loading) loading.style.display = '';
  if (empty) empty.style.display = 'none';
  if (board) board.style.display = 'grid';
  if (activeCards) activeCards.innerHTML = '';
  if (pausedCards) pausedCards.innerHTML = '';
  if (historyCards) historyCards.innerHTML = '';

  try {
    const result = { jobs: [] } as any; // stub — engine cron API coming soon
    if (loading) loading.style.display = 'none';

    const jobs = result.jobs ?? [];
    if (!jobs.length) {
      if (empty) empty.style.display = 'flex';
      if (board) board.style.display = 'none';
      return;
    }

    let active = 0, paused = 0;
    for (const job of jobs) {
      const scheduleStr = typeof job.schedule === 'string' ? job.schedule : (job.schedule?.type ?? '');
      const nextRun = job.nextRunAt ? new Date(job.nextRunAt).toLocaleString() : '';
      const lastRun = job.lastRunAt ? new Date(job.lastRunAt).toLocaleString() : '';
      const card = document.createElement('div');
      card.className = 'auto-card';
      card.innerHTML = `
        <div class="auto-card-title">${escHtml(job.label ?? job.id)}</div>
        <div class="auto-card-schedule">${escHtml(scheduleStr)}</div>
        ${job.prompt ? `<div class="auto-card-prompt">${escHtml(String(job.prompt))}</div>` : ''}
        ${nextRun ? `<div class="auto-card-meta">Next: ${escHtml(nextRun)}</div>` : ''}
        ${lastRun ? `<div class="auto-card-meta">Last: ${escHtml(lastRun)}</div>` : ''}
        <div class="auto-card-actions">
          <button class="btn btn-ghost btn-sm cron-run" data-id="${escAttr(job.id)}" title="Run now">▶ Run</button>
          <button class="btn btn-ghost btn-sm cron-edit" data-id="${escAttr(job.id)}" title="Edit">✎ Edit</button>
          <button class="btn btn-ghost btn-sm cron-toggle" data-id="${escAttr(job.id)}" data-enabled="${job.enabled}">${job.enabled ? 'Pause' : 'Enable'}</button>
          <button class="btn btn-ghost btn-sm cron-delete" data-id="${escAttr(job.id)}">Delete</button>
        </div>
      `;
      // Store job data for editing
      (card as unknown as Record<string, unknown>)._job = job;
      if (job.enabled) {
        active++;
        activeCards?.appendChild(card);
      } else {
        paused++;
        pausedCards?.appendChild(card);
      }
    }
    if (activeCount) activeCount.textContent = String(active);
    if (pausedCount) pausedCount.textContent = String(paused);

    // Wire card actions
    wireCardActions(activeCards);
    wireCardActions(pausedCards);

    // Load run history
    try {
      const runs = { runs: [] } as any; // stub
      if (runs.runs?.length && historyCards) {
        for (const run of runs.runs.slice(0, 15)) {
          renderRunHistoryCard(run, historyCards);
        }
      }
    } catch { /* run history not available */ }
  } catch (e) {
    console.warn('Cron load failed:', e);
    if (loading) loading.style.display = 'none';
    if (empty) empty.style.display = 'flex';
    if (board) board.style.display = 'none';
  }
}

// ── Run History Card (with error highlighting + timeout visualization) ──────
function renderRunHistoryCard(run: CronRunLogEntry, container: HTMLElement) {
  const card = document.createElement('div');
  const isFailed = run.status === 'error' || run.status === 'failed' || run.status === 'timeout';
  const isRunning = run.status === 'running';
  const statusClass = isFailed ? 'failed' : (isRunning ? 'running' : 'success');
  card.className = `auto-card${isFailed ? ' auto-card-error' : ''}`;

  // Duration calculation
  let durationStr = '';
  let durationPct = 0;
  if (run.startedAt) {
    const endMs = run.finishedAt ?? Date.now();
    const durationMs = endMs - run.startedAt;
    if (durationMs < 1000) durationStr = `${durationMs}ms`;
    else if (durationMs < 60_000) durationStr = `${(durationMs / 1000).toFixed(1)}s`;
    else durationStr = `${(durationMs / 60_000).toFixed(1)}m`;
    // Timeout bar: assume 5 min default timeout for visualization
    const timeoutMs = 300_000;
    durationPct = Math.min((durationMs / timeoutMs) * 100, 100);
  }

  // Error icon for failed runs
  const errorIcon = isFailed
    ? '<svg class="icon-sm auto-card-error-icon" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10"/><line x1="15" y1="9" x2="9" y2="15"/><line x1="9" y1="9" x2="15" y2="15"/></svg>'
    : '';

  const timeStr = run.startedAt ? new Date(run.startedAt).toLocaleString() : '';

  card.innerHTML = `
    <div class="auto-card-header-row">
      ${errorIcon}
      <div class="auto-card-time">${timeStr}</div>
      <span class="auto-card-status ${statusClass}">${run.status ?? 'unknown'}</span>
    </div>
    <div class="auto-card-title">${escHtml(run.jobLabel ?? run.jobId ?? 'Job')}</div>
    ${durationStr ? `<div class="auto-card-duration">
      <span class="auto-card-duration-text">${durationStr}</span>
      <div class="auto-card-duration-bar"><div class="auto-card-duration-fill${durationPct >= 80 ? ' danger' : durationPct >= 50 ? ' warning' : ''}" style="width:${durationPct}%"></div></div>
    </div>` : ''}
    ${run.error ? `<div class="auto-card-error-detail auto-card-error-collapsed" data-expandable="true">
      <div class="auto-card-error-toggle">Error details ▸</div>
      <div class="auto-card-error-content"><pre>${escHtml(run.error)}</pre></div>
    </div>` : ''}
  `;

  // Wire error detail expansion
  const expandable = card.querySelector('[data-expandable]');
  if (expandable) {
    const toggle = expandable.querySelector('.auto-card-error-toggle');
    toggle?.addEventListener('click', () => {
      expandable.classList.toggle('auto-card-error-collapsed');
      if (toggle) {
        toggle.textContent = expandable.classList.contains('auto-card-error-collapsed')
          ? 'Error details ▸'
          : 'Error details ▾';
      }
    });
  }

  container.appendChild(card);
}

function wireCardActions(container: HTMLElement | null) {
  if (!container) return;
  container.querySelectorAll('.cron-run').forEach(btn => {
    btn.addEventListener('click', async () => {
      const id = (btn as HTMLElement).dataset.id!;
      try { alert('Cron management coming soon to the Paw engine'); }
      catch (e) { alert(`Failed: ${e}`); }
    });
  });
  container.querySelectorAll('.cron-edit').forEach(btn => {
    btn.addEventListener('click', () => {
      const id = (btn as HTMLElement).dataset.id!;
      // Find job data from card
      const card = (btn as HTMLElement).closest('.auto-card') as (HTMLElement & { _job?: CronJob }) | null;
      if (card?._job) {
        openEditModal(card._job);
      } else {
        // Fallback: just open with ID, fields blank
        _editingJobId = id;
        openEditModal({ id, enabled: true });
      }
    });
  });
  container.querySelectorAll('.cron-toggle').forEach(btn => {
    btn.addEventListener('click', async () => {
      const id = (btn as HTMLElement).dataset.id!;
      const enabled = (btn as HTMLElement).dataset.enabled === 'true';
      try { alert('Cron management coming soon'); loadCron(); }
      catch (e) { alert(`Failed: ${e}`); }
    });
  });
  container.querySelectorAll('.cron-delete').forEach(btn => {
    btn.addEventListener('click', async () => {
      const id = (btn as HTMLElement).dataset.id!;
      if (!confirm('Delete this automation?')) return;
      try { alert('Cron management coming soon'); loadCron(); }
      catch (e) { alert(`Failed: ${e}`); }
    });
  });
}

// ── Cron Modal (Create + Edit) ─────────────────────────────────────────────
function openCreateModal() {
  _editingJobId = null;
  const modal = $('cron-modal');
  const title = $('cron-modal-title');
  const saveBtn = $('cron-modal-save');
  if (modal) modal.style.display = 'flex';
  if (title) title.textContent = 'New Automation';
  if (saveBtn) saveBtn.textContent = 'Create';
  // Reset form
  const label = $('cron-form-label') as HTMLInputElement;
  const schedule = $('cron-form-schedule') as HTMLInputElement;
  const prompt_ = $('cron-form-prompt') as HTMLTextAreaElement;
  const preset = $('cron-form-schedule-preset') as HTMLSelectElement;
  const agentId = $('cron-form-agent') as HTMLInputElement | null;
  if (label) label.value = '';
  if (schedule) schedule.value = '';
  if (prompt_) prompt_.value = '';
  if (preset) preset.value = '';
  if (agentId) agentId.value = '';
}

function openEditModal(job: CronJob) {
  _editingJobId = job.id;
  const modal = $('cron-modal');
  const title = $('cron-modal-title');
  const saveBtn = $('cron-modal-save');
  if (modal) modal.style.display = 'flex';
  if (title) title.textContent = 'Edit Automation';
  if (saveBtn) saveBtn.textContent = 'Save';
  // Populate form
  const label = $('cron-form-label') as HTMLInputElement;
  const schedule = $('cron-form-schedule') as HTMLInputElement;
  const prompt_ = $('cron-form-prompt') as HTMLTextAreaElement;
  const preset = $('cron-form-schedule-preset') as HTMLSelectElement;
  const agentId = $('cron-form-agent') as HTMLInputElement | null;
  if (label) label.value = job.label ?? '';
  const schedStr = typeof job.schedule === 'string' ? job.schedule : '';
  if (schedule) schedule.value = schedStr;
  if (preset) preset.value = '';
  if (prompt_) prompt_.value = job.prompt ?? '';
  if (agentId) agentId.value = (job.agentId as string) ?? '';
}

function hideCronModal() {
  const modal = $('cron-modal');
  if (modal) modal.style.display = 'none';
  _editingJobId = null;
}

async function saveCronJob() {
  const label = ($('cron-form-label') as HTMLInputElement).value.trim();
  const schedule = ($('cron-form-schedule') as HTMLInputElement).value.trim();
  const prompt_ = ($('cron-form-prompt') as HTMLTextAreaElement).value.trim();
  const agentId = ($('cron-form-agent') as HTMLInputElement | null)?.value.trim() || undefined;
  if (!label || !schedule || !prompt_) { alert('Name, schedule, and prompt are required'); return; }

  try {
    if (_editingJobId) {
      // Update existing job
      await Promise.reject(new Error('Engine cron API coming soon'));
    } else {
      // Create new job
      await Promise.reject(new Error('Engine cron API coming soon'));
    }
    hideCronModal();
    loadCron();
  } catch (e) {
    alert(`Failed to ${_editingJobId ? 'update' : 'create'}: ${e instanceof Error ? e.message : e}`);
  }
}

// ── Initialize ─────────────────────────────────────────────────────────────
export function initAutomations() {
  $('add-cron-btn')?.addEventListener('click', openCreateModal);
  $('cron-empty-add')?.addEventListener('click', openCreateModal);
  $('cron-modal-close')?.addEventListener('click', hideCronModal);
  $('cron-modal-cancel')?.addEventListener('click', hideCronModal);
  
  $('cron-form-schedule-preset')?.addEventListener('change', () => {
    const preset = ($('cron-form-schedule-preset') as HTMLSelectElement).value;
    const scheduleInput = $('cron-form-schedule') as HTMLInputElement;
    if (preset && scheduleInput) scheduleInput.value = preset;
  });
  
  $('cron-modal-save')?.addEventListener('click', saveCronJob);
}
