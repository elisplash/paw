// ─────────────────────────────────────────────────────────────────────────────
// Flow Visualization Engine — Index (Orchestrator)
// Owns module state, wires state bridge, handles persistence, exports public API.
// ─────────────────────────────────────────────────────────────────────────────

import {
  type FlowGraph,
  type FlowTemplate,
  createGraph,
  createNode as createNodeFn,
  serializeGraph,
  deserializeGraph,
  instantiateTemplate,
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
} from './molecules';
import { parseFlowText } from './parser';
import { FLOW_TEMPLATES } from './templates';
import { createFlowExecutor, type FlowExecutorController } from './executor';
import { createFlowChatReporter, type FlowChatReporterController } from './chat-reporter';
import { type FlowSchedule, type ScheduleFireLog, nextCronFire } from './executor-atoms';

// ── Module State ───────────────────────────────────────────────────────────

let _graphs: FlowGraph[] = [];
let _activeGraphId: string | null = null;
let _selectedNodeId: string | null = null;
let _mounted = false;
let _executor: FlowExecutorController | null = null;
let _reporter: FlowChatReporterController | null = null;
let _sidebarTab: 'flows' | 'templates' = 'flows';

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
    // Rebuild schedule registry on any graph change
    rebuildScheduleRegistry();
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

function renderActiveGraph() {
  initStateBridge();
  renderGraph();
  updateNodePanel();
}

function updateFlowList() {
  const container = el('flows-list');
  if (!container) return;

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
        renderActiveGraph();
        updateFlowList();
      },
      () => {
        newFlow();
      },
    );
  }
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

  try {
    await _executor.run(graph);
  } catch (err) {
    console.error('[flows] Execution error:', err);
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
