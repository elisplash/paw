// src/engine/molecules/conversation_list.ts
// Phase 11.1 — Conversation list molecule (left panel).
// Renders the scrollable list of agent conversations.
// Self-contained DOM — no global lookups.

import {
  sortConversations,
  filterConversations,
  filterByTab,
  truncatePreview,
  formatRelativeTime,
  type ConversationEntry,
  type InboxState,
} from '../atoms/inbox';
import * as AgentsModule from '../../views/agents';

// ── Types ────────────────────────────────────────────────────────────────

export interface ConversationListController {
  /** Root DOM element */
  el: HTMLElement;
  /** Re-render the conversation list from current state */
  render(conversations: ConversationEntry[], activeKey: string | null, filter: InboxState['filter']): void;
  /** Update search query and re-filter */
  setSearch(query: string): void;
  /** Set streaming state on a conversation row */
  setStreaming(sessionKey: string, active: boolean): void;
  /** Update unread badge for a specific conversation */
  setUnread(sessionKey: string, count: number): void;
  /** Destroy + cleanup */
  destroy(): void;
}

export interface ConversationListCallbacks {
  /** Conversation selected */
  onSelect: (sessionKey: string) => void;
  /** New chat requested */
  onNewChat: () => void;
  /** Filter tab changed */
  onFilter: (filter: InboxState['filter']) => void;
  /** Search query changed */
  onSearch: (query: string) => void;
  /** Context menu action */
  onAction?: (sessionKey: string, action: 'rename' | 'delete' | 'pin') => void;
}

// ── Factory ──────────────────────────────────────────────────────────────

export function createConversationList(
  callbacks: ConversationListCallbacks,
): ConversationListController {
  let destroyed = false;
  let _conversations: ConversationEntry[] = [];
  let _activeKey: string | null = null;
  let _filter: InboxState['filter'] = 'all';
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
  title.textContent = 'Inbox';
  const newBtn = document.createElement('button');
  newBtn.className = 'inbox-new-chat-btn';
  newBtn.title = 'New conversation';
  newBtn.innerHTML = `<span class="ms" style="font-size:16px">edit_square</span>`;
  newBtn.addEventListener('click', () => callbacks.onNewChat());
  titleRow.appendChild(title);
  titleRow.appendChild(newBtn);
  header.appendChild(titleRow);

  // Search bar
  const searchWrap = document.createElement('div');
  searchWrap.className = 'inbox-search';
  searchWrap.innerHTML = `<span class="ms">search</span>`;
  const searchInput = document.createElement('input');
  searchInput.type = 'text';
  searchInput.placeholder = 'Search conversations…';
  searchInput.addEventListener('input', () => {
    _searchQuery = searchInput.value;
    callbacks.onSearch(_searchQuery);
    renderRows();
  });
  searchWrap.appendChild(searchInput);
  header.appendChild(searchWrap);

  // Filter tabs
  const filters = document.createElement('div');
  filters.className = 'inbox-filters';
  const filterOptions: { key: InboxState['filter']; label: string }[] = [
    { key: 'all', label: 'All' },
    { key: 'unread', label: 'Unread' },
    { key: 'agents', label: 'Agents' },
    { key: 'groups', label: 'Groups' },
  ];
  for (const f of filterOptions) {
    const btn = document.createElement('button');
    btn.className = `inbox-filter-btn${f.key === 'all' ? ' active' : ''}`;
    btn.textContent = f.label;
    btn.dataset.filter = f.key;
    btn.addEventListener('click', () => {
      _filter = f.key;
      filters.querySelectorAll('.inbox-filter-btn').forEach((b) => b.classList.remove('active'));
      btn.classList.add('active');
      callbacks.onFilter(f.key);
      renderRows();
    });
    filters.appendChild(btn);
  }
  header.appendChild(filters);
  root.appendChild(header);

  // Scrollable conversation list
  const scrollArea = document.createElement('div');
  scrollArea.className = 'inbox-conv-scroll';
  root.appendChild(scrollArea);

  // ── Render rows ────────────────────────────────────────────────────────

  function renderRows(): void {
    scrollArea.innerHTML = '';

    let visible = filterByTab(_conversations, _filter);
    visible = filterConversations(visible, _searchQuery);
    visible = sortConversations(visible);

    if (visible.length === 0) {
      const empty = document.createElement('div');
      empty.className = 'inbox-conv-empty';
      empty.innerHTML = `<span class="ms">chat_bubble_outline</span><span>No conversations</span>`;
      scrollArea.appendChild(empty);
      return;
    }

    const frag = document.createDocumentFragment();
    const now = Date.now();

    for (const conv of visible) {
      const row = document.createElement('div');
      row.className = 'inbox-conv-row';
      if (conv.sessionKey === _activeKey) row.classList.add('active');
      if (conv.unread > 0) row.classList.add('unread');
      row.dataset.session = conv.sessionKey;

      // Avatar
      const avatar = document.createElement('div');
      avatar.className = 'inbox-conv-avatar';
      avatar.style.borderColor = conv.agentColor;
      const avatarContent = AgentsModule.spriteAvatar(conv.agentAvatar, 24);
      if (avatarContent.startsWith('<img') || avatarContent.startsWith('<svg')) {
        avatar.innerHTML = avatarContent;
      } else {
        avatar.textContent = conv.agentAvatar;
      }
      // Streaming dot
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
      name.textContent = conv.label || conv.agentName;
      const time = document.createElement('span');
      time.className = 'inbox-conv-time';
      time.textContent = conv.lastTs ? formatRelativeTime(conv.lastTs, now) : '';
      topRow.appendChild(name);
      topRow.appendChild(time);

      const bottomRow = document.createElement('div');
      bottomRow.className = 'inbox-conv-bottom';
      const preview = document.createElement('span');
      preview.className = 'inbox-conv-preview';
      const rolePrefix = conv.lastRole === 'user' ? 'You: ' : '';
      preview.textContent = conv.lastMessage ? rolePrefix + truncatePreview(conv.lastMessage) : 'No messages yet';
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

      // Click handler
      row.addEventListener('click', () => callbacks.onSelect(conv.sessionKey));

      frag.appendChild(row);
    }

    scrollArea.appendChild(frag);
  }

  // ── Controller ─────────────────────────────────────────────────────────

  const controller: ConversationListController = {
    el: root,

    render(conversations, activeKey, filter) {
      _conversations = conversations;
      _activeKey = activeKey;
      _filter = filter;
      // Sync filter tab UI
      filters.querySelectorAll('.inbox-filter-btn').forEach((b) => {
        const btn = b as HTMLElement;
        btn.classList.toggle('active', btn.dataset.filter === filter);
      });
      renderRows();
    },

    setSearch(query) {
      _searchQuery = query;
      searchInput.value = query;
      renderRows();
    },

    setStreaming(sessionKey, active) {
      const row = scrollArea.querySelector(`[data-session="${sessionKey}"]`);
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

    setUnread(sessionKey, count) {
      const row = scrollArea.querySelector(`[data-session="${sessionKey}"]`) as HTMLElement | null;
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
