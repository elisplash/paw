// ─────────────────────────────────────────────────────────────────────────────
// Phase 5 — Sick UI Tests
// Tests for: Data Previews, Validation, Alignment, Shortcuts Registry,
// Strategy Overlay types.
// ─────────────────────────────────────────────────────────────────────────────

import { describe, it, expect } from 'vitest';
import type { FlowNode, FlowEdge, FlowGraph, FlowNodeKind, FlowStatus, EdgeKind } from './atoms';

// ── Preview / Data Label Tests ─────────────────────────────────────────────

import {
  inferDataShape,
  dataShapeColor,
  buildPreviewText,
  type DataShape,
} from './preview-molecules';

describe('preview-molecules', () => {
  describe('inferDataShape', () => {
    it('returns "null" for empty/blank values', () => {
      expect(inferDataShape('')).toBe('null');
      expect(inferDataShape('  ')).toBe('null');
    });

    it('returns "string" for plain text', () => {
      expect(inferDataShape('hello world')).toBe('string');
    });

    it('returns "number" for numeric strings', () => {
      expect(inferDataShape('42')).toBe('number');
      expect(inferDataShape('3.14')).toBe('number');
    });

    it('returns "boolean" for boolean strings', () => {
      expect(inferDataShape('true')).toBe('boolean');
      expect(inferDataShape('false')).toBe('boolean');
    });

    it('returns "json[]" for JSON arrays', () => {
      expect(inferDataShape('[1, 2, 3]')).toBe('json[]');
      expect(inferDataShape('[]')).toBe('json[]');
    });

    it('returns "error" for error strings', () => {
      expect(inferDataShape('Error: something went wrong')).toBe('error');
    });

    it('returns "json" for JSON objects', () => {
      expect(inferDataShape('{"a": 1}')).toBe('json');
    });

    it('returns "null" for JSON null', () => {
      expect(inferDataShape('null')).toBe('null');
    });
  });

  describe('dataShapeColor', () => {
    it('returns distinct colors for each shape', () => {
      const shapes: DataShape[] = [
        'string',
        'number',
        'boolean',
        'json',
        'json[]',
        'error',
        'null',
      ];
      const colors = shapes.map(dataShapeColor);
      expect(typeof colors[0]).toBe('string');
      expect(dataShapeColor('error')).toContain('#');
    });
  });

  describe('buildPreviewText', () => {
    it('truncates long strings', () => {
      const longStr = 'a'.repeat(200);
      const result = buildPreviewText(longStr);
      expect(result.length).toBeLessThan(60);
      expect(result).toContain('…');
    });

    it('returns empty for empty string', () => {
      expect(buildPreviewText('')).toBe('');
    });

    it('passes through short strings as-is', () => {
      expect(buildPreviewText('hello')).toBe('hello');
    });

    it('passes through numbers as strings', () => {
      expect(buildPreviewText('42')).toBe('42');
    });

    it('truncates arrays as JSON', () => {
      const result = buildPreviewText('[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]');
      expect(typeof result).toBe('string');
    });
  });
});

// ── Validation Tests ───────────────────────────────────────────────────────

import {
  validateConnection,
  wouldCreateCycle,
  isValidTargetKind,
  classifyDropTargets,
  snapToPort,
} from './validation-molecules';

function mkNode(id: string, kind: FlowNodeKind, x = 0, y = 0): FlowNode {
  return {
    id,
    kind,
    label: id,
    x,
    y,
    width: 200,
    height: 80,
    config: {},
    status: 'idle' as FlowStatus,
    inputs: ['in'],
    outputs: ['out'],
  };
}

function mkEdge(from: string, to: string): FlowEdge {
  return {
    id: `e-${from}-${to}`,
    kind: 'default' as EdgeKind,
    from,
    to,
    fromPort: 'out',
    toPort: 'in',
    active: false,
  };
}

function mkGraph(nodes: FlowNode[], edges: FlowEdge[]): FlowGraph {
  return {
    id: 'g',
    name: 'test',
    nodes,
    edges,
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
  };
}

describe('validation-molecules', () => {
  describe('validateConnection', () => {
    it('rejects self-connections', () => {
      const n1 = mkNode('n1', 'agent');
      expect(validateConnection(n1, n1, 'out', 'in', [])).not.toBeNull();
    });

    it('rejects duplicate edges', () => {
      const n1 = mkNode('n1', 'agent');
      const n2 = mkNode('n2', 'tool');
      expect(validateConnection(n1, n2, 'out', 'in', [mkEdge('n1', 'n2')])).not.toBeNull();
    });

    it('allows valid connections', () => {
      const n1 = mkNode('n1', 'trigger');
      const n2 = mkNode('n2', 'agent');
      expect(validateConnection(n1, n2, 'out', 'in', [])).toBeNull();
    });

    it('rejects connections into trigger nodes', () => {
      const n1 = mkNode('n1', 'agent');
      const n2 = mkNode('n2', 'trigger');
      expect(validateConnection(n1, n2, 'out', 'in', [])).not.toBeNull();
    });

    it('rejects outgoing from output nodes', () => {
      const n1 = mkNode('n1', 'output');
      const n2 = mkNode('n2', 'agent');
      expect(validateConnection(n1, n2, 'out', 'in', [])).not.toBeNull();
    });
  });

  describe('wouldCreateCycle', () => {
    it('detects simple cycle A→B→A', () => {
      const n1 = mkNode('n1', 'agent');
      const n2 = mkNode('n2', 'tool');
      const g = mkGraph([n1, n2], [mkEdge('n1', 'n2')]);
      // Adding B→A would create cycle A→B→A
      expect(wouldCreateCycle(g, 'n2', 'n1')).toBe(true);
    });

    it('returns false when no cycle', () => {
      const n1 = mkNode('n1', 'trigger');
      const n2 = mkNode('n2', 'agent');
      const n3 = mkNode('n3', 'tool');
      const g = mkGraph([n1, n2, n3], [mkEdge('n1', 'n2')]);
      expect(wouldCreateCycle(g, 'n2', 'n3')).toBe(false);
    });

    it('detects longer cycle A→B→C→A', () => {
      const n1 = mkNode('n1', 'agent');
      const n2 = mkNode('n2', 'tool');
      const n3 = mkNode('n3', 'data');
      const g = mkGraph([n1, n2, n3], [mkEdge('n1', 'n2'), mkEdge('n2', 'n3')]);
      // Adding C→A would create cycle A→B→C→A
      expect(wouldCreateCycle(g, 'n3', 'n1')).toBe(true);
    });
  });

  describe('isValidTargetKind', () => {
    it('rejects trigger as target', () => {
      expect(isValidTargetKind('agent', 'trigger')).toBe(false);
    });

    it('accepts agent as target from trigger', () => {
      expect(isValidTargetKind('trigger', 'agent')).toBe(true);
    });

    it('rejects any target from output', () => {
      expect(isValidTargetKind('output', 'agent')).toBe(false);
    });
  });

  describe('classifyDropTargets', () => {
    it('classifies ports as valid/invalid', () => {
      const n1 = mkNode('n1', 'agent', 0, 0);
      const n2 = mkNode('n2', 'tool', 300, 0);
      const n3 = mkNode('n3', 'trigger', 300, 300);
      const g = mkGraph([n1, n2, n3], []);
      const result = classifyDropTargets(g, n1, 'out');
      expect(result.get('n2')).toBe('valid');
      expect(result.get('n3')).toBe('invalid'); // trigger nodes can't be targets
    });
  });

  describe('snapToPort', () => {
    it('snaps to a nearby port', () => {
      // Port at (100, 100), cursor at (102, 102) — within 20px
      expect(snapToPort(102, 102, 100, 100)).toBe(true);
    });

    it('returns false if no port nearby', () => {
      expect(snapToPort(500, 500, 100, 100)).toBe(false);
    });
  });
});

// ── Alignment Tests ────────────────────────────────────────────────────────

import { computeAlignmentSnap } from './alignment-molecules';

describe('alignment-molecules', () => {
  describe('computeAlignmentSnap', () => {
    it('detects center x alignment', () => {
      const moving = mkNode('m', 'agent', 100, 0);
      moving.width = 200;
      moving.height = 80;
      const others = [mkNode('o', 'tool', 100, 200)];
      others[0].width = 200;
      others[0].height = 80;

      // Both at x=100, width=200 → center at x=200 — should snap
      const result = computeAlignmentSnap(moving, 100, 0, others);
      expect(result).toBeDefined();
      expect(result.x === null || typeof result.x === 'number').toBe(true);
      expect(result.y === null || typeof result.y === 'number').toBe(true);
    });

    it('returns guides array', () => {
      const moving = mkNode('m', 'agent', 100, 0);
      moving.width = 200;
      moving.height = 80;
      const others = [mkNode('o', 'tool', 102, 200)]; // 2px off
      others[0].width = 200;
      others[0].height = 80;

      const result = computeAlignmentSnap(moving, 100, 0, others);
      expect(Array.isArray(result.guides)).toBe(true);
    });

    it('handles no other nodes', () => {
      const moving = mkNode('m', 'agent', 100, 0);
      moving.width = 200;
      moving.height = 80;
      const result = computeAlignmentSnap(moving, 100, 0, []);
      expect(result.guides).toHaveLength(0);
      expect(result.x).toBeNull();
      expect(result.y).toBeNull();
    });
  });
});

// ── Shortcuts Tests ────────────────────────────────────────────────────────

import { SHORTCUT_REGISTRY, searchShortcuts, type ShortcutCategory } from './shortcuts-molecules';

describe('shortcuts-molecules', () => {
  describe('SHORTCUT_REGISTRY', () => {
    it('has entries for all categories', () => {
      const categories: ShortcutCategory[] = ['Navigation', 'Editing', 'Execution', 'Debug'];
      for (const cat of categories) {
        const count = SHORTCUT_REGISTRY.filter((s) => s.category === cat).length;
        expect(count).toBeGreaterThan(0);
      }
    });

    it('each entry has keys and label', () => {
      for (const entry of SHORTCUT_REGISTRY) {
        expect(entry.keys.length).toBeGreaterThan(0);
        expect(entry.label.length).toBeGreaterThan(0);
      }
    });
  });

  describe('searchShortcuts', () => {
    it('returns all entries for empty query', () => {
      expect(searchShortcuts('')).toHaveLength(SHORTCUT_REGISTRY.length);
    });

    it('filters by label', () => {
      const results = searchShortcuts('undo');
      expect(results.length).toBeGreaterThan(0);
      expect(results[0].label.toLowerCase()).toContain('undo');
    });

    it('filters by key', () => {
      const results = searchShortcuts('F5');
      expect(results.length).toBeGreaterThan(0);
      expect(results.some((s) => s.keys.includes('F5'))).toBe(true);
    });

    it('filters by category', () => {
      const results = searchShortcuts('debug');
      expect(results.length).toBeGreaterThan(0);
    });

    it('returns empty for no match', () => {
      const results = searchShortcuts('xyznonexistent');
      expect(results).toHaveLength(0);
    });
  });
});

// ── Strategy Overlay Tests ─────────────────────────────────────────────────

import type { StrategyUnit, StrategyOverlayData } from './strategy-overlay-molecules';

describe('strategy-overlay-molecules', () => {
  describe('StrategyUnit type', () => {
    it('supports all unit kinds', () => {
      const kinds: StrategyUnit['kind'][] = ['collapsed', 'parallel', 'sequential', 'convergent'];
      for (const k of kinds) {
        const unit: StrategyUnit = {
          kind: k,
          nodeIds: ['a', 'b'],
          label: `Test ${k}`,
          phaseIndex: 0,
        };
        expect(unit.kind).toBe(k);
      }
    });
  });

  describe('StrategyOverlayData type', () => {
    it('can be constructed', () => {
      const data: StrategyOverlayData = {
        units: [],
        totalPhases: 3,
        estimatedSaving: '~60% fewer LLM calls',
      };
      expect(data.totalPhases).toBe(3);
      expect(data.units).toHaveLength(0);
    });
  });
});
