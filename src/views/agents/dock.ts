// dock.ts — Floating agent dock tray (FB Messenger–style)
// Persistent bar of agent avatar circles at the bottom-right of the screen.

import { type Agent, spriteAvatar } from './atoms';
import { escAttr } from '../../components/helpers';

export interface DockDeps {
  getAgents: () => Agent[];
  getMiniChatState: (agentId: string) => { unreadCount: number } | undefined;
  isMiniChatOpen: (agentId: string) => boolean;
  openMiniChat: (agentId: string) => void;
}

let _dockEl: HTMLElement | null = null;
let _dockCollapsed = localStorage.getItem('paw-dock-collapsed') === 'true';

function setDockCollapsed(collapsed: boolean) {
  _dockCollapsed = collapsed;
  localStorage.setItem('paw-dock-collapsed', String(collapsed));
  if (_dockEl) _dockEl.classList.toggle('agent-dock-collapsed', collapsed);
  const icon = _dockEl?.querySelector('.agent-dock-toggle .ms') as HTMLElement | null;
  if (icon) icon.textContent = collapsed ? 'left_panel_open' : 'right_panel_close';
}

/**
 * Render or refresh the floating agent dock tray.
 * Called after agents load and whenever agents list changes.
 */
export function renderAgentDock(deps: DockDeps) {
  // Create dock container if needed
  if (!_dockEl) {
    _dockEl = document.createElement('div');
    _dockEl.id = 'agent-dock';
    _dockEl.className = 'agent-dock';
    if (_dockCollapsed) _dockEl.classList.add('agent-dock-collapsed');
    document.body.appendChild(_dockEl);
  }

  const agents = deps.getAgents();
  if (agents.length === 0) {
    _dockEl.style.display = 'none';
    return;
  }
  _dockEl.style.display = '';

  const toggleIcon = _dockCollapsed ? 'left_panel_open' : 'right_panel_close';
  const agentItems = agents
    .map((a) => {
      const isOpen = deps.isMiniChatOpen(a.id);
      const mc = deps.getMiniChatState(a.id);
      const unread = mc?.unreadCount ?? 0;
      return `
      <div class="agent-dock-item${isOpen ? ' agent-dock-active' : ''}" data-agent-id="${a.id}">
        <div class="agent-dock-avatar">${spriteAvatar(a.avatar, 40)}</div>
        <span class="agent-dock-tooltip">${escAttr(a.name)}</span>
        ${unread > 0 ? `<span class="agent-dock-badge">${unread > 9 ? '9+' : unread}</span>` : ''}
      </div>
    `;
    })
    .join('');

  _dockEl.innerHTML = `
    <button class="agent-dock-toggle" title="${_dockCollapsed ? 'Show agents' : 'Hide agents'}">
      <span class="ms ms-sm">${toggleIcon}</span>
    </button>
    <div class="agent-dock-items">
      ${agentItems}
    </div>
  `;

  // Toggle button
  _dockEl.querySelector('.agent-dock-toggle')?.addEventListener('click', () => {
    setDockCollapsed(!_dockCollapsed);
  });

  // Bind click events on agent items
  _dockEl.querySelectorAll('.agent-dock-item').forEach((item) => {
    item.addEventListener('click', () => {
      const agentId = (item as HTMLElement).dataset.agentId;
      if (agentId) deps.openMiniChat(agentId);
    });
  });
}
