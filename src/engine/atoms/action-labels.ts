// src/engine/atoms/action-labels.ts — Human-readable action label translator
//
// Atom-level: pure functions, no DOM, no IPC.
// Translates raw integration tool calls into readable summaries.

// ── Types ──────────────────────────────────────────────────────────────

export interface IntegrationActionLog {
  id: string;
  timestamp: string;
  service: string;
  serviceName: string;
  action: string;
  actionLabel: string;
  summary: string;
  agent: string;
  status: 'success' | 'failed' | 'running';
  durationMs: number;
  input?: unknown;
  output?: unknown;
  errorMessage?: string;
  externalUrl?: string;
}

export type OutputCardType = 'table' | 'summary' | 'timeline' | 'notification';

export interface ActionStats {
  total: number;
  success: number;
  failed: number;
  running: number;
  byService: Record<string, { count: number; failed: number; label: string }>;
}

// ── Label templates ────────────────────────────────────────────────────

interface LabelTemplate {
  pattern: RegExp;
  label: (match: RegExpMatchArray, input?: Record<string, unknown>) => string;
}

const SERVICE_LABELS: Record<string, LabelTemplate[]> = {
  slack: [
    { pattern: /post_?message/i, label: (_m, i) => `Sent message to ${_ch(i)}` },
    { pattern: /list_?channels/i, label: () => 'Listed channels' },
    { pattern: /get_?channel/i, label: (_m, i) => `Fetched channel info for ${_ch(i)}` },
    { pattern: /list_?users/i, label: () => 'Listed workspace users' },
    { pattern: /invite/i, label: (_m, i) => `Invited user to ${_ch(i)}` },
    { pattern: /pin/i, label: (_m, i) => `Pinned message in ${_ch(i)}` },
    { pattern: /search/i, label: (_m, i) => `Searched Slack for "${_str(i, 'query')}"` },
  ],
  discord: [
    { pattern: /send_?message/i, label: (_m, i) => `Sent message to ${_ch(i)}` },
    { pattern: /list/i, label: () => 'Listed Discord resources' },
  ],
  github: [
    {
      pattern: /create_?issue/i,
      label: (_m, i) => `Created issue '${_str(i, 'title')}' in ${_str(i, 'repo', 'repository')}`,
    },
    {
      pattern: /create_?pr|create_?pull/i,
      label: (_m, i) => `Created PR '${_str(i, 'title')}' in ${_str(i, 'repo', 'repository')}`,
    },
    {
      pattern: /list_?issues/i,
      label: (_m, i) => `Listed issues in ${_str(i, 'repo', 'repository')}`,
    },
    {
      pattern: /list_?prs|list_?pulls/i,
      label: (_m, i) => `Listed PRs in ${_str(i, 'repo', 'repository')}`,
    },
    { pattern: /comment/i, label: (_m, i) => `Commented on #${_str(i, 'issue_number', 'number')}` },
    { pattern: /close/i, label: (_m, i) => `Closed #${_str(i, 'issue_number', 'number')}` },
    { pattern: /merge/i, label: (_m, i) => `Merged PR #${_str(i, 'number')}` },
    { pattern: /star/i, label: (_m, i) => `Starred ${_str(i, 'repo')}` },
  ],
  gmail: [
    {
      pattern: /send/i,
      label: (_m, i) => `Sent email '${_str(i, 'subject')}' to ${_str(i, 'to')}`,
    },
    { pattern: /list|search/i, label: () => 'Searched emails' },
    { pattern: /read|get/i, label: (_m, i) => `Read email: ${_str(i, 'subject')}` },
    { pattern: /draft/i, label: (_m, i) => `Created draft: ${_str(i, 'subject')}` },
  ],
  hubspot: [
    { pattern: /list_?deals/i, label: () => 'Fetched deals' },
    { pattern: /create_?deal/i, label: (_m, i) => `Created deal '${_str(i, 'name', 'dealname')}'` },
    { pattern: /list_?contacts/i, label: () => 'Fetched contacts' },
    { pattern: /create_?contact/i, label: (_m, i) => `Created contact ${_str(i, 'email')}` },
    { pattern: /update/i, label: (_m, i) => `Updated ${_str(i, 'type', 'resource')}` },
  ],
  jira: [
    {
      pattern: /create_?issue/i,
      label: (_m, i) => `Created ticket '${_str(i, 'summary', 'title')}'`,
    },
    {
      pattern: /list_?issues|search/i,
      label: (_m, i) => `Searched Jira: ${_str(i, 'jql', 'query')}`,
    },
    {
      pattern: /transition|update_?status/i,
      label: (_m, i) => `Moved ${_str(i, 'key')} to ${_str(i, 'status')}`,
    },
    { pattern: /comment/i, label: (_m, i) => `Commented on ${_str(i, 'key')}` },
    { pattern: /assign/i, label: (_m, i) => `Assigned ${_str(i, 'key')}` },
  ],
  linear: [
    { pattern: /create_?issue/i, label: (_m, i) => `Created issue '${_str(i, 'title')}'` },
    { pattern: /list/i, label: () => 'Listed Linear issues' },
    { pattern: /update/i, label: (_m, i) => `Updated ${_str(i, 'id')}` },
  ],
  trello: [
    { pattern: /create_?card/i, label: (_m, i) => `Created card '${_str(i, 'name')}'` },
    { pattern: /move/i, label: (_m, i) => `Moved card to ${_str(i, 'list', 'listName')}` },
    { pattern: /list_?cards/i, label: () => 'Listed Trello cards' },
    { pattern: /archive/i, label: (_m, i) => `Archived card '${_str(i, 'name')}'` },
  ],
  notion: [
    { pattern: /create_?page/i, label: (_m, i) => `Created page '${_str(i, 'title')}'` },
    { pattern: /update_?page/i, label: (_m, i) => `Updated page '${_str(i, 'title')}'` },
    { pattern: /query|search/i, label: () => 'Searched Notion' },
  ],
  'google-sheets': [
    { pattern: /read|get/i, label: (_m, i) => `Read from ${_str(i, 'spreadsheetId', 'sheet')}` },
    {
      pattern: /write|update|append/i,
      label: (_m, i) => `Updated ${_str(i, 'spreadsheetId', 'sheet')}`,
    },
    { pattern: /create/i, label: (_m, i) => `Created spreadsheet '${_str(i, 'title')}'` },
  ],
  shopify: [
    { pattern: /list_?orders/i, label: () => 'Fetched orders' },
    { pattern: /list_?products/i, label: () => 'Fetched products' },
    { pattern: /create_?product/i, label: (_m, i) => `Created product '${_str(i, 'title')}'` },
    { pattern: /update_?inventory/i, label: () => 'Updated inventory' },
  ],
  stripe: [
    { pattern: /list_?charges|list_?payments/i, label: () => 'Fetched payments' },
    { pattern: /create_?charge/i, label: (_m, i) => `Created charge: $${_str(i, 'amount')}` },
    { pattern: /refund/i, label: (_m, i) => `Refunded: $${_str(i, 'amount')}` },
    { pattern: /list_?customers/i, label: () => 'Fetched customers' },
  ],
  salesforce: [
    { pattern: /query/i, label: (_m, i) => `Queried Salesforce: ${_str(i, 'query')}` },
    { pattern: /create/i, label: (_m, i) => `Created ${_str(i, 'sobject', 'type')} record` },
    { pattern: /update/i, label: (_m, i) => `Updated ${_str(i, 'sobject', 'type')} record` },
  ],
  sendgrid: [
    { pattern: /send/i, label: (_m, i) => `Sent email to ${_str(i, 'to')}` },
    { pattern: /list/i, label: () => 'Listed SendGrid resources' },
  ],
  twilio: [
    { pattern: /send_?sms|send_?message/i, label: (_m, i) => `Sent SMS to ${_str(i, 'to')}` },
    { pattern: /call/i, label: (_m, i) => `Called ${_str(i, 'to')}` },
  ],
  zendesk: [
    { pattern: /create_?ticket/i, label: (_m, i) => `Created ticket '${_str(i, 'subject')}'` },
    { pattern: /list_?tickets/i, label: () => 'Listed tickets' },
    { pattern: /update/i, label: (_m, i) => `Updated ticket #${_str(i, 'id')}` },
  ],
  telegram: [
    {
      pattern: /send_?message/i,
      label: (_m, i) => `Sent message to ${_str(i, 'chat_id', 'chatId')}`,
    },
    { pattern: /send_?photo/i, label: () => 'Sent photo' },
  ],
};

// ── Translator ─────────────────────────────────────────────────────────

/**
 * Translate a raw tool-call name + input into a human-readable label.
 */
export function translateAction(
  service: string,
  action: string,
  input?: Record<string, unknown>,
): string {
  const templates = SERVICE_LABELS[service];
  if (templates) {
    for (const t of templates) {
      const match = action.match(t.pattern);
      if (match) return t.label(match, input);
    }
  }
  // Fallback: title-case the action
  const pretty = action.replace(/[_-]/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
  return `Ran ${service} action: ${pretty}`;
}

/**
 * Detect the OutputCardType for a result.
 */
export function detectOutputType(action: string, result?: unknown): OutputCardType {
  const lower = action.toLowerCase();
  if (lower.includes('list') || lower.includes('search') || lower.includes('query')) {
    if (
      Array.isArray(result) ||
      (result && typeof result === 'object' && 'items' in (result as Record<string, unknown>))
    ) {
      return 'table';
    }
    return 'summary';
  }
  if (lower.includes('message') || lower.includes('notification') || lower.includes('unread')) {
    return 'notification';
  }
  if (lower.includes('log') || lower.includes('history') || lower.includes('activity')) {
    return 'timeline';
  }
  return 'summary';
}

/**
 * Compute aggregate stats from a list of actions.
 */
export function computeStats(actions: IntegrationActionLog[]): ActionStats {
  const byService: ActionStats['byService'] = {};
  let success = 0;
  let failed = 0;
  let running = 0;

  for (const a of actions) {
    if (a.status === 'success') success++;
    else if (a.status === 'failed') failed++;
    else running++;

    if (!byService[a.service]) {
      byService[a.service] = { count: 0, failed: 0, label: a.serviceName };
    }
    byService[a.service].count++;
    if (a.status === 'failed') byService[a.service].failed++;
  }

  return { total: actions.length, success, failed, running, byService };
}

/**
 * Format a duration in ms to a human-readable string.
 */
export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  return `${Math.floor(ms / 60_000)}m ${Math.round((ms % 60_000) / 1000)}s`;
}

/**
 * Format a relative timestamp (e.g. "2m ago", "1h ago").
 */
export function timeAgo(timestamp: string): string {
  const diff = Date.now() - new Date(timestamp).getTime();
  if (diff < 60_000) return 'just now';
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
  return `${Math.floor(diff / 86_400_000)}d ago`;
}

// ── Internal helpers ───────────────────────────────────────────────────

/** Extract a channel-like field from input. */
function _ch(input?: Record<string, unknown>): string {
  if (!input) return '(channel)';
  const v = input.channel ?? input.channelId ?? input.channel_id ?? input.channelName;
  return typeof v === 'string' ? v : '(channel)';
}

/** Extract a string field, trying multiple keys. */
function _str(input?: Record<string, unknown>, ...keys: string[]): string {
  if (!input) return '…';
  for (const k of keys) {
    const v = input[k];
    if (typeof v === 'string' && v.length > 0) {
      return v.length > 60 ? `${v.slice(0, 57)}…` : v;
    }
  }
  return '…';
}
