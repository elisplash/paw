// ─────────────────────────────────────────────────────────────────────────────
// Channel Routing — Public API
// ─────────────────────────────────────────────────────────────────────────────

// Atoms (pure)
export {
  type RoutingRule,
  type RoutingConfig,
  type RouteResult,
  ALL_CHANNELS,
  DEFAULT_ROUTING_CONFIG,
  resolveRoute,
  createRule,
  validateRoutingConfig,
  describeRoutingConfig,
} from './atoms';

// Molecules (side-effects)
export {
  loadRoutingConfig,
  saveRoutingConfig,
  addRoutingRule,
  removeRoutingRule,
  updateRoutingRule,
  reorderRule,
  setDefaultAgent,
  resolveChannelAgent,
} from './molecules';
