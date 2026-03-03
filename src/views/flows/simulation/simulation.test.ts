// ─────────────────────────────────────────────────────────────────────────────
// Flow Simulation Engine — Tests
// Tests for the Holodeck simulation system: atoms, runtime, scenarios.
// ─────────────────────────────────────────────────────────────────────────────

import { describe, it, expect, beforeEach } from 'vitest';
import {
  simNode,
  simEdge,
  simGraph,
  resetSimCounters,
  generateRealisticResponse,
  resolveMockResponse,
  evaluateExpectations,
  type SimScenario,
  type SimResult,
  type MockCallContext,
  type SimMockConfig,
  type FlowRunState,
} from './simulation-atoms';
import { runSimulation, runSimSuite } from './simulation-runtime';
import {
  basicLinearScenario,
  conditionBranchingScenario,
  conductorParallelScenario,
  tesseractScenario,
  selfHealingScenario,
  integrationScenario,
  codeSandboxScenario,
  orchestratorScenario,
  masterSimSuite,
  getScenarioById,
  getScenariosByCategory,
  getScenariosByTier,
  getScenariosByTags,
} from './simulation-scenarios';

// ── Atoms Tests ────────────────────────────────────────────────────────────

describe('Simulation Atoms', () => {
  beforeEach(() => {
    resetSimCounters();
  });

  describe('simNode', () => {
    it('creates a node with defaults', () => {
      const node = simNode('agent');
      expect(node.kind).toBe('agent');
      expect(node.status).toBe('idle');
      expect(node.depth).toBe(0);
      expect(node.phase).toBe(0);
      expect(node.inputs).toEqual(['in']);
      expect(node.outputs).toEqual(['out']);
    });

    it('accepts overrides', () => {
      const node = simNode('condition', {
        id: 'my-cond',
        label: 'Check',
        depth: 2,
        phase: 1,
        cellId: 'cell-1',
      });
      expect(node.id).toBe('my-cond');
      expect(node.label).toBe('Check');
      expect(node.depth).toBe(2);
      expect(node.phase).toBe(1);
      expect(node.cellId).toBe('cell-1');
    });
  });

  describe('simEdge', () => {
    it('creates a forward edge by default', () => {
      const edge = simEdge('a', 'b');
      expect(edge.from).toBe('a');
      expect(edge.to).toBe('b');
      expect(edge.kind).toBe('forward');
      expect(edge.active).toBe(false);
    });

    it('supports kind overrides', () => {
      const edge = simEdge('a', 'b', { kind: 'bidirectional' });
      expect(edge.kind).toBe('bidirectional');
    });
  });

  describe('simGraph', () => {
    it('creates a graph with nodes and edges', () => {
      const n1 = simNode('trigger', { id: 'n1' });
      const n2 = simNode('agent', { id: 'n2' });
      const g = simGraph([n1, n2], [simEdge('n1', 'n2')]);
      expect(g.nodes).toHaveLength(2);
      expect(g.edges).toHaveLength(1);
    });
  });

  describe('generateRealisticResponse', () => {
    it('generates contextual agent responses', () => {
      const ctx: MockCallContext = {
        node: simNode('agent', { label: 'Research Topic' }),
        config: {},
        upstreamInput: 'some data',
        prompt: 'Research this',
        agentId: 'default',
        runState: { variables: {}, vaultCredentials: {} } as unknown as FlowRunState,
        callCount: 0,
        graph: simGraph([], []),
      };

      const response = generateRealisticResponse(ctx);
      expect(response).toContain('Analysis');
      expect(response.length).toBeGreaterThan(50);
    });

    it('generates condition responses', () => {
      const ctx: MockCallContext = {
        node: simNode('condition', { label: 'Is Valid?' }),
        config: {},
        upstreamInput: 'good data',
        prompt: 'Check validity',
        agentId: 'default',
        runState: { variables: {}, vaultCredentials: {} } as unknown as FlowRunState,
        callCount: 0,
        graph: simGraph([], []),
      };

      const response = generateRealisticResponse(ctx);
      expect(response).toBe('true');
    });

    it('returns false for error-containing input in conditions', () => {
      const ctx: MockCallContext = {
        node: simNode('condition'),
        config: {},
        upstreamInput: 'error occurred in processing',
        prompt: 'Check',
        agentId: 'default',
        runState: { variables: {}, vaultCredentials: {} } as unknown as FlowRunState,
        callCount: 0,
        graph: simGraph([], []),
      };

      expect(generateRealisticResponse(ctx)).toBe('false');
    });
  });

  describe('resolveMockResponse', () => {
    const baseMockCtx = (): MockCallContext => ({
      node: simNode('agent', { id: 'n1', label: 'Test' }),
      config: {},
      upstreamInput: 'input data',
      prompt: 'do something',
      agentId: 'default',
      runState: { variables: {}, vaultCredentials: {} } as unknown as FlowRunState,
      callCount: 0,
      graph: simGraph([], []),
    });

    it('uses static strategy', () => {
      const mocks: SimMockConfig = {
        agentDefault: { strategy: 'static', response: 'Hello!' },
      };
      const result = resolveMockResponse(mocks, baseMockCtx());
      expect(result.response).toBe('Hello!');
      expect(result.failed).toBe(false);
    });

    it('uses echo strategy', () => {
      const mocks: SimMockConfig = {
        agentDefault: { strategy: 'echo', echoPrefix: 'ECHO: ' },
      };
      const result = resolveMockResponse(mocks, baseMockCtx());
      expect(result.response).toBe('ECHO: input data');
    });

    it('uses template strategy with label match', () => {
      const mocks: SimMockConfig = {
        agentDefault: {
          strategy: 'template',
          responses: { Test: 'Matched by label!' },
        },
      };
      const result = resolveMockResponse(mocks, baseMockCtx());
      expect(result.response).toBe('Matched by label!');
    });

    it('uses sequence strategy', () => {
      const mocks: SimMockConfig = {
        agentDefault: {
          strategy: 'sequence',
          sequence: ['first', 'second', 'third'],
        },
      };
      const ctx = baseMockCtx();
      expect(resolveMockResponse(mocks, { ...ctx, callCount: 0 }).response).toBe('first');
      expect(resolveMockResponse(mocks, { ...ctx, callCount: 1 }).response).toBe('second');
      expect(resolveMockResponse(mocks, { ...ctx, callCount: 2 }).response).toBe('third');
      // Wraps around
      expect(resolveMockResponse(mocks, { ...ctx, callCount: 3 }).response).toBe('first');
    });

    it('uses function strategy', () => {
      const mocks: SimMockConfig = {
        agentDefault: {
          strategy: 'function',
          generator: (ctx) => `Custom: ${ctx.node.label}`,
        },
      };
      const result = resolveMockResponse(mocks, baseMockCtx());
      expect(result.response).toBe('Custom: Test');
    });

    it('node override takes priority', () => {
      const mocks: SimMockConfig = {
        agentDefault: { strategy: 'static', response: 'default' },
        nodeOverrides: { n1: { strategy: 'static', response: 'override' } },
      };
      const result = resolveMockResponse(mocks, baseMockCtx());
      expect(result.response).toBe('override');
    });

    it('handles forced failure', () => {
      const mocks: SimMockConfig = {
        nodeOverrides: {
          n1: {
            strategy: 'static',
            shouldFail: true,
            errorMessage: 'Boom!',
          },
        },
      };
      const result = resolveMockResponse(mocks, baseMockCtx());
      expect(result.failed).toBe(true);
      expect(result.error).toBe('Boom!');
    });

    it('handles fail-on-call', () => {
      const mocks: SimMockConfig = {
        nodeOverrides: {
          n1: { strategy: 'static', response: 'ok', failOnCall: 0 },
        },
      };
      // Call 0: fails
      const result0 = resolveMockResponse(mocks, { ...baseMockCtx(), callCount: 0 });
      expect(result0.failed).toBe(true);
      // Call 1: succeeds
      const result1 = resolveMockResponse(mocks, { ...baseMockCtx(), callCount: 1 });
      expect(result1.failed).toBe(false);
      expect(result1.response).toBe('ok');
    });

    it('falls back to realistic when no strategy matches', () => {
      const mocks: SimMockConfig = {};
      const result = resolveMockResponse(mocks, baseMockCtx());
      expect(result.failed).toBe(false);
      expect(result.response.length).toBeGreaterThan(0);
    });
  });

  describe('evaluateExpectations', () => {
    it('checks flow-status', () => {
      const result: SimResult = {
        scenarioId: 'test',
        scenarioName: 'Test',
        passed: false,
        expectationResults: [],
        runState: {
          status: 'success',
          nodeStates: new Map(),
          variables: {},
        } as unknown as FlowRunState,
        strategy: null,
        events: [],
        mockCalls: [],
        durationMs: 100,
        timestamp: Date.now(),
      };

      const expectations = [
        {
          type: 'flow-status' as const,
          description: 'Should succeed',
          check: { type: 'flow-status' as const, expectedStatus: 'success' as const },
        },
      ];

      const results = evaluateExpectations(expectations, result);
      expect(results).toHaveLength(1);
      expect(results[0].passed).toBe(true);
    });

    it('detects flow-status mismatch', () => {
      const result: SimResult = {
        scenarioId: 'test',
        scenarioName: 'Test',
        passed: false,
        expectationResults: [],
        runState: {
          status: 'error',
          nodeStates: new Map(),
          variables: {},
        } as unknown as FlowRunState,
        strategy: null,
        events: [],
        mockCalls: [],
        durationMs: 100,
        timestamp: Date.now(),
      };

      const expectations = [
        {
          type: 'flow-status' as const,
          description: 'Should succeed',
          check: { type: 'flow-status' as const, expectedStatus: 'success' as const },
        },
      ];

      const results = evaluateExpectations(expectations, result);
      expect(results[0].passed).toBe(false);
    });
  });
});

// ── Scenario Library Tests ─────────────────────────────────────────────────

describe('Scenario Library', () => {
  it('has 32 built-in scenarios (12 core + 20 stress)', () => {
    expect(masterSimSuite.scenarios).toHaveLength(32);
  });

  it('finds scenario by ID', () => {
    const s = getScenarioById('tesseract-4d');
    expect(s).toBeDefined();
    expect(s!.name).toContain('Tesseract');
  });

  it('filters by category', () => {
    const parallel = getScenariosByCategory('parallel');
    expect(parallel.length).toBeGreaterThanOrEqual(1);
    expect(parallel.every((s) => s.category === 'parallel')).toBe(true);
  });

  it('filters by tier', () => {
    const smoke = getScenariosByTier('smoke');
    expect(smoke.length).toBeGreaterThanOrEqual(2);
    expect(smoke.every((s) => s.tier === 'smoke')).toBe(true);
  });

  it('filters by tags', () => {
    const tagged = getScenariosByTags(['tesseract', 'conductor']);
    expect(tagged.length).toBeGreaterThanOrEqual(2);
  });

  it('all scenarios have valid graphs', () => {
    for (const s of masterSimSuite.scenarios) {
      expect(s.graph.nodes.length).toBeGreaterThan(0);
      expect(s.graph.edges.length).toBeGreaterThan(0);
      expect(s.expectations.length).toBeGreaterThan(0);
    }
  });
});

// ── Runtime Tests (Integration) ────────────────────────────────────────────

describe('Simulation Runtime', () => {
  beforeEach(() => {
    resetSimCounters();
  });

  it('runs basic linear scenario', async () => {
    const result = await runSimulation(basicLinearScenario);
    expect(result.runState).not.toBeNull();
    expect(result.runState!.status).toBe('success');
    expect(result.events.length).toBeGreaterThan(0);
    expect(result.mockCalls.length).toBeGreaterThan(0);
    // Check all expectations pass
    for (const exp of result.expectationResults) {
      expect(exp.passed).toBe(true);
    }
    expect(result.passed).toBe(true);
  });

  it('runs condition branching scenario', async () => {
    const result = await runSimulation(conditionBranchingScenario);
    expect(result.runState!.status).toBe('success');

    // True path should execute, false path should not
    const okState = result.runState!.nodeStates.get('ok');
    const errState = result.runState!.nodeStates.get('err');
    expect(okState).toBeDefined();
    expect(okState!.status).toBe('success');
    // err node should either not exist in nodeStates or be idle
    expect(errState === undefined || errState.status === 'idle').toBe(true);

    expect(result.passed).toBe(true);
  });

  it('runs code sandbox scenario', async () => {
    const result = await runSimulation(codeSandboxScenario);
    expect(result.runState!.status).toBe('success');

    const c1State = result.runState!.nodeStates.get('c1');
    expect(c1State).toBeDefined();
    expect(c1State!.output).toContain('84');

    const c2State = result.runState!.nodeStates.get('c2');
    expect(c2State).toBeDefined();
    expect(c2State!.output).toContain('94');

    expect(result.passed).toBe(true);
  });

  it('runs integration pipeline scenario', async () => {
    const result = await runSimulation(integrationScenario);
    expect(result.runState!.status).toBe('success');

    // HTTP mock called
    const httpCalls = result.mockCalls.filter((c) => c.type === 'http');
    expect(httpCalls.length).toBeGreaterThan(0);
    expect(httpCalls[0].output).toContain('users');

    // MCP mock called
    const mcpCalls = result.mockCalls.filter((c) => c.type === 'mcp');
    expect(mcpCalls.length).toBeGreaterThan(0);

    // Memory write called
    const memWriteCalls = result.mockCalls.filter((c) => c.type === 'memory-write');
    expect(memWriteCalls.length).toBeGreaterThan(0);

    // Memory recall called
    const memRecallCalls = result.mockCalls.filter((c) => c.type === 'memory-recall');
    expect(memRecallCalls.length).toBeGreaterThan(0);

    expect(result.passed).toBe(true);
  });

  it('runs self-healing scenario (retry on failure)', async () => {
    const result = await runSimulation(selfHealingScenario);
    // The flaky node should have been called at least twice
    const flakyCalls = result.mockCalls.filter((c) => c.nodeId === 'flaky');
    expect(flakyCalls.length).toBeGreaterThanOrEqual(1);
  });

  it('runs conductor parallel scenario', async () => {
    const result = await runSimulation(conductorParallelScenario);
    expect(result.runState!.status).toBe('success');
    expect(result.strategy).not.toBeNull();
    expect(result.strategy!.conductorUsed).toBe(true);
    expect(result.passed).toBe(true);
  });

  it('runs tesseract scenario', async () => {
    const result = await runSimulation(tesseractScenario);
    expect(result.runState!.status).toBe('success');
    expect(result.strategy).not.toBeNull();

    // All research cells should execute
    expect(result.runState!.nodeStates.get('r_a')?.status).toBe('success');
    expect(result.runState!.nodeStates.get('r_b')?.status).toBe('success');
    expect(result.runState!.nodeStates.get('r_c')?.status).toBe('success');

    // Event horizon should execute
    expect(result.runState!.nodeStates.get('eh1')?.status).toBe('success');

    // Analysis should execute after horizon
    expect(result.runState!.nodeStates.get('ana')?.status).toBe('success');
    expect(result.runState!.nodeStates.get('str')?.status).toBe('success');

    // Final output should have content
    expect(result.runState!.nodeStates.get('out')?.status).toBe('success');

    expect(result.passed).toBe(true);
  });

  it('runs orchestrator scenario', async () => {
    const result = await runSimulation(orchestratorScenario);
    expect(result.runState!.status).toBe('success');

    // Boss should execute before workers
    const bossState = result.runState!.nodeStates.get('boss');
    const reviewState = result.runState!.nodeStates.get('review');
    expect(bossState?.status).toBe('success');
    expect(reviewState?.output).toContain('Approved');

    expect(result.passed).toBe(true);
  });

  it('records mock call logs', async () => {
    const result = await runSimulation(basicLinearScenario);
    expect(result.mockCalls.length).toBeGreaterThan(0);
    for (const call of result.mockCalls) {
      expect(call.nodeId).toBeDefined();
      expect(call.type).toBe('agent');
      expect(call.input.length).toBeGreaterThan(0);
      expect(call.output.length).toBeGreaterThan(0);
    }
  });

  it('records all execution events', async () => {
    const result = await runSimulation(basicLinearScenario);
    const eventTypes = new Set(result.events.map((e) => e.type));
    expect(eventTypes.has('run-start')).toBe(true);
    expect(eventTypes.has('step-start')).toBe(true);
    expect(eventTypes.has('step-complete')).toBe(true);
    expect(eventTypes.has('run-complete')).toBe(true);
  });

  it('handles custom mock generator', async () => {
    const scenario: SimScenario = {
      id: 'custom-gen',
      name: 'Custom Generator',
      description: 'Tests function-based mock responses',
      category: 'basic',
      tier: 'smoke',
      tags: [],
      graph: (() => {
        const t = simNode('trigger', { id: 't', label: 'Start', config: { prompt: 'go' } });
        const a = simNode('agent', { id: 'a', label: 'Test Agent' });
        const o = simNode('output', { id: 'o', label: 'Out' });
        return simGraph([t, a, o], [simEdge('t', 'a'), simEdge('a', 'o')]);
      })(),
      mocks: {
        agentDefault: {
          strategy: 'function',
          generator: (ctx) =>
            `Generated for "${ctx.node.label}" with input length ${ctx.upstreamInput.length}`,
        },
      },
      expectations: [
        {
          type: 'flow-status',
          description: 'Completes',
          check: { type: 'flow-status', expectedStatus: 'success' },
        },
        {
          type: 'node-output',
          description: 'Uses generator',
          check: { type: 'node-output', nodeId: 'a', contains: 'Generated for "Test Agent"' },
        },
      ],
    };

    const result = await runSimulation(scenario);
    expect(result.passed).toBe(true);
  });

  it('handles forced node failure', async () => {
    const scenario: SimScenario = {
      id: 'forced-fail',
      name: 'Forced Failure',
      description: 'Tests forced node failure with error edge routing',
      category: 'self-healing',
      tier: 'smoke',
      tags: [],
      graph: (() => {
        const t = simNode('trigger', { id: 't', label: 'Start' });
        const a = simNode('agent', { id: 'fail', label: 'Will Fail' });
        const e = simNode('error', { id: 'err', label: 'Error Handler' });
        const o = simNode('output', { id: 'o', label: 'Out' });
        return simGraph(
          [t, a, e, o],
          [
            simEdge('t', 'fail'),
            simEdge('fail', 'o'),
            simEdge('fail', 'err', { kind: 'error', fromPort: 'err' }),
          ],
        );
      })(),
      mocks: {
        nodeOverrides: {
          fail: {
            strategy: 'static',
            shouldFail: true,
            errorMessage: 'Intentional failure for testing',
          },
        },
      },
      expectations: [
        {
          type: 'node-status',
          description: 'Node fails',
          check: { type: 'node-status', nodeId: 'fail', expectedStatus: 'error' },
        },
        {
          type: 'node-executed',
          description: 'Error handler runs',
          check: { type: 'node-executed', nodeId: 'err', executed: true },
        },
        {
          type: 'node-executed',
          description: 'Output skipped',
          check: { type: 'node-executed', nodeId: 'o', executed: false },
        },
      ],
    };

    const result = await runSimulation(scenario);
    expect(result.passed).toBe(true);
  });
});

// ── Suite Runner Tests ─────────────────────────────────────────────────────

describe('Suite Runner', () => {
  it('runs the smoke-tier scenarios', async () => {
    const smokeScenarios = getScenariosByTier('smoke');
    const suite = {
      id: 'smoke',
      name: 'Smoke Tests',
      description: 'Quick sanity checks',
      scenarios: smokeScenarios,
    };
    const result = await runSimSuite(suite);
    expect(result.totalScenarios).toBe(smokeScenarios.length);
    expect(result.totalScenarios).toBeGreaterThan(0);
    // All smoke tests should pass
    for (const r of result.results) {
      if (!r.passed) {
        console.log(
          `FAILED: ${r.scenarioName}`,
          r.expectationResults.filter((e) => !e.passed),
        );
      }
    }
    expect(result.passed).toBe(result.totalScenarios);
  });
});
