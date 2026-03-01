// ─────────────────────────────────────────────────────────────────────────────
// Conductor Protocol — Collapse Detection Atoms
// Detects chains of compatible agent nodes that can be merged into a single
// LLM call (Collapse primitive). Pure logic, no DOM, no IPC.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowGraph, FlowNode } from './atoms';
import { getNodeExecConfig, type NodeExecConfig } from './executor-atoms';
import { buildAdjacency } from './conductor-graph';

// ── Collapse Group ─────────────────────────────────────────────────────────

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
      if (children.length !== 1) break;

      const nextId = children[0];
      const nextNode = nodeMap.get(nextId);
      if (!nextNode) break;
      if (nextNode.kind !== 'agent') break;
      if (nextNode.config?.noCollapse) break;

      const parents = backward.get(nextId) ?? [];
      if (parents.length !== 1) break;

      const nextConfig = getNodeExecConfig(nextNode);
      if (!isCollapseCompatible(startConfig, nextConfig)) break;
      if (consumed.has(nextId)) break;
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
  if (a.agentId && b.agentId && a.agentId !== b.agentId) return false;
  if (a.model && b.model && a.model !== b.model) return false;
  return true;
}

/**
 * Build a merged compound prompt from a chain of agent nodes.
 */
function buildCollapsedPrompt(chain: string[], nodeMap: Map<string, FlowNode>): string {
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
    const stepPrompt = config.prompt || node.description || `Perform this step: ${stepLabel}`;

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
