// Tasks Hub — Index (orchestration, state, exports)
// Agents pick up tasks, work on them autonomously, move them through columns.
// Supports drag-and-drop, live feed, cron scheduling, and agent auto-work.

import { pawEngine, type EngineTask, type EngineTaskActivity, type TaskAgent } from '../../engine';
import { showToast } from '../../components/toast';
import { $ } from '../../components/helpers';
import * as SquadsModule from '../squads';
import {
  setMoleculesState,
  renderBoard,
  renderFeed,
  renderStats,
  openTaskModal,
  closeTaskModal,
  saveTask,
  deleteTask,
  runTask,
  addAgentToTask,
  setupDragAndDrop,
} from './molecules';

// ── Module state ───────────────────────────────────────────────────────────

let _tasks: EngineTask[] = [];
let _activity: EngineTaskActivity[] = [];
let _editingTask: EngineTask | null = null;
let _feedFilter: 'all' | 'tasks' | 'status' = 'all';
let _agents: { id: string; name: string; avatar: string }[] = [];
let _modalSelectedAgents: TaskAgent[] = [];
let _cronInterval: ReturnType<typeof setInterval> | null = null;

// ── State bridge for molecules ─────────────────────────────────────────────

function initMoleculesState() {
  setMoleculesState({
    getTasks: () => _tasks,
    getActivity: () => _activity,
    getEditingTask: () => _editingTask,
    setEditingTask: (t: EngineTask | null) => {
      _editingTask = t;
    },
    getFeedFilter: () => _feedFilter,
    setFeedFilter: (f: 'all' | 'tasks' | 'status') => {
      _feedFilter = f;
    },
    getAgents: () => _agents,
    getModalSelectedAgents: () => _modalSelectedAgents,
    setModalSelectedAgents: (agents: TaskAgent[]) => {
      _modalSelectedAgents = agents;
    },
    reload: () => loadTasks(),
  });
}

// ── Public API ─────────────────────────────────────────────────────────────

export async function loadTasks() {
  initMoleculesState();
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
  loadTasks();
}

// ── Cron Timer ─────────────────────────────────────────────────────────────

export function startCronTimer() {
  if (_cronInterval) return;
  _cronInterval = setInterval(async () => {
    try {
      const triggered = await pawEngine.tasksCronTick();
      if (triggered.length > 0) {
        showToast(`${triggered.length} cron task(s) triggered`, 'info');
        for (const taskId of triggered) {
          try {
            await pawEngine.taskRun(taskId);
          } catch (e) {
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

// ── Event Binding ──────────────────────────────────────────────────────────

export function bindTaskEvents() {
  initMoleculesState();

  // ── Kinetic stagger on side panel cards ─────────────────────────────
  document.querySelectorAll('.tasks-side-panel .tasks-panel-card').forEach((card, i) => {
    (card as HTMLElement).style.animationDelay = `${i * 60}ms`;
  });

  // New task button (hero + quick action)
  $('tasks-add-btn')?.addEventListener('click', () => openTaskModal());
  $('tasks-qa-new-task')?.addEventListener('click', () => openTaskModal());

  // Empty state create button
  $('tasks-empty-create')?.addEventListener('click', () => openTaskModal());

  // Quick action: new automation (switches to scheduled tab)
  $('tasks-qa-new-automation')?.addEventListener('click', () => {
    switchTab('scheduled');
    $('add-cron-btn')?.click();
  });

  // Column add buttons
  document.querySelectorAll<HTMLElement>('.tasks-column-add').forEach((btn) => {
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
      sel.value = '';
    }
  });

  // Modal backdrop close
  $('tasks-detail-modal')?.addEventListener('click', (e) => {
    if ((e.target as HTMLElement).classList.contains('tasks-modal-overlay')) {
      closeTaskModal();
    }
  });

  // Feed tabs
  document.querySelectorAll<HTMLElement>('.tasks-feed-tab').forEach((tab) => {
    tab.addEventListener('click', () => {
      document.querySelectorAll('.tasks-feed-tab').forEach((t) => t.classList.remove('active'));
      tab.classList.add('active');
      _feedFilter = (tab.dataset.feed as typeof _feedFilter) || 'all';
      renderFeed();
    });
  });

  // Drag & drop
  setupDragAndDrop();

  // Tab switching (Board | Scheduled)
  document.querySelectorAll<HTMLElement>('[data-tasks-tab]').forEach((tab) => {
    tab.addEventListener('click', () => {
      switchTab(tab.dataset.tasksTab ?? 'board');
    });
  });
}

// ── Tab Panel Switching ────────────────────────────────────────────────────

export function switchTab(tabName: string) {
  document.querySelectorAll<HTMLElement>('[data-tasks-tab]').forEach((t) => {
    t.classList.toggle('active', t.dataset.tasksTab === tabName);
  });
  const panels = ['board', 'scheduled', 'projects', 'squads'];
  for (const panel of panels) {
    const el = $(`tasks-tab-${panel}`);
    if (el) el.style.display = panel === tabName ? '' : 'none';
  }
  // Load squads content into embedded container when that tab is selected
  if (tabName === 'squads') {
    const container = $('tasks-squads-container');
    const sourceView = $('squads-view');
    if (container && sourceView && !container.hasChildNodes()) {
      // Move the squads view content into the tasks tab container
      while (sourceView.firstChild) {
        container.appendChild(sourceView.firstChild);
      }
    }
    SquadsModule.loadSquads();
  }
}

// ── Re-exports ─────────────────────────────────────────────────────────────

export { openTaskModal } from './molecules';
