// ─── Canvas · Molecules ─────────────────────────────────────────────────
// Composed functions with side effects: Tauri IPC calls.
// Builds on atoms for canvas, node, and edge management.

import type { Canvas, CanvasViewport, CanvasNode, CanvasEdge } from './atoms';

// Dynamic import to avoid circular dep
const getEngine = async () => {
  const { pawEngine } = await import('../../engine/molecules/ipc_client');
  return pawEngine;
};

// ── Canvas CRUD ────────────────────────────────────────────────────────

/** List all canvases, ordered by last updated */
export async function listCanvases(): Promise<Canvas[]> {
  const engine = await getEngine();
  return engine.canvasList();
}

/** Create a new canvas with a name and optional description */
export async function createCanvas(name: string, description?: string): Promise<Canvas> {
  const engine = await getEngine();
  return engine.canvasCreate(name, description);
}

/** Update canvas metadata (name, description, and/or viewport) */
export async function updateCanvas(
  id: string,
  name?: string,
  description?: string,
  viewport?: CanvasViewport,
): Promise<void> {
  const engine = await getEngine();
  return engine.canvasUpdate(id, name, description, viewport);
}

/** Delete a canvas and all its nodes/edges */
export async function deleteCanvas(id: string): Promise<void> {
  const engine = await getEngine();
  return engine.canvasDelete(id);
}

// ── Node CRUD ──────────────────────────────────────────────────────────

/** Load all nodes for a canvas */
export async function loadNodes(canvasId: string): Promise<CanvasNode[]> {
  const engine = await getEngine();
  return engine.canvasNodes(canvasId);
}

/** Create a node on a canvas */
export async function createNode(
  canvasId: string,
  kind: string,
  x: number,
  y: number,
  title?: string,
  content?: string,
  width?: number,
  height?: number,
  color?: string,
  metadata?: string,
): Promise<CanvasNode> {
  const engine = await getEngine();
  return engine.canvasNodeCreate(canvasId, kind, x, y, title, content, width, height, color, metadata);
}

/** Update node position, content, appearance, or metadata */
export async function updateNode(
  id: string,
  opts: {
    x?: number; y?: number;
    width?: number; height?: number;
    title?: string; content?: string;
    color?: string; zIndex?: number;
    collapsed?: boolean; metadata?: string;
  },
): Promise<void> {
  const engine = await getEngine();
  return engine.canvasNodeUpdate(
    id, opts.x, opts.y, opts.width, opts.height,
    opts.title, opts.content, opts.color, opts.zIndex,
    opts.collapsed, opts.metadata,
  );
}

/** Delete a node and its edges */
export async function deleteNode(id: string): Promise<void> {
  const engine = await getEngine();
  return engine.canvasNodeDelete(id);
}

// ── Edge CRUD ──────────────────────────────────────────────────────────

/** Load all edges for a canvas */
export async function loadEdges(canvasId: string): Promise<CanvasEdge[]> {
  const engine = await getEngine();
  return engine.canvasEdges(canvasId);
}

/** Create an edge between two nodes */
export async function createEdge(
  canvasId: string,
  fromNode: string,
  toNode: string,
  label?: string,
  color?: string,
  style?: string,
): Promise<CanvasEdge> {
  const engine = await getEngine();
  return engine.canvasEdgeCreate(canvasId, fromNode, toNode, label, color, style);
}

/** Delete an edge */
export async function deleteEdge(id: string): Promise<void> {
  const engine = await getEngine();
  return engine.canvasEdgeDelete(id);
}
