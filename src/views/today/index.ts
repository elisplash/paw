// Today View — Orchestration, state, exports

import type { Task } from './atoms';
import { initMoleculesState, fetchWeather, fetchUnreadEmails, renderToday } from './molecules';

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
  loadTasks();
  renderToday();

  await Promise.all([fetchWeather(), fetchUnreadEmails()]);
}

export function initToday() {
  // Called on app startup
}

// ── Private ───────────────────────────────────────────────────────────

function loadTasks() {
  try {
    const stored = localStorage.getItem('paw-tasks');
    _tasks = stored ? JSON.parse(stored) : [];
  } catch {
    _tasks = [];
  }
}

// ── Re-exports ────────────────────────────────────────────────────────

export { getWeatherIcon } from './atoms';
