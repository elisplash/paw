// Today View — Pure helpers (no DOM, no IPC)

import type { EngineTask, TaskStatus } from '../../engine/atoms/types';

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
  if (c === 113) return ms('light_mode');
  if (c === 116) return ms('partly_cloudy_day');
  if ([119, 122].includes(c)) return ms('cloud');
  if ([143, 248, 260].includes(c)) return ms('mist');
  if ([176, 263, 266, 293, 296, 299, 302, 305, 308, 311, 314, 353, 356, 359].includes(c))
    return ms('rainy');
  if (
    [
      179, 182, 185, 281, 284, 317, 320, 323, 326, 329, 332, 335, 338, 350, 362, 365, 368, 371, 374,
      377,
    ].includes(c)
  )
    return ms('weather_snowy');
  if ([200, 386, 389, 392, 395].includes(c)) return ms('thunderstorm');
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
