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
  /** Pause execution after current step completes */
  pause: () => void;
  /** Resume a paused execution */
  resume: () => void;
  /** Abort execution immediately */
  abort: () => void;
  /** Whether a flow is currently running */
  isRunning: () => boolean;
  /** Get the current run state */
  getRunState: () => FlowRunState | null;
}

// ── Factory ────────────────────────────────────────────────────────────────

export function createFlowExecutor(callbacks: FlowExecutorCallbacks): FlowExecutorController {
  let _runState: FlowRunState | null = null;
  let _aborted = false;
  let _paused = false;
  let _pauseResolve: (() => void) | null = null;
  let _running = false;

  // Nodes that should be skipped (e.g. due to condition branching)
  let _skipNodes = new Set<string>();

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

      _runState.currentStep = i;
      const node = graph.nodes.find((n) => n.id === nodeId);
      if (!node) continue;

      await executeNode(graph, node, defaultAgentId);
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
    if (_pauseResolve) {
      _pauseResolve();
      _pauseResolve = null;
    }
  }

  return {
    run,
    pause,
    resume,
    abort,
    isRunning: () => _running,
    getRunState: () => _runState,
  };
}
