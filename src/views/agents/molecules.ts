// molecules.ts — DOM rendering for the agents grid
// Depends on: atoms, helpers, toast, agent-policies

import { $, escHtml } from '../../components/helpers';
import { getAgentPolicy } from '../../features/agent-policies';
import { type Agent, spriteAvatar } from './atoms';
import { kineticRow } from '../../components/kinetic-row';

let _createBtnBound = false;

export interface RenderAgentsCallbacks {
  onChat: (agentId: string) => void;
  onMiniChat: (agentId: string) => void;
  onEdit: (agentId: string) => void;
  onCreate: () => void;
}

// Track which view mode is active
let _viewMode: 'roster' | 'grid' = 'roster';

export function renderAgents(agents: Agent[], cbs: RenderAgentsCallbacks) {
  console.debug('[agents] renderAgents called');
  const grid = $('agents-grid');
  console.debug('[agents] grid element:', grid);
  if (!grid) return;

  // Guided empty state when no agents exist
  if (agents.length === 0) {
    grid.innerHTML = `
      <div class="empty-state" style="grid-column:1/-1">
        <div class="empty-icon"><span class="ms" style="font-size:48px">smart_toy</span></div>
        <div class="empty-title">Create your first AI agent</div>
        <div class="empty-subtitle">Agents are AI personas you configure with a model, personality, skills, and boundaries. Each agent can handle different tasks.</div>
        <div class="empty-features">
          <div class="empty-feature-item"><span class="ms ms-sm">check_circle</span> Choose from templates: general, research, creative, technical</div>
          <div class="empty-feature-item"><span class="ms ms-sm">check_circle</span> Assign tools and skills per agent</div>
          <div class="empty-feature-item"><span class="ms ms-sm">check_circle</span> Set personality and communication style</div>
          <div class="empty-feature-item"><span class="ms ms-sm">check_circle</span> Add boundaries and safety rules</div>
        </div>
        <div class="empty-actions">
          <button class="btn btn-primary" id="agents-empty-create"><span class="ms ms-sm">add</span> Create Agent</button>
        </div>
        <div class="empty-hint">You'll need an AI provider configured in Settings first</div>
      </div>
    `;
    grid.querySelector('#agents-empty-create')?.addEventListener('click', () => cbs.onCreate());
    return;
  }

  // Render toggle buttons into the section header slot
  const toggleSlot = document.getElementById('agents-toggle-btns');
  if (toggleSlot) {
    toggleSlot.innerHTML = `
      <button class="agents-toggle-btn${_viewMode === 'roster' ? ' active' : ''}" data-mode="roster">Roster</button>
      <button class="agents-toggle-btn${_viewMode === 'grid' ? ' active' : ''}" data-mode="grid">Grid</button>
    `;
  }

  if (_viewMode === 'roster') {
    // Roster/table view (default)
    grid.classList.remove('agents-grid-cards');
    grid.classList.add('agents-grid-roster');
    grid.innerHTML = `
    <table class="agents-roster">
      <thead>
        <tr>
          <th>STAT</th>
          <th>AGENT</th>
          <th>MODEL</th>
          <th>TOOLS</th>
          <th>LAST ACTIVE</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        ${agents
          .map((agent) => {
            const p = getAgentPolicy(agent.id);
            const toolCount = p.mode === 'unrestricted' ? 'All' : String(p.allowed.length);
            const lastUsed = agent.lastUsed ? _timeAgo(new Date(agent.lastUsed)) : '—';
            const isActive =
              agent.lastUsed && Date.now() - new Date(agent.lastUsed).getTime() < 600000;
            return `<tr class="agents-roster-row" data-id="${agent.id}">
            <td class="roster-stat">${isActive ? '<span class="roster-dot-active">◉</span>' : '<span class="roster-dot-idle">○</span>'}</td>
            <td class="roster-name">${escHtml(agent.name)}</td>
            <td class="roster-model">${escHtml(agent.model || '—')}</td>
            <td class="roster-tools">${toolCount}</td>
            <td class="roster-time">${lastUsed}</td>
            <td class="roster-actions">
              <button class="btn btn-primary btn-sm agent-chat-btn">Chat</button>
              <button class="btn btn-ghost btn-sm agent-edit-btn">Edit</button>
            </td>
          </tr>`;
          })
          .join('')}
      </tbody>
      <tfoot>
        <tr class="agents-roster-footer-row">
          <td colspan="6">
            <button class="btn btn-ghost btn-sm" id="agent-card-new">
              <span class="ms ms-sm">add</span> New Agent
            </button>
          </td>
        </tr>
      </tfoot>
    </table>`;
  } else {
    // Grid view (compact cards, no bio)
    grid.classList.remove('agents-grid-roster');
    grid.classList.add('agents-grid-cards');
    grid.innerHTML = `${agents
      .map(
        (agent) => `
      <div class="agent-card k-row k-spring k-materialise${agent.source === 'backend' ? ' agent-card-backend' : ''}" data-id="${agent.id}">
        <div class="agent-card-header">
          <div class="agent-avatar" style="background:${agent.color}">${spriteAvatar(agent.avatar, 48)}</div>
          <div class="agent-info">
            <div class="agent-name">${escHtml(agent.name)}</div>
            <div class="agent-template">${agent.model || (agent.source === 'backend' ? 'AI-Created' : agent.template.charAt(0).toUpperCase() + agent.template.slice(1))}</div>
          </div>
          <button class="btn-icon agent-menu-btn" title="Options">⋮</button>
        </div>
        <div class="agent-actions">
          <button class="btn btn-primary btn-sm agent-chat-btn">Chat</button>
          <button class="btn btn-ghost btn-sm agent-minichat-btn" title="Open mini chat window"><span class="ms ms-sm">chat</span></button>
          <button class="btn btn-ghost btn-sm agent-edit-btn">Edit</button>
        </div>
      </div>
    `,
      )
      .join('')}
      <div class="agent-card agent-card-new" id="agent-card-new">
        <div class="agent-card-new-icon">+</div>
        <div class="agent-card-new-label">Create Agent</div>
      </div>
    `;
  }

  // Bind view toggle (buttons are in the section header, not in the grid)
  const toggleContainer = document.getElementById('agents-toggle-btns');
  if (toggleContainer) {
    toggleContainer.querySelectorAll('.agents-toggle-btn').forEach((btn) => {
      btn.addEventListener('click', (e) => {
        const mode = (e.target as HTMLElement).getAttribute('data-mode');
        if (mode === 'roster' || mode === 'grid') {
          _viewMode = mode;
          renderAgents(agents, cbs);
        }
      });
    });
  }

  // Apply kinetic to grid cards (not roster rows — k-row breaks on <tr>)
  grid.querySelectorAll('.agent-card.k-row').forEach((card) => {
    kineticRow(card as HTMLElement, { spring: true, materialise: true });
  });

  // Bind events
  grid.querySelectorAll('.agent-chat-btn').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      const row = (e.target as HTMLElement).closest('[data-id]');
      const id = row?.getAttribute('data-id');
      if (id) cbs.onChat(id);
    });
  });

  grid.querySelectorAll('.agent-minichat-btn').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      const card = (e.target as HTMLElement).closest('.agent-card');
      const id = card?.getAttribute('data-id');
      if (id) cbs.onMiniChat(id);
    });
  });

  grid.querySelectorAll('.agent-edit-btn').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      const row = (e.target as HTMLElement).closest('[data-id]');
      const id = row?.getAttribute('data-id');
      if (id) cbs.onEdit(id);
    });
  });

  $('agent-card-new')?.addEventListener('click', () => cbs.onCreate());
  if (!_createBtnBound) {
    _createBtnBound = true;
    $('agents-create-btn')?.addEventListener('click', () => cbs.onCreate());
  }
}

/** Relative time helper */
function _timeAgo(d: Date): string {
  const diff = Date.now() - d.getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  const days = Math.floor(hrs / 24);
  if (days === 1) return 'yesterday';
  return `${days}d ago`;
}

// ── Dock badge / active helpers (exported so mini-chat.ts can call them) ────

/** Update the dock tray badge for a specific agent */
export function updateDockBadge(agentId: string, count: number) {
  const dockItem = document.querySelector(`.agent-dock-item[data-agent-id="${agentId}"]`);
  if (!dockItem) return;
  let badge = dockItem.querySelector('.agent-dock-badge') as HTMLElement | null;
  if (count > 0) {
    if (!badge) {
      badge = document.createElement('span');
      badge.className = 'agent-dock-badge';
      dockItem.appendChild(badge);
    }
    badge.textContent = count > 9 ? '9+' : String(count);
    badge.style.display = '';
  } else if (badge) {
    badge.style.display = 'none';
  }
}

/** Set active ring on dock item when mini-chat is open */
export function updateDockActive(agentId: string, active: boolean) {
  const dockItem = document.querySelector(`.agent-dock-item[data-agent-id="${agentId}"]`);
  if (dockItem) dockItem.classList.toggle('agent-dock-active', active);
}
