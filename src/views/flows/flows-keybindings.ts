// ─────────────────────────────────────────────────────────────────────────────
// Flow Keyboard Shortcuts
// ─────────────────────────────────────────────────────────────────────────────

import {
  type FlowGraph,
  type FlowNode,
  type FlowEdge,
  type UndoStack,
  createNode as createNodeFn,
  createEdge,
  pushUndo,
} from './atoms';
import { renderGraph } from './molecules';

// ── Dependency Injection ───────────────────────────────────────────────────

export interface KeybindingDeps {
  el: (id: string) => HTMLElement | null;
  getGraph: () => FlowGraph | undefined;
  getSelectedNodeId: () => string | null;
  setSelectedNodeId: (id: string | null) => void;
  getSelectedNodeIds: () => Set<string>;
  setSelectedNodeIds: (ids: Set<string>) => void;
  getSelectedEdgeId: () => string | null;
  setSelectedEdgeId: (id: string | null) => void;
  getClipboard: () => { nodes: FlowNode[]; edges: FlowEdge[] } | null;
  setClipboard: (c: { nodes: FlowNode[]; edges: FlowEdge[] } | null) => void;
  getUndoStack: (graphId: string) => UndoStack;
  persist: () => void;
  updateFlowList: () => void;
  updateNodePanel: () => void;
  performUndo: () => void;
  performRedo: () => void;
}

let _deps: KeybindingDeps | null = null;

export function initFlowsKeybindings(deps: KeybindingDeps) {
  _deps = deps;
}

// ── Key Handler ────────────────────────────────────────────────────────────

export function onKeyDown(e: KeyboardEvent) {
  if (!_deps) return;

  // Only handle when flows view is active
  const flowsView = _deps.el('flows-view');
  if (!flowsView?.classList.contains('active')) return;

  const graph = _deps.getGraph();
  if (!graph) return;

  switch (e.key) {
    case 'Delete':
    case 'Backspace':
      if (!(e.target instanceof HTMLInputElement) && !(e.target instanceof HTMLTextAreaElement)) {
        // Collect nodes to delete: multi-select or single
        const selectedNodeIds = _deps.getSelectedNodeIds();
        const selectedNodeId = _deps.getSelectedNodeId();
        const idsToDelete =
          selectedNodeIds.size > 0
            ? new Set(selectedNodeIds)
            : selectedNodeId
              ? new Set([selectedNodeId])
              : new Set<string>();
        if (idsToDelete.size > 0) {
          const stack = _deps.getUndoStack(graph.id);
          pushUndo(stack, graph);
          graph.nodes = graph.nodes.filter((n) => !idsToDelete.has(n.id));
          graph.edges = graph.edges.filter(
            (ee) => !idsToDelete.has(ee.from) && !idsToDelete.has(ee.to),
          );
          _deps.setSelectedNodeId(null);
          _deps.setSelectedNodeIds(new Set());
          _deps.setSelectedEdgeId(null);
          graph.updatedAt = new Date().toISOString();
          _deps.persist();
          renderGraph();
          _deps.updateFlowList();
          _deps.updateNodePanel();
          e.preventDefault();
        } else {
          const selectedEdgeId = _deps.getSelectedEdgeId();
          if (selectedEdgeId) {
            // Delete selected edge
            const stack = _deps.getUndoStack(graph.id);
            pushUndo(stack, graph);
            graph.edges = graph.edges.filter((ee) => ee.id !== selectedEdgeId);
            _deps.setSelectedEdgeId(null);
            graph.updatedAt = new Date().toISOString();
            _deps.persist();
            renderGraph();
            _deps.updateNodePanel();
            e.preventDefault();
          }
        }
      }
      break;
    case 'Escape':
      _deps.setSelectedNodeId(null);
      _deps.setSelectedNodeIds(new Set());
      _deps.setSelectedEdgeId(null);
      renderGraph();
      _deps.updateNodePanel();
      break;
    case 'z':
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey) {
        _deps.performUndo();
        e.preventDefault();
      } else if ((e.ctrlKey || e.metaKey) && e.shiftKey) {
        _deps.performRedo();
        e.preventDefault();
      }
      break;
    case 'Z':
      // Shift+Ctrl+Z (capital Z on some keyboards)
      if (e.ctrlKey || e.metaKey) {
        _deps.performRedo();
        e.preventDefault();
      }
      break;
    case 'y':
      // Ctrl+Y as alternative redo
      if (e.ctrlKey || e.metaKey) {
        _deps.performRedo();
        e.preventDefault();
      }
      break;
    case 'a':
      if (e.ctrlKey || e.metaKey) {
        // Select all nodes
        _deps.setSelectedNodeIds(new Set(graph.nodes.map((n) => n.id)));
        _deps.setSelectedNodeId(null);
        renderGraph();
        e.preventDefault();
      }
      break;
    case 'c':
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey) {
        // Copy selected nodes to clipboard
        const selIds = _deps.getSelectedNodeIds();
        const selId = _deps.getSelectedNodeId();
        const copyIds = selIds.size > 0 ? selIds : selId ? new Set([selId]) : new Set<string>();
        if (copyIds.size > 0) {
          const copiedNodes = graph.nodes
            .filter((n) => copyIds.has(n.id))
            .map((n) => JSON.parse(JSON.stringify(n)) as FlowNode);
          const copiedEdges = graph.edges
            .filter((ee) => copyIds.has(ee.from) && copyIds.has(ee.to))
            .map((ee) => JSON.parse(JSON.stringify(ee)) as FlowEdge);
          _deps.setClipboard({ nodes: copiedNodes, edges: copiedEdges });
          e.preventDefault();
        }
      }
      break;
    case 'v':
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey) {
        const clipboard = _deps.getClipboard();
        if (clipboard && clipboard.nodes.length > 0) {
          // Paste from clipboard with new IDs and offset positions
          const stack = _deps.getUndoStack(graph.id);
          pushUndo(stack, graph);

          const idMap = new Map<string, string>();
          const PASTE_OFFSET = 40;
          const newIds = new Set<string>();

          for (const srcNode of clipboard.nodes) {
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

          for (const srcEdge of clipboard.edges) {
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

          _deps.setSelectedNodeIds(newIds);
          _deps.setSelectedNodeId(newIds.size === 1 ? [...newIds][0] : null);
          graph.updatedAt = new Date().toISOString();
          _deps.persist();
          renderGraph();
          _deps.updateFlowList();
          e.preventDefault();
        }
      }
      break;
  }
}
