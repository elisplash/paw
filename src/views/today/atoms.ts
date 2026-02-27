// Today View — Pure helpers (no DOM, no IPC)

import type {
  EngineTask,
  TaskStatus,
  EngineTaskActivity,
  EngineSkillStatus,
} from '../../engine/atoms/types';

export interface Task {
  id: string;
  text: string;
  done: boolean;
  createdAt: string;
}

/** Convert an EngineTask to the lighter Today Task for display. */
export function engineTaskToToday(et: EngineTask): Task {
  return {
    id: et.id,
    text: et.title,
    done: et.status === 'done',
    createdAt: et.created_at,
  };
}

/** Filter engine tasks relevant to the Today view (not cron, not done-old). */
export function filterTodayTasks(tasks: EngineTask[]): EngineTask[] {
  return tasks.filter((t) => !t.cron_schedule);
}

/** The status to set when toggling a task's done state. */
export function toggledStatus(current: TaskStatus): TaskStatus {
  return current === 'done' ? 'inbox' : 'done';
}

/** Map WMO weather code to Material Symbol icon */
export function getWeatherIcon(code: string): string {
  const c = parseInt(code);
  const ms = (name: string) => `<span class="ms ms-lg">${name}</span>`;
  if (c === 0) return ms('light_mode');
  if (c >= 1 && c <= 2) return ms('partly_cloudy_day');
  if (c === 3) return ms('cloud');
  if ([45, 48].includes(c)) return ms('mist');
  if (c >= 51 && c <= 67) return ms('rainy');
  if ((c >= 71 && c <= 77) || c === 85 || c === 86) return ms('weather_snowy');
  if (c >= 80 && c <= 82) return ms('rainy');
  if (c >= 95 && c <= 99) return ms('thunderstorm');
  return ms('partly_cloudy_day');
}

export function getGreeting(): string {
  const hour = new Date().getHours();
  if (hour < 12) return 'Good morning';
  if (hour < 17) return 'Good afternoon';
  return 'Good evening';
}

export function getPawzMessage(pendingTasks: number, completedToday: number): string {
  const hour = new Date().getHours();
  const day = new Date().toLocaleDateString('en-US', { weekday: 'long' });

  let message = '';

  if (hour < 12) {
    message = `Happy ${day}! Ready to make today count? `;
  } else if (hour < 17) {
    message = `Hope your ${day} is going well. `;
  } else {
    message = `Winding down this ${day}. `;
  }

  if (completedToday > 0 && pendingTasks === 0) {
    message += `You crushed it — ${completedToday} task${completedToday > 1 ? 's' : ''} done and nothing pending!`;
  } else if (completedToday > 0) {
    message += `Nice progress! ${completedToday} down, ${pendingTasks} to go.`;
  } else if (pendingTasks > 0) {
    message += `You've got ${pendingTasks} task${pendingTasks > 1 ? 's' : ''} lined up. Let's knock them out.`;
  } else {
    message += `No tasks on the board yet. Add something or hit Morning Briefing to get started.`;
  }

  return message;
}

export function isToday(dateStr: string): boolean {
  const date = new Date(dateStr);
  const today = new Date();
  return date.toDateString() === today.toDateString();
}

// ── Activity Feed Helpers ─────────────────────────────────────────────

export interface ActivityDisplayItem {
  id: string;
  icon: string;
  label: string;
  time: string;
  agent?: string;
}

/** Map activity kind to a Material Symbol icon name. */
export function activityIcon(kind: string): string {
  const map: Record<string, string> = {
    created: 'add_circle',
    status_change: 'swap_horiz',
    comment: 'chat_bubble',
    tool_call: 'build',
    message: 'forum',
    completed: 'check_circle',
    failed: 'error',
    started: 'play_arrow',
  };
  return map[kind] ?? 'info';
}

/** Format an ISO timestamp to a short relative string (e.g. "3m ago", "2h ago"). */
export function relativeTime(isoStr: string): string {
  const diff = Date.now() - new Date(isoStr).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

/** Truncate content to maxLen characters, adding ellipsis. */
export function truncateContent(content: string, maxLen: number): string {
  if (content.length <= maxLen) return content;
  return `${content.slice(0, maxLen)}…`;
}

// ── Command Center Helpers ────────────────────────────────────────────

/** Format a token count to a compact string (e.g. 14200 → "14.2k"). */
export function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return n.toString();
}

/** Format cost in dollars with 2 decimal precision. */
export function formatCost(n: number): string {
  return `$${n.toFixed(2)}`;
}

/** Determine agent status from its last activity timestamp. */
export function agentStatus(lastUsed?: string): 'active' | 'idle' | 'offline' {
  if (!lastUsed) return 'offline';
  const diffMs = Date.now() - new Date(lastUsed).getTime();
  if (diffMs < 60_000) return 'active'; // active within last minute
  return 'idle';
}

// ── Capabilities Helpers ──────────────────────────────────────────────

export interface CapabilityGroup {
  label: string;
  icon: string;
  capabilities: string[];
}

const CATEGORY_META: Record<string, { icon: string; label: string }> = {
  communication: { icon: 'mail', label: 'Communication' },
  web: { icon: 'language', label: 'Web & Research' },
  development: { icon: 'code', label: 'Development' },
  trading: { icon: 'candlestick_chart', label: 'Trading' },
  productivity: { icon: 'task_alt', label: 'Productivity' },
  media: { icon: 'image', label: 'Media & Content' },
  system: { icon: 'settings', label: 'System' },
  storage: { icon: 'cloud', label: 'Storage' },
  search: { icon: 'search', label: 'Search' },
  automation: { icon: 'smart_toy', label: 'Automation' },
};

/** Group enabled skills into human-readable capability categories. */
export function buildCapabilityGroups(skills: EngineSkillStatus[]): CapabilityGroup[] {
  const groups = new Map<string, string[]>();

  for (const skill of skills) {
    const cat = (skill.category || 'general').toLowerCase();
    if (!groups.has(cat)) groups.set(cat, []);
    groups.get(cat)!.push(skill.description || skill.name);
  }

  return Array.from(groups.entries())
    .map(([cat, descriptions]) => {
      const meta = CATEGORY_META[cat] || {
        icon: 'extension',
        label: cat.charAt(0).toUpperCase() + cat.slice(1),
      };
      return {
        label: meta.label,
        icon: meta.icon,
        capabilities: descriptions,
      };
    })
    .sort((a, b) => a.label.localeCompare(b.label));
}

// ── Tour Step Definitions ─────────────────────────────────────────────

export interface TourStep {
  target: string; // CSS selector
  title: string;
  description: string;
  position: 'right' | 'bottom' | 'left';
}

export const TOUR_STEPS: TourStep[] = [
  {
    target: '[data-view="chat"]',
    title: 'Chat with AI',
    description:
      'Talk to your agents, ask questions, and get tasks done through natural conversation.',
    position: 'right',
  },
  {
    target: '[data-view="agents"]',
    title: 'Your Agent Fleet',
    description:
      'Create AI agents with unique personas, specialized tools, and custom instructions.',
    position: 'right',
  },
  {
    target: '[data-view="settings-skills"]',
    title: 'Skills & Integrations',
    description:
      'Enable capabilities like email, web browsing, coding, trading, and hundreds more.',
    position: 'right',
  },
  {
    target: '[data-view="tasks"]',
    title: 'Task Board',
    description: 'Organize work on a kanban board. Assign tasks to agents and track progress.',
    position: 'right',
  },
  {
    target: '[data-view="settings"]',
    title: 'Settings',
    description: 'Configure AI providers, models, security policies, and customize your workspace.',
    position: 'right',
  },
];

// ── Showcase Demo Data ────────────────────────────────────────────────

export interface ShowcaseAgent {
  name: string;
  avatar: string;
  lastUsed: string;
}

export interface ShowcaseData {
  agents: ShowcaseAgent[];
  tasks: Task[];
  skillNames: string[];
  tokenCount: number;
  cost: number;
}

/** Generate synthetic demo data for Showcase mode. */
export function buildShowcaseData(): ShowcaseData {
  const now = Date.now();
  return {
    agents: [
      { name: 'Atlas', avatar: 'default', lastUsed: new Date(now - 30_000).toISOString() },
      { name: 'Scout', avatar: 'default', lastUsed: new Date(now - 120_000).toISOString() },
      { name: 'Forge', avatar: 'default', lastUsed: new Date(now - 600_000).toISOString() },
    ],
    tasks: [
      {
        id: 'demo-1',
        text: 'Review pull request #42',
        done: false,
        createdAt: new Date(now - 3600_000).toISOString(),
      },
      {
        id: 'demo-2',
        text: 'Draft weekly standup notes',
        done: false,
        createdAt: new Date(now - 7200_000).toISOString(),
      },
      {
        id: 'demo-3',
        text: 'Research competitor pricing',
        done: true,
        createdAt: new Date(now - 1800_000).toISOString(),
      },
      {
        id: 'demo-4',
        text: 'Update API documentation',
        done: false,
        createdAt: new Date(now - 5400_000).toISOString(),
      },
    ],
    skillNames: ['Email', 'Browser', 'GitHub', 'File System', 'Shell', 'Web Search', 'Calendar'],
    tokenCount: 48_720,
    cost: 1.24,
  };
}

/** Build 30-day activity data from task activity items. */
export function buildHeatmapData(
  activities: EngineTaskActivity[],
): { date: string; count: number }[] {
  const days: { date: string; count: number }[] = [];
  const today = new Date();
  const counts = new Map<string, number>();

  for (const a of activities) {
    const d = a.created_at.slice(0, 10); // "YYYY-MM-DD"
    counts.set(d, (counts.get(d) ?? 0) + 1);
  }

  for (let i = 29; i >= 0; i--) {
    const d = new Date(today);
    d.setDate(d.getDate() - i);
    const key = d.toISOString().slice(0, 10);
    days.push({ date: key, count: counts.get(key) ?? 0 });
  }

  return days;
}
