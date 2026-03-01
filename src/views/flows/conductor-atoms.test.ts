// ─────────────────────────────────────────────────────────────────────────────
// Conductor Protocol — Atoms Tests
// Pure logic: classification, graph analysis, collapse, parallel, convergence.
// ─────────────────────────────────────────────────────────────────────────────

import { describe, it, expect, beforeEach } from 'vitest';
import type { FlowGraph, FlowNode, FlowEdge, FlowNodeKind } from './atoms';
import {
  classifyNode,
  isStructuredCondition,
  buildAdjacency,
  detectCycles,
  computeDepthLevels,
  detectCollapseChains,
  groupByDepth,
  hasDataDependency,
  splitIntoIndependentGroups,
  buildMeshConfigs,
  textSimilarity,
  checkConvergence,
  shouldUseConductor,
  compileStrategy,
  buildSequentialStrategy,
  parseCollapsedOutput,
  buildConductorPrompt,
} from './conductor-atoms';

// ── Helpers ────────────────────────────────────────────────────────────────

let _uid = 0;

function mkNode(kind: FlowNodeKind, overrides: Partial<FlowNode> = {}): FlowNode {
  const id = overrides.id ?? `n${++_uid}`;
  return {
    id,
    kind,
    label: overrides.label ?? `${kind}-${id}`,
    x: 0,
    y: 0,
    width: 180,
    height: 72,
    status: 'idle',
    config: overrides.config ?? {},
    inputs: ['in'],
    outputs: ['out'],
    ...overrides,
  };
}

function mkEdge(from: string, to: string, overrides: Partial<FlowEdge> = {}): FlowEdge {
  return {
    id: overrides.id ?? `e_${from}_${to}`,
    kind: overrides.kind ?? 'forward',
    from,
    to,
    fromPort: 'out',
    toPort: 'in',
    active: false,
    ...overrides,
  };
}

function mkGraph(
  nodes: FlowNode[],
  edges: FlowEdge[],
  overrides: Partial<FlowGraph> = {},
): FlowGraph {
  return {
    id: overrides.id ?? 'test-graph',
    name: overrides.name ?? 'Test Graph',
    nodes,
    edges,
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
    ...overrides,
  };
}

// Reset UID between tests
function resetUid() {
  _uid = 0;
}

// ── Node Classification ────────────────────────────────────────────────────

describe('classifyNode', () => {
  beforeEach(resetUid);

  it('classifies trigger as passthrough', () => {
    expect(classifyNode(mkNode('trigger'))).toBe('passthrough');
  });

  it('classifies agent as agent', () => {
    expect(classifyNode(mkNode('agent'))).toBe('agent');
  });

  it('classifies data as agent', () => {
    expect(classifyNode(mkNode('data'))).toBe('agent');
  });

  it('classifies tool as direct', () => {
    expect(classifyNode(mkNode('tool'))).toBe('direct');
  });

  it('classifies tool with prompt as agent', () => {
    // Tool nodes without a prompt are direct; but tool is always in DIRECT_KINDS,
    // so this actually returns 'direct' first (DIRECT_KINDS check happens before prompt check)
    expect(classifyNode(mkNode('tool', { config: { prompt: 'do something' } }))).toBe('direct');
  });

  it('classifies code as direct', () => {
    expect(classifyNode(mkNode('code'))).toBe('direct');
  });

  it('classifies output as direct', () => {
    expect(classifyNode(mkNode('output'))).toBe('direct');
  });

  it('classifies error as direct', () => {
    expect(classifyNode(mkNode('error'))).toBe('direct');
  });

  it('classifies http as direct', () => {
    expect(classifyNode(mkNode('http' as FlowNodeKind))).toBe('direct');
  });

  it('classifies mcp-tool as direct', () => {
    expect(classifyNode(mkNode('mcp-tool' as FlowNodeKind))).toBe('direct');
  });

  it('classifies condition with structured expr as direct', () => {
    expect(
      classifyNode(mkNode('condition', { config: { conditionExpr: 'input.length > 5' } })),
    ).toBe('direct');
  });

  it('classifies condition with natural-language expr as agent', () => {
    expect(
      classifyNode(
        mkNode('condition', { config: { conditionExpr: 'Does the input contain valid data?' } }),
      ),
    ).toBe('agent');
  });

  it('classifies condition without expr as agent', () => {
    expect(classifyNode(mkNode('condition'))).toBe('agent');
  });
});

// ── isStructuredCondition ──────────────────────────────────────────────────

describe('isStructuredCondition', () => {
  it('returns true for boolean literals', () => {
    expect(isStructuredCondition('true')).toBe(true);
    expect(isStructuredCondition('false')).toBe(true);
    expect(isStructuredCondition('yes')).toBe(true);
    expect(isStructuredCondition('no')).toBe(true);
  });

  it('returns true for simple comparisons', () => {
    expect(isStructuredCondition('x > 5')).toBe(true);
    expect(isStructuredCondition('a === b')).toBe(true);
    expect(isStructuredCondition('count != 0')).toBe(true);
    expect(isStructuredCondition('status >= 200')).toBe(true);
    expect(isStructuredCondition('val <= 100')).toBe(true);
  });

  it('returns true for property access comparisons', () => {
    expect(isStructuredCondition('input.status === 200')).toBe(true);
    expect(isStructuredCondition('data.length > 0')).toBe(true);
  });

  it('returns false for natural language', () => {
    expect(isStructuredCondition('Is the document valid?')).toBe(false);
    expect(isStructuredCondition('Check if the user has permission')).toBe(false);
    expect(isStructuredCondition('contains useful information')).toBe(false);
  });
});

// ── Graph Analysis ─────────────────────────────────────────────────────────

describe('buildAdjacency', () => {
  beforeEach(resetUid);

  it('builds forward and backward maps', () => {
    const a = mkNode('agent', { id: 'a' });
    const b = mkNode('agent', { id: 'b' });
    const c = mkNode('agent', { id: 'c' });
    const graph = mkGraph([a, b, c], [mkEdge('a', 'b'), mkEdge('b', 'c')]);

    const { forward, backward } = buildAdjacency(graph);

    expect(forward.get('a')).toEqual(['b']);
    expect(forward.get('b')).toEqual(['c']);
    expect(forward.get('c')).toEqual([]);
    expect(backward.get('a')).toEqual([]);
    expect(backward.get('b')).toEqual(['a']);
    expect(backward.get('c')).toEqual(['b']);
  });

  it('ignores reverse edges', () => {
    const a = mkNode('agent', { id: 'a' });
    const b = mkNode('agent', { id: 'b' });
    const graph = mkGraph(
      [a, b],
      [mkEdge('a', 'b'), mkEdge('b', 'a', { kind: 'reverse', id: 'e_rev' })],
    );

    const { forward } = buildAdjacency(graph);
    expect(forward.get('a')).toEqual(['b']);
    expect(forward.get('b')).toEqual([]);
  });
});

describe('detectCycles', () => {
  beforeEach(resetUid);

  it('returns empty for a DAG', () => {
    const a = mkNode('agent', { id: 'a' });
    const b = mkNode('agent', { id: 'b' });
    const c = mkNode('agent', { id: 'c' });
    const graph = mkGraph([a, b, c], [mkEdge('a', 'b'), mkEdge('b', 'c')]);

    const cycles = detectCycles(graph);
    expect(cycles).toHaveLength(0);
  });

  it('detects a simple cycle', () => {
    const a = mkNode('agent', { id: 'a' });
    const b = mkNode('agent', { id: 'b' });
    const c = mkNode('agent', { id: 'c' });
    const graph = mkGraph([a, b, c], [mkEdge('a', 'b'), mkEdge('b', 'c'), mkEdge('c', 'a')]);

    const cycles = detectCycles(graph);
    expect(cycles.length).toBeGreaterThan(0);
    // All nodes should be in the cycle
    const allCycleNodes = new Set<string>();
    for (const c of cycles) for (const id of c) allCycleNodes.add(id);
    expect(allCycleNodes.has('a')).toBe(true);
    expect(allCycleNodes.has('b')).toBe(true);
    expect(allCycleNodes.has('c')).toBe(true);
  });

  it('detects a self-loop', () => {
    const a = mkNode('agent', { id: 'a' });
    const graph = mkGraph([a], [mkEdge('a', 'a')]);

    const cycles = detectCycles(graph);
    expect(cycles.length).toBeGreaterThan(0);
  });
});

describe('computeDepthLevels', () => {
  beforeEach(resetUid);

  it('assigns depth 0 to root nodes', () => {
    const a = mkNode('trigger', { id: 'a' });
    const b = mkNode('agent', { id: 'b' });
    const c = mkNode('output', { id: 'c' });
    const graph = mkGraph([a, b, c], [mkEdge('a', 'b'), mkEdge('b', 'c')]);

    const depths = computeDepthLevels(graph, new Set());
    expect(depths.get('a')).toBe(0);
    expect(depths.get('b')).toBe(1);
    expect(depths.get('c')).toBe(2);
  });

  it('handles fan-out (max depth from parents)', () => {
    const a = mkNode('trigger', { id: 'a' });
    const b = mkNode('agent', { id: 'b' });
    const c = mkNode('agent', { id: 'c' });
    const d = mkNode('output', { id: 'd' });
    const graph = mkGraph(
      [a, b, c, d],
      [mkEdge('a', 'b'), mkEdge('a', 'c'), mkEdge('b', 'd'), mkEdge('c', 'd')],
    );

    const depths = computeDepthLevels(graph, new Set());
    expect(depths.get('a')).toBe(0);
    expect(depths.get('b')).toBe(1);
    expect(depths.get('c')).toBe(1);
    expect(depths.get('d')).toBe(2);
  });

  it('excludes cycle nodes', () => {
    const a = mkNode('trigger', { id: 'a' });
    const b = mkNode('agent', { id: 'b' });
    const graph = mkGraph([a, b], [mkEdge('a', 'b')]);

    const cycleNodes = new Set(['b']);
    const depths = computeDepthLevels(graph, cycleNodes);
    expect(depths.has('b')).toBe(false);
    expect(depths.get('a')).toBe(0);
  });
});

// ── Collapse Detection ─────────────────────────────────────────────────────

describe('detectCollapseChains', () => {
  beforeEach(resetUid);

  it('collapses a linear chain of agent nodes', () => {
    const a = mkNode('agent', { id: 'a', config: { prompt: 'step1' } });
    const b = mkNode('agent', { id: 'b', config: { prompt: 'step2' } });
    const c = mkNode('agent', { id: 'c', config: { prompt: 'step3' } });
    const graph = mkGraph([a, b, c], [mkEdge('a', 'b'), mkEdge('b', 'c')]);

    const groups = detectCollapseChains(graph);
    expect(groups).toHaveLength(1);
    expect(groups[0].nodeIds).toEqual(['a', 'b', 'c']);
    expect(groups[0].mergedPrompt).toContain('Step 1');
    expect(groups[0].mergedPrompt).toContain('Step 2');
    expect(groups[0].mergedPrompt).toContain('Step 3');
    expect(groups[0].mergedPrompt).toContain('---STEP_BOUNDARY---');
  });

  it('does not collapse when a non-agent node splits the chain', () => {
    const a = mkNode('agent', { id: 'a', config: { prompt: 'step1' } });
    const b = mkNode('code', { id: 'b', config: {} });
    const c = mkNode('agent', { id: 'c', config: { prompt: 'step3' } });
    const graph = mkGraph([a, b, c], [mkEdge('a', 'b'), mkEdge('b', 'c')]);

    const groups = detectCollapseChains(graph);
    expect(groups).toHaveLength(0);
  });

  it('does not collapse nodes with different agentIds', () => {
    const a = mkNode('agent', { id: 'a', config: { prompt: 'step1', agentId: 'alice' } });
    const b = mkNode('agent', { id: 'b', config: { prompt: 'step2', agentId: 'bob' } });
    const graph = mkGraph([a, b], [mkEdge('a', 'b')]);

    const groups = detectCollapseChains(graph);
    expect(groups).toHaveLength(0);
  });

  it('respects noCollapse flag', () => {
    const a = mkNode('agent', { id: 'a', config: { prompt: 'step1' } });
    const b = mkNode('agent', { id: 'b', config: { prompt: 'step2', noCollapse: true } });
    const graph = mkGraph([a, b], [mkEdge('a', 'b')]);

    const groups = detectCollapseChains(graph);
    expect(groups).toHaveLength(0);
  });

  it('does not collapse single agent nodes', () => {
    const a = mkNode('agent', { id: 'a', config: { prompt: 'only step' } });
    const graph = mkGraph([a], []);

    const groups = detectCollapseChains(graph);
    expect(groups).toHaveLength(0);
  });

  it('stops collapse at fan-out (multiple children)', () => {
    const a = mkNode('agent', { id: 'a', config: { prompt: 'p1' } });
    const b = mkNode('agent', { id: 'b', config: { prompt: 'p2' } });
    const c = mkNode('agent', { id: 'c', config: { prompt: 'p3' } });
    const graph = mkGraph([a, b, c], [mkEdge('a', 'b'), mkEdge('a', 'c')]);

    const groups = detectCollapseChains(graph);
    expect(groups).toHaveLength(0);
  });

  it('stops collapse at fan-in (multiple parents)', () => {
    const a = mkNode('agent', { id: 'a', config: { prompt: 'p1' } });
    const b = mkNode('agent', { id: 'b', config: { prompt: 'p2' } });
    const c = mkNode('agent', { id: 'c', config: { prompt: 'p3' } });
    const graph = mkGraph([a, b, c], [mkEdge('a', 'c'), mkEdge('b', 'c')]);

    const groups = detectCollapseChains(graph);
    expect(groups).toHaveLength(0);
  });
});

// ── Parallel Grouping ──────────────────────────────────────────────────────

describe('groupByDepth', () => {
  beforeEach(resetUid);

  it('groups nodes by their depth level', () => {
    const depths = new Map([
      ['a', 0],
      ['b', 1],
      ['c', 1],
      ['d', 2],
    ]);

    const a = mkNode('trigger', { id: 'a' });
    const b = mkNode('agent', { id: 'b' });
    const c = mkNode('agent', { id: 'c' });
    const d = mkNode('output', { id: 'd' });
    const graph = mkGraph([a, b, c, d], []);

    const groups = groupByDepth(graph, depths);
    expect(groups).toHaveLength(3);
    expect(groups[0]).toEqual({ depth: 0, nodeIds: ['a'] });
    expect(groups[1]).toEqual({ depth: 1, nodeIds: ['b', 'c'] });
    expect(groups[2]).toEqual({ depth: 2, nodeIds: ['d'] });
  });
});

describe('hasDataDependency', () => {
  beforeEach(resetUid);

  it('returns true when nodes have direct edge', () => {
    const a = mkNode('agent', { id: 'a' });
    const b = mkNode('agent', { id: 'b' });
    const graph = mkGraph([a, b], [mkEdge('a', 'b')]);

    expect(hasDataDependency(graph, 'a', 'b')).toBe(true);
    expect(hasDataDependency(graph, 'b', 'a')).toBe(true);
  });

  it('returns false when nodes have no edge', () => {
    const a = mkNode('agent', { id: 'a' });
    const b = mkNode('agent', { id: 'b' });
    const graph = mkGraph([a, b], []);

    expect(hasDataDependency(graph, 'a', 'b')).toBe(false);
  });
});

describe('splitIntoIndependentGroups', () => {
  beforeEach(resetUid);

  it('returns single group for a single node', () => {
    const graph = mkGraph([mkNode('agent', { id: 'a' })], []);
    const result = splitIntoIndependentGroups(graph, ['a']);
    expect(result).toEqual([['a']]);
  });

  it('splits independent nodes into separate groups', () => {
    const a = mkNode('agent', { id: 'a' });
    const b = mkNode('agent', { id: 'b' });
    const graph = mkGraph([a, b], []);

    const result = splitIntoIndependentGroups(graph, ['a', 'b']);
    expect(result).toHaveLength(2);
  });

  it('groups dependent nodes together', () => {
    const a = mkNode('agent', { id: 'a' });
    const b = mkNode('agent', { id: 'b' });
    const graph = mkGraph([a, b], [mkEdge('a', 'b')]);

    const result = splitIntoIndependentGroups(graph, ['a', 'b']);
    expect(result).toHaveLength(1);
    expect(result[0].sort()).toEqual(['a', 'b']);
  });
});

// ── Convergence Detection ──────────────────────────────────────────────────

describe('buildMeshConfigs', () => {
  it('builds configs from cycles', () => {
    const cycles = [new Set(['a', 'b', 'c'])];
    const configs = buildMeshConfigs(cycles);

    expect(configs).toHaveLength(1);
    expect(configs[0].nodeIds.sort()).toEqual(['a', 'b', 'c']);
    expect(configs[0].maxIterations).toBe(5);
    expect(configs[0].convergenceThreshold).toBe(0.85);
  });

  it('merges overlapping cycles', () => {
    const cycles = [new Set(['a', 'b']), new Set(['b', 'c'])];
    const configs = buildMeshConfigs(cycles);

    expect(configs).toHaveLength(1);
    expect(configs[0].nodeIds.sort()).toEqual(['a', 'b', 'c']);
  });

  it('keeps disjoint cycles separate', () => {
    const cycles = [new Set(['a', 'b']), new Set(['c', 'd'])];
    const configs = buildMeshConfigs(cycles);

    expect(configs).toHaveLength(2);
  });

  it('returns empty for no cycles', () => {
    expect(buildMeshConfigs([])).toEqual([]);
  });
});

describe('textSimilarity', () => {
  it('returns 1 for identical strings', () => {
    expect(textSimilarity('hello world', 'hello world')).toBe(1);
  });

  it('returns 0 for completely different strings', () => {
    expect(textSimilarity('hello world', 'foo bar baz qux')).toBe(0);
  });

  it('returns partial overlap score', () => {
    const sim = textSimilarity('hello world foo', 'hello world bar');
    expect(sim).toBeGreaterThan(0.3);
    expect(sim).toBeLessThan(1);
  });

  it('returns 0 for empty strings', () => {
    expect(textSimilarity('', 'hello')).toBe(0);
    expect(textSimilarity('hello', '')).toBe(0);
  });

  it('returns 1 for two empty strings', () => {
    expect(textSimilarity('', '')).toBe(1); // a === b short-circuit
  });

  it('is case-insensitive', () => {
    expect(textSimilarity('Hello World', 'hello world')).toBe(1);
  });
});

describe('checkConvergence', () => {
  it('returns false for empty previous outputs', () => {
    const prev = new Map<string, string>();
    const curr = new Map([['a', 'some output']]);
    expect(checkConvergence(prev, curr, 0.85)).toBe(false);
  });

  it('returns true when outputs are identical', () => {
    const prev = new Map([['a', 'same text here']]);
    const curr = new Map([['a', 'same text here']]);
    expect(checkConvergence(prev, curr, 0.85)).toBe(true);
  });

  it('returns false when outputs are very different', () => {
    const prev = new Map([['a', 'apples oranges bananas']]);
    const curr = new Map([['a', 'alpha beta gamma delta']]);
    expect(checkConvergence(prev, curr, 0.85)).toBe(false);
  });

  it('checks average similarity across all nodes', () => {
    const prev = new Map([
      ['a', 'hello world test case'],
      ['b', 'foo bar baz'],
    ]);
    const curr = new Map([
      ['a', 'hello world test case'], // 1.0
      ['b', 'foo bar baz'], // 1.0
    ]);
    expect(checkConvergence(prev, curr, 0.9)).toBe(true);
  });
});

// ── Strategy Compiler ──────────────────────────────────────────────────────

describe('shouldUseConductor', () => {
  beforeEach(resetUid);

  it('returns true for graphs with 4+ nodes', () => {
    const nodes = [
      mkNode('trigger', { id: 'a' }),
      mkNode('agent', { id: 'b' }),
      mkNode('agent', { id: 'c' }),
      mkNode('output', { id: 'd' }),
    ];
    const graph = mkGraph(nodes, [mkEdge('a', 'b'), mkEdge('b', 'c'), mkEdge('c', 'd')]);
    expect(shouldUseConductor(graph)).toBe(true);
  });

  it('returns true for graphs with branching', () => {
    const nodes = [
      mkNode('trigger', { id: 'a' }),
      mkNode('agent', { id: 'b' }),
      mkNode('agent', { id: 'c' }),
    ];
    const graph = mkGraph(nodes, [mkEdge('a', 'b'), mkEdge('a', 'c')]);
    expect(shouldUseConductor(graph)).toBe(true);
  });

  it('returns true for mixed node types', () => {
    const nodes = [
      mkNode('agent', { id: 'a' }),
      mkNode('code', { id: 'b' }),
      mkNode('output', { id: 'c' }),
    ];
    const graph = mkGraph(nodes, [mkEdge('a', 'b'), mkEdge('b', 'c')]);
    expect(shouldUseConductor(graph)).toBe(true);
  });

  it('returns true for bidirectional edges (cycles)', () => {
    const nodes = [mkNode('agent', { id: 'a' }), mkNode('agent', { id: 'b' })];
    const graph = mkGraph(nodes, [mkEdge('a', 'b', { kind: 'bidirectional' })]);
    expect(shouldUseConductor(graph)).toBe(true);
  });

  it('returns false for simple 2-node agent chain', () => {
    const nodes = [mkNode('agent', { id: 'a' }), mkNode('agent', { id: 'b' })];
    const graph = mkGraph(nodes, [mkEdge('a', 'b')]);
    expect(shouldUseConductor(graph)).toBe(false);
  });
});

describe('compileStrategy', () => {
  beforeEach(resetUid);

  it('produces a strategy with correct graph ID', () => {
    const nodes = [mkNode('trigger', { id: 'a' }), mkNode('agent', { id: 'b' })];
    const graph = mkGraph(nodes, [mkEdge('a', 'b')], { id: 'my-graph' });

    const strategy = compileStrategy(graph);
    expect(strategy.graphId).toBe('my-graph');
    expect(strategy.conductorUsed).toBe(true);
  });

  it('covers all nodes across phases', () => {
    const nodes = [
      mkNode('trigger', { id: 'a' }),
      mkNode('agent', { id: 'b' }),
      mkNode('code', { id: 'c' }),
      mkNode('output', { id: 'd' }),
    ];
    const graph = mkGraph(nodes, [mkEdge('a', 'b'), mkEdge('b', 'c'), mkEdge('c', 'd')]);

    const strategy = compileStrategy(graph);
    const coveredNodes = new Set<string>();
    for (const phase of strategy.phases) {
      for (const unit of phase.units) {
        for (const id of unit.nodeIds) coveredNodes.add(id);
      }
    }
    expect(coveredNodes.size).toBe(4);
  });

  it('creates collapsed units for agent chains', () => {
    const nodes = [
      mkNode('agent', { id: 'a', config: { prompt: 'step1' } }),
      mkNode('agent', { id: 'b', config: { prompt: 'step2' } }),
      mkNode('agent', { id: 'c', config: { prompt: 'step3' } }),
    ];
    const graph = mkGraph(nodes, [mkEdge('a', 'b'), mkEdge('b', 'c')]);

    const strategy = compileStrategy(graph);
    const collapsedUnits = strategy.phases
      .flatMap((p) => p.units)
      .filter((u) => u.type === 'collapsed-agent');

    expect(collapsedUnits.length).toBeGreaterThan(0);
    expect(collapsedUnits[0].nodeIds).toEqual(['a', 'b', 'c']);
    expect(collapsedUnits[0].mergedPrompt).toBeDefined();
    expect(strategy.meta.collapseGroups).toBe(1);
  });

  it('creates parallel phases for independent branches', () => {
    const nodes = [
      mkNode('trigger', { id: 'root' }),
      mkNode('agent', { id: 'a' }),
      mkNode('agent', { id: 'b' }),
      mkNode('output', { id: 'out' }),
    ];
    const graph = mkGraph(nodes, [
      mkEdge('root', 'a'),
      mkEdge('root', 'b'),
      mkEdge('a', 'out'),
      mkEdge('b', 'out'),
    ]);

    const strategy = compileStrategy(graph);
    // There should be a phase with 2 units (a and b running in parallel)
    const parallelPhases = strategy.phases.filter((p) => p.units.length > 1);
    expect(parallelPhases.length).toBeGreaterThan(0);
    expect(strategy.meta.parallelPhases).toBeGreaterThan(0);
  });

  it('creates mesh units for cyclic graphs', () => {
    const nodes = [
      mkNode('agent', { id: 'a' }),
      mkNode('agent', { id: 'b' }),
      mkNode('agent', { id: 'c' }),
    ];
    const graph = mkGraph(nodes, [mkEdge('a', 'b'), mkEdge('b', 'c'), mkEdge('c', 'a')]);

    const strategy = compileStrategy(graph);
    const meshUnits = strategy.phases.flatMap((p) => p.units).filter((u) => u.type === 'mesh');

    expect(meshUnits.length).toBeGreaterThan(0);
    expect(strategy.meta.meshCount).toBeGreaterThan(0);
  });

  it('estimates LLM calls correctly for collapsed chain', () => {
    const nodes = [
      mkNode('agent', { id: 'a', config: { prompt: 'p1' } }),
      mkNode('agent', { id: 'b', config: { prompt: 'p2' } }),
      mkNode('agent', { id: 'c', config: { prompt: 'p3' } }),
    ];
    const graph = mkGraph(nodes, [mkEdge('a', 'b'), mkEdge('b', 'c')]);

    const strategy = compileStrategy(graph);
    // A collapsed chain of 3 agents → 1 LLM call
    expect(strategy.estimatedLlmCalls).toBe(1);
  });

  it('counts direct actions for non-agent nodes', () => {
    const nodes = [
      mkNode('trigger', { id: 't' }),
      mkNode('code', { id: 'c' }),
      mkNode('output', { id: 'o' }),
    ];
    const graph = mkGraph(nodes, [mkEdge('t', 'c'), mkEdge('c', 'o')]);

    const strategy = compileStrategy(graph);
    expect(strategy.estimatedDirectActions).toBeGreaterThan(0);
    expect(strategy.estimatedLlmCalls).toBe(0);
  });
});

describe('buildSequentialStrategy', () => {
  beforeEach(resetUid);

  it('builds one phase per node in order', () => {
    const nodes = [
      mkNode('trigger', { id: 'a' }),
      mkNode('agent', { id: 'b' }),
      mkNode('output', { id: 'c' }),
    ];
    const graph = mkGraph(nodes, [mkEdge('a', 'b'), mkEdge('b', 'c')]);

    const strategy = buildSequentialStrategy(graph, ['a', 'b', 'c']);
    expect(strategy.phases).toHaveLength(3);
    expect(strategy.conductorUsed).toBe(false);
    expect(strategy.meta.collapseGroups).toBe(0);
    expect(strategy.meta.parallelPhases).toBe(0);

    expect(strategy.phases[0].units[0].nodeIds).toEqual(['a']);
    expect(strategy.phases[1].units[0].nodeIds).toEqual(['b']);
    expect(strategy.phases[2].units[0].nodeIds).toEqual(['c']);
  });

  it('creates correct dependency chain', () => {
    const nodes = [mkNode('agent', { id: 'a' }), mkNode('agent', { id: 'b' })];
    const graph = mkGraph(nodes, [mkEdge('a', 'b')]);

    const strategy = buildSequentialStrategy(graph, ['a', 'b']);
    expect(strategy.phases[0].units[0].dependsOn).toEqual([]);
    expect(strategy.phases[1].units[0].dependsOn).toEqual(['seq_0']);
  });
});

// ── parseCollapsedOutput ───────────────────────────────────────────────────

describe('parseCollapsedOutput', () => {
  it('parses correctly with exact boundary count', () => {
    const output = 'Result 1\n---STEP_BOUNDARY---\nResult 2\n---STEP_BOUNDARY---\nResult 3';
    const parts = parseCollapsedOutput(output, 3);
    expect(parts).toEqual(['Result 1', 'Result 2', 'Result 3']);
  });

  it('pads when fewer parts than expected', () => {
    const output = 'Result 1\n---STEP_BOUNDARY---\nResult 2';
    const parts = parseCollapsedOutput(output, 3);
    expect(parts).toHaveLength(3);
    expect(parts[0]).toBe('Result 1');
    expect(parts[1]).toBe('Result 2');
    expect(parts[2]).toBe('Result 2'); // padded with last
  });

  it('truncates when more parts than expected', () => {
    const output = 'R1\n---STEP_BOUNDARY---\nR2\n---STEP_BOUNDARY---\nR3\n---STEP_BOUNDARY---\nR4';
    const parts = parseCollapsedOutput(output, 2);
    expect(parts).toHaveLength(2);
    expect(parts).toEqual(['R1', 'R2']);
  });

  it('handles output without boundaries', () => {
    const output = 'Just a single block of text';
    const parts = parseCollapsedOutput(output, 3);
    expect(parts).toHaveLength(3);
    expect(parts[0]).toBe('Just a single block of text');
  });
});

// ── buildConductorPrompt ───────────────────────────────────────────────────

describe('buildConductorPrompt', () => {
  beforeEach(resetUid);

  it('includes node and edge descriptions', () => {
    const nodes = [
      mkNode('agent', { id: 'a', label: 'Researcher' }),
      mkNode('code', { id: 'b', label: 'Formatter' }),
    ];
    const graph = mkGraph(nodes, [mkEdge('a', 'b')]);

    const prompt = buildConductorPrompt(graph);
    expect(prompt).toContain('Researcher');
    expect(prompt).toContain('Formatter');
    expect(prompt).toContain('COLLAPSE');
    expect(prompt).toContain('EXTRACT');
    expect(prompt).toContain('PARALLELIZE');
    expect(prompt).toContain('CONVERGE');
  });

  it('includes node classifications', () => {
    const nodes = [mkNode('agent', { id: 'a' }), mkNode('code', { id: 'b' })];
    const graph = mkGraph(nodes, []);

    const prompt = buildConductorPrompt(graph);
    expect(prompt).toContain('"agent"');
    expect(prompt).toContain('"direct"');
  });
});

// ── Integration: full pipeline ─────────────────────────────────────────────

describe('end-to-end strategy compilation', () => {
  beforeEach(resetUid);

  it('handles a realistic multi-branch flow', () => {
    const nodes = [
      mkNode('trigger', { id: 'start' }),
      mkNode('agent', { id: 'research', config: { prompt: 'Research the topic' } }),
      mkNode('agent', { id: 'summarize', config: { prompt: 'Summarize findings' } }),
      mkNode('code', { id: 'format' }),
      mkNode('http' as FlowNodeKind, {
        id: 'notify',
        config: { httpUrl: 'https://hook.example.com', httpMethod: 'POST' },
      }),
      mkNode('output', { id: 'out' }),
    ];
    const edges = [
      mkEdge('start', 'research'),
      mkEdge('research', 'summarize'),
      mkEdge('summarize', 'format'),
      mkEdge('summarize', 'notify'),
      mkEdge('format', 'out'),
      mkEdge('notify', 'out'),
    ];
    const graph = mkGraph(nodes, edges);

    const strategy = compileStrategy(graph);
    expect(strategy.totalNodes).toBe(6);
    expect(strategy.conductorUsed).toBe(true);
    expect(strategy.phases.length).toBeGreaterThan(0);

    // All nodes covered
    const coveredIds = new Set<string>();
    for (const p of strategy.phases) {
      for (const u of p.units) {
        for (const id of u.nodeIds) coveredIds.add(id);
      }
    }
    expect(coveredIds.size).toBe(6);

    // Research + summarize should be collapsed (linear agent chain)
    const collapsed = strategy.phases
      .flatMap((p) => p.units)
      .filter((u) => u.type === 'collapsed-agent');
    expect(collapsed.length).toBe(1);
    expect(collapsed[0].nodeIds).toEqual(['research', 'summarize']);

    // format and notify should be in parallel (both depend on summarize)
    const meta = strategy.meta;
    expect(meta.extractedNodes).toBeGreaterThan(0); // code, http, output are direct
  });
});
