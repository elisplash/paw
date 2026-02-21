import { describe, it, expect, beforeEach } from 'vitest';
import {
  createLogger,
  setLogLevel,
  getLogLevel,
  getRecentLogs,
  clearLogBuffer,
  getLogCounts,
  setLogTransport,
  getLogTransport,
  flushBufferToTransport,
  formatLogEntry,
} from './logger';
import type { LogEntry } from './logger';

beforeEach(() => {
  clearLogBuffer();
  setLogLevel('debug');
  setLogTransport(null);
});

describe('createLogger', () => {
  it('creates a logger with all level methods', () => {
    const log = createLogger('test');
    expect(typeof log.debug).toBe('function');
    expect(typeof log.info).toBe('function');
    expect(typeof log.warn).toBe('function');
    expect(typeof log.error).toBe('function');
  });

  it('logs to the ring buffer', () => {
    const log = createLogger('engine');
    log.info('Session started', { sessionId: 's1' });
    const logs = getRecentLogs();
    expect(logs.length).toBe(1);
    expect(logs[0].message).toBe('Session started');
    expect(logs[0].module).toBe('engine');
    expect(logs[0].level).toBe('info');
    expect(logs[0].data).toEqual({ sessionId: 's1' });
  });
});

describe('setLogLevel / getLogLevel', () => {
  it('defaults to info', () => {
    setLogLevel('info');
    expect(getLogLevel()).toBe('info');
  });

  it('suppresses lower levels', () => {
    setLogLevel('warn');
    const log = createLogger('test');
    log.debug('should not appear');
    log.info('should not appear');
    log.warn('should appear');
    log.error('should appear');
    const logs = getRecentLogs();
    expect(logs.length).toBe(2);
    expect(logs[0].level).toBe('warn');
    expect(logs[1].level).toBe('error');
  });
});

describe('getRecentLogs', () => {
  it('returns requested count', () => {
    const log = createLogger('test');
    for (let i = 0; i < 10; i++) log.info(`msg ${i}`);
    expect(getRecentLogs(3).length).toBe(3);
  });
});

describe('clearLogBuffer', () => {
  it('clears all logs', () => {
    createLogger('test').info('test');
    expect(getRecentLogs().length).toBe(1);
    clearLogBuffer();
    expect(getRecentLogs().length).toBe(0);
  });
});

describe('getLogCounts', () => {
  it('counts by level', () => {
    const log = createLogger('test');
    log.debug('d');
    log.info('i');
    log.info('i2');
    log.warn('w');
    log.error('e');
    const counts = getLogCounts();
    expect(counts.debug).toBe(1);
    expect(counts.info).toBe(2);
    expect(counts.warn).toBe(1);
    expect(counts.error).toBe(1);
  });
});

describe('formatLogEntry', () => {
  it('formats entry without data', () => {
    const entry: LogEntry = {
      level: 'info',
      message: 'hello',
      module: 'test',
      timestamp: '2026-01-01T00:00:00.000Z',
    };
    expect(formatLogEntry(entry)).toBe('[2026-01-01T00:00:00.000Z] [INFO ] [test] hello');
  });

  it('formats entry with data as JSON', () => {
    const entry: LogEntry = {
      level: 'error',
      message: 'fail',
      module: 'db',
      timestamp: '2026-01-01T00:00:00.000Z',
      data: { code: 42 },
    };
    expect(formatLogEntry(entry)).toBe('[2026-01-01T00:00:00.000Z] [ERROR] [db] fail {"code":42}');
  });
});

describe('setLogTransport / getLogTransport', () => {
  it('defaults to null', () => {
    expect(getLogTransport()).toBeNull();
  });

  it('registers and retrieves a transport', () => {
    const fn = () => {};
    setLogTransport(fn);
    expect(getLogTransport()).toBe(fn);
  });

  it('clears transport with null', () => {
    setLogTransport(() => {});
    setLogTransport(null);
    expect(getLogTransport()).toBeNull();
  });

  it('calls transport on each log emit', () => {
    const received: Array<{ entry: LogEntry; formatted: string }> = [];
    setLogTransport((entry, formatted) => {
      received.push({ entry, formatted });
    });
    const log = createLogger('mod');
    log.info('test message', { key: 'val' });
    expect(received).toHaveLength(1);
    expect(received[0].entry.message).toBe('test message');
    expect(received[0].entry.module).toBe('mod');
    expect(received[0].formatted).toContain('[mod] test message');
  });

  it('does not call transport for suppressed levels', () => {
    const received: LogEntry[] = [];
    setLogTransport((entry) => {
      received.push(entry);
    });
    setLogLevel('error');
    const log = createLogger('test');
    log.debug('skip');
    log.info('skip');
    log.warn('skip');
    log.error('keep');
    expect(received).toHaveLength(1);
    expect(received[0].level).toBe('error');
  });

  it('swallows transport errors silently', () => {
    setLogTransport(() => {
      throw new Error('boom');
    });
    const log = createLogger('test');
    expect(() => log.info('should not throw')).not.toThrow();
    // Entry should still be in the buffer despite transport failure
    expect(getRecentLogs()).toHaveLength(1);
  });
});

describe('flushBufferToTransport', () => {
  it('replays buffered entries through transport', () => {
    const log = createLogger('early');
    log.info('before transport');
    log.warn('also before');

    const received: string[] = [];
    setLogTransport((_entry, formatted) => {
      received.push(formatted);
    });
    flushBufferToTransport();

    expect(received).toHaveLength(2);
    expect(received[0]).toContain('[early] before transport');
    expect(received[1]).toContain('[early] also before');
  });

  it('does nothing when no transport is set', () => {
    createLogger('test').info('log');
    expect(() => flushBufferToTransport()).not.toThrow();
  });
});
