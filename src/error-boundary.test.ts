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
