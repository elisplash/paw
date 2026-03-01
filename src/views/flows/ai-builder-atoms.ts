// ─────────────────────────────────────────────────────────────────────────────
// Flow AI Builder — Atoms (Pure Logic)
// Prompt templates and types for AI-driven flow graph generation.
// No DOM, no IPC — fully testable.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowGraph, FlowNodeKind } from './atoms';

// ── Types ──────────────────────────────────────────────────────────────────

/** Request to build a flow from natural language intent. */
export interface FlowBuildRequest {
  /** User's natural language description of the desired flow */
  intent: string;
  /** Available agent names/IDs for the agent-node dropdown */
  availableAgents?: Array<{ id: string; name: string }>;
  /** Available MCP tool names for mcp-tool nodes */
  availableTools?: string[];
  /** Whether to include error handling nodes */
  includeErrorHandling?: boolean;
  /** Maximum nodes to generate (safety cap) */
  maxNodes?: number;
}

/** AI builder response — a generated flow graph + explanation. */
export interface FlowBuildResult {
  /** The generated flow graph */
  graph: FlowGraph;
  /** Natural language explanation of what was built */
  explanation: string;
  /** Suggestions for improvements */
  suggestions?: string[];
}

/** Request to modify an existing flow via AI. */
export interface FlowModifyRequest {
  /** The current flow graph */
  graph: FlowGraph;
  /** User's modification instruction (e.g. "add error handling") */
  instruction: string;
  /** Available agents */
  availableAgents?: Array<{ id: string; name: string }>;
}

/** Request to explain a flow in natural language. */
export interface FlowExplainRequest {
  graph: FlowGraph;
  /** Detail level */
  detail?: 'brief' | 'full';
}

// ── Prompt Templates ───────────────────────────────────────────────────────

/** All valid node kinds for the AI builder prompt. */
const VALID_KINDS: FlowNodeKind[] = [
  'trigger',
  'agent',
  'tool',
  'condition',
  'data',
  'code',
  'output',
  'error',
  'group',
  'http',
  'mcp-tool',
  'loop',
  'squad',
  'memory',
  'memory-recall',
];

/**
 * Build the system prompt for the AI flow builder agent.
 */
export function buildFlowBuilderSystemPrompt(): string {
  return `You are a flow builder assistant for OpenPawz, an AI workflow automation platform.

When the user describes a workflow, you create a FlowGraph JSON object.

## Node Kinds
${VALID_KINDS.map((k) => `- \`${k}\``).join('\n')}

## Rules
1. Every flow MUST start with a \`trigger\` node
2. Every flow MUST end with an \`output\` node
3. Agent/tool/data nodes do AI work — use them for reasoning, drafting, analysis
4. Use \`http\` for direct API calls, \`mcp-tool\` for MCP integrations (no LLM needed)
5. Use \`code\` for data transformation via JavaScript
6. Use \`condition\` for branching logic
7. Use \`loop\` for iterating over arrays
8. Use \`squad\` to invoke multi-agent teams for complex tasks
9. Use \`memory\` to write information to long-term memory
10. Use \`memory-recall\` to search/retrieve from long-term memory
11. Use \`error\` nodes for error handling paths
12. Use \`group\` to embed sub-flows
13. Nodes should have clear, descriptive labels
14. Add appropriate edge labels for condition branches ("true"/"false")
15. Position nodes in a readable left-to-right layout (x increases by ~240, y by ~100 for branches)

## Output Format
Respond with ONLY a JSON object:
{
  "graph": { FlowGraph JSON },
  "explanation": "Brief description of what the flow does",
  "suggestions": ["Optional improvement suggestions"]
}

The FlowGraph must include: id, name, description, nodes[], edges[], createdAt, updatedAt.
Each node needs: id, kind, label, x, y, width, height, status: "idle", config: {}, inputs: ["in"], outputs: ["out"].
Each edge needs: id, kind: "forward", from, fromPort: "out", to, toPort: "in", active: false.`;
}

/**
 * Build the user prompt for generating a flow from intent.
 */
export function buildFlowFromIntentPrompt(request: FlowBuildRequest): string {
  const parts: string[] = [];

  parts.push(`Create a flow for: "${request.intent}"`);

  if (request.availableAgents?.length) {
    parts.push(
      `\nAvailable agents: ${request.availableAgents.map((a) => `${a.name} (${a.id})`).join(', ')}`,
    );
  }

  if (request.availableTools?.length) {
    parts.push(`\nAvailable MCP tools: ${request.availableTools.join(', ')}`);
  }

  if (request.includeErrorHandling) {
    parts.push('\nInclude error handling nodes for robustness.');
  }

  if (request.maxNodes) {
    parts.push(`\nUse at most ${request.maxNodes} nodes.`);
  }

  return parts.join('');
}

/**
 * Build the prompt for modifying an existing flow.
 */
export function buildFlowModifyPrompt(request: FlowModifyRequest): string {
  const graphJson = JSON.stringify(request.graph, null, 2);
  return `Here is the current flow:\n\`\`\`json\n${graphJson}\n\`\`\`\n\nModification: "${request.instruction}"\n\nReturn the COMPLETE modified FlowGraph JSON with the same format as before.`;
}

/**
 * Build the prompt for explaining a flow.
 */
export function buildFlowExplainPrompt(request: FlowExplainRequest): string {
  const graphJson = JSON.stringify(request.graph, null, 2);
  const detail = request.detail ?? 'full';
  return `Explain this flow in ${detail === 'brief' ? 'one paragraph' : 'detail, step by step'}:\n\`\`\`json\n${graphJson}\n\`\`\``;
}

/**
 * Parse the AI builder response into a FlowBuildResult.
 * Handles both clean JSON and Markdown-wrapped JSON responses.
 */
export function parseFlowBuildResponse(response: string): FlowBuildResult | null {
  // Try to extract JSON from markdown code blocks
  let jsonStr = response.trim();
  const codeBlockMatch = jsonStr.match(/```(?:json)?\s*\n?([\s\S]*?)\n?```/);
  if (codeBlockMatch) {
    jsonStr = codeBlockMatch[1].trim();
  }

  try {
    const parsed = JSON.parse(jsonStr);

    // Validate minimum structure
    if (!parsed.graph || !Array.isArray(parsed.graph.nodes) || !Array.isArray(parsed.graph.edges)) {
      return null;
    }

    // Ensure required fields have defaults
    const graph: FlowGraph = {
      id: parsed.graph.id || `flow_${Date.now().toString(36)}`,
      name: parsed.graph.name || 'AI Generated Flow',
      description: parsed.graph.description || '',
      nodes: parsed.graph.nodes,
      edges: parsed.graph.edges,
      createdAt: parsed.graph.createdAt || new Date().toISOString(),
      updatedAt: parsed.graph.updatedAt || new Date().toISOString(),
    };

    return {
      graph,
      explanation: parsed.explanation || 'Flow generated by AI.',
      suggestions: parsed.suggestions,
    };
  } catch {
    return null;
  }
}

/**
 * Validate a generated flow graph for basic structural correctness.
 */
export function validateGeneratedFlow(graph: FlowGraph): string[] {
  const errors: string[] = [];

  if (graph.nodes.length === 0) {
    errors.push('Flow has no nodes.');
    return errors;
  }

  // Check for trigger node
  const hasTrigger = graph.nodes.some((n) => n.kind === 'trigger');
  if (!hasTrigger) {
    errors.push('Flow must have at least one trigger node.');
  }

  // Check for output node
  const hasOutput = graph.nodes.some((n) => n.kind === 'output');
  if (!hasOutput) {
    errors.push('Flow should have at least one output node.');
  }

  // Check all edge references are valid
  const nodeIds = new Set(graph.nodes.map((n) => n.id));
  for (const edge of graph.edges) {
    if (!nodeIds.has(edge.from)) {
      errors.push(`Edge "${edge.id}" references unknown source node "${edge.from}".`);
    }
    if (!nodeIds.has(edge.to)) {
      errors.push(`Edge "${edge.id}" references unknown target node "${edge.to}".`);
    }
  }

  // Check for duplicate node IDs
  const seen = new Set<string>();
  for (const node of graph.nodes) {
    if (seen.has(node.id)) {
      errors.push(`Duplicate node ID: "${node.id}".`);
    }
    seen.add(node.id);
  }

  // Validate node kinds
  const validKinds = new Set<string>(VALID_KINDS);
  for (const node of graph.nodes) {
    if (!validKinds.has(node.kind)) {
      errors.push(`Node "${node.label}" has invalid kind: "${node.kind}".`);
    }
  }

  return errors;
}
