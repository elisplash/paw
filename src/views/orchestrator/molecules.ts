// Orchestrator View — DOM rendering + IPC

import {
  pawEngine,
  type EngineProject,
  type EngineProjectAgent,
  type EngineProjectMessage,
} from '../../engine';
import { showToast } from '../../components/toast';
import {
  populateModelSelect,
  escHtml,
  formatTimeAgo,
  confirmModal,
} from '../../components/helpers';
import { specialtyIcon, messageKindLabel, formatTime } from './atoms';

// ── State bridge ──────────────────────────────────────────────────────

interface MoleculesState {
  getProjects: () => EngineProject[];
  setProjects: (p: EngineProject[]) => void;
  getCurrentProject: () => EngineProject | null;
  setCurrentProject: (p: EngineProject | null) => void;
  getMessagePollInterval: () => ReturnType<typeof setInterval> | null;
  setMessagePollInterval: (i: ReturnType<typeof setInterval> | null) => void;
  getLoadProjects: () => typeof _loadProjectsFn;
}

let _state: MoleculesState;
// placeholder for loadProjects from index - resolved via state bridge
const _loadProjectsFn: () => Promise<void> = async () => {};

export function initMoleculesState() {
  return {
    setMoleculesState(s: MoleculesState) {
      _state = s;
    },
  };
}

// ── DOM refs ──────────────────────────────────────────────────────────

const els = {
  get list() {
    return document.getElementById('orch-project-list')!;
  },
  get detail() {
    return document.getElementById('orch-project-detail')!;
  },
  get statTotal() {
    return document.getElementById('orch-stat-total')!;
  },
  get statRunning() {
    return document.getElementById('orch-stat-running')!;
  },
  get detailName() {
    return document.getElementById('orch-detail-name')!;
  },
  get detailStatus() {
    return document.getElementById('orch-detail-status')!;
  },
  get detailGoal() {
    return document.getElementById('orch-detail-goal')!;
  },
  get agentRoster() {
    return document.getElementById('orch-agent-roster')!;
  },
  get messageBus() {
    return document.getElementById('orch-message-bus')!;
  },
  get modal() {
    return document.getElementById('orch-modal')!;
  },
  get modalTitle() {
    return document.getElementById('orch-modal-title')!;
  },
  get formTitle() {
    return document.getElementById('orch-form-title') as HTMLInputElement;
  },
  get formGoal() {
    return document.getElementById('orch-form-goal') as HTMLTextAreaElement;
  },
  get formBoss() {
    return document.getElementById('orch-form-boss') as HTMLInputElement;
  },
  get agentModal() {
    return document.getElementById('orch-agent-modal')!;
  },
  get agentFormId() {
    return document.getElementById('orch-agent-form-id') as HTMLInputElement;
  },
  get agentFormSpecialty() {
    return document.getElementById('orch-agent-form-specialty') as HTMLSelectElement;
  },
  get agentFormModel() {
    return document.getElementById('orch-agent-form-model') as HTMLSelectElement | null;
  },
};

// ── Render list ───────────────────────────────────────────────────────

export function renderList() {
  const projects = _state.getProjects();
  const container = els.list;
  if (projects.length === 0) {
    container.innerHTML = `
      <div class="empty-state">
        <span class="ms" style="font-size:48px">account_tree</span>
        <h3>No projects yet</h3>
        <p>Create a project to orchestrate multiple agents working together on a shared goal.</p>
      </div>`;
    return;
  }

  container.innerHTML = projects
    .map(
      (p) => `
    <div class="orch-project-card" data-id="${p.id}">
      <div class="orch-card-header">
        <span class="orch-card-title">${escHtml(p.title)}</span>
        <span class="orch-status-badge orch-status-${p.status}">${p.status}</span>
      </div>
      <div class="orch-card-goal">${escHtml(p.goal.substring(0, 120))}${p.goal.length > 120 ? '...' : ''}</div>
      <div class="orch-card-footer">
        <span class="orch-card-agents">${p.agents.length} agent${p.agents.length !== 1 ? 's' : ''}</span>
        <span class="orch-card-time">${formatTimeAgo(p.updated_at)}</span>
      </div>
    </div>
  `,
    )
    .join('');

  container.querySelectorAll('.orch-project-card').forEach((card) => {
    card.addEventListener('click', () => {
      const id = (card as HTMLElement).dataset.id!;
      loadProjectDetail(id);
    });
  });
}

export function updateStats() {
  const projects = _state.getProjects();
  els.statTotal.textContent = `${projects.length} project${projects.length !== 1 ? 's' : ''}`;
  const running = projects.filter((p) => p.status === 'running').length;
  els.statRunning.textContent = `${running} running`;
}

// ── Project detail ────────────────────────────────────────────────────

export async function loadProjectDetail(projectId: string) {
  try {
    const projects = await pawEngine.projectsList();
    _state.setProjects(projects);
    const project = projects.find((p) => p.id === projectId);
    if (!project) {
      showToast('Project not found', 'error');
      return;
    }
    _state.setCurrentProject(project);

    els.list.style.display = 'none';
    els.detail.style.display = 'block';

    els.detailName.textContent = project.title;
    els.detailStatus.textContent = project.status;
    els.detailStatus.className = `orch-status-badge orch-status-${project.status}`;
    els.detailGoal.textContent = project.goal;

    const runBtn = document.getElementById('orch-run-btn')!;
    if (project.status === 'running') {
      runBtn.textContent = '⏳ Running...';
      (runBtn as HTMLButtonElement).disabled = true;
    } else {
      runBtn.textContent = '▶ Run';
      (runBtn as HTMLButtonElement).disabled = false;
    }

    renderAgentRoster(project.agents);
    await refreshMessages();

    const oldInterval = _state.getMessagePollInterval();
    if (oldInterval) clearInterval(oldInterval);
    _state.setMessagePollInterval(
      setInterval(() => {
        if (_state.getCurrentProject()) refreshMessages();
      }, 3000),
    );
  } catch (e) {
    console.error('[orchestrator] Failed to load project detail:', e);
  }
}

export function showList() {
  _state.setCurrentProject(null);
  const interval = _state.getMessagePollInterval();
  if (interval) {
    clearInterval(interval);
    _state.setMessagePollInterval(null);
  }
  els.detail.style.display = 'none';
  els.list.style.display = 'block';
  _state.getLoadProjects()();
}

// ── Agent roster ──────────────────────────────────────────────────────

export function renderAgentRoster(agents: EngineProjectAgent[]) {
  if (agents.length === 0) {
    els.agentRoster.innerHTML = '<div class="empty-state-sm">No agents assigned yet.</div>';
    return;
  }

  els.agentRoster.innerHTML = agents
    .map(
      (a) => `
    <div class="orch-agent-card orch-agent-${a.status}">
      <div class="orch-agent-header">
        <span class="orch-agent-icon">${specialtyIcon(a.specialty)}</span>
        <span class="orch-agent-name">${escHtml(a.agent_id)}</span>
        <span class="orch-agent-role-badge">${a.role}</span>
        <span class="orch-agent-status-dot orch-dot-${a.status}" title="${a.status}"></span>
        <button class="btn btn-ghost btn-xs orch-remove-agent" data-agent="${a.agent_id}" title="Remove">×</button>
      </div>
      <div class="orch-agent-meta">
        <span class="orch-agent-specialty">${a.specialty}</span>
        ${a.model ? `<span class="orch-agent-model" title="Model: ${escHtml(a.model)}">${escHtml(a.model)}</span>` : ''}
        ${a.current_task ? `<span class="orch-agent-task" title="${escHtml(a.current_task)}">${escHtml(a.current_task.substring(0, 60))}</span>` : ''}
      </div>
    </div>
  `,
    )
    .join('');

  els.agentRoster.querySelectorAll('.orch-remove-agent').forEach((btn) => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const agentId = (btn as HTMLElement).dataset.agent!;
      const currentProject = _state.getCurrentProject();
      if (!currentProject) return;
      const newAgents = currentProject.agents.filter((a) => a.agent_id !== agentId);
      await pawEngine.projectSetAgents(currentProject.id, newAgents);
      currentProject.agents = newAgents;
      renderAgentRoster(newAgents);
      showToast(`Removed agent ${agentId}`);
    });
  });
}

async function refreshAgents() {
  const currentProject = _state.getCurrentProject();
  if (!currentProject) return;
  try {
    const projects = await pawEngine.projectsList();
    _state.setProjects(projects);
    const p = projects.find((p) => p.id === currentProject.id);
    if (p) {
      _state.setCurrentProject(p);
      renderAgentRoster(p.agents);
    }
  } catch {
    // Ignore refresh errors
  }
}

// ── Messages ──────────────────────────────────────────────────────────

async function refreshMessages() {
  const currentProject = _state.getCurrentProject();
  if (!currentProject) return;
  try {
    const messages = await pawEngine.projectMessages(currentProject.id, 100);
    renderMessageBus(messages);
  } catch {
    // Ignore refresh errors
  }
}

function renderMessageBus(messages: EngineProjectMessage[]) {
  if (messages.length === 0) {
    els.messageBus.innerHTML =
      '<div class="empty-state-sm">No messages yet. Run the project to see agent communication.</div>';
    return;
  }

  els.messageBus.innerHTML = messages
    .map((m) => {
      const kindClass = `orch-msg-${m.kind}`;
      const arrow = m.to_agent ? ` → ${escHtml(m.to_agent)}` : ' (broadcast)';
      return `
      <div class="orch-message ${kindClass}">
        <div class="orch-msg-header">
          <span class="orch-msg-kind">${messageKindLabel(m.kind)}</span>
          <span class="orch-msg-from">${escHtml(m.from_agent)}${arrow}</span>
          <span class="orch-msg-time">${formatTime(m.created_at)}</span>
        </div>
        <div class="orch-msg-content">${escHtml(m.content)}</div>
      </div>
    `;
    })
    .join('');

  els.messageBus.scrollTop = els.messageBus.scrollHeight;
}

// ── Create/Edit project ───────────────────────────────────────────────

let editingProjectId: string | null = null;

export function openCreateModal() {
  editingProjectId = null;
  document.getElementById('orch-modal-title')!.textContent = 'New Project';
  document.getElementById('orch-modal-save')!.textContent = 'Create Project';
  els.formTitle.value = '';
  els.formGoal.value = '';
  els.formBoss.value = 'default';
  els.modal.style.display = 'flex';
}

export function editProject() {
  const currentProject = _state.getCurrentProject();
  if (!currentProject) return;
  editingProjectId = currentProject.id;
  document.getElementById('orch-modal-title')!.textContent = 'Edit Project';
  document.getElementById('orch-modal-save')!.textContent = 'Save Changes';
  els.formTitle.value = currentProject.title;
  els.formGoal.value = currentProject.goal;
  els.formBoss.value = currentProject.boss_agent;
  els.modal.style.display = 'flex';
}

export function closeModal() {
  els.modal.style.display = 'none';
  editingProjectId = null;
}

export async function saveProject() {
  const title = els.formTitle.value.trim();
  const goal = els.formGoal.value.trim();
  const boss = els.formBoss.value.trim() || 'default';

  if (!title) {
    showToast('Project title is required', 'error');
    return;
  }
  if (!goal) {
    showToast('Project goal is required', 'error');
    return;
  }

  try {
    const projects = _state.getProjects();
    const currentProject = _state.getCurrentProject();

    if (editingProjectId) {
      const existing = projects.find((p) => p.id === editingProjectId);
      if (existing) {
        const updated: EngineProject = {
          ...existing,
          title,
          goal,
          boss_agent: boss,
        };
        await pawEngine.projectUpdate(updated);
        showToast('Project updated');
        if (currentProject) {
          await loadProjectDetail(editingProjectId);
        }
      }
    } else {
      const project: EngineProject = {
        id: crypto.randomUUID(),
        title,
        goal,
        status: 'planning',
        boss_agent: boss,
        agents: [
          {
            agent_id: boss,
            role: 'boss',
            specialty: 'general',
            status: 'idle',
            current_task: undefined,
          },
        ],
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
      };
      await pawEngine.projectCreate(project);
      await pawEngine.projectSetAgents(project.id, project.agents);
      showToast('Project created');
    }

    closeModal();
    await _state.getLoadProjects()();
  } catch (e: unknown) {
    showToast(`Error: ${e instanceof Error ? e.message : String(e)}`, 'error');
  }
}

// ── Delete & Run ──────────────────────────────────────────────────────

export async function deleteProject() {
  const currentProject = _state.getCurrentProject();
  if (!currentProject) return;
  if (!(await confirmModal(`Delete project "${currentProject.title}"? This cannot be undone.`)))
    return;

  try {
    await pawEngine.projectDelete(currentProject.id);
    showToast('Project deleted');
    showList();
  } catch (e: unknown) {
    showToast(`Error: ${e instanceof Error ? e.message : String(e)}`, 'error');
  }
}

export async function runProject() {
  const currentProject = _state.getCurrentProject();
  if (!currentProject) return;
  if (currentProject.status === 'running') {
    showToast('Project is already running', 'error');
    return;
  }
  if (currentProject.agents.length === 0) {
    showToast('Add at least one agent before running', 'error');
    return;
  }

  try {
    await pawEngine.projectRun(currentProject.id);
    showToast('Project started! The boss agent is orchestrating.');

    const runBtn = document.getElementById('orch-run-btn')!;
    runBtn.textContent = '⏳ Running...';
    (runBtn as HTMLButtonElement).disabled = true;
    els.detailStatus.textContent = 'running';
    els.detailStatus.className = 'orch-status-badge orch-status-running';
  } catch (e: unknown) {
    showToast(`Error: ${e instanceof Error ? e.message : String(e)}`, 'error');
  }
}

// ── Agent modal ───────────────────────────────────────────────────────

export function openAgentModal() {
  els.agentFormId.value = '';
  els.agentFormSpecialty.value = 'general';
  if (els.agentFormModel) {
    pawEngine
      .getConfig()
      .then((config) => {
        populateModelSelect(els.agentFormModel!, config.providers ?? [], {
          defaultLabel: '(use routing default)',
          currentValue: '',
        });
      })
      .catch(() => {});
  }
  els.agentModal.style.display = 'flex';
}

export function closeAgentModal() {
  els.agentModal.style.display = 'none';
}

export async function addAgent() {
  const currentProject = _state.getCurrentProject();
  if (!currentProject) return;
  const agentId = els.agentFormId.value.trim();
  const specialty = els.agentFormSpecialty.value;

  if (!agentId) {
    showToast('Agent ID is required', 'error');
    return;
  }

  if (currentProject.agents.some((a) => a.agent_id === agentId)) {
    showToast('This agent is already on the team', 'error');
    return;
  }

  const newAgent: EngineProjectAgent = {
    agent_id: agentId,
    role: 'worker',
    specialty,
    status: 'idle',
    current_task: undefined,
    model: els.agentFormModel?.value || undefined,
  };

  const allAgents = [...currentProject.agents, newAgent];
  try {
    await pawEngine.projectSetAgents(currentProject.id, allAgents);
    currentProject.agents = allAgents;
    renderAgentRoster(allAgents);
    closeAgentModal();
    showToast(`Added agent ${agentId}`);
  } catch (e: unknown) {
    showToast(`Error: ${e instanceof Error ? e.message : String(e)}`, 'error');
  }
}

// ── Refresh helpers (exported for event handler) ──────────────────────

export { refreshAgents, refreshMessages };
