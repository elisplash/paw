// ─────────────────────────────────────────────────────────────────────────────
// Structured Logger — Enterprise-grade logging with levels, context, and format
// Pure module — no DOM, no Tauri IPC. Safe to use everywhere.
// ─────────────────────────────────────────────────────────────────────────────

export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

export interface LogEntry {
  level: LogLevel;
  message: string;
  module: string;
  timestamp: string;
  data?: Record<string, unknown>;
}

const LOG_LEVEL_PRIORITY: Record<LogLevel, number> = {
  debug: 0,
  info: 1,
  warn: 2,
  error: 3,
};

let globalMinLevel: LogLevel = 'info';

/** Set the minimum log level. Messages below this level are suppressed. */
export function setLogLevel(level: LogLevel): void {
  globalMinLevel = level;
}

/** Get the current minimum log level. */
export function getLogLevel(): LogLevel {
  return globalMinLevel;
}

// ── Pluggable transport ──────────────────────────────────────────────────

/** A transport receives every log entry that passes the level filter. */
export type LogTransport = (entry: LogEntry, formatted: string) => void;

let _transport: LogTransport | null = null;

/**
 * Register a persistent log transport (e.g. file writer).
 * Only one transport is supported — subsequent calls replace the previous one.
 */
export function setLogTransport(transport: LogTransport | null): void {
  _transport = transport;
}

/** Get the currently registered transport (for testing). */
export function getLogTransport(): LogTransport | null {
  return _transport;
}

/** Format a log entry into a single line suitable for file output. */
export function formatLogEntry(entry: LogEntry): string {
  const dataStr = entry.data ? ` ${JSON.stringify(entry.data)}` : '';
  return `[${entry.timestamp}] [${entry.level.toUpperCase().padEnd(5)}] [${entry.module}] ${entry.message}${dataStr}`;
}

/**
 * Replay the in-memory buffer through the current transport.
 * Call this once after setting a transport so pre-init logs are persisted.
 */
export function flushBufferToTransport(): void {
  if (!_transport) return;
  for (const entry of logBuffer) {
    _transport(entry, formatLogEntry(entry));
  }
}

/**
 * Create a scoped logger for a specific module.
 *
 * Usage:
 * ```ts
 * const log = createLogger('engine');
 * log.info('Session started', { sessionId: 's1' });
 * log.error('Failed to connect', { err: e.message });
 * ```
 */
export function createLogger(module: string) {
  function shouldLog(level: LogLevel): boolean {
    return LOG_LEVEL_PRIORITY[level] >= LOG_LEVEL_PRIORITY[globalMinLevel];
  }

  function emit(level: LogLevel, message: string, data?: Record<string, unknown>): void {
    if (!shouldLog(level)) return;

    const entry: LogEntry = {
      level,
      message,
      module,
      timestamp: new Date().toISOString(),
      data,
    };

    const prefix = `[${entry.timestamp}] [${level.toUpperCase().padEnd(5)}] [${module}]`;

    switch (level) {
      case 'debug':
        console.debug(prefix, message, data ?? '');
        break;
      case 'info':
        // eslint-disable-next-line no-console
        console.log(prefix, message, data ?? '');
        break;
      case 'warn':
        console.warn(prefix, message, data ?? '');
        break;
      case 'error':
        console.error(prefix, message, data ?? '');
        break;
    }

    // Push to in-memory ring buffer for diagnostics panel
    pushToBuffer(entry);

    // Forward to persistent transport (file, network, etc.) if registered
    if (_transport) {
      try {
        _transport(entry, formatLogEntry(entry));
      } catch {
        /* transport must not throw */
      }
    }
  }

  return {
    debug: (msg: string, data?: Record<string, unknown>) => emit('debug', msg, data),
    info: (msg: string, data?: Record<string, unknown>) => emit('info', msg, data),
    warn: (msg: string, data?: Record<string, unknown>) => emit('warn', msg, data),
    error: (msg: string, data?: Record<string, unknown>) => emit('error', msg, data),
  };
}

// ── In-memory ring buffer ────────────────────────────────────────────────

const MAX_BUFFER_SIZE = 500;
const logBuffer: LogEntry[] = [];

function pushToBuffer(entry: LogEntry): void {
  logBuffer.push(entry);
  if (logBuffer.length > MAX_BUFFER_SIZE) {
    logBuffer.shift();
  }
}

/** Get recent log entries (for diagnostics/debug panel). */
export function getRecentLogs(count = 50): readonly LogEntry[] {
  return logBuffer.slice(-count);
}

/** Clear the in-memory log buffer. */
export function clearLogBuffer(): void {
  logBuffer.length = 0;
}

/** Count log entries by level. */
export function getLogCounts(): Record<LogLevel, number> {
  const counts: Record<LogLevel, number> = { debug: 0, info: 0, warn: 0, error: 0 };
  for (const entry of logBuffer) {
    counts[entry.level]++;
  }
  return counts;
}
