// ─────────────────────────────────────────────────────────────────────────────
// Flow Visualization Engine — Minimap Molecules (Phase 5.1)
// Thumbnail overview of the flow canvas in the bottom-right corner.
// Shows all nodes as colored dots and a draggable viewport rectangle.
// ─────────────────────────────────────────────────────────────────────────────

import { type FlowGraph, NODE_DEFAULTS } from './atoms';
import { getMoleculesState } from './molecule-state';
import { getCanvasViewport, setPanZoom, scheduleRender } from './canvas-molecules';

// ── Constants ──────────────────────────────────────────────────────────────

const MINIMAP_WIDTH = 180;
const MINIMAP_HEIGHT = 120;
const MINIMAP_PADDING = 10;
const NODE_DOT_RADIUS = 3;

// ── State ──────────────────────────────────────────────────────────────────

let _minimapEl: HTMLElement | null = null;
let _canvas: HTMLCanvasElement | null = null;
let _ctx: CanvasRenderingContext2D | null = null;
let _visible = true;
let _dragging = false;

// World-space bounds of all nodes (cached per render)
let _worldBounds = { minX: 0, minY: 0, maxX: 800, maxY: 600 };

// ── Public API ─────────────────────────────────────────────────────────────

export function isMinimapVisible(): boolean {
  return _visible;
}

export function toggleMinimap(): void {
  _visible = !_visible;
  if (_minimapEl) {
    _minimapEl.style.display = _visible ? 'block' : 'none';
  }
}

export function mountMinimap(parent: HTMLElement): void {
  _minimapEl = document.createElement('div');
  _minimapEl.className = 'flow-minimap';
  _minimapEl.style.display = _visible ? 'block' : 'none';

  _canvas = document.createElement('canvas');
  _canvas.width = MINIMAP_WIDTH;
  _canvas.height = MINIMAP_HEIGHT;
  _canvas.className = 'flow-minimap-canvas';
  _ctx = _canvas.getContext('2d');

  _minimapEl.appendChild(_canvas);
  parent.appendChild(_minimapEl);

  // Draggable viewport
  _canvas.addEventListener('mousedown', onMinimapMouseDown);
  _canvas.addEventListener('mousemove', onMinimapMouseMove);
  _canvas.addEventListener('mouseup', onMinimapMouseUp);
  _canvas.addEventListener('mouseleave', onMinimapMouseUp);
}

export function unmountMinimap(): void {
  if (_canvas) {
    _canvas.removeEventListener('mousedown', onMinimapMouseDown);
    _canvas.removeEventListener('mousemove', onMinimapMouseMove);
    _canvas.removeEventListener('mouseup', onMinimapMouseUp);
    _canvas.removeEventListener('mouseleave', onMinimapMouseUp);
  }
  _minimapEl?.remove();
  _minimapEl = null;
  _canvas = null;
  _ctx = null;
}

export function renderMinimap(): void {
  if (!_visible || !_ctx || !_canvas) return;
  const _state = getMoleculesState();
  const graph = _state?.getGraph();
  if (!graph || graph.nodes.length === 0) {
    _ctx.clearRect(0, 0, MINIMAP_WIDTH, MINIMAP_HEIGHT);
    return;
  }

  computeWorldBounds(graph);
  const scale = computeScale();

  _ctx.clearRect(0, 0, MINIMAP_WIDTH, MINIMAP_HEIGHT);

  // Background
  _ctx.fillStyle = 'var(--bg-primary, #1a1a1a)';
  _ctx.fillRect(0, 0, MINIMAP_WIDTH, MINIMAP_HEIGHT);

  // Edge lines
  _ctx.strokeStyle = 'rgba(128, 128, 128, 0.3)';
  _ctx.lineWidth = 0.5;
  for (const edge of graph.edges) {
    const fromNode = graph.nodes.find((n) => n.id === edge.from);
    const toNode = graph.nodes.find((n) => n.id === edge.to);
    if (!fromNode || !toNode) continue;
    const fx = worldToMinimap(fromNode.x + fromNode.width / 2, 'x', scale);
    const fy = worldToMinimap(fromNode.y + fromNode.height / 2, 'y', scale);
    const tx = worldToMinimap(toNode.x + toNode.width / 2, 'x', scale);
    const ty = worldToMinimap(toNode.y + toNode.height / 2, 'y', scale);
    _ctx.beginPath();
    _ctx.moveTo(fx, fy);
    _ctx.lineTo(tx, ty);
    _ctx.stroke();
  }

  // Node dots
  for (const node of graph.nodes) {
    const cx = worldToMinimap(node.x + node.width / 2, 'x', scale);
    const cy = worldToMinimap(node.y + node.height / 2, 'y', scale);
    const defaults = NODE_DEFAULTS[node.kind];
    _ctx.fillStyle = resolveColor(defaults.color);
    _ctx.beginPath();
    _ctx.arc(cx, cy, NODE_DOT_RADIUS, 0, Math.PI * 2);
    _ctx.fill();
  }

  // Viewport rectangle
  const vp = getCanvasViewport();
  const vpX = worldToMinimap(-vp.panX / vp.zoom, 'x', scale);
  const vpY = worldToMinimap(-vp.panY / vp.zoom, 'y', scale);
  const vpW = (vp.width / vp.zoom) * scale;
  const vpH = (vp.height / vp.zoom) * scale;
  _ctx.strokeStyle = 'var(--accent, #5E9EFF)';
  _ctx.lineWidth = 1.5;
  _ctx.strokeRect(vpX, vpY, vpW, vpH);
  _ctx.fillStyle = 'rgba(94, 158, 255, 0.08)';
  _ctx.fillRect(vpX, vpY, vpW, vpH);
}

// ── Helpers ────────────────────────────────────────────────────────────────

function computeWorldBounds(graph: FlowGraph): void {
  let minX = Infinity,
    minY = Infinity,
    maxX = -Infinity,
    maxY = -Infinity;
  for (const n of graph.nodes) {
    if (n.x < minX) minX = n.x;
    if (n.y < minY) minY = n.y;
    if (n.x + n.width > maxX) maxX = n.x + n.width;
    if (n.y + n.height > maxY) maxY = n.y + n.height;
  }
  const pad = 50;
  _worldBounds = { minX: minX - pad, minY: minY - pad, maxX: maxX + pad, maxY: maxY + pad };
}

function computeScale(): number {
  const worldW = _worldBounds.maxX - _worldBounds.minX;
  const worldH = _worldBounds.maxY - _worldBounds.minY;
  const inner = MINIMAP_WIDTH - MINIMAP_PADDING * 2;
  const innerH = MINIMAP_HEIGHT - MINIMAP_PADDING * 2;
  return Math.min(inner / Math.max(worldW, 1), innerH / Math.max(worldH, 1));
}

function worldToMinimap(val: number, axis: 'x' | 'y', scale: number): number {
  const origin = axis === 'x' ? _worldBounds.minX : _worldBounds.minY;
  return MINIMAP_PADDING + (val - origin) * scale;
}

function minimapToWorld(val: number, axis: 'x' | 'y', scale: number): number {
  const origin = axis === 'x' ? _worldBounds.minX : _worldBounds.minY;
  return (val - MINIMAP_PADDING) / scale + origin;
}

/** Best-effort CSS variable resolver for canvas 2D (which doesn't support var()). */
function resolveColor(cssColor: string): string {
  const match = cssColor.match(/,\s*(#[0-9a-fA-F]+)\)/);
  return match ? match[1] : '#5E9EFF';
}

// ── Minimap Interaction ────────────────────────────────────────────────────

function onMinimapMouseDown(e: MouseEvent): void {
  _dragging = true;
  navigateToMinimapPoint(e);
}

function onMinimapMouseMove(e: MouseEvent): void {
  if (!_dragging) return;
  navigateToMinimapPoint(e);
}

function onMinimapMouseUp(): void {
  _dragging = false;
}

function navigateToMinimapPoint(e: MouseEvent): void {
  if (!_canvas) return;
  const rect = _canvas.getBoundingClientRect();
  const mx = e.clientX - rect.left;
  const my = e.clientY - rect.top;
  const scale = computeScale();
  const vp = getCanvasViewport();

  const worldX = minimapToWorld(mx, 'x', scale);
  const worldY = minimapToWorld(my, 'y', scale);

  // Center the viewport on clicked world position
  const newPanX = -(worldX * vp.zoom - vp.width / 2);
  const newPanY = -(worldY * vp.zoom - vp.height / 2);

  setPanZoom(newPanX, newPanY, vp.zoom);
  scheduleRender();
  renderMinimap();
}
