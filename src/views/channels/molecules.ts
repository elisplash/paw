// molecules.ts — Channel card rendering, IPC operations, status display
// Depends on: atoms, engine, helpers, toast

import { pawEngine, type ChannelStatus } from '../../engine';
import { $, escHtml, escAttr, confirmModal } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { CHANNEL_CLASSES, CHANNEL_SETUPS, isChannelConfigured, emptyChannelConfig } from './atoms';

// ── Injected dependency (set by index.ts to break circular import) ─────────

let _openChannelSetup: (channelType: string) => void = () => {};
export function setOpenChannelSetup(fn: (channelType: string) => void): void {
  _openChannelSetup = fn;
}

// ── Channel Operation Helpers ──────────────────────────────────────────────

export async function getChannelConfig(ch: string): Promise<Record<string, unknown> | null> {
  try {
    switch (ch) {
      case 'discord':
        return (await pawEngine.discordGetConfig()) as unknown as Record<string, unknown>;
      case 'irc':
        return (await pawEngine.ircGetConfig()) as unknown as Record<string, unknown>;
      case 'slack':
        return (await pawEngine.slackGetConfig()) as unknown as Record<string, unknown>;
      case 'matrix':
        return (await pawEngine.matrixGetConfig()) as unknown as Record<string, unknown>;
      case 'mattermost':
        return (await pawEngine.mattermostGetConfig()) as unknown as Record<string, unknown>;
      case 'nextcloud':
        return (await pawEngine.nextcloudGetConfig()) as unknown as Record<string, unknown>;
      case 'nostr':
        return (await pawEngine.nostrGetConfig()) as unknown as Record<string, unknown>;
      case 'twitch':
        return (await pawEngine.twitchGetConfig()) as unknown as Record<string, unknown>;
      case 'whatsapp':
        return (await pawEngine.whatsappGetConfig()) as unknown as Record<string, unknown>;
      default:
        return null;
    }
  } catch {
    return null;
  }
}

export async function setChannelConfig(ch: string, config: Record<string, unknown>): Promise<void> {
  switch (ch) {
    case 'discord':
      return pawEngine.discordSetConfig(config as never);
    case 'irc':
      return pawEngine.ircSetConfig(config as never);
    case 'slack':
      return pawEngine.slackSetConfig(config as never);
    case 'matrix':
      return pawEngine.matrixSetConfig(config as never);
    case 'mattermost':
      return pawEngine.mattermostSetConfig(config as never);
    case 'nextcloud':
      return pawEngine.nextcloudSetConfig(config as never);
    case 'nostr':
      return pawEngine.nostrSetConfig(config as never);
    case 'twitch':
      return pawEngine.twitchSetConfig(config as never);
    case 'whatsapp':
      return pawEngine.whatsappSetConfig(config as never);
  }
}

export async function startChannel(ch: string): Promise<void> {
  switch (ch) {
    case 'discord':
      return pawEngine.discordStart();
    case 'irc':
      return pawEngine.ircStart();
    case 'slack':
      return pawEngine.slackStart();
    case 'matrix':
      return pawEngine.matrixStart();
    case 'mattermost':
      return pawEngine.mattermostStart();
    case 'nextcloud':
      return pawEngine.nextcloudStart();
    case 'nostr':
      return pawEngine.nostrStart();
    case 'twitch':
      return pawEngine.twitchStart();
    case 'whatsapp':
      return pawEngine.whatsappStart();
  }
}

export async function stopChannel(ch: string): Promise<void> {
  switch (ch) {
    case 'discord':
      return pawEngine.discordStop();
    case 'irc':
      return pawEngine.ircStop();
    case 'slack':
      return pawEngine.slackStop();
    case 'matrix':
      return pawEngine.matrixStop();
    case 'mattermost':
      return pawEngine.mattermostStop();
    case 'nextcloud':
      return pawEngine.nextcloudStop();
    case 'nostr':
      return pawEngine.nostrStop();
    case 'twitch':
      return pawEngine.twitchStop();
    case 'whatsapp':
      return pawEngine.whatsappStop();
  }
}

export async function getChannelStatus(ch: string): Promise<ChannelStatus | null> {
  try {
    switch (ch) {
      case 'discord':
        return await pawEngine.discordStatus();
      case 'irc':
        return await pawEngine.ircStatus();
      case 'slack':
        return await pawEngine.slackStatus();
      case 'matrix':
        return await pawEngine.matrixStatus();
      case 'mattermost':
        return await pawEngine.mattermostStatus();
      case 'nextcloud':
        return await pawEngine.nextcloudStatus();
      case 'nostr':
        return await pawEngine.nostrStatus();
      case 'twitch':
        return await pawEngine.twitchStatus();
      case 'whatsapp':
        return await pawEngine.whatsappStatus();
      default:
        return null;
    }
  } catch {
    return null;
  }
}

async function approveChannelUser(ch: string, userId: string): Promise<void> {
  switch (ch) {
    case 'discord':
      return pawEngine.discordApproveUser(userId);
    case 'irc':
      return pawEngine.ircApproveUser(userId);
    case 'slack':
      return pawEngine.slackApproveUser(userId);
    case 'matrix':
      return pawEngine.matrixApproveUser(userId);
    case 'mattermost':
      return pawEngine.mattermostApproveUser(userId);
    case 'nextcloud':
      return pawEngine.nextcloudApproveUser(userId);
    case 'nostr':
      return pawEngine.nostrApproveUser(userId);
    case 'twitch':
      return pawEngine.twitchApproveUser(userId);
    case 'whatsapp':
      return pawEngine.whatsappApproveUser(userId);
  }
}

async function denyChannelUser(ch: string, userId: string): Promise<void> {
  switch (ch) {
    case 'discord':
      return pawEngine.discordDenyUser(userId);
    case 'irc':
      return pawEngine.ircDenyUser(userId);
    case 'slack':
      return pawEngine.slackDenyUser(userId);
    case 'matrix':
      return pawEngine.matrixDenyUser(userId);
    case 'mattermost':
      return pawEngine.mattermostDenyUser(userId);
    case 'nextcloud':
      return pawEngine.nextcloudDenyUser(userId);
    case 'nostr':
      return pawEngine.nostrDenyUser(userId);
    case 'twitch':
      return pawEngine.twitchDenyUser(userId);
    case 'whatsapp':
      return pawEngine.whatsappDenyUser(userId);
  }
}

// ── loadChannels — render all configured channel cards ─────────────────────

/** Build pending-users approval section for any channel */
function renderPendingSection(
  name: string,
  ch: string,
  pendingUsers: Array<{
    display_name?: string;
    username: string;
    user_id: string;
    first_name?: string;
  }>,
): HTMLElement {
  const section = document.createElement('div');
  section.className = 'channel-pairing-section';
  section.style.cssText =
    'margin-top:8px;border:1px solid var(--border);border-radius:8px;padding:12px;';
  section.innerHTML = `<h4 style="font-size:13px;font-weight:600;margin:0 0 8px 0">${escHtml(name)} — Pending Requests</h4>`;
  for (const p of pendingUsers) {
    const row = document.createElement('div');
    row.style.cssText =
      'display:flex;align-items:center;justify-content:space-between;padding:6px 0;border-bottom:1px solid var(--border-light,rgba(255,255,255,0.06))';
    const displayName = p.display_name || p.first_name || p.username;
    row.innerHTML = `<div><strong>${escHtml(displayName)}</strong> <span style="color:var(--text-muted);font-size:12px">${escHtml(p.user_id)}</span></div>
      <div style="display:flex;gap:6px"><button class="btn btn-primary btn-sm ch-approve" data-ch="${ch}" data-uid="${escAttr(p.user_id)}">Approve</button><button class="btn btn-danger btn-sm ch-deny" data-ch="${ch}" data-uid="${escAttr(p.user_id)}">Deny</button></div>`;
    section.appendChild(row);
  }
  return section;
}

/** Bind approve/deny buttons inside a pending section */
function bindPendingActions(section: HTMLElement): void {
  section.querySelectorAll('.ch-approve').forEach((btn) =>
    btn.addEventListener('click', async () => {
      const _ch = (btn as HTMLElement).dataset.ch!;
      const _uid = (btn as HTMLElement).dataset.uid!;
      try {
        await approveChannelUser(_ch, _uid);
        showToast('Approved', 'success');
        loadChannels();
      } catch (e) {
        showToast(`${e}`, 'error');
      }
    }),
  );
  section.querySelectorAll('.ch-deny').forEach((btn) =>
    btn.addEventListener('click', async () => {
      const _ch = (btn as HTMLElement).dataset.ch!;
      const _uid = (btn as HTMLElement).dataset.uid!;
      try {
        await denyChannelUser(_ch, _uid);
        showToast('Denied', 'success');
        loadChannels();
      } catch (e) {
        showToast(`${e}`, 'error');
      }
    }),
  );
}

/** Build the card HTML for any channel */
function buildChannelCardHtml(
  ch: string,
  name: string,
  iconStr: string,
  isConnected: boolean,
  status: ChannelStatus,
): string {
  const cardId = `ch-${ch}`;
  return `
    <div class="channel-card-header">
      <div class="channel-card-icon ${CHANNEL_CLASSES[ch] ?? 'default'}">${iconStr}</div>
      <div>
        <div class="channel-card-title">${escHtml(name)}${status.bot_name ? ` — ${escHtml(status.bot_name)}` : ''}</div>
        <div class="channel-card-status">
          <span class="status-dot ${isConnected ? 'connected' : 'error'}"></span>
          <span>${isConnected ? 'Connected' : 'Not running'}</span>
        </div>
      </div>
    </div>
    ${isConnected ? `<div class="channel-card-accounts" style="font-size:12px;color:var(--text-muted)">${status.message_count} messages · Policy: ${escHtml(status.dm_policy)}</div>` : ''}
    <div class="channel-card-actions">
      ${!isConnected ? `<button class="btn btn-primary btn-sm" id="${cardId}-start">Start</button>` : ''}
      ${isConnected ? `<button class="btn btn-ghost btn-sm" id="${cardId}-stop">Stop</button>` : ''}
      <button class="btn btn-ghost btn-sm" id="${cardId}-edit">Edit</button>
      <button class="btn btn-ghost btn-sm" id="${cardId}-remove">Remove</button>
    </div>`;
}

/** Handle WhatsApp-specific start flow with real-time status events */
async function handleWhatsAppStart(card: HTMLElement, cardId: string): Promise<void> {
  // Hide Start button immediately
  const startBtn = document.getElementById(`${cardId}-start`) as HTMLButtonElement | null;
  if (startBtn) {
    startBtn.disabled = true;
    startBtn.textContent = 'Starting...';
    startBtn.style.opacity = '0.5';
  }

  const statusBannerId = `${cardId}-status-banner`;
  let banner = document.getElementById(statusBannerId);
  if (!banner) {
    banner = document.createElement('div');
    banner.id = statusBannerId;
    banner.className = 'wa-status-banner';
    card.appendChild(banner);
  }
  banner.innerHTML = '<span class="wa-spinner"></span> Setting up WhatsApp...';
  banner.style.display = 'flex';

  const { listen } = await import('@tauri-apps/api/event');
  let gotMeaningfulEvent = false;
  const unlisten = await listen<{ kind: string; message?: string; qr?: string }>(
    'whatsapp-status',
    (event) => {
      const { kind, message, qr } = event.payload;
      if (!banner) return;

      if (!gotMeaningfulEvent && (kind === 'disconnected' || kind === 'error')) {
        console.debug('[wa-ui] Ignoring stale event from old bridge:', kind);
        return;
      }

      switch (kind) {
        case 'docker_starting':
        case 'docker_ready':
        case 'starting':
          banner.innerHTML = `<span class="wa-spinner"></span> Setting up WhatsApp...`;
          break;
        case 'installing':
          banner.innerHTML = `<span class="wa-spinner"></span> Installing WhatsApp service (first time only — this may take a minute)...`;
          break;
        case 'install_failed':
          banner.innerHTML = `<span class="wa-status-icon">⚠️</span> <span>Couldn't set up WhatsApp automatically. Check your internet connection and try again.</span>`;
          banner.className = 'wa-status-banner wa-status-error';
          break;
        case 'docker_timeout':
          banner.innerHTML = `<span class="wa-status-icon">⏱️</span> <span>WhatsApp is still loading. Give it a moment and click Start again.</span>`;
          banner.className = 'wa-status-banner wa-status-warning';
          break;
        case 'downloading':
          banner.innerHTML = `<span class="wa-spinner"></span> First-time setup — downloading WhatsApp service (this may take a minute)...`;
          break;
        case 'connecting':
          gotMeaningfulEvent = true;
          banner.innerHTML = `<span class="wa-spinner"></span> Connecting to WhatsApp...`;
          break;
        case 'qr_code':
          banner.innerHTML = `<div class="wa-qr-section"><p style="margin:0 0 4px;font-weight:600">Scan with the agent's phone — not your personal one</p><p style="font-size:12px;color:var(--text-muted);margin:0 0 10px">The number you scan becomes the agent. Use a separate number.</p>${qr ? `<img src="${qr.startsWith('data:') ? qr : `data:image/png;base64,${qr}`}" alt="WhatsApp QR code" class="wa-qr-image" />` : ''}<p style="font-size:12px;color:var(--text-muted);margin:8px 0 0">Open WhatsApp → Settings → Linked Devices → Link a Device</p></div>`;
          banner.className = 'wa-status-banner wa-status-qr';
          break;
        case 'connected':
          banner.innerHTML = `<span class="wa-status-icon">✅</span> ${escHtml(message ?? 'WhatsApp connected!')}`;
          banner.className = 'wa-status-banner wa-status-success';
          setTimeout(() => {
            banner!.style.display = 'none';
            unlisten();
            loadChannels();
          }, 2000);
          break;
        case 'disconnected':
          banner.style.display = 'none';
          unlisten();
          loadChannels();
          break;
        case 'error':
          banner.innerHTML = `<span class="wa-status-icon">❌</span> ${escHtml(message ?? 'Something went wrong')}`;
          banner.className = 'wa-status-banner wa-status-error';
          unlisten();
          setTimeout(() => loadChannels(), 2000);
          break;
      }
    },
  );
}

/** Bind start/stop/edit/remove actions for a generic channel card */
function bindChannelCardActions(card: HTMLElement, ch: string, name: string): void {
  const cardId = `ch-${ch}`;
  $(`${cardId}-start`)?.addEventListener('click', async () => {
    try {
      if (ch === 'whatsapp') await handleWhatsAppStart(card, cardId);
      await startChannel(ch);
      if (ch !== 'whatsapp') {
        showToast(`${name} started`, 'success');
        loadChannels();
      }
    } catch (e) {
      const statusBanner = document.getElementById(`${cardId}-status-banner`);
      if (ch === 'whatsapp' && statusBanner) {
        const errMsg = e instanceof Error ? e.message : String(e);
        if (errMsg.includes('automatically') || errMsg.includes('internet')) {
          statusBanner.innerHTML = `<span class="wa-status-icon">⚠️</span> <span>Couldn't set up WhatsApp. Check your internet connection and try again.</span>`;
        } else if (errMsg.includes("didn't start in time") || errMsg.includes('timeout')) {
          statusBanner.innerHTML = `<span class="wa-status-icon">⏱️</span> <span>WhatsApp is still loading. Give it a moment and try again.</span>`;
        } else {
          statusBanner.innerHTML = `<span class="wa-status-icon">❌</span> ${escHtml(errMsg)}`;
        }
        statusBanner.className = 'wa-status-banner wa-status-error';
      } else {
        showToast(`Start failed: ${e}`, 'error');
      }
    }
  });
  $(`${cardId}-stop`)?.addEventListener('click', async () => {
    try {
      await stopChannel(ch);
      showToast(`${name} stopped`, 'success');
      loadChannels();
    } catch (e) {
      showToast(`Stop failed: ${e}`, 'error');
    }
  });
  $(`${cardId}-edit`)?.addEventListener('click', () => _openChannelSetup(ch));
  $(`${cardId}-remove`)?.addEventListener('click', async function removeHandler() {
    const btn = this as HTMLButtonElement;
    if (btn.dataset.confirm !== 'yes') {
      btn.dataset.confirm = 'yes';
      btn.textContent = 'Confirm?';
      btn.classList.add('btn-danger');
      btn.classList.remove('btn-ghost');
      setTimeout(() => {
        btn.dataset.confirm = '';
        btn.textContent = 'Remove';
        btn.classList.remove('btn-danger');
        btn.classList.add('btn-ghost');
      }, 3000);
      return;
    }
    try {
      await stopChannel(ch);
      const emptyConfig = emptyChannelConfig(ch);
      await setChannelConfig(ch, emptyConfig);
      showToast(`${name} removed`, 'success');
      loadChannels();
    } catch (e) {
      showToast(`Remove failed: ${e}`, 'error');
    }
  });
}

export async function loadChannels() {
  const list = $('channels-list');
  const empty = $('channels-empty');
  const loading = $('channels-loading');
  if (!list) return;

  if (loading) loading.style.display = '';
  if (empty) empty.style.display = 'none';
  list.innerHTML = '';

  try {
    let anyConfigured = false;

    // ── Telegram ────────────────────────────────────────────────────────
    try {
      const tgStatus = await pawEngine.telegramStatus();
      const tgConfig = await pawEngine.telegramGetConfig();
      const tgConfigured = !!tgConfig.bot_token;
      if (tgConfigured) {
        anyConfigured = true;
        const tgConnected = tgStatus.running && tgStatus.connected;
        const cardId = 'ch-telegram';
        const tgCard = document.createElement('div');
        tgCard.className = 'channel-card';
        tgCard.innerHTML = `
          <div class="channel-card-header">
            <div class="channel-card-icon telegram">TG</div>
            <div>
              <div class="channel-card-title">Telegram${tgStatus.bot_username ? ` — @${escHtml(tgStatus.bot_username)}` : ''}</div>
              <div class="channel-card-status">
                <span class="status-dot ${tgConnected ? 'connected' : 'error'}"></span>
                <span>${tgConnected ? 'Connected' : 'Not running'}</span>
              </div>
            </div>
          </div>
          ${tgConnected ? `<div class="channel-card-accounts" style="font-size:12px;color:var(--text-muted)">${tgStatus.message_count} messages · Policy: ${escHtml(tgStatus.dm_policy)}</div>` : ''}
          <div class="channel-card-actions">
            ${!tgConnected ? `<button class="btn btn-primary btn-sm" id="${cardId}-start">Start</button>` : ''}
            ${tgConnected ? `<button class="btn btn-ghost btn-sm" id="${cardId}-stop">Stop</button>` : ''}
            <button class="btn btn-ghost btn-sm" id="${cardId}-edit">Edit</button>
            <button class="btn btn-ghost btn-sm" id="${cardId}-remove">Remove</button>
          </div>`;
        list.appendChild(tgCard);

        $(`${cardId}-start`)?.addEventListener('click', async () => {
          try {
            await pawEngine.telegramStart();
            showToast('Telegram started', 'success');
            loadChannels();
          } catch (e) {
            showToast(`Start failed: ${e}`, 'error');
          }
        });
        $(`${cardId}-stop`)?.addEventListener('click', async () => {
          try {
            await pawEngine.telegramStop();
            showToast('Telegram stopped', 'success');
            loadChannels();
          } catch (e) {
            showToast(`Stop failed: ${e}`, 'error');
          }
        });
        $(`${cardId}-edit`)?.addEventListener('click', () => _openChannelSetup('telegram'));
        $(`${cardId}-remove`)?.addEventListener('click', async () => {
          if (!(await confirmModal('Remove Telegram configuration?'))) return;
          try {
            await pawEngine.telegramStop();
            await pawEngine.telegramSetConfig({
              bot_token: '',
              enabled: false,
              dm_policy: 'pairing',
              allowed_users: [],
              pending_users: [],
            });
            showToast('Telegram removed', 'success');
            loadChannels();
          } catch (e) {
            showToast(`Remove failed: ${e}`, 'error');
          }
        });

        if (tgStatus.pending_users.length > 0) {
          const section = renderPendingSection(
            'Telegram',
            'telegram',
            tgStatus.pending_users.map(
              (p: { first_name: string; username: string; user_id: number }) => ({
                display_name: p.first_name,
                username: p.username,
                user_id: String(p.user_id),
                first_name: p.first_name,
              }),
            ),
          );
          list.appendChild(section);
          // Telegram uses numeric user IDs
          section.querySelectorAll('.ch-approve').forEach((btn) =>
            btn.addEventListener('click', async () => {
              try {
                await pawEngine.telegramApproveUser(parseInt((btn as HTMLElement).dataset.uid!));
                showToast('Approved', 'success');
                loadChannels();
              } catch (e) {
                showToast(`${e}`, 'error');
              }
            }),
          );
          section.querySelectorAll('.ch-deny').forEach((btn) =>
            btn.addEventListener('click', async () => {
              try {
                await pawEngine.telegramDenyUser(parseInt((btn as HTMLElement).dataset.uid!));
                showToast('Denied', 'success');
                loadChannels();
              } catch (e) {
                showToast(`${e}`, 'error');
              }
            }),
          );
        }
      }
    } catch {
      /* no telegram */
    }

    // ── Generic Channels ─────────────────────────────────────────────────
    const genericChannels = [
      'discord',
      'irc',
      'slack',
      'matrix',
      'mattermost',
      'nextcloud',
      'nostr',
      'twitch',
      'whatsapp',
    ];

    for (const ch of genericChannels) {
      try {
        const status = await getChannelStatus(ch);
        const config = await getChannelConfig(ch);
        if (!status || !config) continue;

        const _isConfigured = isChannelConfigured(ch, config);
        if (!_isConfigured) continue;

        anyConfigured = true;
        const isConnected = status.running && status.connected;
        const def = CHANNEL_SETUPS.find((c) => c.id === ch);
        const name = def?.name ?? ch;
        const iconStr = def?.icon ?? ch.substring(0, 2).toUpperCase();

        const card = document.createElement('div');
        card.className = 'channel-card';
        card.innerHTML = buildChannelCardHtml(ch, name, iconStr, isConnected, status);
        list.appendChild(card);
        bindChannelCardActions(card, ch, name);

        if (status.pending_users.length > 0) {
          const section = renderPendingSection(name, ch, status.pending_users);
          list.appendChild(section);
          bindPendingActions(section);
        }
      } catch {
        /* skip erroring channel */
      }
    }

    if (loading) loading.style.display = 'none';
    if (!anyConfigured) {
      if (empty) empty.style.display = 'flex';
    }

    const sendSection = $('channel-send-section');
    if (sendSection) sendSection.style.display = 'none';
  } catch (e) {
    console.warn('Channels load failed:', e);
    if (loading) loading.style.display = 'none';
    if (empty) empty.style.display = 'flex';
  }
}
