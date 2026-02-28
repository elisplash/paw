// ─────────────────────────────────────────────────────────────────────────────
// Flow Visualization Engine — Canvas Molecules
// SVG canvas rendering, node/edge drawing, interaction handlers (pan, zoom,
// drag, connect, rubber-band selection), and mount/unmount lifecycle.
// ─────────────────────────────────────────────────────────────────────────────

import {
  type FlowGraph,
  type FlowNode,
  type FlowEdge,
  type Point,
  NODE_DEFAULTS,
  PORT_RADIUS,
  GRID_SIZE,
  getOutputPort,
  getInputPort,
  buildEdgePath,
  hitTestNode,
  hitTestPort,
  snapToGrid,
  createEdge,
} from './atoms';
import {
  getMoleculesState,
  getDebugBreakpoints,
  getDebugCursorNodeId,
  getDebugEdgeValues,
  getSelectedEdgeIdLocal,
  setSelectedEdgeIdLocal,
} from './molecule-state';

// ── Canvas State ───────────────────────────────────────────────────────────

let _svg: SVGSVGElement | null = null;
let _nodesGroup: SVGGElement | null = null;
let _edgesGroup: SVGGElement | null = null;
let _portsGroup: SVGGElement | null = null;
let _dragPreviewGroup: SVGGElement | null = null;

// Pan & zoom
let _panX = 0;
let _panY = 0;
let _zoom = 1;
const MIN_ZOOM = 0.25;
const MAX_ZOOM = 2;

// Drag state
let _dragging: { nodeId: string; offsetX: number; offsetY: number } | null = null;
let _panning = false;
let _panStartX = 0;
let _panStartY = 0;

// Edge drawing state
let _drawingEdge: {
  fromNodeId: string;
  fromPort: string;
  cursorX: number;
  cursorY: number;
} | null = null;

// Phase 3.5: Rubber-band box selection
let _rubberBand: {
  startX: number;
  startY: number;
  cursorX: number;
  cursorY: number;
} | null = null;
let _rubberBandEl: SVGRectElement | null = null;

// ── Phase 0.2: rAF render throttle ──────────────────────────────────────
let _renderScheduled = false;

/** Schedule a single renderGraph() call on the next animation frame. */
export function scheduleRender(): void {
  if (_renderScheduled) return;
  _renderScheduled = true;
  requestAnimationFrame(() => {
    _renderScheduled = false;
    renderGraph();
  });
}

// ── Phase 0.3: Node & edge index maps ───────────────────────────────────
let _nodeMap: Map<string, FlowNode> = new Map();
let _outEdges: Map<string, FlowEdge[]> = new Map();
let _inEdges: Map<string, FlowEdge[]> = new Map();

function rebuildIndexes(graph: FlowGraph): void {
  _nodeMap = new Map(graph.nodes.map((n) => [n.id, n]));
  _outEdges = new Map();
  _inEdges = new Map();
  for (const n of graph.nodes) {
    _outEdges.set(n.id, []);
    _inEdges.set(n.id, []);
  }
  for (const e of graph.edges) {
    _outEdges.get(e.from)?.push(e);
    _inEdges.get(e.to)?.push(e);
  }
}

// Track recently-added nodes for materialise entrance animation
const _newNodeIds = new Set<string>();

/** Mark a node ID as new so it gets the materialise entrance animation */
export function markNodeNew(id: string) {
  _newNodeIds.add(id);
}

/** Add a node ID to the new-node animation set (for toolbar use). */
export function addNewNodeId(id: string) {
  _newNodeIds.add(id);
}

// ── Canvas Placement Helpers (for toolbar node creation) ───────────────────

/** Get the center of the visible canvas area in graph coordinates. */
export function getCanvasCenter(): { x: number; y: number } {
  return { x: (-_panX + 400) / _zoom, y: (-_panY + 200) / _zoom };
}

/** Zoom in one step. */
export function zoomIn(): void {
  _zoom = Math.min(MAX_ZOOM, _zoom * 1.2);
  applyTransform();
}

/** Zoom out one step. */
export function zoomOut(): void {
  _zoom = Math.max(MIN_ZOOM, _zoom * 0.8);
  applyTransform();
}

// ── Mount / Unmount ────────────────────────────────────────────────────────

export function mountCanvas(container: HTMLElement) {
  container.innerHTML = '';

  _svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  _svg.setAttribute('class', 'flow-canvas');
  _svg.setAttribute('width', '100%');
  _svg.setAttribute('height', '100%');

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
  _svg.appendChild(defs);

  // Background grid
  const bg = svgEl('rect');
  bg.setAttribute('class', 'flow-bg');
  bg.setAttribute('width', '10000');
  bg.setAttribute('height', '10000');
  bg.setAttribute('x', '-5000');
  bg.setAttribute('y', '-5000');
  bg.setAttribute('fill', 'url(#flow-grid)');
  _svg.appendChild(bg);

  // Groups in z-order
  _edgesGroup = svgEl('g') as SVGGElement;
  _edgesGroup.setAttribute('class', 'flow-edges');
  _svg.appendChild(_edgesGroup);

  _portsGroup = svgEl('g') as SVGGElement;
  _portsGroup.setAttribute('class', 'flow-ports');
  _svg.appendChild(_portsGroup);

  _nodesGroup = svgEl('g') as SVGGElement;
  _nodesGroup.setAttribute('class', 'flow-nodes');
  _svg.appendChild(_nodesGroup);

  _dragPreviewGroup = svgEl('g') as SVGGElement;
  _dragPreviewGroup.setAttribute('class', 'flow-drag-preview');
  _svg.appendChild(_dragPreviewGroup);

  container.appendChild(_svg);

  // Wire events
  _svg.addEventListener('mousedown', onMouseDown);
  _svg.addEventListener('mousemove', onMouseMove);
  _svg.addEventListener('mouseup', onMouseUp);
  _svg.addEventListener('wheel', onWheel, { passive: false });
  _svg.addEventListener('dblclick', onDoubleClick);

  applyTransform();
}

export function unmountCanvas() {
  if (_svg) {
    _svg.removeEventListener('mousedown', onMouseDown);
    _svg.removeEventListener('mousemove', onMouseMove);
    _svg.removeEventListener('mouseup', onMouseUp);
    _svg.removeEventListener('wheel', onWheel);
    _svg.removeEventListener('dblclick', onDoubleClick);
    _svg.remove();
    _svg = null;
  }
  _nodesGroup = null;
  _edgesGroup = null;
  _portsGroup = null;
  _dragPreviewGroup = null;
}

// ── Phase 0.2b: Dirty-region rendering during drag ─────────────────────────

function updateDraggedNodePosition(nodeId: string): void {
  if (!_nodesGroup || !_edgesGroup || !_portsGroup) return;
  const node = _nodeMap.get(nodeId);
  if (!node) return;

  const nodeG = _nodesGroup.querySelector(`[data-node-id="${nodeId}"]`) as SVGGElement | null;
  if (nodeG) {
    nodeG.setAttribute('transform', `translate(${node.x}, ${node.y})`);
    (nodeG as unknown as HTMLElement).style.setProperty('--node-tx', `${node.x}px`);
    (nodeG as unknown as HTMLElement).style.setProperty('--node-ty', `${node.y}px`);
  }

  const oldPorts = _portsGroup.querySelectorAll(`[data-node-id="${nodeId}"]`);
  oldPorts.forEach((p) => p.remove());
  renderPorts(node);

  const connectedEdges = [...(_outEdges.get(nodeId) ?? []), ...(_inEdges.get(nodeId) ?? [])];
  for (const edge of connectedEdges) {
    const oldEdgeEl = _edgesGroup.querySelector(`[data-edge-id="${edge.id}"]`);
    if (oldEdgeEl) oldEdgeEl.remove();
    const fromNode = _nodeMap.get(edge.from);
    const toNode = _nodeMap.get(edge.to);
    if (fromNode && toNode) {
      _edgesGroup.appendChild(renderEdge(edge, fromNode, toNode));
    }
  }
}

// ── Full Render ────────────────────────────────────────────────────────────

export function renderGraph() {
  const _state = getMoleculesState();
  const graph = _state?.getGraph();
  if (!graph || !_nodesGroup || !_edgesGroup || !_portsGroup) return;

  rebuildIndexes(graph);

  _edgesGroup.innerHTML = '';
  _nodesGroup.innerHTML = '';
  _portsGroup.innerHTML = '';

  for (const edge of graph.edges) {
    const fromNode = _nodeMap.get(edge.from);
    const toNode = _nodeMap.get(edge.to);
    if (fromNode && toNode) {
      _edgesGroup.appendChild(renderEdge(edge, fromNode, toNode));
    }
  }

  const selectedId = _state.getSelectedNodeId();
  const selectedIds = _state.getSelectedNodeIds();
  for (const node of graph.nodes) {
    const isSelected = selectedIds.size > 0
      ? selectedIds.has(node.id)
      : node.id === selectedId;
    _nodesGroup.appendChild(renderNode(node, isSelected));
    renderPorts(node);
  }
}

// ── Node Rendering ─────────────────────────────────────────────────────────

function renderNode(node: FlowNode, selected: boolean): SVGGElement {
  const g = svgEl('g') as SVGGElement;
  const isNew = _newNodeIds.has(node.id);
  const hasBreakpoint = getDebugBreakpoints().has(node.id);
  const isCursor = getDebugCursorNodeId() === node.id;
  g.setAttribute(
    'class',
    `flow-node flow-node-${node.kind}${selected ? ' flow-node-selected' : ''}${node.status !== 'idle' ? ` flow-node-${node.status}` : ''}${isNew ? ' flow-node-new' : ''}${hasBreakpoint ? ' flow-node-breakpoint' : ''}${isCursor ? ' flow-node-cursor' : ''}`,
  );
  g.setAttribute('data-node-id', node.id);
  g.setAttribute('transform', `translate(${node.x}, ${node.y})`);
  (g as unknown as HTMLElement).style.setProperty('--node-tx', `${node.x}px`);
  (g as unknown as HTMLElement).style.setProperty('--node-ty', `${node.y}px`);

  if (isNew) {
    requestAnimationFrame(() => {
      setTimeout(() => _newNodeIds.delete(node.id), 600);
    });
  }

  const defaults = NODE_DEFAULTS[node.kind];

  // Shadow rect
  const shadow = svgEl('rect');
  shadow.setAttribute('x', '2');
  shadow.setAttribute('y', '2');
  shadow.setAttribute('width', String(node.width));
  shadow.setAttribute('height', String(node.height));
  shadow.setAttribute('rx', '6');
  shadow.setAttribute('fill', 'rgba(0,0,0,0.3)');
  g.appendChild(shadow);

  // Main body
  const body = svgEl('rect');
  body.setAttribute('class', 'flow-node-body');
  body.setAttribute('width', String(node.width));
  body.setAttribute('height', String(node.height));
  body.setAttribute('rx', '6');
  body.setAttribute('fill', 'var(--bg-secondary)');
  body.setAttribute('stroke', selected ? 'var(--accent)' : defaults.color);
  body.setAttribute('stroke-width', selected ? '2' : '1.5');
  if (selected) body.setAttribute('filter', 'url(#flow-selected-glow)');
  if (node.status === 'running') body.setAttribute('filter', 'url(#flow-kinetic-glow)');
  g.appendChild(body);

  // Status bar
  if (node.status !== 'idle') {
    const statusBar = svgEl('rect');
    statusBar.setAttribute('class', 'flow-node-status');
    statusBar.setAttribute('width', String(node.width));
    statusBar.setAttribute('height', '3');
    statusBar.setAttribute('rx', '6');
    const statusColors: Record<string, string> = {
      running: 'var(--kinetic-red, #FF4D4D)',
      success: 'var(--kinetic-sage, #8FB0A0)',
      error: 'var(--kinetic-red, #FF4D4D)',
      paused: 'var(--kinetic-gold, #D4A853)',
    };
    statusBar.setAttribute('fill', statusColors[node.status] ?? 'var(--kinetic-steel, #7A8B9A)');
    g.appendChild(statusBar);
  }

  // Breathing indicator dot
  if (node.status === 'running' || node.status === 'paused') {
    const breathDot = svgEl('circle');
    breathDot.setAttribute('class', 'flow-node-breathe');
    breathDot.setAttribute('cx', String(node.width - 12));
    breathDot.setAttribute('cy', '12');
    breathDot.setAttribute('r', '4');
    breathDot.setAttribute(
      'fill',
      node.status === 'running' ? 'var(--kinetic-red, #FF4D4D)' : 'var(--kinetic-gold, #D4A853)',
    );
    g.appendChild(breathDot);
  }

  // Halftone overlay
  if (node.status === 'running') {
    const halftone = svgEl('rect');
    halftone.setAttribute('class', 'flow-node-halftone');
    halftone.setAttribute('width', String(node.width));
    halftone.setAttribute('height', String(node.height));
    halftone.setAttribute('rx', '6');
    halftone.setAttribute('fill', 'url(#flow-halftone)');
    halftone.setAttribute('opacity', String(0.03));
    halftone.setAttribute('pointer-events', 'none');
    g.appendChild(halftone);
    g.classList.add('flow-node-executing');
  }

  // Breakpoint indicator
  if (hasBreakpoint) {
    const bpDot = svgEl('circle');
    bpDot.setAttribute('class', 'flow-node-bp-dot');
    bpDot.setAttribute('cx', '-4');
    bpDot.setAttribute('cy', String(node.height / 2));
    bpDot.setAttribute('r', '5');
    bpDot.setAttribute('fill', 'var(--kinetic-red, #FF4D4D)');
    g.appendChild(bpDot);
  }

  // Execution cursor
  if (isCursor) {
    const cursorRing = svgEl('rect');
    cursorRing.setAttribute('class', 'flow-node-cursor-ring');
    cursorRing.setAttribute('x', '-3');
    cursorRing.setAttribute('y', '-3');
    cursorRing.setAttribute('width', String(node.width + 6));
    cursorRing.setAttribute('height', String(node.height + 6));
    cursorRing.setAttribute('rx', '8');
    cursorRing.setAttribute('fill', 'none');
    cursorRing.setAttribute('stroke', 'var(--kinetic-gold, #D4A853)');
    cursorRing.setAttribute('stroke-width', '2');
    g.appendChild(cursorRing);
  }

  // Kind icon
  const iconText = svgEl('text');
  iconText.setAttribute('class', 'flow-node-icon ms');
  iconText.setAttribute('x', '12');
  iconText.setAttribute('y', String(node.height / 2 + 1));
  iconText.setAttribute('dominant-baseline', 'central');
  iconText.setAttribute('fill', defaults.color);
  iconText.setAttribute('font-size', '18');
  iconText.setAttribute('font-family', 'Material Symbols Rounded');
  iconText.textContent = defaults.icon;
  g.appendChild(iconText);

  // Label
  const label = svgEl('text');
  label.setAttribute('class', 'flow-node-label');
  label.setAttribute('x', '36');
  label.setAttribute(
    'y',
    node.description ? String(node.height / 2 - 6) : String(node.height / 2 + 1),
  );
  label.setAttribute('dominant-baseline', 'central');
  label.setAttribute('fill', 'var(--text-primary)');
  label.setAttribute('font-size', '12');
  label.setAttribute('font-weight', '600');
  label.textContent = truncate(node.label, 18);
  g.appendChild(label);

  // Description
  if (node.description) {
    const desc = svgEl('text');
    desc.setAttribute('class', 'flow-node-desc');
    desc.setAttribute('x', '36');
    desc.setAttribute('y', String(node.height / 2 + 10));
    desc.setAttribute('dominant-baseline', 'central');
    desc.setAttribute('fill', 'var(--text-muted)');
    desc.setAttribute('font-size', '10');
    desc.textContent = truncate(node.description, 22);
    g.appendChild(desc);
  }

  // Kind badge
  const badge = svgEl('text');
  badge.setAttribute('class', 'flow-node-badge');
  badge.setAttribute('x', String(node.width - 8));
  badge.setAttribute('y', '14');
  badge.setAttribute('text-anchor', 'end');
  badge.setAttribute('fill', defaults.color);
  badge.setAttribute('font-size', '8');
  badge.textContent = node.kind.toUpperCase();
  g.appendChild(badge);

  return g;
}

function renderPorts(node: FlowNode) {
  if (!_portsGroup) return;

  for (const p of node.outputs) {
    const pos = getOutputPort(node, p);
    const isErrPort = p === 'err';
    const circle = svgEl('circle');
    circle.setAttribute(
      'class',
      `flow-port flow-port-output${isErrPort ? ' flow-port-error' : ''}`,
    );
    circle.setAttribute('cx', String(pos.x));
    circle.setAttribute('cy', String(pos.y));
    circle.setAttribute('r', String(PORT_RADIUS));
    circle.setAttribute('fill', 'var(--bg-primary)');
    circle.setAttribute('stroke', isErrPort ? 'var(--kinetic-red, #D64045)' : 'var(--accent)');
    circle.setAttribute('stroke-width', '1.5');
    circle.setAttribute('data-node-id', node.id);
    circle.setAttribute('data-port', p);
    circle.setAttribute('data-port-kind', 'output');
    _portsGroup.appendChild(circle);
  }

  for (const p of node.inputs) {
    const pos = getInputPort(node, p);
    const circle = svgEl('circle');
    circle.setAttribute('class', 'flow-port flow-port-input');
    circle.setAttribute('cx', String(pos.x));
    circle.setAttribute('cy', String(pos.y));
    circle.setAttribute('r', String(PORT_RADIUS));
    circle.setAttribute('fill', 'var(--bg-primary)');
    circle.setAttribute('stroke', 'var(--text-muted)');
    circle.setAttribute('stroke-width', '1.5');
    circle.setAttribute('data-node-id', node.id);
    circle.setAttribute('data-port', p);
    circle.setAttribute('data-port-kind', 'input');
    _portsGroup.appendChild(circle);
  }
}

// ── Edge Rendering ─────────────────────────────────────────────────────────

function renderEdge(edge: FlowEdge, fromNode: FlowNode, toNode: FlowNode): SVGGElement {
  const g = svgEl('g') as SVGGElement;
  const selectedEdgeId = getSelectedEdgeIdLocal();
  const isSelected = selectedEdgeId === edge.id;
  g.setAttribute(
    'class',
    `flow-edge flow-edge-${edge.kind}${edge.active ? ' flow-edge-active' : ''}${isSelected ? ' flow-edge-selected' : ''}`,
  );
  g.setAttribute('data-edge-id', edge.id);

  const fromPt = getOutputPort(fromNode, edge.fromPort);
  const toPt = getInputPort(toNode, edge.toPort);
  const pathD = buildEdgePath(fromPt, toPt);

  // Invisible wide hit-area for click selection
  const hitArea = svgEl('path');
  hitArea.setAttribute('d', pathD);
  hitArea.setAttribute('fill', 'none');
  hitArea.setAttribute('stroke', 'transparent');
  hitArea.setAttribute('stroke-width', '12');
  hitArea.setAttribute('class', 'flow-edge-hit');
  hitArea.style.cursor = 'pointer';
  g.appendChild(hitArea);

  const path = svgEl('path');
  path.setAttribute('class', 'flow-edge-path');
  path.setAttribute('d', pathD);
  path.setAttribute('fill', 'none');
  path.setAttribute('stroke-width', isSelected ? '3' : (edge.active ? '2.5' : '1.5'));

  switch (edge.kind) {
    case 'forward':
      path.setAttribute('stroke', edge.active ? 'var(--accent)' : 'var(--text-muted)');
      path.setAttribute('marker-end', 'url(#flow-arrow-fwd)');
      break;
    case 'reverse':
      path.setAttribute('stroke', 'var(--status-info)');
      path.setAttribute('stroke-dasharray', '6 3');
      path.setAttribute('marker-start', 'url(#flow-arrow-rev)');
      break;
    case 'bidirectional':
      path.setAttribute('stroke', 'var(--kinetic-gold)');
      path.setAttribute('marker-end', 'url(#flow-arrow-bi-end)');
      path.setAttribute('marker-start', 'url(#flow-arrow-bi-start)');
      break;
    case 'error':
      path.setAttribute(
        'stroke',
        edge.active ? 'var(--kinetic-red)' : 'var(--kinetic-red-60, rgba(214, 64, 69, 0.6))',
      );
      path.setAttribute('stroke-dasharray', '8 4');
      path.setAttribute('marker-end', 'url(#flow-arrow-fwd)');
      break;
  }

  if (edge.active) path.setAttribute('filter', 'url(#flow-glow)');
  if (isSelected) path.setAttribute('filter', 'url(#flow-selected-glow)');
  g.appendChild(path);

  // Edge label
  if (edge.label) {
    const mid = { x: (fromPt.x + toPt.x) / 2, y: (fromPt.y + toPt.y) / 2 - 10 };
    const labelBg = svgEl('rect');
    labelBg.setAttribute('x', String(mid.x - 30));
    labelBg.setAttribute('y', String(mid.y - 8));
    labelBg.setAttribute('width', '60');
    labelBg.setAttribute('height', '16');
    labelBg.setAttribute('rx', '3');
    labelBg.setAttribute('fill', 'var(--bg-primary)');
    labelBg.setAttribute('stroke', 'var(--border-subtle)');
    labelBg.setAttribute('stroke-width', '0.5');
    g.appendChild(labelBg);

    const labelText = svgEl('text');
    labelText.setAttribute('x', String(mid.x));
    labelText.setAttribute('y', String(mid.y + 2));
    labelText.setAttribute('text-anchor', 'middle');
    labelText.setAttribute('dominant-baseline', 'central');
    labelText.setAttribute('fill', 'var(--text-secondary)');
    labelText.setAttribute('font-size', '9');
    labelText.textContent = edge.label;
    g.appendChild(labelText);
  }

  // Debug: data value on edge
  const edgeValue = getDebugEdgeValues().get(edge.id);
  if (edgeValue) {
    const mid = { x: (fromPt.x + toPt.x) / 2, y: (fromPt.y + toPt.y) / 2 + (edge.label ? 12 : 0) };
    const truncVal = edgeValue.length > 40 ? `${edgeValue.slice(0, 37)}…` : edgeValue;

    const valBg = svgEl('rect');
    valBg.setAttribute('class', 'flow-edge-value-bg');
    valBg.setAttribute('x', String(mid.x - 70));
    valBg.setAttribute('y', String(mid.y - 6));
    valBg.setAttribute('width', '140');
    valBg.setAttribute('height', '14');
    valBg.setAttribute('rx', '3');
    valBg.setAttribute('fill', 'var(--bg-tertiary, var(--bg-secondary))');
    valBg.setAttribute('stroke', 'var(--kinetic-gold, #D4A853)');
    valBg.setAttribute('stroke-width', '0.5');
    valBg.setAttribute('opacity', '0.9');
    g.appendChild(valBg);

    const valText = svgEl('text');
    valText.setAttribute('class', 'flow-edge-value-text');
    valText.setAttribute('x', String(mid.x));
    valText.setAttribute('y', String(mid.y + 1));
    valText.setAttribute('text-anchor', 'middle');
    valText.setAttribute('dominant-baseline', 'central');
    valText.setAttribute('fill', 'var(--kinetic-gold, #D4A853)');
    valText.setAttribute('font-size', '8');
    valText.textContent = truncVal;
    g.appendChild(valText);
  }

  return g;
}

// ── Interaction: Pan, Zoom, Drag, Connect ──────────────────────────────────

function canvasCoords(e: MouseEvent): Point {
  if (!_svg) return { x: 0, y: 0 };
  const rect = _svg.getBoundingClientRect();
  return {
    x: (e.clientX - rect.left - _panX) / _zoom,
    y: (e.clientY - rect.top - _panY) / _zoom,
  };
}

function applyTransform() {
  if (!_svg) return;
  const groups = [_edgesGroup, _nodesGroup, _portsGroup, _dragPreviewGroup];
  for (const g of groups) {
    g?.setAttribute('transform', `translate(${_panX}, ${_panY}) scale(${_zoom})`);
  }
  const bg = _svg.querySelector('.flow-bg');
  bg?.setAttribute('transform', `translate(${_panX}, ${_panY}) scale(${_zoom})`);
}

function onMouseDown(e: MouseEvent) {
  if (!_svg) return;
  const _state = getMoleculesState();
  if (!_state) return;
  const graph = _state.getGraph();
  if (!graph) return;

  const pt = canvasCoords(e);

  // Check for port hit (start drawing edge)
  const portHit = hitTestPort(graph, pt.x, pt.y);
  if (portHit && portHit.kind === 'output') {
    _drawingEdge = {
      fromNodeId: portHit.node.id,
      fromPort: portHit.port,
      cursorX: pt.x,
      cursorY: pt.y,
    };
    e.preventDefault();
    return;
  }

  // Check for node hit (start drag)
  const node = hitTestNode(graph, pt.x, pt.y);
  if (node) {
    setSelectedEdgeIdLocal(null);
    _state.setSelectedEdgeId(null);
    // Shift+click toggles breakpoint
    if (e.shiftKey && !e.ctrlKey && !e.metaKey) {
      const event = new CustomEvent('flow:toggle-breakpoint', { detail: { nodeId: node.id } });
      document.dispatchEvent(event);
      e.preventDefault();
      return;
    }

    // Ctrl/Meta+click: toggle in multi-select set
    if (e.ctrlKey || e.metaKey) {
      const ids = new Set(_state.getSelectedNodeIds());
      if (ids.has(node.id)) {
        ids.delete(node.id);
      } else {
        ids.add(node.id);
      }
      _state.setSelectedNodeIds(ids);
      _state.setSelectedNodeId(ids.size === 1 ? [...ids][0] : (ids.size > 0 ? node.id : null));
    } else {
      const selectedIds = _state.getSelectedNodeIds();
      if (!selectedIds.has(node.id)) {
        _state.setSelectedNodeIds(new Set([node.id]));
      }
      _state.setSelectedNodeId(node.id);
    }

    _dragging = { nodeId: node.id, offsetX: pt.x - node.x, offsetY: pt.y - node.y };
    renderGraph();
    e.preventDefault();
    return;
  }

  // Check for edge hit
  const target = e.target as SVGElement;
  const edgeGroup = target.closest('[data-edge-id]') as SVGElement | null;
  if (edgeGroup) {
    const edgeId = edgeGroup.getAttribute('data-edge-id');
    setSelectedEdgeIdLocal(edgeId);
    _state.setSelectedEdgeId(edgeId);
    _state.setSelectedNodeId(null);
    _state.setSelectedNodeIds(new Set());
    renderGraph();
    e.preventDefault();
    return;
  }

  // Click empty space
  setSelectedEdgeIdLocal(null);
  _state.setSelectedEdgeId(null);
  if (e.shiftKey || e.ctrlKey || e.metaKey) {
    _rubberBand = { startX: pt.x, startY: pt.y, cursorX: pt.x, cursorY: pt.y };
    e.preventDefault();
    return;
  }
  _state.setSelectedNodeId(null);
  _state.setSelectedNodeIds(new Set());
  _panning = true;
  _panStartX = e.clientX - _panX;
  _panStartY = e.clientY - _panY;
  renderGraph();
}

function onMouseMove(e: MouseEvent) {
  if (!_svg) return;
  const _state = getMoleculesState();
  if (!_state) return;

  if (_panning) {
    _panX = e.clientX - _panStartX;
    _panY = e.clientY - _panStartY;
    applyTransform();
    return;
  }

  if (_dragging) {
    const graph = _state.getGraph();
    if (!graph) return;
    const pt = canvasCoords(e);
    const primaryNode =
      _nodeMap.get(_dragging.nodeId) ?? graph.nodes.find((n) => n.id === _dragging!.nodeId);
    if (primaryNode) {
      const newX = snapToGrid(pt.x - _dragging.offsetX);
      const newY = snapToGrid(pt.y - _dragging.offsetY);
      const dx = newX - primaryNode.x;
      const dy = newY - primaryNode.y;

      const selectedIds = _state.getSelectedNodeIds();
      if (selectedIds.size > 1 && selectedIds.has(_dragging.nodeId)) {
        for (const nid of selectedIds) {
          const n = _nodeMap.get(nid) ?? graph.nodes.find((nn) => nn.id === nid);
          if (n) {
            n.x = snapToGrid(n.x + dx);
            n.y = snapToGrid(n.y + dy);
            updateDraggedNodePosition(nid);
          }
        }
      } else {
        primaryNode.x = newX;
        primaryNode.y = newY;
        updateDraggedNodePosition(_dragging.nodeId);
      }
    }
    return;
  }

  if (_rubberBand) {
    const pt = canvasCoords(e);
    _rubberBand.cursorX = pt.x;
    _rubberBand.cursorY = pt.y;
    renderRubberBand();
    return;
  }

  if (_drawingEdge) {
    const pt = canvasCoords(e);
    _drawingEdge.cursorX = pt.x;
    _drawingEdge.cursorY = pt.y;
    renderEdgePreview();
    return;
  }
}

function onMouseUp(e: MouseEvent) {
  const _state = getMoleculesState();
  if (!_state) return;
  const graph = _state.getGraph();

  if (_drawingEdge && graph) {
    const pt = canvasCoords(e);
    const portHit = hitTestPort(graph, pt.x, pt.y);
    if (portHit && portHit.kind === 'input' && portHit.node.id !== _drawingEdge.fromNodeId) {
      const exists = graph.edges.some(
        (ee) => ee.from === _drawingEdge!.fromNodeId && ee.to === portHit.node.id,
      );
      if (!exists) {
        const isErrorEdge = _drawingEdge.fromPort === 'err';
        const edge = createEdge(
          _drawingEdge.fromNodeId,
          portHit.node.id,
          isErrorEdge ? 'error' : 'forward',
          {
            fromPort: _drawingEdge.fromPort,
            toPort: portHit.port,
            label: isErrorEdge ? 'error' : undefined,
          },
        );
        graph.edges.push(edge);
        _state.onGraphChanged();
      }
    }
    _drawingEdge = null;
    clearEdgePreview();
    renderGraph();
  }

  if (_dragging) {
    _dragging = null;
    _state.onGraphChanged();
  }

  if (_rubberBand && graph) {
    const x1 = Math.min(_rubberBand.startX, _rubberBand.cursorX);
    const y1 = Math.min(_rubberBand.startY, _rubberBand.cursorY);
    const x2 = Math.max(_rubberBand.startX, _rubberBand.cursorX);
    const y2 = Math.max(_rubberBand.startY, _rubberBand.cursorY);

    const ids = new Set<string>();
    for (const node of graph.nodes) {
      const cx = node.x + node.width / 2;
      const cy = node.y + node.height / 2;
      if (cx >= x1 && cx <= x2 && cy >= y1 && cy <= y2) {
        ids.add(node.id);
      }
    }
    _state.setSelectedNodeIds(ids);
    _state.setSelectedNodeId(ids.size === 1 ? [...ids][0] : null);
    _rubberBand = null;
    clearRubberBand();
    renderGraph();
  }

  _panning = false;
}

function onWheel(e: WheelEvent) {
  e.preventDefault();
  if (!_svg) return;

  const rect = _svg.getBoundingClientRect();
  const mx = e.clientX - rect.left;
  const my = e.clientY - rect.top;

  const delta = e.deltaY > 0 ? 0.9 : 1.1;
  const newZoom = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, _zoom * delta));

  _panX = mx - (mx - _panX) * (newZoom / _zoom);
  _panY = my - (my - _panY) * (newZoom / _zoom);
  _zoom = newZoom;

  applyTransform();
}

function onDoubleClick(e: MouseEvent) {
  const _state = getMoleculesState();
  if (!_state) return;
  const graph = _state.getGraph();
  if (!graph) return;

  const pt = canvasCoords(e);
  const node = hitTestNode(graph, pt.x, pt.y);
  if (node) {
    const event = new CustomEvent('flow:edit-node', { detail: { nodeId: node.id } });
    document.dispatchEvent(event);
    return;
  }

  const event = new CustomEvent('flow:add-node', {
    detail: { x: snapToGrid(pt.x), y: snapToGrid(pt.y) },
  });
  document.dispatchEvent(event);
}

// ── Edge Preview ───────────────────────────────────────────────────────────

function renderEdgePreview() {
  if (!_drawingEdge || !_dragPreviewGroup) return;
  const _state = getMoleculesState();
  const graph = _state?.getGraph();
  if (!graph) return;

  _dragPreviewGroup.innerHTML = '';
  const fromNode =
    _nodeMap.get(_drawingEdge.fromNodeId) ??
    graph.nodes.find((n) => n.id === _drawingEdge!.fromNodeId);
  if (!fromNode) return;

  const fromPt = getOutputPort(fromNode, _drawingEdge.fromPort);
  const toPt = { x: _drawingEdge.cursorX, y: _drawingEdge.cursorY };
  const pathD = buildEdgePath(fromPt, toPt);

  const path = svgEl('path');
  path.setAttribute('d', pathD);
  path.setAttribute('fill', 'none');
  path.setAttribute('stroke', 'var(--accent)');
  path.setAttribute('stroke-width', '1.5');
  path.setAttribute('stroke-dasharray', '4 4');
  path.setAttribute('opacity', '0.6');
  _dragPreviewGroup.appendChild(path);
}

function clearEdgePreview() {
  if (_dragPreviewGroup) _dragPreviewGroup.innerHTML = '';
}

// ── Phase 3.5: Rubber-band rendering ───────────────────────────────────────

function renderRubberBand() {
  if (!_rubberBand || !_svg) return;
  clearRubberBand();
  const x = Math.min(_rubberBand.startX, _rubberBand.cursorX);
  const y = Math.min(_rubberBand.startY, _rubberBand.cursorY);
  const w = Math.abs(_rubberBand.cursorX - _rubberBand.startX);
  const h = Math.abs(_rubberBand.cursorY - _rubberBand.startY);
  if (w < 2 && h < 2) return;

  const rect = svgEl('rect') as SVGRectElement;
  rect.setAttribute('x', String(x));
  rect.setAttribute('y', String(y));
  rect.setAttribute('width', String(w));
  rect.setAttribute('height', String(h));
  rect.setAttribute('fill', 'rgba(78, 205, 196, 0.1)');
  rect.setAttribute('stroke', 'var(--accent, #4ECDC4)');
  rect.setAttribute('stroke-width', '1');
  rect.setAttribute('stroke-dasharray', '4 2');
  rect.setAttribute('class', 'flow-rubber-band');
  if (_dragPreviewGroup) {
    _dragPreviewGroup.appendChild(rect);
  }
  _rubberBandEl = rect;
}

function clearRubberBand() {
  if (_rubberBandEl) {
    _rubberBandEl.remove();
    _rubberBandEl = null;
  }
}

// ── Utilities ──────────────────────────────────────────────────────────────

export function fitView() {
  if (!_svg) return;
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

  const rect = _svg.getBoundingClientRect();
  const graphW = maxX - minX + 80;
  const graphH = maxY - minY + 80;
  _zoom = Math.min(rect.width / graphW, rect.height / graphH, 1.5);
  _panX = (rect.width - graphW * _zoom) / 2 - minX * _zoom + 40;
  _panY = (rect.height - graphH * _zoom) / 2 - minY * _zoom + 40;

  applyTransform();
}

export function deleteSelected() {
  const _state = getMoleculesState();
  if (!_state) return;
  const graph = _state.getGraph();
  if (!graph) return;

  const selectedIds = _state.getSelectedNodeIds();
  const selectedId = _state.getSelectedNodeId();
  const idsToDelete = selectedIds.size > 0
    ? selectedIds
    : (selectedId ? new Set([selectedId]) : new Set<string>());

  if (idsToDelete.size === 0) return;

  graph.nodes = graph.nodes.filter((n) => !idsToDelete.has(n.id));
  graph.edges = graph.edges.filter((e) => !idsToDelete.has(e.from) && !idsToDelete.has(e.to));
  _state.setSelectedNodeId(null);
  _state.setSelectedNodeIds(new Set());
  _state.onGraphChanged();
  renderGraph();
}

export function resetView() {
  _panX = 0;
  _panY = 0;
  _zoom = 1;
  applyTransform();
}

function svgEl(tag: string): SVGElement {
  return document.createElementNS('http://www.w3.org/2000/svg', tag);
}

function truncate(s: string, max: number): string {
  return s.length > max ? `${s.slice(0, max - 1)}…` : s;
}
