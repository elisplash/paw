// Orchestrator View — Orchestration, state, exports

import type { EngineProject } from '../../engine';
import { listen } from '@tauri-apps/api/event';
import {
  initMoleculesState,
  renderList,
  updateStats,
  loadProjectDetail,
  showList,
  openCreateModal,
  closeModal,
  saveProject,
  editProject,
  deleteProject,
  runProject,
  openAgentModal,
  closeAgentModal,
  addAgent,
  refreshAgents,
  refreshMessages,
} from './molecules';

// ── State ─────────────────────────────────────────────────────────────

let projects: EngineProject[] = [];
let currentProject: EngineProject | null = null;
let messagePollInterval: ReturnType<typeof setInterval> | null = null;

// ── State bridge ──────────────────────────────────────────────────────

const { setMoleculesState } = initMoleculesState();
setMoleculesState({
  getProjects: () => projects,
  setProjects: (p: EngineProject[]) => {
    projects = p;
  },
  getCurrentProject: () => currentProject,
  setCurrentProject: (p: EngineProject | null) => {
    currentProject = p;
  },
  getMessagePollInterval: () => messagePollInterval,
  setMessagePollInterval: (i: ReturnType<typeof setInterval> | null) => {
    messagePollInterval = i;
  },
  getLoadProjects: () => loadProjects,
});

// ── Init ──────────────────────────────────────────────────────────────

export function initOrchestrator() {
  document.getElementById('orch-create-btn')?.addEventListener('click', () => openCreateModal());
  document.getElementById('orch-modal-close')?.addEventListener('click', () => closeModal());
  document.getElementById('orch-modal-cancel')?.addEventListener('click', () => closeModal());
  document.getElementById('orch-modal-save')?.addEventListener('click', () => saveProject());
  document.getElementById('orch-back-btn')?.addEventListener('click', () => showList());
  document.getElementById('orch-run-btn')?.addEventListener('click', () => runProject());
  document.getElementById('orch-edit-btn')?.addEventListener('click', () => editProject());
  document.getElementById('orch-delete-btn')?.addEventListener('click', () => deleteProject());
  document.getElementById('orch-add-agent-btn')?.addEventListener('click', () => openAgentModal());
  document
    .getElementById('orch-agent-modal-close')
    ?.addEventListener('click', () => closeAgentModal());
  document
    .getElementById('orch-agent-modal-cancel')
    ?.addEventListener('click', () => closeAgentModal());
  document.getElementById('orch-agent-modal-save')?.addEventListener('click', () => addAgent());

  listen<Record<string, unknown>>('project-event', (event) => {
    const data = event.payload;
    if (
      data.kind === 'project_started' ||
      data.kind === 'project_finished' ||
      data.kind === 'project_complete'
    ) {
      loadProjects();
      if (currentProject && currentProject.id === data.project_id) {
        loadProjectDetail(currentProject.id);
      }
    }
    if (
      data.kind === 'delegation' ||
      data.kind === 'progress' ||
      data.kind === 'message' ||
      data.kind === 'agent_finished'
    ) {
      if (currentProject && currentProject.id === data.project_id) {
        refreshMessages();
        refreshAgents();
      }
    }
  });
}

// ── Load projects ─────────────────────────────────────────────────────

export async function loadProjects() {
  try {
    const { pawEngine } = await import('../../engine');
    projects = await pawEngine.projectsList();
    renderList();
    updateStats();
  } catch (e) {
    console.error('[orchestrator] Failed to load projects:', e);
  }
}

/** Clear the message-poll interval (call on view unmount). */
export function stopMessagePoll() {
  if (messagePollInterval) {
    clearInterval(messagePollInterval);
    messagePollInterval = null;
  }
}

// ── Re-exports ────────────────────────────────────────────────────────

export { specialtyIcon, messageKindLabel } from './atoms';
