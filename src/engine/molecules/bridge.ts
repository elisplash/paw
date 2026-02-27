// src/engine/molecules/bridge.ts
// Engine bridge — translates Tauri engine events into agent events for the frontend.
// Extracted from engine-bridge.ts. Import this instead of ../../engine-bridge.

import { pawEngine } from './ipc_client';
import type { EngineEvent, EngineChatRequest } from '../atoms/types';
import { getAgentAllowedTools, ALL_TOOLS } from '../../features/agent-policies';
import { getIntegrationHint } from './auto-discover-bridge';
import { appState } from '../../state';

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
    // Kinetic pulse: flash UI indicators on engine events
    kineticPulse(event.kind);

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

  // ── Queue-ready listener: re-send queued messages after yield ─────────
  // When the backend completes a yielded run and finds a queued message,
  // it emits "engine-queue-ready".  We re-send via the normal chat pipeline
  // so system prompt, context, and tool lists are properly reconstructed.
  const { listen } = await import('@tauri-apps/api/event');
  await listen<{ sessionId: string; message: string; model?: string }>(
    'engine-queue-ready',
    async (event) => {
      const { sessionId, message, model } = event.payload;
      console.debug(`[bridge] Queue-ready: re-sending message for session ${sessionId}`);

      // Guard: if the user has already navigated to a different session,
      // skip firing the frontend send (the backend already stored nothing;
      // the message will simply be lost from the queue, which is correct —
      // the user abandoned the context).
      if (appState.currentSessionKey && appState.currentSessionKey !== sessionId) {
        console.debug(
          `[bridge] Queue-ready: session ${sessionId} != current ${appState.currentSessionKey} — skipping`,
        );
        return;
      }

      try {
        // The Rust queue processor does NOT store the message — it will be
        // stored when engineChatSend triggers the normal engine_chat_send flow.
        await engineChatSend(sessionId, message, { model: model || undefined });
      } catch (e) {
        console.error('[bridge] Queue re-send failed:', e);
      }
    },
  );
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
    thinkingLevel?: string;
    agentProfile?: {
      id?: string;
      name?: string;
      bio?: string;
      systemPrompt?: string;
      model?: string;
      personality?: { tone?: string; initiative?: string; detail?: string };
      boundaries?: string[];
      autoApproveAll?: boolean;
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

  // Inject integration auto-discovery context
  const integrationHint = await getIntegrationHint(content);
  if (integrationHint) {
    agentSystemPrompt = agentSystemPrompt
      ? `${agentSystemPrompt}\n\n${integrationHint}`
      : integrationHint;
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
    thinking_level: opts.thinkingLevel,
    auto_approve_all: !!opts.agentProfile?.autoApproveAll,
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
        agentId: event.agent_id,
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
        agentId: event.agent_id,
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
        agentId: event.agent_id,
      };

    case 'error':
      return {
        stream: 'error',
        data: { message: event.message },
        runId: event.run_id,
        sessionKey: event.session_id,
      };

    case 'thinking_delta':
      return {
        stream: 'thinking',
        data: { delta: event.text },
        runId: event.run_id,
        sessionKey: event.session_id,
        agentId: event.agent_id,
      };

    case 'tool_auto_approved':
      return {
        stream: 'tool',
        data: {
          phase: 'auto_approved',
          name: event.tool_name,
          tool: event.tool_name,
          tool_call_id: event.tool_call_id,
        },
        runId: event.run_id,
        sessionKey: event.session_id,
      };

    default:
      return null;
  }
}

/**
 * Kinetic Pulse — CSS-driven visual heartbeat on engine events.
 * Adds a flash class to the app shell that auto-removes after animation.
 */
function kineticPulse(kind: string): void {
  const app = document.querySelector('.app');
  if (!app) return;

  const classMap: Record<string, string> = {
    delta: 'kinetic-pulse-stream',
    tool_request: 'kinetic-pulse-tool',
    tool_result: 'kinetic-pulse-tool-end',
    error: 'kinetic-pulse-error',
    complete: 'kinetic-pulse-complete',
  };

  const cls = classMap[kind];
  if (!cls) return;

  app.classList.add(cls);
  setTimeout(() => app.classList.remove(cls), 300);
}
