// ─────────────────────────────────────────────────────────────────────────────
// Flow Visualization Engine — Atoms
// Pure data types, layout math, serialization. No DOM, no IPC.
// ─────────────────────────────────────────────────────────────────────────────

// ── Node Kinds ─────────────────────────────────────────────────────────────

export type FlowNodeKind =
  | 'trigger'    // Event that starts the flow (webhook, cron, user input)
  | 'agent'      // AI agent processing step
  | 'tool'       // MCP tool invocation
  | 'condition'  // If/else branch
  | 'data'       // Data transform / mapping
  | 'output'     // Terminal output (log, send, store)
  | 'group';     // Sub-flow / compound node

export type EdgeKind =
  | 'forward'       // Normal A → B
  | 'reverse'       // Pull: B ← A (data request)
  | 'bidirectional'; // Handshake: A ↔ B

export type FlowStatus = 'idle' | 'running' | 'success' | 'error' | 'paused';

// ── Core Types ─────────────────────────────────────────────────────────────

export interface FlowNode {
  id: string;
  kind: FlowNodeKind;
  label: string;
  /** Optional sub-label (model name, tool ID, etc.) */
  description?: string;
  /** Position on canvas (set by layout or drag) */
  x: number;
  y: number;
  /** Dimensions (computed from content, overridable) */
  width: number;
  height: number;
  /** Runtime status overlay */
  status: FlowStatus;
  /** Configuration payload (kind-specific) */
  config: Record<string, unknown>;
  /** Ports: named connection points */
  inputs: string[];
  outputs: string[];
}

export interface FlowEdge {
  id: string;
  kind: EdgeKind;
  /** Source node ID */
  from: string;
  /** Source port name (default: first output) */
  fromPort: string;
  /** Target node ID */
  to: string;
  /** Target port name (default: first input) */
  toPort: string;
  /** Optional label on the edge */
  label?: string;
  /** Condition expression (for condition edges) */
  condition?: string;
  /** Is this edge currently carrying data? (runtime) */
  active: boolean;
}

export interface FlowGraph {
  id: string;
  name: string;
  description?: string;
  nodes: FlowNode[];
  edges: FlowEdge[];
  /** Created timestamp */
  createdAt: string;
  /** Last modified */
  updatedAt: string;
}

// ── Constants ──────────────────────────────────────────────────────────────

export const NODE_DEFAULTS: Record<FlowNodeKind, { width: number; height: number; color: string; icon: string }> = {
  trigger:   { width: 160, height: 64, color: 'var(--warning)',      icon: 'bolt' },
  agent:     { width: 180, height: 72, color: 'var(--accent)',       icon: 'smart_toy' },
  tool:      { width: 180, height: 64, color: 'var(--kinetic-sage)', icon: 'build' },
  condition: { width: 140, height: 64, color: 'var(--status-info)',  icon: 'call_split' },
  data:      { width: 160, height: 56, color: 'var(--kinetic-gold)', icon: 'data_object' },
  output:    { width: 160, height: 64, color: 'var(--success)',      icon: 'output' },
  group:     { width: 240, height: 120, color: 'var(--border)',      icon: 'folder' },
};

export const GRID_SIZE = 20;
export const PORT_RADIUS = 5;
export const CANVAS_PADDING = 80;
export const MIN_NODE_SPACING_X = 240;
export const MIN_NODE_SPACING_Y = 100;

// ── Factory Functions ──────────────────────────────────────────────────────

let _nextId = 1;

export function genId(prefix = 'n'): string {
  return `${prefix}_${Date.now().toString(36)}_${(_nextId++).toString(36)}`;
}

export function createNode(
  kind: FlowNodeKind,
  label: string,
  x = 0,
  y = 0,
  overrides: Partial<FlowNode> = {},
): FlowNode {
  const defaults = NODE_DEFAULTS[kind];
  return {
    id: genId('node'),
    kind,
    label,
    x,
    y,
    width: defaults.width,
    height: defaults.height,
    status: 'idle',
    config: {},
    inputs: kind === 'trigger' ? [] : ['in'],
    outputs: kind === 'output' ? [] : ['out'],
    ...overrides,
  };
}

export function createEdge(
  from: string,
  to: string,
  kind: EdgeKind = 'forward',
  overrides: Partial<FlowEdge> = {},
): FlowEdge {
  return {
    id: genId('edge'),
    kind,
    from,
    fromPort: 'out',
    to,
    toPort: 'in',
    active: false,
    ...overrides,
  };
}

export function createGraph(name: string, nodes: FlowNode[] = [], edges: FlowEdge[] = []): FlowGraph {
  const now = new Date().toISOString();
  return {
    id: genId('flow'),
    name,
    nodes,
    edges,
    createdAt: now,
    updatedAt: now,
  };
}

// ── Layout (simple layered / left-to-right) ────────────────────────────────

/**
 * Compute adjacency layers for a DAG (modified Coffman-Graham).
 * Returns a Map of nodeId → { layer, order }.
 */
export function computeLayers(graph: FlowGraph): Map<string, { layer: number; order: number }> {
  const result = new Map<string, { layer: number; order: number }>();
  const inDegree = new Map<string, number>();
  const adj = new Map<string, string[]>();

  // Build adjacency
  for (const n of graph.nodes) {
    inDegree.set(n.id, 0);
    adj.set(n.id, []);
  }
  for (const e of graph.edges) {
    adj.get(e.from)?.push(e.to);
    inDegree.set(e.to, (inDegree.get(e.to) ?? 0) + 1);
  }

  // BFS-based layer assignment
  const queue: string[] = [];
  for (const [id, deg] of inDegree) {
    if (deg === 0) queue.push(id);
  }

  let layer = 0;
  while (queue.length) {
    const nextQueue: string[] = [];
    const layerNodes = [...queue];
    for (let order = 0; order < layerNodes.length; order++) {
      const nid = layerNodes[order];
      result.set(nid, { layer, order });
      for (const child of adj.get(nid) ?? []) {
        const newDeg = (inDegree.get(child) ?? 1) - 1;
        inDegree.set(child, newDeg);
        if (newDeg === 0) nextQueue.push(child);
      }
    }
    queue.length = 0;
    queue.push(...nextQueue);
    layer++;
  }

  // Handle orphans (nodes with no edges — shouldn't happen, but safety)
  for (const n of graph.nodes) {
    if (!result.has(n.id)) {
      result.set(n.id, { layer, order: 0 });
    }
  }

  return result;
}

/**
 * Apply layered position to nodes in-place.
 * Returns the bounding box { width, height }.
 */
export function applyLayout(graph: FlowGraph): { width: number; height: number } {
  const layers = computeLayers(graph);

  // Count nodes per layer for centering
  const layerCounts = new Map<number, number>();
  for (const { layer } of layers.values()) {
    layerCounts.set(layer, (layerCounts.get(layer) ?? 0) + 1);
  }

  let maxW = 0;
  let maxH = 0;

  for (const node of graph.nodes) {
    const pos = layers.get(node.id);
    if (!pos) continue;

    const layerCount = layerCounts.get(pos.layer) ?? 1;
    const colHeight = layerCount * MIN_NODE_SPACING_Y;

    node.x = CANVAS_PADDING + pos.layer * MIN_NODE_SPACING_X;
    node.y = CANVAS_PADDING + pos.order * MIN_NODE_SPACING_Y + (MIN_NODE_SPACING_Y - node.height) / 2;

    // Center small layers vertically
    if (layerCount < (layerCounts.get(0) ?? 1)) {
      const maxLayerHeight = (layerCounts.get(0) ?? 1) * MIN_NODE_SPACING_Y;
      node.y += (maxLayerHeight - colHeight) / 2;
    }

    maxW = Math.max(maxW, node.x + node.width + CANVAS_PADDING);
    maxH = Math.max(maxH, node.y + node.height + CANVAS_PADDING);
  }

  return { width: Math.max(maxW, 600), height: Math.max(maxH, 400) };
}

/**
 * Snap a coordinate to the nearest grid point.
 */
export function snapToGrid(val: number): number {
  return Math.round(val / GRID_SIZE) * GRID_SIZE;
}

// ── Edge Path Geometry ─────────────────────────────────────────────────────

export interface Point { x: number; y: number; }

/**
 * Compute the output port position for a node (right-center by default).
 */
export function getOutputPort(node: FlowNode, _portName = 'out'): Point {
  return { x: node.x + node.width, y: node.y + node.height / 2 };
}

/**
 * Compute the input port position for a node (left-center by default).
 */
export function getInputPort(node: FlowNode, _portName = 'in'): Point {
  return { x: node.x, y: node.y + node.height / 2 };
}

/**
 * Build an SVG cubic bezier path string between two points.
 * Uses horizontal control points for clean left-to-right flow.
 */
export function buildEdgePath(from: Point, to: Point): string {
  const dx = Math.abs(to.x - from.x);
  const cp = Math.max(dx * 0.5, 40);
  return `M ${from.x} ${from.y} C ${from.x + cp} ${from.y}, ${to.x - cp} ${to.y}, ${to.x} ${to.y}`;
}

/**
 * Build arrowhead marker path at a given angle.
 */
export function arrowPath(tip: Point, angle: number, size = 8): string {
  const a1 = angle + Math.PI * 0.85;
  const a2 = angle - Math.PI * 0.85;
  const p1 = { x: tip.x + size * Math.cos(a1), y: tip.y + size * Math.sin(a1) };
  const p2 = { x: tip.x + size * Math.cos(a2), y: tip.y + size * Math.sin(a2) };
  return `M ${p1.x} ${p1.y} L ${tip.x} ${tip.y} L ${p2.x} ${p2.y}`;
}

// ── Serialization ──────────────────────────────────────────────────────────

export function serializeGraph(graph: FlowGraph): string {
  return JSON.stringify(graph, null, 2);
}

export function deserializeGraph(json: string): FlowGraph | null {
  try {
    const obj = JSON.parse(json);
    if (obj && obj.nodes && obj.edges && obj.id) return obj as FlowGraph;
    return null;
  } catch {
    return null;
  }
}

// ── Hit Testing ────────────────────────────────────────────────────────────

/**
 * Find which node (if any) is at canvas position (cx, cy).
 */
export function hitTestNode(graph: FlowGraph, cx: number, cy: number): FlowNode | null {
  // Iterate in reverse so topmost (last-rendered) nodes are hit first
  for (let i = graph.nodes.length - 1; i >= 0; i--) {
    const n = graph.nodes[i];
    if (cx >= n.x && cx <= n.x + n.width && cy >= n.y && cy <= n.y + n.height) {
      return n;
    }
  }
  return null;
}

/**
 * Find which port (if any) is near canvas position (cx, cy).
 */
export function hitTestPort(
  graph: FlowGraph,
  cx: number,
  cy: number,
  radius = PORT_RADIUS * 3,
): { node: FlowNode; port: string; kind: 'input' | 'output' } | null {
  for (const node of graph.nodes) {
    for (const p of node.outputs) {
      const pos = getOutputPort(node, p);
      if (Math.hypot(cx - pos.x, cy - pos.y) < radius) {
        return { node, port: p, kind: 'output' };
      }
    }
    for (const p of node.inputs) {
      const pos = getInputPort(node, p);
      if (Math.hypot(cx - pos.x, cy - pos.y) < radius) {
        return { node, port: p, kind: 'input' };
      }
    }
  }
  return null;
}
