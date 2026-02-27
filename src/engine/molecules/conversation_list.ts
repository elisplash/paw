// src/engine/molecules/conversation_list.ts
// Phase 11.1 — Agent list molecule (left panel).
// Renders the scrollable list of agents (grouped from conversations).
// Clicking an agent opens chat with that agent; session switching is via
// a dropdown in the thread header.
// Self-contained DOM — no global lookups.

import {
  filterConversations,
  truncatePreview,
  formatRelativeTime,
  groupByAgent,
  type ConversationEntry,
  type AgentGroup,
  type InboxState,
} from '../atoms/inbox';
import * as AgentsModule from '../../views/agents';

// ── Types ────────────────────────────────────────────────────────────────

export interface ConversationListController {
  /** Root DOM element */
  el: HTMLElement;
  /** Re-render the agent list from current conversation state */
  render(
    conversations: ConversationEntry[],
    activeAgentId: string | null,
    filter: InboxState['filter'],
  ): void;
  /** Update search query and re-filter */
  setSearch(query: string): void;
  /** Set streaming state on an agent row (by agentId) */
  setStreaming(agentId: string, active: boolean): void;
  /** Update unread badge for a specific agent */
  setUnread(agentId: string, count: number): void;
  /** Destroy + cleanup */
  destroy(): void;
}

export interface ConversationListCallbacks {
  /** Agent selected — opens chat with this agent */
  onSelect: (agentId: string) => void;
  /** New direct chat requested */
  onNewChat: () => void;
  /** New group chat requested */
  onNewGroup: () => void;
  /** Filter tab changed */
  onFilter: (filter: InboxState['filter']) => void;
  /** Search query changed */
  onSearch: (query: string) => void;
  /** Context menu action */
  onAction?: (agentId: string, action: 'rename' | 'delete' | 'pin') => void;
}

// ── Factory ──────────────────────────────────────────────────────────────

export function createConversationList(
  callbacks: ConversationListCallbacks,
): ConversationListController {
  let destroyed = false;
  let _conversations: ConversationEntry[] = [];
  let _activeAgentId: string | null = null;
  let _searchQuery = '';

  // ── Build DOM ──────────────────────────────────────────────────────────

  const root = document.createElement('div');
  root.className = 'inbox-conv-list';

  // Header
  const header = document.createElement('div');
  header.className = 'inbox-conv-header';

  // Title row
  const titleRow = document.createElement('div');
  titleRow.className = 'inbox-conv-title-row';
  const title = document.createElement('span');
  title.className = 'inbox-conv-title';
  title.textContent = 'Agents';

  // New chat button with dropdown
  const newBtnWrap = document.createElement('div');
  newBtnWrap.className = 'inbox-new-chat-wrap';
  newBtnWrap.style.position = 'relative';

  const newBtn = document.createElement('button');
  newBtn.className = 'inbox-new-chat-btn';
  newBtn.title = 'New conversation';
  newBtn.innerHTML = `<span class="ms" style="font-size:16px">edit_square</span>`;

  const newDropdown = document.createElement('div');
  newDropdown.className = 'inbox-new-dropdown';
  newDropdown.style.display = 'none';

  const newDirectBtn = document.createElement('button');
  newDirectBtn.className = 'inbox-new-dropdown-item';
  newDirectBtn.innerHTML = `<span class="ms" style="font-size:14px">chat</span> New Chat`;
  newDirectBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    newDropdown.style.display = 'none';
    callbacks.onNewChat();
  });

  const newGroupBtn = document.createElement('button');
  newGroupBtn.className = 'inbox-new-dropdown-item';
  newGroupBtn.innerHTML = `<span class="ms" style="font-size:14px">group</span> New Group Chat`;
  newGroupBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    newDropdown.style.display = 'none';
    callbacks.onNewGroup();
  });

  newDropdown.appendChild(newDirectBtn);
  newDropdown.appendChild(newGroupBtn);

  newBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    const isOpen = newDropdown.style.display !== 'none';
    newDropdown.style.display = isOpen ? 'none' : 'flex';
  });
  // Close dropdown on outside click
  document.addEventListener('click', () => {
    newDropdown.style.display = 'none';
  });

  newBtnWrap.appendChild(newBtn);
  newBtnWrap.appendChild(newDropdown);

  titleRow.appendChild(title);
  titleRow.appendChild(newBtnWrap);
  header.appendChild(titleRow);

  // Search bar
  const searchWrap = document.createElement('div');
  searchWrap.className = 'inbox-search';
  searchWrap.innerHTML = `<span class="ms">search</span>`;
  const searchInput = document.createElement('input');
  searchInput.type = 'text';
  searchInput.placeholder = 'Search agents…';
  searchInput.addEventListener('input', () => {
    _searchQuery = searchInput.value;
    callbacks.onSearch(_searchQuery);
    renderRows();
  });
  searchWrap.appendChild(searchInput);
  header.appendChild(searchWrap);

  root.appendChild(header);

  // Scrollable agent list
  const scrollArea = document.createElement('div');
  scrollArea.className = 'inbox-conv-scroll';
  root.appendChild(scrollArea);

  // ── Render agent rows ──────────────────────────────────────────────────

  function renderRows(): void {
    scrollArea.innerHTML = '';

    // Filter conversations first, then group by agent
    let visible = _conversations;
    if (_searchQuery) {
      visible = filterConversations(visible, _searchQuery);
    }

    // Separate group chats from direct conversations
    const directConvs = visible.filter((c) => c.kind !== 'group');
    const groupConvs = visible.filter((c) => c.kind === 'group');

    const agentGroups = groupByAgent(directConvs);

    if (agentGroups.length === 0 && groupConvs.length === 0) {
      const empty = document.createElement('div');
      empty.className = 'inbox-conv-empty';
      empty.innerHTML = `<span class="ms">smart_toy</span><span>No agents</span>`;
      scrollArea.appendChild(empty);
      return;
    }

    const frag = document.createDocumentFragment();
    const now = Date.now();

    // Render agent rows
    for (const group of agentGroups) {
      frag.appendChild(buildAgentRow(group, now));
    }

    // Render group chat rows (separate section)
    if (groupConvs.length > 0) {
      const groupHeader = document.createElement('div');
      groupHeader.className = 'inbox-section-header';
      groupHeader.innerHTML = `<span class="ms" style="font-size:14px">group</span> Groups`;
      frag.appendChild(groupHeader);

      for (const conv of groupConvs) {
        frag.appendChild(buildGroupRow(conv, now));
      }
    }

    scrollArea.appendChild(frag);
  }

  function buildAgentRow(group: AgentGroup, now: number): HTMLElement {
    const row = document.createElement('div');
    row.className = 'inbox-conv-row';
    if (group.agentId === _activeAgentId) row.classList.add('active');
    if (group.totalUnread > 0) row.classList.add('unread');
    row.dataset.agent = group.agentId;

    // Avatar
    const avatar = document.createElement('div');
    avatar.className = 'inbox-conv-avatar';
    avatar.style.borderColor = group.agentColor;
    const avatarContent = AgentsModule.spriteAvatar(group.agentAvatar, 24);
    if (avatarContent.startsWith('<img') || avatarContent.startsWith('<svg')) {
      avatar.innerHTML = avatarContent;
    } else {
      avatar.textContent = group.agentAvatar;
    }
    // Streaming dot — show if any conversation for this agent is streaming
    const isStreaming = group.conversations.some((c) => c.isStreaming);
    if (isStreaming) {
      const dot = document.createElement('span');
      dot.className = 'streaming-dot';
      avatar.appendChild(dot);
    }
    row.appendChild(avatar);

    // Body
    const body = document.createElement('div');
    body.className = 'inbox-conv-body';

    const topRow = document.createElement('div');
    topRow.className = 'inbox-conv-top';
    const name = document.createElement('span');
    name.className = 'inbox-conv-name';
    name.textContent = group.agentName;
    const time = document.createElement('span');
    time.className = 'inbox-conv-time';
    time.textContent = group.latestTs ? formatRelativeTime(group.latestTs, now) : '';
    topRow.appendChild(name);
    topRow.appendChild(time);

    const bottomRow = document.createElement('div');
    bottomRow.className = 'inbox-conv-bottom';
    const preview = document.createElement('span');
    preview.className = 'inbox-conv-preview';
    // Show last message preview from the most recent conversation
    const latest = group.conversations[0];
    if (latest?.lastMessage) {
      const rolePrefix = latest.lastRole === 'user' ? 'You: ' : '';
      preview.textContent = rolePrefix + truncatePreview(latest.lastMessage);
    } else {
      const sessionCount = group.conversations.length;
      preview.textContent = sessionCount === 1 ? '1 session' : `${sessionCount} sessions`;
    }
    bottomRow.appendChild(preview);
    if (group.totalUnread > 0) {
      const badge = document.createElement('span');
      badge.className = 'inbox-conv-badge';
      badge.textContent = String(group.totalUnread);
      bottomRow.appendChild(badge);
    }

    body.appendChild(topRow);
    body.appendChild(bottomRow);
    row.appendChild(body);

    // Click handler — select agent
    row.addEventListener('click', () => callbacks.onSelect(group.agentId));

    return row;
  }

  function buildGroupRow(conv: ConversationEntry, now: number): HTMLElement {
    const row = document.createElement('div');
    row.className = 'inbox-conv-row group';
    row.dataset.agent = conv.agentId;
    if (conv.unread > 0) row.classList.add('unread');

    // Avatar with group overlay
    const avatar = document.createElement('div');
    avatar.className = 'inbox-conv-avatar';
    avatar.style.borderColor = conv.agentColor;
    const avatarContent = AgentsModule.spriteAvatar(conv.agentAvatar, 24);
    if (avatarContent.startsWith('<img') || avatarContent.startsWith('<svg')) {
      avatar.innerHTML = avatarContent;
    } else {
      avatar.textContent = conv.agentAvatar;
    }
    // Group icon overlay
    const groupIcon = document.createElement('span');
    groupIcon.className = 'ms inbox-conv-group-icon';
    groupIcon.textContent = 'group';
    avatar.appendChild(groupIcon);
    if (conv.isStreaming) {
      const dot = document.createElement('span');
      dot.className = 'streaming-dot';
      avatar.appendChild(dot);
    }
    row.appendChild(avatar);

    // Body
    const body = document.createElement('div');
    body.className = 'inbox-conv-body';

    const topRow = document.createElement('div');
    topRow.className = 'inbox-conv-top';
    const name = document.createElement('span');
    name.className = 'inbox-conv-name';
    name.textContent = conv.label || 'Group Chat';
    const time = document.createElement('span');
    time.className = 'inbox-conv-time';
    time.textContent = conv.lastTs ? formatRelativeTime(conv.lastTs, now) : '';
    topRow.appendChild(name);
    topRow.appendChild(time);

    const bottomRow = document.createElement('div');
    bottomRow.className = 'inbox-conv-bottom';
    const preview = document.createElement('span');
    preview.className = 'inbox-conv-preview';
    if (conv.lastMessage) {
      const rolePrefix = conv.lastRole === 'user' ? 'You: ' : '';
      preview.textContent = rolePrefix + truncatePreview(conv.lastMessage);
    } else {
      const memberCount = conv.members?.length ?? 0;
      preview.textContent = memberCount > 0 ? `${memberCount} agents` : 'No messages yet';
    }
    bottomRow.appendChild(preview);
    if (conv.unread > 0) {
      const badge = document.createElement('span');
      badge.className = 'inbox-conv-badge';
      badge.textContent = String(conv.unread);
      bottomRow.appendChild(badge);
    }

    body.appendChild(topRow);
    body.appendChild(bottomRow);
    row.appendChild(body);

    // Click — select the primary agent for this group
    row.addEventListener('click', () => callbacks.onSelect(conv.agentId));

    return row;
  }

  // ── Controller ─────────────────────────────────────────────────────────

  const controller: ConversationListController = {
    el: root,

    render(conversations, activeAgentId, _filter) {
      _conversations = conversations;
      _activeAgentId = activeAgentId;
      renderRows();
    },

    setSearch(query) {
      _searchQuery = query;
      searchInput.value = query;
      renderRows();
    },

    setStreaming(agentId, active) {
      const row = scrollArea.querySelector(`[data-agent="${agentId}"]`);
      if (!row) return;
      const avatar = row.querySelector('.inbox-conv-avatar');
      if (!avatar) return;
      const existing = avatar.querySelector('.streaming-dot');
      if (active && !existing) {
        const dot = document.createElement('span');
        dot.className = 'streaming-dot';
        avatar.appendChild(dot);
      } else if (!active && existing) {
        existing.remove();
      }
    },

    setUnread(agentId, count) {
      const row = scrollArea.querySelector(`[data-agent="${agentId}"]`) as HTMLElement | null;
      if (!row) return;
      row.classList.toggle('unread', count > 0);
      const badge = row.querySelector('.inbox-conv-badge');
      if (count > 0) {
        if (badge) {
          badge.textContent = String(count);
        } else {
          const bottom = row.querySelector('.inbox-conv-bottom');
          if (bottom) {
            const b = document.createElement('span');
            b.className = 'inbox-conv-badge';
            b.textContent = String(count);
            bottom.appendChild(b);
          }
        }
      } else if (badge) {
        badge.remove();
      }
    },

    destroy() {
      if (destroyed) return;
      destroyed = true;
      root.remove();
    },
  };

  return controller;
}
