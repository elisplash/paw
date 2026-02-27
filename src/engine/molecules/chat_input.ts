// src/engine/molecules/chat_input.ts
// Scoped input controller molecule.
// Encapsulates all chat input area logic: textarea, send button,
// attachment handling, slash autocomplete.
// Returns a ChatInputController — instance-able for mini-hubs.

import { icon, escHtml } from '../../components/helpers';
import { fileTypeIcon } from '../atoms/chat';
import { getAutocompleteSuggestions } from '../../features/slash-commands';

// ── Types ────────────────────────────────────────────────────────────────

export interface ChatInputController {
  /** The root DOM element to mount */
  el: HTMLElement;
  /** Get current textarea value */
  getValue(): string;
  /** Set textarea value */
  setValue(text: string): void;
  /** Clear textarea and attachments */
  clear(): void;
  /** Focus the textarea */
  focus(): void;
  /** Get pending file attachments */
  getAttachments(): File[];
  /** Set pending attachments (replaces all) */
  setAttachments(files: File[]): void;
  /** Clear all pending attachments */
  clearAttachments(): void;
  /** Render the attachment preview strip */
  renderAttachmentPreview(): void;
  /** Send callback — set by the consumer */
  onSend: ((content: string, attachments: File[]) => void) | null;
  /** Talk mode callback — set by the consumer */
  onTalk: (() => void) | null;
  /** Teardown: remove listeners, cleanup */
  destroy(): void;
}

export interface ChatInputConfig {
  /** Placeholder text for the textarea */
  placeholder?: string;
  /** Whether to show the attach button */
  showAttachBtn?: boolean;
  /** Whether to show the talk (mic) button */
  showTalkBtn?: boolean;
  /** Max textarea height in px */
  maxHeight?: number;
}

// ── Factory ──────────────────────────────────────────────────────────────

/**
 * Create a scoped chat input controller.
 * All DOM is self-contained — no global getElementById calls.
 */
export function createChatInput(config: ChatInputConfig = {}): ChatInputController {
  const {
    placeholder = 'Message your agent…',
    showAttachBtn = true,
    showTalkBtn = true,
    maxHeight = 120,
  } = config;

  let pendingAttachments: File[] = [];
  let destroyed = false;

  // ── Build DOM ──────────────────────────────────────────────────────────

  const root = document.createElement('div');
  root.className = 'chat-input-area';

  // Attachment preview strip
  const attachPreview = document.createElement('div');
  attachPreview.className = 'chat-attachment-preview';
  attachPreview.style.display = 'none';
  root.appendChild(attachPreview);

  // Input row container
  const inputRow = document.createElement('div');
  inputRow.className = 'chat-input-row';

  // Attach button
  let fileInput: HTMLInputElement | null = null;
  if (showAttachBtn) {
    const attachBtn = document.createElement('button');
    attachBtn.className = 'chat-attach-btn';
    attachBtn.title = 'Attach files';
    attachBtn.innerHTML = icon('paperclip');
    fileInput = document.createElement('input');
    fileInput.type = 'file';
    fileInput.multiple = true;
    fileInput.style.display = 'none';
    attachBtn.addEventListener('click', () => fileInput?.click());
    fileInput.addEventListener('change', () => {
      if (!fileInput?.files) return;
      for (const file of Array.from(fileInput.files)) {
        pendingAttachments.push(file);
      }
      fileInput.value = '';
      renderAttachmentPreviewFn();
    });
    inputRow.appendChild(attachBtn);
    inputRow.appendChild(fileInput);
  }

  // Textarea
  const textarea = document.createElement('textarea');
  textarea.className = 'chat-input';
  textarea.placeholder = placeholder;
  textarea.rows = 1;
  inputRow.appendChild(textarea);

  // Slash autocomplete popup (created lazily)
  let acPopup: HTMLElement | null = null;

  // Talk button
  if (showTalkBtn) {
    const talkBtn = document.createElement('button');
    talkBtn.className = 'chat-talk-btn';
    talkBtn.title = 'Talk Mode — hold to speak';
    talkBtn.innerHTML = `<span class="ms">mic</span>`;
    talkBtn.addEventListener('click', () => {
      controller.onTalk?.();
    });
    inputRow.appendChild(talkBtn);
  }

  // Send button
  const sendBtn = document.createElement('button');
  sendBtn.className = 'chat-send';
  sendBtn.title = 'Send';
  sendBtn.innerHTML = icon('send');
  sendBtn.addEventListener('click', handleSend);
  inputRow.appendChild(sendBtn);

  root.appendChild(inputRow);

  // ── Event handlers ─────────────────────────────────────────────────────

  function handleSend(): void {
    const content = textarea.value.trim();
    if (!content) return;
    controller.onSend?.(content, [...pendingAttachments]);
  }

  function handleKeydown(e: KeyboardEvent): void {
    // Handle slash autocomplete navigation
    if (acPopup && acPopup.style.display !== 'none') {
      if (e.key === 'Escape') {
        acPopup.style.display = 'none';
        e.preventDefault();
        return;
      }
      if (e.key === 'Tab' || (e.key === 'Enter' && !e.shiftKey)) {
        const selected = acPopup.querySelector('.slash-ac-item.selected') as HTMLElement | null;
        if (selected) {
          e.preventDefault();
          const cmd = selected.dataset.command ?? '';
          textarea.value = `${cmd} `;
          textarea.focus();
          acPopup.style.display = 'none';
          return;
        }
      }
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        const items = Array.from(acPopup.querySelectorAll('.slash-ac-item')) as HTMLElement[];
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
    // Enter to send (shift+enter for newline)
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  }

  function handleInput(): void {
    // Auto-resize textarea
    textarea.style.height = 'auto';
    textarea.style.height = `${Math.min(textarea.scrollHeight, maxHeight)}px`;

    // Slash autocomplete
    const val = textarea.value;
    if (val.startsWith('/') && !val.includes(' ')) {
      const suggestions = getAutocompleteSuggestions(val);
      if (suggestions.length > 0) {
        if (!acPopup) {
          acPopup = document.createElement('div');
          acPopup.className = 'slash-autocomplete-popup';
          textarea.parentElement?.insertBefore(acPopup, textarea);
        }
        acPopup.innerHTML = suggestions
          .map(
            (s, i) =>
              `<div class="slash-ac-item${i === 0 ? ' selected' : ''}" data-command="${escHtml(s.command)}">
            <span class="slash-ac-cmd">${escHtml(s.command)}</span>
            <span class="slash-ac-desc">${escHtml(s.description)}</span>
          </div>`,
          )
          .join('');
        acPopup.style.display = 'block';
        acPopup.querySelectorAll('.slash-ac-item').forEach((item) => {
          item.addEventListener('click', () => {
            const cmd = (item as HTMLElement).dataset.command ?? '';
            textarea.value = `${cmd} `;
            textarea.focus();
            if (acPopup) acPopup.style.display = 'none';
          });
        });
      } else if (acPopup) {
        acPopup.style.display = 'none';
      }
    } else if (acPopup) {
      acPopup.style.display = 'none';
    }
  }

  textarea.addEventListener('keydown', handleKeydown);
  textarea.addEventListener('input', handleInput);

  // ── Attachment preview rendering ───────────────────────────────────────

  function renderAttachmentPreviewFn(): void {
    if (pendingAttachments.length === 0) {
      attachPreview.style.display = 'none';
      attachPreview.innerHTML = '';
      return;
    }
    attachPreview.style.display = 'flex';
    attachPreview.innerHTML = '';
    for (let i = 0; i < pendingAttachments.length; i++) {
      const file = pendingAttachments[i];
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
        pendingAttachments.splice(idx, 1);
        renderAttachmentPreviewFn();
      });
      chip.appendChild(removeBtn);
      attachPreview.appendChild(chip);
    }
  }

  // ── Controller object ──────────────────────────────────────────────────

  const controller: ChatInputController = {
    el: root,
    getValue: () => textarea.value.trim(),
    setValue: (text: string) => {
      textarea.value = text;
      textarea.style.height = 'auto';
      textarea.style.height = `${Math.min(textarea.scrollHeight, maxHeight)}px`;
    },
    clear: () => {
      textarea.value = '';
      textarea.style.height = 'auto';
      pendingAttachments = [];
      renderAttachmentPreviewFn();
    },
    focus: () => textarea.focus(),
    getAttachments: () => [...pendingAttachments],
    setAttachments: (files: File[]) => {
      pendingAttachments = [...files];
      renderAttachmentPreviewFn();
    },
    clearAttachments: () => {
      pendingAttachments = [];
      renderAttachmentPreviewFn();
    },
    renderAttachmentPreview: renderAttachmentPreviewFn,
    onSend: null,
    onTalk: null,
    destroy: () => {
      if (destroyed) return;
      destroyed = true;
      textarea.removeEventListener('keydown', handleKeydown);
      textarea.removeEventListener('input', handleInput);
      root.remove();
    },
  };

  return controller;
}
