// ─────────────────────────────────────────────────────────────────────────────
// Flow Suggestions — Atoms (Pure Logic)
// Pattern matching, common sequences, suggestion scoring for flow autocomplete.
// No DOM, no IPC — fully testable.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowNode, FlowGraph, FlowNodeKind } from './atoms';

// ── Types ──────────────────────────────────────────────────────────────────

/** A suggested next node to add after the current selection. */
export interface NodeSuggestion {
  /** Suggested node kind */
  kind: FlowNodeKind;
  /** Suggested label */
  label: string;
  /** Why this was suggested */
  reason: string;
  /** Confidence score 0–1 */
  score: number;
  /** Suggested config (partial) */
  config?: Record<string, unknown>;
}

// ── Common Patterns ────────────────────────────────────────────────────────

/** Known flow patterns: "after node kind X, Y is commonly next". */
interface SequencePattern {
  after: FlowNodeKind;
  suggestions: Array<{
    kind: FlowNodeKind;
    label: string;
    score: number;
    reason: string;
  }>;
}

const COMMON_PATTERNS: SequencePattern[] = [
  {
    after: 'trigger',
    suggestions: [
      {
        kind: 'agent',
        label: 'Process Input',
        score: 0.9,
        reason: 'Triggers typically feed into an agent for processing',
      },
      {
        kind: 'http',
        label: 'Fetch Data',
        score: 0.7,
        reason: 'Fetch external data before processing',
      },
      {
        kind: 'memory-recall',
        label: 'Recall Context',
        score: 0.6,
        reason: 'Retrieve relevant memory before processing',
      },
      {
        kind: 'condition',
        label: 'Check Input',
        score: 0.5,
        reason: 'Route based on input type or content',
      },
    ],
  },
  {
    after: 'agent',
    suggestions: [
      {
        kind: 'output',
        label: 'Send Result',
        score: 0.8,
        reason: "Send the agent's output to the user",
      },
      {
        kind: 'condition',
        label: 'Check Result',
        score: 0.7,
        reason: "Branch based on the agent's output",
      },
      {
        kind: 'agent',
        label: 'Refine Result',
        score: 0.6,
        reason: 'Chain another agent for refinement',
      },
      {
        kind: 'memory',
        label: 'Save to Memory',
        score: 0.5,
        reason: 'Store the result in long-term memory',
      },
      {
        kind: 'data',
        label: 'Transform Result',
        score: 0.5,
        reason: 'Transform the output format',
      },
    ],
  },
  {
    after: 'tool',
    suggestions: [
      {
        kind: 'agent',
        label: 'Analyze Result',
        score: 0.8,
        reason: 'Have an agent analyze the tool output',
      },
      {
        kind: 'output',
        label: 'Show Result',
        score: 0.7,
        reason: 'Display the tool result directly',
      },
      { kind: 'data', label: 'Transform Data', score: 0.6, reason: 'Transform the tool output' },
    ],
  },
  {
    after: 'condition',
    suggestions: [
      { kind: 'agent', label: 'True Branch', score: 0.8, reason: 'Handle the true condition path' },
      {
        kind: 'error',
        label: 'Handle Error',
        score: 0.6,
        reason: 'Handle the false/error condition path',
      },
      {
        kind: 'output',
        label: 'Output Result',
        score: 0.5,
        reason: 'Output directly from condition',
      },
    ],
  },
  {
    after: 'data',
    suggestions: [
      {
        kind: 'agent',
        label: 'Process Data',
        score: 0.8,
        reason: 'Feed transformed data to an agent',
      },
      { kind: 'output', label: 'Send Data', score: 0.7, reason: 'Send the transformed data' },
      {
        kind: 'condition',
        label: 'Filter Data',
        score: 0.5,
        reason: 'Route based on transformed data',
      },
    ],
  },
  {
    after: 'code',
    suggestions: [
      { kind: 'output', label: 'Show Result', score: 0.7, reason: 'Display the code output' },
      {
        kind: 'agent',
        label: 'Analyze Code Output',
        score: 0.6,
        reason: 'Have an agent interpret the code result',
      },
      {
        kind: 'condition',
        label: 'Check Result',
        score: 0.5,
        reason: 'Branch based on code output',
      },
    ],
  },
  {
    after: 'http',
    suggestions: [
      {
        kind: 'agent',
        label: 'Process Response',
        score: 0.8,
        reason: 'Have an agent analyze the API response',
      },
      { kind: 'data', label: 'Extract Fields', score: 0.7, reason: 'Transform the HTTP response' },
      {
        kind: 'condition',
        label: 'Check Status',
        score: 0.6,
        reason: 'Branch based on response status',
      },
    ],
  },
  {
    after: 'mcp-tool',
    suggestions: [
      {
        kind: 'agent',
        label: 'Analyze Result',
        score: 0.8,
        reason: 'Have an agent analyze the MCP tool result',
      },
      { kind: 'output', label: 'Show Result', score: 0.6, reason: 'Display the MCP tool result' },
    ],
  },
  {
    after: 'loop',
    suggestions: [
      {
        kind: 'agent',
        label: 'Process Items',
        score: 0.8,
        reason: 'Process each loop iteration with an agent',
      },
      {
        kind: 'output',
        label: 'Collect Results',
        score: 0.6,
        reason: 'Collect and output loop results',
      },
    ],
  },
  {
    after: 'squad',
    suggestions: [
      {
        kind: 'output',
        label: 'Send Squad Result',
        score: 0.8,
        reason: "Output the squad's consensus result",
      },
      {
        kind: 'memory',
        label: 'Save Decision',
        score: 0.6,
        reason: "Save the squad's decision to memory",
      },
      {
        kind: 'condition',
        label: 'Evaluate Decision',
        score: 0.5,
        reason: 'Branch based on squad result',
      },
    ],
  },
  {
    after: 'memory-recall',
    suggestions: [
      {
        kind: 'agent',
        label: 'Use Context',
        score: 0.9,
        reason: 'Feed recalled memory to an agent for context-aware processing',
      },
      {
        kind: 'condition',
        label: 'Has Relevant Memory?',
        score: 0.6,
        reason: 'Check if relevant memories were found',
      },
    ],
  },
  {
    after: 'memory',
    suggestions: [
      {
        kind: 'output',
        label: 'Confirm Saved',
        score: 0.7,
        reason: 'Confirm memory was saved successfully',
      },
      {
        kind: 'agent',
        label: 'Continue Processing',
        score: 0.5,
        reason: 'Continue with additional processing',
      },
    ],
  },
];

// ── Suggestion Engine ──────────────────────────────────────────────────────

/**
 * Get node suggestions based on the currently selected node.
 * Uses pattern matching on common flow sequences.
 */
export function getSuggestionsForNode(selectedNode: FlowNode, graph: FlowGraph): NodeSuggestion[] {
  const suggestions: NodeSuggestion[] = [];

  // 1. Pattern-based suggestions
  const pattern = COMMON_PATTERNS.find((p) => p.after === selectedNode.kind);
  if (pattern) {
    for (const s of pattern.suggestions) {
      suggestions.push({
        kind: s.kind,
        label: s.label,
        reason: s.reason,
        score: s.score,
      });
    }
  }

  // 2. Graph-aware adjustments
  const existingKinds = new Set(graph.nodes.map((n) => n.kind));
  const hasOutput = existingKinds.has('output');
  const hasError = existingKinds.has('error');

  // Boost output suggestion if flow has no output yet
  if (!hasOutput) {
    const outputSuggestion = suggestions.find((s) => s.kind === 'output');
    if (outputSuggestion) {
      outputSuggestion.score = Math.min(1.0, outputSuggestion.score + 0.2);
      outputSuggestion.reason += ' (flow has no output node yet)';
    } else {
      suggestions.push({
        kind: 'output',
        label: 'Output Result',
        reason: 'Flow has no output node yet',
        score: 0.4,
      });
    }
  }

  // Add error handler suggestion if none exists
  if (!hasError && graph.nodes.length >= 3) {
    suggestions.push({
      kind: 'error',
      label: 'Handle Errors',
      reason: 'No error handler in flow — add one for robustness',
      score: 0.3,
    });
  }

  // Sort by score descending and cap at 5
  suggestions.sort((a, b) => b.score - a.score);
  return suggestions.slice(0, 5);
}

/**
 * Calculate the suggested position for a new node relative to the source node.
 * Places it to the right with appropriate spacing.
 */
export function suggestedNodePosition(
  sourceNode: FlowNode,
  graph: FlowGraph,
  suggestionIndex = 0,
): { x: number; y: number } {
  const xOffset = 240;
  const yOffset = suggestionIndex * 100; // Stack vertically for multiple suggestions

  const x = sourceNode.x + sourceNode.width + xOffset;
  const y = sourceNode.y + yOffset;

  // Avoid overlapping existing nodes
  const occupied = graph.nodes.some((n) => Math.abs(n.x - x) < 100 && Math.abs(n.y - y) < 50);
  if (occupied) {
    return { x, y: y + 120 }; // Push down if overlapping
  }

  return { x, y };
}
