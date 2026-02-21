// ─────────────────────────────────────────────────────────────────────────────
// Error Boundary — Centralized error handling for the Paw application
// Catches unhandled errors, logs them, and shows user-friendly toasts.
// ─────────────────────────────────────────────────────────────────────────────

import { createLogger } from './logger';

const log = createLogger('error-boundary');

export interface ErrorReport {
  message: string;
  stack?: string;
  source: 'unhandled' | 'promise' | 'tauri' | 'network' | 'manual';
  timestamp: string;
  context?: Record<string, unknown>;
}

const errorHistory: ErrorReport[] = [];
const MAX_ERROR_HISTORY = 100;

/** Callback for UI notification (set by main.ts during init) */
let onErrorCallback: ((report: ErrorReport) => void) | null = null;

/** Register a callback to display errors in the UI (e.g., toast notification). */
export function setErrorHandler(handler: (report: ErrorReport) => void): void {
  onErrorCallback = handler;
}

/** Report an error programmatically from anywhere in the app. */
export function reportError(
  error: unknown,
  source: ErrorReport['source'] = 'manual',
  context?: Record<string, unknown>,
): void {
  const report = toErrorReport(error, source, context);
  logAndStore(report);
}

/** Get recent error reports for diagnostics. */
export function getErrorHistory(count = 20): readonly ErrorReport[] {
  return errorHistory.slice(-count);
}

/** Clear error history. */
export function clearErrorHistory(): void {
  errorHistory.length = 0;
}

// ── Install global handlers ────────────────────────────────────────────────

/** Call once from main.ts to install global error handlers. */
export function installErrorBoundary(): void {
  // Uncaught errors
  window.addEventListener('error', (event) => {
    const report = toErrorReport(event.error ?? event.message, 'unhandled', {
      filename: event.filename,
      lineno: event.lineno,
      colno: event.colno,
    });
    logAndStore(report);
    event.preventDefault();
  });

  // Unhandled promise rejections
  window.addEventListener('unhandledrejection', (event) => {
    const report = toErrorReport(event.reason, 'promise');
    logAndStore(report);
    event.preventDefault();
  });

  log.info('Error boundary installed');
}

// ── Internal helpers ───────────────────────────────────────────────────────

function toErrorReport(
  error: unknown,
  source: ErrorReport['source'],
  context?: Record<string, unknown>,
): ErrorReport {
  if (error instanceof Error) {
    return {
      message: error.message,
      stack: error.stack,
      source,
      timestamp: new Date().toISOString(),
      context,
    };
  }

  return {
    message: String(error),
    source,
    timestamp: new Date().toISOString(),
    context,
  };
}

function logAndStore(report: ErrorReport): void {
  log.error(report.message, {
    source: report.source,
    stack: report.stack?.split('\n').slice(0, 3).join(' | '),
    ...report.context,
  });

  errorHistory.push(report);
  if (errorHistory.length > MAX_ERROR_HISTORY) {
    errorHistory.shift();
  }

  onErrorCallback?.(report);
}
