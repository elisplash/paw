// Today View — Orchestration, state, exports

import { type Task, filterTodayTasks, engineTaskToToday } from './atoms';
import {
  initMoleculesState,
  fetchWeather,
  fetchCalendarEvents,
  fetchSkillOutputs,
  fetchActiveSkills,
  fetchFleetStatus,
  fetchTelemetry,
  fetchEngramStats,
  fetchRecentSessions,
  loadIntegrationsDashboard,
  renderToday,
  reloadTodayTasks,
} from './molecules';
import { fetchAndRenderActivity } from './activity';
import { pawEngine } from '../../engine';
import { staggerCards } from '../../components/animations';

// ── State ─────────────────────────────────────────────────────────────

let _tasks: Task[] = [];

// ── Auto-refresh interval handles ────────────────────────────────────────
const _refreshTimers: ReturnType<typeof setInterval>[] = [];
function clearRefreshTimers() {
  _refreshTimers.splice(0).forEach(clearInterval);
}

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

  // Clear any previous refresh intervals before starting fresh
  clearRefreshTimers();

  // 1. Load tasks FIRST so renderToday() has task data for the initial render
  try {
    const all = await pawEngine.tasksList();
    const filtered = filterTodayTasks(all);
    const mapped = filtered.map(engineTaskToToday);
    _tasks = mapped;
  } catch (e) {
    console.warn('[today] Pre-load tasks failed (non-fatal):', e);
  }

  // 2. Render the full page (one-time, with task data baked in)
  renderToday();

  // 2b. Animate dashboard cards cascading in
  staggerCards('.cmd-card');

  // 3. Fetch all card data in parallel — each updates its own DOM element.
  //    reloadTodayTasks uses inPlace=true to avoid re-rendering the whole page.
  //    allSettled ensures one failure/timeout can't block others.
  await Promise.allSettled([
    withTimeout(reloadTodayTasks(true), 20000, 'tasks'),
    withTimeout(fetchWeather(), 20000, 'weather'),
    withTimeout(fetchCalendarEvents(), 20000, 'calendar'),
    withTimeout(fetchSkillOutputs(), 20000, 'skill-outputs'),
    withTimeout(fetchFleetStatus(), 20000, 'fleet'),
    withTimeout(fetchActiveSkills(), 20000, 'skills'),
    withTimeout(fetchTelemetry(), 15000, 'telemetry'),
    withTimeout(fetchAndRenderActivity(), 20000, 'activity'),
    withTimeout(loadIntegrationsDashboard(), 20000, 'integrations'),
    withTimeout(fetchEngramStats(), 10000, 'engram-stats'),
    withTimeout(fetchRecentSessions(), 10000, 'recent-sessions'),
  ]);

  // Auto-refresh time-sensitive cards while the view is open
  const FIVE_MIN = 5 * 60 * 1000;
  const THIRTY_MIN = 30 * 60 * 1000;
  _refreshTimers.push(
    setInterval(() => withTimeout(fetchCalendarEvents(), 20000, 'cal-refresh'), FIVE_MIN),
    setInterval(() => withTimeout(loadIntegrationsDashboard(), 20000, 'integ-refresh'), FIVE_MIN),
    setInterval(() => withTimeout(fetchWeather(), 20000, 'weather-refresh'), THIRTY_MIN),
  );
}

export function initToday() {
  // Called on app startup
}

// ── Re-exports ────────────────────────────────────────────────────────

export { getWeatherIcon } from './atoms';
