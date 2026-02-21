import { describe, it, expect } from 'vitest';
import {
  isAvatar,
  spriteAvatar,
  TOOL_GROUPS,
  AGENT_TEMPLATES,
  AVATAR_COLORS,
  SPRITE_AVATARS,
  DEFAULT_AVATAR,
} from './atoms';

// ── isAvatar ───────────────────────────────────────────────────────────

describe('isAvatar', () => {
  it('returns true for numeric strings', () => {
    expect(isAvatar('1')).toBe(true);
    expect(isAvatar('42')).toBe(true);
  });

  it('returns false for emoji strings', () => {
    expect(isAvatar('🐱')).toBe(false);
    expect(isAvatar('hello')).toBe(false);
  });

  it('returns false for empty string', () => {
    expect(isAvatar('')).toBe(false);
  });
});

// ── spriteAvatar ───────────────────────────────────────────────────────

describe('spriteAvatar', () => {
  it('returns img tag for numeric avatar', () => {
    const html = spriteAvatar('5');
    expect(html).toContain('<img');
    expect(html).toContain('/src/assets/avatars/5.png');
  });

  it('respects size parameter', () => {
    const html = spriteAvatar('1', 64);
    expect(html).toContain('width="64"');
    expect(html).toContain('height="64"');
  });

  it('returns span for emoji fallback', () => {
    const html = spriteAvatar('🐱');
    expect(html).toContain('<span');
    expect(html).toContain('🐱');
  });
});

// ── Constants ──────────────────────────────────────────────────────────

describe('TOOL_GROUPS', () => {
  it('has multiple groups', () => {
    expect(TOOL_GROUPS.length).toBeGreaterThan(5);
  });

  it('each group has label, icon, and tools', () => {
    for (const group of TOOL_GROUPS) {
      expect(group.label).toBeTruthy();
      expect(group.icon).toBeTruthy();
      expect(group.tools.length).toBeGreaterThan(0);
    }
  });
});

describe('AGENT_TEMPLATES', () => {
  it('has all template types', () => {
    expect(Object.keys(AGENT_TEMPLATES)).toEqual(
      expect.arrayContaining(['general', 'research', 'creative', 'technical', 'custom']),
    );
  });

  it('each template has personality and skills', () => {
    for (const tmpl of Object.values(AGENT_TEMPLATES)) {
      expect(tmpl.personality).toBeDefined();
      expect(Array.isArray(tmpl.skills)).toBe(true);
    }
  });
});

describe('AVATAR_COLORS', () => {
  it('has 7 colors', () => {
    expect(AVATAR_COLORS).toHaveLength(7);
  });

  it('each color is hex', () => {
    for (const color of AVATAR_COLORS) {
      expect(color).toMatch(/^#[0-9A-Fa-f]{6}$/);
    }
  });
});

describe('SPRITE_AVATARS', () => {
  it('has 50 avatars', () => {
    expect(SPRITE_AVATARS).toHaveLength(50);
  });

  it('each avatar is a numeric string', () => {
    for (const a of SPRITE_AVATARS) {
      expect(isAvatar(a)).toBe(true);
    }
  });
});

describe('DEFAULT_AVATAR', () => {
  it('is "5"', () => {
    expect(DEFAULT_AVATAR).toBe('5');
  });
});
