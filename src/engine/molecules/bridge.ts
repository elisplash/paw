// src/engine/molecules/bridge.ts
// Engine bridge — translates Tauri engine events into agent events for the frontend.
// Extracted from engine-bridge.ts. Import this instead of ../../engine-bridge.

import { pawEngine } from './ipc_client';
import type { EngineEvent, EngineChatRequest } from '../atoms/types';
import { getAgentAllowedTools, ALL_TOOLS } from '../../features/agent-policies';

type AgentEventHandler = (payload: unknown) => void;
type ToolApprovalHandler = (event: EngineEvent) => void;

let _engineListening = false;
const _agentHandlers: AgentEventHandler[] = [];
const _toolApprovalHandlers: ToolApprovalHandler[] = [];

/** Whether the engine mode is active. */
export function isEngineMode(): boolean {
  return localStorage.getItem('paw-runtime-mode') === 'engine';
}

/** Set the runtime mode. */
export function setEngineMode(enabled: boolean): void {
  localStorage.setItem('paw-runtime-mode', enabled ? 'engine' : 'disabled');
}

/**
 * Register a handler that receives agent-style events.
 */
export function onEngineAgent(handler: AgentEventHandler): void {
  _agentHandlers.push(handler);
}

/**
 * Register a handler for engine tool approval requests (HIL).
 */
export function onEngineToolApproval(handler: ToolApprovalHandler): void {
  _toolApprovalHandlers.push(handler);
}

/**
 * Resolve a tool approval from the frontend.
 */
export function resolveEngineToolApproval(toolCallId: string, approved: boolean): void {
  pawEngine.approveTool(toolCallId, approved).catch((e) => {
    console.error('[bridge] Failed to resolve tool approval:', e);
  });
}

/**
 * Start listening for engine events and forward them as agent events.
 * Call this once at startup.
 */
export async function startEngineBridge(): Promise<void> {
  if (_engineListening) return;
  _engineListening = true;

  await pawEngine.startListening();

  pawEngine.on('*', (event: EngineEvent) => {
    if (event.kind === 'tool_request') {
      for (const h of _toolApprovalHandlers) {
        try {
          h(event);
        } catch (e) {
          console.error('[bridge] approval handler error:', e);
        }
      }
    }
    const agentEvt = translateEngineEvent(event);
    if (agentEvt) {
      for (const h of _agentHandlers) {
        try {
          h(agentEvt);
        } catch (e) {
          console.error('[bridge] handler error:', e);
        }
      }
    }
  });
}

/**
 * Send a chat message via the Paw Engine.
 */
export async function engineChatSend(
  sessionKey: string,
  content: string,
  opts: {
    model?: string;
    temperature?: number;
    agentProfile?: {
      id?: string;
      name?: string;
      bio?: string;
      systemPrompt?: string;
      model?: string;
      personality?: { tone?: string; initiative?: string; detail?: string };
      boundaries?: string[];
    };
    attachments?: Array<{
      type?: string;
      mimeType: string;
      content: string;
      name?: string;
      fileName?: string;
    }>;
  } = {},
): Promise<{
  runId: string;
  sessionKey: string;
  status: string;
  usage?: Record<string, unknown>;
  text?: string;
  response?: unknown;
}> {
  const rawModel = opts.model ?? opts.agentProfile?.model;
  const resolvedModel =
    rawModel && rawModel !== 'default' && rawModel !== 'Default' ? rawModel : undefined;

  let agentSystemPrompt: string | undefined;
  if (opts.agentProfile) {
    const profile = opts.agentProfile;
    const parts: string[] = [];
    if (profile.name) parts.push(`You are ${profile.name}.`);
    if (profile.bio) parts.push(profile.bio);
    if (profile.personality) {
      const p = profile.personality;
      const personalityDesc: string[] = [];
      if (p.tone) personalityDesc.push(`your tone is ${p.tone}`);
      if (p.initiative) personalityDesc.push(`you are ${p.initiative} in your initiative`);
      if (p.detail) personalityDesc.push(`you are ${p.detail} in your responses`);
      if (personalityDesc.length > 0) {
        parts.push(`Your personality is defined as follows: ${personalityDesc.join(', ')}.`);
      }
    }
    if (profile.boundaries && profile.boundaries.length > 0) {
      parts.push(
        `You must strictly follow these rules:\n${profile.boundaries.map((b) => `- ${b}`).join('\n')}`,
      );
    }
    if (profile.systemPrompt) parts.push(profile.systemPrompt);
    if (parts.length > 0) agentSystemPrompt = parts.join(' ');
  }

  const agentId = opts.agentProfile?.id ?? 'default';
  const allowedTools = getAgentAllowedTools(agentId, [...ALL_TOOLS]);
  const toolFilter = allowedTools.length < ALL_TOOLS.length ? allowedTools : undefined;

  const request: EngineChatRequest = {
    session_id: sessionKey === 'default' || !sessionKey ? undefined : sessionKey,
    message: content,
    model: resolvedModel,
    system_prompt: agentSystemPrompt,
    temperature: opts.temperature,
    tools_enabled: true,
    tool_filter: toolFilter,
    agent_id: agentId !== 'default' ? agentId : undefined,
    attachments: opts.attachments?.map((a) => ({
      mimeType: a.mimeType,
      content: a.content,
      name: a.name || a.fileName,
    })),
  };

  const result = await pawEngine.chatSend(request);
  return { runId: result.run_id, sessionKey: result.session_id, status: 'started' };
}

/**
 * Translate a Rust EngineEvent into the agent event shape expected by event_bus.ts.
 */
function translateEngineEvent(event: EngineEvent): Record<string, unknown> | null {
  switch (event.kind) {
    case 'delta':
      return {
        stream: 'assistant',
        data: { delta: event.text },
        runId: event.run_id,
        sessionKey: event.session_id,
      };

    case 'tool_request':
      return {
        stream: 'tool',
        data: {
          phase: 'start',
          name: event.tool_call?.function?.name ?? 'tool',
          tool: event.tool_call?.function?.name,
        },
        runId: event.run_id,
        sessionKey: event.session_id,
      };

    case 'tool_result':
      return {
        stream: 'tool',
        data: {
          phase: 'end',
          tool_call_id: event.tool_call_id,
          output: event.output,
          success: event.success,
        },
        runId: event.run_id,
        sessionKey: event.session_id,
      };

    case 'complete':
      if (event.tool_calls_count && event.tool_calls_count > 0) return null;
      return {
        stream: 'lifecycle',
        data: {
          phase: 'end',
          usage: event.usage
            ? {
                input_tokens: event.usage.input_tokens,
                output_tokens: event.usage.output_tokens,
                total_tokens: event.usage.total_tokens,
              }
            : undefined,
          model: event.model,
        },
        runId: event.run_id,
        sessionKey: event.session_id,
      };

    case 'error':
      return {
        stream: 'error',
        data: { message: event.message },
        runId: event.run_id,
        sessionKey: event.session_id,
      };

    default:
      return null;
  }
}
