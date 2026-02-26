// src/engine/atoms/auto-discover.ts — Agent Integration Auto-Discovery
//
// PURE ATOM: Matches user intent to available integrations.
// No DOM, no IPC — just pattern detection and suggestion generation.

// ── Types ──────────────────────────────────────────────────────────────

export interface IntentMatch {
  service: string;
  serviceName: string;
  confidence: 'high' | 'medium' | 'low';
  action: string; // e.g. "send_email", "create_issue"
  actionLabel: string; // human-readable: "Send an email"
  connected: boolean; // is the service currently connected?
}

export interface DiscoveryResult {
  matches: IntentMatch[];
  hasConnectedMatch: boolean; // at least one match is connected
  hasDisconnectedMatch: boolean; // at least one match is NOT connected
  bestMatch: IntentMatch | null;
  systemHint: string | null; // system prompt addition
}

export interface IntentPattern {
  pattern: RegExp;
  service: string;
  serviceName: string;
  action: string;
  actionLabel: string;
  confidence: 'high' | 'medium' | 'low';
}

// ── Intent Pattern Registry ────────────────────────────────────────────

export const INTENT_PATTERNS: IntentPattern[] = [
  // Email / Gmail
  {
    pattern: /\b(send|write|compose|draft|reply|forward)\b.*\b(email|mail|message)\b/i,
    service: 'gmail',
    serviceName: 'Gmail',
    action: 'send_email',
    actionLabel: 'Send an email',
    confidence: 'high',
  },
  {
    pattern: /\b(check|read|summarize|inbox|unread)\b.*\b(email|mail|inbox)\b/i,
    service: 'gmail',
    serviceName: 'Gmail',
    action: 'read_inbox',
    actionLabel: 'Check inbox',
    confidence: 'high',
  },
  {
    pattern: /\bemail\b/i,
    service: 'gmail',
    serviceName: 'Gmail',
    action: 'email',
    actionLabel: 'Email action',
    confidence: 'low',
  },

  // Slack
  {
    pattern: /\b(send|post|message|write)\b.*\b(slack|channel|#\w+)\b/i,
    service: 'slack',
    serviceName: 'Slack',
    action: 'post_message',
    actionLabel: 'Send a Slack message',
    confidence: 'high',
  },
  {
    pattern: /\b(check|read|summarize)\b.*\bslack\b/i,
    service: 'slack',
    serviceName: 'Slack',
    action: 'read_messages',
    actionLabel: 'Read Slack messages',
    confidence: 'high',
  },
  {
    pattern: /\bslack\b/i,
    service: 'slack',
    serviceName: 'Slack',
    action: 'slack',
    actionLabel: 'Slack action',
    confidence: 'low',
  },

  // GitHub
  {
    pattern: /\b(create|open|file)\b.*\b(issue|bug|ticket)\b.*\bgithub\b/i,
    service: 'github',
    serviceName: 'GitHub',
    action: 'create_issue',
    actionLabel: 'Create a GitHub issue',
    confidence: 'high',
  },
  {
    pattern: /\b(create|open)\b.*\b(pr|pull\s*request)\b/i,
    service: 'github',
    serviceName: 'GitHub',
    action: 'create_pr',
    actionLabel: 'Create a pull request',
    confidence: 'high',
  },
  {
    pattern: /\b(github|repo|repository)\b.*\b(issue|pr|pull|commit|branch)\b/i,
    service: 'github',
    serviceName: 'GitHub',
    action: 'github_action',
    actionLabel: 'GitHub action',
    confidence: 'medium',
  },
  {
    pattern: /\bgithub\b/i,
    service: 'github',
    serviceName: 'GitHub',
    action: 'github',
    actionLabel: 'GitHub action',
    confidence: 'low',
  },

  // Discord
  {
    pattern: /\b(send|post|message)\b.*\bdiscord\b/i,
    service: 'discord',
    serviceName: 'Discord',
    action: 'send_message',
    actionLabel: 'Send a Discord message',
    confidence: 'high',
  },
  {
    pattern: /\bdiscord\b/i,
    service: 'discord',
    serviceName: 'Discord',
    action: 'discord',
    actionLabel: 'Discord action',
    confidence: 'low',
  },

  // Telegram
  {
    pattern: /\b(send|post|message)\b.*\btelegram\b/i,
    service: 'telegram',
    serviceName: 'Telegram',
    action: 'send_message',
    actionLabel: 'Send a Telegram message',
    confidence: 'high',
  },
  {
    pattern: /\btelegram\b/i,
    service: 'telegram',
    serviceName: 'Telegram',
    action: 'telegram',
    actionLabel: 'Telegram action',
    confidence: 'low',
  },

  // Trello
  {
    pattern: /\b(create|add|make)\b.*\b(card|trello)\b/i,
    service: 'trello',
    serviceName: 'Trello',
    action: 'create_card',
    actionLabel: 'Create a Trello card',
    confidence: 'high',
  },
  {
    pattern: /\b(move|update|assign)\b.*\b(card|trello)\b/i,
    service: 'trello',
    serviceName: 'Trello',
    action: 'update_card',
    actionLabel: 'Update a Trello card',
    confidence: 'high',
  },
  {
    pattern: /\btrello\b/i,
    service: 'trello',
    serviceName: 'Trello',
    action: 'trello',
    actionLabel: 'Trello action',
    confidence: 'low',
  },

  // Jira
  {
    pattern: /\b(create|file|open)\b.*\b(jira|ticket|issue)\b/i,
    service: 'jira',
    serviceName: 'Jira',
    action: 'create_issue',
    actionLabel: 'Create a Jira issue',
    confidence: 'high',
  },
  {
    pattern: /\bjira\b/i,
    service: 'jira',
    serviceName: 'Jira',
    action: 'jira',
    actionLabel: 'Jira action',
    confidence: 'low',
  },

  // Linear
  {
    pattern: /\b(create|file|add)\b.*\blinear\b/i,
    service: 'linear',
    serviceName: 'Linear',
    action: 'create_issue',
    actionLabel: 'Create a Linear issue',
    confidence: 'high',
  },
  {
    pattern: /\blinear\b/i,
    service: 'linear',
    serviceName: 'Linear',
    action: 'linear',
    actionLabel: 'Linear action',
    confidence: 'low',
  },

  // Notion
  {
    pattern: /\b(create|add|write)\b.*\b(notion|page|doc)\b/i,
    service: 'notion',
    serviceName: 'Notion',
    action: 'create_page',
    actionLabel: 'Create a Notion page',
    confidence: 'medium',
  },
  {
    pattern: /\bnotion\b/i,
    service: 'notion',
    serviceName: 'Notion',
    action: 'notion',
    actionLabel: 'Notion action',
    confidence: 'low',
  },

  // Google Sheets
  {
    pattern: /\b(add|update|insert|append)\b.*\b(spreadsheet|sheet|row)\b/i,
    service: 'google-sheets',
    serviceName: 'Google Sheets',
    action: 'update_sheet',
    actionLabel: 'Update spreadsheet',
    confidence: 'medium',
  },
  {
    pattern: /\bsheet(s)?\b/i,
    service: 'google-sheets',
    serviceName: 'Google Sheets',
    action: 'sheets',
    actionLabel: 'Sheets action',
    confidence: 'low',
  },

  // HubSpot
  {
    pattern: /\b(create|add|update)\b.*\b(contact|deal|hubspot)\b/i,
    service: 'hubspot',
    serviceName: 'HubSpot',
    action: 'crm_action',
    actionLabel: 'HubSpot CRM action',
    confidence: 'high',
  },
  {
    pattern: /\b(deal|pipeline|contact)\b.*\bhubspot\b/i,
    service: 'hubspot',
    serviceName: 'HubSpot',
    action: 'crm_query',
    actionLabel: 'Query HubSpot',
    confidence: 'medium',
  },
  {
    pattern: /\bhubspot\b/i,
    service: 'hubspot',
    serviceName: 'HubSpot',
    action: 'hubspot',
    actionLabel: 'HubSpot action',
    confidence: 'low',
  },

  // Salesforce
  {
    pattern: /\bsalesforce\b/i,
    service: 'salesforce',
    serviceName: 'Salesforce',
    action: 'salesforce',
    actionLabel: 'Salesforce action',
    confidence: 'low',
  },

  // Stripe
  {
    pattern: /\b(create|send)\b.*\b(invoice|payment|charge|stripe)\b/i,
    service: 'stripe',
    serviceName: 'Stripe',
    action: 'create_invoice',
    actionLabel: 'Create an invoice',
    confidence: 'high',
  },
  {
    pattern: /\bstripe\b/i,
    service: 'stripe',
    serviceName: 'Stripe',
    action: 'stripe',
    actionLabel: 'Stripe action',
    confidence: 'low',
  },

  // Shopify
  {
    pattern: /\b(order|product|shopify|inventory)\b/i,
    service: 'shopify',
    serviceName: 'Shopify',
    action: 'shopify',
    actionLabel: 'Shopify action',
    confidence: 'low',
  },

  // Zendesk
  {
    pattern: /\b(create|open)\b.*\b(zendesk|ticket|support)\b/i,
    service: 'zendesk',
    serviceName: 'Zendesk',
    action: 'create_ticket',
    actionLabel: 'Create a support ticket',
    confidence: 'medium',
  },
  {
    pattern: /\bzendesk\b/i,
    service: 'zendesk',
    serviceName: 'Zendesk',
    action: 'zendesk',
    actionLabel: 'Zendesk action',
    confidence: 'low',
  },

  // Twilio
  {
    pattern: /\b(send|text)\b.*\b(sms|text|twilio)\b/i,
    service: 'twilio',
    serviceName: 'Twilio',
    action: 'send_sms',
    actionLabel: 'Send an SMS',
    confidence: 'high',
  },
  {
    pattern: /\btwilio\b/i,
    service: 'twilio',
    serviceName: 'Twilio',
    action: 'twilio',
    actionLabel: 'Twilio action',
    confidence: 'low',
  },

  // SendGrid
  {
    pattern: /\bsendgrid\b/i,
    service: 'sendgrid',
    serviceName: 'SendGrid',
    action: 'sendgrid',
    actionLabel: 'SendGrid action',
    confidence: 'low',
  },

  // Google Calendar
  {
    pattern: /\b(schedule|calendar|meeting|event)\b/i,
    service: 'google-calendar',
    serviceName: 'Google Calendar',
    action: 'calendar',
    actionLabel: 'Calendar action',
    confidence: 'medium',
  },

  // Google Drive
  {
    pattern: /\b(upload|share|drive|folder)\b.*\b(file|document|drive)\b/i,
    service: 'google-drive',
    serviceName: 'Google Drive',
    action: 'drive',
    actionLabel: 'Google Drive action',
    confidence: 'medium',
  },
];

// ── Discovery Engine (Pure) ────────────────────────────────────────────

/**
 * Match a user message to available integrations. Pure function.
 *
 * @param message     The raw user message text
 * @param connectedIds Set of service IDs that are currently connected
 * @returns DiscoveryResult with all matches ranked by confidence + connected status
 */
export function discoverIntegrations(message: string, connectedIds: Set<string>): DiscoveryResult {
  const seen = new Set<string>();
  const matches: IntentMatch[] = [];

  for (const p of INTENT_PATTERNS) {
    if (!p.pattern.test(message)) continue;
    // Dedupe by service — keep highest confidence
    const key = `${p.service}:${p.action}`;
    if (seen.has(key)) continue;
    seen.add(key);

    matches.push({
      service: p.service,
      serviceName: p.serviceName,
      confidence: p.confidence,
      action: p.action,
      actionLabel: p.actionLabel,
      connected: connectedIds.has(p.service),
    });
  }

  // Sort: high confidence first, then connected first, then medium, then low
  matches.sort((a, b) => {
    const confOrder = { high: 0, medium: 1, low: 2 };
    const aDelta = confOrder[a.confidence] - confOrder[b.confidence];
    if (aDelta !== 0) return aDelta;
    if (a.connected !== b.connected) return a.connected ? -1 : 1;
    return 0;
  });

  const hasConnectedMatch = matches.some((m) => m.connected);
  const hasDisconnectedMatch = matches.some((m) => !m.connected);
  const bestMatch = matches[0] ?? null;

  const systemHint = buildSystemHint(matches, connectedIds);

  return { matches, hasConnectedMatch, hasDisconnectedMatch, bestMatch, systemHint };
}

/**
 * Build a system prompt hint to inject into the agent context,
 * informing it about available integrations for the user's request.
 */
export function buildSystemHint(matches: IntentMatch[], _connectedIds: Set<string>): string | null {
  if (!matches.length) return null;

  const connected = matches.filter((m) => m.connected);
  const disconnected = matches.filter((m) => !m.connected);

  const parts: string[] = [];

  if (connected.length) {
    parts.push(
      `[Integration Context] The following services are connected and available for this request: ${connected.map((m) => `${m.serviceName} (${m.actionLabel})`).join(', ')}. Use these integrations to fulfill the user's request when appropriate.`,
    );
  }

  if (disconnected.length && !connected.length) {
    parts.push(
      `[Integration Context] The user's request could benefit from these integrations which are NOT yet connected: ${disconnected.map((m) => m.serviceName).join(', ')}. You may suggest connecting them via Settings → Integrations.`,
    );
  }

  return parts.length ? parts.join(' ') : null;
}

/**
 * Quick check: does this message look like it might involve an integration?
 * Fast pre-filter to avoid expensive matching on every message.
 */
export function mightNeedIntegration(message: string): boolean {
  // Quick keyword check — covers ~95% of integration-relevant messages
  return /\b(email|mail|slack|github|discord|telegram|trello|jira|linear|notion|sheet|hubspot|salesforce|stripe|shopify|zendesk|twilio|sendgrid|calendar|drive|send|post|create|schedule|check|read|inbox|sms|text|invoice|ticket|issue|card|deal|contact|pipeline|pr|pull\s*request|repo|channel)\b/i.test(
    message,
  );
}

/**
 * Count how many integrations could serve this message.
 */
export function countPotentialServices(message: string): number {
  const services = new Set<string>();
  for (const p of INTENT_PATTERNS) {
    if (p.pattern.test(message)) services.add(p.service);
  }
  return services.size;
}
