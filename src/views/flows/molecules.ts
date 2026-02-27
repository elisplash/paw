// ─────────────────────────────────────────────────────────────────────────────
// Flow Visualization Engine — Molecules
// DOM rendering (SVG canvas), interaction handlers, IPC.
// State is injected via a state bridge from index.ts.
// ─────────────────────────────────────────────────────────────────────────────

import {
  type FlowGraph,
  type FlowNode,
  type FlowEdge,
  type FlowNodeKind,
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
  createNode,
  createEdge,
  applyLayout,
} from './atoms';

// ── State Bridge ───────────────────────────────────────────────────────────

interface MoleculesState {
  getGraph: () => FlowGraph | null;
  setGraph: (g: FlowGraph) => void;
  getSelectedNodeId: () => string | null;
  setSelectedNodeId: (id: string | null) => void;
  onGraphChanged: () => void;
}

let _state: MoleculesState;

export function setMoleculesState(s: MoleculesState) {
  _state = s;
}

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
let _drawingEdge: { fromNodeId: string; fromPort: string; cursorX: number; cursorY: number } | null = null;

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

// ── Full Render ────────────────────────────────────────────────────────────

export function renderGraph() {
  const graph = _state?.getGraph();
  if (!graph || !_nodesGroup || !_edgesGroup || !_portsGroup) return;

  _edgesGroup.innerHTML = '';
  _nodesGroup.innerHTML = '';
  _portsGroup.innerHTML = '';

  // Edges first (below nodes)
  for (const edge of graph.edges) {
    const fromNode = graph.nodes.find((n) => n.id === edge.from);
    const toNode = graph.nodes.find((n) => n.id === edge.to);
    if (fromNode && toNode) {
      _edgesGroup.appendChild(renderEdge(edge, fromNode, toNode));
    }
  }

  // Nodes
  const selectedId = _state.getSelectedNodeId();
  for (const node of graph.nodes) {
    _nodesGroup.appendChild(renderNode(node, node.id === selectedId));
    renderPorts(node);
  }
}

// ── Node Rendering ─────────────────────────────────────────────────────────

function renderNode(node: FlowNode, selected: boolean): SVGGElement {
  const g = svgEl('g') as SVGGElement;
  g.setAttribute('class', `flow-node flow-node-${node.kind}${selected ? ' flow-node-selected' : ''}${node.status !== 'idle' ? ` flow-node-${node.status}` : ''}`);
  g.setAttribute('data-node-id', node.id);
  g.setAttribute('transform', `translate(${node.x}, ${node.y})`);

  const defaults = NODE_DEFAULTS[node.kind];

  // Shadow rect (offset)
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
  if (node.status === 'running') body.setAttribute('filter', 'url(#flow-glow)');
  g.appendChild(body);

  // Status bar (top 3px)
  if (node.status !== 'idle') {
    const statusBar = svgEl('rect');
    statusBar.setAttribute('class', 'flow-node-status');
    statusBar.setAttribute('width', String(node.width));
    statusBar.setAttribute('height', '3');
    statusBar.setAttribute('rx', '6');
    const statusColors: Record<string, string> = {
      running: 'var(--accent)',
      success: 'var(--success)',
      error: 'var(--error)',
      paused: 'var(--warning)',
    };
    statusBar.setAttribute('fill', statusColors[node.status] ?? 'var(--text-muted)');
    g.appendChild(statusBar);
  }

  // Kind icon (left side)
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
  label.setAttribute('y', node.description ? String(node.height / 2 - 6) : String(node.height / 2 + 1));
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

  // Kind badge (top-right)
  const badge = svgEl('text');
  badge.setAttribute('class', 'flow-node-badge');
  badge.setAttribute('x', String(node.width - 8));
  badge.setAttribute('y', '14');
  badge.setAttribute('text-anchor', 'end');
  badge.setAttribute('fill', 'var(--text-tertiary)');
  badge.setAttribute('font-size', '8');
  badge.setAttribute('text-transform', 'uppercase');
  badge.textContent = node.kind.toUpperCase();
  g.appendChild(badge);

  return g;
}

function renderPorts(node: FlowNode) {
  if (!_portsGroup) return;

  for (const p of node.outputs) {
    const pos = getOutputPort(node, p);
    const circle = svgEl('circle');
    circle.setAttribute('class', 'flow-port flow-port-output');
    circle.setAttribute('cx', String(pos.x));
    circle.setAttribute('cy', String(pos.y));
    circle.setAttribute('r', String(PORT_RADIUS));
    circle.setAttribute('fill', 'var(--bg-primary)');
    circle.setAttribute('stroke', 'var(--accent)');
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
  g.setAttribute('class', `flow-edge flow-edge-${edge.kind}${edge.active ? ' flow-edge-active' : ''}`);
  g.setAttribute('data-edge-id', edge.id);

  const fromPt = getOutputPort(fromNode, edge.fromPort);
  const toPt = getInputPort(toNode, edge.toPort);
  const pathD = buildEdgePath(fromPt, toPt);

  const path = svgEl('path');
  path.setAttribute('class', 'flow-edge-path');
  path.setAttribute('d', pathD);
  path.setAttribute('fill', 'none');
  path.setAttribute('stroke-width', edge.active ? '2.5' : '1.5');

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
  }

  if (edge.active) path.setAttribute('filter', 'url(#flow-glow)');
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
  // Grid also transforms
  const bg = _svg.querySelector('.flow-bg');
  bg?.setAttribute('transform', `translate(${_panX}, ${_panY}) scale(${_zoom})`);
}

function onMouseDown(e: MouseEvent) {
  if (!_svg || !_state) return;
  const graph = _state.getGraph();
  if (!graph) return;

  const pt = canvasCoords(e);

  // Check for port hit (start drawing edge)
  const portHit = hitTestPort(graph, pt.x, pt.y);
  if (portHit && portHit.kind === 'output') {
    _drawingEdge = { fromNodeId: portHit.node.id, fromPort: portHit.port, cursorX: pt.x, cursorY: pt.y };
    e.preventDefault();
    return;
  }

  // Check for node hit (start drag)
  const node = hitTestNode(graph, pt.x, pt.y);
  if (node) {
    _state.setSelectedNodeId(node.id);
    _dragging = { nodeId: node.id, offsetX: pt.x - node.x, offsetY: pt.y - node.y };
    renderGraph();
    e.preventDefault();
    return;
  }

  // Click empty space — deselect + start pan
  _state.setSelectedNodeId(null);
  _panning = true;
  _panStartX = e.clientX - _panX;
  _panStartY = e.clientY - _panY;
  renderGraph();
}

function onMouseMove(e: MouseEvent) {
  if (!_svg || !_state) return;

  // Pan
  if (_panning) {
    _panX = e.clientX - _panStartX;
    _panY = e.clientY - _panStartY;
    applyTransform();
    return;
  }

  // Drag node
  if (_dragging) {
    const graph = _state.getGraph();
    if (!graph) return;
    const pt = canvasCoords(e);
    const node = graph.nodes.find((n) => n.id === _dragging!.nodeId);
    if (node) {
      node.x = snapToGrid(pt.x - _dragging.offsetX);
      node.y = snapToGrid(pt.y - _dragging.offsetY);
      renderGraph();
    }
    return;
  }

  // Draw edge preview
  if (_drawingEdge) {
    const pt = canvasCoords(e);
    _drawingEdge.cursorX = pt.x;
    _drawingEdge.cursorY = pt.y;
    renderEdgePreview();
    return;
  }
}

function onMouseUp(e: MouseEvent) {
  if (!_state) return;
  const graph = _state.getGraph();

  // Finish edge drawing
  if (_drawingEdge && graph) {
    const pt = canvasCoords(e);
    const portHit = hitTestPort(graph, pt.x, pt.y);
    if (portHit && portHit.kind === 'input' && portHit.node.id !== _drawingEdge.fromNodeId) {
      // Check for duplicate
      const exists = graph.edges.some(
        (ee) => ee.from === _drawingEdge!.fromNodeId && ee.to === portHit.node.id,
      );
      if (!exists) {
        const edge = createEdge(_drawingEdge.fromNodeId, portHit.node.id);
        graph.edges.push(edge);
        _state.onGraphChanged();
      }
    }
    _drawingEdge = null;
    clearEdgePreview();
    renderGraph();
  }

  // Finish drag
  if (_dragging) {
    _dragging = null;
    _state.onGraphChanged();
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

  // Zoom toward cursor
  _panX = mx - (mx - _panX) * (newZoom / _zoom);
  _panY = my - (my - _panY) * (newZoom / _zoom);
  _zoom = newZoom;

  applyTransform();
}

function onDoubleClick(e: MouseEvent) {
  if (!_state) return;
  const graph = _state.getGraph();
  if (!graph) return;

  const pt = canvasCoords(e);
  const node = hitTestNode(graph, pt.x, pt.y);
  if (node) {
    // Open node editor (emit event for organism to handle)
    const event = new CustomEvent('flow:edit-node', { detail: { nodeId: node.id } });
    document.dispatchEvent(event);
    return;
  }

  // Double-click empty space → add node at cursor
  const event = new CustomEvent('flow:add-node', { detail: { x: snapToGrid(pt.x), y: snapToGrid(pt.y) } });
  document.dispatchEvent(event);
}

// ── Edge Preview ───────────────────────────────────────────────────────────

function renderEdgePreview() {
  if (!_drawingEdge || !_dragPreviewGroup || !_state) return;
  const graph = _state.getGraph();
  if (!graph) return;

  _dragPreviewGroup.innerHTML = '';
  const fromNode = graph.nodes.find((n) => n.id === _drawingEdge!.fromNodeId);
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

// ── Toolbar Rendering ──────────────────────────────────────────────────────

export function renderToolbar(container: HTMLElement) {
  container.innerHTML = `
    <div class="flow-toolbar">
      <div class="flow-toolbar-group">
        <button class="flow-tb-btn" data-action="add-trigger" title="Add Trigger">
          <span class="ms">${NODE_DEFAULTS.trigger.icon}</span>
        </button>
        <button class="flow-tb-btn" data-action="add-agent" title="Add Agent">
          <span class="ms">${NODE_DEFAULTS.agent.icon}</span>
        </button>
        <button class="flow-tb-btn" data-action="add-tool" title="Add Tool">
          <span class="ms">${NODE_DEFAULTS.tool.icon}</span>
        </button>
        <button class="flow-tb-btn" data-action="add-condition" title="Add Condition">
          <span class="ms">${NODE_DEFAULTS.condition.icon}</span>
        </button>
        <button class="flow-tb-btn" data-action="add-data" title="Add Data">
          <span class="ms">${NODE_DEFAULTS.data.icon}</span>
        </button>
        <button class="flow-tb-btn" data-action="add-output" title="Add Output">
          <span class="ms">${NODE_DEFAULTS.output.icon}</span>
        </button>
      </div>
      <div class="flow-toolbar-divider"></div>
      <div class="flow-toolbar-group">
        <button class="flow-tb-btn" data-action="auto-layout" title="Auto Layout">
          <span class="ms">auto_fix_high</span>
        </button>
        <button class="flow-tb-btn" data-action="fit-view" title="Fit to View">
          <span class="ms">fit_screen</span>
        </button>
        <button class="flow-tb-btn" data-action="zoom-in" title="Zoom In">
          <span class="ms">zoom_in</span>
        </button>
        <button class="flow-tb-btn" data-action="zoom-out" title="Zoom Out">
          <span class="ms">zoom_out</span>
        </button>
      </div>
      <div class="flow-toolbar-divider"></div>
      <div class="flow-toolbar-group">
        <button class="flow-tb-btn flow-tb-btn-danger" data-action="delete-selected" title="Delete Selected">
          <span class="ms">delete</span>
        </button>
      </div>
    </div>
  `;

  container.querySelectorAll('[data-action]').forEach((btn) => {
    btn.addEventListener('click', () => {
      const action = (btn as HTMLElement).dataset.action!;
      handleToolbarAction(action);
    });
  });
}

function handleToolbarAction(action: string) {
  if (!_state) return;
  const graph = _state.getGraph();
  if (!graph) return;

  const addKinds: Record<string, FlowNodeKind> = {
    'add-trigger': 'trigger',
    'add-agent': 'agent',
    'add-tool': 'tool',
    'add-condition': 'condition',
    'add-data': 'data',
    'add-output': 'output',
  };

  if (action in addKinds) {
    const kind = addKinds[action];
    // Place at center of visible area
    const cx = (-_panX + 400) / _zoom;
    const cy = (-_panY + 200) / _zoom;
    const node = createNode(kind, `${kind.charAt(0).toUpperCase() + kind.slice(1)} ${graph.nodes.length + 1}`, snapToGrid(cx), snapToGrid(cy));
    graph.nodes.push(node);
    _state.setSelectedNodeId(node.id);
    _state.onGraphChanged();
    renderGraph();
    return;
  }

  switch (action) {
    case 'auto-layout':
      applyLayout(graph);
      _state.onGraphChanged();
      renderGraph();
      break;
    case 'fit-view':
      fitView();
      break;
    case 'zoom-in':
      _zoom = Math.min(MAX_ZOOM, _zoom * 1.2);
      applyTransform();
      break;
    case 'zoom-out':
      _zoom = Math.max(MIN_ZOOM, _zoom * 0.8);
      applyTransform();
      break;
    case 'delete-selected':
      deleteSelected();
      break;
  }
}

// ── Utilities ──────────────────────────────────────────────────────────────

function fitView() {
  if (!_svg || !_state) return;
  const graph = _state.getGraph();
  if (!graph || !graph.nodes.length) return;

  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
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

function deleteSelected() {
  if (!_state) return;
  const graph = _state.getGraph();
  const selectedId = _state.getSelectedNodeId();
  if (!graph || !selectedId) return;

  graph.nodes = graph.nodes.filter((n) => n.id !== selectedId);
  graph.edges = graph.edges.filter((e) => e.from !== selectedId && e.to !== selectedId);
  _state.setSelectedNodeId(null);
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
  return s.length > max ? s.slice(0, max - 1) + '…' : s;
}

// ── Flow List Rendering ────────────────────────────────────────────────────

export function renderFlowList(container: HTMLElement, graphs: FlowGraph[], activeId: string | null, onSelect: (id: string) => void, onDelete: (id: string) => void, onNew: () => void) {
  container.innerHTML = `
    <div class="flow-list-header">
      <h3>Flows</h3>
      <button class="flow-list-new-btn" title="New Flow"><span class="ms">add</span></button>
    </div>
    <div class="flow-list-items">${
      graphs.length === 0
        ? '<div class="flow-list-empty">No flows yet.<br>Create one or use <code>/flow</code> in Chat.</div>'
        : graphs
            .map(
              (g) => `
          <div class="flow-list-item${g.id === activeId ? ' active' : ''}" data-flow-id="${g.id}">
            <span class="ms flow-list-icon">account_tree</span>
            <div class="flow-list-meta">
              <div class="flow-list-name">${g.name}</div>
              <div class="flow-list-date">${formatDate(g.updatedAt)}</div>
            </div>
            <button class="flow-list-del" data-del-id="${g.id}" title="Delete"><span class="ms">close</span></button>
          </div>`,
            )
            .join('')
    }</div>
  `;

  container.querySelector('.flow-list-new-btn')?.addEventListener('click', onNew);
  container.querySelectorAll('.flow-list-item').forEach((el) => {
    const id = (el as HTMLElement).dataset.flowId!;
    el.addEventListener('click', (e) => {
      if ((e.target as HTMLElement).closest('.flow-list-del')) return;
      onSelect(id);
    });
  });
  container.querySelectorAll('.flow-list-del').forEach((btn) => {
    btn.addEventListener('click', () => {
      const id = (btn as HTMLElement).dataset.delId!;
      onDelete(id);
    });
  });
}

// ── Node Properties Panel ──────────────────────────────────────────────────

export function renderNodePanel(container: HTMLElement, node: FlowNode | null, onUpdate: (patch: Partial<FlowNode>) => void) {
  if (!node) {
    container.innerHTML = '<div class="flow-panel-empty"><span class="ms">touch_app</span><p>Select a node to edit</p></div>';
    return;
  }

  const defaults = NODE_DEFAULTS[node.kind];
  container.innerHTML = `
    <div class="flow-panel">
      <div class="flow-panel-header">
        <span class="ms" style="color:${defaults.color}">${defaults.icon}</span>
        <span class="flow-panel-kind">${node.kind.toUpperCase()}</span>
      </div>
      <label class="flow-panel-field">
        <span>Label</span>
        <input type="text" class="flow-panel-input" data-field="label" value="${escAttr(node.label)}" />
      </label>
      <label class="flow-panel-field">
        <span>Description</span>
        <input type="text" class="flow-panel-input" data-field="description" value="${escAttr(node.description ?? '')}" />
      </label>
      <label class="flow-panel-field">
        <span>Status</span>
        <select class="flow-panel-select" data-field="status">
          ${['idle', 'running', 'success', 'error', 'paused'].map((s) => `<option value="${s}"${node.status === s ? ' selected' : ''}>${s}</option>`).join('')}
        </select>
      </label>
      <div class="flow-panel-section">
        <span class="flow-panel-section-label">Position</span>
        <div class="flow-panel-pos">
          <span>x: ${node.x}</span>
          <span>y: ${node.y}</span>
          <span>${node.width}×${node.height}</span>
        </div>
      </div>
    </div>
  `;

  container.querySelectorAll('.flow-panel-input, .flow-panel-select').forEach((el) => {
    el.addEventListener('change', () => {
      const field = (el as HTMLElement).dataset.field!;
      const value = (el as HTMLInputElement).value;
      onUpdate({ [field]: value } as Partial<FlowNode>);
    });
  });
}

// ── Helpers ────────────────────────────────────────────────────────────────

function formatDate(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
  } catch {
    return '';
  }
}

function escAttr(s: string): string {
  return s.replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}
