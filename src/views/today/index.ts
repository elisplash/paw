// Today View — Orchestration, state, exports

import { type Task, filterTodayTasks, engineTaskToToday } from './atoms';
import {
  initMoleculesState,
  fetchWeather,
  fetchUnreadEmails,
  fetchCalendarEvents,
  fetchSkillOutputs,
  fetchActiveSkills,
  fetchCapabilities,
  fetchFleetStatus,
  fetchHeatmap,
  renderToday,
  reloadTodayTasks,
} from './molecules';
import { fetchAndRenderActivity } from './activity';
import { pawEngine } from '../../engine';
import { staggerCards } from '../../components/animations';

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
  staggerCards('.today-card');

  // 3. Fetch all card data in parallel — each updates its own DOM element.
  //    reloadTodayTasks uses inPlace=true to avoid re-rendering the whole page.
  //    allSettled ensures one failure/timeout can't block others.
  await Promise.allSettled([
    withTimeout(reloadTodayTasks(true), 20000, 'tasks'),
    withTimeout(fetchWeather(), 20000, 'weather'),
    withTimeout(fetchUnreadEmails(), 20000, 'emails'),
    withTimeout(fetchCalendarEvents(), 20000, 'calendar'),
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
