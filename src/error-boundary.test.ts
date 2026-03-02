import { describe, it, expect, beforeEach } from 'vitest';
import { reportError, getErrorHistory, clearErrorHistory, setErrorHandler } from './error-boundary';

beforeEach(() => {
  clearErrorHistory();
});

describe('reportError', () => {
  it('stores error in history', () => {
    reportError(new Error('test failure'), 'manual');
    const history = getErrorHistory();
    expect(history.length).toBe(1);
    expect(history[0].message).toBe('test failure');
    expect(history[0].source).toBe('manual');
  });

  it('handles string errors', () => {
    reportError('something went wrong', 'network');
    const history = getErrorHistory();
    expect(history[0].message).toBe('something went wrong');
  });

  it('includes context', () => {
    reportError('fail', 'tauri', { command: 'get_config' });
    const history = getErrorHistory();
    expect(history[0].context).toEqual({ command: 'get_config' });
  });

  it('preserves stack for Error objects', () => {
    const err = new Error('with stack');
    reportError(err);
    const history = getErrorHistory();
    expect(history[0].stack).toBeTruthy();
  });
});

describe('getErrorHistory', () => {
  it('returns requested count', () => {
    for (let i = 0; i < 10; i++) reportError(`error ${i}`);
    expect(getErrorHistory(3).length).toBe(3);
  });

  it('returns all if fewer than count', () => {
    reportError('only one');
    expect(getErrorHistory(50).length).toBe(1);
  });
});

describe('clearErrorHistory', () => {
  it('clears all errors', () => {
    reportError('error');
    expect(getErrorHistory().length).toBe(1);
    clearErrorHistory();
    expect(getErrorHistory().length).toBe(0);
  });
});

describe('setErrorHandler', () => {
  it('calls callback when error is reported', () => {
    let called = false;
    setErrorHandler(() => {
      called = true;
    });
    reportError('trigger callback');
    expect(called).toBe(true);
    // Clean up
    setErrorHandler(() => {});
  });
});

// ── Edge cases ─────────────────────────────────────────────────────────

describe('error history overflow', () => {
  it('evicts oldest entry when exceeding MAX_ERROR_HISTORY (100)', () => {
    for (let i = 0; i < 105; i++) {
      reportError(`error-${i}`, 'manual');
    }
    const history = getErrorHistory(200);
    expect(history.length).toBe(100);
    // Oldest (0–4) should have been evicted
    expect(history[0].message).toBe('error-5');
    expect(history[history.length - 1].message).toBe('error-104');
  });
});

describe('reportError with unusual inputs', () => {
  it('handles null error', () => {
    reportError(null, 'manual');
    const history = getErrorHistory();
    expect(history[0].message).toBe('null');
  });

  it('handles undefined error', () => {
    reportError(undefined, 'manual');
    const history = getErrorHistory();
    expect(history[0].message).toBe('undefined');
  });

  it('handles numeric error', () => {
    reportError(42, 'manual');
    const history = getErrorHistory();
    expect(history[0].message).toBe('42');
  });

  it('handles object error', () => {
    reportError({ code: 'ERR_TIMEOUT' }, 'network');
    const history = getErrorHistory();
    // Non-Error objects get String() conversion → [object Object]
    expect(history[0].message).toBe('[object Object]');
    expect(history[0].source).toBe('network');
  });

  it('defaults source to manual', () => {
    reportError('default source');
    const history = getErrorHistory();
    expect(history[0].source).toBe('manual');
  });
});

describe('getErrorHistory edge cases', () => {
  it('returns at most default 20 items', () => {
    for (let i = 0; i < 30; i++) reportError(`e${i}`);
    const history = getErrorHistory();
    expect(history.length).toBe(20);
  });

  it('returns empty array when no errors', () => {
    expect(getErrorHistory()).toHaveLength(0);
  });

  it('returns most recent entries (not oldest)', () => {
    for (let i = 0; i < 30; i++) reportError(`e${i}`);
    const history = getErrorHistory(5);
    expect(history[0].message).toBe('e25');
    expect(history[4].message).toBe('e29');
  });
});
