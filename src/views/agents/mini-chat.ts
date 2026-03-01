// mini-chat.ts — FB Messenger–style mini-chat popup windows
// Depends on: atoms, molecules (badge helpers), helpers, toast, engine

import { pawEngine } from '../../engine';
import { escHtml, escAttr } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { type Agent, spriteAvatar } from './atoms';
import { updateDockBadge, updateDockActive } from './molecules';

// ═══ Mini-Chat Popup System ═══════════════════════════════════════════════

interface MiniChatWindow {
  agentId: string;
  agent: Agent;
  sessionId: string | null;
  el: HTMLElement;
  messagesEl: HTMLElement;
  inputEl: HTMLInputElement;
  isMinimized: boolean;
  isStreaming: boolean;
  streamingContent: string;
  streamingEl: HTMLElement | null;
  runId: string | null;
  unreadCount: number;
  unlistenDelta: (() => void) | null;
  unlistenComplete: (() => void) | null;
  unlistenError: (() => void) | null;
}

export const _miniChats: Map<string, MiniChatWindow> = new Map();

const MINI_CHAT_WIDTH = 320;
const MINI_CHAT_GAP = 12;
const DOCK_RESERVED = 72; /* 48px dock + 12px right margin + 12px gap */

function getMiniChatOffset(index: number): number {
  return DOCK_RESERVED + index * (MINI_CHAT_WIDTH + MINI_CHAT_GAP);
}

function repositionMiniChats() {
  let idx = 0;
  for (const mc of _miniChats.values()) {
    mc.el.style.right = `${getMiniChatOffset(idx)}px`;
    idx++;
  }
}

export function openMiniChat(agentId: string, getAgentsFn: () => Agent[]) {
  // If already open, just un-minimize
  const existing = _miniChats.get(agentId);
  if (existing) {
    if (existing.isMinimized) toggleMinimizeMiniChat(agentId);
    existing.inputEl.focus();
    return;
  }

  const agent = getAgentsFn().find((a) => a.id === agentId);
  if (!agent) return;

  const el = document.createElement('div');
  el.className = 'mini-chat';
  el.style.right = `${getMiniChatOffset(_miniChats.size)}px`;
  el.innerHTML = `
    <div class="mini-chat-header" style="background:${agent.color}">
      <div class="mini-chat-avatar">${spriteAvatar(agent.avatar, 24)}</div>
      <div class="mini-chat-name">${escHtml(agent.name)}</div>
      <div class="mini-chat-controls">
        <button class="mini-chat-btn mini-chat-minimize" title="Minimize">—</button>
        <button class="mini-chat-btn mini-chat-close" title="Close">×</button>
      </div>
    </div>
    <div class="mini-chat-body">
      <div class="mini-chat-messages"></div>
      <div class="mini-chat-input-row">
        <input type="text" class="mini-chat-input" placeholder="Message ${escAttr(agent.name)}…">
        <button class="mini-chat-send"><span class="ms ms-sm">send</span></button>
      </div>
    </div>
  `;
  document.body.appendChild(el);

  const messagesEl = el.querySelector('.mini-chat-messages') as HTMLElement;
  const inputEl = el.querySelector('.mini-chat-input') as HTMLInputElement;

  const mc: MiniChatWindow = {
    agentId,
    agent,
    sessionId: null,
    el,
    messagesEl,
    inputEl,
    isMinimized: false,
    isStreaming: false,
    streamingContent: '',
    streamingEl: null,
    runId: null,
    unreadCount: 0,
    unlistenDelta: null,
    unlistenComplete: null,
    unlistenError: null,
  };
  _miniChats.set(agentId, mc);

  // Set up engine event listeners for this chat
  setupMiniChatListeners(mc);

  // Header drag/minimize
  el.querySelector('.mini-chat-minimize')?.addEventListener('click', () =>
    toggleMinimizeMiniChat(agentId),
  );
  el.querySelector('.mini-chat-close')?.addEventListener('click', () => closeMiniChat(agentId));
  el.querySelector('.mini-chat-header')?.addEventListener('dblclick', () =>
    toggleMinimizeMiniChat(agentId),
  );

  // Send on Enter or button
  inputEl.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendMiniChatMessage(agentId);
    }
  });
  el.querySelector('.mini-chat-send')?.addEventListener('click', () =>
    sendMiniChatMessage(agentId),
  );

  // Animate in
  requestAnimationFrame(() => el.classList.add('mini-chat-visible'));
  updateDockActive(agentId, true);
  updateDockBadge(agentId, 0);
  inputEl.focus();
}

/** Lightweight markdown → HTML for mini-chat bubbles (bold, italic, code, links) */
function miniChatMd(raw: string): string {
  let s = escHtml(raw);
  // Code blocks: ```...```
  s = s.replace(/```([\s\S]*?)```/g, '<pre class="mini-chat-code">$1</pre>');
  // Inline code: `...`
  s = s.replace(/`([^`]+)`/g, '<code>$1</code>');
  // Bold: **...**
  s = s.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
  // Italic: *...* (bold already replaced above, so single * is safe)
  s = s.replace(/\*(.+?)\*/g, '<em>$1</em>');
  // Links: [text](url)
  s = s.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" target="_blank" rel="noopener">$1</a>');
  // Newlines
  s = s.replace(/\n/g, '<br>');
  return s;
}

function setupMiniChatListeners(mc: MiniChatWindow) {
  // Listen for delta events
  mc.unlistenDelta = pawEngine.on('delta', (event) => {
    if (!mc.runId || event.run_id !== mc.runId) return;
    mc.streamingContent += event.text || '';
    if (mc.streamingEl) {
      // During streaming: plain text for speed, markdown on finalize
      mc.streamingEl.textContent = mc.streamingContent;
      mc.messagesEl.scrollTop = mc.messagesEl.scrollHeight;
    }
  });

  // Listen for completion — only finalize on the FINAL completion (no pending tool calls).
  // Intermediate completions (tool_calls_count > 0) mean the agent is still working
  // through tool rounds and more deltas/responses will follow.
  mc.unlistenComplete = pawEngine.on('complete', (event) => {
    if (!mc.runId || event.run_id !== mc.runId) return;
    if (event.tool_calls_count && event.tool_calls_count > 0) return;
    finalizeMiniChatStreaming(mc);
  });

  // Listen for errors
  mc.unlistenError = pawEngine.on('error', (event) => {
    if (!mc.runId || event.run_id !== mc.runId) return;
    mc.streamingContent += `\nError: ${event.message || 'Error'}`;
    finalizeMiniChatStreaming(mc);
  });
}

function finalizeMiniChatStreaming(mc: MiniChatWindow) {
  mc.isStreaming = false;
  mc.runId = null;
  if (mc.streamingEl) {
    // Render final content with markdown formatting
    mc.streamingEl.innerHTML = miniChatMd(mc.streamingContent);
    mc.streamingEl.classList.remove('mini-chat-streaming');
    mc.streamingEl = null;
  }
  // Track unread when minimized
  if (mc.isMinimized) {
    mc.unreadCount++;
    updateMiniChatBadge(mc);
    updateDockBadge(mc.agentId, mc.unreadCount);
  }
  mc.inputEl.disabled = false;
  mc.inputEl.focus();
}

async function sendMiniChatMessage(agentId: string) {
  const mc = _miniChats.get(agentId);
  if (!mc || mc.isStreaming) return;

  const text = mc.inputEl.value.trim();
  if (!text) return;

  mc.inputEl.value = '';
  mc.isStreaming = true;
  mc.streamingContent = '';
  mc.inputEl.disabled = true;

  // Add user message bubble
  const userBubble = document.createElement('div');
  userBubble.className = 'mini-chat-msg mini-chat-msg-user';
  userBubble.textContent = text;
  mc.messagesEl.appendChild(userBubble);

  // Add assistant streaming bubble
  const asstBubble = document.createElement('div');
  asstBubble.className = 'mini-chat-msg mini-chat-msg-assistant mini-chat-streaming';
  asstBubble.innerHTML = '<span class="mini-chat-dots">···</span>';
  mc.messagesEl.appendChild(asstBubble);
  mc.streamingEl = asstBubble;
  mc.messagesEl.scrollTop = mc.messagesEl.scrollHeight;

  try {
    // Build agent system prompt
    const parts: string[] = [];
    if (mc.agent.name) parts.push(`You are ${mc.agent.name}.`);
    if (mc.agent.bio) parts.push(mc.agent.bio);
    if (mc.agent.systemPrompt) parts.push(mc.agent.systemPrompt);
    const systemPrompt = parts.length > 0 ? parts.join(' ') : undefined;

    const resolvedModel =
      mc.agent.model && mc.agent.model !== 'default' ? mc.agent.model : undefined;

    const request = {
      session_id: mc.sessionId || undefined,
      message: text,
      model: resolvedModel,
      system_prompt: systemPrompt,
      tools_enabled: true,
      agent_id: mc.agentId,
    };

    const result = await pawEngine.chatSend(request);
    mc.runId = result.run_id;
    mc.sessionId = result.session_id;
  } catch (e) {
    console.error('[mini-chat] Send error:', e);
    asstBubble.textContent = `Error: ${e instanceof Error ? e.message : 'Failed to send'}`;
    asstBubble.classList.remove('mini-chat-streaming');
    mc.isStreaming = false;
    mc.streamingEl = null;
    mc.inputEl.disabled = false;
  }
}

function toggleMinimizeMiniChat(agentId: string) {
  const mc = _miniChats.get(agentId);
  if (!mc) return;
  mc.isMinimized = !mc.isMinimized;
  mc.el.classList.toggle('mini-chat-minimized', mc.isMinimized);
  if (!mc.isMinimized) {
    mc.unreadCount = 0;
    updateMiniChatBadge(mc);
    updateDockBadge(agentId, 0);
    mc.messagesEl.scrollTop = mc.messagesEl.scrollHeight;
  }
}

export function closeMiniChat(agentId: string) {
  const mc = _miniChats.get(agentId);
  if (!mc) return;
  // Cleanup listeners
  mc.unlistenDelta?.();
  mc.unlistenComplete?.();
  mc.unlistenError?.();
  mc.el.classList.remove('mini-chat-visible');
  setTimeout(() => mc.el.remove(), 200);
  _miniChats.delete(agentId);
  repositionMiniChats();
  updateDockBadge(agentId, 0);
  updateDockActive(agentId, false);
}

// ── Badge helpers ────────────────────────────────────────────────────────

/** Update the header badge inside a mini-chat window */
function updateMiniChatBadge(mc: MiniChatWindow) {
  let badge = mc.el.querySelector('.mini-chat-unread') as HTMLElement | null;
  if (mc.unreadCount > 0) {
    if (!badge) {
      badge = document.createElement('span');
      badge.className = 'mini-chat-unread';
      mc.el.querySelector('.mini-chat-name')?.appendChild(badge);
    }
    badge.textContent = mc.unreadCount > 9 ? '9+' : String(mc.unreadCount);
    badge.style.display = '';
  } else if (badge) {
    badge.style.display = 'none';
  }
}

/** Show/hide a toast for mini-chat errors (re-export for symmetry) */
export { showToast as miniChatToast };
