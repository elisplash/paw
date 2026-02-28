// ─────────────────────────────────────────────────────────────────────────────
// The Conductor Protocol — Atoms (Pure Logic)
// AI-compiled flow execution: Collapse, Extract, Parallelize, Converge.
// No DOM, no IPC — fully testable.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowGraph, FlowNode, FlowEdge, FlowNodeKind } from './atoms';
import { getNodeExecConfig, type NodeExecConfig } from './executor-atoms';

// ── Conductor Types ────────────────────────────────────────────────────────

/** Classification of how a node should execute. */
export type NodeExecClassification =
  | 'agent' // Needs LLM call (agent, data, semantic condition)
  | 'direct' // Deterministic — bypass LLM (tool, code, http, mcp-tool, output, error)
  | 'passthrough'; // No execution needed (trigger, output with no transform)

/** A single unit in the compiled execution strategy. */
export interface ExecutionUnit {
  /** Unique unit ID */
  id: string;
  /** Type of unit */
  type: 'collapsed-agent' | 'direct-action' | 'single-agent' | 'single-direct' | 'mesh';
  /** Node IDs in this unit (ordered) */
  nodeIds: string[];
  /** For collapsed-agent: merged compound prompt */
  mergedPrompt?: string;
  /** For mesh: max iterations */
  maxIterations?: number;
  /** Dependencies: unit IDs that must complete before this unit starts */
  dependsOn: string[];
}

/** A phase in the execution strategy — units within a phase run in parallel. */
export interface ExecutionPhase {
  /** Phase index (0-based) */
  index: number;
  /** Units to execute in this phase (all run concurrently) */
  units: ExecutionUnit[];
}

/** The compiled execution strategy produced by the Conductor. */
export interface ExecutionStrategy {
  /** Original graph ID */
  graphId: string;
  /** Ordered phases of execution */
  phases: ExecutionPhase[];
  /** Total node count covered */
  totalNodes: number;
  /** How many LLM calls the strategy requires (vs original sequential count) */
  estimatedLlmCalls: number;
  /** How many direct actions (no LLM) */
  estimatedDirectActions: number;
  /** Whether the Conductor was used (vs threshold bypass) */
  conductorUsed: boolean;
  /** Compilation metadata */
  meta: {
    collapseGroups: number;
    parallelPhases: number;
    meshCount: number;
    extractedNodes: number;
  };
}

/** Result of convergent mesh execution. */
export interface MeshRound {
  round: number;
  nodeOutputs: Map<string, string>;
  converged: boolean;
}

// ── Node Classification ────────────────────────────────────────────────────

/** Kinds that bypass LLM entirely — direct execution. */
const DIRECT_KINDS: Set<FlowNodeKind> = new Set([
  'tool',
  'code',
  'output',
  'error',
  'http' as FlowNodeKind,
  'mcp-tool' as FlowNodeKind,
  'loop' as FlowNodeKind,
  'group',
  'memory' as FlowNodeKind,
  'memory-recall' as FlowNodeKind,
]);

/** Kinds that are passthrough (no real execution). */
const PASSTHROUGH_KINDS: Set<FlowNodeKind> = new Set(['trigger']);

/**
 * Classify how a node should be executed.
 * - Direct nodes bypass LLM (deterministic actions)
 * - Agent nodes need LLM calls
 * - Passthrough nodes just forward data
 */
export function classifyNode(node: FlowNode): NodeExecClassification {
  if (PASSTHROUGH_KINDS.has(node.kind)) return 'passthrough';
  if (DIRECT_KINDS.has(node.kind)) return 'direct';

  // Squad nodes invoke multi-agent teams — always agent, never collapse
  if (node.kind === ('squad' as FlowNodeKind)) return 'agent';

  // Tool nodes with no prompt are direct (action-only)
  if (node.kind === 'tool') {
    const config = getNodeExecConfig(node);
    if (!config.prompt) return 'direct';
  }

  // Condition nodes: check if they have a structured expression (direct eval)
  // or need AI evaluation (agent)
  if (node.kind === 'condition') {
    const config = getNodeExecConfig(node);
    if (config.conditionExpr && isStructuredCondition(config.conditionExpr)) {
      return 'direct';
    }
    return 'agent';
  }

  return 'agent';
}

/**
 * Check if a condition expression can be evaluated structurally (no AI needed).
 * Supports: comparisons, boolean literals, simple expressions.
 */
export function isStructuredCondition(expr: string): boolean {
  const normalized = expr.trim().toLowerCase();
  // Boolean literals
  if (['true', 'false', 'yes', 'no'].includes(normalized)) return true;
  // Simple comparisons: >, <, >=, <=, ===, !==, ==, !=
  if (/^.+\s*(===|!==|>=|<=|==|!=|>|<)\s*.+$/.test(normalized)) return true;
  // Property access patterns: input.status, data.length
  if (/^[a-z_$][\w$.]*\s*(===|!==|>=|<=|==|!=|>|<)\s*.+$/i.test(normalized)) return true;
  return false;
}

// ── Graph Analysis ─────────────────────────────────────────────────────────

/**
 * Build adjacency info for the graph.
 * Returns maps for forward edges and reverse lookup.
 */
export function buildAdjacency(graph: FlowGraph): {
  forward: Map<string, string[]>;
  backward: Map<string, string[]>;
  edgeMap: Map<string, FlowEdge>;
} {
  const forward = new Map<string, string[]>();
  const backward = new Map<string, string[]>();
  const edgeMap = new Map<string, FlowEdge>();

  for (const n of graph.nodes) {
    forward.set(n.id, []);
    backward.set(n.id, []);
  }

  for (const e of graph.edges) {
    if (e.kind === 'reverse') continue;
    forward.get(e.from)?.push(e.to);
    backward.get(e.to)?.push(e.from);
    edgeMap.set(e.id, e);
  }

  return { forward, backward, edgeMap };
}

/**
 * Detect cycles in the graph using DFS.
 * Returns sets of node IDs that participate in cycles.
 */
export function detectCycles(graph: FlowGraph): Set<string>[] {
  const { forward } = buildAdjacency(graph);
  const visited = new Set<string>();
  const inStack = new Set<string>();
  const cycleNodes = new Set<string>();
  const cycles: Set<string>[] = [];

  function dfs(nodeId: string, path: string[]): void {
    if (inStack.has(nodeId)) {
      // Found a cycle — collect all nodes from where the cycle starts
      const cycleStart = path.indexOf(nodeId);
      if (cycleStart >= 0) {
        const cycle = new Set<string>();
        for (let i = cycleStart; i < path.length; i++) {
          cycle.add(path[i]);
          cycleNodes.add(path[i]);
        }
        cycles.push(cycle);
      }
      return;
    }
    if (visited.has(nodeId)) return;

    visited.add(nodeId);
    inStack.add(nodeId);
    path.push(nodeId);

    for (const child of forward.get(nodeId) ?? []) {
      dfs(child, path);
    }

    path.pop();
    inStack.delete(nodeId);
  }

  for (const node of graph.nodes) {
    if (!visited.has(node.id)) {
      dfs(node.id, []);
    }
  }

  return cycles;
}

/**
 * Compute depth levels for nodes in a DAG (ignoring cycles).
 * Returns Map<nodeId, depth> where depth 0 = root nodes.
 */
export function computeDepthLevels(
  graph: FlowGraph,
  cycleNodes: Set<string>,
): Map<string, number> {
  const { forward, backward } = buildAdjacency(graph);
  const depths = new Map<string, number>();
  const inDegree = new Map<string, number>();

  // Only consider non-cycle nodes
  const acyclicNodes = graph.nodes.filter((n) => !cycleNodes.has(n.id));

  for (const n of acyclicNodes) {
    // Count incoming edges from non-cycle nodes
    const inEdges = (backward.get(n.id) ?? []).filter((id) => !cycleNodes.has(id));
    inDegree.set(n.id, inEdges.length);
  }

  // BFS layer assignment
  const queue: string[] = [];
  for (const n of acyclicNodes) {
    if ((inDegree.get(n.id) ?? 0) === 0) {
      queue.push(n.id);
      depths.set(n.id, 0);
    }
  }

  while (queue.length > 0) {
    const nodeId = queue.shift()!;
    const depth = depths.get(nodeId) ?? 0;

    for (const child of forward.get(nodeId) ?? []) {
      if (cycleNodes.has(child)) continue;
      const newDeg = (inDegree.get(child) ?? 1) - 1;
      inDegree.set(child, newDeg);
      // Take max depth from all parents
      const existingDepth = depths.get(child) ?? 0;
      depths.set(child, Math.max(existingDepth, depth + 1));
      if (newDeg === 0) {
        queue.push(child);
      }
    }
  }

  return depths;
}

// ── Collapse Detection ─────────────────────────────────────────────────────

/** A group of consecutive agent nodes that can be collapsed into one LLM call. */
export interface CollapseGroup {
  /** Ordered node IDs in the chain */
  nodeIds: string[];
  /** Merged compound prompt */
  mergedPrompt: string;
}

/**
 * Detect chains of consecutive agent nodes that can be collapsed.
 * Rules:
 * - Same agent profile or no agent specified
 * - Same model or no model override
 * - No condition/branch nodes between them
 * - No side-effecting action nodes in the chain
 * - Nodes marked "noCollapse" in config are excluded
 * - Minimum 2 nodes for a collapse to be worthwhile
 */
export function detectCollapseChains(graph: FlowGraph): CollapseGroup[] {
  const { forward, backward } = buildAdjacency(graph);
  const nodeMap = new Map(graph.nodes.map((n) => [n.id, n]));
  const groups: CollapseGroup[] = [];
  const consumed = new Set<string>();

  // Sort nodes topologically for stable traversal
  const visited = new Set<string>();
  const topoOrder: string[] = [];

  function topoVisit(id: string) {
    if (visited.has(id)) return;
    visited.add(id);
    for (const child of forward.get(id) ?? []) {
      topoVisit(child);
    }
    topoOrder.unshift(id);
  }
  for (const n of graph.nodes) topoVisit(n.id);

  for (const startId of topoOrder) {
    if (consumed.has(startId)) continue;

    const startNode = nodeMap.get(startId);
    if (!startNode || startNode.kind !== 'agent') continue;
    if (startNode.config?.noCollapse) continue;

    const startConfig = getNodeExecConfig(startNode);
    const chain: string[] = [startId];

    // Walk forward, extending the chain
    let current = startId;
    while (true) {
      const children = forward.get(current) ?? [];
      // Must have exactly one forward edge to continue the chain
      if (children.length !== 1) break;

      const nextId = children[0];
      const nextNode = nodeMap.get(nextId);
      if (!nextNode) break;

      // Must be an agent node
      if (nextNode.kind !== 'agent') break;
      if (nextNode.config?.noCollapse) break;

      // Must have exactly one parent (no fan-in)
      const parents = backward.get(nextId) ?? [];
      if (parents.length !== 1) break;

      // Must have compatible agent/model config
      const nextConfig = getNodeExecConfig(nextNode);
      if (!isCollapseCompatible(startConfig, nextConfig)) break;

      // Already consumed by another chain
      if (consumed.has(nextId)) break;

      // Already in this chain (cycle back to start)
      if (chain.includes(nextId)) break;

      chain.push(nextId);
      current = nextId;
    }

    // Need at least 2 nodes for a meaningful collapse
    if (chain.length >= 2) {
      for (const id of chain) consumed.add(id);

      const mergedPrompt = buildCollapsedPrompt(chain, nodeMap);
      groups.push({ nodeIds: chain, mergedPrompt });
    }
  }

  return groups;
}

/**
 * Check if two agent configs are compatible for collapsing.
 */
function isCollapseCompatible(a: NodeExecConfig, b: NodeExecConfig): boolean {
  // If both specify an agent, they must be the same
  if (a.agentId && b.agentId && a.agentId !== b.agentId) return false;
  // If both specify a model, they must be the same
  if (a.model && b.model && a.model !== b.model) return false;
  return true;
}

/**
 * Build a merged compound prompt from a chain of agent nodes.
 */
function buildCollapsedPrompt(
  chain: string[],
  nodeMap: Map<string, FlowNode>,
): string {
  const parts: string[] = [];
  parts.push(
    'You are executing a multi-step task. Complete ALL of the following steps in order, providing output for each:',
  );
  parts.push('');

  for (let i = 0; i < chain.length; i++) {
    const node = nodeMap.get(chain[i]);
    if (!node) continue;

    const config = getNodeExecConfig(node);
    const stepLabel = node.label || `Step ${i + 1}`;
    const stepPrompt =
      config.prompt || node.description || `Perform this step: ${stepLabel}`;

    parts.push(`## Step ${i + 1}: ${stepLabel}`);
    parts.push(stepPrompt);
    parts.push('');
  }

  parts.push('---');
  parts.push(
    'Provide your complete response covering all steps above. ' +
      'Separate each step\'s output with a line containing only "---STEP_BOUNDARY---" ' +
      'so the outputs can be parsed back to individual steps.',
  );

  return parts.join('\n');
}

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
export function groupByDepth(
  _graph: FlowGraph,
  depths: Map<string, number>,
): ParallelGroup[] {
  const byDepth = new Map<number, string[]>();

  for (const [nodeId, depth] of depths) {
    if (!byDepth.has(depth)) byDepth.set(depth, []);
    byDepth.get(depth)!.push(nodeId);
  }

  // Sort by depth
  const sorted = [...byDepth.entries()].sort((a, b) => a[0] - b[0]);

  return sorted.map(([depth, nodeIds]) => ({ depth, nodeIds }));
}

/**
 * Check if two nodes at the same depth have a data dependency between them.
 * (Shared upstream or downstream that would create ordering requirements.)
 */
export function hasDataDependency(
  graph: FlowGraph,
  nodeA: string,
  nodeB: string,
): boolean {
  // Direct edge between them
  for (const e of graph.edges) {
    if (
      (e.from === nodeA && e.to === nodeB) ||
      (e.from === nodeB && e.to === nodeA)
    ) {
      return true;
    }
  }
  return false;
}

/**
 * Split a depth group into independent parallel sub-groups.
 * Nodes with mutual dependencies go into the same sub-group (sequential within).
 */
export function splitIntoIndependentGroups(
  graph: FlowGraph,
  nodeIds: string[],
): string[][] {
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

  // Group dependent nodes
  for (let i = 0; i < nodeIds.length; i++) {
    for (let j = i + 1; j < nodeIds.length; j++) {
      if (hasDataDependency(graph, nodeIds[i], nodeIds[j])) {
        union(nodeIds[i], nodeIds[j]);
      }
    }
  }

  // Collect groups
  const groupMap = new Map<string, string[]>();
  for (const id of nodeIds) {
    const root = find(id);
    if (!groupMap.has(root)) groupMap.set(root, []);
    groupMap.get(root)!.push(id);
  }

  return [...groupMap.values()];
}

// ── Convergene Detection ───────────────────────────────────────────────────

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
  // Merge overlapping cycles
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
        // Check overlap
        let overlaps = false;
        for (const id of result[i]) {
          if (result[j].has(id)) {
            overlaps = true;
            break;
          }
        }
        if (overlaps) {
          // Merge j into i
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

// ── Strategy Compiler ──────────────────────────────────────────────────────

/** Adaptive threshold: should we invoke the Conductor for this graph? */
export function shouldUseConductor(graph: FlowGraph): boolean {
  // 4+ nodes
  if (graph.nodes.length >= 4) return true;
  // Any branching (fan-out: node with >1 outgoing edge)
  const outCount = new Map<string, number>();
  for (const e of graph.edges) {
    outCount.set(e.from, (outCount.get(e.from) ?? 0) + 1);
  }
  for (const count of outCount.values()) {
    if (count > 1) return true;
  }
  // Any cycles (bidirectional edges)
  for (const e of graph.edges) {
    if (e.kind === 'bidirectional') return true;
  }
  // Mixed node types (agents + direct actions)
  const hasAgent = graph.nodes.some(
    (n) => classifyNode(n) === 'agent',
  );
  const hasDirect = graph.nodes.some(
    (n) => classifyNode(n) === 'direct',
  );
  if (hasAgent && hasDirect) return true;

  return false;
}

/**
 * Compile an execution strategy from a flow graph.
 * This is the core of the Conductor Protocol — static analysis version.
 * (Phase 2.5 adds AI-based compilation on top of this.)
 */
export function compileStrategy(graph: FlowGraph): ExecutionStrategy {
  const nodeMap = new Map(graph.nodes.map((n) => [n.id, n]));
  let unitCounter = 0;
  const genUnitId = () => `unit_${++unitCounter}`;

  // 1. Detect cycles → mesh configs
  const cycles = detectCycles(graph);
  const allCycleNodes = new Set<string>();
  for (const cycle of cycles) {
    for (const id of cycle) allCycleNodes.add(id);
  }
  const meshConfigs = buildMeshConfigs(cycles);

  // 2. Compute depth levels for acyclic portion
  const depths = computeDepthLevels(graph, allCycleNodes);

  // 3. Detect collapse chains (only among acyclic agent nodes)
  const collapseGroups = detectCollapseChains(graph);
  const collapsedNodes = new Set<string>();
  for (const group of collapseGroups) {
    for (const id of group.nodeIds) collapsedNodes.add(id);
  }

  // 4. Group remaining nodes by depth for parallel execution
  const depthGroups = groupByDepth(graph, depths);

  // 5. Build phases
  const phases: ExecutionPhase[] = [];

  for (const depthGroup of depthGroups) {
    const phaseUnits: ExecutionUnit[] = [];

    // Separate nodes into: collapsed groups, single agents, direct actions, passthroughs
    const remainingNodes = depthGroup.nodeIds.filter(
      (id) => !collapsedNodes.has(id) && !allCycleNodes.has(id),
    );

    // Add collapse group units that start at this depth
    for (const group of collapseGroups) {
      const firstNodeDepth = depths.get(group.nodeIds[0]);
      if (firstNodeDepth === depthGroup.depth) {
        phaseUnits.push({
          id: genUnitId(),
          type: 'collapsed-agent',
          nodeIds: group.nodeIds,
          mergedPrompt: group.mergedPrompt,
          dependsOn: getDependencies(group.nodeIds[0], depths, depthGroup.depth, nodeMap, graph),
        });
      }
    }

    // Split remaining nodes into independent parallel groups
    const independentGroups = splitIntoIndependentGroups(graph, remainingNodes);

    for (const group of independentGroups) {
      for (const nodeId of group) {
        const node = nodeMap.get(nodeId);
        if (!node) continue;

        const classification = classifyNode(node);
        if (classification === 'passthrough') {
          // Passthrough nodes get a direct unit (trigger start signal)
          phaseUnits.push({
            id: genUnitId(),
            type: 'single-direct',
            nodeIds: [nodeId],
            dependsOn: getDependencies(nodeId, depths, depthGroup.depth, nodeMap, graph),
          });
        } else if (classification === 'direct') {
          phaseUnits.push({
            id: genUnitId(),
            type: 'single-direct',
            nodeIds: [nodeId],
            dependsOn: getDependencies(nodeId, depths, depthGroup.depth, nodeMap, graph),
          });
        } else {
          phaseUnits.push({
            id: genUnitId(),
            type: 'single-agent',
            nodeIds: [nodeId],
            dependsOn: getDependencies(nodeId, depths, depthGroup.depth, nodeMap, graph),
          });
        }
      }
    }

    if (phaseUnits.length > 0) {
      phases.push({ index: phases.length, units: phaseUnits });
    }
  }

  // 6. Add mesh phases at the end (or interleaved with dependencies)
  for (const mesh of meshConfigs) {
    phases.push({
      index: phases.length,
      units: [
        {
          id: genUnitId(),
          type: 'mesh',
          nodeIds: mesh.nodeIds,
          maxIterations: mesh.maxIterations,
          dependsOn: [],
        },
      ],
    });
  }

  // 7. Compute estimates
  let llmCalls = 0;
  let directActions = 0;
  for (const phase of phases) {
    for (const unit of phase.units) {
      switch (unit.type) {
        case 'collapsed-agent':
          llmCalls += 1; // N nodes → 1 LLM call
          break;
        case 'single-agent':
          llmCalls += 1;
          break;
        case 'single-direct':
        case 'direct-action':
          directActions += unit.nodeIds.length;
          break;
        case 'mesh':
          // Estimate: each node in mesh × avg 3 rounds
          llmCalls += unit.nodeIds.length * 3;
          break;
      }
    }
  }

  return {
    graphId: graph.id,
    phases,
    totalNodes: graph.nodes.length,
    estimatedLlmCalls: llmCalls,
    estimatedDirectActions: directActions,
    conductorUsed: true,
    meta: {
      collapseGroups: collapseGroups.length,
      parallelPhases: phases.filter((p) => p.units.length > 1).length,
      meshCount: meshConfigs.length,
      extractedNodes: graph.nodes.filter((n) => classifyNode(n) === 'direct').length,
    },
  };
}

/**
 * Get unit IDs that this node depends on (units from the previous phase).
 */
function getDependencies(
  _nodeId: string,
  _depths: Map<string, number>,
  _currentDepth: number,
  _nodeMap: Map<string, FlowNode>,
  _graph: FlowGraph,
): string[] {
  // For now, dependencies are handled implicitly by phase ordering.
  // Units within a phase are independent; phases execute sequentially.
  // This is a simplified model — Phase 2.5 can add explicit dependency tracking.
  return [];
}

/**
 * Build a sequential (non-Conductor) strategy as fallback.
 * Each node gets its own unit, one per phase, in topological order.
 */
export function buildSequentialStrategy(
  graph: FlowGraph,
  plan: string[],
): ExecutionStrategy {
  const nodeMap = new Map(graph.nodes.map((n) => [n.id, n]));
  const phases: ExecutionPhase[] = [];

  for (let i = 0; i < plan.length; i++) {
    const nodeId = plan[i];
    const node = nodeMap.get(nodeId);
    if (!node) continue;

    const classification = classifyNode(node);
    phases.push({
      index: i,
      units: [
        {
          id: `seq_${i}`,
          type: classification === 'agent' ? 'single-agent' : 'single-direct',
          nodeIds: [nodeId],
          dependsOn: i > 0 ? [`seq_${i - 1}`] : [],
        },
      ],
    });
  }

  return {
    graphId: graph.id,
    phases,
    totalNodes: graph.nodes.length,
    estimatedLlmCalls: graph.nodes.filter((n) => classifyNode(n) === 'agent').length,
    estimatedDirectActions: graph.nodes.filter((n) => classifyNode(n) === 'direct').length,
    conductorUsed: false,
    meta: {
      collapseGroups: 0,
      parallelPhases: 0,
      meshCount: 0,
      extractedNodes: 0,
    },
  };
}

/**
 * Parse the output of a collapsed agent call back into individual step outputs.
 * Looks for "---STEP_BOUNDARY---" separators.
 */
export function parseCollapsedOutput(
  output: string,
  nodeCount: number,
): string[] {
  const BOUNDARY = '---STEP_BOUNDARY---';
  const parts = output.split(BOUNDARY).map((s) => s.trim()).filter(Boolean);

  // If we got the right number of parts, great
  if (parts.length === nodeCount) return parts;

  // If we got fewer parts, pad with the last part (or the full output)
  if (parts.length < nodeCount) {
    while (parts.length < nodeCount) {
      parts.push(parts[parts.length - 1] || output);
    }
    return parts;
  }

  // If we got more parts, just take the first nodeCount
  return parts.slice(0, nodeCount);
}

/**
 * Build the Conductor system prompt for Phase 2.5 (AI-compiled strategy).
 */
export function buildConductorPrompt(graph: FlowGraph): string {
  const nodeDescriptions = graph.nodes.map((n) => {
    const config = getNodeExecConfig(n);
    return {
      id: n.id,
      kind: n.kind,
      label: n.label,
      classification: classifyNode(n),
      hasPrompt: !!config.prompt,
      agentId: config.agentId,
      model: config.model,
    };
  });

  const edgeDescriptions = graph.edges.map((e) => ({
    from: e.from,
    to: e.to,
    kind: e.kind,
  }));

  return `You are the Conductor for an AI workflow execution engine. You will receive a
flow graph (JSON) describing a pipeline of AI agent nodes, tool nodes, code
nodes, and action nodes.

Your job is to compile an optimal execution strategy using these primitives:
- COLLAPSE: Merge adjacent agent nodes into a single LLM call
- EXTRACT: Route deterministic nodes (tool, code, http) to direct execution
- PARALLELIZE: Run independent branches concurrently
- CONVERGE: Handle cyclic subgraphs as iterative meshes

## Flow Graph

### Nodes
${JSON.stringify(nodeDescriptions, null, 2)}

### Edges
${JSON.stringify(edgeDescriptions, null, 2)}

## Rules
1. Every node ID must appear in exactly one unit
2. Dependencies must be respected (no unit can depend on a later phase)
3. Deterministic nodes (tool, code, http, mcp-tool) should be EXTRACTED
4. Adjacent agent nodes with compatible configs should be COLLAPSED
5. Independent branches should be PARALLELIZED
6. Cycles should be marked for CONVERGENT MESH execution

Output a JSON ExecutionStrategy with phases, units, and configurations.
Do not execute anything. Only produce the plan.`;
}
