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
  /** Max retries on error (0 = no retry) */
  maxRetries?: number;
  /** Delay between retries in ms (default 1000) */
  retryDelayMs?: number;
  /** Backoff multiplier (default 2 = exponential backoff) */
  retryBackoff?: number;
  /** Timeout in ms */
  timeoutMs?: number;
}

// ── Schedule Types ─────────────────────────────────────────────────────────

/** A registered flow schedule (trigger node with cron expression). */
export interface FlowSchedule {
  flowId: string;
  flowName: string;
  nodeId: string;
  schedule: string;
  enabled: boolean;
  lastFiredAt: number | null;
  nextFireAt: number | null;
}

/** Log entry for a schedule fire event. */
export interface ScheduleFireLog {
  flowId: string;
  flowName: string;
  firedAt: number;
  status: 'success' | 'error';
  error?: string;
}

/**
 * Common cron presets for the UI picker.
 */
export const CRON_PRESETS: Array<{ label: string; value: string; description: string }> = [
  { label: 'Every minute', value: '* * * * *', description: 'Runs every 60 seconds' },
  { label: 'Every 5 minutes', value: '*/5 * * * *', description: 'Runs every 5 minutes' },
  { label: 'Every 15 minutes', value: '*/15 * * * *', description: 'Runs every 15 minutes' },
  { label: 'Every hour', value: '0 * * * *', description: 'Runs at the start of every hour' },
  { label: 'Every 6 hours', value: '0 */6 * * *', description: 'Runs every 6 hours' },
  { label: 'Daily at midnight', value: '0 0 * * *', description: 'Runs once a day at 00:00' },
  { label: 'Daily at 9 AM', value: '0 9 * * *', description: 'Runs once a day at 09:00' },
  { label: 'Weekdays at 9 AM', value: '0 9 * * 1-5', description: 'Mon–Fri at 09:00' },
  { label: 'Every Monday', value: '0 9 * * 1', description: 'Every Monday at 09:00' },
  { label: 'Monthly (1st)', value: '0 0 1 * *', description: 'First day of every month at 00:00' },
];

/**
 * Parse a simple cron expression (minute hour dom month dow)
 * and determine the next fire time from a given reference.
 * Supports: *, N, * /N (step), N-M (range), N,M (list)
 * Returns null if the expression is invalid.
 */
export function nextCronFire(expression: string, from: Date = new Date()): Date | null {
  const parts = expression.trim().split(/\s+/);
  if (parts.length !== 5) return null;

  function matches(field: string, value: number, max: number): boolean {
    if (field === '*') return true;
    // Step: */N
    if (field.startsWith('*/')) {
      const step = parseInt(field.slice(2), 10);
      return !isNaN(step) && step > 0 && value % step === 0;
    }
    // Range: N-M
    if (field.includes('-') && !field.includes(',')) {
      const [lo, hi] = field.split('-').map(Number);
      if (isNaN(lo) || isNaN(hi)) return false;
      return value >= lo && value <= hi;
    }
    // List: N,M,...
    if (field.includes(',')) {
      return field.split(',').map(Number).includes(value);
    }
    // Exact
    const exact = parseInt(field, 10);
    return !isNaN(exact) && exact >= 0 && exact <= max && value === exact;
  }

  // Search forward up to 1 year (525600 minutes)
  const candidate = new Date(from.getTime());
  candidate.setSeconds(0, 0);
  candidate.setMinutes(candidate.getMinutes() + 1);

  for (let i = 0; i < 525600; i++) {
    const min = candidate.getMinutes();
    const hour = candidate.getHours();
    const dom = candidate.getDate();
    const month = candidate.getMonth() + 1; // 1-indexed
    const dow = candidate.getDay(); // 0=Sun

    if (
      matches(parts[0], min, 59) &&
      matches(parts[1], hour, 23) &&
      matches(parts[2], dom, 31) &&
      matches(parts[3], month, 12) &&
      matches(parts[4], dow, 7)
    ) {
      return candidate;
    }

    candidate.setMinutes(candidate.getMinutes() + 1);
  }

  return null; // No match within a year
}

/**
 * Validate a cron expression. Returns null if valid, or an error message.
 */
export function validateCron(expression: string): string | null {
  const parts = expression.trim().split(/\s+/);
  if (parts.length !== 5) return 'Expected 5 fields: minute hour day month weekday';
  const valid = /^(\*|\d+(-\d+)?(,\d+(-\d+)?)*|\*\/\d+)$/;
  const labels = ['minute', 'hour', 'day', 'month', 'weekday'];
  for (let i = 0; i < 5; i++) {
    if (!valid.test(parts[i])) return `Invalid ${labels[i]} field: "${parts[i]}"`;
  }
  return null;
}

/**
 * Human-readable description of a cron expression.
 */
export function describeCron(expression: string): string {
  const preset = CRON_PRESETS.find((p) => p.value === expression);
  if (preset) return preset.description;
  return `Schedule: ${expression}`;
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

export function createFlowRunState(graphId: string, plan: string[]): FlowRunState {
  return {
    runId: createRunId(),
    graphId,
    status: 'idle',
    plan,
    currentStep: 0,
    nodeStates: new Map(),
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
    maxRetries: (c.maxRetries as number) ?? 0,
    retryDelayMs: (c.retryDelayMs as number) ?? 1000,
    retryBackoff: (c.retryBackoff as number) ?? 2,
    timeoutMs: (c.timeoutMs as number) ?? 120_000,
  };
}

// ── Validation ─────────────────────────────────────────────────────────────

export interface FlowValidationError {
  nodeId?: string;
  message: string;
}

/**
 * Validate a flow graph before execution.
 * Returns an array of errors (empty = valid).
 */
export function validateFlowForExecution(graph: FlowGraph): FlowValidationError[] {
  const errors: FlowValidationError[] = [];

  if (graph.nodes.length === 0) {
    errors.push({ message: 'Flow has no nodes.' });
    return errors;
  }

  // Check for nodes with no edges (disconnected)
  const connectedNodes = new Set<string>();
  for (const e of graph.edges) {
    connectedNodes.add(e.from);
    connectedNodes.add(e.to);
  }

  // Single-node flows are OK (just run the one node)
  if (graph.nodes.length > 1) {
    for (const n of graph.nodes) {
      if (!connectedNodes.has(n.id)) {
        errors.push({ nodeId: n.id, message: `Node "${n.label}" is disconnected.` });
      }
    }
  }

  // Check for agent nodes without an agent configured (warning, not blocking)
  for (const n of graph.nodes) {
    if (n.kind === 'agent') {
      const config = getNodeExecConfig(n);
      if (!config.prompt && !n.description) {
        errors.push({ nodeId: n.id, message: `Agent node "${n.label}" has no prompt configured.` });
      }
    }
  }

  return errors;
}

/**
 * Generates a human-readable summary of a flow run.
 */
export function summarizeRun(runState: FlowRunState, graph: FlowGraph): string {
  const lines: string[] = [];
  lines.push(`**Flow Run: ${graph.name}**`);
  lines.push(
    `Status: ${runState.status} | Steps: ${runState.plan.length} | Duration: ${formatMs(runState.totalDurationMs)}`,
  );
  lines.push('');

  for (const entry of runState.outputLog) {
    const icon = entry.status === 'success' ? '✓' : entry.status === 'error' ? '✗' : '…';
    lines.push(
      `${icon} **${entry.nodeLabel}** (${entry.nodeKind}) — ${formatMs(entry.durationMs)}`,
    );
    if (entry.output) {
      const preview = entry.output.length > 200 ? `${entry.output.slice(0, 200)}…` : entry.output;
      lines.push(`  ${preview}`);
    }
    if (entry.error) {
      lines.push(`  Error: ${entry.error}`);
    }
  }

  return lines.join('\n');
}

function formatMs(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  return `${(ms / 60_000).toFixed(1)}m`;
}

// ── Sandboxed Code Execution ───────────────────────────────────────────────

/**
 * Execute inline JavaScript in a restricted sandbox.
 * - No access to window, document, fetch, eval, Function
 * - Input is provided as `input` (string) and `data` (parsed JSON or null)
 * - Must return a string (or value that gets stringified)
 * - Timeout enforced (default 5s)
 */
export function executeCodeSandboxed(
  code: string,
  input: string,
  timeoutMs = 5000,
): { output: string; error?: string } {
  // Parse input as JSON if possible, otherwise pass as string
  let parsedData: unknown = null;
  try {
    parsedData = JSON.parse(input);
  } catch {
    parsedData = null;
  }

  // Block dangerous patterns
  const forbidden = [
    /\bwindow\b/,
    /\bdocument\b/,
    /\bfetch\b/,
    /\bXMLHttpRequest\b/,
    /\bimport\s*\(/,
    /\brequire\s*\(/,
    /\beval\s*\(/,
    /\bnew\s+Function\b/,
    /\bglobalThis\b/,
    /\bprocess\b/,
    /\b__proto__\b/,
    /\bconstructor\s*\[/,
  ];
  for (const pattern of forbidden) {
    if (pattern.test(code)) {
      return { output: '', error: `Blocked: code contains forbidden pattern "${pattern.source}"` };
    }
  }

  try {
    // Build sandboxed function with restricted scope
    // The function receives `input` (string), `data` (parsed), and utility helpers
    // Shadow dangerous globals by declaring them as undefined parameters
    // eslint-disable-next-line no-new-func -- intentional: sandboxed execution with blocked globals
    const sandboxFn = new Function(
      'input',
      'data',
      'console',
      'JSON',
      'Math',
      'Date',
      'Array',
      'Object',
      'String',
      'Number',
      'Boolean',
      'RegExp',
      'Map',
      'Set',
      // Shadow dangerous globals (cannot use reserved words as params)
      'window',
      'document',
      'fetch',
      'XMLHttpRequest',
      'globalThis',
      'process',
      'require',
      `"use strict";\n${code}`,
    );

    const safeConsole = {
      log: (...args: unknown[]) => logs.push(args.map(String).join(' ')),
      warn: (...args: unknown[]) => logs.push(`[warn] ${args.map(String).join(' ')}`),
      error: (...args: unknown[]) => logs.push(`[error] ${args.map(String).join(' ')}`),
    };
    const logs: string[] = [];

    // Execute with timeout via synchronous execution (no async support in sandbox)
    const start = Date.now();
    const result = sandboxFn(
      input,
      parsedData,
      safeConsole,
      JSON,
      Math,
      Date,
      Array,
      Object,
      String,
      Number,
      Boolean,
      RegExp,
      Map,
      Set,
      // Shadowed as undefined
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
    );

    const elapsed = Date.now() - start;
    if (elapsed > timeoutMs) {
      return { output: '', error: `Code execution exceeded timeout (${timeoutMs}ms)` };
    }

    // Build output: combine return value + console logs
    let output = '';
    if (result !== undefined && result !== null) {
      output = typeof result === 'string' ? result : JSON.stringify(result, null, 2);
    }
    if (logs.length > 0) {
      const logStr = logs.join('\n');
      output = output ? `${output}\n\n[Console]\n${logStr}` : logStr;
    }

    return { output: output || 'Code executed (no output)' };
  } catch (err) {
    return { output: '', error: err instanceof Error ? err.message : String(err) };
  }
}
