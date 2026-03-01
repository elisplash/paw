// ─────────────────────────────────────────────────────────────────────────────
// Flow Visualization Engine — Alignment Guides Molecules (Phase 5.7)
// Figma-style smart guides: horizontal/vertical alignment lines,
// snap-to-alignment, equal spacing indicators.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowNode } from './atoms';

// ── Constants ──────────────────────────────────────────────────────────────

const SNAP_THRESHOLD = 5; // px to trigger alignment snap
const SPACING_THRESHOLD = 10; // px tolerance for equal-spacing detection

// ── Types ──────────────────────────────────────────────────────────────────

export interface AlignmentGuide {
  axis: 'horizontal' | 'vertical';
  position: number; // x for vertical, y for horizontal
  from: number; // start of the guide line
  to: number; // end of the guide line
  type: 'center' | 'edge' | 'spacing';
}

export interface SnapResult {
  /** Snapped X position (null if no snap on this axis) */
  x: number | null;
  /** Snapped Y position (null if no snap on this axis) */
  y: number | null;
  /** Active guides to render */
  guides: AlignmentGuide[];
}

// ── Public API ─────────────────────────────────────────────────────────────

/**
 * Calculate snap positions and alignment guides for a node being dragged.
 * Compares against all other nodes to find alignment opportunities.
 */
export function computeAlignmentSnap(
  draggedNode: FlowNode,
  dragX: number,
  dragY: number,
  allNodes: FlowNode[],
): SnapResult {
  const guides: AlignmentGuide[] = [];
  let snapX: number | null = null;
  let snapY: number | null = null;

  const dragCenterX = dragX + draggedNode.width / 2;
  const dragCenterY = dragY + draggedNode.height / 2;
  const dragLeft = dragX;
  const dragRight = dragX + draggedNode.width;
  const dragTop = dragY;
  const dragBottom = dragY + draggedNode.height;

  const others = allNodes.filter((n) => n.id !== draggedNode.id);

  for (const other of others) {
    const oCenterX = other.x + other.width / 2;
    const oCenterY = other.y + other.height / 2;
    const oLeft = other.x;
    const oRight = other.x + other.width;
    const oTop = other.y;
    const oBottom = other.y + other.height;

    // ── Vertical alignment (x-axis snap) ───────

    // Center-to-center vertical
    if (Math.abs(dragCenterX - oCenterX) < SNAP_THRESHOLD) {
      snapX = oCenterX - draggedNode.width / 2;
      guides.push({
        axis: 'vertical',
        position: oCenterX,
        from: Math.min(dragTop, oTop) - 20,
        to: Math.max(dragBottom, oBottom) + 20,
        type: 'center',
      });
    }

    // Left-edge alignment
    if (Math.abs(dragLeft - oLeft) < SNAP_THRESHOLD) {
      snapX = oLeft;
      guides.push({
        axis: 'vertical',
        position: oLeft,
        from: Math.min(dragTop, oTop) - 10,
        to: Math.max(dragBottom, oBottom) + 10,
        type: 'edge',
      });
    }

    // Right-edge alignment
    if (Math.abs(dragRight - oRight) < SNAP_THRESHOLD) {
      snapX = oRight - draggedNode.width;
      guides.push({
        axis: 'vertical',
        position: oRight,
        from: Math.min(dragTop, oTop) - 10,
        to: Math.max(dragBottom, oBottom) + 10,
        type: 'edge',
      });
    }

    // ── Horizontal alignment (y-axis snap) ──────

    // Center-to-center horizontal
    if (Math.abs(dragCenterY - oCenterY) < SNAP_THRESHOLD) {
      snapY = oCenterY - draggedNode.height / 2;
      guides.push({
        axis: 'horizontal',
        position: oCenterY,
        from: Math.min(dragLeft, oLeft) - 20,
        to: Math.max(dragRight, oRight) + 20,
        type: 'center',
      });
    }

    // Top-edge alignment
    if (Math.abs(dragTop - oTop) < SNAP_THRESHOLD) {
      snapY = oTop;
      guides.push({
        axis: 'horizontal',
        position: oTop,
        from: Math.min(dragLeft, oLeft) - 10,
        to: Math.max(dragRight, oRight) + 10,
        type: 'edge',
      });
    }

    // Bottom-edge alignment
    if (Math.abs(dragBottom - oBottom) < SNAP_THRESHOLD) {
      snapY = oBottom - draggedNode.height;
      guides.push({
        axis: 'horizontal',
        position: oBottom,
        from: Math.min(dragLeft, oLeft) - 10,
        to: Math.max(dragRight, oRight) + 10,
        type: 'edge',
      });
    }
  }

  // ── Equal spacing detection ───────────────────

  const spacingGuides = detectEqualSpacing(draggedNode, dragX, dragY, others);
  guides.push(...spacingGuides);

  return { x: snapX, y: snapY, guides: deduplicateGuides(guides) };
}

/**
 * Render alignment guide SVG elements into a group.
 */
export function renderAlignmentGuides(
  guides: AlignmentGuide[],
  svgEl: (tag: string) => SVGElement,
): SVGGElement {
  const g = svgEl('g') as SVGGElement;
  g.setAttribute('class', 'flow-alignment-guides');
  g.setAttribute('pointer-events', 'none');

  for (const guide of guides) {
    const line = svgEl('line');
    line.setAttribute('class', `flow-guide flow-guide-${guide.type}`);

    if (guide.axis === 'vertical') {
      line.setAttribute('x1', String(guide.position));
      line.setAttribute('y1', String(guide.from));
      line.setAttribute('x2', String(guide.position));
      line.setAttribute('y2', String(guide.to));
    } else {
      line.setAttribute('x1', String(guide.from));
      line.setAttribute('y1', String(guide.position));
      line.setAttribute('x2', String(guide.to));
      line.setAttribute('y2', String(guide.position));
    }

    const color =
      guide.type === 'spacing' ? 'var(--kinetic-gold, #D4A853)' : 'var(--accent, #5E9EFF)';
    const dash = guide.type === 'center' ? 'none' : '4 2';

    line.setAttribute('stroke', color);
    line.setAttribute('stroke-width', '0.5');
    if (dash !== 'none') line.setAttribute('stroke-dasharray', dash);
    line.setAttribute('opacity', '0.7');

    g.appendChild(line);
  }

  return g;
}

// ── Helpers ────────────────────────────────────────────────────────────────

function detectEqualSpacing(
  draggedNode: FlowNode,
  dragX: number,
  dragY: number,
  others: FlowNode[],
): AlignmentGuide[] {
  const guides: AlignmentGuide[] = [];
  if (others.length < 2) return guides;

  // Sort others by X for horizontal spacing
  const sortedX = [...others].sort((a, b) => a.x - b.x);
  for (let i = 0; i < sortedX.length - 1; i++) {
    const gap = sortedX[i + 1].x - (sortedX[i].x + sortedX[i].width);
    const dragGapLeft = dragX - (sortedX[i].x + sortedX[i].width);
    const dragGapRight = sortedX[i + 1].x - (dragX + draggedNode.width);

    if (Math.abs(dragGapLeft - gap) < SPACING_THRESHOLD) {
      const midY = Math.min(dragY, sortedX[i].y, sortedX[i + 1].y);
      const maxY = Math.max(
        dragY + draggedNode.height,
        sortedX[i].y + sortedX[i].height,
        sortedX[i + 1].y + sortedX[i + 1].height,
      );
      guides.push({
        axis: 'horizontal',
        position: (midY + maxY) / 2,
        from: sortedX[i].x + sortedX[i].width,
        to: dragX,
        type: 'spacing',
      });
    }

    if (Math.abs(dragGapRight - gap) < SPACING_THRESHOLD) {
      const midY = Math.min(dragY, sortedX[i].y, sortedX[i + 1].y);
      const maxY = Math.max(
        dragY + draggedNode.height,
        sortedX[i].y + sortedX[i].height,
        sortedX[i + 1].y + sortedX[i + 1].height,
      );
      guides.push({
        axis: 'horizontal',
        position: (midY + maxY) / 2,
        from: dragX + draggedNode.width,
        to: sortedX[i + 1].x,
        type: 'spacing',
      });
    }
  }

  return guides;
}

function deduplicateGuides(guides: AlignmentGuide[]): AlignmentGuide[] {
  const seen = new Set<string>();
  return guides.filter((g) => {
    const key = `${g.axis}-${g.position.toFixed(0)}-${g.type}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}
