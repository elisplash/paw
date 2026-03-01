// ─────────────────────────────────────────────────────────────────────────────
// Agent Tool Policies — Atoms
// Pure functions for tool policy evaluation. No side effects.
// ─────────────────────────────────────────────────────────────────────────────

// ── Types ──────────────────────────────────────────────────────────────────

/**
 * Policy mode determines the default behaviour:
 * - 'allowlist': only explicitly allowed tools can be used (strict)
 * - 'denylist': all tools allowed except explicitly denied (permissive)
 * - 'unrestricted': no restrictions (default for backward compat)
 */
export type PolicyMode = 'allowlist' | 'denylist' | 'unrestricted';

/**
 * Per-agent tool policy configuration.
 * Stored as part of the Agent object in localStorage.
 */
export interface ToolPolicy {
  /** Policy mode */
  mode: PolicyMode;
  /** Tools explicitly allowed (used in allowlist mode) */
  allowed: string[];
  /** Tools explicitly denied (used in denylist mode) */
  denied: string[];
  /** If true, require HIL approval for tools not in the allowed list */
  requireApprovalForUnlisted: boolean;
  /** Maximum number of tool calls per agent turn */
  maxToolCallsPerTurn?: number;
  /** Tools that always require HIL approval regardless of mode */
  alwaysRequireApproval: string[];
}

/**
 * Result of a tool policy check.
 */
export interface PolicyDecision {
  /** Whether the tool is allowed */
  allowed: boolean;
  /** Whether HIL approval is required even if allowed */
  requiresApproval: boolean;
  /** Reason for the decision */
  reason: string;
}

// ── Constants ──────────────────────────────────────────────────────────────

/** All known built-in tools. */
export const ALL_TOOLS = [
  // Core
  'exec',
  'fetch',
  'read_file',
  'write_file',
  'list_directory',
  'append_file',
  'delete_file',
  // Web
  'web_search',
  'web_read',
  'web_screenshot',
  'web_browse',
  // Soul / persona
  'soul_read',
  'soul_write',
  'soul_list',
  // Memory
  'memory_store',
  'memory_search',
  // Self-awareness
  'self_info',
  // Agent management
  'update_profile',
  'create_agent',
  'agent_list',
  'agent_skills',
  'agent_skill_assign',
  // Task / Automation management
  'create_task',
  'list_tasks',
  'manage_task',
  // Community skills management
  'skill_search',
  'skill_install',
  'skill_list',
  // Communication skills
  'telegram_send',
  'telegram_read',
  'rest_api_call',
  'webhook_send',
  'image_generate',
  // Trading: Coinbase
  'coinbase_prices',
  'coinbase_balance',
  'coinbase_wallet_create',
  'coinbase_trade',
  'coinbase_transfer',
  // Trading: Solana (Jupiter)
  'sol_wallet_create',
  'sol_balance',
  'sol_quote',
  'sol_swap',
  'sol_portfolio',
  'sol_token_info',
  'sol_transfer',
  // Trading: EVM DEX (Uniswap)
  'dex_wallet_create',
  'dex_balance',
  'dex_quote',
  'dex_swap',
  'dex_portfolio',
  'dex_token_info',
  'dex_check_token',
  'dex_search_token',
  'dex_watch_wallet',
  'dex_whale_transfers',
  'dex_top_traders',
  'dex_trending',
  'dex_transfer',
  // Tool RAG
  'request_tools',
  // Inter-agent comms
  'agent_send_message',
  'agent_read_messages',
  // Squads
  'create_squad',
  'list_squads',
  'manage_squad',
  'squad_broadcast',
  // Dashboard & storage
  'skill_output',
  'delete_skill_output',
  'skill_store_set',
  'skill_store_get',
  'skill_store_list',
  'skill_store_delete',
] as const;

/** Read-only tools that are generally safe. */
export const SAFE_TOOLS: readonly string[] = [
  'read_file',
  'list_directory',
  'web_search',
  'web_read',
  'memory_search',
  'soul_read',
  'soul_list',
  'self_info',
  'fetch',
  'agent_list',
  'agent_skills',
  'skill_list',
  'coinbase_prices',
  'sol_balance',
  'sol_portfolio',
  'sol_token_info',
  'dex_balance',
  'dex_portfolio',
  'dex_token_info',
  'dex_check_token',
  'dex_search_token',
  'dex_trending',
  'telegram_read',
  'request_tools',
  'list_tasks',
  'agent_read_messages',
  'list_squads',
  'skill_search',
];

/** High-risk tools that modify the system or send data externally. */
export const HIGH_RISK_TOOLS: readonly string[] = [
  'exec',
  'write_file',
  'delete_file',
  'append_file',
  'webhook_send',
  'rest_api_call',
  'telegram_send',
  'coinbase_trade',
  'coinbase_transfer',
  'coinbase_wallet_create',
  'sol_swap',
  'sol_transfer',
  'sol_wallet_create',
  'dex_swap',
  'dex_transfer',
  'dex_wallet_create',
  'image_generate',
  'soul_write',
  'update_profile',
  'create_agent',
  'create_task',
  'manage_task',
  'skill_search',
  'skill_install',
  'agent_skill_assign',
];

/** Default policy: unrestricted (backward-compatible). */
export const DEFAULT_POLICY: ToolPolicy = {
  mode: 'unrestricted',
  allowed: [],
  denied: [],
  requireApprovalForUnlisted: false,
  alwaysRequireApproval: [],
};

/** Restrictive preset: only safe read tools allowed. */
export const READONLY_POLICY: ToolPolicy = {
  mode: 'allowlist',
  allowed: [...SAFE_TOOLS],
  denied: [],
  requireApprovalForUnlisted: false,
  alwaysRequireApproval: [],
};

/** Standard preset: all tools but exec/write require approval. */
export const STANDARD_POLICY: ToolPolicy = {
  mode: 'denylist',
  allowed: [],
  denied: [],
  requireApprovalForUnlisted: false,
  alwaysRequireApproval: [...HIGH_RISK_TOOLS],
};

// ── Policy presets for quick selection ──────────────────────────────────

export const POLICY_PRESETS: Record<
  string,
  { label: string; description: string; policy: ToolPolicy }
> = {
  unrestricted: {
    label: 'Unrestricted',
    description: 'Full access to all tools (default)',
    policy: DEFAULT_POLICY,
  },
  standard: {
    label: 'Standard',
    description: 'All tools available, high-risk tools require approval',
    policy: STANDARD_POLICY,
  },
  readonly: {
    label: 'Read-Only',
    description: 'Only read/search tools — no modifications',
    policy: READONLY_POLICY,
  },
  sandbox: {
    label: 'Sandbox',
    description: 'Web search and memory only — no file or exec access',
    policy: {
      mode: 'allowlist',
      allowed: ['web_search', 'web_read', 'memory_store', 'memory_search', 'self_info'],
      denied: [],
      requireApprovalForUnlisted: false,
      alwaysRequireApproval: [],
    },
  },
};

// ── Pure Functions ─────────────────────────────────────────────────────────

/**
 * Check whether a tool is allowed under a given policy.
 */
export function checkToolPolicy(toolName: string, policy: ToolPolicy): PolicyDecision {
  // Always-require-approval check (overrides everything)
  if (policy.alwaysRequireApproval.includes(toolName)) {
    return {
      allowed: true,
      requiresApproval: true,
      reason: `Tool "${toolName}" always requires approval per policy.`,
    };
  }

  switch (policy.mode) {
    case 'unrestricted':
      return { allowed: true, requiresApproval: false, reason: 'Unrestricted mode.' };

    case 'allowlist':
      if (policy.allowed.includes(toolName)) {
        return {
          allowed: true,
          requiresApproval: false,
          reason: `Tool "${toolName}" is in the allowlist.`,
        };
      }
      if (policy.requireApprovalForUnlisted) {
        return {
          allowed: true,
          requiresApproval: true,
          reason: `Tool "${toolName}" not in allowlist — requires approval.`,
        };
      }
      return {
        allowed: false,
        requiresApproval: false,
        reason: `Tool "${toolName}" is not in the allowlist.`,
      };

    case 'denylist':
      if (policy.denied.includes(toolName)) {
        return {
          allowed: false,
          requiresApproval: false,
          reason: `Tool "${toolName}" is in the denylist.`,
        };
      }
      return {
        allowed: true,
        requiresApproval: false,
        reason: `Tool "${toolName}" is not denied.`,
      };

    default:
      return {
        allowed: true,
        requiresApproval: false,
        reason: 'Unknown policy mode — defaulting to allow.',
      };
  }
}

/**
 * Filter a list of tool definitions to only those allowed by the policy.
 * Returns the names of tools that should be offered to the AI.
 */
export function filterToolsByPolicy(toolNames: string[], policy: ToolPolicy): string[] {
  return toolNames.filter((name) => {
    const decision = checkToolPolicy(name, policy);
    return decision.allowed;
  });
}

/**
 * Check if the number of tool calls in a turn exceeds the policy limit.
 */
export function isOverToolCallLimit(callCount: number, policy: ToolPolicy): boolean {
  if (!policy.maxToolCallsPerTurn) return false;
  return callCount > policy.maxToolCallsPerTurn;
}

/**
 * Get a human-readable summary of a policy.
 */
export function describePolicySummary(policy: ToolPolicy): string {
  switch (policy.mode) {
    case 'unrestricted':
      return 'Unrestricted — all tools allowed';
    case 'allowlist':
      return `Allowlist — ${policy.allowed.length} tools permitted`;
    case 'denylist':
      return `Denylist — ${policy.denied.length} tools blocked`;
    default:
      return 'Unknown policy mode';
  }
}
