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
}

/** View entries for the palette, keyed by router viewMap name. */
const VIEW_ENTRIES: { key: string; label: string; icon: string }[] = [
  { key: 'today', label: 'Today', icon: '☀️' },
  { key: 'chat', label: 'Chat', icon: '💬' },
  { key: 'agents', label: 'Agents', icon: '🤖' },
  { key: 'tasks', label: 'Tasks', icon: '📋' },
  { key: 'squads', label: 'Squads', icon: '👥' },
  { key: 'code', label: 'Code', icon: '💻' },
  { key: 'mail', label: 'Mail', icon: '📧' },
  { key: 'channels', label: 'Channels', icon: '📡' },
  { key: 'research', label: 'Research', icon: '🔍' },
  { key: 'trading', label: 'Trading', icon: '📊' },
  { key: 'memory', label: 'Memory Palace', icon: '🧠' },
  { key: 'skills', label: 'Skills', icon: '🔌' },
  { key: 'foundry', label: 'Foundry', icon: '🔧' },
  { key: 'nodes', label: 'Engine', icon: '⚙️' },
  { key: 'settings', label: 'Settings', icon: '⚙️' },
];

/** Simple agent info passed in — avoids importing the full Agent type. */
export interface AgentInfo {
  id: string;
  name: string;
  avatar: string;
}

/** Build the full list of palette items from agents + static views. */
export function buildPaletteItems(agents: AgentInfo[]): PaletteItem[] {
  const items: PaletteItem[] = [];

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
    });
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
