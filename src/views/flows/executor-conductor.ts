// ─────────────────────────────────────────────────────────────────────────────
// Flow Executor — Conductor Strategy Execution
// Handles Conductor Protocol execution: collapsed units, convergent mesh,
// and strategy-phase orchestration. Called from the main executor factory.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowGraph, FlowNode } from './atoms';
import {
  type FlowRunState,
  type NodeExecConfig,
  createNodeRunState,
  getNodeExecConfig,
  collectNodeInput,
} from './executor-atoms';
import {
  parseCollapsedOutput,
  checkConvergence,
  type ExecutionStrategy,
  type ExecutionUnit,
} from './conductor-atoms';
import type { FlowExecutorCallbacks } from './executor';

// ── Dependency Interface ───────────────────────────────────────────────────

/** Dependencies injected from the executor closure into conductor functions. */
export interface ConductorDeps {
  getRunState: () => FlowRunState | null;
  isAborted: () => boolean;
  skipNodes: Set<string>;
  callbacks: FlowExecutorCallbacks;
  executeNode: (graph: FlowGraph, node: FlowNode, agentId?: string) => Promise<void>;
  executeAgentStep: (
    graph: FlowGraph,
    node: FlowNode,
    input: string,
    config: NodeExecConfig,
    agentId?: string,
  ) => Promise<string>;
  recordEdgeValues: (graph: FlowGraph, nodeId: string) => void;
}

// ── Strategy Execution ─────────────────────────────────────────────────────

/**
 * Execute a compiled Conductor strategy — walk through phases and
 * dispatch each unit (collapsed, mesh, single) accordingly.
 */
export async function runConductorStrategy(
  deps: ConductorDeps,
  graph: FlowGraph,
  strategy: ExecutionStrategy,
  defaultAgentId?: string,
): Promise<void> {
  const nodeMap = new Map(graph.nodes.map((n) => [n.id, n]));
  const runState = deps.getRunState();

  for (const phase of strategy.phases) {
    if (deps.isAborted()) {
      if (runState) runState.status = 'error';
      deps.callbacks.onEvent({ type: 'run-aborted', runId: runState!.runId });
      break;
    }

    // Execute all units in this phase concurrently (Parallelize primitive)
    if (phase.units.length === 1) {
      await executeConductorUnit(deps, graph, phase.units[0], nodeMap, defaultAgentId);
    } else {
      await Promise.all(
        phase.units.map((unit) => executeConductorUnit(deps, graph, unit, nodeMap, defaultAgentId)),
      );
    }
  }
}

// ── Unit Dispatch ──────────────────────────────────────────────────────────

async function executeConductorUnit(
  deps: ConductorDeps,
  graph: FlowGraph,
  unit: ExecutionUnit,
  nodeMap: Map<string, FlowNode>,
  defaultAgentId?: string,
): Promise<void> {
  if (deps.isAborted()) return;

  switch (unit.type) {
    case 'collapsed-agent':
      await executeCollapsedUnit(deps, graph, unit, nodeMap, defaultAgentId);
      break;
    case 'mesh':
      await executeMeshRounds(deps, graph, unit, nodeMap, defaultAgentId);
      break;
    case 'single-agent':
    case 'single-direct':
    case 'direct-action': {
      for (const nodeId of unit.nodeIds) {
        if (deps.isAborted() || deps.skipNodes.has(nodeId)) continue;
        const node = nodeMap.get(nodeId);
        if (!node) continue;
        await deps.executeNode(graph, node, defaultAgentId);
        deps.recordEdgeValues(graph, nodeId);
      }
      break;
    }
  }
}

// ── Collapsed Unit Execution ───────────────────────────────────────────────

/**
 * Execute a collapsed unit — multiple sequential agent nodes merged into a
 * single LLM call with a combined prompt, then output parsed back out.
 */
async function executeCollapsedUnit(
  deps: ConductorDeps,
  graph: FlowGraph,
  unit: ExecutionUnit,
  nodeMap: Map<string, FlowNode>,
  defaultAgentId?: string,
): Promise<void> {
  const runState = deps.getRunState();
  if (!runState || !unit.mergedPrompt) return;

  // Mark all nodes in the collapse group as running
  for (const nodeId of unit.nodeIds) {
    const node = nodeMap.get(nodeId);
    if (!node) continue;
    node.status = 'running';
    deps.callbacks.onNodeStatusChange(nodeId, 'running');

    const inEdges = graph.edges.filter((e) => e.to === nodeId);
    for (const e of inEdges) {
      e.active = true;
      deps.callbacks.onEdgeActive(e.id, true);
    }
  }

  // Collect upstream input for the first node in the chain
  const firstNodeId = unit.nodeIds[0];
  const upstreamInput = collectNodeInput(graph, firstNodeId, runState.nodeStates);

  // Build combined prompt
  let prompt = unit.mergedPrompt;
  if (upstreamInput) {
    prompt = `[Previous step output]\n${upstreamInput}\n\n${prompt}`;
  }

  deps.callbacks.onEvent({
    type: 'step-start',
    runId: runState.runId,
    stepIndex: runState.currentStep,
    nodeId: firstNodeId,
    nodeLabel: `Collapsed: ${unit.nodeIds.length} steps`,
    nodeKind: 'agent',
  });

  const startTime = Date.now();

  try {
    // Execute as a single LLM call
    const firstNode = nodeMap.get(firstNodeId)!;
    const config = getNodeExecConfig(firstNode);
    const output = await deps.executeAgentStep(
      graph,
      firstNode,
      upstreamInput,
      {
        ...config,
        prompt: prompt,
      },
      defaultAgentId,
    );

    const durationMs = Date.now() - startTime;

    // Parse output back into individual step outputs
    const stepOutputs = parseCollapsedOutput(output, unit.nodeIds.length);

    // Record state for each node in the chain
    for (let i = 0; i < unit.nodeIds.length; i++) {
      const nodeId = unit.nodeIds[i];
      const node = nodeMap.get(nodeId);
      if (!node) continue;

      const nodeState = createNodeRunState(nodeId);
      nodeState.startedAt = startTime;
      nodeState.output = stepOutputs[i];
      nodeState.status = 'success';
      nodeState.finishedAt = Date.now();
      nodeState.durationMs = durationMs;
      runState.nodeStates.set(nodeId, nodeState);

      node.status = 'success';
      deps.callbacks.onNodeStatusChange(nodeId, 'success');

      deps.callbacks.onEvent({
        type: 'step-complete',
        runId: runState.runId,
        nodeId,
        output: stepOutputs[i],
        durationMs,
      });

      runState.outputLog.push({
        nodeId,
        nodeLabel: node.label,
        nodeKind: node.kind,
        status: 'success',
        output: stepOutputs[i],
        durationMs,
        timestamp: Date.now(),
      });

      deps.recordEdgeValues(graph, nodeId);
    }
  } catch (err) {
    const errorMsg = err instanceof Error ? err.message : String(err);
    // Mark all nodes in the group as error
    for (const nodeId of unit.nodeIds) {
      const node = nodeMap.get(nodeId);
      if (!node) continue;

      const nodeState = createNodeRunState(nodeId);
      nodeState.status = 'error';
      nodeState.error = errorMsg;
      nodeState.finishedAt = Date.now();
      nodeState.durationMs = Date.now() - startTime;
      runState.nodeStates.set(nodeId, nodeState);

      node.status = 'error';
      deps.callbacks.onNodeStatusChange(nodeId, 'error');
    }

    deps.callbacks.onEvent({
      type: 'step-error',
      runId: runState.runId,
      nodeId: firstNodeId,
      error: errorMsg,
      durationMs: Date.now() - startTime,
    });
  } finally {
    // Deactivate edges
    for (const nodeId of unit.nodeIds) {
      const inEdges = graph.edges.filter((e) => e.to === nodeId);
      for (const e of inEdges) {
        e.active = false;
        deps.callbacks.onEdgeActive(e.id, false);
      }
    }
  }
}

// ── Convergent Mesh Execution ──────────────────────────────────────────────

/**
 * Execute a mesh unit — multiple agents iterate in rounds until their
 * outputs converge (similarity exceeds threshold) or max iterations.
 */
async function executeMeshRounds(
  deps: ConductorDeps,
  graph: FlowGraph,
  unit: ExecutionUnit,
  nodeMap: Map<string, FlowNode>,
  defaultAgentId?: string,
): Promise<void> {
  const runState = deps.getRunState();
  if (!runState) return;

  const maxIterations = unit.maxIterations ?? 5;
  const convergenceThreshold = 0.85;
  let prevOutputs = new Map<string, string>();
  const meshContext: string[] = [];

  // Mark mesh nodes as running
  for (const nodeId of unit.nodeIds) {
    const node = nodeMap.get(nodeId);
    if (node) {
      node.status = 'running';
      deps.callbacks.onNodeStatusChange(nodeId, 'running');
    }
  }

  for (let round = 1; round <= maxIterations; round++) {
    if (deps.isAborted()) break;

    const currOutputs = new Map<string, string>();

    // Execute each node in the mesh with shared context
    for (const nodeId of unit.nodeIds) {
      if (deps.isAborted()) break;
      const node = nodeMap.get(nodeId);
      if (!node) continue;

      const config = getNodeExecConfig(node);

      // Build context: all previous outputs from other mesh members
      const contextParts = [`[Convergent Mesh — Round ${round}/${maxIterations}]`];
      if (meshContext.length > 0) {
        contextParts.push('[Previous round outputs]');
        contextParts.push(meshContext.join('\n---\n'));
      }
      const upstreamInput = contextParts.join('\n\n');

      const output = await deps.executeAgentStep(
        graph,
        node,
        upstreamInput,
        config,
        defaultAgentId,
      );
      currOutputs.set(nodeId, output);

      // Update node state
      const nodeState = createNodeRunState(nodeId);
      nodeState.output = output;
      nodeState.status = 'success';
      nodeState.startedAt = Date.now();
      nodeState.finishedAt = Date.now();
      runState.nodeStates.set(nodeId, nodeState);

      deps.callbacks.onEvent({
        type: 'step-progress',
        runId: runState.runId,
        nodeId,
        delta: `[Round ${round}] ${output.slice(0, 100)}`,
      });
    }

    // Build mesh context for next round
    meshContext.length = 0;
    for (const [nodeId, output] of currOutputs) {
      const node = nodeMap.get(nodeId);
      meshContext.push(`${node?.label ?? nodeId}: ${output}`);
    }

    // Check convergence
    if (checkConvergence(prevOutputs, currOutputs, convergenceThreshold)) {
      console.debug(`[conductor-mesh] Converged at round ${round}`);
      break;
    }

    prevOutputs = currOutputs;
  }

  // Mark mesh nodes as complete
  for (const nodeId of unit.nodeIds) {
    const node = nodeMap.get(nodeId);
    if (node) {
      node.status = 'success';
      deps.callbacks.onNodeStatusChange(nodeId, 'success');

      const nodeState = runState.nodeStates.get(nodeId);
      if (nodeState) {
        deps.callbacks.onEvent({
          type: 'step-complete',
          runId: runState.runId,
          nodeId,
          output: nodeState.output,
          durationMs: nodeState.durationMs,
        });

        runState.outputLog.push({
          nodeId,
          nodeLabel: node.label,
          nodeKind: node.kind,
          status: 'success',
          output: nodeState.output,
          durationMs: nodeState.durationMs,
          timestamp: Date.now(),
        });
      }

      deps.recordEdgeValues(graph, nodeId);
    }
  }
}
