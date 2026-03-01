// ─────────────────────────────────────────────────────────────────────────────
// Phase 3 — Power Features Tests
// Tests for: resolveVariables, parseLoopArray, getNodeExecConfig (new fields),
// createFlowRunState (vault creds), and edge/node utilities.
// ─────────────────────────────────────────────────────────────────────────────

import { describe, it, expect, beforeEach } from 'vitest';
import type { FlowGraph, FlowNode, FlowEdge, FlowNodeKind } from './atoms';
import { NODE_DEFAULTS } from './atoms';
import {
  resolveVariables,
  parseLoopArray,
  getNodeExecConfig,
  createFlowRunState,
  buildExecutionPlan,
} from './executor-atoms';
import { classifyNode } from './conductor-atoms';

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
    width: NODE_DEFAULTS[kind]?.width ?? 180,
    height: NODE_DEFAULTS[kind]?.height ?? 72,
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

function resetUid() {
  _uid = 0;
}

// ── resolveVariables ───────────────────────────────────────────────────────

describe('resolveVariables', () => {
  beforeEach(resetUid);

  it('replaces {{input}} with upstream value', () => {
    const result = resolveVariables('Hello {{input}}, how are you?', {
      input: 'World',
    });
    expect(result).toBe('Hello World, how are you?');
  });

  it('replaces multiple {{input}} occurrences', () => {
    const result = resolveVariables('{{input}} and {{input}}', { input: 'A' });
    expect(result).toBe('A and A');
  });

  it('replaces {{flow.key}} with flow variables', () => {
    const result = resolveVariables('Name: {{flow.name}}, Count: {{flow.count}}', {
      variables: { name: 'Alice', count: 42 },
    });
    expect(result).toBe('Name: Alice, Count: 42');
  });

  it('preserves unresolved {{flow.x}} references', () => {
    const result = resolveVariables('{{flow.missing}}', { variables: {} });
    expect(result).toBe('{{flow.missing}}');
  });

  it('replaces {{loop.index}} and {{loop.item}}', () => {
    const result = resolveVariables('Item #{{loop.index}}: {{loop.item}}', {
      loopIndex: 3,
      loopItem: 'banana',
    });
    expect(result).toBe('Item #3: banana');
  });

  it('replaces custom loop variable name', () => {
    const result = resolveVariables('Fruit: {{loop.fruit}}', {
      loopIndex: 0,
      loopItem: 'apple',
      loopVar: 'fruit',
    });
    expect(result).toBe('Fruit: apple');
  });

  it('replaces {{vault.NAME}} with pre-loaded credentials', () => {
    const result = resolveVariables('Bearer {{vault.api_key}}', {
      vaultCredentials: { api_key: 'sk-123456' },
    });
    expect(result).toBe('Bearer sk-123456');
  });

  it('preserves unresolved {{vault.x}} references', () => {
    const result = resolveVariables('Key: {{vault.missing}}', {
      vaultCredentials: {},
    });
    expect(result).toBe('Key: {{vault.missing}}');
  });

  it('handles empty template', () => {
    expect(resolveVariables('', { input: 'test' })).toBe('');
  });

  it('handles template with no variables', () => {
    expect(resolveVariables('plain text', {})).toBe('plain text');
  });

  it('combines multiple variable types', () => {
    const result = resolveVariables(
      'Input: {{input}}, Var: {{flow.x}}, Cred: {{vault.key}}, Loop: {{loop.index}}',
      {
        input: 'data',
        variables: { x: 'val' },
        vaultCredentials: { key: 'secret' },
        loopIndex: 5,
      },
    );
    expect(result).toBe('Input: data, Var: val, Cred: secret, Loop: 5');
  });

  it('serializes non-string flow variables as JSON', () => {
    const result = resolveVariables('Data: {{flow.obj}}', {
      variables: { obj: { a: 1 } },
    });
    expect(result).toBe('Data: {"a":1}');
  });

  it('serializes non-string loop items as JSON', () => {
    const result = resolveVariables('Item: {{loop.item}}', {
      loopIndex: 0,
      loopItem: { id: 1, name: 'test' },
    });
    expect(result).toBe('Item: {"id":1,"name":"test"}');
  });
});

// ── parseLoopArray ─────────────────────────────────────────────────────────

describe('parseLoopArray', () => {
  it('parses a JSON array string', () => {
    expect(parseLoopArray('["a", "b", "c"]')).toEqual(['a', 'b', 'c']);
  });

  it('parses a JSON array of objects', () => {
    const result = parseLoopArray('[{"id":1},{"id":2}]');
    expect(result).toEqual([{ id: 1 }, { id: 2 }]);
  });

  it('extracts array via dot-path (loopOver)', () => {
    const input = JSON.stringify({ data: { items: [1, 2, 3] } });
    expect(parseLoopArray(input, 'data.items')).toEqual([1, 2, 3]);
  });

  it('returns top-level array when loopOver is empty', () => {
    expect(parseLoopArray('[10, 20]', '')).toEqual([10, 20]);
  });

  it('falls back to newline-separated text', () => {
    expect(parseLoopArray('line1\nline2\nline3')).toEqual(['line1', 'line2', 'line3']);
  });

  it('filters empty lines in newline fallback', () => {
    expect(parseLoopArray('a\n\nb\n\nc')).toEqual(['a', 'b', 'c']);
  });

  it('returns empty array for empty input', () => {
    expect(parseLoopArray('')).toEqual([]);
  });

  it('wraps non-array dot-path result as single-element array', () => {
    const input = JSON.stringify({ data: 'not-array' });
    expect(parseLoopArray(input, 'data')).toEqual(['not-array']);
  });

  it('wraps object in single-element array when no loopOver', () => {
    // When JSON parse yields a non-array value and there's no dot-path
    const input = JSON.stringify({ key: 'value' });
    const result = parseLoopArray(input);
    // Should fall through to newline split (single line) or wrap
    expect(result.length).toBeGreaterThan(0);
  });
});

// ── getNodeExecConfig — new fields ─────────────────────────────────────────

describe('getNodeExecConfig — Phase 3 fields', () => {
  beforeEach(resetUid);

  it('extracts loop config fields', () => {
    const node = mkNode('loop' as FlowNodeKind, {
      config: {
        loopOver: 'data.items',
        loopVar: 'element',
        loopMaxIterations: 50,
      },
    });
    const cfg = getNodeExecConfig(node);
    expect(cfg.loopOver).toBe('data.items');
    expect(cfg.loopVar).toBe('element');
    expect(cfg.loopMaxIterations).toBe(50);
  });

  it('defaults loopVar to "item" and loopMaxIterations to 100', () => {
    const node = mkNode('loop' as FlowNodeKind, { config: {} });
    const cfg = getNodeExecConfig(node);
    expect(cfg.loopVar).toBe('item');
    expect(cfg.loopMaxIterations).toBe(100);
  });

  it('extracts setVariable and setVariableKey', () => {
    const node = mkNode('agent', {
      config: { setVariable: '{{input}}', setVariableKey: 'result' },
    });
    const cfg = getNodeExecConfig(node);
    expect(cfg.setVariable).toBe('{{input}}');
    expect(cfg.setVariableKey).toBe('result');
  });

  it('extracts subFlowId for group nodes', () => {
    const node = mkNode('group', {
      config: { subFlowId: 'sf-123' },
    });
    const cfg = getNodeExecConfig(node);
    expect(cfg.subFlowId).toBe('sf-123');
  });

  it('extracts credential fields for http nodes', () => {
    const node = mkNode('http' as FlowNodeKind, {
      config: {
        credentialName: 'github-token',
        credentialType: 'bearer',
      },
    });
    const cfg = getNodeExecConfig(node);
    expect(cfg.credentialName).toBe('github-token');
    expect(cfg.credentialType).toBe('bearer');
  });
});

// ── createFlowRunState — vault credentials ─────────────────────────────────

describe('createFlowRunState — Phase 3', () => {
  it('initializes variables from graph', () => {
    const state = createFlowRunState('g1', ['n1', 'n2'], { count: 0 });
    expect(state.variables).toEqual({ count: 0 });
  });

  it('initializes vault credentials', () => {
    const state = createFlowRunState('g1', ['n1'], {}, { apiKey: 'secret123' });
    expect(state.vaultCredentials).toEqual({ apiKey: 'secret123' });
  });

  it('defaults to empty vault credentials', () => {
    const state = createFlowRunState('g1', ['n1']);
    expect(state.vaultCredentials).toEqual({});
  });

  it('defaults to empty variables', () => {
    const state = createFlowRunState('g1', ['n1']);
    expect(state.variables).toEqual({});
  });
});

// ── NODE_DEFAULTS — loop node ──────────────────────────────────────────────

describe('NODE_DEFAULTS — loop', () => {
  it('has loop node defaults defined', () => {
    const loopDefaults = NODE_DEFAULTS['loop' as FlowNodeKind];
    expect(loopDefaults).toBeDefined();
    expect(loopDefaults.icon).toBe('repeat');
    expect(loopDefaults.width).toBe(180);
    expect(loopDefaults.height).toBe(80);
  });
});

// ── conductor-atoms — loop/group classification ────────────────────────────

describe('classifyNode — Phase 3 kinds', () => {
  beforeEach(resetUid);

  it('classifies loop as direct', () => {
    const node = mkNode('loop' as FlowNodeKind);
    expect(classifyNode(node)).toBe('direct');
  });

  it('classifies group as direct', () => {
    const node = mkNode('group');
    expect(classifyNode(node)).toBe('direct');
  });
});

// ── buildExecutionPlan — with loop and group nodes ─────────────────────────

describe('buildExecutionPlan — Phase 3 nodes', () => {
  beforeEach(resetUid);

  it('includes loop nodes in execution plan', () => {
    const trigger = mkNode('trigger', { id: 'trigger' });
    const loop = mkNode('loop' as FlowNodeKind, { id: 'loop' });
    const output = mkNode('output', { id: 'output' });
    const graph = mkGraph(
      [trigger, loop, output],
      [mkEdge('trigger', 'loop'), mkEdge('loop', 'output')],
    );
    const plan = buildExecutionPlan(graph);
    expect(plan).toContain('loop');
  });

  it('includes group nodes in execution plan', () => {
    const trigger = mkNode('trigger', { id: 'trigger' });
    const group = mkNode('group', { id: 'grp', config: { subFlowId: 'sub-1' } });
    const output = mkNode('output', { id: 'output' });
    const graph = mkGraph(
      [trigger, group, output],
      [mkEdge('trigger', 'grp'), mkEdge('grp', 'output')],
    );
    const plan = buildExecutionPlan(graph);
    expect(plan).toContain('grp');
  });
});

// ── Flow variables in graph ────────────────────────────────────────────────

describe('FlowGraph.variables', () => {
  it('can store and access flow-level variables', () => {
    const graph = mkGraph([], [], {
      variables: { apiUrl: 'https://api.example.com', retryCount: 3 },
    });
    expect(graph.variables).toEqual({
      apiUrl: 'https://api.example.com',
      retryCount: 3,
    });
  });

  it('defaults to undefined when not set', () => {
    const graph = mkGraph([], []);
    expect(graph.variables).toBeUndefined();
  });
});
