// Memory Palace — Interactive Force-Directed Knowledge Graph
// Kinetic design: glowing nodes, animated particle edges, category nebulae
// Pan, zoom, hover tooltips, click-to-recall, real edges

import { pawEngine } from '../../engine';
import { $, escHtml } from '../../components/helpers';
import { getCategoryColor } from './atoms';
import type { MemoryEdge } from '../../engine/atoms/types';

// ── Types ──────────────────────────────────────────────────────────────────

interface GraphNode {
  id: string;
  label: string;
  category: string;
  importance: number;
  x: number;
  y: number;
  vx: number;
  vy: number;
  radius: number;
  pinned: boolean;
  pulsePhase: number;
}

interface EdgeParticle {
  t: number;
  speed: number;
}

interface GraphEdge {
  source: GraphNode;
  target: GraphNode;
  type: string;
  weight: number;
  particles: EdgeParticle[];
}

// ── State ──────────────────────────────────────────────────────────────────

let _nodes: GraphNode[] = [];
let _edges: GraphEdge[] = [];
let _canvas: HTMLCanvasElement | null = null;
let _ctx: CanvasRenderingContext2D | null = null;
let _animId = 0;
let _hoveredNode: GraphNode | null = null;
let _dragNode: GraphNode | null = null;
let _isPanning = false;
let _time = 0;

// Camera transform
let _camX = 0;
let _camY = 0;
let _zoom = 1;

// Simulation
let _simRunning = false;
let _alpha = 1;

// Tooltip element
let _tooltip: HTMLDivElement | null = null;
// Mini-mode flag: true when rendering inside a card (suppresses labels/HUD)
let _miniMode = false;

// ── Edge type colors ───────────────────────────────────────────────────────

const EDGE_TYPE_COLORS: Record<string, string> = {
  related_to: 'rgba(212, 101, 74, 0.5)',
  supported_by: 'rgba(143, 176, 160, 0.6)',
  caused_by: 'rgba(212, 168, 83, 0.6)',
  contradicts: 'rgba(255, 77, 77, 0.6)',
  consolidated_into: 'rgba(122, 139, 154, 0.5)',
  supersedes: 'rgba(163, 130, 100, 0.5)',
  temporally_adjacent: 'rgba(122, 139, 154, 0.4)',
  inferred_from: 'rgba(167, 139, 250, 0.5)',
  learned_from: 'rgba(143, 176, 160, 0.5)',
  example_of: 'rgba(90, 150, 200, 0.5)',
  part_of: 'rgba(212, 168, 83, 0.5)',
  similar_to: 'rgba(212, 101, 74, 0.4)',
};

// ── Color helpers ──────────────────────────────────────────────────────────

function _colorToRgb(color: string): { r: number; g: number; b: number } {
  if (color.startsWith('#')) {
    const h = color.replace('#', '');
    return {
      r: parseInt(h.slice(0, 2), 16),
      g: parseInt(h.slice(2, 4), 16),
      b: parseInt(h.slice(4, 6), 16),
    };
  }
  // hsl(h, s%, l%) → approximate RGB
  const m = color.match(/hsl\((\d+),\s*(\d+)%,\s*(\d+)%\)/);
  if (m) {
    const hue = +m[1] / 360;
    const s = +m[2] / 100;
    const l = +m[3] / 100;
    const hue2rgb = (p: number, q: number, t: number) => {
      if (t < 0) t += 1;
      if (t > 1) t -= 1;
      if (t < 1 / 6) return p + (q - p) * 6 * t;
      if (t < 1 / 2) return q;
      if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
      return p;
    };
    const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
    const p = 2 * l - q;
    return {
      r: Math.round(hue2rgb(p, q, hue + 1 / 3) * 255),
      g: Math.round(hue2rgb(p, q, hue) * 255),
      b: Math.round(hue2rgb(p, q, hue - 1 / 3) * 255),
    };
  }
  return { r: 212, g: 101, b: 74 }; // warm fallback
}

// ── Public API ─────────────────────────────────────────────────────────────

export function initPalaceGraph(): void {
  // rendering triggered by tab switch
}

/** Stop the animation loop and release all graph state. Safe to call multiple times. */
export function destroyPalaceGraph(): void {
  if (_animId) {
    cancelAnimationFrame(_animId);
    _animId = 0;
  }
  _simRunning = false;
  _miniMode = false;
  _eventsBound = false;
  _tooltip?.remove();
  _tooltip = null;
  _canvas = null;
  _ctx = null;
  _nodes = [];
  _edges = [];
  _hoveredNode = null;
  _dragNode = null;
}

/**
 * Render the knowledge graph into an arbitrary container element.
 * Creates its own <canvas> if none already present.
 * Use this for embedded views (e.g. Today dashboard); use renderPalaceGraph()
 * for the full Memory Palace view where fixed DOM IDs are present.
 */
export async function renderPalaceGraphInto(container: HTMLElement): Promise<void> {
  destroyPalaceGraph();
  _miniMode = true;

  // Ensure the container has position:relative so the canvas fills it
  if (getComputedStyle(container).position === 'static') {
    container.style.position = 'relative';
  }

  let canvas = container.querySelector('canvas') as HTMLCanvasElement | null;
  if (!canvas) {
    canvas = document.createElement('canvas');
    // Let CSS control size; _buildGraph will set pixel dimensions after layout
    canvas.style.cssText = 'display:block;position:absolute;inset:0;';
    container.appendChild(canvas);
  }

  // Wait for a paint frame so getBoundingClientRect returns real dimensions
  await new Promise<void>((r) => requestAnimationFrame(() => requestAnimationFrame(() => r())));

  try {
    const [engineMems, engineEdges] = await Promise.all([
      pawEngine.memoryList(200),
      pawEngine.memoryEdges(500).catch(() => [] as MemoryEdge[]),
    ]);

    if (!engineMems.length) {
      const dpr = window.devicePixelRatio || 1;
      const w = container.clientWidth || 300;
      const h = container.clientHeight || 180;
      canvas.width = w * dpr;
      canvas.height = h * dpr;
      canvas.style.width = `${w}px`;
      canvas.style.height = `${h}px`;
      const ctx = canvas.getContext('2d');
      if (ctx) {
        ctx.scale(dpr, dpr);
        ctx.fillStyle = 'rgba(255,255,255,0.22)';
        ctx.font = '700 11px ui-monospace,monospace';
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';
        ctx.fillText('No memories yet', w / 2, h / 2);
      }
      return;
    }

    _buildGraph(engineMems, engineEdges, canvas);
  } catch (e) {
    console.warn('[graph] renderPalaceGraphInto failed:', e);
  }
}

export async function renderPalaceGraph(): Promise<void> {
  _miniMode = false;
  _eventsBound = false;
  const canvas = $('palace-graph-render') as HTMLCanvasElement | null;
  const emptyEl = $('palace-graph-empty');
  if (!canvas) return;

  if (emptyEl) {
    emptyEl.style.display = 'flex';
    (emptyEl as HTMLElement).innerHTML = `
      <div class="empty-icon"><span class="ms" style="font-size:48px">hub</span></div>
      <div class="empty-title">Loading memory map\u2026</div>
    `;
  }

  try {
    const [engineMems, engineEdges] = await Promise.all([
      pawEngine.memoryList(200),
      pawEngine.memoryEdges(500).catch(() => [] as MemoryEdge[]),
    ]);

    if (!engineMems.length) {
      if (emptyEl) {
        (emptyEl as HTMLElement).innerHTML = `
          <div class="empty-icon"><span class="ms" style="font-size:48px">hub</span></div>
          <div class="empty-title">No memories yet</div>
          <div class="empty-subtitle">Memories will appear here as your agents learn</div>
        `;
        emptyEl.style.display = 'flex';
      }
      return;
    }

    if (emptyEl) emptyEl.style.display = 'none';
    canvas.style.display = '';

    _buildGraph(engineMems, engineEdges, canvas);
  } catch (e) {
    console.warn('Graph load failed:', e);
    if (emptyEl) {
      (emptyEl as HTMLElement).innerHTML = `
        <div class="empty-icon"><span class="ms" style="font-size:48px">error</span></div>
        <div class="empty-title">Failed to load memory map</div>
        <div class="empty-subtitle">${escHtml(String(e))}</div>
      `;
      emptyEl.style.display = 'flex';
    }
  }
}

// ── Graph construction ─────────────────────────────────────────────────────

interface RawMem {
  id: string;
  content: string;
  category: string;
  importance: number;
}

function _buildGraph(mems: RawMem[], rawEdges: MemoryEdge[], canvas: HTMLCanvasElement): void {
  if (_animId) cancelAnimationFrame(_animId);
  _simRunning = false;

  const rect = canvas.parentElement?.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  const w = rect?.width ?? 800;
  const h = rect?.height ?? 600;
  canvas.width = w * dpr;
  canvas.height = h * dpr;
  canvas.style.width = `${w}px`;
  canvas.style.height = `${h}px`;

  _ctx = canvas.getContext('2d');
  if (!_ctx) return;
  _ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  _canvas = canvas;

  // Build node map grouped by category
  const nodeMap = new Map<string, GraphNode>();
  const catGroups = new Map<string, RawMem[]>();
  for (const m of mems) {
    const cat = m.category || 'other';
    if (!catGroups.has(cat)) catGroups.set(cat, []);
    catGroups.get(cat)!.push(m);
  }

  const cats = Array.from(catGroups.keys());
  const cx = w / 2;
  const cy = h / 2;
  const clusterRadius = Math.min(cx, cy) * 0.55;

  cats.forEach((cat, ci) => {
    const angle = (ci / cats.length) * Math.PI * 2 - Math.PI / 2;
    const gx = cx + Math.cos(angle) * clusterRadius;
    const gy = cy + Math.sin(angle) * clusterRadius;
    const group = catGroups.get(cat)!;

    group.forEach((m, mi) => {
      const innerAngle = (mi / Math.max(group.length, 1)) * Math.PI * 2;
      const spread = Math.min(35 + group.length * 10, 140);
      const node: GraphNode = {
        id: m.id,
        label: m.content.length > 80 ? `${m.content.slice(0, 77)}...` : m.content,
        category: cat,
        importance: m.importance,
        x: gx + Math.cos(innerAngle) * spread * (0.4 + Math.random() * 0.6),
        y: gy + Math.sin(innerAngle) * spread * (0.4 + Math.random() * 0.6),
        vx: 0,
        vy: 0,
        radius: 6 + m.importance * 1.2,
        pinned: false,
        pulsePhase: Math.random() * Math.PI * 2,
      };
      nodeMap.set(m.id, node);
    });
  });

  _nodes = Array.from(nodeMap.values());

  // Build edges with particles from DB
  _edges = [];
  for (const e of rawEdges) {
    const src = nodeMap.get(e.source_id);
    const tgt = nodeMap.get(e.target_id);
    if (src && tgt) {
      _addEdge(src, tgt, e.edge_type, e.weight);
    }
  }

  // If no DB edges, infer connections client-side
  if (_edges.length === 0 && _nodes.length > 1) {
    _inferEdges(_nodes);
  }

  _camX = 0;
  _camY = 0;
  _zoom = _miniMode ? 0.38 : 1;
  _hoveredNode = null;
  _dragNode = null;
  _time = 0;

  console.debug(
    `[graph] ${_nodes.length} nodes, ${_edges.length} edges (${rawEdges.length} from DB)`,
  );

  if (!_tooltip && !_miniMode) {
    _tooltip = document.createElement('div');
    _tooltip.className = 'palace-graph-tooltip';
    canvas.parentElement?.appendChild(_tooltip);
  }
  if (_tooltip) _tooltip.style.display = 'none';

  _bindEvents(canvas);

  _alpha = 1;
  _simRunning = true;
  _tick();
}

// ── Edge helpers ───────────────────────────────────────────────────────────

function _addEdge(src: GraphNode, tgt: GraphNode, type: string, weight: number): void {
  const particleCount = Math.max(1, Math.round(weight * 3));
  const particles: EdgeParticle[] = Array.from({ length: particleCount }, (_, i) => ({
    t: i / particleCount,
    speed: 0.002 + Math.random() * 0.004,
  }));
  _edges.push({ source: src, target: tgt, type, weight, particles });
}

/** Infer edges client-side when the DB has none — same-category proximity + word overlap */
function _inferEdges(nodes: GraphNode[]): void {
  const connected = new Set<string>();
  const edgeKey = (a: string, b: string) => (a < b ? `${a}:${b}` : `${b}:${a}`);

  // 1. Same-category: connect each node to 1-2 random neighbors
  const byCategory = new Map<string, GraphNode[]>();
  for (const n of nodes) {
    if (!byCategory.has(n.category)) byCategory.set(n.category, []);
    byCategory.get(n.category)!.push(n);
  }

  for (const group of byCategory.values()) {
    for (let i = 0; i < group.length; i++) {
      const a = group[i];
      const neighbors = group
        .filter((_, j) => j !== i)
        .sort(() => Math.random() - 0.5)
        .slice(0, 2);
      for (const b of neighbors) {
        const key = edgeKey(a.id, b.id);
        if (!connected.has(key)) {
          connected.add(key);
          _addEdge(a, b, 'similar_to', 0.4 + Math.random() * 0.3);
        }
      }
    }
  }

  // 2. Cross-category: connect nodes that share significant words
  const stopWords = new Set([
    'the',
    'a',
    'an',
    'is',
    'was',
    'are',
    'to',
    'of',
    'in',
    'for',
    'and',
    'or',
    'it',
    'that',
    'this',
    'with',
    'on',
    'at',
    'by',
    'from',
    'as',
    'user',
    'asked',
    'work',
    'session',
  ]);
  const nodeWords = new Map<GraphNode, Set<string>>();
  for (const n of nodes) {
    const words = new Set(
      n.label
        .toLowerCase()
        .split(/\W+/)
        .filter((w) => w.length > 3 && !stopWords.has(w)),
    );
    nodeWords.set(n, words);
  }

  let crossCount = 0;
  const maxCross = Math.min(nodes.length * 2, 60);
  for (let i = 0; i < nodes.length && crossCount < maxCross; i++) {
    const a = nodes[i];
    const wordsA = nodeWords.get(a)!;
    if (wordsA.size === 0) continue;
    for (let j = i + 1; j < nodes.length && crossCount < maxCross; j++) {
      const b = nodes[j];
      if (a.category === b.category) continue;
      const key = edgeKey(a.id, b.id);
      if (connected.has(key)) continue;
      const wordsB = nodeWords.get(b)!;
      let overlap = 0;
      for (const w of wordsA) if (wordsB.has(w)) overlap++;
      if (overlap >= 2) {
        connected.add(key);
        const weight = Math.min(0.3 + overlap * 0.15, 0.9);
        _addEdge(a, b, 'related_to', weight);
        crossCount++;
      }
    }
  }
}

// ── Force simulation ───────────────────────────────────────────────────────

function _tick(): void {
  if (!_simRunning || !_ctx || !_canvas) return;

  const w = _canvas.width / (window.devicePixelRatio || 1);
  const h = _canvas.height / (window.devicePixelRatio || 1);
  _time++;

  if (_alpha > 0.001) {
    _applyForces(w, h);
    _alpha *= 0.995;
  }

  for (const n of _nodes) {
    if (n.pinned) continue;
    n.x += n.vx;
    n.y += n.vy;
    n.vx *= 0.85;
    n.vy *= 0.85;
  }

  // Advance edge particles
  for (const e of _edges) {
    for (const p of e.particles) {
      p.t += p.speed;
      if (p.t > 1) p.t -= 1;
    }
  }

  _draw(w, h);
  _animId = requestAnimationFrame(_tick);
}

function _applyForces(w: number, h: number): void {
  const strength = _alpha;
  const gravityStrength = 0.004 * strength;
  const cx = w / 2;
  const cy = h / 2;

  for (const n of _nodes) {
    n.vx += (cx - n.x) * gravityStrength;
    n.vy += (cy - n.y) * gravityStrength;
  }

  const repulsionStrength = 800 * strength;
  for (let i = 0; i < _nodes.length; i++) {
    for (let j = i + 1; j < _nodes.length; j++) {
      const a = _nodes[i];
      const b = _nodes[j];
      const dx = b.x - a.x;
      const dy = b.y - a.y;
      const dist2 = dx * dx + dy * dy || 1;
      const dist = Math.sqrt(dist2);
      const force = repulsionStrength / dist2;
      const catFactor = a.category === b.category ? 0.5 : 1;
      const fx = (dx / dist) * force * catFactor;
      const fy = (dy / dist) * force * catFactor;
      a.vx -= fx;
      a.vy -= fy;
      b.vx += fx;
      b.vy += fy;
    }
  }

  const springStrength = 0.04 * strength;
  const idealLength = 140;
  for (const e of _edges) {
    const dx = e.target.x - e.source.x;
    const dy = e.target.y - e.source.y;
    const dist = Math.sqrt(dx * dx + dy * dy) || 1;
    const displacement = dist - idealLength;
    const force = displacement * springStrength * e.weight;
    const fx = (dx / dist) * force;
    const fy = (dy / dist) * force;
    e.source.vx += fx;
    e.source.vy += fy;
    e.target.vx -= fx;
    e.target.vy -= fy;
  }

  const catAttract = 0.002 * strength;
  const catCenters = new Map<string, { x: number; y: number; count: number }>();
  for (const n of _nodes) {
    const c = catCenters.get(n.category);
    if (c) {
      c.x += n.x;
      c.y += n.y;
      c.count++;
    } else {
      catCenters.set(n.category, { x: n.x, y: n.y, count: 1 });
    }
  }
  for (const c of catCenters.values()) {
    c.x /= c.count;
    c.y /= c.count;
  }
  for (const n of _nodes) {
    const c = catCenters.get(n.category)!;
    n.vx += (c.x - n.x) * catAttract;
    n.vy += (c.y - n.y) * catAttract;
  }
}

// ── Bezier helpers ─────────────────────────────────────────────────────────

function _edgeControlPoint(
  ax: number,
  ay: number,
  bx: number,
  by: number,
): { cpx: number; cpy: number } {
  const mx = (ax + bx) / 2;
  const my = (ay + by) / 2;
  const dx = bx - ax;
  const dy = by - ay;
  const len = Math.sqrt(dx * dx + dy * dy) || 1;
  const offset = len * 0.15;
  return { cpx: mx - (dy / len) * offset, cpy: my + (dx / len) * offset };
}

function _quadBezierAt(
  ax: number,
  ay: number,
  bx: number,
  by: number,
  cpx: number,
  cpy: number,
  t: number,
): { x: number; y: number } {
  const mt = 1 - t;
  return {
    x: mt * mt * ax + 2 * mt * t * cpx + t * t * bx,
    y: mt * mt * ay + 2 * mt * t * cpy + t * t * by,
  };
}

// ── Canvas rendering ───────────────────────────────────────────────────────

function _draw(w: number, h: number): void {
  const ctx = _ctx!;
  ctx.save();
  ctx.clearRect(0, 0, w, h);

  // Apply camera
  ctx.translate(w / 2, h / 2);
  ctx.scale(_zoom, _zoom);
  ctx.translate(-w / 2 + _camX, -h / 2 + _camY);

  // ── 1. Category nebulae (soft ambient glow behind clusters) ──────────
  const catCenters = new Map<string, { x: number; y: number; count: number; spread: number }>();
  for (const n of _nodes) {
    const c = catCenters.get(n.category);
    if (c) {
      c.x += n.x;
      c.y += n.y;
      c.count++;
    } else {
      catCenters.set(n.category, { x: n.x, y: n.y, count: 1, spread: 0 });
    }
  }
  for (const c of catCenters.values()) {
    c.x /= c.count;
    c.y /= c.count;
  }
  // Compute spread (avg distance from center)
  for (const n of _nodes) {
    const c = catCenters.get(n.category)!;
    const dx = n.x - c.x;
    const dy = n.y - c.y;
    c.spread += Math.sqrt(dx * dx + dy * dy);
  }
  for (const c of catCenters.values()) {
    c.spread = c.count > 0 ? c.spread / c.count : 50;
  }

  // Draw nebulae
  for (const [cat, c] of catCenters) {
    const rgb = _colorToRgb(getCategoryColor(cat));
    const nebulaRadius = Math.max(c.spread * 1.5, 60);
    const grad = ctx.createRadialGradient(c.x, c.y, 0, c.x, c.y, nebulaRadius);
    grad.addColorStop(0, `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, 0.06)`);
    grad.addColorStop(0.6, `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, 0.02)`);
    grad.addColorStop(1, `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, 0)`);
    ctx.fillStyle = grad;
    ctx.beginPath();
    ctx.arc(c.x, c.y, nebulaRadius, 0, Math.PI * 2);
    ctx.fill();
  }

  // ── 2. Edges with curved lines + flowing particles ───────────────────
  for (const e of _edges) {
    const { source: s, target: t } = e;
    const { cpx, cpy } = _edgeControlPoint(s.x, s.y, t.x, t.y);
    const edgeColor = EDGE_TYPE_COLORS[e.type] ?? 'rgba(212, 101, 74, 0.3)';
    const isHighlight = _hoveredNode && (_hoveredNode === s || _hoveredNode === t);

    // Edge curve
    ctx.beginPath();
    ctx.moveTo(s.x, s.y);
    ctx.quadraticCurveTo(cpx, cpy, t.x, t.y);
    ctx.strokeStyle = isHighlight ? edgeColor.replace(/[\d.]+\)$/, '0.8)') : edgeColor;
    ctx.lineWidth = isHighlight ? 1.5 + e.weight * 2 : 0.6 + e.weight * 1.2;
    ctx.stroke();

    // Flowing particles along edge
    for (const p of e.particles) {
      const pos = _quadBezierAt(s.x, s.y, t.x, t.y, cpx, cpy, p.t);
      const particleAlpha = isHighlight ? 0.9 : 0.5 + e.weight * 0.3;
      const particleSize = isHighlight ? 2.5 : 1.5;

      // Particle glow
      const grd = ctx.createRadialGradient(pos.x, pos.y, 0, pos.x, pos.y, particleSize * 3);
      grd.addColorStop(0, edgeColor.replace(/[\d.]+\)$/, `${particleAlpha})`));
      grd.addColorStop(1, edgeColor.replace(/[\d.]+\)$/, '0)'));
      ctx.fillStyle = grd;
      ctx.beginPath();
      ctx.arc(pos.x, pos.y, particleSize * 3, 0, Math.PI * 2);
      ctx.fill();

      // Particle core
      ctx.beginPath();
      ctx.arc(pos.x, pos.y, particleSize, 0, Math.PI * 2);
      ctx.fillStyle = '#fff';
      ctx.globalAlpha = particleAlpha * 0.8;
      ctx.fill();
      ctx.globalAlpha = 1;
    }

    // Edge type label on hover
    if (isHighlight) {
      const midPos = _quadBezierAt(s.x, s.y, t.x, t.y, cpx, cpy, 0.5);
      ctx.font = '9px Figtree, system-ui, sans-serif';
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillStyle = 'rgba(255, 255, 255, 0.6)';
      ctx.fillText(e.type.replace(/_/g, ' '), midPos.x, midPos.y - 8);
    }
  }

  // ── 3. Category labels ───────────────────────────────────────────────
  if (!_miniMode) {
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    for (const [cat, c] of catCenters) {
      const rgb = _colorToRgb(getCategoryColor(cat));
      ctx.font = '600 10px Figtree, system-ui, sans-serif';
      ctx.fillStyle = `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, 0.35)`;
      ctx.letterSpacing = '0.08em';
      ctx.fillText(cat.toUpperCase(), c.x, c.y - c.spread * 0.6 - 12);
      // Count badge
      ctx.font = '9px Figtree, system-ui, sans-serif';
      ctx.fillStyle = `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, 0.25)`;
      ctx.fillText(`${c.count}`, c.x, c.y - c.spread * 0.6 - 1);
    }
  }

  // ── 4. Nodes — pulsing halos + cores ─────────────────────────────────
  for (const n of _nodes) {
    const color = getCategoryColor(n.category);
    const rgb = _colorToRgb(color);
    const isHovered = n === _hoveredNode;
    const isDragged = n === _dragNode;
    const isConnected =
      _hoveredNode &&
      _edges.some(
        (e) =>
          (e.source === _hoveredNode && e.target === n) ||
          (e.target === _hoveredNode && e.source === n),
      );
    const r = n.radius;

    // Pulse animation
    const pulse = 0.5 + 0.5 * Math.sin(_time * 0.02 + n.pulsePhase);
    const glowRadius = r + 6 + pulse * 4;

    // Outer glow halo
    const haloAlpha = isHovered ? 0.45 : isConnected ? 0.3 : 0.15 + pulse * 0.08;
    const halo = ctx.createRadialGradient(n.x, n.y, r * 0.3, n.x, n.y, glowRadius);
    halo.addColorStop(0, `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, ${haloAlpha})`);
    halo.addColorStop(1, `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, 0)`);
    ctx.fillStyle = halo;
    ctx.beginPath();
    ctx.arc(n.x, n.y, glowRadius, 0, Math.PI * 2);
    ctx.fill();

    // Node ring for hovered
    if (isHovered || isConnected) {
      ctx.beginPath();
      ctx.arc(n.x, n.y, r + 3, 0, Math.PI * 2);
      ctx.strokeStyle = `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, ${isHovered ? 0.6 : 0.3})`;
      ctx.lineWidth = 1;
      ctx.stroke();
    }

    // Core circle
    const coreAlpha = isDragged ? 1 : isHovered ? 0.95 : 0.85 + pulse * 0.1;
    ctx.beginPath();
    ctx.arc(n.x, n.y, isHovered ? r + 1.5 : r, 0, Math.PI * 2);
    const coreGrad = ctx.createRadialGradient(n.x - r * 0.3, n.y - r * 0.3, 0, n.x, n.y, r * 1.2);
    coreGrad.addColorStop(
      0,
      `rgba(${Math.min(rgb.r + 60, 255)}, ${Math.min(rgb.g + 40, 255)}, ${Math.min(rgb.b + 30, 255)}, ${coreAlpha})`,
    );
    coreGrad.addColorStop(1, `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, ${coreAlpha * 0.8})`);
    ctx.fillStyle = coreGrad;
    ctx.fill();

    // Subtle inner highlight
    ctx.beginPath();
    ctx.arc(n.x - r * 0.2, n.y - r * 0.2, r * 0.4, 0, Math.PI * 2);
    ctx.fillStyle = `rgba(255, 255, 255, ${isHovered ? 0.25 : 0.12})`;
    ctx.fill();

    // Importance ring (shown for importance >= 7)
    if (n.importance >= 7) {
      ctx.beginPath();
      ctx.arc(n.x, n.y, r + 1.5, 0, Math.PI * 2);
      ctx.strokeStyle = `rgba(255, 255, 255, ${0.15 + pulse * 0.1})`;
      ctx.lineWidth = 0.5;
      ctx.stroke();
    }
  }

  // ── 5. Hover card (on-canvas tooltip) ────────────────────────────────
  if (_hoveredNode && !_miniMode) {
    const n = _hoveredNode;
    const color = getCategoryColor(n.category);
    const rgb = _colorToRgb(color);
    const label = n.label.length > 50 ? `${n.label.slice(0, 47)}...` : n.label;

    ctx.font = '11px Figtree, system-ui, sans-serif';
    const metrics = ctx.measureText(label);
    const pad = 10;
    const cardW = Math.min(metrics.width + pad * 2, 280);
    const cardH = 44;
    const cardX = n.x - cardW / 2;
    const cardY = n.y - n.radius - cardH - 12;

    // Card shadow
    ctx.fillStyle = 'rgba(0, 0, 0, 0.4)';
    _roundRect(ctx, cardX + 2, cardY + 2, cardW, cardH, 5);
    ctx.fill();

    // Card background
    ctx.fillStyle = 'rgba(10, 10, 12, 0.92)';
    _roundRect(ctx, cardX, cardY, cardW, cardH, 5);
    ctx.fill();

    // Card accent border-left
    ctx.fillStyle = `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, 0.8)`;
    _roundRect(ctx, cardX, cardY, 3, cardH, 5);
    ctx.fill();

    // Category label
    ctx.font = '600 8px Figtree, system-ui, sans-serif';
    ctx.textAlign = 'left';
    ctx.textBaseline = 'top';
    ctx.fillStyle = `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, 0.9)`;
    ctx.fillText(n.category.toUpperCase(), cardX + pad, cardY + 6);

    // Importance dots
    const dotY = cardY + 8;
    for (let i = 0; i < Math.min(n.importance, 10); i++) {
      ctx.beginPath();
      ctx.arc(cardX + cardW - pad - (10 - i) * 6, dotY, 2, 0, Math.PI * 2);
      ctx.fillStyle =
        i < n.importance ? `rgba(${rgb.r}, ${rgb.g}, ${rgb.b}, 0.7)` : 'rgba(255,255,255,0.1)';
      ctx.fill();
    }

    // Content text
    ctx.font = '11px Figtree, system-ui, sans-serif';
    ctx.fillStyle = 'rgba(232, 224, 212, 0.9)';
    ctx.textBaseline = 'top';
    ctx.fillText(label, cardX + pad, cardY + 20, cardW - pad * 2);

    // Hint text
    ctx.font = '8px Figtree, system-ui, sans-serif';
    ctx.fillStyle = 'rgba(138, 132, 120, 0.7)';
    ctx.fillText('double-click to recall', cardX + pad, cardY + 34);

    // Connected edges count
    const edgeCount = _edges.filter((e) => e.source === n || e.target === n).length;
    if (edgeCount > 0) {
      ctx.textAlign = 'right';
      ctx.fillStyle = 'rgba(212, 101, 74, 0.7)';
      ctx.fillText(`${edgeCount} link${edgeCount > 1 ? 's' : ''}`, cardX + cardW - pad, cardY + 34);
    }
    ctx.textAlign = 'left';
  }

  // Dim non-connected nodes when hovering
  if (_hoveredNode) {
    const connectedIds = new Set<string>();
    connectedIds.add(_hoveredNode.id);
    for (const e of _edges) {
      if (e.source === _hoveredNode) connectedIds.add(e.target.id);
      if (e.target === _hoveredNode) connectedIds.add(e.source.id);
    }
    // Overdraw dimming on non-connected nodes
    for (const n of _nodes) {
      if (!connectedIds.has(n.id)) {
        ctx.beginPath();
        ctx.arc(n.x, n.y, n.radius + 8, 0, Math.PI * 2);
        ctx.fillStyle = 'rgba(5, 5, 5, 0.5)';
        ctx.fill();
      }
    }
  }

  ctx.restore();

  // ── HUD ──────────────────────────────────────────────────────────────
  if (!_miniMode) {
    ctx.save();
    ctx.font = '600 9px Figtree, system-ui, sans-serif';
    ctx.textAlign = 'left';
    ctx.textBaseline = 'bottom';
    ctx.fillStyle = 'rgba(138, 132, 120, 0.5)';
    ctx.fillText(
      `${_nodes.length} MEMORIES  \u00B7  ${_edges.length} LINKS  \u00B7  ${Math.round(_zoom * 100)}%`,
      10,
      h - 10,
    );

    // Zoom controls hint
    ctx.textAlign = 'right';
    ctx.fillText('scroll to zoom \u00B7 drag to pan \u00B7 double-click to recall', w - 10, h - 10);
    ctx.restore();
  }
}

// ── Rounded rect helper ────────────────────────────────────────────────────

function _roundRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  r: number,
): void {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.lineTo(x + w - r, y);
  ctx.arcTo(x + w, y, x + w, y + r, r);
  ctx.lineTo(x + w, y + h - r);
  ctx.arcTo(x + w, y + h, x + w - r, y + h, r);
  ctx.lineTo(x + r, y + h);
  ctx.arcTo(x, y + h, x, y + h - r, r);
  ctx.lineTo(x, y + r);
  ctx.arcTo(x, y, x + r, y, r);
  ctx.closePath();
}

// ── Event handling ─────────────────────────────────────────────────────────

let _eventsBound = false;

function _bindEvents(canvas: HTMLCanvasElement): void {
  if (_eventsBound) return;
  _eventsBound = true;

  canvas.addEventListener('mousemove', _onMouseMove);
  canvas.addEventListener('mousedown', _onMouseDown);
  canvas.addEventListener('mouseup', _onMouseUp);
  canvas.addEventListener('mouseleave', _onMouseLeave);
  canvas.addEventListener('wheel', _onWheel, { passive: false });
  canvas.addEventListener('dblclick', _onDoubleClick);
  canvas.style.cursor = 'grab';
}

function _screenToWorld(sx: number, sy: number): { x: number; y: number } {
  if (!_canvas) return { x: sx, y: sy };
  const w = _canvas.width / (window.devicePixelRatio || 1);
  const h = _canvas.height / (window.devicePixelRatio || 1);
  return {
    x: (sx - w / 2) / _zoom + w / 2 - _camX,
    y: (sy - h / 2) / _zoom + h / 2 - _camY,
  };
}

function _nodeAt(sx: number, sy: number): GraphNode | null {
  const { x, y } = _screenToWorld(sx, sy);
  for (let i = _nodes.length - 1; i >= 0; i--) {
    const n = _nodes[i];
    const dx = x - n.x;
    const dy = y - n.y;
    if (dx * dx + dy * dy <= (n.radius + 6) * (n.radius + 6)) return n;
  }
  return null;
}

function _canvasPos(e: MouseEvent): { x: number; y: number } {
  const rect = _canvas!.getBoundingClientRect();
  return { x: e.clientX - rect.left, y: e.clientY - rect.top };
}

function _onMouseMove(e: MouseEvent): void {
  const pos = _canvasPos(e);

  if (_dragNode) {
    const world = _screenToWorld(pos.x, pos.y);
    _dragNode.x = world.x;
    _dragNode.y = world.y;
    _dragNode.vx = 0;
    _dragNode.vy = 0;
    _alpha = Math.max(_alpha, 0.05);
    return;
  }

  if (_isPanning) {
    const dx = e.movementX / _zoom;
    const dy = e.movementY / _zoom;
    _camX += dx;
    _camY += dy;
    return;
  }

  const node = _nodeAt(pos.x, pos.y);
  _hoveredNode = node;
  _canvas!.style.cursor = node ? 'pointer' : 'grab';

  // External tooltip (for edge info outside canvas)
  if (_tooltip) _tooltip.style.display = 'none';
}

function _onMouseDown(e: MouseEvent): void {
  const pos = _canvasPos(e);
  const node = _nodeAt(pos.x, pos.y);

  if (node) {
    _dragNode = node;
    node.pinned = true;
    _canvas!.style.cursor = 'grabbing';
  } else {
    _isPanning = true;
    _canvas!.style.cursor = 'grabbing';
  }
}

function _onMouseUp(): void {
  if (_dragNode) {
    _dragNode.pinned = false;
    _dragNode = null;
  }
  _isPanning = false;
  _canvas!.style.cursor = _hoveredNode ? 'pointer' : 'grab';
}

function _onMouseLeave(): void {
  _dragNode = null;
  _isPanning = false;
  _hoveredNode = null;
  if (_tooltip) _tooltip.style.display = 'none';
}

function _onWheel(e: WheelEvent): void {
  e.preventDefault();
  const factor = e.deltaY > 0 ? 0.92 : 1.08;
  _zoom = Math.max(0.15, Math.min(6, _zoom * factor));
}

function _onDoubleClick(e: MouseEvent): void {
  const pos = _canvasPos(e);
  const node = _nodeAt(pos.x, pos.y);
  if (node?.id) {
    import('./molecules').then((m) => m.palaceRecallById(node.id));
  }
}
