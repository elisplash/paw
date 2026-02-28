// ─────────────────────────────────────────────────────────────────────────────
// Canvas State — Shared mutable state for canvas sub-modules
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowNode, FlowEdge } from './atoms';

/** Shared canvas state accessed by canvas-molecules, canvas-render, canvas-interaction. */
export const cs = {
  svg: null as SVGSVGElement | null,
  nodesGroup: null as SVGGElement | null,
  edgesGroup: null as SVGGElement | null,
  portsGroup: null as SVGGElement | null,
  dragPreviewGroup: null as SVGGElement | null,

  // Pan & zoom
  panX: 0,
  panY: 0,
  zoom: 1,
  MIN_ZOOM: 0.25,
  MAX_ZOOM: 2,

  // Drag state
  dragging: null as { nodeId: string; offsetX: number; offsetY: number } | null,
  panning: false,
  panStartX: 0,
  panStartY: 0,

  // Edge drawing
  drawingEdge: null as {
    fromNodeId: string;
    fromPort: string;
    cursorX: number;
    cursorY: number;
  } | null,

  // Rubber-band selection
  rubberBand: null as {
    startX: number;
    startY: number;
    cursorX: number;
    cursorY: number;
  } | null,
  rubberBandEl: null as SVGRectElement | null,

  // Render throttle
  renderScheduled: false,

  // Index maps
  nodeMap: new Map<string, FlowNode>(),
  outEdges: new Map<string, FlowEdge[]>(),
  inEdges: new Map<string, FlowEdge[]>(),

  // Animation
  newNodeIds: new Set<string>(),
};

// ── SVG Utilities ──────────────────────────────────────────────────────────

export function svgEl(tag: string): SVGElement {
  return document.createElementNS('http://www.w3.org/2000/svg', tag);
}

export function truncate(s: string, max: number): string {
  return s.length > max ? `${s.slice(0, max - 1)}…` : s;
}

/** Apply current pan/zoom transform to all canvas groups. */
export function applyTransform(): void {
  if (!cs.svg) return;
  const groups = [cs.edgesGroup, cs.nodesGroup, cs.portsGroup, cs.dragPreviewGroup];
  for (const g of groups) {
    g?.setAttribute('transform', `translate(${cs.panX}, ${cs.panY}) scale(${cs.zoom})`);
  }
  const bg = cs.svg.querySelector('.flow-bg');
  bg?.setAttribute('transform', `translate(${cs.panX}, ${cs.panY}) scale(${cs.zoom})`);
}
