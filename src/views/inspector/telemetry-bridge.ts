// Telemetry Bridge — Canvas Phase 5
// Listens for `telemetry-flush` Tauri events and feeds aggregated
// metrics into the Inspector panel and any connected dashboard widgets.

import type { TelemetryTurnSummary } from '../../engine/atoms/types';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

// ── Callback registry ─────────────────────────────────────────────────

type TelemetryCallback = (summary: TelemetryTurnSummary) => void;
const _callbacks: TelemetryCallback[] = [];

/** Register a callback to receive telemetry flush events. */
export function onTelemetryFlush(cb: TelemetryCallback): () => void {
  _callbacks.push(cb);
  return () => {
    const idx = _callbacks.indexOf(cb);
    if (idx >= 0) _callbacks.splice(idx, 1);
  };
}

// ── Recent history buffer ─────────────────────────────────────────────

const MAX_HISTORY = 50;
const _history: TelemetryTurnSummary[] = [];

/** Get recent telemetry summaries (newest first). */
export function getTelemetryHistory(): readonly TelemetryTurnSummary[] {
  return _history;
}

/** Get the most recent turn summary, or null if none received yet. */
export function getLastTelemetrySummary(): TelemetryTurnSummary | null {
  return _history.length > 0 ? _history[0] : null;
}

// ── Tauri event listener ──────────────────────────────────────────────

let _unlisten: UnlistenFn | null = null;

/** Start listening for telemetry-flush events from the Rust backend. */
export async function startTelemetryListener(): Promise<void> {
  if (_unlisten) return; // Already listening

  _unlisten = await listen<TelemetryTurnSummary>('telemetry-flush', (event) => {
    const summary = event.payload;

    // Prepend to history buffer (newest first)
    _history.unshift(summary);
    if (_history.length > MAX_HISTORY) _history.pop();

    // Notify all registered callbacks
    for (const cb of _callbacks) {
      try {
        cb(summary);
      } catch (err) {
        console.warn('[telemetry-bridge] Callback error:', err);
      }
    }
  });
}

/** Stop listening (cleanup). */
export function stopTelemetryListener(): void {
  if (_unlisten) {
    _unlisten();
    _unlisten = null;
  }
}
