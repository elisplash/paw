// ─────────────────────────────────────────────────────────────────────────────
// Flow Visualization Engine — Atoms Tests
// ─────────────────────────────────────────────────────────────────────────────

import { describe, it, expect } from 'vitest';
import {
  createNode,
  createEdge,
  createGraph,
  computeLayers,
  applyLayout,
  snapToGrid,
  buildEdgePath,
  getOutputPort,
  getInputPort,
  hitTestNode,
  hitTestPort,
  serializeGraph,
  deserializeGraph,
  instantiateTemplate,
  filterTemplates,
  type FlowGraph,
  type FlowTemplate,
  NODE_DEFAULTS,
  GRID_SIZE,
  TEMPLATE_CATEGORIES,
} from './atoms';

// ── Factory functions ──────────────────────────────────────────────────────

describe('createNode', () => {
  it('creates a node with defaults for the given kind', () => {
    const n = createNode('agent', 'My Agent');
    expect(n.kind).toBe('agent');
    expect(n.label).toBe('My Agent');
    expect(n.width).toBe(NODE_DEFAULTS.agent.width);
    expect(n.height).toBe(NODE_DEFAULTS.agent.height);
    expect(n.status).toBe('idle');
    expect(n.inputs).toEqual(['in']);
    expect(n.outputs).toEqual(['out', 'err']);
  });

  it('trigger nodes have no inputs', () => {
    const n = createNode('trigger', 'Start');
    expect(n.inputs).toEqual([]);
    expect(n.outputs).toEqual(['out', 'err']);
  });

  it('output nodes have no outputs', () => {
    const n = createNode('output', 'End');
    expect(n.inputs).toEqual(['in']);
    expect(n.outputs).toEqual([]);
  });

  it('error nodes have no outputs and one input', () => {
    const n = createNode('error', 'Error Handler');
    expect(n.inputs).toEqual(['in']);
    expect(n.outputs).toEqual([]);
    expect(n.kind).toBe('error');
  });

  it('respects position overrides', () => {
    const n = createNode('tool', 'Hammer', 100, 200);
    expect(n.x).toBe(100);
    expect(n.y).toBe(200);
  });

  it('respects partial overrides', () => {
    const n = createNode('data', 'Transform', 0, 0, { label: 'Custom', description: 'desc' });
    expect(n.label).toBe('Custom');
    expect(n.description).toBe('desc');
  });
});

describe('createEdge', () => {
  it('creates a forward edge by default', () => {
    const e = createEdge('a', 'b');
    expect(e.from).toBe('a');
    expect(e.to).toBe('b');
    expect(e.kind).toBe('forward');
    expect(e.active).toBe(false);
  });

  it('supports reverse edges', () => {
    const e = createEdge('a', 'b', 'reverse');
    expect(e.kind).toBe('reverse');
  });

  it('supports bidirectional edges', () => {
    const e = createEdge('a', 'b', 'bidirectional');
    expect(e.kind).toBe('bidirectional');
  });
});

describe('createGraph', () => {
  it('creates an empty graph with timestamps', () => {
    const g = createGraph('Test Flow');
    expect(g.name).toBe('Test Flow');
    expect(g.nodes).toEqual([]);
    expect(g.edges).toEqual([]);
    expect(g.createdAt).toBeTruthy();
    expect(g.updatedAt).toBeTruthy();
  });

  it('accepts initial nodes and edges', () => {
    const n = createNode('trigger', 'Start');
    const g = createGraph('With Nodes', [n]);
    expect(g.nodes).toHaveLength(1);
  });
});

// ── Layout ─────────────────────────────────────────────────────────────────

function makeLinearGraph(): FlowGraph {
  const a = createNode('trigger', 'A');
  const b = createNode('agent', 'B');
  const c = createNode('output', 'C');
  const e1 = createEdge(a.id, b.id);
  const e2 = createEdge(b.id, c.id);
  return createGraph('Linear', [a, b, c], [e1, e2]);
}

function makeBranchGraph(): FlowGraph {
  const start = createNode('trigger', 'Start');
  const cond = createNode('condition', 'If');
  const yes = createNode('agent', 'Yes');
  const no = createNode('agent', 'No');
  const end = createNode('output', 'End');
  return createGraph(
    'Branch',
    [start, cond, yes, no, end],
    [
      createEdge(start.id, cond.id),
      createEdge(cond.id, yes.id),
      createEdge(cond.id, no.id),
      createEdge(yes.id, end.id),
      createEdge(no.id, end.id),
    ],
  );
}

describe('computeLayers', () => {
  it('assigns sequential layers to a linear chain', () => {
    const g = makeLinearGraph();
    const layers = computeLayers(g);
    const vals = g.nodes.map((n) => layers.get(n.id)!.layer);
    expect(vals).toEqual([0, 1, 2]);
  });

  it('assigns parallel nodes to the same layer in a branch', () => {
    const g = makeBranchGraph();
    const layers = computeLayers(g);
    // Start=0, Condition=1, Yes and No=2, End=3
    expect(layers.get(g.nodes[0].id)!.layer).toBe(0);
    expect(layers.get(g.nodes[1].id)!.layer).toBe(1);
    expect(layers.get(g.nodes[2].id)!.layer).toBe(layers.get(g.nodes[3].id)!.layer);
  });

  it('handles single-node graphs', () => {
    const n = createNode('trigger', 'Solo');
    const g = createGraph('Solo', [n]);
    const layers = computeLayers(g);
    expect(layers.get(n.id)!.layer).toBe(0);
  });
});

describe('applyLayout', () => {
  it('returns positive bounding box', () => {
    const g = makeLinearGraph();
    const bbox = applyLayout(g);
    expect(bbox.width).toBeGreaterThan(0);
    expect(bbox.height).toBeGreaterThan(0);
  });

  it('positions nodes left-to-right by layer', () => {
    const g = makeLinearGraph();
    applyLayout(g);
    expect(g.nodes[0].x).toBeLessThan(g.nodes[1].x);
    expect(g.nodes[1].x).toBeLessThan(g.nodes[2].x);
  });

  it('branch nodes at same layer have same x', () => {
    const g = makeBranchGraph();
    applyLayout(g);
    // Nodes 2 and 3 (Yes and No) should share x
    expect(g.nodes[2].x).toBe(g.nodes[3].x);
  });
});

// ── Grid snapping ──────────────────────────────────────────────────────────

describe('snapToGrid', () => {
  it('snaps to nearest grid point', () => {
    expect(snapToGrid(12)).toBe(GRID_SIZE); // 12/20=0.6 → round=1 → 20
    expect(snapToGrid(0)).toBe(0);
    expect(snapToGrid(30)).toBe(GRID_SIZE * 2); // 30/20=1.5 → round=2 → 40
    expect(snapToGrid(31)).toBe(GRID_SIZE * 2); // 31/20=1.55 → round=2 → 40
    expect(snapToGrid(25)).toBe(GRID_SIZE); // 25/20=1.25 → round=1 → 20
  });

  it('handles negative values', () => {
    expect(snapToGrid(-5)).toBe(-0); // -5/20=-0.25 → round=-0
    expect(snapToGrid(-15)).toBe(-GRID_SIZE); // -15/20=-0.75 → round=-1 → -20
  });
});

// ── Edge path geometry ──────────────────────────────────────────────────────

describe('getOutputPort / getInputPort', () => {
  it('output port is at the right, upper portion of the node', () => {
    const n = createNode('agent', 'A', 100, 50);
    const p = getOutputPort(n);
    expect(p.x).toBe(100 + n.width);
    expect(p.y).toBe(50 + n.height * 0.35);
  });

  it('input port is at the left-center of the node', () => {
    const n = createNode('agent', 'A', 100, 50);
    const p = getInputPort(n);
    expect(p.x).toBe(100);
    expect(p.y).toBe(50 + n.height / 2);
  });

  it('error port is at the right-bottom of the node', () => {
    const n = createNode('agent', 'A', 100, 50);
    const p = getOutputPort(n, 'err');
    expect(p.x).toBe(100 + n.width);
    expect(p.y).toBe(50 + n.height * 0.8);
  });
});

describe('buildEdgePath', () => {
  it('returns an SVG path string starting with M', () => {
    const path = buildEdgePath({ x: 0, y: 0 }, { x: 200, y: 100 });
    expect(path).toMatch(/^M /);
    expect(path).toContain('C ');
  });
});

// ── Serialization ──────────────────────────────────────────────────────────

describe('serializeGraph / deserializeGraph', () => {
  it('round-trips a graph', () => {
    const g = makeLinearGraph();
    const json = serializeGraph(g);
    const restored = deserializeGraph(json);
    expect(restored).not.toBeNull();
    expect(restored!.id).toBe(g.id);
    expect(restored!.nodes).toHaveLength(3);
    expect(restored!.edges).toHaveLength(2);
  });

  it('returns null for invalid JSON', () => {
    expect(deserializeGraph('not json')).toBeNull();
    expect(deserializeGraph('{"foo":1}')).toBeNull();
  });
});

// ── Hit Testing ────────────────────────────────────────────────────────────

describe('hitTestNode', () => {
  it('finds a node at its center', () => {
    const n = createNode('agent', 'A', 100, 100);
    const g = createGraph('Test', [n]);
    const hit = hitTestNode(g, 100 + n.width / 2, 100 + n.height / 2);
    expect(hit).not.toBeNull();
    expect(hit!.id).toBe(n.id);
  });

  it('returns null for empty space', () => {
    const n = createNode('agent', 'A', 100, 100);
    const g = createGraph('Test', [n]);
    expect(hitTestNode(g, 0, 0)).toBeNull();
  });
});

describe('hitTestPort', () => {
  it('finds an output port near click position', () => {
    const n = createNode('agent', 'A', 100, 100);
    const g = createGraph('Test', [n]);
    const port = getOutputPort(n);
    const hit = hitTestPort(g, port.x + 2, port.y + 2);
    expect(hit).not.toBeNull();
    expect(hit!.kind).toBe('output');
  });

  it('returns null when far from any port', () => {
    const n = createNode('agent', 'A', 100, 100);
    const g = createGraph('Test', [n]);
    expect(hitTestPort(g, 500, 500)).toBeNull();
  });
});

// ── Template Types & Functions ─────────────────────────────────────────────

describe('TEMPLATE_CATEGORIES', () => {
  it('has entries for all categories', () => {
    const cats = [
      'ai',
      'communication',
      'devops',
      'productivity',
      'data',
      'research',
      'social',
      'finance',
      'support',
      'custom',
    ];
    for (const cat of cats) {
      expect(TEMPLATE_CATEGORIES[cat as keyof typeof TEMPLATE_CATEGORIES]).toBeDefined();
      expect(TEMPLATE_CATEGORIES[cat as keyof typeof TEMPLATE_CATEGORIES].label).toBeTruthy();
      expect(TEMPLATE_CATEGORIES[cat as keyof typeof TEMPLATE_CATEGORIES].icon).toBeTruthy();
    }
  });
});

describe('instantiateTemplate', () => {
  const tpl: FlowTemplate = {
    id: 'test-tpl',
    name: 'Test Template',
    description: 'A test',
    category: 'ai',
    tags: ['test'],
    icon: 'bolt',
    nodes: [
      { kind: 'trigger', label: 'Start' },
      { kind: 'agent', label: 'Process', config: { prompt: 'do stuff' } },
      { kind: 'output', label: 'End' },
    ],
    edges: [
      { fromIdx: 0, toIdx: 1 },
      { fromIdx: 1, toIdx: 2 },
    ],
  };

  it('creates a FlowGraph with fresh IDs', () => {
    const g = instantiateTemplate(tpl);
    expect(g.name).toBe('Test Template');
    expect(g.description).toBe('A test');
    expect(g.nodes).toHaveLength(3);
    expect(g.edges).toHaveLength(2);
  });

  it('nodes get unique IDs', () => {
    const g = instantiateTemplate(tpl);
    const ids = g.nodes.map((n) => n.id);
    expect(new Set(ids).size).toBe(3);
  });

  it('edges reference correct node IDs', () => {
    const g = instantiateTemplate(tpl);
    expect(g.edges[0].from).toBe(g.nodes[0].id);
    expect(g.edges[0].to).toBe(g.nodes[1].id);
    expect(g.edges[1].from).toBe(g.nodes[1].id);
    expect(g.edges[1].to).toBe(g.nodes[2].id);
  });

  it('preserves node config', () => {
    const g = instantiateTemplate(tpl);
    expect((g.nodes[1].config as Record<string, unknown>).prompt).toBe('do stuff');
  });

  it('applies layout to nodes', () => {
    const g = instantiateTemplate(tpl);
    // After layout, nodes should have non-zero x positions (layered)
    expect(g.nodes[0].x).toBeGreaterThanOrEqual(0);
    expect(g.nodes[2].x).toBeGreaterThan(g.nodes[0].x);
  });

  it('handles out-of-bounds edge indices gracefully', () => {
    const badTpl: FlowTemplate = {
      ...tpl,
      edges: [{ fromIdx: 0, toIdx: 99 }],
    };
    const g = instantiateTemplate(badTpl);
    expect(g.edges).toHaveLength(0);
  });
});

describe('filterTemplates', () => {
  const templates: FlowTemplate[] = [
    {
      id: 'a',
      name: 'Daily Digest',
      description: 'Summarize channels',
      category: 'ai',
      tags: ['summary', 'daily'],
      icon: 'a',
      nodes: [],
      edges: [],
    },
    {
      id: 'b',
      name: 'Email Responder',
      description: 'Auto-reply emails',
      category: 'communication',
      tags: ['email', 'reply'],
      icon: 'b',
      nodes: [],
      edges: [],
    },
    {
      id: 'c',
      name: 'PR Reviewer',
      description: 'Review pull requests',
      category: 'devops',
      tags: ['github', 'review'],
      icon: 'c',
      nodes: [],
      edges: [],
    },
  ];

  it('returns all templates when category=all and no query', () => {
    expect(filterTemplates(templates, 'all', '')).toHaveLength(3);
  });

  it('filters by category', () => {
    expect(filterTemplates(templates, 'ai', '')).toHaveLength(1);
    expect(filterTemplates(templates, 'devops', '')).toHaveLength(1);
  });

  it('filters by search query in name', () => {
    expect(filterTemplates(templates, 'all', 'digest')).toHaveLength(1);
  });

  it('filters by search query in description', () => {
    expect(filterTemplates(templates, 'all', 'pull request')).toHaveLength(1);
  });

  it('filters by search query in tags', () => {
    expect(filterTemplates(templates, 'all', 'email')).toHaveLength(1);
  });

  it('combines category and query filters', () => {
    expect(filterTemplates(templates, 'ai', 'digest')).toHaveLength(1);
    expect(filterTemplates(templates, 'devops', 'digest')).toHaveLength(0);
  });

  it('is case-insensitive', () => {
    expect(filterTemplates(templates, 'all', 'DAILY')).toHaveLength(1);
  });
});
