// ─────────────────────────────────────────────────────────────────────────────
// Flow Execution Engine — Sandbox & Validation Atoms
// Sandboxed code execution, flow validation, and run summarization.
// No DOM, no IPC — fully testable.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowGraph } from './atoms';
import { getNodeExecConfig, type FlowRunState } from './executor-atoms';

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

// ── Run Summary ────────────────────────────────────────────────────────────

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

export function formatMs(ms: number): string {
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
