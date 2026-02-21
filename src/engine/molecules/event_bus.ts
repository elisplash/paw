// src/engine/molecules/event_bus.ts
// Registers the Tauri IPC agent event listener and routes incoming events
// to the correct handler: streaming chat bubbles, research view, or task sessions.
//
// Uses a callback registration pattern so the engine layer never imports
// from the view layer (chat_controller, views/research).  The view layer
// calls registerStreamHandlers() and registerResearchRouter() at startup.

import { onEngineAgent } from './bridge';
import { appState, type StreamState } from '../../state/index';

// ── Callback type definitions ────────────────────────────────────────────────

export interface StreamHandlers {
  onDelta: (text: string) => void;
  onToken: (usage: Record<string, unknown> | undefined) => void;
  onModel: (model: string) => void;
}

export interface ResearchRouter {
  isStreaming: () => boolean;
  getRunId: () => string | null;
  appendDelta: (text: string) => void;
  resolveStream: (text?: string) => void;
}

let _streamHandlers: StreamHandlers | null = null;
let _researchRouter: ResearchRouter | null = null;

/** Register stream handlers (call once from chat_controller at startup). */
export function registerStreamHandlers(h: StreamHandlers): void {
  _streamHandlers = h;
}

/** Register research router (call once from views/research at startup). */
export function registerResearchRouter(r: ResearchRouter): void {
  _researchRouter = r;
}

function handleAgentEvent(payload: unknown): void {
  try {
    const evt = payload as Record<string, unknown>;
    const stream = evt.stream as string | undefined;
    const data = evt.data as Record<string, unknown> | undefined;
    const runId = evt.runId as string | undefined;
    const evtSession = evt.sessionKey as string | undefined;

    if (stream !== 'assistant') {
      console.debug(
        `[event_bus] stream=${stream} session=${evtSession} runId=${String(runId).slice(0, 12)} isLoading=${appState.isLoading}`,
      );
    }

    // ── Route research sessions ──
    if (evtSession && evtSession.startsWith('paw-research-')) {
      if (!_researchRouter || !_researchRouter.isStreaming()) return;
      if (_researchRouter.getRunId() && runId && runId !== _researchRouter.getRunId()) return;
      if (stream === 'assistant' && data) {
        const delta = data.delta as string | undefined;
        if (delta) _researchRouter.appendDelta(delta);
      } else if (stream === 'lifecycle' && data) {
        if ((data.phase as string) === 'end') _researchRouter.resolveStream();
      } else if (stream === 'tool' && data) {
        const tool = (data.name ?? data.tool) as string | undefined;
        const phase = data.phase as string | undefined;
        if (phase === 'start' && tool) _researchRouter.appendDelta(`\n\n▶ ${tool}...`);
      } else if (stream === 'error' && data) {
        const error = (data.message ?? data.error ?? '') as string;
        if (error) _researchRouter.appendDelta(`\n\nError: ${error}`);
        _researchRouter.resolveStream();
      }
      return;
    }

    // ── Drop background task events unless the user is viewing that session ──
    if (
      evtSession &&
      evtSession.startsWith('eng-task-') &&
      evtSession !== appState.currentSessionKey
    )
      return;

    // ── Drop other paw-* internal sessions ──
    if (evtSession && evtSession.startsWith('paw-')) return;

    // ── Drop channel bridge sessions ──
    if (
      evtSession &&
      (evtSession.startsWith('eng-tg-') ||
        evtSession.startsWith('eng-discord-') ||
        evtSession.startsWith('eng-irc-') ||
        evtSession.startsWith('eng-slack-') ||
        evtSession.startsWith('eng-matrix-'))
    )
      return;

    // ── Guard: only process while streaming is active ──
    // Look up the stream for this event's session (or current session)
    const streamKey = evtSession ?? appState.currentSessionKey ?? '';
    const stream_s: StreamState | undefined = appState.activeStreams.get(streamKey);

    if (!stream_s && !appState.isLoading) return;
    if (stream_s?.runId && runId && runId !== stream_s.runId) return;
    if (evtSession && appState.currentSessionKey && evtSession !== appState.currentSessionKey)
      return;

    if (stream === 'assistant' && data) {
      const delta = data.delta as string | undefined;
      if (delta) _streamHandlers?.onDelta(delta);
    } else if (stream === 'lifecycle' && data) {
      const phase = data.phase as string | undefined;
      if (phase === 'start') {
        if (stream_s && !stream_s.runId && runId) stream_s.runId = runId;
        console.debug(`[event_bus] Agent run started: ${runId}`);
      } else if (phase === 'end') {
        console.debug(
          `[event_bus] Agent run ended: ${runId} chars=${stream_s?.content.length ?? 0}`,
        );
        const dAny = data as Record<string, unknown>;
        const dNested = dAny.response as Record<string, unknown> | undefined;

        // D-3.3: Deduplicate token recording — only fire once per run
        if (stream_s && !stream_s.tokenRecorded) {
          stream_s.tokenRecorded = true;
          // Prefer nested usage over top-level to avoid double-count
          const agentUsage = (dAny.usage ?? dNested?.usage ?? data) as
            | Record<string, unknown>
            | undefined;
          _streamHandlers?.onToken(agentUsage);
        }

        // Update model selector with API-confirmed model name
        const confirmedModel = dAny.model as string | undefined;
        if (confirmedModel) {
          const modelSel = document.getElementById('chat-model-select') as HTMLSelectElement | null;
          if (modelSel) {
            const exists = Array.from(modelSel.options).some((o) => o.value === confirmedModel);
            if (!exists) {
              const opt = document.createElement('option');
              opt.value = confirmedModel;
              opt.textContent = `✓ ${confirmedModel}`;
              modelSel.appendChild(opt);
            }
            if (modelSel.value === 'default' || modelSel.value === '')
              modelSel.value = confirmedModel;
          }
          _streamHandlers?.onModel(confirmedModel);
        }

        if (stream_s?.resolve) {
          if (stream_s.content) {
            stream_s.resolve(stream_s.content);
            stream_s.resolve = null;
          } else {
            console.debug('[event_bus] No content at lifecycle end — waiting 3s for chat.final...');
            const savedResolve = stream_s.resolve;
            setTimeout(() => {
              if (stream_s.resolve === savedResolve && stream_s.resolve) {
                console.warn('[event_bus] Grace period expired — resolving with empty content');
                stream_s.resolve(stream_s.content || '');
                stream_s.resolve = null;
              }
            }, 3000);
          }
        }
      }
    } else if (stream === 'tool' && data) {
      const tool = (data.name ?? data.tool) as string | undefined;
      const phase = data.phase as string | undefined;
      if (phase === 'start' && tool) {
        console.debug(`[event_bus] Tool: ${tool}`);
        if (stream_s?.el) _streamHandlers?.onDelta(`\n\n▶ ${tool}...`);
      }
    } else if (stream === 'error' && data) {
      const error = (data.message ?? data.error ?? '') as string;
      console.error(`[event_bus] Agent error: ${error}`);
      if (error && stream_s?.el) _streamHandlers?.onDelta(`\n\nError: ${error}`);
      if (stream_s?.resolve) {
        stream_s.resolve(stream_s.content);
        stream_s.resolve = null;
      }
    }
  } catch (e) {
    console.warn('[event_bus] Handler error:', e);
  }
}

// Register immediately — this module is imported once at startup.
onEngineAgent(handleAgentEvent);
