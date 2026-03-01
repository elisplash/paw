// ─────────────────────────────────────────────────────────────────────────────
// Flow Visualization Engine — Panel Molecules
// Node properties panel, edge panel, and debug inspector rendering.
// Config field generation delegated to panel-config-fields.ts.
// ─────────────────────────────────────────────────────────────────────────────

import { type FlowGraph, type FlowNode, NODE_DEFAULTS } from './atoms';
import {
  getMoleculesState,
  getSelectedEdgeIdLocal,
  setSelectedEdgeIdLocal,
  getDebugNodeStates,
  escAttr,
  formatDate,
} from './molecule-state';
import { renderGraph } from './canvas-molecules';
import { buildConfigFieldsHtml } from './panel-config-fields';

// ── Node Properties Panel ──────────────────────────────────────────────────

export function renderNodePanel(
  container: HTMLElement,
  node: FlowNode | null,
  onUpdate: (patch: Partial<FlowNode>) => void,
  activeGraph?: FlowGraph | null,
  onGraphUpdate?: (patch: Partial<FlowGraph>) => void,
) {
  const _state = getMoleculesState();
  const _selectedEdgeId = getSelectedEdgeIdLocal();

  if (!node) {
    // Show flow-level properties if a graph is active
    if (activeGraph && onGraphUpdate) {
      const folderVal = escAttr(activeGraph.folder ?? '');
      const descVal = escAttr(activeGraph.description ?? '');

      container.innerHTML = `
        <div class="flow-panel">
          <div class="flow-panel-header">
            <span class="ms" style="color: var(--kinetic-red, #FF4D4D)">account_tree</span>
            <div>
              <div class="flow-panel-kind">FLOW PROPERTIES</div>
            </div>
          </div>
          <label class="flow-panel-field">
            <span>Name</span>
            <input type="text" class="flow-panel-input" data-flow-field="name" value="${escAttr(activeGraph.name)}" />
          </label>
          <label class="flow-panel-field">
            <span>Description</span>
            <textarea class="flow-panel-textarea" data-flow-field="description" rows="2" placeholder="Describe this flow…">${descVal}</textarea>
          </label>
          <label class="flow-panel-field">
            <span>Folder</span>
            <input type="text" class="flow-panel-input" data-flow-field="folder" value="${folderVal}" placeholder="(root)" />
          </label>
          <div class="flow-panel-section">
            <div class="flow-panel-section-label">Stats</div>
            <div class="flow-panel-pos">
              <span>${activeGraph.nodes.length} integrations</span>
              <span>${activeGraph.edges.length} edges</span>
            </div>
          </div>
          <div class="flow-panel-section">
            <div class="flow-panel-pos">
              <span>Created: ${formatDate(activeGraph.createdAt)}</span>
            </div>
            <div class="flow-panel-pos" style="margin-top: 2px">
              <span>Updated: ${formatDate(activeGraph.updatedAt)}</span>
            </div>
          </div>
        </div>
      `;

      container.querySelectorAll('[data-flow-field]').forEach((el) => {
        el.addEventListener('change', () => {
          const field = (el as HTMLElement).dataset.flowField!;
          const value = (el as HTMLInputElement).value;
          onGraphUpdate({ [field]: value } as Partial<FlowGraph>);
        });
      });
      return;
    }

    // Edge panel when an edge is selected
    if (_selectedEdgeId && activeGraph) {
      const edge = activeGraph.edges.find((ee) => ee.id === _selectedEdgeId);
      if (edge && onGraphUpdate) {
        const edgeLabel = escAttr(edge.label ?? '');
        const fromNode = activeGraph.nodes.find((n) => n.id === edge.from);
        const toNode = activeGraph.nodes.find((n) => n.id === edge.to);
        container.innerHTML = `
          <div class="flow-panel">
            <div class="flow-panel-header">
              <span class="ms" style="color: var(--accent)">arrow_forward</span>
              <div>
                <div class="flow-panel-kind">EDGE</div>
                <div class="flow-panel-label">${fromNode?.label ?? edge.from} → ${toNode?.label ?? edge.to}</div>
              </div>
            </div>
            <label class="flow-panel-field">
              <span>Kind</span>
              <select class="flow-panel-input" data-edge-field="kind">
                <option value="forward"${edge.kind === 'forward' ? ' selected' : ''}>Forward</option>
                <option value="reverse"${edge.kind === 'reverse' ? ' selected' : ''}>Reverse</option>
                <option value="bidirectional"${edge.kind === 'bidirectional' ? ' selected' : ''}>Bidirectional</option>
                <option value="error"${edge.kind === 'error' ? ' selected' : ''}>Error</option>
              </select>
            </label>
            <label class="flow-panel-field">
              <span>Label</span>
              <input type="text" class="flow-panel-input" data-edge-field="label" value="${edgeLabel}" placeholder="Optional label…" />
            </label>
            <label class="flow-panel-field">
              <span>Condition</span>
              <input type="text" class="flow-panel-input" data-edge-field="conditionExpr" value="${escAttr(((edge as unknown as Record<string, unknown>).conditionExpr as string) ?? '')}" placeholder="Expression for conditional routing" />
            </label>
            <div class="flow-panel-section" style="margin-top: 12px">
              <button class="flow-btn flow-btn-danger" data-edge-action="delete" style="width:100%">
                <span class="ms" style="font-size:14px">delete</span> Delete Edge
              </button>
            </div>
          </div>
        `;

        container.querySelectorAll('[data-edge-field]').forEach((el) => {
          el.addEventListener('change', () => {
            const field = (el as HTMLElement).dataset.edgeField!;
            const value = (el as HTMLInputElement).value;
            (edge as unknown as Record<string, unknown>)[field] = value;
            activeGraph.updatedAt = new Date().toISOString();
            onGraphUpdate({} as Partial<FlowGraph>);
            renderGraph();
          });
        });

        const deleteBtn = container.querySelector('[data-edge-action="delete"]');
        deleteBtn?.addEventListener('click', () => {
          activeGraph.edges = activeGraph.edges.filter((ee) => ee.id !== _selectedEdgeId);
          setSelectedEdgeIdLocal(null);
          _state?.setSelectedEdgeId(null);
          activeGraph.updatedAt = new Date().toISOString();
          onGraphUpdate({} as Partial<FlowGraph>);
          renderGraph();
          renderNodePanel(container, null, onUpdate, activeGraph, onGraphUpdate);
        });
        return;
      }
    }

    container.innerHTML =
      '<div class="flow-panel-empty"><span class="ms">touch_app</span><p>Select a node or edge to edit</p></div>';
    return;
  }

  const defaults = NODE_DEFAULTS[node.kind];
  const configFieldsHtml = buildConfigFieldsHtml(node);

  // Debug output inspector
  const debugState = getDebugNodeStates().get(node.id);
  const debugHtml = debugState
    ? `
    <div class="flow-panel-divider"></div>
    <div class="flow-panel-section">
      <span class="flow-panel-section-label">Debug Inspector</span>
      <div class="flow-panel-debug-status">
        <span class="flow-debug-badge flow-debug-badge-${debugState.status}">${debugState.status.toUpperCase()}</span>
      </div>
      ${
        debugState.input
          ? `
        <div class="flow-panel-debug-block">
          <span class="flow-panel-debug-label">Input</span>
          <pre class="flow-panel-debug-pre">${escAttr(debugState.input)}</pre>
        </div>
      `
          : ''
      }
      ${
        debugState.output
          ? `
        <div class="flow-panel-debug-block">
          <span class="flow-panel-debug-label">Output</span>
          <pre class="flow-panel-debug-pre">${escAttr(debugState.output)}</pre>
        </div>
      `
          : ''
      }
    </div>
  `
    : '';

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
      ${configFieldsHtml ? `<div class="flow-panel-divider"></div><div class="flow-panel-section"><span class="flow-panel-section-label">Execution Config</span></div>${configFieldsHtml}` : ''}
      <div class="flow-panel-divider"></div>
      <div class="flow-panel-section">
        <span class="flow-panel-section-label">Info</span>
        <div class="flow-panel-pos">
          <span>Status: <strong>${node.status}</strong></span>
          <span>x: ${node.x}  y: ${node.y}</span>
          <span>${node.width}×${node.height}</span>
        </div>
      </div>
      ${debugHtml}
    </div>
  `;

  // Bind direct node fields
  container.querySelectorAll('[data-field]').forEach((el) => {
    el.addEventListener('change', () => {
      const field = (el as HTMLElement).dataset.field!;
      const value = (el as HTMLInputElement).value;
      onUpdate({ [field]: value } as Partial<FlowNode>);
    });
  });

  // Bind config fields
  container.querySelectorAll('[data-config]').forEach((el) => {
    el.addEventListener('change', () => {
      const key = (el as HTMLElement).dataset.config!;
      let value: unknown = (el as HTMLInputElement).value;
      if (key === 'timeoutMs') value = Number(value) * 1000;
      if ((el as HTMLInputElement).type === 'checkbox') value = (el as HTMLInputElement).checked;
      const newConfig = { ...node.config, [key]: value };
      onUpdate({ config: newConfig });
    });
  });

  // Schedule preset dropdown
  const presetSelect = container.querySelector(
    '[data-schedule-preset]',
  ) as HTMLSelectElement | null;
  if (presetSelect) {
    presetSelect.addEventListener('change', () => {
      const val = presetSelect.value;
      if (!val) return;
      const cronInput = container.querySelector(
        '[data-config="schedule"]',
      ) as HTMLInputElement | null;
      if (cronInput) {
        cronInput.value = val;
        cronInput.dispatchEvent(new Event('change'));
      }
    });
  }

  // Error target checkboxes
  container.querySelectorAll('[data-error-target]').forEach((el) => {
    el.addEventListener('change', () => {
      const targets: string[] = [];
      container.querySelectorAll('[data-error-target]').forEach((cb) => {
        if ((cb as HTMLInputElement).checked) {
          targets.push((cb as HTMLElement).dataset.errorTarget!);
        }
      });
      const newConfig = { ...node.config, errorTargets: targets };
      onUpdate({ config: newConfig });
    });
  });
}
