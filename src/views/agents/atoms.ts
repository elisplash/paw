// atoms.ts — Pure types, constants, and zero-dependency helpers
// NO pawEngine, NO document.*, NO Tauri imports allowed here

export interface Agent {
  id: string;
  name: string;
  avatar: string; // avatar ID (e.g. '5') or legacy emoji
  color: string;
  bio: string;
  model: string; // AI model to use
  template: 'general' | 'research' | 'creative' | 'technical' | 'custom';
  personality: {
    tone: 'casual' | 'balanced' | 'formal';
    initiative: 'reactive' | 'balanced' | 'proactive';
    detail: 'brief' | 'balanced' | 'thorough';
  };
  skills: string[];
  boundaries: string[];
  systemPrompt?: string; // Custom instructions
  createdAt: string;
  lastUsed?: string;
  source?: 'local' | 'backend'; // Where this agent comes from
  projectId?: string; // If backend-created, which project
}

// Tool groups for the per-agent tool assignment UI
export const TOOL_GROUPS: {
  label: string;
  icon: string;
  tools: { id: string; name: string; desc: string }[];
}[] = [
  {
    label: 'Core',
    icon: 'terminal',
    tools: [
      { id: 'exec', name: 'Run Commands', desc: 'Execute shell commands' },
      { id: 'fetch', name: 'HTTP Fetch', desc: 'Make HTTP requests' },
    ],
  },
  {
    label: 'Files',
    icon: 'folder_open',
    tools: [
      { id: 'read_file', name: 'Read File', desc: 'Read file contents' },
      { id: 'write_file', name: 'Write File', desc: 'Create and edit files' },
      { id: 'list_directory', name: 'List Directory', desc: 'Browse file listings' },
      { id: 'append_file', name: 'Append File', desc: 'Add content to files' },
      { id: 'delete_file', name: 'Delete File', desc: 'Remove files' },
    ],
  },
  {
    label: 'Web',
    icon: 'language',
    tools: [
      { id: 'web_search', name: 'Web Search', desc: 'Search the internet' },
      { id: 'web_read', name: 'Web Read', desc: 'Read web page content' },
      { id: 'web_screenshot', name: 'Web Screenshot', desc: 'Capture screenshots' },
      { id: 'web_browse', name: 'Web Browse', desc: 'Interactive browsing' },
    ],
  },
  {
    label: 'Soul & Memory',
    icon: 'psychology',
    tools: [
      { id: 'soul_read', name: 'Soul Read', desc: 'Read persona files' },
      { id: 'soul_write', name: 'Soul Write', desc: 'Write persona files' },
      { id: 'soul_list', name: 'Soul List', desc: 'List persona files' },
      { id: 'memory_store', name: 'Memory Store', desc: 'Save to long-term memory' },
      { id: 'memory_search', name: 'Memory Search', desc: 'Recall from memory' },
      { id: 'self_info', name: 'Self Info', desc: 'View own configuration' },
    ],
  },
  {
    label: 'Agents & Tasks',
    icon: 'group',
    tools: [
      { id: 'update_profile', name: 'Update Profile', desc: 'Modify agent profile' },
      { id: 'create_agent', name: 'Create Agent', desc: 'Spawn new agents' },
      { id: 'agent_list', name: 'Agent List', desc: 'List all agents' },
      { id: 'agent_skills', name: 'Agent Skills', desc: 'View agent skills' },
      { id: 'agent_skill_assign', name: 'Assign Skill', desc: 'Assign skills to agents' },
      { id: 'create_task', name: 'Create Task', desc: 'Create new tasks' },
      { id: 'list_tasks', name: 'List Tasks', desc: 'View task list' },
      { id: 'manage_task', name: 'Manage Task', desc: 'Update/delete tasks' },
      { id: 'skill_search', name: 'Skill Search', desc: 'Search community skills' },
      { id: 'skill_install', name: 'Skill Install', desc: 'Install community skills' },
      { id: 'skill_list', name: 'Skill List', desc: 'List installed skills' },
    ],
  },
  {
    label: 'Communication',
    icon: 'chat',
    tools: [
      { id: 'email_send', name: 'Email Send', desc: 'Send emails' },
      { id: 'email_read', name: 'Email Read', desc: 'Read emails' },
      { id: 'slack_send', name: 'Slack Send', desc: 'Send Slack messages' },
      { id: 'slack_read', name: 'Slack Read', desc: 'Read Slack messages' },
      { id: 'telegram_send', name: 'Telegram Send', desc: 'Send Telegram messages' },
      { id: 'telegram_read', name: 'Telegram Read', desc: 'Read Telegram status' },
      { id: 'github_api', name: 'GitHub API', desc: 'Interact with GitHub' },
      { id: 'rest_api_call', name: 'REST API', desc: 'Call REST APIs' },
      { id: 'webhook_send', name: 'Webhook', desc: 'Send webhooks' },
      { id: 'image_generate', name: 'Image Generate', desc: 'Generate images' },
    ],
  },
  {
    label: 'Trading: Coinbase',
    icon: 'account_balance',
    tools: [
      { id: 'coinbase_prices', name: 'Prices', desc: 'Get crypto prices' },
      { id: 'coinbase_balance', name: 'Balance', desc: 'Check balances' },
      { id: 'coinbase_wallet_create', name: 'Create Wallet', desc: 'Create wallets' },
      { id: 'coinbase_trade', name: 'Trade', desc: 'Execute trades' },
      { id: 'coinbase_transfer', name: 'Transfer', desc: 'Transfer funds' },
    ],
  },
  {
    label: 'Trading: Solana',
    icon: 'currency_bitcoin',
    tools: [
      { id: 'sol_wallet_create', name: 'Create Wallet', desc: 'Create Solana wallet' },
      { id: 'sol_balance', name: 'Balance', desc: 'Check SOL balance' },
      { id: 'sol_quote', name: 'Quote', desc: 'Get swap quotes' },
      { id: 'sol_swap', name: 'Swap', desc: 'Execute swaps' },
      { id: 'sol_portfolio', name: 'Portfolio', desc: 'View portfolio' },
      { id: 'sol_token_info', name: 'Token Info', desc: 'Look up tokens' },
      { id: 'sol_transfer', name: 'Transfer', desc: 'Transfer tokens' },
    ],
  },
  {
    label: 'Trading: EVM DEX',
    icon: 'swap_horiz',
    tools: [
      { id: 'dex_wallet_create', name: 'Create Wallet', desc: 'Create EVM wallet' },
      { id: 'dex_balance', name: 'Balance', desc: 'Check balances' },
      { id: 'dex_quote', name: 'Quote', desc: 'Get swap quotes' },
      { id: 'dex_swap', name: 'Swap', desc: 'Execute swaps' },
      { id: 'dex_portfolio', name: 'Portfolio', desc: 'View portfolio' },
      { id: 'dex_token_info', name: 'Token Info', desc: 'Token details' },
      { id: 'dex_check_token', name: 'Check Token', desc: 'Verify token safety' },
      { id: 'dex_search_token', name: 'Search Token', desc: 'Find tokens' },
      { id: 'dex_watch_wallet', name: 'Watch Wallet', desc: 'Monitor wallets' },
      { id: 'dex_whale_transfers', name: 'Whale Transfers', desc: 'Track large transfers' },
      { id: 'dex_top_traders', name: 'Top Traders', desc: 'See top traders' },
      { id: 'dex_trending', name: 'Trending', desc: 'Trending tokens' },
      { id: 'dex_transfer', name: 'Transfer', desc: 'Transfer tokens' },
    ],
  },
];

// Default agent templates
export const AGENT_TEMPLATES: Record<string, Partial<Agent>> = {
  general: {
    bio: 'A helpful all-purpose assistant',
    personality: { tone: 'balanced', initiative: 'balanced', detail: 'balanced' },
    skills: ['web_search', 'web_fetch', 'read', 'write'],
  },
  research: {
    bio: 'Deep research and analysis specialist',
    personality: { tone: 'formal', initiative: 'proactive', detail: 'thorough' },
    skills: ['web_search', 'web_fetch', 'read', 'write', 'browser'],
  },
  creative: {
    bio: 'Writing, brainstorming, and creative projects',
    personality: { tone: 'casual', initiative: 'proactive', detail: 'balanced' },
    skills: ['web_search', 'read', 'write', 'image'],
  },
  technical: {
    bio: 'Code, debugging, and technical problem-solving',
    personality: { tone: 'balanced', initiative: 'reactive', detail: 'thorough' },
    skills: ['read', 'write', 'edit', 'exec', 'web_search'],
  },
  custom: {
    bio: '',
    personality: { tone: 'balanced', initiative: 'balanced', detail: 'balanced' },
    skills: [],
  },
};

export const AVATAR_COLORS = [
  '#0073EA',
  '#10b981',
  '#8b5cf6',
  '#f59e0b',
  '#ec4899',
  '#06b6d4',
  '#ef4444',
];

// ── Avatars ────────────────────────────────────────────────────────────────
// Pawz Boi avatar set (96×96 PNGs in /src/assets/avatars/)
export const SPRITE_AVATARS = Array.from({ length: 50 }, (_, i) => String(i + 1));

/** Default avatar for the main Pawz agent */
export const DEFAULT_AVATAR = '5';

/** Check if avatar string is a numeric avatar ID vs a legacy emoji */
export function isAvatar(avatar: string): boolean {
  return /^\d+$/.test(avatar);
}

/** Render an agent avatar as an <img> or legacy emoji <span> */
export function spriteAvatar(avatar: string, size = 32): string {
  if (isAvatar(avatar)) {
    return `<img src="/src/assets/avatars/${avatar}.png" alt="" width="${size}" height="${size}" style="display:block;border-radius:50%">`;
  }
  // Legacy emoji fallback
  return `<span style="font-size:${Math.round(size * 0.7)}px;line-height:1">${avatar}</span>`;
}
