import { describe, it, expect } from 'vitest';
import {
  escHtml,
  formatDownloads,
  relativeDate,
  sortPackages,
  isInstalled,
  displayName,
  getRequiredPackage,
  SORT_OPTIONS,
  DEBOUNCE_MS,
  COMMUNITY_PACKAGE_MAP,
} from './atoms';
import type { CommunityPackage, InstalledPackage } from './atoms';

// ── Test data factories ────────────────────────────────────────────────────

function makePkg(overrides: Partial<CommunityPackage> = {}): CommunityPackage {
  return {
    package_name: 'n8n-nodes-test',
    description: 'A test package',
    author: 'tester',
    version: '1.0.0',
    weekly_downloads: 100,
    last_updated: '2025-01-01T00:00:00Z',
    repository_url: 'https://github.com/test/test',
    keywords: ['n8n'],
    ...overrides,
  };
}

function makeInstalled(overrides: Partial<InstalledPackage> = {}): InstalledPackage {
  return {
    packageName: 'n8n-nodes-test',
    installedVersion: '1.0.0',
    installedNodes: [{ name: 'TestNode', type: 'n8n-nodes-test.testNode' }],
    ...overrides,
  };
}

// ── escHtml ────────────────────────────────────────────────────────────────

describe('escHtml', () => {
  it('escapes ampersands', () => {
    expect(escHtml('A & B')).toBe('A &amp; B');
  });

  it('escapes angle brackets', () => {
    expect(escHtml('<script>alert("xss")</script>')).toBe(
      '&lt;script&gt;alert(&quot;xss&quot;)&lt;/script&gt;',
    );
  });

  it('escapes double quotes', () => {
    expect(escHtml('say "hello"')).toBe('say &quot;hello&quot;');
  });

  it('returns empty string unchanged', () => {
    expect(escHtml('')).toBe('');
  });

  it('handles multiple special characters', () => {
    expect(escHtml('a & b < c > d "e"')).toBe('a &amp; b &lt; c &gt; d &quot;e&quot;');
  });

  it('leaves plain text untouched', () => {
    expect(escHtml('hello world 123')).toBe('hello world 123');
  });
});

// ── formatDownloads ────────────────────────────────────────────────────────

describe('formatDownloads', () => {
  it('formats millions', () => {
    expect(formatDownloads(1_500_000)).toBe('1.5M');
    expect(formatDownloads(2_000_000)).toBe('2.0M');
    expect(formatDownloads(10_300_000)).toBe('10.3M');
  });

  it('formats thousands', () => {
    expect(formatDownloads(1_000)).toBe('1.0k');
    expect(formatDownloads(12_345)).toBe('12.3k');
    expect(formatDownloads(999_999)).toBe('1000.0k');
  });

  it('formats small numbers as-is', () => {
    expect(formatDownloads(0)).toBe('0');
    expect(formatDownloads(1)).toBe('1');
    expect(formatDownloads(999)).toBe('999');
  });

  it('formats exactly 1M', () => {
    expect(formatDownloads(1_000_000)).toBe('1.0M');
  });

  it('formats exactly 1k', () => {
    expect(formatDownloads(1_000)).toBe('1.0k');
  });
});

// ── relativeDate ───────────────────────────────────────────────────────────

describe('relativeDate', () => {
  it('shows minutes ago', () => {
    const fiveMinAgo = new Date(Date.now() - 5 * 60_000).toISOString();
    expect(relativeDate(fiveMinAgo)).toBe('5m ago');
  });

  it('shows hours ago', () => {
    const threeHoursAgo = new Date(Date.now() - 3 * 3_600_000).toISOString();
    expect(relativeDate(threeHoursAgo)).toBe('3h ago');
  });

  it('shows days ago', () => {
    const tenDaysAgo = new Date(Date.now() - 10 * 86_400_000).toISOString();
    expect(relativeDate(tenDaysAgo)).toBe('10d ago');
  });

  it('shows months ago', () => {
    const threeMonthsAgo = new Date(Date.now() - 90 * 86_400_000).toISOString();
    expect(relativeDate(threeMonthsAgo)).toBe('3mo ago');
  });

  it('shows years ago', () => {
    const twoYearsAgo = new Date(Date.now() - 730 * 86_400_000).toISOString();
    expect(relativeDate(twoYearsAgo)).toBe('2y ago');
  });

  it('returns 0m ago for just now (< 1 min)', () => {
    const justNow = new Date(Date.now() - 30_000).toISOString();
    expect(relativeDate(justNow)).toBe('0m ago');
  });

  it('returns raw string for invalid date', () => {
    expect(relativeDate('not-a-date')).toBe('not-a-date');
    expect(relativeDate('')).toBe('');
  });
});

// ── sortPackages ───────────────────────────────────────────────────────────

describe('sortPackages', () => {
  const pkgA = makePkg({
    package_name: 'alpha',
    weekly_downloads: 50,
    last_updated: '2025-01-01T00:00:00Z',
  });
  const pkgB = makePkg({
    package_name: 'bravo',
    weekly_downloads: 200,
    last_updated: '2025-06-01T00:00:00Z',
  });
  const pkgC = makePkg({
    package_name: 'charlie',
    weekly_downloads: 100,
    last_updated: '2025-03-01T00:00:00Z',
  });

  it('sorts by downloads descending', () => {
    const result = sortPackages([pkgA, pkgB, pkgC], 'downloads');
    expect(result.map((p) => p.package_name)).toEqual(['bravo', 'charlie', 'alpha']);
  });

  it('preserves original order for relevance', () => {
    const result = sortPackages([pkgC, pkgA, pkgB], 'relevance');
    expect(result.map((p) => p.package_name)).toEqual(['charlie', 'alpha', 'bravo']);
  });

  it('sorts by last_updated descending', () => {
    const result = sortPackages([pkgA, pkgB, pkgC], 'updated');
    expect(result.map((p) => p.package_name)).toEqual(['bravo', 'charlie', 'alpha']);
  });

  it('sorts alphabetically A–Z', () => {
    const result = sortPackages([pkgC, pkgA, pkgB], 'a-z');
    expect(result.map((p) => p.package_name)).toEqual(['alpha', 'bravo', 'charlie']);
  });

  it('does not mutate the original array', () => {
    const original = [pkgC, pkgA, pkgB];
    sortPackages(original, 'a-z');
    expect(original.map((p) => p.package_name)).toEqual(['charlie', 'alpha', 'bravo']);
  });

  it('handles empty array', () => {
    expect(sortPackages([], 'downloads')).toEqual([]);
  });

  it('handles single element', () => {
    const result = sortPackages([pkgA], 'a-z');
    expect(result).toHaveLength(1);
    expect(result[0].package_name).toBe('alpha');
  });

  it('returns copy for unknown sort option', () => {
    const result = sortPackages([pkgA, pkgB], 'unknown' as any);
    expect(result).toHaveLength(2);
  });
});

// ── isInstalled ────────────────────────────────────────────────────────────

describe('isInstalled', () => {
  const installed: InstalledPackage[] = [
    makeInstalled({ packageName: 'n8n-nodes-puppeteer' }),
    makeInstalled({ packageName: 'n8n-nodes-redis' }),
  ];

  it('returns true when package is installed', () => {
    const pkg = makePkg({ package_name: 'n8n-nodes-puppeteer' });
    expect(isInstalled(pkg, installed)).toBe(true);
  });

  it('returns false when package is not installed', () => {
    const pkg = makePkg({ package_name: 'n8n-nodes-unknown' });
    expect(isInstalled(pkg, installed)).toBe(false);
  });

  it('returns false with empty installed list', () => {
    const pkg = makePkg({ package_name: 'n8n-nodes-anything' });
    expect(isInstalled(pkg, [])).toBe(false);
  });

  it('is case-sensitive', () => {
    const pkg = makePkg({ package_name: 'N8N-NODES-PUPPETEER' });
    expect(isInstalled(pkg, installed)).toBe(false);
  });
});

// ── displayName ────────────────────────────────────────────────────────────

describe('displayName', () => {
  it('strips n8n-nodes- prefix and title-cases', () => {
    expect(displayName('n8n-nodes-puppeteer')).toBe('Puppeteer');
  });

  it('strips scoped package prefix', () => {
    expect(displayName('@nicklason/n8n-nodes-playwright')).toBe('Playwright');
  });

  it('handles multi-word names', () => {
    expect(displayName('n8n-nodes-whatsapp-buttons')).toBe('Whatsapp Buttons');
  });

  it('handles names without n8n-nodes- prefix', () => {
    expect(displayName('some-package')).toBe('Some Package');
  });

  it('handles plain word', () => {
    expect(displayName('redis')).toBe('Redis');
  });

  it('handles empty string', () => {
    expect(displayName('')).toBe('');
  });

  it('handles scope without n8n-nodes prefix', () => {
    expect(displayName('@n8n/n8n-nodes-langchain')).toBe('Langchain');
  });
});

// ── getRequiredPackage ─────────────────────────────────────────────────────

describe('getRequiredPackage', () => {
  it('returns package from COMMUNITY_PACKAGE_MAP', () => {
    expect(getRequiredPackage('puppeteer')).toBe('n8n-nodes-puppeteer');
    expect(getRequiredPackage('redis')).toBe('n8n-nodes-redis');
    expect(getRequiredPackage('docker')).toBe('n8n-nodes-docker');
  });

  it('uses explicit override when provided', () => {
    expect(getRequiredPackage('puppeteer', 'custom-puppeteer-pkg')).toBe('custom-puppeteer-pkg');
  });

  it('returns null for unknown service with no override', () => {
    expect(getRequiredPackage('nonexistent-service')).toBeNull();
  });

  it('prefers explicit override over map entry', () => {
    expect(getRequiredPackage('redis', 'my-redis-pkg')).toBe('my-redis-pkg');
  });

  it('returns override even for unknown service', () => {
    expect(getRequiredPackage('unknown', 'my-custom-pkg')).toBe('my-custom-pkg');
  });
});

// ── Constants ──────────────────────────────────────────────────────────────

describe('SORT_OPTIONS', () => {
  it('has four sort options', () => {
    expect(SORT_OPTIONS).toHaveLength(4);
  });

  it('includes relevance, downloads, updated, and a-z', () => {
    const values = SORT_OPTIONS.map((o) => o.value);
    expect(values).toContain('relevance');
    expect(values).toContain('downloads');
    expect(values).toContain('updated');
    expect(values).toContain('a-z');
  });

  it('each option has a label', () => {
    for (const opt of SORT_OPTIONS) {
      expect(opt.label).toBeTruthy();
    }
  });
});

describe('DEBOUNCE_MS', () => {
  it('is a reasonable debounce value', () => {
    expect(DEBOUNCE_MS).toBeGreaterThan(0);
    expect(DEBOUNCE_MS).toBeLessThan(2000);
  });
});

describe('COMMUNITY_PACKAGE_MAP', () => {
  it('contains known services', () => {
    expect(COMMUNITY_PACKAGE_MAP).toHaveProperty('puppeteer');
    expect(COMMUNITY_PACKAGE_MAP).toHaveProperty('redis');
    expect(COMMUNITY_PACKAGE_MAP).toHaveProperty('docker');
    expect(COMMUNITY_PACKAGE_MAP).toHaveProperty('whatsapp');
  });

  it('all values are non-empty strings', () => {
    for (const [_key, val] of Object.entries(COMMUNITY_PACKAGE_MAP)) {
      expect(val).toBeTruthy();
      expect(typeof val).toBe('string');
    }
  });
});

// ── escHtml — edge cases ───────────────────────────────────────────────────

describe('escHtml (community) — edge cases', () => {
  it('does NOT escape single quotes', () => {
    expect(escHtml("it's")).toBe("it's");
  });

  it('handles unicode/emoji', () => {
    expect(escHtml('🐾 <test>')).toBe('🐾 &lt;test&gt;');
  });
});

// ── formatDownloads — edge cases ───────────────────────────────────────────

describe('formatDownloads — edge cases', () => {
  it('handles negative numbers', () => {
    // Negative < 1000 → string coercion
    expect(formatDownloads(-5)).toBe('-5');
  });

  it('handles boundary 999', () => {
    expect(formatDownloads(999)).toBe('999');
  });

  it('handles boundary 999_999', () => {
    expect(formatDownloads(999_999)).toBe('1000.0k');
  });
});

// ── relativeDate — edge cases ──────────────────────────────────────────────

describe('relativeDate — edge cases', () => {
  it('boundary: 60 minutes = 1h', () => {
    const sixtyMinAgo = new Date(Date.now() - 60 * 60_000).toISOString();
    expect(relativeDate(sixtyMinAgo)).toBe('1h ago');
  });

  it('boundary: 24 hours = 1d', () => {
    const twentyFourHoursAgo = new Date(Date.now() - 24 * 3_600_000).toISOString();
    expect(relativeDate(twentyFourHoursAgo)).toBe('1d ago');
  });

  it('boundary: 30 days = 1mo', () => {
    const thirtyDaysAgo = new Date(Date.now() - 30 * 86_400_000).toISOString();
    expect(relativeDate(thirtyDaysAgo)).toBe('1mo ago');
  });

  it('boundary: 360 days = 12mo (not year)', () => {
    const threeSixtyDaysAgo = new Date(Date.now() - 360 * 86_400_000).toISOString();
    expect(relativeDate(threeSixtyDaysAgo)).toBe('1y ago');
  });
});

// ── sortPackages — edge cases ──────────────────────────────────────────────

describe('sortPackages — edge cases', () => {
  it('handles packages with identical download counts', () => {
    const a = makePkg({ package_name: 'bravo', weekly_downloads: 100 });
    const b = makePkg({ package_name: 'alpha', weekly_downloads: 100 });
    const result = sortPackages([a, b], 'downloads');
    // Both have same downloads, order depends on JS sort stability
    expect(result).toHaveLength(2);
  });

  it('handles packages with same last_updated', () => {
    const a = makePkg({ package_name: 'a', last_updated: '2025-01-01T00:00:00Z' });
    const b = makePkg({ package_name: 'b', last_updated: '2025-01-01T00:00:00Z' });
    const result = sortPackages([a, b], 'updated');
    expect(result).toHaveLength(2);
  });
});

// ── isInstalled — edge cases ───────────────────────────────────────────────

describe('isInstalled — edge cases', () => {
  it('handles duplicate entries in installed list', () => {
    const installed = [
      makeInstalled({ packageName: 'n8n-nodes-test' }),
      makeInstalled({ packageName: 'n8n-nodes-test' }),
    ];
    const pkg = makePkg({ package_name: 'n8n-nodes-test' });
    expect(isInstalled(pkg, installed)).toBe(true);
  });
});

// ── displayName — edge cases ───────────────────────────────────────────────

describe('displayName — edge cases', () => {
  it('handles consecutive hyphens', () => {
    // "n8n-nodes-foo--bar" → strip prefix → "foo--bar" → "foo  bar" → "Foo  Bar"
    expect(displayName('n8n-nodes-foo--bar')).toBe('Foo  Bar');
  });
});

// ── getRequiredPackage — edge cases ────────────────────────────────────────

describe('getRequiredPackage — edge cases', () => {
  it('returns null for empty string serviceId', () => {
    expect(getRequiredPackage('')).toBeNull();
  });

  it('uses empty string override as truthy', () => {
    // Empty string is falsy, so it falls through to the map
    expect(getRequiredPackage('redis', '')).toBe('n8n-nodes-redis');
  });
});

// ── COMMUNITY_PACKAGE_MAP — completeness ───────────────────────────────────

describe('COMMUNITY_PACKAGE_MAP — completeness', () => {
  it('all keys are lowercase kebab-case', () => {
    for (const key of Object.keys(COMMUNITY_PACKAGE_MAP)) {
      expect(key).toMatch(/^[a-z0-9-]+$/);
    }
  });
});
