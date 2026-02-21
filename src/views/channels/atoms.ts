// atoms.ts — Channel types, constants, and pure predicates (zero DOM, zero IPC)

// ── Types ──────────────────────────────────────────────────────────────────

export interface ChannelField {
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

export interface ChannelSetupDef {
  id: string;
  name: string;
  icon: string;
  description: string;
  descriptionHtml?: string;
  fields: ChannelField[];
  buildConfig: (values: Record<string, string | boolean>) => Record<string, unknown>;
}

// ── Constants ──────────────────────────────────────────────────────────────

export const CHANNEL_CLASSES: Record<string, string> = {
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

export const CHANNEL_SETUPS: ChannelSetupDef[] = [
  {
    id: 'telegram',
    name: 'Telegram',
    icon: 'TG',
    description:
      'Connect your agent to Telegram via a Bot token from @BotFather. No gateway or public URL needed — uses long polling.',
    fields: [
      {
        key: 'botToken',
        label: 'Bot Token',
        type: 'password',
        placeholder: '123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11',
        hint: 'Get this from @BotFather on Telegram',
        required: true,
        sensitive: true,
      },
      {
        key: 'dmPolicy',
        label: 'Access Policy',
        type: 'select',
        options: [
          { value: 'pairing', label: 'Pairing (new users must be approved)' },
          { value: 'allowlist', label: 'Allowlist only (pre-approved IDs)' },
          { value: 'open', label: 'Open (anyone can message)' },
        ],
        defaultValue: 'pairing',
      },
      {
        key: 'allowFrom',
        label: 'Allowed User IDs',
        type: 'text',
        placeholder: '123456789, 987654321',
        hint: 'Telegram user IDs (numbers), comma-separated. Leave blank for pairing mode.',
      },
      {
        key: 'agentId',
        label: 'Agent ID (optional)',
        type: 'text',
        placeholder: '',
        hint: 'Use a specific agent config. Leave blank for default.',
      },
    ],
    buildConfig: (v) => ({
      bot_token: v.botToken as string,
      enabled: true,
      dm_policy: (v.dmPolicy as string) || 'pairing',
    }),
  },
  {
    id: 'discord',
    name: 'Discord',
    icon: 'DC',
    description:
      'Connect to Discord via the Bot Gateway (outbound WebSocket). Create a bot at discord.com/developers → New Application → Bot → Copy Token.',
    fields: [
      {
        key: 'botToken',
        label: 'Bot Token',
        type: 'password',
        placeholder: 'MTIzNDU2Nzg5MA.XXXXXX.XXXXXXXX',
        hint: 'Discord Developer Portal → Bot → Reset Token',
        required: true,
        sensitive: true,
      },
      {
        key: 'dmPolicy',
        label: 'Access Policy',
        type: 'select',
        options: [
          { value: 'pairing', label: 'Pairing (new users must be approved)' },
          { value: 'allowlist', label: 'Allowlist only' },
          { value: 'open', label: 'Open (anyone can DM)' },
        ],
        defaultValue: 'pairing',
      },
      {
        key: 'respondToMentions',
        label: 'Respond to @mentions in servers',
        type: 'toggle',
        defaultValue: true,
      },
      { key: 'agentId', label: 'Agent ID (optional)', type: 'text', placeholder: '' },
    ],
    buildConfig: (v) => ({
      bot_token: v.botToken as string,
      enabled: true,
      dm_policy: (v.dmPolicy as string) || 'pairing',
      respond_to_mentions: v.respondToMentions !== false,
    }),
  },
  {
    id: 'irc',
    name: 'IRC',
    icon: 'IRC',
    description:
      'Connect to any IRC server via outbound TCP/TLS. The simplest chat protocol — text-based, no special API.',
    fields: [
      {
        key: 'server',
        label: 'Server',
        type: 'text',
        placeholder: 'irc.libera.chat',
        required: true,
      },
      { key: 'port', label: 'Port', type: 'text', placeholder: '6697', defaultValue: '6697' },
      { key: 'tls', label: 'Use TLS', type: 'toggle', defaultValue: true },
      { key: 'nick', label: 'Nickname', type: 'text', placeholder: 'paw-bot', required: true },
      { key: 'password', label: 'Server Password (optional)', type: 'password', placeholder: '' },
      {
        key: 'channels',
        label: 'Channels to Join',
        type: 'text',
        placeholder: '#general, #paw',
        hint: 'Comma-separated channel names',
      },
    ],
    buildConfig: (v) => ({
      server: v.server as string,
      port: parseInt(v.port as string) || 6697,
      tls: v.tls !== false,
      nick: v.nick as string,
      enabled: true,
      dm_policy: 'pairing',
    }),
  },
  {
    id: 'slack',
    name: 'Slack',
    icon: 'SL',
    description:
      'Connect to Slack via Socket Mode (outbound WebSocket). Create an app at api.slack.com → Enable Socket Mode → get Bot + App tokens.',
    fields: [
      {
        key: 'botToken',
        label: 'Bot Token (xoxb-...)',
        type: 'password',
        placeholder: 'xoxb-...',
        hint: 'OAuth & Permissions → Bot User OAuth Token',
        required: true,
        sensitive: true,
      },
      {
        key: 'appToken',
        label: 'App Token (xapp-...)',
        type: 'password',
        placeholder: 'xapp-...',
        hint: 'Basic Information → App-Level Tokens (connections:write scope)',
        required: true,
        sensitive: true,
      },
      {
        key: 'dmPolicy',
        label: 'Access Policy',
        type: 'select',
        options: [
          { value: 'pairing', label: 'Pairing (new users must be approved)' },
          { value: 'allowlist', label: 'Allowlist only' },
          { value: 'open', label: 'Open (anyone can DM)' },
        ],
        defaultValue: 'pairing',
      },
      {
        key: 'respondToMentions',
        label: 'Respond to @mentions in channels',
        type: 'toggle',
        defaultValue: true,
      },
      { key: 'agentId', label: 'Agent ID (optional)', type: 'text', placeholder: '' },
    ],
    buildConfig: (v) => ({
      bot_token: v.botToken as string,
      app_token: v.appToken as string,
      enabled: true,
      dm_policy: (v.dmPolicy as string) || 'pairing',
      respond_to_mentions: v.respondToMentions !== false,
    }),
  },
  {
    id: 'matrix',
    name: 'Matrix',
    icon: 'MX',
    description:
      'Connect to any Matrix homeserver via the Client-Server API (HTTP long-polling). Works with matrix.org, Synapse, Dendrite, etc.',
    fields: [
      {
        key: 'homeserver',
        label: 'Homeserver URL',
        type: 'text',
        placeholder: 'https://matrix.org',
        required: true,
      },
      {
        key: 'accessToken',
        label: 'Access Token',
        type: 'password',
        placeholder: 'syt_...',
        hint: 'Element → Settings → Help & About → Access Token, or use a bot account',
        required: true,
        sensitive: true,
      },
      {
        key: 'dmPolicy',
        label: 'Access Policy',
        type: 'select',
        options: [
          { value: 'pairing', label: 'Pairing (new users must be approved)' },
          { value: 'allowlist', label: 'Allowlist only' },
          { value: 'open', label: 'Open (anyone can DM)' },
        ],
        defaultValue: 'pairing',
      },
      {
        key: 'respondInRooms',
        label: 'Respond in group rooms (when mentioned)',
        type: 'toggle',
        defaultValue: false,
      },
      { key: 'agentId', label: 'Agent ID (optional)', type: 'text', placeholder: '' },
    ],
    buildConfig: (v) => ({
      homeserver: v.homeserver as string,
      access_token: v.accessToken as string,
      enabled: true,
      dm_policy: (v.dmPolicy as string) || 'pairing',
      respond_in_rooms: !!v.respondInRooms,
    }),
  },
  {
    id: 'mattermost',
    name: 'Mattermost',
    icon: 'MM',
    description:
      'Connect to a Mattermost server via WebSocket + REST API. Use a Personal Access Token or Bot Account token.',
    fields: [
      {
        key: 'serverUrl',
        label: 'Server URL',
        type: 'text',
        placeholder: 'https://chat.example.com',
        required: true,
      },
      {
        key: 'token',
        label: 'Access Token',
        type: 'password',
        placeholder: '',
        hint: 'Mattermost → Settings → Security → Personal Access Tokens, or Integrations → Bot Accounts',
        required: true,
        sensitive: true,
      },
      {
        key: 'dmPolicy',
        label: 'Access Policy',
        type: 'select',
        options: [
          { value: 'pairing', label: 'Pairing (new users must be approved)' },
          { value: 'allowlist', label: 'Allowlist only' },
          { value: 'open', label: 'Open (anyone can DM)' },
        ],
        defaultValue: 'pairing',
      },
      {
        key: 'respondToMentions',
        label: 'Respond to @mentions in channels',
        type: 'toggle',
        defaultValue: true,
      },
      { key: 'agentId', label: 'Agent ID (optional)', type: 'text', placeholder: '' },
    ],
    buildConfig: (v) => ({
      server_url: v.serverUrl as string,
      token: v.token as string,
      enabled: true,
      dm_policy: (v.dmPolicy as string) || 'pairing',
      respond_to_mentions: v.respondToMentions !== false,
    }),
  },
  {
    id: 'nextcloud',
    name: 'Nextcloud Talk',
    icon: 'NC',
    description:
      'Connect to Nextcloud Talk via HTTP polling. Uses Basic Auth with an app password.',
    fields: [
      {
        key: 'serverUrl',
        label: 'Nextcloud URL',
        type: 'text',
        placeholder: 'https://cloud.example.com',
        required: true,
      },
      { key: 'username', label: 'Username', type: 'text', placeholder: 'paw-bot', required: true },
      {
        key: 'password',
        label: 'App Password',
        type: 'password',
        placeholder: '',
        hint: 'Nextcloud → Settings → Security → Create App Password',
        required: true,
        sensitive: true,
      },
      {
        key: 'dmPolicy',
        label: 'Access Policy',
        type: 'select',
        options: [
          { value: 'pairing', label: 'Pairing (new users must be approved)' },
          { value: 'allowlist', label: 'Allowlist only' },
          { value: 'open', label: 'Open (anyone can message)' },
        ],
        defaultValue: 'pairing',
      },
      {
        key: 'respondInGroups',
        label: 'Respond in group conversations',
        type: 'toggle',
        defaultValue: false,
      },
      { key: 'agentId', label: 'Agent ID (optional)', type: 'text', placeholder: '' },
    ],
    buildConfig: (v) => ({
      server_url: v.serverUrl as string,
      username: v.username as string,
      password: v.password as string,
      enabled: true,
      dm_policy: (v.dmPolicy as string) || 'pairing',
      respond_in_groups: !!v.respondInGroups,
    }),
  },
  {
    id: 'nostr',
    name: 'Nostr',
    icon: 'NS',
    description:
      'Connect to the Nostr network via relay WebSockets. The bot listens for mentions and replies with signed kind-1 notes.',
    fields: [
      {
        key: 'privateKeyHex',
        label: 'Private Key (hex)',
        type: 'password',
        placeholder: '64 hex characters',
        hint: 'Your Nostr private key in hex format (not nsec). Keep this secret!',
        required: true,
        sensitive: true,
      },
      {
        key: 'relays',
        label: 'Relay URLs',
        type: 'text',
        placeholder: 'wss://relay.damus.io, wss://nos.lol',
        hint: 'Comma-separated relay WebSocket URLs',
        defaultValue: 'wss://relay.damus.io, wss://nos.lol',
      },
      {
        key: 'dmPolicy',
        label: 'Access Policy',
        type: 'select',
        options: [
          { value: 'open', label: 'Open (respond to all mentions)' },
          { value: 'allowlist', label: 'Allowlist only (by pubkey)' },
          { value: 'pairing', label: 'Pairing (approve first-time users)' },
        ],
        defaultValue: 'open',
      },
      { key: 'agentId', label: 'Agent ID (optional)', type: 'text', placeholder: '' },
    ],
    buildConfig: (v) => ({
      private_key_hex: v.privateKeyHex as string,
      relays: ((v.relays as string) || '')
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean),
      enabled: true,
      dm_policy: (v.dmPolicy as string) || 'open',
    }),
  },
  {
    id: 'twitch',
    name: 'Twitch',
    icon: 'TW',
    description:
      'Connect to Twitch chat via IRC-over-WebSocket. Get an OAuth token from dev.twitch.tv or twitchapps.com/tmi/.',
    fields: [
      {
        key: 'oauthToken',
        label: 'OAuth Token',
        type: 'password',
        placeholder: 'oauth:xxxxxxxxxxxxx',
        hint: 'Get from dev.twitch.tv or twitchapps.com/tmi/',
        required: true,
        sensitive: true,
      },
      {
        key: 'botUsername',
        label: 'Bot Username',
        type: 'text',
        placeholder: 'my_paw_bot',
        hint: 'Twitch username for the bot account',
        required: true,
      },
      {
        key: 'channels',
        label: 'Channels to Join',
        type: 'text',
        placeholder: '#mychannel, #friend',
        hint: 'Comma-separated Twitch channel names',
        required: true,
      },
      {
        key: 'dmPolicy',
        label: 'Access Policy',
        type: 'select',
        options: [
          { value: 'open', label: 'Open (respond to all)' },
          { value: 'allowlist', label: 'Allowlist only' },
          { value: 'pairing', label: 'Pairing (approve first-time users)' },
        ],
        defaultValue: 'open',
      },
      {
        key: 'requireMention',
        label: 'Only respond when @mentioned',
        type: 'toggle',
        defaultValue: true,
      },
      { key: 'agentId', label: 'Agent ID (optional)', type: 'text', placeholder: '' },
    ],
    buildConfig: (v) => ({
      oauth_token: v.oauthToken as string,
      bot_username: v.botUsername as string,
      channels_to_join: ((v.channels as string) || '')
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean),
      enabled: true,
      dm_policy: (v.dmPolicy as string) || 'open',
      require_mention: v.requireMention !== false,
    }),
  },
  {
    id: 'whatsapp',
    name: 'WhatsApp',
    icon: 'WA',
    description:
      'Give your agent a WhatsApp number. People message that number, your agent replies.',
    descriptionHtml: `
      <div class="wa-setup-guide">
        <div style="background:rgba(255,180,0,0.12);border:1px solid rgba(255,180,0,0.3);border-radius:8px;padding:12px 14px;margin-bottom:14px;font-size:13px;line-height:1.5">
          <strong style="color:#ffb400">⚠️ Important — read before scanning</strong><br>
          The phone number you scan <strong>becomes the agent</strong>. Anyone who messages that number will talk to your AI, not you.<br><br>
          <strong>Don't use your personal number</strong> unless you want your agent replying to all your contacts.
          Use a cheap prepaid SIM or spare number instead — you only need it for the initial WhatsApp verification.
        </div>
        <div class="wa-steps">
          <div class="wa-step"><span class="wa-step-num">1</span> Get a <strong>separate phone number</strong> for your agent (prepaid SIM, eSIM, etc.)</div>
          <div class="wa-step"><span class="wa-step-num">2</span> Register WhatsApp on that number</div>
          <div class="wa-step"><span class="wa-step-num">3</span> Save this form, then click <strong>Start</strong> on the WhatsApp card</div>
          <div class="wa-step"><span class="wa-step-num">4</span> A QR code will appear — scan it <strong>from the agent's phone</strong></div>
          <div class="wa-step-sub">WhatsApp → Settings → Linked Devices → Link a Device</div>
          <div class="wa-step"><span class="wa-step-num">5</span> Done! People can now message that number to talk to your agent</div>
        </div>
      </div>
    `,
    fields: [
      {
        key: 'dmPolicy',
        label: 'Who can message your agent?',
        type: 'select',
        options: [
          { value: 'pairing', label: 'New contacts need my approval first' },
          { value: 'open', label: 'Anyone can message' },
          { value: 'allowlist', label: 'Only specific phone numbers' },
        ],
        defaultValue: 'pairing',
      },
      {
        key: 'respondInGroups',
        label: 'Reply in group chats too',
        type: 'toggle',
        defaultValue: false,
      },
      {
        key: 'allowFrom',
        label: 'Allowed phone numbers',
        type: 'text',
        placeholder: '15551234567, 447700900000',
        hint: 'Only needed if you chose "Only specific phone numbers" above. Include country code.',
      },
      {
        key: 'agentId',
        label: 'Agent',
        type: 'text',
        placeholder: 'Leave blank to use your default agent',
        hint: 'Optional — paste an agent ID to use a specific agent',
      },
      {
        key: 'apiPort',
        label: 'API Port',
        type: 'text',
        placeholder: '8085',
        defaultValue: '8085',
        hint: 'Advanced. Change only if port 8085 is already in use.',
      },
      {
        key: 'webhookPort',
        label: 'Webhook Port',
        type: 'text',
        placeholder: '8086',
        defaultValue: '8086',
        hint: 'Advanced. Change only if port 8086 is already in use.',
      },
    ],
    buildConfig: (v) => ({
      enabled: true,
      api_port: parseInt(v.apiPort as string) || 8085,
      webhook_port: parseInt(v.webhookPort as string) || 8086,
      dm_policy: (v.dmPolicy as string) || 'pairing',
      respond_in_groups: !!v.respondInGroups,
    }),
  },
  {
    id: 'webchat',
    name: 'Web Chat',
    icon: '',
    description:
      'Share a link so friends can chat with your agent from their browser. No accounts needed — just a URL and access token.',
    fields: [
      { key: 'port', label: 'Port', type: 'text', placeholder: '3939', defaultValue: '3939' },
      {
        key: 'bindAddress',
        label: 'Bind Address',
        type: 'select',
        options: [
          { value: '0.0.0.0', label: '0.0.0.0 (LAN accessible)' },
          { value: '127.0.0.1', label: '127.0.0.1 (localhost only)' },
        ],
        defaultValue: '0.0.0.0',
      },
      {
        key: 'accessToken',
        label: 'Access Token',
        type: 'text',
        placeholder: 'Auto-generated if empty',
        hint: 'Share this token with friends so they can connect',
      },
      {
        key: 'pageTitle',
        label: 'Page Title',
        type: 'text',
        placeholder: 'Paw Chat',
        defaultValue: 'Paw Chat',
      },
      {
        key: 'dmPolicy',
        label: 'Access Policy',
        type: 'select',
        options: [
          { value: 'open', label: 'Open (anyone with the link + token)' },
          { value: 'pairing', label: 'Pairing (approve first-time users)' },
          { value: 'allowlist', label: 'Allowlist only' },
        ],
        defaultValue: 'open',
      },
      { key: 'agentId', label: 'Agent ID (optional)', type: 'text', placeholder: '' },
    ],
    buildConfig: (v) => ({
      port: parseInt(v.port as string) || 3939,
      bind_address: (v.bindAddress as string) || '0.0.0.0',
      access_token: (v.accessToken as string) || '',
      page_title: (v.pageTitle as string) || 'Paw Chat',
      enabled: true,
      dm_policy: (v.dmPolicy as string) || 'open',
    }),
  },
];

// ── Pure predicates ────────────────────────────────────────────────────────

export function isChannelConfigured(ch: string, config: Record<string, unknown>): boolean {
  switch (ch) {
    case 'discord':
      return !!config.bot_token;
    case 'irc':
      return !!config.server && !!config.nick;
    case 'slack':
      return !!config.bot_token && !!config.app_token;
    case 'matrix':
      return !!config.homeserver && !!config.access_token;
    case 'mattermost':
      return !!config.server_url && !!config.token;
    case 'nextcloud':
      return !!config.server_url && !!config.username && !!config.password;
    case 'nostr':
      return !!config.private_key_hex;
    case 'twitch':
      return !!config.oauth_token && !!config.bot_username;
    case 'whatsapp':
      return !!config.enabled;
    default:
      return false;
  }
}

export function emptyChannelConfig(ch: string): Record<string, unknown> {
  const base = { enabled: false, dm_policy: 'pairing', allowed_users: [], pending_users: [] };
  switch (ch) {
    case 'discord':
      return { ...base, bot_token: '', respond_to_mentions: true };
    case 'irc':
      return {
        ...base,
        server: '',
        port: 6697,
        tls: true,
        nick: '',
        channels_to_join: [],
        respond_in_channels: false,
      };
    case 'slack':
      return { ...base, bot_token: '', app_token: '', respond_to_mentions: true };
    case 'matrix':
      return { ...base, homeserver: '', access_token: '', respond_in_rooms: false };
    case 'mattermost':
      return { ...base, server_url: '', token: '', respond_to_mentions: true };
    case 'nextcloud':
      return { ...base, server_url: '', username: '', password: '', respond_in_groups: false };
    case 'nostr':
      return { ...base, private_key_hex: '', relays: [], dm_policy: 'open' };
    case 'twitch':
      return {
        ...base,
        oauth_token: '',
        bot_username: '',
        channels_to_join: [],
        dm_policy: 'open',
        require_mention: true,
      };
    case 'whatsapp':
      return {
        ...base,
        instance_name: 'paw',
        api_url: 'http://127.0.0.1:8085',
        api_key: '',
        api_port: 8085,
        webhook_port: 8086,
        respond_in_groups: false,
        session_connected: false,
      };
    default:
      return base;
  }
}
