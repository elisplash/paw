// ─────────────────────────────────────────────────────────────────────────────
// Flow Visualization Engine — Preview Molecules (Phase 5.2 + 5.3)
// Node data previews (execution output badges) and edge data type labels.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowNode } from './atoms';

// ── Constants ──────────────────────────────────────────────────────────────

const PREVIEW_MAX_CHARS = 40;
const PREVIEW_BADGE_HEIGHT = 16;

// ── Data Type Detection ────────────────────────────────────────────────────

export type DataShape = 'string' | 'number' | 'boolean' | 'json' | 'json[]' | 'error' | 'null';

/** Infer the data shape from a raw string value. */
export function inferDataShape(value: string): DataShape {
  if (!value || value.trim() === '') return 'null';

  const trimmed = value.trim();

  // Error detection
  if (trimmed.startsWith('Error:') || trimmed.startsWith('[error]')) return 'error';

  // Try JSON parse
  try {
    const parsed = JSON.parse(trimmed);
    if (Array.isArray(parsed)) return 'json[]';
    if (parsed === null) return 'null';
    if (typeof parsed === 'number') return 'number';
    if (typeof parsed === 'boolean') return 'boolean';
    if (typeof parsed === 'object') return 'json';
    if (typeof parsed === 'string') return 'string';
  } catch {
    // Not JSON
  }

  // Number check
  const num = Number(trimmed);
  if (!isNaN(num) && trimmed !== '') return 'number';

  // Boolean check
  if (trimmed === 'true' || trimmed === 'false') return 'boolean';

  return 'string';
}

/** Get the color for a data shape. */
export function dataShapeColor(shape: DataShape): string {
  switch (shape) {
    case 'string':
      return 'var(--accent, #5E9EFF)';
    case 'number':
      return 'var(--kinetic-sage, #5BA08C)';
    case 'boolean':
      return 'var(--kinetic-gold, #D4A853)';
    case 'json':
      return 'var(--kinetic-purple, #A855F7)';
    case 'json[]':
      return 'var(--kinetic-purple, #A855F7)';
    case 'error':
      return 'var(--kinetic-red, #FF4D4D)';
    case 'null':
      return 'var(--text-muted, #666)';
  }
}

// ── Node Preview Badge ─────────────────────────────────────────────────────

/** Build the preview text for a node's last output. */
export function buildPreviewText(output: string): string {
  if (!output) return '';
  const clean = output.replace(/\n/g, ' ').trim();
  if (clean.length <= PREVIEW_MAX_CHARS) return clean;
  return `${clean.slice(0, PREVIEW_MAX_CHARS - 1)}…`;
}

/**
 * Create SVG elements for a node data preview badge.
 * Positioned below the node body.
 */
export function createNodePreviewBadge(
  node: FlowNode,
  output: string,
  svgEl: (tag: string) => SVGElement,
): SVGGElement | null {
  if (!output) return null;

  const shape = inferDataShape(output);
  const preview = buildPreviewText(output);
  const color = dataShapeColor(shape);

  const g = svgEl('g') as SVGGElement;
  g.setAttribute('class', 'flow-node-preview');
  g.setAttribute('transform', `translate(0, ${node.height + 4})`);

  // Badge background
  const bg = svgEl('rect');
  bg.setAttribute('x', '4');
  bg.setAttribute('y', '0');
  bg.setAttribute('width', String(node.width - 8));
  bg.setAttribute('height', String(PREVIEW_BADGE_HEIGHT));
  bg.setAttribute('rx', '3');
  bg.setAttribute('fill', 'var(--bg-tertiary, rgba(0,0,0,0.3))');
  bg.setAttribute('stroke', color);
  bg.setAttribute('stroke-width', '0.5');
  bg.setAttribute('opacity', '0.9');
  g.appendChild(bg);

  // Preview text
  const text = svgEl('text');
  text.setAttribute('x', '8');
  text.setAttribute('y', String(PREVIEW_BADGE_HEIGHT / 2 + 1));
  text.setAttribute('dominant-baseline', 'central');
  text.setAttribute('fill', color);
  text.setAttribute('font-size', '8');
  text.setAttribute('font-family', 'var(--font-mono, monospace)');
  text.textContent = preview;
  g.appendChild(text);

  return g;
}

// ── Edge Data Label ────────────────────────────────────────────────────────

/**
 * Create an SVG label showing the data shape flowing through an edge.
 * Positioned at the midpoint of the edge path.
 */
export function createEdgeDataLabel(
  shape: DataShape,
  midX: number,
  midY: number,
  svgEl: (tag: string) => SVGElement,
): SVGGElement {
  const color = dataShapeColor(shape);
  const label = shape.toUpperCase();

  const g = svgEl('g') as SVGGElement;
  g.setAttribute('class', 'flow-edge-data-label');

  const bg = svgEl('rect');
  bg.setAttribute('x', String(midX - 18));
  bg.setAttribute('y', String(midY - 6));
  bg.setAttribute('width', '36');
  bg.setAttribute('height', '12');
  bg.setAttribute('rx', '2');
  bg.setAttribute('fill', 'var(--bg-primary, #1a1a1a)');
  bg.setAttribute('stroke', color);
  bg.setAttribute('stroke-width', '0.5');
  bg.setAttribute('opacity', '0.85');
  g.appendChild(bg);

  const text = svgEl('text');
  text.setAttribute('x', String(midX));
  text.setAttribute('y', String(midY + 1));
  text.setAttribute('text-anchor', 'middle');
  text.setAttribute('dominant-baseline', 'central');
  text.setAttribute('fill', color);
  text.setAttribute('font-size', '7');
  text.setAttribute('font-weight', '600');
  text.textContent = label;
  g.appendChild(text);

  return g;
}
