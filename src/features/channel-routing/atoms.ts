// ─────────────────────────────────────────────────────────────────────────────
// Channel Routing — Atoms
// Pure functions for resolving which agent handles messages from which channel.
// ─────────────────────────────────────────────────────────────────────────────

// ── Types ──────────────────────────────────────────────────────────────────

/**
 * A routing rule that binds a channel (or channel+user) to a specific agent.
 * Rules are evaluated in order — first match wins.
 */
export interface RoutingRule {
  id: string;
  /** Channel type: 'telegram' | 'discord' | 'irc' | 'slack' | 'matrix' | 'webchat' | '*' */
  channel: string;
  /** Optional: restrict to specific user IDs. Empty = all users. */
  userFilter: string[];
  /** Optional: restrict to specific channel/group IDs. Empty = all. */
  channelIdFilter: string[];
  /** Agent ID to route to */
  agentId: string;
  /** Human-readable description */
  label: string;
  /** Whether this rule is active */
  enabled: boolean;
}

/**
 * The full routing configuration.
 */
export interface RoutingConfig {
  /** Ordered list of routing rules (first match wins) */
  rules: RoutingRule[];
  /** Default agent ID for messages that don't match any rule */
  defaultAgentId: string;
}

/**
 * Result of resolving a route.
 */
export interface RouteResult {
  agentId: string;
  matchedRuleId: string | null;
  matchedRuleLabel: string | null;
}

// ── Constants ──────────────────────────────────────────────────────────────

export const ALL_CHANNELS = [
  'telegram',
  'discord',
  'irc',
  'slack',
  'matrix',
  'mattermost',
  'nextcloud',
  'nostr',
  'twitch',
  'webchat',
] as const;

export const DEFAULT_ROUTING_CONFIG: RoutingConfig = {
  rules: [],
  defaultAgentId: 'default',
};

// ── Pure Functions ─────────────────────────────────────────────────────────

/**
 * Resolve which agent should handle a message based on routing rules.
 * Evaluates rules in order — first match wins.
 */
export function resolveRoute(
  config: RoutingConfig,
  channel: string,
  userId: string,
  channelId?: string,
): RouteResult {
  for (const rule of config.rules) {
    if (!rule.enabled) continue;

    // Channel match: '*' matches all, or exact match
    if (rule.channel !== '*' && rule.channel !== channel) continue;

    // User filter: empty = matches all users
    if (rule.userFilter.length > 0 && !rule.userFilter.includes(userId)) continue;

    // Channel ID filter: empty = matches all channel/group IDs
    if (rule.channelIdFilter.length > 0 && channelId && !rule.channelIdFilter.includes(channelId))
      continue;

    return {
      agentId: rule.agentId,
      matchedRuleId: rule.id,
      matchedRuleLabel: rule.label,
    };
  }

  // No rule matched — use default agent
  return {
    agentId: config.defaultAgentId,
    matchedRuleId: null,
    matchedRuleLabel: null,
  };
}

/**
 * Create a new routing rule with a unique ID.
 */
export function createRule(
  channel: string,
  agentId: string,
  label: string,
  opts?: {
    userFilter?: string[];
    channelIdFilter?: string[];
    enabled?: boolean;
  },
): RoutingRule {
  return {
    id: `route_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
    channel,
    agentId,
    label,
    userFilter: opts?.userFilter ?? [],
    channelIdFilter: opts?.channelIdFilter ?? [],
    enabled: opts?.enabled ?? true,
  };
}

/**
 * Validate a routing config for common issues.
 * Returns a list of warnings/errors.
 */
export function validateRoutingConfig(config: RoutingConfig): string[] {
  const issues: string[] = [];

  // Check for duplicate rules (same channel+agent)
  const seen = new Set<string>();
  for (const rule of config.rules) {
    const key = `${rule.channel}→${rule.agentId}`;
    if (seen.has(key) && rule.userFilter.length === 0 && rule.channelIdFilter.length === 0) {
      issues.push(`Duplicate rule: ${rule.label} (${key})`);
    }
    seen.add(key);
  }

  // Check for unreachable rules (wildcard before specific)
  let wildcardSeen = false;
  for (const rule of config.rules) {
    if (!rule.enabled) continue;
    if (rule.channel === '*' && rule.userFilter.length === 0) {
      if (wildcardSeen) {
        issues.push(`Rule "${rule.label}" is unreachable (earlier wildcard catches all)`);
      }
      wildcardSeen = true;
    }
    if (wildcardSeen && rule.channel !== '*') {
      issues.push(`Rule "${rule.label}" is unreachable (earlier wildcard catches all)`);
    }
  }

  return issues;
}

/**
 * Get a human-readable summary of routing config.
 */
export function describeRoutingConfig(config: RoutingConfig): string {
  if (config.rules.length === 0) {
    return `All channels → ${config.defaultAgentId}`;
  }
  const active = config.rules.filter((r) => r.enabled);
  return `${active.length} routing rules, default → ${config.defaultAgentId}`;
}
