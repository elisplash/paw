// ─────────────────────────────────────────────────────────────────────────────
// The Conductor Protocol — Atoms Hub (Re-exports + Strategy Compiler)
// AI-compiled flow execution: Collapse, Extract, Parallelize, Converge.
//
// Sub-modules:
//   conductor-graph.ts    — Node classification, adjacency, cycles, depths
//   conductor-collapse.ts — Collapse chain detection & prompt merging
//   conductor-parallel.ts — Parallel grouping, mesh configs, convergence
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowGraph, FlowNode } from './atoms';
import { getNodeExecConfig } from './executor-atoms';

// Re-export sub-module symbols for backward compatibility
export {
  classifyNode,
  isStructuredCondition,
  buildAdjacency,
  detectCycles,
  computeDepthLevels,
  type NodeExecClassification,
} from './conductor-graph';
export { detectCollapseChains, type CollapseGroup } from './conductor-collapse';
export {
  groupByDepth,
  hasDataDependency,
  splitIntoIndependentGroups,
  buildMeshConfigs,
  textSimilarity,
  checkConvergence,
  type ParallelGroup,
  type MeshConfig,
} from './conductor-parallel';

// Import for internal use in strategy compiler
import { classifyNode, detectCycles, computeDepthLevels } from './conductor-graph';
import { detectCollapseChains } from './conductor-collapse';
import { groupByDepth, splitIntoIndependentGroups, buildMeshConfigs } from './conductor-parallel';

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
  const hasAgent = graph.nodes.some((n) => classifyNode(n) === 'agent');
  const hasDirect = graph.nodes.some((n) => classifyNode(n) === 'direct');
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
export function buildSequentialStrategy(graph: FlowGraph, plan: string[]): ExecutionStrategy {
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
export function parseCollapsedOutput(output: string, nodeCount: number): string[] {
  const BOUNDARY = '---STEP_BOUNDARY---';
  const parts = output
    .split(BOUNDARY)
    .map((s) => s.trim())
    .filter(Boolean);

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
