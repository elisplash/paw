// src/engine/molecules/chat_renderer.ts
// Scoped message rendering molecule.
// Every render function receives a container element as its first argument —
// no global DOM lookups. This makes rendering instance-able for mini-hubs.

import { formatMarkdown } from '../../components/molecules/markdown';
import { icon } from '../../components/helpers';
import { findLastIndex } from '../atoms/chat';
import type { Message } from '../../types';
import type { MessageWithAttachments } from '../../state/index';

// ── Render options ───────────────────────────────────────────────────────

export interface RenderOpts {
  agentName: string;
  agentAvatar?: string;
  onRetry?: (content: string) => void;
  onSpeak?: (text: string, btn: HTMLButtonElement) => void;
  /** For multi-agent: map of agentId → display info */
  agentMap?: Map<string, { name: string; avatar?: string; color?: string }>;
  /** Whether to show retry button (e.g. disable during streaming) */
  isStreaming?: boolean;
}

// ── Screenshot card ──────────────────────────────────────────────────────

/** Render an inline screenshot card for assistant messages. */
export function renderScreenshotCard(msgContent: string): HTMLElement | null {
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
      const { pawEngine: eng } = await import('./ipc_client');
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

// ── Attachment strip ─────────────────────────────────────────────────────

/** Render attachment strip (images + file chips) for a message. */
export function renderAttachmentStrip(
  attachments: NonNullable<Message['attachments']>,
): HTMLElement {
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

// ── Single message rendering ─────────────────────────────────────────────

/**
 * Build a single message DOM element.
 * Scoped: no global state access — all data comes via arguments.
 */
export function renderSingleMessage(
  _container: HTMLElement,
  msg: Message,
  index: number,
  lastUserIdx: number,
  lastAssistantIdx: number,
  opts: RenderOpts,
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

  // Terminal-style prefix: YOU › or AGENT ›
  // For multi-agent (squad) messages, use per-agent name and color from agentMap
  const prefix = document.createElement('span');
  prefix.className = 'message-prefix';
  if (msg.role === 'user') {
    prefix.textContent = 'YOU ›';
  } else if (msg.role === 'assistant') {
    const agentInfo = msg.agentId && opts.agentMap?.get(msg.agentId);
    if (agentInfo) {
      prefix.textContent = `${(agentInfo.name ?? msg.agentId).toUpperCase()} ›`;
      if (agentInfo.color) prefix.style.color = agentInfo.color;
    } else {
      prefix.textContent = `${(msg.agentName ?? opts.agentName ?? 'AGENT').toUpperCase()} ›`;
    }
  } else {
    prefix.textContent = 'SYS ›';
  }
  contentEl.appendChild(prefix);

  const textNode = document.createElement('span');
  if (msg.role === 'assistant' || msg.role === 'system') {
    textNode.innerHTML = formatMarkdown(msg.content);
  } else {
    textNode.textContent = msg.content;
  }
  contentEl.appendChild(textNode);

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
  if ((isLastUser || isErrored) && !opts.isStreaming && opts.onRetry) {
    const retryBtn = document.createElement('button');
    retryBtn.className = 'message-retry-btn';
    retryBtn.title = 'Retry';
    retryBtn.innerHTML = `${icon('rotate-ccw')} Retry`;
    const retryContent = msg.content;
    retryBtn.addEventListener('click', () => opts.onRetry!(retryContent));
    div.appendChild(retryBtn);
  }

  // TTS button
  if (msg.role === 'assistant' && msg.content && !msg.content.startsWith('Error:') && opts.onSpeak) {
    const ttsBtn = document.createElement('button');
    ttsBtn.className = 'message-tts-btn';
    ttsBtn.title = 'Read aloud';
    ttsBtn.innerHTML = `<span class="ms">volume_up</span>`;
    const msgContent = msg.content;
    ttsBtn.addEventListener('click', () => opts.onSpeak!(msgContent, ttsBtn));
    div.appendChild(ttsBtn);
  }

  return div;
}

// ── Full message list rendering ──────────────────────────────────────────

/**
 * Render the full message list into a container element.
 * Clears existing messages and rebuilds from the provided array.
 * Scoped: operates only on the given container, not global DOM.
 */
export function renderMessages(
  container: HTMLElement,
  messages: MessageWithAttachments[],
  opts: RenderOpts,
  emptyStateEl?: HTMLElement | null,
): void {
  // Remove existing messages from container
  container.querySelectorAll('.message').forEach((m) => m.remove());

  if (messages.length === 0) {
    if (emptyStateEl) emptyStateEl.style.display = 'flex';
    return;
  }
  if (emptyStateEl) emptyStateEl.style.display = 'none';

  const frag = document.createDocumentFragment();
  const lastUserIdx = findLastIndex(messages, (m) => m.role === 'user');
  const lastAssistantIdx = findLastIndex(messages, (m) => m.role === 'assistant');

  for (let i = 0; i < messages.length; i++) {
    frag.appendChild(
      renderSingleMessage(container, messages[i], i, lastUserIdx, lastAssistantIdx, opts),
    );
  }

  // Preserve streaming message if present
  const streamingEl = container.querySelector('#streaming-message');
  if (streamingEl) container.insertBefore(frag, streamingEl);
  else container.appendChild(frag);
}

// ── Streaming message helpers ────────────────────────────────────────────

/**
 * Insert a streaming placeholder message into the container.
 * Returns the content element that deltas should be appended to.
 */
export function showStreamingMessage(
  container: HTMLElement,
  agentName: string,
): HTMLElement {
  const div = document.createElement('div');
  div.className = 'message assistant';
  div.id = 'streaming-message';

  const contentEl = document.createElement('div');
  contentEl.className = 'message-content';

  // Terminal-style prefix for streaming
  const prefix = document.createElement('span');
  prefix.className = 'message-prefix';
  prefix.textContent = `${(agentName ?? 'AGENT').toUpperCase()} ›`;
  contentEl.appendChild(prefix);

  const streamSpan = document.createElement('span');
  streamSpan.innerHTML = '<div class="loading-dots"><span></span><span></span><span></span></div>';
  contentEl.appendChild(streamSpan);

  const time = document.createElement('div');
  time.className = 'message-time';
  time.textContent = new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });

  div.appendChild(contentEl);
  div.appendChild(time);
  container.appendChild(div);

  return contentEl;
}

/**
 * Update the streaming content element with accumulated delta text.
 */
export function appendStreamingDelta(el: HTMLElement, content: string): void {
  el.innerHTML = formatMarkdown(content);
}

/**
 * Append a thinking/reasoning delta to the streaming message.
 * Renders inside a collapsible `<details>` block above the main response.
 */
export function appendThinkingDelta(
  streamingMsg: HTMLElement,
  thinkingContent: string,
): void {
  let thinkingEl = streamingMsg.querySelector('.thinking-block') as HTMLElement | null;
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
    const contentEl = streamingMsg.querySelector('.message-content');
    if (contentEl) {
      streamingMsg.insertBefore(thinkingEl, contentEl);
    } else {
      streamingMsg.prepend(thinkingEl);
    }
  }

  const contentDiv = thinkingEl.querySelector('.thinking-content') as HTMLElement | null;
  if (contentDiv) {
    contentDiv.innerHTML = formatMarkdown(thinkingContent);
  }
}

// ── Scroll helper ────────────────────────────────────────────────────────

/** Scroll a container to its bottom, RAF-debounced. */
export function scrollToBottom(container: HTMLElement, rafPendingRef: { value: boolean }): void {
  if (rafPendingRef.value || !container) return;
  rafPendingRef.value = true;
  requestAnimationFrame(() => {
    container.scrollTop = container.scrollHeight;
    rafPendingRef.value = false;
  });
}
