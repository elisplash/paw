// src/engine/organisms/chat_controller.ts
// Thin orchestrator for the main chat view.
// Imports atoms + molecules and wires them to the existing DOM.
// All rendering, input, metering, and TTS logic lives in molecules.

import { pawEngine } from '../../engine';
import { engineChatSend } from '../molecules/bridge';
import {
  appState,
  agentSessionMap,
  persistAgentSessionMap,
  MODEL_COST_PER_TOKEN,
  createStreamState,
  sweepStaleStreams,
  type StreamState,
  type MessageWithAttachments,
} from '../../state/index';
// helpers & toast moved to chat_listeners molecule
import * as AgentsModule from '../../views/agents';
import * as SettingsModule from '../../views/settings-main';
import {
  addActiveJob,
  clearActiveJobs,
} from '../../components/chat-mission-panel';
import {
  interceptSlashCommand,
  getSessionOverrides as getSlashOverrides,
  isSlashCommand,
  type CommandContext,
} from '../../features/slash-commands';
import { parseCredentialSignal, handleCredentialRequired } from '../molecules/credential_bridge';
import type { Agent, ToolCall, Message } from '../../types';

// ── Molecule imports ─────────────────────────────────────────────────────
import {
  generateSessionLabel,
  extractContent,
  findLastIndex,
} from '../atoms/chat';
import {
  renderMessages as rendererRenderMessages,
  showStreamingMessage as rendererShowStreaming,
  appendStreamingDelta as rendererAppendDelta,
  appendThinkingDelta as rendererAppendThinking,
  scrollToBottom as rendererScrollToBottom,
  type RenderOpts,
} from '../molecules/chat_renderer';
import { createTokenMeter, type TokenMeterController, type TokenMeterState } from '../molecules/token_meter';
import { speakMessage, autoSpeakIfEnabled, type TtsState } from '../molecules/tts';
import { createSessionManager, type SessionManager } from '../molecules/chat_sessions';
import {
  renderAttachmentPreview,
  clearPendingAttachments,
  encodeFileAttachments,
} from '../molecules/chat_attachments';
import { initChatListeners as _initChatListeners } from '../molecules/chat_listeners';

// ── Re-exports from extracted molecules (backward compat) ────────────────
export { renderAttachmentPreview, clearPendingAttachments };
export { fileToBase64, extractContent } from '../atoms/chat';

// ── DOM shorthand ────────────────────────────────────────────────────────
const $ = (id: string) => document.getElementById(id);

// ── Scroll helper (RAF-debounced) ────────────────────────────────────────
const _scrollRaf = { value: false };

export function scrollToBottom(): void {
  const chatMessages = $('chat-messages');
  if (!chatMessages) return;
  rendererScrollToBottom(chatMessages, _scrollRaf);
}

// ── TTS state (scoped to main chat view) ─────────────────────────────────
const _ttsState: TtsState = {
  ttsAudio: null,
  ttsActiveBtn: null,
};

// Sync TTS state with appState for backward compat
function syncTtsToAppState(): void {
  appState.ttsAudio = _ttsState.ttsAudio;
  appState.ttsActiveBtn = _ttsState.ttsActiveBtn;
}

// ── Token meter (lazily initialized) ─────────────────────────────────────
let _tokenMeter: TokenMeterController | null = null;

function getTokenMeter(): TokenMeterController {
  if (!_tokenMeter) {
    _tokenMeter = createTokenMeter({
      meterId: 'token-meter',
      fillId: 'token-meter-fill',
      labelId: 'token-meter-label',
      breakdownPanelId: 'context-breakdown-panel',
      compactionWarningId: 'compaction-warning',
      compactionWarningTextId: 'compaction-warning-text',
      budgetAlertId: 'session-budget-alert',
      budgetAlertTextId: 'session-budget-alert-text',
    });
  }
  return _tokenMeter;
}

/** Build a TokenMeterState snapshot from appState. */
function meterSnapshot(): TokenMeterState {
  return {
    sessionTokensUsed: appState.sessionTokensUsed,
    sessionInputTokens: appState.sessionInputTokens,
    sessionOutputTokens: appState.sessionOutputTokens,
    sessionCost: appState.sessionCost,
    modelContextLimit: appState.modelContextLimit,
    compactionDismissed: appState.compactionDismissed,
    lastRecordedTotal: appState.lastRecordedTotal,
    activeModelKey: appState.activeModelKey,
    sessionToolResultTokens: appState.sessionToolResultTokens,
    sessionToolCallCount: appState.sessionToolCallCount,
    messageCount: appState.messages.length,
    messages: appState.messages,
  };
}

/** Write token meter state changes back to appState. */
function syncMeterToAppState(state: TokenMeterState): void {
  appState.sessionTokensUsed = state.sessionTokensUsed;
  appState.sessionInputTokens = state.sessionInputTokens;
  appState.sessionOutputTokens = state.sessionOutputTokens;
  appState.sessionCost = state.sessionCost;
  appState.modelContextLimit = state.modelContextLimit;
  appState.compactionDismissed = state.compactionDismissed;
  appState.lastRecordedTotal = state.lastRecordedTotal;
  appState.activeModelKey = state.activeModelKey;
  appState.sessionToolResultTokens = state.sessionToolResultTokens;
  appState.sessionToolCallCount = state.sessionToolCallCount;
}

// ── Stream teardown ──────────────────────────────────────────────────────

function teardownStream(sessionKey: string, reason: string): void {
  const stream = appState.activeStreams.get(sessionKey);
  if (!stream) return;
  console.debug(
    `[chat] Tearing down stream for ${sessionKey.slice(0, 12) || '(empty)'}: ${reason}`,
  );
  pawEngine.chatAbort(sessionKey).catch(() => {});
  if (stream.resolve) {
    stream.resolve(stream.content || `(${reason})`);
    stream.resolve = null;
  }
  if (stream.timeout) {
    clearTimeout(stream.timeout);
    stream.timeout = null;
  }
  appState.activeStreams.delete(sessionKey);
  // Clean up streaming UI
  document.getElementById('streaming-message')?.remove();
  clearActiveJobs();
  const abortBtn = document.getElementById('chat-abort-btn');
  if (abortBtn) abortBtn.style.display = 'none';
}

// ── Render opts builder ──────────────────────────────────────────────────

function buildRenderOpts(): RenderOpts {
  const agent = AgentsModule.getCurrentAgent();
  return {
    agentName: agent?.name ?? 'AGENT',
    agentAvatar: agent?.avatar,
    onRetry: (content: string) => retryMessage(content),
    onSpeak: (text: string, btn: HTMLButtonElement) => {
      speakMessage(text, btn, _ttsState);
      syncTtsToAppState();
    },
    isStreaming: appState.activeStreams.has(appState.currentSessionKey ?? ''),
  };
}

// ── Session manager (delegates to molecule) ──────────────────────────────

let _sessions: SessionManager | null = null;

function getSessionManager(): SessionManager {
  if (!_sessions) {
    _sessions = createSessionManager({
      teardownStream,
      resetTokenMeter,
      renderMessages,
    });
  }
  return _sessions;
}

export function loadSessions(opts?: { skipHistory?: boolean }): Promise<void> {
  return getSessionManager().loadSessions(opts);
}
export function renderSessionSelect(): void {
  getSessionManager().renderSessionSelect();
}
export function populateAgentSelect(): void {
  getSessionManager().populateAgentSelect();
}
export function switchToAgent(agentId: string): Promise<void> {
  return getSessionManager().switchToAgent(agentId);
}
export function loadChatHistory(sessionKey: string): Promise<void> {
  return getSessionManager().loadChatHistory(sessionKey);
}

// ── Token metering (delegates to molecule) ───────────────────────────────

export function resetTokenMeter(): void {
  const state = meterSnapshot();
  getTokenMeter().reset(state);
  syncMeterToAppState(state);
}

export function updateTokenMeter(): void {
  getTokenMeter().update(meterSnapshot());
}

export function recordTokenUsage(usage: Record<string, unknown> | undefined): void {
  const state = meterSnapshot();
  getTokenMeter().recordUsage(usage, state, SettingsModule.getBudgetLimit);
  syncMeterToAppState(state);
}

export function updateContextLimitFromModel(modelName: string): void {
  const state = meterSnapshot();
  getTokenMeter().updateContextLimitFromModel(modelName, state);
  syncMeterToAppState(state);
}

export function updateContextBreakdownPopover(): void {
  getTokenMeter().updateBreakdownPopover(meterSnapshot());
}

// ── Streaming pipeline (delegates to renderer molecule) ──────────────────

export function showStreamingMessage(): void {
  const chatEmpty = $('chat-empty');
  const chatMessages = $('chat-messages');
  if (chatEmpty) chatEmpty.style.display = 'none';
  if (!chatMessages) return;

  const agent = AgentsModule.getCurrentAgent();
  const contentEl = rendererShowStreaming(chatMessages, agent?.name ?? 'AGENT');

  // Create session-keyed stream state
  const key = appState.currentSessionKey ?? '';
  sweepStaleStreams();
  const ss = createStreamState(agent?.id);
  ss.el = contentEl;
  appState.activeStreams.set(key, ss);

  const abortBtn = $('chat-abort-btn');
  if (abortBtn) abortBtn.style.display = '';
  scrollToBottom();

  const modelName =
    ($('chat-model-select') as HTMLSelectElement | null)?.selectedOptions?.[0]?.text ?? 'model';
  addActiveJob(`Streaming · ${modelName}`);
}

export function appendStreamingDelta(text: string): void {
  const key = appState.currentSessionKey ?? '';
  const ss = appState.activeStreams.get(key);
  if (!ss) return;
  ss.content += text;
  if (ss.el) {
    rendererAppendDelta(ss.el, ss.content);
    scrollToBottom();
  }
}

export function appendThinkingDelta(text: string): void {
  const key = appState.currentSessionKey ?? '';
  const ss = appState.activeStreams.get(key);
  if (!ss) return;
  ss.thinkingContent += text;

  const streamMsg = document.getElementById('streaming-message');
  if (!streamMsg) return;

  rendererAppendThinking(streamMsg, ss.thinkingContent);
  scrollToBottom();
}

export function finalizeStreaming(
  finalContent: string,
  toolCalls?: ToolCall[],
  streamSessionKey?: string,
): void {
  $('streaming-message')?.remove();
  clearActiveJobs();

  const key = streamSessionKey ?? appState.currentSessionKey ?? '';
  const ss = appState.activeStreams.get(key);
  const savedRunId = ss?.runId ?? null;
  const streamingAgent = ss?.agentId ?? null;
  const thinkingContent = ss?.thinkingContent || undefined;
  appState.activeStreams.delete(key);

  const abortBtn = $('chat-abort-btn');
  if (abortBtn) abortBtn.style.display = 'none';

  const currentAgent = AgentsModule.getCurrentAgent();
  if (streamingAgent && currentAgent && streamingAgent !== currentAgent.id) {
    console.debug(
      `[chat] Streaming agent (${streamingAgent}) differs from current (${currentAgent.id}) — skipping UI render`,
    );
    return;
  }

  if (finalContent) {
    addMessage({
      role: 'assistant',
      content: finalContent,
      timestamp: new Date(),
      toolCalls,
      thinkingContent,
    });
    autoSpeakIfEnabled(finalContent, _ttsState).then(() => syncTtsToAppState());

    // Fallback token estimation
    if (
      appState.sessionTokensUsed === 0 ||
      appState.lastRecordedTotal === appState.sessionTokensUsed
    ) {
      const userMsg = appState.messages.filter((m) => m.role === 'user').pop();
      const userChars = userMsg?.content?.length ?? 0;
      const assistantChars = finalContent.length;
      const estInput = Math.ceil(userChars / 4);
      const estOutput = Math.ceil(assistantChars / 4);
      appState.sessionInputTokens += estInput;
      appState.sessionOutputTokens += estOutput;
      appState.sessionTokensUsed += estInput + estOutput;
      const rate = MODEL_COST_PER_TOKEN[appState.activeModelKey] ?? MODEL_COST_PER_TOKEN['default'];
      appState.sessionCost += estInput * rate.input + estOutput * rate.output;
      console.debug(`[token] Fallback estimate: ~${estInput + estOutput} tokens`);
      updateTokenMeter();
    }
  } else {
    console.warn(
      `[chat] finalizeStreaming: empty content (runId=${savedRunId?.slice(0, 12) ?? 'null'}). Fetching history fallback...`,
    );
    const sk = appState.currentSessionKey;
    if (sk) {
      pawEngine
        .chatHistory(sk, 10)
        .then((stored) => {
          for (let i = stored.length - 1; i >= 0; i--) {
            if (stored[i].role === 'assistant' && stored[i].content) {
              addMessage({ role: 'assistant', content: stored[i].content, timestamp: new Date() });
              return;
            }
          }
          addMessage({
            role: 'assistant',
            content: '*(No response received)*',
            timestamp: new Date(),
          });
        })
        .catch(() => {
          addMessage({
            role: 'assistant',
            content: '*(No response received)*',
            timestamp: new Date(),
          });
        });
    } else {
      addMessage({ role: 'assistant', content: '*(No response received)*', timestamp: new Date() });
    }
  }
}

// ── Message rendering (delegates to renderer molecule) ───────────────────

export function addMessage(message: MessageWithAttachments): void {
  appState.messages.push(message);
  renderMessages();

  // Credential bridge: detect [CREDENTIAL_REQUIRED] signals
  if (message.role === 'assistant' && message.content) {
    const signal = parseCredentialSignal(message.content);
    if (signal) {
      handleCredentialRequired(signal).catch((e) =>
        console.warn('[chat] Credential bridge error:', e),
      );
    }
  }
}

function retryMessage(content: string): void {
  const currentKey = appState.currentSessionKey ?? '';
  if (appState.activeStreams.has(currentKey) || !content) return;
  const lastUserIdx = findLastIndex(appState.messages, (m) => m.role === 'user');
  if (lastUserIdx >= 0) appState.messages.splice(lastUserIdx);
  renderMessages();
  const chatInput = document.getElementById('chat-input') as HTMLTextAreaElement | null;
  if (chatInput) {
    chatInput.value = content;
    chatInput.style.height = 'auto';
  }
  sendMessage();
}

export function renderMessages(): void {
  const chatMessages = $('chat-messages');
  const chatEmpty = $('chat-empty');
  if (!chatMessages) return;

  rendererRenderMessages(chatMessages, appState.messages, buildRenderOpts(), chatEmpty);
  scrollToBottom();
}

// ── Send message ──────────────────────────────────────────────────────────

function buildSlashCommandContext(chatModelSelect: HTMLSelectElement | null): CommandContext {
  return {
    sessionKey: appState.currentSessionKey,
    addSystemMessage: (text: string) =>
      addMessage({ role: 'assistant', content: text, timestamp: new Date() }),
    clearChatUI: () => {
      const el = document.getElementById('chat-messages');
      if (el) el.innerHTML = '';
      appState.messages = [];
    },
    newSession: async (label?: string) => {
      appState.currentSessionKey = null;
      if (label) {
        const newId = `session_${Date.now()}`;
        const result = await pawEngine.chatSend({ session_id: newId, message: '', model: '' });
        if (result.session_id) {
          appState.currentSessionKey = result.session_id;
          await pawEngine.sessionRename(appState.currentSessionKey!, label);
        }
      }
    },
    reloadSessions: () => loadSessions({ skipHistory: true }),
    getCurrentModel: () => chatModelSelect?.value || 'default',
  };
}

function handleSendResult(
  result: {
    sessionKey?: string;
    session_id?: string;
    runId?: string;
    text?: string;
    response?: unknown;
    usage?: unknown;
  },
  ss: StreamState,
  streamKey: string,
): void {
  if (result.runId) ss.runId = result.runId;
  if (result.sessionKey) {
    appState.currentSessionKey = result.sessionKey;
    if (result.sessionKey !== streamKey) {
      appState.activeStreams.delete(streamKey);
      appState.activeStreams.set(result.sessionKey, ss);
    }
    const curAgent = AgentsModule.getCurrentAgent();
    if (curAgent) {
      agentSessionMap.set(curAgent.id, result.sessionKey);
      persistAgentSessionMap();
    }

    const isNewSession = result.sessionKey !== streamKey || streamKey === 'default' || !streamKey;
    const existingSession = appState.sessions.find((s) => s.key === result.sessionKey);
    if (isNewSession || !existingSession?.label) {
      const chatInput = document.getElementById('chat-input') as HTMLTextAreaElement | null;
      const msgContent =
        chatInput?.value || appState.messages[appState.messages.length - 1]?.content || '';
      const autoLabel = generateSessionLabel(msgContent);
      pawEngine
        .sessionRename(result.sessionKey, autoLabel)
        .then(() => {
          const s = appState.sessions.find((s2) => s2.key === result.sessionKey);
          if (s) {
            s.label = autoLabel;
            s.displayName = autoLabel;
          }
          renderSessionSelect();
          console.debug('[chat] Auto-labeled session:', autoLabel);
        })
        .catch((e) => console.warn('[chat] Auto-label failed:', e));
    }
  }

  if (result.usage) recordTokenUsage(result.usage as Record<string, unknown>);

  const ackText =
    result.text ??
    (typeof result.response === 'string' ? result.response : null) ??
    extractContent(result.response);
  if (ackText && ss.resolve) {
    appendStreamingDelta(ackText);
    ss.resolve(ackText);
    ss.resolve = null;
  }
}

export async function sendMessage(): Promise<void> {
  const chatInput = document.getElementById('chat-input') as HTMLTextAreaElement | null;
  const chatSend = document.getElementById('chat-send') as HTMLButtonElement | null;
  const chatModelSelect = document.getElementById('chat-model-select') as HTMLSelectElement | null;
  let content = chatInput?.value.trim();
  const currentKey = appState.currentSessionKey ?? '';
  if (!content || appState.activeStreams.has(currentKey)) return;

  // Slash command interception
  if (isSlashCommand(content)) {
    const cmdCtx = buildSlashCommandContext(chatModelSelect);
    const result = await interceptSlashCommand(content, cmdCtx);
    if (result.handled) {
      if (chatInput) {
        chatInput.value = '';
        chatInput.style.height = 'auto';
      }
      if (result.systemMessage) cmdCtx.addSystemMessage(result.systemMessage);
      if (result.refreshSessions) loadSessions({ skipHistory: true }).catch(() => {});
      if (result.preventDefault && !result.rewrittenInput) return;
      if (result.rewrittenInput) content = result.rewrittenInput;
    }
  }

  const attachments = await encodeFileAttachments();

  const userMsg: Message = { role: 'user', content, timestamp: new Date() };
  if (attachments.length) {
    userMsg.attachments = attachments.map((a) => ({
      name: a.name ?? 'attachment',
      mimeType: a.mimeType,
      data: a.content,
    }));
  }
  addMessage(userMsg);
  if (chatInput) {
    chatInput.value = '';
    chatInput.style.height = 'auto';
  }
  clearPendingAttachments();
  if (chatSend) chatSend.disabled = true;

  showStreamingMessage();

  const streamKey = appState.currentSessionKey ?? '';
  const ss = appState.activeStreams.get(streamKey);
  if (!ss) {
    console.error('[chat] Stream state missing for key:', streamKey);
    if (chatSend) chatSend.disabled = false;
    return;
  }

  const responsePromise = new Promise<string>((resolve) => {
    ss.resolve = resolve;
    ss.timeout = setTimeout(() => {
      console.warn('[chat] Streaming timeout — auto-finalizing');
      resolve(ss.content || '(Response timed out)');
    }, 600_000);
  });

  try {
    const sessionKey = appState.currentSessionKey ?? 'default';
    const chatOpts: Record<string, unknown> = {};
    const currentAgent = AgentsModule.getCurrentAgent();
    if (currentAgent) {
      if (currentAgent.model && currentAgent.model !== 'default')
        chatOpts.model = currentAgent.model;
      chatOpts.agentProfile = currentAgent;
    }
    if (attachments.length) chatOpts.attachments = attachments;
    const chatModelVal = chatModelSelect?.value;
    if (chatModelVal && chatModelVal !== 'default') chatOpts.model = chatModelVal;
    const slashOverrides = getSlashOverrides();
    if (slashOverrides.model) chatOpts.model = slashOverrides.model;
    if (slashOverrides.thinkingLevel) {
      chatOpts.thinkingLevel = slashOverrides.thinkingLevel;
    } else if (currentAgent?.thinking_level) {
      chatOpts.thinkingLevel = currentAgent.thinking_level;
    }
    if (slashOverrides.temperature !== undefined) chatOpts.temperature = slashOverrides.temperature;

    const result = await engineChatSend(
      sessionKey,
      content,
      chatOpts as {
        model?: string;
        thinkingLevel?: string;
        temperature?: number;
        attachments?: Array<{ type?: string; mimeType: string; content: string }>;
        agentProfile?: Partial<Agent>;
      },
    );
    console.debug('[chat] send ack:', JSON.stringify(result).slice(0, 300));
    handleSendResult(result, ss, streamKey);

    const finalText = await responsePromise;
    if (appState.activeStreams.has(streamKey)) {
      finalizeStreaming(finalText, undefined, streamKey);
    } else {
      console.debug('[chat] Stream already torn down — skipping finalizeStreaming');
    }
    loadSessions({ skipHistory: true }).catch(() => {});
  } catch (error) {
    console.error('[chat] error:', error);
    if (ss?.el && appState.activeStreams.has(streamKey)) {
      const errMsg = error instanceof Error ? error.message : 'Failed to get response';
      finalizeStreaming(ss.content || `Error: ${errMsg}`, undefined, streamKey);
    }
  } finally {
    const finalKey = appState.currentSessionKey ?? streamKey;
    appState.activeStreams.delete(finalKey);
    appState.activeStreams.delete(streamKey);
    if (ss?.timeout) {
      clearTimeout(ss.timeout);
      ss.timeout = null;
    }
    const chatSendBtn = document.getElementById('chat-send') as HTMLButtonElement | null;
    if (chatSendBtn) chatSendBtn.disabled = false;
  }
}

// ── Wire up all chat DOM event listeners ─────────────────────────────────
// Called once from main.ts DOMContentLoaded.
// Delegates to the chat_listeners molecule with organism-level deps.

export function initChatListeners(): void {
  _initChatListeners({
    sendMessage,
    switchToAgent,
    loadSessions,
    loadChatHistory,
    renderMessages,
    resetTokenMeter,
    teardownStream,
    getTokenMeter,
    meterSnapshot,
  });
}
