// Automations / Cron View — Orchestration, state, exports

import { $ } from '../../components/helpers';
import {
  initMoleculesState,
  loadCron,
  openCreateModal,
  hideCronModal,
  saveCronJob,
  createMorningBrief,
} from './molecules';

// ── State ─────────────────────────────────────────────────────────────

let _agents: { id: string; name: string; avatar: string }[] = [];
let _editingTaskId: string | null = null;

// ── State bridge ──────────────────────────────────────────────────────

const { setMoleculesState } = initMoleculesState();
setMoleculesState({
  getAgents: () => _agents,
  getEditingTaskId: () => _editingTaskId,
  setEditingTaskId: (id: string | null) => {
    _editingTaskId = id;
  },
});

// ── Public API ────────────────────────────────────────────────────────

export function setAgents(agents: { id: string; name: string; avatar: string }[]) {
  _agents = agents;
}

export { loadCron };

export function initAutomations() {
  $('add-cron-btn')?.addEventListener('click', openCreateModal);
  $('cron-empty-add')?.addEventListener('click', openCreateModal);
  $('add-morning-brief-btn')?.addEventListener('click', createMorningBrief);
  $('cron-empty-morning-brief')?.addEventListener('click', createMorningBrief);
  $('cron-modal-close')?.addEventListener('click', hideCronModal);
  $('cron-modal-cancel')?.addEventListener('click', hideCronModal);

  $('cron-form-schedule-preset')?.addEventListener('change', () => {
    const preset = ($('cron-form-schedule-preset') as HTMLSelectElement).value;
    const scheduleInput = $('cron-form-schedule') as HTMLInputElement;
    if (preset && scheduleInput) scheduleInput.value = preset;
  });

  $('cron-modal-save')?.addEventListener('click', saveCronJob);

  // Listen for heartbeat events from backend
  (async () => {
    try {
      const { listen } = await import('@tauri-apps/api/event');
      listen('cron-heartbeat', () => {
        const view = $('automations-view');
        if (view && view.style.display !== 'none') {
          loadCron();
        }
      });
    } catch {
      /* not in Tauri context */
    }
  })();
}
