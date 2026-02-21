import { describe, it, expect } from 'vitest';
import {
  isSlashCommand,
  parseCommand,
  validateCommand,
  getAutocompleteSuggestions,
  buildHelpText,
  getCommandDef,
  COMMANDS,
} from './atoms';

// ── isSlashCommand ─────────────────────────────────────────────────────

describe('isSlashCommand', () => {
  it('returns true for /model arg', () => {
    expect(isSlashCommand('/model gpt-4')).toBe(true);
  });

  it('returns true for /help alone', () => {
    expect(isSlashCommand('/help')).toBe(true);
  });

  it('returns false for regular text', () => {
    expect(isSlashCommand('hello world')).toBe(false);
  });

  it('returns false for URL paths', () => {
    expect(isSlashCommand('https://example.com/path')).toBe(false);
  });
});

// ── parseCommand ───────────────────────────────────────────────────────

describe('parseCommand', () => {
  it('parses command with args', () => {
    const c = parseCommand('/model gpt-4');
    expect(c.name).toBe('model');
    expect(c.args).toBe('gpt-4');
    expect(c.recognized).toBe(true);
  });

  it('parses command without args', () => {
    const c = parseCommand('/help');
    expect(c.name).toBe('help');
    expect(c.args).toBe('');
    expect(c.recognized).toBe(true);
  });

  it('marks unknown commands as unrecognized', () => {
    const c = parseCommand('/foobar something');
    expect(c.name).toBe('foobar');
    expect(c.recognized).toBe(false);
  });

  it('handles non-command input', () => {
    const c = parseCommand('just text');
    expect(c.recognized).toBe(false);
    expect(c.name).toBe('');
  });
});

// ── validateCommand ────────────────────────────────────────────────────

describe('validateCommand', () => {
  it('returns null for valid no-arg command', () => {
    const c = parseCommand('/help');
    expect(validateCommand(c)).toBeNull();
  });

  it('returns error for unknown command', () => {
    const c = parseCommand('/foobar');
    expect(validateCommand(c)).toContain('Unknown command');
  });

  it('returns error for missing required arg', () => {
    const c = parseCommand('/model');
    expect(validateCommand(c)).toContain('Missing argument');
  });

  it('validates /think levels', () => {
    expect(validateCommand(parseCommand('/think high'))).toBeNull();
    expect(validateCommand(parseCommand('/think extreme'))).toContain('Invalid thinking level');
  });

  it('validates /temp range', () => {
    expect(validateCommand(parseCommand('/temp 0.5'))).toBeNull();
    expect(validateCommand(parseCommand('/temp 3.0'))).toContain('between 0.0 and 2.0');
    expect(validateCommand(parseCommand('/temp abc'))).toContain('number');
  });
});

// ── getAutocompleteSuggestions ──────────────────────────────────────────

describe('getAutocompleteSuggestions', () => {
  it('returns all commands for /', () => {
    const suggestions = getAutocompleteSuggestions('/');
    expect(suggestions.length).toBe(COMMANDS.length);
  });

  it('filters by prefix', () => {
    const suggestions = getAutocompleteSuggestions('/mo');
    expect(suggestions.some((s) => s.command === '/model')).toBe(true);
    expect(suggestions.some((s) => s.command === '/mode')).toBe(true);
  });

  it('returns empty for non-slash input', () => {
    expect(getAutocompleteSuggestions('hello')).toEqual([]);
  });
});

// ── buildHelpText ──────────────────────────────────────────────────────

describe('buildHelpText', () => {
  it('contains all command categories', () => {
    const text = buildHelpText();
    expect(text).toContain('Chat');
    expect(text).toContain('Session');
    expect(text).toContain('Memory');
    expect(text).toContain('Tools');
    expect(text).toContain('Config');
  });
});

// ── getCommandDef ──────────────────────────────────────────────────────

describe('getCommandDef', () => {
  it('returns definition for known command', () => {
    const def = getCommandDef('model');
    expect(def).toBeDefined();
    expect(def!.requiresArg).toBe(true);
  });

  it('returns undefined for unknown', () => {
    expect(getCommandDef('nonexistent')).toBeUndefined();
  });

  it('is case-insensitive', () => {
    expect(getCommandDef('MODEL')).toBeDefined();
  });
});
