// ─────────────────────────────────────────────────────────────────────────────
// Flow Visualization Engine — Index (Orchestrator)
// Owns module state, wires state bridge, exports public API.
// Sub-modules: flows-persistence, flows-scheduler, flows-keybindings.
// ─────────────────────────────────────────────────────────────────────────────

import {
  type FlowGraph,
  type FlowNode,
  type FlowEdge,
  type FlowTemplate,
  createGraph,
  createNode as createNodeFn,
  serializeGraph,
  deserializeGraph,
  instantiateTemplate,
  type UndoStack,
  createUndoStack,
  pushUndo,
  undo,
  redo,
} from './atoms';
import {
  setMoleculesState,
  mountCanvas,
  unmountCanvas,
  renderGraph,
  renderToolbar,
  renderFlowList,
  renderNodePanel,
  renderTemplateBrowser,
  markNodeNew,
  setDebugState,
  setAvailableAgents,
} from './molecules';
import { parseFlowText } from './parser';
import { fitView } from './canvas-molecules';
import { FLOW_TEMPLATES } from './templates';
import { createFlowExecutor, type FlowExecutorController } from './executor';
import { createFlowChatReporter, type FlowChatReporterController } from './chat-reporter';
import { pawEngine } from '../../engine/molecules/ipc_client';
import type { EngineFlowRun } from '../../engine/atoms/types';
import { initFlowsPersistence, persist, restore, deleteFromBackend } from './flows-persistence';
import {
  initFlowsScheduler,
  rebuildScheduleRegistry,
  startScheduleTicker,
  stopScheduleTicker,
  getScheduleFireLog,
  getScheduleRegistry,
} from './flows-scheduler';
import { initFlowsKeybindings, onKeyDown } from './flows-keybindings';

// ── Module State ───────────────────────────────────────────────────────────

let _graphs: FlowGraph[] = [];
let _activeGraphId: string | null = null;
let _selectedNodeId: string | null = null;
let _selectedNodeIds = new Set<string>();
let _selectedEdgeId: string | null = null;
let _clipboard: { nodes: FlowNode[]; edges: FlowEdge[] } | null = null;
let _mounted = false;
let _executor: FlowExecutorController | null = null;
let _reporter: FlowChatReporterController | null = null;
let _sidebarTab: 'flows' | 'templates' = 'flows';

// Undo/redo stack — one per active graph
const _undoStacks: Map<string, UndoStack> = new Map();

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
    getSelectedNodeIds: () => _selectedNodeIds,
    setSelectedNodeIds: (ids: Set<string>) => {
      _selectedNodeIds = ids;
    },
    getSelectedEdgeId: () => _selectedEdgeId,
    setSelectedEdgeId: (id: string | null) => {
      _selectedEdgeId = id;
      updateNodePanel();
    },
    onGraphChanged: () => {
      const g = _graphs.find((gg) => gg.id === _activeGraphId);
      if (g) {
        // Push undo snapshot before recording the change
        const stack = getUndoStack(g.id);
        pushUndo(stack, g);
        g.updatedAt = new Date().toISOString();
      }
      persist();
      updateFlowList();
    },
    onUndo: () => performUndo(),
    onRedo: () => performRedo(),
    onExport: () => exportActiveFlow(),
    onImport: () => importFlow(),
  });
}

// ── DOM References ─────────────────────────────────────────────────────────

function el(id: string): HTMLElement | null {
  return document.getElementById(id);
}

// ── Module Initialization ──────────────────────────────────────────────────

function initModules() {
  initStateBridge();
  initFlowsPersistence({
    getGraphs: () => _graphs,
    setGraphs: (g) => {
      _graphs = g;
    },
    afterPersist: () => rebuildScheduleRegistry(),
  });
  initFlowsScheduler({
    getGraphs: () => _graphs,
    getActiveGraphId: () => _activeGraphId,
    setActiveGraphId: (id) => {
      _activeGraphId = id;
    },
    getExecutor: () => _executor,
    runActiveFlow: () => runActiveFlow(),
  });
  initFlowsKeybindings({
    el,
    getGraph: () => _graphs.find((g) => g.id === _activeGraphId),
    getSelectedNodeId: () => _selectedNodeId,
    setSelectedNodeId: (id) => {
      _selectedNodeId = id;
    },
    getSelectedNodeIds: () => _selectedNodeIds,
    setSelectedNodeIds: (ids) => {
      _selectedNodeIds = ids;
    },
    getSelectedEdgeId: () => _selectedEdgeId,
    setSelectedEdgeId: (id) => {
      _selectedEdgeId = id;
    },
    getClipboard: () => _clipboard,
    setClipboard: (c) => {
      _clipboard = c;
    },
    getUndoStack,
    persist,
    updateFlowList,
    updateNodePanel,
    performUndo,
    performRedo,
  });
}

// ── Public API ─────────────────────────────────────────────────────────────

/**
 * Called when the Flows view is activated (from router.ts switchView).
 */
export async function loadFlows() {
  initModules();
  await restore();

  // Inject available agents from the agents module for agent node dropdowns
  try {
    // Dynamic import to avoid circular dependency
    const agentStore = localStorage.getItem('paw-agents');
    if (agentStore) {
      const agents = JSON.parse(agentStore) as { id: string; name: string }[];
      setAvailableAgents(agents.map((a) => ({ id: a.id, name: a.name })));
    }
  } catch {
    /* ignore */
  }

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
export async function createFlowFromText(text: string, name?: string): Promise<FlowGraph> {
  initModules();
  await restore();

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
export async function getFlows(): Promise<FlowGraph[]> {
  await restore();
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
  requestAnimationFrame(() => fitView());
}

// ── Mount ──────────────────────────────────────────────────────────────────

function mount() {
  const canvasContainer = el('flows-canvas');
  const textInput = el('flows-text-input') as HTMLInputElement | null;

  // Restore collapsed panel/list states before canvas mount
  restorePanelStates();

  if (canvasContainer) mountCanvas(canvasContainer);

  // Create executor
  _executor = createFlowExecutor({
    onEvent: (event) => {
      _reporter?.handleEvent(event);
      if (event.type === 'run-complete' || event.type === 'run-aborted') {
        updateToolbar();
      }
    },
    onNodeStatusChange: (nodeId, status) => {
      const graph = _graphs.find((g) => g.id === _activeGraphId);
      if (graph) {
        const node = graph.nodes.find((n) => n.id === nodeId);
        if (node) node.status = status as FlowGraph['nodes'][0]['status'];
        syncDebugState();
        renderGraph();
      }
    },
    onEdgeActive: (_edgeId, _active) => {
      syncDebugState();
      renderGraph();
    },
    flowResolver: (flowId: string) => _graphs.find((g) => g.id === flowId) ?? null,
    credentialLoader: async (name: string) => {
      try {
        return await pawEngine.skillGetCredential('flow-vault', name);
      } catch {
        return null;
      }
    },
  });

  updateToolbar();

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

  // Breakpoint toggle (Shift+click on canvas nodes)
  document.addEventListener('flow:toggle-breakpoint', ((e: CustomEvent) => {
    if (_executor) {
      _executor.toggleBreakpoint(e.detail.nodeId);
      syncDebugState();
      renderGraph();
    }
  }) as EventListener);

  // Panel / list toggle from toolbar
  document.addEventListener('flow:toolbar', ((e: CustomEvent) => {
    const action = e.detail?.action;
    if (action === 'toggle-panel') togglePanel();
    else if (action === 'toggle-list') toggleList();
  }) as EventListener);

  // Edge-tab expand buttons (appear when panels are collapsed)
  const edgeTabLeft = el('flows-edge-tab-left');
  const edgeTabRight = el('flows-edge-tab-right');
  if (edgeTabLeft) edgeTabLeft.addEventListener('click', () => toggleList());
  if (edgeTabRight) edgeTabRight.addEventListener('click', () => togglePanel());

  // Keyboard shortcuts
  document.addEventListener('keydown', onKeyDown);

  updateFlowList();
  updateNodePanel();

  // Start schedule ticker
  startScheduleTicker();
}

export function unmountFlows() {
  unmountCanvas();
  stopScheduleTicker();
  document.removeEventListener('keydown', onKeyDown);
  _mounted = false;
}

// ── Internal Actions ───────────────────────────────────────────────────────

/** Toggle the right-hand properties panel. */
function togglePanel() {
  const view = el('flows-view');
  if (!view) return;
  const isCollapsed = view.classList.toggle('flows-panel-collapsed');
  // For responsive override: explicit "shown" class
  view.classList.toggle('flows-panel-shown', !isCollapsed);
  // Update toggle button icon
  const btn = document.querySelector('[data-action="toggle-panel"] .ms');
  if (btn) btn.textContent = isCollapsed ? 'right_panel_open' : 'right_panel_close';
  localStorage.setItem('paw-flows-panel-collapsed', String(isCollapsed));
}

/** Toggle the left-hand flow list sidebar. */
function toggleList() {
  const view = el('flows-view');
  if (!view) return;
  const isCollapsed = view.classList.toggle('flows-list-collapsed');
  // For responsive override: explicit "shown" class
  view.classList.toggle('flows-list-shown', !isCollapsed);
  // Update toggle button icon
  const btn = document.querySelector('[data-action="toggle-list"] .ms');
  if (btn) btn.textContent = isCollapsed ? 'left_panel_open' : 'left_panel_close';
  localStorage.setItem('paw-flows-list-collapsed', String(isCollapsed));
}

/** Restore panel/list collapsed states from localStorage. */
function restorePanelStates() {
  const view = el('flows-view');
  if (!view) return;
  if (localStorage.getItem('paw-flows-panel-collapsed') === 'true') {
    view.classList.add('flows-panel-collapsed');
  } else {
    view.classList.add('flows-panel-shown');
  }
  if (localStorage.getItem('paw-flows-list-collapsed') === 'true') {
    view.classList.add('flows-list-collapsed');
  } else {
    view.classList.add('flows-list-shown');
  }
}

/** Update the hero stat counters to reflect current state. */
function updateHeroStats() {
  const totalEl = el('flows-stat-total');
  const integEl = el('flows-stat-integrations');
  const schedEl = el('flows-stat-scheduled');

  if (totalEl) totalEl.textContent = String(_graphs.length);
  if (integEl) {
    const total = _graphs.reduce((sum, g) => sum + g.nodes.length, 0);
    integEl.textContent = String(total);
  }
  if (schedEl) {
    schedEl.textContent = String(getScheduleRegistry().length);
  }
}

function renderActiveGraph() {
  initStateBridge();
  renderGraph();
  updateNodePanel();
}

function updateFlowList() {
  const container = el('flows-list');
  if (!container) return;

  // Update hero stats
  updateHeroStats();

  // Render tab switcher
  const tabHtml = `<div class="flow-sidebar-tabs">
    <button class="flow-sidebar-tab${_sidebarTab === 'flows' ? ' active' : ''}" data-tab="flows">Flows</button>
    <button class="flow-sidebar-tab${_sidebarTab === 'templates' ? ' active' : ''}" data-tab="templates">Templates</button>
  </div>`;

  // Create a content area below tabs
  container.innerHTML = `${tabHtml}<div class="flow-sidebar-content"></div>`;

  // Wire tab clicks
  container.querySelectorAll('.flow-sidebar-tab').forEach((btn) => {
    btn.addEventListener('click', () => {
      _sidebarTab = (btn as HTMLElement).dataset.tab as 'flows' | 'templates';
      updateFlowList();
    });
  });

  const content = container.querySelector('.flow-sidebar-content') as HTMLElement;
  if (!content) return;

  if (_sidebarTab === 'templates') {
    renderTemplateBrowser(content, FLOW_TEMPLATES, (tpl: FlowTemplate) => {
      instantiateFromTemplate(tpl);
    });
  } else {
    renderFlowList(
      content,
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
        deleteFromBackend(id);
        renderActiveGraph();
        updateFlowList();
      },
      () => {
        newFlow();
      },
      // Move flow to folder
      (flowId, folder) => {
        const g = _graphs.find((gg) => gg.id === flowId);
        if (g) {
          g.folder = folder || undefined;
          g.updatedAt = new Date().toISOString();
          persist();
          updateFlowList();
        }
      },
    );
  }
}

function updateNodePanel() {
  const container = el('flows-panel');
  if (!container) return;

  const graph = _graphs.find((g) => g.id === _activeGraphId);
  const node = graph?.nodes.find((n) => n.id === _selectedNodeId) ?? null;

  renderNodePanel(
    container,
    node,
    (patch) => {
      if (!graph || !node) return;
      Object.assign(node, patch);
      graph.updatedAt = new Date().toISOString();
      persist();
      renderGraph();
      updateNodePanel();
    },
    graph ?? null,
    (graphPatch) => {
      if (!graph) return;
      Object.assign(graph, graphPatch);
      graph.updatedAt = new Date().toISOString();
      persist();
      updateFlowList();
      updateNodePanel();
    },
  );
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

function instantiateFromTemplate(tpl: FlowTemplate) {
  const graph = instantiateTemplate(tpl);
  // Mark all nodes as new for materialise animation
  for (const node of graph.nodes) {
    markNodeNew(node.id);
  }
  _graphs.push(graph);
  _activeGraphId = graph.id;
  _selectedNodeId = null;
  _sidebarTab = 'flows';
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
  markNodeNew(node.id);
  graph.nodes.push(node);
  _selectedNodeId = node.id;
  graph.updatedAt = new Date().toISOString();
  persist();
  renderGraph();
  updateFlowList();
  updateNodePanel();
}

// ── Undo/Redo ──────────────────────────────────────────────────────────────

function getUndoStack(graphId: string): UndoStack {
  let stack = _undoStacks.get(graphId);
  if (!stack) {
    stack = createUndoStack();
    _undoStacks.set(graphId, stack);
  }
  return stack;
}

function performUndo() {
  if (!_activeGraphId) return;
  const graph = _graphs.find((g) => g.id === _activeGraphId);
  if (!graph) return;
  const stack = getUndoStack(_activeGraphId);
  const restored = undo(stack, graph);
  if (!restored) return;
  // Replace in-place
  const idx = _graphs.findIndex((g) => g.id === _activeGraphId);
  if (idx >= 0) _graphs[idx] = restored;
  _selectedNodeId = null;
  persist();
  renderGraph();
  updateFlowList();
  updateNodePanel();
}

function performRedo() {
  if (!_activeGraphId) return;
  const graph = _graphs.find((g) => g.id === _activeGraphId);
  if (!graph) return;
  const stack = getUndoStack(_activeGraphId);
  const restored = redo(stack, graph);
  if (!restored) return;
  const idx = _graphs.findIndex((g) => g.id === _activeGraphId);
  if (idx >= 0) _graphs[idx] = restored;
  _selectedNodeId = null;
  persist();
  renderGraph();
  updateFlowList();
  updateNodePanel();
}

// ── Import/Export ──────────────────────────────────────────────────────────

function exportActiveFlow() {
  const graph = _graphs.find((g) => g.id === _activeGraphId);
  if (!graph) return;
  const json = serializeGraph(graph);
  const blob = new Blob([json], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `${graph.name.replace(/[^a-zA-Z0-9_-]/g, '_')}.pawflow.json`;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

function importFlow() {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = '.json,.pawflow.json';
  input.onchange = () => {
    const file = input.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      try {
        const json = reader.result as string;
        const graph = deserializeGraph(json);
        if (!graph) {
          alert('Invalid flow file: could not parse graph.');
          return;
        }
        // Assign a new ID to avoid collisions with existing flows
        graph.id = crypto.randomUUID();
        graph.name = `${graph.name} (imported)`;
        graph.createdAt = new Date().toISOString();
        graph.updatedAt = new Date().toISOString();
        _graphs.push(graph);
        _activeGraphId = graph.id;
        _selectedNodeId = null;
        persist();
        renderActiveGraph();
        updateFlowList();
      } catch (err) {
        alert(`Import failed: ${err instanceof Error ? err.message : String(err)}`);
      }
    };
    reader.readAsText(file);
  };
  input.click();
}

// ── Toolbar & Execution ────────────────────────────────────────────────────

function updateToolbar() {
  const toolbarContainer = el('flows-toolbar');
  if (!toolbarContainer) return;

  const isRunning = _executor?.isRunning() ?? false;
  const runState = _executor?.getRunState();
  const isPaused = runState?.status === 'paused';
  const isDebug = _executor?.isDebugMode() ?? false;

  renderToolbar(toolbarContainer, { isRunning, isPaused, isDebug });

  // Sync toggle button icons with current collapsed state
  const view = el('flows-view');
  if (view) {
    const panelBtn = toolbarContainer.querySelector('[data-action="toggle-panel"] .ms');
    if (panelBtn)
      panelBtn.textContent = view.classList.contains('flows-panel-collapsed')
        ? 'right_panel_open'
        : 'right_panel_close';
    const listBtn = toolbarContainer.querySelector('[data-action="toggle-list"] .ms');
    if (listBtn)
      listBtn.textContent = view.classList.contains('flows-list-collapsed')
        ? 'left_panel_open'
        : 'left_panel_close';
  }

  // Wire toolbar action buttons
  toolbarContainer.querySelectorAll('[data-action]').forEach((btn) => {
    btn.addEventListener('click', () => {
      const action = (btn as HTMLElement).dataset.action;
      if (action) handleToolbarAction(action);
    });
  });
}

function handleToolbarAction(action: string) {
  switch (action) {
    case 'run-flow':
      runActiveFlow();
      break;
    case 'debug-flow':
      startDebugMode();
      break;
    case 'step-next':
      debugStepNext();
      break;
    case 'pause-flow':
      if (_executor?.getRunState()?.status === 'paused') {
        _executor.resume();
      } else {
        _executor?.pause();
      }
      updateToolbar();
      break;
    case 'stop-flow':
      _executor?.abort();
      syncDebugState();
      updateToolbar();
      renderGraph();
      break;
  }
}

async function runActiveFlow() {
  const graph = _graphs.find((g) => g.id === _activeGraphId);
  if (!graph) {
    const { showToast } = await import('../../components/toast');
    showToast('No flow selected to run', 'error');
    return;
  }

  if (!_executor) return;
  if (_executor.isRunning()) return;

  // Create a fresh chat reporter
  _reporter?.destroy();
  _reporter = createFlowChatReporter();

  // Append reporter element into the chat messages area
  const chatMessages = document.getElementById('chat-messages');
  if (chatMessages) {
    chatMessages.appendChild(_reporter.getElement());
    // Scroll to show the report
    _reporter.getElement().scrollIntoView({ behavior: 'smooth' });
  }

  updateToolbar();

  // Phase 1.5: Record flow run start in backend
  const runId = crypto.randomUUID();
  const startedAt = new Date().toISOString();
  const flowRun: EngineFlowRun = {
    id: runId,
    flow_id: graph.id,
    status: 'running',
    started_at: startedAt,
  };
  try {
    await pawEngine.flowRunCreate(flowRun);
  } catch {
    // Backend unavailable — continue execution without persistence
  }

  try {
    const result = await _executor.run(graph);

    // Phase 1.5: Update flow run with result
    try {
      const finishedAt = new Date().toISOString();
      const updatedRun: EngineFlowRun = {
        id: runId,
        flow_id: graph.id,
        status: result.status === 'success' ? 'success' : 'error',
        duration_ms: result.totalDurationMs,
        events_json: JSON.stringify(result.outputLog ?? []),
        error:
          result.status === 'error'
            ? result.outputLog?.find((e) => e.status === 'error')?.error
            : undefined,
        started_at: startedAt,
        finished_at: finishedAt,
      };
      await pawEngine.flowRunUpdate(updatedRun);
    } catch {
      // Best-effort persistence
    }
  } catch (err) {
    console.error('[flows] Execution error:', err);

    // Record error in run history
    try {
      const updatedRun: EngineFlowRun = {
        id: runId,
        flow_id: graph.id,
        status: 'error',
        error: err instanceof Error ? err.message : String(err),
        started_at: startedAt,
        finished_at: new Date().toISOString(),
      };
      await pawEngine.flowRunUpdate(updatedRun);
    } catch {
      // Best-effort
    }
  }

  // Reset node statuses to idle after run
  for (const node of graph.nodes) {
    node.status = 'idle';
  }
  syncDebugState();
  renderGraph();
  updateToolbar();
  persist();
}

// ── Debug Mode ─────────────────────────────────────────────────────────────

async function startDebugMode() {
  const graph = _graphs.find((g) => g.id === _activeGraphId);
  if (!graph) {
    const { showToast } = await import('../../components/toast');
    showToast('No flow selected to debug', 'error');
    return;
  }

  if (!_executor) return;
  if (_executor.isRunning() || _executor.isDebugMode()) return;

  // Create a fresh chat reporter for debug session
  _reporter?.destroy();
  _reporter = createFlowChatReporter();

  const chatMessages = document.getElementById('chat-messages');
  if (chatMessages) {
    chatMessages.appendChild(_reporter.getElement());
    _reporter.getElement().scrollIntoView({ behavior: 'smooth' });
  }

  _executor.startDebug(graph);
  syncDebugState();
  renderGraph();
  updateToolbar();
}

async function debugStepNext() {
  if (!_executor || !_executor.isDebugMode()) return;

  await _executor.stepNext();
  syncDebugState();
  renderGraph();
  updateToolbar();
  updateNodePanel();
}

/**
 * Synchronize debug state from the executor to molecules for rendering.
 * This pushes breakpoints, cursor position, edge values, and node
 * execution states into the molecules layer.
 */
function syncDebugState() {
  if (!_executor) {
    setDebugState({
      breakpoints: new Set(),
      cursorNodeId: null,
      edgeValues: new Map(),
    });
    return;
  }

  // Build node states map for the debug inspector
  const debugNodeStates = new Map<string, { input: string; output: string; status: string }>();
  const runState = _executor.getRunState();
  if (runState) {
    for (const [nodeId, ns] of runState.nodeStates) {
      debugNodeStates.set(nodeId, {
        input: ns.input,
        output: ns.output,
        status: ns.status,
      });
    }
  }

  setDebugState({
    breakpoints: _executor.getBreakpoints(),
    cursorNodeId: _executor.getNextNodeId(),
    edgeValues: _executor.getEdgeValues(),
    nodeStates: debugNodeStates,
  });
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

  // Auto-zoom so the new flow is visible on the canvas
  requestAnimationFrame(() => fitView());
}

// ── Re-exports from sub-modules ────────────────────────────────────────────

export { getScheduleFireLog, getScheduleRegistry };
