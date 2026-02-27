// src/engine/organisms/inbox_controller.ts
// Phase 11.7 — Inbox controller organism.
// Wires conversation_list + inbox_thread + inbox_sidebar molecules together
// and delegates message rendering / sending to the existing chat_controller.
// Reads session/agent state from appState and the pawEngine IPC.

import { pawEngine } from '../../engine';
import {
  appState,
  agentSessionMap,
  persistAgentSessionMap,
  groupSessionMap,
  persistGroupSessionMap,
} from '../../state/index';
import { showToast } from '../../components/toast';
import { confirmModal, promptModal } from '../../components/helpers';
import * as AgentsModule from '../../views/agents';
import {
  type ConversationEntry,
  filterConversations,
  truncatePreview,
  persistConvlistPref,
} from '../atoms/inbox';
import {
  createConversationList,
  type ConversationListController,
} from '../molecules/conversation_list';
import { createInboxThread, type InboxThreadController } from '../molecules/inbox_thread';
import { createInboxSidebar, type InboxSidebarController } from '../molecules/inbox_sidebar';
import {
  loadSessions,
  loadChatHistory,
  switchToAgent,
  renderMessages,
  resetTokenMeter,
  renderSessionSelect,
} from './chat_controller';

// ── DOM shorthand ────────────────────────────────────────────────────────

const $ = (id: string) => document.getElementById(id);

// ── Module state ─────────────────────────────────────────────────────────

let _list: ConversationListController | null = null;
let _thread: InboxThreadController | null = null;
let _sidebar: InboxSidebarController | null = null;
let _mounted = false;
let _refreshTimer: ReturnType<typeof setInterval> | null = null;

// ── Public API ───────────────────────────────────────────────────────────

/**
 * Mount the inbox layout into the chat-view.
 * Call once after initChatListeners has been called.
 */
export function mountInbox(): void {
  if (_mounted) return;

  const chatView = $('chat-view');
  if (!chatView) {
    console.warn('[inbox] chat-view element not found');
    return;
  }

  // ── Build molecules ────────────────────────────────────────────────────

  _list = createConversationList({
    onSelect: handleSelectAgent,
    onNewChat: handleNewChat,
    onNewGroup: handleNewGroup,
    onFilter: handleFilter,
    onSearch: handleSearch,
    onAction: handleConversationAction,
  });

  _thread = createInboxThread({
    onToggleSidebar: handleToggleSidebar,
    onToggleConvlist: handleToggleConvlist,
    modelSelectEl: $('chat-model-select') as HTMLSelectElement | null,
    sessionSelectEl: $('chat-session-select') as HTMLSelectElement | null,
    onNewChat: handleNewChat,
    onSwapAgent: handleSwapAgent,
  });

  _sidebar = createInboxSidebar({
    onRename: handleRename,
    onDelete: handleDelete,
    onClear: handleClear,
    onCompact: handleCompact,
    onSearch: handleSearchInConversation,
  });

  // ── Construct layout ───────────────────────────────────────────────────

  const layout = document.createElement('div');
  layout.className = 'inbox-layout';
  layout.id = 'inbox-layout';

  layout.appendChild(_list.el);
  layout.appendChild(_thread.el);
  layout.appendChild(_sidebar.el);

  // Move existing chat DOM elements into the thread body
  const chatMessages = $('chat-messages');
  // chat-input-container has class only (no id) — use querySelector
  const chatInputContainer = chatView.querySelector('.chat-input-container') as HTMLElement | null;
  const compactionWarning = $('compaction-warning');
  const budgetAlert = $('session-budget-alert');

  _thread.mountChatElements({
    compactionWarning,
    budgetAlert,
    messagesContainer: chatMessages,
    inputContainer: chatInputContainer,
  });

  // Also grab abort button
  const abortBtn = $('chat-abort-btn');
  if (abortBtn) {
    const threadBody = _thread.el.querySelector('.inbox-thread-body .chat-main-col');
    if (threadBody) threadBody.appendChild(abortBtn);
  }

  // Hide old header + mission body (sidebar re-parents the panel)
  const chatHeader = chatView.querySelector('.chat-header') as HTMLElement | null;
  if (chatHeader) chatHeader.style.display = 'none';
  const chatMissionBody = chatView.querySelector('.chat-mission-body') as HTMLElement | null;

  // Insert layout into chat-view
  if (chatMissionBody) {
    chatMissionBody.style.display = 'none';
    chatView.appendChild(layout);
  } else {
    chatView.appendChild(layout);
  }

  // Sidebar state from preferences
  if (!appState.inbox.sidebarOpen) {
    layout.classList.add('sidebar-collapsed');
    _sidebar.toggle(false);
  }

  // Convlist (left panel) state from preferences
  if (!appState.inbox.convlistOpen) {
    layout.classList.add('convlist-collapsed');
  }

  _mounted = true;

  // Initial population — await so agents are rendered before user sees empty state
  refreshConversationList().then(() => {
    // Auto-select the current agent if one is active
    const currentAgent = AgentsModule.getCurrentAgent();
    if (currentAgent && _thread) {
      appState.inbox.activeSessionKey = appState.currentSessionKey;
      _thread.showThread();
      updateThreadHeader();
      _list?.render(appState.inbox.conversations, currentAgent.id, appState.inbox.filter);
      updateSwapAgents();
      renderSessionSelect();
    }
  });

  // Auto-refresh every 30 seconds
  _refreshTimer = setInterval(() => refreshConversationList(), 30_000);

  console.debug('[inbox] Mounted inbox layout');
}

/**
 * Refresh the conversation list from session state + previews.
 */
export async function refreshConversationList(): Promise<void> {
  if (!_list || !_mounted) return;

  try {
    // Ensure sessions are loaded (engine mode uses IPC, not WS)
    if (!appState.sessions.length) {
      await loadSessions({ skipHistory: true });
    }

    const agents = AgentsModule.getAgents();
    const agentMap = new Map(agents.map((a) => [a.id, a]));

    // Build ConversationEntry for each session.
    // Fetch last message for visible sessions (batch, max 20 parallel).
    const entries: ConversationEntry[] = [];
    const previewBatch = appState.sessions.slice(0, 20).map(async (session) => {
      const agentId = session.agentId ?? 'default';
      const agent = agentMap.get(agentId);
      let lastMessage = '';
      let lastRole: 'user' | 'assistant' = 'user';
      let lastTs = session.updatedAt ?? Date.now();
      try {
        const msgs = await pawEngine.chatHistory(session.key, 1);
        if (msgs.length) {
          lastMessage = truncatePreview(msgs[0].content ?? '');
          lastRole = msgs[0].role === 'assistant' ? 'assistant' : 'user';
          lastTs = new Date(msgs[0].created_at).getTime() || lastTs;
        }
      } catch {
        // Swallow preview errors
      }
      return {
        sessionKey: session.key,
        agentId,
        agentName: agent?.name ?? 'Paw',
        agentAvatar: agent?.avatar ?? '5',
        agentColor: agent?.color ?? 'var(--accent)',
        lastMessage,
        lastRole,
        lastTs,
        unread: 0,
        label: session.label ?? session.displayName ?? '',
        isStreaming: appState.activeStreams.has(session.key),
        kind: session.kind ?? 'direct',
        pinned: false,
        members: session.members,
      } satisfies ConversationEntry;
    });

    entries.push(...(await Promise.all(previewBatch)));

    // Remaining sessions (>20) get entries without previews
    for (let i = 20; i < appState.sessions.length; i++) {
      const session = appState.sessions[i];
      const agentId = session.agentId ?? 'default';
      const agent = agentMap.get(agentId);
      entries.push({
        sessionKey: session.key,
        agentId,
        agentName: agent?.name ?? 'Paw',
        agentAvatar: agent?.avatar ?? '5',
        agentColor: agent?.color ?? 'var(--accent)',
        lastMessage: '',
        lastRole: 'user',
        lastTs: session.updatedAt ?? Date.now(),
        unread: 0,
        label: session.label ?? session.displayName ?? '',
        isStreaming: appState.activeStreams.has(session.key),
        kind: session.kind ?? 'direct',
        pinned: false,
        members: session.members,
      });
    }

    appState.inbox.conversations = entries;

    // Pass all conversations — the list molecule groups by agent internally
    let filtered = entries;
    if (appState.inbox.searchQuery) {
      filtered = filterConversations(filtered, appState.inbox.searchQuery);
    }

    // Determine active agent ID
    const currentAgent = AgentsModule.getCurrentAgent();
    const activeAgentId = currentAgent?.id ?? null;

    _list.render(filtered, activeAgentId, appState.inbox.filter);

    // Update thread header if we have an active conversation
    updateThreadHeader();
  } catch (e) {
    console.warn('[inbox] Refresh failed:', e);
  }
}

/**
 * Destroy the inbox layout and restore original chat view.
 */
export function unmountInbox(): void {
  if (!_mounted) return;
  if (_refreshTimer) {
    clearInterval(_refreshTimer);
    _refreshTimer = null;
  }
  _list?.destroy();
  _thread?.destroy();
  _sidebar?.destroy();
  _list = null;
  _thread = null;
  _sidebar = null;

  const layout = $('inbox-layout');
  if (layout) layout.remove();

  // Restore hidden elements (sidebar.destroy() puts mission panel back)
  const chatView = $('chat-view');
  if (chatView) {
    const chatHeader = chatView.querySelector('.chat-header') as HTMLElement | null;
    if (chatHeader) chatHeader.style.display = '';
    const chatMissionBody = chatView.querySelector('.chat-mission-body') as HTMLElement | null;
    if (chatMissionBody) chatMissionBody.style.display = '';
  }

  _mounted = false;
  console.debug('[inbox] Unmounted inbox layout');
}

/** Whether the inbox is currently mounted */
export function isInboxMounted(): boolean {
  return _mounted;
}

// ── Handlers ─────────────────────────────────────────────────────────────

async function handleSelectAgent(agentId: string): Promise<void> {
  if (!_thread || !_sidebar) return;

  // Switch to this agent (loads its last session or creates blank)
  const currentAgent = AgentsModule.getCurrentAgent();
  if (currentAgent?.id !== agentId) {
    await switchToAgent(agentId);
  }

  // Render the session select dropdown so user can switch between sessions
  renderSessionSelect();

  // Update active session tracking
  appState.inbox.activeSessionKey = appState.currentSessionKey;

  // Show thread
  _thread.showThread();
  updateThreadHeader();

  // Determine active agent ID and refresh list
  const activeAgent = AgentsModule.getCurrentAgent();
  _list?.render(appState.inbox.conversations, activeAgent?.id ?? null, appState.inbox.filter);

  // Clear unread for all conversations belonging to this agent
  for (const conv of appState.inbox.conversations) {
    if (conv.agentId === agentId && conv.unread > 0) {
      conv.unread = 0;
    }
  }
  _list?.setUnread(agentId, 0);

  // Populate swap agent dropdown
  updateSwapAgents();
}

async function handleNewChat(): Promise<void> {
  // Create lazy — next sendMessage will auto-create the session
  appState.currentSessionKey = null;
  appState.messages = [];

  const chatMessages = $('chat-messages');
  if (chatMessages) chatMessages.innerHTML = '';
  const chatEmpty = $('chat-empty');
  if (chatEmpty) chatEmpty.style.display = '';

  resetTokenMeter();

  if (_thread) {
    _thread.showThread();
    const agent = AgentsModule.getCurrentAgent();
    if (agent) {
      _thread.setAgent(agent.name, agent.avatar, agent.color, appState.activeModelKey || '');
    }
  }

  showToast('New conversation started', 'success');
}

async function handleNewGroup(): Promise<void> {
  // Build agent multi-select overlay
  const agents = AgentsModule.getAgents();
  if (agents.length < 2) {
    showToast('You need at least 2 agents to create a group chat', 'error');
    return;
  }

  // Create modal overlay
  const overlay = document.createElement('div');
  overlay.className = 'inbox-group-modal-overlay';
  const modal = document.createElement('div');
  modal.className = 'inbox-group-modal';

  modal.innerHTML = `
    <h3 class="inbox-group-modal-title">New Group Chat</h3>
    <label class="inbox-group-modal-label">Group Name</label>
    <input type="text" class="inbox-group-name-input" placeholder="e.g. Research Team" />
    <label class="inbox-group-modal-label">Select Agents</label>
    <div class="inbox-group-agent-list"></div>
    <div class="inbox-group-modal-actions">
      <button class="inbox-group-cancel">Cancel</button>
      <button class="inbox-group-create">Create Group</button>
    </div>
  `;

  const agentListEl = modal.querySelector('.inbox-group-agent-list')!;
  const selected = new Set<string>();

  for (const agent of agents) {
    const row = document.createElement('label');
    row.className = 'inbox-group-agent-row';
    const avatarHtml = AgentsModule.spriteAvatar(agent.avatar, 20);
    row.innerHTML = `
      <input type="checkbox" value="${agent.id}" />
      <span class="inbox-group-agent-avatar" style="border-color:${agent.color}">${avatarHtml}</span>
      <span class="inbox-group-agent-name">${agent.name}</span>
    `;
    const checkbox = row.querySelector('input')!;
    checkbox.addEventListener('change', () => {
      if (checkbox.checked) selected.add(agent.id);
      else selected.delete(agent.id);
    });
    agentListEl.appendChild(row);
  }

  overlay.appendChild(modal);
  document.body.appendChild(overlay);

  return new Promise<void>((resolve) => {
    const cancelBtn = modal.querySelector('.inbox-group-cancel')!;
    const createBtn = modal.querySelector('.inbox-group-create')!;
    const nameInput = modal.querySelector('.inbox-group-name-input') as HTMLInputElement;

    const cleanup = () => {
      overlay.remove();
      resolve();
    };

    cancelBtn.addEventListener('click', cleanup);
    overlay.addEventListener('click', (e) => {
      if (e.target === overlay) cleanup();
    });

    createBtn.addEventListener('click', async () => {
      const name = nameInput.value.trim();
      if (selected.size < 2) {
        showToast('Select at least 2 agents for a group chat', 'error');
        return;
      }
      if (!name) {
        showToast('Please enter a group name', 'error');
        return;
      }

      const memberIds = Array.from(selected);
      const primaryAgentId = memberIds[0];

      // Switch to primary agent
      await switchToAgent(primaryAgentId);

      // Use a pending key — the real session gets created when the first message is sent
      const pendingKey = `pending-group_${Date.now()}`;

      // Clear current session so sendMessage creates a new one
      appState.currentSessionKey = null;
      appState.messages = [];
      resetTokenMeter();

      // Store group metadata so sendMessage applies it to the new session
      appState._pendingGroupMeta = {
        name,
        members: memberIds,
        kind: 'group' as const,
      };

      // Also add a local-only session entry so the sidebar shows it immediately
      appState.sessions.unshift({
        key: pendingKey,
        kind: 'group',
        agentId: primaryAgentId,
        label: name,
        displayName: name,
        members: memberIds,
        updatedAt: Date.now(),
      });

      // Persist group metadata so it survives across reloads
      groupSessionMap.set(pendingKey, { name, members: memberIds, kind: 'group' });
      persistGroupSessionMap();

      const chatMessages = $('chat-messages');
      if (chatMessages) chatMessages.innerHTML = '';
      const chatEmpty = $('chat-empty');
      if (chatEmpty) chatEmpty.style.display = '';

      if (_thread) {
        _thread.showThread();
        const memberNames = memberIds
          .map((id) => agents.find((a) => a.id === id)?.name ?? id)
          .join(', ');
        _thread.setAgent(
          name,
          agents.find((a) => a.id === primaryAgentId)?.avatar ?? '5',
          agents.find((a) => a.id === primaryAgentId)?.color ?? 'var(--accent)',
          `Group: ${memberNames}`,
        );
      }

      cleanup();
      showToast(`Group "${name}" created — send a message to start`, 'success');
      refreshConversationList();
    });
  });
}

async function handleSwapAgent(agentId: string): Promise<void> {
  const key = appState.currentSessionKey;
  if (!key) return;

  // Switch agent for the current session — keep session key, just change the agent
  const currentAgent = AgentsModule.getCurrentAgent();
  if (currentAgent?.id === agentId) return;

  // Update session-agent mapping
  agentSessionMap.set(agentId, key);
  persistAgentSessionMap();

  // Switch agent context (loads agent config/prompt but keeps current session)
  await switchToAgent(agentId);

  // Force-restore the original session key (switchToAgent may have changed it)
  appState.currentSessionKey = key;
  await loadChatHistory(key);

  // Update header
  const agent = AgentsModule.getAgents().find((a) => a.id === agentId);
  if (agent && _thread) {
    _thread.setAgent(agent.name, agent.avatar, agent.color, appState.activeModelKey || '');
  }

  // Update conversation entry
  const conv = appState.inbox.conversations.find((c) => c.sessionKey === key);
  if (conv) {
    conv.agentId = agentId;
    conv.agentName = agent?.name ?? 'Paw';
    conv.agentAvatar = agent?.avatar ?? '5';
    conv.agentColor = agent?.color ?? 'var(--accent)';
  }

  refreshConversationList();
  updateSwapAgents();
  renderSessionSelect();
  showToast(`Swapped to ${agent?.name ?? agentId}`, 'success');
}

/** Populate the swap agent dropdown (exclude current agent). */
function updateSwapAgents(): void {
  if (!_thread) return;
  const agents = AgentsModule.getAgents();
  const current = AgentsModule.getCurrentAgent();
  const others = agents
    .filter((a) => a.id !== current?.id)
    .map((a) => ({ id: a.id, name: a.name, avatar: a.avatar, color: a.color }));
  _thread.setSwapAgents(others);
}

function handleFilter(filter: string): void {
  appState.inbox.filter = filter as 'all' | 'unread' | 'agents' | 'groups';
  let filtered = appState.inbox.conversations;
  if (appState.inbox.searchQuery) {
    filtered = filterConversations(filtered, appState.inbox.searchQuery);
  }
  const currentAgent = AgentsModule.getCurrentAgent();
  _list?.render(filtered, currentAgent?.id ?? null, appState.inbox.filter);
}

function handleSearch(query: string): void {
  appState.inbox.searchQuery = query;
  handleFilter(appState.inbox.filter); // re-render with search
}

function handleToggleConvlist(): void {
  appState.inbox.convlistOpen = !appState.inbox.convlistOpen;
  const layout = $('inbox-layout');
  if (layout) {
    layout.classList.toggle('convlist-collapsed', !appState.inbox.convlistOpen);
  }
  persistConvlistPref(appState.inbox.convlistOpen);
}

function handleToggleSidebar(): void {
  appState.inbox.sidebarOpen = !appState.inbox.sidebarOpen;
  const layout = $('inbox-layout');
  if (layout) {
    layout.classList.toggle('sidebar-collapsed', !appState.inbox.sidebarOpen);
  }
  _sidebar?.toggle(appState.inbox.sidebarOpen);
}

async function handleRename(): Promise<void> {
  const key = appState.currentSessionKey;
  if (!key) return;
  const session = appState.sessions.find((s) => s.key === key);
  const current = session?.label ?? '';
  const name = await promptModal('Rename session', current || 'Session label');
  if (name === null) return;
  try {
    await pawEngine.sessionRename(key, name);
    if (session) session.label = name;
    showToast('Session renamed', 'success');
    refreshConversationList();
  } catch {
    showToast('Rename failed', 'error');
  }
}

async function handleDelete(): Promise<void> {
  const key = appState.currentSessionKey;
  if (!key) return;
  const ok = await confirmModal('Delete this session? This cannot be undone.');
  if (!ok) return;
  try {
    await pawEngine.sessionDelete(key);
    appState.sessions = appState.sessions.filter((s) => s.key !== key);
    appState.currentSessionKey = null;
    appState.messages = [];
    _thread?.showEmpty();
    showToast('Session deleted', 'success');
    refreshConversationList();
  } catch {
    showToast('Delete failed', 'error');
  }
}

async function handleClear(): Promise<void> {
  const key = appState.currentSessionKey;
  if (!key) return;
  const ok = await confirmModal('Clear all messages in this session?');
  if (!ok) return;
  try {
    await pawEngine.sessionClear(key);
    appState.messages = [];
    renderMessages();
    resetTokenMeter();
    showToast('History cleared', 'success');
    refreshConversationList();
  } catch {
    showToast('Clear failed', 'error');
  }
}

async function handleCompact(): Promise<void> {
  const key = appState.currentSessionKey;
  if (!key) return;
  try {
    await pawEngine.sessionCompact(key);
    showToast('Session compacted', 'success');
    await loadChatHistory(key);
  } catch {
    showToast('Compact failed', 'error');
  }
}

function handleSearchInConversation(query: string): void {
  // Highlight matching messages in the thread
  const chatMessages = $('chat-messages');
  if (!chatMessages) return;
  const msgEls = chatMessages.querySelectorAll('.chat-message-content');
  const q = query.toLowerCase();
  msgEls.forEach((el) => {
    const textEl = el as HTMLElement;
    if (!q) {
      textEl.style.opacity = '';
      return;
    }
    const match = textEl.textContent?.toLowerCase().includes(q);
    textEl.style.opacity = match ? '' : '0.3';
  });
}

async function handleConversationAction(sessionKey: string, action: string): Promise<void> {
  if (action === 'delete') {
    const ok = await confirmModal('Delete this session?');
    if (!ok) return;
    try {
      await pawEngine.sessionDelete(sessionKey);
      appState.sessions = appState.sessions.filter((s) => s.key !== sessionKey);
      if (appState.currentSessionKey === sessionKey) {
        appState.currentSessionKey = null;
        appState.messages = [];
        _thread?.showEmpty();
      }
      showToast('Session deleted', 'success');
      refreshConversationList();
    } catch {
      showToast('Delete failed', 'error');
    }
  } else if (action === 'pin') {
    const conv = appState.inbox.conversations.find((c) => c.sessionKey === sessionKey);
    if (conv) {
      conv.pinned = !conv.pinned;
      refreshConversationList();
    }
  }
}

// ── Private helpers ──────────────────────────────────────────────────────

function updateThreadHeader(): void {
  if (!_thread) return;
  const key = appState.inbox.activeSessionKey ?? appState.currentSessionKey;
  if (!key) {
    _thread.showEmpty();
    return;
  }

  const conv = appState.inbox.conversations.find((c) => c.sessionKey === key);
  if (!conv) return;

  _thread.setAgent(
    conv.agentName,
    conv.agentAvatar,
    conv.agentColor,
    appState.activeModelKey || '',
  );
  _thread.setStreaming(conv.isStreaming);
}

/**
 * Notify the inbox that streaming state changed for a session.
 * Called from event_bus or chat_controller hooks.
 */
export function notifyStreamingChange(sessionKey: string, active: boolean): void {
  if (!_list || !_mounted) return;
  const conv = appState.inbox.conversations.find((c) => c.sessionKey === sessionKey);
  if (conv) {
    conv.isStreaming = active;
    _list.setStreaming(conv.agentId, active);
  }
  if (sessionKey === appState.inbox.activeSessionKey) {
    _thread?.setStreaming(active);
  }
}

/**
 * Notify inbox that new messages arrived (unread badge update).
 */
export function notifyNewMessage(sessionKey: string): void {
  if (!_list || !_mounted) return;
  if (sessionKey === appState.inbox.activeSessionKey) return; // currently viewing
  const conv = appState.inbox.conversations.find((c) => c.sessionKey === sessionKey);
  if (conv) {
    conv.unread += 1;
    // Sum total unread for this agent
    const agentUnread = appState.inbox.conversations
      .filter((c) => c.agentId === conv.agentId)
      .reduce((sum, c) => sum + c.unread, 0);
    _list.setUnread(conv.agentId, agentUnread);
  }
}
