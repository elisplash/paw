// ─────────────────────────────────────────────────────────────────────────────
// Flow Architect Agent — Molecules
// Panel UI controller: mount/unmount, render, send, streaming, chips.
// ─────────────────────────────────────────────────────────────────────────────

import {
  type FlowAgentMessage,
  type FlowAgentState,
  getDefaultChips,
  makeFlowAgentSessionKey,
  serializeGraphForAgent,
  buildSystemPrompt,
  nextAgentMsgId,
} from './flow-agent-atoms';
import type { FlowGraph } from './atoms';
import { formatMarkdown } from '../../components/molecules/markdown';
import { engineChatSend } from '../../engine/molecules/bridge';
import { subscribeSession, type StreamHandlers } from '../../engine/molecules/event_bus';

// ── Module State ───────────────────────────────────────────────────────────

let _state: FlowAgentState = {
  sessionKey: '',
  messages: [],
  isStreaming: false,
  streamContent: '',
};
let _getGraph: (() => FlowGraph | undefined) | null = null;
let _unsubscribe: (() => void) | null = null;
let _streamingEl: HTMLElement | null = null;
const _rafPending = { value: false };

// ── DOM Helpers ────────────────────────────────────────────────────────────

function el(id: string): HTMLElement | null {
  return document.getElementById(id);
}

function msgContainer(): HTMLElement | null {
  return el('flows-agent-messages');
}

// ── Public API ─────────────────────────────────────────────────────────────

/**
 * Initialize the flow agent module. Call once from flows/index.ts.
 */
export function initFlowAgent(getGraph: () => FlowGraph | undefined): void {
  _getGraph = getGraph;

  // Input handlers
  const input = el('flows-agent-input') as HTMLInputElement | null;
  const sendBtn = el('flows-agent-send');

  if (input) {
    input.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && !e.shiftKey && !_state.isStreaming) {
        e.preventDefault();
        sendUserMessage(input.value.trim());
        input.value = '';
      }
    });
  }

  if (sendBtn && input) {
    sendBtn.addEventListener('click', () => {
      if (!_state.isStreaming) {
        sendUserMessage(input.value.trim());
        input.value = '';
      }
    });
  }

  // Close button
  const closeBtn = el('flows-agent-close');
  if (closeBtn) {
    closeBtn.addEventListener('click', () => {
      toggleFlowAgent(false);
    });
  }
}

/**
 * Toggle the flow agent panel open/closed.
 */
export function toggleFlowAgent(open?: boolean): void {
  const view = el('flows-view');
  if (!view) return;

  const shouldOpen = open ?? !view.classList.contains('flows-agent-open');

  if (shouldOpen) {
    view.classList.add('flows-agent-open');
    // Subscribe to this flow's session
    const graph = _getGraph?.();
    if (graph) {
      switchSession(graph);
    }
    renderChips();
    renderEmptyState();
    // Focus input
    const input = el('flows-agent-input') as HTMLInputElement | null;
    if (input) setTimeout(() => input.focus(), 100);
  } else {
    view.classList.remove('flows-agent-open');
    // Unsubscribe from streaming events
    if (_unsubscribe) {
      _unsubscribe();
      _unsubscribe = null;
    }
  }

  localStorage.setItem('paw-flows-agent-open', String(shouldOpen));
}

/**
 * Check if the agent panel is currently open.
 */
export function isFlowAgentOpen(): boolean {
  return el('flows-view')?.classList.contains('flows-agent-open') ?? false;
}

/**
 * Restore agent panel state from localStorage on mount.
 */
export function restoreFlowAgentState(): void {
  if (localStorage.getItem('paw-flows-agent-open') === 'true') {
    toggleFlowAgent(true);
  }
}

/**
 * Notify the agent module that the active graph changed.
 */
export function onGraphChanged(graph: FlowGraph): void {
  if (isFlowAgentOpen()) {
    switchSession(graph);
    renderChips();
  }
}

/**
 * Clean up on unmount.
 */
export function unmountFlowAgent(): void {
  if (_unsubscribe) {
    _unsubscribe();
    _unsubscribe = null;
  }
  _state = { sessionKey: '', messages: [], isStreaming: false, streamContent: '' };
  _streamingEl = null;
}

// ── Session Management ─────────────────────────────────────────────────────

function switchSession(graph: FlowGraph): void {
  const newKey = makeFlowAgentSessionKey(graph.id);
  if (_state.sessionKey === newKey) return;

  // Unsubscribe from old session
  if (_unsubscribe) {
    _unsubscribe();
    _unsubscribe = null;
  }

  _state = {
    sessionKey: newKey,
    messages: loadMessages(newKey),
    isStreaming: false,
    streamContent: '',
  };

  renderAllMessages();
  subscribeToSession(newKey);
}

function subscribeToSession(sessionKey: string): void {
  const handlers: StreamHandlers = {
    onDelta: (text) => {
      _state.streamContent += text;
      if (_streamingEl) {
        _streamingEl.innerHTML = formatMarkdown(_state.streamContent);
      }
      scrollToBottom();
    },
    onThinking: () => {
      // Could show thinking indicator — skip for now
    },
    onToken: () => {},
    onModel: () => {},
    onStreamEnd: (content) => {
      finalizeStream(content || _state.streamContent);
    },
    onStreamError: (error) => {
      finalizeStream(`Error: ${error}`);
    },
  };

  _unsubscribe = subscribeSession(sessionKey, handlers);
}

// ── Send / Receive ─────────────────────────────────────────────────────────

async function sendUserMessage(content: string): Promise<void> {
  if (!content || _state.isStreaming) return;

  const graph = _getGraph?.();
  if (!graph) return;

  // Ensure session is set
  if (!_state.sessionKey) switchSession(graph);

  // Add user message
  const userMsg: FlowAgentMessage = {
    id: nextAgentMsgId(),
    role: 'user',
    content,
    timestamp: new Date().toISOString(),
  };
  _state.messages.push(userMsg);
  appendMessageEl(userMsg);
  saveMessages(_state.sessionKey, _state.messages);

  // Start streaming state
  _state.isStreaming = true;
  _state.streamContent = '';
  showStreamingPlaceholder();
  updateInputState();

  // Build context and send
  const graphContext = serializeGraphForAgent(graph);
  const systemPrompt = buildSystemPrompt(graphContext);

  try {
    await engineChatSend(_state.sessionKey, content, {
      agentProfile: {
        id: 'flow-architect',
        name: 'Flow Architect',
        systemPrompt,
        personality: { tone: 'professional', initiative: 'proactive', detail: 'concise' },
      },
    });
  } catch (err) {
    finalizeStream(`Error sending message: ${err instanceof Error ? err.message : String(err)}`);
  }
}

function finalizeStream(content: string): void {
  // Remove streaming placeholder
  const container = msgContainer();
  const streamEl = container?.querySelector('#streaming-message');
  if (streamEl) streamEl.remove();
  _streamingEl = null;

  // Add assistant message
  const assistantMsg: FlowAgentMessage = {
    id: nextAgentMsgId(),
    role: 'assistant',
    content,
    timestamp: new Date().toISOString(),
  };
  _state.messages.push(assistantMsg);
  appendMessageEl(assistantMsg);
  saveMessages(_state.sessionKey, _state.messages);

  _state.isStreaming = false;
  _state.streamContent = '';
  updateInputState();
  scrollToBottom();
}

// ── Rendering ──────────────────────────────────────────────────────────────

function renderAllMessages(): void {
  const container = msgContainer();
  if (!container) return;

  // Clear
  container.innerHTML = '';

  if (_state.messages.length === 0) {
    renderEmptyState();
    return;
  }

  // Remove empty state if present
  const empty = container.querySelector('.flows-agent-empty');
  if (empty) empty.remove();

  for (const msg of _state.messages) {
    appendMessageEl(msg);
  }
  scrollToBottom();
}

function renderEmptyState(): void {
  const container = msgContainer();
  if (!container) return;

  // Only show if no messages
  if (_state.messages.length > 0) return;
  if (container.querySelector('.flows-agent-empty')) return;

  const empty = document.createElement('div');
  empty.className = 'flows-agent-empty';
  empty.innerHTML = `
    <span class="ms">smart_toy</span>
    <div class="flows-agent-empty-text">
      I'm your <strong>Flow Architect</strong>. Ask me to explain this flow, suggest optimizations, or build something new.
    </div>
  `;
  container.appendChild(empty);
}

function appendMessageEl(msg: FlowAgentMessage): void {
  const container = msgContainer();
  if (!container) return;

  // Remove empty state
  const empty = container.querySelector('.flows-agent-empty');
  if (empty) empty.remove();

  const div = document.createElement('div');
  div.className = `message ${msg.role}`;

  const contentEl = document.createElement('div');
  contentEl.className = 'message-content';

  if (msg.role === 'assistant') {
    const prefix = document.createElement('span');
    prefix.className = 'message-prefix';
    prefix.textContent = 'ARCHITECT ›';
    contentEl.appendChild(prefix);

    const body = document.createElement('span');
    body.innerHTML = formatMarkdown(msg.content);
    contentEl.appendChild(body);
  } else {
    contentEl.textContent = msg.content;
  }

  const time = document.createElement('div');
  time.className = 'message-time';
  time.textContent = new Date(msg.timestamp).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });

  div.appendChild(contentEl);
  div.appendChild(time);
  container.appendChild(div);
  scrollToBottom();
}

function showStreamingPlaceholder(): void {
  const container = msgContainer();
  if (!container) return;

  const div = document.createElement('div');
  div.className = 'message assistant';
  div.id = 'streaming-message';

  const contentEl = document.createElement('div');
  contentEl.className = 'message-content';

  const prefix = document.createElement('span');
  prefix.className = 'message-prefix';
  prefix.textContent = 'ARCHITECT ›';
  contentEl.appendChild(prefix);

  const streamSpan = document.createElement('span');
  streamSpan.innerHTML = '<div class="loading-dots"><span></span><span></span><span></span></div>';
  contentEl.appendChild(streamSpan);

  div.appendChild(contentEl);
  container.appendChild(div);
  _streamingEl = contentEl;
  scrollToBottom();
}

function renderChips(): void {
  const container = el('flows-agent-chips');
  if (!container) return;

  const graph = _getGraph?.();
  const chips = getDefaultChips(graph);

  container.innerHTML = chips
    .map(
      (c) =>
        `<button class="flows-agent-chip" data-prompt="${c.prompt.replace(/"/g, '&quot;')}">
        <span class="ms">${c.icon}</span>${c.label}
      </button>`,
    )
    .join('');

  container.querySelectorAll('.flows-agent-chip').forEach((btn) => {
    btn.addEventListener('click', () => {
      const prompt = (btn as HTMLElement).dataset.prompt;
      if (prompt && !_state.isStreaming) {
        const input = el('flows-agent-input') as HTMLInputElement | null;
        if (input) input.value = '';
        sendUserMessage(prompt);
      }
    });
  });
}

function updateInputState(): void {
  const input = el('flows-agent-input') as HTMLInputElement | null;
  const sendBtn = el('flows-agent-send') as HTMLButtonElement | null;

  if (input) input.disabled = _state.isStreaming;
  if (sendBtn) sendBtn.disabled = _state.isStreaming;
}

function scrollToBottom(): void {
  const container = msgContainer();
  if (!container || _rafPending.value) return;
  _rafPending.value = true;
  requestAnimationFrame(() => {
    if (container) container.scrollTop = container.scrollHeight;
    _rafPending.value = false;
  });
}

// ── Persistence ────────────────────────────────────────────────────────────

const MAX_STORED_MESSAGES = 50;

function saveMessages(sessionKey: string, messages: FlowAgentMessage[]): void {
  try {
    const trimmed = messages.slice(-MAX_STORED_MESSAGES);
    localStorage.setItem(`paw-flow-agent-${sessionKey}`, JSON.stringify(trimmed));
  } catch {
    /* storage full — ignore */
  }
}

function loadMessages(sessionKey: string): FlowAgentMessage[] {
  try {
    const raw = localStorage.getItem(`paw-flow-agent-${sessionKey}`);
    if (raw) return JSON.parse(raw) as FlowAgentMessage[];
  } catch {
    /* corrupt data — ignore */
  }
  return [];
}
