// ─────────────────────────────────────────────────────────────────────────────
// Flow Persistence — SQLite backend via Tauri IPC, localStorage fallback
// ─────────────────────────────────────────────────────────────────────────────

import { type FlowGraph, serializeGraph, deserializeGraph } from './atoms';
import { pawEngine } from '../../engine/molecules/ipc_client';
import type { EngineFlow } from '../../engine/atoms/types';

// ── Local State ────────────────────────────────────────────────────────────

let _persistTimer: ReturnType<typeof setTimeout> | null = null;
let _backendAvailable = true; // assume Tauri is present until first failure
const PERSIST_DEBOUNCE_MS = 1_000;
const STORAGE_KEY = 'openpawz-flows';

// ── Dependency Injection ───────────────────────────────────────────────────

export interface PersistenceDeps {
  getGraphs: () => FlowGraph[];
  setGraphs: (g: FlowGraph[]) => void;
  afterPersist: () => void; // e.g. rebuildScheduleRegistry
}

let _deps: PersistenceDeps | null = null;

export function initFlowsPersistence(deps: PersistenceDeps) {
  _deps = deps;
}

// ── Persistence Functions ──────────────────────────────────────────────────

/** Debounced persist — coalesces rapid mutations into a single backend write. */
export function persist() {
  if (!_deps) return;
  const graphs = _deps.getGraphs();

  // Always keep localStorage in sync (fast, synchronous, offline fallback)
  try {
    const data = graphs.map(serializeGraph);
    localStorage.setItem(STORAGE_KEY, JSON.stringify(data));
  } catch (e) {
    console.error('[flows] localStorage persist failed:', e);
  }

  // Rebuild schedule registry on any graph change
  _deps.afterPersist();

  // Debounce backend save
  if (_persistTimer) clearTimeout(_persistTimer);
  _persistTimer = setTimeout(() => {
    _persistTimer = null;
    persistToBackend();
  }, PERSIST_DEBOUNCE_MS);
}

/** Non-debounced: save all graphs to the Rust SQLite backend. */
export async function persistToBackend() {
  if (!_deps || !_backendAvailable) return;
  try {
    for (const graph of _deps.getGraphs()) {
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

/** Save a single graph to backend (used for targeted saves). */
export async function persistSingleToBackend(graph: FlowGraph) {
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
export async function deleteFromBackend(flowId: string) {
  if (!_backendAvailable) return;
  try {
    await pawEngine.flowsDelete(flowId);
  } catch {
    // Silently fall back
  }
}

/** Restore flows from backend (primary) or localStorage (fallback). */
export async function restore() {
  if (!_deps) return;

  // Try backend first
  try {
    const backendFlows = await pawEngine.flowsList();
    if (backendFlows && backendFlows.length > 0) {
      _deps.setGraphs(
        backendFlows.map((f) => deserializeGraph(f.graph_json)).filter(Boolean) as FlowGraph[],
      );
      _backendAvailable = true;

      // Migrate any localStorage-only flows that aren't in the backend
      await migrateLocalStorageFlows(backendFlows);
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
    const graphs = arr.map((s) => deserializeGraph(s)).filter(Boolean) as FlowGraph[];
    _deps.setGraphs(graphs);

    // If backend just became available, migrate localStorage flows up
    if (_backendAvailable && graphs.length > 0) {
      persistToBackend();
    }
  } catch (e) {
    console.error('[flows] Restore failed:', e);
    _deps.setGraphs([]);
  }
}

/** One-time migration: push localStorage flows to backend if they don't exist there. */
async function migrateLocalStorageFlows(backendFlows: EngineFlow[]) {
  if (!_deps) return;
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return;
    const arr = JSON.parse(raw) as string[];
    const localGraphs = arr.map((s) => deserializeGraph(s)).filter(Boolean) as FlowGraph[];
    const backendIds = new Set(backendFlows.map((f) => f.id));

    let migrated = 0;
    const graphs = _deps.getGraphs();
    for (const graph of localGraphs) {
      if (!backendIds.has(graph.id)) {
        // This flow exists in localStorage but not in the backend — migrate it
        await persistSingleToBackend(graph);
        // Also add to in-memory state if not already present
        if (!graphs.find((g) => g.id === graph.id)) {
          graphs.push(graph);
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
