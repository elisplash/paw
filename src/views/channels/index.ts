// index.ts — Channels module orchestration, event wiring, and public API
// Imports from sub-modules and provides the unified public interface

import { pawEngine } from '../../engine';
import { $, escHtml } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { appState } from '../../state/index';
import { CHANNEL_SETUPS, CHANNEL_CLASSES } from './atoms';
import {
  loadChannels,
  getChannelConfig,
  getChannelStatus,
  startChannel,
  setOpenChannelSetup,
} from './molecules';
import { openChannelSetup, closeChannelSetup, saveChannelSetup } from './setup';

// Wire the circular dependency: molecules needs openChannelSetup from setup
setOpenChannelSetup(openChannelSetup);

// ── Cron modal helpers ─────────────────────────────────────────────────────

function showCronModal() {
  const modal = $('cron-modal');
  if (modal) modal.style.display = 'flex';
  const label = $('cron-form-label') as HTMLInputElement;
  const schedule = $('cron-form-schedule') as HTMLInputElement;
  const prompt_ = $('cron-form-prompt') as HTMLTextAreaElement;
  const preset = $('cron-form-schedule-preset') as HTMLSelectElement;
  if (label) label.value = '';
  if (schedule) schedule.value = '';
  if (prompt_) prompt_.value = '';
  if (preset) preset.value = '';
}

function hideCronModal() {
  const modal = $('cron-modal');
  if (modal) modal.style.display = 'none';
}

// ── Memory stubs ───────────────────────────────────────────────────────────

export async function loadMemory() {
  const list = $('memory-list');
  const empty = $('memory-empty');
  const loading = $('memory-loading');
  if (loading) loading.style.display = 'none';
  if (list) list.innerHTML = '';
  if (empty) {
    empty.style.display = 'flex';
    empty.textContent = 'Agent files managed via Memory Palace';
  }
}

export async function openMemoryFile(filePath: string) {
  console.debug('[main] openMemoryFile:', filePath);
}

// ── Dashboard / Space cron stubs ───────────────────────────────────────────

export async function loadDashboardCron() {
  const section = $('dashboard-cron-section');
  if (section) section.style.display = 'none';
}

export async function loadSpaceCron(_space: string) {
  // TODO: engine-native cron
}

// ── initChannels: wire all DOM event listeners ─────────────────────────────

export function initChannels() {
  $('channel-setup-close')?.addEventListener('click', closeChannelSetup);
  $('channel-setup-cancel')?.addEventListener('click', closeChannelSetup);
  const _saveBtn = $('channel-setup-save');
  console.debug('[mail-debug] Binding save button, element found:', !!_saveBtn);
  _saveBtn?.addEventListener('click', saveChannelSetup);
  $('channel-setup-modal')?.addEventListener('click', (e) => {
    if ((e.target as HTMLElement).id === 'channel-setup-modal') closeChannelSetup();
  });

  $('add-channel-btn')?.addEventListener('click', () => {
    const body = $('channel-setup-body');
    const title = $('channel-setup-title');
    const modal = $('channel-setup-modal');
    const footer = $('channel-setup-save') as HTMLButtonElement | null;
    if (!body || !title || !modal) return;

    title.textContent = 'Add Channel';
    if (footer) footer.style.display = 'none';

    let html = '<div class="channel-picker-grid">';
    for (const def of CHANNEL_SETUPS) {
      html += `<button class="channel-pick-btn" data-ch-pick="${def.id}">
        <span class="channel-pick-icon ${CHANNEL_CLASSES[def.id] ?? 'default'}">${def.icon}</span>
        <span>${escHtml(def.name)}</span>
      </button>`;
    }
    html += '</div>';
    body.innerHTML = html;

    body.querySelectorAll('[data-ch-pick]').forEach((btn) => {
      btn.addEventListener('click', () => {
        if (footer) footer.style.display = '';
        openChannelSetup((btn as HTMLElement).dataset.chPick!);
      });
    });

    modal.style.display = '';
  });

  document.querySelectorAll('#channels-picker-empty .channel-pick-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      const chType = (btn as HTMLElement).dataset.chType;
      if (chType) openChannelSetup(chType);
    });
  });

  $('refresh-channels-btn')?.addEventListener('click', () => loadChannels());

  $('channel-send-btn')?.addEventListener('click', async () => {
    const target = ($('channel-send-target') as HTMLSelectElement)?.value;
    const msgInput = $('channel-send-message') as HTMLInputElement;
    const message = msgInput?.value.trim();
    if (!target || !message || !appState.wsConnected) return;
    try {
      await pawEngine.chatSend(target, message);
      showToast(`Sent to ${target}`, 'success');
      if (msgInput) msgInput.value = '';
    } catch (e) {
      showToast(`Send failed: ${e instanceof Error ? e.message : e}`, 'error');
    }
  });

  $('add-cron-btn')?.addEventListener('click', showCronModal);
  $('cron-empty-add')?.addEventListener('click', showCronModal);
  $('cron-modal-close')?.addEventListener('click', hideCronModal);
  $('cron-modal-cancel')?.addEventListener('click', hideCronModal);

  $('cron-form-schedule-preset')?.addEventListener('change', () => {
    const preset = ($('cron-form-schedule-preset') as HTMLSelectElement).value;
    const scheduleInput = $('cron-form-schedule') as HTMLInputElement;
    if (preset && scheduleInput) scheduleInput.value = preset;
  });

  $('cron-modal-save')?.addEventListener('click', async () => {
    showToast('Automations scheduler coming soon', 'info');
    hideCronModal();
  });

  $('memory-editor-save')?.addEventListener('click', async () => {
    showToast('Use Memory Palace for file management', 'info');
  });

  $('memory-editor-close')?.addEventListener('click', () => {
    const editor = $('memory-editor');
    if (editor) editor.style.display = 'none';
  });

  $('refresh-memory-btn')?.addEventListener('click', () => loadMemory());
}

// ── Auto-start configured channel bridges on app boot ─────────────────────

/** Auto-connect all channels that are enabled and have credentials. Called once at startup. */
export async function autoStartConfiguredChannels(): Promise<void> {
  try {
    const tgCfg = await pawEngine.telegramGetConfig();
    if (tgCfg.enabled && tgCfg.bot_token) {
      const tgStatus = await pawEngine.telegramStatus();
      if (!tgStatus.running) {
        await pawEngine.telegramStart();
        console.debug('[channels] Auto-started Telegram bridge');
      }
    }
  } catch (e) {
    console.warn('[channels] Telegram auto-start skipped:', e);
  }

  const channels = [
    'discord',
    'irc',
    'slack',
    'matrix',
    'mattermost',
    'nextcloud',
    'nostr',
    'twitch',
  ] as const;
  for (const ch of channels) {
    try {
      const cfg = await getChannelConfig(ch);
      if (cfg && (cfg as Record<string, unknown>).enabled) {
        const status = await getChannelStatus(ch);
        if (status && !status.running) {
          await startChannel(ch);
          console.debug(`[channels] Auto-started ${ch} bridge`);
        }
      }
    } catch (e) {
      console.warn(`[channels] ${ch} auto-start skipped:`, e);
    }
  }
}

// ── Re-exports (maintain public interface for existing callers) ────────────

export { loadChannels } from './molecules';
export { openChannelSetup, closeChannelSetup, saveChannelSetup } from './setup';
export { getChannelConfig, getChannelStatus, startChannel } from './molecules';
