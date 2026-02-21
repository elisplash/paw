// Tasks Hub — Molecules (DOM rendering + IPC)

import {
  pawEngine,
  type EngineTask,
  type EngineTaskActivity,
  type TaskStatus,
  type TaskPriority,
  type TaskAgent,
} from '../../engine';
import { showToast } from '../../components/toast';
import { populateModelSelect, $, escHtml, formatTimeAgo } from '../../components/helpers';
import { spriteAvatar } from '../agents';
import { COLUMNS } from './atoms';

// ── State accessors (set by index.ts) ──────────────────────────────────────

interface MoleculesState {
  getTasks: () => EngineTask[];
  getActivity: () => EngineTaskActivity[];
  getEditingTask: () => EngineTask | null;
  setEditingTask: (t: EngineTask | null) => void;
  getFeedFilter: () => 'all' | 'tasks' | 'status';
  setFeedFilter: (f: 'all' | 'tasks' | 'status') => void;
  getAgents: () => { id: string; name: string; avatar: string }[];
  getModalSelectedAgents: () => TaskAgent[];
  setModalSelectedAgents: (agents: TaskAgent[]) => void;
  reload: () => Promise<void>;
}

let _state: MoleculesState;

export function setMoleculesState(s: MoleculesState) {
  _state = s;
}

// ── Render Board ───────────────────────────────────────────────────────

export function renderBoard() {
  const tasks = _state.getTasks();
  for (const status of COLUMNS) {
    const container = $(`tasks-cards-${status}`);
    const countEl = $(`tasks-count-${status}`);
    if (!container) continue;

    const columnTasks = tasks.filter((t) => t.status === status);
    if (countEl) countEl.textContent = String(columnTasks.length);

    container.innerHTML = '';
    for (const task of columnTasks) {
      container.appendChild(createTaskCard(task));
    }
  }
}

export function createTaskCard(task: EngineTask): HTMLElement {
  const card = document.createElement('div');
  card.className = 'task-card';
  card.draggable = true;
  card.dataset.taskId = task.id;

  const priorityColor = task.priority;

  // Show all assigned agents (multi-agent)
  const agents = task.assigned_agents?.length
    ? task.assigned_agents
    : task.assigned_agent
      ? [{ agent_id: task.assigned_agent, role: 'lead' }]
      : [];
  const agentHtml = agents
    .map(
      (a) =>
        `<span class="task-card-agent${a.role === 'lead' ? ' lead' : ''}" title="${escHtml(a.role)}">${escHtml(a.agent_id)}</span>`,
    )
    .join('');

  const cronHtml =
    task.cron_enabled && task.cron_schedule
      ? `<span class="task-card-cron"><span class="ms ms-sm">sync</span> ${escHtml(task.cron_schedule)}</span>`
      : '';
  const modelHtml = task.model
    ? `<span class="task-card-model" title="Model override">${escHtml(task.model)}</span>`
    : '';
  const timeAgo = formatTimeAgo(task.updated_at || task.created_at);

  // Show run button for tasks with agents
  const hasAgents = agents.length > 0;
  const canRun = hasAgents && ['assigned', 'inbox'].includes(task.status);
  const runBtnHtml = canRun
    ? `<button class="task-card-action run-btn" data-action="run" title="Run now">▶</button>`
    : '';
  const agentCountHtml =
    agents.length > 1 ? `<span class="task-card-agent-count">${agents.length} agents</span>` : '';

  card.innerHTML = `
    <div class="task-card-actions">
      ${runBtnHtml}
      <button class="task-card-action" data-action="edit" title="Edit"><span class="ms ms-sm">edit</span></button>
    </div>
    <div class="task-card-title">${escHtml(task.title)}</div>
    <div class="task-card-meta">
      <span class="task-card-priority ${priorityColor}"></span>
      ${agentHtml}
      ${agentCountHtml}
      ${cronHtml}
      ${modelHtml}
      <span style="margin-left:auto">${timeAgo}</span>
    </div>
  `;

  // Drag events
  card.addEventListener('dragstart', (e) => {
    card.classList.add('dragging');
    e.dataTransfer?.setData('text/plain', task.id);
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
  });
  card.addEventListener('dragend', () => {
    card.classList.remove('dragging');
    document
      .querySelectorAll('.tasks-column-cards.drag-over')
      .forEach((el) => el.classList.remove('drag-over'));
  });

  // Click → edit
  card.addEventListener('click', (e) => {
    const target = e.target as HTMLElement;
    const action = target.closest('[data-action]')?.getAttribute('data-action');
    if (action === 'run') {
      e.stopPropagation();
      runTask(task.id);
    } else if (action === 'edit') {
      e.stopPropagation();
      openTaskModal(task);
    } else {
      openTaskModal(task);
    }
  });

  return card;
}

// ── Render Feed ────────────────────────────────────────────────────────

export function renderFeed() {
  const list = $('tasks-feed-list');
  if (!list) return;

  const feedFilter = _state.getFeedFilter();
  let filtered = _state.getActivity();
  if (feedFilter === 'tasks') {
    filtered = filtered.filter((a) =>
      ['created', 'assigned', 'agent_started', 'agent_completed'].includes(a.kind),
    );
  } else if (feedFilter === 'status') {
    filtered = filtered.filter((a) =>
      [
        'status_change',
        'agent_started',
        'agent_completed',
        'agent_error',
        'cron_triggered',
      ].includes(a.kind),
    );
  }

  if (!filtered.length) {
    list.innerHTML = '<div class="tasks-feed-empty">No activity yet</div>';
    return;
  }

  list.innerHTML = '';
  for (const item of filtered.slice(0, 30)) {
    const el = document.createElement('div');
    el.className = 'feed-item';

    const agentName = item.agent || 'System';
    const avatar = getAgentAvatar(item.agent);
    const time = formatTimeAgo(item.created_at);

    el.innerHTML = `
      <div class="feed-item-dot ${escHtml(item.kind)}"></div>
      <div class="feed-item-avatar">${avatar}</div>
      <div class="feed-item-body">
        <div class="feed-item-agent">${escHtml(agentName)}</div>
        <div class="feed-item-content">${escHtml(item.content)}</div>
        <div class="feed-item-time">${time}</div>
      </div>
    `;

    list.appendChild(el);
  }
}

export function renderStats() {
  const tasks = _state.getTasks();
  const total = $('tasks-stat-total');
  const active = $('tasks-stat-active');
  const cron = $('tasks-stat-cron');
  if (total) total.textContent = `${tasks.length} tasks`;
  if (active)
    active.textContent = `${tasks.filter((t) => t.status === 'in_progress').length} active`;
  if (cron) cron.textContent = `${tasks.filter((t) => t.cron_enabled).length} scheduled`;
}

// ── Task Modal ─────────────────────────────────────────────────────────

export function openTaskModal(task?: EngineTask) {
  const modal = $('tasks-detail-modal');
  if (!modal) return;

  _state.setEditingTask(task || null);
  const isNew = !task;

  const titleEl = $('tasks-modal-title');
  const inputTitle = $('tasks-modal-input-title') as HTMLInputElement;
  const inputDesc = $('tasks-modal-input-desc') as HTMLTextAreaElement;
  const inputPriority = $('tasks-modal-input-priority') as HTMLSelectElement;
  const inputAgent = $('tasks-modal-input-agent') as HTMLSelectElement;
  const inputCron = $('tasks-modal-input-cron') as HTMLInputElement;
  const inputCronEnabled = $('tasks-modal-input-cron-enabled') as HTMLInputElement;
  const inputModel = $('tasks-modal-input-model') as HTMLSelectElement;
  const deleteBtn = $('tasks-modal-delete');
  const runBtn = $('tasks-modal-run');
  const activitySection = $('tasks-modal-activity-section');

  if (titleEl) titleEl.textContent = isNew ? 'New Task' : 'Edit Task';
  if (inputTitle) inputTitle.value = task?.title || '';
  if (inputDesc) inputDesc.value = task?.description || '';
  if (inputPriority) inputPriority.value = task?.priority || 'medium';
  if (inputCron) inputCron.value = task?.cron_schedule || '';
  if (inputCronEnabled) inputCronEnabled.checked = task?.cron_enabled || false;
  if (inputModel) inputModel.value = task?.model || '';

  // Dynamically populate model dropdown from configured providers
  if (inputModel) {
    pawEngine
      .getConfig()
      .then((config) => {
        populateModelSelect(inputModel, config.providers ?? [], {
          defaultLabel: '(use default)',
          currentValue: task?.model || '',
        });
      })
      .catch(() => {});
  }

  if (deleteBtn) deleteBtn.style.display = isNew ? 'none' : '';
  const hasAgents = task?.assigned_agents?.length || task?.assigned_agent;
  if (runBtn) runBtn.style.display = hasAgents ? '' : 'none';

  // Multi-agent picker: populate selected agents from task
  _state.setModalSelectedAgents(
    task?.assigned_agents?.length
      ? [...task.assigned_agents]
      : task?.assigned_agent
        ? [{ agent_id: task.assigned_agent, role: 'lead' }]
        : [],
  );
  renderAgentPicker();

  // Also set legacy dropdown for backward compat
  if (inputAgent) {
    inputAgent.innerHTML = '<option value="">+ Add agent</option>';
    for (const agent of _state.getAgents()) {
      const opt = document.createElement('option');
      opt.value = agent.id;
      opt.textContent = agent.name;
      inputAgent.appendChild(opt);
    }
    inputAgent.value = '';
  }

  // Load activity for existing tasks
  if (task && activitySection) {
    activitySection.style.display = '';
    loadTaskActivity(task.id);
  } else if (activitySection) {
    activitySection.style.display = 'none';
  }

  modal.style.display = 'flex';
}

export function renderAgentPicker() {
  const container = $('tasks-modal-agents-tags');
  if (!container) return;
  container.innerHTML = '';
  const modalSelectedAgents = _state.getModalSelectedAgents();
  for (const ta of modalSelectedAgents) {
    const agent = _state.getAgents().find((a) => a.id === ta.agent_id);
    const tag = document.createElement('span');
    tag.className = `agent-tag${ta.role === 'lead' ? ' lead' : ''}`;
    tag.innerHTML = `${agent ? `${spriteAvatar(agent.avatar, 18)} ` : ''}${escHtml(ta.agent_id)}${ta.role === 'lead' ? ' ★' : ''}<button class="agent-tag-remove" title="Remove">×</button>`;

    // Click tag → toggle lead/collaborator
    tag.addEventListener('click', (e) => {
      if ((e.target as HTMLElement).classList.contains('agent-tag-remove')) return;
      ta.role = ta.role === 'lead' ? 'collaborator' : 'lead';
      renderAgentPicker();
    });

    // Remove button
    tag.querySelector('.agent-tag-remove')?.addEventListener('click', (e) => {
      e.stopPropagation();
      _state.setModalSelectedAgents(
        _state.getModalSelectedAgents().filter((a) => a.agent_id !== ta.agent_id),
      );
      renderAgentPicker();
    });

    container.appendChild(tag);
  }
  if (!modalSelectedAgents.length) {
    container.innerHTML = '<span class="agent-tag-empty">No agents assigned</span>';
  }
}

export function addAgentToTask(agentId: string) {
  const modalSelectedAgents = _state.getModalSelectedAgents();
  if (!agentId || modalSelectedAgents.some((a) => a.agent_id === agentId)) return;
  const role = modalSelectedAgents.length === 0 ? 'lead' : 'collaborator';
  _state.setModalSelectedAgents([...modalSelectedAgents, { agent_id: agentId, role }]);
  renderAgentPicker();
}

async function loadTaskActivity(taskId: string) {
  const container = $('tasks-modal-activity');
  if (!container) return;
  try {
    const items = await pawEngine.taskActivity(taskId, 20);
    if (!items.length) {
      container.innerHTML = '<div class="tasks-modal-activity-item">No activity yet</div>';
      return;
    }
    container.innerHTML = '';
    for (const item of items) {
      const el = document.createElement('div');
      el.className = 'tasks-modal-activity-item';
      el.innerHTML = `${escHtml(item.content)} <time>${formatTimeAgo(item.created_at)}</time>`;
      container.appendChild(el);
    }
  } catch {
    container.innerHTML = '<div class="tasks-modal-activity-item">Failed to load activity</div>';
  }
}

export function closeTaskModal() {
  const modal = $('tasks-detail-modal');
  if (modal) modal.style.display = 'none';
  _state.setEditingTask(null);
}

export async function saveTask() {
  const editingTask = _state.getEditingTask();
  const modalSelectedAgents = _state.getModalSelectedAgents();

  const inputTitle = $('tasks-modal-input-title') as HTMLInputElement;
  const inputDesc = $('tasks-modal-input-desc') as HTMLTextAreaElement;
  const inputPriority = $('tasks-modal-input-priority') as HTMLSelectElement;
  const inputCron = $('tasks-modal-input-cron') as HTMLInputElement;
  const inputCronEnabled = $('tasks-modal-input-cron-enabled') as HTMLInputElement;
  const inputModel = $('tasks-modal-input-model') as HTMLSelectElement;

  const title = inputTitle?.value.trim();
  if (!title) {
    showToast('Task title is required', 'warning');
    return;
  }

  const cronSchedule = inputCron?.value.trim() || undefined;
  const cronEnabled = inputCronEnabled?.checked || false;
  const taskModel = inputModel?.value || undefined;

  // Primary agent = first lead or first agent in the multi-select
  const primaryAgent = modalSelectedAgents.find((a) => a.role === 'lead') || modalSelectedAgents[0];
  const agentId = primaryAgent?.agent_id;

  // Determine status
  let status: TaskStatus = editingTask?.status || 'inbox';
  if (!editingTask && modalSelectedAgents.length > 0) status = 'assigned';

  const now = new Date().toISOString();
  const task: EngineTask = {
    id: editingTask?.id || crypto.randomUUID(),
    title,
    description: inputDesc?.value || '',
    status,
    priority: (inputPriority?.value || 'medium') as TaskPriority,
    assigned_agent: agentId,
    assigned_agents: modalSelectedAgents,
    session_id: editingTask?.session_id,
    model: taskModel,
    cron_schedule: cronSchedule,
    cron_enabled: cronEnabled,
    last_run_at: editingTask?.last_run_at,
    next_run_at: editingTask?.next_run_at,
    created_at: editingTask?.created_at || now,
    updated_at: now,
  };

  try {
    if (editingTask) {
      await pawEngine.taskUpdate(task);
      if (modalSelectedAgents.length > 0) {
        await pawEngine.taskSetAgents(task.id, modalSelectedAgents);
      }
      showToast('Task updated', 'success');
    } else {
      await pawEngine.taskCreate(task);
      if (modalSelectedAgents.length > 0) {
        await pawEngine.taskSetAgents(task.id, modalSelectedAgents);
      }
      showToast('Task created', 'success');
    }
    closeTaskModal();
    await _state.reload();
  } catch (e) {
    showToast(`Failed: ${e instanceof Error ? e.message : e}`, 'error');
  }
}

export async function deleteTask() {
  const editingTask = _state.getEditingTask();
  if (!editingTask) return;
  try {
    await pawEngine.taskDelete(editingTask.id);
    showToast('Task deleted', 'success');
    closeTaskModal();
    await _state.reload();
  } catch (e) {
    showToast(`Failed: ${e instanceof Error ? e.message : e}`, 'error');
  }
}

export async function runTask(taskId: string) {
  try {
    showToast('Starting agent work...', 'info');
    await pawEngine.taskRun(taskId);
    showToast('Agent is working on the task', 'success');
    await _state.reload();
  } catch (e) {
    showToast(`Run failed: ${e instanceof Error ? e.message : e}`, 'error');
  }
}

// ── Drag & Drop ────────────────────────────────────────────────────────

export function setupDragAndDrop() {
  document.querySelectorAll<HTMLElement>('.tasks-column-cards').forEach((column) => {
    column.addEventListener('dragover', (e) => {
      e.preventDefault();
      if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
      column.classList.add('drag-over');
    });

    column.addEventListener('dragleave', (e) => {
      if (!column.contains(e.relatedTarget as Node)) {
        column.classList.remove('drag-over');
      }
    });

    column.addEventListener('drop', async (e) => {
      e.preventDefault();
      column.classList.remove('drag-over');
      const taskId = e.dataTransfer?.getData('text/plain');
      const newStatus = column.dataset.status;
      if (!taskId || !newStatus) return;

      const tasks = _state.getTasks();
      const task = tasks.find((t) => t.id === taskId);
      if (!task || task.status === newStatus) return;

      try {
        await pawEngine.taskMove(taskId, newStatus);
        await _state.reload();
      } catch (err) {
        showToast(`Move failed: ${err instanceof Error ? err.message : err}`, 'error');
      }
    });
  });
}

// ── Helpers ────────────────────────────────────────────────────────────

function getAgentAvatar(agentId?: string | null): string {
  if (!agentId) return '<span class="ms">build</span>';
  const agent = _state.getAgents().find((a) => a.id === agentId || a.name === agentId);
  return agent ? spriteAvatar(agent.avatar, 20) : '<span class="ms">smart_toy</span>';
}
