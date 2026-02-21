// ─────────────────────────────────────────────────────────────────────────────
// Channel Routing — Molecules
// Composed behaviours: persist routing config, resolve routes for channels.
// ─────────────────────────────────────────────────────────────────────────────

import { pawEngine } from '../../engine';
import {
  type RoutingConfig,
  type RoutingRule,
  type RouteResult,
  DEFAULT_ROUTING_CONFIG,
  resolveRoute,
} from './atoms';

// ── Storage ────────────────────────────────────────────────────────────────

// const ENGINE_CONFIG_KEY = 'channel_routing';

/**
 * Load routing config from the engine's config store.
 */
export async function loadRoutingConfig(): Promise<RoutingConfig> {
  try {
    // const config = await pawEngine.getConfig();
    void pawEngine.getConfig();
    // The routing config is stored as a JSON string in engine_config
    // We use localStorage as a fallback if engine isn't available
    const raw = localStorage.getItem('paw_channel_routing');
    if (raw) {
      return JSON.parse(raw);
    }
  } catch {
    // ignore
  }
  return { ...DEFAULT_ROUTING_CONFIG };
}

/**
 * Save routing config.
 */
export async function saveRoutingConfig(config: RoutingConfig): Promise<void> {
  localStorage.setItem('paw_channel_routing', JSON.stringify(config));
}

/**
 * Add a routing rule.
 */
export async function addRoutingRule(rule: RoutingRule): Promise<RoutingConfig> {
  const config = await loadRoutingConfig();
  config.rules.push(rule);
  await saveRoutingConfig(config);
  return config;
}

/**
 * Remove a routing rule by ID.
 */
export async function removeRoutingRule(ruleId: string): Promise<RoutingConfig> {
  const config = await loadRoutingConfig();
  config.rules = config.rules.filter((r) => r.id !== ruleId);
  await saveRoutingConfig(config);
  return config;
}

/**
 * Update a routing rule.
 */
export async function updateRoutingRule(
  ruleId: string,
  updates: Partial<RoutingRule>,
): Promise<RoutingConfig> {
  const config = await loadRoutingConfig();
  const idx = config.rules.findIndex((r) => r.id === ruleId);
  if (idx >= 0) {
    config.rules[idx] = { ...config.rules[idx], ...updates };
  }
  await saveRoutingConfig(config);
  return config;
}

/**
 * Reorder routing rules (move rule to new position).
 */
export async function reorderRule(ruleId: string, newIndex: number): Promise<RoutingConfig> {
  const config = await loadRoutingConfig();
  const idx = config.rules.findIndex((r) => r.id === ruleId);
  if (idx >= 0) {
    const [rule] = config.rules.splice(idx, 1);
    config.rules.splice(newIndex, 0, rule);
  }
  await saveRoutingConfig(config);
  return config;
}

/**
 * Set the default agent ID.
 */
export async function setDefaultAgent(agentId: string): Promise<RoutingConfig> {
  const config = await loadRoutingConfig();
  config.defaultAgentId = agentId;
  await saveRoutingConfig(config);
  return config;
}

// ── Resolution ─────────────────────────────────────────────────────────────

/**
 * Resolve the agent for an incoming channel message.
 * This is the main entry point called by channel bridges.
 */
export async function resolveChannelAgent(
  channel: string,
  userId: string,
  channelId?: string,
): Promise<RouteResult> {
  const config = await loadRoutingConfig();
  return resolveRoute(config, channel, userId, channelId);
}
