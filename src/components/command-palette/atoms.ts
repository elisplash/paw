// atoms.ts — Pure types and helpers for the command palette
// NO DOM, NO side effects, NO imports with side effects

export type PaletteItemKind = 'agent' | 'view' | 'action';

export interface PaletteItem {
  id: string;
  label: string;
  kind: PaletteItemKind;
  icon?: string;
  /** For agents: the agent id. For views: the view key. For actions: action key. */
  payload: string;
  description?: string;
  /** Keyboard hint shown on the right, e.g. "⌘K" */
  shortcut?: string;
}

/** View entries for the palette, keyed by router viewMap name. */
const VIEW_ENTRIES: { key: string; label: string; icon: string; shortcut?: string }[] = [
  { key: 'today', label: 'Today', icon: '☀️', shortcut: '1' },
  { key: 'chat', label: 'Chat', icon: '💬', shortcut: '2' },
  { key: 'agents', label: 'Agents', icon: '🤖', shortcut: '3' },
  { key: 'tasks', label: 'Tasks', icon: '📋', shortcut: '4' },
  { key: 'mail', label: 'Mail', icon: '📧', shortcut: '5' },
  { key: 'channels', label: 'Channels', icon: '📡', shortcut: '6' },
  { key: 'skills', label: 'My Skills', icon: '🔌', shortcut: '7' },
  { key: 'pawzhub', label: 'PawzHub', icon: '🏪', shortcut: '8' },
  { key: 'foundry', label: 'Foundry', icon: '🔧', shortcut: '9' },
  { key: 'settings', label: 'Settings', icon: '⚙️', shortcut: '⌘,' },
];

/** Action entries that appear in the palette. */
const ACTION_ENTRIES: { key: string; label: string; icon: string; description: string; shortcut?: string }[] = [
  { key: 'new-task', label: 'New Task', icon: '➕', description: 'Create a task', shortcut: '⌘N' },
  { key: 'new-chat', label: 'New Chat', icon: '💬', description: 'Start a new conversation' },
  { key: 'toggle-theme', label: 'Toggle Theme', icon: '🎨', description: 'Switch dark/light mode' },
  { key: 'shortcuts', label: 'Keyboard Shortcuts', icon: '⌨️', description: 'Show all shortcuts', shortcut: '?' },
];

/** Simple agent info passed in — avoids importing the full Agent type. */
export interface AgentInfo {
  id: string;
  name: string;
  avatar: string;
}

/** Skill info for palette search. */
export interface SkillInfo {
  id: string;
  name: string;
  enabled: boolean;
  icon?: string;
}

/** Build the full list of palette items from agents + views + actions + skills. */
export function buildPaletteItems(agents: AgentInfo[], skills?: SkillInfo[]): PaletteItem[] {
  const items: PaletteItem[] = [];

  // Action items (come first — they're quick commands)
  for (const a of ACTION_ENTRIES) {
    items.push({
      id: `action-${a.key}`,
      label: a.label,
      kind: 'action',
      icon: a.icon,
      payload: a.key,
      description: a.description,
      shortcut: a.shortcut,
    });
  }

  // Agent items
  for (const agent of agents) {
    items.push({
      id: `agent-${agent.id}`,
      label: agent.name,
      kind: 'agent',
      icon: agent.avatar,
      payload: agent.id,
      description: 'Switch to agent',
    });
  }

  // View items
  for (const v of VIEW_ENTRIES) {
    items.push({
      id: `view-${v.key}`,
      label: v.label,
      kind: 'view',
      icon: v.icon,
      payload: v.key,
      description: 'Go to view',
      shortcut: v.shortcut,
    });
  }

  // Skill items (if provided)
  if (skills?.length) {
    for (const s of skills) {
      items.push({
        id: `skill-${s.id}`,
        label: `${s.name} ${s.enabled ? '(on)' : '(off)'}`,
        kind: 'action',
        icon: s.icon ?? '🔌',
        payload: `skill-toggle:${s.id}`,
        description: s.enabled ? 'Disable skill' : 'Enable skill',
      });
    }
  }

  return items;
}

/** Filter palette items by query string (case-insensitive substring match). */
export function filterPaletteItems(items: PaletteItem[], query: string): PaletteItem[] {
  const q = query.toLowerCase().trim();
  if (!q) return items;
  return items.filter(
    (item) =>
      item.label.toLowerCase().includes(q) ||
      item.kind.toLowerCase().includes(q) ||
      (item.description ?? '').toLowerCase().includes(q),
  );
}

/** Clamp an index within [0, length - 1]. Returns -1 when length is 0. */
export function clampIndex(index: number, length: number): number {
  if (length <= 0) return -1;
  if (index < 0) return 0;
  if (index >= length) return length - 1;
  return index;
}
