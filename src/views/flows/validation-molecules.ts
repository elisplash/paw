// ─────────────────────────────────────────────────────────────────────────────
// Flow Visualization Engine — Connection Validation Molecules (Phase 5.6)
// Valid/invalid target highlighting during edge creation drag.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowNode, FlowEdge, FlowNodeKind, FlowGraph } from './atoms';

// ── Validation Rules ───────────────────────────────────────────────────────

/**
 * Rules for which node kinds can connect to each other.
 * Returns null if the connection is valid, or a reason string if invalid.
 */
export function validateConnection(
  fromNode: FlowNode,
  toNode: FlowNode,
  fromPort: string,
  toPort: string,
  existingEdges: FlowEdge[],
): string | null {
  // Can't connect to self
  if (fromNode.id === toNode.id) return 'Cannot connect a node to itself';

  // Check for duplicate edges (same from/to/ports)
  const dup = existingEdges.find(
    (e) =>
      e.from === fromNode.id &&
      e.to === toNode.id &&
      e.fromPort === fromPort &&
      e.toPort === toPort,
  );
  if (dup) return 'Connection already exists';

  // Error port should only connect to error-kind nodes
  if (fromPort === 'err' && toNode.kind !== 'error') {
    return 'Error ports should connect to error handler nodes';
  }

  // Output nodes shouldn't have outgoing connections
  if (fromNode.kind === 'output') return 'Output nodes have no outgoing connections';

  // Don't connect into trigger nodes (they're entry points)
  if (toNode.kind === 'trigger') return 'Cannot connect into a trigger node';

  return null; // Valid
}

/**
 * Check if connecting would create a cycle.
 * Uses BFS from toNode to see if it reaches fromNode.
 */
export function wouldCreateCycle(graph: FlowGraph, fromNodeId: string, toNodeId: string): boolean {
  // BFS forward from toNode — if we can reach fromNode, it's a cycle
  const visited = new Set<string>();
  const queue = [toNodeId];

  while (queue.length > 0) {
    const current = queue.shift()!;
    if (current === fromNodeId) return true;
    if (visited.has(current)) continue;
    visited.add(current);

    for (const edge of graph.edges) {
      if (edge.from === current && edge.kind !== 'reverse') {
        queue.push(edge.to);
      }
    }
  }

  return false;
}

// ── Port Compatibility ─────────────────────────────────────────────────────

/** Which node kinds are valid targets for a given source kind. */
const VALID_TARGETS: Record<string, FlowNodeKind[] | 'all'> = {
  trigger: 'all',
  agent: 'all',
  tool: 'all',
  condition: 'all',
  data: 'all',
  code: 'all',
  loop: 'all',
  squad: 'all',
  memory: 'all',
  'memory-recall': 'all',
  http: 'all',
  'mcp-tool': 'all',
  group: 'all',
  error: 'all',
  output: [], // output can't connect to anything
};

/**
 * Check if a source node kind can connect to a target node kind.
 */
export function isValidTargetKind(fromKind: FlowNodeKind, toKind: FlowNodeKind): boolean {
  const targets = VALID_TARGETS[fromKind];
  if (!targets) return false;
  if (targets === 'all') return toKind !== 'trigger'; // Can't connect TO triggers
  return targets.includes(toKind);
}

/**
 * Given a source node, return the classification of each potential target node:
 * 'valid' (green glow), 'invalid' (red), or 'neutral' (no highlight).
 */
export function classifyDropTargets(
  graph: FlowGraph,
  sourceNode: FlowNode,
  sourcePort: string,
): Map<string, 'valid' | 'invalid' | 'neutral'> {
  const result = new Map<string, 'valid' | 'invalid' | 'neutral'>();

  for (const node of graph.nodes) {
    if (node.id === sourceNode.id) {
      result.set(node.id, 'neutral');
      continue;
    }

    // Check kind compatibility
    if (!isValidTargetKind(sourceNode.kind, node.kind)) {
      result.set(node.id, 'invalid');
      continue;
    }

    // Check specific connection rules
    const reason = validateConnection(
      sourceNode,
      node,
      sourcePort,
      node.inputs[0] ?? 'in',
      graph.edges,
    );
    if (reason) {
      result.set(node.id, 'invalid');
      continue;
    }

    // Check cycle
    if (wouldCreateCycle(graph, sourceNode.id, node.id)) {
      result.set(node.id, 'invalid');
      continue;
    }

    result.set(node.id, 'valid');
  }

  return result;
}

// ── Port Snap ──────────────────────────────────────────────────────────────

const SNAP_DISTANCE = 20;

/** Check if a point is close enough to snap to a port. */
export function snapToPort(mouseX: number, mouseY: number, portX: number, portY: number): boolean {
  const dx = mouseX - portX;
  const dy = mouseY - portY;
  return Math.sqrt(dx * dx + dy * dy) <= SNAP_DISTANCE;
}
