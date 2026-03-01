// ─────────────────────────────────────────────────────────────────────────────
// Flow Visualization Engine — Strategy Overlay Molecules (Phase 5.8)
// Visualizes the Conductor Protocol's compiled execution strategy on the canvas.
// Shows collapsed groups, parallel phases, convergent meshes.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowNode } from './atoms';

// ── Types ──────────────────────────────────────────────────────────────────

/** A compiled strategy unit from the Conductor. */
export interface StrategyUnit {
  kind: 'collapsed' | 'parallel' | 'sequential' | 'convergent';
  nodeIds: string[];
  label: string;
  phaseIndex: number;
}

/** Full strategy visualization data. */
export interface StrategyOverlayData {
  units: StrategyUnit[];
  totalPhases: number;
  estimatedSaving: string; // e.g. "~60% fewer LLM calls"
}

// ── Colors ─────────────────────────────────────────────────────────────────

const UNIT_COLORS: Record<StrategyUnit['kind'], string> = {
  collapsed: 'var(--kinetic-sage, #5BA08C)',
  parallel: 'var(--accent, #5E9EFF)',
  sequential: 'var(--text-muted, #666)',
  convergent: 'var(--kinetic-gold, #D4A853)',
};

const UNIT_ICONS: Record<StrategyUnit['kind'], string> = {
  collapsed: 'compress',
  parallel: 'stacks',
  sequential: 'arrow_downward',
  convergent: 'cycle',
};

// ── Public API ─────────────────────────────────────────────────────────────

/**
 * Render the strategy overlay as SVG elements on the canvas.
 * Shows bounding boxes around grouped nodes with labels.
 */
export function renderStrategyOverlay(
  data: StrategyOverlayData,
  nodeMap: Map<string, FlowNode>,
  svgEl: (tag: string) => SVGElement,
): SVGGElement {
  const g = svgEl('g') as SVGGElement;
  g.setAttribute('class', 'flow-strategy-overlay');
  g.setAttribute('pointer-events', 'none');

  for (const unit of data.units) {
    const unitG = renderUnit(unit, nodeMap, svgEl);
    if (unitG) g.appendChild(unitG);
  }

  // Summary badge at top-left
  const summary = renderSummaryBadge(data, svgEl);
  g.appendChild(summary);

  return g;
}

/**
 * Build an HTML overlay for run mode selection.
 * Shows "Run as planned" vs "Run sequential (safe mode)".
 */
export function buildRunModeSelector(): string {
  return `
    <div class="flow-strategy-run-mode">
      <div class="flow-strategy-run-mode-title">
        <span class="ms" style="font-size:16px">auto_awesome</span>
        Conductor Strategy
      </div>
      <div class="flow-strategy-run-mode-options">
        <button class="flow-btn flow-btn-primary" data-run-mode="conductor">
          <span class="ms" style="font-size:14px">bolt</span>
          Run as planned
        </button>
        <button class="flow-btn" data-run-mode="sequential">
          <span class="ms" style="font-size:14px">format_list_numbered</span>
          Sequential (safe)
        </button>
      </div>
    </div>
  `;
}

// ── Unit Rendering ─────────────────────────────────────────────────────────

function renderUnit(
  unit: StrategyUnit,
  nodeMap: Map<string, FlowNode>,
  svgEl: (tag: string) => SVGElement,
): SVGGElement | null {
  // Calculate bounding box of all nodes in the unit
  const nodes = unit.nodeIds.map((id) => nodeMap.get(id)).filter((n): n is FlowNode => n != null);

  if (nodes.length === 0) return null;

  const bounds = computeBounds(nodes);
  const pad = 12;
  const color = UNIT_COLORS[unit.kind];
  const icon = UNIT_ICONS[unit.kind];

  const g = svgEl('g') as SVGGElement;
  g.setAttribute('class', `flow-strategy-unit flow-strategy-${unit.kind}`);

  // Bounding rectangle
  const rect = svgEl('rect');
  rect.setAttribute('x', String(bounds.x - pad));
  rect.setAttribute('y', String(bounds.y - pad - 18)); // Room for label
  rect.setAttribute('width', String(bounds.w + pad * 2));
  rect.setAttribute('height', String(bounds.h + pad * 2 + 18));
  rect.setAttribute('rx', '8');
  rect.setAttribute('fill', 'none');
  rect.setAttribute('stroke', color);
  rect.setAttribute('stroke-width', '1.5');
  rect.setAttribute('stroke-dasharray', unit.kind === 'convergent' ? '6 3' : 'none');
  rect.setAttribute('opacity', '0.6');
  g.appendChild(rect);

  // Phase label at top
  const labelBg = svgEl('rect');
  labelBg.setAttribute('x', String(bounds.x - pad));
  labelBg.setAttribute('y', String(bounds.y - pad - 18));
  labelBg.setAttribute('width', String(Math.min(bounds.w + pad * 2, 200)));
  labelBg.setAttribute('height', '16');
  labelBg.setAttribute('rx', '3');
  labelBg.setAttribute('fill', color);
  labelBg.setAttribute('opacity', '0.15');
  g.appendChild(labelBg);

  const labelText = svgEl('text');
  labelText.setAttribute('x', String(bounds.x - pad + 4));
  labelText.setAttribute('y', String(bounds.y - pad - 8));
  labelText.setAttribute('dominant-baseline', 'central');
  labelText.setAttribute('fill', color);
  labelText.setAttribute('font-size', '9');
  labelText.setAttribute('font-weight', '600');
  labelText.textContent = `${icon} Phase ${unit.phaseIndex + 1}: ${unit.label}`;
  g.appendChild(labelText);

  // Parallel indicator (‖) between nodes
  if (unit.kind === 'parallel' && nodes.length > 1) {
    const midX = bounds.x + bounds.w / 2;
    const midY = bounds.y - pad - 3;
    const indicator = svgEl('text');
    indicator.setAttribute('x', String(midX));
    indicator.setAttribute('y', String(midY));
    indicator.setAttribute('text-anchor', 'middle');
    indicator.setAttribute('fill', color);
    indicator.setAttribute('font-size', '14');
    indicator.setAttribute('font-weight', '700');
    indicator.textContent = '‖';
    g.appendChild(indicator);
  }

  // Convergent iteration cap badge
  if (unit.kind === 'convergent') {
    const badgeX = bounds.x + bounds.w + pad - 24;
    const badgeY = bounds.y - pad - 18;

    const capBg = svgEl('rect');
    capBg.setAttribute('x', String(badgeX));
    capBg.setAttribute('y', String(badgeY));
    capBg.setAttribute('width', '24');
    capBg.setAttribute('height', '14');
    capBg.setAttribute('rx', '3');
    capBg.setAttribute('fill', color);
    capBg.setAttribute('opacity', '0.3');
    g.appendChild(capBg);

    const capText = svgEl('text');
    capText.setAttribute('x', String(badgeX + 12));
    capText.setAttribute('y', String(badgeY + 8));
    capText.setAttribute('text-anchor', 'middle');
    capText.setAttribute('dominant-baseline', 'central');
    capText.setAttribute('fill', color);
    capText.setAttribute('font-size', '8');
    capText.setAttribute('font-weight', '600');
    capText.textContent = '×5';
    g.appendChild(capText);
  }

  return g;
}

// ── Summary Badge ──────────────────────────────────────────────────────────

function renderSummaryBadge(
  data: StrategyOverlayData,
  svgEl: (tag: string) => SVGElement,
): SVGGElement {
  const g = svgEl('g') as SVGGElement;
  g.setAttribute('class', 'flow-strategy-summary');
  g.setAttribute('transform', 'translate(10, 10)');

  const bg = svgEl('rect');
  bg.setAttribute('width', '200');
  bg.setAttribute('height', '32');
  bg.setAttribute('rx', '4');
  bg.setAttribute('fill', 'var(--bg-secondary, #2a2a2a)');
  bg.setAttribute('stroke', 'var(--border, #333)');
  bg.setAttribute('stroke-width', '1');
  bg.setAttribute('opacity', '0.95');
  g.appendChild(bg);

  const text = svgEl('text');
  text.setAttribute('x', '8');
  text.setAttribute('y', '17');
  text.setAttribute('dominant-baseline', 'central');
  text.setAttribute('fill', 'var(--text-secondary)');
  text.setAttribute('font-size', '10');
  text.textContent = `⚡ ${data.totalPhases} phases · ${data.estimatedSaving}`;
  g.appendChild(text);

  return g;
}

// ── Helpers ────────────────────────────────────────────────────────────────

function computeBounds(nodes: FlowNode[]): { x: number; y: number; w: number; h: number } {
  let minX = Infinity,
    minY = Infinity,
    maxX = -Infinity,
    maxY = -Infinity;
  for (const n of nodes) {
    if (n.x < minX) minX = n.x;
    if (n.y < minY) minY = n.y;
    if (n.x + n.width > maxX) maxX = n.x + n.width;
    if (n.y + n.height > maxY) maxY = n.y + n.height;
  }
  return { x: minX, y: minY, w: maxX - minX, h: maxY - minY };
}
