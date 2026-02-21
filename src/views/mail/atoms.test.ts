import { describe, it, expect } from 'vitest';
import {
  extractContent,
  formatMailDate,
  getAvatarClass,
  getInitials,
  EMAIL_PROVIDERS,
} from './atoms';

// ── extractContent ─────────────────────────────────────────────────────

describe('extractContent', () => {
  it('returns string content as-is', () => {
    expect(extractContent('hello')).toBe('hello');
  });

  it('joins array of objects with text', () => {
    expect(extractContent([{ text: 'a' }, { text: 'b' }])).toBe('ab');
  });

  it('handles empty array', () => {
    expect(extractContent([])).toBe('');
  });

  it('returns empty string for non-string, non-array', () => {
    expect(extractContent(42)).toBe('');
  });
});

// ── formatMailDate ─────────────────────────────────────────────────────

describe('formatMailDate', () => {
  it('returns time for today', () => {
    const now = new Date();
    const result = formatMailDate(now);
    // Should be a time string like "10:30 AM"
    expect(result).toBeTruthy();
  });

  it('handles old dates', () => {
    const old = new Date('2020-01-15T10:00:00');
    const result = formatMailDate(old);
    expect(result).toContain('Jan');
  });
});

// ── getAvatarClass ─────────────────────────────────────────────────────

describe('getAvatarClass', () => {
  it('returns google class for google senders', () => {
    expect(getAvatarClass('Google Team <noreply@google.com>')).toBe('avatar-google');
  });

  it('returns microsoft class for microsoft senders', () => {
    expect(getAvatarClass('Microsoft Support')).toBe('avatar-microsoft');
  });

  it('returns hash-based color for other senders', () => {
    const cls = getAvatarClass('John Doe');
    expect(['', 'avatar-green', 'avatar-purple', 'avatar-orange', 'avatar-pink']).toContain(cls);
  });
});

// ── getInitials ────────────────────────────────────────────────────────

describe('getInitials', () => {
  it('returns two initials for full name', () => {
    expect(getInitials('John Doe')).toBe('JD');
  });

  it('handles email-style sender', () => {
    expect(getInitials('john@example.com')).toBe('JE');
  });

  it('returns first two chars for single word', () => {
    expect(getInitials('Admin')).toBe('AD');
  });

  it('returns ? for empty string', () => {
    expect(getInitials('')).toBe('?');
  });

  it('strips angle bracket addresses', () => {
    expect(getInitials('John Doe <john@example.com>')).toBe('JD');
  });
});

// ── EMAIL_PROVIDERS ────────────────────────────────────────────────────

describe('EMAIL_PROVIDERS', () => {
  it('contains gmail with correct imap', () => {
    expect(EMAIL_PROVIDERS.gmail.imap).toBe('imap.gmail.com');
  });

  it('contains all expected providers', () => {
    expect(Object.keys(EMAIL_PROVIDERS)).toEqual(
      expect.arrayContaining(['gmail', 'outlook', 'yahoo', 'icloud', 'fastmail', 'custom']),
    );
  });
});
