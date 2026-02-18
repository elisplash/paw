// Tasks Hub — Kanban Board View
// Agents pick up tasks, work on them autonomously, move them through columns.
// Supports drag-and-drop, live feed, cron scheduling, and agent auto-work.

import { pawEngine } from '../engine';
import type { EngineTask, EngineTaskActivity, TaskStatus, TaskPriority, TaskAgent } from '../engine';
import { showToast } from '../components/toast';
import { spriteAvatar } from './agents';

const $ = (id: string) => document.getElementById(id);

function escHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

// ── State ──────────────────────────────────────────────────────────────

let _tasks: EngineTask[] = [];
let _activity: EngineTaskActivity[] = [];
let _editingTask: EngineTask | null = null;
let _feedFilter: 'all' | 'tasks' | 'status' = 'all';
let _agents: { id: string; name: string; avatar: string }[] = [];
let _modalSelectedAgents: TaskAgent[] = [];

const COLUMNS: TaskStatus[] = ['inbox', 'assigned', 'in_progress', 'review', 'blocked', 'done'];

// ── Public API ─────────────────────────────────────────────────────────

export async function loadTasks() {
  try {
    const [tasks, activity] = await Promise.all([
      pawEngine.tasksList(),
      pawEngine.taskActivity(undefined, 50),
    ]);
    _tasks = tasks;
    _activity = activity;
    renderBoard();
    renderFeed();
    renderStats();
  } catch (e) {
    console.error('[tasks] Load failed:', e);
  }
}

export function setAgents(agents: { id: string; name: string; avatar: string }[]) {
  _agents = agents;
}

/** Called from main.ts when a task-updated event fires */
export function onTaskUpdated(_data: { task_id: string; status: string }) {
  loadTasks(); // Full refresh — simple and reliable
}

// ── Render Board ───────────────────────────────────────────────────────

function renderBoard() {
  for (const status of COLUMNS) {
    const container = $(`tasks-cards-${status}`);
    const countEl = $(`tasks-count-${status}`);
    if (!container) continue;

    const columnTasks = _tasks.filter(t => t.status === status);
    if (countEl) countEl.textContent = String(columnTasks.length);

    container.innerHTML = '';
    for (const task of columnTasks) {
      container.appendChild(createTaskCard(task));
    }
  }
}

function createTaskCard(task: EngineTask): HTMLElement {
  const card = document.createElement('div');
  card.className = 'task-card';
  card.draggable = true;
  card.dataset.taskId = task.id;

  const priorityColor = task.priority;

  // Show all assigned agents (multi-agent)
  const agents = task.assigned_agents?.length
    ? task.assigned_agents
    : task.assigned_agent ? [{ agent_id: task.assigned_agent, role: 'lead' }] : [];
  const agentHtml = agents.map(a =>
    `<span class="task-card-agent${a.role === 'lead' ? ' lead' : ''}" title="${escHtml(a.role)}">${escHtml(a.agent_id)}</span>`
  ).join('');

  const cronHtml = task.cron_enabled && task.cron_schedule
    ? `<span class="task-card-cron">🔄 ${escHtml(task.cron_schedule)}</span>`
    : '';
  const timeAgo = formatTimeAgo(task.updated_at || task.created_at);
  
  // Show run button for tasks with agents
  const hasAgents = agents.length > 0;
  const canRun = hasAgents && ['assigned', 'inbox'].includes(task.status);
  const runBtnHtml = canRun
    ? `<button class="task-card-action run-btn" data-action="run" title="Run now">▶</button>`
    : '';
  const agentCountHtml = agents.length > 1 ? `<span class="task-card-agent-count">${agents.length} agents</span>` : '';

  card.innerHTML = `
    <div class="task-card-actions">
      ${runBtnHtml}
      <button class="task-card-action" data-action="edit" title="Edit">✏️</button>
    </div>
    <div class="task-card-title">${escHtml(task.title)}</div>
    <div class="task-card-meta">
      <span class="task-card-priority ${priorityColor}"></span>
      ${agentHtml}
      ${agentCountHtml}
      ${cronHtml}
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
    document.querySelectorAll('.tasks-column-cards.drag-over').forEach(el => el.classList.remove('drag-over'));
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

function renderFeed() {
  const list = $('tasks-feed-list');
  if (!list) return;

  let filtered = _activity;
  if (_feedFilter === 'tasks') {
    filtered = _activity.filter(a => ['created', 'assigned', 'agent_started', 'agent_completed'].includes(a.kind));
  } else if (_feedFilter === 'status') {
    filtered = _activity.filter(a => ['status_change', 'agent_started', 'agent_completed', 'agent_error', 'cron_triggered'].includes(a.kind));
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

function renderStats() {
  const total = $('tasks-stat-total');
  const active = $('tasks-stat-active');
  const cron = $('tasks-stat-cron');
  if (total) total.textContent = `${_tasks.length} tasks`;
  if (active) active.textContent = `${_tasks.filter(t => t.status === 'in_progress').length} active`;
  if (cron) cron.textContent = `${_tasks.filter(t => t.cron_enabled).length} scheduled`;
}

// ── Task Modal ─────────────────────────────────────────────────────────

function openTaskModal(task?: EngineTask) {
  const modal = $('tasks-detail-modal');
  if (!modal) return;

  _editingTask = task || null;
  const isNew = !task;

  const titleEl = $('tasks-modal-title');
  const inputTitle = $('tasks-modal-input-title') as HTMLInputElement;
  const inputDesc = $('tasks-modal-input-desc') as HTMLTextAreaElement;
  const inputPriority = $('tasks-modal-input-priority') as HTMLSelectElement;
  const inputAgent = $('tasks-modal-input-agent') as HTMLSelectElement;
  const inputCron = $('tasks-modal-input-cron') as HTMLInputElement;
  const inputCronEnabled = $('tasks-modal-input-cron-enabled') as HTMLInputElement;
  const deleteBtn = $('tasks-modal-delete');
  const runBtn = $('tasks-modal-run');
  const activitySection = $('tasks-modal-activity-section');

  if (titleEl) titleEl.textContent = isNew ? 'New Task' : 'Edit Task';
  if (inputTitle) inputTitle.value = task?.title || '';
  if (inputDesc) inputDesc.value = task?.description || '';
  if (inputPriority) inputPriority.value = task?.priority || 'medium';
  if (inputCron) inputCron.value = task?.cron_schedule || '';
  if (inputCronEnabled) inputCronEnabled.checked = task?.cron_enabled || false;
  if (deleteBtn) deleteBtn.style.display = isNew ? 'none' : '';
  const hasAgents = task?.assigned_agents?.length || task?.assigned_agent;
  if (runBtn) runBtn.style.display = hasAgents ? '' : 'none';

  // Multi-agent picker: populate selected agents from task
  _modalSelectedAgents = task?.assigned_agents?.length
    ? [...task.assigned_agents]
    : task?.assigned_agent
      ? [{ agent_id: task.assigned_agent, role: 'lead' }]
      : [];
  renderAgentPicker();

  // Also set legacy dropdown for backward compat
  if (inputAgent) {
    inputAgent.innerHTML = '<option value="">+ Add agent</option>';
    for (const agent of _agents) {
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

function renderAgentPicker() {
  const container = $('tasks-modal-agents-tags');
  if (!container) return;
  container.innerHTML = '';
  for (const ta of _modalSelectedAgents) {
    const agent = _agents.find(a => a.id === ta.agent_id);
    const tag = document.createElement('span');
    tag.className = `agent-tag${ta.role === 'lead' ? ' lead' : ''}`;
    tag.innerHTML = `${agent ? spriteAvatar(agent.avatar, 18) + ' ' : ''}${escHtml(ta.agent_id)}${ta.role === 'lead' ? ' ★' : ''}<button class="agent-tag-remove" title="Remove">×</button>`;

    // Click tag → toggle lead/collaborator
    tag.addEventListener('click', (e) => {
      if ((e.target as HTMLElement).classList.contains('agent-tag-remove')) return;
      ta.role = ta.role === 'lead' ? 'collaborator' : 'lead';
      renderAgentPicker();
    });

    // Remove button
    tag.querySelector('.agent-tag-remove')?.addEventListener('click', (e) => {
      e.stopPropagation();
      _modalSelectedAgents = _modalSelectedAgents.filter(a => a.agent_id !== ta.agent_id);
      renderAgentPicker();
    });

    container.appendChild(tag);
  }
  if (!_modalSelectedAgents.length) {
    container.innerHTML = '<span class="agent-tag-empty">No agents assigned</span>';
  }
}

function addAgentToTask(agentId: string) {
  if (!agentId || _modalSelectedAgents.some(a => a.agent_id === agentId)) return;
  const role = _modalSelectedAgents.length === 0 ? 'lead' : 'collaborator';
  _modalSelectedAgents.push({ agent_id: agentId, role });
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

function closeTaskModal() {
  const modal = $('tasks-detail-modal');
  if (modal) modal.style.display = 'none';
  _editingTask = null;
}

async function saveTask() {
  const inputTitle = $('tasks-modal-input-title') as HTMLInputElement;
  const inputDesc = $('tasks-modal-input-desc') as HTMLTextAreaElement;
  const inputPriority = $('tasks-modal-input-priority') as HTMLSelectElement;
  const inputCron = $('tasks-modal-input-cron') as HTMLInputElement;
  const inputCronEnabled = $('tasks-modal-input-cron-enabled') as HTMLInputElement;

  const title = inputTitle?.value.trim();
  if (!title) { showToast('Task title is required', 'warning'); return; }

  const cronSchedule = inputCron?.value.trim() || undefined;
  const cronEnabled = inputCronEnabled?.checked || false;

  // Primary agent = first lead or first agent in the multi-select
  const primaryAgent = _modalSelectedAgents.find(a => a.role === 'lead')
    || _modalSelectedAgents[0];
  const agentId = primaryAgent?.agent_id;

  // Determine status
  let status: TaskStatus = _editingTask?.status || 'inbox';
  if (!_editingTask && _modalSelectedAgents.length > 0) status = 'assigned';

  const now = new Date().toISOString();
  const task: EngineTask = {
    id: _editingTask?.id || crypto.randomUUID(),
    title,
    description: inputDesc?.value || '',
    status,
    priority: (inputPriority?.value || 'medium') as TaskPriority,
    assigned_agent: agentId,
    assigned_agents: _modalSelectedAgents,
    session_id: _editingTask?.session_id,
    cron_schedule: cronSchedule,
    cron_enabled: cronEnabled,
    last_run_at: _editingTask?.last_run_at,
    next_run_at: _editingTask?.next_run_at,
    created_at: _editingTask?.created_at || now,
    updated_at: now,
  };

  try {
    if (_editingTask) {
      await pawEngine.taskUpdate(task);
      // Save multi-agent assignments
      if (_modalSelectedAgents.length > 0) {
        await pawEngine.taskSetAgents(task.id, _modalSelectedAgents);
      }
      showToast('Task updated', 'success');
    } else {
      await pawEngine.taskCreate(task);
      // Save multi-agent assignments for new task
      if (_modalSelectedAgents.length > 0) {
        await pawEngine.taskSetAgents(task.id, _modalSelectedAgents);
      }
      showToast('Task created', 'success');
    }
    closeTaskModal();
    await loadTasks();
  } catch (e) {
    showToast(`Failed: ${e instanceof Error ? e.message : e}`, 'error');
  }
}

async function deleteTask() {
  if (!_editingTask) return;
  try {
    await pawEngine.taskDelete(_editingTask.id);
    showToast('Task deleted', 'success');
    closeTaskModal();
    await loadTasks();
  } catch (e) {
    showToast(`Failed: ${e instanceof Error ? e.message : e}`, 'error');
  }
}

async function runTask(taskId: string) {
  try {
    showToast('Starting agent work...', 'info');
    await pawEngine.taskRun(taskId);
    showToast('Agent is working on the task', 'success');
    await loadTasks();
  } catch (e) {
    showToast(`Run failed: ${e instanceof Error ? e.message : e}`, 'error');
  }
}

// ── Drag & Drop ────────────────────────────────────────────────────────

function setupDragAndDrop() {
  document.querySelectorAll<HTMLElement>('.tasks-column-cards').forEach(column => {
    column.addEventListener('dragover', (e) => {
      e.preventDefault();
      if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
      column.classList.add('drag-over');
    });

    column.addEventListener('dragleave', (e) => {
      // Only remove if leaving the column container itself
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

      // Find the task
      const task = _tasks.find(t => t.id === taskId);
      if (!task || task.status === newStatus) return;

      try {
        await pawEngine.taskMove(taskId, newStatus);
        await loadTasks();
      } catch (err) {
        showToast(`Move failed: ${err instanceof Error ? err.message : err}`, 'error');
      }
    });
  });
}

// ── Cron Timer ─────────────────────────────────────────────────────────

let _cronInterval: ReturnType<typeof setInterval> | null = null;

export function startCronTimer() {
  if (_cronInterval) return;
  // Check for due cron tasks every 30 seconds.
  // The backend heartbeat (60s) handles actual execution.
  // The frontend tick just updates cron timestamps and refreshes the board.
  _cronInterval = setInterval(async () => {
    try {
      const triggered = await pawEngine.tasksCronTick();
      if (triggered.length > 0) {
        showToast(`${triggered.length} cron task(s) triggered`, 'info');
        // Backend heartbeat will execute these — just refresh the board.
        // The dedup guard prevents double-execution if both fire close together.
        for (const taskId of triggered) {
          try {
            await pawEngine.taskRun(taskId);
          } catch (e) {
            // Expected: "already running" from dedup guard is fine
            if (!String(e).includes('already running')) {
              console.warn('[tasks] Auto-run failed for', taskId, e);
            }
          }
        }
        await loadTasks();
      }
    } catch (e) {
      console.warn('[tasks] Cron tick failed:', e);
    }
  }, 30_000);
}

export function stopCronTimer() {
  if (_cronInterval) {
    clearInterval(_cronInterval);
    _cronInterval = null;
  }
}

// ── Helpers ────────────────────────────────────────────────────────────

function getAgentAvatar(agentId?: string | null): string {
  if (!agentId) return '<span class="ms">build</span>';
  const agent = _agents.find(a => a.id === agentId || a.name === agentId);
  return agent ? spriteAvatar(agent.avatar, 20) : '<span class="ms">smart_toy</span>';
}

function formatTimeAgo(dateStr: string): string {
  if (!dateStr) return '';
  try {
    const d = new Date(dateStr);
    const now = Date.now();
    const diff = now - d.getTime();
    if (diff < 60_000) return 'just now';
    if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
    if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
    if (diff < 604_800_000) return `${Math.floor(diff / 86_400_000)}d ago`;
    return d.toLocaleDateString();
  } catch {
    return '';
  }
}

// ── Event Binding ──────────────────────────────────────────────────────

export function bindTaskEvents() {
  // New task button
  $('tasks-add-btn')?.addEventListener('click', () => openTaskModal());

  // Column add buttons
  document.querySelectorAll<HTMLElement>('.tasks-column-add').forEach(btn => {
    btn.addEventListener('click', () => openTaskModal());
  });

  // Modal controls
  $('tasks-modal-close')?.addEventListener('click', closeTaskModal);
  $('tasks-modal-save')?.addEventListener('click', saveTask);
  $('tasks-modal-delete')?.addEventListener('click', deleteTask);
  $('tasks-modal-run')?.addEventListener('click', () => {
    if (_editingTask) {
      closeTaskModal();
      runTask(_editingTask.id);
    }
  });

  // Agent dropdown → add agent tag
  $('tasks-modal-input-agent')?.addEventListener('change', () => {
    const sel = $('tasks-modal-input-agent') as HTMLSelectElement;
    if (sel?.value) {
      addAgentToTask(sel.value);
      sel.value = ''; // reset dropdown
    }
  });

  // Modal backdrop close
  $('tasks-detail-modal')?.addEventListener('click', (e) => {
    if ((e.target as HTMLElement).classList.contains('tasks-modal-overlay')) {
      closeTaskModal();
    }
  });

  // Feed tabs
  document.querySelectorAll<HTMLElement>('.tasks-feed-tab').forEach(tab => {
    tab.addEventListener('click', () => {
      document.querySelectorAll('.tasks-feed-tab').forEach(t => t.classList.remove('active'));
      tab.classList.add('active');
      _feedFilter = (tab.dataset.feed as typeof _feedFilter) || 'all';
      renderFeed();
    });
  });

  // Drag & drop
  setupDragAndDrop();
}
