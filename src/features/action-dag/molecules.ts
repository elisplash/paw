// ─────────────────────────────────────────────────────────────────────────────
// Action DAG Planning — Molecules
// Side effects: event listening, state updates, IPC. Imports from atoms.
// ─────────────────────────────────────────────────────────────────────────────

import {
  createPlanExecution,
  type PlanCompleteEvent,
  type PlanExecution,
  type PlanNodeStartEvent,
  type PlanStartEvent,
} from './atoms';

// ── In-memory plan execution tracker ───────────────────────────────────────

/** Map of session_id → active plan execution. */
const activePlans = new Map<string, PlanExecution>();

/** Get the active plan for a session (if any). */
export function getActivePlan(sessionId: string): PlanExecution | undefined {
  return activePlans.get(sessionId);
}

/** Handle a plan_start event from the engine. */
export function handlePlanStart(event: PlanStartEvent): PlanExecution {
  const plan = createPlanExecution(event.description, event.node_count);
  activePlans.set(event.session_id, plan);
  return plan;
}

/** Handle a plan_node_start event — mark a node as running. */
export function handlePlanNodeStart(event: PlanNodeStartEvent): void {
  const plan = activePlans.get(event.session_id);
  if (!plan) return;

  // We track running nodes via a lightweight approach — the result
  // will come in via the normal tool_result event, and the plan_complete
  // event will finalize everything.
}

/** Handle a plan_complete event from the engine. */
export function handlePlanComplete(event: PlanCompleteEvent): PlanExecution | undefined {
  const plan = activePlans.get(event.session_id);
  if (!plan) return undefined;

  plan.status = event.success_count === event.total_count ? 'complete' : 'partial';
  plan.durationMs = event.duration_ms;

  // Clean up after a short delay so the UI can show the final state
  setTimeout(() => {
    activePlans.delete(event.session_id);
  }, 5000);

  return plan;
}

/** Clear all active plans (e.g., on session switch). */
export function clearActivePlans(): void {
  activePlans.clear();
}
