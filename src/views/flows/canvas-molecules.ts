// ─────────────────────────────────────────────────────────────────────────────
// Flow Visualization Engine — Canvas Molecules (Hub)
// Mount/unmount lifecycle, render orchestration, and canvas helpers.
// Sub-modules: canvas-state, canvas-render, canvas-interaction.
// ─────────────────────────────────────────────────────────────────────────────

import { type FlowGraph, GRID_SIZE } from './atoms';
import { getMoleculesState } from './molecule-state';
import { cs, svgEl, applyTransform } from './canvas-state';
import { renderNode, renderPorts, renderEdge } from './canvas-render';
import {
  setCanvasCallbacks,
  onMouseDown,
  onMouseMove,
  onMouseUp,
  onWheel,
  onDoubleClick,
} from './canvas-interaction';

/** Schedule a single renderGraph() call on the next animation frame. */
export function scheduleRender(): void {
  if (cs.renderScheduled) return;
  cs.renderScheduled = true;
  requestAnimationFrame(() => {
    cs.renderScheduled = false;
    renderGraph();
  });
}

// ── Index Maps ──────────────────────────────────────────────────────────────

function rebuildIndexes(graph: FlowGraph): void {
  cs.nodeMap = new Map(graph.nodes.map((n) => [n.id, n]));
  cs.outEdges = new Map();
  cs.inEdges = new Map();
  for (const n of graph.nodes) {
    cs.outEdges.set(n.id, []);
    cs.inEdges.set(n.id, []);
  }
  for (const e of graph.edges) {
    cs.outEdges.get(e.from)?.push(e);
    cs.inEdges.get(e.to)?.push(e);
  }
}

/** Mark a node ID as new so it gets the materialise entrance animation */
export function markNodeNew(id: string) {
  cs.newNodeIds.add(id);
}

/** Add a node ID to the new-node animation set (for toolbar use). */
export function addNewNodeId(id: string) {
  cs.newNodeIds.add(id);
}

// ── Canvas Placement Helpers (for toolbar node creation) ───────────────────

/** Get the center of the visible canvas area in graph coordinates. */
export function getCanvasCenter(): { x: number; y: number } {
  return { x: (-cs.panX + 400) / cs.zoom, y: (-cs.panY + 200) / cs.zoom };
}

/** Return current viewport state for minimap synchronisation. */
export function getCanvasViewport(): {
  panX: number;
  panY: number;
  zoom: number;
  width: number;
  height: number;
} {
  return {
    panX: cs.panX,
    panY: cs.panY,
    zoom: cs.zoom,
    width: cs.svg?.clientWidth ?? 800,
    height: cs.svg?.clientHeight ?? 600,
  };
}

/** Programmatically set pan / zoom (used by minimap drag). */
export function setPanZoom(panX: number, panY: number, zoom: number): void {
  cs.panX = panX;
  cs.panY = panY;
  cs.zoom = Math.max(cs.MIN_ZOOM, Math.min(cs.MAX_ZOOM, zoom));
  applyTransform();
}

/** Zoom in one step. */
export function zoomIn(): void {
  cs.zoom = Math.min(cs.MAX_ZOOM, cs.zoom * 1.2);
  applyTransform();
}

/** Zoom out one step. */
export function zoomOut(): void {
  cs.zoom = Math.max(cs.MIN_ZOOM, cs.zoom * 0.8);
  applyTransform();
}

// ── Mount / Unmount ────────────────────────────────────────────────────────

export function mountCanvas(container: HTMLElement) {
  container.innerHTML = '';

  cs.svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  cs.svg.setAttribute('class', 'flow-canvas');
  cs.svg.setAttribute('width', '100%');
  cs.svg.setAttribute('height', '100%');

  // Defs: arrow markers, glow filters
  const defs = svgEl('defs');
  defs.innerHTML = `
    <marker id="flow-arrow-fwd" markerWidth="10" markerHeight="8" refX="9" refY="4" orient="auto">
      <path d="M 0 0 L 10 4 L 0 8 Z" fill="var(--text-muted)"/>
    </marker>
    <marker id="flow-arrow-rev" markerWidth="10" markerHeight="8" refX="1" refY="4" orient="auto">
      <path d="M 10 0 L 0 4 L 10 8 Z" fill="var(--status-info)"/>
    </marker>
    <marker id="flow-arrow-bi-end" markerWidth="10" markerHeight="8" refX="9" refY="4" orient="auto">
      <path d="M 0 0 L 10 4 L 0 8 Z" fill="var(--kinetic-gold)"/>
    </marker>
    <marker id="flow-arrow-bi-start" markerWidth="10" markerHeight="8" refX="1" refY="4" orient="auto">
      <path d="M 10 0 L 0 4 L 10 8 Z" fill="var(--kinetic-gold)"/>
    </marker>
    <filter id="flow-glow" x="-20%" y="-20%" width="140%" height="140%">
      <feGaussianBlur stdDeviation="3" result="blur"/>
      <feMerge><feMergeNode in="blur"/><feMergeNode in="SourceGraphic"/></feMerge>
    </filter>
    <filter id="flow-selected-glow" x="-30%" y="-30%" width="160%" height="160%">
      <feGaussianBlur stdDeviation="5" result="blur"/>
      <feMerge><feMergeNode in="blur"/><feMergeNode in="SourceGraphic"/></feMerge>
    </filter>
    <pattern id="flow-grid" width="${GRID_SIZE}" height="${GRID_SIZE}" patternUnits="userSpaceOnUse">
      <circle cx="${GRID_SIZE / 2}" cy="${GRID_SIZE / 2}" r="0.5" fill="var(--border-subtle)"/>
    </pattern>
    <pattern id="flow-halftone" width="6" height="6" patternUnits="userSpaceOnUse">
      <circle cx="3" cy="3" r="0.5" fill="var(--kinetic-red, #FF4D4D)"/>
    </pattern>
    <filter id="flow-kinetic-glow" x="-30%" y="-30%" width="160%" height="160%">
      <feGaussianBlur stdDeviation="4" result="blur"/>
      <feFlood flood-color="var(--kinetic-red, #FF4D4D)" flood-opacity="0.15" result="color"/>
      <feComposite in="color" in2="blur" operator="in" result="glow"/>
      <feMerge><feMergeNode in="glow"/><feMergeNode in="SourceGraphic"/></feMerge>
    </filter>
  `;
  cs.svg.appendChild(defs);

  // Background grid
  const bg = svgEl('rect');
  bg.setAttribute('class', 'flow-bg');
  bg.setAttribute('width', '10000');
  bg.setAttribute('height', '10000');
  bg.setAttribute('x', '-5000');
  bg.setAttribute('y', '-5000');
  bg.setAttribute('fill', 'url(#flow-grid)');
  cs.svg.appendChild(bg);

  // Groups in z-order
  cs.edgesGroup = svgEl('g') as SVGGElement;
  cs.edgesGroup.setAttribute('class', 'flow-edges');
  cs.svg.appendChild(cs.edgesGroup);

  cs.portsGroup = svgEl('g') as SVGGElement;
  cs.portsGroup.setAttribute('class', 'flow-ports');
  cs.svg.appendChild(cs.portsGroup);

  cs.nodesGroup = svgEl('g') as SVGGElement;
  cs.nodesGroup.setAttribute('class', 'flow-nodes');
  cs.svg.appendChild(cs.nodesGroup);

  cs.dragPreviewGroup = svgEl('g') as SVGGElement;
  cs.dragPreviewGroup.setAttribute('class', 'flow-drag-preview');
  cs.svg.appendChild(cs.dragPreviewGroup);

  container.appendChild(cs.svg);

  // Wire interaction callbacks before adding event listeners
  setCanvasCallbacks(renderGraph, updateDraggedNodePosition);

  // Wire events
  cs.svg.addEventListener('mousedown', onMouseDown);
  cs.svg.addEventListener('mousemove', onMouseMove);
  cs.svg.addEventListener('mouseup', onMouseUp);
  cs.svg.addEventListener('wheel', onWheel, { passive: false });
  cs.svg.addEventListener('dblclick', onDoubleClick);

  applyTransform();
}

export function unmountCanvas() {
  if (cs.svg) {
    cs.svg.removeEventListener('mousedown', onMouseDown);
    cs.svg.removeEventListener('mousemove', onMouseMove);
    cs.svg.removeEventListener('mouseup', onMouseUp);
    cs.svg.removeEventListener('wheel', onWheel);
    cs.svg.removeEventListener('dblclick', onDoubleClick);
    cs.svg.remove();
    cs.svg = null;
  }
  cs.nodesGroup = null;
  cs.edgesGroup = null;
  cs.portsGroup = null;
  cs.dragPreviewGroup = null;
}

// ── Phase 0.2b: Dirty-region rendering during drag ─────────────────────────

function updateDraggedNodePosition(nodeId: string): void {
  if (!cs.nodesGroup || !cs.edgesGroup || !cs.portsGroup) return;
  const node = cs.nodeMap.get(nodeId);
  if (!node) return;

  const nodeG = cs.nodesGroup.querySelector(`[data-node-id="${nodeId}"]`) as SVGGElement | null;
  if (nodeG) {
    nodeG.setAttribute('transform', `translate(${node.x}, ${node.y})`);
    (nodeG as unknown as HTMLElement).style.setProperty('--node-tx', `${node.x}px`);
    (nodeG as unknown as HTMLElement).style.setProperty('--node-ty', `${node.y}px`);
  }

  const oldPorts = cs.portsGroup.querySelectorAll(`[data-node-id="${nodeId}"]`);
  oldPorts.forEach((p) => p.remove());
  renderPorts(node);

  const connectedEdges = [...(cs.outEdges.get(nodeId) ?? []), ...(cs.inEdges.get(nodeId) ?? [])];
  for (const edge of connectedEdges) {
    const oldEdgeEl = cs.edgesGroup.querySelector(`[data-edge-id="${edge.id}"]`);
    if (oldEdgeEl) oldEdgeEl.remove();
    const fromNode = cs.nodeMap.get(edge.from);
    const toNode = cs.nodeMap.get(edge.to);
    if (fromNode && toNode) {
      cs.edgesGroup.appendChild(renderEdge(edge, fromNode, toNode));
    }
  }
}

// ── Full Render ────────────────────────────────────────────────────────────

export function renderGraph() {
  const _state = getMoleculesState();
  const graph = _state?.getGraph();
  if (!graph || !cs.nodesGroup || !cs.edgesGroup || !cs.portsGroup) return;

  rebuildIndexes(graph);

  cs.edgesGroup.innerHTML = '';
  cs.nodesGroup.innerHTML = '';
  cs.portsGroup.innerHTML = '';

  for (const edge of graph.edges) {
    const fromNode = cs.nodeMap.get(edge.from);
    const toNode = cs.nodeMap.get(edge.to);
    if (fromNode && toNode) {
      cs.edgesGroup.appendChild(renderEdge(edge, fromNode, toNode));
    }
  }

  const selectedId = _state.getSelectedNodeId();
  const selectedIds = _state.getSelectedNodeIds();
  for (const node of graph.nodes) {
    const isSelected = selectedIds.size > 0 ? selectedIds.has(node.id) : node.id === selectedId;
    cs.nodesGroup.appendChild(renderNode(node, isSelected));
    renderPorts(node);
  }
}

// ── Utilities ──────────────────────────────────────────────────────────────

export function fitView() {
  if (!cs.svg) return;
  const _state = getMoleculesState();
  if (!_state) return;
  const graph = _state.getGraph();
  if (!graph || !graph.nodes.length) return;

  let minX = Infinity,
    minY = Infinity,
    maxX = -Infinity,
    maxY = -Infinity;
  for (const n of graph.nodes) {
    minX = Math.min(minX, n.x);
    minY = Math.min(minY, n.y);
    maxX = Math.max(maxX, n.x + n.width);
    maxY = Math.max(maxY, n.y + n.height);
  }

  const rect = cs.svg.getBoundingClientRect();
  const graphW = maxX - minX + 80;
  const graphH = maxY - minY + 80;
  cs.zoom = Math.min(rect.width / graphW, rect.height / graphH, 1.5);
  cs.panX = (rect.width - graphW * cs.zoom) / 2 - minX * cs.zoom + 40;
  cs.panY = (rect.height - graphH * cs.zoom) / 2 - minY * cs.zoom + 40;

  applyTransform();
}

export function deleteSelected() {
  const _state = getMoleculesState();
  if (!_state) return;
  const graph = _state.getGraph();
  if (!graph) return;

  const selectedIds = _state.getSelectedNodeIds();
  const selectedId = _state.getSelectedNodeId();
  const idsToDelete =
    selectedIds.size > 0 ? selectedIds : selectedId ? new Set([selectedId]) : new Set<string>();

  if (idsToDelete.size === 0) return;

  graph.nodes = graph.nodes.filter((n) => !idsToDelete.has(n.id));
  graph.edges = graph.edges.filter((e) => !idsToDelete.has(e.from) && !idsToDelete.has(e.to));
  _state.setSelectedNodeId(null);
  _state.setSelectedNodeIds(new Set());
  _state.onGraphChanged();
  renderGraph();
}

export function resetView() {
  cs.panX = 0;
  cs.panY = 0;
  cs.zoom = 1;
  applyTransform();
}
