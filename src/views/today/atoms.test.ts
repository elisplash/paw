import { describe, it, expect } from 'vitest';
import {
  getWeatherIcon,
  getGreeting,
  getPawzMessage,
  isToday,
  engineTaskToToday,
  filterTodayTasks,
  toggledStatus,
  activityIcon,
  relativeTime,
  truncateContent,
} from './atoms';
import type { EngineTask } from '../../engine/atoms/types';

// ── getWeatherIcon ─────────────────────────────────────────────────────

describe('getWeatherIcon', () => {
  it('returns sun icon for clear code 113', () => {
    expect(getWeatherIcon('113')).toContain('light_mode');
  });

  it('returns cloud icon for overcast codes', () => {
    expect(getWeatherIcon('119')).toContain('cloud');
  });

  it('returns rain icon for rain codes', () => {
    expect(getWeatherIcon('176')).toContain('rainy');
  });

  it('returns snow icon for snow codes', () => {
    expect(getWeatherIcon('179')).toContain('weather_snowy');
  });

  it('returns thunderstorm icon', () => {
    expect(getWeatherIcon('200')).toContain('thunderstorm');
  });

  it('returns default for unknown code', () => {
    expect(getWeatherIcon('999')).toContain('partly_cloudy_day');
  });
});

// ── getGreeting ────────────────────────────────────────────────────────

describe('getGreeting', () => {
  it('returns a greeting string', () => {
    const g = getGreeting();
    expect(g).toMatch(/Good (morning|afternoon|evening)/);
  });
});

// ── getPawzMessage ─────────────────────────────────────────────────────

describe('getPawzMessage', () => {
  it('mentions completed tasks when all done', () => {
    const msg = getPawzMessage(0, 5);
    expect(msg).toContain('done');
  });

  it('mentions progress when both pending and completed', () => {
    const msg = getPawzMessage(3, 2);
    expect(msg).toContain('2 down');
  });

  it('mentions pending tasks when none completed', () => {
    const msg = getPawzMessage(5, 0);
    expect(msg).toContain('5 tasks');
  });

  it('handles no tasks', () => {
    const msg = getPawzMessage(0, 0);
    expect(msg).toContain('No tasks');
  });
});

// ── isToday ────────────────────────────────────────────────────────────

describe('isToday', () => {
  it('returns true for today', () => {
    expect(isToday(new Date().toISOString())).toBe(true);
  });

  it('returns false for yesterday', () => {
    const yesterday = new Date();
    yesterday.setDate(yesterday.getDate() - 1);
    expect(isToday(yesterday.toISOString())).toBe(false);
  });
});

// ── engineTaskToToday ──────────────────────────────────────────────────

function makeEngineTask(overrides: Partial<EngineTask> = {}): EngineTask {
  return {
    id: 'task-1',
    title: 'Test task',
    description: '',
    status: 'inbox',
    priority: 'medium',
    assigned_agents: [],
    cron_enabled: false,
    created_at: '2025-01-01T00:00:00Z',
    updated_at: '2025-01-01T00:00:00Z',
    ...overrides,
  };
}

describe('engineTaskToToday', () => {
  it('maps title to text', () => {
    const result = engineTaskToToday(makeEngineTask({ title: 'Hello' }));
    expect(result.text).toBe('Hello');
  });

  it('maps id through', () => {
    const result = engineTaskToToday(makeEngineTask({ id: 'abc' }));
    expect(result.id).toBe('abc');
  });

  it('maps status done → done true', () => {
    const result = engineTaskToToday(makeEngineTask({ status: 'done' }));
    expect(result.done).toBe(true);
  });

  it('maps status inbox → done false', () => {
    const result = engineTaskToToday(makeEngineTask({ status: 'inbox' }));
    expect(result.done).toBe(false);
  });

  it('maps status in_progress → done false', () => {
    const result = engineTaskToToday(makeEngineTask({ status: 'in_progress' }));
    expect(result.done).toBe(false);
  });

  it('preserves created_at', () => {
    const result = engineTaskToToday(makeEngineTask({ created_at: '2025-06-15T12:00:00Z' }));
    expect(result.createdAt).toBe('2025-06-15T12:00:00Z');
  });
});

// ── filterTodayTasks ───────────────────────────────────────────────────

describe('filterTodayTasks', () => {
  it('excludes cron tasks', () => {
    const tasks = [
      makeEngineTask({ id: '1' }),
      makeEngineTask({ id: '2', cron_schedule: '0 9 * * *' }),
    ];
    const result = filterTodayTasks(tasks);
    expect(result).toHaveLength(1);
    expect(result[0].id).toBe('1');
  });

  it('includes tasks without cron_schedule', () => {
    const tasks = [makeEngineTask({ id: '1' }), makeEngineTask({ id: '2' })];
    expect(filterTodayTasks(tasks)).toHaveLength(2);
  });

  it('returns empty for all-cron list', () => {
    const tasks = [makeEngineTask({ cron_schedule: '* * * * *' })];
    expect(filterTodayTasks(tasks)).toHaveLength(0);
  });

  it('returns empty for empty input', () => {
    expect(filterTodayTasks([])).toHaveLength(0);
  });
});

// ── toggledStatus ──────────────────────────────────────────────────────

describe('toggledStatus', () => {
  it('toggles done → inbox', () => {
    expect(toggledStatus('done')).toBe('inbox');
  });

  it('toggles inbox → done', () => {
    expect(toggledStatus('inbox')).toBe('done');
  });

  it('toggles in_progress → done', () => {
    expect(toggledStatus('in_progress')).toBe('done');
  });

  it('toggles assigned → done', () => {
    expect(toggledStatus('assigned')).toBe('done');
  });
});

// ── activityIcon ──────────────────────────────────────────────────────

describe('activityIcon', () => {
  it('returns add_circle for created', () => {
    expect(activityIcon('created')).toBe('add_circle');
  });

  it('returns check_circle for completed', () => {
    expect(activityIcon('completed')).toBe('check_circle');
  });

  it('returns build for tool_call', () => {
    expect(activityIcon('tool_call')).toBe('build');
  });

  it('returns info for unknown kind', () => {
    expect(activityIcon('something_unknown')).toBe('info');
  });
});

// ── relativeTime ──────────────────────────────────────────────────────

describe('relativeTime', () => {
  it('returns "just now" for recent timestamps', () => {
    expect(relativeTime(new Date().toISOString())).toBe('just now');
  });

  it('returns minutes ago', () => {
    const fiveMinAgo = new Date(Date.now() - 5 * 60000).toISOString();
    expect(relativeTime(fiveMinAgo)).toBe('5m ago');
  });

  it('returns hours ago', () => {
    const twoHoursAgo = new Date(Date.now() - 2 * 3600000).toISOString();
    expect(relativeTime(twoHoursAgo)).toBe('2h ago');
  });

  it('returns days ago', () => {
    const threeDaysAgo = new Date(Date.now() - 3 * 86400000).toISOString();
    expect(relativeTime(threeDaysAgo)).toBe('3d ago');
  });
});

// ── truncateContent ──────────────────────────────────────────────────

describe('truncateContent', () => {
  it('returns content unchanged when under limit', () => {
    expect(truncateContent('hello', 10)).toBe('hello');
  });

  it('truncates and adds ellipsis', () => {
    expect(truncateContent('hello world', 5)).toBe('hello…');
  });

  it('returns content at exact limit without ellipsis', () => {
    expect(truncateContent('hello', 5)).toBe('hello');
  });
});
