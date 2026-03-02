import { describe, it, expect } from 'vitest';
import {
  checkToolPolicy,
  filterToolsByPolicy,
  isOverToolCallLimit,
  describePolicySummary,
  DEFAULT_POLICY,
  READONLY_POLICY,
  STANDARD_POLICY,
  POLICY_PRESETS,
  ALL_TOOLS,
  SAFE_TOOLS,
  HIGH_RISK_TOOLS,
} from './atoms';
import type { ToolPolicy } from './atoms';

// ── checkToolPolicy ────────────────────────────────────────────────────

describe('checkToolPolicy', () => {
  it('unrestricted mode allows everything', () => {
    const d = checkToolPolicy('exec', DEFAULT_POLICY);
    expect(d.allowed).toBe(true);
    expect(d.requiresApproval).toBe(false);
  });

  it('allowlist mode allows listed tools', () => {
    const d = checkToolPolicy('read_file', READONLY_POLICY);
    expect(d.allowed).toBe(true);
  });

  it('allowlist mode blocks unlisted tools', () => {
    const d = checkToolPolicy('exec', READONLY_POLICY);
    expect(d.allowed).toBe(false);
  });

  it('allowlist with requireApprovalForUnlisted still allows with approval', () => {
    const policy: ToolPolicy = {
      ...READONLY_POLICY,
      requireApprovalForUnlisted: true,
    };
    const d = checkToolPolicy('exec', policy);
    expect(d.allowed).toBe(true);
    expect(d.requiresApproval).toBe(true);
  });

  it('denylist mode blocks denied tools', () => {
    const policy: ToolPolicy = {
      mode: 'denylist',
      allowed: [],
      denied: ['exec'],
      requireApprovalForUnlisted: false,
      alwaysRequireApproval: [],
    };
    const d = checkToolPolicy('exec', policy);
    expect(d.allowed).toBe(false);
  });

  it('denylist mode allows non-denied tools', () => {
    const policy: ToolPolicy = {
      mode: 'denylist',
      allowed: [],
      denied: ['exec'],
      requireApprovalForUnlisted: false,
      alwaysRequireApproval: [],
    };
    const d = checkToolPolicy('read_file', policy);
    expect(d.allowed).toBe(true);
  });

  it('alwaysRequireApproval overrides mode', () => {
    const policy: ToolPolicy = {
      ...DEFAULT_POLICY,
      alwaysRequireApproval: ['exec'],
    };
    const d = checkToolPolicy('exec', policy);
    expect(d.allowed).toBe(true);
    expect(d.requiresApproval).toBe(true);
  });
});

// ── filterToolsByPolicy ────────────────────────────────────────────────

describe('filterToolsByPolicy', () => {
  it('returns all tools for unrestricted', () => {
    const tools = ['exec', 'read_file', 'write_file'];
    expect(filterToolsByPolicy(tools, DEFAULT_POLICY)).toEqual(tools);
  });

  it('filters to only allowed tools for allowlist', () => {
    const tools = ['exec', 'read_file', 'write_file'];
    const result = filterToolsByPolicy(tools, READONLY_POLICY);
    expect(result).toContain('read_file');
    expect(result).not.toContain('exec');
  });
});

// ── isOverToolCallLimit ────────────────────────────────────────────────

describe('isOverToolCallLimit', () => {
  it('returns false when no limit set', () => {
    expect(isOverToolCallLimit(100, DEFAULT_POLICY)).toBe(false);
  });

  it('returns true when over limit', () => {
    const policy: ToolPolicy = { ...DEFAULT_POLICY, maxToolCallsPerTurn: 5 };
    expect(isOverToolCallLimit(6, policy)).toBe(true);
  });

  it('returns false when within limit', () => {
    const policy: ToolPolicy = { ...DEFAULT_POLICY, maxToolCallsPerTurn: 5 };
    expect(isOverToolCallLimit(3, policy)).toBe(false);
  });
});

// ── describePolicySummary ──────────────────────────────────────────────

describe('describePolicySummary', () => {
  it('describes unrestricted', () => {
    expect(describePolicySummary(DEFAULT_POLICY)).toContain('Unrestricted');
  });

  it('describes allowlist with count', () => {
    expect(describePolicySummary(READONLY_POLICY)).toContain('Allowlist');
    expect(describePolicySummary(READONLY_POLICY)).toMatch(/\d+ tools/);
  });
});

// ── Constants ──────────────────────────────────────────────────────────

describe('Tool constants', () => {
  it('ALL_TOOLS has many tools', () => {
    expect(ALL_TOOLS.length).toBeGreaterThan(30);
  });

  it('SAFE_TOOLS is subset of ALL_TOOLS', () => {
    for (const tool of SAFE_TOOLS) {
      expect(ALL_TOOLS).toContain(tool);
    }
  });

  it('HIGH_RISK_TOOLS is subset of ALL_TOOLS', () => {
    for (const tool of HIGH_RISK_TOOLS) {
      expect(ALL_TOOLS).toContain(tool);
    }
  });

  it('POLICY_PRESETS has 4 presets', () => {
    expect(Object.keys(POLICY_PRESETS)).toHaveLength(4);
  });
});

// ── Additional edge cases ──────────────────────────────────────────────

describe('checkToolPolicy — unknown mode', () => {
  it('defaults to allowed for unknown mode', () => {
    const policy = {
      mode: 'something-new' as any,
      allowed: [],
      denied: [],
      requireApprovalForUnlisted: false,
      alwaysRequireApproval: [],
    };
    const decision = checkToolPolicy('exec', policy);
    expect(decision.allowed).toBe(true);
  });
});

describe('describePolicySummary — additional modes', () => {
  it('describes denylist mode', () => {
    const summary = describePolicySummary(STANDARD_POLICY);
    expect(summary.toLowerCase()).toContain('deny');
  });

  it('describes readonly policy', () => {
    const summary = describePolicySummary(READONLY_POLICY);
    expect(summary.toLowerCase()).toContain('allow');
  });
});

describe('isOverToolCallLimit — boundary', () => {
  it('returns false at exact limit', () => {
    const policy = {
      ...DEFAULT_POLICY,
      maxToolCallsPerTurn: 5,
    };
    expect(isOverToolCallLimit(5, policy)).toBe(false);
  });

  it('returns true at limit + 1', () => {
    const policy = {
      ...DEFAULT_POLICY,
      maxToolCallsPerTurn: 5,
    };
    expect(isOverToolCallLimit(6, policy)).toBe(true);
  });
});

describe('filterToolsByPolicy — denylist', () => {
  it('filters denied tools from list', () => {
    const policy = {
      mode: 'denylist' as const,
      allowed: [],
      denied: ['exec', 'write_file'],
      requireApprovalForUnlisted: false,
      alwaysRequireApproval: [],
    };
    const filtered = filterToolsByPolicy(['exec', 'read_file', 'write_file', 'list_dir'], policy);
    expect(filtered).toContain('read_file');
    expect(filtered).toContain('list_dir');
    expect(filtered).not.toContain('exec');
    expect(filtered).not.toContain('write_file');
  });

  it('returns empty array for empty input', () => {
    expect(filterToolsByPolicy([], DEFAULT_POLICY)).toHaveLength(0);
  });
});

describe('SAFE_TOOLS / HIGH_RISK_TOOLS integrity', () => {
  it('SAFE_TOOLS and HIGH_RISK_TOOLS overlap is limited to skill_search', () => {
    // NOTE: skill_search appears in both — this is a known issue
    const overlap = SAFE_TOOLS.filter((t) => HIGH_RISK_TOOLS.includes(t));
    expect(overlap).toEqual(['skill_search']);
  });

  it('STANDARD_POLICY alwaysRequireApproval includes all HIGH_RISK_TOOLS', () => {
    for (const tool of HIGH_RISK_TOOLS) {
      expect(STANDARD_POLICY.alwaysRequireApproval).toContain(tool);
    }
  });
});
