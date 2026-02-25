// src/engine/atoms/tool-remap.ts — Service-native tool name remapping
//
// Atom-level: pure functions, no DOM, no IPC.
// Remaps raw n8n/MCP tool names to service-native names with rich descriptions.

// ── Types ──────────────────────────────────────────────────────────────

export interface RemappedTool {
  /** Service-native name, e.g. "slack_send_message" */
  name: string;
  /** Original raw tool name from MCP/n8n */
  originalName: string;
  /** Human-readable description for the LLM */
  description: string;
  /** Service identifier, e.g. "slack" */
  service: string;
  /** Service display name, e.g. "Slack" */
  serviceName: string;
  /** Action verb, e.g. "send_message" */
  action: string;
  /** JSON Schema for parameters */
  parameters?: Record<string, unknown>;
  /** Source attribution */
  source: string;
}

export interface ServiceToolSet {
  service: string;
  serviceName: string;
  tools: RemappedTool[];
  connected: boolean;
}

export interface AgentToolAssignment {
  agentId: string;
  /** Per-service tool access: service id → enabled tool names (or '*' for all) */
  services: Record<string, string[]>;
}

// ── N8n → service-native remap rules ───────────────────────────────────

interface RemapRule {
  /** Regex pattern matching the raw n8n/MCP tool name */
  pattern: RegExp;
  /** Service identifier */
  service: string;
  /** Service display name */
  serviceName: string;
  /** Function to derive the action name */
  action: (match: RegExpMatchArray) => string;
  /** LLM-optimized description template */
  description: (match: RegExpMatchArray) => string;
}

const REMAP_RULES: RemapRule[] = [
  // Slack
  { pattern: /^(?:n8n_?)?(?:send)?slack(?:_?(?:post|send))?_?message$/i, service: 'slack', serviceName: 'Slack', action: () => 'send_message', description: () => 'Send a message to a Slack channel or DM a user. Use this when the user asks you to post, share, or send something to Slack.' },
  { pattern: /^(?:n8n_?)?slack_?list_?channels$/i, service: 'slack', serviceName: 'Slack', action: () => 'list_channels', description: () => 'List all channels in the Slack workspace.' },
  { pattern: /^(?:n8n_?)?slack_?(?:get|read)_?(?:channel|messages?)$/i, service: 'slack', serviceName: 'Slack', action: () => 'read_channel', description: () => 'Read messages from a Slack channel.' },
  { pattern: /^(?:n8n_?)?slack_?list_?users$/i, service: 'slack', serviceName: 'Slack', action: () => 'list_users', description: () => 'List all users in the Slack workspace.' },
  { pattern: /^(?:n8n_?)?slack_?search$/i, service: 'slack', serviceName: 'Slack', action: () => 'search', description: () => 'Search messages, files, and channels in Slack.' },

  // GitHub
  { pattern: /^(?:n8n_?)?github_?create_?issue$/i, service: 'github', serviceName: 'GitHub', action: () => 'create_issue', description: () => 'Create a new issue in a GitHub repository.' },
  { pattern: /^(?:n8n_?)?github_?list_?issues$/i, service: 'github', serviceName: 'GitHub', action: () => 'list_issues', description: () => 'List issues in a GitHub repository. Supports filtering by state, labels, and assignee.' },
  { pattern: /^(?:n8n_?)?github_?create_?(?:pr|pull_?request)$/i, service: 'github', serviceName: 'GitHub', action: () => 'create_pr', description: () => 'Create a new pull request in a GitHub repository.' },
  { pattern: /^(?:n8n_?)?github_?list_?(?:repos|repositories)$/i, service: 'github', serviceName: 'GitHub', action: () => 'list_repos', description: () => 'List repositories the authenticated user has access to.' },
  { pattern: /^(?:n8n_?)?github_?search(?:_?code)?$/i, service: 'github', serviceName: 'GitHub', action: () => 'search_code', description: () => 'Search code across GitHub repositories.' },
  { pattern: /^(?:n8n_?)?github_?(?:add_?)?comment$/i, service: 'github', serviceName: 'GitHub', action: () => 'comment', description: () => 'Add a comment to a GitHub issue or pull request.' },

  // Gmail
  { pattern: /^(?:n8n_?)?gmail_?send(?:_?email)?$/i, service: 'gmail', serviceName: 'Gmail', action: () => 'send_email', description: () => 'Send an email via Gmail. Use this when the user asks you to email someone.' },
  { pattern: /^(?:n8n_?)?gmail_?(?:search|list|read)(?:_?(?:inbox|emails?))?$/i, service: 'gmail', serviceName: 'Gmail', action: () => 'search_inbox', description: () => 'Search or list emails in Gmail. Supports query filters.' },
  { pattern: /^(?:n8n_?)?gmail_?draft$/i, service: 'gmail', serviceName: 'Gmail', action: () => 'create_draft', description: () => 'Create a draft email in Gmail.' },

  // HubSpot
  { pattern: /^(?:n8n_?)?hubspot_?(?:list|get)_?deals$/i, service: 'hubspot', serviceName: 'HubSpot', action: () => 'list_deals', description: () => 'List deals from HubSpot CRM. Use this for sales pipeline queries.' },
  { pattern: /^(?:n8n_?)?hubspot_?create_?deal$/i, service: 'hubspot', serviceName: 'HubSpot', action: () => 'create_deal', description: () => 'Create a new deal in HubSpot CRM.' },
  { pattern: /^(?:n8n_?)?hubspot_?(?:list|get)_?contacts$/i, service: 'hubspot', serviceName: 'HubSpot', action: () => 'list_contacts', description: () => 'List contacts from HubSpot CRM.' },
  { pattern: /^(?:n8n_?)?hubspot_?create_?contact$/i, service: 'hubspot', serviceName: 'HubSpot', action: () => 'create_contact', description: () => 'Create a new contact in HubSpot CRM.' },

  // Jira
  { pattern: /^(?:n8n_?)?jira_?create_?issue$/i, service: 'jira', serviceName: 'Jira', action: () => 'create_issue', description: () => 'Create a new Jira issue/ticket.' },
  { pattern: /^(?:n8n_?)?jira_?(?:search|list|query)(?:_?issues)?$/i, service: 'jira', serviceName: 'Jira', action: () => 'search_issues', description: () => 'Search Jira issues using JQL or text query.' },
  { pattern: /^(?:n8n_?)?jira_?(?:transition|update_?status)$/i, service: 'jira', serviceName: 'Jira', action: () => 'transition', description: () => 'Move a Jira issue to a different status.' },

  // Trello
  { pattern: /^(?:n8n_?)?trello_?create_?card$/i, service: 'trello', serviceName: 'Trello', action: () => 'create_card', description: () => 'Create a new card on a Trello board.' },
  { pattern: /^(?:n8n_?)?trello_?(?:list|get)_?cards$/i, service: 'trello', serviceName: 'Trello', action: () => 'list_cards', description: () => 'List cards from a Trello board or list.' },
  { pattern: /^(?:n8n_?)?trello_?move(?:_?card)?$/i, service: 'trello', serviceName: 'Trello', action: () => 'move_card', description: () => 'Move a Trello card to a different list.' },

  // Notion
  { pattern: /^(?:n8n_?)?notion_?create_?page$/i, service: 'notion', serviceName: 'Notion', action: () => 'create_page', description: () => 'Create a new page in a Notion database or workspace.' },
  { pattern: /^(?:n8n_?)?notion_?(?:search|query)$/i, service: 'notion', serviceName: 'Notion', action: () => 'search', description: () => 'Search pages and databases in Notion.' },

  // Google Sheets
  { pattern: /^(?:n8n_?)?(?:google_?)?sheets?_?(?:read|get)$/i, service: 'google-sheets', serviceName: 'Google Sheets', action: () => 'read', description: () => 'Read data from a Google Sheets spreadsheet.' },
  { pattern: /^(?:n8n_?)?(?:google_?)?sheets?_?(?:write|update|append)$/i, service: 'google-sheets', serviceName: 'Google Sheets', action: () => 'write', description: () => 'Write or append data to a Google Sheets spreadsheet.' },

  // Shopify
  { pattern: /^(?:n8n_?)?shopify_?(?:list|get)_?orders$/i, service: 'shopify', serviceName: 'Shopify', action: () => 'list_orders', description: () => 'List orders from your Shopify store.' },
  { pattern: /^(?:n8n_?)?shopify_?(?:list|get)_?products$/i, service: 'shopify', serviceName: 'Shopify', action: () => 'list_products', description: () => 'List products from your Shopify store.' },

  // Stripe
  { pattern: /^(?:n8n_?)?stripe_?(?:list|get)_?(?:charges|payments)$/i, service: 'stripe', serviceName: 'Stripe', action: () => 'list_payments', description: () => 'List recent payments/charges from Stripe.' },
  { pattern: /^(?:n8n_?)?stripe_?(?:list|get)_?customers$/i, service: 'stripe', serviceName: 'Stripe', action: () => 'list_customers', description: () => 'List customers from Stripe.' },

  // Salesforce
  { pattern: /^(?:n8n_?)?salesforce_?query$/i, service: 'salesforce', serviceName: 'Salesforce', action: () => 'query', description: () => 'Execute a SOQL query against Salesforce.' },
  { pattern: /^(?:n8n_?)?salesforce_?create$/i, service: 'salesforce', serviceName: 'Salesforce', action: () => 'create_record', description: () => 'Create a new record in Salesforce.' },

  // SendGrid
  { pattern: /^(?:n8n_?)?sendgrid_?send$/i, service: 'sendgrid', serviceName: 'SendGrid', action: () => 'send_email', description: () => 'Send an email via SendGrid.' },

  // Twilio
  { pattern: /^(?:n8n_?)?twilio_?send(?:_?(?:sms|message))?$/i, service: 'twilio', serviceName: 'Twilio', action: () => 'send_sms', description: () => 'Send an SMS message via Twilio.' },

  // Zendesk
  { pattern: /^(?:n8n_?)?zendesk_?create_?ticket$/i, service: 'zendesk', serviceName: 'Zendesk', action: () => 'create_ticket', description: () => 'Create a new support ticket in Zendesk.' },
  { pattern: /^(?:n8n_?)?zendesk_?(?:list|get)_?tickets$/i, service: 'zendesk', serviceName: 'Zendesk', action: () => 'list_tickets', description: () => 'List support tickets from Zendesk.' },

  // Telegram
  { pattern: /^(?:n8n_?)?telegram_?send(?:_?message)?$/i, service: 'telegram', serviceName: 'Telegram', action: () => 'send_message', description: () => 'Send a message via Telegram.' },

  // Discord
  { pattern: /^(?:n8n_?)?discord_?send(?:_?message)?$/i, service: 'discord', serviceName: 'Discord', action: () => 'send_message', description: () => 'Send a message to a Discord channel.' },

  // Linear
  { pattern: /^(?:n8n_?)?linear_?create_?issue$/i, service: 'linear', serviceName: 'Linear', action: () => 'create_issue', description: () => 'Create a new issue in Linear.' },
  { pattern: /^(?:n8n_?)?linear_?(?:list|search)(?:_?issues)?$/i, service: 'linear', serviceName: 'Linear', action: () => 'list_issues', description: () => 'List or search issues in Linear.' },
];

// ── Remap functions ────────────────────────────────────────────────────

/**
 * Remap a raw MCP/n8n tool name to a service-native tool.
 * Returns null if no match is found (unknown tool).
 */
export function remapTool(rawName: string, params?: Record<string, unknown>): RemappedTool | null {
  for (const rule of REMAP_RULES) {
    const match = rawName.match(rule.pattern);
    if (match) {
      const action = rule.action(match);
      return {
        name: `${rule.service}_${action}`,
        originalName: rawName,
        description: rule.description(match),
        service: rule.service,
        serviceName: rule.serviceName,
        action,
        parameters: params,
        source: `${rule.serviceName} (via Integrations)`,
      };
    }
  }
  return null;
}

/**
 * Remap a raw tool name, with fallback for unrecognized tools.
 * Always returns a RemappedTool (with original name if no match).
 */
export function remapToolOrFallback(rawName: string, params?: Record<string, unknown>): RemappedTool {
  const mapped = remapTool(rawName, params);
  if (mapped) return mapped;

  // Detect service from prefix
  const prefixMatch = rawName.match(/^(?:n8n_?)?(\w+?)_/);
  const service = prefixMatch ? prefixMatch[1].toLowerCase() : 'unknown';
  const action = rawName.replace(/^(?:n8n_?)?(?:\w+_)?/, '').toLowerCase() || rawName;

  return {
    name: rawName,
    originalName: rawName,
    description: `Integration tool: ${rawName}`,
    service,
    serviceName: service.charAt(0).toUpperCase() + service.slice(1),
    action,
    parameters: params,
    source: 'Integrations',
  };
}

/**
 * Group remapped tools by service for the agent tool picker.
 */
export function groupToolsByService(
  tools: RemappedTool[],
  connectedServices: string[] = [],
): ServiceToolSet[] {
  const groups: Record<string, ServiceToolSet> = {};

  for (const tool of tools) {
    if (!groups[tool.service]) {
      groups[tool.service] = {
        service: tool.service,
        serviceName: tool.serviceName,
        tools: [],
        connected: connectedServices.includes(tool.service),
      };
    }
    groups[tool.service].tools.push(tool);
  }

  return Object.values(groups).sort((a, b) => {
    // Connected first, then alphabetical
    if (a.connected !== b.connected) return a.connected ? -1 : 1;
    return a.serviceName.localeCompare(b.serviceName);
  });
}

/**
 * Filter tools based on agent assignment.
 */
export function filterToolsForAgent(
  tools: RemappedTool[],
  assignment: AgentToolAssignment,
): RemappedTool[] {
  return tools.filter((tool) => {
    const allowed = assignment.services[tool.service];
    if (!allowed) return false;
    if (allowed.includes('*')) return true;
    return allowed.includes(tool.name) || allowed.includes(tool.action);
  });
}

/**
 * Build an LLM-friendly tool schema from a RemappedTool.
 */
export function buildToolSchema(tool: RemappedTool): Record<string, unknown> {
  return {
    name: tool.name,
    description: tool.description,
    parameters: tool.parameters ?? { type: 'object', properties: {} },
  };
}
