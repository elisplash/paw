// ─────────────────────────────────────────────────────────────────────────────
// Flow Execution Engine — Atoms (Pure Logic)
// Graph walker, step resolution, condition evaluation, execution plan builder.
// No DOM, no IPC — fully testable.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowGraph, FlowNode, FlowEdge, FlowNodeKind, FlowStatus } from './atoms';

// ── Execution Types ────────────────────────────────────────────────────────

/** Runtime state of one node during execution. */
export interface NodeRunState {
  nodeId: string;
  status: FlowStatus;
  /** Input data received from upstream edges */
  input: string;
  /** Output produced by this node */
  output: string;
  /** Error message if status === 'error' */
  error?: string;
  /** Duration in ms */
  durationMs: number;
  /** Timestamp when node started */
  startedAt: number;
  /** Timestamp when node finished */
  finishedAt: number;
}

/** Configuration for how a node should execute. */
export interface NodeExecConfig {
  /** The prompt to send to the agent (for agent/tool nodes) */
  prompt?: string;
  /** Agent ID to use (overrides flow default) */
  agentId?: string;
  /** Model override */
  model?: string;
  /** For condition nodes: the expression to evaluate */
  conditionExpr?: string;
  /** For data nodes: transform instructions */
  transform?: string;
  /** For code nodes: inline JavaScript source */
  code?: string;
  /** For trigger nodes: cron schedule expression */
  schedule?: string;
  /** Whether the schedule is enabled */
  scheduleEnabled?: boolean;
  /** For output nodes: target (chat, log, store) */
  outputTarget?: 'chat' | 'log' | 'store';
  /** For error nodes: notification targets */
  errorTargets?: ('log' | 'toast' | 'chat')[];
  /** For http nodes: HTTP method */
  httpMethod?: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
  /** For http nodes: URL to request */
  httpUrl?: string;
  /** For http nodes: request headers (JSON string) */
  httpHeaders?: string;
  /** For http nodes: request body */
  httpBody?: string;
  /** For mcp-tool nodes: MCP tool name (e.g. mcp_n8n_execute_workflow) */
  mcpToolName?: string;
  /** For mcp-tool nodes: MCP tool arguments (JSON string) */
  mcpToolArgs?: string;
  /** For loop nodes: expression to extract array from upstream (e.g. data.items) */
  loopOver?: string;
  /** For loop nodes: iteration variable name (default: 'item') */
  loopVar?: string;
  /** For loop nodes: max iterations (safety cap, default 100) */
  loopMaxIterations?: number;
  /** For any node: set a flow variable after execution */
  setVariable?: string;
  /** For any node: flow variable name for the set-variable value */
  setVariableKey?: string;
  /** For group nodes: ID of the sub-flow to embed */
  subFlowId?: string;
  /** For http/mcp-tool nodes: vault credential name to inject */
  credentialName?: string;
  /** Credential type hint: api-key, bearer, basic, oauth2 */
  credentialType?: string;
  /** Max retries on error (0 = no retry) */
  maxRetries?: number;
  /** Delay between retries in ms (default 1000) */
  retryDelayMs?: number;
  /** Backoff multiplier (default 2 = exponential backoff) */
  retryBackoff?: number;
  /** Timeout in ms */
  timeoutMs?: number;
  // ── Phase 4: Squad / Memory / Memory-Recall fields ───────────────────────
  /** For squad nodes: squad ID to invoke */
  squadId?: string;
  /** For squad nodes: objective / task description */
  squadObjective?: string;
  /** For squad nodes: timeout in ms (default 300000) */
  squadTimeoutMs?: number;
  /** For squad nodes: max discussion rounds */
  squadMaxRounds?: number;
  /** For memory nodes: content source ('output' | 'custom') */
  memorySource?: 'output' | 'custom';
  /** For memory nodes: custom content to store */
  memoryContent?: string;
  /** For memory/memory-recall nodes: agent ID to scope by */
  memoryAgentId?: string;
  /** For memory nodes: category */
  memoryCategory?: string;
  /** For memory nodes: importance 0–1 */
  memoryImportance?: number;
  /** For memory-recall nodes: query source ('input' | 'custom') */
  memoryQuerySource?: 'input' | 'custom';
  /** For memory-recall nodes: custom search query */
  memoryQuery?: string;
  /** For memory-recall nodes: max results */
  memoryLimit?: number;
  /** For memory-recall nodes: min relevance threshold 0–1 */
  memoryThreshold?: number;
  /** For memory-recall nodes: output format */
  memoryOutputFormat?: 'text' | 'json';
  /** Whether self-healing is enabled on this node */
  selfHealEnabled?: boolean;
}

/** Full execution state for a flow run. */
export interface FlowRunState {
  runId: string;
  graphId: string;
  status: FlowStatus;
  /** Ordered execution plan */
  plan: string[];
  /** Current step index in the plan */
  currentStep: number;
  /** Per-node runtime state */
  nodeStates: Map<string, NodeRunState>;
  /** Flow-level variables: mutable key-value store */
  variables: Record<string, unknown>;
  /** Pre-loaded vault credentials (name → decrypted value) */
  vaultCredentials: Record<string, string>;
  /** Accumulated output log */
  outputLog: FlowOutputEntry[];
  /** Start time */
  startedAt: number;
  /** End time */
  finishedAt: number;
  /** Total duration */
  totalDurationMs: number;
}

/** One entry in the execution output log. */
export interface FlowOutputEntry {
  nodeId: string;
  nodeLabel: string;
  nodeKind: FlowNodeKind;
  status: FlowStatus;
  output: string;
  error?: string;
  durationMs: number;
  timestamp: number;
}

/** Events emitted during flow execution. */
export type FlowExecEvent =
  | { type: 'run-start'; runId: string; graphName: string; totalSteps: number }
  | {
      type: 'step-start';
      runId: string;
      stepIndex: number;
      nodeId: string;
      nodeLabel: string;
      nodeKind: FlowNodeKind;
    }
  | { type: 'step-progress'; runId: string; nodeId: string; delta: string }
  | { type: 'step-complete'; runId: string; nodeId: string; output: string; durationMs: number }
  | { type: 'step-error'; runId: string; nodeId: string; error: string; durationMs: number }
  | {
      type: 'run-complete';
      runId: string;
      status: FlowStatus;
      totalDurationMs: number;
      outputLog: FlowOutputEntry[];
    }
  | { type: 'run-paused'; runId: string; stepIndex: number }
  | { type: 'run-aborted'; runId: string }
  | { type: 'debug-cursor'; runId: string; nodeId: string; stepIndex: number }
  | { type: 'debug-breakpoint-hit'; runId: string; nodeId: string; stepIndex: number }
  | { type: 'debug-edge-value'; runId: string; edgeId: string; value: string };

// ── Execution Plan Builder ─────────────────────────────────────────────────

/**
 * Build a topological execution order for the graph.
 * Returns an array of node IDs in the order they should execute.
 * Handles DAGs with multiple roots and orphan nodes.
 */
export function buildExecutionPlan(graph: FlowGraph): string[] {
  const inDegree = new Map<string, number>();
  const adj = new Map<string, string[]>();

  for (const n of graph.nodes) {
    inDegree.set(n.id, 0);
    adj.set(n.id, []);
  }
  for (const e of graph.edges) {
    if (e.kind !== 'reverse') {
      adj.get(e.from)?.push(e.to);
      inDegree.set(e.to, (inDegree.get(e.to) ?? 0) + 1);
    }
  }

  // Kahn's algorithm — topological sort
  const queue: string[] = [];
  for (const [id, deg] of inDegree) {
    if (deg === 0) queue.push(id);
  }

  // Sort root nodes: triggers first, then by label
  queue.sort((a, b) => {
    const na = graph.nodes.find((n) => n.id === a);
    const nb = graph.nodes.find((n) => n.id === b);
    if (na?.kind === 'trigger' && nb?.kind !== 'trigger') return -1;
    if (nb?.kind === 'trigger' && na?.kind !== 'trigger') return 1;
    return (na?.label ?? '').localeCompare(nb?.label ?? '');
  });

  const result: string[] = [];

  while (queue.length) {
    const nodeId = queue.shift()!;
    result.push(nodeId);

    const children = adj.get(nodeId) ?? [];
    for (const child of children) {
      const newDeg = (inDegree.get(child) ?? 1) - 1;
      inDegree.set(child, newDeg);
      if (newDeg === 0) queue.push(child);
    }
  }

  // Handle cycle detection: add remaining nodes that weren't visited
  for (const n of graph.nodes) {
    if (!result.includes(n.id)) {
      result.push(n.id);
    }
  }

  return result;
}

/**
 * Get the immediate upstream node IDs for a given node.
 */
export function getUpstreamNodes(graph: FlowGraph, nodeId: string): string[] {
  return graph.edges.filter((e) => e.to === nodeId && e.kind !== 'reverse').map((e) => e.from);
}

/**
 * Get the immediate downstream node IDs for a given node.
 */
export function getDownstreamNodes(graph: FlowGraph, nodeId: string): string[] {
  return graph.edges.filter((e) => e.from === nodeId && e.kind !== 'reverse').map((e) => e.to);
}

/**
 * Collect the aggregated input for a node by joining upstream outputs.
 */
export function collectNodeInput(
  graph: FlowGraph,
  nodeId: string,
  nodeStates: Map<string, NodeRunState>,
): string {
  const upstreamIds = getUpstreamNodes(graph, nodeId);
  const parts: string[] = [];

  for (const uid of upstreamIds) {
    const state = nodeStates.get(uid);
    if (state?.output) {
      parts.push(state.output);
    }
  }

  return parts.join('\n\n');
}

// ── Node Prompt Builder ────────────────────────────────────────────────────

/**
 * Build the prompt to send to an agent for a given node.
 * Combines the node's configured prompt with upstream data.
 */
export function buildNodePrompt(
  node: FlowNode,
  upstreamInput: string,
  config: NodeExecConfig,
): string {
  const parts: string[] = [];

  // Context from upstream
  if (upstreamInput) {
    parts.push(`[Previous step output]\n${upstreamInput}`);
  }

  // Node-specific instructions
  switch (node.kind) {
    case 'trigger':
      if (config.prompt) parts.push(config.prompt);
      else parts.push(`Start the flow: ${node.label}`);
      break;

    case 'agent':
      if (config.prompt) {
        parts.push(config.prompt);
      } else {
        parts.push(`You are performing step "${node.label}" in an automated flow.`);
        if (node.description) parts.push(node.description);
        if (upstreamInput) {
          parts.push('Process the above input and produce your output.');
        }
      }
      break;

    case 'tool':
      if (config.prompt) {
        parts.push(config.prompt);
      } else {
        parts.push(`Execute the tool step: ${node.label}`);
        if (node.description) parts.push(`Instructions: ${node.description}`);
      }
      break;

    case 'condition':
      if (config.conditionExpr) {
        parts.push(`Evaluate this condition: ${config.conditionExpr}`);
        parts.push('Respond with only "true" or "false".');
      } else {
        parts.push(`Evaluate the condition: ${node.label}`);
        parts.push('Based on the input above, respond with only "true" or "false".');
      }
      break;

    case 'data':
      if (config.transform) {
        parts.push(`Transform the data: ${config.transform}`);
      } else {
        parts.push(`Transform the data according to: ${node.label}`);
        if (node.description) parts.push(node.description);
      }
      break;

    case 'code':
      // Code nodes don't need a prompt — they execute inline JS
      parts.push(`[Code node: ${node.label}]`);
      break;

    case 'error':
      // Error handler nodes receive error info
      parts.push(`[Error handler: ${node.label}]`);
      if (config.prompt) parts.push(config.prompt);
      break;

    case 'output':
      parts.push(upstreamInput || 'No output to report.');
      break;

    case 'group':
      // Group/sub-flow nodes describe their embedded flow
      parts.push(`[Sub-flow: ${node.label}]`);
      if (config.prompt) parts.push(config.prompt);
      if (node.description) parts.push(node.description);
      break;

    default:
      if (config.prompt) parts.push(config.prompt);
      else parts.push(`Execute step: ${node.label}`);
  }

  return parts.join('\n\n');
}

// ── Condition Evaluation ───────────────────────────────────────────────────

/**
 * Evaluate a simple condition expression against a string response.
 * Returns true if the response is truthy.
 */
export function evaluateCondition(response: string): boolean {
  const normalized = response.trim().toLowerCase();
  if (normalized === 'true' || normalized === 'yes' || normalized === '1') return true;
  if (normalized === 'false' || normalized === 'no' || normalized === '0') return false;
  // If the response contains "true" somewhere, treat as truthy
  return normalized.includes('true') || normalized.includes('yes');
}

/**
 * Determine which downstream edges to follow based on a condition result.
 * Edges with label "true"/"yes" are taken when condition is true.
 * Edges with label "false"/"no" are taken when condition is false.
 * Edges with no label are always taken.
 */
export function resolveConditionEdges(
  graph: FlowGraph,
  conditionNodeId: string,
  conditionResult: boolean,
): FlowEdge[] {
  const outEdges = graph.edges.filter((e) => e.from === conditionNodeId && e.kind !== 'reverse');

  return outEdges.filter((e) => {
    if (!e.label && !e.condition) return true; // No label = always follow
    const label = (e.label ?? e.condition ?? '').trim().toLowerCase();
    if (conditionResult) {
      return label === 'true' || label === 'yes' || label === '';
    } else {
      return label === 'false' || label === 'no';
    }
  });
}

// ── Run State Factory ──────────────────────────────────────────────────────

let _runCounter = 0;

export function createRunId(): string {
  return `run_${Date.now().toString(36)}_${(++_runCounter).toString(36)}`;
}

export function createFlowRunState(
  graphId: string,
  plan: string[],
  initialVars?: Record<string, unknown>,
  vaultCredentials?: Record<string, string>,
): FlowRunState {
  return {
    runId: createRunId(),
    graphId,
    status: 'idle',
    plan,
    currentStep: 0,
    nodeStates: new Map(),
    variables: { ...(initialVars ?? {}) },
    vaultCredentials: { ...(vaultCredentials ?? {}) },
    outputLog: [],
    startedAt: 0,
    finishedAt: 0,
    totalDurationMs: 0,
  };
}

export function createNodeRunState(nodeId: string): NodeRunState {
  return {
    nodeId,
    status: 'idle',
    input: '',
    output: '',
    durationMs: 0,
    startedAt: 0,
    finishedAt: 0,
  };
}

// ── Exec Config Extraction ─────────────────────────────────────────────────

/**
 * Extract execution config from a node's config object.
 */
export function getNodeExecConfig(node: FlowNode): NodeExecConfig {
  const c = node.config ?? {};
  return {
    prompt: (c.prompt as string) ?? undefined,
    agentId: (c.agentId as string) ?? undefined,
    model: (c.model as string) ?? undefined,
    conditionExpr: (c.conditionExpr as string) ?? undefined,
    transform: (c.transform as string) ?? undefined,
    code: (c.code as string) ?? undefined,
    schedule: (c.schedule as string) ?? undefined,
    scheduleEnabled: (c.scheduleEnabled as boolean) ?? false,
    outputTarget: (c.outputTarget as 'chat' | 'log' | 'store') ?? 'chat',
    errorTargets: (c.errorTargets as ('log' | 'toast' | 'chat')[]) ?? ['log'],
    httpMethod: (c.httpMethod as 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE') ?? undefined,
    httpUrl: (c.httpUrl as string) ?? undefined,
    httpHeaders: (c.httpHeaders as string) ?? undefined,
    httpBody: (c.httpBody as string) ?? undefined,
    mcpToolName: (c.mcpToolName as string) ?? undefined,
    mcpToolArgs: (c.mcpToolArgs as string) ?? undefined,
    loopOver: (c.loopOver as string) ?? undefined,
    loopVar: (c.loopVar as string) ?? 'item',
    loopMaxIterations: (c.loopMaxIterations as number) ?? 100,
    setVariable: (c.setVariable as string) ?? undefined,
    setVariableKey: (c.setVariableKey as string) ?? undefined,
    subFlowId: (c.subFlowId as string) ?? undefined,
    credentialName: (c.credentialName as string) ?? undefined,
    credentialType: (c.credentialType as string) ?? undefined,
    maxRetries: (c.maxRetries as number) ?? 0,
    retryDelayMs: (c.retryDelayMs as number) ?? 1000,
    retryBackoff: (c.retryBackoff as number) ?? 2,
    timeoutMs: (c.timeoutMs as number) ?? 120_000,
    // Phase 4: Squad / Memory
    squadId: (c.squadId as string) ?? undefined,
    squadObjective: (c.squadObjective as string) ?? undefined,
    squadTimeoutMs: (c.squadTimeoutMs as number) ?? 300_000,
    squadMaxRounds: (c.squadMaxRounds as number) ?? 5,
    memorySource: (c.memorySource as 'output' | 'custom') ?? 'output',
    memoryContent: (c.memoryContent as string) ?? undefined,
    memoryAgentId: (c.memoryAgentId as string) ?? undefined,
    memoryCategory: (c.memoryCategory as string) ?? 'insight',
    memoryImportance: (c.memoryImportance as number) ?? 0.5,
    memoryQuerySource: (c.memoryQuerySource as 'input' | 'custom') ?? 'input',
    memoryQuery: (c.memoryQuery as string) ?? undefined,
    memoryLimit: (c.memoryLimit as number) ?? 5,
    memoryThreshold: (c.memoryThreshold as number) ?? 0.3,
    memoryOutputFormat: (c.memoryOutputFormat as 'text' | 'json') ?? 'text',
    selfHealEnabled: (c.selfHealEnabled as boolean) ?? false,
  };
}

// ── Re-exports from split modules ──────────────────────────────────────────

export {
  type FlowSchedule,
  type ScheduleFireLog,
  CRON_PRESETS,
  nextCronFire,
  validateCron,
  describeCron,
} from './cron-atoms';
export { resolveVariables, parseLoopArray } from './variable-atoms';
export {
  type FlowValidationError,
  validateFlowForExecution,
  summarizeRun,
  formatMs,
  executeCodeSandboxed,
} from './sandbox-atoms';
