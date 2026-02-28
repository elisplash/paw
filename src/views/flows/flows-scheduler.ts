// ─────────────────────────────────────────────────────────────────────────────
// Flow Schedule Registry — Cron-based trigger scheduling
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowGraph } from './atoms';
import { type FlowSchedule, type ScheduleFireLog, nextCronFire } from './executor-atoms';
import type { FlowExecutorController } from './executor';

// ── Local State ────────────────────────────────────────────────────────────

let _scheduleTimerId: ReturnType<typeof setInterval> | null = null;
let _scheduleRegistry: FlowSchedule[] = [];
const _scheduleFireLog: ScheduleFireLog[] = [];
const SCHEDULE_TICK_MS = 30_000; // Check every 30 seconds

// ── Dependency Injection ───────────────────────────────────────────────────

export interface SchedulerDeps {
  getGraphs: () => FlowGraph[];
  getActiveGraphId: () => string | null;
  setActiveGraphId: (id: string | null) => void;
  getExecutor: () => FlowExecutorController | null;
  runActiveFlow: () => Promise<void>;
}

let _deps: SchedulerDeps | null = null;

export function initFlowsScheduler(deps: SchedulerDeps) {
  _deps = deps;
}

// ── Schedule Functions ─────────────────────────────────────────────────────

/**
 * Scan all flows for trigger nodes with active schedules and build the registry.
 */
export function rebuildScheduleRegistry() {
  if (!_deps) return;
  _scheduleRegistry = [];
  for (const graph of _deps.getGraphs()) {
    for (const node of graph.nodes) {
      if (node.kind !== 'trigger') continue;
      const schedule = (node.config?.schedule as string) ?? '';
      const enabled = (node.config?.scheduleEnabled as boolean) ?? false;
      if (!schedule || !enabled) continue;

      const next = nextCronFire(schedule);
      _scheduleRegistry.push({
        flowId: graph.id,
        flowName: graph.name,
        nodeId: node.id,
        schedule,
        enabled,
        lastFiredAt: null,
        nextFireAt: next ? next.getTime() : null,
      });
    }
  }
}

/**
 * Start the schedule ticker. Checks every 30 seconds for due schedules.
 */
export function startScheduleTicker() {
  if (_scheduleTimerId) return;
  rebuildScheduleRegistry();

  _scheduleTimerId = setInterval(() => {
    scheduleTickCheck();
  }, SCHEDULE_TICK_MS);
}

/**
 * Stop the schedule ticker.
 */
export function stopScheduleTicker() {
  if (_scheduleTimerId) {
    clearInterval(_scheduleTimerId);
    _scheduleTimerId = null;
  }
}

/**
 * Check all schedules for any that are due and fire them.
 */
async function scheduleTickCheck() {
  if (!_deps) return;
  const now = Date.now();
  for (const entry of _scheduleRegistry) {
    if (!entry.enabled || !entry.nextFireAt) continue;
    if (entry.nextFireAt > now) continue;

    // This schedule is due — fire it
    console.debug(`[flows] Schedule fired: ${entry.flowName} (${entry.schedule})`);
    entry.lastFiredAt = now;

    const graphs = _deps.getGraphs();
    const graph = graphs.find((g) => g.id === entry.flowId);
    if (!graph) continue;

    // Don't run if executor is busy
    const executor = _deps.getExecutor();
    if (executor?.isRunning()) {
      _scheduleFireLog.push({
        flowId: entry.flowId,
        flowName: entry.flowName,
        firedAt: now,
        status: 'error',
        error: 'Executor busy — skipped',
      });
      continue;
    }

    try {
      // Switch to this flow and run it
      _deps.setActiveGraphId(entry.flowId);
      await _deps.runActiveFlow();

      _scheduleFireLog.push({
        flowId: entry.flowId,
        flowName: entry.flowName,
        firedAt: now,
        status: 'success',
      });
    } catch (err) {
      _scheduleFireLog.push({
        flowId: entry.flowId,
        flowName: entry.flowName,
        firedAt: now,
        status: 'error',
        error: String(err),
      });
    }

    // Recalculate next fire time
    const next = nextCronFire(entry.schedule);
    entry.nextFireAt = next ? next.getTime() : null;
  }
}

/**
 * Get the schedule fire log (for UI display).
 */
export function getScheduleFireLog(): ScheduleFireLog[] {
  return [..._scheduleFireLog];
}

/**
 * Get the active schedule registry (for UI display).
 */
export function getScheduleRegistry(): FlowSchedule[] {
  return [..._scheduleRegistry];
}
