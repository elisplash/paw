// ─────────────────────────────────────────────────────────────────────────────
// Flow Simulation Engine — Scenario Library
// Pre-built scenarios covering every flow system capability:
//   - Basic linear / branching
//   - Conductor: collapse, parallel, extract
//   - Convergent mesh (cyclic graphs)
//   - Tesseract (4D hyper-dimensional)
//   - Self-healing & retry
//   - Orchestrator (boss/worker)
//   - Integration (HTTP, MCP, memory, squads)
//
// No DOM, no IPC — pure data.
// ─────────────────────────────────────────────────────────────────────────────

import { simNode, simEdge, simGraph, type SimScenario, type SimSuite } from './simulation-atoms';
import { allStressScenarios } from './simulation-stress-scenarios';

// ── 1. Basic Linear Flow ───────────────────────────────────────────────────

export const basicLinearScenario: SimScenario = {
  id: 'basic-linear',
  name: 'Basic Linear Pipeline',
  description: 'Simple trigger → agent → agent → output chain. Tests sequential execution.',
  category: 'basic',
  tier: 'smoke',
  tags: ['linear', 'sequential'],
  graph: (() => {
    const trigger = simNode('trigger', {
      id: 'trigger',
      label: 'Start',
      config: { prompt: 'Hello world' },
    });
    const agent1 = simNode('agent', {
      id: 'agent1',
      label: 'Analyze Data',
      config: { prompt: 'Analyze this input thoroughly.' },
    });
    const agent2 = simNode('agent', {
      id: 'agent2',
      label: 'Summarize Results',
      config: { prompt: 'Summarize the analysis above.' },
    });
    const output = simNode('output', { id: 'out', label: 'Final Output' });
    return simGraph(
      [trigger, agent1, agent2, output],
      [simEdge('trigger', 'agent1'), simEdge('agent1', 'agent2'), simEdge('agent2', 'out')],
      { name: 'Basic Linear Pipeline' },
    );
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'Flow should complete successfully',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'node-executed',
      description: 'All nodes should execute',
      check: { type: 'node-executed', nodeId: 'agent1', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Output node should execute',
      check: { type: 'node-executed', nodeId: 'out', executed: true },
    },
    {
      type: 'execution-order',
      description: 'Trigger before output',
      check: { type: 'execution-order', nodeIds: ['trigger', 'out'] },
    },
  ],
};

// ── 2. Condition Branching ─────────────────────────────────────────────────

export const conditionBranchingScenario: SimScenario = {
  id: 'condition-branching',
  name: 'Condition Branching (If/Else)',
  description:
    'Tests condition node routing: trigger → condition → true-path / false-path → output.',
  category: 'branching',
  tier: 'smoke',
  tags: ['condition', 'branching', 'routing'],
  graph: (() => {
    const trigger = simNode('trigger', {
      id: 't',
      label: 'User Request',
      config: { prompt: 'Process this data.' },
    });
    const condition = simNode('condition', {
      id: 'cond',
      label: 'Is Valid?',
      config: { conditionExpr: 'data.isValid' },
    });
    const truePath = simNode('agent', {
      id: 'ok',
      label: 'Process Valid Data',
      config: { prompt: 'Data is valid. Process it.' },
    });
    const falsePath = simNode('agent', {
      id: 'err',
      label: 'Handle Invalid Data',
      config: { prompt: 'Data is invalid. Report error.' },
    });
    const output = simNode('output', { id: 'out', label: 'Result' });
    return simGraph(
      [trigger, condition, truePath, falsePath, output],
      [
        simEdge('t', 'cond'),
        simEdge('cond', 'ok', { label: 'true' }),
        simEdge('cond', 'err', { label: 'false' }),
        simEdge('ok', 'out'),
        simEdge('err', 'out'),
      ],
      { name: 'Condition Branching' },
    );
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
    nodeOverrides: {
      cond: { strategy: 'static', response: 'true', forceConditionResult: true },
    },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'Flow completes',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'node-executed',
      description: 'True path executed',
      check: { type: 'node-executed', nodeId: 'ok', executed: true },
    },
    {
      type: 'node-executed',
      description: 'False path skipped',
      check: { type: 'node-executed', nodeId: 'err', executed: false },
    },
  ],
};

// ── 3. Conductor Collapse Chain ────────────────────────────────────────────

export const conductorCollapseScenario: SimScenario = {
  id: 'conductor-collapse',
  name: 'Conductor: Collapse Chain',
  description:
    'Three sequential agent nodes with compatible configs are collapsed into a single LLM call.',
  category: 'collapse',
  tier: 'standard',
  tags: ['conductor', 'collapse', 'optimization'],
  graph: (() => {
    const trigger = simNode('trigger', { id: 't', label: 'Start' });
    const a1 = simNode('agent', {
      id: 'a1',
      label: 'Research Topic',
      config: { prompt: 'Research the topic in depth.' },
    });
    const a2 = simNode('agent', {
      id: 'a2',
      label: 'Draft Content',
      config: { prompt: 'Draft content based on the research.' },
    });
    const a3 = simNode('agent', {
      id: 'a3',
      label: 'Review & Polish',
      config: { prompt: 'Review and polish the draft.' },
    });
    const code = simNode('code', {
      id: 'c',
      label: 'Format Output',
      config: { code: 'return input.toUpperCase()' },
    });
    const output = simNode('output', { id: 'out', label: 'Final' });
    return simGraph(
      [trigger, a1, a2, a3, code, output],
      [
        simEdge('t', 'a1'),
        simEdge('a1', 'a2'),
        simEdge('a2', 'a3'),
        simEdge('a3', 'c'),
        simEdge('c', 'out'),
      ],
      { name: 'Collapse Chain' },
    );
  })(),
  mocks: {
    agentDefault: {
      strategy: 'static',
      response:
        'Research findings.\n---STEP_BOUNDARY---\nDraft based on research.\n---STEP_BOUNDARY---\nPolished final version.',
    },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'Flow completes',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'conductor-used',
      description: 'Conductor activates',
      check: { type: 'conductor-used', expected: true },
    },
    {
      type: 'strategy-shape',
      description: 'Strategy has collapse groups',
      check: { type: 'strategy-shape', hasCollapse: true },
    },
  ],
};

// ── 4. Conductor Parallel Branches ─────────────────────────────────────────

export const conductorParallelScenario: SimScenario = {
  id: 'conductor-parallel',
  name: 'Conductor: Parallel Branches',
  description: 'Fan-out from trigger to 3 independent agent branches, then fan-in to output.',
  category: 'parallel',
  tier: 'standard',
  tags: ['conductor', 'parallel', 'fan-out', 'fan-in'],
  graph: (() => {
    const trigger = simNode('trigger', {
      id: 't',
      label: 'Start',
      config: { prompt: 'Analyze from three angles.' },
    });
    const a1 = simNode('agent', { id: 'a1', label: 'Technical Analysis' });
    const a2 = simNode('agent', { id: 'a2', label: 'Business Analysis' });
    const a3 = simNode('agent', { id: 'a3', label: 'User Analysis' });
    const merge = simNode('agent', {
      id: 'merge',
      label: 'Merge Perspectives',
      config: { prompt: 'Merge the three analysis perspectives into a unified report.' },
    });
    const output = simNode('output', { id: 'out', label: 'Report' });
    return simGraph(
      [trigger, a1, a2, a3, merge, output],
      [
        simEdge('t', 'a1'),
        simEdge('t', 'a2'),
        simEdge('t', 'a3'),
        simEdge('a1', 'merge'),
        simEdge('a2', 'merge'),
        simEdge('a3', 'merge'),
        simEdge('merge', 'out'),
      ],
      { name: 'Parallel Branches' },
    );
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
    nodeOverrides: {
      a1: {
        strategy: 'static',
        response: 'Technical: Architecture is sound, scalable to 10k concurrent users.',
      },
      a2: {
        strategy: 'static',
        response: 'Business: ROI projection shows 340% return within 18 months.',
      },
      a3: {
        strategy: 'static',
        response: 'User: UX research indicates 92% satisfaction rate with current design.',
      },
    },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'Flow completes',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'conductor-used',
      description: 'Conductor activates for fan-out',
      check: { type: 'conductor-used', expected: true },
    },
    {
      type: 'strategy-shape',
      description: 'Strategy has parallel phases',
      check: { type: 'strategy-shape', hasParallel: true },
    },
    {
      type: 'node-executed',
      description: 'All branches execute',
      check: { type: 'node-executed', nodeId: 'a1', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Merge node executes',
      check: { type: 'node-executed', nodeId: 'merge', executed: true },
    },
  ],
};

// ── 5. Convergent Mesh (Cyclic) ────────────────────────────────────────────

export const convergentMeshScenario: SimScenario = {
  id: 'convergent-mesh',
  name: 'Convergent Mesh (Iterative Consensus)',
  description:
    'Two agents debate back and forth until their outputs converge. Tests the Converge primitive.',
  category: 'convergent',
  tier: 'complex',
  tags: ['conductor', 'mesh', 'convergence', 'cycle', 'bidirectional'],
  graph: (() => {
    const trigger = simNode('trigger', {
      id: 't',
      label: 'Debate Topic',
      config: { prompt: 'Should AI be open-source? Discuss.' },
    });
    const pro = simNode('agent', {
      id: 'pro',
      label: 'Pro Advocate',
      config: { prompt: 'Argue in favor of open-source AI.' },
    });
    const con = simNode('agent', {
      id: 'con',
      label: 'Con Advocate',
      config: { prompt: 'Argue against open-source AI.' },
    });
    const synthesis = simNode('agent', {
      id: 'syn',
      label: 'Synthesize Consensus',
      config: { prompt: 'Synthesize the debate into a balanced conclusion.' },
    });
    const output = simNode('output', { id: 'out', label: 'Conclusion' });
    return simGraph(
      [trigger, pro, con, synthesis, output],
      [
        simEdge('t', 'pro'),
        simEdge('t', 'con'),
        simEdge('pro', 'con', { kind: 'bidirectional' }),
        simEdge('con', 'pro', { kind: 'bidirectional' }),
        simEdge('pro', 'syn'),
        simEdge('con', 'syn'),
        simEdge('syn', 'out'),
      ],
      { name: 'Convergent Debate' },
    );
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
    nodeOverrides: {
      // Use sequence to simulate convergence over rounds
      pro: {
        strategy: 'sequence',
        sequence: [
          'Open-source AI democratizes access and accelerates innovation through community collaboration.',
          'While risks exist, open governance and community oversight provide strong safeguards. The benefits of transparency outweigh concerns.',
          'The consensus is clear: open-source AI with responsible governance frameworks maximizes societal benefit while managing risks effectively.',
        ],
      },
      con: {
        strategy: 'sequence',
        sequence: [
          'Open-source AI poses serious risks: misuse, safety concerns, and lack of accountability when models are freely available.',
          'Acknowledging benefits of openness, the key issue is implementing proper safeguards. Perhaps a tiered release approach balances both.',
          'The consensus is clear: open-source AI with responsible governance frameworks maximizes societal benefit while managing risks effectively.',
        ],
      },
    },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'Flow completes',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'conductor-used',
      description: 'Conductor activates for cycles',
      check: { type: 'conductor-used', expected: true },
    },
    {
      type: 'strategy-shape',
      description: 'Strategy has mesh units',
      check: { type: 'strategy-shape', hasMesh: true },
    },
    {
      type: 'node-executed',
      description: 'Synthesis node executes',
      check: { type: 'node-executed', nodeId: 'syn', executed: true },
    },
  ],
};

// ── 6. Tesseract (4D Hyper-Dimensional) ────────────────────────────────────

export const tesseractScenario: SimScenario = {
  id: 'tesseract-4d',
  name: 'Tesseract: 4D Multi-Phase Pipeline',
  description:
    'Full tesseract flow with cells, event horizons, and phase transitions. The crown jewel.',
  category: 'tesseract',
  tier: 'extreme',
  tags: ['tesseract', '4d', 'event-horizon', 'phases', 'cells'],
  graph: (() => {
    // Phase 0 (W=0): Research cells
    const trigger = simNode('trigger', {
      id: 't',
      label: 'Research Brief',
      phase: 0,
      depth: 0,
      config: { prompt: 'Research the market landscape for AI developer tools.' },
    });
    const researchA = simNode('agent', {
      id: 'r_a',
      label: 'Market Research',
      phase: 0,
      depth: 1,
      cellId: 'cell_research_a',
      config: { prompt: 'Research market size, key players, and trends.' },
    });
    const researchB = simNode('agent', {
      id: 'r_b',
      label: 'Technical Research',
      phase: 0,
      depth: 1,
      cellId: 'cell_research_b',
      config: { prompt: 'Research technical approaches, architectures, and innovations.' },
    });
    const researchC = simNode('agent', {
      id: 'r_c',
      label: 'User Research',
      phase: 0,
      depth: 1,
      cellId: 'cell_research_c',
      config: { prompt: 'Research user needs, pain points, and desired features.' },
    });

    // Event Horizon 1: Research converges
    const horizon1 = simNode('event-horizon', {
      id: 'eh1',
      label: 'Research Convergence',
      phase: 0,
      depth: 2,
      config: { mergePolicy: 'synthesize' },
    });

    // Phase 1 (W=1): Analysis cells
    const analysis = simNode('agent', {
      id: 'ana',
      label: 'Gap Analysis',
      phase: 1,
      depth: 3,
      cellId: 'cell_analysis',
      config: { prompt: 'Identify gaps and opportunities from the research.' },
    });
    const strategy = simNode('agent', {
      id: 'str',
      label: 'Strategy Formation',
      phase: 1,
      depth: 3,
      cellId: 'cell_strategy',
      config: { prompt: 'Formulate competitive strategy based on gaps.' },
    });

    // Event Horizon 2: Strategy converges
    const horizon2 = simNode('event-horizon', {
      id: 'eh2',
      label: 'Strategy Lock',
      phase: 1,
      depth: 4,
      config: { mergePolicy: 'vote' },
    });

    // Phase 2 (W=2): Execution cells
    const roadmap = simNode('agent', {
      id: 'road',
      label: 'Build Roadmap',
      phase: 2,
      depth: 5,
      cellId: 'cell_execution',
      config: { prompt: 'Create detailed product roadmap with milestones.' },
    });
    const comm = simNode('agent', {
      id: 'comm',
      label: 'Draft Comms Plan',
      phase: 2,
      depth: 5,
      cellId: 'cell_comms',
      config: { prompt: 'Draft communications and launch plan.' },
    });

    // Final output
    const output = simNode('output', { id: 'out', label: 'Strategic Plan', phase: 2, depth: 6 });

    return simGraph(
      [
        trigger,
        researchA,
        researchB,
        researchC,
        horizon1,
        analysis,
        strategy,
        horizon2,
        roadmap,
        comm,
        output,
      ],
      [
        // Phase 0: Fan-out to research cells
        simEdge('t', 'r_a'),
        simEdge('t', 'r_b'),
        simEdge('t', 'r_c'),
        // Research → Event Horizon 1
        simEdge('r_a', 'eh1'),
        simEdge('r_b', 'eh1'),
        simEdge('r_c', 'eh1'),
        // EH1 → Phase 1 analysis
        simEdge('eh1', 'ana'),
        simEdge('eh1', 'str'),
        // Analysis → Event Horizon 2
        simEdge('ana', 'eh2'),
        simEdge('str', 'eh2'),
        // EH2 → Phase 2 execution
        simEdge('eh2', 'road'),
        simEdge('eh2', 'comm'),
        // Execution → output
        simEdge('road', 'out'),
        simEdge('comm', 'out'),
      ],
      { name: 'Tesseract Strategic Planning' },
    );
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
    nodeOverrides: {
      r_a: {
        strategy: 'static',
        response:
          'Market Analysis: TAM of $15B for AI developer tools. Key players: GitHub Copilot, Cursor, Cody. Growth rate: 45% YoY.',
      },
      r_b: {
        strategy: 'static',
        response:
          'Technical Analysis: RAG-based approaches dominate. MCP protocol emerging as standard. Local-first architecture gaining traction.',
      },
      r_c: {
        strategy: 'static',
        response:
          'User Research: Developers want context-aware AI that understands their full codebase. Privacy concerns drive preference for local models.',
      },
      ana: {
        strategy: 'static',
        response:
          'Gap Analysis: No tool combines local-first + multi-model + workflow automation. OpenPawz uniquely positioned to fill this.',
      },
      str: {
        strategy: 'static',
        response:
          'Strategy: Lead with Tesseract flows (unique differentiator), emphasize local-first privacy, target power users first then expand.',
      },
      road: {
        strategy: 'static',
        response:
          'Roadmap:\nQ1: Core flow engine + Conductor\nQ2: Tesseract + MCP bridge\nQ3: Community marketplace\nQ4: Enterprise features',
      },
      comm: {
        strategy: 'static',
        response:
          'Launch Plan:\nWeek 1: HackerNews + Reddit\nWeek 2: Product Hunt\nWeek 3: Dev conferences\nWeek 4: Blog series',
      },
    },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'Tesseract flow completes',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'conductor-used',
      description: 'Conductor activates',
      check: { type: 'conductor-used', expected: true },
    },
    {
      type: 'strategy-shape',
      description: 'Has parallel phases for cells',
      check: { type: 'strategy-shape', hasParallel: true },
    },
    {
      type: 'node-executed',
      description: 'All research cells execute',
      check: { type: 'node-executed', nodeId: 'r_a', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Event horizon reached',
      check: { type: 'node-executed', nodeId: 'eh1', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Analysis executes after horizon',
      check: { type: 'node-executed', nodeId: 'ana', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Roadmap executes',
      check: { type: 'node-executed', nodeId: 'road', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Final output produced',
      check: { type: 'node-executed', nodeId: 'out', executed: true },
    },
    {
      type: 'execution-order',
      description: 'Horizons enforce ordering',
      check: { type: 'execution-order', nodeIds: ['t', 'eh1', 'eh2', 'out'] },
    },
  ],
};

// ── 7. Self-Healing & Retry ────────────────────────────────────────────────

export const selfHealingScenario: SimScenario = {
  id: 'self-healing',
  name: 'Self-Healing: Retry & Error Recovery',
  description: 'Node fails once then succeeds on retry. Tests retry logic and error edge routing.',
  category: 'self-healing',
  tier: 'standard',
  tags: ['retry', 'error-handling', 'self-healing', 'resilience'],
  graph: (() => {
    const trigger = simNode('trigger', { id: 't', label: 'Start' });
    const flaky = simNode('agent', {
      id: 'flaky',
      label: 'Flaky API Call',
      config: { prompt: 'Call the API.', maxRetries: 2, retryDelayMs: 100 },
    });
    const fallback = simNode('error', {
      id: 'fallback',
      label: 'Error Handler',
      config: { errorTargets: ['log', 'chat'] },
    });
    const output = simNode('output', { id: 'out', label: 'Result' });
    return simGraph(
      [trigger, flaky, fallback, output],
      [
        simEdge('t', 'flaky'),
        simEdge('flaky', 'out'),
        simEdge('flaky', 'fallback', { kind: 'error', fromPort: 'err' }),
      ],
      { name: 'Self-Healing Flow' },
    );
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
    nodeOverrides: {
      // Fail on first call, succeed on second (retry)
      flaky: {
        strategy: 'sequence',
        sequence: ['API call succeeded on retry.'],
        failOnCall: 0,
        errorMessage: 'HTTP 503: Service temporarily unavailable',
      },
    },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'Flow completes after retry',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'node-status',
      description: 'Flaky node succeeds (after retry)',
      check: { type: 'node-status', nodeId: 'flaky', expectedStatus: 'success' },
    },
    {
      type: 'event-emitted',
      description: 'Step-start emitted for flaky node',
      check: { type: 'event-emitted', eventType: 'step-start' },
    },
  ],
};

// ── 8. Integration: HTTP + MCP + Memory ────────────────────────────────────

export const integrationScenario: SimScenario = {
  id: 'integration-pipeline',
  name: 'Integration Pipeline (HTTP + MCP + Memory)',
  description:
    'Full integration scenario: fetch data via HTTP, process with MCP tool, store in memory, recall later.',
  category: 'integration',
  tier: 'complex',
  tags: ['http', 'mcp', 'memory', 'integration', 'pipeline'],
  graph: (() => {
    const trigger = simNode('trigger', { id: 't', label: 'Start Pipeline' });
    const fetch = simNode('http', {
      id: 'fetch',
      label: 'Fetch Data',
      config: { httpMethod: 'GET', httpUrl: 'https://api.example.com/data' },
    });
    const transform = simNode('mcp-tool', {
      id: 'transform',
      label: 'Transform Data',
      config: { mcpToolName: 'mcp_n8n_transform', mcpToolArgs: '{"format":"json"}' },
    });
    const analyze = simNode('agent', {
      id: 'analyze',
      label: 'Analyze Results',
      config: { prompt: 'Analyze the transformed data.' },
    });
    const store = simNode('memory', {
      id: 'store',
      label: 'Store Insights',
      config: { memoryCategory: 'analysis', memoryImportance: 0.8 },
    });
    const recall = simNode('memory-recall', {
      id: 'recall',
      label: 'Recall Previous',
      config: { memoryQuery: 'analysis insights', memoryLimit: 3 },
    });
    const report = simNode('agent', {
      id: 'report',
      label: 'Generate Report',
      config: { prompt: 'Generate a comprehensive report combining new and historical analysis.' },
    });
    const output = simNode('output', { id: 'out', label: 'Final Report' });
    return simGraph(
      [trigger, fetch, transform, analyze, store, recall, report, output],
      [
        simEdge('t', 'fetch'),
        simEdge('fetch', 'transform'),
        simEdge('transform', 'analyze'),
        simEdge('analyze', 'store'),
        simEdge('analyze', 'recall'),
        simEdge('store', 'report'),
        simEdge('recall', 'report'),
        simEdge('report', 'out'),
      ],
      { name: 'Integration Pipeline' },
    );
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
    httpMocks: [
      {
        urlPattern: 'api.example.com/data',
        status: 200,
        body: JSON.stringify({ users: 1500, revenue: 45000, growth: 0.23 }),
      },
    ],
    mcpMocks: {
      mcp_n8n_transform: {
        success: true,
        content: JSON.stringify({
          formatted: true,
          users: '1,500',
          revenue: '$45,000',
          growth: '23%',
        }),
      },
    },
    memoryMocks: {
      memories: [
        {
          keywords: ['analysis', 'insights', 'previous'],
          content:
            'Previous analysis (Q3): Users were at 1,200 with 18% growth. Revenue was $38,000.',
          relevance: 0.92,
          category: 'analysis',
        },
      ],
      defaultRecallResponse: 'No previous analysis data found.',
    },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'Pipeline completes',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'node-output',
      description: 'HTTP returns data',
      check: { type: 'node-output', nodeId: 'fetch', contains: 'users' },
    },
    {
      type: 'node-output',
      description: 'MCP transforms data',
      check: { type: 'node-output', nodeId: 'transform', contains: 'formatted' },
    },
    {
      type: 'node-executed',
      description: 'Memory store executes',
      check: { type: 'node-executed', nodeId: 'store', executed: true },
    },
    {
      type: 'node-output',
      description: 'Memory recall finds history',
      check: { type: 'node-output', nodeId: 'recall', contains: 'Previous analysis' },
    },
    {
      type: 'node-executed',
      description: 'Report generated',
      check: { type: 'node-executed', nodeId: 'report', executed: true },
    },
  ],
};

// ── 9. Code Sandbox with Variables ─────────────────────────────────────────

export const codeSandboxScenario: SimScenario = {
  id: 'code-sandbox',
  name: 'Code Sandbox: Transform & Variables',
  description: 'Tests code nodes with sandboxed JS execution and flow variable propagation.',
  category: 'basic',
  tier: 'smoke',
  tags: ['code', 'sandbox', 'variables'],
  graph: (() => {
    const trigger = simNode('trigger', { id: 't', label: 'Input', config: { prompt: '42' } });
    const code1 = simNode('code', {
      id: 'c1',
      label: 'Double It',
      config: { code: 'return String(Number(input) * 2)', setVariableKey: 'doubled' },
    });
    const code2 = simNode('code', {
      id: 'c2',
      label: 'Add 10',
      config: { code: 'return String(Number(input) + 10)' },
    });
    const output = simNode('output', { id: 'out', label: 'Result' });
    return simGraph(
      [trigger, code1, code2, output],
      [simEdge('t', 'c1'), simEdge('c1', 'c2'), simEdge('c2', 'out')],
      { name: 'Code Pipeline' },
    );
  })(),
  mocks: {},
  expectations: [
    {
      type: 'flow-status',
      description: 'Flow completes',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'node-output',
      description: 'Code1 doubles: 84',
      check: { type: 'node-output', nodeId: 'c1', contains: '84' },
    },
    {
      type: 'node-output',
      description: 'Code2 adds 10: 94',
      check: { type: 'node-output', nodeId: 'c2', contains: '94' },
    },
    {
      type: 'variable-set',
      description: 'Variable "doubled" was set',
      check: { type: 'variable-set', key: 'doubled', exists: true },
    },
  ],
};

// ── 10. Chaos Testing ──────────────────────────────────────────────────────

export const chaosTestingScenario: SimScenario = {
  id: 'chaos-testing',
  name: 'Chaos: Random Failures',
  description: 'Tests flow resilience under random failure conditions. Some nodes may fail.',
  category: 'self-healing',
  tier: 'standard',
  tags: ['chaos', 'resilience', 'random-failure'],
  graph: (() => {
    const trigger = simNode('trigger', { id: 't', label: 'Start' });
    const a1 = simNode('agent', { id: 'a1', label: 'Step 1', config: { maxRetries: 1 } });
    const a2 = simNode('agent', { id: 'a2', label: 'Step 2', config: { maxRetries: 1 } });
    const a3 = simNode('agent', { id: 'a3', label: 'Step 3', config: { maxRetries: 1 } });
    const errorHandler = simNode('error', { id: 'err', label: 'Error Log' });
    const output = simNode('output', { id: 'out', label: 'Result' });
    return simGraph(
      [trigger, a1, a2, a3, errorHandler, output],
      [
        simEdge('t', 'a1'),
        simEdge('a1', 'a2'),
        simEdge('a2', 'a3'),
        simEdge('a3', 'out'),
        simEdge('a1', 'err', { kind: 'error', fromPort: 'err' }),
        simEdge('a2', 'err', { kind: 'error', fromPort: 'err' }),
        simEdge('a3', 'err', { kind: 'error', fromPort: 'err' }),
      ],
      { name: 'Chaos Test' },
    );
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
    failureRate: 0.3,
    failureMessage: 'Random chaos failure!',
  },
  expectations: [
    // We don't assert success — chaos mode may or may not fail
    {
      type: 'event-emitted',
      description: 'Run started',
      check: { type: 'event-emitted', eventType: 'run-start' },
    },
    {
      type: 'event-emitted',
      description: 'Run completed (success or error)',
      check: { type: 'event-emitted', eventType: 'run-complete' },
    },
  ],
};

// ── 11. Deep Orchestrator (Boss/Worker) ────────────────────────────────────

export const orchestratorScenario: SimScenario = {
  id: 'orchestrator-deep',
  name: 'Orchestrator: Boss/Worker Multi-Agent',
  description:
    'Boss agent delegates tasks to worker agents, collects results, and produces final output.',
  category: 'orchestrator',
  tier: 'complex',
  tags: ['orchestrator', 'boss-worker', 'delegation', 'multi-agent'],
  graph: (() => {
    const trigger = simNode('trigger', {
      id: 't',
      label: 'Project Brief',
      config: { prompt: 'Build a landing page for our new AI product.' },
    });
    const boss = simNode('agent', {
      id: 'boss',
      label: 'Project Manager (Boss)',
      config: {
        prompt: 'You are the project manager. Break this into tasks and delegate to your team.',
        model: 'claude-sonnet',
      },
    });
    const worker1 = simNode('agent', {
      id: 'w1',
      label: 'Designer (Worker)',
      config: {
        prompt: 'You are a UI designer. Create the visual design spec.',
        model: 'cheap-model',
      },
    });
    const worker2 = simNode('agent', {
      id: 'w2',
      label: 'Copywriter (Worker)',
      config: { prompt: 'You are a copywriter. Write compelling copy.', model: 'cheap-model' },
    });
    const worker3 = simNode('agent', {
      id: 'w3',
      label: 'Developer (Worker)',
      config: {
        prompt: 'You are a frontend developer. Plan the technical implementation.',
        model: 'cheap-model',
      },
    });
    const review = simNode('agent', {
      id: 'review',
      label: 'Boss Review',
      config: {
        prompt: 'Review all deliverables and compile the final plan.',
        model: 'claude-sonnet',
      },
    });
    const output = simNode('output', { id: 'out', label: 'Project Plan' });
    return simGraph(
      [trigger, boss, worker1, worker2, worker3, review, output],
      [
        simEdge('t', 'boss'),
        simEdge('boss', 'w1'),
        simEdge('boss', 'w2'),
        simEdge('boss', 'w3'),
        simEdge('w1', 'review'),
        simEdge('w2', 'review'),
        simEdge('w3', 'review'),
        simEdge('review', 'out'),
      ],
      { name: 'Orchestrator: Landing Page Project' },
    );
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
    nodeOverrides: {
      boss: {
        strategy: 'static',
        response:
          'Task delegation:\n1. Designer: Create visual mockup\n2. Copywriter: Draft headline + CTA copy\n3. Developer: Plan React component structure\n\nDeadline: 3 days',
      },
      w1: {
        strategy: 'static',
        response:
          'Design Spec: Hero section (gradient bg), feature grid (3 cols), testimonial carousel, CTA button (primary blue).',
      },
      w2: {
        strategy: 'static',
        response:
          'Copy:\nHeadline: "AI That Works With You, Not Against You"\nSubheading: "The open-source AI assistant that respects your privacy"\nCTA: "Get Started Free"',
      },
      w3: {
        strategy: 'static',
        response:
          'Tech Plan: Next.js + Tailwind. Components: HeroSection, FeatureGrid, TestimonialCarousel, CTABlock. Estimated: 16 dev hours.',
      },
      review: {
        strategy: 'static',
        response:
          '## Final Project Plan\n\nAll deliverables reviewed and approved.\n\n- Design: Approved ✓\n- Copy: Approved ✓\n- Tech: Approved ✓\n\nReady for implementation sprint.',
      },
    },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'Project completes',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'conductor-used',
      description: 'Conductor activates for fan-out',
      check: { type: 'conductor-used', expected: true },
    },
    {
      type: 'node-executed',
      description: 'Boss delegates',
      check: { type: 'node-executed', nodeId: 'boss', executed: true },
    },
    {
      type: 'node-executed',
      description: 'All workers execute',
      check: { type: 'node-executed', nodeId: 'w1', executed: true },
    },
    {
      type: 'node-output',
      description: 'Review approves deliverables',
      check: { type: 'node-output', nodeId: 'review', contains: 'Approved' },
    },
    {
      type: 'execution-order',
      description: 'Boss before workers, review after',
      check: { type: 'execution-order', nodeIds: ['boss', 'review', 'out'] },
    },
  ],
};

// ── 12. Mixed Extract + Collapse + Parallel ────────────────────────────────

export const mixedConductorScenario: SimScenario = {
  id: 'mixed-conductor',
  name: 'Mixed: Extract + Collapse + Parallel',
  description: 'Complex graph that triggers all three Conductor primitives simultaneously.',
  category: 'parallel',
  tier: 'complex',
  tags: ['conductor', 'collapse', 'extract', 'parallel', 'mixed'],
  graph: (() => {
    const trigger = simNode('trigger', { id: 't', label: 'Start' });
    // Branch A: Collapse chain (3 sequential agents)
    const a1 = simNode('agent', { id: 'a1', label: 'Research' });
    const a2 = simNode('agent', { id: 'a2', label: 'Draft' });
    const a3 = simNode('agent', { id: 'a3', label: 'Polish' });
    // Branch B: Extract chain (HTTP → code)
    const http = simNode('http', {
      id: 'h',
      label: 'Fetch API',
      config: { httpUrl: 'https://api.example.com/stats', httpMethod: 'GET' },
    });
    const code = simNode('code', {
      id: 'c',
      label: 'Parse JSON',
      config: {
        code: 'try { return JSON.stringify(JSON.parse(input), null, 2) } catch { return input }',
      },
    });
    // Merge point
    const merge = simNode('agent', {
      id: 'merge',
      label: 'Combine Results',
      config: { prompt: 'Combine the drafted content with the API data.' },
    });
    const output = simNode('output', { id: 'out', label: 'Final' });
    return simGraph(
      [trigger, a1, a2, a3, http, code, merge, output],
      [
        simEdge('t', 'a1'),
        simEdge('a1', 'a2'),
        simEdge('a2', 'a3'),
        simEdge('t', 'h'),
        simEdge('h', 'c'),
        simEdge('a3', 'merge'),
        simEdge('c', 'merge'),
        simEdge('merge', 'out'),
      ],
      { name: 'Mixed Conductor' },
    );
  })(),
  mocks: {
    agentDefault: {
      strategy: 'static',
      response:
        'Research complete.\n---STEP_BOUNDARY---\nDraft written.\n---STEP_BOUNDARY---\nContent polished.',
    },
    httpMocks: [
      {
        urlPattern: 'api.example.com',
        status: 200,
        body: '{"active_users": 5000, "uptime": "99.9%"}',
      },
    ],
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'Flow completes',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'conductor-used',
      description: 'Conductor activates',
      check: { type: 'conductor-used', expected: true },
    },
    {
      type: 'strategy-shape',
      description: 'Has collapse + parallel',
      check: { type: 'strategy-shape', hasCollapse: true, hasParallel: true },
    },
    {
      type: 'node-executed',
      description: 'HTTP node extracted (no LLM)',
      check: { type: 'node-executed', nodeId: 'h', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Code node extracted (no LLM)',
      check: { type: 'node-executed', nodeId: 'c', executed: true },
    },
  ],
};

// ── Master Suite ───────────────────────────────────────────────────────────

/** All built-in scenarios as a suite. */
export const masterSimSuite: SimSuite = {
  id: 'master',
  name: 'Master Simulation Suite',
  description: 'All built-in simulation scenarios covering every flow capability.',
  scenarios: [
    basicLinearScenario,
    conditionBranchingScenario,
    conductorCollapseScenario,
    conductorParallelScenario,
    convergentMeshScenario,
    tesseractScenario,
    selfHealingScenario,
    integrationScenario,
    codeSandboxScenario,
    chaosTestingScenario,
    orchestratorScenario,
    mixedConductorScenario,
    ...allStressScenarios,
  ],
  globalMocks: {
    agentDefault: { strategy: 'realistic', modelName: 'sim-mock-7b' },
    simulateStreaming: false,
    latencyMs: 0,
  },
};

/** Get scenario by ID. */
export function getScenarioById(id: string): SimScenario | undefined {
  return masterSimSuite.scenarios.find((s) => s.id === id);
}

/** Get scenarios by category. */
export function getScenariosByCategory(category: SimScenario['category']): SimScenario[] {
  return masterSimSuite.scenarios.filter((s) => s.category === category);
}

/** Get scenarios by tier. */
export function getScenariosByTier(tier: SimScenario['tier']): SimScenario[] {
  return masterSimSuite.scenarios.filter((s) => s.tier === tier);
}

/** Get scenarios matching any of the given tags. */
export function getScenariosByTags(tags: string[]): SimScenario[] {
  return masterSimSuite.scenarios.filter((s) => s.tags?.some((t) => tags.includes(t)));
}
