// ─────────────────────────────────────────────────────────────────────────────
// Flow Simulation Engine — Runtime ("Holodeck")
// Executes flow graphs against mock dependencies so agents/sub-agents
// believe they are interacting with real services.
//
// Architecture:
//   1. Intercepts all IPC calls (engineChatSend, pawEngine, subscribeSession)
//   2. Routes them through the mock config from the scenario
//   3. Runs the real Conductor strategy compiler + executor logic
//   4. Collects events, mock call logs, and run state
//   5. Evaluates expectations and returns a SimResult
//
// The key insight: we DON'T create a separate executor. We use the real
// executor-atoms (plan builder, condition evaluator, variable resolver,
// code sandbox) and only mock the external boundaries (LLM, HTTP, MCP).
// This means the simulation tests EXACTLY the same code paths that run
// in production — agents genuinely don't know they're in a simulation.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowGraph, FlowNode, FlowStatus } from '../atoms';
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
} from '../executor-atoms';
import {
  compileStrategy,
  shouldUseConductor,
  parseCollapsedOutput,
  checkConvergence,
  type ExecutionStrategy,
  type ExecutionUnit,
} from '../conductor-atoms';

import {
  type SimScenario,
  type SimResult,
  type SimMockConfig,
  type MockCallContext,
  type MockCallLog,
  resolveMockResponse,
  evaluateExpectations,
} from './simulation-atoms';

// ── Holodeck Runtime ───────────────────────────────────────────────────────

/**
 * Run a single simulation scenario.
 * This is the main entry point. No real IPC calls are made.
 */
export async function runSimulation(scenario: SimScenario): Promise<SimResult> {
  const startTime = Date.now();
  const events: FlowExecEvent[] = [];
  const mockCalls: MockCallLog[] = [];
  const callCounts = new Map<string, number>();

  // Deep-clone the graph so we don't mutate the scenario definition
  const graph: FlowGraph = JSON.parse(JSON.stringify(scenario.graph));
  const mocks = scenario.mocks;

  let runState: FlowRunState | null = null;
  let strategy: ExecutionStrategy | null = null;
  let error: string | undefined;

  const onEvent = (event: FlowExecEvent) => {
    events.push(event);
  };

  const nodeStatusChanges = new Map<string, FlowStatus>();
  const onNodeStatusChange = (nodeId: string, status: string) => {
    nodeStatusChanges.set(nodeId, status as FlowStatus);
  };

  const activeEdges = new Set<string>();
  const onEdgeActive = (edgeId: string, active: boolean) => {
    if (active) activeEdges.add(edgeId);
    else activeEdges.delete(edgeId);
  };

  try {
    // Validate graph
    const validationErrors = validateFlowForExecution(graph);
    const blocking = validationErrors.filter((e) => e.message.includes('no nodes'));
    if (blocking.length > 0) {
      throw new Error(blocking[0].message);
    }

    // Build execution plan
    const plan = buildExecutionPlan(graph);

    // Create run state
    runState = createFlowRunState(
      graph.id,
      plan,
      { ...scenario.initialVariables, ...graph.variables },
      scenario.vaultCredentials ?? {},
    );
    runState.startedAt = Date.now();
    runState.status = 'running';

    onEvent({
      type: 'run-start',
      runId: runState.runId,
      graphName: graph.name,
      totalSteps: plan.length,
    });

    // Track skipped nodes (condition branching)
    const skipNodes = new Set<string>();

    // ── Mock agent step: the core mock interceptor ─────────────────────
    async function mockAgentStep(
      node: FlowNode,
      upstreamInput: string,
      config: NodeExecConfig,
      agentId: string,
    ): Promise<string> {
      const count = callCounts.get(node.id) ?? 0;
      callCounts.set(node.id, count + 1);

      const prompt = buildNodePrompt(node, upstreamInput, config);

      const ctx: MockCallContext = {
        node,
        config,
        upstreamInput,
        prompt,
        agentId,
        runState: runState!,
        callCount: count,
        graph,
      };

      // Simulate latency
      const latency = resolveLatency(mocks, node.id);
      if (latency > 0) {
        await delay(latency);
      }

      const mock = resolveMockResponse(mocks, ctx);
      const callStart = Date.now();

      // Simulate streaming if enabled
      if (mocks.simulateStreaming && !mock.failed) {
        const chunkSize = mocks.streamingChunkSize ?? 20;
        const chunkDelay = mocks.streamingDelayMs ?? 30;
        for (let i = 0; i < mock.response.length; i += chunkSize) {
          const chunk = mock.response.slice(i, i + chunkSize);
          onEvent({
            type: 'step-progress',
            runId: runState!.runId,
            nodeId: node.id,
            delta: chunk,
          });
          if (chunkDelay > 0) await delay(chunkDelay);
        }
      }

      // Log the mock call
      mockCalls.push({
        type: 'agent',
        nodeId: node.id,
        nodeLabel: node.label,
        input: prompt,
        output: mock.response,
        failed: mock.failed,
        error: mock.error,
        durationMs: Date.now() - callStart + latency,
        timestamp: Date.now(),
      });

      if (mock.failed) {
        throw new Error(mock.error || 'Simulated failure');
      }

      return mock.response;
    }

    // ── Mock HTTP handler ──────────────────────────────────────────────
    async function mockHttpRequest(node: FlowNode, config: NodeExecConfig): Promise<string> {
      const url = config.httpUrl || '';
      const method = config.httpMethod || 'GET';

      // Find matching HTTP mock
      if (mocks.httpMocks) {
        for (const rule of mocks.httpMocks) {
          const matches =
            rule.urlPattern instanceof RegExp
              ? rule.urlPattern.test(url)
              : url.includes(rule.urlPattern);
          const methodOk = !rule.method || rule.method === method;

          if (matches && methodOk) {
            if (rule.latencyMs) await delay(rule.latencyMs);
            mockCalls.push({
              type: 'http',
              nodeId: node.id,
              nodeLabel: node.label,
              input: `${method} ${url}`,
              output: rule.body,
              failed: rule.status >= 400,
              error: rule.status >= 400 ? `HTTP ${rule.status}` : undefined,
              durationMs: rule.latencyMs ?? 0,
              timestamp: Date.now(),
            });
            if (rule.status >= 400) {
              throw new Error(`HTTP ${rule.status}: ${rule.body}`);
            }
            return rule.body;
          }
        }
      }

      // Default: return a mock 200 OK
      const body = JSON.stringify({ status: 'ok', url, method, simulated: true });
      mockCalls.push({
        type: 'http',
        nodeId: node.id,
        nodeLabel: node.label,
        input: `${method} ${url}`,
        output: body,
        failed: false,
        durationMs: 0,
        timestamp: Date.now(),
      });
      return body;
    }

    // ── Mock MCP handler ───────────────────────────────────────────────
    async function mockMcpCall(node: FlowNode, config: NodeExecConfig): Promise<string> {
      const toolName = config.mcpToolName || '';
      const mockResp = mocks.mcpMocks?.[toolName];

      if (mockResp) {
        mockCalls.push({
          type: 'mcp',
          nodeId: node.id,
          nodeLabel: node.label,
          input: `${toolName}(${config.mcpToolArgs || '{}'})`,
          output: mockResp.content,
          failed: !mockResp.success,
          error: mockResp.error,
          durationMs: 0,
          timestamp: Date.now(),
        });
        if (!mockResp.success) {
          throw new Error(mockResp.error || `MCP tool "${toolName}" failed`);
        }
        return mockResp.content;
      }

      // Default
      const output = JSON.stringify({ tool: toolName, result: 'ok', simulated: true });
      mockCalls.push({
        type: 'mcp',
        nodeId: node.id,
        nodeLabel: node.label,
        input: `${toolName}(${config.mcpToolArgs || '{}'})`,
        output,
        failed: false,
        durationMs: 0,
        timestamp: Date.now(),
      });
      return output;
    }

    // ── Mock Memory handlers ───────────────────────────────────────────
    async function mockMemoryWrite(
      node: FlowNode,
      input: string,
      config: NodeExecConfig,
    ): Promise<string> {
      const content = config.memorySource === 'custom' ? config.memoryContent || input : input;
      mockCalls.push({
        type: 'memory-write',
        nodeId: node.id,
        nodeLabel: node.label,
        input: content,
        output: 'Memory stored.',
        failed: false,
        durationMs: 0,
        timestamp: Date.now(),
      });
      return `Memory stored (category: ${config.memoryCategory || 'insight'}, importance: ${config.memoryImportance ?? 0.5})`;
    }

    async function mockMemoryRecall(
      node: FlowNode,
      input: string,
      config: NodeExecConfig,
    ): Promise<string> {
      const query = config.memoryQuerySource === 'custom' ? config.memoryQuery || input : input;
      const memConfig = mocks.memoryMocks;

      let results: string[] = [];
      if (memConfig?.memories) {
        // Match by keywords
        results = memConfig.memories
          .filter((m) => m.keywords.some((k) => query.toLowerCase().includes(k.toLowerCase())))
          .filter((m) => m.relevance >= (config.memoryThreshold ?? 0.3))
          .sort((a, b) => b.relevance - a.relevance)
          .slice(0, config.memoryLimit ?? 5)
          .map((m) => m.content);
      }

      const output =
        results.length > 0
          ? config.memoryOutputFormat === 'json'
            ? JSON.stringify(results)
            : results.join('\n\n---\n\n')
          : (memConfig?.defaultRecallResponse ?? 'No matching memories found.');

      mockCalls.push({
        type: 'memory-recall',
        nodeId: node.id,
        nodeLabel: node.label,
        input: query,
        output,
        failed: false,
        durationMs: 0,
        timestamp: Date.now(),
      });
      return output;
    }

    // ── Mock Squad handler ─────────────────────────────────────────────
    async function mockSquadTask(
      node: FlowNode,
      input: string,
      config: NodeExecConfig,
    ): Promise<string> {
      const ctx: MockCallContext = {
        node,
        config,
        upstreamInput: input,
        prompt: config.squadObjective || node.label,
        agentId: 'squad',
        runState: runState!,
        callCount: 0,
        graph,
      };
      const resp = resolveMockResponse(mocks, ctx);
      mockCalls.push({
        type: 'squad',
        nodeId: node.id,
        nodeLabel: node.label,
        input: config.squadObjective || input,
        output: resp.response,
        failed: resp.failed,
        error: resp.error,
        durationMs: 0,
        timestamp: Date.now(),
      });
      if (resp.failed) throw new Error(resp.error);
      return resp.response;
    }

    // ── Node Executor (mirrors real executor.ts logic) ─────────────────
    async function executeNode(node: FlowNode, defaultAgentId?: string): Promise<void> {
      if (!runState) return;

      const config = getNodeExecConfig(node);
      const nodeState = createNodeRunState(node.id);
      nodeState.startedAt = Date.now();
      nodeState.status = 'running';
      runState.nodeStates.set(node.id, nodeState);

      node.status = 'running';
      onNodeStatusChange(node.id, 'running');

      const inEdges = graph.edges.filter((e) => e.to === node.id);
      for (const e of inEdges) {
        e.active = true;
        onEdgeActive(e.id, true);
      }

      onEvent({
        type: 'step-start',
        runId: runState.runId,
        stepIndex: runState.currentStep,
        nodeId: node.id,
        nodeLabel: node.label,
        nodeKind: node.kind,
      });

      try {
        let upstreamInput = collectNodeInput(graph, node.id, runState.nodeStates);
        upstreamInput = resolveVariables(upstreamInput, {
          input: upstreamInput,
          variables: runState.variables,
          vaultCredentials: runState.vaultCredentials,
        });
        nodeState.input = upstreamInput;

        let output: string;
        const agentId = config.agentId || defaultAgentId || 'default';

        switch (node.kind) {
          case 'trigger':
            output = config.prompt || upstreamInput || `Flow "${graph.name}" started.`;
            break;

          case 'output':
            output = upstreamInput || 'No output.';
            break;

          case 'condition': {
            // Check for forced condition result
            const nodeOverride = mocks.nodeOverrides?.[node.id];
            if (nodeOverride?.forceConditionResult !== undefined) {
              output = nodeOverride.forceConditionResult ? 'true' : 'false';
            } else {
              output = await mockAgentStep(node, upstreamInput, config, agentId);
            }
            handleConditionResult(node, output);
            break;
          }

          case 'code': {
            // Check for code output override
            const codeOverride = mocks.nodeOverrides?.[node.id];
            if (codeOverride?.codeOutput !== undefined) {
              output = codeOverride.codeOutput;
            } else {
              const codeSource = (node.config.code as string) ?? config.prompt ?? '';
              if (!codeSource.trim()) {
                output = 'No code to execute.';
              } else {
                const codeResult = executeCodeSandboxed(
                  codeSource,
                  upstreamInput,
                  config.timeoutMs ?? 5000,
                );
                if (codeResult.error) throw new Error(`Code error: ${codeResult.error}`);
                output = codeResult.output;
              }
            }
            break;
          }

          case 'error': {
            const targets = config.errorTargets ?? ['log'];
            const errorInfo = upstreamInput || 'Unknown error';
            const parts: string[] = [];
            if (targets.includes('log')) parts.push('Logged');
            if (targets.includes('toast')) parts.push('Toast sent');
            if (targets.includes('chat')) parts.push('Chat notified');
            output = `Error handled (${parts.join(', ')}): ${errorInfo}`;
            break;
          }

          case 'http':
            output = await mockHttpRequest(node, config);
            break;

          case 'mcp-tool':
            output = await mockMcpCall(node, config);
            break;

          case 'loop':
            output = await executeLoop(node, upstreamInput, config, agentId);
            break;

          case 'memory':
            output = await mockMemoryWrite(node, upstreamInput, config);
            break;

          case 'memory-recall':
            output = await mockMemoryRecall(node, upstreamInput, config);
            break;

          case 'squad':
            output = await mockSquadTask(node, upstreamInput, config);
            break;

          case 'event-horizon':
            // Event horizons are sync barriers — pass through accumulated data
            output = upstreamInput || 'Event horizon reached.';
            break;

          case 'agent':
          case 'tool':
          case 'data':
          default:
            output = await mockAgentStep(node, upstreamInput, config, agentId);
            break;
        }

        // Success
        nodeState.output = output;
        nodeState.status = 'success';
        nodeState.finishedAt = Date.now();
        nodeState.durationMs = nodeState.finishedAt - nodeState.startedAt;
        node.status = 'success';
        onNodeStatusChange(node.id, 'success');

        // Set flow variable if configured
        if (config.setVariableKey && runState) {
          runState.variables[config.setVariableKey] = config.setVariable
            ? resolveVariables(config.setVariable, {
                input: output,
                variables: runState.variables,
                vaultCredentials: runState.vaultCredentials,
              })
            : output;
        }

        onEvent({
          type: 'step-complete',
          runId: runState.runId,
          nodeId: node.id,
          output,
          durationMs: nodeState.durationMs,
        });

        runState.outputLog.push({
          nodeId: node.id,
          nodeLabel: node.label,
          nodeKind: node.kind,
          status: 'success',
          output,
          durationMs: nodeState.durationMs,
          timestamp: Date.now(),
        });

        // Record edge values
        recordEdgeValues(node.id);
      } catch (err) {
        const errorMsg = err instanceof Error ? err.message : String(err);

        // Retry logic (mirrors real executor)
        const maxRetries = config.maxRetries ?? 0;
        let retried = false;

        if (maxRetries > 0) {
          for (let attempt = 1; attempt <= maxRetries; attempt++) {
            const retryDelay =
              (config.retryDelayMs ?? 1000) * Math.pow(config.retryBackoff ?? 2, attempt - 1);
            // In simulation, we use minimal delay
            await delay(Math.min(retryDelay, 50));

            try {
              const agentId = config.agentId || defaultAgentId || 'default';
              let retryOutput: string;

              if (node.kind === 'code') {
                const src = (node.config.code as string) ?? '';
                const result = executeCodeSandboxed(src, nodeState.input, config.timeoutMs ?? 5000);
                if (result.error) throw new Error(result.error);
                retryOutput = result.output;
              } else {
                retryOutput = await mockAgentStep(node, nodeState.input, config, agentId);
              }

              nodeState.output = retryOutput;
              nodeState.status = 'success';
              nodeState.finishedAt = Date.now();
              nodeState.durationMs = nodeState.finishedAt - nodeState.startedAt;
              node.status = 'success';
              onNodeStatusChange(node.id, 'success');
              onEvent({
                type: 'step-complete',
                runId: runState.runId,
                nodeId: node.id,
                output: retryOutput,
                durationMs: nodeState.durationMs,
              });
              retried = true;
              break;
            } catch {
              // Continue to next retry
            }
          }
        }

        if (!retried) {
          nodeState.status = 'error';
          nodeState.error = errorMsg;
          nodeState.finishedAt = Date.now();
          nodeState.durationMs = nodeState.finishedAt - nodeState.startedAt;
          node.status = 'error';
          onNodeStatusChange(node.id, 'error');

          onEvent({
            type: 'step-error',
            runId: runState.runId,
            nodeId: node.id,
            error: errorMsg,
            durationMs: nodeState.durationMs,
          });

          runState.outputLog.push({
            nodeId: node.id,
            nodeLabel: node.label,
            nodeKind: node.kind,
            status: 'error',
            output: '',
            error: errorMsg,
            durationMs: nodeState.durationMs,
            timestamp: Date.now(),
          });

          // Error edge routing
          const errorEdges = graph.edges.filter(
            (e) => e.from === node.id && (e.kind === 'error' || e.fromPort === 'err'),
          );
          const errorTargetIds = new Set(errorEdges.map((e) => e.to));

          if (errorEdges.length > 0) {
            const errorPayload = JSON.stringify({
              error: errorMsg,
              nodeId: node.id,
              nodeLabel: node.label,
            });
            runState.nodeStates.set(node.id, { ...nodeState, output: errorPayload });
          }

          const successEdges = graph.edges.filter(
            (e) => e.from === node.id && e.kind !== 'error' && e.fromPort !== 'err',
          );
          for (const e of successEdges) skipNodes.add(e.to);
          for (const id of errorTargetIds) skipNodes.delete(id);
        }
      } finally {
        for (const e of inEdges) {
          e.active = false;
          onEdgeActive(e.id, false);
        }
      }
    }

    // ── Condition handling ──────────────────────────────────────────────
    function handleConditionResult(condNode: FlowNode, response: string): void {
      const result = evaluateCondition(response);
      const activeEdges = resolveConditionEdges(graph, condNode.id, result);
      const activeTargets = new Set(activeEdges.map((e) => e.to));
      const allDownstream = graph.edges.filter((e) => e.from === condNode.id).map((e) => e.to);
      for (const targetId of allDownstream) {
        if (!activeTargets.has(targetId)) {
          skipNodes.add(targetId);
          skipSubtree(targetId);
        }
      }
    }

    function skipSubtree(nodeId: string): void {
      const downstream = graph.edges.filter((e) => e.from === nodeId).map((e) => e.to);
      for (const dId of downstream) {
        if (!skipNodes.has(dId)) {
          skipNodes.add(dId);
          skipSubtree(dId);
        }
      }
    }

    function recordEdgeValues(nodeId: string): void {
      if (!runState) return;
      const nodeState = runState.nodeStates.get(nodeId);
      if (!nodeState?.output) return;
      const truncated =
        nodeState.output.length > 80 ? `${nodeState.output.slice(0, 77)}…` : nodeState.output;
      const outEdges = graph.edges.filter((e) => e.from === nodeId);
      for (const edge of outEdges) {
        onEvent({
          type: 'debug-edge-value',
          runId: runState.runId,
          edgeId: edge.id,
          value: truncated,
        });
      }
    }

    // ── Loop handler ───────────────────────────────────────────────────
    async function executeLoop(
      node: FlowNode,
      input: string,
      config: NodeExecConfig,
      agentId: string,
    ): Promise<string> {
      let items: unknown[];
      try {
        items = JSON.parse(input);
        if (!Array.isArray(items)) items = [items];
      } catch {
        items = input
          .split('\n')
          .map((s) => s.trim())
          .filter(Boolean);
      }

      const maxIter = config.loopMaxIterations ?? 100;
      const results: string[] = [];

      for (let i = 0; i < Math.min(items.length, maxIter); i++) {
        const itemStr =
          typeof items[i] === 'string' ? (items[i] as string) : JSON.stringify(items[i]);
        const iterOutput = await mockAgentStep(
          node,
          itemStr,
          { ...config, prompt: `[Loop iteration ${i + 1}/${items.length}]\n${itemStr}` },
          agentId,
        );
        results.push(iterOutput);
      }

      return results.join('\n\n---\n\n');
    }

    // ── Execution path: sequential or Conductor ────────────────────────
    if (shouldUseConductor(graph)) {
      try {
        strategy = compileStrategy(graph);
        onEvent({
          type: 'run-start',
          runId: runState.runId,
          graphName: `${graph.name} [Conductor: ${strategy.meta.collapseGroups} collapse, ${strategy.meta.parallelPhases} parallel, ${strategy.meta.extractedNodes} extracted]`,
          totalSteps: strategy.phases.length,
        });
        await runConductorSim(
          strategy,
          graph,
          skipNodes,
          runState,
          onEvent,
          executeNode,
          mockAgentStep,
          callCounts,
          mocks,
          mockCalls,
        );
      } catch (conductorErr) {
        // Conductor failed — fall back to sequential
        console.warn('[sim-conductor] Strategy failed, falling back:', conductorErr);
        strategy = null;
        skipNodes.clear();
        for (const node of graph.nodes) {
          if (node.status !== 'success') {
            node.status = 'idle';
            onNodeStatusChange(node.id, 'idle');
          }
        }
        await runSequentialSim(plan, graph, skipNodes, runState, onEvent, executeNode);
      }
    } else {
      await runSequentialSim(plan, graph, skipNodes, runState, onEvent, executeNode);
    }

    // Finalize
    runState.finishedAt = Date.now();
    runState.totalDurationMs = runState.finishedAt - runState.startedAt;
    if (runState.status === 'running') {
      runState.status = 'success';
    }

    onEvent({
      type: 'run-complete',
      runId: runState.runId,
      status: runState.status,
      totalDurationMs: runState.totalDurationMs,
      outputLog: runState.outputLog,
    });
  } catch (err) {
    error = err instanceof Error ? err.message : String(err);
    if (runState) {
      runState.status = 'error';
      runState.finishedAt = Date.now();
      runState.totalDurationMs = runState.finishedAt - runState.startedAt;
    }
  }

  // Build result
  const result: SimResult = {
    scenarioId: scenario.id,
    scenarioName: scenario.name,
    passed: false, // Will be set after evaluation
    expectationResults: [],
    runState,
    strategy,
    events,
    mockCalls,
    durationMs: Date.now() - startTime,
    error,
    timestamp: Date.now(),
  };

  // Evaluate expectations
  result.expectationResults = evaluateExpectations(scenario.expectations, result);
  result.passed = result.expectationResults.every((r) => r.passed) && !error;

  return result;
}

// ── Sequential Execution ─────────────────────────────────────────────────

async function runSequentialSim(
  plan: string[],
  graph: FlowGraph,
  skipNodes: Set<string>,
  runState: FlowRunState,
  _onEvent: (event: FlowExecEvent) => void,
  executeNode: (node: FlowNode, agentId?: string) => Promise<void>,
): Promise<void> {
  for (let i = 0; i < plan.length; i++) {
    const nodeId = plan[i];
    if (skipNodes.has(nodeId)) continue;

    runState.currentStep = i;
    const node = graph.nodes.find((n) => n.id === nodeId);
    if (!node) continue;

    await executeNode(node);
  }
}

// ── Conductor Strategy Execution (Simulated) ─────────────────────────────

async function runConductorSim(
  strategy: ExecutionStrategy,
  graph: FlowGraph,
  skipNodes: Set<string>,
  runState: FlowRunState,
  onEvent: (event: FlowExecEvent) => void,
  executeNode: (node: FlowNode, agentId?: string) => Promise<void>,
  mockAgentStep: (
    node: FlowNode,
    input: string,
    config: NodeExecConfig,
    agentId: string,
  ) => Promise<string>,
  callCounts: Map<string, number>,
  mocks: SimMockConfig,
  mockCalls: MockCallLog[],
): Promise<void> {
  const nodeMap = new Map(graph.nodes.map((n) => [n.id, n]));

  for (const phase of strategy.phases) {
    // Units within a phase run "in parallel" (we simulate with Promise.all)
    if (phase.units.length === 1) {
      await executeConductorUnitSim(
        phase.units[0],
        graph,
        nodeMap,
        skipNodes,
        runState,
        onEvent,
        executeNode,
        mockAgentStep,
        callCounts,
        mocks,
        mockCalls,
      );
    } else {
      await Promise.all(
        phase.units.map((unit) =>
          executeConductorUnitSim(
            unit,
            graph,
            nodeMap,
            skipNodes,
            runState,
            onEvent,
            executeNode,
            mockAgentStep,
            callCounts,
            mocks,
            mockCalls,
          ),
        ),
      );
    }
  }
}

async function executeConductorUnitSim(
  unit: ExecutionUnit,
  graph: FlowGraph,
  nodeMap: Map<string, FlowNode>,
  skipNodes: Set<string>,
  runState: FlowRunState,
  onEvent: (event: FlowExecEvent) => void,
  executeNode: (node: FlowNode, agentId?: string) => Promise<void>,
  mockAgentStep: (
    node: FlowNode,
    input: string,
    config: NodeExecConfig,
    agentId: string,
  ) => Promise<string>,
  callCounts: Map<string, number>,
  mocks: SimMockConfig,
  mockCalls: MockCallLog[],
): Promise<void> {
  switch (unit.type) {
    case 'collapsed-agent':
      await executeCollapsedUnitSim(
        unit,
        graph,
        nodeMap,
        runState,
        onEvent,
        mockAgentStep,
        callCounts,
        mocks,
        mockCalls,
      );
      break;

    case 'mesh':
      await executeMeshSim(
        unit,
        graph,
        nodeMap,
        runState,
        onEvent,
        mockAgentStep,
        callCounts,
        mocks,
        mockCalls,
      );
      break;

    case 'single-agent':
    case 'single-direct':
    case 'direct-action':
      for (const nodeId of unit.nodeIds) {
        if (skipNodes.has(nodeId)) continue;
        const node = nodeMap.get(nodeId);
        if (!node) continue;
        await executeNode(node);
      }
      break;
  }
}

async function executeCollapsedUnitSim(
  unit: ExecutionUnit,
  graph: FlowGraph,
  nodeMap: Map<string, FlowNode>,
  runState: FlowRunState,
  onEvent: (event: FlowExecEvent) => void,
  mockAgentStep: (
    node: FlowNode,
    input: string,
    config: NodeExecConfig,
    agentId: string,
  ) => Promise<string>,
  _callCounts: Map<string, number>,
  _mocks: SimMockConfig,
  _mockCalls: MockCallLog[],
): Promise<void> {
  if (!unit.mergedPrompt) return;

  const firstNodeId = unit.nodeIds[0];
  const firstNode = nodeMap.get(firstNodeId);
  if (!firstNode) return;

  // Mark all nodes running
  for (const nodeId of unit.nodeIds) {
    const node = nodeMap.get(nodeId);
    if (node) {
      node.status = 'running';
    }
  }

  const upstreamInput = collectNodeInput(graph, firstNodeId, runState.nodeStates);
  const config = getNodeExecConfig(firstNode);

  onEvent({
    type: 'step-start',
    runId: runState.runId,
    stepIndex: runState.currentStep,
    nodeId: firstNodeId,
    nodeLabel: `Collapsed: ${unit.nodeIds.length} steps`,
    nodeKind: 'agent',
  });

  const startTime = Date.now();

  try {
    // Generate combined response — use mock for collapsed prompt
    const prompt = upstreamInput
      ? `[Previous step output]\n${upstreamInput}\n\n${unit.mergedPrompt}`
      : unit.mergedPrompt;

    const output = await mockAgentStep(
      firstNode,
      upstreamInput,
      { ...config, prompt },
      config.agentId || 'default',
    );

    const durationMs = Date.now() - startTime;
    const stepOutputs = parseCollapsedOutput(output, unit.nodeIds.length);

    // Record state for each node
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

      onEvent({
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
    }
  } catch (err) {
    const errorMsg = err instanceof Error ? err.message : String(err);
    for (const nodeId of unit.nodeIds) {
      const node = nodeMap.get(nodeId);
      if (!node) continue;
      const nodeState = createNodeRunState(nodeId);
      nodeState.status = 'error';
      nodeState.error = errorMsg;
      runState.nodeStates.set(nodeId, nodeState);
      node.status = 'error';
    }
    onEvent({
      type: 'step-error',
      runId: runState.runId,
      nodeId: firstNodeId,
      error: errorMsg,
      durationMs: Date.now() - startTime,
    });
  }
}

async function executeMeshSim(
  unit: ExecutionUnit,
  _graph: FlowGraph,
  nodeMap: Map<string, FlowNode>,
  runState: FlowRunState,
  onEvent: (event: FlowExecEvent) => void,
  mockAgentStep: (
    node: FlowNode,
    input: string,
    config: NodeExecConfig,
    agentId: string,
  ) => Promise<string>,
  _callCounts: Map<string, number>,
  _mocks: SimMockConfig,
  _mockCalls: MockCallLog[],
): Promise<void> {
  const maxIterations = unit.maxIterations ?? 5;
  const convergenceThreshold = 0.85;
  let prevOutputs = new Map<string, string>();
  const meshContext: string[] = [];

  for (const nodeId of unit.nodeIds) {
    const node = nodeMap.get(nodeId);
    if (node) node.status = 'running';
  }

  for (let round = 1; round <= maxIterations; round++) {
    const currOutputs = new Map<string, string>();

    for (const nodeId of unit.nodeIds) {
      const node = nodeMap.get(nodeId);
      if (!node) continue;

      const config = getNodeExecConfig(node);
      const contextParts = [`[Convergent Mesh — Round ${round}/${maxIterations}]`];
      if (meshContext.length > 0) {
        contextParts.push('[Previous round outputs]');
        contextParts.push(meshContext.join('\n---\n'));
      }
      const upstreamInput = contextParts.join('\n\n');

      const output = await mockAgentStep(node, upstreamInput, config, config.agentId || 'default');
      currOutputs.set(nodeId, output);

      const nodeState = createNodeRunState(nodeId);
      nodeState.output = output;
      nodeState.status = 'success';
      nodeState.startedAt = Date.now();
      nodeState.finishedAt = Date.now();
      runState.nodeStates.set(nodeId, nodeState);

      onEvent({
        type: 'step-progress',
        runId: runState.runId,
        nodeId,
        delta: `[Round ${round}] ${output.slice(0, 100)}`,
      });
    }

    // Build context for next round
    meshContext.length = 0;
    for (const [nodeId, output] of currOutputs) {
      const node = nodeMap.get(nodeId);
      meshContext.push(`${node?.label ?? nodeId}: ${output}`);
    }

    // Check convergence using the real algorithm
    if (checkConvergence(prevOutputs, currOutputs, convergenceThreshold)) {
      break;
    }
    prevOutputs = currOutputs;
  }

  // Mark complete
  for (const nodeId of unit.nodeIds) {
    const node = nodeMap.get(nodeId);
    if (node) {
      node.status = 'success';
      const nodeState = runState.nodeStates.get(nodeId);
      if (nodeState) {
        onEvent({
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
    }
  }
}

// ── Suite Runner ──────────────────────────────────────────────────────────

import type { SimSuite, SimSuiteResult } from './simulation-atoms'; // eslint-disable-line no-duplicate-imports

/**
 * Run all scenarios in a suite.
 * Results are collected and summarized.
 */
export async function runSimSuite(suite: SimSuite): Promise<SimSuiteResult> {
  const startTime = Date.now();
  const results: SimResult[] = [];

  for (const scenario of suite.scenarios) {
    // Merge global mocks with scenario mocks (scenario overrides global)
    const mergedScenario: SimScenario = {
      ...scenario,
      mocks: {
        ...suite.globalMocks,
        ...scenario.mocks,
        nodeOverrides: {
          ...suite.globalMocks?.nodeOverrides,
          ...scenario.mocks.nodeOverrides,
        },
      },
    };

    const result = await runSimulation(mergedScenario);
    results.push(result);
  }

  return {
    suiteId: suite.id,
    suiteName: suite.name,
    results,
    totalScenarios: suite.scenarios.length,
    passed: results.filter((r) => r.passed).length,
    failed: results.filter((r) => !r.passed).length,
    durationMs: Date.now() - startTime,
    timestamp: Date.now(),
  };
}

// ── Utilities ────────────────────────────────────────────────────────────

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function resolveLatency(mocks: SimMockConfig, nodeId: string): number {
  // Per-node override takes priority
  const nodeOverride = mocks.nodeOverrides?.[nodeId];
  if (nodeOverride?.latencyMs !== undefined) return nodeOverride.latencyMs;

  // Global latency + jitter
  const base = mocks.latencyMs ?? 0;
  const jitter = mocks.latencyJitterMs ?? 0;
  if (jitter > 0) {
    return base + Math.floor(Math.random() * jitter * 2) - jitter;
  }
  return base;
}
