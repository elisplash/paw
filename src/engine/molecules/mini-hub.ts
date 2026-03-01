// src/engine/molecules/mini-hub.ts
// Phase 3.2 — Mini-Hub Builder molecule.
// Factory function that constructs a complete mini-hub DOM tree and returns
// a MiniHubController. Composes chat_renderer + chat_input molecules.

import { buildSquadAgentMap, type MiniHubConfig, type MiniHubController } from '../atoms/mini-hub';
import type { MessageWithAttachments } from '../../state/index';
import { icon, populateModelSelect } from '../../components/helpers';
import { spriteAvatar } from '../../views/agents/atoms';
import {
  renderMessages,
  renderSingleMessage,
  showStreamingMessage,
  appendStreamingDelta,
  appendThinkingDelta,
  scrollToBottom,
  type RenderOpts,
} from './chat_renderer';
import { createChatInput, type ChatInputController } from './chat_input';
import { createTalkMode, type TalkModeController } from './tts';
import { findLastIndex } from '../atoms/chat';

// ── Constants ────────────────────────────────────────────────────────────

const HUB_MIN_WIDTH = 320;
const HUB_DEFAULT_WIDTH = 360;
const HUB_DEFAULT_HEIGHT = 500;

// ── Factory ──────────────────────────────────────────────────────────────

/**
 * Create a mini-hub instance with fully self-contained DOM.
 * The caller is responsible for:
 *   - Appending `controller.el` to the DOM
 *   - Wiring `onSend` / `onClose` / `onMaximize` callbacks
 *   - Subscribing to the event bus and forwarding deltas
 *
 * @returns MiniHubController
 */
export function createMiniHub(
  config: MiniHubConfig,
  callbacks: {
    /** Called when user hits send. Consumer should call engineChatSend(). */
    onSend: (hubId: string, content: string, attachments: File[]) => void;
    /** Called when user clicks close (✕). Consumer should clean up state. */
    onClose: (hubId: string) => void;
    /** Called when user clicks maximize (□). Consumer should switch main view. */
    onMaximize: (hubId: string) => void;
    /** Called when position changes (drag). Consumer should persist. */
    onPositionChange?: (hubId: string, pos: { x: number; y: number }) => void;
    /** Called when model selection changes. */
    onModelChange?: (hubId: string, model: string) => void;
  },
): MiniHubController {
  let sessionKey: string | null = config.sessionKey ?? null;
  let messages: MessageWithAttachments[] = [];
  let minimized = false;
  let unreadCount = 0;
  let destroyed = false;
  let streamingEl: HTMLElement | null = null;
  let streamingContent = '';
  let thinkingContent = '';
  let currentModel = config.modelOverride ?? '';
  let position = config.position ?? { x: 100, y: 100 };
  let streamingActive = false;
  const rafPending = { value: false };

  // ── Build DOM ──────────────────────────────────────────────────────────

  const root = document.createElement('div');
  root.className = 'mini-hub';
  root.dataset.hubId = config.hubId;
  root.style.width = `${HUB_DEFAULT_WIDTH}px`;
  root.style.height = `${HUB_DEFAULT_HEIGHT}px`;
  root.style.position = 'fixed';
  root.style.left = `${position.x}px`;
  root.style.top = `${position.y}px`;
  root.style.zIndex = '9000';

  // ── Titlebar ─────────────────────────────────────────────────────────

  const titlebar = document.createElement('div');
  titlebar.className = 'mini-hub-titlebar';
  if (config.agentColor) {
    titlebar.style.background = config.agentColor;
  }

  const avatarSpan = document.createElement('span');
  avatarSpan.className = 'mini-hub-avatar';
  avatarSpan.innerHTML = spriteAvatar(config.agentAvatar ?? '🤖', 16);
  titlebar.appendChild(avatarSpan);

  const titleSpan = document.createElement('span');
  titleSpan.className = 'mini-hub-title';
  titleSpan.textContent = config.agentName;
  titlebar.appendChild(titleSpan);

  // Streaming indicator dot (hidden initially)
  const streamingDot = document.createElement('span');
  streamingDot.className = 'mini-hub-streaming-dot';
  streamingDot.title = 'Agent is working…';
  titlebar.appendChild(streamingDot);

  // Unread badge (hidden initially)
  const unreadBadge = document.createElement('span');
  unreadBadge.className = 'mini-hub-unread-badge';
  unreadBadge.style.display = 'none';
  titlebar.appendChild(unreadBadge);

  // Titlebar button group (right side)
  const btnGroup = document.createElement('div');
  btnGroup.className = 'mini-hub-btn-group';

  // Minimize button
  const minimizeBtn = document.createElement('button');
  minimizeBtn.className = 'mini-hub-minimize';
  minimizeBtn.title = 'Minimize';
  minimizeBtn.innerHTML = icon('minus');
  minimizeBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    controller.minimize();
  });
  btnGroup.appendChild(minimizeBtn);

  // Maximize button
  const maximizeBtn = document.createElement('button');
  maximizeBtn.className = 'mini-hub-maximize';
  maximizeBtn.title = 'Open in main chat';
  maximizeBtn.innerHTML = icon('maximize-2');
  maximizeBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    callbacks.onMaximize(config.hubId);
  });
  btnGroup.appendChild(maximizeBtn);

  // Close button
  const closeBtn = document.createElement('button');
  closeBtn.className = 'mini-hub-close';
  closeBtn.title = 'Close';
  closeBtn.innerHTML = icon('x');
  closeBtn.addEventListener('click', (e) => {
    e.stopPropagation();
    callbacks.onClose(config.hubId);
  });
  btnGroup.appendChild(closeBtn);

  titlebar.appendChild(btnGroup);
  root.appendChild(titlebar);

  // ── Toolbar (model selector below titlebar) ────────────────────────────

  const toolbar = document.createElement('div');
  toolbar.className = 'mini-hub-toolbar';

  const modelSelect = document.createElement('select');
  modelSelect.className = 'mini-hub-model-select';
  modelSelect.title = 'Model override';
  const defaultOpt = document.createElement('option');
  defaultOpt.value = '';
  defaultOpt.textContent = 'Default model';
  modelSelect.appendChild(defaultOpt);
  if (currentModel) {
    const opt = document.createElement('option');
    opt.value = currentModel;
    opt.textContent = currentModel;
    opt.selected = true;
    modelSelect.appendChild(opt);
  }
  modelSelect.addEventListener('change', () => {
    currentModel = modelSelect.value;
    callbacks.onModelChange?.(config.hubId, currentModel);
  });
  toolbar.appendChild(modelSelect);
  root.appendChild(toolbar);

  // ── Message feed ─────────────────────────────────────────────────────

  const messagesContainer = document.createElement('div');
  messagesContainer.className = 'mini-hub-messages';
  root.appendChild(messagesContainer);

  // ── Input area (re-use ChatInputController molecule) ─────────────────

  const chatInput: ChatInputController = createChatInput({
    placeholder: `Message ${config.agentName}…`,
    showAttachBtn: true,
    showTalkBtn: true,
    maxHeight: 80,
  });
  chatInput.el.classList.add('mini-hub-input-area');
  chatInput.onSend = (content, attachments) => {
    if (destroyed) return;
    callbacks.onSend(config.hubId, content, attachments);
    chatInput.clear();
  };
  root.appendChild(chatInput.el);

  // ── Voice-to-text (Talk Mode) ──────────────────────────────────────────

  const talkMode: TalkModeController = createTalkMode(
    () => chatInput.el.querySelector('.chat-input') as HTMLTextAreaElement | null,
    () => chatInput.el.querySelector('.chat-talk-btn') as HTMLElement | null,
    30_000,
  );
  chatInput.onTalk = () => {
    talkMode.toggle();
  };

  // ── Drag-and-drop files on messages area ───────────────────────────────

  messagesContainer.addEventListener('dragover', (e) => {
    e.preventDefault();
    messagesContainer.classList.add('drag-active');
  });
  messagesContainer.addEventListener('dragleave', () => {
    messagesContainer.classList.remove('drag-active');
  });
  messagesContainer.addEventListener('drop', (e) => {
    e.preventDefault();
    messagesContainer.classList.remove('drag-active');
    const files = Array.from(e.dataTransfer?.files ?? []);
    if (files.length) {
      const existing = chatInput.getAttachments();
      chatInput.setAttachments([...existing, ...files]);
    }
  });

  // ── Drag-to-reposition ───────────────────────────────────────────────

  let dragging = false;
  let dragOffsetX = 0;
  let dragOffsetY = 0;

  function onDragStart(e: MouseEvent) {
    // Don't drag when clicking buttons / select
    if ((e.target as HTMLElement).closest('button, select')) return;
    dragging = true;
    dragOffsetX = e.clientX - position.x;
    dragOffsetY = e.clientY - position.y;
    root.style.transition = 'none';
    document.addEventListener('mousemove', onDragMove);
    document.addEventListener('mouseup', onDragEnd);
    e.preventDefault();
  }

  function onDragMove(e: MouseEvent) {
    if (!dragging) return;
    position = {
      x: Math.max(0, Math.min(e.clientX - dragOffsetX, window.innerWidth - HUB_MIN_WIDTH)),
      y: Math.max(0, Math.min(e.clientY - dragOffsetY, window.innerHeight - 40)),
    };
    root.style.left = `${position.x}px`;
    root.style.top = `${position.y}px`;
  }

  function onDragEnd() {
    if (!dragging) return;
    dragging = false;
    root.style.transition = '';
    document.removeEventListener('mousemove', onDragMove);
    document.removeEventListener('mouseup', onDragEnd);
    callbacks.onPositionChange?.(config.hubId, position);
  }

  titlebar.addEventListener('mousedown', onDragStart);

  // Double-click titlebar toggles minimize
  titlebar.addEventListener('dblclick', () => {
    if (minimized) controller.restore();
    else controller.minimize();
  });

  // ── Render helpers ───────────────────────────────────────────────────

  // Build squad agent map once if in squad mode
  const squadAgentMap = config.squadMembers?.length
    ? buildSquadAgentMap(config.squadMembers)
    : undefined;

  function getRenderOpts(): RenderOpts {
    return {
      agentName: config.agentName,
      agentAvatar: config.agentAvatar,
      agentMap: squadAgentMap,
      isStreaming: !!streamingEl,
    };
  }

  function rerenderMessages() {
    renderMessages(messagesContainer, messages, getRenderOpts());
    scrollToBottom(messagesContainer, rafPending);
  }

  function updateUnreadBadge() {
    if (unreadCount > 0) {
      unreadBadge.textContent = unreadCount > 9 ? '9+' : String(unreadCount);
      unreadBadge.style.display = '';
    } else {
      unreadBadge.style.display = 'none';
    }
  }

  // ── Controller ───────────────────────────────────────────────────────

  const controller: MiniHubController = {
    el: root,
    hubId: config.hubId,

    getSessionKey: () => sessionKey,
    setSessionKey: (key: string) => {
      sessionKey = key;
    },

    appendMessage(msg: MessageWithAttachments) {
      messages.push(msg);
      // Remove streaming placeholder if present
      const streamEl = messagesContainer.querySelector('#streaming-message');
      if (streamEl) streamEl.remove();
      streamingEl = null;
      streamingContent = '';
      thinkingContent = '';

      const lastUserIdx = findLastIndex(messages, (m) => m.role === 'user');
      const lastAssistantIdx = findLastIndex(messages, (m) => m.role === 'assistant');
      const el = renderSingleMessage(
        messagesContainer,
        msg,
        messages.length - 1,
        lastUserIdx,
        lastAssistantIdx,
        getRenderOpts(),
      );
      messagesContainer.appendChild(el);
      scrollToBottom(messagesContainer, rafPending);
    },

    setMessages(msgs: MessageWithAttachments[]) {
      messages = msgs;
      rerenderMessages();
    },

    startStreaming(agentName: string) {
      streamingContent = '';
      thinkingContent = '';
      streamingEl = showStreamingMessage(messagesContainer, agentName);
      scrollToBottom(messagesContainer, rafPending);
      controller.setStreamingActive(true);
    },

    appendDelta(text: string) {
      streamingContent += text;
      if (streamingEl) {
        appendStreamingDelta(streamingEl, streamingContent);
        scrollToBottom(messagesContainer, rafPending);
      }
    },

    appendThinking(text: string) {
      thinkingContent += text;
      const streamMsg = messagesContainer.querySelector('#streaming-message') as HTMLElement | null;
      if (streamMsg) {
        appendThinkingDelta(streamMsg, thinkingContent);
        scrollToBottom(messagesContainer, rafPending);
      }
    },

    finalizeStream(content: string) {
      // Remove streaming placeholder
      const streamMsg = messagesContainer.querySelector('#streaming-message');
      if (streamMsg) streamMsg.remove();
      streamingEl = null;
      streamingContent = '';
      thinkingContent = '';
      controller.setStreamingActive(false);

      // The caller should append the final message via appendMessage
      // This method just ensures cleanup of the streaming UI
      if (content) {
        // If content is provided, render the final message directly
        const finalMsg: MessageWithAttachments = {
          role: 'assistant',
          content,
          timestamp: new Date(),
        };
        controller.appendMessage(finalMsg);
      }
    },

    setModel(modelKey: string) {
      currentModel = modelKey;
      // If the model isn't already in the select, add it as a confirmed option
      const exists = Array.from(modelSelect.options).some((o) => o.value === modelKey);
      if (!exists && modelKey) {
        const opt = document.createElement('option');
        opt.value = modelKey;
        opt.textContent = `\u2713 ${modelKey}`;
        // Insert after default option
        if (modelSelect.children.length > 1) {
          modelSelect.insertBefore(opt, modelSelect.children[1]);
        } else {
          modelSelect.appendChild(opt);
        }
      }
      modelSelect.value = modelKey;
    },

    getModel: () => currentModel || config.modelOverride || '',

    populateModels(providers: Array<{ id: string; kind: string; default_model?: string }>) {
      populateModelSelect(modelSelect, providers, {
        defaultLabel: 'Default',
        currentValue: currentModel || '',
      });
    },

    setStreamingActive(active: boolean) {
      streamingActive = active;
      root.classList.toggle('mini-hub-streaming', active);
      streamingDot.classList.toggle('active', active);
    },

    isStreamingActive: () => streamingActive,

    minimize() {
      minimized = true;
      root.classList.add('mini-hub-minimized');
    },

    restore() {
      minimized = false;
      root.classList.remove('mini-hub-minimized');
      unreadCount = 0;
      updateUnreadBadge();
      scrollToBottom(messagesContainer, rafPending);
      chatInput.focus();
    },

    isMinimized: () => minimized,

    incrementUnread() {
      unreadCount++;
      updateUnreadBadge();
    },

    clearUnread() {
      unreadCount = 0;
      updateUnreadBadge();
    },

    focus() {
      root.style.zIndex = `${9000 + (Date.now() % 1000)}`;
      chatInput.focus();
    },

    getPosition: () => ({ ...position }),

    destroy() {
      if (destroyed) return;
      destroyed = true;
      talkMode.cleanup();
      titlebar.removeEventListener('mousedown', onDragStart);
      document.removeEventListener('mousemove', onDragMove);
      document.removeEventListener('mouseup', onDragEnd);
      chatInput.destroy();
      root.remove();
    },
  };

  // Bring to front on click
  root.addEventListener('mousedown', () => {
    if (!destroyed) {
      root.style.zIndex = `${9000 + (Date.now() % 1000)}`;
    }
  });

  return controller;
}
