// ─────────────────────────────────────────────────────────────────────────────
// Phase 4 — AI Superpowers Tests
// Tests for: AI Flow Builder, Self-Healing, Suggestions, Smart Conditions,
// Memory-Flow atoms, new node kind classification, and executor config.
// ─────────────────────────────────────────────────────────────────────────────

import { describe, it, expect, beforeEach } from 'vitest';
import type { FlowGraph, FlowNode, FlowEdge, FlowNodeKind } from './atoms';
import { NODE_DEFAULTS } from './atoms';

// AI Builder
import {
  buildFlowBuilderSystemPrompt,
  buildFlowFromIntentPrompt,
  buildFlowModifyPrompt,
  buildFlowExplainPrompt,
  parseFlowBuildResponse,
  validateGeneratedFlow,
} from './ai-builder-atoms';

// Self-Healing
import {
  classifyError,
  isTransientError,
  suggestQuickFixes,
  buildDiagnosisSystemPrompt,
  buildDiagnosisPrompt,
  parseDiagnosisResponse,
  applyFixToConfig,
} from './self-healing-atoms';

// Suggestions
import { getSuggestionsForNode, suggestedNodePosition } from './suggestion-atoms';

// Smart Conditions
import {
  parseConditionExpr,
  resolvePath,
  compareValues,
  evaluateSmartCondition,
  evaluateBuiltinCondition,
} from './smart-condition-atoms';

// Memory Flow
import {
  getMemoryWriteConfig,
  getMemoryRecallConfig,
  getSquadNodeConfig,
  formatRecalledMemories,
  buildMemoryContent,
  DEFAULT_MEMORY_WRITE,
  DEFAULT_MEMORY_RECALL,
  DEFAULT_SQUAD_CONFIG,
} from './memory-flow-atoms';

// Existing modules
import { getNodeExecConfig } from './executor-atoms';
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
    description: '',
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
    ...overrides,
  };
}

beforeEach(() => {
  _uid = 0;
});

// ═══════════════════════════════════════════════════════════════════════════
// 4.0 — New Node Kinds in atoms.ts
// ═══════════════════════════════════════════════════════════════════════════

describe('Phase 4: New node kinds', () => {
  it('NODE_DEFAULTS contains squad, memory, memory-recall', () => {
    expect(NODE_DEFAULTS['squad' as FlowNodeKind]).toBeDefined();
    expect(NODE_DEFAULTS['memory' as FlowNodeKind]).toBeDefined();
    expect(NODE_DEFAULTS['memory-recall' as FlowNodeKind]).toBeDefined();
  });

  it('squad defaults have expected dims and color', () => {
    const d = NODE_DEFAULTS['squad' as FlowNodeKind];
    expect(d.width).toBe(200);
    expect(d.height).toBe(80);
    expect(d.color).toBe('var(--kinetic-purple, #A855F7)');
  });

  it('memory defaults have expected dims and color', () => {
    const d = NODE_DEFAULTS['memory' as FlowNodeKind];
    expect(d.width).toBe(180);
    expect(d.height).toBe(72);
    expect(d.color).toBe('var(--kinetic-sage, #5BA08C)');
  });

  it('memory-recall defaults have expected dims and color', () => {
    const d = NODE_DEFAULTS['memory-recall' as FlowNodeKind];
    expect(d.width).toBe(180);
    expect(d.height).toBe(72);
    expect(d.color).toBe('var(--kinetic-gold, #D4A853)');
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 4.1 — AI Flow Builder
// ═══════════════════════════════════════════════════════════════════════════

describe('Phase 4.1: AI Flow Builder', () => {
  describe('buildFlowBuilderSystemPrompt', () => {
    it('produces a non-empty system prompt', () => {
      const prompt = buildFlowBuilderSystemPrompt();
      expect(prompt.length).toBeGreaterThan(200);
      expect(prompt).toContain('trigger');
      expect(prompt).toContain('squad');
      expect(prompt).toContain('memory');
    });
  });

  describe('buildFlowFromIntentPrompt', () => {
    it('includes user intent', () => {
      const prompt = buildFlowFromIntentPrompt({ intent: 'Summarize daily emails' });
      expect(prompt).toContain('Summarize daily emails');
    });

    it('includes available agents when provided', () => {
      const prompt = buildFlowFromIntentPrompt({
        intent: 'Test',
        availableAgents: [{ id: 'a1', name: 'Writer' }],
      });
      expect(prompt).toContain('Writer');
    });

    it('includes available tools when provided', () => {
      const prompt = buildFlowFromIntentPrompt({
        intent: 'Test',
        availableTools: ['search_web', 'read_file'],
      });
      expect(prompt).toContain('search_web');
    });
  });

  describe('buildFlowModifyPrompt', () => {
    it('includes the graph JSON and instruction', () => {
      const graph = mkGraph([mkNode('trigger')], []);
      const prompt = buildFlowModifyPrompt({ graph, instruction: 'Add error handling' });
      expect(prompt).toContain('Add error handling');
      expect(prompt).toContain('trigger');
    });
  });

  describe('buildFlowExplainPrompt', () => {
    it('includes the graph and detail level', () => {
      const graph = mkGraph([mkNode('trigger')], []);
      const prompt = buildFlowExplainPrompt({ graph, detail: 'brief' });
      expect(prompt).toContain('one paragraph');
    });
  });

  describe('parseFlowBuildResponse', () => {
    it('parses a valid JSON response', () => {
      const response = JSON.stringify({
        graph: {
          id: 'g1',
          name: 'Test',
          description: 'A test flow',
          nodes: [
            {
              id: 'n1',
              kind: 'trigger',
              label: 'Start',
              x: 0,
              y: 0,
              width: 180,
              height: 72,
              status: 'idle',
              config: {},
              inputs: ['in'],
              outputs: ['out'],
            },
          ],
          edges: [],
          createdAt: '2024-01-01',
          updatedAt: '2024-01-01',
        },
        explanation: 'A simple test flow.',
      });
      const result = parseFlowBuildResponse(response);
      expect(result).not.toBeNull();
      expect(result!.graph.nodes).toHaveLength(1);
      expect(result!.explanation).toBe('A simple test flow.');
    });

    it('extracts JSON from markdown code blocks', () => {
      const response =
        '```json\n{"graph":{"id":"g1","name":"Test","nodes":[],"edges":[],"createdAt":"2024","updatedAt":"2024"},"explanation":"Empty"}\n```';
      const result = parseFlowBuildResponse(response);
      expect(result).not.toBeNull();
      expect(result!.graph.nodes).toHaveLength(0);
    });

    it('returns null for invalid JSON', () => {
      expect(parseFlowBuildResponse('not json')).toBeNull();
    });

    it('returns null for missing graph', () => {
      expect(parseFlowBuildResponse('{"explanation":"no graph"}')).toBeNull();
    });
  });

  describe('validateGeneratedFlow', () => {
    it('reports missing trigger', () => {
      const graph = mkGraph([mkNode('agent')], []);
      const errors = validateGeneratedFlow(graph);
      expect(errors.some((e) => e.includes('trigger'))).toBe(true);
    });

    it('reports missing output', () => {
      const graph = mkGraph([mkNode('trigger')], []);
      const errors = validateGeneratedFlow(graph);
      expect(errors.some((e) => e.includes('output'))).toBe(true);
    });

    it('reports invalid edge references', () => {
      const n1 = mkNode('trigger', { id: 'n1' });
      const edge = mkEdge('n1', 'n_missing');
      const graph = mkGraph([n1], [edge]);
      const errors = validateGeneratedFlow(graph);
      expect(errors.some((e) => e.includes('n_missing'))).toBe(true);
    });

    it('reports duplicate node IDs', () => {
      const n1 = mkNode('trigger', { id: 'dup' });
      const n2 = mkNode('output', { id: 'dup' });
      const graph = mkGraph([n1, n2], []);
      const errors = validateGeneratedFlow(graph);
      expect(errors.some((e) => e.includes('Duplicate'))).toBe(true);
    });

    it('passes for a valid flow', () => {
      const n1 = mkNode('trigger', { id: 't1' });
      const n2 = mkNode('output', { id: 'o1' });
      const graph = mkGraph([n1, n2], [mkEdge('t1', 'o1')]);
      const errors = validateGeneratedFlow(graph);
      expect(errors).toHaveLength(0);
    });
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 4.2 — Self-Healing
// ═══════════════════════════════════════════════════════════════════════════

describe('Phase 4.2: Self-Healing', () => {
  describe('classifyError', () => {
    it('classifies timeout errors', () => {
      expect(classifyError('Request timed out after 30s')).toBe('timeout');
    });
    it('classifies rate limit errors', () => {
      expect(classifyError('429 Too Many Requests')).toBe('rate-limit');
    });
    it('classifies auth errors', () => {
      expect(classifyError('401 Unauthorized')).toBe('auth');
    });
    it('classifies network errors', () => {
      expect(classifyError('ECONNREFUSED')).toBe('network');
    });
    it('classifies config errors', () => {
      expect(classifyError('no model configured')).toBe('config');
    });
    it('classifies code errors', () => {
      expect(classifyError('Code error: x is not defined')).toBe('code-error');
    });
    it('returns unknown for unrecognized errors', () => {
      expect(classifyError('something weird happened')).toBe('unknown');
    });
  });

  describe('isTransientError', () => {
    it('timeout is transient', () => expect(isTransientError('timed out')).toBe(true));
    it('rate-limit is transient', () => expect(isTransientError('429')).toBe(true));
    it('network is transient', () => expect(isTransientError('ECONNREFUSED')).toBe(true));
    it('auth is not transient', () => expect(isTransientError('401')).toBe(false));
  });

  describe('suggestQuickFixes', () => {
    it('suggests timeout fix for timeout errors', () => {
      const node = mkNode('agent');
      const config = getNodeExecConfig(node);
      const fixes = suggestQuickFixes('request timed out', node, config);
      expect(fixes.length).toBeGreaterThan(0);
      expect(fixes[0].configPatch?.timeoutMs).toBeDefined();
    });

    it('suggests retry for rate-limit errors', () => {
      const node = mkNode('tool');
      const config = getNodeExecConfig(node);
      const fixes = suggestQuickFixes('429 too many requests', node, config);
      expect(fixes.length).toBeGreaterThan(0);
      expect(fixes[0].configPatch?.maxRetries).toBeDefined();
    });

    it('suggests prompt for config errors on agent nodes', () => {
      const node = mkNode('agent');
      const config = getNodeExecConfig(node);
      const fixes = suggestQuickFixes('no model configured', node, config);
      expect(fixes.length).toBeGreaterThan(0);
    });
  });

  describe('buildDiagnosisSystemPrompt', () => {
    it('returns a non-empty prompt', () => {
      const prompt = buildDiagnosisSystemPrompt();
      expect(prompt.length).toBeGreaterThan(100);
      expect(prompt).toContain('JSON');
    });
  });

  describe('buildDiagnosisPrompt', () => {
    it('includes error and node info', () => {
      const node = mkNode('agent', { label: 'Summarizer' });
      const graph = mkGraph([node], []);
      const config = getNodeExecConfig(node);
      const prompt = buildDiagnosisPrompt({
        node,
        config,
        error: 'timed out',
        input: 'hello',
        graph,
      });
      expect(prompt).toContain('Summarizer');
      expect(prompt).toContain('timed out');
    });
  });

  describe('parseDiagnosisResponse', () => {
    it('parses valid diagnosis JSON', () => {
      const json = JSON.stringify({
        rootCause: 'Timeout',
        explanation: 'The upstream API is slow.',
        fixes: [
          {
            diagnosis: 'Slow API',
            description: 'Increase timeout',
            confidence: 0.9,
            autoApplicable: true,
          },
        ],
        isTransient: true,
      });
      const result = parseDiagnosisResponse(json);
      expect(result).not.toBeNull();
      expect(result!.rootCause).toBe('Timeout');
      expect(result!.fixes).toHaveLength(1);
      expect(result!.isTransient).toBe(true);
    });

    it('returns null for invalid JSON', () => {
      expect(parseDiagnosisResponse('not json')).toBeNull();
    });
  });

  describe('applyFixToConfig', () => {
    it('merges configPatch into current config', () => {
      const result = applyFixToConfig(
        { maxRetries: 0 },
        {
          diagnosis: '',
          description: '',
          confidence: 1,
          autoApplicable: true,
          configPatch: { maxRetries: 3 },
        },
      );
      expect(result.maxRetries).toBe(3);
    });

    it('returns original config if no patch', () => {
      const original = { maxRetries: 0 };
      const result = applyFixToConfig(original, {
        diagnosis: '',
        description: '',
        confidence: 1,
        autoApplicable: true,
      });
      expect(result).toBe(original);
    });
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 4.3 — Squad Node (classification + config)
// ═══════════════════════════════════════════════════════════════════════════

describe('Phase 4.3: Squad Node', () => {
  it('classifyNode returns agent for squad nodes', () => {
    const node = mkNode('squad' as FlowNodeKind);
    expect(classifyNode(node)).toBe('agent');
  });

  it('getNodeExecConfig extracts squad fields', () => {
    const node = mkNode('squad' as FlowNodeKind, {
      config: {
        squadId: 'sq-1',
        squadObjective: 'Research AI',
        squadTimeoutMs: 60000,
        squadMaxRounds: 3,
      },
    });
    const config = getNodeExecConfig(node);
    expect(config.squadId).toBe('sq-1');
    expect(config.squadObjective).toBe('Research AI');
    expect(config.squadTimeoutMs).toBe(60000);
    expect(config.squadMaxRounds).toBe(3);
  });

  it('squad config has proper defaults', () => {
    const node = mkNode('squad' as FlowNodeKind);
    const config = getNodeExecConfig(node);
    expect(config.squadTimeoutMs).toBe(300000);
    expect(config.squadMaxRounds).toBe(5);
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 4.4 — Flow Suggestions / Autocomplete
// ═══════════════════════════════════════════════════════════════════════════

describe('Phase 4.4: Flow Suggestions', () => {
  describe('getSuggestionsForNode', () => {
    it('suggests agent after trigger', () => {
      const trigger = mkNode('trigger', { id: 't1' });
      const graph = mkGraph([trigger], []);
      const suggestions = getSuggestionsForNode(trigger, graph);
      expect(suggestions.length).toBeGreaterThan(0);
      expect(suggestions[0].kind).toBe('agent');
    });

    it('suggests output after agent', () => {
      const agent = mkNode('agent', { id: 'a1' });
      const graph = mkGraph([agent], []);
      const suggestions = getSuggestionsForNode(agent, graph);
      expect(suggestions.some((s) => s.kind === 'output')).toBe(true);
    });

    it('boosts output when flow has no output node', () => {
      const agent = mkNode('agent', { id: 'a1' });
      const graph = mkGraph([agent], []);
      const suggestions = getSuggestionsForNode(agent, graph);
      const outputSugg = suggestions.find((s) => s.kind === 'output');
      expect(outputSugg).toBeDefined();
      expect(outputSugg!.reason).toContain('no output node');
    });

    it('caps at 5 suggestions', () => {
      const trigger = mkNode('trigger', { id: 't1' });
      const graph = mkGraph([trigger], []);
      const suggestions = getSuggestionsForNode(trigger, graph);
      expect(suggestions.length).toBeLessThanOrEqual(5);
    });

    it('suggests memory-recall after trigger', () => {
      const trigger = mkNode('trigger', { id: 't1' });
      const graph = mkGraph([trigger], []);
      const suggestions = getSuggestionsForNode(trigger, graph);
      expect(suggestions.some((s) => s.kind === ('memory-recall' as FlowNodeKind))).toBe(true);
    });

    it('suggests agent after memory-recall', () => {
      const recall = mkNode('memory-recall' as FlowNodeKind, { id: 'mr1' });
      const graph = mkGraph([recall], []);
      const suggestions = getSuggestionsForNode(recall, graph);
      expect(suggestions[0].kind).toBe('agent');
    });

    it('suggests output after squad', () => {
      const squad = mkNode('squad' as FlowNodeKind, { id: 'sq1' });
      const graph = mkGraph([squad], []);
      const suggestions = getSuggestionsForNode(squad, graph);
      expect(suggestions[0].kind).toBe('output');
    });
  });

  describe('suggestedNodePosition', () => {
    it('places new node to the right of source', () => {
      const source = mkNode('trigger', { id: 't1', x: 100, y: 50 });
      const graph = mkGraph([source], []);
      const pos = suggestedNodePosition(source, graph);
      expect(pos.x).toBeGreaterThan(source.x + source.width);
      expect(pos.y).toBe(50);
    });

    it('offsets vertically for multiple suggestions', () => {
      const source = mkNode('trigger', { id: 't1', x: 100, y: 50 });
      const graph = mkGraph([source], []);
      const pos0 = suggestedNodePosition(source, graph, 0);
      const pos1 = suggestedNodePosition(source, graph, 1);
      expect(pos1.y).toBeGreaterThan(pos0.y);
    });
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 4.5 — Smart Condition Evaluation
// ═══════════════════════════════════════════════════════════════════════════

describe('Phase 4.5: Smart Condition Evaluation', () => {
  describe('parseConditionExpr', () => {
    it('parses simple comparison', () => {
      const result = parseConditionExpr('data.status === 200');
      expect(result).not.toBeNull();
      expect(result!.left).toBe('data.status');
      expect(result!.operator).toBe('===');
      expect(result!.right).toBe('200');
    });

    it('parses greater-than', () => {
      const result = parseConditionExpr('input.length > 0');
      expect(result).not.toBeNull();
      expect(result!.operator).toBe('>');
    });

    it('returns null for non-comparison strings', () => {
      expect(parseConditionExpr('just a word')).toBeNull();
    });
  });

  describe('resolvePath', () => {
    it('resolves "input" keyword', () => {
      expect(resolvePath('input', null, 'hello')).toBe('hello');
    });

    it('resolves boolean literals', () => {
      expect(resolvePath('true', null)).toBe(true);
      expect(resolvePath('false', null)).toBe(false);
    });

    it('resolves numeric literals', () => {
      expect(resolvePath('42', null)).toBe(42);
    });

    it('resolves string literals', () => {
      expect(resolvePath('"hello"', null)).toBe('hello');
    });

    it('resolves dot-path on data', () => {
      expect(resolvePath('status', { status: 200 })).toBe(200);
    });

    it('resolves nested dot-path', () => {
      expect(resolvePath('data.user.name', { data: { user: { name: 'Alice' } } })).toBe('Alice');
    });

    it('resolves array index', () => {
      expect(resolvePath('items[0]', { items: ['a', 'b'] })).toBe('a');
    });

    it('returns undefined for missing paths', () => {
      expect(resolvePath('missing.path', { x: 1 })).toBeUndefined();
    });
  });

  describe('compareValues', () => {
    it('strict equality', () => {
      expect(compareValues(200, '===', 200)).toBe(true);
      expect(compareValues(200, '===', '200')).toBe(false);
    });

    it('loose equality', () => {
      expect(compareValues(200, '==', '200')).toBe(true);
    });

    it('greater / less than', () => {
      expect(compareValues(10, '>', 5)).toBe(true);
      expect(compareValues(10, '<', 5)).toBe(false);
      expect(compareValues(10, '>=', 10)).toBe(true);
      expect(compareValues(10, '<=', 9)).toBe(false);
    });

    it('not equal', () => {
      expect(compareValues('a', '!==', 'b')).toBe(true);
      expect(compareValues('a', '!==', 'a')).toBe(false);
    });
  });

  describe('evaluateBuiltinCondition', () => {
    it('handles true/yes', () => {
      expect(evaluateBuiltinCondition('true')?.result).toBe(true);
      expect(evaluateBuiltinCondition('YES')?.result).toBe(true);
    });

    it('handles false/no', () => {
      expect(evaluateBuiltinCondition('false')?.result).toBe(false);
      expect(evaluateBuiltinCondition('NO')?.result).toBe(false);
    });

    it('returns null for non-builtin', () => {
      expect(evaluateBuiltinCondition('something else')).toBeNull();
    });
  });

  describe('evaluateSmartCondition', () => {
    it('evaluates simple comparison against JSON input', () => {
      const result = evaluateSmartCondition('data.status === 200', '{"status": 200}');
      expect(result).not.toBeNull();
      expect(result!.result).toBe(true);
      expect(result!.method).toBe('structured');
    });

    it('evaluates string comparison', () => {
      const result = evaluateSmartCondition('data.type === "error"', '{"type": "error"}');
      expect(result).not.toBeNull();
      expect(result!.result).toBe(true);
    });

    it('evaluates numeric comparison', () => {
      const result = evaluateSmartCondition('data.count > 5', '{"count": 10}');
      expect(result).not.toBeNull();
      expect(result!.result).toBe(true);
    });

    it('evaluates compound AND', () => {
      const result = evaluateSmartCondition(
        'data.status === 200 && data.count > 0',
        '{"status": 200, "count": 5}',
      );
      expect(result).not.toBeNull();
      expect(result!.result).toBe(true);
    });

    it('evaluates compound OR', () => {
      const result = evaluateSmartCondition(
        'data.type === "error" || data.type === "warning"',
        '{"type": "warning"}',
      );
      expect(result).not.toBeNull();
      expect(result!.result).toBe(true);
    });

    it('evaluates truthiness check', () => {
      const result = evaluateSmartCondition('data.items', '{"items": [1,2,3]}');
      expect(result).not.toBeNull();
      expect(result!.result).toBe(true);
    });

    it('returns null for complex expressions needing AI', () => {
      const result = evaluateSmartCondition('Is this message positive?', 'Hello!');
      expect(result).toBeNull();
    });

    it('handles boolean literals', () => {
      expect(evaluateSmartCondition('true', '')?.result).toBe(true);
      expect(evaluateSmartCondition('false', '')?.result).toBe(false);
    });
  });
});

// ═══════════════════════════════════════════════════════════════════════════
// 4.6 — Memory-Aware Flows
// ═══════════════════════════════════════════════════════════════════════════

describe('Phase 4.6: Memory-Aware Flows', () => {
  describe('getMemoryWriteConfig', () => {
    it('returns defaults for empty config', () => {
      const config = getMemoryWriteConfig({});
      expect(config.memorySource).toBe(DEFAULT_MEMORY_WRITE.memorySource);
      expect(config.memoryCategory).toBe(DEFAULT_MEMORY_WRITE.memoryCategory);
      expect(config.memoryImportance).toBe(DEFAULT_MEMORY_WRITE.memoryImportance);
    });

    it('extracts custom values', () => {
      const config = getMemoryWriteConfig({
        memorySource: 'custom',
        memoryContent: 'Important fact',
        memoryCategory: 'fact',
        memoryImportance: 0.9,
        memoryAgentId: 'agent-1',
      });
      expect(config.memorySource).toBe('custom');
      expect(config.memoryContent).toBe('Important fact');
      expect(config.memoryCategory).toBe('fact');
      expect(config.memoryImportance).toBe(0.9);
      expect(config.memoryAgentId).toBe('agent-1');
    });
  });

  describe('getMemoryRecallConfig', () => {
    it('returns defaults for empty config', () => {
      const config = getMemoryRecallConfig({});
      expect(config.memoryQuerySource).toBe(DEFAULT_MEMORY_RECALL.memoryQuerySource);
      expect(config.memoryLimit).toBe(DEFAULT_MEMORY_RECALL.memoryLimit);
      expect(config.memoryThreshold).toBe(DEFAULT_MEMORY_RECALL.memoryThreshold);
      expect(config.memoryOutputFormat).toBe(DEFAULT_MEMORY_RECALL.memoryOutputFormat);
    });

    it('extracts custom values', () => {
      const config = getMemoryRecallConfig({
        memoryQuerySource: 'custom',
        memoryQuery: 'What is the user preference?',
        memoryLimit: 10,
        memoryThreshold: 0.5,
        memoryOutputFormat: 'json',
      });
      expect(config.memoryQuerySource).toBe('custom');
      expect(config.memoryQuery).toBe('What is the user preference?');
      expect(config.memoryLimit).toBe(10);
      expect(config.memoryOutputFormat).toBe('json');
    });
  });

  describe('getSquadNodeConfig', () => {
    it('returns defaults for empty config', () => {
      const config = getSquadNodeConfig({});
      expect(config.squadId).toBe(DEFAULT_SQUAD_CONFIG.squadId);
      expect(config.squadTimeoutMs).toBe(DEFAULT_SQUAD_CONFIG.squadTimeoutMs);
      expect(config.squadMaxRounds).toBe(DEFAULT_SQUAD_CONFIG.squadMaxRounds);
    });

    it('extracts custom values', () => {
      const config = getSquadNodeConfig({
        squadId: 'sq-research',
        squadObjective: 'Analyze market trends',
        squadTimeoutMs: 60000,
        squadMaxRounds: 3,
      });
      expect(config.squadId).toBe('sq-research');
      expect(config.squadObjective).toBe('Analyze market trends');
    });
  });

  describe('formatRecalledMemories', () => {
    const memories = [
      { content: 'User prefers dark mode', category: 'preference', importance: 0.8, score: 0.95 },
      { content: 'Earth is round', category: 'fact', importance: 0.5, score: 0.7 },
    ];

    it('formats as text (numbered list)', () => {
      const text = formatRecalledMemories(memories, 'text');
      expect(text).toContain('1.');
      expect(text).toContain('2.');
      expect(text).toContain('preference');
      expect(text).toContain('95%');
    });

    it('formats as JSON', () => {
      const json = formatRecalledMemories(memories, 'json');
      const parsed = JSON.parse(json);
      expect(parsed).toHaveLength(2);
      expect(parsed[0].content).toBe('User prefers dark mode');
    });

    it('handles empty memories (text)', () => {
      expect(formatRecalledMemories([], 'text')).toBe('No relevant memories found.');
    });

    it('handles empty memories (json)', () => {
      expect(formatRecalledMemories([], 'json')).toBe('[]');
    });
  });

  describe('buildMemoryContent', () => {
    it('uses custom content when source is custom', () => {
      const result = buildMemoryContent(
        'node output',
        {
          ...DEFAULT_MEMORY_WRITE,
          memorySource: 'custom',
          memoryContent: 'Custom fact',
        },
        'Test Node',
      );
      expect(result).toBe('Custom fact');
    });

    it('prefixes node output when source is output', () => {
      const result = buildMemoryContent('the result', DEFAULT_MEMORY_WRITE, 'My Agent');
      expect(result).toContain('My Agent');
      expect(result).toContain('the result');
    });
  });

  describe('Memory/Memory-recall node classification', () => {
    it('classifyNode returns direct for memory nodes', () => {
      const node = mkNode('memory' as FlowNodeKind);
      expect(classifyNode(node)).toBe('direct');
    });

    it('classifyNode returns direct for memory-recall nodes', () => {
      const node = mkNode('memory-recall' as FlowNodeKind);
      expect(classifyNode(node)).toBe('direct');
    });
  });

  describe('getNodeExecConfig extracts memory fields', () => {
    it('extracts memory-write fields', () => {
      const node = mkNode('memory' as FlowNodeKind, {
        config: {
          memoryCategory: 'fact',
          memoryImportance: 0.9,
          memorySource: 'custom',
          memoryContent: 'Hello',
        },
      });
      const config = getNodeExecConfig(node);
      expect(config.memoryCategory).toBe('fact');
      expect(config.memoryImportance).toBe(0.9);
      expect(config.memorySource).toBe('custom');
      expect(config.memoryContent).toBe('Hello');
    });

    it('extracts memory-recall fields', () => {
      const node = mkNode('memory-recall' as FlowNodeKind, {
        config: {
          memoryQuerySource: 'custom',
          memoryQuery: 'search this',
          memoryLimit: 10,
          memoryOutputFormat: 'json',
        },
      });
      const config = getNodeExecConfig(node);
      expect(config.memoryQuerySource).toBe('custom');
      expect(config.memoryQuery).toBe('search this');
      expect(config.memoryLimit).toBe(10);
      expect(config.memoryOutputFormat).toBe('json');
    });
  });
});
