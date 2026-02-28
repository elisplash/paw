// ─────────────────────────────────────────────────────────────────────────────
// Flow Executor — Debug / Step Mode
// Debug session lifecycle, step-by-step execution, breakpoints, and
// pause / resume / abort controls. Called from the main executor factory.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowGraph, FlowNode } from './atoms';
import type {
  FlowRunState,
} from './executor-atoms';
import {
  buildExecutionPlan,
  validateFlowForExecution,
  createFlowRunState,
} from './executor-atoms';
import { showToast } from '../../components/toast';
import type { FlowExecutorCallbacks } from './executor';

// ── Dependency Interface ───────────────────────────────────────────────────

/** Mutable state container shared between the executor and debug module. */
export interface DebugState {
  runState: FlowRunState | null;
  running: boolean;
  debugMode: boolean;
  debugGraph: FlowGraph | null;
  debugAgentId: string | undefined;
  skipNodes: Set<string>;
  edgeValues: Map<string, string>;
}

/** Dependencies injected from the executor closure into debug functions. */
export interface DebugDeps {
  state: DebugState;
  callbacks: FlowExecutorCallbacks;
  executeNode: (graph: FlowGraph, node: FlowNode, agentId?: string) => Promise<void>;
  recordEdgeValues: (graph: FlowGraph, nodeId: string) => void;
}

// ── Debug Session Lifecycle ────────────────────────────────────────────────

/**
 * Initialize debug/step mode. Sets up the execution plan and places
 * the cursor at step 0 without executing anything.
 */
export function initDebugSession(deps: DebugDeps, graph: FlowGraph, defaultAgentId?: string): void {
  const errors = validateFlowForExecution(graph);
  const blocking = errors.filter((e) => e.message.includes('no nodes'));
  if (blocking.length > 0) {
    showToast(blocking[0].message, 'error');
    return;
  }

  const plan = buildExecutionPlan(graph);
  const s = deps.state;
  s.runState = createFlowRunState(graph.id, plan, graph.variables);
  s.running = false;
  s.debugMode = true;
  s.debugGraph = graph;
  s.debugAgentId = defaultAgentId;
  s.skipNodes = new Set();
  s.edgeValues.clear();

  s.runState.startedAt = Date.now();
  s.runState.status = 'paused';
  s.runState.currentStep = 0;

  // Mark all nodes idle
  for (const node of graph.nodes) {
    node.status = 'idle';
    deps.callbacks.onNodeStatusChange(node.id, 'idle');
  }

  deps.callbacks.onEvent({
    type: 'run-start',
    runId: s.runState.runId,
    graphName: graph.name,
    totalSteps: plan.length,
  });

  // Emit cursor for the first executable node
  const firstNodeId = findNextNode(s.runState, s.skipNodes, 0);
  if (firstNodeId) {
    deps.callbacks.onEvent({
      type: 'debug-cursor',
      runId: s.runState.runId,
      nodeId: firstNodeId,
      stepIndex: 0,
    });
  }
}

/**
 * Execute exactly one node in the plan (debug step).
 * Advances the cursor after completion.
 */
export async function debugStepForward(deps: DebugDeps): Promise<void> {
  const s = deps.state;
  if (!s.runState || !s.debugMode || !s.debugGraph) return;
  if (s.runState.currentStep >= s.runState.plan.length) return;

  s.running = true;
  s.runState.status = 'running';

  // Find next executable node
  let stepIdx = s.runState.currentStep;
  while (stepIdx < s.runState.plan.length && s.skipNodes.has(s.runState.plan[stepIdx])) {
    stepIdx++;
  }

  if (stepIdx >= s.runState.plan.length) {
    finalizeDebug(deps);
    return;
  }

  const nodeId = s.runState.plan[stepIdx];
  const node = s.debugGraph.nodes.find((n: FlowNode) => n.id === nodeId);
  if (!node) {
    s.runState.currentStep = stepIdx + 1;
    s.running = false;
    s.runState.status = 'paused';
    return;
  }

  s.runState.currentStep = stepIdx;
  await deps.executeNode(s.debugGraph, node, s.debugAgentId);
  deps.recordEdgeValues(s.debugGraph, nodeId);

  // Advance cursor
  s.runState.currentStep = stepIdx + 1;
  s.running = false;
  s.runState.status = 'paused';

  // Check if there are more nodes
  const nextId = findNextNode(s.runState, s.skipNodes, s.runState.currentStep);
  if (nextId) {
    deps.callbacks.onEvent({
      type: 'debug-cursor',
      runId: s.runState.runId,
      nodeId: nextId,
      stepIndex: s.runState.currentStep,
    });
  } else {
    finalizeDebug(deps);
  }
}

/**
 * Find the next non-skipped node starting from stepIndex.
 */
export function findNextNode(
  runState: FlowRunState | null,
  skipNodes: Set<string>,
  fromIndex: number,
): string | null {
  if (!runState) return null;
  for (let i = fromIndex; i < runState.plan.length; i++) {
    const nodeId = runState.plan[i];
    if (!skipNodes.has(nodeId)) return nodeId;
  }
  return null;
}

/**
 * Finalize a debug run — mark as complete and emit run-complete event.
 */
export function finalizeDebug(deps: DebugDeps): void {
  const s = deps.state;
  if (!s.runState) return;
  s.runState.finishedAt = Date.now();
  s.runState.totalDurationMs = s.runState.finishedAt - s.runState.startedAt;
  s.runState.status = 'success';
  s.running = false;
  s.debugMode = false;

  deps.callbacks.onEvent({
    type: 'run-complete',
    runId: s.runState.runId,
    status: s.runState.status,
    totalDurationMs: s.runState.totalDurationMs,
    outputLog: s.runState.outputLog,
  });
}
