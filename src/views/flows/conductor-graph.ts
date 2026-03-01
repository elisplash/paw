// ─────────────────────────────────────────────────────────────────────────────
// Conductor Protocol — Graph Analysis Atoms
// Node classification, adjacency, cycle detection, depth computation.
// Pure logic, no DOM, no IPC.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowGraph, FlowNode, FlowEdge, FlowNodeKind } from './atoms';
import { getNodeExecConfig } from './executor-atoms';

// ── Node Classification ────────────────────────────────────────────────────

/** Classification of how a node should execute. */
export type NodeExecClassification =
  | 'agent' // Needs LLM call (agent, data, semantic condition)
  | 'direct' // Deterministic — bypass LLM (tool, code, http, mcp-tool, output, error)
  | 'passthrough'; // No execution needed (trigger, output with no transform)

/** Kinds that bypass LLM entirely — direct execution. */
const DIRECT_KINDS: Set<FlowNodeKind> = new Set([
  'tool',
  'code',
  'output',
  'error',
  'http' as FlowNodeKind,
  'mcp-tool' as FlowNodeKind,
  'loop' as FlowNodeKind,
  'group',
  'memory' as FlowNodeKind,
  'memory-recall' as FlowNodeKind,
]);

/** Kinds that are passthrough (no real execution). */
const PASSTHROUGH_KINDS: Set<FlowNodeKind> = new Set(['trigger']);

/**
 * Classify how a node should be executed.
 * - Direct nodes bypass LLM (deterministic actions)
 * - Agent nodes need LLM calls
 * - Passthrough nodes just forward data
 */
export function classifyNode(node: FlowNode): NodeExecClassification {
  if (PASSTHROUGH_KINDS.has(node.kind)) return 'passthrough';
  if (DIRECT_KINDS.has(node.kind)) return 'direct';

  // Squad nodes invoke multi-agent teams — always agent, never collapse
  if (node.kind === ('squad' as FlowNodeKind)) return 'agent';

  // Tool nodes with no prompt are direct (action-only)
  if (node.kind === 'tool') {
    const config = getNodeExecConfig(node);
    if (!config.prompt) return 'direct';
  }

  // Condition nodes: check if they have a structured expression (direct eval)
  // or need AI evaluation (agent)
  if (node.kind === 'condition') {
    const config = getNodeExecConfig(node);
    if (config.conditionExpr && isStructuredCondition(config.conditionExpr)) {
      return 'direct';
    }
    return 'agent';
  }

  return 'agent';
}

/**
 * Check if a condition expression can be evaluated structurally (no AI needed).
 * Supports: comparisons, boolean literals, simple expressions.
 */
export function isStructuredCondition(expr: string): boolean {
  const normalized = expr.trim().toLowerCase();
  // Boolean literals
  if (['true', 'false', 'yes', 'no'].includes(normalized)) return true;
  // Simple comparisons: >, <, >=, <=, ===, !==, ==, !=
  if (/^.+\s*(===|!==|>=|<=|==|!=|>|<)\s*.+$/.test(normalized)) return true;
  // Property access patterns: input.status, data.length
  if (/^[a-z_$][\w$.]*\s*(===|!==|>=|<=|==|!=|>|<)\s*.+$/i.test(normalized)) return true;
  return false;
}

// ── Graph Analysis ─────────────────────────────────────────────────────────

/**
 * Build adjacency info for the graph.
 * Returns maps for forward edges and reverse lookup.
 */
export function buildAdjacency(graph: FlowGraph): {
  forward: Map<string, string[]>;
  backward: Map<string, string[]>;
  edgeMap: Map<string, FlowEdge>;
} {
  const forward = new Map<string, string[]>();
  const backward = new Map<string, string[]>();
  const edgeMap = new Map<string, FlowEdge>();

  for (const n of graph.nodes) {
    forward.set(n.id, []);
    backward.set(n.id, []);
  }

  for (const e of graph.edges) {
    if (e.kind === 'reverse') continue;
    forward.get(e.from)?.push(e.to);
    backward.get(e.to)?.push(e.from);
    edgeMap.set(e.id, e);
  }

  return { forward, backward, edgeMap };
}

/**
 * Detect cycles in the graph using DFS.
 * Returns sets of node IDs that participate in cycles.
 */
export function detectCycles(graph: FlowGraph): Set<string>[] {
  const { forward } = buildAdjacency(graph);
  const visited = new Set<string>();
  const inStack = new Set<string>();
  const cycles: Set<string>[] = [];

  function dfs(nodeId: string, path: string[]): void {
    if (inStack.has(nodeId)) {
      const cycleStart = path.indexOf(nodeId);
      if (cycleStart >= 0) {
        const cycle = new Set<string>();
        for (let i = cycleStart; i < path.length; i++) {
          cycle.add(path[i]);
        }
        cycles.push(cycle);
      }
      return;
    }
    if (visited.has(nodeId)) return;

    visited.add(nodeId);
    inStack.add(nodeId);
    path.push(nodeId);

    for (const child of forward.get(nodeId) ?? []) {
      dfs(child, path);
    }

    path.pop();
    inStack.delete(nodeId);
  }

  for (const node of graph.nodes) {
    if (!visited.has(node.id)) {
      dfs(node.id, []);
    }
  }

  return cycles;
}

/**
 * Compute depth levels for nodes in a DAG (ignoring cycles).
 * Returns Map<nodeId, depth> where depth 0 = root nodes.
 */
export function computeDepthLevels(graph: FlowGraph, cycleNodes: Set<string>): Map<string, number> {
  const { forward, backward } = buildAdjacency(graph);
  const depths = new Map<string, number>();
  const inDegree = new Map<string, number>();

  // Only consider non-cycle nodes
  const acyclicNodes = graph.nodes.filter((n) => !cycleNodes.has(n.id));

  for (const n of acyclicNodes) {
    const inEdges = (backward.get(n.id) ?? []).filter((id) => !cycleNodes.has(id));
    inDegree.set(n.id, inEdges.length);
  }

  // BFS layer assignment
  const queue: string[] = [];
  for (const n of acyclicNodes) {
    if ((inDegree.get(n.id) ?? 0) === 0) {
      queue.push(n.id);
      depths.set(n.id, 0);
    }
  }

  while (queue.length > 0) {
    const nodeId = queue.shift()!;
    const depth = depths.get(nodeId) ?? 0;

    for (const child of forward.get(nodeId) ?? []) {
      if (cycleNodes.has(child)) continue;
      const newDeg = (inDegree.get(child) ?? 1) - 1;
      inDegree.set(child, newDeg);
      const existingDepth = depths.get(child) ?? 0;
      depths.set(child, Math.max(existingDepth, depth + 1));
      if (newDeg === 0) {
        queue.push(child);
      }
    }
  }

  return depths;
}
