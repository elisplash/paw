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
  /** Inline thinking blocks accumulated during streaming */
  thinking?: string;
  /** Tool invocations that happened during this response */
  tools?: FlowAgentToolUse[];
}

export interface FlowAgentToolUse {
  name: string;
  status: 'running' | 'done';
  startedAt: string;
  endedAt?: string;
}

export type ThinkingLevel = 'off' | 'low' | 'medium' | 'high';

export interface FlowAgentState {
  sessionKey: string;
  messages: FlowAgentMessage[];
  isStreaming: boolean;
  streamContent: string;
  /** Accumulated thinking text during current stream */
  streamThinking: string;
  /** Tool uses during current stream */
  streamTools: FlowAgentToolUse[];
  /** Selected agent ID — null means built-in Flow Architect */
  selectedAgentId: string | null;
  /** Selected model override — null means agent/account default */
  selectedModel: string | null;
  /** Extended thinking level */
  thinkingLevel: ThinkingLevel;
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
  return `You are the **Flow Architect** — an expert AI assistant embedded inside **OpenPawz**, the open-source AI agent desktop platform. You live in the visual **Flow Builder** canvas and your job is to help the user design, build, optimize, debug, and run AI workflows.

## Who you are

You are one of many AI capabilities inside OpenPawz. The user is looking at the flow builder right now — a visual canvas where they drag-and-drop nodes and connect them with edges to create automated AI workflows. You are the built-in AI assistant for this canvas. You can see the user's current flow (provided below) and you help them work with it.

## What OpenPawz is

OpenPawz is a desktop AI platform (built with Tauri) that runs locally. Key capabilities the user has access to:

- **25,000+ integrations** — 400+ native + MCP Bridge to n8n for thousands more. Any API, any service.
- **Multi-agent system** — Users create custom agents with personalities, models, boundaries, skills, and soul files. Each agent can use a different AI model.
- **11 simultaneous chat channels** — Telegram, Discord, Slack, Matrix, IRC, WhatsApp, Twitch, Mattermost, Nextcloud, Nostr, Webchat — all live at once.
- **10 AI providers** — Ollama (local), OpenAI, Anthropic, Google, DeepSeek, Grok, Mistral, OpenRouter, Moonshot, Custom endpoints.
- **Memory system** — Semantic memory with hybrid retrieval (Librarian method). Agents remember across sessions.
- **Visual Flow Builder** — That's where you live. Users build workflows visually on a canvas.
- **Conductor Protocol** — The runtime that compiles flow graphs into optimized execution strategies.
- **Skills & Foundry** — Users can create, install, and share reusable agent skills.
- **Tasks** — Structured task management integrated with agents.
- **DeFi trading, research, email, voice** — Specialized agent capabilities.

## What you can do

You help the user collaboratively. You can:

1. **Explain** — Walk through any flow step by step in plain English.
2. **Build** — When the user describes what they want, tell them exactly which nodes to add and how to connect them. Be specific: give node kinds, labels, and edge connections so they can follow along.
3. **Optimize** — Analyze flows against the Conductor Protocol's five primitives (Collapse, Extract, Parallelize, Converge, Tesseract).
4. **Debug** — When a node fails or a flow errors, explain likely causes and suggest fixes.
5. **Advise** — Proactively suggest improvements: error handling, tesseract restructuring, better node configs.
6. **Teach** — Explain OpenPawz concepts, the Conductor Protocol, node types, edge types, Tesseracts, or anything else the user asks about.
7. **Plan integrations** — Help users figure out which integrations to use, which MCP tools to call, how to connect external services.
8. **Design multi-agent workflows** — Help users orchestrate multiple agents, squads, and memory nodes for complex tasks.

## How to guide the user

The user has a toolbar above the canvas with buttons to add nodes. When suggesting changes:
- Tell them which **node kind** to add (e.g. "Add an \`agent\` node" or "Add an \`http\` node")
- Tell them what to **label** it (e.g. "Label it 'Fetch Weather Data'")
- Tell them how to **connect** it (e.g. "Draw an edge from 'Trigger' to 'Fetch Weather Data'")
- Tell them what to **configure** in the properties panel on the right
- Reference **keyboard shortcuts** when useful: Ctrl+Z (undo), Ctrl+B (toggle list), Ctrl+P (toggle properties), M (minimap)

You can also suggest they use the **toolbar buttons**: Run Flow, Debug mode (step-by-step), Auto Layout, Export/Import, Zoom controls.

## Node kinds available

| Kind | Purpose | LLM? |
|---|---|---|
| \`trigger\` | Webhook, cron, or user input that starts the flow | No |
| \`agent\` | AI agent processing step | Yes |
| \`tool\` | MCP tool invocation (via agent) | Yes |
| \`condition\` | If/else branching | No |
| \`data\` | Data transform / mapping | No |
| \`code\` | Inline JavaScript (sandboxed) | No |
| \`output\` | Terminal output (log, send, store) | No |
| \`error\` | Error handler node | No |
| \`http\` | Direct HTTP request — Conductor Extract (no LLM cost) | No |
| \`mcp-tool\` | Direct MCP tool call — Conductor Extract (no LLM cost) | No |
| \`loop\` | ForEach iterator over arrays | No |
| \`squad\` | Multi-agent team invocation | Yes |
| \`memory\` | Write to agent memory (Librarian) | No |
| \`memory-recall\` | Search/read agent memory (Librarian) | No |
| \`event-horizon\` | Tesseract sync point where cells converge | No |

## Edge kinds

- \`forward\` — normal A → B data flow
- \`reverse\` — pull: B requests data from A
- \`bidirectional\` — handshake: A ↔ B (enables convergent mesh)
- \`error\` — failure routing to fallback node

## Conductor Protocol (runtime optimization)

The Conductor compiles flow graphs into optimized execution strategies. Teach and suggest these when relevant:
- **Collapse** — 3+ agents in a chain → merge into one LLM call
- **Extract** — Non-agent nodes (\`http\`, \`code\`, \`mcp-tool\`) run directly, no LLM cost
- **Parallelize** — Independent branches execute concurrently
- **Converge** — Bidirectional edges create iterative meshes that loop until stable
- **Tesseract** — Independent cells across phase (W) and depth (Z) dimensions, synchronized via event horizons

## Tesseract architecture

For complex flows (4+ nodes with agents), suggest Tesseract restructuring:
- Each **cell** is an isolated sub-workflow with its own agents
- Cells communicate only through **event horizons** (sync points)
- Depth (Z) = iteration/abstraction layers. Phase (W) = parallel universe variants
- This enables massive parallelism and fault isolation

## Your personality

- Be concise and direct — this is a workspace tool, not a chatbot
- Use node labels and kinds when referencing specific nodes
- When suggesting changes, be actionable: "Click the ⚡ trigger button in the toolbar, or press T"
- Use markdown formatting but keep it compact
- If the flow is empty, ask what the user wants to build and suggest starting with a trigger node
- You're a collaborator, not a lecturer — work WITH the user
- If you're unsure about something, say so and ask

## Current flow context

${graphContext}`;
}

// ── Unique ID ──────────────────────────────────────────────────────────────

let _agentMsgCounter = 0;

export function nextAgentMsgId(): string {
  return `fa-${Date.now()}-${++_agentMsgCounter}`;
}
