// ─────────────────────────────────────────────────────────────────────────────
// Conductor Protocol — Parallel & Mesh Atoms
// Parallel group detection, independent subgroup splitting, convergent mesh
// configuration, and text-similarity convergence checking.
// Pure logic, no DOM, no IPC.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowGraph } from './atoms';

// ── Parallel Grouping ──────────────────────────────────────────────────────

/** Group of nodes at the same depth that can execute concurrently. */
export interface ParallelGroup {
  depth: number;
  nodeIds: string[];
}

/**
 * Group nodes by depth level for parallel execution.
 * Nodes at the same depth with no mutual data dependency can run concurrently.
 */
export function groupByDepth(_graph: FlowGraph, depths: Map<string, number>): ParallelGroup[] {
  const byDepth = new Map<number, string[]>();

  for (const [nodeId, depth] of depths) {
    if (!byDepth.has(depth)) byDepth.set(depth, []);
    byDepth.get(depth)!.push(nodeId);
  }

  const sorted = [...byDepth.entries()].sort((a, b) => a[0] - b[0]);
  return sorted.map(([depth, nodeIds]) => ({ depth, nodeIds }));
}

/**
 * Check if two nodes at the same depth have a data dependency between them.
 */
export function hasDataDependency(graph: FlowGraph, nodeA: string, nodeB: string): boolean {
  for (const e of graph.edges) {
    if ((e.from === nodeA && e.to === nodeB) || (e.from === nodeB && e.to === nodeA)) {
      return true;
    }
  }
  return false;
}

/**
 * Split a depth group into independent parallel sub-groups.
 * Nodes with mutual dependencies go into the same sub-group (sequential within).
 */
export function splitIntoIndependentGroups(graph: FlowGraph, nodeIds: string[]): string[][] {
  if (nodeIds.length <= 1) return [nodeIds];

  // Build a simple union-find for grouping dependent nodes
  const parent = new Map<string, string>();
  for (const id of nodeIds) parent.set(id, id);

  function find(x: string): string {
    while (parent.get(x) !== x) {
      parent.set(x, parent.get(parent.get(x)!)!);
      x = parent.get(x)!;
    }
    return x;
  }

  function union(a: string, b: string) {
    const ra = find(a);
    const rb = find(b);
    if (ra !== rb) parent.set(ra, rb);
  }

  for (let i = 0; i < nodeIds.length; i++) {
    for (let j = i + 1; j < nodeIds.length; j++) {
      if (hasDataDependency(graph, nodeIds[i], nodeIds[j])) {
        union(nodeIds[i], nodeIds[j]);
      }
    }
  }

  const groupMap = new Map<string, string[]>();
  for (const id of nodeIds) {
    const root = find(id);
    if (!groupMap.has(root)) groupMap.set(root, []);
    groupMap.get(root)!.push(id);
  }

  return [...groupMap.values()];
}

// ── Convergence Detection ──────────────────────────────────────────────────

/** Configuration for a convergent mesh. */
export interface MeshConfig {
  /** Node IDs participating in the mesh (cycle members) */
  nodeIds: string[];
  /** Maximum iterations before forced synthesis */
  maxIterations: number;
  /** Convergence threshold (0-1, cosine similarity of consecutive outputs) */
  convergenceThreshold: number;
}

/**
 * Build mesh configurations from detected cycles.
 */
export function buildMeshConfigs(
  cycles: Set<string>[],
  defaultMaxIterations = 5,
  defaultThreshold = 0.85,
): MeshConfig[] {
  const merged = mergeCycles(cycles);

  return merged.map((nodeIds) => ({
    nodeIds: [...nodeIds],
    maxIterations: defaultMaxIterations,
    convergenceThreshold: defaultThreshold,
  }));
}

/**
 * Merge overlapping cycle sets into disjoint groups.
 */
function mergeCycles(cycles: Set<string>[]): Set<string>[] {
  if (cycles.length === 0) return [];

  const result: Set<string>[] = [...cycles.map((c) => new Set(c))];

  let changed = true;
  while (changed) {
    changed = false;
    for (let i = 0; i < result.length; i++) {
      for (let j = i + 1; j < result.length; j++) {
        let overlaps = false;
        for (const id of result[i]) {
          if (result[j].has(id)) {
            overlaps = true;
            break;
          }
        }
        if (overlaps) {
          for (const id of result[j]) result[i].add(id);
          result.splice(j, 1);
          changed = true;
          break;
        }
      }
      if (changed) break;
    }
  }

  return result;
}

/**
 * Simple text similarity check for convergence detection.
 * Uses normalized Jaccard similarity on word tokens.
 * Returns 0-1 where 1 = identical.
 */
export function textSimilarity(a: string, b: string): number {
  if (a === b) return 1;
  if (!a || !b) return 0;

  const wordsA = new Set(a.toLowerCase().split(/\s+/).filter(Boolean));
  const wordsB = new Set(b.toLowerCase().split(/\s+/).filter(Boolean));

  if (wordsA.size === 0 && wordsB.size === 0) return 1;
  if (wordsA.size === 0 || wordsB.size === 0) return 0;

  let intersection = 0;
  for (const w of wordsA) {
    if (wordsB.has(w)) intersection++;
  }

  const unionSize = wordsA.size + wordsB.size - intersection;
  return unionSize > 0 ? intersection / unionSize : 0;
}

/**
 * Check if mesh outputs have converged across consecutive rounds.
 */
export function checkConvergence(
  prevOutputs: Map<string, string>,
  currOutputs: Map<string, string>,
  threshold: number,
): boolean {
  if (prevOutputs.size === 0) return false;

  let totalSim = 0;
  let count = 0;

  for (const [nodeId, currText] of currOutputs) {
    const prevText = prevOutputs.get(nodeId);
    if (prevText !== undefined) {
      totalSim += textSimilarity(prevText, currText);
      count++;
    }
  }

  if (count === 0) return false;
  return totalSim / count >= threshold;
}
