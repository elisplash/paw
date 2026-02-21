// ─────────────────────────────────────────────────────────────────────────────
// Slash Commands — Atoms
// Pure functions: parsing, validation, autocomplete matching. No side effects.
// ─────────────────────────────────────────────────────────────────────────────

// ── Types ──────────────────────────────────────────────────────────────────

export interface SlashCommandDef {
  name: string;
  description: string;
  usage: string;
  /** If true, requires at least one argument */
  requiresArg: boolean;
  /** Category for grouping in help */
  category: 'chat' | 'session' | 'memory' | 'tools' | 'config';
}

export interface ParsedCommand {
  /** The command name without the slash (e.g. "model") */
  name: string;
  /** Raw argument string after the command */
  args: string;
  /** The original full input */
  raw: string;
  /** Whether the command was recognized */
  recognized: boolean;
}

export interface AutocompleteSuggestion {
  command: string;
  description: string;
  usage: string;
}

// ── Command Registry ───────────────────────────────────────────────────────

export const COMMANDS: SlashCommandDef[] = [
  // Chat
  {
    name: 'model',
    description: 'Switch AI model for this session',
    usage: '/model <name>',
    requiresArg: true,
    category: 'chat',
  },
  {
    name: 'think',
    description: 'Set thinking/reasoning level',
    usage: '/think <none|low|medium|high>',
    requiresArg: true,
    category: 'chat',
  },
  {
    name: 'mode',
    description: 'Switch chat mode',
    usage: '/mode <name>',
    requiresArg: true,
    category: 'chat',
  },
  {
    name: 'agent',
    description: 'Switch active agent',
    usage: '/agent <name>',
    requiresArg: true,
    category: 'chat',
  },
  {
    name: 'temp',
    description: 'Set temperature (0.0–2.0)',
    usage: '/temp <value>',
    requiresArg: true,
    category: 'chat',
  },

  // Session
  {
    name: 'clear',
    description: 'Clear current session history',
    usage: '/clear',
    requiresArg: false,
    category: 'session',
  },
  {
    name: 'compact',
    description: 'Summarize & compact session context',
    usage: '/compact',
    requiresArg: false,
    category: 'session',
  },
  {
    name: 'new',
    description: 'Start a new session',
    usage: '/new [label]',
    requiresArg: false,
    category: 'session',
  },
  {
    name: 'rename',
    description: 'Rename current session',
    usage: '/rename <label>',
    requiresArg: true,
    category: 'session',
  },

  // Memory
  {
    name: 'remember',
    description: 'Store text in long-term memory',
    usage: '/remember <text>',
    requiresArg: true,
    category: 'memory',
  },
  {
    name: 'forget',
    description: 'Delete a memory by ID',
    usage: '/forget <id>',
    requiresArg: true,
    category: 'memory',
  },
  {
    name: 'recall',
    description: 'Search memories',
    usage: '/recall <query>',
    requiresArg: true,
    category: 'memory',
  },

  // Tools
  {
    name: 'web',
    description: 'Force a web search',
    usage: '/web <query>',
    requiresArg: true,
    category: 'tools',
  },
  {
    name: 'img',
    description: 'Generate an image',
    usage: '/img <prompt>',
    requiresArg: true,
    category: 'tools',
  },
  {
    name: 'exec',
    description: 'Execute a shell command',
    usage: '/exec <command>',
    requiresArg: true,
    category: 'tools',
  },

  // Config
  {
    name: 'help',
    description: 'Show available slash commands',
    usage: '/help',
    requiresArg: false,
    category: 'config',
  },
  {
    name: 'status',
    description: 'Show engine status & current settings',
    usage: '/status',
    requiresArg: false,
    category: 'config',
  },
  {
    name: 'debug',
    description: 'Toggle debug/verbose mode',
    usage: '/debug',
    requiresArg: false,
    category: 'config',
  },
];

const COMMAND_MAP = new Map(COMMANDS.map((c) => [c.name, c]));

// ── Pure Functions ─────────────────────────────────────────────────────────

/**
 * Determine if a string looks like a slash command (starts with / followed by word).
 */
export function isSlashCommand(input: string): boolean {
  return /^\/[a-z]+(\s|$)/i.test(input.trim());
}

/**
 * Parse raw input into a structured SlashCommand.
 * Does NOT execute anything — pure extraction.
 */
export function parseCommand(input: string): ParsedCommand {
  const trimmed = input.trim();
  const match = trimmed.match(/^\/([a-z]+)(?:\s+(.*))?$/is);

  if (!match) {
    return { name: '', args: '', raw: input, recognized: false };
  }

  const name = match[1].toLowerCase();
  const args = (match[2] ?? '').trim();
  const recognized = COMMAND_MAP.has(name);

  return { name, args, raw: input, recognized };
}

/**
 * Validate a parsed command — check arg requirements.
 * Returns null if valid, or an error message.
 */
export function validateCommand(cmd: ParsedCommand): string | null {
  if (!cmd.recognized) {
    return `Unknown command: /${cmd.name}. Type /help for available commands.`;
  }

  const def = COMMAND_MAP.get(cmd.name)!;

  if (def.requiresArg && !cmd.args) {
    return `Missing argument. Usage: ${def.usage}`;
  }

  // Command-specific validation
  switch (cmd.name) {
    case 'think': {
      const levels = ['none', 'low', 'medium', 'high'];
      if (!levels.includes(cmd.args.toLowerCase())) {
        return `Invalid thinking level "${cmd.args}". Choose: ${levels.join(', ')}`;
      }
      break;
    }
    case 'temp': {
      const val = parseFloat(cmd.args);
      if (isNaN(val) || val < 0 || val > 2.0) {
        return `Temperature must be a number between 0.0 and 2.0`;
      }
      break;
    }
  }

  return null;
}

/**
 * Get autocomplete suggestions for a partial slash command.
 * Call this as the user types after '/'.
 */
export function getAutocompleteSuggestions(partial: string): AutocompleteSuggestion[] {
  if (!partial.startsWith('/')) return [];

  const typed = partial.slice(1).toLowerCase();

  // If nothing typed yet, show all commands
  if (!typed) {
    return COMMANDS.map((c) => ({
      command: `/${c.name}`,
      description: c.description,
      usage: c.usage,
    }));
  }

  // Filter by prefix match
  return COMMANDS.filter((c) => c.name.startsWith(typed)).map((c) => ({
    command: `/${c.name}`,
    description: c.description,
    usage: c.usage,
  }));
}

/**
 * Build the /help output text, grouped by category.
 */
export function buildHelpText(): string {
  const categories: Record<string, SlashCommandDef[]> = {};
  for (const cmd of COMMANDS) {
    (categories[cmd.category] ??= []).push(cmd);
  }

  const labels: Record<string, string> = {
    chat: 'Chat',
    session: 'Session',
    memory: 'Memory',
    tools: 'Tools',
    config: 'Config',
  };

  const lines: string[] = ['**Available Slash Commands**\n'];
  for (const [cat, cmds] of Object.entries(categories)) {
    lines.push(`**${labels[cat] ?? cat}**`);
    for (const c of cmds) {
      lines.push(`  \`${c.usage}\` — ${c.description}`);
    }
    lines.push('');
  }

  return lines.join('\n');
}

/**
 * Get the command definition by name (if it exists).
 */
export function getCommandDef(name: string): SlashCommandDef | undefined {
  return COMMAND_MAP.get(name.toLowerCase());
}
