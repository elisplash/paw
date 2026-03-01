// ─────────────────────────────────────────────────────────────────────────────
// Flow Visualization Engine — Animation Molecules (Phase 5.4)
// Animated execution flow: particles/pulses along edges during execution.
// CSS offset-path driven, with conductor-aware parallel group animation.
// ─────────────────────────────────────────────────────────────────────────────

import { type FlowEdge, type FlowNode, getOutputPort, getInputPort, buildEdgePath } from './atoms';

// ── Constants ──────────────────────────────────────────────────────────────

const PARTICLE_COUNT = 3;
const PARTICLE_RADIUS = 3;
const PARTICLE_DURATION_MS = 1200;
const PARTICLE_STAGGER_MS = 200;

// ── State ──────────────────────────────────────────────────────────────────

let _animationLayer: SVGGElement | null = null;
const _activeAnimations = new Map<string, SVGElement[]>();
let _animFrameId: number | null = null;

// ── Public API ─────────────────────────────────────────────────────────────

/** Create the animation overlay layer. Call once at canvas mount. */
export function createAnimationLayer(svg: SVGSVGElement): SVGGElement {
  _animationLayer = document.createElementNS('http://www.w3.org/2000/svg', 'g') as SVGGElement;
  _animationLayer.setAttribute('class', 'flow-animation-layer');
  _animationLayer.setAttribute('pointer-events', 'none');
  svg.appendChild(_animationLayer);
  return _animationLayer;
}

/** Remove the animation layer. Call at canvas unmount. */
export function destroyAnimationLayer(): void {
  stopAllAnimations();
  _animationLayer?.remove();
  _animationLayer = null;
}

/**
 * Animate particles along an edge to visualize data flow.
 * Call when a node starts executing to show data flowing from upstream.
 */
export function animateEdge(edge: FlowEdge, fromNode: FlowNode, toNode: FlowNode): void {
  if (!_animationLayer) return;

  // Don't duplicate
  if (_activeAnimations.has(edge.id)) return;

  const fromPt = getOutputPort(fromNode, edge.fromPort);
  const toPt = getInputPort(toNode, edge.toPort);
  const pathD = buildEdgePath(fromPt, toPt);

  const particles: SVGElement[] = [];

  for (let i = 0; i < PARTICLE_COUNT; i++) {
    const particle = document.createElementNS('http://www.w3.org/2000/svg', 'circle');
    particle.setAttribute('r', String(PARTICLE_RADIUS));
    particle.setAttribute('class', 'flow-anim-particle');
    particle.setAttribute('fill', edgeAnimColor(edge.kind));
    particle.setAttribute('opacity', '0');

    // Create a hidden path for CSS offset-path (or manual animation)
    const pathEl = document.createElementNS('http://www.w3.org/2000/svg', 'path');
    pathEl.setAttribute('d', pathD);
    pathEl.setAttribute('class', 'flow-anim-guide');
    pathEl.setAttribute('fill', 'none');
    pathEl.setAttribute('stroke', 'none');
    pathEl.id = `flow-anim-path-${edge.id}-${i}`;
    _animationLayer!.appendChild(pathEl);

    // Animate along the path using getTotalLength + getPointAtLength
    const totalLen = pathEl.getTotalLength();
    const delay = i * PARTICLE_STAGGER_MS;
    const startTime = performance.now() + delay;

    particle.dataset.startTime = String(startTime);
    particle.dataset.pathId = pathEl.id;
    particle.dataset.totalLen = String(totalLen);

    _animationLayer!.appendChild(particle);
    particles.push(particle, pathEl);
  }

  _activeAnimations.set(edge.id, particles);

  // Start animation loop if not running
  if (!_animFrameId) {
    _animFrameId = requestAnimationFrame(animationTick);
  }
}

/** Stop animation on a specific edge. */
export function stopEdgeAnimation(edgeId: string): void {
  const els = _activeAnimations.get(edgeId);
  if (els) {
    for (const el of els) el.remove();
    _activeAnimations.delete(edgeId);
  }
  if (_activeAnimations.size === 0 && _animFrameId) {
    cancelAnimationFrame(_animFrameId);
    _animFrameId = null;
  }
}

/** Stop all running animations. */
export function stopAllAnimations(): void {
  for (const [, els] of _activeAnimations) {
    for (const el of els) el.remove();
  }
  _activeAnimations.clear();
  if (_animFrameId) {
    cancelAnimationFrame(_animFrameId);
    _animFrameId = null;
  }
}

/**
 * Bulk-animate edges for a parallel execution group.
 * All edges in the group pulse simultaneously.
 */
export function animateParallelGroup(edges: FlowEdge[], nodeMap: Map<string, FlowNode>): void {
  for (const edge of edges) {
    const from = nodeMap.get(edge.from);
    const to = nodeMap.get(edge.to);
    if (from && to) animateEdge(edge, from, to);
  }
}

// ── Animation Loop ─────────────────────────────────────────────────────────

function animationTick(now: number): void {
  if (_activeAnimations.size === 0) {
    _animFrameId = null;
    return;
  }

  for (const [edgeId, els] of _activeAnimations) {
    const particles = els.filter((el) => el.tagName === 'circle');
    const paths = els.filter((el) => el.tagName === 'path');

    let allDone = true;

    for (let i = 0; i < particles.length; i++) {
      const particle = particles[i];
      const pathEl = paths[i] as SVGPathElement | undefined;
      if (!pathEl) continue;

      const startTime = Number(particle.dataset.startTime ?? 0);
      const totalLen = Number(particle.dataset.totalLen ?? 100);

      if (now < startTime) {
        allDone = false;
        continue;
      }

      const elapsed = now - startTime;
      const progress = (elapsed % PARTICLE_DURATION_MS) / PARTICLE_DURATION_MS;
      const point = pathEl.getPointAtLength(progress * totalLen);

      particle.setAttribute('cx', String(point.x));
      particle.setAttribute('cy', String(point.y));

      // Fade in/out at ends
      const fade = progress < 0.1 ? progress / 0.1 : progress > 0.9 ? (1 - progress) / 0.1 : 1;
      particle.setAttribute('opacity', String(fade * 0.8));

      // Auto-stop after 3 cycles
      if (elapsed > PARTICLE_DURATION_MS * 3) {
        stopEdgeAnimation(edgeId);
        break;
      }
      allDone = false;
    }

    if (allDone) {
      stopEdgeAnimation(edgeId);
    }
  }

  _animFrameId = requestAnimationFrame(animationTick);
}

// ── Helpers ────────────────────────────────────────────────────────────────

function edgeAnimColor(kind: string): string {
  switch (kind) {
    case 'forward':
      return 'var(--accent, #5E9EFF)';
    case 'error':
      return 'var(--kinetic-red, #FF4D4D)';
    case 'reverse':
      return 'var(--status-info, #4DC9F6)';
    case 'bidirectional':
      return 'var(--kinetic-gold, #D4A853)';
    default:
      return 'var(--accent, #5E9EFF)';
  }
}
