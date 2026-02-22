// src/engine/organisms/chat_controller.ts
// Owns all chat UI logic: sessions, messages, streaming, token metering, TTS,
// attachments, and retry.  Imports from state/index.ts so it never touches
// the main.ts closure scope.

import { pawEngine } from '../../engine';
import { engineChatSend } from '../molecules/bridge';
import {
  appState,
  agentSessionMap,
  persistAgentSessionMap,
  MODEL_CONTEXT_SIZES,
  MODEL_COST_PER_TOKEN,
  COMPACTION_WARN_THRESHOLD,
  createStreamState,
  sweepStaleStreams,
  type StreamState,
  type MessageWithAttachments,
} from '../../state/index';
import { formatMarkdown } from '../../components/molecules/markdown';
import { escHtml, icon, confirmModal } from '../../components/helpers';
import { showToast } from '../../components/toast';
import * as AgentsModule from '../../views/agents';
import * as SettingsModule from '../../views/settings-main';
import {
  interceptSlashCommand,
  getSessionOverrides as getSlashOverrides,
  getAutocompleteSuggestions,
  isSlashCommand,
  type CommandContext,
} from '../../features/slash-commands';
import type { Message, ToolCall, Agent } from '../../types';

const $ = (id: string) => document.getElementById(id);

// ── Auto-label helper ────────────────────────────────────────────────────
/** Generate a short label from the user's first message (max 50 chars). */
function generateSessionLabel(message: string): string {
  // Strip leading slashes, markdown, excessive whitespace
  let label = message
    .replace(/^\/\w+\s*/, '')
    .replace(/[#*_~`>]+/g, '')
    .trim();
  // Collapse whitespace
  label = label.replace(/\s+/g, ' ');
  if (label.length > 50) {
    label = `${label.slice(0, 47).replace(/\s+\S*$/, '')}…`;
  }
  return label || 'New chat';
}

/** Relative time string (e.g. "2h ago", "3d ago"). */
function relativeTime(date: Date | number): string {
  const now = Date.now();
  const ms = typeof date === 'number' ? date : date.getTime();
  const diff = now - ms;
  const secs = Math.floor(diff / 1000);
  if (secs < 60) return 'just now';
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  const days = Math.floor(hrs / 24);
  if (days < 30) return `${days}d ago`;
  return new Date(ms).toLocaleDateString();
}

// ── Utility helpers ──────────────────────────────────────────────────────
export function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result as string;
      resolve(result.split(',')[1] || result);
    };
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
}

function fileTypeIcon(mimeType: string): string {
  if (mimeType.startsWith('image/')) return 'image';
  if (mimeType === 'application/pdf' || mimeType.startsWith('text/')) return 'file-text';
  return 'file';
}

export function extractContent(content: unknown): string {
  if (typeof content === 'string') return content;
  if (Array.isArray(content)) {
    return (content as Record<string, unknown>[])
      .filter((b) => b.type === 'text' && typeof b.text === 'string')
      .map((b) => b.text as string)
      .join('\n');
  }
  if (content && typeof content === 'object') {
    const obj = content as Record<string, unknown>;
    if (obj.type === 'text' && typeof obj.text === 'string') return obj.text;
  }
  return content == null ? '' : String(content);
}

function findLastIndex<T>(arr: T[], pred: (item: T) => boolean): number {
  for (let i = arr.length - 1; i >= 0; i--) {
    if (pred(arr[i])) return i;
  }
  return -1;
}

// ── Session management ───────────────────────────────────────────────────
export async function loadSessions(opts?: { skipHistory?: boolean }): Promise<void> {
  if (!appState.wsConnected) return;
  try {
    const engineSessions = await pawEngine.sessionsList(200);

    // ── Auto-prune empty sessions older than 1 hour (bulk Rust-side) ──
    pawEngine
      .sessionCleanup(3600, appState.currentSessionKey ?? undefined)
      .then((n) => {
        if (n > 0) console.debug(`[chat] Pruned ${n} empty session(s)`);
      })
      .catch((e) => console.warn('[chat] Session cleanup failed:', e));

    // Filter out empty old sessions on the display side too
    const ONE_HOUR = 60 * 60 * 1000;
    const now = Date.now();
    const keptSessions = engineSessions.filter((s) => {
      const age = s.updated_at ? now - new Date(s.updated_at).getTime() : Infinity;
      const isEmpty = s.message_count === 0;
      const isCurrentSession = s.id === appState.currentSessionKey;
      return !(isEmpty && age > ONE_HOUR && !isCurrentSession);
    });

    appState.sessions = keptSessions.map((s) => ({
      key: s.id,
      kind: 'direct' as const,
      label: s.label ?? undefined,
      displayName: s.label ?? s.id,
      updatedAt: s.updated_at ? new Date(s.updated_at).getTime() : undefined,
      agentId: s.agent_id ?? undefined,
    }));

    const currentAgent = AgentsModule.getCurrentAgent();
    if (!appState.currentSessionKey && currentAgent) {
      const savedKey = agentSessionMap.get(currentAgent.id);
      const isValidSaved =
        savedKey &&
        appState.sessions.some(
          (s) =>
            s.key === savedKey &&
            (s.agentId === currentAgent.id || (currentAgent.id === 'default' && !s.agentId)),
        );
      if (isValidSaved) {
        appState.currentSessionKey = savedKey;
      } else {
        const agentSession = appState.sessions.find(
          (s) => s.agentId === currentAgent.id || (currentAgent.id === 'default' && !s.agentId),
        );
        if (agentSession) {
          appState.currentSessionKey = agentSession.key;
          agentSessionMap.set(currentAgent.id, agentSession.key);
          persistAgentSessionMap();
        }
      }
    } else if (!appState.currentSessionKey && appState.sessions.length) {
      appState.currentSessionKey = appState.sessions[0].key;
    }

    renderSessionSelect();
    if (!opts?.skipHistory && appState.currentSessionKey && !appState.isLoading) {
      await loadChatHistory(appState.currentSessionKey);
    }
  } catch (e) {
    console.warn('[chat] Sessions load failed:', e);
  }
}

export function renderSessionSelect(): void {
  const chatSessionSelect = $('chat-session-select') as HTMLSelectElement | null;
  if (!chatSessionSelect) return;
  chatSessionSelect.innerHTML = '';

  const currentAgent = AgentsModule.getCurrentAgent();
  const agentSessions = currentAgent
    ? appState.sessions.filter(
        (s) => s.agentId === currentAgent.id || (currentAgent.id === 'default' && !s.agentId),
      )
    : appState.sessions;

  if (!agentSessions.length) {
    const opt = document.createElement('option');
    opt.value = '';
    opt.textContent = 'No sessions — send a message to start';
    chatSessionSelect.appendChild(opt);
    return;
  }
  for (const s of agentSessions) {
    const opt = document.createElement('option');
    opt.value = s.key;
    // Show label + relative time, or clean fallback for unlabeled sessions
    const label = s.label ?? s.displayName ?? 'Untitled chat';
    const timeStr = s.updatedAt ? ` (${relativeTime(s.updatedAt)})` : '';
    opt.textContent = label + timeStr;
    if (s.key === appState.currentSessionKey) opt.selected = true;
    chatSessionSelect.appendChild(opt);
  }
}

export function populateAgentSelect(): void {
  const chatAgentSelect = $('chat-agent-select') as HTMLSelectElement | null;
  if (!chatAgentSelect) return;
  const agents = AgentsModule.getAgents();
  const currentAgent = AgentsModule.getCurrentAgent();
  chatAgentSelect.innerHTML = '';
  for (const a of agents) {
    const opt = document.createElement('option');
    opt.value = a.id;
    opt.textContent = a.name;
    if (a.id === currentAgent?.id) opt.selected = true;
    chatAgentSelect.appendChild(opt);
  }
}

export async function switchToAgent(agentId: string): Promise<void> {
  const prevAgent = AgentsModule.getCurrentAgent();
  if (prevAgent && appState.currentSessionKey) {
    agentSessionMap.set(prevAgent.id, appState.currentSessionKey);
    persistAgentSessionMap();
  }

  // Clean up stale streaming UI
  document.getElementById('streaming-message')?.remove();
  appState.streamingEl = null;

  AgentsModule.setSelectedAgent(agentId);
  const agent = AgentsModule.getCurrentAgent();
  const chatAgentName = $('chat-agent-name');
  if (chatAgentName && agent) {
    chatAgentName.innerHTML = `${AgentsModule.spriteAvatar(agent.avatar, 20)} ${escHtml(agent.name)}`;
  }
  const chatAvatarEl = document.getElementById('chat-avatar');
  if (chatAvatarEl && agent) {
    chatAvatarEl.innerHTML = AgentsModule.spriteAvatar(agent.avatar, 32);
  }
  const chatAgentSelect = $('chat-agent-select') as HTMLSelectElement | null;
  if (chatAgentSelect) chatAgentSelect.value = agentId;

  resetTokenMeter();

  const savedSessionKey = agentSessionMap.get(agentId);
  const savedSessionValid =
    savedSessionKey &&
    appState.sessions.some(
      (s) =>
        s.key === savedSessionKey &&
        (s.agentId === agentId || (agentId === 'default' && !s.agentId)),
    );
  if (savedSessionValid) {
    appState.currentSessionKey = savedSessionKey;
    renderSessionSelect();
    await loadChatHistory(savedSessionKey);
    const chatSessionSelect = $('chat-session-select') as HTMLSelectElement | null;
    if (chatSessionSelect) chatSessionSelect.value = savedSessionKey;
  } else {
    const agentSession = appState.sessions.find(
      (s) => s.agentId === agentId || (agentId === 'default' && !s.agentId),
    );
    if (agentSession) {
      appState.currentSessionKey = agentSession.key;
      agentSessionMap.set(agentId, agentSession.key);
      persistAgentSessionMap();
      renderSessionSelect();
      await loadChatHistory(agentSession.key);
      const chatSessionSelect = $('chat-session-select') as HTMLSelectElement | null;
      if (chatSessionSelect) chatSessionSelect.value = agentSession.key;
    } else {
      appState.currentSessionKey = null;
      appState.messages = [];
      renderSessionSelect();
      renderMessages();
      const chatSessionSelect = $('chat-session-select') as HTMLSelectElement | null;
      if (chatSessionSelect) chatSessionSelect.value = '';
    }
  }
  console.debug(
    `[chat] Switched to agent "${agent?.name}" (${agentId}), session=${appState.currentSessionKey ?? 'new'}`,
  );
}

export async function loadChatHistory(sessionKey: string): Promise<void> {
  if (!appState.wsConnected) return;
  try {
    const stored = await pawEngine.chatHistory(sessionKey, 200);
    appState.messages = stored
      .filter((m) => m.role === 'user' || m.role === 'assistant')
      .map((m) => ({
        id: m.id,
        role: m.role as 'user' | 'assistant' | 'system',
        content: m.content,
        timestamp: new Date(m.created_at),
      }));
    renderMessages();
  } catch (e) {
    console.warn('[chat] History load failed:', e);
    appState.messages = [];
    renderMessages();
  }
}

// ── Token metering ─────────────────────────────────────────────────────────
export function resetTokenMeter(): void {
  appState.sessionTokensUsed = 0;
  appState.sessionInputTokens = 0;
  appState.sessionOutputTokens = 0;
  appState.sessionCost = 0;
  appState.lastRecordedTotal = 0;
  appState.compactionDismissed = false;
  updateTokenMeter();
  ($('session-budget-alert') as HTMLElement | null)?.style !== undefined &&
    (($('session-budget-alert') as HTMLElement).style.display = 'none');
}

export function updateTokenMeter(): void {
  const meter = $('token-meter');
  const fill = $('token-meter-fill');
  const label = $('token-meter-label');
  if (!meter || !fill || !label) return;
  meter.style.display = '';

  if (appState.sessionTokensUsed <= 0) {
    fill.style.width = '0%';
    fill.className = 'token-meter-fill';
    const lim =
      appState.modelContextLimit >= 1000
        ? `${(appState.modelContextLimit / 1000).toFixed(0)}k`
        : `${appState.modelContextLimit}`;
    label.textContent = `0 / ${lim} tokens`;
    meter.title = 'Token tracking active — send a message to see usage';
    return;
  }

  const pct = Math.min((appState.sessionTokensUsed / appState.modelContextLimit) * 100, 100);
  fill.style.width = `${pct}%`;
  fill.className =
    pct >= 80
      ? 'token-meter-fill danger'
      : pct >= 60
        ? 'token-meter-fill warning'
        : 'token-meter-fill';

  const used =
    appState.sessionTokensUsed >= 1000
      ? `${(appState.sessionTokensUsed / 1000).toFixed(1)}k`
      : `${appState.sessionTokensUsed}`;
  const lim =
    appState.modelContextLimit >= 1000
      ? `${(appState.modelContextLimit / 1000).toFixed(0)}k`
      : `${appState.modelContextLimit}`;
  const cost = appState.sessionCost > 0 ? ` · $${appState.sessionCost.toFixed(4)}` : '';
  label.textContent = `${used} / ${lim} tokens${cost}`;
  meter.title = `Session tokens: ${appState.sessionTokensUsed.toLocaleString()} / ${appState.modelContextLimit.toLocaleString()} (In: ${appState.sessionInputTokens.toLocaleString()} / Out: ${appState.sessionOutputTokens.toLocaleString()}) — Est. cost: $${appState.sessionCost.toFixed(4)}`;

  updateCompactionWarning(pct);
}

function updateCompactionWarning(pct: number): void {
  const warning = $('compaction-warning');
  if (!warning) return;
  if (pct >= COMPACTION_WARN_THRESHOLD * 100 && !appState.compactionDismissed) {
    warning.style.display = '';
    const text = $('compaction-warning-text');
    if (text) {
      text.textContent =
        pct >= 95
          ? `Context window ${pct.toFixed(0)}% full — messages will be compacted imminently`
          : `Context window ${pct.toFixed(0)}% full — older messages may be compacted soon`;
    }
  } else {
    warning.style.display = 'none';
  }
}

export function updateContextLimitFromModel(modelName: string): void {
  const lower = modelName.toLowerCase();
  for (const [prefix, limit] of Object.entries(MODEL_CONTEXT_SIZES)) {
    if (lower.includes(prefix)) {
      if (appState.modelContextLimit !== limit) {
        console.debug(
          `[token] Context limit: ${appState.modelContextLimit.toLocaleString()} → ${limit.toLocaleString()} (${modelName})`,
        );
        appState.modelContextLimit = limit;
        updateTokenMeter();
      }
      appState.activeModelKey = prefix;
      return;
    }
  }
}

export function recordTokenUsage(usage: Record<string, unknown> | undefined): void {
  if (!usage) return;
  const uAny = usage as Record<string, unknown>;
  const nested = uAny.response as Record<string, unknown> | undefined;
  const inner = (uAny.usage ?? nested?.usage ?? usage) as Record<string, unknown>;
  const totalTokens = (inner.totalTokens ??
    inner.total_tokens ??
    inner.totalTokenCount ??
    0) as number;
  const inputTokens = (inner.promptTokens ??
    inner.prompt_tokens ??
    inner.inputTokens ??
    inner.input_tokens ??
    inner.prompt_token_count ??
    0) as number;
  const outputTokens = (inner.completionTokens ??
    inner.completion_tokens ??
    inner.outputTokens ??
    inner.output_tokens ??
    inner.completion_token_count ??
    0) as number;

  if (totalTokens > 0 || inputTokens > 0 || outputTokens > 0) {
    appState.sessionInputTokens = inputTokens;
    appState.sessionOutputTokens += outputTokens;
    appState.sessionTokensUsed = inputTokens + appState.sessionOutputTokens;
    appState.lastRecordedTotal = appState.sessionTokensUsed;
  }

  const rate = MODEL_COST_PER_TOKEN[appState.activeModelKey] ?? MODEL_COST_PER_TOKEN['default'];
  appState.sessionCost += inputTokens * rate.input + outputTokens * rate.output;

  const budgetLimit = SettingsModule.getBudgetLimit();
  if (budgetLimit != null && appState.sessionCost >= budgetLimit * 0.8) {
    const budgetAlert = $('session-budget-alert');
    if (budgetAlert) {
      budgetAlert.style.display = '';
      const alertText = $('session-budget-alert-text');
      if (alertText) {
        alertText.textContent =
          appState.sessionCost >= budgetLimit
            ? `Session budget exceeded: $${appState.sessionCost.toFixed(4)} / $${budgetLimit.toFixed(2)}`
            : `Nearing session budget: $${appState.sessionCost.toFixed(4)} / $${budgetLimit.toFixed(2)}`;
      }
    }
  }
  updateTokenMeter();
}

// ── Streaming pipeline ────────────────────────────────────────────────────
export function showStreamingMessage(): void {
  const chatEmpty = $('chat-empty');
  const chatMessages = $('chat-messages');
  if (chatEmpty) chatEmpty.style.display = 'none';

  const div = document.createElement('div');
  div.className = 'message assistant';
  div.id = 'streaming-message';

  const contentEl = document.createElement('div');
  contentEl.className = 'message-content';
  contentEl.innerHTML = '<div class="loading-dots"><span></span><span></span><span></span></div>';

  const time = document.createElement('div');
  time.className = 'message-time';
  time.textContent = new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });

  div.appendChild(contentEl);
  div.appendChild(time);
  chatMessages?.appendChild(div);

  // Create session-keyed stream state
  const key = appState.currentSessionKey ?? '';
  sweepStaleStreams(); // Evict leaked entries before adding a new one
  const ss = createStreamState(AgentsModule.getCurrentAgent()?.id);
  ss.el = contentEl;
  appState.activeStreams.set(key, ss);

  const abortBtn = $('chat-abort-btn');
  if (abortBtn) abortBtn.style.display = '';
  scrollToBottom();
}

export function scrollToBottom(): void {
  const chatMessages = $('chat-messages');
  if (appState.scrollRafPending || !chatMessages) return;
  appState.scrollRafPending = true;
  requestAnimationFrame(() => {
    const el = document.getElementById('chat-messages');
    if (el) el.scrollTop = el.scrollHeight;
    appState.scrollRafPending = false;
  });
}

export function appendStreamingDelta(text: string): void {
  appState.streamingContent += text;
  if (appState.streamingEl) {
    appState.streamingEl.innerHTML = formatMarkdown(appState.streamingContent);
    scrollToBottom();
  }
}

/**
 * Append a thinking/reasoning delta to the streaming message.
 * Renders inside a collapsible `<details>` block above the main response.
 */
export function appendThinkingDelta(text: string): void {
  const key = appState.currentSessionKey ?? '';
  const ss = appState.activeStreams.get(key);
  if (!ss) return;
  ss.thinkingContent += text;
  console.debug('[thinking] delta received:', JSON.stringify(text), 'total:', ss.thinkingContent.length);

  // Find or create the thinking container inside the streaming message
  const streamMsg = document.getElementById('streaming-message');
  if (!streamMsg) {
    console.debug('[thinking] no streaming-message element found');
    return;
  }

  let thinkingEl = streamMsg.querySelector('.thinking-block') as HTMLElement | null;
  if (!thinkingEl) {
    thinkingEl = document.createElement('details');
    thinkingEl.className = 'thinking-block';
    thinkingEl.setAttribute('open', '');
    const summary = document.createElement('summary');
    summary.textContent = 'Thinking\u2026';
    thinkingEl.appendChild(summary);
    const content = document.createElement('div');
    content.className = 'thinking-content';
    thinkingEl.appendChild(content);
    // Insert before the message-content element
    const contentEl = streamMsg.querySelector('.message-content');
    if (contentEl) {
      streamMsg.insertBefore(thinkingEl, contentEl);
    } else {
      streamMsg.prepend(thinkingEl);
    }
  }

  const contentDiv = thinkingEl.querySelector('.thinking-content') as HTMLElement | null;
  if (contentDiv) {
    contentDiv.innerHTML = formatMarkdown(ss.thinkingContent);
  }
  scrollToBottom();
}

export function finalizeStreaming(finalContent: string, toolCalls?: ToolCall[]): void {
  $('streaming-message')?.remove();

  // Tear down session-keyed stream
  const key = appState.currentSessionKey ?? '';
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
    autoSpeakIfEnabled(finalContent);

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

// ── Message rendering ────────────────────────────────────────────────────
export function addMessage(message: MessageWithAttachments): void {
  appState.messages.push(message);
  renderMessages();
}

function retryMessage(content: string): void {
  if (appState.isLoading || !content) return;
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

/** Render an inline screenshot card for assistant messages */
function renderScreenshotCard(msgContent: string): HTMLElement | null {
  const ssMatch = msgContent.match(/Screenshot saved:\s*([^\n]+\.png)/);
  if (!ssMatch) return null;
  const ssFilename = ssMatch[1].split('/').pop() || '';
  if (!ssFilename.startsWith('screenshot-')) return null;

  const ssCard = document.createElement('div');
  ssCard.className = 'message-screenshot-card';
  ssCard.style.cssText =
    'margin:8px 0;border-radius:8px;overflow:hidden;border:1px solid var(--border-color);cursor:pointer;max-width:400px';
  ssCard.innerHTML =
    '<div style="padding:8px;text-align:center;color:var(--text-muted);font-size:12px">Loading screenshot…</div>';
  (async () => {
    try {
      const { pawEngine: eng } = await import('../molecules/ipc_client');
      const ss = await eng.screenshotGet(ssFilename);
      if (ss.base64_png) {
        ssCard.innerHTML = '';
        const img = document.createElement('img');
        img.src = `data:image/png;base64,${ss.base64_png}`;
        img.style.cssText = 'width:100%;display:block';
        img.alt = ssFilename;
        ssCard.appendChild(img);
        ssCard.addEventListener('click', () => {
          const win = window.open('', '_blank');
          if (win) {
            win.document.title = ssFilename;
            win.document.body.style.cssText =
              'margin:0;background:#111;display:flex;align-items:center;justify-content:center;min-height:100vh';
            const fullImg = win.document.createElement('img');
            fullImg.src = img.src;
            fullImg.style.maxWidth = '100%';
            win.document.body.appendChild(fullImg);
          }
        });
      }
    } catch {
      ssCard.innerHTML =
        '<div style="padding:8px;color:var(--text-muted);font-size:12px">Screenshot unavailable</div>';
    }
  })();
  return ssCard;
}

/** Render attachment strip (images + file chips) for a message */
function renderAttachmentStrip(attachments: NonNullable<Message['attachments']>): HTMLElement {
  const strip = document.createElement('div');
  strip.className = 'message-attachments';
  for (const att of attachments) {
    if (att.mimeType?.startsWith('image/')) {
      const card = document.createElement('div');
      card.className = 'message-attachment-card';
      const img = document.createElement('img');
      img.className = 'message-attachment-img';
      img.alt = att.name || 'attachment';
      if (att.url) img.src = att.url;
      else if (att.data) img.src = `data:${att.mimeType};base64,${att.data}`;
      const overlay = document.createElement('div');
      overlay.className = 'message-attachment-overlay';
      overlay.innerHTML = icon('external-link');
      card.appendChild(img);
      card.appendChild(overlay);
      card.addEventListener('click', () => window.open(img.src, '_blank'));
      if (att.name) {
        const lbl = document.createElement('div');
        lbl.className = 'message-attachment-label';
        lbl.textContent = att.name;
        card.appendChild(lbl);
      }
      strip.appendChild(card);
    } else {
      const docChip = document.createElement('div');
      docChip.className = 'message-attachment-doc';
      const iconName =
        att.mimeType?.startsWith('text/') || att.mimeType === 'application/pdf'
          ? 'file-text'
          : 'file';
      docChip.innerHTML = icon(iconName);
      const nameSpan = document.createElement('span');
      nameSpan.textContent = att.name || 'file';
      docChip.appendChild(nameSpan);
      strip.appendChild(docChip);
    }
  }
  return strip;
}

/** Render a single message element */
function renderSingleMessage(
  msg: Message,
  index: number,
  lastUserIdx: number,
  lastAssistantIdx: number,
): HTMLElement {
  const div = document.createElement('div');
  div.className = `message ${msg.role}`;

  // Thinking block (collapsed in history)
  if (msg.thinkingContent) {
    const thinkingEl = document.createElement('details');
    thinkingEl.className = 'thinking-block';
    const summary = document.createElement('summary');
    summary.textContent = 'Thinking';
    thinkingEl.appendChild(summary);
    const thinkingDiv = document.createElement('div');
    thinkingDiv.className = 'thinking-content';
    thinkingDiv.innerHTML = formatMarkdown(msg.thinkingContent);
    thinkingEl.appendChild(thinkingDiv);
    div.appendChild(thinkingEl);
  }

  const contentEl = document.createElement('div');
  contentEl.className = 'message-content';
  if (msg.role === 'assistant' || msg.role === 'system') {
    contentEl.innerHTML = formatMarkdown(msg.content);
  } else {
    contentEl.textContent = msg.content;
  }

  const time = document.createElement('div');
  time.className = 'message-time';
  time.textContent = msg.timestamp.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });

  div.appendChild(contentEl);

  // Inline screenshot detection
  if (msg.role === 'assistant' && msg.content.includes('Screenshot saved:')) {
    const ssCard = renderScreenshotCard(msg.content);
    if (ssCard) div.appendChild(ssCard);
  }

  // Image/file attachments
  if (msg.attachments?.length) {
    div.appendChild(renderAttachmentStrip(msg.attachments));
  }

  div.appendChild(time);

  // Tool calls badge
  if (msg.toolCalls?.length) {
    const badge = document.createElement('div');
    badge.className = 'tool-calls-badge';
    badge.innerHTML = `${icon('wrench')} ${msg.toolCalls.length} tool call${msg.toolCalls.length > 1 ? 's' : ''}`;
    div.appendChild(badge);
  }

  // Retry button
  const isLastUser = index === lastUserIdx;
  const isErrored = index === lastAssistantIdx && msg.content.startsWith('Error:');
  if ((isLastUser || isErrored) && !appState.isLoading) {
    const retryBtn = document.createElement('button');
    retryBtn.className = 'message-retry-btn';
    retryBtn.title = 'Retry';
    retryBtn.innerHTML = `${icon('rotate-ccw')} Retry`;
    const retryContent = isLastUser
      ? msg.content
      : lastUserIdx >= 0
        ? appState.messages[lastUserIdx].content
        : '';
    retryBtn.addEventListener('click', () => retryMessage(retryContent));
    div.appendChild(retryBtn);
  }

  // TTS button
  if (msg.role === 'assistant' && msg.content && !msg.content.startsWith('Error:')) {
    const ttsBtn = document.createElement('button');
    ttsBtn.className = 'message-tts-btn';
    ttsBtn.title = 'Read aloud';
    ttsBtn.innerHTML = `<span class="ms">volume_up</span>`;
    const msgContent = msg.content;
    ttsBtn.addEventListener('click', () => speakMessage(msgContent, ttsBtn));
    div.appendChild(ttsBtn);
  }

  return div;
}

export function renderMessages(): void {
  const chatMessages = $('chat-messages');
  const chatEmpty = $('chat-empty');
  if (!chatMessages) return;
  chatMessages.querySelectorAll('.message').forEach((m) => m.remove());

  if (appState.messages.length === 0) {
    if (chatEmpty) chatEmpty.style.display = 'flex';
    return;
  }
  if (chatEmpty) chatEmpty.style.display = 'none';

  const frag = document.createDocumentFragment();
  const lastUserIdx = findLastIndex(appState.messages, (m) => m.role === 'user');
  const lastAssistantIdx = findLastIndex(appState.messages, (m) => m.role === 'assistant');

  for (let i = 0; i < appState.messages.length; i++) {
    frag.appendChild(renderSingleMessage(appState.messages[i], i, lastUserIdx, lastAssistantIdx));
  }

  const streamingEl = $('streaming-message');
  if (streamingEl) chatMessages.insertBefore(frag, streamingEl);
  else chatMessages.appendChild(frag);
  scrollToBottom();
}

// ── Attachment helpers ─────────────────────────────────────────────────────
export function renderAttachmentPreview(): void {
  const chatAttachmentPreview = $('chat-attachment-preview');
  if (!chatAttachmentPreview) return;
  if (appState.pendingAttachments.length === 0) {
    chatAttachmentPreview.style.display = 'none';
    chatAttachmentPreview.innerHTML = '';
    return;
  }
  chatAttachmentPreview.style.display = 'flex';
  chatAttachmentPreview.innerHTML = '';
  for (let i = 0; i < appState.pendingAttachments.length; i++) {
    const file = appState.pendingAttachments[i];
    const chip = document.createElement('div');
    chip.className = 'attachment-chip';
    if (file.type.startsWith('image/')) {
      const img = document.createElement('img');
      img.src = URL.createObjectURL(file);
      img.className = 'attachment-chip-thumb';
      img.onload = () => URL.revokeObjectURL(img.src);
      chip.appendChild(img);
    } else {
      const iconWrap = document.createElement('span');
      iconWrap.className = 'attachment-chip-icon';
      iconWrap.innerHTML = icon(fileTypeIcon(file.type));
      chip.appendChild(iconWrap);
    }
    const meta = document.createElement('div');
    meta.className = 'attachment-chip-meta';
    const nameEl = document.createElement('span');
    nameEl.className = 'attachment-chip-name';
    nameEl.textContent = file.name.length > 24 ? `${file.name.slice(0, 21)}...` : file.name;
    nameEl.title = file.name;
    meta.appendChild(nameEl);
    const sizeEl = document.createElement('span');
    sizeEl.className = 'attachment-chip-size';
    sizeEl.textContent =
      file.size < 1024
        ? `${file.size} B`
        : file.size < 1048576
          ? `${(file.size / 1024).toFixed(1)} KB`
          : `${(file.size / 1048576).toFixed(1)} MB`;
    meta.appendChild(sizeEl);
    chip.appendChild(meta);
    const removeBtn = document.createElement('button');
    removeBtn.className = 'attachment-chip-remove';
    removeBtn.innerHTML = icon('x');
    removeBtn.title = 'Remove';
    const idx = i;
    removeBtn.addEventListener('click', () => {
      appState.pendingAttachments.splice(idx, 1);
      renderAttachmentPreview();
    });
    chip.appendChild(removeBtn);
    chatAttachmentPreview.appendChild(chip);
  }
}

export function clearPendingAttachments(): void {
  appState.pendingAttachments = [];
  renderAttachmentPreview();
}

// ── TTS ───────────────────────────────────────────────────────────────────
export async function speakMessage(text: string, btn: HTMLButtonElement): Promise<void> {
  if (appState.ttsAudio && appState.ttsActiveBtn === btn) {
    appState.ttsAudio.pause();
    appState.ttsAudio = null;
    btn.innerHTML = `<span class="ms">volume_up</span>`;
    btn.classList.remove('tts-playing');
    appState.ttsActiveBtn = null;
    return;
  }
  if (appState.ttsAudio) {
    appState.ttsAudio.pause();
    appState.ttsAudio = null;
    if (appState.ttsActiveBtn) {
      appState.ttsActiveBtn.innerHTML = `<span class="ms">volume_up</span>`;
      appState.ttsActiveBtn.classList.remove('tts-playing');
    }
  }
  btn.innerHTML = `<span class="ms">hourglass_top</span>`;
  btn.classList.add('tts-loading');
  try {
    const base64Audio = await pawEngine.ttsSpeak(text);
    const audioBytes = Uint8Array.from(atob(base64Audio), (c) => c.charCodeAt(0));
    const blob = new Blob([audioBytes], { type: 'audio/mp3' });
    const url = URL.createObjectURL(blob);
    appState.ttsAudio = new Audio(url);
    appState.ttsActiveBtn = btn;
    btn.innerHTML = `<span class="ms">stop_circle</span>`;
    btn.classList.remove('tts-loading');
    btn.classList.add('tts-playing');
    appState.ttsAudio.addEventListener('ended', () => {
      btn.innerHTML = `<span class="ms">volume_up</span>`;
      btn.classList.remove('tts-playing');
      URL.revokeObjectURL(url);
      appState.ttsAudio = null;
      appState.ttsActiveBtn = null;
    });
    appState.ttsAudio.addEventListener('error', () => {
      btn.innerHTML = `<span class="ms">volume_up</span>`;
      btn.classList.remove('tts-playing');
      URL.revokeObjectURL(url);
      appState.ttsAudio = null;
      appState.ttsActiveBtn = null;
    });
    appState.ttsAudio.play();
  } catch (e) {
    console.error('[tts] Error:', e);
    btn.innerHTML = `<span class="ms">volume_up</span>`;
    btn.classList.remove('tts-loading', 'tts-playing');
    showToast(e instanceof Error ? e.message : 'TTS failed — check Voice settings', 'error');
  }
}

async function autoSpeakIfEnabled(text: string): Promise<void> {
  try {
    const cfg = await pawEngine.ttsGetConfig();
    if (!cfg.auto_speak) return;
    const base64Audio = await pawEngine.ttsSpeak(text);
    const audioBytes = Uint8Array.from(atob(base64Audio), (c) => c.charCodeAt(0));
    const blob = new Blob([audioBytes], { type: 'audio/mp3' });
    const url = URL.createObjectURL(blob);
    if (appState.ttsAudio) appState.ttsAudio.pause();
    appState.ttsAudio = new Audio(url);
    appState.ttsAudio.addEventListener('ended', () => {
      URL.revokeObjectURL(url);
      appState.ttsAudio = null;
    });
    appState.ttsAudio.play();
  } catch (e) {
    console.warn('[tts] Auto-speak failed:', e);
  }
}

// ── Send message ──────────────────────────────────────────────────────────
/** Build the command context object for slash command interception */
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

/** Encode pending file attachments to base64 for sending */
async function encodeFileAttachments(): Promise<
  Array<{ type: string; mimeType: string; content: string; name?: string }>
> {
  const attachments: Array<{ type: string; mimeType: string; content: string; name?: string }> = [];
  for (const file of appState.pendingAttachments) {
    try {
      const base64 = await fileToBase64(file);
      const mime =
        file.type ||
        (file.name?.match(/\.(txt|md|csv|json|xml|html|css|js|ts|py|rs|sh|yaml|yml|toml|log)$/i)
          ? 'text/plain'
          : 'application/octet-stream');
      attachments.push({
        type: mime.startsWith('image/') ? 'image' : 'file',
        mimeType: mime,
        content: base64,
        name: file.name,
      });
    } catch (e) {
      console.error('[chat] Attachment encode failed:', file.name, e);
    }
  }
  return attachments;
}

/** Handle the send result — update session, auto-label, process ack text */
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
  if (!content || appState.isLoading) return;

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

  // Encode pending file attachments
  const attachments = await encodeFileAttachments();

  // User message
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
  appState.isLoading = true;
  if (chatSend) chatSend.disabled = true;

  showStreamingMessage();

  const streamKey = appState.currentSessionKey ?? '';
  const ss = appState.activeStreams.get(streamKey);
  if (!ss) {
    console.error('[chat] Stream state missing for key:', streamKey);
    appState.isLoading = false;
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
    if (slashOverrides.thinkingLevel) chatOpts.thinkingLevel = slashOverrides.thinkingLevel;
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
    finalizeStreaming(finalText);
    loadSessions({ skipHistory: true }).catch(() => {});
  } catch (error) {
    console.error('[chat] error:', error);
    if (ss?.el) {
      const errMsg = error instanceof Error ? error.message : 'Failed to get response';
      finalizeStreaming(ss.content || `Error: ${errMsg}`);
    }
  } finally {
    appState.isLoading = false;
    const finalKey = appState.currentSessionKey ?? streamKey;
    appState.activeStreams.delete(finalKey);
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
export function initChatListeners(): void {
  const chatSend = document.getElementById('chat-send') as HTMLButtonElement | null;
  const chatInput = document.getElementById('chat-input') as HTMLTextAreaElement | null;
  const chatAttachBtn = document.getElementById('chat-attach-btn');
  const chatFileInput = document.getElementById('chat-file-input') as HTMLInputElement | null;
  const chatSessionSelect = document.getElementById(
    'chat-session-select',
  ) as HTMLSelectElement | null;
  const chatAgentSelect = document.getElementById('chat-agent-select') as HTMLSelectElement | null;

  chatSend?.addEventListener('click', sendMessage);

  chatInput?.addEventListener('keydown', (e) => {
    const popup = document.getElementById('slash-autocomplete');
    if (popup && popup.style.display !== 'none') {
      if (e.key === 'Escape') {
        popup.style.display = 'none';
        e.preventDefault();
        return;
      }
      if (e.key === 'Tab' || (e.key === 'Enter' && !e.shiftKey)) {
        const selected = popup.querySelector('.slash-ac-item.selected') as HTMLElement | null;
        if (selected) {
          e.preventDefault();
          const cmd = selected.dataset.command ?? '';
          if (chatInput) {
            chatInput.value = `${cmd} `;
            chatInput.focus();
          }
          popup.style.display = 'none';
          return;
        }
      }
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        const items = Array.from(popup.querySelectorAll('.slash-ac-item')) as HTMLElement[];
        const cur = items.findIndex((el) => el.classList.contains('selected'));
        items.forEach((el) => el.classList.remove('selected'));
        const next =
          e.key === 'ArrowDown'
            ? (cur + 1) % items.length
            : (cur - 1 + items.length) % items.length;
        items[next]?.classList.add('selected');
        return;
      }
    }
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendMessage();
    }
  });

  chatInput?.addEventListener('input', () => {
    if (!chatInput) return;
    chatInput.style.height = 'auto';
    chatInput.style.height = `${Math.min(chatInput.scrollHeight, 120)}px`;
    const val = chatInput.value;
    let popup = document.getElementById('slash-autocomplete') as HTMLElement | null;
    if (val.startsWith('/') && !val.includes(' ')) {
      const suggestions = getAutocompleteSuggestions(val);
      if (suggestions.length > 0) {
        if (!popup) {
          popup = document.createElement('div');
          popup.id = 'slash-autocomplete';
          popup.className = 'slash-autocomplete-popup';
          chatInput.parentElement?.insertBefore(popup, chatInput);
        }
        popup.innerHTML = suggestions
          .map(
            (s, i) =>
              `<div class="slash-ac-item${i === 0 ? ' selected' : ''}" data-command="${s.command}">
            <span class="slash-ac-cmd">${s.command}</span>
            <span class="slash-ac-desc">${s.description}</span>
          </div>`,
          )
          .join('');
        popup.style.display = 'block';
        popup.querySelectorAll('.slash-ac-item').forEach((item) => {
          item.addEventListener('click', () => {
            const cmd = (item as HTMLElement).dataset.command ?? '';
            if (chatInput) {
              chatInput.value = `${cmd} `;
              chatInput.focus();
            }
            if (popup) popup.style.display = 'none';
          });
        });
      } else if (popup) {
        popup.style.display = 'none';
      }
    } else if (popup) {
      popup.style.display = 'none';
    }
  });

  chatAttachBtn?.addEventListener('click', () => chatFileInput?.click());
  chatFileInput?.addEventListener('change', () => {
    if (!chatFileInput?.files) return;
    for (const file of Array.from(chatFileInput.files)) appState.pendingAttachments.push(file);
    chatFileInput.value = '';
    renderAttachmentPreview();
  });

  chatSessionSelect?.addEventListener('change', () => {
    const key = chatSessionSelect?.value;
    if (!key) return;
    appState.currentSessionKey = key;
    const curAgent = AgentsModule.getCurrentAgent();
    if (curAgent) {
      agentSessionMap.set(curAgent.id, key);
      persistAgentSessionMap();
    }
    resetTokenMeter();
    loadChatHistory(key);
  });

  chatAgentSelect?.addEventListener('change', () => {
    const agentId = chatAgentSelect?.value;
    if (agentId) switchToAgent(agentId);
  });

  $('new-chat-btn')?.addEventListener('click', () => {
    appState.messages = [];
    appState.currentSessionKey = null;
    resetTokenMeter();
    renderMessages();
    const chatSessionSelect2 = document.getElementById(
      'chat-session-select',
    ) as HTMLSelectElement | null;
    if (chatSessionSelect2) chatSessionSelect2.value = '';
  });

  $('chat-abort-btn')?.addEventListener('click', async () => {
    const key = appState.currentSessionKey ?? 'default';
    try {
      await pawEngine.chatAbort(key);
      showToast('Agent stopped', 'info');
    } catch (e) {
      console.warn('[chat] Abort failed:', e);
    }
  });

  $('session-rename-btn')?.addEventListener('click', async () => {
    if (!appState.currentSessionKey || !appState.wsConnected) return;
    const { promptModal } = await import('../../components/helpers');
    const name = await promptModal('Rename session', 'New name…');
    if (!name) return;
    try {
      await pawEngine.sessionRename(appState.currentSessionKey, name);
      showToast('Session renamed', 'success');
      await loadSessions();
    } catch (e) {
      showToast(`Rename failed: ${e instanceof Error ? e.message : e}`, 'error');
    }
  });

  $('session-delete-btn')?.addEventListener('click', async () => {
    if (!appState.currentSessionKey || !appState.wsConnected) return;
    if (!(await confirmModal('Delete this session? This cannot be undone.'))) return;
    try {
      await pawEngine.sessionDelete(appState.currentSessionKey);
      appState.currentSessionKey = null;
      appState.messages = [];
      renderMessages();
      showToast('Session deleted', 'success');
      await loadSessions();
    } catch (e) {
      showToast(`Delete failed: ${e instanceof Error ? e.message : e}`, 'error');
    }
  });

  $('session-clear-btn')?.addEventListener('click', async () => {
    if (!appState.currentSessionKey || !appState.wsConnected) return;
    if (!(await confirmModal('Clear all messages in this session?'))) return;
    try {
      await pawEngine.sessionClear(appState.currentSessionKey);
      appState.messages = [];
      resetTokenMeter();
      renderMessages();
      showToast('Session history cleared', 'success');
    } catch (e) {
      showToast(`Clear failed: ${e instanceof Error ? e.message : e}`, 'error');
    }
  });

  $('session-compact-btn')?.addEventListener('click', async () => {
    if (!appState.wsConnected || !appState.currentSessionKey) return;
    try {
      const result = await pawEngine.sessionCompact(appState.currentSessionKey);
      showToast(
        `Compacted: ${result.messages_before} → ${result.messages_after} messages`,
        'success',
      );
      resetTokenMeter();
      const ba = document.getElementById('session-budget-alert');
      if (ba) ba.style.display = 'none';
      // Reload compacted history into the UI
      const history = await pawEngine.chatHistory(appState.currentSessionKey, 100);
      appState.messages = history.map((m) => ({
        role: m.role as 'user' | 'assistant' | 'system',
        content: m.content,
        timestamp: new Date(m.created_at),
      }));
      renderMessages();
    } catch (e) {
      showToast(`Compact failed: ${e instanceof Error ? e.message : e}`, 'error');
    }
  });

  $('compaction-warning-dismiss')?.addEventListener('click', () => {
    appState.compactionDismissed = true;
    const warning = document.getElementById('compaction-warning');
    if (warning) warning.style.display = 'none';
  });

  // Talk Mode button in chat input
  $('chat-talk-btn')?.addEventListener('click', () => toggleChatTalkMode());
}

// ═══ Chat Talk Mode ═════════════════════════════════════════════════════
// Quick talk button next to chat input — records one utterance and sends it

let _chatTalkActive = false;
let _chatMediaRecorder: MediaRecorder | null = null;
let _chatAudioStream: MediaStream | null = null;
let _chatTalkTimeout: ReturnType<typeof setTimeout> | null = null;

async function toggleChatTalkMode() {
  if (_chatTalkActive) {
    stopChatTalk();
  } else {
    await startChatTalk();
  }
}

async function startChatTalk() {
  const btn = $('chat-talk-btn');
  if (!btn) return;

  try {
    _chatAudioStream = await navigator.mediaDevices.getUserMedia({
      audio: { echoCancellation: true, noiseSuppression: true, sampleRate: 16000 },
    });

    _chatTalkActive = true;
    btn.innerHTML = `<span class="ms">stop_circle</span>`;
    btn.classList.add('talk-active');
    btn.title = 'Stop recording';

    const mimeType = MediaRecorder.isTypeSupported('audio/webm;codecs=opus')
      ? 'audio/webm;codecs=opus'
      : MediaRecorder.isTypeSupported('audio/webm')
        ? 'audio/webm'
        : 'audio/ogg';

    _chatMediaRecorder = new MediaRecorder(_chatAudioStream, { mimeType });
    const chunks: Blob[] = [];

    _chatMediaRecorder.ondataavailable = (e) => {
      if (e.data.size > 0) chunks.push(e.data);
    };

    _chatMediaRecorder.onstop = async () => {
      cleanupChatTalk();
      if (chunks.length === 0) return;

      const blob = new Blob(chunks, { type: mimeType });
      if (blob.size < 4000) {
        showToast('Recording too short — try again', 'info');
        return;
      }

      btn.innerHTML = `<span class="ms">hourglass_top</span>`;
      btn.title = 'Transcribing...';

      try {
        const reader = new FileReader();
        const base64 = await new Promise<string>((resolve, reject) => {
          reader.onload = () => resolve((reader.result as string).split(',')[1] || '');
          reader.onerror = () => reject(reader.error);
          reader.readAsDataURL(blob);
        });

        const transcript = await pawEngine.ttsTranscribe(base64, mimeType);
        if (transcript.trim()) {
          const chatInput = $('chat-input') as HTMLTextAreaElement | null;
          if (chatInput) {
            chatInput.value = transcript;
            chatInput.style.height = 'auto';
            chatInput.style.height = `${Math.min(chatInput.scrollHeight, 120)}px`;
            chatInput.focus();
          }
        } else {
          showToast('No speech detected — try again', 'info');
        }
      } catch (e) {
        console.error('[talk] Transcription error:', e);
        showToast(`Transcription failed: ${e instanceof Error ? e.message : e}`, 'error');
      } finally {
        btn.innerHTML = `<span class="ms">mic</span>`;
        btn.title = 'Talk Mode — hold to speak';
      }
    };

    _chatMediaRecorder.start();

    // Auto-stop after 30 seconds max
    _chatTalkTimeout = setTimeout(() => {
      _chatTalkTimeout = null;
      if (_chatMediaRecorder && _chatMediaRecorder.state === 'recording') {
        _chatMediaRecorder.stop();
      }
    }, 30_000);
  } catch (e) {
    showToast('Microphone access denied', 'error');
    console.error('[talk] Mic error:', e);
    cleanupChatTalk();
  }
}

function stopChatTalk() {
  if (_chatTalkTimeout) {
    clearTimeout(_chatTalkTimeout);
    _chatTalkTimeout = null;
  }
  if (_chatMediaRecorder && _chatMediaRecorder.state === 'recording') {
    _chatMediaRecorder.stop();
  }
}

function cleanupChatTalk() {
  if (_chatTalkTimeout) {
    clearTimeout(_chatTalkTimeout);
    _chatTalkTimeout = null;
  }
  _chatTalkActive = false;
  _chatMediaRecorder = null;
  if (_chatAudioStream) {
    _chatAudioStream.getTracks().forEach((t) => t.stop());
    _chatAudioStream = null;
  }
  const btn = $('chat-talk-btn');
  if (btn) {
    btn.innerHTML = `<span class="ms">mic</span>`;
    btn.classList.remove('talk-active');
    btn.title = 'Talk Mode — hold to speak';
  }
}
