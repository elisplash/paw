// Today View — Orchestration, state, exports

import type { Task } from './atoms';
import {
  initMoleculesState,
  fetchWeather,
  fetchUnreadEmails,
  fetchSkillOutputs,
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

export async function loadToday() {
  console.debug('[today] loadToday called');
  renderToday();

  await Promise.all([
    reloadTodayTasks(),
    fetchWeather(),
    fetchUnreadEmails(),
    fetchSkillOutputs(),
    fetchAndRenderActivity(),
  ]);
}

export function initToday() {
  // Called on app startup
}

// ── Re-exports ────────────────────────────────────────────────────────

export { getWeatherIcon } from './atoms';
