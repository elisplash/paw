// Canvas View — Visual Workspace
// Infinite pan/zoom surface with draggable nodes and SVG edges.
// Full CRUD backed by SQLite via Tauri IPC.

import {
  listCanvases, createCanvas, deleteCanvas, updateCanvas,
  loadNodes, createNode, updateNode, deleteNode,
  loadEdges, createEdge, deleteEdge,
  NODE_KINDS, NODE_COLORS,
  clampZoom, snapToGrid, getNodeCenter, kindIcon,
  type Canvas, type CanvasNode, type CanvasEdge, type CanvasViewport,
} from '../features/canvas';
import { showToast } from '../components/toast';

const $ = (id: string) => document.getElementById(id);

// ── Module state ───────────────────────────────────────────────────────
let _canvases: Canvas[] = [];
let _activeCanvasId: string | null = null;
let _nodes: CanvasNode[] = [];
let _edges: CanvasEdge[] = [];
let _viewport: CanvasViewport = { x: 0, y: 0, zoom: 1 };

// Interaction state
let _isPanning = false;
let _panStart = { x: 0, y: 0 };
let _draggingNodeId: string | null = null;
let _dragOffset = { x: 0, y: 0 };
let _selectedNodeId: string | null = null;
let _connectingFromId: string | null = null;

function esc(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

// ── Public API ─────────────────────────────────────────────────────────

export async function loadCanvas() {
  try {
    _canvases = await listCanvases();
  } catch { _canvases = []; }
  renderCanvasList();
  if (_activeCanvasId) {
    await loadActiveCanvas();
  } else if (_canvases.length > 0) {
    _activeCanvasId = _canvases[0].id;
    await loadActiveCanvas();
  } else {
    renderEmptyState();
  }
}

// ── Canvas List (Sidebar) ─────────────────────────────────────────────

function renderCanvasList() {
  const list = $('canvas-list');
  if (!list) return;
  list.innerHTML = '';

  for (const c of _canvases) {
    const item = document.createElement('div');
    item.className = `canvas-list-item${c.id === _activeCanvasId ? ' active' : ''}`;
    item.style.cssText = `display:flex;align-items:center;justify-content:space-between;padding:8px 12px;cursor:pointer;border-radius:6px;font-size:13px;${c.id === _activeCanvasId ? 'background:var(--bg-active,rgba(99,102,241,0.12));color:var(--accent)' : ''}`;
    item.innerHTML = `
      <span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${esc(c.name)}</span>
      <span style="font-size:11px;color:var(--text-muted)">${c.node_count} nodes</span>`;
    item.addEventListener('click', () => {
      _activeCanvasId = c.id;
      loadActiveCanvas();
      renderCanvasList();
    });
    list.appendChild(item);
  }
}

async function loadActiveCanvas() {
  if (!_activeCanvasId) return;
  const canvas = _canvases.find(c => c.id === _activeCanvasId);
  if (canvas) _viewport = canvas.viewport ?? { x: 0, y: 0, zoom: 1 };
  try {
    [_nodes, _edges] = await Promise.all([
      loadNodes(_activeCanvasId),
      loadEdges(_activeCanvasId),
    ]);
  } catch { _nodes = []; _edges = []; }
  renderSurface();
}

function renderEmptyState() {
  const surface = $('canvas-surface');
  if (!surface) return;
  surface.innerHTML = `<div style="display:flex;flex-direction:column;align-items:center;justify-content:center;height:100%;color:var(--text-muted);gap:12px">
    <span class="ms ms-xl">palette</span>
    <p style="font-size:15px;font-weight:600">No canvases yet</p>
    <p style="font-size:13px">Create one to start building your visual workspace.</p>
  </div>`;
}

// ── Canvas Surface ────────────────────────────────────────────────────

function renderSurface() {
  const surface = $('canvas-surface');
  if (!surface) return;
  surface.innerHTML = '';
  surface.style.cssText = 'position:relative;flex:1;overflow:hidden;background:var(--bg-primary);cursor:grab';

  // World container (panned/zoomed)
  const world = document.createElement('div');
  world.id = 'canvas-world';
  world.style.cssText = `position:absolute;top:0;left:0;transform-origin:0 0;transform:translate(${_viewport.x}px,${_viewport.y}px) scale(${_viewport.zoom})`;

  // SVG layer for edges
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.id = 'canvas-edges-svg';
  svg.style.cssText = 'position:absolute;top:0;left:0;width:10000px;height:10000px;pointer-events:none;overflow:visible';
  world.appendChild(svg);

  // Render nodes
  for (const node of _nodes) {
    world.appendChild(createNodeElement(node));
  }
  surface.appendChild(world);

  renderEdges();
  bindSurfaceEvents(surface, world);
}

function createNodeElement(node: CanvasNode): HTMLDivElement {
  const el = document.createElement('div');
  el.className = 'canvas-node';
  el.dataset.nodeId = node.id;
  const isSelected = node.id === _selectedNodeId;
  el.style.cssText = `position:absolute;left:${node.x}px;top:${node.y}px;width:${node.width}px;min-height:${node.height}px;background:var(--bg-secondary);border:2px solid ${isSelected ? 'var(--accent)' : node.color};border-radius:8px;box-shadow:0 2px 8px rgba(0,0,0,0.15);cursor:move;user-select:none;display:flex;flex-direction:column;z-index:${node.z_index}`;

  // Header with kind icon + title
  const header = document.createElement('div');
  header.style.cssText = `padding:8px 10px;font-size:12px;font-weight:600;border-bottom:1px solid var(--border);display:flex;align-items:center;gap:6px;justify-content:space-between`;
  header.innerHTML = `<span style="display:flex;align-items:center;gap:4px"><span>${kindIcon(node.kind)}</span> <span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:140px">${esc(node.title || node.kind)}</span></span>`;

  // Action buttons
  const actions = document.createElement('span');
  actions.style.cssText = 'display:flex;gap:2px;flex-shrink:0';

  const editBtn = document.createElement('button');
  editBtn.innerHTML = '<span class="ms ms-sm">edit</span>';
  editBtn.title = 'Edit';
  editBtn.style.cssText = 'background:none;border:none;cursor:pointer;padding:2px;display:flex;align-items:center';
  editBtn.addEventListener('click', (e) => { e.stopPropagation(); openNodeEditor(node); });

  const connectBtn = document.createElement('button');
  connectBtn.innerHTML = '<span class="ms ms-sm">link</span>';
  connectBtn.title = 'Connect to another node';
  connectBtn.style.cssText = 'background:none;border:none;cursor:pointer;padding:2px;display:flex;align-items:center';
  connectBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    if (_connectingFromId === node.id) {
      _connectingFromId = null;
      showToast('Connection cancelled', 'info');
    } else if (_connectingFromId) {
      // Complete connection
      handleCreateEdge(_connectingFromId, node.id);
    } else {
      _connectingFromId = node.id;
      showToast('Click link on another node to connect', 'info');
    }
  });

  const delBtn = document.createElement('button');
  delBtn.innerHTML = '<span class="ms ms-sm">delete</span>';
  delBtn.title = 'Delete node';
  delBtn.style.cssText = 'background:none;border:none;cursor:pointer;padding:2px;display:flex;align-items:center';
  delBtn.addEventListener('click', (e) => { e.stopPropagation(); handleDeleteNode(node.id); });

  actions.appendChild(editBtn);
  actions.appendChild(connectBtn);
  actions.appendChild(delBtn);
  header.appendChild(actions);
  el.appendChild(header);

  // Body
  if (node.content && !node.collapsed) {
    const body = document.createElement('div');
    body.style.cssText = 'padding:8px 10px;font-size:12px;color:var(--text-secondary);white-space:pre-wrap;word-break:break-word;overflow:auto;max-height:200px';
    if (node.kind === 'code') {
      body.innerHTML = `<pre style="margin:0;font-family:monospace;font-size:11px;background:var(--bg-tertiary,#1a1a2e);padding:6px;border-radius:4px;overflow-x:auto">${esc(node.content)}</pre>`;
    } else {
      body.textContent = node.content;
    }
    el.appendChild(body);
  }

  // Color strip
  const strip = document.createElement('div');
  strip.style.cssText = `height:3px;background:${node.color};border-radius:0 0 6px 6px;flex-shrink:0`;
  el.appendChild(strip);

  // Select on click
  el.addEventListener('mousedown', (e) => {
    if ((e.target as HTMLElement).tagName === 'BUTTON') return;
    _selectedNodeId = node.id;
    _draggingNodeId = node.id;
    const rect = el.getBoundingClientRect();
    _dragOffset = { x: e.clientX - rect.left, y: e.clientY - rect.top };
    e.stopPropagation();
    renderSurface();
  });

  return el;
}

function renderEdges() {
  const svg = document.getElementById('canvas-edges-svg') as Element | null;
  if (!svg) return;
  svg.innerHTML = '';

  for (const edge of _edges) {
    const fromNode = _nodes.find(n => n.id === edge.from_node);
    const toNode = _nodes.find(n => n.id === edge.to_node);
    if (!fromNode || !toNode) continue;

    const from = getNodeCenter(fromNode.x, fromNode.y, fromNode.width, fromNode.height);
    const to = getNodeCenter(toNode.x, toNode.y, toNode.width, toNode.height);

    const line = document.createElementNS('http://www.w3.org/2000/svg', 'line');
    line.setAttribute('x1', String(from.cx));
    line.setAttribute('y1', String(from.cy));
    line.setAttribute('x2', String(to.cx));
    line.setAttribute('y2', String(to.cy));
    line.setAttribute('stroke', edge.color || '#888');
    line.setAttribute('stroke-width', '2');
    if (edge.style === 'dashed') line.setAttribute('stroke-dasharray', '8,4');
    if (edge.style === 'dotted') line.setAttribute('stroke-dasharray', '2,4');
    svg.appendChild(line);

    // Label
    if (edge.label) {
      const text = document.createElementNS('http://www.w3.org/2000/svg', 'text');
      text.setAttribute('x', String((from.cx + to.cx) / 2));
      text.setAttribute('y', String((from.cy + to.cy) / 2 - 6));
      text.setAttribute('text-anchor', 'middle');
      text.setAttribute('fill', 'var(--text-muted, #888)');
      text.setAttribute('font-size', '11');
      text.textContent = edge.label;
      svg.appendChild(text);
    }

    // Delete handle (small circle at midpoint)
    const mid = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
    mid.setAttribute('cx', String((from.cx + to.cx) / 2));
    mid.setAttribute('cy', String((from.cy + to.cy) / 2));
    mid.setAttribute('r', '6');
    mid.setAttribute('fill', edge.color || '#888');
    mid.setAttribute('opacity', '0.4');
    mid.setAttribute('cursor', 'pointer');
    mid.style.pointerEvents = 'all';
    mid.addEventListener('click', () => handleDeleteEdge(edge.id));
    svg.appendChild(mid);
  }
}

// ── Surface Events ────────────────────────────────────────────────────

function bindSurfaceEvents(surface: HTMLElement, world: HTMLElement) {
  // Pan
  surface.addEventListener('mousedown', (e) => {
    if (e.target !== surface) return;
    _isPanning = true;
    _panStart = { x: e.clientX - _viewport.x, y: e.clientY - _viewport.y };
    surface.style.cursor = 'grabbing';
    _selectedNodeId = null;
  });

  const onMove = (e: MouseEvent) => {
    if (_isPanning) {
      _viewport.x = e.clientX - _panStart.x;
      _viewport.y = e.clientY - _panStart.y;
      world.style.transform = `translate(${_viewport.x}px,${_viewport.y}px) scale(${_viewport.zoom})`;
    }
    if (_draggingNodeId) {
      const node = _nodes.find(n => n.id === _draggingNodeId);
      if (!node) return;
      const newX = snapToGrid((e.clientX - _viewport.x - _dragOffset.x * _viewport.zoom) / _viewport.zoom);
      const newY = snapToGrid((e.clientY - _viewport.y - _dragOffset.y * _viewport.zoom) / _viewport.zoom);
      node.x = newX;
      node.y = newY;
      const el = surface.querySelector(`[data-node-id="${_draggingNodeId}"]`) as HTMLElement | null;
      if (el) {
        el.style.left = `${newX}px`;
        el.style.top = `${newY}px`;
      }
      renderEdges();
    }
  };

  const onUp = () => {
    if (_isPanning) {
      _isPanning = false;
      surface.style.cursor = 'grab';
      saveViewport();
    }
    if (_draggingNodeId) {
      const node = _nodes.find(n => n.id === _draggingNodeId);
      if (node) {
        updateNode(node.id, { x: node.x, y: node.y }).catch(() => {});
      }
      _draggingNodeId = null;
    }
  };

  surface.addEventListener('mousemove', onMove);
  surface.addEventListener('mouseup', onUp);
  surface.addEventListener('mouseleave', onUp);

  // Zoom
  surface.addEventListener('wheel', (e) => {
    e.preventDefault();
    const delta = e.deltaY > 0 ? -0.1 : 0.1;
    _viewport.zoom = clampZoom(_viewport.zoom + delta);
    world.style.transform = `translate(${_viewport.x}px,${_viewport.y}px) scale(${_viewport.zoom})`;
    saveViewport();
  }, { passive: false });
}

// ── CRUD Handlers ─────────────────────────────────────────────────────

async function handleCreateCanvas() {
  const name = prompt('Canvas name:');
  if (!name?.trim()) return;
  try {
    const canvas = await createCanvas(name.trim());
    _canvases.unshift(canvas);
    _activeCanvasId = canvas.id;
    renderCanvasList();
    await loadActiveCanvas();
    showToast(`Canvas "${name}" created`, 'success');
  } catch (e: any) {
    showToast(`Failed: ${e.message ?? e}`, 'error');
  }
}

async function handleDeleteCanvas(id: string) {
  const canvas = _canvases.find(c => c.id === id);
  if (!confirm(`Delete canvas "${canvas?.name ?? id}"? This removes all nodes and edges.`)) return;
  try {
    await deleteCanvas(id);
    _canvases = _canvases.filter(c => c.id !== id);
    if (_activeCanvasId === id) {
      _activeCanvasId = _canvases[0]?.id ?? null;
    }
    renderCanvasList();
    if (_activeCanvasId) await loadActiveCanvas();
    else renderEmptyState();
    showToast('Canvas deleted', 'success');
  } catch (e: any) {
    showToast(`Failed: ${e.message ?? e}`, 'error');
  }
}

async function handleAddNode(kind: string) {
  if (!_activeCanvasId) return;
  const cx = (-_viewport.x + 400) / _viewport.zoom;
  const cy = (-_viewport.y + 300) / _viewport.zoom;
  const x = snapToGrid(cx);
  const y = snapToGrid(cy);
  try {
    const node = await createNode(_activeCanvasId, kind, x, y);
    _nodes.push(node);
    renderSurface();
  } catch (e: any) {
    showToast(`Failed: ${e.message ?? e}`, 'error');
  }
}

async function handleDeleteNode(id: string) {
  try {
    await deleteNode(id);
    _nodes = _nodes.filter(n => n.id !== id);
    _edges = _edges.filter(e => e.from_node !== id && e.to_node !== id);
    if (_selectedNodeId === id) _selectedNodeId = null;
    renderSurface();
    showToast('Node deleted', 'success');
  } catch (e: any) {
    showToast(`Failed: ${e.message ?? e}`, 'error');
  }
}

async function handleCreateEdge(fromId: string, toId: string) {
  if (!_activeCanvasId) return;
  _connectingFromId = null;
  if (fromId === toId) return;
  // Check for duplicate
  if (_edges.some(e => e.from_node === fromId && e.to_node === toId)) {
    showToast('Edge already exists', 'info');
    return;
  }
  try {
    const edge = await createEdge(_activeCanvasId, fromId, toId);
    _edges.push(edge);
    renderEdges();
    showToast('Connected', 'success');
  } catch (e: any) {
    showToast(`Failed: ${e.message ?? e}`, 'error');
  }
}

async function handleDeleteEdge(id: string) {
  try {
    await deleteEdge(id);
    _edges = _edges.filter(e => e.id !== id);
    renderEdges();
  } catch (e: any) {
    showToast(`Failed: ${e.message ?? e}`, 'error');
  }
}

function openNodeEditor(node: CanvasNode) {
  const dialog = document.createElement('div');
  dialog.style.cssText = 'position:fixed;top:0;left:0;right:0;bottom:0;background:rgba(0,0,0,0.5);display:flex;align-items:center;justify-content:center;z-index:10000';

  const modal = document.createElement('div');
  modal.style.cssText = 'background:var(--bg-primary);border-radius:12px;padding:24px;width:420px;max-height:80vh;overflow-y:auto;box-shadow:0 8px 32px rgba(0,0,0,0.3)';
  modal.innerHTML = `
    <h3 style="margin:0 0 16px 0;font-size:15px">${kindIcon(node.kind)} Edit Node</h3>
    <div style="display:flex;flex-direction:column;gap:12px">
      <label style="font-size:12px;font-weight:600">Title
        <input type="text" id="ne-title" value="${esc(node.title)}" style="width:100%;margin-top:4px;padding:8px;border-radius:6px;border:1px solid var(--border);background:var(--bg-secondary);color:var(--text-primary);font-size:13px" />
      </label>
      <label style="font-size:12px;font-weight:600">Content
        <textarea id="ne-content" rows="6" style="width:100%;margin-top:4px;padding:8px;border-radius:6px;border:1px solid var(--border);background:var(--bg-secondary);color:var(--text-primary);font-size:13px;resize:vertical;font-family:${node.kind === 'code' ? 'monospace' : 'inherit'}">${esc(node.content)}</textarea>
      </label>
      <label style="font-size:12px;font-weight:600">Color
        <div style="display:flex;gap:6px;margin-top:4px;flex-wrap:wrap" id="ne-colors"></div>
      </label>
      <div style="display:flex;gap:8px;justify-content:flex-end;margin-top:8px">
        <button class="btn btn-ghost btn-sm" id="ne-cancel">Cancel</button>
        <button class="btn btn-primary btn-sm" id="ne-save">Save</button>
      </div>
    </div>`;

  dialog.appendChild(modal);
  document.body.appendChild(dialog);

  // Color swatches
  const colorsEl = modal.querySelector('#ne-colors')!;
  for (const c of NODE_COLORS) {
    const swatch = document.createElement('div');
    swatch.style.cssText = `width:24px;height:24px;border-radius:50%;background:${c};cursor:pointer;border:2px solid ${c === node.color ? 'white' : 'transparent'}`;
    swatch.dataset.color = c;
    swatch.addEventListener('click', () => {
      colorsEl.querySelectorAll('div').forEach(d => (d as HTMLElement).style.borderColor = 'transparent');
      swatch.style.borderColor = 'white';
    });
    colorsEl.appendChild(swatch);
  }

  dialog.addEventListener('click', (e) => { if (e.target === dialog) dialog.remove(); });
  modal.querySelector('#ne-cancel')!.addEventListener('click', () => dialog.remove());
  modal.querySelector('#ne-save')!.addEventListener('click', async () => {
    const title = (modal.querySelector('#ne-title') as HTMLInputElement).value;
    const content = (modal.querySelector('#ne-content') as HTMLTextAreaElement).value;
    const selected = colorsEl.querySelector('div[style*="border-color: white"]') as HTMLElement | null
      ?? colorsEl.querySelector('div[style*="border-color:white"]') as HTMLElement | null;
    const color = selected?.dataset.color ?? node.color;
    try {
      await updateNode(node.id, { title, content, color });
      Object.assign(node, { title, content, color });
      renderSurface();
      dialog.remove();
      showToast('Node updated', 'success');
    } catch (e: any) {
      showToast(`Failed: ${e.message ?? e}`, 'error');
    }
  });
}

function saveViewport() {
  if (!_activeCanvasId) return;
  updateCanvas(_activeCanvasId, undefined, undefined, _viewport).catch(() => {});
}

// ── Toolbar Rendering ─────────────────────────────────────────────────

export function initCanvasView() {
  // New canvas button
  $('canvas-new-btn')?.addEventListener('click', handleCreateCanvas);

  // Delete active canvas
  $('canvas-delete-btn')?.addEventListener('click', () => {
    if (_activeCanvasId) handleDeleteCanvas(_activeCanvasId);
  });

  // Node kind toolbar
  const toolbar = $('canvas-toolbar');
  if (toolbar) {
    toolbar.innerHTML = '';
    for (const kind of NODE_KINDS) {
      const btn = document.createElement('button');
      btn.className = 'btn btn-ghost btn-sm';
      btn.title = `Add ${kind.label}`;
      btn.innerHTML = `<span style="font-size:14px">${kind.icon}</span>`;
      btn.style.cssText = 'padding:6px 8px';
      btn.addEventListener('click', () => handleAddNode(kind.value));
      toolbar.appendChild(btn);
    }
  }

  // Zoom controls
  $('canvas-zoom-in')?.addEventListener('click', () => {
    _viewport.zoom = clampZoom(_viewport.zoom + 0.1);
    const world = $('canvas-world');
    if (world) world.style.transform = `translate(${_viewport.x}px,${_viewport.y}px) scale(${_viewport.zoom})`;
    updateZoomLabel();
    saveViewport();
  });
  $('canvas-zoom-out')?.addEventListener('click', () => {
    _viewport.zoom = clampZoom(_viewport.zoom - 0.1);
    const world = $('canvas-world');
    if (world) world.style.transform = `translate(${_viewport.x}px,${_viewport.y}px) scale(${_viewport.zoom})`;
    updateZoomLabel();
    saveViewport();
  });
  $('canvas-zoom-reset')?.addEventListener('click', () => {
    _viewport = { x: 0, y: 0, zoom: 1 };
    const world = $('canvas-world');
    if (world) world.style.transform = `translate(0px,0px) scale(1)`;
    updateZoomLabel();
    saveViewport();
  });
}

function updateZoomLabel() {
  const label = $('canvas-zoom-label');
  if (label) label.textContent = `${Math.round(_viewport.zoom * 100)}%`;
}
