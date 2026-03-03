// ─────────────────────────────────────────────────────────────────────────────
// Flow Simulation Engine — Stress-Test Scenario Library
// Massive, complex, adversarial flows designed to break every assumption:
//   - Zapier-scale linear chains (30+ nodes)
//   - Deep tesseract with nested event horizons
//   - Reverse & bidirectional edge topologies
//   - Diamond fan-out → fan-in at multiple levels
//   - Every node kind in a single flow
//   - Cascading condition trees (deep nesting)
//   - Multi-error cascading fallback paths
//   - Loop nodes with array iteration
//   - Squad + memory + MCP combined flows
//   - Massive parallel fan-out (10+ branches)
//   - Deep collapse chains (6+ sequential agents)
//   - Graph with every EdgeKind simultaneously
//   - Triple-mesh convergent debate
//   - Adversarial mock failures
//
// No DOM, no IPC — pure data.
// ─────────────────────────────────────────────────────────────────────────────

import { simNode, simEdge, simGraph, type SimScenario, type SimSuite } from './simulation-atoms';

// ── Helper: Generate N agents in a chain ───────────────────────────────────

function agentChain(n: number, prefix = 'a') {
  const nodes = Array.from({ length: n }, (_, i) =>
    simNode('agent', {
      id: `${prefix}${i}`,
      label: `Step ${i + 1}`,
      config: { prompt: `Execute step ${i + 1} of ${n}.` },
    }),
  );
  const edges = nodes.slice(1).map((_, i) => simEdge(`${prefix}${i}`, `${prefix}${i + 1}`));
  return { nodes, edges };
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. ZAPIER-SCALE LINEAR MEGA-CHAIN (30 nodes)
// ═══════════════════════════════════════════════════════════════════════════

export const zapierMegaChainScenario: SimScenario = {
  id: 'zapier-mega-chain',
  name: 'Zapier Mega-Chain (30 Nodes)',
  description:
    'Simulates a massive Zapier-style automation with 30 sequential steps — trigger, 28 processing nodes, output.',
  category: 'basic',
  tier: 'extreme',
  tags: ['zapier', 'mega-chain', 'linear', 'scale', 'stress'],
  graph: (() => {
    const trigger = simNode('trigger', {
      id: 't',
      label: 'Webhook Trigger',
      config: { prompt: 'Incoming form submission' },
    });
    const { nodes: agents, edges: agentEdges } = agentChain(26, 'z');
    // Mix in some non-agent nodes at intervals
    const code1 = simNode('code', {
      id: 'code1',
      label: 'Validate Email',
      config: { code: 'return input.includes("@") ? input : "invalid"' },
    });
    const http1 = simNode('http', {
      id: 'http1',
      label: 'Enrichment API',
      config: { httpUrl: 'https://api.clearbit.com/enrich', httpMethod: 'POST' },
    });
    const output = simNode('output', { id: 'out', label: 'Final CRM Update' });

    const allNodes = [
      trigger,
      ...agents.slice(0, 10),
      code1,
      ...agents.slice(10, 20),
      http1,
      ...agents.slice(20),
      output,
    ];
    const allEdges = [
      simEdge('t', 'z0'),
      ...agentEdges.slice(0, 10),
      simEdge('z9', 'code1'),
      simEdge('code1', 'z10'),
      ...agentEdges.slice(10, 20),
      simEdge('z19', 'http1'),
      simEdge('http1', 'z20'),
      ...agentEdges.slice(20),
      simEdge('z25', 'out'),
    ];

    return simGraph(allNodes, allEdges, { name: 'Zapier Mega-Chain' });
  })(),
  mocks: {
    agentDefault: {
      strategy: 'static',
      response: 'Step processed.\n---STEP_BOUNDARY---\nData forwarded.',
    },
    httpMocks: [
      { urlPattern: 'clearbit', status: 200, body: '{"company":"Acme","employees":500}' },
    ],
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'Mega-chain completes',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'conductor-used',
      description: 'Conductor activates for 30 nodes',
      check: { type: 'conductor-used', expected: true },
    },
    {
      type: 'strategy-shape',
      description: 'Strategy has collapse (sequential agents)',
      check: { type: 'strategy-shape', hasCollapse: true },
    },
    {
      type: 'node-executed',
      description: 'First agent ran',
      check: { type: 'node-executed', nodeId: 'z0', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Code node ran',
      check: { type: 'node-executed', nodeId: 'code1', executed: true },
    },
    {
      type: 'node-executed',
      description: 'HTTP node ran',
      check: { type: 'node-executed', nodeId: 'http1', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Last agent ran',
      check: { type: 'node-executed', nodeId: 'z25', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Output reached',
      check: { type: 'node-executed', nodeId: 'out', executed: true },
    },
    {
      type: 'execution-order',
      description: 'Trigger → Output ordering',
      check: { type: 'execution-order', nodeIds: ['t', 'out'] },
    },
  ],
};

// ═══════════════════════════════════════════════════════════════════════════
// 2. DEEP TESSERACT (5 phases, 4 cells each, nested horizons)
// ═══════════════════════════════════════════════════════════════════════════

export const deepTesseractScenario: SimScenario = {
  id: 'deep-tesseract-5phase',
  name: 'Deep Tesseract (5 Phases × 4 Cells)',
  description:
    '5 phases, each with 4 parallel cells, separated by event horizons. 25+ nodes, deeply structured.',
  category: 'tesseract',
  tier: 'extreme',
  tags: ['tesseract', 'deep', 'event-horizon', 'phases', 'stress'],
  graph: (() => {
    const trigger = simNode('trigger', {
      id: 't',
      label: 'Initiative Kickoff',
      phase: 0,
      depth: 0,
      config: { prompt: 'Launch multi-phase initiative' },
    });

    const phaseNames = ['Research', 'Design', 'Build', 'Test', 'Deploy'];
    const cellsPerPhase = 4;
    const allNodes: ReturnType<typeof simNode>[] = [trigger];
    const allEdges: ReturnType<typeof simEdge>[] = [];
    const horizons: string[] = [];

    for (let p = 0; p < 5; p++) {
      const cellIds: string[] = [];
      for (let c = 0; c < cellsPerPhase; c++) {
        const cellId = `p${p}_c${c}`;
        cellIds.push(cellId);
        allNodes.push(
          simNode('agent', {
            id: cellId,
            label: `${phaseNames[p]} Cell ${c + 1}`,
            phase: p,
            depth: p * 2 + 1,
            cellId: `cell_${cellId}`,
            config: { prompt: `Execute ${phaseNames[p]} task ${c + 1}` },
          }),
        );
      }

      // Connect: previous horizon (or trigger) → cells
      const source = p === 0 ? 't' : horizons[p - 1];
      for (const cid of cellIds) {
        allEdges.push(simEdge(source, cid));
      }

      // Event horizon at the end of each phase (except last)
      if (p < 4) {
        const ehId = `eh${p}`;
        horizons.push(ehId);
        allNodes.push(
          simNode('event-horizon', {
            id: ehId,
            label: `${phaseNames[p]} → ${phaseNames[p + 1]}`,
            phase: p,
            depth: p * 2 + 2,
            config: { mergePolicy: 'synthesize' },
          }),
        );
        for (const cid of cellIds) {
          allEdges.push(simEdge(cid, ehId));
        }
      }

      // Last phase cells → output
      if (p === 4) {
        for (const cid of cellIds) {
          allEdges.push(simEdge(cid, 'out'));
        }
      }
    }

    const output = simNode('output', {
      id: 'out',
      label: 'Initiative Report',
      phase: 4,
      depth: 10,
    });
    allNodes.push(output);

    return simGraph(allNodes, allEdges, { name: 'Deep Tesseract 5-Phase' });
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'Deep tesseract completes',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'conductor-used',
      description: 'Conductor activates',
      check: { type: 'conductor-used', expected: true },
    },
    {
      type: 'strategy-shape',
      description: 'Has parallel phases',
      check: { type: 'strategy-shape', hasParallel: true },
    },
    {
      type: 'node-executed',
      description: 'First cell executes',
      check: { type: 'node-executed', nodeId: 'p0_c0', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Phase 0 horizon',
      check: { type: 'node-executed', nodeId: 'eh0', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Phase 2 cell',
      check: { type: 'node-executed', nodeId: 'p2_c0', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Phase 4 cell',
      check: { type: 'node-executed', nodeId: 'p4_c0', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Output reached',
      check: { type: 'node-executed', nodeId: 'out', executed: true },
    },
    {
      type: 'execution-order',
      description: 'Horizons enforce phase order',
      check: { type: 'execution-order', nodeIds: ['t', 'eh0', 'eh1', 'eh2', 'eh3', 'out'] },
    },
  ],
};

// ═══════════════════════════════════════════════════════════════════════════
// 3. REVERSE EDGE DATA-PULL TOPOLOGY
// ═══════════════════════════════════════════════════════════════════════════

export const reverseEdgeScenario: SimScenario = {
  id: 'reverse-edge-pull',
  name: 'Reverse Edge: Data Pull Pattern',
  description:
    'Tests reverse edges where downstream nodes pull data from upstream. B requests data from A.',
  category: 'branching',
  tier: 'complex',
  tags: ['reverse', 'edge', 'pull', 'data-request'],
  graph: (() => {
    const trigger = simNode('trigger', {
      id: 't',
      label: 'Start',
      config: { prompt: 'Begin data processing' },
    });
    const dataSource = simNode('agent', {
      id: 'src',
      label: 'Data Source',
      config: { prompt: 'Provide raw data.' },
    });
    const processor = simNode('agent', {
      id: 'proc',
      label: 'Processor',
      config: { prompt: 'Process the pulled data.' },
    });
    const validator = simNode('agent', {
      id: 'val',
      label: 'Validator',
      config: { prompt: 'Validate processed results.' },
    });
    const enricher = simNode('agent', {
      id: 'enrich',
      label: 'Enricher',
      config: { prompt: 'Enrich with metadata.' },
    });
    const output = simNode('output', { id: 'out', label: 'Result' });
    return simGraph(
      [trigger, dataSource, processor, validator, enricher, output],
      [
        simEdge('t', 'src'),
        simEdge('src', 'proc'),
        simEdge('proc', 'val'),
        // Reverse: validator can also pull from data source for cross-check
        simEdge('src', 'val', { kind: 'reverse' }),
        simEdge('val', 'enrich'),
        simEdge('enrich', 'out'),
      ],
      { name: 'Reverse Edge Pull' },
    );
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
    nodeOverrides: {
      src: { strategy: 'static', response: 'Raw dataset: [1, 2, 3, 4, 5]' },
      proc: { strategy: 'static', response: 'Processed: [2, 4, 6, 8, 10]' },
      val: { strategy: 'static', response: 'Validation passed: sums match (15 vs 30, ratio 2x).' },
      enrich: { strategy: 'static', response: 'Enriched output with timestamps and metadata.' },
    },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'Flow completes with reverse edges',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'node-executed',
      description: 'Source provides data',
      check: { type: 'node-executed', nodeId: 'src', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Output reached',
      check: { type: 'node-executed', nodeId: 'out', executed: true },
    },
  ],
};

// ═══════════════════════════════════════════════════════════════════════════
// 4. BIDIRECTIONAL MULTI-AGENT DEBATE (3-way)
// ═══════════════════════════════════════════════════════════════════════════

export const tripleMeshDebateScenario: SimScenario = {
  id: 'triple-mesh-debate',
  name: 'Triple Mesh: 3-Agent Convergent Debate',
  description:
    '3 agents debate with bidirectional edges forming a triangle. Tests mesh convergence with 3 participants.',
  category: 'convergent',
  tier: 'extreme',
  tags: ['mesh', 'bidirectional', 'convergence', '3-way', 'debate', 'stress'],
  graph: (() => {
    const trigger = simNode('trigger', {
      id: 't',
      label: 'Debate Topic',
      config: { prompt: 'Best programming paradigm for AI systems?' },
    });
    const funcAgent = simNode('agent', {
      id: 'func',
      label: 'Functional Advocate',
      config: { prompt: 'Argue for functional programming in AI.' },
    });
    const ooAgent = simNode('agent', {
      id: 'oo',
      label: 'OOP Advocate',
      config: { prompt: 'Argue for object-oriented programming in AI.' },
    });
    const procAgent = simNode('agent', {
      id: 'proc',
      label: 'Procedural Advocate',
      config: { prompt: 'Argue for procedural programming in AI.' },
    });
    const synthesis = simNode('agent', {
      id: 'syn',
      label: 'Synthesize',
      config: { prompt: 'Find common ground between all three paradigms.' },
    });
    const output = simNode('output', { id: 'out', label: 'Conclusion' });
    return simGraph(
      [trigger, funcAgent, ooAgent, procAgent, synthesis, output],
      [
        simEdge('t', 'func'),
        simEdge('t', 'oo'),
        simEdge('t', 'proc'),
        // Full triangle of bidirectional edges
        simEdge('func', 'oo', { kind: 'bidirectional' }),
        simEdge('oo', 'func', { kind: 'bidirectional' }),
        simEdge('oo', 'proc', { kind: 'bidirectional' }),
        simEdge('proc', 'oo', { kind: 'bidirectional' }),
        simEdge('func', 'proc', { kind: 'bidirectional' }),
        simEdge('proc', 'func', { kind: 'bidirectional' }),
        // All → synthesis
        simEdge('func', 'syn'),
        simEdge('oo', 'syn'),
        simEdge('proc', 'syn'),
        simEdge('syn', 'out'),
      ],
      { name: 'Triple Mesh Debate' },
    );
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
    nodeOverrides: {
      func: {
        strategy: 'sequence',
        sequence: [
          'Functional: Immutability and pure functions make AI pipelines predictable and testable.',
          'Conceding OOP has good encapsulation. But functional composition is more powerful for data transformations.',
          'All three have merit. Multi-paradigm approach with functional core, OOP boundaries, procedural scripts.',
        ],
      },
      oo: {
        strategy: 'sequence',
        sequence: [
          'OOP: Encapsulation and polymorphism make AI agents modular and extensible.',
          'Acknowledging functional purity benefits. OOP still better for modeling real-world agent interactions.',
          'All three have merit. Multi-paradigm approach with functional core, OOP boundaries, procedural scripts.',
        ],
      },
      proc: {
        strategy: 'sequence',
        sequence: [
          'Procedural: Simple, direct, efficient. ML training loops are inherently procedural.',
          'Admit both raise good points. Procedural remains essential for performance-critical inner loops.',
          'All three have merit. Multi-paradigm approach with functional core, OOP boundaries, procedural scripts.',
        ],
      },
    },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'Triple debate completes',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'conductor-used',
      description: 'Conductor for bidirectional mesh',
      check: { type: 'conductor-used', expected: true },
    },
    {
      type: 'strategy-shape',
      description: 'Has mesh units',
      check: { type: 'strategy-shape', hasMesh: true },
    },
    {
      type: 'node-executed',
      description: 'All debaters execute',
      check: { type: 'node-executed', nodeId: 'func', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Synthesis executes',
      check: { type: 'node-executed', nodeId: 'syn', executed: true },
    },
  ],
};

// ═══════════════════════════════════════════════════════════════════════════
// 5. DIAMOND FAN-OUT / FAN-IN (Multi-Level)
// ═══════════════════════════════════════════════════════════════════════════

export const multiDiamondScenario: SimScenario = {
  id: 'multi-diamond',
  name: 'Multi-Level Diamond (Fan-Out → Fan-In × 3)',
  description: 'Three stacked diamonds: fan-out to 4 branches, fan-in, repeat 3 times. 16+ nodes.',
  category: 'parallel',
  tier: 'extreme',
  tags: ['diamond', 'fan-out', 'fan-in', 'multi-level', 'stress'],
  graph: (() => {
    const trigger = simNode('trigger', { id: 't', label: 'Start' });
    const allNodes: ReturnType<typeof simNode>[] = [trigger];
    const allEdges: ReturnType<typeof simEdge>[] = [];
    let lastMerge = 't';

    for (let d = 0; d < 3; d++) {
      const branchIds: string[] = [];
      for (let b = 0; b < 4; b++) {
        const bid = `d${d}_b${b}`;
        branchIds.push(bid);
        allNodes.push(
          simNode('agent', {
            id: bid,
            label: `Diamond ${d + 1} Branch ${b + 1}`,
            config: { prompt: `Process branch ${b + 1} of diamond ${d + 1}` },
          }),
        );
        allEdges.push(simEdge(lastMerge, bid));
      }
      const mergeId = `merge${d}`;
      allNodes.push(
        simNode('agent', {
          id: mergeId,
          label: `Diamond ${d + 1} Merge`,
          config: { prompt: `Merge all 4 branches of diamond ${d + 1}` },
        }),
      );
      for (const bid of branchIds) {
        allEdges.push(simEdge(bid, mergeId));
      }
      lastMerge = mergeId;
    }

    const output = simNode('output', { id: 'out', label: 'Final Merge' });
    allNodes.push(output);
    allEdges.push(simEdge(lastMerge, 'out'));

    return simGraph(allNodes, allEdges, { name: 'Multi-Level Diamond' });
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'All diamonds complete',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'conductor-used',
      description: 'Conductor for fan-out',
      check: { type: 'conductor-used', expected: true },
    },
    {
      type: 'strategy-shape',
      description: 'Has parallel phases',
      check: { type: 'strategy-shape', hasParallel: true },
    },
    {
      type: 'node-executed',
      description: 'First diamond branch',
      check: { type: 'node-executed', nodeId: 'd0_b0', executed: true },
    },
    {
      type: 'node-executed',
      description: 'First merge',
      check: { type: 'node-executed', nodeId: 'merge0', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Second diamond branch',
      check: { type: 'node-executed', nodeId: 'd1_b0', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Third merge',
      check: { type: 'node-executed', nodeId: 'merge2', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Output',
      check: { type: 'node-executed', nodeId: 'out', executed: true },
    },
    {
      type: 'execution-order',
      description: 'Merges in order',
      check: { type: 'execution-order', nodeIds: ['t', 'merge0', 'merge1', 'merge2', 'out'] },
    },
  ],
};

// ═══════════════════════════════════════════════════════════════════════════
// 6. EVERY NODE KIND IN ONE FLOW
// ═══════════════════════════════════════════════════════════════════════════

export const everyNodeKindScenario: SimScenario = {
  id: 'every-node-kind',
  name: 'Kitchen Sink: Every Node Kind',
  description:
    'One flow containing every FlowNodeKind (16 types). The ultimate compatibility test.',
  category: 'integration',
  tier: 'extreme',
  tags: ['every-kind', 'kitchen-sink', 'compatibility', 'stress'],
  graph: (() => {
    const trigger = simNode('trigger', { id: 'trigger', label: 'Start', config: { prompt: 'Go' } });
    const agent1 = simNode('agent', {
      id: 'agent1',
      label: 'AI Agent',
      config: { prompt: 'Analyze.' },
    });
    const tool1 = simNode('tool', {
      id: 'tool1',
      label: 'MCP Tool (via LLM)',
      config: { prompt: 'Use a tool.' },
    });
    const cond = simNode('condition', {
      id: 'cond',
      label: 'Check Result',
      config: { conditionExpr: 'data.valid' },
    });
    const data1 = simNode('data', {
      id: 'data1',
      label: 'Transform Data',
      config: { prompt: 'Map the data.' },
    });
    const code1 = simNode('code', {
      id: 'code1',
      label: 'JS Sandbox',
      config: { code: 'return input.toUpperCase()' },
    });
    const http1 = simNode('http', {
      id: 'http1',
      label: 'HTTP Call',
      config: { httpUrl: 'https://api.test.com/data', httpMethod: 'GET' },
    });
    const mcpTool = simNode('mcp-tool', {
      id: 'mcp1',
      label: 'Direct MCP Call',
      config: { mcpToolName: 'mcp_test_tool', mcpToolArgs: '{}' },
    });
    const loop1 = simNode('loop', {
      id: 'loop1',
      label: 'Loop Each',
      config: { loopItems: '["a","b","c"]', code: 'return item.toUpperCase()' },
    });
    const squad1 = simNode('squad', {
      id: 'squad1',
      label: 'Agent Squad',
      config: { prompt: 'Squad coordination.' },
    });
    const mem1 = simNode('memory', {
      id: 'mem1',
      label: 'Store Memory',
      config: { memoryCategory: 'test', memoryImportance: 0.5 },
    });
    const memRecall = simNode('memory-recall', {
      id: 'recall1',
      label: 'Recall Memory',
      config: { memoryQuery: 'test data', memoryLimit: 5 },
    });
    const eh1 = simNode('event-horizon', {
      id: 'eh1',
      label: 'Sync Point',
      config: { mergePolicy: 'synthesize' },
    });
    const errNode = simNode('error', { id: 'err1', label: 'Error Handler' });
    const output = simNode('output', { id: 'out', label: 'Done' });

    return simGraph(
      [
        trigger,
        agent1,
        tool1,
        cond,
        data1,
        code1,
        http1,
        mcpTool,
        loop1,
        squad1,
        mem1,
        memRecall,
        eh1,
        errNode,
        output,
      ],
      [
        simEdge('trigger', 'agent1'),
        simEdge('agent1', 'tool1'),
        simEdge('tool1', 'cond'),
        simEdge('cond', 'data1', { label: 'true' }),
        simEdge('cond', 'err1', { label: 'false' }),
        simEdge('data1', 'code1'),
        simEdge('code1', 'http1'),
        simEdge('http1', 'mcp1'),
        simEdge('mcp1', 'loop1'),
        simEdge('loop1', 'squad1'),
        simEdge('squad1', 'mem1'),
        simEdge('mem1', 'recall1'),
        simEdge('recall1', 'eh1'),
        simEdge('eh1', 'out'),
        simEdge('err1', 'out'),
      ],
      { name: 'Kitchen Sink: Every Node Kind' },
    );
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
    nodeOverrides: {
      cond: { strategy: 'static', response: 'true', forceConditionResult: true },
    },
    httpMocks: [{ urlPattern: 'api.test.com', status: 200, body: '{"result":"ok"}' }],
    mcpMocks: {
      mcp_test_tool: { success: true, content: '{"tool_output":"processed"}' },
    },
    memoryMocks: {
      memories: [
        {
          keywords: ['test', 'data'],
          content: 'Previously stored test data from earlier runs.',
          relevance: 0.9,
          category: 'test',
        },
      ],
      defaultRecallResponse: 'No test data found.',
    },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'Kitchen sink flow completes',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'conductor-used',
      description: 'Conductor activates (16 nodes)',
      check: { type: 'conductor-used', expected: true },
    },
    {
      type: 'node-executed',
      description: 'Agent node ran',
      check: { type: 'node-executed', nodeId: 'agent1', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Tool node ran',
      check: { type: 'node-executed', nodeId: 'tool1', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Code node ran',
      check: { type: 'node-executed', nodeId: 'code1', executed: true },
    },
    {
      type: 'node-executed',
      description: 'HTTP node ran',
      check: { type: 'node-executed', nodeId: 'http1', executed: true },
    },
    {
      type: 'node-executed',
      description: 'MCP node ran',
      check: { type: 'node-executed', nodeId: 'mcp1', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Loop node ran',
      check: { type: 'node-executed', nodeId: 'loop1', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Memory store ran',
      check: { type: 'node-executed', nodeId: 'mem1', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Memory recall ran',
      check: { type: 'node-executed', nodeId: 'recall1', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Event horizon ran',
      check: { type: 'node-executed', nodeId: 'eh1', executed: true },
    },
  ],
};

// ═══════════════════════════════════════════════════════════════════════════
// 7. CASCADING CONDITION TREE (4 levels deep)
// ═══════════════════════════════════════════════════════════════════════════

export const cascadingConditionTreeScenario: SimScenario = {
  id: 'cascading-condition-tree',
  name: 'Cascading Condition Tree (4 Levels)',
  description: 'Binary decision tree: 4 levels of if/else branching, 16 possible leaf paths.',
  category: 'branching',
  tier: 'complex',
  tags: ['condition', 'tree', 'deep-branching', 'decision', 'stress'],
  graph: (() => {
    const trigger = simNode('trigger', {
      id: 't',
      label: 'Classify Input',
      config: { prompt: 'Classify this request.' },
    });
    // Level 1
    const c1 = simNode('condition', {
      id: 'c1',
      label: 'Is Business?',
      config: { conditionExpr: 'category === "business"' },
    });
    // Level 2
    const c2a = simNode('condition', {
      id: 'c2a',
      label: 'Is Urgent?',
      config: { conditionExpr: 'priority === "urgent"' },
    });
    const c2b = simNode('condition', {
      id: 'c2b',
      label: 'Is Technical?',
      config: { conditionExpr: 'type === "technical"' },
    });
    // Level 3
    const c3a = simNode('condition', {
      id: 'c3a',
      label: 'Has Budget?',
      config: { conditionExpr: 'budget > 0' },
    });
    const c3b = simNode('condition', {
      id: 'c3b',
      label: 'Is Critical?',
      config: { conditionExpr: 'severity === "critical"' },
    });
    // Leaf agents — one per path taken
    const leaf1 = simNode('agent', { id: 'leaf1', label: 'Urgent+Budget Handler' });
    const leaf2 = simNode('agent', { id: 'leaf2', label: 'Urgent+NoBudget Handler' });
    const leaf3 = simNode('agent', { id: 'leaf3', label: 'NonUrgent Business Handler' });
    const leaf4 = simNode('agent', { id: 'leaf4', label: 'Critical Tech Handler' });
    const leaf5 = simNode('agent', { id: 'leaf5', label: 'NonCritical Tech Handler' });
    const leaf6 = simNode('agent', { id: 'leaf6', label: 'General Handler' });
    const output = simNode('output', { id: 'out', label: 'Response' });

    return simGraph(
      [trigger, c1, c2a, c2b, c3a, c3b, leaf1, leaf2, leaf3, leaf4, leaf5, leaf6, output],
      [
        simEdge('t', 'c1'),
        // Level 1 branches
        simEdge('c1', 'c2a', { label: 'true' }),
        simEdge('c1', 'c2b', { label: 'false' }),
        // Level 2a branches
        simEdge('c2a', 'c3a', { label: 'true' }),
        simEdge('c2a', 'leaf3', { label: 'false' }),
        // Level 2b branches
        simEdge('c2b', 'c3b', { label: 'true' }),
        simEdge('c2b', 'leaf6', { label: 'false' }),
        // Level 3a branches
        simEdge('c3a', 'leaf1', { label: 'true' }),
        simEdge('c3a', 'leaf2', { label: 'false' }),
        // Level 3b branches
        simEdge('c3b', 'leaf4', { label: 'true' }),
        simEdge('c3b', 'leaf5', { label: 'false' }),
        // All leaves → output
        simEdge('leaf1', 'out'),
        simEdge('leaf2', 'out'),
        simEdge('leaf3', 'out'),
        simEdge('leaf4', 'out'),
        simEdge('leaf5', 'out'),
        simEdge('leaf6', 'out'),
      ],
      { name: 'Cascading Condition Tree' },
    );
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
    nodeOverrides: {
      // Take the longest path: true → true → true → leaf1
      c1: { strategy: 'static', response: 'true', forceConditionResult: true },
      c2a: { strategy: 'static', response: 'true', forceConditionResult: true },
      c3a: { strategy: 'static', response: 'true', forceConditionResult: true },
      // These won't be reached but set them anyway
      c2b: { strategy: 'static', response: 'true', forceConditionResult: true },
      c3b: { strategy: 'static', response: 'true', forceConditionResult: true },
    },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'Decision tree completes',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'node-executed',
      description: 'Root condition ran',
      check: { type: 'node-executed', nodeId: 'c1', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Level 2 condition ran',
      check: { type: 'node-executed', nodeId: 'c2a', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Level 3 condition ran',
      check: { type: 'node-executed', nodeId: 'c3a', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Correct leaf executed (true→true→true)',
      check: { type: 'node-executed', nodeId: 'leaf1', executed: true },
    },
    // Wrong-path leaves should NOT execute
    {
      type: 'node-executed',
      description: 'Wrong leaf skipped (c2b path)',
      check: { type: 'node-executed', nodeId: 'leaf6', executed: false },
    },
    {
      type: 'node-executed',
      description: 'Wrong leaf skipped (c3b path)',
      check: { type: 'node-executed', nodeId: 'leaf4', executed: false },
    },
    {
      type: 'execution-order',
      description: 'Conditions cascade in order',
      check: { type: 'execution-order', nodeIds: ['c1', 'c2a', 'c3a', 'leaf1'] },
    },
  ],
};

// ═══════════════════════════════════════════════════════════════════════════
// 8. MULTI-ERROR CASCADE (error chains with fallbacks)
// ═══════════════════════════════════════════════════════════════════════════

export const multiErrorCascadeScenario: SimScenario = {
  id: 'multi-error-cascade',
  name: 'Multi-Error Cascade (3 Fallback Levels)',
  description:
    'Primary → fails → Fallback 1 → fails → Fallback 2 → fails → Emergency Handler. Tests deep error routing.',
  category: 'self-healing',
  tier: 'complex',
  tags: ['error', 'cascade', 'fallback', 'multi-level', 'stress'],
  graph: (() => {
    const trigger = simNode('trigger', { id: 't', label: 'Start' });
    const primary = simNode('agent', {
      id: 'primary',
      label: 'Primary Service',
      config: { prompt: 'Call primary API.' },
    });
    const fallback1 = simNode('agent', {
      id: 'fb1',
      label: 'Fallback 1',
      config: { prompt: 'Try backup API.' },
    });
    const fallback2 = simNode('agent', {
      id: 'fb2',
      label: 'Fallback 2',
      config: { prompt: 'Try emergency API.' },
    });
    const emergency = simNode('error', { id: 'emerg', label: 'Emergency Handler' });
    const output = simNode('output', { id: 'out', label: 'Result' });
    return simGraph(
      [trigger, primary, fallback1, fallback2, emergency, output],
      [
        simEdge('t', 'primary'),
        simEdge('primary', 'out'),
        simEdge('primary', 'fb1', { kind: 'error', fromPort: 'err' }),
        simEdge('fb1', 'out'),
        simEdge('fb1', 'fb2', { kind: 'error', fromPort: 'err' }),
        simEdge('fb2', 'out'),
        simEdge('fb2', 'emerg', { kind: 'error', fromPort: 'err' }),
        simEdge('emerg', 'out'),
      ],
      { name: 'Multi-Error Cascade' },
    );
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
    nodeOverrides: {
      // Primary fails
      primary: { strategy: 'static', shouldFail: true, errorMessage: 'Primary API timeout (503)' },
      // Fallback 1 also fails
      fb1: { strategy: 'static', shouldFail: true, errorMessage: 'Fallback 1 rate limited (429)' },
      // Fallback 2 also fails
      fb2: { strategy: 'static', shouldFail: true, errorMessage: 'Fallback 2 DNS failure' },
    },
  },
  expectations: [
    // Flow should still complete because emergency handler catches everything
    {
      type: 'node-status',
      description: 'Primary fails',
      check: { type: 'node-status', nodeId: 'primary', expectedStatus: 'error' },
    },
    {
      type: 'node-status',
      description: 'Fallback 1 fails',
      check: { type: 'node-status', nodeId: 'fb1', expectedStatus: 'error' },
    },
    {
      type: 'node-status',
      description: 'Fallback 2 fails',
      check: { type: 'node-status', nodeId: 'fb2', expectedStatus: 'error' },
    },
    {
      type: 'node-executed',
      description: 'Emergency handler activated',
      check: { type: 'node-executed', nodeId: 'emerg', executed: true },
    },
  ],
};

// ═══════════════════════════════════════════════════════════════════════════
// 9. DEEP COLLAPSE CHAIN (7 sequential agents)
// ═══════════════════════════════════════════════════════════════════════════

export const deepCollapseChainScenario: SimScenario = {
  id: 'deep-collapse-chain',
  name: 'Deep Collapse: 7 Sequential Agents',
  description:
    'Seven sequential agents that should all collapse into a single Conductor unit. Tests maximum collapse depth.',
  category: 'collapse',
  tier: 'complex',
  tags: ['conductor', 'collapse', 'deep', 'sequential', 'stress'],
  graph: (() => {
    const trigger = simNode('trigger', { id: 't', label: 'Start' });
    const { nodes: agents, edges: agentEdges } = agentChain(7, 'dc');
    const output = simNode('output', { id: 'out', label: 'Final' });
    return simGraph(
      [trigger, ...agents, output],
      [simEdge('t', 'dc0'), ...agentEdges, simEdge('dc6', 'out')],
      { name: 'Deep Collapse Chain' },
    );
  })(),
  mocks: {
    agentDefault: {
      strategy: 'static',
      response: Array.from({ length: 7 }, (_, i) => `Step ${i + 1} complete.`).join(
        '\n---STEP_BOUNDARY---\n',
      ),
    },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'Deep collapse completes',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'conductor-used',
      description: 'Conductor activates',
      check: { type: 'conductor-used', expected: true },
    },
    {
      type: 'strategy-shape',
      description: 'Has collapse groups',
      check: { type: 'strategy-shape', hasCollapse: true },
    },
    {
      type: 'node-executed',
      description: 'First agent ran',
      check: { type: 'node-executed', nodeId: 'dc0', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Last agent ran',
      check: { type: 'node-executed', nodeId: 'dc6', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Output reached',
      check: { type: 'node-executed', nodeId: 'out', executed: true },
    },
  ],
};

// ═══════════════════════════════════════════════════════════════════════════
// 10. MASSIVE PARALLEL FAN-OUT (10 branches)
// ═══════════════════════════════════════════════════════════════════════════

export const massiveParallelFanOutScenario: SimScenario = {
  id: 'massive-parallel-fanout',
  name: 'Massive Parallel Fan-Out (10 Branches)',
  description:
    'Trigger fans out to 10 independent branches, all merging back. Tests parallel scheduling limits.',
  category: 'parallel',
  tier: 'extreme',
  tags: ['parallel', 'fan-out', 'massive', 'scale', 'stress'],
  graph: (() => {
    const trigger = simNode('trigger', {
      id: 't',
      label: 'Distribute Tasks',
      config: { prompt: 'Distribute to 10 workers' },
    });
    const branches: ReturnType<typeof simNode>[] = [];
    const edges: ReturnType<typeof simEdge>[] = [];

    for (let i = 0; i < 10; i++) {
      const bid = `w${i}`;
      branches.push(
        simNode('agent', {
          id: bid,
          label: `Worker ${i + 1}`,
          config: { prompt: `Execute task ${i + 1}` },
        }),
      );
      edges.push(simEdge('t', bid));
      edges.push(simEdge(bid, 'merge'));
    }

    const merge = simNode('agent', {
      id: 'merge',
      label: 'Aggregator',
      config: { prompt: 'Aggregate all 10 results.' },
    });
    const output = simNode('output', { id: 'out', label: 'Combined Result' });
    edges.push(simEdge('merge', 'out'));

    return simGraph([trigger, ...branches, merge, output], edges, { name: 'Massive Fan-Out' });
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'All 10 branches complete',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'conductor-used',
      description: 'Conductor for massive fan-out',
      check: { type: 'conductor-used', expected: true },
    },
    {
      type: 'strategy-shape',
      description: 'Has parallel phases',
      check: { type: 'strategy-shape', hasParallel: true },
    },
    {
      type: 'node-executed',
      description: 'First worker',
      check: { type: 'node-executed', nodeId: 'w0', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Fifth worker',
      check: { type: 'node-executed', nodeId: 'w4', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Tenth worker',
      check: { type: 'node-executed', nodeId: 'w9', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Merge executed',
      check: { type: 'node-executed', nodeId: 'merge', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Output reached',
      check: { type: 'node-executed', nodeId: 'out', executed: true },
    },
  ],
};

// ═══════════════════════════════════════════════════════════════════════════
// 11. ALL EDGE KINDS IN ONE GRAPH
// ═══════════════════════════════════════════════════════════════════════════

export const allEdgeKindsScenario: SimScenario = {
  id: 'all-edge-kinds',
  name: 'All Edge Kinds (forward, reverse, bidirectional, error)',
  description: 'Single graph using every EdgeKind simultaneously. Tests edge kind coexistence.',
  category: 'convergent',
  tier: 'complex',
  tags: ['edges', 'forward', 'reverse', 'bidirectional', 'error', 'mixed'],
  graph: (() => {
    const trigger = simNode('trigger', { id: 't', label: 'Start' });
    const a1 = simNode('agent', { id: 'a1', label: 'Forward Source' });
    const a2 = simNode('agent', { id: 'a2', label: 'Forward Target' });
    const a3 = simNode('agent', { id: 'a3', label: 'Bidir Partner A' });
    const a4 = simNode('agent', { id: 'a4', label: 'Bidir Partner B' });
    const a5 = simNode('agent', { id: 'a5', label: 'Reverse Source' });
    const errNode = simNode('error', { id: 'err', label: 'Error Catcher' });
    const merge = simNode('agent', { id: 'merge', label: 'Final Merge' });
    const output = simNode('output', { id: 'out', label: 'Done' });
    return simGraph(
      [trigger, a1, a2, a3, a4, a5, errNode, merge, output],
      [
        // Forward edges
        simEdge('t', 'a1'),
        simEdge('t', 'a3'),
        simEdge('a1', 'a2'),
        // Bidirectional edges (debate)
        simEdge('a3', 'a4', { kind: 'bidirectional' }),
        simEdge('a4', 'a3', { kind: 'bidirectional' }),
        // Reverse edge (a5 pulls from a2)
        simEdge('a2', 'a5', { kind: 'reverse' }),
        simEdge('a2', 'a5'),
        // Error edge
        simEdge('a5', 'err', { kind: 'error', fromPort: 'err' }),
        // All paths → merge → output
        simEdge('a2', 'merge'),
        simEdge('a3', 'merge'),
        simEdge('a4', 'merge'),
        simEdge('a5', 'merge'),
        simEdge('merge', 'out'),
      ],
      { name: 'All Edge Kinds' },
    );
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
    nodeOverrides: {
      a3: {
        strategy: 'sequence',
        sequence: [
          'Initial position from partner A.',
          'Converging toward agreement after round 2.',
          'Full agreement reached between partners.',
        ],
      },
      a4: {
        strategy: 'sequence',
        sequence: [
          'Initial position from partner B.',
          'Converging toward agreement after round 2.',
          'Full agreement reached between partners.',
        ],
      },
    },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'All edge kinds coexist',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'conductor-used',
      description: 'Conductor activates (bidirectional)',
      check: { type: 'conductor-used', expected: true },
    },
    {
      type: 'strategy-shape',
      description: 'Has mesh (bidirectional)',
      check: { type: 'strategy-shape', hasMesh: true },
    },
    {
      type: 'node-executed',
      description: 'Forward chain',
      check: { type: 'node-executed', nodeId: 'a1', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Bidirectional pair',
      check: { type: 'node-executed', nodeId: 'a3', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Reverse edge target',
      check: { type: 'node-executed', nodeId: 'a5', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Merge node',
      check: { type: 'node-executed', nodeId: 'merge', executed: true },
    },
  ],
};

// ═══════════════════════════════════════════════════════════════════════════
// 12. LOOP NODE WITH ARRAY ITERATION
// ═══════════════════════════════════════════════════════════════════════════

export const loopIterationScenario: SimScenario = {
  id: 'loop-array-iteration',
  name: 'Loop: Array Iteration Pipeline',
  description: 'Processes an array of items through a loop node, then aggregates results.',
  category: 'basic',
  tier: 'standard',
  tags: ['loop', 'iteration', 'array', 'foreach'],
  graph: (() => {
    const trigger = simNode('trigger', {
      id: 't',
      label: 'Input Array',
      config: { prompt: '["alpha","beta","gamma","delta","epsilon"]' },
    });
    const preprocess = simNode('agent', {
      id: 'pre',
      label: 'Preprocess',
      config: { prompt: 'Prepare items for processing.' },
    });
    const loop = simNode('loop', {
      id: 'loop',
      label: 'Process Each Item',
      config: {
        loopItems: '["alpha","beta","gamma","delta","epsilon"]',
        code: 'return item.toUpperCase() + "_processed"',
      },
    });
    const aggregate = simNode('agent', {
      id: 'agg',
      label: 'Aggregate Results',
      config: { prompt: 'Combine all processed items into a report.' },
    });
    const output = simNode('output', { id: 'out', label: 'Final Report' });
    return simGraph(
      [trigger, preprocess, loop, aggregate, output],
      [simEdge('t', 'pre'), simEdge('pre', 'loop'), simEdge('loop', 'agg'), simEdge('agg', 'out')],
      { name: 'Loop Iteration' },
    );
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'Loop completes',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'node-executed',
      description: 'Loop node ran',
      check: { type: 'node-executed', nodeId: 'loop', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Aggregator ran',
      check: { type: 'node-executed', nodeId: 'agg', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Output reached',
      check: { type: 'node-executed', nodeId: 'out', executed: true },
    },
  ],
};

// ═══════════════════════════════════════════════════════════════════════════
// 13. SQUAD + MEMORY + MCP COMBINED
// ═══════════════════════════════════════════════════════════════════════════

export const squadMemoryMcpScenario: SimScenario = {
  id: 'squad-memory-mcp-combo',
  name: 'Squad + Memory + MCP Combined',
  description:
    'Agent squad coordinates via memory, uses MCP tools, recalls past context. Tests all integration primitives together.',
  category: 'integration',
  tier: 'complex',
  tags: ['squad', 'memory', 'mcp', 'combined', 'integration', 'stress'],
  graph: (() => {
    const trigger = simNode('trigger', {
      id: 't',
      label: 'Research Task',
      config: { prompt: 'Research and compile competitive analysis.' },
    });
    const recall = simNode('memory-recall', {
      id: 'recall',
      label: 'Recall Prior Research',
      config: { memoryQuery: 'competitive analysis', memoryLimit: 5 },
    });
    const squad = simNode('squad', {
      id: 'squad',
      label: 'Research Squad (3 agents)',
      config: { prompt: 'Coordinate research across three domains.' },
    });
    const mcpFetch = simNode('mcp-tool', {
      id: 'mcp_fetch',
      label: 'Fetch Market Data',
      config: { mcpToolName: 'mcp_market_data', mcpToolArgs: '{"sector":"ai-tools"}' },
    });
    const analyze = simNode('agent', {
      id: 'analyze',
      label: 'Synthesize Findings',
      config: { prompt: 'Combine squad output with market data.' },
    });
    const store = simNode('memory', {
      id: 'store',
      label: 'Store Analysis',
      config: { memoryCategory: 'competitive', memoryImportance: 0.9 },
    });
    const format = simNode('code', {
      id: 'fmt',
      label: 'Format Report',
      config: { code: 'return "## Competitive Analysis\\n\\n" + input' },
    });
    const output = simNode('output', { id: 'out', label: 'Final Report' });

    return simGraph(
      [trigger, recall, squad, mcpFetch, analyze, store, format, output],
      [
        simEdge('t', 'recall'),
        simEdge('t', 'squad'),
        simEdge('t', 'mcp_fetch'),
        simEdge('recall', 'analyze'),
        simEdge('squad', 'analyze'),
        simEdge('mcp_fetch', 'analyze'),
        simEdge('analyze', 'store'),
        simEdge('analyze', 'fmt'),
        simEdge('store', 'out'),
        simEdge('fmt', 'out'),
      ],
      { name: 'Squad + Memory + MCP' },
    );
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
    mcpMocks: {
      mcp_market_data: {
        success: true,
        content: '{"market_size":"$15B","growth":"42%","top_players":["copilot","cursor","cody"]}',
      },
    },
    memoryMocks: {
      memories: [
        {
          keywords: ['competitive', 'analysis'],
          content:
            'Q3 analysis: GitHub Copilot leads with 40% market share. Cursor growing fastest at 200% QoQ.',
          relevance: 0.95,
          category: 'competitive',
        },
      ],
      defaultRecallResponse: 'No prior competitive analysis found.',
    },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'Combined flow completes',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'conductor-used',
      description: 'Conductor for fan-out',
      check: { type: 'conductor-used', expected: true },
    },
    {
      type: 'node-executed',
      description: 'Memory recall ran',
      check: { type: 'node-executed', nodeId: 'recall', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Squad ran',
      check: { type: 'node-executed', nodeId: 'squad', executed: true },
    },
    {
      type: 'node-executed',
      description: 'MCP fetch ran',
      check: { type: 'node-executed', nodeId: 'mcp_fetch', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Memory store ran',
      check: { type: 'node-executed', nodeId: 'store', executed: true },
    },
    {
      type: 'node-output',
      description: 'Memory recall returned history',
      check: { type: 'node-output', nodeId: 'recall', contains: 'Copilot' },
    },
    {
      type: 'node-output',
      description: 'MCP returned market data',
      check: { type: 'node-output', nodeId: 'mcp_fetch', contains: 'market_size' },
    },
  ],
};

// ═══════════════════════════════════════════════════════════════════════════
// 14. CHAOS + RETRY COMBINED TORTURE TEST
// ═══════════════════════════════════════════════════════════════════════════

export const chaosRetryTortureScenario: SimScenario = {
  id: 'chaos-retry-torture',
  name: 'Chaos + Retry Torture Test',
  description:
    'High failure rate (50%) with retry on every node. Tests resilience under extreme pressure.',
  category: 'self-healing',
  tier: 'extreme',
  tags: ['chaos', 'retry', 'torture', 'resilience', 'stress'],
  graph: (() => {
    const trigger = simNode('trigger', { id: 't', label: 'Start' });
    const nodes: ReturnType<typeof simNode>[] = [trigger];
    const edges: ReturnType<typeof simEdge>[] = [];

    // 6 agents in a chain, each with retries
    for (let i = 0; i < 6; i++) {
      nodes.push(
        simNode('agent', {
          id: `a${i}`,
          label: `Retry Node ${i + 1}`,
          config: { prompt: `Process step ${i + 1}`, maxRetries: 3, retryDelayMs: 10 },
        }),
      );
      if (i === 0) edges.push(simEdge('t', 'a0'));
      else edges.push(simEdge(`a${i - 1}`, `a${i}`));
    }

    // Error handler for anything that gets through
    const errHandler = simNode('error', { id: 'err', label: 'Error Sink' });
    nodes.push(errHandler);
    for (let i = 0; i < 6; i++) {
      edges.push(simEdge(`a${i}`, 'err', { kind: 'error', fromPort: 'err' }));
    }

    const output = simNode('output', { id: 'out', label: 'Result' });
    nodes.push(output);
    edges.push(simEdge('a5', 'out'));

    return simGraph(nodes, edges, { name: 'Chaos + Retry Torture' });
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
    failureRate: 0.5,
    failureMessage: 'Random chaos failure during torture test!',
  },
  expectations: [
    // We can't guarantee success with 50% failure even with retries
    {
      type: 'event-emitted',
      description: 'Run started',
      check: { type: 'event-emitted', eventType: 'run-start' },
    },
    {
      type: 'event-emitted',
      description: 'Run completed',
      check: { type: 'event-emitted', eventType: 'run-complete' },
    },
    {
      type: 'node-executed',
      description: 'First node attempted',
      check: { type: 'node-executed', nodeId: 'a0', executed: true },
    },
  ],
};

// ═══════════════════════════════════════════════════════════════════════════
// 15. WIDE ORCHESTRATOR (Boss + 8 Workers + Reviewer)
// ═══════════════════════════════════════════════════════════════════════════

export const wideOrchestratorScenario: SimScenario = {
  id: 'wide-orchestrator',
  name: 'Wide Orchestrator (Boss + 8 Workers)',
  description:
    'Boss delegates to 8 specialized workers, reviewer validates, then final output. Enterprise-scale orchestration.',
  category: 'orchestrator',
  tier: 'extreme',
  tags: ['orchestrator', 'wide', 'boss-worker', 'enterprise', 'stress'],
  graph: (() => {
    const trigger = simNode('trigger', {
      id: 't',
      label: 'Enterprise Project',
      config: { prompt: 'Build enterprise SaaS platform.' },
    });
    const boss = simNode('agent', {
      id: 'boss',
      label: 'CTO (Boss)',
      config: { prompt: 'Break down into 8 workstreams and delegate.', model: 'claude-opus' },
    });

    const roles = [
      'Backend',
      'Frontend',
      'DevOps',
      'Security',
      'Database',
      'API Design',
      'Testing',
      'Documentation',
    ];
    const workers: ReturnType<typeof simNode>[] = [];
    const edges: ReturnType<typeof simEdge>[] = [simEdge('t', 'boss')];

    for (let i = 0; i < 8; i++) {
      const wid = `w${i}`;
      workers.push(
        simNode('agent', {
          id: wid,
          label: `${roles[i]} Engineer`,
          config: { prompt: `Handle ${roles[i]} workstream.`, model: 'cheap-model' },
        }),
      );
      edges.push(simEdge('boss', wid));
      edges.push(simEdge(wid, 'reviewer'));
    }

    const reviewer = simNode('agent', {
      id: 'reviewer',
      label: 'Architecture Reviewer',
      config: { prompt: 'Review all 8 workstreams for consistency.', model: 'claude-opus' },
    });
    const output = simNode('output', { id: 'out', label: 'Architecture Document' });
    edges.push(simEdge('reviewer', 'out'));

    return simGraph([trigger, boss, ...workers, reviewer, output], edges, {
      name: 'Wide Orchestrator',
    });
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
    nodeOverrides: {
      boss: {
        strategy: 'static',
        response:
          'Delegating to 8 workstreams: Backend, Frontend, DevOps, Security, Database, API, Testing, Docs.',
      },
      reviewer: {
        strategy: 'static',
        response:
          '## Architecture Review\n\nAll 8 workstreams are consistent and aligned.\n\n- Integration points verified ✓\n- Security posture validated ✓\n- Performance targets achievable ✓\n\nApproved for implementation.',
      },
    },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'Wide orchestrator completes',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'conductor-used',
      description: 'Conductor for 8-way fan-out',
      check: { type: 'conductor-used', expected: true },
    },
    {
      type: 'strategy-shape',
      description: 'Has parallel',
      check: { type: 'strategy-shape', hasParallel: true },
    },
    {
      type: 'node-executed',
      description: 'Boss delegates',
      check: { type: 'node-executed', nodeId: 'boss', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Backend worker',
      check: { type: 'node-executed', nodeId: 'w0', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Testing worker',
      check: { type: 'node-executed', nodeId: 'w6', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Docs worker',
      check: { type: 'node-executed', nodeId: 'w7', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Reviewer validates',
      check: { type: 'node-executed', nodeId: 'reviewer', executed: true },
    },
    {
      type: 'node-output',
      description: 'Reviewer approves',
      check: { type: 'node-output', nodeId: 'reviewer', contains: 'Approved' },
    },
    {
      type: 'execution-order',
      description: 'Boss before reviewer',
      check: { type: 'execution-order', nodeIds: ['boss', 'reviewer', 'out'] },
    },
  ],
};

// ═══════════════════════════════════════════════════════════════════════════
// 16. VARIABLE PROPAGATION STRESS (chain of code nodes setting variables)
// ═══════════════════════════════════════════════════════════════════════════

export const variablePropagationStressScenario: SimScenario = {
  id: 'variable-propagation-stress',
  name: 'Variable Propagation Stress (8 Code Nodes)',
  description:
    'Chain of 8 code nodes, each reading the previous variable and computing a new one. Tests variable system under load.',
  category: 'basic',
  tier: 'complex',
  tags: ['variables', 'code', 'propagation', 'chain', 'stress'],
  graph: (() => {
    const trigger = simNode('trigger', { id: 't', label: 'Seed', config: { prompt: '1' } });
    const nodes: ReturnType<typeof simNode>[] = [trigger];
    const edges: ReturnType<typeof simEdge>[] = [];

    for (let i = 0; i < 8; i++) {
      nodes.push(
        simNode('code', {
          id: `v${i}`,
          label: `Compute ${i}`,
          config: {
            code: `return String(Number(input || '1') * 2)`,
            setVariableKey: `val_${i}`,
          },
        }),
      );
      if (i === 0) edges.push(simEdge('t', 'v0'));
      else edges.push(simEdge(`v${i - 1}`, `v${i}`));
    }

    const output = simNode('output', { id: 'out', label: 'Final Value' });
    nodes.push(output);
    edges.push(simEdge('v7', 'out'));

    return simGraph(nodes, edges, { name: 'Variable Propagation' });
  })(),
  mocks: {},
  expectations: [
    {
      type: 'flow-status',
      description: 'Variable chain completes',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'variable-set',
      description: 'val_0 set',
      check: { type: 'variable-set', key: 'val_0', exists: true },
    },
    {
      type: 'variable-set',
      description: 'val_7 set',
      check: { type: 'variable-set', key: 'val_7', exists: true },
    },
    // 1 * 2^8 = 256
    {
      type: 'node-output',
      description: 'Final value is 256',
      check: { type: 'node-output', nodeId: 'v7', contains: '256' },
    },
  ],
};

// ═══════════════════════════════════════════════════════════════════════════
// 17. ZAPIER ETL MEGA-PIPELINE (HTTP → Transform → Agent × 5 branches)
// ═══════════════════════════════════════════════════════════════════════════

export const zapierEtlPipelineScenario: SimScenario = {
  id: 'zapier-etl-pipeline',
  name: 'Zapier ETL: 5-Source Data Pipeline',
  description:
    'Simulates real Zapier ETL: fetch from 5 APIs, transform each, merge, analyze, store. 20+ nodes.',
  category: 'integration',
  tier: 'extreme',
  tags: ['zapier', 'etl', 'pipeline', 'multi-source', 'http', 'stress'],
  graph: (() => {
    const trigger = simNode('trigger', {
      id: 't',
      label: 'ETL Trigger (Cron/Webhook)',
      config: { prompt: 'Daily ETL pipeline run' },
    });
    const sources = ['salesforce', 'hubspot', 'stripe', 'postgres', 'analytics'];
    const allNodes: ReturnType<typeof simNode>[] = [trigger];
    const allEdges: ReturnType<typeof simEdge>[] = [];

    for (const src of sources) {
      const fetchId = `fetch_${src}`;
      const transformId = `tx_${src}`;
      const validateId = `val_${src}`;
      allNodes.push(
        simNode('http', {
          id: fetchId,
          label: `Fetch ${src}`,
          config: { httpUrl: `https://api.${src}.com/export`, httpMethod: 'GET' },
        }),
        simNode('code', {
          id: transformId,
          label: `Transform ${src}`,
          config: {
            code: `return JSON.stringify({ source: "${src}", data: input, ts: Date.now() })`,
          },
        }),
        simNode('agent', {
          id: validateId,
          label: `Validate ${src}`,
          config: { prompt: `Validate ${src} data integrity.` },
        }),
      );
      allEdges.push(
        simEdge('t', fetchId),
        simEdge(fetchId, transformId),
        simEdge(transformId, validateId),
        simEdge(validateId, 'merge'),
      );
    }

    const merge = simNode('agent', {
      id: 'merge',
      label: 'Merge All Sources',
      config: { prompt: 'Merge data from all 5 sources into unified schema.' },
    });
    const dedupe = simNode('code', {
      id: 'dedupe',
      label: 'Deduplicate',
      config: { code: 'return input' },
    });
    const analyze = simNode('agent', {
      id: 'analyze',
      label: 'Analyze Merged Data',
      config: { prompt: 'Run cross-source analysis.' },
    });
    const store = simNode('memory', {
      id: 'store',
      label: 'Store in Data Lake',
      config: { memoryCategory: 'etl', memoryImportance: 0.7 },
    });
    const notify = simNode('agent', {
      id: 'notify',
      label: 'Send Notification',
      config: { prompt: 'Notify team of ETL completion.' },
    });
    const output = simNode('output', { id: 'out', label: 'ETL Complete' });

    allNodes.push(merge, dedupe, analyze, store, notify, output);
    allEdges.push(
      simEdge('merge', 'dedupe'),
      simEdge('dedupe', 'analyze'),
      simEdge('analyze', 'store'),
      simEdge('analyze', 'notify'),
      simEdge('store', 'out'),
      simEdge('notify', 'out'),
    );

    return simGraph(allNodes, allEdges, { name: 'Zapier ETL Pipeline' });
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
    httpMocks: [
      { urlPattern: 'salesforce', status: 200, body: '{"accounts":150,"deals":42}' },
      { urlPattern: 'hubspot', status: 200, body: '{"contacts":3200,"campaigns":12}' },
      { urlPattern: 'stripe', status: 200, body: '{"revenue":125000,"subscriptions":890}' },
      { urlPattern: 'postgres', status: 200, body: '{"rows":50000,"tables":45}' },
      { urlPattern: 'analytics', status: 200, body: '{"pageviews":1200000,"sessions":340000}' },
    ],
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'ETL pipeline completes',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'conductor-used',
      description: 'Conductor for 5-way fan-out',
      check: { type: 'conductor-used', expected: true },
    },
    {
      type: 'strategy-shape',
      description: 'Has parallel phases',
      check: { type: 'strategy-shape', hasParallel: true },
    },
    {
      type: 'node-executed',
      description: 'Salesforce fetch',
      check: { type: 'node-executed', nodeId: 'fetch_salesforce', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Stripe fetch',
      check: { type: 'node-executed', nodeId: 'fetch_stripe', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Analytics transform',
      check: { type: 'node-executed', nodeId: 'tx_analytics', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Merge executed',
      check: { type: 'node-executed', nodeId: 'merge', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Store executed',
      check: { type: 'node-executed', nodeId: 'store', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Notification sent',
      check: { type: 'node-executed', nodeId: 'notify', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Output reached',
      check: { type: 'node-executed', nodeId: 'out', executed: true },
    },
  ],
};

// ═══════════════════════════════════════════════════════════════════════════
// 18. TESSERACT REVERSED (Reverse-edge tesseract — cells pull from horizons)
// ═══════════════════════════════════════════════════════════════════════════

export const tesseractReversedScenario: SimScenario = {
  id: 'tesseract-reversed',
  name: 'Tesseract Reversed (Pull-Based Phases)',
  description:
    'Tesseract where later phases pull data from earlier phases via reverse edges. Inverts the normal push model.',
  category: 'tesseract',
  tier: 'extreme',
  tags: ['tesseract', 'reverse', 'pull', 'phases', 'stress'],
  graph: (() => {
    const trigger = simNode('trigger', { id: 't', label: 'Kick-off', phase: 0 });
    // Phase 0: Data providers
    const p0a = simNode('agent', {
      id: 'p0a',
      label: 'Data Provider A',
      phase: 0,
      cellId: 'cell_p0a',
    });
    const p0b = simNode('agent', {
      id: 'p0b',
      label: 'Data Provider B',
      phase: 0,
      cellId: 'cell_p0b',
    });
    const eh0 = simNode('event-horizon', {
      id: 'eh0',
      label: 'Phase 0 → 1',
      phase: 0,
      config: { mergePolicy: 'synthesize' },
    });
    // Phase 1: Processors (pull from phase 0)
    const p1a = simNode('agent', { id: 'p1a', label: 'Processor A', phase: 1, cellId: 'cell_p1a' });
    const p1b = simNode('agent', { id: 'p1b', label: 'Processor B', phase: 1, cellId: 'cell_p1b' });
    const eh1 = simNode('event-horizon', {
      id: 'eh1',
      label: 'Phase 1 → 2',
      phase: 1,
      config: { mergePolicy: 'vote' },
    });
    // Phase 2: Final
    const final = simNode('agent', { id: 'final', label: 'Final Synthesis', phase: 2 });
    const output = simNode('output', { id: 'out', label: 'Result', phase: 2 });

    return simGraph(
      [trigger, p0a, p0b, eh0, p1a, p1b, eh1, final, output],
      [
        simEdge('t', 'p0a'),
        simEdge('t', 'p0b'),
        simEdge('p0a', 'eh0'),
        simEdge('p0b', 'eh0'),
        simEdge('eh0', 'p1a'),
        simEdge('eh0', 'p1b'),
        // Reverse edges: processors pull directly from data providers for raw data
        simEdge('p0a', 'p1a', { kind: 'reverse' }),
        simEdge('p0b', 'p1b', { kind: 'reverse' }),
        simEdge('p1a', 'eh1'),
        simEdge('p1b', 'eh1'),
        simEdge('eh1', 'final'),
        // Reverse edge: final pulls from phase 0 for cross-check
        simEdge('p0a', 'final', { kind: 'reverse' }),
        simEdge('final', 'out'),
      ],
      { name: 'Tesseract Reversed' },
    );
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
    nodeOverrides: {
      p0a: { strategy: 'static', response: 'Raw dataset A: [10, 20, 30]' },
      p0b: { strategy: 'static', response: 'Raw dataset B: [40, 50, 60]' },
      p1a: { strategy: 'static', response: 'Processed A: sum=60, mean=20' },
      p1b: { strategy: 'static', response: 'Processed B: sum=150, mean=50' },
      final: {
        strategy: 'static',
        response: 'Grand total: sum=210, cross-validated with raw data.',
      },
    },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'Reversed tesseract completes',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'conductor-used',
      description: 'Conductor activates',
      check: { type: 'conductor-used', expected: true },
    },
    {
      type: 'node-executed',
      description: 'Phase 0 providers run',
      check: { type: 'node-executed', nodeId: 'p0a', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Phase 1 processors run',
      check: { type: 'node-executed', nodeId: 'p1a', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Horizons fire',
      check: { type: 'node-executed', nodeId: 'eh0', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Final synthesis',
      check: { type: 'node-executed', nodeId: 'final', executed: true },
    },
    {
      type: 'execution-order',
      description: 'Phase ordering maintained',
      check: { type: 'execution-order', nodeIds: ['t', 'eh0', 'eh1', 'out'] },
    },
  ],
};

// ═══════════════════════════════════════════════════════════════════════════
// 19. PARALLEL COLLAPSE HYBRID (parallel branches, each internally collapsed)
// ═══════════════════════════════════════════════════════════════════════════

export const parallelCollapseHybridScenario: SimScenario = {
  id: 'parallel-collapse-hybrid',
  name: 'Parallel × Collapse Hybrid',
  description:
    '3 parallel branches, each containing 3 collapsible sequential agents. Tests parallel + collapse interplay.',
  category: 'parallel',
  tier: 'complex',
  tags: ['conductor', 'parallel', 'collapse', 'hybrid', 'stress'],
  graph: (() => {
    const trigger = simNode('trigger', { id: 't', label: 'Start' });
    const allNodes: ReturnType<typeof simNode>[] = [trigger];
    const allEdges: ReturnType<typeof simEdge>[] = [];

    for (let branch = 0; branch < 3; branch++) {
      const prefix = `b${branch}`;
      for (let step = 0; step < 3; step++) {
        const nid = `${prefix}_s${step}`;
        allNodes.push(
          simNode('agent', {
            id: nid,
            label: `Branch ${branch + 1} Step ${step + 1}`,
            config: { prompt: `Execute step ${step + 1} in branch ${branch + 1}.` },
          }),
        );
        if (step === 0) allEdges.push(simEdge('t', nid));
        else allEdges.push(simEdge(`${prefix}_s${step - 1}`, nid));
      }
      // Last step of each branch → merge
      allEdges.push(simEdge(`${prefix}_s2`, 'merge'));
    }

    const merge = simNode('agent', {
      id: 'merge',
      label: 'Merge All',
      config: { prompt: 'Merge 3 branch results.' },
    });
    const output = simNode('output', { id: 'out', label: 'Combined Output' });
    allNodes.push(merge, output);
    allEdges.push(simEdge('merge', 'out'));

    return simGraph(allNodes, allEdges, { name: 'Parallel × Collapse' });
  })(),
  mocks: {
    agentDefault: {
      strategy: 'static',
      response: 'Step A.\n---STEP_BOUNDARY---\nStep B.\n---STEP_BOUNDARY---\nStep C.',
    },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'Hybrid completes',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'conductor-used',
      description: 'Conductor activates',
      check: { type: 'conductor-used', expected: true },
    },
    {
      type: 'strategy-shape',
      description: 'Has both parallel and collapse',
      check: { type: 'strategy-shape', hasParallel: true, hasCollapse: true },
    },
    {
      type: 'node-executed',
      description: 'Branch 0 start',
      check: { type: 'node-executed', nodeId: 'b0_s0', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Branch 1 end',
      check: { type: 'node-executed', nodeId: 'b1_s2', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Branch 2 end',
      check: { type: 'node-executed', nodeId: 'b2_s2', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Merge ran',
      check: { type: 'node-executed', nodeId: 'merge', executed: true },
    },
  ],
};

// ═══════════════════════════════════════════════════════════════════════════
// 20. ADVERSARIAL MOCK: Sequence Exhaustion & Edge Cases
// ═══════════════════════════════════════════════════════════════════════════

export const adversarialMockScenario: SimScenario = {
  id: 'adversarial-mock-stress',
  name: 'Adversarial Mock: Edge Cases',
  description:
    'Tests mock system edge cases: empty responses, huge outputs, sequence wrap-around, unicode.',
  category: 'basic',
  tier: 'standard',
  tags: ['mock', 'adversarial', 'edge-case', 'unicode', 'stress'],
  graph: (() => {
    const trigger = simNode('trigger', {
      id: 't',
      label: 'Start',
      config: { prompt: 'Test edge cases' },
    });
    const emptyAgent = simNode('agent', { id: 'empty', label: 'Empty Response Agent' });
    const unicodeAgent = simNode('agent', { id: 'unicode', label: 'Unicode Agent' });
    const seqAgent = simNode('agent', { id: 'seq', label: 'Sequence Wrap Agent' });
    const echoAgent = simNode('agent', { id: 'echo', label: 'Echo Agent' });
    const output = simNode('output', { id: 'out', label: 'Result' });
    return simGraph(
      [trigger, emptyAgent, unicodeAgent, seqAgent, echoAgent, output],
      [
        simEdge('t', 'empty'),
        simEdge('t', 'unicode'),
        simEdge('t', 'seq'),
        simEdge('t', 'echo'),
        simEdge('empty', 'out'),
        simEdge('unicode', 'out'),
        simEdge('seq', 'out'),
        simEdge('echo', 'out'),
      ],
      { name: 'Adversarial Mock' },
    );
  })(),
  mocks: {
    agentDefault: { strategy: 'realistic' },
    nodeOverrides: {
      empty: { strategy: 'static', response: '' },
      unicode: { strategy: 'static', response: '🌟 Ünïcödé テスト: 你好世界 مرحبا العالم 🎉' },
      seq: {
        strategy: 'sequence',
        sequence: ['First', 'Second'], // only 2 items, 3rd call wraps
      },
      echo: { strategy: 'echo', echoPrefix: '[ECHO] ' },
    },
  },
  expectations: [
    {
      type: 'flow-status',
      description: 'Edge cases dont crash',
      check: { type: 'flow-status', expectedStatus: 'success' },
    },
    {
      type: 'node-executed',
      description: 'Empty response handled',
      check: { type: 'node-executed', nodeId: 'empty', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Unicode handled',
      check: { type: 'node-executed', nodeId: 'unicode', executed: true },
    },
    {
      type: 'node-output',
      description: 'Unicode preserved',
      check: { type: 'node-output', nodeId: 'unicode', contains: '🌟' },
    },
    {
      type: 'node-executed',
      description: 'Sequence agent ran',
      check: { type: 'node-executed', nodeId: 'seq', executed: true },
    },
    {
      type: 'node-executed',
      description: 'Echo agent ran',
      check: { type: 'node-executed', nodeId: 'echo', executed: true },
    },
  ],
};

// ═══════════════════════════════════════════════════════════════════════════
// STRESS TEST SUITE
// ═══════════════════════════════════════════════════════════════════════════

/** All 20 stress-test scenarios. */
export const allStressScenarios: SimScenario[] = [
  zapierMegaChainScenario, // 1
  deepTesseractScenario, // 2
  reverseEdgeScenario, // 3
  tripleMeshDebateScenario, // 4
  multiDiamondScenario, // 5
  everyNodeKindScenario, // 6
  cascadingConditionTreeScenario, // 7
  multiErrorCascadeScenario, // 8
  deepCollapseChainScenario, // 9
  massiveParallelFanOutScenario, // 10
  allEdgeKindsScenario, // 11
  loopIterationScenario, // 12
  squadMemoryMcpScenario, // 13
  chaosRetryTortureScenario, // 14
  wideOrchestratorScenario, // 15
  variablePropagationStressScenario, // 16
  zapierEtlPipelineScenario, // 17
  tesseractReversedScenario, // 18
  parallelCollapseHybridScenario, // 19
  adversarialMockScenario, // 20
];

/** Stress test suite. */
export const stressTestSuite: SimSuite = {
  id: 'stress',
  name: 'Stress Test Suite',
  description: 'Massive, complex, adversarial scenarios designed to break every assumption.',
  scenarios: allStressScenarios,
  globalMocks: {
    agentDefault: { strategy: 'realistic', modelName: 'sim-mock-stress-7b' },
    simulateStreaming: false,
    latencyMs: 0,
  },
};
