// src/engine/atoms/inbox.ts
// Phase 11.1 — Inbox atom layer.
// Pure types and helper functions for the Agent Inbox.
// Zero side effects, zero DOM access, zero imports from molecules/organisms.

// ── Conversation entry ───────────────────────────────────────────────────

/**
 * Lightweight summary for the conversation list (left panel).
 * Derived from Session + agent data + last-message preview.
 */
export interface ConversationEntry {
  /** Backend session key */
  sessionKey: string;
  /** Agent id that owns this conversation */
  agentId: string;
  /** Agent display name */
  agentName: string;
  /** Agent avatar (sprite id or emoji) */
  agentAvatar: string;
  /** Agent accent colour (CSS value) */
  agentColor: string;
  /** Last message preview (truncated) */
  lastMessage: string;
  /** Role of last message */
  lastRole: 'user' | 'assistant' | 'system';
  /** Timestamp of last message (epoch ms) */
  lastTs: number;
  /** Unread message count since last viewed */
  unread: number;
  /** Session display name / label */
  label: string;
  /** Whether a stream is active for this session */
  isStreaming: boolean;
  /** Session kind */
  kind: 'direct' | 'group' | 'global' | 'unknown';
  /** Whether this entry is pinned to the top */
  pinned: boolean;
}

// ── Inbox state ──────────────────────────────────────────────────────────

export interface InboxState {
  conversations: ConversationEntry[];
  activeSessionKey: string | null;
  searchQuery: string;
  sidebarOpen: boolean;
  filter: 'all' | 'unread' | 'agents' | 'groups';
}

export function createInboxState(): InboxState {
  return {
    conversations: [],
    activeSessionKey: null,
    searchQuery: '',
    sidebarOpen: loadSidebarPref(),
    filter: 'all',
  };
}

// ── Pure helpers ─────────────────────────────────────────────────────────

/** Sort conversations: pinned first, then by lastTs descending. */
export function sortConversations(entries: ConversationEntry[]): ConversationEntry[] {
  return [...entries].sort((a, b) => {
    if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
    return b.lastTs - a.lastTs;
  });
}

/** Filter conversations by search query (matches agent name, label, last message). */
export function filterConversations(
  entries: ConversationEntry[],
  query: string,
): ConversationEntry[] {
  if (!query.trim()) return entries;
  const q = query.toLowerCase();
  return entries.filter(
    (e) =>
      e.agentName.toLowerCase().includes(q) ||
      e.label.toLowerCase().includes(q) ||
      e.lastMessage.toLowerCase().includes(q),
  );
}

/** Filter by inbox filter tab. */
export function filterByTab(
  entries: ConversationEntry[],
  filter: InboxState['filter'],
): ConversationEntry[] {
  switch (filter) {
    case 'unread':
      return entries.filter((e) => e.unread > 0);
    case 'agents':
      return entries.filter((e) => e.kind === 'direct');
    case 'groups':
      return entries.filter((e) => e.kind === 'group');
    default:
      return entries;
  }
}

/** Truncate a message string for preview display. */
export function truncatePreview(content: string, maxLen = 60): string {
  const cleaned = content.replace(/\s+/g, ' ').trim();
  if (cleaned.length <= maxLen) return cleaned;
  // Cut at word boundary
  const cut = cleaned.slice(0, maxLen);
  const lastSpace = cut.lastIndexOf(' ');
  return (lastSpace > maxLen * 0.6 ? cut.slice(0, lastSpace) : cut) + '…';
}

/** Format a timestamp as a relative string ("2m", "1h", "Yesterday", "Jan 15"). */
export function formatRelativeTime(ts: number, now = Date.now()): string {
  const diff = now - ts;
  if (diff < 0) return 'now';
  const secs = Math.floor(diff / 1000);
  if (secs < 60) return 'now';
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h`;

  const date = new Date(ts);
  const today = new Date(now);
  today.setHours(0, 0, 0, 0);
  const yesterday = new Date(today);
  yesterday.setDate(yesterday.getDate() - 1);

  if (ts >= yesterday.getTime() && ts < today.getTime()) return 'Yesterday';

  const thisYear = today.getFullYear();
  const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
  const monthStr = months[date.getMonth()];
  if (date.getFullYear() === thisYear) return `${monthStr} ${date.getDate()}`;
  return `${monthStr} ${date.getDate()}, ${date.getFullYear()}`;
}

/** Find a conversation by session key. */
export function findConversation(
  entries: ConversationEntry[],
  sessionKey: string,
): ConversationEntry | undefined {
  return entries.find((e) => e.sessionKey === sessionKey);
}

/** Update a conversation entry (immutable — returns new array). */
export function updateConversation(
  entries: ConversationEntry[],
  sessionKey: string,
  patch: Partial<ConversationEntry>,
): ConversationEntry[] {
  return entries.map((e) => (e.sessionKey === sessionKey ? { ...e, ...patch } : e));
}

/** Remove a conversation by session key. */
export function removeConversation(
  entries: ConversationEntry[],
  sessionKey: string,
): ConversationEntry[] {
  return entries.filter((e) => e.sessionKey !== sessionKey);
}

// ── Sidebar preference ───────────────────────────────────────────────────

const SIDEBAR_KEY = 'paw_inbox_sidebar';

function loadSidebarPref(): boolean {
  try {
    const v = localStorage.getItem(SIDEBAR_KEY);
    return v !== 'false'; // default open
  } catch {
    return true;
  }
}

export function persistSidebarPref(open: boolean): void {
  try {
    localStorage.setItem(SIDEBAR_KEY, String(open));
  } catch {
    /* ignore */
  }
}
