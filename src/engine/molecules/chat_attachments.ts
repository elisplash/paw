// src/engine/molecules/chat_attachments.ts
// Attachment preview + encoding molecule.
// Extracted from chat_controller.ts to respect atomic boundaries.

import { appState } from '../../state/index';
import { icon } from '../../components/helpers';
import { fileToBase64, fileTypeIcon } from '../atoms/chat';

// ── DOM shorthand ────────────────────────────────────────────────────────
const $ = (id: string) => document.getElementById(id);

// ── Attachment preview ───────────────────────────────────────────────────

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

// ── Clear attachments ────────────────────────────────────────────────────

export function clearPendingAttachments(): void {
  appState.pendingAttachments = [];
  renderAttachmentPreview();
}

// ── Encode file attachments to base64 ────────────────────────────────────

export async function encodeFileAttachments(): Promise<
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
