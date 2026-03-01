// ─────────────────────────────────────────────────────────────────────────────
// Flow Visualization Engine — Parser
// Converts natural-language / shorthand text into a FlowGraph.
// Pure functions — no DOM, no IPC.
// ─────────────────────────────────────────────────────────────────────────────

import {
  type FlowGraph,
  type FlowNode,
  type FlowEdge,
  type FlowNodeKind,
  type EdgeKind,
  createNode,
  createEdge,
  createGraph,
  applyLayout,
} from './atoms';

// ── Types ──────────────────────────────────────────────────────────────────

export interface ParseResult {
  graph: FlowGraph;
  /** Warnings or notes from parsing */
  warnings: string[];
}

// ── Keyword → Kind Mapping ─────────────────────────────────────────────────

const KIND_KEYWORDS: [RegExp, FlowNodeKind][] = [
  [/\b(trigger|start|webhook|cron|on|when|listen|event)\b/i, 'trigger'],
  [/\b(agent|ai|llm|model|gpt|claude|ask|chat|think)\b/i, 'agent'],
  [/\b(tool|mcp|call|invoke|run|execute|api|fetch|http|function)\b/i, 'tool'],
  [/\b(if|else|condition|branch|switch|check|decide|filter|gate|test)\b/i, 'condition'],
  [/\b(data|transform|map|parse|format|convert|extract|merge|split)\b/i, 'data'],
  [/\b(output|send|email|notify|log|store|save|write|return|respond|emit|post)\b/i, 'output'],
];

function detectKind(text: string): FlowNodeKind {
  for (const [re, kind] of KIND_KEYWORDS) {
    if (re.test(text)) return kind;
  }
  return 'tool'; // default fallback
}

// ── Edge Kind Detection ────────────────────────────────────────────────────

const REVERSE_MARKERS = /\b(pull|request|fetch from|get from|ask for|needs|requires)\b/i;
const BIDI_MARKERS = /\b(sync|handshake|exchange|negotiate|bi-?directional|two-?way)\b/i;

function detectEdgeKind(text: string): EdgeKind {
  if (BIDI_MARKERS.test(text)) return 'bidirectional';
  if (REVERSE_MARKERS.test(text)) return 'reverse';
  return 'forward';
}

// ── Main Parser ────────────────────────────────────────────────────────────

/**
 * Parse natural-language text into a FlowGraph.
 *
 * Supported formats:
 *   1. Arrow syntax:  "webhook → agent → send email"
 *   2. Step list:     "1. webhook  2. agent  3. send email"
 *   3. Prose:         "When a webhook fires, the agent processes it, then sends an email"
 *   4. Pipe syntax:   "webhook | agent | send email"
 */
export function parseFlowText(text: string, name = 'Untitled Flow'): ParseResult {
  const warnings: string[] = [];
  const trimmed = text.trim();

  if (!trimmed) {
    return { graph: createGraph(name), warnings: ['Empty input'] };
  }

  // Try arrow syntax first (most explicit)
  if (/[→⟶⟹➜➡>]/.test(trimmed) || /->|=>/.test(trimmed)) {
    return parseArrowSyntax(trimmed, name, warnings);
  }

  // Try "then" / "and then" as a lightweight connector (e.g. "webhook then agent then email")
  // Only match when "then" appears as a standalone word separator (at least 2 segments)
  if (/\bthen\b/i.test(trimmed)) {
    const thenSegments = trimmed.split(/\s+(?:and\s+)?then\s+/i).filter(Boolean);
    if (thenSegments.length >= 2) {
      return parseArrowSyntax(thenSegments.join(' → '), name, warnings);
    }
  }

  // Try numbered list
  if (/^\s*\d+[\.\)]\s/m.test(trimmed)) {
    return parseNumberedList(trimmed, name, warnings);
  }

  // Try pipe syntax
  if (/\s*\|\s*/.test(trimmed) && trimmed.split('|').length >= 2) {
    return parsePipeSyntax(trimmed, name, warnings);
  }

  // Fallback: prose (sentence-based)
  return parseProse(trimmed, name, warnings);
}

// ── Arrow Syntax ───────────────────────────────────────────────────────────

function parseArrowSyntax(text: string, name: string, warnings: string[]): ParseResult {
  // Normalize arrow variants
  const normalized = text
    .replace(/[⟶⟹➜➡]/g, '→')
    .replace(/->/g, '→')
    .replace(/=>/g, '→')
    .replace(/[<>]/g, (m) => (m === '>' ? '→' : '←'));

  // Split by lines (multi-line flows)
  const lines = normalized
    .split(/\n/)
    .map((l) => l.trim())
    .filter(Boolean);

  const nodes: FlowNode[] = [];
  const edges: FlowEdge[] = [];
  const nodeMap = new Map<string, FlowNode>();

  for (const line of lines) {
    // Check for reverse arrows
    const segments = line.split(/\s*[→←↔]\s*/).filter(Boolean);
    const arrows = [...line.matchAll(/[→←↔]/g)].map((m) => m[0]);

    let prevNode: FlowNode | null = null;

    for (let i = 0; i < segments.length; i++) {
      const seg = segments[i].trim();
      if (!seg) continue;

      const node = getOrCreateNode(seg, nodeMap, nodes);

      if (prevNode && i > 0) {
        const arrow = arrows[i - 1] ?? '→';
        let edgeKind: EdgeKind = 'forward';
        if (arrow === '←') edgeKind = 'reverse';
        if (arrow === '↔') edgeKind = 'bidirectional';

        // Also check segment text for edge kind hints
        if (edgeKind === 'forward') {
          edgeKind = detectEdgeKind(seg);
        }

        const edge = createEdge(
          edgeKind === 'reverse' ? node.id : prevNode.id,
          edgeKind === 'reverse' ? prevNode.id : node.id,
          edgeKind,
        );
        edges.push(edge);
      }

      prevNode = node;
    }
  }

  const graph = createGraph(name, nodes, edges);
  applyLayout(graph);
  return { graph, warnings };
}

// ── Numbered List ──────────────────────────────────────────────────────────

function parseNumberedList(text: string, name: string, warnings: string[]): ParseResult {
  const lines = text.split(/\n/).filter((l) => /^\s*\d+[\.\)]/.test(l));
  const nodes: FlowNode[] = [];
  const edges: FlowEdge[] = [];
  const nodeMap = new Map<string, FlowNode>();

  let prevNode: FlowNode | null = null;

  for (const line of lines) {
    const label = line.replace(/^\s*\d+[\.\)]\s*/, '').trim();
    if (!label) continue;

    const node = getOrCreateNode(label, nodeMap, nodes);

    if (prevNode) {
      edges.push(createEdge(prevNode.id, node.id, detectEdgeKind(label)));
    }

    prevNode = node;
  }

  const graph = createGraph(name, nodes, edges);
  applyLayout(graph);
  return { graph, warnings };
}

// ── Pipe Syntax ────────────────────────────────────────────────────────────

function parsePipeSyntax(text: string, name: string, warnings: string[]): ParseResult {
  const segments = text.split(/\s*\|\s*/).filter(Boolean);
  const nodes: FlowNode[] = [];
  const edges: FlowEdge[] = [];
  const nodeMap = new Map<string, FlowNode>();

  let prevNode: FlowNode | null = null;

  for (const seg of segments) {
    const node = getOrCreateNode(seg.trim(), nodeMap, nodes);

    if (prevNode) {
      edges.push(createEdge(prevNode.id, node.id, detectEdgeKind(seg)));
    }

    prevNode = node;
  }

  const graph = createGraph(name, nodes, edges);
  applyLayout(graph);
  return { graph, warnings };
}

// ── Prose (Sentence-based) ─────────────────────────────────────────────────

function parseProse(text: string, name: string, warnings: string[]): ParseResult {
  // Split on transition phrases
  const splitPattern =
    /\s*(?:,\s*then\b|,\s*and\s+then\b|\bthen\b|,\s*next\b|\bnext\b|,\s*after\s+that\b|\bafter\s+that\b|,\s*finally\b|\bfinally\b|;\s*|\.\s+)/i;
  const segments = text
    .split(splitPattern)
    .map((s) => s.trim())
    .filter(Boolean);

  if (segments.length < 2) {
    // Can't parse as flow — create single node
    warnings.push(
      'Could not detect flow steps. Try using arrows (->), "then", pipes (|), or numbered steps.',
    );
    const node = createNode(detectKind(text), cleanLabel(text));
    const graph = createGraph(name, [node], []);
    return { graph, warnings };
  }

  const nodes: FlowNode[] = [];
  const edges: FlowEdge[] = [];
  const nodeMap = new Map<string, FlowNode>();

  let prevNode: FlowNode | null = null;

  for (const seg of segments) {
    const cleaned = cleanLabel(seg);
    if (!cleaned) continue;

    const node = getOrCreateNode(cleaned, nodeMap, nodes);

    if (prevNode) {
      edges.push(createEdge(prevNode.id, node.id, detectEdgeKind(seg)));
    }

    prevNode = node;
  }

  const graph = createGraph(name, nodes, edges);
  applyLayout(graph);
  return { graph, warnings };
}

// ── Shared Helpers ─────────────────────────────────────────────────────────

function getOrCreateNode(
  label: string,
  nodeMap: Map<string, FlowNode>,
  nodes: FlowNode[],
): FlowNode {
  const key = label.toLowerCase().replace(/\s+/g, ' ');
  if (nodeMap.has(key)) return nodeMap.get(key)!;

  const cleaned = cleanLabel(label);
  const kind = detectKind(label);
  const node = createNode(kind, cleaned);
  nodeMap.set(key, node);
  nodes.push(node);
  return node;
}

function cleanLabel(text: string): string {
  return text
    .replace(/^\s*(when|if|then|and|or|next|finally|after\s+that)\s+/i, '')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, 40);
}
