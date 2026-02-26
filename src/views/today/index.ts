// Today View — Orchestration, state, exports

import type { Task } from './atoms';
import {
  initMoleculesState,
  fetchWeather,
  fetchUnreadEmails,
  fetchSkillOutputs,
  fetchActiveSkills,
  fetchCapabilities,
  fetchFleetStatus,
  fetchHeatmap,
  renderToday,
  reloadTodayTasks,
} from './molecules';
import { fetchAndRenderActivity } from './activity';

// ── State ─────────────────────────────────────────────────────────────

let _tasks: Task[] = [];

// ── State bridge ──────────────────────────────────────────────────────

const { setMoleculesState } = initMoleculesState();
setMoleculesState({
  getTasks: () => _tasks,
  setTasks: (t: Task[]) => {
    _tasks = t;
  },
  getRenderToday: () => renderToday,
});

// ── Public API ────────────────────────────────────────────────────────

export function configure(_opts: Record<string, unknown>) {
  // Future: callbacks for navigation etc
}

// ── Helpers ───────────────────────────────────────────────────────────

/** Wrap a promise with a timeout so no single card can block the dashboard. */
function withTimeout<T>(promise: Promise<T>, ms: number, label: string): Promise<T | void> {
  return Promise.race([
    promise,
    new Promise<void>((resolve) =>
      setTimeout(() => {
        console.warn(`[today] ${label} timed out after ${ms}ms`);
        resolve();
      }, ms),
    ),
  ]);
}

export async function loadToday() {
  console.debug('[today] loadToday called');
  renderToday();

  // Use allSettled so one slow/failing card can't block the rest.
  // Each fetch has a 20s safety timeout.
  await Promise.allSettled([
    withTimeout(reloadTodayTasks(), 20000, 'tasks'),
    withTimeout(fetchWeather(), 20000, 'weather'),
    withTimeout(fetchUnreadEmails(), 20000, 'emails'),
    withTimeout(fetchSkillOutputs(), 20000, 'skill-outputs'),
    withTimeout(fetchFleetStatus(), 20000, 'fleet'),
    withTimeout(fetchActiveSkills(), 20000, 'skills'),
    withTimeout(fetchCapabilities(), 20000, 'capabilities'),
    withTimeout(fetchHeatmap(), 20000, 'heatmap'),
    withTimeout(fetchAndRenderActivity(), 20000, 'activity'),
  ]);
}

export function initToday() {
  // Called on app startup
}

// ── Re-exports ────────────────────────────────────────────────────────

export { getWeatherIcon } from './atoms';
