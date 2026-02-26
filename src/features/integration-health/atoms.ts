// src/features/integration-health/atoms.ts — Integration health monitoring types
//
// Atom-level: pure types and helpers. No DOM, no IPC.
// Used by Today view, integration detail panels, and notification system.

// ── Types ──────────────────────────────────────────────────────────────

export type HealthStatus = 'healthy' | 'degraded' | 'error' | 'expired' | 'unknown';

export interface ServiceHealth {
  service: string;
  serviceName: string;
  icon: string;
  status: HealthStatus;
  lastChecked: string;
  message?: string;
  /** Token expiry date (ISO string), if known */
  tokenExpiry?: string;
  /** Days until token expires */
  daysUntilExpiry?: number;
  /** Number of recent failures */
  recentFailures: number;
  /** Number of successful actions today */
  todayActions: number;
}

export interface HealthSummary {
  total: number;
  healthy: number;
  degraded: number;
  error: number;
  expired: number;
  needsAttention: ServiceHealth[];
}

export interface IntegrationSuggestion {
  id: string;
  service: string;
  serviceName: string;
  icon: string;
  text: string;
  action: string;
  actionLabel: string;
}

export interface ChainRule {
  id: string;
  name: string;
  trigger: { service: string; action: string };
  then: { service: string; action: string; params?: Record<string, string> };
  enabled: boolean;
}

// ── Pure helpers ───────────────────────────────────────────────────────

export function statusIcon(status: HealthStatus): string {
  switch (status) {
    case 'healthy':
      return 'check_circle';
    case 'degraded':
      return 'warning';
    case 'error':
      return 'error';
    case 'expired':
      return 'lock_clock';
    case 'unknown':
      return 'help';
  }
}

export function statusColor(status: HealthStatus): string {
  switch (status) {
    case 'healthy':
      return 'var(--success, #22c55e)';
    case 'degraded':
      return 'var(--warning, #f59e0b)';
    case 'error':
      return 'var(--danger, #ef4444)';
    case 'expired':
      return 'var(--danger, #ef4444)';
    case 'unknown':
      return 'var(--text-secondary)';
  }
}

export function statusLabel(status: HealthStatus): string {
  switch (status) {
    case 'healthy':
      return 'Connected';
    case 'degraded':
      return 'Degraded';
    case 'error':
      return 'Error';
    case 'expired':
      return 'Token Expired';
    case 'unknown':
      return 'Unknown';
  }
}

/** Compute overall health summary. */
export function computeHealthSummary(services: ServiceHealth[]): HealthSummary {
  let healthy = 0;
  let degraded = 0;
  let error = 0;
  let expired = 0;
  const needsAttention: ServiceHealth[] = [];

  for (const s of services) {
    switch (s.status) {
      case 'healthy':
        healthy++;
        break;
      case 'degraded':
        degraded++;
        needsAttention.push(s);
        break;
      case 'error':
        error++;
        needsAttention.push(s);
        break;
      case 'expired':
        expired++;
        needsAttention.push(s);
        break;
      default:
        break;
    }
  }

  return { total: services.length, healthy, degraded, error, expired, needsAttention };
}

/** Determine health status from token expiry and failures. */
export function deriveHealthStatus(
  tokenExpiry: string | undefined,
  recentFailures: number,
  hasCredentials: boolean,
): HealthStatus {
  if (!hasCredentials) return 'unknown';

  if (tokenExpiry) {
    const daysLeft = Math.floor((new Date(tokenExpiry).getTime() - Date.now()) / 86_400_000);
    if (daysLeft <= 0) return 'expired';
    if (daysLeft <= 7) return 'degraded';
  }

  if (recentFailures >= 3) return 'error';
  if (recentFailures >= 1) return 'degraded';

  return 'healthy';
}

/** Days until a token expires (negative if already expired). */
export function daysUntilExpiry(expiryDate: string): number {
  return Math.floor((new Date(expiryDate).getTime() - Date.now()) / 86_400_000);
}

/** Generate smart suggestions based on connected services. */
export function generateSuggestions(connectedServices: string[]): IntegrationSuggestion[] {
  const suggestions: IntegrationSuggestion[] = [];

  const SUGGESTION_TEMPLATES: Record<string, IntegrationSuggestion> = {
    gmail: {
      id: 'suggest-gmail',
      service: 'gmail',
      serviceName: 'Gmail',
      icon: 'mail',
      text: 'You have unread emails — want me to summarize them?',
      action: 'gmail_search_inbox',
      actionLabel: 'Summarize inbox',
    },
    slack: {
      id: 'suggest-slack',
      service: 'slack',
      serviceName: 'Slack',
      icon: 'tag',
      text: 'Check your Slack messages and mentions',
      action: 'slack_read_channel',
      actionLabel: 'Check Slack',
    },
    github: {
      id: 'suggest-github',
      service: 'github',
      serviceName: 'GitHub',
      icon: 'code',
      text: 'Review assigned issues and open PRs',
      action: 'github_list_issues',
      actionLabel: 'Check GitHub',
    },
    hubspot: {
      id: 'suggest-hubspot',
      service: 'hubspot',
      serviceName: 'HubSpot',
      icon: 'handshake',
      text: 'Check your sales pipeline and deals',
      action: 'hubspot_list_deals',
      actionLabel: 'View deals',
    },
    trello: {
      id: 'suggest-trello',
      service: 'trello',
      serviceName: 'Trello',
      icon: 'dashboard',
      text: 'Check for stale cards on your boards',
      action: 'trello_list_cards',
      actionLabel: 'Review boards',
    },
    jira: {
      id: 'suggest-jira',
      service: 'jira',
      serviceName: 'Jira',
      icon: 'bug_report',
      text: 'Review your assigned tickets',
      action: 'jira_search_issues',
      actionLabel: 'Check Jira',
    },
  };

  for (const svc of connectedServices) {
    const tpl = SUGGESTION_TEMPLATES[svc];
    if (tpl) suggestions.push({ ...tpl });
  }

  return suggestions.slice(0, 3); // max 3 suggestions
}
