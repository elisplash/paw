// ── Channels — Connection Hub ──────────────────────────────────────────────
import { pawEngine } from '../engine';
import type { ChannelStatus } from '../engine';
import { escHtml, escAttr } from '../components/molecules/markdown';
import { showToast } from '../components/toast';
import { appState } from '../state/index';
import { getChannelSetupType, saveMailImapSetup, clearChannelSetupType } from './mail';

const $ = (id: string) => document.getElementById(id);

const CHANNEL_CLASSES: Record<string, string> = {
  telegram: 'telegram',
  discord: 'discord',
  irc: 'irc',
  slack: 'slack',
  matrix: 'matrix',
  mattermost: 'mattermost',
  nextcloud: 'nextcloud',
  nostr: 'nostr',
  twitch: 'twitch',
  whatsapp: 'whatsapp',
};

// ── Channel Setup Definitions ──────────────────────────────────────────────
interface ChannelField {
  key: string;
  label: string;
  type: 'text' | 'password' | 'select' | 'toggle';
  placeholder?: string;
  hint?: string;
  required?: boolean;
  options?: { value: string; label: string }[];
  defaultValue?: string | boolean;
  sensitive?: boolean;
}

interface ChannelSetupDef {
  id: string;
  name: string;
  icon: string;
  description: string;
  descriptionHtml?: string;
  fields: ChannelField[];
  buildConfig: (values: Record<string, string | boolean>) => Record<string, unknown>;
}

const CHANNEL_SETUPS: ChannelSetupDef[] = [
  {
    id: 'telegram',
    name: 'Telegram',
    icon: 'TG',
    description: 'Connect your agent to Telegram via a Bot token from @BotFather. No gateway or public URL needed — uses long polling.',
    fields: [
      { key: 'botToken', label: 'Bot Token', type: 'password', placeholder: '123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11', hint: 'Get this from @BotFather on Telegram', required: true, sensitive: true },
      { key: 'dmPolicy', label: 'Access Policy', type: 'select', options: [
        { value: 'pairing', label: 'Pairing (new users must be approved)' },
        { value: 'allowlist', label: 'Allowlist only (pre-approved IDs)' },
        { value: 'open', label: 'Open (anyone can message)' },
      ], defaultValue: 'pairing' },
      { key: 'allowFrom', label: 'Allowed User IDs', type: 'text', placeholder: '123456789, 987654321', hint: 'Telegram user IDs (numbers), comma-separated. Leave blank for pairing mode.' },
      { key: 'agentId', label: 'Agent ID (optional)', type: 'text', placeholder: '', hint: 'Use a specific agent config. Leave blank for default.' },
    ],
    buildConfig: (v) => ({ bot_token: v.botToken as string, enabled: true, dm_policy: v.dmPolicy as string || 'pairing' }),
  },
  {
    id: 'discord',
    name: 'Discord',
    icon: 'DC',
    description: 'Connect to Discord via the Bot Gateway (outbound WebSocket). Create a bot at discord.com/developers → New Application → Bot → Copy Token.',
    fields: [
      { key: 'botToken', label: 'Bot Token', type: 'password', placeholder: 'MTIzNDU2Nzg5MA.XXXXXX.XXXXXXXX', hint: 'Discord Developer Portal → Bot → Reset Token', required: true, sensitive: true },
      { key: 'dmPolicy', label: 'Access Policy', type: 'select', options: [
        { value: 'pairing', label: 'Pairing (new users must be approved)' },
        { value: 'allowlist', label: 'Allowlist only' },
        { value: 'open', label: 'Open (anyone can DM)' },
      ], defaultValue: 'pairing' },
      { key: 'respondToMentions', label: 'Respond to @mentions in servers', type: 'toggle', defaultValue: true },
      { key: 'agentId', label: 'Agent ID (optional)', type: 'text', placeholder: '' },
    ],
    buildConfig: (v) => ({ bot_token: v.botToken as string, enabled: true, dm_policy: v.dmPolicy as string || 'pairing', respond_to_mentions: v.respondToMentions !== false }),
  },
  {
    id: 'irc',
    name: 'IRC',
    icon: 'IRC',
    description: 'Connect to any IRC server via outbound TCP/TLS. The simplest chat protocol — text-based, no special API.',
    fields: [
      { key: 'server', label: 'Server', type: 'text', placeholder: 'irc.libera.chat', required: true },
      { key: 'port', label: 'Port', type: 'text', placeholder: '6697', defaultValue: '6697' },
      { key: 'tls', label: 'Use TLS', type: 'toggle', defaultValue: true },
      { key: 'nick', label: 'Nickname', type: 'text', placeholder: 'paw-bot', required: true },
      { key: 'password', label: 'Server Password (optional)', type: 'password', placeholder: '' },
      { key: 'channels', label: 'Channels to Join', type: 'text', placeholder: '#general, #paw', hint: 'Comma-separated channel names' },
    ],
    buildConfig: (v) => ({ server: v.server as string, port: parseInt(v.port as string) || 6697, tls: v.tls !== false, nick: v.nick as string, enabled: true, dm_policy: 'pairing' }),
  },
  {
    id: 'slack',
    name: 'Slack',
    icon: 'SL',
    description: 'Connect to Slack via Socket Mode (outbound WebSocket). Create an app at api.slack.com → Enable Socket Mode → get Bot + App tokens.',
    fields: [
      { key: 'botToken', label: 'Bot Token (xoxb-...)', type: 'password', placeholder: 'xoxb-...', hint: 'OAuth & Permissions → Bot User OAuth Token', required: true, sensitive: true },
      { key: 'appToken', label: 'App Token (xapp-...)', type: 'password', placeholder: 'xapp-...', hint: 'Basic Information → App-Level Tokens (connections:write scope)', required: true, sensitive: true },
      { key: 'dmPolicy', label: 'Access Policy', type: 'select', options: [
        { value: 'pairing', label: 'Pairing (new users must be approved)' },
        { value: 'allowlist', label: 'Allowlist only' },
        { value: 'open', label: 'Open (anyone can DM)' },
      ], defaultValue: 'pairing' },
      { key: 'respondToMentions', label: 'Respond to @mentions in channels', type: 'toggle', defaultValue: true },
      { key: 'agentId', label: 'Agent ID (optional)', type: 'text', placeholder: '' },
    ],
    buildConfig: (v) => ({ bot_token: v.botToken as string, app_token: v.appToken as string, enabled: true, dm_policy: v.dmPolicy as string || 'pairing', respond_to_mentions: v.respondToMentions !== false }),
  },
  {
    id: 'matrix',
    name: 'Matrix',
    icon: 'MX',
    description: 'Connect to any Matrix homeserver via the Client-Server API (HTTP long-polling). Works with matrix.org, Synapse, Dendrite, etc.',
    fields: [
      { key: 'homeserver', label: 'Homeserver URL', type: 'text', placeholder: 'https://matrix.org', required: true },
      { key: 'accessToken', label: 'Access Token', type: 'password', placeholder: 'syt_...', hint: 'Element → Settings → Help & About → Access Token, or use a bot account', required: true, sensitive: true },
      { key: 'dmPolicy', label: 'Access Policy', type: 'select', options: [
        { value: 'pairing', label: 'Pairing (new users must be approved)' },
        { value: 'allowlist', label: 'Allowlist only' },
        { value: 'open', label: 'Open (anyone can DM)' },
      ], defaultValue: 'pairing' },
      { key: 'respondInRooms', label: 'Respond in group rooms (when mentioned)', type: 'toggle', defaultValue: false },
      { key: 'agentId', label: 'Agent ID (optional)', type: 'text', placeholder: '' },
    ],
    buildConfig: (v) => ({ homeserver: v.homeserver as string, access_token: v.accessToken as string, enabled: true, dm_policy: v.dmPolicy as string || 'pairing', respond_in_rooms: !!v.respondInRooms }),
  },
  {
    id: 'mattermost',
    name: 'Mattermost',
    icon: 'MM',
    description: 'Connect to a Mattermost server via WebSocket + REST API. Use a Personal Access Token or Bot Account token.',
    fields: [
      { key: 'serverUrl', label: 'Server URL', type: 'text', placeholder: 'https://chat.example.com', required: true },
      { key: 'token', label: 'Access Token', type: 'password', placeholder: '', hint: 'Mattermost → Settings → Security → Personal Access Tokens, or Integrations → Bot Accounts', required: true, sensitive: true },
      { key: 'dmPolicy', label: 'Access Policy', type: 'select', options: [
        { value: 'pairing', label: 'Pairing (new users must be approved)' },
        { value: 'allowlist', label: 'Allowlist only' },
        { value: 'open', label: 'Open (anyone can DM)' },
      ], defaultValue: 'pairing' },
      { key: 'respondToMentions', label: 'Respond to @mentions in channels', type: 'toggle', defaultValue: true },
      { key: 'agentId', label: 'Agent ID (optional)', type: 'text', placeholder: '' },
    ],
    buildConfig: (v) => ({ server_url: v.serverUrl as string, token: v.token as string, enabled: true, dm_policy: v.dmPolicy as string || 'pairing', respond_to_mentions: v.respondToMentions !== false }),
  },
  {
    id: 'nextcloud',
    name: 'Nextcloud Talk',
    icon: 'NC',
    description: 'Connect to Nextcloud Talk via HTTP polling. Uses Basic Auth with an app password.',
    fields: [
      { key: 'serverUrl', label: 'Nextcloud URL', type: 'text', placeholder: 'https://cloud.example.com', required: true },
      { key: 'username', label: 'Username', type: 'text', placeholder: 'paw-bot', required: true },
      { key: 'password', label: 'App Password', type: 'password', placeholder: '', hint: 'Nextcloud → Settings → Security → Create App Password', required: true, sensitive: true },
      { key: 'dmPolicy', label: 'Access Policy', type: 'select', options: [
        { value: 'pairing', label: 'Pairing (new users must be approved)' },
        { value: 'allowlist', label: 'Allowlist only' },
        { value: 'open', label: 'Open (anyone can message)' },
      ], defaultValue: 'pairing' },
      { key: 'respondInGroups', label: 'Respond in group conversations', type: 'toggle', defaultValue: false },
      { key: 'agentId', label: 'Agent ID (optional)', type: 'text', placeholder: '' },
    ],
    buildConfig: (v) => ({ server_url: v.serverUrl as string, username: v.username as string, password: v.password as string, enabled: true, dm_policy: v.dmPolicy as string || 'pairing', respond_in_groups: !!v.respondInGroups }),
  },
  {
    id: 'nostr',
    name: 'Nostr',
    icon: 'NS',
    description: 'Connect to the Nostr network via relay WebSockets. The bot listens for mentions and replies with signed kind-1 notes.',
    fields: [
      { key: 'privateKeyHex', label: 'Private Key (hex)', type: 'password', placeholder: '64 hex characters', hint: 'Your Nostr private key in hex format (not nsec). Keep this secret!', required: true, sensitive: true },
      { key: 'relays', label: 'Relay URLs', type: 'text', placeholder: 'wss://relay.damus.io, wss://nos.lol', hint: 'Comma-separated relay WebSocket URLs', defaultValue: 'wss://relay.damus.io, wss://nos.lol' },
      { key: 'dmPolicy', label: 'Access Policy', type: 'select', options: [
        { value: 'open', label: 'Open (respond to all mentions)' },
        { value: 'allowlist', label: 'Allowlist only (by pubkey)' },
        { value: 'pairing', label: 'Pairing (approve first-time users)' },
      ], defaultValue: 'open' },
      { key: 'agentId', label: 'Agent ID (optional)', type: 'text', placeholder: '' },
    ],
    buildConfig: (v) => ({ private_key_hex: v.privateKeyHex as string, relays: (v.relays as string || '').split(',').map(s => s.trim()).filter(Boolean), enabled: true, dm_policy: v.dmPolicy as string || 'open' }),
  },
  {
    id: 'twitch',
    name: 'Twitch',
    icon: 'TW',
    description: 'Connect to Twitch chat via IRC-over-WebSocket. Get an OAuth token from dev.twitch.tv or twitchapps.com/tmi/.',
    fields: [
      { key: 'oauthToken', label: 'OAuth Token', type: 'password', placeholder: 'oauth:xxxxxxxxxxxxx', hint: 'Get from dev.twitch.tv or twitchapps.com/tmi/', required: true, sensitive: true },
      { key: 'botUsername', label: 'Bot Username', type: 'text', placeholder: 'my_paw_bot', hint: 'Twitch username for the bot account', required: true },
      { key: 'channels', label: 'Channels to Join', type: 'text', placeholder: '#mychannel, #friend', hint: 'Comma-separated Twitch channel names', required: true },
      { key: 'dmPolicy', label: 'Access Policy', type: 'select', options: [
        { value: 'open', label: 'Open (respond to all)' },
        { value: 'allowlist', label: 'Allowlist only' },
        { value: 'pairing', label: 'Pairing (approve first-time users)' },
      ], defaultValue: 'open' },
      { key: 'requireMention', label: 'Only respond when @mentioned', type: 'toggle', defaultValue: true },
      { key: 'agentId', label: 'Agent ID (optional)', type: 'text', placeholder: '' },
    ],
    buildConfig: (v) => ({ oauth_token: v.oauthToken as string, bot_username: v.botUsername as string, channels_to_join: (v.channels as string || '').split(',').map(s => s.trim()).filter(Boolean), enabled: true, dm_policy: v.dmPolicy as string || 'open', require_mention: v.requireMention !== false }),
  },
  {
    id: 'whatsapp',
    name: 'WhatsApp',
    icon: 'WA',
    description: 'Connect your WhatsApp to Pawz. Your agent will respond to messages automatically.',
    descriptionHtml: `
      <div class="wa-setup-guide">
        <div class="wa-steps">
          <div class="wa-step"><span class="wa-step-num">1</span> Save this form (defaults are fine)</div>
          <div class="wa-step"><span class="wa-step-num">2</span> Click <strong>Start</strong> on the WhatsApp card</div>
          <div class="wa-step"><span class="wa-step-num">3</span> A QR code will appear — scan it with your phone</div>
          <div class="wa-step-sub">WhatsApp → Settings → Linked Devices → Link a Device</div>
          <div class="wa-step"><span class="wa-step-num">4</span> Done! Your agent is now live on WhatsApp</div>
        </div>
      </div>
    `,
    fields: [
      { key: 'dmPolicy', label: 'Who can message your agent?', type: 'select', options: [
        { value: 'pairing', label: 'New contacts need my approval first' },
        { value: 'open', label: 'Anyone can message' },
        { value: 'allowlist', label: 'Only specific phone numbers' },
      ], defaultValue: 'pairing' },
      { key: 'respondInGroups', label: 'Reply in group chats too', type: 'toggle', defaultValue: false },
      { key: 'allowFrom', label: 'Allowed phone numbers', type: 'text', placeholder: '15551234567, 447700900000', hint: 'Only needed if you chose "Only specific phone numbers" above. Include country code.' },
      { key: 'agentId', label: 'Agent', type: 'text', placeholder: 'Leave blank to use your default agent', hint: 'Optional — paste an agent ID to use a specific agent' },
      { key: 'apiPort', label: 'API Port', type: 'text', placeholder: '8085', defaultValue: '8085', hint: 'Advanced. Change only if port 8085 is already in use.' },
      { key: 'webhookPort', label: 'Webhook Port', type: 'text', placeholder: '8086', defaultValue: '8086', hint: 'Advanced. Change only if port 8086 is already in use.' },
    ],
    buildConfig: (v) => ({ enabled: true, api_port: parseInt(v.apiPort as string) || 8085, webhook_port: parseInt(v.webhookPort as string) || 8086, dm_policy: v.dmPolicy as string || 'pairing', respond_in_groups: !!v.respondInGroups }),
  },
  {
    id: 'webchat',
    name: 'Web Chat',
    icon: '',
    description: 'Share a link so friends can chat with your agent from their browser. No accounts needed — just a URL and access token.',
    fields: [
      { key: 'port', label: 'Port', type: 'text', placeholder: '3939', defaultValue: '3939' },
      { key: 'bindAddress', label: 'Bind Address', type: 'select', options: [
        { value: '0.0.0.0', label: '0.0.0.0 (LAN accessible)' },
        { value: '127.0.0.1', label: '127.0.0.1 (localhost only)' },
      ], defaultValue: '0.0.0.0' },
      { key: 'accessToken', label: 'Access Token', type: 'text', placeholder: 'Auto-generated if empty', hint: 'Share this token with friends so they can connect' },
      { key: 'pageTitle', label: 'Page Title', type: 'text', placeholder: 'Paw Chat', defaultValue: 'Paw Chat' },
      { key: 'dmPolicy', label: 'Access Policy', type: 'select', options: [
        { value: 'open', label: 'Open (anyone with the link + token)' },
        { value: 'pairing', label: 'Pairing (approve first-time users)' },
        { value: 'allowlist', label: 'Allowlist only' },
      ], defaultValue: 'open' },
      { key: 'agentId', label: 'Agent ID (optional)', type: 'text', placeholder: '' },
    ],
    buildConfig: (v) => ({ port: parseInt(v.port as string) || 3939, bind_address: v.bindAddress as string || '0.0.0.0', access_token: v.accessToken as string || '', page_title: v.pageTitle as string || 'Paw Chat', enabled: true, dm_policy: v.dmPolicy as string || 'open' }),
  },
];

let _channelSetupType: string | null = null;

export async function openChannelSetup(channelType: string) {
  const def = CHANNEL_SETUPS.find(c => c.id === channelType);
  if (!def) return;
  _channelSetupType = channelType;

  const title = $('channel-setup-title');
  const body = $('channel-setup-body');
  const modal = $('channel-setup-modal');
  if (!title || !body || !modal) return;

  title.textContent = `Set Up ${def.name}`;

  let existingValues: Record<string, string> = {};
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
      if (cfg.channels_to_join?.length) existingValues['channels'] = cfg.channels_to_join.join(', ');
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
      if (cfg.channels_to_join?.length) existingValues['channels'] = cfg.channels_to_join.join(', ');
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
  } catch { /* no existing config */ }

  let html = def.descriptionHtml
    ? `<div class="channel-setup-desc">${def.descriptionHtml}</div>`
    : `<p class="channel-setup-desc">${escHtml(def.description)}</p>`;

  // Group fields: regular first, then "Advanced." hint fields in a collapsible
  const regularFields = def.fields.filter(f => !f.hint?.startsWith('Advanced.'));
  const advancedFields = def.fields.filter(f => f.hint?.startsWith('Advanced.'));

  const renderField = (field: ChannelField) => {
    let fhtml = `<div class="form-group">`;
    fhtml += `<label class="form-label" for="ch-field-${field.key}">${escHtml(field.label)}${field.required ? ' <span class="required">*</span>' : ''}</label>`;

    const existVal = existingValues[field.key];

    if (field.type === 'select' && field.options) {
      fhtml += `<select class="form-input" id="ch-field-${field.key}" data-ch-field="${field.key}">`;
      for (const opt of field.options) {
        const selVal = existVal ?? (field.defaultValue ?? '');
        const sel = opt.value === selVal ? ' selected' : '';
        fhtml += `<option value="${escAttr(opt.value)}"${sel}>${escHtml(opt.label)}</option>`;
      }
      fhtml += `</select>`;
    } else if (field.type === 'toggle') {
      const checked = field.defaultValue ? ' checked' : '';
      fhtml += `<label class="toggle-label"><input type="checkbox" id="ch-field-${field.key}" data-ch-field="${field.key}"${checked}> Enabled</label>`;
    } else {
      const inputType = field.type === 'password' ? 'password' : 'text';
      const populateVal = existVal ?? (typeof field.defaultValue === 'string' ? field.defaultValue : '');
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

export function closeChannelSetup() {
  const modal = $('channel-setup-modal');
  if (modal) modal.style.display = 'none';
  _channelSetupType = null;
}

export async function saveChannelSetup() {
  console.log('[mail-debug] saveChannelSetup called, _channelSetupType=', _channelSetupType, 'mailType=', getChannelSetupType());
  if (_channelSetupType === '__mail_imap__' || getChannelSetupType() === '__mail_imap__') {
    console.log('[mail-debug] Routing to MailModule.saveMailImapSetup()');
    await saveMailImapSetup();
    clearChannelSetupType();
    _channelSetupType = null;
    return;
  }

  if (!_channelSetupType) return;

  // ── Telegram ────────────────────────────────────────────────────────────
  if (_channelSetupType === 'telegram') {
    const saveBtn = $('channel-setup-save') as HTMLButtonElement | null;
    if (saveBtn) { saveBtn.disabled = true; saveBtn.textContent = 'Saving...'; }

    try {
      const botToken = ($('ch-field-botToken') as HTMLInputElement)?.value?.trim() ?? '';
      const dmPolicy = ($('ch-field-dmPolicy') as HTMLSelectElement)?.value ?? 'pairing';
      const allowFrom = ($('ch-field-allowFrom') as HTMLInputElement)?.value?.trim() ?? '';
      const agentId = ($('ch-field-agentId') as HTMLInputElement)?.value?.trim() ?? '';

      if (!botToken) {
        showToast('Bot token is required', 'error');
        if (saveBtn) { saveBtn.disabled = false; saveBtn.textContent = 'Save & Connect'; }
        return;
      }

      let existing;
      try { existing = await pawEngine.telegramGetConfig(); } catch { existing = null; }

      const allowedUsers = allowFrom
        ? allowFrom.split(',').map(s => parseInt(s.trim())).filter(n => !isNaN(n))
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

      setTimeout(() => loadChannels(), 1000);
    } catch (e) {
      showToast(`Failed to save: ${e instanceof Error ? e.message : e}`, 'error');
    } finally {
      if (saveBtn) { $('channel-setup-save') && (($('channel-setup-save') as HTMLButtonElement).disabled = false); ($('channel-setup-save') as HTMLButtonElement | null) && (($('channel-setup-save') as HTMLButtonElement).textContent = 'Save & Connect'); }
    }
    return;
  }

  // ── Generic channel save ────────────────────────────────────────────────
  const _chDef = CHANNEL_SETUPS.find(c => c.id === _channelSetupType);
  if (_chDef) {
    const saveBtn = $('channel-setup-save') as HTMLButtonElement | null;
    if (saveBtn) { saveBtn.disabled = true; saveBtn.textContent = 'Saving...'; }

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
          if (saveBtn) { saveBtn.disabled = false; saveBtn.textContent = 'Save & Connect'; }
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
          api_key: 'paw-wa-' + Math.random().toString(36).slice(2, 18),
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

      try {
        await startChannel(channelType);
        showToast(`${_chDef.name} bridge started`, 'success');
      } catch (e) {
        console.warn('Auto-start failed:', e);
      }

      setTimeout(() => loadChannels(), 1000);
    } catch (e) {
      showToast(`Failed to save: ${e instanceof Error ? e.message : e}`, 'error');
    } finally {
      const btn = $('channel-setup-save') as HTMLButtonElement | null;
      if (btn) { btn.disabled = false; btn.textContent = 'Save & Connect'; }
    }
    return;
  }

  showToast(`Unknown channel type: ${_channelSetupType}`, 'error');
}

// ── Channel Operation Helpers ──────────────────────────────────────────────

export async function getChannelConfig(ch: string): Promise<Record<string, unknown> | null> {
  try {
    switch (ch) {
      case 'discord': return await pawEngine.discordGetConfig() as unknown as Record<string, unknown>;
      case 'irc': return await pawEngine.ircGetConfig() as unknown as Record<string, unknown>;
      case 'slack': return await pawEngine.slackGetConfig() as unknown as Record<string, unknown>;
      case 'matrix': return await pawEngine.matrixGetConfig() as unknown as Record<string, unknown>;
      case 'mattermost': return await pawEngine.mattermostGetConfig() as unknown as Record<string, unknown>;
      case 'nextcloud': return await pawEngine.nextcloudGetConfig() as unknown as Record<string, unknown>;
      case 'nostr': return await pawEngine.nostrGetConfig() as unknown as Record<string, unknown>;
      case 'twitch': return await pawEngine.twitchGetConfig() as unknown as Record<string, unknown>;
      case 'whatsapp': return await pawEngine.whatsappGetConfig() as unknown as Record<string, unknown>;
      default: return null;
    }
  } catch { return null; }
}

async function setChannelConfig(ch: string, config: Record<string, unknown>): Promise<void> {
  switch (ch) {
    case 'discord': return pawEngine.discordSetConfig(config as any);
    case 'irc': return pawEngine.ircSetConfig(config as any);
    case 'slack': return pawEngine.slackSetConfig(config as any);
    case 'matrix': return pawEngine.matrixSetConfig(config as any);
    case 'mattermost': return pawEngine.mattermostSetConfig(config as any);
    case 'nextcloud': return pawEngine.nextcloudSetConfig(config as any);
    case 'nostr': return pawEngine.nostrSetConfig(config as any);
    case 'twitch': return pawEngine.twitchSetConfig(config as any);
    case 'whatsapp': return pawEngine.whatsappSetConfig(config as any);
  }
}

export async function startChannel(ch: string): Promise<void> {
  switch (ch) {
    case 'discord': return pawEngine.discordStart();
    case 'irc': return pawEngine.ircStart();
    case 'slack': return pawEngine.slackStart();
    case 'matrix': return pawEngine.matrixStart();
    case 'mattermost': return pawEngine.mattermostStart();
    case 'nextcloud': return pawEngine.nextcloudStart();
    case 'nostr': return pawEngine.nostrStart();
    case 'twitch': return pawEngine.twitchStart();
    case 'whatsapp': return pawEngine.whatsappStart();
  }
}

async function stopChannel(ch: string): Promise<void> {
  switch (ch) {
    case 'discord': return pawEngine.discordStop();
    case 'irc': return pawEngine.ircStop();
    case 'slack': return pawEngine.slackStop();
    case 'matrix': return pawEngine.matrixStop();
    case 'mattermost': return pawEngine.mattermostStop();
    case 'nextcloud': return pawEngine.nextcloudStop();
    case 'nostr': return pawEngine.nostrStop();
    case 'twitch': return pawEngine.twitchStop();
    case 'whatsapp': return pawEngine.whatsappStop();
  }
}

export async function getChannelStatus(ch: string): Promise<ChannelStatus | null> {
  try {
    switch (ch) {
      case 'discord': return await pawEngine.discordStatus();
      case 'irc': return await pawEngine.ircStatus();
      case 'slack': return await pawEngine.slackStatus();
      case 'matrix': return await pawEngine.matrixStatus();
      case 'mattermost': return await pawEngine.mattermostStatus();
      case 'nextcloud': return await pawEngine.nextcloudStatus();
      case 'nostr': return await pawEngine.nostrStatus();
      case 'twitch': return await pawEngine.twitchStatus();
      case 'whatsapp': return await pawEngine.whatsappStatus();
      default: return null;
    }
  } catch { return null; }
}

async function approveChannelUser(ch: string, userId: string): Promise<void> {
  switch (ch) {
    case 'discord': return pawEngine.discordApproveUser(userId);
    case 'irc': return pawEngine.ircApproveUser(userId);
    case 'slack': return pawEngine.slackApproveUser(userId);
    case 'matrix': return pawEngine.matrixApproveUser(userId);
    case 'mattermost': return pawEngine.mattermostApproveUser(userId);
    case 'nextcloud': return pawEngine.nextcloudApproveUser(userId);
    case 'nostr': return pawEngine.nostrApproveUser(userId);
    case 'twitch': return pawEngine.twitchApproveUser(userId);
    case 'whatsapp': return pawEngine.whatsappApproveUser(userId);
  }
}

async function denyChannelUser(ch: string, userId: string): Promise<void> {
  switch (ch) {
    case 'discord': return pawEngine.discordDenyUser(userId);
    case 'irc': return pawEngine.ircDenyUser(userId);
    case 'slack': return pawEngine.slackDenyUser(userId);
    case 'matrix': return pawEngine.matrixDenyUser(userId);
    case 'mattermost': return pawEngine.mattermostDenyUser(userId);
    case 'nextcloud': return pawEngine.nextcloudDenyUser(userId);
    case 'nostr': return pawEngine.nostrDenyUser(userId);
    case 'twitch': return pawEngine.twitchDenyUser(userId);
    case 'whatsapp': return pawEngine.whatsappDenyUser(userId);
  }
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
          try { await pawEngine.telegramStart(); showToast('Telegram started', 'success'); setTimeout(() => loadChannels(), 1000); }
          catch (e) { showToast(`Start failed: ${e}`, 'error'); }
        });
        $(`${cardId}-stop`)?.addEventListener('click', async () => {
          try { await pawEngine.telegramStop(); showToast('Telegram stopped', 'success'); setTimeout(() => loadChannels(), 500); }
          catch (e) { showToast(`Stop failed: ${e}`, 'error'); }
        });
        $(`${cardId}-edit`)?.addEventListener('click', () => openChannelSetup('telegram'));
        $(`${cardId}-remove`)?.addEventListener('click', async () => {
          if (!confirm('Remove Telegram configuration?')) return;
          try {
            await pawEngine.telegramStop();
            await pawEngine.telegramSetConfig({ bot_token: '', enabled: false, dm_policy: 'pairing', allowed_users: [], pending_users: [] });
            showToast('Telegram removed', 'success'); loadChannels();
          } catch (e) { showToast(`Remove failed: ${e}`, 'error'); }
        });

        if (tgStatus.pending_users.length > 0) {
          const section = document.createElement('div');
          section.className = 'channel-pairing-section';
          section.style.cssText = 'margin-top:8px;border:1px solid var(--border);border-radius:8px;padding:12px;';
          section.innerHTML = `<h4 style="font-size:13px;font-weight:600;margin:0 0 8px 0">Telegram — Pending Requests</h4>`;
          for (const p of tgStatus.pending_users) {
            const row = document.createElement('div');
            row.style.cssText = 'display:flex;align-items:center;justify-content:space-between;padding:6px 0;border-bottom:1px solid var(--border-light,rgba(255,255,255,0.06))';
            row.innerHTML = `<div><strong>${escHtml(p.first_name)}</strong> <span style="color:var(--text-muted);font-size:12px">@${escHtml(p.username)} · ${p.user_id}</span></div>
              <div style="display:flex;gap:6px"><button class="btn btn-primary btn-sm tg-approve" data-uid="${p.user_id}">Approve</button><button class="btn btn-danger btn-sm tg-deny" data-uid="${p.user_id}">Deny</button></div>`;
            section.appendChild(row);
          }
          list.appendChild(section);
          section.querySelectorAll('.tg-approve').forEach(btn => btn.addEventListener('click', async () => {
            try { await pawEngine.telegramApproveUser(parseInt((btn as HTMLElement).dataset.uid!)); showToast('Approved', 'success'); loadChannels(); } catch (e) { showToast(`${e}`, 'error'); }
          }));
          section.querySelectorAll('.tg-deny').forEach(btn => btn.addEventListener('click', async () => {
            try { await pawEngine.telegramDenyUser(parseInt((btn as HTMLElement).dataset.uid!)); showToast('Denied', 'success'); loadChannels(); } catch (e) { showToast(`${e}`, 'error'); }
          }));
        }
      }
    } catch { /* no telegram */ }

    // ── Generic Channels ─────────────────────────────────────────────────
    const genericChannels = ['discord', 'irc', 'slack', 'matrix', 'mattermost', 'nextcloud', 'nostr', 'twitch', 'whatsapp'];

    for (const ch of genericChannels) {
      try {
        const status = await getChannelStatus(ch);
        const config = await getChannelConfig(ch);
        if (!status || !config) continue;

        const isConfigured = isChannelConfigured(ch, config);
        if (!isConfigured) continue;

        anyConfigured = true;
        const isConnected = status.running && status.connected;
        const def = CHANNEL_SETUPS.find(c => c.id === ch);
        const name = def?.name ?? ch;
        const iconStr = def?.icon ?? ch.substring(0, 2).toUpperCase();
        const cardId = `ch-${ch}`;

        const card = document.createElement('div');
        card.className = 'channel-card';
        card.innerHTML = `
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
        list.appendChild(card);

        $(`${cardId}-start`)?.addEventListener('click', async () => {
          try {
            // For WhatsApp, listen for real-time status updates during startup
            if (ch === 'whatsapp') {
              const statusBannerId = `${cardId}-status-banner`;
              let banner = document.getElementById(statusBannerId);
              if (!banner) {
                banner = document.createElement('div');
                banner.id = statusBannerId;
                banner.className = 'wa-status-banner';
                card.appendChild(banner);
              }
              banner.innerHTML = '<span class="wa-spinner"></span> Starting...';
              banner.style.display = 'flex';

              const { listen } = await import('@tauri-apps/api/event');
              const unlisten = await listen<{kind: string; message?: string; qr?: string}>('whatsapp-status', (event) => {
                const { kind, message, qr } = event.payload;
                if (!banner) return;
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
                    banner.innerHTML = `<span class="wa-spinner"></span> Connecting to WhatsApp...`;
                    break;
                  case 'qr_code':
                    banner.innerHTML = `<div class="wa-qr-section"><p style="margin:0 0 8px">Scan this QR code with your phone's WhatsApp app:</p>${qr ? `<img src="${qr.startsWith('data:') ? qr : 'data:image/png;base64,' + qr}" alt="WhatsApp QR code" class="wa-qr-image" />` : ''}<p style="font-size:12px;color:var(--text-muted);margin:8px 0 0">Open WhatsApp → Settings → Linked Devices → Link a Device</p></div>`;
                    banner.className = 'wa-status-banner wa-status-qr';
                    break;
                  case 'connected':
                    banner.innerHTML = `<span class="wa-status-icon">✅</span> ${escHtml(message ?? 'WhatsApp connected!')}`;
                    banner.className = 'wa-status-banner wa-status-success';
                    setTimeout(() => { banner!.style.display = 'none'; unlisten(); loadChannels(); }, 2000);
                    break;
                  case 'disconnected':
                    banner.style.display = 'none';
                    unlisten();
                    loadChannels();
                    break;
                }
              });
            }
            await startChannel(ch);
            if (ch !== 'whatsapp') {
              showToast(`${name} started`, 'success');
            }
            setTimeout(() => loadChannels(), 1000);
          }
          catch (e) {
            const statusBanner = document.getElementById(`${cardId}-status-banner`);
            if (ch === 'whatsapp' && statusBanner) {
              const errMsg = e instanceof Error ? e.message : String(e);
              if (errMsg.includes('automatically') || errMsg.includes('internet')) {
                statusBanner.innerHTML = `<span class="wa-status-icon">⚠️</span> <span>Couldn't set up WhatsApp. Check your internet connection and try again.</span>`;
              } else if (errMsg.includes('didn\'t start in time') || errMsg.includes('timeout')) {
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
          try { await stopChannel(ch); showToast(`${name} stopped`, 'success'); setTimeout(() => loadChannels(), 500); }
          catch (e) { showToast(`Stop failed: ${e}`, 'error'); }
        });
        $(`${cardId}-edit`)?.addEventListener('click', () => openChannelSetup(ch));
        $(`${cardId}-remove`)?.addEventListener('click', async () => {
          if (!confirm(`Remove ${name} configuration?`)) return;
          try {
            await stopChannel(ch);
            const emptyConfig = emptyChannelConfig(ch);
            await setChannelConfig(ch, emptyConfig);
            showToast(`${name} removed`, 'success'); loadChannels();
          } catch (e) { showToast(`Remove failed: ${e}`, 'error'); }
        });

        if (status.pending_users.length > 0) {
          const section = document.createElement('div');
          section.className = 'channel-pairing-section';
          section.style.cssText = 'margin-top:8px;border:1px solid var(--border);border-radius:8px;padding:12px;';
          section.innerHTML = `<h4 style="font-size:13px;font-weight:600;margin:0 0 8px 0">${escHtml(name)} — Pending Requests</h4>`;
          for (const p of status.pending_users) {
            const row = document.createElement('div');
            row.style.cssText = 'display:flex;align-items:center;justify-content:space-between;padding:6px 0;border-bottom:1px solid var(--border-light,rgba(255,255,255,0.06))';
            row.innerHTML = `<div><strong>${escHtml(p.display_name || p.username)}</strong> <span style="color:var(--text-muted);font-size:12px">${escHtml(p.user_id)}</span></div>
              <div style="display:flex;gap:6px"><button class="btn btn-primary btn-sm ch-approve" data-ch="${ch}" data-uid="${escAttr(p.user_id)}">Approve</button><button class="btn btn-danger btn-sm ch-deny" data-ch="${ch}" data-uid="${escAttr(p.user_id)}">Deny</button></div>`;
            section.appendChild(row);
          }
          list.appendChild(section);
          section.querySelectorAll('.ch-approve').forEach(btn => btn.addEventListener('click', async () => {
            const _ch = (btn as HTMLElement).dataset.ch!;
            const _uid = (btn as HTMLElement).dataset.uid!;
            try { await approveChannelUser(_ch, _uid); showToast('Approved', 'success'); loadChannels(); } catch (e) { showToast(`${e}`, 'error'); }
          }));
          section.querySelectorAll('.ch-deny').forEach(btn => btn.addEventListener('click', async () => {
            const _ch = (btn as HTMLElement).dataset.ch!;
            const _uid = (btn as HTMLElement).dataset.uid!;
            try { await denyChannelUser(_ch, _uid); showToast('Denied', 'success'); loadChannels(); } catch (e) { showToast(`${e}`, 'error'); }
          }));
        }
      } catch { /* skip erroring channel */ }
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

function isChannelConfigured(ch: string, config: Record<string, unknown>): boolean {
  switch (ch) {
    case 'discord': return !!config.bot_token;
    case 'irc': return !!config.server && !!config.nick;
    case 'slack': return !!config.bot_token && !!config.app_token;
    case 'matrix': return !!config.homeserver && !!config.access_token;
    case 'mattermost': return !!config.server_url && !!config.token;
    case 'nextcloud': return !!config.server_url && !!config.username && !!config.password;
    case 'nostr': return !!config.private_key_hex;
    case 'twitch': return !!config.oauth_token && !!config.bot_username;
    case 'whatsapp': return !!config.enabled;
    default: return false;
  }
}

function emptyChannelConfig(ch: string): Record<string, unknown> {
  const base = { enabled: false, dm_policy: 'pairing', allowed_users: [], pending_users: [] };
  switch (ch) {
    case 'discord': return { ...base, bot_token: '', respond_to_mentions: true };
    case 'irc': return { ...base, server: '', port: 6697, tls: true, nick: '', channels_to_join: [], respond_in_channels: false };
    case 'slack': return { ...base, bot_token: '', app_token: '', respond_to_mentions: true };
    case 'matrix': return { ...base, homeserver: '', access_token: '', respond_in_rooms: false };
    case 'mattermost': return { ...base, server_url: '', token: '', respond_to_mentions: true };
    case 'nextcloud': return { ...base, server_url: '', username: '', password: '', respond_in_groups: false };
    case 'nostr': return { ...base, private_key_hex: '', relays: [], dm_policy: 'open' };
    case 'twitch': return { ...base, oauth_token: '', bot_username: '', channels_to_join: [], dm_policy: 'open', require_mention: true };
    case 'whatsapp': return { ...base, instance_name: 'paw', api_url: 'http://127.0.0.1:8085', api_key: '', api_port: 8085, webhook_port: 8086, respond_in_groups: false, session_connected: false };
    default: return base;
  }
}

// ── Cron modal ─────────────────────────────────────────────────────────────
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
  if (empty) { empty.style.display = 'flex'; empty.textContent = 'Agent files managed via Memory Palace'; }
}

export async function openMemoryFile(filePath: string) {
  console.log('[main] openMemoryFile:', filePath);
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
  console.log('[mail-debug] Binding save button, element found:', !!_saveBtn);
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

    _channelSetupType = null;
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

    body.querySelectorAll('[data-ch-pick]').forEach(btn => {
      btn.addEventListener('click', () => {
        if (footer) footer.style.display = '';
        openChannelSetup((btn as HTMLElement).dataset.chPick!);
      });
    });

    modal.style.display = '';
  });

  document.querySelectorAll('#channels-picker-empty .channel-pick-btn').forEach(btn => {
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
        console.log('[channels] Auto-started Telegram bridge');
      }
    }
  } catch (e) { console.warn('[channels] Telegram auto-start skipped:', e); }

  const channels = ['discord', 'irc', 'slack', 'matrix', 'mattermost', 'nextcloud', 'nostr', 'twitch'] as const;
  for (const ch of channels) {
    try {
      const cfg = await getChannelConfig(ch);
      if (cfg && (cfg as Record<string, unknown>).enabled) {
        const status = await getChannelStatus(ch);
        if (status && !status.running) {
          await startChannel(ch);
          console.log(`[channels] Auto-started ${ch} bridge`);
        }
      }
    } catch (e) { console.warn(`[channels] ${ch} auto-start skipped:`, e); }
  }
}
