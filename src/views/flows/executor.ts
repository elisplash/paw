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
  executeCodeSandboxed,
  resolveVariables,
  parseLoopArray,
  type FlowRunState,
  type FlowExecEvent,
  type NodeExecConfig,
} from './executor-atoms';
import {
  compileStrategy,
  shouldUseConductor,
  parseCollapsedOutput,
  checkConvergence,
  type ExecutionStrategy,
  type ExecutionUnit,
} from './conductor-atoms';
import { engineChatSend } from '../../engine/molecules/bridge';
import { pawEngine } from '../../engine/molecules/ipc_client';
import { subscribeSession } from '../../engine/molecules/event_bus';
import { showToast } from '../../components/toast';

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
        } catch { /* skip failed loads */ }
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

  // ── Conductor Strategy Execution ───────────────────────────────────────

  async function runWithStrategy(
    graph: FlowGraph,
    strategy: ExecutionStrategy,
    defaultAgentId?: string,
  ): Promise<void> {
    const nodeMap = new Map(graph.nodes.map((n) => [n.id, n]));

    for (const phase of strategy.phases) {
      if (_aborted) {
        _runState!.status = 'error';
        callbacks.onEvent({ type: 'run-aborted', runId: _runState!.runId });
        break;
      }

      // Execute all units in this phase concurrently (Parallelize primitive)
      if (phase.units.length === 1) {
        // Single unit — no parallelism needed
        await executeUnit(graph, phase.units[0], nodeMap, defaultAgentId);
      } else {
        // Multiple units — run in parallel via Promise.all
        await Promise.all(
          phase.units.map((unit) =>
            executeUnit(graph, unit, nodeMap, defaultAgentId),
          ),
        );
      }
    }
  }

  async function executeUnit(
    graph: FlowGraph,
    unit: ExecutionUnit,
    nodeMap: Map<string, FlowNode>,
    defaultAgentId?: string,
  ): Promise<void> {
    if (_aborted) return;

    switch (unit.type) {
      case 'collapsed-agent':
        await executeCollapsedUnit(graph, unit, nodeMap, defaultAgentId);
        break;
      case 'mesh':
        await executeMeshUnit(graph, unit, nodeMap, defaultAgentId);
        break;
      case 'single-agent':
      case 'single-direct':
      case 'direct-action': {
        // Execute each node in the unit sequentially
        for (const nodeId of unit.nodeIds) {
          if (_aborted || _skipNodes.has(nodeId)) continue;
          const node = nodeMap.get(nodeId);
          if (!node) continue;
          await executeNode(graph, node, defaultAgentId);
          recordEdgeValues(graph, nodeId);
        }
        break;
      }
    }
  }

  // ── Collapse Execution ─────────────────────────────────────────────────

  async function executeCollapsedUnit(
    graph: FlowGraph,
    unit: ExecutionUnit,
    nodeMap: Map<string, FlowNode>,
    defaultAgentId?: string,
  ): Promise<void> {
    if (!_runState || !unit.mergedPrompt) return;

    // Mark all nodes in the collapse group as running
    for (const nodeId of unit.nodeIds) {
      const node = nodeMap.get(nodeId);
      if (!node) continue;
      node.status = 'running';
      callbacks.onNodeStatusChange(nodeId, 'running');

      const inEdges = graph.edges.filter((e) => e.to === nodeId);
      for (const e of inEdges) {
        e.active = true;
        callbacks.onEdgeActive(e.id, true);
      }
    }

    // Collect upstream input for the first node in the chain
    const firstNodeId = unit.nodeIds[0];
    const upstreamInput = collectNodeInput(graph, firstNodeId, _runState.nodeStates);

    // Build combined prompt
    let prompt = unit.mergedPrompt;
    if (upstreamInput) {
      prompt = `[Previous step output]\n${upstreamInput}\n\n${prompt}`;
    }

    callbacks.onEvent({
      type: 'step-start',
      runId: _runState.runId,
      stepIndex: _runState.currentStep,
      nodeId: firstNodeId,
      nodeLabel: `Collapsed: ${unit.nodeIds.length} steps`,
      nodeKind: 'agent',
    });

    const startTime = Date.now();

    try {
      // Execute as a single LLM call
      const firstNode = nodeMap.get(firstNodeId)!;
      const config = getNodeExecConfig(firstNode);
      const output = await executeAgentStep(graph, firstNode, upstreamInput, {
        ...config,
        prompt: prompt,
      }, defaultAgentId);

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
        _runState.nodeStates.set(nodeId, nodeState);

        node.status = 'success';
        callbacks.onNodeStatusChange(nodeId, 'success');

        callbacks.onEvent({
          type: 'step-complete',
          runId: _runState.runId,
          nodeId,
          output: stepOutputs[i],
          durationMs,
        });

        _runState.outputLog.push({
          nodeId,
          nodeLabel: node.label,
          nodeKind: node.kind,
          status: 'success',
          output: stepOutputs[i],
          durationMs,
          timestamp: Date.now(),
        });

        recordEdgeValues(graph, nodeId);
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
        _runState.nodeStates.set(nodeId, nodeState);

        node.status = 'error';
        callbacks.onNodeStatusChange(nodeId, 'error');
      }

      callbacks.onEvent({
        type: 'step-error',
        runId: _runState.runId,
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
          callbacks.onEdgeActive(e.id, false);
        }
      }
    }
  }

  // ── Convergent Mesh Execution ──────────────────────────────────────────

  async function executeMeshUnit(
    graph: FlowGraph,
    unit: ExecutionUnit,
    nodeMap: Map<string, FlowNode>,
    defaultAgentId?: string,
  ): Promise<void> {
    if (!_runState) return;

    const maxIterations = unit.maxIterations ?? 5;
    const convergenceThreshold = 0.85;
    let prevOutputs = new Map<string, string>();
    const meshContext: string[] = [];

    // Mark mesh nodes as running
    for (const nodeId of unit.nodeIds) {
      const node = nodeMap.get(nodeId);
      if (node) {
        node.status = 'running';
        callbacks.onNodeStatusChange(nodeId, 'running');
      }
    }

    for (let round = 1; round <= maxIterations; round++) {
      if (_aborted) break;

      const currOutputs = new Map<string, string>();

      // Execute each node in the mesh with shared context
      for (const nodeId of unit.nodeIds) {
        if (_aborted) break;
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

        const output = await executeAgentStep(graph, node, upstreamInput, config, defaultAgentId);
        currOutputs.set(nodeId, output);

        // Update node state
        const nodeState = createNodeRunState(nodeId);
        nodeState.output = output;
        nodeState.status = 'success';
        nodeState.startedAt = Date.now();
        nodeState.finishedAt = Date.now();
        _runState.nodeStates.set(nodeId, nodeState);

        callbacks.onEvent({
          type: 'step-progress',
          runId: _runState.runId,
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
        callbacks.onNodeStatusChange(nodeId, 'success');

        const nodeState = _runState.nodeStates.get(nodeId);
        if (nodeState) {
          callbacks.onEvent({
            type: 'step-complete',
            runId: _runState.runId,
            nodeId,
            output: nodeState.output,
            durationMs: nodeState.durationMs,
          });

          _runState.outputLog.push({
            nodeId,
            nodeLabel: node.label,
            nodeKind: node.kind,
            status: 'success',
            output: nodeState.output,
            durationMs: nodeState.durationMs,
            timestamp: Date.now(),
          });
        }

        recordEdgeValues(graph, nodeId);
      }
    }
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
          output = await executeHttpNode(node, upstreamInput, config);
          break;

        case 'mcp-tool' as FlowNode['kind']:
          // MCP-tool nodes: direct MCP call via Conductor Extract
          output = await executeMcpToolNode(node, upstreamInput, config);
          break;

        case 'loop' as FlowNode['kind']:
          // Loop nodes: iterate over array data, execute children for each item
          output = await executeLoopNode(graph, node, upstreamInput, config, defaultAgentId);
          break;

        case 'group':
          // Group/sub-flow nodes: execute the referenced sub-flow
          output = await executeSubFlow(node, upstreamInput, config, defaultAgentId);
          break;

        case 'squad' as FlowNode['kind']:
          // Squad nodes: invoke multi-agent team
          output = await executeSquadNode(node, upstreamInput, config);
          break;

        case 'memory' as FlowNode['kind']:
          // Memory-write nodes: store data to long-term memory
          output = await executeMemoryWriteNode(node, upstreamInput, config);
          break;

        case 'memory-recall' as FlowNode['kind']:
          // Memory-recall nodes: search/retrieve from long-term memory
          output = await executeMemoryRecallNode(node, upstreamInput, config);
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
   * Execute a direct HTTP request node (Conductor Extract primitive).
   * Bypasses LLM entirely — routes directly through Rust backend.
   */
  async function executeHttpNode(
    node: FlowNode,
    upstreamInput: string,
    config: NodeExecConfig,
  ): Promise<string> {
    const method = config.httpMethod || (node.config.httpMethod as string) || 'GET';
    let url = config.httpUrl || (node.config.httpUrl as string) || '';
    const headersStr = config.httpHeaders || (node.config.httpHeaders as string) || '{}';
    let body = config.httpBody || (node.config.httpBody as string) || undefined;

    if (!url) {
      throw new Error(`HTTP node "${node.label}" has no URL configured`);
    }

    // Template substitution: replace {{input}} with upstream data
    url = url.replace(/\{\{input\}\}/g, encodeURIComponent(upstreamInput));
    if (body) {
      body = body.replace(/\{\{input\}\}/g, upstreamInput);
    }

    let headers: Record<string, string> = {};
    try {
      headers = JSON.parse(headersStr);
    } catch {
      // Ignore invalid headers JSON
    }

    try {
      const response = await pawEngine.flowDirectHttp({
        method,
        url,
        headers: Object.keys(headers).length > 0 ? headers : undefined,
        body,
        timeout_ms: config.timeoutMs ?? 30000,
      });

      return JSON.stringify({
        status: response.status,
        body: response.body,
        duration_ms: response.duration_ms,
      });
    } catch (_err) {
      // Fallback: try in-browser fetch (for dev mode without Tauri)
      const resp = await fetch(url, {
        method,
        headers,
        body: method !== 'GET' ? body : undefined,
      });
      const text = await resp.text();
      return JSON.stringify({ status: resp.status, body: text });
    }
  }

  /**
   * Execute a direct MCP tool call node (Conductor Extract primitive).
   * Bypasses LLM entirely — routes through Rust MCP registry.
   */
  async function executeMcpToolNode(
    node: FlowNode,
    upstreamInput: string,
    config: NodeExecConfig,
  ): Promise<string> {
    const toolName = config.mcpToolName || (node.config.mcpToolName as string) || '';
    const argsStr = config.mcpToolArgs || (node.config.mcpToolArgs as string) || '{}';

    if (!toolName) {
      throw new Error(`MCP-tool node "${node.label}" has no tool name configured`);
    }

    let args: Record<string, unknown> = {};
    try {
      args = JSON.parse(argsStr);
    } catch {
      // Try using upstream as args
      try {
        args = JSON.parse(upstreamInput);
      } catch {
        args = { input: upstreamInput };
      }
    }

    // Template substitution in args
    const argsJson = JSON.stringify(args).replace(/\{\{input\}\}/g, upstreamInput);
    args = JSON.parse(argsJson);

    const response = await pawEngine.flowDirectMcp({
      tool_name: toolName,
      arguments: args,
    });

    if (!response.success) {
      throw new Error(`MCP tool "${toolName}" failed: ${response.output}`);
    }

    return response.output;
  }

  /**
   * Execute a loop node — iterate over array data and execute downstream
   * nodes for each item. The loop node itself collects all iteration results.
   */
  async function executeLoopNode(
    graph: FlowGraph,
    node: FlowNode,
    upstreamInput: string,
    config: NodeExecConfig,
    defaultAgentId?: string,
  ): Promise<string> {
    const items = parseLoopArray(upstreamInput, config.loopOver);
    const maxIter = config.loopMaxIterations ?? 100;
    const loopVar = config.loopVar ?? 'item';

    if (items.length === 0) {
      return 'Loop: no items to iterate.';
    }

    const cappedItems = items.slice(0, maxIter);
    const results: string[] = [];

    // Find downstream nodes connected to this loop node
    const downstreamEdges = graph.edges.filter((e) => e.from === node.id && e.kind !== 'reverse');
    const downstreamIds = downstreamEdges.map((e) => e.to);

    for (let i = 0; i < cappedItems.length; i++) {
      const item = cappedItems[i];
      const itemStr = typeof item === 'string' ? item : JSON.stringify(item);

      // Set loop context variables in run state
      if (_runState) {
        _runState.variables['__loop_index'] = i;
        _runState.variables['__loop_item'] = item;
        _runState.variables['__loop_var'] = loopVar;
        _runState.variables['__loop_total'] = cappedItems.length;
      }

      callbacks.onEvent({
        type: 'step-progress' as FlowExecEvent['type'],
        runId: _runState!.runId,
        nodeId: node.id,
        output: `Loop iteration ${i + 1}/${cappedItems.length}`,
      } as FlowExecEvent);

      // For each downstream node, execute it with the current item as input
      const iterResults: string[] = [];
      for (const targetId of downstreamIds) {
        const targetNode = graph.nodes.find((n) => n.id === targetId);
        if (!targetNode) continue;

        // Temporarily inject the loop item as this node's upstream
        const targetConfig = getNodeExecConfig(targetNode);
        const resolvedInput = resolveVariables(itemStr, {
          input: itemStr,
          variables: _runState?.variables,
          loopIndex: i,
          loopItem: item,
          loopVar,
          vaultCredentials: _runState?.vaultCredentials,
        });

        try {
          let iterOutput: string;
          switch (targetNode.kind) {
            case 'agent':
            case 'tool':
            case 'data':
              iterOutput = await executeAgentStep(
                graph,
                targetNode,
                resolvedInput,
                targetConfig,
                defaultAgentId,
              );
              break;
            case 'code': {
              const codeSource = (targetNode.config.code as string) ?? targetConfig.prompt ?? '';
              const codeResult = executeCodeSandboxed(codeSource, resolvedInput, targetConfig.timeoutMs ?? 5000);
              if (codeResult.error) throw new Error(`Code error: ${codeResult.error}`);
              iterOutput = codeResult.output;
              break;
            }
            default:
              iterOutput = resolvedInput;
          }
          iterResults.push(iterOutput);
        } catch (err) {
          iterResults.push(`Error in iteration ${i + 1}: ${err instanceof Error ? err.message : String(err)}`);
        }
      }

      results.push(
        downstreamIds.length > 0
          ? iterResults.join('\n')
          : itemStr,
      );
    }

    // Mark downstream nodes as handled (they were executed inside the loop)
    for (const targetId of downstreamIds) {
      _skipNodes.add(targetId);
    }

    // Clean up loop context
    if (_runState) {
      delete _runState.variables['__loop_index'];
      delete _runState.variables['__loop_item'];
      delete _runState.variables['__loop_var'];
      delete _runState.variables['__loop_total'];
    }

    return results.join('\n---\n');
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

  // ── Phase 4: Squad / Memory / Memory-Recall Node Handlers ────────────────

  /**
   * Execute a squad node — invoke a multi-agent team via the squad API.
   * Sends a task to the squad and returns the combined result.
   */
  async function executeSquadNode(
    node: FlowNode,
    upstreamInput: string,
    config: NodeExecConfig,
  ): Promise<string> {
    const squadId = config.squadId;
    if (!squadId) {
      throw new Error('Squad node: no squad selected. Configure a squad ID.');
    }

    const objective = config.squadObjective || upstreamInput || node.label;
    const timeoutMs = config.squadTimeoutMs ?? 300_000;

    callbacks.onEvent({
      type: 'step-progress' as FlowExecEvent['type'],
      runId: _runState!.runId,
      nodeId: node.id,
      delta: `Dispatching to squad ${squadId}…`,
    } as FlowExecEvent);

    // Use engineChatSend to orchestrate the squad task
    // The squad profile routes to squad-handling in the Rust backend
    const sessionKey = `flow-squad-${_runState!.runId}-${node.id}`;
    let result = '';

    try {
      const response = await Promise.race([
        engineChatSend(
          sessionKey,
          `[Squad Task] ${objective}\n\n[Context]\n${upstreamInput}`,
          {
            agentProfile: {
              id: squadId,
              name: `Squad ${squadId}`,
              bio: `Multi-agent squad executing: ${node.label}`,
              systemPrompt: `You are coordinating a squad of agents. Objective: ${objective}`,
              model: config.model || '',
              personality: { tone: 'focused' },
              boundaries: [],
              autoApproveAll: true,
            },
          },
        ),
        new Promise<never>((_, reject) =>
          setTimeout(() => reject(new Error(`Squad timeout after ${timeoutMs / 1000}s`)), timeoutMs),
        ),
      ]);
      result = response?.text ?? 'Squad completed with no output.';
    } catch (err) {
      throw new Error(`Squad execution failed: ${err instanceof Error ? err.message : String(err)}`);
    }

    return result;
  }

  /**
   * Execute a memory-write node — store data to long-term memory via IPC.
   */
  async function executeMemoryWriteNode(
    node: FlowNode,
    upstreamInput: string,
    config: NodeExecConfig,
  ): Promise<string> {
    const category = config.memoryCategory ?? 'insight';
    const importance = config.memoryImportance ?? 0.5;
    const agentId = config.memoryAgentId;

    // Determine what to store
    let content: string;
    if (config.memorySource === 'custom' && config.memoryContent) {
      content = config.memoryContent;
    } else {
      content = upstreamInput || 'No content to store.';
    }

    // Prefix with node context
    const contextContent = `[Flow: ${node.label}] ${content}`;

    callbacks.onEvent({
      type: 'step-progress' as FlowExecEvent['type'],
      runId: _runState!.runId,
      nodeId: node.id,
      delta: `Storing to memory (${category})…`,
    } as FlowExecEvent);

    try {
      await pawEngine.memoryStore(contextContent, category, importance, agentId);
    } catch (err) {
      throw new Error(`Memory store failed: ${err instanceof Error ? err.message : String(err)}`);
    }

    return `Stored to memory [${category}] (importance: ${importance}): ${content.slice(0, 100)}${content.length > 100 ? '…' : ''}`;
  }

  /**
   * Execute a memory-recall node — search long-term memory via IPC.
   */
  async function executeMemoryRecallNode(
    node: FlowNode,
    upstreamInput: string,
    config: NodeExecConfig,
  ): Promise<string> {
    const limit = config.memoryLimit ?? 5;
    const agentId = config.memoryAgentId;
    const outputFormat = config.memoryOutputFormat ?? 'text';

    // Determine the search query
    let query: string;
    if (config.memoryQuerySource === 'custom' && config.memoryQuery) {
      query = config.memoryQuery;
    } else {
      query = upstreamInput || node.label;
    }

    if (!query.trim()) {
      return outputFormat === 'json' ? '[]' : 'No query provided for memory recall.';
    }

    callbacks.onEvent({
      type: 'step-progress' as FlowExecEvent['type'],
      runId: _runState!.runId,
      nodeId: node.id,
      delta: `Searching memory: "${query.slice(0, 50)}"…`,
    } as FlowExecEvent);

    try {
      const results = await pawEngine.memorySearch(query, limit, agentId);

      if (!results || results.length === 0) {
        return outputFormat === 'json' ? '[]' : 'No relevant memories found.';
      }

      // Filter by threshold
      const threshold = config.memoryThreshold ?? 0.3;
      const filtered = results.filter((m: { score?: number }) => (m.score ?? 1) >= threshold);

      if (outputFormat === 'json') {
        return JSON.stringify(filtered, null, 2);
      }

      // Text format: numbered list
      return filtered
        .map((m: { content: string; category?: string; score?: number }, i: number) => {
          const scoreStr = m.score !== undefined ? ` (${(m.score * 100).toFixed(0)}% match)` : '';
          return `${i + 1}. [${m.category ?? 'memory'}] ${m.content}${scoreStr}`;
        })
        .join('\n');
    } catch (err) {
      throw new Error(`Memory search failed: ${err instanceof Error ? err.message : String(err)}`);
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
    _runState = createFlowRunState(graph.id, plan, graph.variables);
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
    getLastStrategy: () => _lastStrategy,
  };
}
