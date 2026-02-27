// ─────────────────────────────────────────────────────────────────────────────
// Flow Execution Engine — Atoms Tests
// ─────────────────────────────────────────────────────────────────────────────

import { describe, it, expect } from 'vitest';
import {
  buildExecutionPlan,
  getUpstreamNodes,
  getDownstreamNodes,
  collectNodeInput,
  buildNodePrompt,
  evaluateCondition,
  resolveConditionEdges,
  createFlowRunState,
  createNodeRunState,
  createRunId,
  getNodeExecConfig,
  validateFlowForExecution,
  summarizeRun,
  executeCodeSandboxed,
  type NodeRunState,
} from './executor-atoms';
import { createGraph, createNode, createEdge, type FlowGraph } from './atoms';

// ── Helpers ────────────────────────────────────────────────────────────────

function linearGraph(): FlowGraph {
  const n1 = createNode('trigger', 'Start', 0, 0);
  const n2 = createNode('agent', 'Research', 200, 0, { description: 'Research the topic' });
  const n3 = createNode('agent', 'Summarize', 400, 0, { description: 'Summarize findings' });
  const n4 = createNode('output', 'Report', 600, 0);
  const g = createGraph(
    'Test Flow',
    [n1, n2, n3, n4],
    [createEdge(n1.id, n2.id), createEdge(n2.id, n3.id), createEdge(n3.id, n4.id)],
  );
  return g;
}

function branchingGraph(): FlowGraph {
  const n1 = createNode('trigger', 'Input', 0, 0);
  const n2 = createNode('condition', 'Is Valid?', 200, 0);
  const n3 = createNode('agent', 'Process', 400, 0);
  const n4 = createNode('output', 'Error', 400, 100);
  const g = createGraph(
    'Branch Flow',
    [n1, n2, n3, n4],
    [
      createEdge(n1.id, n2.id),
      createEdge(n2.id, n3.id, 'forward', { label: 'true' }),
      createEdge(n2.id, n4.id, 'forward', { label: 'false' }),
    ],
  );
  return g;
}

// ── Tests ──────────────────────────────────────────────────────────────────

describe('buildExecutionPlan', () => {
  it('returns nodes in topological order for a linear chain', () => {
    const g = linearGraph();
    const plan = buildExecutionPlan(g);
    expect(plan).toHaveLength(4);
    // Trigger should come first
    expect(plan[0]).toBe(g.nodes[0].id);
    // Output should come last
    expect(plan[3]).toBe(g.nodes[3].id);
  });

  it('handles branching graphs', () => {
    const g = branchingGraph();
    const plan = buildExecutionPlan(g);
    expect(plan).toHaveLength(4);
    // Trigger first
    expect(plan[0]).toBe(g.nodes[0].id);
    // Condition second
    expect(plan[1]).toBe(g.nodes[1].id);
  });

  it('handles empty graphs', () => {
    const g = createGraph('Empty');
    expect(buildExecutionPlan(g)).toEqual([]);
  });

  it('handles single-node graphs', () => {
    const n = createNode('agent', 'Solo', 0, 0);
    const g = createGraph('Solo', [n]);
    expect(buildExecutionPlan(g)).toEqual([n.id]);
  });

  it('puts triggers before other root nodes', () => {
    const a = createNode('agent', 'Agent A', 0, 0);
    const t = createNode('trigger', 'Trigger', 0, 0);
    const g = createGraph('Test', [a, t]);
    const plan = buildExecutionPlan(g);
    expect(plan[0]).toBe(t.id);
  });
});

describe('getUpstreamNodes / getDownstreamNodes', () => {
  it('finds upstream nodes', () => {
    const g = linearGraph();
    const secondNode = g.nodes[1].id;
    const upstream = getUpstreamNodes(g, secondNode);
    expect(upstream).toEqual([g.nodes[0].id]);
  });

  it('finds downstream nodes', () => {
    const g = linearGraph();
    const secondNode = g.nodes[1].id;
    const downstream = getDownstreamNodes(g, secondNode);
    expect(downstream).toEqual([g.nodes[2].id]);
  });

  it('returns empty for root nodes upstream', () => {
    const g = linearGraph();
    expect(getUpstreamNodes(g, g.nodes[0].id)).toEqual([]);
  });

  it('returns empty for leaf nodes downstream', () => {
    const g = linearGraph();
    expect(getDownstreamNodes(g, g.nodes[3].id)).toEqual([]);
  });
});

describe('collectNodeInput', () => {
  it('joins upstream outputs', () => {
    const g = linearGraph();
    const states = new Map<string, NodeRunState>();
    states.set(g.nodes[0].id, createNodeRunState(g.nodes[0].id));
    states.get(g.nodes[0].id)!.output = 'Hello from trigger';
    const input = collectNodeInput(g, g.nodes[1].id, states);
    expect(input).toBe('Hello from trigger');
  });

  it('returns empty when no upstream output', () => {
    const g = linearGraph();
    const states = new Map<string, NodeRunState>();
    const input = collectNodeInput(g, g.nodes[1].id, states);
    expect(input).toBe('');
  });

  it('joins multiple upstream outputs with double newline', () => {
    const n1 = createNode('agent', 'A', 0, 0);
    const n2 = createNode('agent', 'B', 0, 100);
    const n3 = createNode('output', 'Out', 200, 50);
    const g = createGraph(
      'Multi',
      [n1, n2, n3],
      [createEdge(n1.id, n3.id), createEdge(n2.id, n3.id)],
    );
    const states = new Map<string, NodeRunState>();
    const s1 = createNodeRunState(n1.id);
    s1.output = 'From A';
    const s2 = createNodeRunState(n2.id);
    s2.output = 'From B';
    states.set(n1.id, s1);
    states.set(n2.id, s2);
    expect(collectNodeInput(g, n3.id, states)).toBe('From A\n\nFrom B');
  });
});

describe('buildNodePrompt', () => {
  it('includes upstream input when available', () => {
    const n = createNode('agent', 'Step 1', 0, 0);
    const prompt = buildNodePrompt(n, 'upstream data', {});
    expect(prompt).toContain('upstream data');
    expect(prompt).toContain('Step 1');
  });

  it('uses configured prompt when present', () => {
    const n = createNode('agent', 'Step 1', 0, 0);
    const prompt = buildNodePrompt(n, '', { prompt: 'Do this thing' });
    expect(prompt).toContain('Do this thing');
  });

  it('builds condition prompts correctly', () => {
    const n = createNode('condition', 'Check', 0, 0);
    const prompt = buildNodePrompt(n, 'some data', { conditionExpr: 'is valid?' });
    expect(prompt).toContain('is valid?');
    expect(prompt).toContain('true');
  });

  it('handles output nodes by passing through', () => {
    const n = createNode('output', 'Result', 0, 0);
    const prompt = buildNodePrompt(n, 'final data', {});
    expect(prompt).toContain('final data');
  });

  it('uses description when prompt is absent', () => {
    const n = createNode('agent', 'Analyze', 0, 0, { description: 'Analyze sentiment' });
    const prompt = buildNodePrompt(n, '', {});
    expect(prompt).toContain('Analyze sentiment');
  });
});

describe('evaluateCondition', () => {
  it('returns true for "true"', () => expect(evaluateCondition('true')).toBe(true));
  it('returns true for "yes"', () => expect(evaluateCondition('yes')).toBe(true));
  it('returns true for "True"', () => expect(evaluateCondition('True')).toBe(true));
  it('returns true for "1"', () => expect(evaluateCondition('1')).toBe(true));
  it('returns false for "false"', () => expect(evaluateCondition('false')).toBe(false));
  it('returns false for "no"', () => expect(evaluateCondition('no')).toBe(false));
  it('returns false for "0"', () => expect(evaluateCondition('0')).toBe(false));
  it('returns true for verbose "The answer is true"', () =>
    expect(evaluateCondition('The answer is true')).toBe(true));
  it('returns false for garbage', () => expect(evaluateCondition('maybe')).toBe(false));
});

describe('resolveConditionEdges', () => {
  it('returns true-labeled edges when condition is true', () => {
    const g = branchingGraph();
    const condNode = g.nodes[1];
    const edges = resolveConditionEdges(g, condNode.id, true);
    expect(edges).toHaveLength(1);
    expect(edges[0].to).toBe(g.nodes[2].id); // Process
  });

  it('returns false-labeled edges when condition is false', () => {
    const g = branchingGraph();
    const condNode = g.nodes[1];
    const edges = resolveConditionEdges(g, condNode.id, false);
    expect(edges).toHaveLength(1);
    expect(edges[0].to).toBe(g.nodes[3].id); // Error
  });

  it('returns unlabeled edges regardless of condition', () => {
    const n1 = createNode('condition', 'C', 0, 0);
    const n2 = createNode('output', 'Out', 200, 0);
    const g = createGraph('Test', [n1, n2], [createEdge(n1.id, n2.id)]);
    expect(resolveConditionEdges(g, n1.id, true)).toHaveLength(1);
    expect(resolveConditionEdges(g, n1.id, false)).toHaveLength(1);
  });
});

describe('createRunId / createFlowRunState', () => {
  it('generates unique run IDs', () => {
    const a = createRunId();
    const b = createRunId();
    expect(a).not.toBe(b);
    expect(a).toMatch(/^run_/);
  });

  it('creates valid initial run state', () => {
    const state = createFlowRunState('g1', ['n1', 'n2']);
    expect(state.graphId).toBe('g1');
    expect(state.plan).toEqual(['n1', 'n2']);
    expect(state.status).toBe('idle');
    expect(state.currentStep).toBe(0);
    expect(state.nodeStates.size).toBe(0);
  });
});

describe('getNodeExecConfig', () => {
  it('extracts config fields from node', () => {
    const n = createNode('agent', 'Test', 0, 0, {
      config: { prompt: 'Do stuff', agentId: 'a1', model: 'gpt-4' },
    });
    const c = getNodeExecConfig(n);
    expect(c.prompt).toBe('Do stuff');
    expect(c.agentId).toBe('a1');
    expect(c.model).toBe('gpt-4');
  });

  it('returns defaults for empty config', () => {
    const n = createNode('agent', 'Test', 0, 0);
    const c = getNodeExecConfig(n);
    expect(c.prompt).toBeUndefined();
    expect(c.outputTarget).toBe('chat');
    expect(c.maxRetries).toBe(0);
    expect(c.timeoutMs).toBe(120_000);
  });
});

describe('validateFlowForExecution', () => {
  it('returns empty for valid linear flow', () => {
    const g = linearGraph();
    expect(validateFlowForExecution(g)).toEqual([]);
  });

  it('returns error for empty graph', () => {
    const g = createGraph('Empty');
    const errors = validateFlowForExecution(g);
    expect(errors).toHaveLength(1);
    expect(errors[0].message).toContain('no nodes');
  });

  it('allows single-node graphs', () => {
    const n = createNode('agent', 'Solo', 0, 0);
    n.description = 'I have instructions';
    const g = createGraph('Solo', [n]);
    expect(validateFlowForExecution(g)).toEqual([]);
  });

  it('warns about disconnected nodes', () => {
    const n1 = createNode('agent', 'A', 0, 0);
    const n2 = createNode('agent', 'B', 200, 0);
    const n3 = createNode('agent', 'Orphan', 400, 0);
    const g = createGraph('Test', [n1, n2, n3], [createEdge(n1.id, n2.id)]);
    const errors = validateFlowForExecution(g);
    expect(errors.some((e) => e.message.includes('disconnected'))).toBe(true);
  });

  it('warns about agent nodes without prompts', () => {
    const n = createNode('agent', 'Empty Agent', 0, 0);
    const g = createGraph('Test', [n]);
    const errors = validateFlowForExecution(g);
    expect(errors.some((e) => e.message.includes('no prompt'))).toBe(true);
  });
});

describe('summarizeRun', () => {
  it('produces readable output', () => {
    const g = linearGraph();
    const state = createFlowRunState(
      g.id,
      g.nodes.map((n) => n.id),
    );
    state.status = 'success';
    state.totalDurationMs = 5000;
    state.outputLog = [
      {
        nodeId: g.nodes[0].id,
        nodeLabel: 'Start',
        nodeKind: 'trigger',
        status: 'success',
        output: 'started',
        durationMs: 100,
        timestamp: Date.now(),
      },
      {
        nodeId: g.nodes[1].id,
        nodeLabel: 'Research',
        nodeKind: 'agent',
        status: 'success',
        output: 'Found data',
        durationMs: 2000,
        timestamp: Date.now(),
      },
    ];
    const summary = summarizeRun(state, g);
    expect(summary).toContain('Test Flow');
    expect(summary).toContain('✓');
    expect(summary).toContain('Research');
  });
});

// ── Debug Mode Tests ─────────────────────────────────────────────────────

describe('debug event types', () => {
  it('debug-cursor event has correct shape', () => {
    const event: import('./executor-atoms').FlowExecEvent = {
      type: 'debug-cursor',
      runId: 'run_1',
      nodeId: 'node_1',
      stepIndex: 0,
    };
    expect(event.type).toBe('debug-cursor');
    expect(event.nodeId).toBe('node_1');
    expect(event.stepIndex).toBe(0);
  });

  it('debug-breakpoint-hit event has correct shape', () => {
    const event: import('./executor-atoms').FlowExecEvent = {
      type: 'debug-breakpoint-hit',
      runId: 'run_1',
      nodeId: 'node_2',
      stepIndex: 1,
    };
    expect(event.type).toBe('debug-breakpoint-hit');
  });

  it('debug-edge-value event has correct shape', () => {
    const event: import('./executor-atoms').FlowExecEvent = {
      type: 'debug-edge-value',
      runId: 'run_1',
      edgeId: 'edge_1',
      value: 'Hello world',
    };
    expect(event.type).toBe('debug-edge-value');
    expect(event.value).toBe('Hello world');
  });
});

describe('createFlowRunState for debug', () => {
  it('starts with idle status and step 0', () => {
    const g = linearGraph();
    const plan = buildExecutionPlan(g);
    const state = createFlowRunState(g.id, plan);
    expect(state.status).toBe('idle');
    expect(state.currentStep).toBe(0);
    expect(state.plan).toHaveLength(4);
  });

  it('nodeStates map supports input/output inspection', () => {
    const g = linearGraph();
    const plan = buildExecutionPlan(g);
    const state = createFlowRunState(g.id, plan);

    const ns = createNodeRunState(plan[0]);
    ns.input = 'test input';
    ns.output = 'test output';
    ns.status = 'success';
    state.nodeStates.set(plan[0], ns);

    const retrieved = state.nodeStates.get(plan[0]);
    expect(retrieved?.input).toBe('test input');
    expect(retrieved?.output).toBe('test output');
    expect(retrieved?.status).toBe('success');
  });

  it('plan preserves order for step-by-step traversal', () => {
    const g = linearGraph();
    const plan = buildExecutionPlan(g);
    // Trigger → Research → Summarize → Report
    expect(plan[0]).toBe(g.nodes[0].id); // trigger
    expect(plan[plan.length - 1]).toBe(g.nodes[3].id); // output
    // Each step can be iterated individually
    for (let i = 0; i < plan.length; i++) {
      const node = g.nodes.find((n) => n.id === plan[i]);
      expect(node).toBeDefined();
    }
  });
});

// ── Code Sandbox Tests ───────────────────────────────────────────────────

describe('executeCodeSandboxed', () => {
  it('executes simple return statement', () => {
    const result = executeCodeSandboxed('return 42;', '');
    expect(result.output).toBe('42');
    expect(result.error).toBeUndefined();
  });

  it('receives input string', () => {
    const result = executeCodeSandboxed('return input.toUpperCase();', 'hello world');
    expect(result.output).toBe('HELLO WORLD');
  });

  it('parses JSON data from input', () => {
    const result = executeCodeSandboxed('return data.name;', '{"name":"Pawz"}');
    expect(result.output).toBe('Pawz');
  });

  it('handles data as null for non-JSON input', () => {
    const result = executeCodeSandboxed(
      'return data === null ? "null" : "not null";',
      'plain text',
    );
    expect(result.output).toBe('null');
  });

  it('captures console.log output', () => {
    const result = executeCodeSandboxed('console.log("debug info"); return "done";', '');
    expect(result.output).toContain('done');
    expect(result.output).toContain('debug info');
  });

  it('blocks window access', () => {
    const result = executeCodeSandboxed('return window.location;', '');
    expect(result.error).toContain('forbidden');
  });

  it('blocks document access', () => {
    const result = executeCodeSandboxed('return document.cookie;', '');
    expect(result.error).toContain('forbidden');
  });

  it('blocks fetch access', () => {
    const result = executeCodeSandboxed('return fetch("http://evil.com");', '');
    expect(result.error).toContain('forbidden');
  });

  it('blocks eval access', () => {
    const result = executeCodeSandboxed('return eval("1+1");', '');
    expect(result.error).toContain('forbidden');
  });

  it('handles runtime errors gracefully', () => {
    const result = executeCodeSandboxed('throw new Error("test error");', '');
    expect(result.error).toBe('test error');
    expect(result.output).toBe('');
  });

  it('returns stringified objects', () => {
    const result = executeCodeSandboxed('return { x: 1, y: 2 };', '');
    const parsed = JSON.parse(result.output);
    expect(parsed.x).toBe(1);
    expect(parsed.y).toBe(2);
  });

  it('allows Math usage', () => {
    const result = executeCodeSandboxed('return Math.max(3, 7, 1);', '');
    expect(result.output).toBe('7');
  });

  it('allows Array methods', () => {
    const result = executeCodeSandboxed('return [1,2,3].map(x => x * 2).join(",");', '');
    expect(result.output).toBe('2,4,6');
  });

  it('returns fallback message for no output', () => {
    const result = executeCodeSandboxed('const x = 1;', '');
    expect(result.output).toBe('Code executed (no output)');
  });
});

// ── Schedule / Cron Tests ──────────────────────────────────────────────────

import { nextCronFire, validateCron, describeCron, CRON_PRESETS } from './executor-atoms';

describe('validateCron', () => {
  it('accepts valid 5-field expressions', () => {
    expect(validateCron('* * * * *')).toBeNull();
    expect(validateCron('*/5 * * * *')).toBeNull();
    expect(validateCron('0 9 * * 1-5')).toBeNull();
    expect(validateCron('0 0 1 * *')).toBeNull();
    expect(validateCron('30 14 * * 1,3,5')).toBeNull();
  });

  it('rejects expressions with wrong field count', () => {
    expect(validateCron('* * *')).toContain('5 fields');
    expect(validateCron('* * * * * *')).toContain('5 fields');
    expect(validateCron('')).toContain('5 fields');
  });

  it('rejects invalid field values', () => {
    const result = validateCron('abc * * * *');
    expect(result).toContain('minute');
  });
});

describe('describeCron', () => {
  it('returns preset description for known expressions', () => {
    expect(describeCron('* * * * *')).toBe('Runs every 60 seconds');
    expect(describeCron('0 9 * * 1-5')).toBe('Mon–Fri at 09:00');
    expect(describeCron('0 0 1 * *')).toBe('First day of every month at 00:00');
  });

  it('returns raw schedule string for unknown expressions', () => {
    expect(describeCron('23 4 * * *')).toBe('Schedule: 23 4 * * *');
  });
});

describe('nextCronFire', () => {
  it('returns null for invalid expressions', () => {
    expect(nextCronFire('bad')).toBeNull();
    expect(nextCronFire('* * *')).toBeNull();
  });

  it('finds next minute for "* * * * *"', () => {
    const from = new Date('2025-01-15T10:30:00');
    const next = nextCronFire('* * * * *', from);
    expect(next).not.toBeNull();
    expect(next!.getMinutes()).toBe(31);
    expect(next!.getHours()).toBe(10);
  });

  it('finds next 5-minute mark for "*/5 * * * *"', () => {
    const from = new Date('2025-01-15T10:32:00');
    const next = nextCronFire('*/5 * * * *', from);
    expect(next).not.toBeNull();
    expect(next!.getMinutes()).toBe(35);
  });

  it('finds next occurrence for hourly "0 * * * *"', () => {
    const from = new Date('2025-01-15T10:30:00');
    const next = nextCronFire('0 * * * *', from);
    expect(next).not.toBeNull();
    expect(next!.getMinutes()).toBe(0);
    expect(next!.getHours()).toBe(11);
  });

  it('finds next weekday for "0 9 * * 1-5"', () => {
    // 2025-01-18 is a Saturday
    const from = new Date('2025-01-18T08:00:00');
    const next = nextCronFire('0 9 * * 1-5', from);
    expect(next).not.toBeNull();
    // Should be Monday Jan 20 at 09:00
    expect(next!.getDay()).toBe(1); // Monday
    expect(next!.getHours()).toBe(9);
    expect(next!.getMinutes()).toBe(0);
  });

  it('handles list fields "0 9 * * 1,3,5"', () => {
    // 2025-01-14 is a Tuesday
    const from = new Date('2025-01-14T10:00:00');
    const next = nextCronFire('0 9 * * 1,3,5', from);
    expect(next).not.toBeNull();
    // Next is Wednesday (3)
    expect(next!.getDay()).toBe(3);
  });

  it('handles monthly "0 0 1 * *"', () => {
    const from = new Date('2025-01-15T10:00:00');
    const next = nextCronFire('0 0 1 * *', from);
    expect(next).not.toBeNull();
    expect(next!.getDate()).toBe(1);
    expect(next!.getMonth()).toBe(1); // February
  });
});

describe('CRON_PRESETS', () => {
  it('has 10 entries', () => {
    expect(CRON_PRESETS).toHaveLength(10);
  });

  it('all presets have valid cron expressions', () => {
    for (const preset of CRON_PRESETS) {
      expect(validateCron(preset.value)).toBeNull();
    }
  });

  it('all presets have label, value, and description', () => {
    for (const preset of CRON_PRESETS) {
      expect(preset.label).toBeTruthy();
      expect(preset.value).toBeTruthy();
      expect(preset.description).toBeTruthy();
    }
  });
});
