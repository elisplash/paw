// ─────────────────────────────────────────────────────────────────────────────
// Canvas Render — SVG node, port, and edge rendering
// Pure rendering functions — no state mutation, no event handlers.
// ─────────────────────────────────────────────────────────────────────────────

import {
  type FlowNode,
  type FlowEdge,
  NODE_DEFAULTS,
  PORT_RADIUS,
  getOutputPort,
  getInputPort,
  buildEdgePath,
} from './atoms';
import {
  getDebugBreakpoints,
  getDebugCursorNodeId,
  getDebugEdgeValues,
  getSelectedEdgeIdLocal,
} from './molecule-state';
import { cs, svgEl, truncate } from './canvas-state';

// ── Node Rendering ─────────────────────────────────────────────────────────

export function renderNode(node: FlowNode, selected: boolean): SVGGElement {
  const g = svgEl('g') as SVGGElement;
  const isNew = cs.newNodeIds.has(node.id);
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
      setTimeout(() => cs.newNodeIds.delete(node.id), 600);
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

// ── Port Rendering ─────────────────────────────────────────────────────────

export function renderPorts(node: FlowNode): void {
  if (!cs.portsGroup) return;

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
    cs.portsGroup.appendChild(circle);
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
    cs.portsGroup.appendChild(circle);
  }
}

// ── Edge Rendering ─────────────────────────────────────────────────────────

export function renderEdge(edge: FlowEdge, fromNode: FlowNode, toNode: FlowNode): SVGGElement {
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
  path.setAttribute('stroke-width', isSelected ? '3' : edge.active ? '2.5' : '1.5');

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
