// src/engine/molecules/inbox_thread.ts
// Phase 11.2 — Inbox thread panel molecule (center panel).
// Wraps the existing chat message area + input bar inside the inbox layout.
// Delegates rendering to chat_renderer + existing DOM elements.

import * as AgentsModule from '../../views/agents';

// ── Types ────────────────────────────────────────────────────────────────

export interface InboxThreadController {
  /** Root DOM element */
  el: HTMLElement;
  /** Update the thread header with agent info */
  setAgent(name: string, avatar: string, color: string, model: string): void;
  /** Show/hide streaming indicator in header */
  setStreaming(active: boolean): void;
  /** Show the empty state (no conversation selected) */
  showEmpty(): void;
  /** Show the chat body (conversation selected) */
  showThread(): void;
  /** Mount existing chat DOM elements into the thread body */
  mountChatElements(elements: {
    compactionWarning: HTMLElement | null;
    budgetAlert: HTMLElement | null;
    messagesContainer: HTMLElement | null;
    inputContainer: HTMLElement | null;
  }): void;
  /** Destroy + cleanup */
  destroy(): void;
}

export interface InboxThreadCallbacks {
  /** Toggle sidebar visibility */
  onToggleSidebar: () => void;
  /** Model select element for thread header */
  modelSelectEl?: HTMLSelectElement | null;
  /** New chat button */
  onNewChat?: () => void;
}

// ── Factory ──────────────────────────────────────────────────────────────

export function createInboxThread(
  callbacks: InboxThreadCallbacks,
): InboxThreadController {
  let destroyed = false;

  // ── Build DOM ──────────────────────────────────────────────────────────

  const root = document.createElement('div');
  root.className = 'inbox-thread';

  // Header
  const header = document.createElement('div');
  header.className = 'inbox-thread-header';

  const identity = document.createElement('div');
  identity.className = 'inbox-thread-identity';

  const avatarEl = document.createElement('div');
  avatarEl.className = 'inbox-thread-avatar';

  const infoEl = document.createElement('div');
  infoEl.className = 'inbox-thread-info';
  const nameEl = document.createElement('span');
  nameEl.className = 'inbox-thread-name';
  nameEl.textContent = 'Select a conversation';
  const statusEl = document.createElement('span');
  statusEl.className = 'inbox-thread-status';
  statusEl.textContent = '';
  infoEl.appendChild(nameEl);
  infoEl.appendChild(statusEl);

  identity.appendChild(avatarEl);
  identity.appendChild(infoEl);
  header.appendChild(identity);

  // Header actions
  const actions = document.createElement('div');
  actions.className = 'inbox-thread-actions';

  // Model select (moved to header)
  if (callbacks.modelSelectEl) {
    callbacks.modelSelectEl.style.maxWidth = '140px';
    actions.appendChild(callbacks.modelSelectEl);
  }

  // New chat button
  const newChatBtn = document.createElement('button');
  newChatBtn.title = 'New Chat';
  newChatBtn.innerHTML = `<span class="ms" style="font-size:16px">add</span>`;
  newChatBtn.addEventListener('click', () => callbacks.onNewChat?.());
  actions.appendChild(newChatBtn);

  // Toggle sidebar
  const sidebarToggle = document.createElement('button');
  sidebarToggle.title = 'Toggle sidebar';
  sidebarToggle.innerHTML = `<span class="ms" style="font-size:16px">right_panel_open</span>`;
  sidebarToggle.addEventListener('click', () => callbacks.onToggleSidebar());
  actions.appendChild(sidebarToggle);

  header.appendChild(actions);
  root.appendChild(header);

  // Thread body (where chat content goes)
  const body = document.createElement('div');
  body.className = 'inbox-thread-body';

  // Empty state
  const emptyState = document.createElement('div');
  emptyState.className = 'inbox-thread-empty';
  emptyState.innerHTML = `
    <span class="ms">forum</span>
    <div class="inbox-thread-empty-title">Select a conversation</div>
    <div class="inbox-thread-empty-sub">Choose from the sidebar or start a new chat</div>
  `;

  body.appendChild(emptyState);
  root.appendChild(body);

  // ── Controller ─────────────────────────────────────────────────────────

  const controller: InboxThreadController = {
    el: root,

    setAgent(name, avatar, color, model) {
      const avatarHtml = AgentsModule.spriteAvatar(avatar, 20);
      avatarEl.innerHTML = avatarHtml;
      avatarEl.style.borderColor = color;
      nameEl.textContent = name;
      statusEl.textContent = model ? `Active on ${model}` : '';
    },

    setStreaming(active) {
      if (active) {
        statusEl.textContent = 'Typing…';
        statusEl.style.color = 'var(--kinetic-sage, var(--success))';
      } else {
        statusEl.style.color = '';
        // Will be reset by next setAgent call
      }
    },

    showEmpty() {
      emptyState.style.display = 'flex';
      // Hide chat elements
      const chatEls = body.querySelectorAll('.chat-main-col > *');
      chatEls.forEach((el) => ((el as HTMLElement).style.display = 'none'));
    },

    showThread() {
      emptyState.style.display = 'none';
    },

    mountChatElements(elements) {
      // Wrap existing chat elements in a chat-main-col container
      let chatCol = body.querySelector('.chat-main-col') as HTMLElement | null;
      if (!chatCol) {
        chatCol = document.createElement('div');
        chatCol.className = 'chat-main-col';
        body.appendChild(chatCol);
      }
      // Move elements into the thread body
      if (elements.compactionWarning && !chatCol.contains(elements.compactionWarning)) {
        chatCol.appendChild(elements.compactionWarning);
      }
      if (elements.budgetAlert && !chatCol.contains(elements.budgetAlert)) {
        chatCol.appendChild(elements.budgetAlert);
      }
      if (elements.messagesContainer && !chatCol.contains(elements.messagesContainer)) {
        chatCol.appendChild(elements.messagesContainer);
      }
      if (elements.inputContainer && !chatCol.contains(elements.inputContainer)) {
        chatCol.appendChild(elements.inputContainer);
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
