// setup.ts — Channel setup modal, per-channel save logic
// Depends on: atoms, molecules, engine, helpers, toast, mail

import { pawEngine } from '../../engine';
import { $, escHtml, escAttr } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { CHANNEL_SETUPS, type ChannelField } from './atoms';
import { getChannelConfig, setChannelConfig, startChannel, loadChannels } from './molecules';
import { getChannelSetupType, saveMailImapSetup, clearChannelSetupType } from '../mail';

// ── Module state ───────────────────────────────────────────────────────────

let _channelSetupType: string | null = null;

export function getSetupType(): string | null {
  return _channelSetupType;
}
export function clearSetupType() {
  _channelSetupType = null;
}

// ── Open channel setup modal ───────────────────────────────────────────────

export async function openChannelSetup(channelType: string) {
  const def = CHANNEL_SETUPS.find((c) => c.id === channelType);
  if (!def) return;
  _channelSetupType = channelType;

  const title = $('channel-setup-title');
  const body = $('channel-setup-body');
  const modal = $('channel-setup-modal');
  if (!title || !body || !modal) return;

  title.textContent = `Set Up ${def.name}`;

  const existingValues: Record<string, string> = {};
  try {
    if (channelType === 'telegram') {
      const cfg = await pawEngine.telegramGetConfig();
      if (cfg.bot_token) existingValues['botToken'] = cfg.bot_token;
      if (cfg.dm_policy) existingValues['dmPolicy'] = cfg.dm_policy;
      if (cfg.allowed_users?.length) existingValues['allowFrom'] = cfg.allowed_users.join(', ');
      if (cfg.agent_id) existingValues['agentId'] = cfg.agent_id;
    } else if (channelType === 'discord') {
      const cfg = await pawEngine.discordGetConfig();
      if (cfg.bot_token) existingValues['botToken'] = cfg.bot_token;
      if (cfg.dm_policy) existingValues['dmPolicy'] = cfg.dm_policy;
      if (cfg.agent_id) existingValues['agentId'] = cfg.agent_id;
    } else if (channelType === 'irc') {
      const cfg = await pawEngine.ircGetConfig();
      if (cfg.server) existingValues['server'] = cfg.server;
      if (cfg.port) existingValues['port'] = String(cfg.port);
      if (cfg.nick) existingValues['nick'] = cfg.nick;
      if (cfg.password) existingValues['password'] = cfg.password;
      if (cfg.channels_to_join?.length)
        existingValues['channels'] = cfg.channels_to_join.join(', ');
    } else if (channelType === 'slack') {
      const cfg = await pawEngine.slackGetConfig();
      if (cfg.bot_token) existingValues['botToken'] = cfg.bot_token;
      if (cfg.app_token) existingValues['appToken'] = cfg.app_token;
      if (cfg.dm_policy) existingValues['dmPolicy'] = cfg.dm_policy;
      if (cfg.agent_id) existingValues['agentId'] = cfg.agent_id;
    } else if (channelType === 'matrix') {
      const cfg = await pawEngine.matrixGetConfig();
      if (cfg.homeserver) existingValues['homeserver'] = cfg.homeserver;
      if (cfg.access_token) existingValues['accessToken'] = cfg.access_token;
      if (cfg.dm_policy) existingValues['dmPolicy'] = cfg.dm_policy;
      if (cfg.agent_id) existingValues['agentId'] = cfg.agent_id;
    } else if (channelType === 'mattermost') {
      const cfg = await pawEngine.mattermostGetConfig();
      if (cfg.server_url) existingValues['serverUrl'] = cfg.server_url;
      if (cfg.token) existingValues['token'] = cfg.token;
      if (cfg.dm_policy) existingValues['dmPolicy'] = cfg.dm_policy;
      if (cfg.agent_id) existingValues['agentId'] = cfg.agent_id;
    } else if (channelType === 'nextcloud') {
      const cfg = await pawEngine.nextcloudGetConfig();
      if (cfg.server_url) existingValues['serverUrl'] = cfg.server_url;
      if (cfg.username) existingValues['username'] = cfg.username;
      if (cfg.password) existingValues['password'] = cfg.password;
      if (cfg.dm_policy) existingValues['dmPolicy'] = cfg.dm_policy;
      if (cfg.agent_id) existingValues['agentId'] = cfg.agent_id;
    } else if (channelType === 'nostr') {
      const cfg = await pawEngine.nostrGetConfig();
      if (cfg.private_key_hex) existingValues['privateKeyHex'] = cfg.private_key_hex;
      if (cfg.relays?.length) existingValues['relays'] = cfg.relays.join(', ');
      if (cfg.dm_policy) existingValues['dmPolicy'] = cfg.dm_policy;
      if (cfg.agent_id) existingValues['agentId'] = cfg.agent_id;
    } else if (channelType === 'twitch') {
      const cfg = await pawEngine.twitchGetConfig();
      if (cfg.oauth_token) existingValues['oauthToken'] = cfg.oauth_token;
      if (cfg.bot_username) existingValues['botUsername'] = cfg.bot_username;
      if (cfg.channels_to_join?.length)
        existingValues['channels'] = cfg.channels_to_join.join(', ');
      if (cfg.dm_policy) existingValues['dmPolicy'] = cfg.dm_policy;
      if (cfg.agent_id) existingValues['agentId'] = cfg.agent_id;
    } else if (channelType === 'whatsapp') {
      const cfg = await pawEngine.whatsappGetConfig();
      if (cfg.api_port) existingValues['apiPort'] = String(cfg.api_port);
      if (cfg.webhook_port) existingValues['webhookPort'] = String(cfg.webhook_port);
      if (cfg.dm_policy) existingValues['dmPolicy'] = cfg.dm_policy;
      if (cfg.allowed_users?.length) existingValues['allowFrom'] = cfg.allowed_users.join(', ');
      if (cfg.agent_id) existingValues['agentId'] = cfg.agent_id;
    }
  } catch {
    /* no existing config */
  }

  let html = def.descriptionHtml
    ? `<div class="channel-setup-desc">${def.descriptionHtml}</div>`
    : `<p class="channel-setup-desc">${escHtml(def.description)}</p>`;

  // Group fields: regular first, then "Advanced." hint fields in a collapsible
  const regularFields = def.fields.filter((f) => !f.hint?.startsWith('Advanced.'));
  const advancedFields = def.fields.filter((f) => f.hint?.startsWith('Advanced.'));

  const renderField = (field: ChannelField) => {
    let fhtml = `<div class="form-group">`;
    fhtml += `<label class="form-label" for="ch-field-${field.key}">${escHtml(field.label)}${field.required ? ' <span class="required">*</span>' : ''}</label>`;

    const existVal = existingValues[field.key];

    if (field.type === 'select' && field.options) {
      fhtml += `<select class="form-input" id="ch-field-${field.key}" data-ch-field="${field.key}">`;
      for (const opt of field.options) {
        const selVal = existVal ?? field.defaultValue ?? '';
        const sel = opt.value === selVal ? ' selected' : '';
        fhtml += `<option value="${escAttr(opt.value)}"${sel}>${escHtml(opt.label)}</option>`;
      }
      fhtml += `</select>`;
    } else if (field.type === 'toggle') {
      const checked = field.defaultValue ? ' checked' : '';
      fhtml += `<label class="toggle-label"><input type="checkbox" id="ch-field-${field.key}" data-ch-field="${field.key}"${checked}> Enabled</label>`;
    } else {
      const inputType = field.type === 'password' ? 'password' : 'text';
      const populateVal =
        existVal ?? (typeof field.defaultValue === 'string' ? field.defaultValue : '');
      const val = populateVal ? ` value="${escAttr(populateVal)}"` : '';
      fhtml += `<input class="form-input" id="ch-field-${field.key}" data-ch-field="${field.key}" type="${inputType}" placeholder="${escAttr(field.placeholder ?? '')}"${val}>`;
    }

    if (field.hint) {
      const hintText = field.hint.startsWith('Advanced.') ? field.hint.slice(10) : field.hint;
      fhtml += `<div class="form-hint">${escHtml(hintText)}</div>`;
    }
    fhtml += `</div>`;
    return fhtml;
  };

  for (const field of regularFields) {
    html += renderField(field);
  }

  if (advancedFields.length > 0) {
    html += `<details class="advanced-toggle"><summary>Advanced settings</summary>`;
    for (const field of advancedFields) {
      html += renderField(field);
    }
    html += `</details>`;
  }

  body.innerHTML = html;
  modal.style.display = '';
}

// ── Close setup modal ──────────────────────────────────────────────────────

export function closeChannelSetup() {
  const modal = $('channel-setup-modal');
  if (modal) modal.style.display = 'none';
  _channelSetupType = null;
}

// ── Save channel setup ─────────────────────────────────────────────────────

export async function saveChannelSetup() {
  console.debug(
    '[mail-debug] saveChannelSetup called, _channelSetupType=',
    _channelSetupType,
    'mailType=',
    getChannelSetupType(),
  );
  if (_channelSetupType === '__mail_imap__' || getChannelSetupType() === '__mail_imap__') {
    console.debug('[mail-debug] Routing to MailModule.saveMailImapSetup()');
    await saveMailImapSetup();
    clearChannelSetupType();
    _channelSetupType = null;
    return;
  }

  if (!_channelSetupType) return;

  // ── Telegram ────────────────────────────────────────────────────────────
  if (_channelSetupType === 'telegram') {
    const saveBtn = $('channel-setup-save') as HTMLButtonElement | null;
    if (saveBtn) {
      saveBtn.disabled = true;
      saveBtn.textContent = 'Saving...';
    }

    try {
      const botToken = ($('ch-field-botToken') as HTMLInputElement)?.value?.trim() ?? '';
      const dmPolicy = ($('ch-field-dmPolicy') as HTMLSelectElement)?.value ?? 'pairing';
      const allowFrom = ($('ch-field-allowFrom') as HTMLInputElement)?.value?.trim() ?? '';
      const agentId = ($('ch-field-agentId') as HTMLInputElement)?.value?.trim() ?? '';

      if (!botToken) {
        showToast('Bot token is required', 'error');
        if (saveBtn) {
          saveBtn.disabled = false;
          saveBtn.textContent = 'Save & Connect';
        }
        return;
      }

      let existing;
      try {
        existing = await pawEngine.telegramGetConfig();
      } catch {
        existing = null;
      }

      const allowedUsers = allowFrom
        ? allowFrom
            .split(',')
            .map((s) => parseInt(s.trim()))
            .filter((n) => !isNaN(n))
        : (existing?.allowed_users ?? []);

      const config = {
        bot_token: botToken,
        enabled: true,
        dm_policy: dmPolicy,
        allowed_users: allowedUsers,
        pending_users: existing?.pending_users ?? [],
        agent_id: agentId || undefined,
      };

      await pawEngine.telegramSetConfig(config);
      showToast('Telegram configured!', 'success');
      closeChannelSetup();

      try {
        await pawEngine.telegramStart();
        showToast('Telegram bridge started', 'success');
      } catch (e) {
        console.warn('Auto-start failed:', e);
      }

      loadChannels();
    } catch (e) {
      showToast(`Failed to save: ${e instanceof Error ? e.message : e}`, 'error');
    } finally {
      if (saveBtn) {
        $('channel-setup-save') &&
          (($('channel-setup-save') as HTMLButtonElement).disabled = false);
        ($('channel-setup-save') as HTMLButtonElement | null) &&
          (($('channel-setup-save') as HTMLButtonElement).textContent = 'Save & Connect');
      }
    }
    return;
  }

  // ── Generic channel save ────────────────────────────────────────────────
  const _chDef = CHANNEL_SETUPS.find((c) => c.id === _channelSetupType);
  if (_chDef) {
    const saveBtn = $('channel-setup-save') as HTMLButtonElement | null;
    if (saveBtn) {
      saveBtn.disabled = true;
      saveBtn.textContent = 'Saving...';
    }

    try {
      const values: Record<string, string | boolean> = {};
      for (const field of _chDef.fields) {
        const el = $(`ch-field-${field.key}`);
        if (!el) continue;
        if (field.type === 'toggle') {
          values[field.key] = (el as HTMLInputElement).checked;
        } else {
          values[field.key] = ((el as HTMLInputElement).value ?? '').trim();
        }
      }

      for (const field of _chDef.fields) {
        if (field.required && !values[field.key]) {
          showToast(`${field.label} is required`, 'error');
          if (saveBtn) {
            saveBtn.disabled = false;
            saveBtn.textContent = 'Save & Connect';
          }
          return;
        }
      }

      const configPatch = _chDef.buildConfig(values);

      const channelType = _channelSetupType;
      let existingConfig = await getChannelConfig(channelType);

      // WhatsApp: if no existing config, create a proper default so all required fields are present
      if (!existingConfig && channelType === 'whatsapp') {
        existingConfig = {
          enabled: false,
          instance_name: 'paw',
          api_url: 'http://127.0.0.1:8085',
          api_key: `paw-wa-${Math.random().toString(36).slice(2, 18)}`,
          api_port: 8085,
          webhook_port: 8086,
          dm_policy: 'pairing',
          allowed_users: [],
          pending_users: [],
          respond_in_groups: false,
          session_connected: false,
        };
      }

      const finalConfig: Record<string, unknown> = {
        ...existingConfig,
        ...configPatch,
        enabled: true,
        allowed_users: (existingConfig as Record<string, unknown> | null)?.allowed_users ?? [],
        pending_users: (existingConfig as Record<string, unknown> | null)?.pending_users ?? [],
      };
      if (values['agentId']) finalConfig.agent_id = values['agentId'] as string;

      await setChannelConfig(channelType, finalConfig);
      showToast(`${_chDef.name} configured!`, 'success');
      closeChannelSetup();

      if (channelType === 'whatsapp') {
        // For WhatsApp, render the channel card first so the Start button's
        // event listener (which sets up the QR code banner) is in the DOM,
        // then auto-click Start to trigger the full WhatsApp startup flow.
        await loadChannels();
        const waStartBtn = document.getElementById('ch-whatsapp-start');
        if (waStartBtn) waStartBtn.click();
      } else {
        try {
          await startChannel(channelType);
          showToast(`${_chDef.name} bridge started`, 'success');
        } catch (e) {
          console.warn('Auto-start failed:', e);
        }
        loadChannels();
      }
    } catch (e) {
      showToast(`Failed to save: ${e instanceof Error ? e.message : e}`, 'error');
    } finally {
      const btn = $('channel-setup-save') as HTMLButtonElement | null;
      if (btn) {
        btn.disabled = false;
        btn.textContent = 'Save & Connect';
      }
    }
    return;
  }

  showToast(`Unknown channel type: ${_channelSetupType}`, 'error');
}
