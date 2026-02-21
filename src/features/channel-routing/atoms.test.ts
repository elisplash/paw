import { describe, it, expect } from 'vitest';
import {
  resolveRoute,
  createRule,
  validateRoutingConfig,
  describeRoutingConfig,
  DEFAULT_ROUTING_CONFIG,
  ALL_CHANNELS,
} from './atoms';
import type { RoutingConfig } from './atoms';

// ── resolveRoute ───────────────────────────────────────────────────────

describe('resolveRoute', () => {
  it('returns default agent when no rules', () => {
    const result = resolveRoute(DEFAULT_ROUTING_CONFIG, 'telegram', 'user1');
    expect(result.agentId).toBe('default');
    expect(result.matchedRuleId).toBeNull();
  });

  it('matches first matching rule', () => {
    const config: RoutingConfig = {
      defaultAgentId: 'default',
      rules: [
        {
          id: 'r1',
          channel: 'telegram',
          userFilter: [],
          channelIdFilter: [],
          agentId: 'tg-agent',
          label: 'TG',
          enabled: true,
        },
        {
          id: 'r2',
          channel: 'discord',
          userFilter: [],
          channelIdFilter: [],
          agentId: 'dc-agent',
          label: 'DC',
          enabled: true,
        },
      ],
    };
    const result = resolveRoute(config, 'telegram', 'user1');
    expect(result.agentId).toBe('tg-agent');
    expect(result.matchedRuleId).toBe('r1');
  });

  it('skips disabled rules', () => {
    const config: RoutingConfig = {
      defaultAgentId: 'default',
      rules: [
        {
          id: 'r1',
          channel: 'telegram',
          userFilter: [],
          channelIdFilter: [],
          agentId: 'tg-agent',
          label: 'TG',
          enabled: false,
        },
      ],
    };
    const result = resolveRoute(config, 'telegram', 'user1');
    expect(result.agentId).toBe('default');
  });

  it('filters by user', () => {
    const config: RoutingConfig = {
      defaultAgentId: 'default',
      rules: [
        {
          id: 'r1',
          channel: 'telegram',
          userFilter: ['vip'],
          channelIdFilter: [],
          agentId: 'vip-agent',
          label: 'VIP',
          enabled: true,
        },
      ],
    };
    expect(resolveRoute(config, 'telegram', 'vip').agentId).toBe('vip-agent');
    expect(resolveRoute(config, 'telegram', 'normie').agentId).toBe('default');
  });

  it('wildcard matches all channels', () => {
    const config: RoutingConfig = {
      defaultAgentId: 'default',
      rules: [
        {
          id: 'r1',
          channel: '*',
          userFilter: [],
          channelIdFilter: [],
          agentId: 'catch-all',
          label: 'All',
          enabled: true,
        },
      ],
    };
    expect(resolveRoute(config, 'discord', 'user1').agentId).toBe('catch-all');
  });
});

// ── createRule ──────────────────────────────────────────────────────────

describe('createRule', () => {
  it('creates rule with unique ID', () => {
    const r = createRule('telegram', 'agent-1', 'My rule');
    expect(r.id).toMatch(/^route_/);
    expect(r.channel).toBe('telegram');
    expect(r.agentId).toBe('agent-1');
    expect(r.enabled).toBe(true);
  });

  it('accepts optional filters', () => {
    const r = createRule('discord', 'agent-2', 'VIP', {
      userFilter: ['user1'],
      enabled: false,
    });
    expect(r.userFilter).toEqual(['user1']);
    expect(r.enabled).toBe(false);
  });
});

// ── validateRoutingConfig ──────────────────────────────────────────────

describe('validateRoutingConfig', () => {
  it('returns no issues for empty rules', () => {
    expect(validateRoutingConfig(DEFAULT_ROUTING_CONFIG)).toEqual([]);
  });

  it('detects unreachable rules after wildcard', () => {
    const config: RoutingConfig = {
      defaultAgentId: 'default',
      rules: [
        {
          id: 'r1',
          channel: '*',
          userFilter: [],
          channelIdFilter: [],
          agentId: 'a',
          label: 'All',
          enabled: true,
        },
        {
          id: 'r2',
          channel: 'telegram',
          userFilter: [],
          channelIdFilter: [],
          agentId: 'b',
          label: 'TG',
          enabled: true,
        },
      ],
    };
    const issues = validateRoutingConfig(config);
    expect(issues.length).toBeGreaterThan(0);
    expect(issues[0]).toContain('unreachable');
  });
});

// ── describeRoutingConfig ──────────────────────────────────────────────

describe('describeRoutingConfig', () => {
  it('returns simple description for no rules', () => {
    expect(describeRoutingConfig(DEFAULT_ROUTING_CONFIG)).toContain('All channels');
  });

  it('counts active rules', () => {
    const config: RoutingConfig = {
      defaultAgentId: 'default',
      rules: [
        {
          id: 'r1',
          channel: 'telegram',
          userFilter: [],
          channelIdFilter: [],
          agentId: 'a',
          label: 'TG',
          enabled: true,
        },
      ],
    };
    expect(describeRoutingConfig(config)).toContain('1 routing rules');
  });
});

// ── ALL_CHANNELS ───────────────────────────────────────────────────────

describe('ALL_CHANNELS', () => {
  it('contains expected channels', () => {
    expect(ALL_CHANNELS).toContain('telegram');
    expect(ALL_CHANNELS).toContain('discord');
    expect(ALL_CHANNELS).toContain('webchat');
  });
});
