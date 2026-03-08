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
  formatTokens,
  formatCost,
  agentStatus,
  buildCapabilityGroups,
  buildShowcaseData,
  buildHeatmapData,
  TOUR_STEPS,
} from './atoms';
import type { EngineTask, EngineSkillStatus, EngineTaskActivity } from '../../engine/atoms/types';

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

// ── buildCapabilityGroups ─────────────────────────────────────────────

function makeSkill(overrides: Partial<EngineSkillStatus> = {}): EngineSkillStatus {
  return {
    id: 'test-skill',
    name: 'Test Skill',
    description: 'A test skill',
    icon: '🔧',
    category: 'general',
    tier: 'skill',
    enabled: true,
    required_credentials: [],
    configured_credentials: [],
    missing_credentials: [],
    missing_binaries: [],
    required_env_vars: [],
    missing_env_vars: [],
    install_hint: '',
    has_instructions: false,
    is_ready: true,
    tool_names: [],
    default_instructions: '',
    custom_instructions: '',
    ...overrides,
  };
}

describe('buildCapabilityGroups', () => {
  it('returns empty array for empty skills', () => {
    expect(buildCapabilityGroups([])).toEqual([]);
  });

  it('groups skills by category', () => {
    const skills = [
      makeSkill({
        id: 's1',
        name: 'Email',
        description: 'Send and receive email',
        category: 'communication',
      }),
      makeSkill({
        id: 's2',
        name: 'Slack',
        description: 'Post to Slack',
        category: 'communication',
      }),
      makeSkill({ id: 's3', name: 'Browser', description: 'Browse the web', category: 'web' }),
    ];
    const groups = buildCapabilityGroups(skills);
    expect(groups).toHaveLength(2);

    const commGroup = groups.find((g) => g.label === 'Communication');
    expect(commGroup).toBeDefined();
    expect(commGroup!.capabilities).toHaveLength(2);
    expect(commGroup!.icon).toBe('mail');

    const webGroup = groups.find((g) => g.label === 'Web & Research');
    expect(webGroup).toBeDefined();
    expect(webGroup!.capabilities).toHaveLength(1);
  });

  it('uses skill description as capability text', () => {
    const skills = [makeSkill({ description: 'Browse the web', category: 'web' })];
    const groups = buildCapabilityGroups(skills);
    expect(groups[0].capabilities[0]).toBe('Browse the web');
  });

  it('falls back to skill name when no description', () => {
    const skills = [makeSkill({ name: 'Browser', description: '', category: 'web' })];
    const groups = buildCapabilityGroups(skills);
    expect(groups[0].capabilities[0]).toBe('Browser');
  });

  it('uses generic icon for unknown categories', () => {
    const skills = [makeSkill({ category: 'custom_thing' })];
    const groups = buildCapabilityGroups(skills);
    expect(groups[0].icon).toBe('extension');
    expect(groups[0].label).toBe('Custom_thing');
  });

  it('sorts groups alphabetically by label', () => {
    const skills = [
      makeSkill({ id: 's1', category: 'web' }),
      makeSkill({ id: 's2', category: 'communication' }),
      makeSkill({ id: 's3', category: 'development' }),
    ];
    const groups = buildCapabilityGroups(skills);
    expect(groups[0].label).toBe('Communication');
    expect(groups[1].label).toBe('Development');
    expect(groups[2].label).toBe('Web & Research');
  });
});

// ── buildShowcaseData ─────────────────────────────────────────────────

describe('buildShowcaseData', () => {
  it('returns demo agents', () => {
    const data = buildShowcaseData();
    expect(data.agents).toHaveLength(3);
    expect(data.agents[0].name).toBe('Atlas');
  });

  it('returns demo tasks', () => {
    const data = buildShowcaseData();
    expect(data.tasks.length).toBeGreaterThan(0);
    expect(data.tasks[0].text).toBeTruthy();
  });

  it('returns demo skill names', () => {
    const data = buildShowcaseData();
    expect(data.skillNames.length).toBeGreaterThan(0);
    expect(data.skillNames).toContain('Browser');
  });

  it('returns positive token count and cost', () => {
    const data = buildShowcaseData();
    expect(data.tokenCount).toBeGreaterThan(0);
    expect(data.cost).toBeGreaterThan(0);
  });
});

// ── formatTokens ──────────────────────────────────────────────────────

describe('formatTokens', () => {
  it('formats millions', () => {
    expect(formatTokens(1_500_000)).toBe('1.5M');
  });

  it('formats thousands with k', () => {
    expect(formatTokens(14_200)).toBe('14.2k');
  });

  it('returns raw for small numbers', () => {
    expect(formatTokens(42)).toBe('42');
  });

  it('handles exact boundaries', () => {
    expect(formatTokens(1_000)).toBe('1.0k');
    expect(formatTokens(1_000_000)).toBe('1.0M');
  });

  it('handles zero', () => {
    expect(formatTokens(0)).toBe('0');
  });
});

// ── formatCost ────────────────────────────────────────────────────────

describe('formatCost', () => {
  it('formats with dollar sign and 2 decimals', () => {
    expect(formatCost(1.24)).toBe('$1.24');
  });

  it('formats zero', () => {
    expect(formatCost(0)).toBe('$0.00');
  });

  it('rounds correctly', () => {
    expect(formatCost(1.999)).toBe('$2.00');
  });
});

// ── agentStatus ───────────────────────────────────────────────────────

describe('agentStatus', () => {
  it('returns offline when no timestamp', () => {
    expect(agentStatus()).toBe('offline');
    expect(agentStatus(undefined)).toBe('offline');
  });

  it('returns active within 5 minutes', () => {
    const recent = new Date(Date.now() - 10_000).toISOString();
    expect(agentStatus(recent)).toBe('active');
  });

  it('returns idle after 5 minutes but within 24 hours', () => {
    const old = new Date(Date.now() - 10 * 60_000).toISOString();
    expect(agentStatus(old)).toBe('idle');
  });

  it('returns offline after 24 hours', () => {
    const veryOld = new Date(Date.now() - 25 * 60 * 60_000).toISOString();
    expect(agentStatus(veryOld)).toBe('offline');
  });
});

// ── buildHeatmapData ──────────────────────────────────────────────────

describe('buildHeatmapData', () => {
  it('returns 30 days of data', () => {
    const result = buildHeatmapData([]);
    expect(result).toHaveLength(30);
  });

  it('returns zero for empty input', () => {
    const result = buildHeatmapData([]);
    expect(result.every((d) => d.count === 0)).toBe(true);
  });

  it('counts activities per day', () => {
    const today = new Date().toISOString().slice(0, 10);
    const activities: EngineTaskActivity[] = [
      {
        id: '1',
        task_id: 't1',
        kind: 'created',
        content: '',
        created_at: `${today}T10:00:00Z`,
      } as any,
      {
        id: '2',
        task_id: 't1',
        kind: 'comment',
        content: '',
        created_at: `${today}T11:00:00Z`,
      } as any,
    ];
    const result = buildHeatmapData(activities);
    const todayEntry = result.find((d) => d.date === today);
    expect(todayEntry).toBeDefined();
    expect(todayEntry!.count).toBe(2);
  });

  it('counts sessions when provided', () => {
    const today = new Date().toISOString().slice(0, 10);
    const result = buildHeatmapData(
      [],
      [{ created_at: `${today}T08:00:00Z`, updated_at: `${today}T09:00:00Z` }],
    );
    const todayEntry = result.find((d) => d.date === today);
    expect(todayEntry!.count).toBe(1);
  });

  it('uses updated_at for session date', () => {
    const today = new Date().toISOString().slice(0, 10);
    const result = buildHeatmapData(
      [],
      [{ created_at: '2024-01-01T00:00:00Z', updated_at: `${today}T12:00:00Z` }],
    );
    const todayEntry = result.find((d) => d.date === today);
    expect(todayEntry!.count).toBe(1);
  });

  it('each entry has date and count', () => {
    const result = buildHeatmapData([]);
    for (const entry of result) {
      expect(entry.date).toMatch(/^\d{4}-\d{2}-\d{2}$/);
      expect(typeof entry.count).toBe('number');
    }
  });
});

// ── TOUR_STEPS ────────────────────────────────────────────────────────

describe('TOUR_STEPS', () => {
  it('has steps with required fields', () => {
    for (const step of TOUR_STEPS) {
      expect(step.target).toBeTruthy();
      expect(step.title).toBeTruthy();
      expect(step.description).toBeTruthy();
      expect(['right', 'bottom', 'left']).toContain(step.position);
    }
  });

  it('has at least 3 tour steps', () => {
    expect(TOUR_STEPS.length).toBeGreaterThanOrEqual(3);
  });

  it('targets use data-view selectors', () => {
    for (const step of TOUR_STEPS) {
      expect(step.target).toContain('[data-view=');
    }
  });
});
