// ─── Canvas · Atoms ─────────────────────────────────────────────────────
// Pure types, constants, and functions for the visual workspace canvas.
// No side effects.

// ── Types (re-export from engine types) ────────────────────────────────

export type { Canvas, CanvasViewport, CanvasNode, CanvasEdge } from '../../engine/atoms/types';

// ── Constants ──────────────────────────────────────────────────────────

export const NODE_KINDS = [
  { value: 'text', label: 'Text', icon: 'edit_note' },
  { value: 'markdown', label: 'Markdown', icon: 'description' },
  { value: 'code', label: 'Code', icon: 'code' },
  { value: 'image', label: 'Image', icon: 'image' },
  { value: 'link', label: 'Link', icon: 'link' },
  { value: 'sticky', label: 'Sticky Note', icon: 'sticky_note_2' },
  { value: 'agent', label: 'Agent Output', icon: 'smart_toy' },
] as const;

export const NODE_COLORS = [
  '#ff00ff', '#6366f1', '#3b82f6', '#10b981', '#f59e0b',
  '#ef4444', '#8b5cf6', '#ec4899', '#06b6d4', '#84cc16',
];

export const EDGE_STYLES = ['solid', 'dashed', 'dotted'] as const;

export const DEFAULT_NODE_SIZE = { width: 240, height: 160 };
export const MIN_ZOOM = 0.1;
export const MAX_ZOOM = 3.0;
export const ZOOM_STEP = 0.1;
export const GRID_SIZE = 20;

// ── Pure Functions ─────────────────────────────────────────────────────

/** Snap a coordinate to the nearest grid point */
export function snapToGrid(value: number, gridSize = GRID_SIZE): number {
  return Math.round(value / gridSize) * gridSize;
}

/** Clamp zoom between min and max */
export function clampZoom(zoom: number): number {
  return Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, zoom));
}

/** Check if a point is within a node's bounding box */
export function pointInNode(
  px: number, py: number,
  nx: number, ny: number, nw: number, nh: number,
): boolean {
  return px >= nx && px <= nx + nw && py >= ny && py <= ny + nh;
}

/** Get the edge connection point on a node side */
export function getNodeCenter(x: number, y: number, w: number, h: number): { cx: number; cy: number } {
  return { cx: x + w / 2, cy: y + h / 2 };
}

/** Format node kind for display */
export function kindLabel(kind: string): string {
  return NODE_KINDS.find(k => k.value === kind)?.label ?? kind;
}

/** Format node kind icon */
export function kindIcon(kind: string): string {
  const name = NODE_KINDS.find(k => k.value === kind)?.icon ?? 'edit_note';
  return `<span class="ms ms-sm">${name}</span>`;
}
