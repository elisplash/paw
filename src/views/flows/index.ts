// ─────────────────────────────────────────────────────────────────────────────
// Flow Visualization Engine — Index (Orchestrator)
// Owns module state, wires state bridge, handles persistence, exports public API.
// ─────────────────────────────────────────────────────────────────────────────

import {
  type FlowGraph,
  createGraph,
  createNode as createNodeFn,
  serializeGraph,
  deserializeGraph,
} from './atoms';
import {
  setMoleculesState,
  mountCanvas,
  unmountCanvas,
  renderGraph,
  renderToolbar,
  renderFlowList,
  renderNodePanel,
} from './molecules';
import { parseFlowText } from './parser';

// ── Module State ───────────────────────────────────────────────────────────

let _graphs: FlowGraph[] = [];
let _activeGraphId: string | null = null;
let _selectedNodeId: string | null = null;
let _mounted = false;

const STORAGE_KEY = 'openpawz-flows';

// ── State Bridge ───────────────────────────────────────────────────────────

function initStateBridge() {
  setMoleculesState({
    getGraph: () => _graphs.find((g) => g.id === _activeGraphId) ?? null,
    setGraph: (g: FlowGraph) => {
      const idx = _graphs.findIndex((gg) => gg.id === g.id);
      if (idx >= 0) _graphs[idx] = g;
      else _graphs.push(g);
      _activeGraphId = g.id;
    },
    getSelectedNodeId: () => _selectedNodeId,
    setSelectedNodeId: (id: string | null) => {
      _selectedNodeId = id;
      updateNodePanel();
    },
    onGraphChanged: () => {
      const g = _graphs.find((gg) => gg.id === _activeGraphId);
      if (g) g.updatedAt = new Date().toISOString();
      persist();
      updateFlowList();
    },
  });
}

// ── Persistence (localStorage) ─────────────────────────────────────────────

function persist() {
  try {
    const data = _graphs.map(serializeGraph);
    localStorage.setItem(STORAGE_KEY, JSON.stringify(data));
  } catch (e) {
    console.error('[flows] Persist failed:', e);
  }
}

function restore() {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return;
    const arr = JSON.parse(raw) as string[];
    _graphs = arr.map((s) => deserializeGraph(s)).filter(Boolean) as FlowGraph[];
  } catch (e) {
    console.error('[flows] Restore failed:', e);
    _graphs = [];
  }
}

// ── DOM References ─────────────────────────────────────────────────────────

function el(id: string): HTMLElement | null {
  return document.getElementById(id);
}

// ── Public API ─────────────────────────────────────────────────────────────

/**
 * Called when the Flows view is activated (from router.ts switchView).
 */
export function loadFlows() {
  initStateBridge();
  restore();

  if (!_mounted) {
    mount();
    _mounted = true;
  }

  // If no graphs exist, show empty state
  if (_graphs.length && !_activeGraphId) {
    _activeGraphId = _graphs[0].id;
  }

  updateFlowList();
  renderActiveGraph();
}

/**
 * Create a new flow from text (called from /flow slash command).
 * Returns the created graph.
 */
export function createFlowFromText(text: string, name?: string): FlowGraph {
  initStateBridge();
  restore();

  const result = parseFlowText(text, name);
  _graphs.push(result.graph);
  _activeGraphId = result.graph.id;
  persist();

  // If flows view is mounted, update it
  if (_mounted) {
    updateFlowList();
    renderActiveGraph();
  }

  return result.graph;
}

/**
 * Parse text and return the graph without persisting (preview).
 */
export function previewFlow(text: string, name?: string) {
  return parseFlowText(text, name);
}

/**
 * Get all stored flows.
 */
export function getFlows(): FlowGraph[] {
  restore();
  return [..._graphs];
}

/**
 * Programmatically set the active flow and render it.
 */
export function setActiveFlow(id: string) {
  _activeGraphId = id;
  _selectedNodeId = null;
  renderActiveGraph();
  updateFlowList();
}

// ── Mount ──────────────────────────────────────────────────────────────────

function mount() {
  const canvasContainer = el('flows-canvas');
  const toolbarContainer = el('flows-toolbar');
  const textInput = el('flows-text-input') as HTMLInputElement | null;

  if (canvasContainer) mountCanvas(canvasContainer);
  if (toolbarContainer) renderToolbar(toolbarContainer);

  // Text-to-flow input
  if (textInput) {
    textInput.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && textInput.value.trim()) {
        handleFlowTextInput(textInput.value);
        textInput.value = '';
      }
    });
  }

  // Wire custom events from molecules
  document.addEventListener('flow:add-node', ((e: CustomEvent) => {
    onAddNodeAtPosition(e.detail.x, e.detail.y);
  }) as EventListener);

  document.addEventListener('flow:edit-node', ((e: CustomEvent) => {
    _selectedNodeId = e.detail.nodeId;
    updateNodePanel();
    renderGraph();
  }) as EventListener);

  // Keyboard shortcuts
  document.addEventListener('keydown', onKeyDown);

  updateFlowList();
  updateNodePanel();
}

export function unmountFlows() {
  unmountCanvas();
  document.removeEventListener('keydown', onKeyDown);
  _mounted = false;
}

// ── Internal Actions ───────────────────────────────────────────────────────

function renderActiveGraph() {
  initStateBridge();
  renderGraph();
  updateNodePanel();
}

function updateFlowList() {
  const container = el('flows-list');
  if (!container) return;

  renderFlowList(
    container,
    _graphs,
    _activeGraphId,
    (id) => {
      _activeGraphId = id;
      _selectedNodeId = null;
      renderActiveGraph();
      updateFlowList();
    },
    (id) => {
      _graphs = _graphs.filter((g) => g.id !== id);
      if (_activeGraphId === id) {
        _activeGraphId = _graphs[0]?.id ?? null;
        _selectedNodeId = null;
      }
      persist();
      renderActiveGraph();
      updateFlowList();
    },
    () => {
      newFlow();
    },
  );
}

function updateNodePanel() {
  const container = el('flows-panel');
  if (!container) return;

  const graph = _graphs.find((g) => g.id === _activeGraphId);
  const node = graph?.nodes.find((n) => n.id === _selectedNodeId) ?? null;

  renderNodePanel(container, node, (patch) => {
    if (!graph || !node) return;
    Object.assign(node, patch);
    graph.updatedAt = new Date().toISOString();
    persist();
    renderGraph();
    updateNodePanel();
  });
}

function newFlow() {
  const graph = createGraph(`Flow ${_graphs.length + 1}`);
  _graphs.push(graph);
  _activeGraphId = graph.id;
  _selectedNodeId = null;
  persist();
  renderActiveGraph();
  updateFlowList();
}

function onAddNodeAtPosition(x: number, y: number) {
  const graph = _graphs.find((g) => g.id === _activeGraphId);
  if (!graph) {
    // Create a new graph first
    newFlow();
    return;
  }

  const node = createNodeFn('tool', `Step ${graph.nodes.length + 1}`, x, y);
  graph.nodes.push(node);
  _selectedNodeId = node.id;
  graph.updatedAt = new Date().toISOString();
  persist();
  renderGraph();
  updateFlowList();
  updateNodePanel();
}

function onKeyDown(e: KeyboardEvent) {
  // Only handle when flows view is active
  const flowsView = el('flows-view');
  if (!flowsView?.classList.contains('active')) return;

  const graph = _graphs.find((g) => g.id === _activeGraphId);
  if (!graph) return;

  switch (e.key) {
    case 'Delete':
    case 'Backspace':
      if (_selectedNodeId && !(e.target instanceof HTMLInputElement) && !(e.target instanceof HTMLTextAreaElement)) {
        graph.nodes = graph.nodes.filter((n) => n.id !== _selectedNodeId);
        graph.edges = graph.edges.filter((ee) => ee.from !== _selectedNodeId && ee.to !== _selectedNodeId);
        _selectedNodeId = null;
        graph.updatedAt = new Date().toISOString();
        persist();
        renderGraph();
        updateFlowList();
        updateNodePanel();
        e.preventDefault();
      }
      break;
    case 'Escape':
      _selectedNodeId = null;
      renderGraph();
      updateNodePanel();
      break;
    case 'a':
      if (e.ctrlKey || e.metaKey) {
        // Select all — noop for now
        e.preventDefault();
      }
      break;
  }
}

// ── Text Input (for the text-to-flow box in the UI) ────────────────────────

export function handleFlowTextInput(text: string) {
  if (!text.trim()) return;

  const result = parseFlowText(text);
  _graphs.push(result.graph);
  _activeGraphId = result.graph.id;
  _selectedNodeId = null;
  persist();
  renderActiveGraph();
  updateFlowList();
}
