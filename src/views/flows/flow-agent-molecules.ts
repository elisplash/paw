// ─────────────────────────────────────────────────────────────────────────────
// Flow Architect Agent — Molecules
// Panel UI controller: mount/unmount, render, send, streaming, chips.
// ─────────────────────────────────────────────────────────────────────────────

import {
  type FlowAgentMessage,
  type FlowAgentState,
  type FlowAgentToolUse,
  type ThinkingLevel,
  getDefaultChips,
  makeFlowAgentSessionKey,
  serializeGraphForAgent,
  buildSystemPrompt,
  nextAgentMsgId,
} from './flow-agent-atoms';
import type { FlowGraph } from './atoms';
import type { Agent } from '../agents/atoms';
import { formatMarkdown } from '../../components/molecules/markdown';
import { engineChatSend } from '../../engine/molecules/bridge';
import { subscribeSession, type StreamHandlers } from '../../engine/molecules/event_bus';
import { refreshAvailableModels } from '../agents/helpers';
import { tesseractPlaceholder, activateTesseracts } from '../../components/tesseract';

// ── Module State ───────────────────────────────────────────────────────────

const EMPTY_STATE: FlowAgentState = {
  sessionKey: '',
  messages: [],
  isStreaming: false,
  streamContent: '',
  streamThinking: '',
  streamTools: [],
  selectedAgentId: null,
  selectedModel: null,
  thinkingLevel: 'off',
};

let _state: FlowAgentState = { ...EMPTY_STATE };
let _getGraph: (() => FlowGraph | undefined) | null = null;
let _unsubscribe: (() => void) | null = null;
let _streamingEl: HTMLElement | null = null;
const _rafPending = { value: false };
let _availableModels: { id: string; name: string }[] = [];
let _userAgents: Agent[] = [];

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
      if (_state.isStreaming) {
        stopStreaming();
      } else {
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

  // ── Controls ──

  // Agent selector
  const agentSelect = el('flows-agent-agent-select') as HTMLSelectElement | null;
  if (agentSelect) {
    agentSelect.addEventListener('change', () => {
      _state.selectedAgentId = agentSelect.value || null;
      persistControlState();
      updateHeaderTitle();
    });
  }

  // Model selector
  const modelSelect = el('flows-agent-model-select') as HTMLSelectElement | null;
  if (modelSelect) {
    modelSelect.addEventListener('change', () => {
      _state.selectedModel = modelSelect.value || null;
      persistControlState();
    });
  }

  // Thinking level toggle
  const thinkBtn = el('flows-agent-thinking-btn');
  if (thinkBtn) {
    thinkBtn.addEventListener('click', () => {
      cycleThinkingLevel();
    });
  }

  // Clear / new session
  const clearBtn = el('flows-agent-clear-btn');
  if (clearBtn) {
    clearBtn.addEventListener('click', () => {
      clearConversation();
    });
  }

  // Load persisted control state
  restoreControlState();
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
    // Populate agent & model selectors
    populateAgentSelector();
    populateModelSelector();
    updateHeaderTitle();
    syncThinkingButton();
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
  _state = { ...EMPTY_STATE };
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
    ...EMPTY_STATE,
    sessionKey: newKey,
    messages: loadMessages(newKey),
    selectedAgentId: _state.selectedAgentId,
    selectedModel: _state.selectedModel,
    thinkingLevel: _state.thinkingLevel,
  };

  renderAllMessages();
  subscribeToSession(newKey);
}

function subscribeToSession(sessionKey: string): void {
  const handlers: StreamHandlers = {
    onDelta: (text) => {
      _state.streamContent += text;
      if (_streamingEl) {
        renderStreamContent();
      }
      scrollToBottom();
    },
    onThinking: (text) => {
      _state.streamThinking += text;
      if (_streamingEl) {
        renderStreamThinking();
      }
      scrollToBottom();
    },
    onToken: () => {},
    onModel: () => {},
    onStreamEnd: (content) => {
      finalizeStream(content || _state.streamContent);
    },
    onStreamError: (error) => {
      finalizeStream(`Error: ${error}`);
    },
    onToolStart: (toolName) => {
      const tool: FlowAgentToolUse = {
        name: toolName,
        status: 'running',
        startedAt: new Date().toISOString(),
      };
      _state.streamTools.push(tool);
      if (_streamingEl) {
        appendToolBlock(tool);
      }
      scrollToBottom();
    },
    onToolEnd: (toolName) => {
      const tool = _state.streamTools.find((t) => t.name === toolName && t.status === 'running');
      if (tool) {
        tool.status = 'done';
        tool.endedAt = new Date().toISOString();
        updateToolBlock(tool);
      }
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
  _state.streamThinking = '';
  _state.streamTools = [];
  showStreamingPlaceholder();
  updateInputState();

  // Build context and send
  const graphContext = serializeGraphForAgent(graph);
  const systemPrompt = buildSystemPrompt(graphContext);

  // Resolve agent profile
  const agentProfile = resolveAgentProfile(systemPrompt);

  // Build options
  const opts: Record<string, unknown> = { agentProfile };
  if (_state.selectedModel) {
    opts.model = _state.selectedModel;
  }
  if (_state.thinkingLevel !== 'off') {
    opts.thinkingLevel = _state.thinkingLevel;
  }

  try {
    await engineChatSend(_state.sessionKey, content, opts);
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

  // Add assistant message with thinking & tool data
  const assistantMsg: FlowAgentMessage = {
    id: nextAgentMsgId(),
    role: 'assistant',
    content,
    timestamp: new Date().toISOString(),
    thinking: _state.streamThinking || undefined,
    tools: _state.streamTools.length > 0 ? [..._state.streamTools] : undefined,
  };
  _state.messages.push(assistantMsg);
  appendMessageEl(assistantMsg);
  saveMessages(_state.sessionKey, _state.messages);

  _state.isStreaming = false;
  _state.streamContent = '';
  _state.streamThinking = '';
  _state.streamTools = [];
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

  // Thinking block (if present)
  if (msg.thinking) {
    const thinkBlock = createThinkingBlock(msg.thinking);
    div.appendChild(thinkBlock);
  }

  // Tool use blocks (if present)
  if (msg.tools && msg.tools.length > 0) {
    for (const tool of msg.tools) {
      const toolBlock = createToolBlockEl(tool);
      div.appendChild(toolBlock);
    }
  }

  const contentEl = document.createElement('div');
  contentEl.className = 'message-content';

  if (msg.role === 'assistant') {
    const prefix = document.createElement('span');
    prefix.className = 'message-prefix';
    prefix.textContent = getAgentLabel();
    div.appendChild(prefix);

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
  prefix.textContent = getAgentLabel();
  div.appendChild(prefix);

  const streamSpan = document.createElement('span');
  streamSpan.className = 'stream-text';
  streamSpan.innerHTML = tesseractPlaceholder(20, 'streaming');
  contentEl.appendChild(streamSpan);
  activateTesseracts(streamSpan);

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
  if (sendBtn) {
    sendBtn.disabled = false; // Always clickable — toggles between send/stop
    const icon = sendBtn.querySelector('.ms');
    if (_state.isStreaming) {
      sendBtn.classList.add('streaming');
      sendBtn.title = 'Stop';
      if (icon) icon.textContent = 'stop';
    } else {
      sendBtn.classList.remove('streaming');
      sendBtn.title = 'Send';
      if (icon) icon.textContent = 'send';
    }
  }
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

// ── Agent Profile Resolution ───────────────────────────────────────────────

function resolveAgentProfile(systemPrompt: string): Record<string, unknown> {
  if (_state.selectedAgentId) {
    const agent = _userAgents.find((a) => a.id === _state.selectedAgentId);
    if (agent) {
      return {
        id: agent.id,
        name: agent.name,
        systemPrompt: systemPrompt + (agent.systemPrompt ? `\n\n${agent.systemPrompt}` : ''),
        model: agent.model || undefined,
        personality: agent.personality,
        boundaries: agent.boundaries,
        autoApproveAll: agent.autoApproveAll,
      };
    }
  }
  // Default Flow Architect
  return {
    id: 'flow-architect',
    name: 'Flow Architect',
    systemPrompt,
    personality: { tone: 'professional', initiative: 'proactive', detail: 'concise' },
  };
}

function getAgentLabel(): string {
  if (_state.selectedAgentId) {
    const agent = _userAgents.find((a) => a.id === _state.selectedAgentId);
    if (agent) return agent.name.toUpperCase();
  }
  return 'ARCHITECT';
}

// ── Stop Streaming ─────────────────────────────────────────────────────────

function stopStreaming(): void {
  if (!_state.isStreaming) return;
  // Unsubscribe cancels listening; finalize whatever we have
  if (_unsubscribe) {
    _unsubscribe();
    _unsubscribe = null;
  }
  finalizeStream(_state.streamContent || '(stopped)');
  // Re-subscribe for future messages
  if (_state.sessionKey) {
    subscribeToSession(_state.sessionKey);
  }
}

// ── Clear Conversation ─────────────────────────────────────────────────────

function clearConversation(): void {
  if (_state.isStreaming) stopStreaming();

  // Clear stored messages
  if (_state.sessionKey) {
    localStorage.removeItem(`paw-flow-agent-${_state.sessionKey}`);
  }

  // Reset state but keep control settings
  _state.messages = [];
  _state.streamContent = '';
  _state.streamThinking = '';
  _state.streamTools = [];

  // Generate new session key to avoid backend history collision
  const graph = _getGraph?.();
  if (graph) {
    const newKey = `flow-architect-${graph.id}-${Date.now()}`;
    _state.sessionKey = newKey;
    if (_unsubscribe) {
      _unsubscribe();
      _unsubscribe = null;
    }
    subscribeToSession(newKey);
  }

  renderAllMessages();
}

// ── Thinking Level ─────────────────────────────────────────────────────────

const THINKING_LEVELS: ThinkingLevel[] = ['off', 'low', 'medium', 'high'];

function cycleThinkingLevel(): void {
  const idx = THINKING_LEVELS.indexOf(_state.thinkingLevel);
  _state.thinkingLevel = THINKING_LEVELS[(idx + 1) % THINKING_LEVELS.length];
  syncThinkingButton();
  persistControlState();
}

function syncThinkingButton(): void {
  const btn = el('flows-agent-thinking-btn');
  if (!btn) return;

  const label = btn.querySelector('.flows-agent-thinking-label');
  if (label) label.textContent = _state.thinkingLevel === 'off' ? 'Off' : _state.thinkingLevel;

  if (_state.thinkingLevel !== 'off') {
    btn.classList.add('active');
    btn.title = `Thinking level: ${_state.thinkingLevel}`;
  } else {
    btn.classList.remove('active');
    btn.title = 'Thinking level: Off';
  }
}

// ── Agent Selector ─────────────────────────────────────────────────────────

function populateAgentSelector(): void {
  const select = el('flows-agent-agent-select') as HTMLSelectElement | null;
  if (!select) return;

  // Load agents from localStorage
  try {
    const raw = localStorage.getItem('paw-agents');
    _userAgents = raw ? (JSON.parse(raw) as Agent[]) : [];
  } catch {
    _userAgents = [];
  }

  select.innerHTML = '<option value="">Flow Architect</option>';
  for (const agent of _userAgents) {
    const opt = document.createElement('option');
    opt.value = agent.id;
    opt.textContent = agent.name;
    if (agent.id === _state.selectedAgentId) opt.selected = true;
    select.appendChild(opt);
  }
}

function updateHeaderTitle(): void {
  const title = document.querySelector('.flows-agent-title') as HTMLElement | null;
  const icon = document.querySelector('.flows-agent-icon') as HTMLElement | null;
  if (!title) return;

  if (_state.selectedAgentId) {
    const agent = _userAgents.find((a) => a.id === _state.selectedAgentId);
    if (agent) {
      title.textContent = agent.name;
      if (icon) icon.textContent = 'person';
      return;
    }
  }
  title.textContent = 'Flow Architect';
  if (icon) icon.textContent = 'smart_toy';
}

// ── Model Selector ─────────────────────────────────────────────────────────

async function populateModelSelector(): Promise<void> {
  const select = el('flows-agent-model-select') as HTMLSelectElement | null;
  if (!select) return;

  // If we haven't loaded models yet, fetch them
  if (_availableModels.length === 0) {
    try {
      _availableModels = await refreshAvailableModels();
    } catch {
      _availableModels = [{ id: 'default', name: 'Default (Use account setting)' }];
    }
  }

  select.innerHTML = '<option value="">Default model</option>';
  for (const model of _availableModels) {
    if (model.id === 'default') continue; // skip — already the empty option
    const opt = document.createElement('option');
    opt.value = model.id;
    opt.textContent = model.name;
    if (model.id === _state.selectedModel) opt.selected = true;
    select.appendChild(opt);
  }
}

// ── Streaming Render Helpers ───────────────────────────────────────────────

function renderStreamContent(): void {
  if (!_streamingEl) return;
  // Find or create the text span (skip thinking/tool blocks)
  let textSpan = _streamingEl.querySelector('.stream-text') as HTMLElement | null;
  if (!textSpan) {
    // First content delta — replace loading dots
    _streamingEl.innerHTML = '';
    const prefix = document.createElement('span');
    prefix.className = 'message-prefix';
    prefix.textContent = getAgentLabel();
    _streamingEl.appendChild(prefix);
    textSpan = document.createElement('span');
    textSpan.className = 'stream-text';
    _streamingEl.appendChild(textSpan);
  }
  textSpan.innerHTML = formatMarkdown(_state.streamContent);
}

function renderStreamThinking(): void {
  if (!_streamingEl) return;
  const parent = _streamingEl.parentElement;
  if (!parent) return;

  // Find or create the thinking block above content
  let thinkBlock = parent.querySelector('.flows-agent-thinking-block') as HTMLElement | null;
  if (!thinkBlock) {
    thinkBlock = createThinkingBlock(_state.streamThinking, true);
    parent.insertBefore(thinkBlock, _streamingEl);
  } else {
    const content = thinkBlock.querySelector('.flows-agent-thinking-content');
    if (content) content.textContent = _state.streamThinking;
  }
}

// ── Thinking Block Element ─────────────────────────────────────────────────

function createThinkingBlock(text: string, startOpen = false): HTMLElement {
  const block = document.createElement('div');
  block.className = `flows-agent-thinking-block${startOpen ? ' open' : ''}`;

  const toggle = document.createElement('button');
  toggle.className = 'flows-agent-thinking-toggle';
  toggle.innerHTML = '<span class="ms">chevron_right</span> Thinking…';
  toggle.addEventListener('click', () => {
    block.classList.toggle('open');
  });

  const content = document.createElement('div');
  content.className = 'flows-agent-thinking-content';
  content.textContent = text;

  block.appendChild(toggle);
  block.appendChild(content);
  return block;
}

// ── Tool Block Elements ────────────────────────────────────────────────────

function createToolBlockEl(tool: FlowAgentToolUse): HTMLElement {
  const block = document.createElement('div');
  block.className = `flows-agent-tool-block ${tool.status}`;
  block.dataset.toolName = tool.name;
  block.innerHTML = `
    <span class="ms">${tool.status === 'running' ? 'sync' : 'check_circle'}</span>
    <span class="flows-agent-tool-name">${escapeHtml(tool.name)}</span>
  `;
  return block;
}

function appendToolBlock(tool: FlowAgentToolUse): void {
  const parent = _streamingEl?.parentElement;
  if (!parent) return;
  const block = createToolBlockEl(tool);
  parent.insertBefore(block, _streamingEl);
}

function updateToolBlock(tool: FlowAgentToolUse): void {
  const parent = _streamingEl?.parentElement;
  if (!parent) return;
  const blocks = parent.querySelectorAll(`.flows-agent-tool-block[data-tool-name="${tool.name}"]`);
  const block = blocks[blocks.length - 1]; // last matching
  if (block) {
    block.className = `flows-agent-tool-block ${tool.status}`;
    const icon = block.querySelector('.ms');
    if (icon) icon.textContent = 'check_circle';
  }
}

function escapeHtml(str: string): string {
  return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

// ── Control State Persistence ──────────────────────────────────────────────

function persistControlState(): void {
  try {
    localStorage.setItem(
      'paw-flow-agent-controls',
      JSON.stringify({
        selectedAgentId: _state.selectedAgentId,
        selectedModel: _state.selectedModel,
        thinkingLevel: _state.thinkingLevel,
      }),
    );
  } catch {
    /* ignore */
  }
}

function restoreControlState(): void {
  try {
    const raw = localStorage.getItem('paw-flow-agent-controls');
    if (raw) {
      const data = JSON.parse(raw) as {
        selectedAgentId?: string | null;
        selectedModel?: string | null;
        thinkingLevel?: ThinkingLevel;
      };
      _state.selectedAgentId = data.selectedAgentId ?? null;
      _state.selectedModel = data.selectedModel ?? null;
      _state.thinkingLevel = data.thinkingLevel ?? 'off';
    }
  } catch {
    /* ignore */
  }
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
