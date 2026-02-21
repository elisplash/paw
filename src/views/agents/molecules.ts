// molecules.ts — DOM rendering for the agents grid
// Depends on: atoms, helpers, toast, agent-policies

import { $, escHtml } from '../../components/helpers';
import { getAgentPolicy } from '../../features/agent-policies';
import { type Agent, spriteAvatar } from './atoms';

let _createBtnBound = false;

export interface RenderAgentsCallbacks {
  onChat: (agentId: string) => void;
  onMiniChat: (agentId: string) => void;
  onEdit: (agentId: string) => void;
  onCreate: () => void;
}

export function renderAgents(agents: Agent[], cbs: RenderAgentsCallbacks) {
  console.debug('[agents] renderAgents called');
  const grid = $('agents-grid');
  console.debug('[agents] grid element:', grid);
  if (!grid) return;

  grid.innerHTML = `${agents
    .map(
      (agent) => `
    <div class="agent-card${agent.source === 'backend' ? ' agent-card-backend' : ''}" data-id="${agent.id}">
      <div class="agent-card-header">
        <div class="agent-avatar" style="background:${agent.color}">${spriteAvatar(agent.avatar, 48)}</div>
        <div class="agent-info">
          <div class="agent-name">${escHtml(agent.name)}</div>
          <div class="agent-template">${agent.source === 'backend' ? 'AI-Created' : agent.template.charAt(0).toUpperCase() + agent.template.slice(1)}</div>
        </div>
        <button class="btn-icon agent-menu-btn" title="Options">⋮</button>
      </div>
      <div class="agent-bio">${escHtml(agent.bio)}</div>
      <div class="agent-stats">
        <span class="agent-stat">${(() => {
          const p = getAgentPolicy(agent.id);
          return p.mode === 'unrestricted' ? 'All tools' : `${p.allowed.length} tools`;
        })()}</span>
        <span class="agent-stat">${agent.boundaries.length} rules</span>
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

  // Bind events
  grid.querySelectorAll('.agent-chat-btn').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      const card = (e.target as HTMLElement).closest('.agent-card');
      const id = card?.getAttribute('data-id');
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
      const card = (e.target as HTMLElement).closest('.agent-card');
      const id = card?.getAttribute('data-id');
      if (id) cbs.onEdit(id);
    });
  });

  $('agent-card-new')?.addEventListener('click', () => cbs.onCreate());
  if (!_createBtnBound) {
    _createBtnBound = true;
    $('agents-create-btn')?.addEventListener('click', () => cbs.onCreate());
  }
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
