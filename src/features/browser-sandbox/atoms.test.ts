import { describe, it, expect } from 'vitest';
import {
  formatBytes,
  isValidDomain,
  extractDomain,
  timeAgo,
  DEFAULT_BROWSER_CONFIG,
  DEFAULT_ALLOWED_DOMAINS,
  DEFAULT_BLOCKED_DOMAINS,
} from './atoms';

// ── formatBytes ────────────────────────────────────────────────────────

describe('formatBytes', () => {
  it('formats bytes', () => {
    expect(formatBytes(500)).toBe('500 B');
  });

  it('formats KB', () => {
    expect(formatBytes(2048)).toBe('2.0 KB');
  });

  it('formats MB', () => {
    expect(formatBytes(5 * 1024 * 1024)).toBe('5.0 MB');
  });

  it('formats GB', () => {
    expect(formatBytes(1.5 * 1024 * 1024 * 1024)).toBe('1.50 GB');
  });
});

// ── isValidDomain ──────────────────────────────────────────────────────

describe('isValidDomain', () => {
  it('validates normal domain', () => {
    expect(isValidDomain('example.com')).toBe(true);
  });

  it('validates wildcard domain', () => {
    expect(isValidDomain('*.example.com')).toBe(true);
  });

  it('rejects empty string', () => {
    expect(isValidDomain('')).toBe(false);
  });

  it('rejects too-long domain', () => {
    expect(isValidDomain('a'.repeat(254))).toBe(false);
  });

  it('validates subdomain', () => {
    expect(isValidDomain('api.sub.example.com')).toBe(true);
  });
});

// ── extractDomain ──────────────────────────────────────────────────────

describe('extractDomain', () => {
  it('extracts domain from URL', () => {
    expect(extractDomain('https://api.example.com/path')).toBe('api.example.com');
  });

  it('strips port', () => {
    expect(extractDomain('http://localhost:3000/')).toBe('localhost');
  });

  it('handles plain domain', () => {
    expect(extractDomain('example.com')).toBe('example.com');
  });
});

// ── timeAgo ────────────────────────────────────────────────────────────

describe('timeAgo', () => {
  it('returns "just now" for recent', () => {
    expect(timeAgo(new Date().toISOString())).toBe('just now');
  });

  it('returns minutes ago', () => {
    const d = new Date(Date.now() - 10 * 60 * 1000);
    expect(timeAgo(d.toISOString())).toBe('10m ago');
  });

  it('returns hours ago', () => {
    const d = new Date(Date.now() - 3 * 60 * 60 * 1000);
    expect(timeAgo(d.toISOString())).toBe('3h ago');
  });

  it('returns days ago', () => {
    const d = new Date(Date.now() - 5 * 24 * 60 * 60 * 1000);
    expect(timeAgo(d.toISOString())).toBe('5d ago');
  });
});

// ── Constants ──────────────────────────────────────────────────────────

describe('DEFAULT_BROWSER_CONFIG', () => {
  it('has default profile', () => {
    expect(DEFAULT_BROWSER_CONFIG.default_profile).toBe('default');
    expect(DEFAULT_BROWSER_CONFIG.headless).toBe(true);
  });
});

describe('DEFAULT_ALLOWED_DOMAINS', () => {
  it('includes API domains', () => {
    expect(DEFAULT_ALLOWED_DOMAINS).toContain('api.openai.com');
    expect(DEFAULT_ALLOWED_DOMAINS).toContain('api.anthropic.com');
  });
});

describe('DEFAULT_BLOCKED_DOMAINS', () => {
  it('blocks known exfiltration sites', () => {
    expect(DEFAULT_BLOCKED_DOMAINS).toContain('pastebin.com');
    expect(DEFAULT_BLOCKED_DOMAINS).toContain('transfer.sh');
  });
});

// ── Additional edge cases ──────────────────────────────────────────────

describe('formatBytes — edge cases', () => {
  it('formats 0 bytes', () => {
    const result = formatBytes(0);
    expect(result).toContain('0');
  });

  it('formats exact 1 KB boundary', () => {
    const result = formatBytes(1024);
    expect(result).toMatch(/1.*KB/i);
  });

  it('formats exact 1 MB boundary', () => {
    const result = formatBytes(1024 * 1024);
    expect(result).toMatch(/1.*MB/i);
  });

  it('formats exact 1 GB boundary', () => {
    const result = formatBytes(1024 * 1024 * 1024);
    expect(result).toMatch(/1.*GB/i);
  });
});

describe('isValidDomain — edge cases', () => {
  it('rejects domain with spaces', () => {
    expect(isValidDomain('exam ple.com')).toBe(false);
  });

  it('rejects domain with special characters', () => {
    expect(isValidDomain('exam!ple.com')).toBe(false);
  });

  it('accepts localhost as valid', () => {
    expect(isValidDomain('localhost')).toBe(true);
  });

  it('rejects leading hyphen', () => {
    expect(isValidDomain('-example.com')).toBe(false);
  });
});

describe('extractDomain — edge cases', () => {
  it('lowercases the domain', () => {
    expect(extractDomain('https://API.Example.COM/path')).toBe('api.example.com');
  });

  it('strips port from URL', () => {
    expect(extractDomain('http://localhost:3000/api')).toBe('localhost');
  });
});

describe('timeAgo — edge cases', () => {
  it('shows date for 30+ days ago', () => {
    const old = new Date(Date.now() - 45 * 86_400_000).toISOString();
    const result = timeAgo(old);
    // Should be a formatted date string, not relative
    expect(result).toMatch(/\d/);
  });

  it('shows just now for recent timestamp', () => {
    const now = new Date().toISOString();
    const result = timeAgo(now);
    expect(result.toLowerCase()).toMatch(/just now|second|0/);
  });
});

describe('DEFAULT_BROWSER_CONFIG — additional', () => {
  it('has exactly 1 default profile', () => {
    expect(DEFAULT_BROWSER_CONFIG.profiles).toHaveLength(1);
  });

  it('has idle_timeout_secs of 300', () => {
    expect(DEFAULT_BROWSER_CONFIG.idle_timeout_secs).toBe(300);
  });
});
