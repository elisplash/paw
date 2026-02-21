// Automations / Cron View — DOM rendering + IPC

import { pawEngine, type EngineTask, type EngineTaskActivity } from '../../engine';
import { $, escHtml, escAttr, confirmModal } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { isConnected } from '../../state/connection';
import { MORNING_BRIEF_PROMPT, isValidSchedule } from './atoms';

// ── State bridge ───────────────────────────────────────────────────────────

interface MoleculesState {
  getAgents: () => { id: string; name: string; avatar: string }[];
  getEditingTaskId: () => string | null;
  setEditingTaskId: (id: string | null) => void;
}

let _state: MoleculesState;

export function initMoleculesState() {
  return {
    setMoleculesState: (st: MoleculesState) => {
      _state = st;
    },
  };
}

// ── Load Automations ───────────────────────────────────────────────────────
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
  if (!isConnected()) return;

  if (loading) loading.style.display = '';
  if (empty) empty.style.display = 'none';
  if (board) board.style.display = 'grid';
  if (activeCards) activeCards.innerHTML = '';
  if (pausedCards) pausedCards.innerHTML = '';
  if (historyCards) historyCards.innerHTML = '';

  try {
    const tasks = await pawEngine.tasksList();
    const cronTasks = tasks.filter((t) => t.cron_schedule && t.cron_schedule.length > 0);

    if (loading) loading.style.display = 'none';

    if (statusEl) {
      statusEl.className = 'cron-service-status active';
      statusEl.textContent = 'Heartbeat active';
      statusEl.title = 'Background heartbeat running (60s interval)';
    }

    if (!cronTasks.length) {
      if (empty) empty.style.display = 'flex';
      if (board) board.style.display = 'none';
      return;
    }

    let active = 0,
      paused = 0;
    for (const task of cronTasks) {
      const nextRun = task.next_run_at ? new Date(task.next_run_at).toLocaleString() : '';
      const lastRun = task.last_run_at ? new Date(task.last_run_at).toLocaleString() : '';
      const agentNames = task.assigned_agents.length
        ? task.assigned_agents.map((a) => a.agent_id).join(', ')
        : (task.assigned_agent ?? 'default');

      const card = document.createElement('div');
      card.className = `auto-card${task.status === 'in_progress' ? ' auto-card-running' : ''}`;
      card.innerHTML = `
        <div class="auto-card-title">${escHtml(task.title)}</div>
        <div class="auto-card-schedule">${escHtml(task.cron_schedule ?? '')}</div>
        ${task.description ? `<div class="auto-card-prompt">${escHtml(task.description)}</div>` : ''}
        <div class="auto-card-meta">Agent: ${escHtml(agentNames)}</div>
        ${nextRun ? `<div class="auto-card-meta">Next: ${escHtml(nextRun)}</div>` : ''}
        ${lastRun ? `<div class="auto-card-meta">Last: ${escHtml(lastRun)}</div>` : ''}
        <div class="auto-card-status-badge ${task.status}">${escHtml(task.status)}</div>
        <div class="auto-card-actions">
          <button class="btn btn-ghost btn-sm cron-run" data-id="${escAttr(task.id)}" title="Run now">▶ Run</button>
          <button class="btn btn-ghost btn-sm cron-edit" data-id="${escAttr(task.id)}" title="Edit"><span class="ms ms-sm">edit</span> Edit</button>
          <button class="btn btn-ghost btn-sm cron-toggle" data-id="${escAttr(task.id)}" data-enabled="${task.cron_enabled}">${task.cron_enabled ? 'Pause' : 'Enable'}</button>
          <button class="btn btn-ghost btn-sm cron-delete" data-id="${escAttr(task.id)}">Delete</button>
        </div>
      `;
      (card as unknown as Record<string, unknown>)._task = task;
      if (task.cron_enabled) {
        active++;
        activeCards?.appendChild(card);
      } else {
        paused++;
        pausedCards?.appendChild(card);
      }
    }
    if (activeCount) activeCount.textContent = String(active);
    if (pausedCount) pausedCount.textContent = String(paused);

    wireCardActions(activeCards);
    wireCardActions(pausedCards);

    try {
      const activities = await pawEngine.taskActivity(undefined, 30);
      const cronActivities = activities.filter(
        (a) =>
          a.kind === 'cron_triggered' ||
          a.kind === 'cron_error' ||
          a.kind === 'agent_completed' ||
          a.kind === 'agent_error',
      );
      if (cronActivities.length && historyCards) {
        for (const activity of cronActivities.slice(0, 15)) {
          renderActivityCard(activity, historyCards);
        }
      }
    } catch {
      /* history not available */
    }
  } catch (e) {
    console.warn('Automations load failed:', e);
    if (loading) loading.style.display = 'none';
    if (empty) {
      empty.style.display = 'flex';
      empty.innerHTML = `<div class="empty-title">Automations</div><div class="empty-subtitle">Failed to load: ${e}</div>`;
    }
    if (board) board.style.display = 'none';
  }
}

// ── Run History Card ───────────────────────────────────────────────────────
function renderActivityCard(activity: EngineTaskActivity, container: HTMLElement) {
  const card = document.createElement('div');
  const isFailed = activity.kind === 'cron_error' || activity.kind === 'agent_error';
  const statusClass = isFailed ? 'failed' : 'success';
  card.className = `auto-card${isFailed ? ' auto-card-error' : ''}`;

  const timeStr = activity.created_at ? new Date(activity.created_at).toLocaleString() : '';
  const kindLabel = activity.kind.replace(/_/g, ' ');

  card.innerHTML = `
    <div class="auto-card-header-row">
      <div class="auto-card-time">${timeStr}</div>
      <span class="auto-card-status ${statusClass}">${escHtml(kindLabel)}</span>
    </div>
    ${activity.agent ? `<div class="auto-card-meta">Agent: ${escHtml(activity.agent)}</div>` : ''}
    <div class="auto-card-prompt">${escHtml(activity.content.substring(0, 200))}</div>
  `;

  container.appendChild(card);
}

function wireCardActions(container: HTMLElement | null) {
  if (!container) return;
  container.querySelectorAll('.cron-run').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const id = (btn as HTMLElement).dataset.id!;
      try {
        await pawEngine.taskRun(id);
        (btn as HTMLElement).textContent = '⏳ Running…';
        loadCron();
      } catch (e) {
        showToast(`Run failed: ${e}`, 'error');
      }
    });
  });
  container.querySelectorAll('.cron-edit').forEach((btn) => {
    btn.addEventListener('click', () => {
      const card = (btn as HTMLElement).closest('.auto-card') as
        | (HTMLElement & { _task?: EngineTask })
        | null;
      if (card?._task) {
        openEditModal(card._task);
      }
    });
  });
  container.querySelectorAll('.cron-toggle').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const id = (btn as HTMLElement).dataset.id!;
      const enabled = (btn as HTMLElement).dataset.enabled === 'true';
      try {
        const tasks = await pawEngine.tasksList();
        const task = tasks.find((t) => t.id === id);
        if (task) {
          task.cron_enabled = !enabled;
          if (task.cron_enabled && !task.next_run_at) {
            task.next_run_at = new Date().toISOString();
          }
          await pawEngine.taskUpdate(task);
          loadCron();
        }
      } catch (e) {
        showToast(`Toggle failed: ${e}`, 'error');
      }
    });
  });
  container.querySelectorAll('.cron-delete').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const id = (btn as HTMLElement).dataset.id!;
      if (!(await confirmModal('Delete this automation?'))) return;
      try {
        await pawEngine.taskDelete(id);
        loadCron();
      } catch (e) {
        showToast(`Delete failed: ${e}`, 'error');
      }
    });
  });
}

// ── Agent Dropdown ─────────────────────────────────────────────────────────
function populateAgentDropdown() {
  const el = $('cron-form-agent');
  if (!el) return;

  if (el.tagName === 'INPUT') {
    const newSelect = document.createElement('select');
    newSelect.id = 'cron-form-agent';
    newSelect.className = 'form-input';
    el.replaceWith(newSelect);
    populateAgentDropdownElement(newSelect);
  } else {
    populateAgentDropdownElement(el as HTMLSelectElement);
  }
}

function populateAgentDropdownElement(select: HTMLSelectElement) {
  select.innerHTML = '<option value="default">default</option>';
  for (const agent of _state.getAgents()) {
    const opt = document.createElement('option');
    opt.value = agent.id;
    opt.textContent = `${agent.name} (${agent.id})`;
    select.appendChild(opt);
  }
}

// ── Cron Modal (Create + Edit) ─────────────────────────────────────────────
export function openCreateModal() {
  _state.setEditingTaskId(null);
  const modal = $('cron-modal');
  const title = $('cron-modal-title');
  const saveBtn = $('cron-modal-save');
  if (modal) modal.style.display = 'flex';
  if (title) title.textContent = 'New Automation';
  if (saveBtn) saveBtn.textContent = 'Create';
  const label = $('cron-form-label') as HTMLInputElement;
  const schedule = $('cron-form-schedule') as HTMLInputElement;
  const prompt_ = $('cron-form-prompt') as HTMLTextAreaElement;
  const preset = $('cron-form-schedule-preset') as HTMLSelectElement;
  if (label) label.value = '';
  if (schedule) schedule.value = '';
  if (prompt_) prompt_.value = '';
  if (preset) preset.value = '';
  populateAgentDropdown();
}

function openEditModal(task: EngineTask) {
  _state.setEditingTaskId(task.id);
  const modal = $('cron-modal');
  const title = $('cron-modal-title');
  const saveBtn = $('cron-modal-save');
  if (modal) modal.style.display = 'flex';
  if (title) title.textContent = 'Edit Automation';
  if (saveBtn) saveBtn.textContent = 'Save';
  const label = $('cron-form-label') as HTMLInputElement;
  const schedule = $('cron-form-schedule') as HTMLInputElement;
  const prompt_ = $('cron-form-prompt') as HTMLTextAreaElement;
  const preset = $('cron-form-schedule-preset') as HTMLSelectElement;
  if (label) label.value = task.title;
  if (schedule) schedule.value = task.cron_schedule ?? '';
  if (preset) preset.value = '';
  if (prompt_) prompt_.value = task.description;
  populateAgentDropdown();
  const agentSelect = $('cron-form-agent') as HTMLSelectElement;
  if (agentSelect) {
    const assignedAgent = task.assigned_agents.length
      ? task.assigned_agents[0].agent_id
      : (task.assigned_agent ?? 'default');
    agentSelect.value = assignedAgent;
  }
}

export function hideCronModal() {
  const modal = $('cron-modal');
  if (modal) modal.style.display = 'none';
  _state.setEditingTaskId(null);
}

export async function saveCronJob() {
  const editingTaskId = _state.getEditingTaskId();
  const label = ($('cron-form-label') as HTMLInputElement).value.trim();
  const schedule = ($('cron-form-schedule') as HTMLInputElement).value.trim();
  const prompt_ = ($('cron-form-prompt') as HTMLTextAreaElement).value.trim();
  const agentId =
    ($('cron-form-agent') as HTMLInputElement | HTMLSelectElement)?.value.trim() || 'default';
  if (!label || !schedule || !prompt_) {
    showToast('Name, schedule, and prompt are required', 'error');
    return;
  }

  if (!isValidSchedule(schedule)) {
    showToast('Invalid schedule format. Use: every 5m, every 1h, daily 09:00', 'error');
    return;
  }

  try {
    if (editingTaskId) {
      const tasks = await pawEngine.tasksList();
      const existing = tasks.find((t) => t.id === editingTaskId);
      if (existing) {
        existing.title = label;
        existing.description = prompt_;
        existing.cron_schedule = schedule;
        existing.cron_enabled = true;
        existing.next_run_at = new Date().toISOString();
        await pawEngine.taskUpdate(existing);
        await pawEngine.taskSetAgents(editingTaskId, [{ agent_id: agentId, role: 'lead' }]);
      }
    } else {
      const id = crypto.randomUUID();
      const now = new Date().toISOString();
      const task: EngineTask = {
        id,
        title: label,
        description: prompt_,
        status: 'assigned',
        priority: 'medium',
        assigned_agent: agentId,
        assigned_agents: [{ agent_id: agentId, role: 'lead' }],
        cron_schedule: schedule,
        cron_enabled: true,
        next_run_at: now,
        created_at: now,
        updated_at: now,
      };
      await pawEngine.taskCreate(task);
      await pawEngine.taskSetAgents(id, [{ agent_id: agentId, role: 'lead' }]);
    }
    hideCronModal();
    loadCron();
  } catch (e) {
    showToast(
      `Failed to ${editingTaskId ? 'update' : 'create'}: ${e instanceof Error ? e.message : e}`,
      'error',
    );
  }
}

// ── Morning Brief Template ─────────────────────────────────────────────────
export async function createMorningBrief() {
  try {
    const id = crypto.randomUUID();
    const now = new Date().toISOString();
    const task: EngineTask = {
      id,
      title: 'Morning Brief',
      description: MORNING_BRIEF_PROMPT,
      status: 'assigned',
      priority: 'medium',
      assigned_agent: 'default',
      assigned_agents: [{ agent_id: 'default', role: 'lead' }],
      cron_schedule: 'daily 09:00',
      cron_enabled: true,
      next_run_at: now,
      created_at: now,
      updated_at: now,
    };
    await pawEngine.taskCreate(task);
    await pawEngine.taskSetAgents(id, [{ agent_id: 'default', role: 'lead' }]);
    loadCron();
  } catch (e) {
    showToast(`Failed to create Morning Brief: ${e instanceof Error ? e.message : e}`, 'error');
  }
}
