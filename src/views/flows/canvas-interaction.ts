// ─────────────────────────────────────────────────────────────────────────────
// Canvas Interaction — Pan, zoom, drag, connect, rubber-band selection
// Event handlers and preview rendering for the canvas.
// ─────────────────────────────────────────────────────────────────────────────

import {
  type Point,
  hitTestNode,
  hitTestPort,
  snapToGrid,
  createEdge,
  getOutputPort,
  buildEdgePath,
} from './atoms';
import { getMoleculesState, setSelectedEdgeIdLocal } from './molecule-state';
import { cs, svgEl, applyTransform } from './canvas-state';

// Lazy import helpers — avoid circular dependency with canvas-molecules.ts.
// These are set by canvas-molecules.ts during mount.
let _renderGraphFn: (() => void) | null = null;
let _updateDraggedFn: ((nodeId: string) => void) | null = null;

/** Called by canvas-molecules.ts to wire the render/update callbacks. */
export function setCanvasCallbacks(
  renderGraph: () => void,
  updateDraggedNodePosition: (nodeId: string) => void,
): void {
  _renderGraphFn = renderGraph;
  _updateDraggedFn = updateDraggedNodePosition;
}

function renderGraph(): void {
  _renderGraphFn?.();
}

function updateDraggedNodePosition(nodeId: string): void {
  _updateDraggedFn?.(nodeId);
}

// ── Coordinate Conversion ──────────────────────────────────────────────────

function canvasCoords(e: MouseEvent): Point {
  if (!cs.svg) return { x: 0, y: 0 };
  const rect = cs.svg.getBoundingClientRect();
  return {
    x: (e.clientX - rect.left - cs.panX) / cs.zoom,
    y: (e.clientY - rect.top - cs.panY) / cs.zoom,
  };
}

// ── Mouse Handlers ─────────────────────────────────────────────────────────

export function onMouseDown(e: MouseEvent): void {
  if (!cs.svg) return;
  const _state = getMoleculesState();
  if (!_state) return;
  const graph = _state.getGraph();
  if (!graph) return;

  const pt = canvasCoords(e);

  // Check for port hit (start drawing edge)
  const portHit = hitTestPort(graph, pt.x, pt.y);
  if (portHit && portHit.kind === 'output') {
    cs.drawingEdge = {
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
      _state.setSelectedNodeId(ids.size === 1 ? [...ids][0] : ids.size > 0 ? node.id : null);
    } else {
      const selectedIds = _state.getSelectedNodeIds();
      if (!selectedIds.has(node.id)) {
        _state.setSelectedNodeIds(new Set([node.id]));
      }
      _state.setSelectedNodeId(node.id);
    }

    cs.dragging = { nodeId: node.id, offsetX: pt.x - node.x, offsetY: pt.y - node.y };
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
    cs.rubberBand = { startX: pt.x, startY: pt.y, cursorX: pt.x, cursorY: pt.y };
    e.preventDefault();
    return;
  }
  _state.setSelectedNodeId(null);
  _state.setSelectedNodeIds(new Set());
  cs.panning = true;
  cs.panStartX = e.clientX - cs.panX;
  cs.panStartY = e.clientY - cs.panY;
  renderGraph();
}

export function onMouseMove(e: MouseEvent): void {
  if (!cs.svg) return;
  const _state = getMoleculesState();
  if (!_state) return;

  if (cs.panning) {
    cs.panX = e.clientX - cs.panStartX;
    cs.panY = e.clientY - cs.panStartY;
    applyTransform();
    return;
  }

  if (cs.dragging) {
    const graph = _state.getGraph();
    if (!graph) return;
    const pt = canvasCoords(e);
    const primaryNode =
      cs.nodeMap.get(cs.dragging.nodeId) ?? graph.nodes.find((n) => n.id === cs.dragging!.nodeId);
    if (primaryNode) {
      const newX = snapToGrid(pt.x - cs.dragging.offsetX);
      const newY = snapToGrid(pt.y - cs.dragging.offsetY);
      const dx = newX - primaryNode.x;
      const dy = newY - primaryNode.y;

      const selectedIds = _state.getSelectedNodeIds();
      if (selectedIds.size > 1 && selectedIds.has(cs.dragging.nodeId)) {
        for (const nid of selectedIds) {
          const n = cs.nodeMap.get(nid) ?? graph.nodes.find((nn) => nn.id === nid);
          if (n) {
            n.x = snapToGrid(n.x + dx);
            n.y = snapToGrid(n.y + dy);
            updateDraggedNodePosition(nid);
          }
        }
      } else {
        primaryNode.x = newX;
        primaryNode.y = newY;
        updateDraggedNodePosition(cs.dragging.nodeId);
      }
    }
    return;
  }

  if (cs.rubberBand) {
    const pt = canvasCoords(e);
    cs.rubberBand.cursorX = pt.x;
    cs.rubberBand.cursorY = pt.y;
    renderRubberBand();
    return;
  }

  if (cs.drawingEdge) {
    const pt = canvasCoords(e);
    cs.drawingEdge.cursorX = pt.x;
    cs.drawingEdge.cursorY = pt.y;
    renderEdgePreview();
    return;
  }
}

export function onMouseUp(e: MouseEvent): void {
  const _state = getMoleculesState();
  if (!_state) return;
  const graph = _state.getGraph();

  if (cs.drawingEdge && graph) {
    const pt = canvasCoords(e);
    const portHit = hitTestPort(graph, pt.x, pt.y);
    if (portHit && portHit.kind === 'input' && portHit.node.id !== cs.drawingEdge.fromNodeId) {
      const exists = graph.edges.some(
        (ee) => ee.from === cs.drawingEdge!.fromNodeId && ee.to === portHit.node.id,
      );
      if (!exists) {
        const isErrorEdge = cs.drawingEdge.fromPort === 'err';
        const edge = createEdge(
          cs.drawingEdge.fromNodeId,
          portHit.node.id,
          isErrorEdge ? 'error' : 'forward',
          {
            fromPort: cs.drawingEdge.fromPort,
            toPort: portHit.port,
            label: isErrorEdge ? 'error' : undefined,
          },
        );
        graph.edges.push(edge);
        _state.onGraphChanged();
      }
    }
    cs.drawingEdge = null;
    clearEdgePreview();
    renderGraph();
  }

  if (cs.dragging) {
    cs.dragging = null;
    _state.onGraphChanged();
  }

  if (cs.rubberBand && graph) {
    const x1 = Math.min(cs.rubberBand.startX, cs.rubberBand.cursorX);
    const y1 = Math.min(cs.rubberBand.startY, cs.rubberBand.cursorY);
    const x2 = Math.max(cs.rubberBand.startX, cs.rubberBand.cursorX);
    const y2 = Math.max(cs.rubberBand.startY, cs.rubberBand.cursorY);

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
    cs.rubberBand = null;
    clearRubberBand();
    renderGraph();
  }

  cs.panning = false;
}

export function onWheel(e: WheelEvent): void {
  e.preventDefault();
  if (!cs.svg) return;

  const rect = cs.svg.getBoundingClientRect();
  const mx = e.clientX - rect.left;
  const my = e.clientY - rect.top;

  const delta = e.deltaY > 0 ? 0.9 : 1.1;
  const newZoom = Math.max(cs.MIN_ZOOM, Math.min(cs.MAX_ZOOM, cs.zoom * delta));

  cs.panX = mx - (mx - cs.panX) * (newZoom / cs.zoom);
  cs.panY = my - (my - cs.panY) * (newZoom / cs.zoom);
  cs.zoom = newZoom;

  applyTransform();
}

export function onDoubleClick(e: MouseEvent): void {
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

function renderEdgePreview(): void {
  if (!cs.drawingEdge || !cs.dragPreviewGroup) return;
  const _state = getMoleculesState();
  const graph = _state?.getGraph();
  if (!graph) return;

  cs.dragPreviewGroup.innerHTML = '';
  const fromNode =
    cs.nodeMap.get(cs.drawingEdge.fromNodeId) ??
    graph.nodes.find((n) => n.id === cs.drawingEdge!.fromNodeId);
  if (!fromNode) return;

  const fromPt = getOutputPort(fromNode, cs.drawingEdge.fromPort);
  const toPt = { x: cs.drawingEdge.cursorX, y: cs.drawingEdge.cursorY };
  const pathD = buildEdgePath(fromPt, toPt);

  const path = svgEl('path');
  path.setAttribute('d', pathD);
  path.setAttribute('fill', 'none');
  path.setAttribute('stroke', 'var(--accent)');
  path.setAttribute('stroke-width', '1.5');
  path.setAttribute('stroke-dasharray', '4 4');
  path.setAttribute('opacity', '0.6');
  cs.dragPreviewGroup.appendChild(path);
}

function clearEdgePreview(): void {
  if (cs.dragPreviewGroup) cs.dragPreviewGroup.innerHTML = '';
}

// ── Rubber-band Rendering ──────────────────────────────────────────────────

function renderRubberBand(): void {
  if (!cs.rubberBand || !cs.svg) return;
  clearRubberBand();
  const x = Math.min(cs.rubberBand.startX, cs.rubberBand.cursorX);
  const y = Math.min(cs.rubberBand.startY, cs.rubberBand.cursorY);
  const w = Math.abs(cs.rubberBand.cursorX - cs.rubberBand.startX);
  const h = Math.abs(cs.rubberBand.cursorY - cs.rubberBand.startY);
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
  if (cs.dragPreviewGroup) {
    cs.dragPreviewGroup.appendChild(rect);
  }
  cs.rubberBandEl = rect;
}

function clearRubberBand(): void {
  if (cs.rubberBandEl) {
    cs.rubberBandEl.remove();
    cs.rubberBandEl = null;
  }
}
