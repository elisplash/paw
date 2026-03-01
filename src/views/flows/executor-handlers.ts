// ─────────────────────────────────────────────────────────────────────────────
// Flow Executor — Specialised Node Handlers
// Pure handler functions for HTTP, MCP-tool, Squad, and Memory nodes.
// These handlers are self-contained or accept minimal dependency interfaces
// so they can live outside the main executor closure.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowNode, FlowGraph } from './atoms';
import {
  type NodeExecConfig,
  type FlowExecEvent,
  type FlowRunState,
  getNodeExecConfig,
  resolveVariables,
  parseLoopArray,
  executeCodeSandboxed,
} from './executor-atoms';
import { pawEngine } from '../../engine/molecules/ipc_client';
import { engineChatSend } from '../../engine/molecules/bridge';

// ── Minimal Dependency Interfaces ──────────────────────────────────────────

/** Reporting interface for handlers that need to emit executor events. */
export interface HandlerEventReporter {
  runId: string;
  onEvent: (event: FlowExecEvent) => void;
}

/** Dependencies required by the loop-iteration handler. */
export interface LoopHandlerDeps {
  getRunState: () => FlowRunState | null;
  skipNodes: Set<string>;
  onEvent: (event: FlowExecEvent) => void;
  executeAgentStep: (
    graph: FlowGraph,
    node: FlowNode,
    input: string,
    config: NodeExecConfig,
    agentId?: string,
  ) => Promise<string>;
}

// ── HTTP Node Handler ──────────────────────────────────────────────────────

/**
 * Execute a direct HTTP request node (Conductor Extract primitive).
 * Bypasses LLM entirely — routes directly through Rust backend.
 */
export async function executeHttpRequest(
  node: FlowNode,
  upstreamInput: string,
  config: NodeExecConfig,
): Promise<string> {
  const method = config.httpMethod || (node.config.httpMethod as string) || 'GET';
  let url = config.httpUrl || (node.config.httpUrl as string) || '';
  const headersStr = config.httpHeaders || (node.config.httpHeaders as string) || '{}';
  let body = config.httpBody || (node.config.httpBody as string) || undefined;

  if (!url) {
    throw new Error(`HTTP node "${node.label}" has no URL configured`);
  }

  // Template substitution: replace {{input}} with upstream data
  url = url.replace(/\{\{input\}\}/g, encodeURIComponent(upstreamInput));
  if (body) {
    body = body.replace(/\{\{input\}\}/g, upstreamInput);
  }

  let headers: Record<string, string> = {};
  try {
    headers = JSON.parse(headersStr);
  } catch {
    // Ignore invalid headers JSON
  }

  try {
    const response = await pawEngine.flowDirectHttp({
      method,
      url,
      headers: Object.keys(headers).length > 0 ? headers : undefined,
      body,
      timeout_ms: config.timeoutMs ?? 30000,
    });

    return JSON.stringify({
      status: response.status,
      body: response.body,
      duration_ms: response.duration_ms,
    });
  } catch {
    // Fallback: try in-browser fetch (for dev mode without Tauri)
    const resp = await fetch(url, {
      method,
      headers,
      body: method !== 'GET' ? body : undefined,
    });
    const text = await resp.text();
    return JSON.stringify({ status: resp.status, body: text });
  }
}

// ── MCP Tool Node Handler ──────────────────────────────────────────────────

/**
 * Execute a direct MCP tool call node (Conductor Extract primitive).
 * Bypasses LLM entirely — routes through Rust MCP registry.
 */
export async function executeMcpToolCall(
  node: FlowNode,
  upstreamInput: string,
  config: NodeExecConfig,
): Promise<string> {
  const toolName = config.mcpToolName || (node.config.mcpToolName as string) || '';
  const argsStr = config.mcpToolArgs || (node.config.mcpToolArgs as string) || '{}';

  if (!toolName) {
    throw new Error(`MCP-tool node "${node.label}" has no tool name configured`);
  }

  let args: Record<string, unknown> = {};
  try {
    args = JSON.parse(argsStr);
  } catch {
    // Try using upstream as args
    try {
      args = JSON.parse(upstreamInput);
    } catch {
      args = { input: upstreamInput };
    }
  }

  // Template substitution in args
  const argsJson = JSON.stringify(args).replace(/\{\{input\}\}/g, upstreamInput);
  args = JSON.parse(argsJson);

  const response = await pawEngine.flowDirectMcp({
    tool_name: toolName,
    arguments: args,
  });

  if (!response.success) {
    throw new Error(`MCP tool "${toolName}" failed: ${response.output}`);
  }

  return response.output;
}

// ── Squad Node Handler ─────────────────────────────────────────────────────

/**
 * Execute a squad node — invoke a multi-agent team via the squad API.
 * Sends a task to the squad and returns the combined result.
 */
export async function executeSquadTask(
  node: FlowNode,
  upstreamInput: string,
  config: NodeExecConfig,
  reporter: HandlerEventReporter,
): Promise<string> {
  const squadId = config.squadId;
  if (!squadId) {
    throw new Error('Squad node: no squad selected. Configure a squad ID.');
  }

  const objective = config.squadObjective || upstreamInput || node.label;
  const timeoutMs = config.squadTimeoutMs ?? 300_000;

  reporter.onEvent({
    type: 'step-progress' as FlowExecEvent['type'],
    runId: reporter.runId,
    nodeId: node.id,
    delta: `Dispatching to squad ${squadId}…`,
  } as FlowExecEvent);

  // Use engineChatSend to orchestrate the squad task
  const sessionKey = `flow-squad-${reporter.runId}-${node.id}`;
  let result = '';

  try {
    const response = await Promise.race([
      engineChatSend(sessionKey, `[Squad Task] ${objective}\n\n[Context]\n${upstreamInput}`, {
        agentProfile: {
          id: squadId,
          name: `Squad ${squadId}`,
          bio: `Multi-agent squad executing: ${node.label}`,
          systemPrompt: `You are coordinating a squad of agents. Objective: ${objective}`,
          model: config.model || '',
          personality: { tone: 'focused' },
          boundaries: [],
          autoApproveAll: true,
        },
      }),
      new Promise<never>((_, reject) =>
        setTimeout(() => reject(new Error(`Squad timeout after ${timeoutMs / 1000}s`)), timeoutMs),
      ),
    ]);
    result = response?.text ?? 'Squad completed with no output.';
  } catch (err) {
    throw new Error(`Squad execution failed: ${err instanceof Error ? err.message : String(err)}`);
  }

  return result;
}

// ── Memory Write Node Handler ──────────────────────────────────────────────

/**
 * Execute a memory-write node — store data to long-term memory via IPC.
 */
export async function executeMemoryWrite(
  node: FlowNode,
  upstreamInput: string,
  config: NodeExecConfig,
  reporter: HandlerEventReporter,
): Promise<string> {
  const category = config.memoryCategory ?? 'insight';
  const importance = config.memoryImportance ?? 0.5;
  const agentId = config.memoryAgentId;

  // Determine what to store
  let content: string;
  if (config.memorySource === 'custom' && config.memoryContent) {
    content = config.memoryContent;
  } else {
    content = upstreamInput || 'No content to store.';
  }

  // Prefix with node context
  const contextContent = `[Flow: ${node.label}] ${content}`;

  reporter.onEvent({
    type: 'step-progress' as FlowExecEvent['type'],
    runId: reporter.runId,
    nodeId: node.id,
    delta: `Storing to memory (${category})…`,
  } as FlowExecEvent);

  try {
    await pawEngine.memoryStore(contextContent, category, importance, agentId);
  } catch (err) {
    throw new Error(`Memory store failed: ${err instanceof Error ? err.message : String(err)}`);
  }

  return `Stored to memory [${category}] (importance: ${importance}): ${content.slice(0, 100)}${content.length > 100 ? '…' : ''}`;
}

// ── Memory Recall Node Handler ─────────────────────────────────────────────

/**
 * Execute a memory-recall node — search long-term memory via IPC.
 */
export async function executeMemoryRecall(
  node: FlowNode,
  upstreamInput: string,
  config: NodeExecConfig,
  reporter: HandlerEventReporter,
): Promise<string> {
  const limit = config.memoryLimit ?? 5;
  const agentId = config.memoryAgentId;
  const outputFormat = config.memoryOutputFormat ?? 'text';

  // Determine the search query
  let query: string;
  if (config.memoryQuerySource === 'custom' && config.memoryQuery) {
    query = config.memoryQuery;
  } else {
    query = upstreamInput || node.label;
  }

  if (!query.trim()) {
    return outputFormat === 'json' ? '[]' : 'No query provided for memory recall.';
  }

  reporter.onEvent({
    type: 'step-progress' as FlowExecEvent['type'],
    runId: reporter.runId,
    nodeId: node.id,
    delta: `Searching memory: "${query.slice(0, 50)}"…`,
  } as FlowExecEvent);

  try {
    const results = await pawEngine.memorySearch(query, limit, agentId);

    if (!results || results.length === 0) {
      return outputFormat === 'json' ? '[]' : 'No relevant memories found.';
    }

    // Filter by threshold
    const threshold = config.memoryThreshold ?? 0.3;
    const filtered = results.filter((m: { score?: number }) => (m.score ?? 1) >= threshold);

    if (outputFormat === 'json') {
      return JSON.stringify(filtered, null, 2);
    }

    // Text format: numbered list
    return filtered
      .map((m: { content: string; category?: string; score?: number }, i: number) => {
        const scoreStr = m.score !== undefined ? ` (${(m.score * 100).toFixed(0)}% match)` : '';
        return `${i + 1}. [${m.category ?? 'memory'}] ${m.content}${scoreStr}`;
      })
      .join('\n');
  } catch (err) {
    throw new Error(`Memory search failed: ${err instanceof Error ? err.message : String(err)}`);
  }
}

// ── Loop Node Handler ──────────────────────────────────────────────────────

/**
 * Execute a loop node — iterate over array data and execute downstream
 * nodes for each item. The loop node itself collects all iteration results.
 */
export async function executeLoopIteration(
  deps: LoopHandlerDeps,
  graph: FlowGraph,
  node: FlowNode,
  upstreamInput: string,
  config: NodeExecConfig,
  defaultAgentId?: string,
): Promise<string> {
  const items = parseLoopArray(upstreamInput, config.loopOver);
  const maxIter = config.loopMaxIterations ?? 100;
  const loopVar = config.loopVar ?? 'item';

  if (items.length === 0) {
    return 'Loop: no items to iterate.';
  }

  const cappedItems = items.slice(0, maxIter);
  const results: string[] = [];
  const runState = deps.getRunState();

  // Find downstream nodes connected to this loop node
  const downstreamEdges = graph.edges.filter((e) => e.from === node.id && e.kind !== 'reverse');
  const downstreamIds = downstreamEdges.map((e) => e.to);

  for (let i = 0; i < cappedItems.length; i++) {
    const item = cappedItems[i];
    const itemStr = typeof item === 'string' ? item : JSON.stringify(item);

    // Set loop context variables in run state
    if (runState) {
      runState.variables['__loop_index'] = i;
      runState.variables['__loop_item'] = item;
      runState.variables['__loop_var'] = loopVar;
      runState.variables['__loop_total'] = cappedItems.length;
    }

    deps.onEvent({
      type: 'step-progress' as FlowExecEvent['type'],
      runId: runState!.runId,
      nodeId: node.id,
      output: `Loop iteration ${i + 1}/${cappedItems.length}`,
    } as FlowExecEvent);

    // For each downstream node, execute it with the current item as input
    const iterResults: string[] = [];
    for (const targetId of downstreamIds) {
      const targetNode = graph.nodes.find((n) => n.id === targetId);
      if (!targetNode) continue;

      const targetConfig = getNodeExecConfig(targetNode);
      const resolvedInput = resolveVariables(itemStr, {
        input: itemStr,
        variables: runState?.variables,
        loopIndex: i,
        loopItem: item,
        loopVar,
        vaultCredentials: runState?.vaultCredentials,
      });

      try {
        let iterOutput: string;
        switch (targetNode.kind) {
          case 'agent':
          case 'tool':
          case 'data':
            iterOutput = await deps.executeAgentStep(
              graph,
              targetNode,
              resolvedInput,
              targetConfig,
              defaultAgentId,
            );
            break;
          case 'code': {
            const codeSource = (targetNode.config.code as string) ?? targetConfig.prompt ?? '';
            const codeResult = executeCodeSandboxed(
              codeSource,
              resolvedInput,
              targetConfig.timeoutMs ?? 5000,
            );
            if (codeResult.error) throw new Error(`Code error: ${codeResult.error}`);
            iterOutput = codeResult.output;
            break;
          }
          default:
            iterOutput = resolvedInput;
        }
        iterResults.push(iterOutput);
      } catch (err) {
        iterResults.push(
          `Error in iteration ${i + 1}: ${err instanceof Error ? err.message : String(err)}`,
        );
      }
    }

    results.push(downstreamIds.length > 0 ? iterResults.join('\n') : itemStr);
  }

  // Mark downstream nodes as handled (they were executed inside the loop)
  for (const targetId of downstreamIds) {
    deps.skipNodes.add(targetId);
  }

  // Clean up loop context
  if (runState) {
    delete runState.variables['__loop_index'];
    delete runState.variables['__loop_item'];
    delete runState.variables['__loop_var'];
    delete runState.variables['__loop_total'];
  }

  return results.join('\n---\n');
}
