// src/features/integration-guardrails/atoms.ts — Safety guardrail types
//
// Atom-level: no DOM, no IPC. Pure types and classification logic
// for integration action risk, rate limits, and agent permissions.

// ── Action risk classification ─────────────────────────────────────────

export type IntegrationRiskLevel = 'auto' | 'soft' | 'hard';

export interface IntegrationAction {
  service: string;
  action: string;
  risk: IntegrationRiskLevel;
  label: string;
  description: string;
}

/**
 * Default risk classification for common integration actions.
 * 'auto' = auto-approve (reads), 'soft' = preview card, 'hard' = explicit confirm.
 */
export const ACTION_RISK_MAP: Record<string, IntegrationRiskLevel> = {
  // Read operations — auto-approve
  list: 'auto',
  get: 'auto',
  search: 'auto',
  read: 'auto',
  fetch: 'auto',
  count: 'auto',
  check: 'auto',

  // Write operations — soft confirm
  send: 'soft',
  create: 'soft',
  update: 'soft',
  post: 'soft',
  comment: 'soft',
  assign: 'soft',
  move: 'soft',
  upload: 'soft',
  pin: 'soft',

  // Destructive operations — hard confirm
  delete: 'hard',
  remove: 'hard',
  archive: 'hard',
  close: 'hard',
  bulk_send: 'hard',
  transfer: 'hard',
  modify_billing: 'hard',
  revoke: 'hard',
};

/** Classify an action by its verb. */
export function classifyActionRisk(action: string): IntegrationRiskLevel {
  const lower = action.toLowerCase();
  for (const [verb, risk] of Object.entries(ACTION_RISK_MAP)) {
    if (lower.includes(verb)) return risk;
  }
  return 'soft'; // default to requiring confirmation
}

/** Risk level metadata. */
export function riskMeta(level: IntegrationRiskLevel): {
  icon: string;
  label: string;
  color: string;
  cssClass: string;
} {
  switch (level) {
    case 'auto':
      return {
        icon: 'check_circle',
        label: 'Auto-approved',
        color: 'var(--success, #22c55e)',
        cssClass: 'risk-auto',
      };
    case 'soft':
      return {
        icon: 'visibility',
        label: 'Preview',
        color: 'var(--warning, #f59e0b)',
        cssClass: 'risk-soft',
      };
    case 'hard':
      return {
        icon: 'warning',
        label: 'Confirm',
        color: 'var(--danger, #ef4444)',
        cssClass: 'risk-hard',
      };
  }
}

// ── Rate limits ────────────────────────────────────────────────────────

export interface RateLimitConfig {
  service: string;
  maxActions: number;
  windowMinutes: number;
}

export const DEFAULT_RATE_LIMITS: RateLimitConfig[] = [
  { service: 'slack', maxActions: 30, windowMinutes: 15 },
  { service: 'discord', maxActions: 30, windowMinutes: 15 },
  { service: 'telegram', maxActions: 30, windowMinutes: 15 },
  { service: 'gmail', maxActions: 10, windowMinutes: 15 },
  { service: 'sendgrid', maxActions: 10, windowMinutes: 15 },
  { service: 'github', maxActions: 20, windowMinutes: 15 },
  { service: 'jira', maxActions: 20, windowMinutes: 15 },
  { service: 'linear', maxActions: 20, windowMinutes: 15 },
  { service: 'hubspot', maxActions: 20, windowMinutes: 15 },
  { service: 'salesforce', maxActions: 20, windowMinutes: 15 },
  { service: 'trello', maxActions: 20, windowMinutes: 15 },
  { service: 'notion', maxActions: 20, windowMinutes: 15 },
  { service: 'google-sheets', maxActions: 30, windowMinutes: 15 },
  { service: 'shopify', maxActions: 15, windowMinutes: 15 },
  { service: 'stripe', maxActions: 10, windowMinutes: 15 },
  { service: 'twilio', maxActions: 15, windowMinutes: 15 },
  { service: 'zendesk', maxActions: 20, windowMinutes: 15 },
];

/** Default catch-all limit for unlisted services. */
export const DEFAULT_GENERIC_LIMIT: RateLimitConfig = {
  service: '*',
  maxActions: 50,
  windowMinutes: 15,
};

/** Get rate limit config for a service. */
export function getRateLimit(service: string, overrides?: RateLimitConfig[]): RateLimitConfig {
  const all = overrides ?? DEFAULT_RATE_LIMITS;
  return all.find((r) => r.service === service) ?? DEFAULT_GENERIC_LIMIT;
}

// ── Rate limit tracker (in-memory) ─────────────────────────────────────

export interface RateLimitWindow {
  service: string;
  count: number;
  windowStart: number; // epoch ms
}

const _windows: Map<string, RateLimitWindow> = new Map();

/** Record an action and check if rate limited. Returns remaining quota. */
export function checkRateLimit(
  service: string,
  config?: RateLimitConfig,
): { allowed: boolean; remaining: number; limit: number } {
  const limit = config ?? getRateLimit(service);
  const now = Date.now();
  const windowMs = limit.windowMinutes * 60_000;

  let window = _windows.get(service);
  if (!window || now - window.windowStart > windowMs) {
    window = { service, count: 0, windowStart: now };
    _windows.set(service, window);
  }

  window.count += 1;
  const remaining = Math.max(0, limit.maxActions - window.count);

  return {
    allowed: window.count <= limit.maxActions,
    remaining,
    limit: limit.maxActions,
  };
}

/** Reset rate limit window for a service (e.g. user override). */
export function resetRateLimit(service: string): void {
  _windows.delete(service);
}

/** Bump the limit for a service by N actions (one-time override). */
export function bumpRateLimit(service: string, extra: number): void {
  const window = _windows.get(service);
  if (window) {
    window.count = Math.max(0, window.count - extra);
  }
}

// ── Agent service permissions ──────────────────────────────────────────

export type AccessLevel = 'none' | 'read' | 'write' | 'full';

export interface AgentServicePermission {
  agentId: string;
  service: string;
  access: AccessLevel;
}

/** Check if an access level allows a specific action verb. */
export function isActionAllowed(access: AccessLevel, actionVerb: string): boolean {
  if (access === 'none') return false;
  if (access === 'full') return true;

  const risk = classifyActionRisk(actionVerb);
  if (access === 'read') return risk === 'auto';
  if (access === 'write') return true; // write can do read + write + delete
  return false;
}

/** Access level metadata. */
export function accessMeta(level: AccessLevel): {
  icon: string;
  label: string;
  color: string;
} {
  switch (level) {
    case 'none':
      return { icon: 'block', label: 'No Access', color: 'var(--text-tertiary)' };
    case 'read':
      return { icon: 'visibility', label: 'Read Only', color: 'var(--success, #22c55e)' };
    case 'write':
      return { icon: 'edit', label: 'Read & Write', color: 'var(--warning, #f59e0b)' };
    case 'full':
      return {
        icon: 'admin_panel_settings',
        label: 'Full Access',
        color: 'var(--danger, #ef4444)',
      };
  }
}

// ── Credential audit log ───────────────────────────────────────────────

export interface CredentialUsageLog {
  timestamp: string;
  agent: string;
  service: string;
  action: string;
  accessLevel: AccessLevel;
  approved: boolean;
  result: 'success' | 'denied' | 'failed';
}

// ── Dry-run plan ───────────────────────────────────────────────────────

export interface DryRunPlan {
  id: string;
  steps: DryRunStep[];
  totalActions: number;
  highRiskCount: number;
}

export interface DryRunStep {
  index: number;
  service: string;
  action: string;
  target: string;
  risk: IntegrationRiskLevel;
  preview?: string;
}

/** Count high-risk steps in a plan. */
export function countHighRisk(plan: DryRunPlan): number {
  return plan.steps.filter((s) => s.risk === 'hard').length;
}

/** Check if a plan requires hard confirmation. */
export function planRequiresConfirm(plan: DryRunPlan): boolean {
  return plan.steps.some((s) => s.risk === 'hard') || plan.steps.length > 3;
}
