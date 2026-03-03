// ─────────────────────────────────────────────────────────────────────────────
// Flow Simulation Engine — Atoms (Pure Types & Data)
// "Holodeck Mode" — agents think they're real, but everything is mocked.
//
// This module provides the type system and core data structures for the
// simulation runtime. No DOM, no IPC — fully testable.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowGraph, FlowNode, FlowNodeKind, FlowEdge, FlowStatus } from '../atoms';
import type { FlowRunState, FlowExecEvent, NodeExecConfig } from '../executor-atoms';
import type { ExecutionStrategy } from '../conductor-atoms';

// Re-export executor types used by tests and scenarios
export type { FlowRunState } from '../executor-atoms';

// ── Scenario Definition ────────────────────────────────────────────────────

/**
 * A simulation scenario: a flow graph + mock behaviors + assertions.
 * Think of it as a "holodeck program" — the agents run the flow for real,
 * but every external dependency (LLM, HTTP, MCP, memory) is intercepted
 * by configurable mock responders.
 */
export interface SimScenario {
  /** Unique scenario ID */
  id: string;
  /** Human-readable name */
  name: string;
  /** What this scenario tests */
  description: string;
  /** Category for organization */
  category: SimCategory;
  /** Difficulty / complexity tier */
  tier: SimTier;
  /** The flow graph to execute */
  graph: FlowGraph;
  /** Mock behaviors: how each node kind / specific node should respond */
  mocks: SimMockConfig;
  /** Expected outcomes to assert against */
  expectations: SimExpectation[];
  /** Simulated vault credentials */
  vaultCredentials?: Record<string, string>;
  /** Simulated flow variables */
  initialVariables?: Record<string, unknown>;
  /** Max wall-clock time for the whole scenario (ms) */
  timeoutMs?: number;
  /** Tags for filtering */
  tags?: string[];
}

export type SimCategory =
  | 'basic' // Simple linear flows
  | 'branching' // Condition nodes, fan-out / fan-in
  | 'parallel' // Conductor parallel execution
  | 'collapse' // Conductor collapse chains
  | 'convergent' // Convergent mesh / cyclic flows
  | 'tesseract' // 4D hyper-dimensional flows
  | 'self-healing' // Error recovery & retry flows
  | 'orchestrator' // Boss/worker multi-agent
  | 'sub-flow' // Group nodes with embedded sub-flows
  | 'integration'; // Full integration scenarios (HTTP, MCP, memory)

export type SimTier =
  | 'smoke' // Quick sanity check (< 5 nodes)
  | 'standard' // Normal complexity (5–15 nodes)
  | 'complex' // Advanced flows (15–50 nodes)
  | 'extreme'; // Tesseract / deep orchestration (50+ nodes)

// ── Mock Configuration ─────────────────────────────────────────────────────

/**
 * How the simulation should handle each type of external call.
 * Mocks can be configured at the kind level (all agent nodes) or
 * per specific node ID (overrides kind-level config).
 */
export interface SimMockConfig {
  /** Default mock for agent/tool LLM calls. Applied to all agent-classified nodes. */
  agentDefault?: MockAgentBehavior;
  /** Per-node overrides (node ID → behavior) */
  nodeOverrides?: Record<string, MockNodeBehavior>;
  /** Global latency simulation (ms). Each mock call is delayed by this. */
  latencyMs?: number;
  /** Random latency jitter (± ms). Added to latencyMs. */
  latencyJitterMs?: number;
  /** Probability of random failure (0–1). Useful for chaos testing. */
  failureRate?: number;
  /** Error message to use for random failures */
  failureMessage?: string;
  /** HTTP mock responses (URL pattern → response) */
  httpMocks?: MockHttpRule[];
  /** MCP tool mock responses (tool name → response) */
  mcpMocks?: Record<string, MockMcpResponse>;
  /** Memory mock: what memory-recall returns */
  memoryMocks?: MockMemoryConfig;
  /** Whether to simulate streaming (emit deltas over time) */
  simulateStreaming?: boolean;
  /** Streaming chunk size in characters (default: 20) */
  streamingChunkSize?: number;
  /** Delay between streaming chunks in ms (default: 30) */
  streamingDelayMs?: number;
}

/** Behavior for mocked agent/LLM calls. */
export interface MockAgentBehavior {
  /** Strategy for generating responses */
  strategy: MockResponseStrategy;
  /** Static response text (for 'static' strategy) */
  response?: string;
  /** Response templates keyed by node label or prompt keywords */
  responses?: Record<string, string>;
  /** For 'echo' strategy: prefix added before echoing input */
  echoPrefix?: string;
  /** For 'sequence' strategy: responses given in order, wrapping around */
  sequence?: string[];
  /** For 'function' strategy: custom response generator */
  generator?: MockResponseGenerator;
  /** Model name to report in events (default: 'sim-mock-7b') */
  modelName?: string;
  /** Simulated token count per response */
  tokenCount?: number;
}

export type MockResponseStrategy =
  | 'static' // Always return the same response
  | 'echo' // Echo the input back (optionally with prefix)
  | 'template' // Match node label/prompt to responses map
  | 'sequence' // Return responses in order, cycling
  | 'function' // Custom generator function
  | 'realistic'; // Generate plausible-looking responses based on node kind

/** Custom mock response generator. */
export type MockResponseGenerator = (context: MockCallContext) => string | Promise<string>;

/** Context passed to mock response generators. */
export interface MockCallContext {
  /** The node being executed */
  node: FlowNode;
  /** The node's execution config */
  config: NodeExecConfig;
  /** Input from upstream nodes */
  upstreamInput: string;
  /** The built prompt */
  prompt: string;
  /** Agent ID being used */
  agentId: string;
  /** Current flow run state */
  runState: FlowRunState;
  /** How many times this node has been called (for mesh convergence) */
  callCount: number;
  /** The full graph */
  graph: FlowGraph;
}

/** Per-node mock behavior (overrides kind-level defaults). */
export interface MockNodeBehavior extends MockAgentBehavior {
  /** Force this node to fail */
  shouldFail?: boolean;
  /** Error message when forced to fail */
  errorMessage?: string;
  /** Fail on the Nth call only (for testing retry logic) */
  failOnCall?: number;
  /** Latency override for this specific node (ms) */
  latencyMs?: number;
  /** For condition nodes: force the result */
  forceConditionResult?: boolean;
  /** For code nodes: override the code output */
  codeOutput?: string;
}

/** HTTP mock rule: URL pattern → response */
export interface MockHttpRule {
  /** URL pattern (string match or regex) */
  urlPattern: string | RegExp;
  /** HTTP method filter (optional) */
  method?: string;
  /** Response status code */
  status: number;
  /** Response body */
  body: string;
  /** Response headers */
  headers?: Record<string, string>;
  /** Simulated latency (ms) */
  latencyMs?: number;
}

/** MCP tool mock response */
export interface MockMcpResponse {
  /** Whether the tool call succeeds */
  success: boolean;
  /** Response content */
  content: string;
  /** Error message (if success is false) */
  error?: string;
}

/** Memory mock config */
export interface MockMemoryConfig {
  /** Pre-seeded memories for recall queries */
  memories: MockMemoryEntry[];
  /** Default recall response when no memories match */
  defaultRecallResponse?: string;
}

export interface MockMemoryEntry {
  /** Search keywords that trigger this memory */
  keywords: string[];
  /** The memory content to return */
  content: string;
  /** Relevance score (0–1) */
  relevance: number;
  /** Category */
  category?: string;
}

// ── Expectations / Assertions ──────────────────────────────────────────────

/**
 * An expected outcome to verify after simulation completes.
 * These are the "holodeck safety checks" — did the agents behave correctly?
 */
export interface SimExpectation {
  /** What we're checking */
  type: ExpectationType;
  /** Human-readable description */
  description: string;
  /** Expectation details (type-specific) */
  check: ExpectationCheck;
}

export type ExpectationType =
  | 'flow-status' // Overall flow status
  | 'node-status' // Specific node status
  | 'node-output' // Node output contains/matches text
  | 'node-executed' // Node was/wasn't executed
  | 'execution-order' // Nodes executed in specific order
  | 'conductor-used' // Conductor was/wasn't activated
  | 'strategy-shape' // Strategy has specific properties
  | 'variable-set' // Flow variable was set to value
  | 'convergence' // Mesh converged within N rounds
  | 'duration' // Execution took < N ms
  | 'event-emitted' // Specific event was emitted
  | 'custom'; // Custom assertion function

export type ExpectationCheck =
  | { type: 'flow-status'; expectedStatus: FlowStatus }
  | { type: 'node-status'; nodeId: string; expectedStatus: FlowStatus }
  | {
      type: 'node-output';
      nodeId: string;
      contains?: string;
      matches?: string;
      notContains?: string;
    }
  | { type: 'node-executed'; nodeId: string; executed: boolean }
  | { type: 'execution-order'; nodeIds: string[] }
  | { type: 'conductor-used'; expected: boolean }
  | {
      type: 'strategy-shape';
      minPhases?: number;
      hasCollapse?: boolean;
      hasMesh?: boolean;
      hasParallel?: boolean;
    }
  | { type: 'variable-set'; key: string; expectedValue?: unknown; exists?: boolean }
  | { type: 'convergence'; maxRounds: number }
  | { type: 'duration'; maxMs: number }
  | { type: 'event-emitted'; eventType: FlowExecEvent['type']; count?: number }
  | { type: 'custom'; fn: (result: SimResult) => boolean; message: string };

// ── Simulation Results ─────────────────────────────────────────────────────

/** Complete result of a simulation run. */
export interface SimResult {
  /** Scenario that was run */
  scenarioId: string;
  scenarioName: string;
  /** Overall pass/fail */
  passed: boolean;
  /** Individual expectation results */
  expectationResults: ExpectationResult[];
  /** The flow run state after execution */
  runState: FlowRunState | null;
  /** Compiled execution strategy (if conductor was used) */
  strategy: ExecutionStrategy | null;
  /** All events emitted during execution */
  events: FlowExecEvent[];
  /** All mock calls that were intercepted */
  mockCalls: MockCallLog[];
  /** Total wall-clock duration (ms) */
  durationMs: number;
  /** Error if the simulation itself crashed */
  error?: string;
  /** Timestamp */
  timestamp: number;
}

export interface ExpectationResult {
  /** The expectation that was checked */
  expectation: SimExpectation;
  /** Whether it passed */
  passed: boolean;
  /** Explanation of result */
  message: string;
  /** Actual value (for debugging) */
  actual?: unknown;
  /** Expected value (for debugging) */
  expected?: unknown;
}

/** Log entry for a mock call that was intercepted. */
export interface MockCallLog {
  /** What was mocked */
  type: 'agent' | 'http' | 'mcp' | 'memory-write' | 'memory-recall' | 'squad';
  /** Node that triggered the call */
  nodeId: string;
  /** Node label */
  nodeLabel: string;
  /** The input/prompt sent */
  input: string;
  /** The mock response returned */
  output: string;
  /** Whether the mock was a failure */
  failed: boolean;
  /** Error message if failed */
  error?: string;
  /** Duration of the simulated call (ms) */
  durationMs: number;
  /** Timestamp */
  timestamp: number;
}

// ── Suite Definition ───────────────────────────────────────────────────────

/** A collection of related scenarios. */
export interface SimSuite {
  /** Suite ID */
  id: string;
  /** Suite name */
  name: string;
  /** Description */
  description: string;
  /** Scenarios in this suite */
  scenarios: SimScenario[];
  /** Global mocks applied to all scenarios (scenarios can override) */
  globalMocks?: SimMockConfig;
}

/** Result of running an entire suite. */
export interface SimSuiteResult {
  /** Suite that was run */
  suiteId: string;
  suiteName: string;
  /** Individual scenario results */
  results: SimResult[];
  /** Summary counts */
  totalScenarios: number;
  passed: number;
  failed: number;
  /** Total duration (ms) */
  durationMs: number;
  /** Timestamp */
  timestamp: number;
}

// ── Helpers ────────────────────────────────────────────────────────────────

/** Generate a realistic mock response based on node kind and context. */
export function generateRealisticResponse(ctx: MockCallContext): string {
  switch (ctx.node.kind) {
    case 'trigger':
      return `Flow "${ctx.graph.name}" triggered. Input data received.`;

    case 'agent':
      return generateAgentResponse(ctx);

    case 'tool':
      return `Tool "${ctx.node.label}" executed successfully.\n\nResult: ${ctx.upstreamInput ? `Processed: ${ctx.upstreamInput.slice(0, 200)}` : 'Operation completed.'}`;

    case 'condition':
      // For conditions, generate true/false based on input
      return ctx.upstreamInput.toLowerCase().includes('error') ||
        ctx.upstreamInput.toLowerCase().includes('fail')
        ? 'false'
        : 'true';

    case 'data':
      return `Data transformed: ${ctx.upstreamInput ? ctx.upstreamInput.slice(0, 300) : '{}'}`;

    case 'output':
      return ctx.upstreamInput || 'Flow completed with no output.';

    case 'error':
      return `Error handled: ${ctx.upstreamInput || 'Unknown error'}`;

    case 'squad':
      return generateSquadResponse(ctx);

    default:
      return `[${ctx.node.kind}] Step "${ctx.node.label}" completed. Output: ${ctx.upstreamInput?.slice(0, 100) || 'OK'}`;
  }
}

function generateAgentResponse(ctx: MockCallContext): string {
  const label = ctx.node.label.toLowerCase();
  const input = ctx.upstreamInput?.slice(0, 200) || '';

  // Generate contextually appropriate responses
  if (label.includes('research') || label.includes('analyze')) {
    return `## Analysis Report\n\nBased on the provided data${input ? `: "${input.slice(0, 80)}..."` : ''}, here are the key findings:\n\n1. **Finding A**: The data shows consistent patterns in the primary metrics.\n2. **Finding B**: There is a notable correlation between variables X and Y.\n3. **Finding C**: Edge cases in the dataset suggest further investigation needed.\n\n### Recommendation\nProceed with the identified patterns while monitoring for anomalies.`;
  }

  if (label.includes('summarize') || label.includes('summary')) {
    return `## Summary\n\n${input ? `The input describes: ${input.slice(0, 120)}` : 'Key points from the collected data'}:\n\n- Primary objective has been addressed\n- Supporting evidence confirms initial hypothesis\n- Action items identified for follow-up`;
  }

  if (label.includes('write') || label.includes('draft') || label.includes('create')) {
    return `## Draft Content\n\nTitle: ${ctx.node.label}\n\n${input ? `Based on: ${input.slice(0, 100)}` : 'Generated content follows'}:\n\nLorem ipsum productivity analysis shows that automated workflows reduce manual intervention by 73%. The integration pipeline processes data in three stages: collection, transformation, and output routing.\n\nThis draft is ready for review and refinement.`;
  }

  if (label.includes('review') || label.includes('validate') || label.includes('check')) {
    return `## Review Results\n\n**Status**: Approved with minor suggestions\n\n${input ? `Reviewed: "${input.slice(0, 80)}..."` : 'Content reviewed.'}\n\n- Quality: Good\n- Completeness: 92%\n- Issues: None critical\n\nRecommendation: Proceed to next stage.`;
  }

  if (label.includes('decide') || label.includes('route') || label.includes('classify')) {
    return `Decision: Based on the analysis, the optimal path is **Option A**.\n\nReasoning: The input data aligns with criteria for automated processing.\n\nConfidence: 87%`;
  }

  // Generic agent response
  return `Step "${ctx.node.label}" completed successfully.\n\n${input ? `Processed input: "${input.slice(0, 120)}..."\n\n` : ''}Result: The task has been executed according to the configured parameters. Output data is ready for downstream consumption.`;
}

function generateSquadResponse(ctx: MockCallContext): string {
  return `## Squad Report: ${ctx.node.label}\n\n**Objective**: ${(ctx.config.squadObjective as string) || ctx.node.label}\n\n### Agent Contributions\n- **Agent 1 (Researcher)**: Gathered relevant data and context\n- **Agent 2 (Analyst)**: Processed findings and identified patterns\n- **Agent 3 (Writer)**: Compiled final report\n\n### Consensus\nThe squad reached agreement after 2 rounds of discussion.\n\n### Conclusion\n${ctx.upstreamInput ? `Based on: "${ctx.upstreamInput.slice(0, 100)}..."` : 'Task completed.'}\nThe collaborative analysis confirms the recommended approach.`;
}

/**
 * Resolve which mock response to use for a given node call.
 * Priority: nodeOverrides > agentDefault > realistic fallback.
 */
export function resolveMockResponse(
  mocks: SimMockConfig,
  ctx: MockCallContext,
): { response: string; failed: boolean; error?: string } {
  // Check per-node override first
  const nodeOverride = mocks.nodeOverrides?.[ctx.node.id];
  if (nodeOverride) {
    // Check forced failure
    if (nodeOverride.shouldFail) {
      return {
        response: '',
        failed: true,
        error: nodeOverride.errorMessage || `Simulated failure for "${ctx.node.label}"`,
      };
    }
    // Check fail-on-call
    if (nodeOverride.failOnCall !== undefined && ctx.callCount === nodeOverride.failOnCall) {
      return {
        response: '',
        failed: true,
        error: nodeOverride.errorMessage || `Simulated failure on call #${ctx.callCount}`,
      };
    }
    // Check forced condition result
    if (ctx.node.kind === 'condition' && nodeOverride.forceConditionResult !== undefined) {
      return { response: nodeOverride.forceConditionResult ? 'true' : 'false', failed: false };
    }
    // Use override strategy for response
    const text = resolveStrategy(nodeOverride, ctx);
    if (text !== null) return { response: text, failed: false };
  }

  // Random failure check (chaos mode)
  if (mocks.failureRate && Math.random() < mocks.failureRate) {
    return {
      response: '',
      failed: true,
      error: mocks.failureMessage || 'Random simulated failure (chaos mode)',
    };
  }

  // Fall back to agent default
  const agentDefault = mocks.agentDefault;
  if (agentDefault) {
    const text = resolveStrategy(agentDefault, ctx);
    if (text !== null) return { response: text, failed: false };
  }

  // Ultimate fallback: realistic response
  return { response: generateRealisticResponse(ctx), failed: false };
}

function resolveStrategy(behavior: MockAgentBehavior, ctx: MockCallContext): string | null {
  switch (behavior.strategy) {
    case 'static':
      return behavior.response ?? null;

    case 'echo':
      return `${behavior.echoPrefix || ''}${ctx.upstreamInput || ctx.prompt}`;

    case 'template': {
      if (!behavior.responses) return null;
      // Try exact match on node label first
      const byLabel = behavior.responses[ctx.node.label];
      if (byLabel) return byLabel;
      // Try keyword match in prompt
      for (const [key, val] of Object.entries(behavior.responses)) {
        if (ctx.prompt.toLowerCase().includes(key.toLowerCase())) return val;
      }
      return null;
    }

    case 'sequence': {
      if (!behavior.sequence || behavior.sequence.length === 0) return null;
      return behavior.sequence[ctx.callCount % behavior.sequence.length];
    }

    case 'function': {
      if (!behavior.generator) return null;
      // Note: async generators are handled in the runtime layer
      const result = behavior.generator(ctx);
      if (typeof result === 'string') return result;
      return null; // Promise case handled in runtime
    }

    case 'realistic':
      return generateRealisticResponse(ctx);

    default:
      return null;
  }
}

// ── Expectation Evaluation ─────────────────────────────────────────────────

/**
 * Evaluate all expectations against a simulation result.
 * Returns individual results for each expectation.
 */
export function evaluateExpectations(
  expectations: SimExpectation[],
  result: SimResult,
): ExpectationResult[] {
  return expectations.map((exp) => evaluateSingle(exp, result));
}

function evaluateSingle(exp: SimExpectation, result: SimResult): ExpectationResult {
  const check = exp.check;

  switch (check.type) {
    case 'flow-status': {
      const actual = result.runState?.status ?? 'unknown';
      return {
        expectation: exp,
        passed: actual === check.expectedStatus,
        message:
          actual === check.expectedStatus
            ? `Flow status is "${actual}" as expected`
            : `Expected flow status "${check.expectedStatus}", got "${actual}"`,
        actual,
        expected: check.expectedStatus,
      };
    }

    case 'node-status': {
      const nodeState = result.runState?.nodeStates.get(check.nodeId);
      const actual = nodeState?.status ?? 'not-found';
      return {
        expectation: exp,
        passed: actual === check.expectedStatus,
        message:
          actual === check.expectedStatus
            ? `Node "${check.nodeId}" status is "${actual}" as expected`
            : `Expected node "${check.nodeId}" status "${check.expectedStatus}", got "${actual}"`,
        actual,
        expected: check.expectedStatus,
      };
    }

    case 'node-output': {
      const nodeState = result.runState?.nodeStates.get(check.nodeId);
      const output = nodeState?.output ?? '';
      let passed = true;
      const messages: string[] = [];

      if (check.contains !== undefined) {
        const has = output.includes(check.contains);
        if (!has) {
          passed = false;
          messages.push(`Output does not contain "${check.contains}"`);
        } else {
          messages.push(`Output contains "${check.contains}"`);
        }
      }
      if (check.matches !== undefined) {
        const re = new RegExp(check.matches);
        const has = re.test(output);
        if (!has) {
          passed = false;
          messages.push(`Output does not match /${check.matches}/`);
        } else {
          messages.push(`Output matches /${check.matches}/`);
        }
      }
      if (check.notContains !== undefined) {
        const has = output.includes(check.notContains);
        if (has) {
          passed = false;
          messages.push(`Output should not contain "${check.notContains}"`);
        } else {
          messages.push(`Output correctly does not contain "${check.notContains}"`);
        }
      }

      return {
        expectation: exp,
        passed,
        message: messages.join('; ') || 'Node output check',
        actual: output.slice(0, 200),
        expected: check.contains ?? check.matches ?? `not: ${check.notContains}`,
      };
    }

    case 'node-executed': {
      const nodeState = result.runState?.nodeStates.get(check.nodeId);
      const wasExecuted = nodeState !== undefined && nodeState.status !== 'idle';
      return {
        expectation: exp,
        passed: wasExecuted === check.executed,
        message:
          wasExecuted === check.executed
            ? `Node "${check.nodeId}" execution status correct`
            : `Expected node "${check.nodeId}" ${check.executed ? 'to be executed' : 'not to be executed'}, but it ${wasExecuted ? 'was' : 'was not'}`,
        actual: wasExecuted,
        expected: check.executed,
      };
    }

    case 'execution-order': {
      const executedOrder = result.events
        .filter((e): e is Extract<FlowExecEvent, { type: 'step-start' }> => e.type === 'step-start')
        .map((e) => e.nodeId);
      const expected = check.nodeIds;
      // Check that the expected order is a subsequence of the actual order
      let expectedIdx = 0;
      for (const nodeId of executedOrder) {
        if (expectedIdx < expected.length && nodeId === expected[expectedIdx]) {
          expectedIdx++;
        }
      }
      const passed = expectedIdx === expected.length;
      return {
        expectation: exp,
        passed,
        message: passed
          ? 'Execution order matches expected subsequence'
          : `Execution order mismatch. Expected: [${expected.join(', ')}], actual: [${executedOrder.join(', ')}]`,
        actual: executedOrder,
        expected,
      };
    }

    case 'conductor-used': {
      const used = result.strategy !== null && result.strategy.conductorUsed;
      return {
        expectation: exp,
        passed: used === check.expected,
        message:
          used === check.expected
            ? `Conductor ${check.expected ? 'was' : 'was not'} used as expected`
            : `Expected Conductor to ${check.expected ? 'be used' : 'not be used'}, but it ${used ? 'was' : 'was not'}`,
        actual: used,
        expected: check.expected,
      };
    }

    case 'strategy-shape': {
      const s = result.strategy;
      const messages: string[] = [];
      let passed = true;

      if (check.minPhases !== undefined && s) {
        if (s.phases.length < check.minPhases) {
          passed = false;
          messages.push(`Expected ≥${check.minPhases} phases, got ${s.phases.length}`);
        }
      }
      if (check.hasCollapse !== undefined && s) {
        const has = s.meta.collapseGroups > 0;
        if (has !== check.hasCollapse) {
          passed = false;
          messages.push(`Collapse groups: expected ${check.hasCollapse}, got ${has}`);
        }
      }
      if (check.hasMesh !== undefined && s) {
        const has = s.meta.meshCount > 0;
        if (has !== check.hasMesh) {
          passed = false;
          messages.push(`Mesh: expected ${check.hasMesh}, got ${has}`);
        }
      }
      if (check.hasParallel !== undefined && s) {
        const has = s.meta.parallelPhases > 0;
        if (has !== check.hasParallel) {
          passed = false;
          messages.push(`Parallel: expected ${check.hasParallel}, got ${has}`);
        }
      }
      if (!s) {
        passed = false;
        messages.push('No strategy was compiled');
      }

      return {
        expectation: exp,
        passed,
        message: messages.join('; ') || 'Strategy shape matches',
        actual: s ? s.meta : null,
        expected: check,
      };
    }

    case 'variable-set': {
      const vars = result.runState?.variables ?? {};
      if (check.exists !== undefined) {
        const has = check.key in vars;
        return {
          expectation: exp,
          passed: has === check.exists,
          message:
            has === check.exists
              ? `Variable "${check.key}" ${check.exists ? 'exists' : 'does not exist'}`
              : `Expected variable "${check.key}" to ${check.exists ? 'exist' : 'not exist'}`,
          actual: has,
          expected: check.exists,
        };
      }
      const actual = vars[check.key];
      const passed = check.expectedValue === undefined || actual === check.expectedValue;
      return {
        expectation: exp,
        passed,
        message: passed
          ? `Variable "${check.key}" = ${JSON.stringify(actual)}`
          : `Expected variable "${check.key}" = ${JSON.stringify(check.expectedValue)}, got ${JSON.stringify(actual)}`,
        actual,
        expected: check.expectedValue,
      };
    }

    case 'duration': {
      const actual = result.durationMs;
      return {
        expectation: exp,
        passed: actual <= check.maxMs,
        message:
          actual <= check.maxMs
            ? `Duration ${actual}ms ≤ ${check.maxMs}ms`
            : `Duration ${actual}ms exceeded max ${check.maxMs}ms`,
        actual,
        expected: check.maxMs,
      };
    }

    case 'event-emitted': {
      const matching = result.events.filter((e) => e.type === check.eventType);
      const countOk = check.count === undefined || matching.length === check.count;
      const passed = matching.length > 0 && countOk;
      return {
        expectation: exp,
        passed,
        message: passed
          ? `Event "${check.eventType}" emitted ${matching.length} time(s)`
          : `Expected event "${check.eventType}" ${check.count !== undefined ? `${check.count} times` : 'at least once'}, got ${matching.length}`,
        actual: matching.length,
        expected: check.count ?? '>0',
      };
    }

    case 'custom': {
      try {
        const passed = check.fn(result);
        return {
          expectation: exp,
          passed,
          message: passed ? check.message : `FAILED: ${check.message}`,
        };
      } catch (err) {
        return {
          expectation: exp,
          passed: false,
          message: `Custom check threw: ${err instanceof Error ? err.message : String(err)}`,
        };
      }
    }

    default:
      return {
        expectation: exp,
        passed: false,
        message: `Unknown expectation type: ${(check as { type: string }).type}`,
      };
  }
}

// ── Scenario Factory Helpers ───────────────────────────────────────────────

let _scenarioCounter = 0;

/** Create a minimal flow node for scenario building. */
export function simNode(kind: FlowNodeKind, overrides: Partial<FlowNode> = {}): FlowNode {
  const id = overrides.id ?? `sim_n${++_scenarioCounter}`;
  return {
    id,
    kind,
    label: overrides.label ?? `${kind}-${id}`,
    x: overrides.x ?? 0,
    y: overrides.y ?? 0,
    width: 180,
    height: 72,
    status: 'idle',
    depth: overrides.depth ?? 0,
    phase: overrides.phase ?? 0,
    cellId: overrides.cellId,
    config: overrides.config ?? {},
    inputs: overrides.inputs ?? ['in'],
    outputs: overrides.outputs ?? ['out'],
    ...overrides,
  };
}

/** Create a flow edge for scenario building. */
export function simEdge(from: string, to: string, overrides: Partial<FlowEdge> = {}): FlowEdge {
  return {
    id: overrides.id ?? `sim_e_${from}_${to}`,
    kind: overrides.kind ?? 'forward',
    from,
    to,
    fromPort: overrides.fromPort ?? 'out',
    toPort: overrides.toPort ?? 'in',
    label: overrides.label,
    condition: overrides.condition,
    active: false,
  };
}

/** Create a flow graph for scenario building. */
export function simGraph(
  nodes: FlowNode[],
  edges: FlowEdge[],
  overrides: Partial<FlowGraph> = {},
): FlowGraph {
  return {
    id: overrides.id ?? `sim_graph_${++_scenarioCounter}`,
    name: overrides.name ?? 'Simulation Graph',
    description: overrides.description,
    nodes,
    edges,
    variables: overrides.variables,
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
    ...overrides,
  };
}

/** Reset the scenario counter (useful in tests). */
export function resetSimCounters(): void {
  _scenarioCounter = 0;
}
