// src/engine/organisms/inbox_controller.ts
// Phase 11.7 — Inbox controller organism.
// Wires conversation_list + inbox_thread + inbox_sidebar molecules together
// and delegates message rendering / sending to the existing chat_controller.
// Reads session/agent state from appState and the pawEngine IPC.

import { pawEngine } from '../../engine';
import { appState, agentSessionMap, persistAgentSessionMap } from '../../state/index';
import { showToast } from '../../components/toast';
import { confirmModal, promptModal } from '../../components/helpers';
import * as AgentsModule from '../../views/agents';
import {
  type ConversationEntry,
  sortConversations,
  filterConversations,
  filterByTab,
  truncatePreview,
} from '../atoms/inbox';
import { createConversationList, type ConversationListController } from '../molecules/conversation_list';
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
    onSelect: handleSelectConversation,
    onNewChat: handleNewChat,
    onFilter: handleFilter,
    onSearch: handleSearch,
    onAction: handleConversationAction,
  });

  _thread = createInboxThread({
    onToggleSidebar: handleToggleSidebar,
    modelSelectEl: $('chat-model-select') as HTMLSelectElement | null,
    onNewChat: handleNewChat,
  });

  _sidebar = createInboxSidebar({
    onRename: handleRename,
    onDelete: handleDelete,
    onClear: handleClear,
    onCompact: handleCompact,
    onColorPick: handleColorPick,
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

  // Hide old mission panel + header (we replace them)
  const missionPanel = $('chat-mission-panel');
  if (missionPanel) missionPanel.style.display = 'none';
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

  _mounted = true;

  // Initial population — await so conversations are rendered before user sees empty state
  refreshConversationList().then(() => {
    // Auto-select the current session if one is active
    if (appState.currentSessionKey && _thread) {
      appState.inbox.activeSessionKey = appState.currentSessionKey;
      _thread.showThread();
      updateThreadHeader();
      _list?.render(
        sortConversations(filterByTab(appState.inbox.conversations, appState.inbox.filter)),
        appState.currentSessionKey,
        appState.inbox.filter,
      );
      updateSidebarMetrics();
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
          lastRole = (msgs[0].role === 'assistant' ? 'assistant' : 'user');
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
      } satisfies ConversationEntry;
    });

    entries.push(...await Promise.all(previewBatch));

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
      });
    }

    appState.inbox.conversations = entries;

    // Apply filter + search
    let filtered = filterByTab(entries, appState.inbox.filter);
    if (appState.inbox.searchQuery) {
      filtered = filterConversations(filtered, appState.inbox.searchQuery);
    }
    const sorted = sortConversations(filtered);

    _list.render(sorted, appState.inbox.activeSessionKey, appState.inbox.filter);

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

  // Restore hidden elements
  const missionPanel = $('chat-mission-panel');
  if (missionPanel) missionPanel.style.display = '';
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

async function handleSelectConversation(sessionKey: string): Promise<void> {
  if (!_thread || !_sidebar) return;

  appState.inbox.activeSessionKey = sessionKey;
  appState.currentSessionKey = sessionKey;

  // Find the conversation entry
  const conv = appState.inbox.conversations.find((c) => c.sessionKey === sessionKey);
  if (!conv) return;

  // Switch agent if needed
  const currentAgent = AgentsModule.getCurrentAgent();
  if (currentAgent?.id !== conv.agentId) {
    await switchToAgent(conv.agentId);
  } else {
    // Just load the history for this session
    renderSessionSelect();
    await loadChatHistory(sessionKey);
  }

  // Update agent-session map
  agentSessionMap.set(conv.agentId, sessionKey);
  persistAgentSessionMap();

  // Show thread
  _thread.showThread();
  updateThreadHeader();

  // Refresh list to update active state
  _list?.render(
    sortConversations(filterByTab(appState.inbox.conversations, appState.inbox.filter)),
    sessionKey,
    appState.inbox.filter,
  );

  // Clear unread
  const entry = appState.inbox.conversations.find((c) => c.sessionKey === sessionKey);
  if (entry) entry.unread = 0;
  _list?.setUnread(sessionKey, 0);
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

function handleFilter(filter: string): void {
  appState.inbox.filter = filter as 'all' | 'unread' | 'agents' | 'groups';
  const filtered = filterByTab(appState.inbox.conversations, appState.inbox.filter);
  const searched = appState.inbox.searchQuery
    ? filterConversations(filtered, appState.inbox.searchQuery)
    : filtered;
  _list?.render(sortConversations(searched), appState.inbox.activeSessionKey, appState.inbox.filter);
}

function handleSearch(query: string): void {
  appState.inbox.searchQuery = query;
  handleFilter(appState.inbox.filter); // re-render with search
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

function handleColorPick(color: string): void {
  // Update the agent color for this conversation visually
  const key = appState.inbox.activeSessionKey;
  if (!key) return;
  const conv = appState.inbox.conversations.find((c) => c.sessionKey === key);
  if (conv) {
    conv.agentColor = color;
    refreshConversationList();
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
  if (!_thread || !_sidebar) return;
  const key = appState.inbox.activeSessionKey ?? appState.currentSessionKey;
  if (!key) {
    _thread.showEmpty();
    return;
  }

  const conv = appState.inbox.conversations.find((c) => c.sessionKey === key);
  if (!conv) return;

  const agents = AgentsModule.getAgents();
  const agent = agents.find((a) => a.id === conv.agentId);

  _thread.setAgent(
    conv.agentName,
    conv.agentAvatar,
    conv.agentColor,
    appState.activeModelKey || '',
  );
  _thread.setStreaming(conv.isStreaming);

  // Update sidebar
  _sidebar.setAgent(
    conv.agentName,
    conv.agentAvatar,
    conv.agentColor,
    agent?.bio ?? '',
    appState.activeModelKey || '',
  );
}

/**
 * Notify the inbox that streaming state changed for a session.
 * Called from event_bus or chat_controller hooks.
 */
export function notifyStreamingChange(sessionKey: string, active: boolean): void {
  if (!_list || !_mounted) return;
  const conv = appState.inbox.conversations.find((c) => c.sessionKey === sessionKey);
  if (conv) conv.isStreaming = active;
  _list.setStreaming(sessionKey, active);
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
    _list.setUnread(sessionKey, conv.unread);
  }
}

/**
 * Update sidebar metrics (called from token meter updates).
 */
export function updateSidebarMetrics(): void {
  if (!_sidebar || !_mounted) return;
  _sidebar.setMetrics(
    fmtK(appState.sessionInputTokens),
    fmtK(appState.sessionOutputTokens),
    appState.sessionCost > 0 ? `$${appState.sessionCost.toFixed(4)}` : '$0',
    String(appState.messages.length),
  );
  const limit = appState.modelContextLimit || 128_000;
  const pct = limit > 0 ? (appState.sessionTokensUsed / limit) * 100 : 0;
  _sidebar.setContext(fmtK(appState.sessionTokensUsed), fmtK(limit), pct);
}

// ── Utility ──────────────────────────────────────────────────────────────

function fmtK(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
  return String(n);
}
