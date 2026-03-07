// src/engine/molecules/chat_renderer.ts
// Scoped message rendering molecule.
// Every render function receives a container element as its first argument —
// no global DOM lookups. This makes rendering instance-able for mini-hubs.

import { formatMarkdown, wireCodeCopyButtons, escHtml } from '../../components/molecules/markdown';
import { icon } from '../../components/helpers';
import { findLastIndex } from '../atoms/chat';
import { tesseractPlaceholder, activateTesseracts } from '../../components/tesseract';
import type { Message } from '../../types';
import type { MessageWithAttachments } from '../../state/index';

// ── Render options ───────────────────────────────────────────────────────

// ── Follow-up suggestion generation ──────────────────────────────────────

/**
 * Generate short follow-up question suggestions from assistant content.
 * Uses heuristic extraction (no LLM call — instant).
 * Returns up to 3 suggestions.
 */
function generateFollowUpSuggestions(content: string): string[] {
  const suggestions: string[] = [];
  const lower = content.toLowerCase();

  // If the response mentions code/implementation, suggest explaining or expanding
  if (lower.includes('function') || lower.includes('class') || lower.includes('```')) {
    suggestions.push('Explain this in more detail');
  }
  // If there's a list/steps, suggest continuing or alternatives
  if ((lower.match(/\d\.\s/g)?.length ?? 0) >= 3 || (lower.match(/^[-•·]\s/gm)?.length ?? 0) >= 3) {
    suggestions.push('Are there other approaches?');
  }
  // If there's an error or warning mentioned
  if (lower.includes('error') || lower.includes('warning') || lower.includes('issue')) {
    suggestions.push('How do I fix this?');
  }
  // If it mentions files or configuration
  if (lower.includes('config') || lower.includes('setting') || lower.includes('.json') || lower.includes('.yaml')) {
    suggestions.push('Show me an example configuration');
  }
  // Generic follow-up if we have room
  if (suggestions.length < 2 && content.length > 200) {
    suggestions.push('Tell me more');
  }
  if (suggestions.length < 3 && content.length > 300) {
    suggestions.push('Summarize the key points');
  }

  return suggestions.slice(0, 3);
}

// ── Agent step indicator ─────────────────────────────────────────────────

/**
 * Show or update an agent tool-step indicator in the streaming message.
 * Displays as a subtle status line: "⟳ Running: tool_name"
 */
export function showToolStep(container: HTMLElement, toolName: string): void {
  const streamingMsg = container.querySelector('#streaming-message');
  if (!streamingMsg) return;
  let stepEl = streamingMsg.querySelector('.agent-step-indicator') as HTMLElement | null;
  if (!stepEl) {
    stepEl = document.createElement('div');
    stepEl.className = 'agent-step-indicator';
    // Insert before message-content
    const contentEl = streamingMsg.querySelector('.message-content');
    if (contentEl) streamingMsg.insertBefore(stepEl, contentEl);
    else streamingMsg.appendChild(stepEl);
  }
  const humanLabels: Record<string, string> = {
    exec: 'Running command',
    run_command: 'Running command',
    write_file: 'Writing file',
    read_file: 'Reading file',
    search: 'Searching',
    web_search: 'Searching the web',
    fetch: 'Fetching URL',
    web_read: 'Reading page',
    list_files: 'Listing files',
    grep: 'Searching code',
  };
  const label = humanLabels[toolName] ?? `Using ${toolName}`;
  stepEl.innerHTML = `<span class="ms step-spin" style="font-size:14px">progress_activity</span> ${escHtml(label)}`;
}

/** Remove the tool step indicator from the streaming message. */
export function clearToolStep(container: HTMLElement): void {
  container.querySelector('.agent-step-indicator')?.remove();
}

export interface RenderOpts {
  agentName: string;
  agentAvatar?: string;
  onRetry?: (content: string) => void;
  onSpeak?: (text: string, btn: HTMLButtonElement) => void;
  /** Feedback callback: called when user clicks thumbs up/down on a message */
  onFeedback?: (messageId: string, helpful: boolean) => void;
  /** Edit callback: called when user edits a previous message and re-sends */
  onEdit?: (index: number, newContent: string) => void;
  /** Follow-up click callback: called when user clicks a suggested follow-up */
  onFollowUp?: (text: string) => void;
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

  // Agent name prefix (only for assistant/system — user bubbles speak for themselves)
  if (msg.role !== 'user') {
    const prefix = document.createElement('span');
    prefix.className = 'message-prefix';
    if (msg.role === 'assistant') {
      const agentInfo = msg.agentId && opts.agentMap?.get(msg.agentId);
      if (agentInfo) {
        prefix.textContent = agentInfo.name ?? msg.agentId;
        if (agentInfo.color) prefix.style.color = agentInfo.color;
      } else {
        prefix.textContent = msg.agentName ?? opts.agentName ?? 'Agent';
      }
    } else {
      prefix.textContent = 'System';
    }
    div.appendChild(prefix);
  }

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

  // Edit button on user messages (click to edit and re-send)
  if (msg.role === 'user' && !opts.isStreaming && opts.onEdit) {
    const editBtn = document.createElement('button');
    editBtn.className = 'message-edit-btn';
    editBtn.title = 'Edit message';
    editBtn.innerHTML = `<span class="ms" style="font-size:13px">edit</span>`;
    const msgIndex = index;
    editBtn.addEventListener('click', () => {
      // Replace content with editable textarea
      const editArea = document.createElement('textarea');
      editArea.className = 'message-edit-area';
      editArea.value = msg.content;
      editArea.rows = Math.min(msg.content.split('\n').length + 1, 8);
      const editActions = document.createElement('div');
      editActions.className = 'message-edit-actions';
      const saveBtn = document.createElement('button');
      saveBtn.className = 'btn btn-primary btn-sm';
      saveBtn.textContent = 'Send';
      const cancelBtn = document.createElement('button');
      cancelBtn.className = 'btn btn-ghost btn-sm';
      cancelBtn.textContent = 'Cancel';
      editActions.appendChild(saveBtn);
      editActions.appendChild(cancelBtn);
      contentEl.style.display = 'none';
      div.insertBefore(editArea, contentEl);
      div.insertBefore(editActions, contentEl);
      editArea.focus();
      editArea.setSelectionRange(editArea.value.length, editArea.value.length);
      saveBtn.addEventListener('click', () => {
        const newContent = editArea.value.trim();
        if (newContent && newContent !== msg.content) {
          opts.onEdit!(msgIndex, newContent);
        }
        editArea.remove();
        editActions.remove();
        contentEl.style.display = '';
      });
      cancelBtn.addEventListener('click', () => {
        editArea.remove();
        editActions.remove();
        contentEl.style.display = '';
      });
      editArea.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' && !e.shiftKey) {
          e.preventDefault();
          saveBtn.click();
        }
        if (e.key === 'Escape') cancelBtn.click();
      });
    });
    div.appendChild(editBtn);
  }

  // Follow-up suggestions on the last assistant message
  if (
    msg.role === 'assistant' &&
    index === lastAssistantIdx &&
    !opts.isStreaming &&
    opts.onFollowUp &&
    msg.content.length > 100
  ) {
    const suggestions = generateFollowUpSuggestions(msg.content);
    if (suggestions.length > 0) {
      const followUpRow = document.createElement('div');
      followUpRow.className = 'follow-up-suggestions';
      for (const s of suggestions) {
        const pill = document.createElement('button');
        pill.className = 'follow-up-pill';
        pill.innerHTML = `<span class="ms" style="font-size:12px">arrow_forward</span> ${escHtml(s)}`;
        pill.addEventListener('click', () => opts.onFollowUp!(s));
        followUpRow.appendChild(pill);
      }
      div.appendChild(followUpRow);
    }
  }

  // TTS button
  if (
    msg.role === 'assistant' &&
    msg.content &&
    !msg.content.startsWith('Error:') &&
    opts.onSpeak
  ) {
    const ttsBtn = document.createElement('button');
    ttsBtn.className = 'message-tts-btn';
    ttsBtn.title = 'Read aloud';
    ttsBtn.innerHTML = `<span class="ms">volume_up</span>`;
    const msgContent = msg.content;
    ttsBtn.addEventListener('click', () => opts.onSpeak!(msgContent, ttsBtn));
    div.appendChild(ttsBtn);
  }

  // Feedback buttons (thumbs up / down)
  if (
    msg.role === 'assistant' &&
    msg.content &&
    !msg.content.startsWith('Error:') &&
    opts.onFeedback &&
    !opts.isStreaming
  ) {
    const feedbackRow = document.createElement('div');
    feedbackRow.className = 'chat-feedback-row';

    const msgId = msg.id ?? `msg-${index}`;

    const thumbUp = document.createElement('button');
    thumbUp.className = 'chat-fb-btn';
    thumbUp.title = 'Helpful';
    thumbUp.innerHTML = `<span class="ms">thumb_up</span>`;
    thumbUp.addEventListener('click', () => {
      thumbUp.disabled = true;
      thumbDown.disabled = true;
      opts.onFeedback!(msgId, true);
      feedbackRow.innerHTML = '<span class="chat-fb-done">👍 Thanks!</span>';
    });

    const thumbDown = document.createElement('button');
    thumbDown.className = 'chat-fb-btn';
    thumbDown.title = 'Not helpful';
    thumbDown.innerHTML = `<span class="ms">thumb_down</span>`;
    thumbDown.addEventListener('click', () => {
      thumbUp.disabled = true;
      thumbDown.disabled = true;
      opts.onFeedback!(msgId, false);
      feedbackRow.innerHTML = '<span class="chat-fb-done">👎 Noted</span>';
    });

    feedbackRow.appendChild(thumbUp);
    feedbackRow.appendChild(thumbDown);
    div.appendChild(feedbackRow);
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

  // Wire code block copy buttons (event delegation on container — idempotent)
  if (!(container as HTMLElement & { _codeCopyWired?: boolean })._codeCopyWired) {
    wireCodeCopyButtons(container);
    (container as HTMLElement & { _codeCopyWired?: boolean })._codeCopyWired = true;
  }
}

// ── Streaming message helpers ────────────────────────────────────────────

/**
 * Insert a streaming placeholder message into the container.
 * Returns the content element that deltas should be appended to.
 */
export function showStreamingMessage(container: HTMLElement, agentName: string): HTMLElement {
  const div = document.createElement('div');
  div.className = 'message assistant';
  div.id = 'streaming-message';

  const contentEl = document.createElement('div');
  contentEl.className = 'message-content';

  // Agent name prefix above content
  const prefix = document.createElement('span');
  prefix.className = 'message-prefix';
  prefix.textContent = agentName ?? 'Agent';
  div.appendChild(prefix);

  const streamSpan = document.createElement('span');
  streamSpan.innerHTML = tesseractPlaceholder(20, 'streaming');
  contentEl.appendChild(streamSpan);
  activateTesseracts(streamSpan);

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
export function appendThinkingDelta(streamingMsg: HTMLElement, thinkingContent: string): void {
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
