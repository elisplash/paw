// ─────────────────────────────────────────────────────────────────────────────
// Flow Execution Engine — Executor Molecule
// Walks a flow graph node-by-node, calling the engine for agent/tool steps,
// evaluating conditions, and reporting progress via callbacks.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowGraph, FlowNode } from './atoms';
import {
  buildExecutionPlan,
  collectNodeInput,
  buildNodePrompt,
  evaluateCondition,
  resolveConditionEdges,
  createFlowRunState,
  createNodeRunState,
  getNodeExecConfig,
  validateFlowForExecution,
  type FlowRunState,
  type FlowExecEvent,
  type NodeExecConfig,
} from './executor-atoms';
import { engineChatSend } from '../../engine/molecules/bridge';
import { pawEngine } from '../../engine/molecules/ipc_client';
import { subscribeSession } from '../../engine/molecules/event_bus';
import { appState } from '../../state/index';
import { showToast } from '../../components/toast';

// ── Types ──────────────────────────────────────────────────────────────────

export interface FlowExecutorCallbacks {
  /** Called for every execution event (step start, progress, complete, etc.) */
  onEvent: (event: FlowExecEvent) => void;
  /** Called when a node's status changes (for visual updates) */
  onNodeStatusChange: (nodeId: string, status: string) => void;
  /** Called when an edge becomes active (data flowing) */
  onEdgeActive: (edgeId: string, active: boolean) => void;
}

export interface FlowExecutorController {
  /** Start executing the active flow */
  run: (graph: FlowGraph, defaultAgentId?: string) => Promise<FlowRunState>;
  /** Initialize debug mode without executing (sets up plan + cursor at step 0) */
  startDebug: (graph: FlowGraph, defaultAgentId?: string) => void;
  /** Execute only the next node in the plan (debug mode) */
  stepNext: () => Promise<void>;
  /** Pause execution after current step completes */
  pause: () => void;
  /** Resume a paused execution */
  resume: () => void;
  /** Abort execution immediately */
  abort: () => void;
  /** Whether a flow is currently running */
  isRunning: () => boolean;
  /** Whether the executor is in debug/step mode */
  isDebugMode: () => boolean;
  /** Get the current run state */
  getRunState: () => FlowRunState | null;
  /** Get the next node ID to be executed (debug cursor) */
  getNextNodeId: () => string | null;
  /** Toggle a breakpoint on a node */
  toggleBreakpoint: (nodeId: string) => void;
  /** Get the set of breakpoint node IDs */
  getBreakpoints: () => ReadonlySet<string>;
  /** Get data values flowing on edges (edge ID → truncated value) */
  getEdgeValues: () => ReadonlyMap<string, string>;
}

// ── Factory ────────────────────────────────────────────────────────────────

export function createFlowExecutor(callbacks: FlowExecutorCallbacks): FlowExecutorController {
  let _runState: FlowRunState | null = null;
  let _aborted = false;
  let _paused = false;
  let _pauseResolve: (() => void) | null = null;
  let _running = false;
  let _debugMode = false;
  let _debugGraph: FlowGraph | null = null;
  let _debugAgentId: string | undefined;

  // Nodes that should be skipped (e.g. due to condition branching)
  let _skipNodes = new Set<string>();
  // Breakpoints — node IDs where execution should auto-pause
  const _breakpoints = new Set<string>();
  // Edge data values — edge ID → value flowing through (debug inspection)
  const _edgeValues = new Map<string, string>();

  async function run(graph: FlowGraph, defaultAgentId?: string): Promise<FlowRunState> {
    // Validate
    const errors = validateFlowForExecution(graph);
    const blocking = errors.filter((e) => e.message.includes('no nodes'));
    if (blocking.length > 0) {
      showToast(blocking[0].message, 'error');
      throw new Error(blocking[0].message);
    }

    // Build plan
    const plan = buildExecutionPlan(graph);
    _runState = createFlowRunState(graph.id, plan);
    _aborted = false;
    _paused = false;
    _running = true;
    _skipNodes = new Set();

    _runState.startedAt = Date.now();
    _runState.status = 'running';

    callbacks.onEvent({
      type: 'run-start',
      runId: _runState.runId,
      graphName: graph.name,
      totalSteps: plan.length,
    });

    // Mark all nodes as idle
    for (const node of graph.nodes) {
      node.status = 'idle';
      callbacks.onNodeStatusChange(node.id, 'idle');
    }

    // Execute each step
    for (let i = 0; i < plan.length; i++) {
      if (_aborted) {
        _runState.status = 'error';
        callbacks.onEvent({ type: 'run-aborted', runId: _runState.runId });
        break;
      }

      // Pause gate
      if (_paused) {
        _runState.status = 'paused';
        callbacks.onEvent({ type: 'run-paused', runId: _runState.runId, stepIndex: i });
        await new Promise<void>((resolve) => { _pauseResolve = resolve; });
        _runState.status = 'running';
      }

      const nodeId = plan[i];

      // Skip nodes that were excluded by condition branching
      if (_skipNodes.has(nodeId)) continue;

      // Breakpoint check — auto-pause before executing this node
      if (_breakpoints.has(nodeId) && i > 0) {
        _paused = true;
        _runState.status = 'paused';
        callbacks.onEvent({ type: 'debug-breakpoint-hit', runId: _runState.runId, nodeId, stepIndex: i });
        callbacks.onEvent({ type: 'debug-cursor', runId: _runState.runId, nodeId, stepIndex: i });
        await new Promise<void>((resolve) => { _pauseResolve = resolve; });
        _runState.status = 'running';
        _paused = false;
      }

      _runState.currentStep = i;
      const node = graph.nodes.find((n) => n.id === nodeId);
      if (!node) continue;

      await executeNode(graph, node, defaultAgentId);

      // Record edge values for debug inspection
      recordEdgeValues(graph, nodeId);
    }

    // Finalize
    _runState.finishedAt = Date.now();
    _runState.totalDurationMs = _runState.finishedAt - _runState.startedAt;

    if (!_aborted && _runState.status === 'running') {
      _runState.status = 'success';
    }

    _running = false;

    callbacks.onEvent({
      type: 'run-complete',
      runId: _runState.runId,
      status: _runState.status,
      totalDurationMs: _runState.totalDurationMs,
      outputLog: _runState.outputLog,
    });

    return _runState;
  }

  async function executeNode(
    graph: FlowGraph,
    node: FlowNode,
    defaultAgentId?: string,
  ): Promise<void> {
    if (!_runState) return;

    const config = getNodeExecConfig(node);
    const nodeState = createNodeRunState(node.id);
    nodeState.startedAt = Date.now();
    nodeState.status = 'running';
    _runState.nodeStates.set(node.id, nodeState);

    // Update visual
    node.status = 'running';
    callbacks.onNodeStatusChange(node.id, 'running');

    // Activate incoming edges
    const inEdges = graph.edges.filter((e) => e.to === node.id);
    for (const e of inEdges) {
      e.active = true;
      callbacks.onEdgeActive(e.id, true);
    }

    callbacks.onEvent({
      type: 'step-start',
      runId: _runState.runId,
      stepIndex: _runState.currentStep,
      nodeId: node.id,
      nodeLabel: node.label,
      nodeKind: node.kind,
    });

    try {
      // Collect input from upstream nodes
      const upstreamInput = collectNodeInput(graph, node.id, _runState.nodeStates);
      nodeState.input = upstreamInput;

      let output: string;

      switch (node.kind) {
        case 'trigger':
          // Triggers produce their config prompt or a start signal
          output = config.prompt || upstreamInput || `Flow "${graph.name}" started.`;
          break;

        case 'output':
          // Output nodes pass through upstream data
          output = upstreamInput || 'No output.';
          break;

        case 'condition':
          // Condition nodes ask the agent to evaluate, then route
          output = await executeAgentStep(graph, node, upstreamInput, config, defaultAgentId);
          handleConditionResult(graph, node, output);
          break;

        case 'agent':
        case 'tool':
        case 'data':
        default:
          // Agent/tool/data nodes send prompts to the engine
          output = await executeAgentStep(graph, node, upstreamInput, config, defaultAgentId);
          break;
      }

      // Success
      nodeState.output = output;
      nodeState.status = 'success';
      nodeState.finishedAt = Date.now();
      nodeState.durationMs = nodeState.finishedAt - nodeState.startedAt;
      node.status = 'success';
      callbacks.onNodeStatusChange(node.id, 'success');

      callbacks.onEvent({
        type: 'step-complete',
        runId: _runState.runId,
        nodeId: node.id,
        output,
        durationMs: nodeState.durationMs,
      });

      // Log entry
      _runState.outputLog.push({
        nodeId: node.id,
        nodeLabel: node.label,
        nodeKind: node.kind,
        status: 'success',
        output,
        durationMs: nodeState.durationMs,
        timestamp: Date.now(),
      });

    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : String(err);
      nodeState.status = 'error';
      nodeState.error = errorMsg;
      nodeState.finishedAt = Date.now();
      nodeState.durationMs = nodeState.finishedAt - nodeState.startedAt;
      node.status = 'error';
      callbacks.onNodeStatusChange(node.id, 'error');

      callbacks.onEvent({
        type: 'step-error',
        runId: _runState.runId,
        nodeId: node.id,
        error: errorMsg,
        durationMs: nodeState.durationMs,
      });

      _runState.outputLog.push({
        nodeId: node.id,
        nodeLabel: node.label,
        nodeKind: node.kind,
        status: 'error',
        output: '',
        error: errorMsg,
        durationMs: nodeState.durationMs,
        timestamp: Date.now(),
      });

      // Retry logic
      if (config.maxRetries && config.maxRetries > 0) {
        // Simplified: just log (full retry would need recursion with counter)
        console.warn(`[flow-exec] Node "${node.label}" failed, retries not yet implemented.`);
      }

      // Don't abort the whole flow on one node error — mark and continue
      // but skip downstream since input is missing
      const downstream = graph.edges
        .filter((e) => e.from === node.id)
        .map((e) => e.to);
      for (const dId of downstream) {
        _skipNodes.add(dId);
      }
    } finally {
      // Deactivate incoming edges
      for (const e of inEdges) {
        e.active = false;
        callbacks.onEdgeActive(e.id, false);
      }
    }
  }

  /**
   * Execute an agent interaction for a node.
   * Creates a temporary session, sends the prompt, collects the response.
   */
  async function executeAgentStep(
    _graph: FlowGraph,
    node: FlowNode,
    upstreamInput: string,
    config: NodeExecConfig,
    defaultAgentId?: string,
  ): Promise<string> {
    const agentId = config.agentId || defaultAgentId || 'default';
    const prompt = buildNodePrompt(node, upstreamInput, config);

    // Use a dedicated session key for this flow run + node
    const sessionKey = `flow_${_runState!.runId}_${node.id}`;

    // Accumulate streamed text
    let accumulated = '';

    // Subscribe to session events to capture the response
    const unsubscribe = subscribeSession(sessionKey, {
      onDelta: (text: string) => {
        accumulated += text;
        // Report progress
        if (_runState) {
          callbacks.onEvent({
            type: 'step-progress',
            runId: _runState.runId,
            nodeId: node.id,
            delta: text,
          });
        }
      },
      onThinking: () => { /* ignore thinking deltas for flow execution */ },
      onToken: () => { /* ignore token counts */ },
      onModel: () => { /* ignore model changes */ },
    });

    try {
      // Get agent profile for the request
      const { getAgents } = await import('../../views/agents/index');
      const agents = getAgents();
      const agent = agents.find((a) => a.id === agentId) ?? agents[0];

      const agentProfile = agent ? {
        id: agent.id,
        name: agent.name,
        bio: agent.bio,
        systemPrompt: agent.systemPrompt,
        model: config.model || agent.model,
      } : undefined;

      // Send via engine
      const result = await engineChatSend(sessionKey, prompt, {
        model: config.model,
        agentProfile,
      });

      // Wait for stream to complete with a timeout
      const timeout = config.timeoutMs ?? 120_000;
      const start = Date.now();

      // Poll for completion (engine events are async)
      await new Promise<void>((resolve, reject) => {
        const checkInterval = setInterval(() => {
          // Check if we've accumulated a response
          if (accumulated.length > 0 && !appState.activeStreams.has(sessionKey)) {
            clearInterval(checkInterval);
            resolve();
          }
          // Timeout
          if (Date.now() - start > timeout) {
            clearInterval(checkInterval);
            if (accumulated.length > 0) {
              resolve(); // Got partial response, use it
            } else {
              reject(new Error(`Timeout after ${timeout}ms waiting for response from "${node.label}"`));
            }
          }
        }, 250);

        // Also resolve quickly if stream was never started (sync response)
        setTimeout(() => {
          if (!appState.activeStreams.has(sessionKey) && accumulated.length === 0) {
            // Check if result had a text field
            if (result.text) {
              accumulated = result.text;
              clearInterval(checkInterval);
              resolve();
            }
          }
        }, 1000);
      });

      // Clean up the temporary session
      try {
        await pawEngine.sessionDelete(sessionKey);
      } catch {
        // Best effort cleanup
      }

      return accumulated || 'No response received.';
    } finally {
      unsubscribe();
    }
  }

  /**
   * Handle condition node results — determine which branches to follow/skip.
   */
  function handleConditionResult(graph: FlowGraph, condNode: FlowNode, response: string): void {
    const result = evaluateCondition(response);
    const activeEdges = resolveConditionEdges(graph, condNode.id, result);
    const activeTargets = new Set(activeEdges.map((e) => e.to));

    // All downstream edges that are NOT active should have their targets skipped
    const allDownstream = graph.edges
      .filter((e) => e.from === condNode.id)
      .map((e) => e.to);

    for (const targetId of allDownstream) {
      if (!activeTargets.has(targetId)) {
        _skipNodes.add(targetId);
        // Also skip the entire subtree below skipped nodes
        skipSubtree(graph, targetId);
      }
    }
  }

  /**
   * Recursively mark all downstream nodes as skipped.
   */
  function skipSubtree(graph: FlowGraph, nodeId: string): void {
    const downstream = graph.edges
      .filter((e) => e.from === nodeId)
      .map((e) => e.to);
    for (const dId of downstream) {
      if (!_skipNodes.has(dId)) {
        _skipNodes.add(dId);
        skipSubtree(graph, dId);
      }
    }
  }

  /**
   * Record the data value flowing on outgoing edges from a completed node.
   * Used for debug visualization of data on edges.
   */
  function recordEdgeValues(graph: FlowGraph, nodeId: string): void {
    if (!_runState) return;
    const nodeState = _runState.nodeStates.get(nodeId);
    if (!nodeState?.output) return;

    const truncatedValue = nodeState.output.length > 80
      ? `${nodeState.output.slice(0, 77)}…`
      : nodeState.output;

    const outEdges = graph.edges.filter((e) => e.from === nodeId);
    for (const edge of outEdges) {
      _edgeValues.set(edge.id, truncatedValue);
      callbacks.onEvent({
        type: 'debug-edge-value',
        runId: _runState.runId,
        edgeId: edge.id,
        value: truncatedValue,
      });
    }
  }

  // ── Debug Mode ─────────────────────────────────────────────────────────

  /**
   * Initialize debug/step mode. Sets up the execution plan and places
   * the cursor at step 0 without executing anything.
   */
  function startDebug(graph: FlowGraph, defaultAgentId?: string): void {
    const errors = validateFlowForExecution(graph);
    const blocking = errors.filter((e) => e.message.includes('no nodes'));
    if (blocking.length > 0) {
      showToast(blocking[0].message, 'error');
      return;
    }

    const plan = buildExecutionPlan(graph);
    _runState = createFlowRunState(graph.id, plan);
    _aborted = false;
    _paused = false;
    _running = false;
    _debugMode = true;
    _debugGraph = graph;
    _debugAgentId = defaultAgentId;
    _skipNodes = new Set();
    _edgeValues.clear();

    _runState.startedAt = Date.now();
    _runState.status = 'paused';
    _runState.currentStep = 0;

    // Mark all nodes idle
    for (const node of graph.nodes) {
      node.status = 'idle';
      callbacks.onNodeStatusChange(node.id, 'idle');
    }

    callbacks.onEvent({
      type: 'run-start',
      runId: _runState.runId,
      graphName: graph.name,
      totalSteps: plan.length,
    });

    // Emit cursor for the first executable node
    const firstNodeId = findNextExecutableNode(0);
    if (firstNodeId) {
      callbacks.onEvent({
        type: 'debug-cursor',
        runId: _runState.runId,
        nodeId: firstNodeId,
        stepIndex: 0,
      });
    }
  }

  /**
   * Find the next non-skipped node starting from stepIndex.
   */
  function findNextExecutableNode(fromIndex: number): string | null {
    if (!_runState) return null;
    for (let i = fromIndex; i < _runState.plan.length; i++) {
      const nodeId = _runState.plan[i];
      if (!_skipNodes.has(nodeId)) return nodeId;
    }
    return null;
  }

  /**
   * Execute exactly one node in the plan (debug step).
   * Advances the cursor after completion.
   */
  async function stepNext(): Promise<void> {
    if (!_runState || !_debugMode || !_debugGraph) return;
    if (_runState.currentStep >= _runState.plan.length) return;

    _running = true;
    _runState.status = 'running';

    // Find next executable node
    let stepIdx = _runState.currentStep;
    while (stepIdx < _runState.plan.length && _skipNodes.has(_runState.plan[stepIdx])) {
      stepIdx++;
    }

    if (stepIdx >= _runState.plan.length) {
      // All remaining nodes are skipped — finalize
      finalizeDebugRun();
      return;
    }

    const nodeId = _runState.plan[stepIdx];
    const node = _debugGraph.nodes.find((n) => n.id === nodeId);
    if (!node) {
      _runState.currentStep = stepIdx + 1;
      _running = false;
      _runState.status = 'paused';
      return;
    }

    _runState.currentStep = stepIdx;
    await executeNode(_debugGraph, node, _debugAgentId);
    recordEdgeValues(_debugGraph, nodeId);

    // Advance cursor
    _runState.currentStep = stepIdx + 1;
    _running = false;
    _runState.status = 'paused';

    // Check if there are more nodes
    const nextId = findNextExecutableNode(_runState.currentStep);
    if (nextId) {
      callbacks.onEvent({
        type: 'debug-cursor',
        runId: _runState.runId,
        nodeId: nextId,
        stepIndex: _runState.currentStep,
      });
    } else {
      // Done
      finalizeDebugRun();
    }
  }

  function finalizeDebugRun(): void {
    if (!_runState) return;
    _runState.finishedAt = Date.now();
    _runState.totalDurationMs = _runState.finishedAt - _runState.startedAt;
    _runState.status = 'success';
    _running = false;
    _debugMode = false;

    callbacks.onEvent({
      type: 'run-complete',
      runId: _runState.runId,
      status: _runState.status,
      totalDurationMs: _runState.totalDurationMs,
      outputLog: _runState.outputLog,
    });
  }

  function getNextNodeId(): string | null {
    if (!_runState || !_debugMode) return null;
    return findNextExecutableNode(_runState.currentStep);
  }

  // ── Breakpoints ────────────────────────────────────────────────────────

  function toggleBreakpoint(nodeId: string): void {
    if (_breakpoints.has(nodeId)) {
      _breakpoints.delete(nodeId);
    } else {
      _breakpoints.add(nodeId);
    }
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────

  function pause(): void {
    _paused = true;
  }

  function resume(): void {
    _paused = false;
    if (_pauseResolve) {
      _pauseResolve();
      _pauseResolve = null;
    }
  }

  function abort(): void {
    _aborted = true;
    _paused = false;
    _debugMode = false;
    _debugGraph = null;
    if (_pauseResolve) {
      _pauseResolve();
      _pauseResolve = null;
    }
  }

  return {
    run,
    startDebug,
    stepNext,
    pause,
    resume,
    abort,
    isRunning: () => _running,
    isDebugMode: () => _debugMode,
    getRunState: () => _runState,
    getNextNodeId,
    toggleBreakpoint,
    getBreakpoints: () => _breakpoints,
    getEdgeValues: () => _edgeValues,
  };
}
