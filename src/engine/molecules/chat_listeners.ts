// src/engine/molecules/chat_listeners.ts
// DOM event listener wiring molecule for the chat view.
// Extracted from chat_controller.ts to respect atomic boundaries.

import { pawEngine } from '../../engine';
import {
  appState,
  agentSessionMap,
  persistAgentSessionMap,
} from '../../state/index';
import { escHtml, confirmModal } from '../../components/helpers';
import { showToast } from '../../components/toast';
import * as AgentsModule from '../../views/agents';
import { getAutocompleteSuggestions } from '../../features/slash-commands';
import { createTalkMode } from './tts';
import { renderAttachmentPreview } from './chat_attachments';
import type { TokenMeterController, TokenMeterState } from './token_meter';

// ── Types ────────────────────────────────────────────────────────────────

/** Callbacks/deps the listeners need from the organism. */
export interface ChatListenerDeps {
  sendMessage: () => Promise<void>;
  switchToAgent: (agentId: string) => Promise<void>;
  loadSessions: (opts?: { skipHistory?: boolean }) => Promise<void>;
  loadChatHistory: (key: string) => Promise<void>;
  renderMessages: () => void;
  resetTokenMeter: () => void;
  teardownStream: (key: string, reason: string) => void;
  getTokenMeter: () => TokenMeterController;
  meterSnapshot: () => TokenMeterState;
}

// ── DOM shorthand ────────────────────────────────────────────────────────
const $ = (id: string) => document.getElementById(id);

// ── Talk mode controller (scoped) ────────────────────────────────────────
let _talkMode: ReturnType<typeof createTalkMode> | null = null;

// ── Init ─────────────────────────────────────────────────────────────────

export function initChatListeners(deps: ChatListenerDeps): void {
  const chatSend = document.getElementById('chat-send') as HTMLButtonElement | null;
  const chatInput = document.getElementById('chat-input') as HTMLTextAreaElement | null;
  const chatAttachBtn = document.getElementById('chat-attach-btn');
  const chatFileInput = document.getElementById('chat-file-input') as HTMLInputElement | null;
  const chatSessionSelect = document.getElementById(
    'chat-session-select',
  ) as HTMLSelectElement | null;
  const chatAgentSelect = document.getElementById('chat-agent-select') as HTMLSelectElement | null;

  chatSend?.addEventListener('click', deps.sendMessage);

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
      deps.sendMessage();
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
              `<div class="slash-ac-item${i === 0 ? ' selected' : ''}" data-command="${escHtml(s.command)}">
            <span class="slash-ac-cmd">${escHtml(s.command)}</span>
            <span class="slash-ac-desc">${escHtml(s.description)}</span>
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

    const oldKey = appState.currentSessionKey ?? '';
    if (oldKey !== key) {
      deps.teardownStream(oldKey, 'Session switched');
    }

    appState.currentSessionKey = key;
    const curAgent = AgentsModule.getCurrentAgent();
    if (curAgent) {
      agentSessionMap.set(curAgent.id, key);
      persistAgentSessionMap();
    }
    deps.resetTokenMeter();
    deps.loadChatHistory(key);
  });

  chatAgentSelect?.addEventListener('change', () => {
    const agentId = chatAgentSelect?.value;
    if (agentId) deps.switchToAgent(agentId);
  });

  $('new-chat-btn')?.addEventListener('click', () => {
    const oldKey = appState.currentSessionKey ?? '';
    deps.teardownStream(oldKey, 'New chat');
    appState.messages = [];
    appState.currentSessionKey = null;
    deps.resetTokenMeter();
    deps.renderMessages();
    const chatSessionSelect2 = document.getElementById(
      'chat-session-select',
    ) as HTMLSelectElement | null;
    if (chatSessionSelect2) chatSessionSelect2.value = '';
  });

  $('chat-abort-btn')?.addEventListener('click', async () => {
    const key = appState.currentSessionKey ?? '';
    deps.teardownStream(key, 'Stopped');
    showToast('Agent stopped', 'info');
  });

  $('session-rename-btn')?.addEventListener('click', async () => {
    if (!appState.currentSessionKey || !appState.wsConnected) return;
    const { promptModal } = await import('../../components/helpers');
    const name = await promptModal('Rename session', 'New name…');
    if (!name) return;
    try {
      await pawEngine.sessionRename(appState.currentSessionKey, name);
      showToast('Session renamed', 'success');
      await deps.loadSessions();
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
      deps.renderMessages();
      showToast('Session deleted', 'success');
      await deps.loadSessions();
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
      deps.resetTokenMeter();
      deps.renderMessages();
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
      deps.resetTokenMeter();
      const ba = document.getElementById('session-budget-alert');
      if (ba) ba.style.display = 'none';
      const history = await pawEngine.chatHistory(appState.currentSessionKey, 100);
      appState.messages = history.map((m) => ({
        role: m.role as 'user' | 'assistant' | 'system',
        content: m.content,
        timestamp: new Date(m.created_at),
      }));
      deps.renderMessages();
    } catch (e) {
      showToast(`Compact failed: ${e instanceof Error ? e.message : e}`, 'error');
    }
  });

  $('compaction-warning-dismiss')?.addEventListener('click', () => {
    appState.compactionDismissed = true;
    const warning = document.getElementById('compaction-warning');
    if (warning) warning.style.display = 'none';
  });

  // Talk Mode: use scoped TalkModeController
  _talkMode = createTalkMode(
    () => document.getElementById('chat-input') as HTMLTextAreaElement | null,
    () => document.getElementById('chat-talk-btn'),
  );
  $('chat-talk-btn')?.addEventListener('click', () => _talkMode?.toggle());

  // Context breakdown popover on token meter click
  initContextBreakdownClick(deps);
}

// ── Context breakdown popover ────────────────────────────────────────────

function initContextBreakdownClick(deps: ChatListenerDeps): void {
  const meter = $('token-meter');
  if (!meter) return;
  meter.style.cursor = 'pointer';
  meter.addEventListener('click', (e) => {
    e.stopPropagation();
    deps.getTokenMeter().toggleBreakdown();
    deps.getTokenMeter().updateBreakdownPopover(deps.meterSnapshot());
  });
  document.addEventListener('click', () => {
    const panel = $('context-breakdown-panel');
    if (panel) panel.style.display = 'none';
  });
  const panel = $('context-breakdown-panel');
  if (panel) {
    panel.addEventListener('click', (e) => e.stopPropagation());
  }
}
