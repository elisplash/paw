// ─────────────────────────────────────────────────────────────────────────────
// Flow Visualization Engine — Atoms
// Pure data types, layout math, serialization. No DOM, no IPC.
// ─────────────────────────────────────────────────────────────────────────────

// ── Node Kinds ─────────────────────────────────────────────────────────────

export type FlowNodeKind =
  | 'trigger' // Event that starts the flow (webhook, cron, user input)
  | 'agent' // AI agent processing step
  | 'tool' // MCP tool invocation
  | 'condition' // If/else branch
  | 'data' // Data transform / mapping
  | 'code' // Inline JavaScript evaluation (sandboxed)
  | 'output' // Terminal output (log, send, store)
  | 'error' // Error handler (logs, alerts, notifications)
  | 'group' // Sub-flow / compound node
  | 'http' // Direct HTTP request (no LLM — Conductor Extract)
  | 'mcp-tool' // Direct MCP tool call (no LLM — Conductor Extract)
  | 'loop' // ForEach iterator over arrays
  | 'squad' // Multi-agent squad invocation
  | 'memory' // Write to agent memory (Librarian)
  | 'memory-recall'; // Search/read agent memory (Librarian)

export type EdgeKind =
  | 'forward' // Normal A → B
  | 'reverse' // Pull: B ← A (data request)
  | 'bidirectional' // Handshake: A ↔ B
  | 'error'; // Error path: A --err--> B (fallback)

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
  /** Folder for organization (empty string or undefined = root) */
  folder?: string;
  nodes: FlowNode[];
  edges: FlowEdge[];
  /** Flow-level variables: key-value store accessible via {{flow.key}} */
  variables?: Record<string, unknown>;
  /** Created timestamp */
  createdAt: string;
  /** Last modified */
  updatedAt: string;
}

// ── Template Types ─────────────────────────────────────────────────────────

export type FlowTemplateCategory =
  | 'ai'
  | 'communication'
  | 'devops'
  | 'productivity'
  | 'data'
  | 'research'
  | 'social'
  | 'finance'
  | 'support'
  | 'custom';

export interface FlowTemplate {
  id: string;
  name: string;
  description: string;
  category: FlowTemplateCategory;
  tags: string[];
  /** Icon name (Material Symbols) */
  icon: string;
  /** Node definitions (will be cloned with fresh IDs on instantiation) */
  nodes: Array<{
    kind: FlowNodeKind;
    label: string;
    description?: string;
    config?: Record<string, unknown>;
  }>;
  /** Edge definitions (by index: from nodes[fromIdx] → nodes[toIdx]) */
  edges: Array<{
    fromIdx: number;
    toIdx: number;
    kind?: EdgeKind;
    label?: string;
    condition?: string;
  }>;
}

/** Category display metadata */
export const TEMPLATE_CATEGORIES: Record<
  FlowTemplateCategory,
  { label: string; icon: string; color: string }
> = {
  ai: { label: 'AI & Agents', icon: 'psychology', color: 'var(--kinetic-red, #FF4D4D)' },
  communication: { label: 'Communication', icon: 'forum', color: 'var(--kinetic-sage, #8FB0A0)' },
  devops: { label: 'DevOps & CI', icon: 'build_circle', color: 'var(--kinetic-steel, #7A8B9A)' },
  productivity: { label: 'Productivity', icon: 'task_alt', color: 'var(--kinetic-gold, #D4A853)' },
  data: { label: 'Data & Transform', icon: 'data_object', color: 'var(--kinetic-steel, #7A8B9A)' },
  research: { label: 'Research', icon: 'science', color: 'var(--kinetic-sage, #8FB0A0)' },
  social: { label: 'Social & Content', icon: 'share', color: 'var(--kinetic-gold, #D4A853)' },
  finance: {
    label: 'Finance & Trading',
    icon: 'trending_up',
    color: 'var(--kinetic-red, #FF4D4D)',
  },
  support: { label: 'Support', icon: 'support_agent', color: 'var(--kinetic-sage, #8FB0A0)' },
  custom: { label: 'Custom', icon: 'tune', color: 'var(--text-muted)' },
};

/**
 * Instantiate a template into a FlowGraph with fresh IDs and layout.
 */
export function instantiateTemplate(template: FlowTemplate): FlowGraph {
  const nodes: FlowNode[] = template.nodes.map((spec, _i) =>
    createNode(spec.kind, spec.label, 0, 0, {
      description: spec.description,
      config: spec.config ? { ...spec.config } : {},
    }),
  );

  const edges: FlowEdge[] = template.edges
    .filter((e) => e.fromIdx < nodes.length && e.toIdx < nodes.length)
    .map((e) =>
      createEdge(nodes[e.fromIdx].id, nodes[e.toIdx].id, e.kind ?? 'forward', {
        label: e.label,
        condition: e.condition,
      }),
    );

  const graph = createGraph(template.name, nodes, edges);
  graph.description = template.description;
  applyLayout(graph);
  return graph;
}

/**
 * Filter templates by category and/or search query.
 */
export function filterTemplates(
  templates: FlowTemplate[],
  category: FlowTemplateCategory | 'all',
  query: string,
): FlowTemplate[] {
  let filtered = templates;
  if (category !== 'all') {
    filtered = filtered.filter((t) => t.category === category);
  }
  if (query.trim()) {
    const q = query.toLowerCase();
    filtered = filtered.filter(
      (t) =>
        t.name.toLowerCase().includes(q) ||
        t.description.toLowerCase().includes(q) ||
        t.tags.some((tag) => tag.toLowerCase().includes(q)),
    );
  }
  return filtered;
}

// ── Constants ──────────────────────────────────────────────────────────────

export const NODE_DEFAULTS: Record<
  FlowNodeKind,
  { width: number; height: number; color: string; icon: string }
> = {
  trigger: { width: 160, height: 64, color: 'var(--warning)', icon: 'bolt' },
  agent: { width: 180, height: 72, color: 'var(--accent)', icon: 'smart_toy' },
  tool: { width: 180, height: 64, color: 'var(--kinetic-sage)', icon: 'build' },
  condition: { width: 140, height: 64, color: 'var(--status-info)', icon: 'call_split' },
  data: { width: 160, height: 56, color: 'var(--kinetic-gold)', icon: 'data_object' },
  code: { width: 180, height: 72, color: 'var(--kinetic-steel)', icon: 'code' },
  output: { width: 160, height: 64, color: 'var(--success)', icon: 'output' },
  error: { width: 180, height: 72, color: 'var(--kinetic-red, #D64045)', icon: 'error' },
  group: { width: 240, height: 120, color: 'var(--border)', icon: 'folder' },
  http: { width: 180, height: 72, color: 'var(--kinetic-sage, #5BA08C)', icon: 'http' },
  'mcp-tool': {
    width: 180,
    height: 72,
    color: 'var(--kinetic-steel, #7A8B9A)',
    icon: 'integration_instructions',
  },
  loop: { width: 180, height: 80, color: 'var(--kinetic-gold, #D4A853)', icon: 'repeat' },
  squad: { width: 200, height: 80, color: 'var(--kinetic-purple, #A855F7)', icon: 'groups' },
  memory: { width: 180, height: 72, color: 'var(--kinetic-sage, #5BA08C)', icon: 'save' },
  'memory-recall': {
    width: 180,
    height: 72,
    color: 'var(--kinetic-gold, #D4A853)',
    icon: 'manage_search',
  },
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
    outputs: kind === 'output' ? [] : kind === 'error' ? [] : ['out', 'err'],
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

export function createGraph(
  name: string,
  nodes: FlowNode[] = [],
  edges: FlowEdge[] = [],
): FlowGraph {
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
    node.y =
      CANVAS_PADDING + pos.order * MIN_NODE_SPACING_Y + (MIN_NODE_SPACING_Y - node.height) / 2;

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

export interface Point {
  x: number;
  y: number;
}

/**
 * Compute the output port position for a node.
 * 'out' → right-center, 'err' → right-bottom-quarter.
 */
export function getOutputPort(node: FlowNode, portName = 'out'): Point {
  if (portName === 'err') {
    return { x: node.x + node.width, y: node.y + node.height * 0.8 };
  }
  return { x: node.x + node.width, y: node.y + node.height * 0.35 };
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

// ── Undo/Redo Command Stack ───────────────────────────────────────────────

const MAX_UNDO_STACK = 50;

export interface UndoStack {
  /** Past snapshots (most recent last). */
  past: string[];
  /** Future snapshots for redo (most recent first). */
  future: string[];
}

export function createUndoStack(): UndoStack {
  return { past: [], future: [] };
}

/**
 * Push a snapshot of the current graph onto the undo stack.
 * Clears the redo (future) stack — any new mutation invalidates redo history.
 */
export function pushUndo(stack: UndoStack, graph: FlowGraph): void {
  stack.past.push(serializeGraph(graph));
  if (stack.past.length > MAX_UNDO_STACK) {
    stack.past.shift(); // drop oldest
  }
  stack.future = []; // new mutation clears redo
}

/**
 * Undo: pops the last snapshot from past, pushes current graph to future,
 * returns the restored graph (or null if nothing to undo).
 */
export function undo(stack: UndoStack, currentGraph: FlowGraph): FlowGraph | null {
  const snapshot = stack.past.pop();
  if (!snapshot) return null;
  // Save current state for redo
  stack.future.unshift(serializeGraph(currentGraph));
  return deserializeGraph(snapshot);
}

/**
 * Redo: pops the first snapshot from future, pushes current to past,
 * returns the restored graph (or null if nothing to redo).
 */
export function redo(stack: UndoStack, currentGraph: FlowGraph): FlowGraph | null {
  const snapshot = stack.future.shift();
  if (!snapshot) return null;
  // Save current state for undo
  stack.past.push(serializeGraph(currentGraph));
  return deserializeGraph(snapshot);
}

/** Check if undo is available. */
export function canUndo(stack: UndoStack): boolean {
  return stack.past.length > 0;
}

/** Check if redo is available. */
export function canRedo(stack: UndoStack): boolean {
  return stack.future.length > 0;
}
