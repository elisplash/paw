// ─────────────────────────────────────────────────────────────────────────────
// Flow Visualization Engine — Index (Orchestrator)
// Owns module state, wires state bridge, handles persistence, exports public API.
// ─────────────────────────────────────────────────────────────────────────────

import {
  type FlowGraph,
  type FlowNode,
  type FlowEdge,
  type FlowTemplate,
  createGraph,
  createNode as createNodeFn,
  createEdge,
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
import { FLOW_TEMPLATES } from './templates';
import { createFlowExecutor, type FlowExecutorController } from './executor';
import { createFlowChatReporter, type FlowChatReporterController } from './chat-reporter';
import { type FlowSchedule, type ScheduleFireLog, nextCronFire } from './executor-atoms';
import { pawEngine } from '../../engine/molecules/ipc_client';
import type { EngineFlow, EngineFlowRun } from '../../engine/atoms/types';

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

// Schedule state
let _scheduleTimerId: ReturnType<typeof setInterval> | null = null;
let _scheduleRegistry: FlowSchedule[] = [];
const _scheduleFireLog: ScheduleFireLog[] = [];
const SCHEDULE_TICK_MS = 30_000; // Check every 30 seconds

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

// ── Persistence (SQLite backend via Tauri IPC, localStorage fallback) ──────

let _persistTimer: ReturnType<typeof setTimeout> | null = null;
let _backendAvailable = true; // assume Tauri is present until first failure
const PERSIST_DEBOUNCE_MS = 1_000; // debounce writes by 1 second

/** Debounced persist — coalesces rapid mutations into a single backend write. */
function persist() {
  // Always keep localStorage in sync (fast, synchronous, offline fallback)
  try {
    const data = _graphs.map(serializeGraph);
    localStorage.setItem(STORAGE_KEY, JSON.stringify(data));
  } catch (e) {
    console.error('[flows] localStorage persist failed:', e);
  }

  // Rebuild schedule registry on any graph change
  rebuildScheduleRegistry();

  // Debounce backend save
  if (_persistTimer) clearTimeout(_persistTimer);
  _persistTimer = setTimeout(() => {
    _persistTimer = null;
    persistToBackend();
  }, PERSIST_DEBOUNCE_MS);
}

/** Non-debounced: save all graphs to the Rust SQLite backend. */
async function persistToBackend() {
  if (!_backendAvailable) return;
  try {
    for (const graph of _graphs) {
      const flow: EngineFlow = {
        id: graph.id,
        name: graph.name,
        description: graph.description,
        folder: graph.folder,
        graph_json: serializeGraph(graph),
        created_at: graph.createdAt,
        updated_at: graph.updatedAt,
      };
      await pawEngine.flowsSave(flow);
    }
  } catch (e) {
    // Tauri not available (browser dev mode) — localStorage only
    console.warn('[flows] Backend persist unavailable, using localStorage only:', e);
    _backendAvailable = false;
  }
}

/** Save a single graph to backend (used for targeted saves like auto-save on change). */
async function persistSingleToBackend(graph: FlowGraph) {
  if (!_backendAvailable) return;
  try {
    const flow: EngineFlow = {
      id: graph.id,
      name: graph.name,
      description: graph.description,
      folder: graph.folder,
      graph_json: serializeGraph(graph),
      created_at: graph.createdAt,
      updated_at: graph.updatedAt,
    };
    await pawEngine.flowsSave(flow);
  } catch {
    // Silently fall back — localStorage is already up to date
  }
}

/** Delete a flow from the backend. */
async function deleteFromBackend(flowId: string) {
  if (!_backendAvailable) return;
  try {
    await pawEngine.flowsDelete(flowId);
  } catch {
    // Silently fall back
  }
}

async function restore() {
  // Try backend first
  try {
    const backendFlows = await pawEngine.flowsList();
    if (backendFlows && backendFlows.length > 0) {
      _graphs = backendFlows
        .map((f) => deserializeGraph(f.graph_json))
        .filter(Boolean) as FlowGraph[];
      _backendAvailable = true;

      // Migrate any localStorage-only flows that aren't in the backend
      migrateLocalStorageFlows(backendFlows);
      return;
    }
  } catch {
    // Tauri not available — fall through to localStorage
    _backendAvailable = false;
  }

  // Fallback: localStorage
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return;
    const arr = JSON.parse(raw) as string[];
    _graphs = arr.map((s) => deserializeGraph(s)).filter(Boolean) as FlowGraph[];

    // If backend just became available, migrate localStorage flows up
    if (_backendAvailable && _graphs.length > 0) {
      persistToBackend();
    }
  } catch (e) {
    console.error('[flows] Restore failed:', e);
    _graphs = [];
  }
}

/** One-time migration: push localStorage flows to backend if they don't exist there. */
async function migrateLocalStorageFlows(backendFlows: EngineFlow[]) {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return;
    const arr = JSON.parse(raw) as string[];
    const localGraphs = arr.map((s) => deserializeGraph(s)).filter(Boolean) as FlowGraph[];
    const backendIds = new Set(backendFlows.map((f) => f.id));

    let migrated = 0;
    for (const graph of localGraphs) {
      if (!backendIds.has(graph.id)) {
        // This flow exists in localStorage but not in the backend — migrate it
        await persistSingleToBackend(graph);
        // Also add to in-memory state if not already present
        if (!_graphs.find((g) => g.id === graph.id)) {
          _graphs.push(graph);
        }
        migrated++;
      }
    }

    if (migrated > 0) {
      console.debug(`[flows] Migrated ${migrated} flow(s) from localStorage to backend`);
    }
  } catch {
    // Migration is best-effort
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
export async function loadFlows() {
  initStateBridge();
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
  initStateBridge();
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
}

// ── Mount ──────────────────────────────────────────────────────────────────

function mount() {
  const canvasContainer = el('flows-canvas');
  const textInput = el('flows-text-input') as HTMLInputElement | null;

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
    schedEl.textContent = String(_scheduleRegistry.length);
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

function onKeyDown(e: KeyboardEvent) {
  // Only handle when flows view is active
  const flowsView = el('flows-view');
  if (!flowsView?.classList.contains('active')) return;

  const graph = _graphs.find((g) => g.id === _activeGraphId);
  if (!graph) return;

  switch (e.key) {
    case 'Delete':
    case 'Backspace':
      if (
        !(e.target instanceof HTMLInputElement) &&
        !(e.target instanceof HTMLTextAreaElement)
      ) {
        // Collect nodes to delete: multi-select or single
        const idsToDelete = _selectedNodeIds.size > 0
          ? new Set(_selectedNodeIds)
          : (_selectedNodeId ? new Set([_selectedNodeId]) : new Set<string>());
        if (idsToDelete.size > 0) {
          const stack = getUndoStack(graph.id);
          pushUndo(stack, graph);
          graph.nodes = graph.nodes.filter((n) => !idsToDelete.has(n.id));
          graph.edges = graph.edges.filter(
            (ee) => !idsToDelete.has(ee.from) && !idsToDelete.has(ee.to),
          );
          _selectedNodeId = null;
          _selectedNodeIds = new Set();
          _selectedEdgeId = null;
          graph.updatedAt = new Date().toISOString();
          persist();
          renderGraph();
          updateFlowList();
          updateNodePanel();
          e.preventDefault();
        } else if (_selectedEdgeId) {
          // Delete selected edge
          const stack = getUndoStack(graph.id);
          pushUndo(stack, graph);
          graph.edges = graph.edges.filter((ee) => ee.id !== _selectedEdgeId);
          _selectedEdgeId = null;
          graph.updatedAt = new Date().toISOString();
          persist();
          renderGraph();
          updateNodePanel();
          e.preventDefault();
        }
      }
      break;
    case 'Escape':
      _selectedNodeId = null;
      _selectedNodeIds = new Set();
      _selectedEdgeId = null;
      renderGraph();
      updateNodePanel();
      break;
    case 'z':
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey) {
        performUndo();
        e.preventDefault();
      } else if ((e.ctrlKey || e.metaKey) && e.shiftKey) {
        performRedo();
        e.preventDefault();
      }
      break;
    case 'Z':
      // Shift+Ctrl+Z (capital Z on some keyboards)
      if (e.ctrlKey || e.metaKey) {
        performRedo();
        e.preventDefault();
      }
      break;
    case 'y':
      // Ctrl+Y as alternative redo
      if (e.ctrlKey || e.metaKey) {
        performRedo();
        e.preventDefault();
      }
      break;
    case 'a':
      if (e.ctrlKey || e.metaKey) {
        // Select all nodes
        _selectedNodeIds = new Set(graph.nodes.map((n) => n.id));
        _selectedNodeId = null;
        renderGraph();
        e.preventDefault();
      }
      break;
    case 'c':
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey) {
        // Copy selected nodes to clipboard
        const copyIds = _selectedNodeIds.size > 0
          ? _selectedNodeIds
          : (_selectedNodeId ? new Set([_selectedNodeId]) : new Set<string>());
        if (copyIds.size > 0) {
          const copiedNodes = graph.nodes
            .filter((n) => copyIds.has(n.id))
            .map((n) => JSON.parse(JSON.stringify(n)) as FlowNode);
          const copiedEdges = graph.edges
            .filter((ee) => copyIds.has(ee.from) && copyIds.has(ee.to))
            .map((ee) => JSON.parse(JSON.stringify(ee)) as FlowEdge);
          _clipboard = { nodes: copiedNodes, edges: copiedEdges };
          e.preventDefault();
        }
      }
      break;
    case 'v':
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && _clipboard && _clipboard.nodes.length > 0) {
        // Paste from clipboard with new IDs and offset positions
        const stack = getUndoStack(graph.id);
        pushUndo(stack, graph);

        const idMap = new Map<string, string>();
        const PASTE_OFFSET = 40;
        const newIds = new Set<string>();

        for (const srcNode of _clipboard.nodes) {
          const newNode = createNodeFn(
            srcNode.kind,
            srcNode.label,
            srcNode.x + PASTE_OFFSET,
            srcNode.y + PASTE_OFFSET,
          );
          newNode.config = JSON.parse(JSON.stringify(srcNode.config ?? {}));
          newNode.width = srcNode.width;
          newNode.height = srcNode.height;
          idMap.set(srcNode.id, newNode.id);
          newIds.add(newNode.id);
          graph.nodes.push(newNode);
        }

        for (const srcEdge of _clipboard.edges) {
          const newFrom = idMap.get(srcEdge.from);
          const newTo = idMap.get(srcEdge.to);
          if (newFrom && newTo) {
            const newEdge = createEdge(newFrom, newTo, srcEdge.kind, {
              fromPort: srcEdge.fromPort,
              toPort: srcEdge.toPort,
              label: srcEdge.label,
            });
            graph.edges.push(newEdge);
          }
        }

        _selectedNodeIds = newIds;
        _selectedNodeId = newIds.size === 1 ? [...newIds][0] : null;
        graph.updatedAt = new Date().toISOString();
        persist();
        renderGraph();
        updateFlowList();
        e.preventDefault();
      }
      break;
  }
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

// ── Schedule Registry ──────────────────────────────────────────────────────

/**
 * Scan all flows for trigger nodes with active schedules and build the registry.
 */
function rebuildScheduleRegistry() {
  _scheduleRegistry = [];
  for (const graph of _graphs) {
    for (const node of graph.nodes) {
      if (node.kind !== 'trigger') continue;
      const schedule = (node.config?.schedule as string) ?? '';
      const enabled = (node.config?.scheduleEnabled as boolean) ?? false;
      if (!schedule || !enabled) continue;

      const next = nextCronFire(schedule);
      _scheduleRegistry.push({
        flowId: graph.id,
        flowName: graph.name,
        nodeId: node.id,
        schedule,
        enabled,
        lastFiredAt: null,
        nextFireAt: next ? next.getTime() : null,
      });
    }
  }
}

/**
 * Start the schedule ticker. Checks every 30 seconds for due schedules.
 */
function startScheduleTicker() {
  if (_scheduleTimerId) return;
  rebuildScheduleRegistry();

  _scheduleTimerId = setInterval(() => {
    scheduleTickCheck();
  }, SCHEDULE_TICK_MS);
}

/**
 * Stop the schedule ticker.
 */
function stopScheduleTicker() {
  if (_scheduleTimerId) {
    clearInterval(_scheduleTimerId);
    _scheduleTimerId = null;
  }
}

/**
 * Check all schedules for any that are due and fire them.
 */
async function scheduleTickCheck() {
  const now = Date.now();
  for (const entry of _scheduleRegistry) {
    if (!entry.enabled || !entry.nextFireAt) continue;
    if (entry.nextFireAt > now) continue;

    // This schedule is due — fire it
    console.debug(`[flows] Schedule fired: ${entry.flowName} (${entry.schedule})`);
    entry.lastFiredAt = now;

    const graph = _graphs.find((g) => g.id === entry.flowId);
    if (!graph) continue;

    // Don't run if executor is busy
    if (_executor?.isRunning()) {
      _scheduleFireLog.push({
        flowId: entry.flowId,
        flowName: entry.flowName,
        firedAt: now,
        status: 'error',
        error: 'Executor busy — skipped',
      });
      continue;
    }

    try {
      // Switch to this flow and run it
      _activeGraphId = entry.flowId;
      await runActiveFlow();

      _scheduleFireLog.push({
        flowId: entry.flowId,
        flowName: entry.flowName,
        firedAt: now,
        status: 'success',
      });
    } catch (err) {
      _scheduleFireLog.push({
        flowId: entry.flowId,
        flowName: entry.flowName,
        firedAt: now,
        status: 'error',
        error: String(err),
      });
    }

    // Recalculate next fire time
    const next = nextCronFire(entry.schedule);
    entry.nextFireAt = next ? next.getTime() : null;
  }
}

/**
 * Get the schedule fire log (for UI display).
 */
export function getScheduleFireLog(): ScheduleFireLog[] {
  return [..._scheduleFireLog];
}

/**
 * Get the active schedule registry (for UI display).
 */
export function getScheduleRegistry(): FlowSchedule[] {
  return [..._scheduleRegistry];
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
