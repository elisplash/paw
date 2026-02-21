// ─────────────────────────────────────────────────────────────────────────────
// Slash Commands — Molecules
// Composed behaviours: execute parsed commands against the engine/UI.
// These functions have side effects (Tauri IPC, DOM updates, localStorage).
// ─────────────────────────────────────────────────────────────────────────────

import {
  type ParsedCommand,
  parseCommand,
  validateCommand,
  isSlashCommand,
  buildHelpText,
} from './atoms';

import { pawEngine } from '../../engine';

// ── Types ──────────────────────────────────────────────────────────────────

export interface CommandResult {
  /** Whether the input was handled as a slash command */
  handled: boolean;
  /** System message to display in chat (markdown) */
  systemMessage?: string;
  /** If the command modifies chat flow (e.g. /web injects as user message) */
  rewrittenInput?: string;
  /** Prevent the normal send-to-AI flow */
  preventDefault: boolean;
  /** If true, refresh the session list after execution */
  refreshSessions?: boolean;
  /** If true, reload chat history after execution */
  refreshHistory?: boolean;
}

export type CommandContext = {
  /** Current session key/id */
  sessionKey: string | null;
  /** Function to add a local system-style message to the chat UI */
  addSystemMessage: (text: string) => void;
  /** Function to clear the chat UI */
  clearChatUI: () => void;
  /** Function to start a new session */
  newSession: (label?: string) => Promise<void>;
  /** Function to reload sessions list */
  reloadSessions: () => Promise<void>;
  /** Get the current model name */
  getCurrentModel: () => string;
  /** Get available model names (for validation) */
  getAvailableModels?: () => Promise<string[]>;
};

// ── State ──────────────────────────────────────────────────────────────────

/** In-session overrides set by slash commands. Reset on session change. */
export interface SessionOverrides {
  model?: string;
  thinkingLevel?: string;
  temperature?: number;
}

let _overrides: SessionOverrides = {};

export function getSessionOverrides(): SessionOverrides {
  return { ..._overrides };
}

export function clearSessionOverrides(): void {
  _overrides = {};
}

// ── Executor ───────────────────────────────────────────────────────────────

/**
 * Intercept raw chat input — if it's a slash command, execute it.
 * Returns a CommandResult telling the caller whether to proceed with normal send.
 */
export async function interceptSlashCommand(
  rawInput: string,
  ctx: CommandContext,
): Promise<CommandResult> {
  if (!isSlashCommand(rawInput)) {
    return { handled: false, preventDefault: false };
  }

  const cmd = parseCommand(rawInput);
  const err = validateCommand(cmd);

  if (err) {
    return {
      handled: true,
      systemMessage: `${err}`,
      preventDefault: true,
    };
  }

  return executeCommand(cmd, ctx);
}

/**
 * Execute a validated slash command.
 */
async function executeCommand(cmd: ParsedCommand, ctx: CommandContext): Promise<CommandResult> {
  switch (cmd.name) {
    // ── Chat ───────────────────────────────────────────────────────
    case 'model':
      _overrides.model = cmd.args;
      return {
        handled: true,
        systemMessage: `Model switched to **${cmd.args}** for this session.`,
        preventDefault: true,
      };

    case 'think':
      _overrides.thinkingLevel = cmd.args.toLowerCase();
      return {
        handled: true,
        systemMessage: `Thinking level set to **${cmd.args.toLowerCase()}**.`,
        preventDefault: true,
      };

    case 'temp': {
      const temp = parseFloat(cmd.args);
      _overrides.temperature = temp;
      return {
        handled: true,
        systemMessage: `Temperature set to **${temp}**.`,
        preventDefault: true,
      };
    }

    case 'mode':
      // Mode switching — store as override, the sendMessage flow reads it
      localStorage.setItem('paw_slash_mode_override', cmd.args);
      return {
        handled: true,
        systemMessage: `Mode switched to **${cmd.args}**.`,
        preventDefault: true,
      };

    case 'agent':
      // Agent switching — store as override for the next send
      localStorage.setItem('paw_slash_agent_override', cmd.args);
      return {
        handled: true,
        systemMessage: `Agent switched to **${cmd.args}**.`,
        preventDefault: true,
      };

    // ── Session ────────────────────────────────────────────────────
    case 'clear':
      if (ctx.sessionKey) {
        try {
          await pawEngine.sessionClear(ctx.sessionKey);
          ctx.clearChatUI();
          return {
            handled: true,
            systemMessage: 'Session history cleared.',
            preventDefault: true,
            refreshHistory: true,
          };
        } catch (e) {
          return {
            handled: true,
            systemMessage: `Failed to clear session: ${e}`,
            preventDefault: true,
          };
        }
      }
      return {
        handled: true,
        systemMessage: 'No active session to clear.',
        preventDefault: true,
      };

    case 'compact':
      return await executeCompact(ctx);

    case 'new':
      try {
        await ctx.newSession(cmd.args || undefined);
        _overrides = {}; // reset overrides for new session
        return {
          handled: true,
          systemMessage: cmd.args ? `New session created: **${cmd.args}**` : 'New session created.',
          preventDefault: true,
          refreshSessions: true,
        };
      } catch (e) {
        return {
          handled: true,
          systemMessage: `Failed to create session: ${e}`,
          preventDefault: true,
        };
      }

    case 'rename':
      if (ctx.sessionKey) {
        try {
          await pawEngine.sessionRename(ctx.sessionKey, cmd.args);
          return {
            handled: true,
            systemMessage: `Session renamed to **${cmd.args}**.`,
            preventDefault: true,
            refreshSessions: true,
          };
        } catch (e) {
          return {
            handled: true,
            systemMessage: `Failed to rename session: ${e}`,
            preventDefault: true,
          };
        }
      }
      return {
        handled: true,
        systemMessage: 'No active session to rename.',
        preventDefault: true,
      };

    // ── Memory ─────────────────────────────────────────────────────
    case 'remember':
      try {
        const memId = await pawEngine.memoryStore(cmd.args, 'user', 7);
        return {
          handled: true,
          systemMessage: `Stored in memory (id: \`${memId}\`): "${cmd.args.slice(0, 80)}${cmd.args.length > 80 ? '…' : ''}"`,
          preventDefault: true,
        };
      } catch (e) {
        return {
          handled: true,
          systemMessage: `Failed to store memory: ${e}`,
          preventDefault: true,
        };
      }

    case 'forget':
      try {
        await pawEngine.memoryDelete(cmd.args);
        return {
          handled: true,
          systemMessage: `Memory \`${cmd.args}\` deleted.`,
          preventDefault: true,
        };
      } catch (e) {
        return {
          handled: true,
          systemMessage: `Failed to delete memory: ${e}`,
          preventDefault: true,
        };
      }

    case 'recall':
      try {
        const memories = await pawEngine.memorySearch(cmd.args, 5);
        if (memories.length === 0) {
          return {
            handled: true,
            systemMessage: `No memories found for "${cmd.args}".`,
            preventDefault: true,
          };
        }
        const memLines = memories.map(
          (m, i) =>
            `${i + 1}. \`${m.id}\` [${m.category}] — ${m.content.slice(0, 100)}${m.content.length > 100 ? '…' : ''}`,
        );
        return {
          handled: true,
          systemMessage: `**Memories matching "${cmd.args}":**\n${memLines.join('\n')}`,
          preventDefault: true,
        };
      } catch (e) {
        return {
          handled: true,
          systemMessage: `Memory search failed: ${e}`,
          preventDefault: true,
        };
      }

    // ── Tools ──────────────────────────────────────────────────────
    case 'web':
      // Rewrite as a user message that prompts the agent to search
      return {
        handled: true,
        rewrittenInput: `Please search the web for: ${cmd.args}`,
        preventDefault: false, // let the normal send flow proceed with rewritten input
      };

    case 'img':
      return {
        handled: true,
        rewrittenInput: `Please generate an image: ${cmd.args}`,
        preventDefault: false,
      };

    case 'exec':
      return {
        handled: true,
        rewrittenInput: `Please run this shell command and show me the output: \`${cmd.args}\``,
        preventDefault: false,
      };

    // ── Config ─────────────────────────────────────────────────────
    case 'help':
      return {
        handled: true,
        systemMessage: buildHelpText(),
        preventDefault: true,
      };

    case 'status':
      return await executeStatus(ctx);

    case 'debug':
      return executeDebugToggle();

    default:
      return {
        handled: true,
        systemMessage: `Command /${cmd.name} is recognized but not yet implemented.`,
        preventDefault: true,
      };
  }
}

// ── Sub-executors for complex commands ──────────────────────────────────────

async function executeCompact(ctx: CommandContext): Promise<CommandResult> {
  if (!ctx.sessionKey) {
    return {
      handled: true,
      systemMessage: 'No active session to compact.',
      preventDefault: true,
    };
  }

  try {
    // Use the engine's native compaction (summarizes with AI then replaces old messages)
    const result = await pawEngine.sessionCompact(ctx.sessionKey);
    const saved = result.tokens_before - result.tokens_after;
    return {
      handled: true,
      systemMessage: [
        `**Session compacted successfully**`,
        `  Messages: ${result.messages_before} → ${result.messages_after}`,
        `  Tokens: ~${result.tokens_before.toLocaleString()} → ~${result.tokens_after.toLocaleString()} (saved ~${saved.toLocaleString()})`,
        `  Summary: ${result.summary_length} chars`,
      ].join('\n'),
      preventDefault: true,
      refreshHistory: true,
    };
  } catch (e) {
    // Fallback: if engine compaction fails (e.g. gateway mode), ask AI to summarize
    try {
      const history = await pawEngine.chatHistory(ctx.sessionKey, 500);
      const msgCount = history.length;

      if (msgCount < 6) {
        return {
          handled: true,
          systemMessage: 'Session is too short to compact (< 6 messages).',
          preventDefault: true,
        };
      }

      const totalChars = history.reduce((sum, m) => sum + (m.content?.length ?? 0), 0);
      const estimatedTokens = Math.round(totalChars / 4);

      const summaryPrompt = [
        'Please provide a concise summary of our conversation so far.',
        'Focus on: key decisions, important context, action items, and any preferences I expressed.',
        'Keep it under 500 words. This will be used to resume the conversation with fresh context.',
      ].join(' ');

      return {
        handled: true,
        systemMessage: `**Compacting session** (${msgCount} messages, ~${estimatedTokens.toLocaleString()} tokens).\nAsking AI to summarize…`,
        rewrittenInput: summaryPrompt,
        preventDefault: false,
      };
    } catch {
      return {
        handled: true,
        systemMessage: `Compaction failed: ${e}`,
        preventDefault: true,
      };
    }
  }
}

async function executeStatus(ctx: CommandContext): Promise<CommandResult> {
  try {
    const status = await pawEngine.status();
    const config = await pawEngine.getConfig();
    const memStats = await pawEngine.memoryStats();

    const overrideLines = Object.entries(_overrides)
      .filter(([, v]) => v !== undefined)
      .map(([k, v]) => `  ${k}: **${v}**`);

    const lines = [
      '**Engine Status**',
      `  Ready: ${status.ready ? 'Yes' : 'No'}`,
      `  Providers: ${status.providers ?? 0}`,
      `  Default model: **${config.default_model || 'none'}**`,
      `  Session: \`${ctx.sessionKey || 'none'}\``,
      '',
      '**Memory**',
      `  Total memories: ${memStats.total_memories ?? 0}`,
      `  Has embeddings: ${memStats.has_embeddings ? 'Yes' : 'No'}`,
    ];

    if (overrideLines.length > 0) {
      lines.push('', '**Session Overrides**', ...overrideLines);
    }

    return {
      handled: true,
      systemMessage: lines.join('\n'),
      preventDefault: true,
    };
  } catch (e) {
    return {
      handled: true,
      systemMessage: `Failed to get status: ${e}`,
      preventDefault: true,
    };
  }
}

function executeDebugToggle(): CommandResult {
  const current = localStorage.getItem('paw_debug') === 'true';
  const next = !current;
  localStorage.setItem('paw_debug', String(next));

  return {
    handled: true,
    systemMessage: `Debug mode **${next ? 'enabled' : 'disabled'}**.`,
    preventDefault: true,
  };
}
