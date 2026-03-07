import { describe, it, expect } from 'vitest';
import {
  statusIcon,
  statusColor,
  statusLabel,
  computeHealthSummary,
  deriveHealthStatus,
  daysUntilExpiry,
  generateSuggestions,
} from './atoms';
import type { ServiceHealth } from './atoms';

// ── statusIcon ─────────────────────────────────────────────────────────

describe('statusIcon', () => {
  it('returns check_circle for healthy', () => {
    expect(statusIcon('healthy')).toBe('check_circle');
  });

  it('returns warning for degraded', () => {
    expect(statusIcon('degraded')).toBe('warning');
  });

  it('returns error for error', () => {
    expect(statusIcon('error')).toBe('error');
  });

  it('returns lock_clock for expired', () => {
    expect(statusIcon('expired')).toBe('lock_clock');
  });

  it('returns help for unknown', () => {
    expect(statusIcon('unknown')).toBe('help');
  });
});

// ── statusColor ────────────────────────────────────────────────────────

describe('statusColor', () => {
  it('returns success color for healthy', () => {
    expect(statusColor('healthy')).toContain('#22c55e');
  });

  it('returns warning color for degraded', () => {
    expect(statusColor('degraded')).toContain('#f59e0b');
  });

  it('returns danger color for error', () => {
    expect(statusColor('error')).toContain('#ef4444');
  });

  it('returns danger color for expired', () => {
    expect(statusColor('expired')).toContain('#ef4444');
  });

  it('returns secondary for unknown', () => {
    expect(statusColor('unknown')).toContain('text-secondary');
  });
});

// ── statusLabel ────────────────────────────────────────────────────────

describe('statusLabel', () => {
  it('returns Connected for healthy', () => {
    expect(statusLabel('healthy')).toBe('Connected');
  });

  it('returns Degraded for degraded', () => {
    expect(statusLabel('degraded')).toBe('Degraded');
  });

  it('returns Error for error', () => {
    expect(statusLabel('error')).toBe('Error');
  });

  it('returns Token Expired for expired', () => {
    expect(statusLabel('expired')).toBe('Token Expired');
  });

  it('returns Unknown for unknown', () => {
    expect(statusLabel('unknown')).toBe('Unknown');
  });
});

// ── computeHealthSummary ───────────────────────────────────────────────

describe('computeHealthSummary', () => {
  const base: ServiceHealth = {
    service: 'test',
    serviceName: 'Test',
    icon: 'test',
    status: 'healthy',
    lastChecked: new Date().toISOString(),
    recentFailures: 0,
    todayActions: 5,
  };

  it('counts all healthy', () => {
    const services = [
      { ...base, service: 'a' },
      { ...base, service: 'b' },
    ];
    const summary = computeHealthSummary(services);
    expect(summary.total).toBe(2);
    expect(summary.healthy).toBe(2);
    expect(summary.degraded).toBe(0);
    expect(summary.error).toBe(0);
    expect(summary.expired).toBe(0);
    expect(summary.needsAttention).toHaveLength(0);
  });

  it('counts mixed statuses', () => {
    const services: ServiceHealth[] = [
      { ...base, service: 'a', status: 'healthy' },
      { ...base, service: 'b', status: 'degraded' },
      { ...base, service: 'c', status: 'error' },
      { ...base, service: 'd', status: 'expired' },
      { ...base, service: 'e', status: 'unknown' },
    ];
    const summary = computeHealthSummary(services);
    expect(summary.total).toBe(5);
    expect(summary.healthy).toBe(1);
    expect(summary.degraded).toBe(1);
    expect(summary.error).toBe(1);
    expect(summary.expired).toBe(1);
  });

  it('adds degraded/error/expired to needsAttention', () => {
    const services: ServiceHealth[] = [
      { ...base, service: 'a', status: 'healthy' },
      { ...base, service: 'b', status: 'degraded' },
      { ...base, service: 'c', status: 'error' },
    ];
    const summary = computeHealthSummary(services);
    expect(summary.needsAttention).toHaveLength(2);
    expect(summary.needsAttention.map((s) => s.service)).toContain('b');
    expect(summary.needsAttention.map((s) => s.service)).toContain('c');
  });

  it('handles empty array', () => {
    const summary = computeHealthSummary([]);
    expect(summary.total).toBe(0);
    expect(summary.healthy).toBe(0);
    expect(summary.needsAttention).toHaveLength(0);
  });
});

// ── deriveHealthStatus ─────────────────────────────────────────────────

describe('deriveHealthStatus', () => {
  it('returns unknown when no credentials', () => {
    expect(deriveHealthStatus(undefined, 0, false)).toBe('unknown');
  });

  it('returns expired when token is past', () => {
    const past = new Date(Date.now() - 86_400_000).toISOString();
    expect(deriveHealthStatus(past, 0, true)).toBe('expired');
  });

  it('returns degraded when token expires within 7 days', () => {
    const soon = new Date(Date.now() + 3 * 86_400_000).toISOString(); // 3 days
    expect(deriveHealthStatus(soon, 0, true)).toBe('degraded');
  });

  it('returns healthy when token is far from expiry and no failures', () => {
    const far = new Date(Date.now() + 30 * 86_400_000).toISOString(); // 30 days
    expect(deriveHealthStatus(far, 0, true)).toBe('healthy');
  });

  it('returns error when 3+ recent failures', () => {
    expect(deriveHealthStatus(undefined, 3, true)).toBe('error');
  });

  it('returns degraded when 1-2 recent failures', () => {
    expect(deriveHealthStatus(undefined, 1, true)).toBe('degraded');
    expect(deriveHealthStatus(undefined, 2, true)).toBe('degraded');
  });

  it('returns healthy when no issues', () => {
    expect(deriveHealthStatus(undefined, 0, true)).toBe('healthy');
  });

  it('token expiry takes priority over failures', () => {
    const past = new Date(Date.now() - 86_400_000).toISOString();
    expect(deriveHealthStatus(past, 0, true)).toBe('expired');
  });
});

// ── daysUntilExpiry ────────────────────────────────────────────────────

describe('daysUntilExpiry', () => {
  it('returns positive days for future expiry', () => {
    const future = new Date(Date.now() + 10 * 86_400_000).toISOString();
    const days = daysUntilExpiry(future);
    expect(days).toBeGreaterThanOrEqual(9);
    expect(days).toBeLessThanOrEqual(10);
  });

  it('returns negative days for past expiry', () => {
    const past = new Date(Date.now() - 5 * 86_400_000).toISOString();
    const days = daysUntilExpiry(past);
    expect(days).toBeLessThan(0);
  });

  it('returns 0 for today', () => {
    const today = new Date(Date.now() + 1000).toISOString(); // 1 second ahead
    expect(daysUntilExpiry(today)).toBe(0);
  });
});

// ── generateSuggestions ────────────────────────────────────────────────

describe('generateSuggestions', () => {
  it('returns gmail suggestion when connected', () => {
    const suggestions = generateSuggestions(['gmail']);
    expect(suggestions).toHaveLength(1);
    expect(suggestions[0].service).toBe('gmail');
    expect(suggestions[0].id).toBe('suggest-gmail');
  });

  it('returns multiple suggestions for multiple services', () => {
    const suggestions = generateSuggestions(['gmail', 'slack', 'github']);
    expect(suggestions).toHaveLength(3);
  });

  it('caps at 3 suggestions even with more services', () => {
    const suggestions = generateSuggestions([
      'gmail',
      'slack',
      'github',
      'hubspot',
      'trello',
      'jira',
    ]);
    expect(suggestions).toHaveLength(3);
  });

  it('returns empty for unknown services', () => {
    const suggestions = generateSuggestions(['totally_unknown_service']);
    expect(suggestions).toHaveLength(0);
  });

  it('returns empty for no services', () => {
    expect(generateSuggestions([])).toHaveLength(0);
  });

  it('includes correct action labels', () => {
    const suggestions = generateSuggestions(['slack']);
    expect(suggestions[0].actionLabel).toBe('Check Slack');
  });
});

// ── computeHealthSummary — edge cases ──────────────────────────────────

describe('computeHealthSummary — edge cases', () => {
  const base: ServiceHealth = {
    service: 'test',
    serviceName: 'Test',
    icon: 'test',
    status: 'healthy',
    lastChecked: new Date().toISOString(),
    recentFailures: 0,
    todayActions: 5,
  };

  it('all same non-healthy status (all error)', () => {
    const services: ServiceHealth[] = [
      { ...base, service: 'a', status: 'error' },
      { ...base, service: 'b', status: 'error' },
    ];
    const summary = computeHealthSummary(services);
    expect(summary.error).toBe(2);
    expect(summary.healthy).toBe(0);
    expect(summary.needsAttention).toHaveLength(2);
  });

  it('single service', () => {
    const summary = computeHealthSummary([{ ...base, status: 'degraded' }]);
    expect(summary.total).toBe(1);
    expect(summary.degraded).toBe(1);
    expect(summary.needsAttention).toHaveLength(1);
  });

  it('unknown status is NOT added to needsAttention', () => {
    const services: ServiceHealth[] = [{ ...base, status: 'unknown' }];
    const summary = computeHealthSummary(services);
    expect(summary.needsAttention).toHaveLength(0);
    expect(summary.total).toBe(1);
  });

  it('all 5 statuses together', () => {
    const services: ServiceHealth[] = [
      { ...base, service: 'a', status: 'healthy' },
      { ...base, service: 'b', status: 'degraded' },
      { ...base, service: 'c', status: 'error' },
      { ...base, service: 'd', status: 'expired' },
      { ...base, service: 'e', status: 'unknown' },
    ];
    const summary = computeHealthSummary(services);
    expect(summary.healthy).toBe(1);
    expect(summary.degraded).toBe(1);
    expect(summary.error).toBe(1);
    expect(summary.expired).toBe(1);
    expect(summary.needsAttention).toHaveLength(3); // degraded, error, expired
  });
});

// ── deriveHealthStatus — boundary cases ────────────────────────────────

describe('deriveHealthStatus — boundary cases', () => {
  it('token expiry exactly 7 days out → degraded (<=7)', () => {
    const exactly7 = new Date(Date.now() + 7 * 86_400_000).toISOString();
    expect(deriveHealthStatus(exactly7, 0, true)).toBe('degraded');
  });

  it('token expiry exactly 8 days out → healthy', () => {
    const exactly8 = new Date(Date.now() + 8 * 86_400_000 + 1000).toISOString();
    expect(deriveHealthStatus(exactly8, 0, true)).toBe('healthy');
  });

  it('exactly 3 failures → error', () => {
    expect(deriveHealthStatus(undefined, 3, true)).toBe('error');
  });

  it('token soon-to-expire AND failures >= 3 → token check wins (degraded)', () => {
    const soon = new Date(Date.now() + 3 * 86_400_000).toISOString();
    expect(deriveHealthStatus(soon, 5, true)).toBe('degraded');
  });
});

// ── daysUntilExpiry — edge cases ───────────────────────────────────────

describe('daysUntilExpiry — edge cases', () => {
  it('returns NaN for invalid date string', () => {
    expect(isNaN(daysUntilExpiry('not-a-date'))).toBe(true);
  });

  it('very far future', () => {
    const far = new Date(Date.now() + 365 * 10 * 86_400_000).toISOString();
    expect(daysUntilExpiry(far)).toBeGreaterThan(3000);
  });

  it('very far past', () => {
    const past = new Date(Date.now() - 365 * 10 * 86_400_000).toISOString();
    expect(daysUntilExpiry(past)).toBeLessThan(-3000);
  });
});

// ── generateSuggestions — edge cases ───────────────────────────────────

describe('generateSuggestions — edge cases', () => {
  it('duplicate services produce duplicate suggestions', () => {
    const suggestions = generateSuggestions(['gmail', 'gmail']);
    expect(suggestions).toHaveLength(2);
  });

  it('suggestion objects are copies (not references to template)', () => {
    const s1 = generateSuggestions(['gmail']);
    const s2 = generateSuggestions(['gmail']);
    expect(s1[0]).not.toBe(s2[0]);
    expect(s1[0]).toEqual(s2[0]);
  });

  it('order matches input order', () => {
    const suggestions = generateSuggestions(['github', 'gmail', 'slack']);
    expect(suggestions[0].service).toBe('github');
    expect(suggestions[1].service).toBe('gmail');
    expect(suggestions[2].service).toBe('slack');
  });

  it('all 6 template services produce correct suggestions individually', () => {
    for (const svc of ['gmail', 'slack', 'github', 'hubspot', 'trello', 'jira']) {
      const suggestions = generateSuggestions([svc]);
      expect(suggestions).toHaveLength(1);
      expect(suggestions[0].service).toBe(svc);
      expect(suggestions[0].id).toBe(`suggest-${svc}`);
    }
  });
});
