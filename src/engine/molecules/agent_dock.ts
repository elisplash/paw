// src/engine/molecules/agent_dock.ts
// Phase 3.3 — Floating agent dock tray molecule.
// Renders a row of agent avatar circles at the bottom-right of the screen.
// Each avatar can show an active ring and unread badge.

import type { AgentDockController, AgentDockEntry } from '../atoms/mini-hub';
import { spriteAvatar } from '../../views/agents/atoms';
import { escAttr } from '../../components/helpers';

// ── Constants ────────────────────────────────────────────────────────────

const MAX_VISIBLE_AVATARS = 12;

// ── Factory ──────────────────────────────────────────────────────────────

/**
 * Create a floating agent dock tray.
 * The dock shows avatar circles for each agent. Active hubs get a ring;
 * minimized hubs with unread messages get a badge.
 *
 * @param container  Element to append the dock into (usually document.body)
 * @param onAvatarClick  Called when user clicks an agent avatar
 * @param onNewGroup  Called when user clicks the "new group" button
 */
export function createAgentDock(
  container: HTMLElement,
  onAvatarClick: (agentId: string) => void,
  onNewGroup?: () => void,
): AgentDockController {
  let destroyed = false;
  let agents: AgentDockEntry[] = [];
  let collapsed = localStorage.getItem('paw-minihub-dock-collapsed') === 'true';
  let expanded = false; // whether overflow agents are shown

  // Track active hubs and unread counts
  const activeHubs = new Map<string, string>(); // hubId → agentId
  const unreadCounts = new Map<string, number>(); // agentId → count

  // ── Build DOM ──────────────────────────────────────────────────────────

  const dockEl = document.createElement('div');
  dockEl.className = 'agent-dock mini-hub-dock';
  if (collapsed) dockEl.classList.add('agent-dock-collapsed');
  container.appendChild(dockEl);

  // ── Render ─────────────────────────────────────────────────────────────

  function render() {
    if (destroyed) return;
    const visibleAgents = agents;
    if (visibleAgents.length === 0) {
      dockEl.style.display = 'none';
      return;
    }
    dockEl.style.display = '';

    // Determine which agents have active hubs
    const activeAgentIds = new Set<string>();
    for (const agentId of activeHubs.values()) {
      activeAgentIds.add(agentId);
    }

    const toggleIcon = collapsed ? 'left_panel_open' : 'right_panel_close';

    // Build avatar items
    const displayAgents = expanded ? visibleAgents : visibleAgents.slice(0, MAX_VISIBLE_AVATARS);
    const overflowCount = expanded ? 0 : Math.max(0, visibleAgents.length - MAX_VISIBLE_AVATARS);

    const itemsHtml = displayAgents
      .map((a) => {
        const isActive = activeAgentIds.has(a.id);
        const unread = unreadCounts.get(a.id) ?? 0;
        return `
        <div class="agent-dock-item${isActive ? ' agent-dock-active' : ''}" data-agent-id="${escAttr(a.id)}" title="${escAttr(a.name)}">
          <div class="agent-dock-avatar">${spriteAvatar(a.avatar, 32)}</div>
          <span class="agent-dock-tooltip">${escAttr(a.name)}</span>
          ${unread > 0 ? `<span class="agent-dock-badge">${unread > 9 ? '9+' : unread}</span>` : ''}
        </div>
      `;
      })
      .join('');

    dockEl.innerHTML = `
      <button class="agent-dock-toggle${activeHubs.size > 0 ? ' has-active-hubs' : ''}" title="${collapsed ? 'Show agents' : 'Hide agents'}">
        <span class="ms ms-sm">${toggleIcon}</span>
      </button>
      <div class="agent-dock-items">
        ${itemsHtml}
        ${overflowCount > 0 ? `<button class="agent-dock-expand" title="Show ${overflowCount} more">+${overflowCount}</button>` : ''}
        <button class="agent-dock-new-group" title="New Group Chat">
          <span class="ms" style="font-size:16px">group_add</span>
        </button>
      </div>
    `;

    // Bind events
    dockEl.querySelector('.agent-dock-toggle')?.addEventListener('click', () => {
      collapsed = !collapsed;
      dockEl.classList.toggle('agent-dock-collapsed', collapsed);
      localStorage.setItem('paw-minihub-dock-collapsed', String(collapsed));
      const iconEl = dockEl.querySelector('.agent-dock-toggle .ms') as HTMLElement | null;
      if (iconEl) iconEl.textContent = collapsed ? 'left_panel_open' : 'right_panel_close';
    });

    dockEl.querySelectorAll('.agent-dock-item').forEach((item) => {
      item.addEventListener('click', () => {
        const agentId = (item as HTMLElement).dataset.agentId;
        if (agentId) onAvatarClick(agentId);
      });
    });

    // Expand button — show all agents
    dockEl.querySelector('.agent-dock-expand')?.addEventListener('click', () => {
      expanded = true;
      render();
    });

    dockEl.querySelector('.agent-dock-new-group')?.addEventListener('click', () => {
      onNewGroup?.();
    });
  }

  // ── Controller ─────────────────────────────────────────────────────────

  const controller: AgentDockController = {
    refresh(newAgents: AgentDockEntry[]) {
      agents = newAgents;
      render();
    },

    addHub(hubId: string, agentId: string) {
      activeHubs.set(hubId, agentId);
      render();
    },

    removeHub(hubId: string, agentId: string) {
      activeHubs.delete(hubId);
      // Only remove active state if no other hub exists for this agent
      const stillActive = [...activeHubs.values()].includes(agentId);
      if (!stillActive) {
        // Update the specific item without full re-render
        const item = dockEl.querySelector(`[data-agent-id="${agentId}"]`);
        if (item) item.classList.remove('agent-dock-active');
      }
      render();
    },

    setUnread(agentId: string, count: number) {
      if (count > 0) {
        unreadCounts.set(agentId, count);
      } else {
        unreadCounts.delete(agentId);
      }
      // Update badge in-place (avoid full re-render flicker)
      const item = dockEl.querySelector(`[data-agent-id="${agentId}"]`);
      if (item) {
        let badge = item.querySelector('.agent-dock-badge') as HTMLElement | null;
        if (count > 0) {
          if (!badge) {
            badge = document.createElement('span');
            badge.className = 'agent-dock-badge';
            item.appendChild(badge);
          }
          badge.textContent = count > 9 ? '9+' : String(count);
        } else if (badge) {
          badge.remove();
        }
      }
    },

    setStreaming(agentId: string, active: boolean) {
      const item = dockEl.querySelector(`[data-agent-id="${agentId}"]`);
      if (item) {
        item.classList.toggle('agent-dock-streaming', active);
      }
    },

    destroy() {
      if (destroyed) return;
      destroyed = true;
      dockEl.remove();
    },
  };

  return controller;
}
