// ─────────────────────────────────────────────────────────────────────────────
// Flow Visualization Engine — Molecule Shared State
// Central state bridge, shared types, and utility helpers used by all molecule
// sub-modules.  No DOM, no rendering — just state plumbing.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowGraph } from './atoms';

// ── State Bridge ───────────────────────────────────────────────────────────

export interface MoleculesState {
  getGraph: () => FlowGraph | null;
  setGraph: (g: FlowGraph) => void;
  getSelectedNodeId: () => string | null;
  setSelectedNodeId: (id: string | null) => void;
  /** Phase 3.5: Multi-select — get the set of all selected node IDs */
  getSelectedNodeIds: () => ReadonlySet<string>;
  /** Phase 3.5: Multi-select — replace the entire selection set */
  setSelectedNodeIds: (ids: Set<string>) => void;
  /** Phase 3.6: Get selected edge ID */
  getSelectedEdgeId: () => string | null;
  /** Phase 3.6: Set selected edge ID */
  setSelectedEdgeId: (id: string | null) => void;
  onGraphChanged: () => void;
  /** Phase 1.3: Undo/Redo callbacks (set by index.ts, called by toolbar/keyboard). */
  onUndo?: () => void;
  onRedo?: () => void;
  /** Phase 1.4: Import/Export callbacks. */
  onExport?: () => void;
  onImport?: () => void;
}

let _state: MoleculesState;

export function setMoleculesState(s: MoleculesState) {
  _state = s;
}

export function getMoleculesState(): MoleculesState {
  return _state;
}

// ── Available Agents (injected from index.ts) ──────────────────────────────

export interface AgentRef {
  id: string;
  name: string;
}

let _availableAgents: AgentRef[] = [];

/** Set the list of agents available for agent-node dropdowns. */
export function setAvailableAgents(agents: AgentRef[]) {
  _availableAgents = agents;
}

/** Get the current list of available agents. */
export function getAvailableAgents(): AgentRef[] {
  return _availableAgents;
}

// ── Debug State (shared between canvas and panel) ──────────────────────────

let _breakpoints: ReadonlySet<string> = new Set();
let _debugCursorNodeId: string | null = null;
let _edgeValues: ReadonlyMap<string, string> = new Map();
let _debugNodeStates: Map<string, { input: string; output: string; status: string }> = new Map();

/** Update debug visuals (called from index.ts). */
export function setDebugState(opts: {
  breakpoints: ReadonlySet<string>;
  cursorNodeId: string | null;
  edgeValues: ReadonlyMap<string, string>;
  nodeStates?: Map<string, { input: string; output: string; status: string }>;
}) {
  _breakpoints = opts.breakpoints;
  _debugCursorNodeId = opts.cursorNodeId;
  _edgeValues = opts.edgeValues;
  if (opts.nodeStates) _debugNodeStates = opts.nodeStates;
}

export function getDebugBreakpoints(): ReadonlySet<string> {
  return _breakpoints;
}

export function getDebugCursorNodeId(): string | null {
  return _debugCursorNodeId;
}

export function getDebugEdgeValues(): ReadonlyMap<string, string> {
  return _edgeValues;
}

export function getDebugNodeStates(): Map<
  string,
  { input: string; output: string; status: string }
> {
  return _debugNodeStates;
}

// ── Selected Edge (shared between canvas and panel) ────────────────────────

let _selectedEdgeId: string | null = null;

export function getSelectedEdgeIdLocal(): string | null {
  return _selectedEdgeId;
}

export function setSelectedEdgeIdLocal(id: string | null) {
  _selectedEdgeId = id;
}

// ── Shared Helpers ─────────────────────────────────────────────────────────

export function formatDate(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
  } catch {
    return '';
  }
}

export function escAttr(s: string): string {
  return s.replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}
