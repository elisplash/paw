// ─────────────────────────────────────────────────────────────────────────────
// Action DAG Planning — Atoms
// Pure types and functions for plan execution display. No side effects.
// ─────────────────────────────────────────────────────────────────────────────

// ── Types ──────────────────────────────────────────────────────────────────

/** Status of a single node in the execution plan. */
export type NodeStatus = 'pending' | 'running' | 'success' | 'error' | 'skipped';

/** A single node result from plan execution. */
export interface NodeResult {
  node_id: string;
  tool: string;
  status: NodeStatus;
  output: string;
  retryable: boolean;
  retries: number;
  duration_ms: number;
}

/** Frontend representation of a plan execution. */
export interface PlanExecution {
  /** Plan description */
  description: string;
  /** Total node count */
  nodeCount: number;
  /** Node results as they complete */
  results: NodeResult[];
  /** Overall status */
  status: 'running' | 'complete' | 'partial';
  /** Total duration in ms */
  durationMs: number;
}

/** Plan start event from the engine. */
export interface PlanStartEvent {
  kind: 'plan_start';
  session_id: string;
  run_id: string;
  description: string;
  node_count: number;
}

/** Plan node start event from the engine. */
export interface PlanNodeStartEvent {
  kind: 'plan_node_start';
  session_id: string;
  run_id: string;
  node_id: string;
  tool: string;
}

/** Plan complete event from the engine. */
export interface PlanCompleteEvent {
  kind: 'plan_complete';
  session_id: string;
  run_id: string;
  success_count: number;
  total_count: number;
  duration_ms: number;
}

// ── Pure Functions ─────────────────────────────────────────────────────────

/** Create an empty plan execution tracker. */
export function createPlanExecution(description: string, nodeCount: number): PlanExecution {
  return {
    description,
    nodeCount,
    results: [],
    status: 'running',
    durationMs: 0,
  };
}

/** Compute completion percentage of a plan. */
export function planProgress(plan: PlanExecution): number {
  if (plan.nodeCount === 0) return 100;
  return Math.round((plan.results.length / plan.nodeCount) * 100);
}

/** Format plan status for display. */
export function planStatusLabel(plan: PlanExecution): string {
  const success = plan.results.filter((r) => r.status === 'success').length;
  const errors = plan.results.filter((r) => r.status === 'error').length;
  const skipped = plan.results.filter((r) => r.status === 'skipped').length;

  if (plan.status === 'running') {
    return `Running... ${plan.results.length}/${plan.nodeCount} nodes`;
  }

  const parts: string[] = [];
  if (success > 0) parts.push(`${success} succeeded`);
  if (errors > 0) parts.push(`${errors} failed`);
  if (skipped > 0) parts.push(`${skipped} skipped`);

  const duration =
    plan.durationMs > 1000 ? `${(plan.durationMs / 1000).toFixed(1)}s` : `${plan.durationMs}ms`;

  return `${parts.join(', ')} — ${duration}`;
}

/** Get a CSS-friendly status class for a node status. */
export function nodeStatusClass(status: NodeStatus): string {
  switch (status) {
    case 'success':
      return 'plan-node-success';
    case 'error':
      return 'plan-node-error';
    case 'skipped':
      return 'plan-node-skipped';
    case 'running':
      return 'plan-node-running';
    case 'pending':
      return 'plan-node-pending';
  }
}
