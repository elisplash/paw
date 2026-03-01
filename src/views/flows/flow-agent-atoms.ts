// ─────────────────────────────────────────────────────────────────────────────
// Flow Architect Agent — Atoms
// Pure data: system prompt, tool schemas, types. No DOM, no IPC.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowGraph } from './atoms';

// ── Types ──────────────────────────────────────────────────────────────────

export interface FlowAgentMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: string;
}

export interface FlowAgentState {
  sessionKey: string;
  messages: FlowAgentMessage[];
  isStreaming: boolean;
  streamContent: string;
}

// ── Suggested Action Chips ─────────────────────────────────────────────────

export interface FlowAgentChip {
  label: string;
  icon: string;
  prompt: string;
}

export function getDefaultChips(_graph?: FlowGraph): FlowAgentChip[] {
  const chips: FlowAgentChip[] = [
    { label: 'Explain', icon: 'description', prompt: 'Explain what this flow does step by step.' },
    {
      label: 'Optimize',
      icon: 'speed',
      prompt:
        'Analyze this flow and suggest Conductor optimizations — collapse chains, extract direct actions, parallelize branches.',
    },
  ];

  if (_graph && _graph.nodes.length > 0) {
    chips.push({
      label: 'Add errors',
      icon: 'error_outline',
      prompt: 'Add error handling edges and fallback nodes to this flow.',
    });

    const hasAgents = _graph.nodes.some((n) => n.kind === 'agent');
    if (hasAgents && _graph.nodes.length >= 4) {
      chips.push({
        label: 'Tesseract',
        icon: 'blur_on',
        prompt:
          'Could this flow benefit from a Tesseract structure? Analyze which nodes could become independent cells with event horizons.',
      });
    }
  } else {
    chips.push({
      label: 'Build',
      icon: 'add_circle',
      prompt:
        'Help me build a new flow. Ask me what I want to automate and create the nodes and edges.',
    });
  }

  return chips;
}

// ── Session Key ────────────────────────────────────────────────────────────

export function makeFlowAgentSessionKey(graphId: string): string {
  return `flow-architect-${graphId}`;
}

// ── Graph Serialization for Context ────────────────────────────────────────

/**
 * Serialize a FlowGraph into a compact text summary the LLM can reason about.
 * Keeps token count low by omitting positions and runtime state.
 */
export function serializeGraphForAgent(graph: FlowGraph): string {
  if (graph.nodes.length === 0) return 'Empty flow (no nodes).';

  const lines: string[] = [
    `Flow: "${graph.name}" (${graph.nodes.length} nodes, ${graph.edges.length} edges)`,
  ];

  if (graph.description) lines.push(`Description: ${graph.description}`);

  lines.push('', 'Nodes:');
  for (const n of graph.nodes) {
    let detail = `  [${n.id.slice(0, 8)}] ${n.kind}: "${n.label}"`;
    if (n.description) detail += ` — ${n.description}`;
    if (n.depth > 0) detail += ` (Z=${n.depth})`;
    if (n.phase > 0) detail += ` (W=${n.phase})`;
    if (n.cellId) detail += ` (cell=${n.cellId})`;
    const configKeys = Object.keys(n.config);
    if (configKeys.length > 0) {
      const safeConfig: Record<string, unknown> = {};
      for (const k of configKeys) {
        const v = n.config[k];
        // Truncate long strings
        safeConfig[k] = typeof v === 'string' && v.length > 100 ? `${v.slice(0, 100)}…` : v;
      }
      detail += ` config=${JSON.stringify(safeConfig)}`;
    }
    lines.push(detail);
  }

  if (graph.edges.length > 0) {
    lines.push('', 'Edges:');
    for (const e of graph.edges) {
      const fromNode = graph.nodes.find((n) => n.id === e.from);
      const toNode = graph.nodes.find((n) => n.id === e.to);
      const arrow =
        e.kind === 'bidirectional'
          ? '↔'
          : e.kind === 'reverse'
            ? '←'
            : e.kind === 'error'
              ? '--err→'
              : '→';
      let detail = `  "${fromNode?.label ?? e.from}" ${arrow} "${toNode?.label ?? e.to}"`;
      if (e.label) detail += ` [${e.label}]`;
      if (e.condition) detail += ` when(${e.condition})`;
      lines.push(detail);
    }
  }

  return lines.join('\n');
}

// ── System Prompt ──────────────────────────────────────────────────────────

export function buildSystemPrompt(graphContext: string): string {
  return `You are the **Flow Architect** — an expert AI assistant embedded in the OpenPawz flow builder canvas. You help users build, understand, optimize, and debug AI workflows.

## Your capabilities

1. **Explain** — Walk the user through any flow step by step in plain English.
2. **Build** — When the user describes what they want, describe the nodes and edges needed. Be specific: give node kinds, labels, and connections.
3. **Optimize** — Analyze flows against the Conductor Protocol's five primitives:
   - **Collapse**: adjacent agent chains that can merge into one LLM call
   - **Extract**: deterministic nodes (http, mcp-tool, code) that skip the LLM
   - **Parallelize**: independent branches that run concurrently
   - **Converge**: bidirectional meshes that iterate until stable
   - **Tesseract**: independent cells across phase/depth dimensions with event horizons
4. **Debug** — When a node fails, explain likely causes and suggest fixes.
5. **Advise** — Proactively suggest improvements: error handling, tesseract restructuring, better node configurations.

## Node kinds you can use

- \`trigger\` — webhook, cron, or user input that starts the flow
- \`agent\` — AI agent processing step (uses LLM)
- \`tool\` — MCP tool invocation
- \`condition\` — if/else branching
- \`data\` — data transform / mapping
- \`code\` — inline JavaScript (sandboxed)
- \`output\` — terminal output (log, send, store)
- \`error\` — error handler
- \`http\` — direct HTTP request (no LLM — Conductor Extract)
- \`mcp-tool\` — direct MCP tool call (no LLM — Conductor Extract)
- \`loop\` — forEach iterator
- \`squad\` — multi-agent team
- \`memory\` — write to agent memory
- \`memory-recall\` — search agent memory
- \`event-horizon\` — tesseract sync point where cells converge

## Edge kinds

- \`forward\` — normal A → B data flow
- \`reverse\` — pull: B requests from A
- \`bidirectional\` — handshake: A ↔ B (enables convergent mesh)
- \`error\` — failure routing to fallback

## Conductor Protocol

The Conductor compiles flow graphs into optimized execution strategies. When you see opportunities, call them out:
- **3+ agents in a chain** → suggest Collapse
- **Non-agent nodes (http, code, mcp-tool)** → point out they'll be Extracted (no LLM cost)
- **Independent branches** → note they'll Parallelize automatically
- **Bidirectional edges** → explain the Converge mesh pattern
- **Multiple independent agent groups that merge** → suggest Tesseract cells with event horizons

## Response style

- Be concise and direct — this is a workspace tool, not a chatbot
- Use node labels and kinds when referencing specific nodes
- When suggesting changes, be specific: "Add an \`error\` node after \`API Call\` with a forward error edge"
- Use markdown for formatting but keep it compact
- If the flow is empty, ask what the user wants to build

## Current flow context

${graphContext}`;
}

// ── Unique ID ──────────────────────────────────────────────────────────────

let _agentMsgCounter = 0;

export function nextAgentMsgId(): string {
  return `fa-${Date.now()}-${++_agentMsgCounter}`;
}
