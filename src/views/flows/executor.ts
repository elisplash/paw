// ─────────────────────────────────────────────────────────────────────────────
// Flow Execution Engine — Executor Molecule (Core)
// Walks a flow graph node-by-node, calling the engine for agent/tool steps,
// evaluating conditions, and reporting progress via callbacks.
//
// Heavy-lifting is split across sub-modules:
//   executor-handlers.ts   — HTTP, MCP, Squad, Memory, Loop handlers
//   executor-conductor.ts  — Conductor protocol strategy execution
//   executor-debug.ts      — Debug / step mode lifecycle
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
  executeCodeSandboxed,
  resolveVariables,
  type FlowRunState,
  type FlowExecEvent,
  type NodeExecConfig,
} from './executor-atoms';
import { compileStrategy, shouldUseConductor, type ExecutionStrategy } from './conductor-atoms';
import { engineChatSend } from '../../engine/molecules/bridge';
import { pawEngine } from '../../engine/molecules/ipc_client';
import { subscribeSession } from '../../engine/molecules/event_bus';
import { showToast } from '../../components/toast';

// Sub-module imports
import {
  executeHttpRequest,
  executeMcpToolCall,
  executeSquadTask,
  executeMemoryWrite,
  executeMemoryRecall,
  executeLoopIteration,
  type HandlerEventReporter,
} from './executor-handlers';
import { runConductorStrategy, type ConductorDeps } from './executor-conductor';
import {
  initDebugSession,
  debugStepForward,
  findNextNode,
  type DebugState,
} from './executor-debug';

// ── Types ──────────────────────────────────────────────────────────────────

export interface FlowExecutorCallbacks {
  /** Called for every execution event (step start, progress, complete, etc.) */
  onEvent: (event: FlowExecEvent) => void;
  /** Called when a node's status changes (for visual updates) */
  onNodeStatusChange: (nodeId: string, status: string) => void;
  /** Called when an edge becomes active (data flowing) */
  onEdgeActive: (edgeId: string, active: boolean) => void;
  /** Resolve a flow graph by ID (for sub-flow execution) */
  flowResolver?: (flowId: string) => FlowGraph | null;
  /** Load a vault credential by name (returns decrypted value, or null) */
  credentialLoader?: (name: string) => Promise<string | null>;
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
  /** Get the last compiled execution strategy (null if none) */
  getLastStrategy: () => ExecutionStrategy | null;
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
  // Last compiled execution strategy (for UI display)
  let _lastStrategy: ExecutionStrategy | null = null;
  // Recursion depth for sub-flow execution (max 5)
  let _subFlowDepth = 0;

  /** Build a HandlerEventReporter from current closure state. */
  function handlerReporter(): HandlerEventReporter {
    return { runId: _runState!.runId, onEvent: callbacks.onEvent };
  }

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

    // Pre-load vault credentials referenced in any node config
    const vaultCreds: Record<string, string> = {};
    if (callbacks.credentialLoader) {
      const credNames = new Set<string>();
      for (const node of graph.nodes) {
        const cfg = node.config ?? {};
        // Explicit credentialName field
        if (cfg.credentialName && typeof cfg.credentialName === 'string') {
          credNames.add(cfg.credentialName);
        }
        // Scan string config values for {{vault.NAME}} references
        for (const val of Object.values(cfg)) {
          if (typeof val === 'string') {
            const matches = val.matchAll(/\{\{vault\.(\w[\w.-]*)\}\}/g);
            for (const m of matches) credNames.add(m[1]);
          }
        }
      }
      for (const name of credNames) {
        try {
          const val = await callbacks.credentialLoader(name);
          if (val !== null) vaultCreds[name] = val;
        } catch {
          /* skip failed loads */
        }
      }
    }

    _runState = createFlowRunState(graph.id, plan, graph.variables, vaultCreds);
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

    // ── Conductor Protocol: decide execution path ────────────────────────
    if (shouldUseConductor(graph)) {
      try {
        const strategy = compileStrategy(graph);
        _lastStrategy = strategy;
        callbacks.onEvent({
          type: 'run-start',
          runId: _runState.runId,
          graphName: `${graph.name} [Conductor: ${strategy.meta.collapseGroups} collapse, ${strategy.meta.parallelPhases} parallel, ${strategy.meta.extractedNodes} extracted]`,
          totalSteps: strategy.phases.length,
        });
        await runWithStrategy(graph, strategy, defaultAgentId);
      } catch (err) {
        // Conductor failed — fall back to sequential execution
        console.warn('[conductor] Strategy execution failed, falling back to sequential:', err);
        _lastStrategy = null;
        _skipNodes = new Set();
        for (const node of graph.nodes) {
          if (node.status !== 'success') {
            node.status = 'idle';
            callbacks.onNodeStatusChange(node.id, 'idle');
          }
        }
        await runSequential(graph, plan, defaultAgentId);
      }
    } else {
      _lastStrategy = null;
      await runSequential(graph, plan, defaultAgentId);
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

  // ── Sequential Execution (original path) ───────────────────────────────

  async function runSequential(
    graph: FlowGraph,
    plan: string[],
    defaultAgentId?: string,
  ): Promise<void> {
    for (let i = 0; i < plan.length; i++) {
      if (_aborted) {
        _runState!.status = 'error';
        callbacks.onEvent({ type: 'run-aborted', runId: _runState!.runId });
        break;
      }

      // Pause gate
      if (_paused) {
        _runState!.status = 'paused';
        callbacks.onEvent({ type: 'run-paused', runId: _runState!.runId, stepIndex: i });
        await new Promise<void>((resolve) => {
          _pauseResolve = resolve;
        });
        _runState!.status = 'running';
      }

      const nodeId = plan[i];
      if (_skipNodes.has(nodeId)) continue;

      // Breakpoint check
      if (_breakpoints.has(nodeId) && i > 0) {
        _paused = true;
        _runState!.status = 'paused';
        callbacks.onEvent({
          type: 'debug-breakpoint-hit',
          runId: _runState!.runId,
          nodeId,
          stepIndex: i,
        });
        callbacks.onEvent({ type: 'debug-cursor', runId: _runState!.runId, nodeId, stepIndex: i });
        await new Promise<void>((resolve) => {
          _pauseResolve = resolve;
        });
        _runState!.status = 'running';
        _paused = false;
      }

      _runState!.currentStep = i;
      const node = graph.nodes.find((n) => n.id === nodeId);
      if (!node) continue;

      await executeNode(graph, node, defaultAgentId);
      recordEdgeValues(graph, nodeId);
    }
  }

  // ── Conductor Strategy Execution (delegated to executor-conductor.ts) ──

  function buildConductorDeps(): ConductorDeps {
    return {
      getRunState: () => _runState,
      isAborted: () => _aborted,
      skipNodes: _skipNodes,
      callbacks,
      executeNode,
      executeAgentStep,
      recordEdgeValues,
    };
  }

  async function runWithStrategy(
    graph: FlowGraph,
    strategy: ExecutionStrategy,
    defaultAgentId?: string,
  ): Promise<void> {
    await runConductorStrategy(buildConductorDeps(), graph, strategy, defaultAgentId);
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
      let upstreamInput = collectNodeInput(graph, node.id, _runState.nodeStates);
      // Resolve template variables ({{flow.x}}, {{vault.x}}, {{input}}) in upstream
      upstreamInput = resolveVariables(upstreamInput, {
        input: upstreamInput,
        variables: _runState.variables,
        vaultCredentials: _runState.vaultCredentials,
      });
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

        case 'code': {
          // Code nodes execute inline JavaScript in a sandboxed environment
          const codeSource = (node.config.code as string) ?? config.prompt ?? '';
          if (!codeSource.trim()) {
            output = 'No code to execute.';
          } else {
            const codeResult = executeCodeSandboxed(
              codeSource,
              upstreamInput,
              config.timeoutMs ?? 5000,
            );
            if (codeResult.error) {
              throw new Error(`Code error: ${codeResult.error}`);
            }
            output = codeResult.output;
          }
          break;
        }

        case 'error': {
          // Error handler nodes: receive error info, log/notify, pass through
          const targets = config.errorTargets ?? ['log'];
          const errorInfo = upstreamInput || 'Unknown error';
          const parts: string[] = [];
          if (targets.includes('log')) {
            console.error(`[flow-error-handler] ${graph.name}: ${errorInfo}`);
            parts.push('Logged');
          }
          if (targets.includes('toast')) {
            parts.push('Toast sent');
          }
          if (targets.includes('chat')) {
            parts.push('Chat notified');
          }
          output = `Error handled (${parts.join(', ')}): ${errorInfo}`;
          break;
        }

        case 'agent':
        case 'tool':
        case 'data':
        default:
          // Agent/tool/data nodes send prompts to the engine
          output = await executeAgentStep(graph, node, upstreamInput, config, defaultAgentId);
          break;

        case 'http' as FlowNode['kind']:
          // HTTP nodes: direct HTTP request via Conductor Extract
          output = await executeHttpRequest(node, upstreamInput, config);
          break;

        case 'mcp-tool' as FlowNode['kind']:
          // MCP-tool nodes: direct MCP call via Conductor Extract
          output = await executeMcpToolCall(node, upstreamInput, config);
          break;

        case 'loop' as FlowNode['kind']:
          // Loop nodes: iterate over array data, execute children for each item
          output = await executeLoopIteration(
            {
              getRunState: () => _runState,
              skipNodes: _skipNodes,
              onEvent: callbacks.onEvent,
              executeAgentStep,
            },
            graph,
            node,
            upstreamInput,
            config,
            defaultAgentId,
          );
          break;

        case 'group':
          // Group/sub-flow nodes: execute the referenced sub-flow
          output = await executeSubFlow(node, upstreamInput, config, defaultAgentId);
          break;

        case 'squad' as FlowNode['kind']:
          // Squad nodes: invoke multi-agent team
          output = await executeSquadTask(node, upstreamInput, config, handlerReporter());
          break;

        case 'memory' as FlowNode['kind']:
          // Memory-write nodes: store data to long-term memory
          output = await executeMemoryWrite(node, upstreamInput, config, handlerReporter());
          break;

        case 'memory-recall' as FlowNode['kind']:
          // Memory-recall nodes: search/retrieve from long-term memory
          output = await executeMemoryRecall(node, upstreamInput, config, handlerReporter());
          break;
      }

      // Success
      nodeState.output = output;
      nodeState.status = 'success';
      nodeState.finishedAt = Date.now();
      nodeState.durationMs = nodeState.finishedAt - nodeState.startedAt;
      node.status = 'success';
      callbacks.onNodeStatusChange(node.id, 'success');

      // Set flow variable if configured
      if (config.setVariableKey && _runState) {
        _runState.variables[config.setVariableKey] = config.setVariable
          ? resolveVariables(config.setVariable, {
              input: output,
              variables: _runState.variables,
              vaultCredentials: _runState.vaultCredentials,
            })
          : output;
      }

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

      // ── Retry Logic ──────────────────────────────────────────────────────
      const maxRetries = config.maxRetries ?? 0;
      const retryDelay = config.retryDelayMs ?? 1000;
      const backoff = config.retryBackoff ?? 2;

      let retried = false;
      if (maxRetries > 0) {
        for (let attempt = 1; attempt <= maxRetries; attempt++) {
          const delay = retryDelay * Math.pow(backoff, attempt - 1);
          console.debug(
            `[flow-exec] Retry ${attempt}/${maxRetries} for "${node.label}" in ${delay}ms`,
          );
          await new Promise((r) => setTimeout(r, delay));

          try {
            // Re-attempt the node execution
            const retryInput = nodeState.input;
            let retryOutput: string;

            switch (node.kind) {
              case 'code': {
                const src = (node.config.code as string) ?? '';
                const result = executeCodeSandboxed(src, retryInput, config.timeoutMs ?? 5000);
                if (result.error) throw new Error(result.error);
                retryOutput = result.output;
                break;
              }
              default:
                retryOutput = await executeAgentStep(
                  graph,
                  node,
                  retryInput,
                  config,
                  defaultAgentId,
                );
                break;
            }

            // Retry succeeded
            nodeState.output = retryOutput;
            nodeState.status = 'success';
            nodeState.finishedAt = Date.now();
            nodeState.durationMs = nodeState.finishedAt - nodeState.startedAt;
            node.status = 'success';
            callbacks.onNodeStatusChange(node.id, 'success');
            callbacks.onEvent({
              type: 'step-complete',
              runId: _runState.runId,
              nodeId: node.id,
              output: retryOutput,
              durationMs: nodeState.durationMs,
            });
            _runState.outputLog.push({
              nodeId: node.id,
              nodeLabel: node.label,
              nodeKind: node.kind,
              status: 'success',
              output: retryOutput,
              durationMs: nodeState.durationMs,
              timestamp: Date.now(),
            });
            retried = true;
            break;
          } catch {
            // Continue to next retry attempt
          }
        }
      }

      if (!retried) {
        // All retries exhausted or no retries — mark error
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

        // ── Error Edge Routing ─────────────────────────────────────────────
        // Find error edges from this node (kind=error or fromPort=err)
        const errorEdges = graph.edges.filter(
          (e) => e.from === node.id && (e.kind === 'error' || e.fromPort === 'err'),
        );
        const errorTargetIds = new Set(errorEdges.map((e) => e.to));

        // Provide error info as input for error-path nodes
        if (errorEdges.length > 0) {
          const errorPayload = JSON.stringify({
            error: errorMsg,
            nodeId: node.id,
            nodeLabel: node.label,
          });
          const errNodeState = createNodeRunState(`${node.id}_err_output`);
          errNodeState.output = errorPayload;
          errNodeState.status = 'success';
          _runState.nodeStates.set(node.id, { ...nodeState, output: errorPayload });
        }

        // Skip downstream nodes on SUCCESS path only (not error path)
        const successEdges = graph.edges.filter(
          (e) => e.from === node.id && e.kind !== 'error' && e.fromPort !== 'err',
        );
        for (const e of successEdges) {
          _skipNodes.add(e.to);
        }
        // Error targets are NOT skipped — they'll receive error info
        for (const id of errorTargetIds) {
          _skipNodes.delete(id);
        }
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

    // ── Phase 0.1: Event-driven stream completion ─────────────────────────
    // Instead of polling every 250ms, we resolve/reject directly from the
    // session subscriber's onStreamEnd / onStreamError callbacks.  This
    // eliminates 250ms–1s of artificial latency per node.

    // Promise hooks — filled in the await below, called by subscriber callbacks.
    let streamResolve: (() => void) | null = null;
    let streamReject: ((err: Error) => void) | null = null;
    let streamSettled = false;

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
      onThinking: () => {
        /* ignore thinking deltas for flow execution */
      },
      onToken: () => {
        /* ignore token counts */
      },
      onModel: () => {
        /* ignore model changes */
      },
      onStreamEnd: () => {
        // Stream completed — resolve immediately (no polling delay)
        if (!streamSettled && streamResolve) {
          streamSettled = true;
          streamResolve();
        }
      },
      onStreamError: (error: string) => {
        // Stream errored — reject immediately
        if (!streamSettled && streamReject) {
          streamSettled = true;
          streamReject(new Error(error || `Stream error for "${node.label}"`));
        }
      },
    });

    try {
      // Get agent profile for the request
      const { getAgents } = await import('../../views/agents/index');
      const agents = getAgents();
      const agent = agents.find((a) => a.id === agentId) ?? agents[0];

      const agentProfile = agent
        ? {
            id: agent.id,
            name: agent.name,
            bio: agent.bio,
            systemPrompt: agent.systemPrompt,
            model: config.model || agent.model,
          }
        : undefined;

      // Send via engine
      const result = await engineChatSend(sessionKey, prompt, {
        model: config.model,
        agentProfile,
      });

      // Wait for stream to complete — event-driven, no polling
      const timeout = config.timeoutMs ?? 120_000;

      await new Promise<void>((resolve, reject) => {
        streamResolve = resolve;
        streamReject = reject;

        // If the subscriber already fired before we got here, resolve now
        if (streamSettled) {
          resolve();
          return;
        }

        // Sync response shortcut — if engine returned text directly
        if (result.text && !accumulated) {
          accumulated = result.text;
          streamSettled = true;
          resolve();
          return;
        }

        // Timeout guard — only safety net, not the primary completion path
        setTimeout(() => {
          if (!streamSettled) {
            streamSettled = true;
            if (accumulated.length > 0) {
              resolve(); // Got partial response, use it
            } else {
              reject(
                new Error(`Timeout after ${timeout}ms waiting for response from "${node.label}"`),
              );
            }
          }
        }, timeout);
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
   * Execute a sub-flow (group node) — look up a referenced flow graph by ID
   * and execute it recursively, passing the upstream input as initial data.
   * Max recursion depth: 5.
   */
  async function executeSubFlow(
    node: FlowNode,
    upstreamInput: string,
    config: NodeExecConfig,
    defaultAgentId?: string,
  ): Promise<string> {
    const subFlowId = config.subFlowId;
    if (!subFlowId) {
      return 'Group node: no sub-flow selected.';
    }

    if (!callbacks.flowResolver) {
      return 'Group node: flow resolver unavailable.';
    }

    if (_subFlowDepth >= 5) {
      throw new Error('Sub-flow recursion depth exceeded (max 5). Possible circular reference.');
    }

    const subGraph = callbacks.flowResolver(subFlowId);
    if (!subGraph) {
      throw new Error(`Sub-flow not found: ${subFlowId}`);
    }

    callbacks.onEvent({
      type: 'step-progress' as FlowExecEvent['type'],
      runId: _runState!.runId,
      nodeId: node.id,
      output: `Entering sub-flow: ${subGraph.name}`,
    } as FlowExecEvent);

    // Inject upstream input into the sub-flow's trigger node (if any)
    const subGraphCopy: FlowGraph = JSON.parse(JSON.stringify(subGraph));
    const triggerNode = subGraphCopy.nodes.find((n) => n.kind === 'trigger');
    if (triggerNode) {
      triggerNode.config = triggerNode.config ?? {};
      triggerNode.config.prompt = upstreamInput;
    }

    // Merge parent variables into sub-flow
    if (_runState?.variables) {
      subGraphCopy.variables = { ..._runState.variables, ...subGraphCopy.variables };
    }

    // Create a child executor for the sub-flow
    _subFlowDepth++;
    try {
      const childExecutor = createFlowExecutor({
        onEvent: (event) => {
          // Forward sub-flow events (prefix node IDs for traceability)
          callbacks.onEvent(event);
        },
        onNodeStatusChange: () => {
          /* sub-flow node status changes don't affect parent canvas */
        },
        onEdgeActive: () => {
          /* sub-flow edge changes don't affect parent canvas */
        },
        flowResolver: callbacks.flowResolver,
      });

      // Propagate recursion depth through the child
      const childState = await childExecutor.run(subGraphCopy, defaultAgentId);

      // Collect output from the sub-flow: use the output node's value, or the last successful node
      let subOutput = '';
      const nodeStatesArr = [...childState.nodeStates.values()];
      const outputNodeState = nodeStatesArr.find(
        (ns) => subGraphCopy.nodes.find((n) => n.id === ns.nodeId)?.kind === 'output',
      );
      if (outputNodeState?.output) {
        subOutput = outputNodeState.output;
      } else {
        // Fall back to last successful node's output
        const successNodes = nodeStatesArr
          .filter((ns) => ns.status === 'success' && ns.output)
          .sort((a, b) => (b.finishedAt ?? 0) - (a.finishedAt ?? 0));
        subOutput = successNodes[0]?.output ?? 'Sub-flow completed with no output.';
      }

      // Propagate any variables set by the sub-flow back to parent
      if (_runState && childState.variables) {
        Object.assign(_runState.variables, childState.variables);
      }

      return subOutput;
    } finally {
      _subFlowDepth--;
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
    const allDownstream = graph.edges.filter((e) => e.from === condNode.id).map((e) => e.to);

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
    const downstream = graph.edges.filter((e) => e.from === nodeId).map((e) => e.to);
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

    const truncatedValue =
      nodeState.output.length > 80 ? `${nodeState.output.slice(0, 77)}…` : nodeState.output;

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

  // ── Debug Mode (delegated to executor-debug.ts) ─────────────────────────

  /** Mutable state shared with the debug sub-module. */
  const _debugState: DebugState = {
    get runState() {
      return _runState;
    },
    set runState(v) {
      _runState = v;
    },
    get running() {
      return _running;
    },
    set running(v) {
      _running = v;
    },
    get debugMode() {
      return _debugMode;
    },
    set debugMode(v) {
      _debugMode = v;
    },
    get debugGraph() {
      return _debugGraph;
    },
    set debugGraph(v) {
      _debugGraph = v;
    },
    get debugAgentId() {
      return _debugAgentId;
    },
    set debugAgentId(v) {
      _debugAgentId = v;
    },
    get skipNodes() {
      return _skipNodes;
    },
    set skipNodes(v) {
      _skipNodes = v;
    },
    edgeValues: _edgeValues,
  };

  const _debugDeps = {
    state: _debugState,
    callbacks,
    executeNode,
    recordEdgeValues,
  };

  function startDebug(graph: FlowGraph, defaultAgentId?: string): void {
    _aborted = false;
    _paused = false;
    initDebugSession(_debugDeps, graph, defaultAgentId);
  }

  async function stepNext(): Promise<void> {
    await debugStepForward(_debugDeps);
  }

  function getNextNodeId(): string | null {
    if (!_runState || !_debugMode) return null;
    return findNextNode(_runState, _skipNodes, _runState.currentStep);
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
    getLastStrategy: () => _lastStrategy,
  };
}
