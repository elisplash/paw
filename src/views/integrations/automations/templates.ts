// src/views/integrations/automations/templates.ts — Pre-built template catalog
//
// Atom-level: pure data, no DOM, no IPC.

import type { AutomationTemplate } from './atoms';

// ── Helper ─────────────────────────────────────────────────────────────

function tpl(
  id: string,
  name: string,
  description: string,
  category: AutomationTemplate['category'],
  trigger: AutomationTemplate['trigger'],
  steps: AutomationTemplate['steps'],
  requiredServices: string[],
  tags: string[],
  estimatedSetup = '30 seconds',
): AutomationTemplate {
  return {
    id,
    name,
    description,
    category,
    trigger,
    steps,
    requiredServices,
    tags,
    estimatedSetup,
  };
}

// ── Template Catalog ───────────────────────────────────────────────────

export const TEMPLATE_CATALOG: AutomationTemplate[] = [
  // ── Slack ────────────────────────────────────────────────────────────
  tpl(
    'slack-daily-digest',
    'Daily Channel Digest',
    'Every morning, summarize top messages from key channels',
    'reporting',
    { type: 'schedule', label: 'Daily at 9 AM', cron: '0 9 * * *' },
    [
      { serviceId: 'slack', action: 'Read messages from channels', icon: 'chat' },
      { serviceId: 'slack', action: 'Post digest summary', icon: 'summarize' },
    ],
    ['slack'],
    ['popular', 'communication'],
  ),

  // ── GitHub ───────────────────────────────────────────────────────────
  tpl(
    'github-issue-to-slack',
    'Issue Alert → Slack',
    'When a new GitHub issue is created, post to a Slack channel',
    'alerts',
    { type: 'event', label: 'On new issue', eventSource: 'github' },
    [
      { serviceId: 'github', action: 'Detect new issue', icon: 'bug_report' },
      { serviceId: 'slack', action: 'Post issue details', icon: 'chat' },
    ],
    ['github', 'slack'],
    ['popular', 'devops'],
  ),
  tpl(
    'github-pr-review-reminder',
    'PR Review Reminder',
    'Daily reminder for pull requests awaiting review',
    'devops',
    { type: 'schedule', label: 'Daily at 10 AM', cron: '0 10 * * *' },
    [
      { serviceId: 'github', action: 'List open PRs needing review', icon: 'rate_review' },
      { serviceId: 'slack', action: 'Post review queue', icon: 'chat' },
    ],
    ['github', 'slack'],
    ['devops'],
  ),
  tpl(
    'github-deploy-notify',
    'Deploy Notification',
    'Notify Slack when a release is published on GitHub',
    'devops',
    { type: 'event', label: 'On new release', eventSource: 'github' },
    [
      { serviceId: 'github', action: 'Detect new release', icon: 'rocket_launch' },
      { serviceId: 'slack', action: 'Post release announcement', icon: 'campaign' },
    ],
    ['github', 'slack'],
    ['devops'],
  ),

  // ── HubSpot ──────────────────────────────────────────────────────────
  tpl(
    'hubspot-deal-alert',
    'New Deal Alert',
    'When a deal is created in HubSpot, post to Slack',
    'alerts',
    { type: 'event', label: 'On new deal', eventSource: 'hubspot' },
    [
      { serviceId: 'hubspot', action: 'Detect new deal', icon: 'handshake' },
      { serviceId: 'slack', action: 'Post deal details to #sales', icon: 'chat' },
    ],
    ['hubspot', 'slack'],
    ['popular', 'sales'],
  ),
  tpl(
    'hubspot-daily-crm-summary',
    'Daily CRM Summary',
    'Every morning, get deal pipeline and contact stats',
    'reporting',
    { type: 'schedule', label: 'Daily at 8 AM', cron: '0 8 * * *' },
    [
      { serviceId: 'hubspot', action: 'Get deal & contact stats', icon: 'analytics' },
      { serviceId: 'slack', action: 'Post CRM summary', icon: 'chat' },
    ],
    ['hubspot', 'slack'],
    ['popular', 'sales', 'reporting'],
  ),
  tpl(
    'hubspot-lead-to-trello',
    'Lead → Trello Card',
    'When a new contact is created, create a Trello card',
    'sync',
    { type: 'event', label: 'On new contact', eventSource: 'hubspot' },
    [
      { serviceId: 'hubspot', action: 'Detect new contact', icon: 'person_add' },
      { serviceId: 'trello', action: 'Create card in Leads board', icon: 'add_card' },
    ],
    ['hubspot', 'trello'],
    ['sales', 'sync'],
  ),

  // ── Jira ─────────────────────────────────────────────────────────────
  tpl(
    'jira-ticket-to-slack',
    'Ticket Alert → Slack',
    'When a Jira ticket is created, notify Slack',
    'alerts',
    { type: 'event', label: 'On new ticket', eventSource: 'jira' },
    [
      { serviceId: 'jira', action: 'Detect new ticket', icon: 'confirmation_number' },
      { serviceId: 'slack', action: 'Post ticket details', icon: 'chat' },
    ],
    ['jira', 'slack'],
    ['popular', 'devops'],
  ),
  tpl(
    'jira-sprint-report',
    'Sprint Progress Report',
    'Weekly sprint velocity and burndown summary',
    'reporting',
    { type: 'schedule', label: 'Every Friday at 4 PM', cron: '0 16 * * 5' },
    [
      { serviceId: 'jira', action: 'Get sprint data', icon: 'sprint' },
      { serviceId: 'slack', action: 'Post sprint report', icon: 'bar_chart' },
    ],
    ['jira', 'slack'],
    ['devops', 'reporting'],
  ),

  // ── Stripe ───────────────────────────────────────────────────────────
  tpl(
    'stripe-payment-alert',
    'Payment Received Alert',
    'When a payment succeeds, notify Slack with amount and customer',
    'alerts',
    { type: 'event', label: 'On successful payment', eventSource: 'stripe' },
    [
      { serviceId: 'stripe', action: 'Detect payment', icon: 'payments' },
      { serviceId: 'slack', action: 'Post payment notification', icon: 'chat' },
    ],
    ['stripe', 'slack'],
    ['popular', 'sales'],
  ),
  tpl(
    'stripe-daily-revenue',
    'Daily Revenue Report',
    "Every evening, summarize the day's revenue",
    'reporting',
    { type: 'schedule', label: 'Daily at 6 PM', cron: '0 18 * * *' },
    [
      { serviceId: 'stripe', action: "Get today's transactions", icon: 'attach_money' },
      { serviceId: 'slack', action: 'Post revenue summary', icon: 'bar_chart' },
    ],
    ['stripe', 'slack'],
    ['sales', 'reporting'],
  ),
  tpl(
    'stripe-failed-payment',
    'Failed Payment Alert',
    'When a payment fails, alert the team immediately',
    'alerts',
    { type: 'event', label: 'On failed payment', eventSource: 'stripe' },
    [
      { serviceId: 'stripe', action: 'Detect failed charge', icon: 'credit_card_off' },
      { serviceId: 'slack', action: 'Alert #billing channel', icon: 'warning' },
    ],
    ['stripe', 'slack'],
    ['sales'],
  ),

  // ── Trello ───────────────────────────────────────────────────────────
  tpl(
    'trello-stale-card-alert',
    'Stale Card Reminder',
    'Daily check for cards with no activity in 7+ days',
    'productivity',
    { type: 'schedule', label: 'Daily at 10 AM', cron: '0 10 * * *' },
    [
      { serviceId: 'trello', action: 'Find stale cards', icon: 'hourglass_bottom' },
      { serviceId: 'slack', action: 'Post stale card list', icon: 'chat' },
    ],
    ['trello', 'slack'],
    ['productivity'],
  ),
  tpl(
    'trello-card-to-jira',
    'Trello Card → Jira Ticket',
    'When a card is created in a specific list, create a Jira ticket',
    'sync',
    { type: 'event', label: 'On card created', eventSource: 'trello' },
    [
      { serviceId: 'trello', action: 'Detect new card', icon: 'add_card' },
      { serviceId: 'jira', action: 'Create Jira ticket', icon: 'confirmation_number' },
    ],
    ['trello', 'jira'],
    ['sync', 'devops'],
  ),

  // ── Linear ───────────────────────────────────────────────────────────
  tpl(
    'linear-issue-to-slack',
    'Linear Issue Alert',
    'When a new Linear issue is created, notify Slack',
    'alerts',
    { type: 'event', label: 'On new issue', eventSource: 'linear' },
    [
      { serviceId: 'linear', action: 'Detect new issue', icon: 'bug_report' },
      { serviceId: 'slack', action: 'Post issue details', icon: 'chat' },
    ],
    ['linear', 'slack'],
    ['devops'],
  ),

  // ── Notion ───────────────────────────────────────────────────────────
  tpl(
    'notion-meeting-notes',
    'Meeting Notes → Slack',
    'When a new page is added to Meeting Notes, share to Slack',
    'productivity',
    { type: 'event', label: 'On new page', eventSource: 'notion' },
    [
      { serviceId: 'notion', action: 'Detect new meeting note', icon: 'description' },
      { serviceId: 'slack', action: 'Share note summary', icon: 'chat' },
    ],
    ['notion', 'slack'],
    ['productivity'],
  ),

  // ── Discord ──────────────────────────────────────────────────────────
  tpl(
    'discord-github-feed',
    'GitHub Feed → Discord',
    'Post new issues and PRs to a Discord channel',
    'devops',
    { type: 'event', label: 'On GitHub activity', eventSource: 'github' },
    [
      { serviceId: 'github', action: 'Detect issue/PR', icon: 'code' },
      { serviceId: 'discord', action: 'Post to channel', icon: 'forum' },
    ],
    ['github', 'discord'],
    ['devops'],
  ),

  // ── Telegram ─────────────────────────────────────────────────────────
  tpl(
    'telegram-server-alert',
    'Server Alert → Telegram',
    'Send critical alerts to a Telegram group',
    'alerts',
    { type: 'webhook', label: 'On incoming webhook' },
    [{ serviceId: 'telegram', action: 'Send alert message', icon: 'send' }],
    ['telegram'],
    ['devops', 'alerts'],
  ),

  // ── Google Sheets ────────────────────────────────────────────────────
  tpl(
    'sheets-hubspot-sync',
    'HubSpot → Google Sheets',
    'Every hour, sync new HubSpot contacts to a spreadsheet',
    'sync',
    { type: 'schedule', label: 'Every hour', cron: '0 * * * *' },
    [
      { serviceId: 'hubspot', action: 'Get new contacts', icon: 'people' },
      { serviceId: 'google-sheets', action: 'Append rows', icon: 'table_chart' },
    ],
    ['hubspot', 'google-sheets'],
    ['sync', 'sales'],
  ),
  tpl(
    'sheets-stripe-log',
    'Stripe Payments → Sheets',
    'Log every Stripe payment to a Google Sheet',
    'sync',
    { type: 'event', label: 'On payment', eventSource: 'stripe' },
    [
      { serviceId: 'stripe', action: 'Detect payment', icon: 'payments' },
      { serviceId: 'google-sheets', action: 'Append payment row', icon: 'table_chart' },
    ],
    ['stripe', 'google-sheets'],
    ['sync', 'sales'],
  ),

  // ── Asana ────────────────────────────────────────────────────────────
  tpl(
    'asana-slack-complete',
    'Task Complete → Slack',
    'When an Asana task is completed, celebrate in Slack',
    'alerts',
    { type: 'event', label: 'On task completed', eventSource: 'asana' },
    [
      { serviceId: 'asana', action: 'Detect completed task', icon: 'task_alt' },
      { serviceId: 'slack', action: 'Post completion', icon: 'celebration' },
    ],
    ['asana', 'slack'],
    ['productivity'],
  ),

  // ── Monday.com ───────────────────────────────────────────────────────
  tpl(
    'monday-status-slack',
    'Status Change → Slack',
    'When a Monday.com item status changes, notify Slack',
    'alerts',
    { type: 'event', label: 'On status change', eventSource: 'monday' },
    [
      { serviceId: 'monday', action: 'Detect status change', icon: 'swap_horiz' },
      { serviceId: 'slack', action: 'Post update', icon: 'chat' },
    ],
    ['monday', 'slack'],
    ['productivity'],
  ),

  // ── Shopify ──────────────────────────────────────────────────────────
  tpl(
    'shopify-order-alert',
    'New Order Alert',
    'When a new Shopify order comes in, notify the team',
    'alerts',
    { type: 'event', label: 'On new order', eventSource: 'shopify' },
    [
      { serviceId: 'shopify', action: 'Detect new order', icon: 'shopping_cart' },
      { serviceId: 'slack', action: 'Post order details', icon: 'chat' },
    ],
    ['shopify', 'slack'],
    ['popular', 'sales'],
  ),
  tpl(
    'shopify-daily-sales',
    'Daily Sales Report',
    'Every evening, summarize daily orders and revenue',
    'reporting',
    { type: 'schedule', label: 'Daily at 7 PM', cron: '0 19 * * *' },
    [
      { serviceId: 'shopify', action: "Get today's orders", icon: 'receipt_long' },
      { serviceId: 'slack', action: 'Post sales summary', icon: 'bar_chart' },
    ],
    ['shopify', 'slack'],
    ['sales', 'reporting'],
  ),
  tpl(
    'shopify-low-stock',
    'Low Stock Alert',
    'Daily check for products below stock threshold',
    'alerts',
    { type: 'schedule', label: 'Daily at 8 AM', cron: '0 8 * * *' },
    [
      { serviceId: 'shopify', action: 'Check inventory levels', icon: 'inventory' },
      { serviceId: 'slack', action: 'Alert low-stock items', icon: 'warning' },
    ],
    ['shopify', 'slack'],
    ['sales'],
  ),

  // ── Zendesk ──────────────────────────────────────────────────────────
  tpl(
    'zendesk-urgent-ticket',
    'Urgent Ticket Alert',
    'When an urgent ticket is created, alert Slack immediately',
    'support',
    { type: 'event', label: 'On urgent ticket', eventSource: 'zendesk' },
    [
      { serviceId: 'zendesk', action: 'Detect urgent ticket', icon: 'priority_high' },
      { serviceId: 'slack', action: 'Alert #support', icon: 'warning' },
    ],
    ['zendesk', 'slack'],
    ['support'],
  ),
  tpl(
    'zendesk-daily-stats',
    'Support Stats Report',
    'Daily ticket volume, resolution time, and satisfaction',
    'reporting',
    { type: 'schedule', label: 'Daily at 5 PM', cron: '0 17 * * *' },
    [
      { serviceId: 'zendesk', action: 'Get ticket stats', icon: 'analytics' },
      { serviceId: 'slack', action: 'Post support report', icon: 'bar_chart' },
    ],
    ['zendesk', 'slack'],
    ['support', 'reporting'],
  ),

  // ── Salesforce ───────────────────────────────────────────────────────
  tpl(
    'salesforce-deal-won',
    'Deal Won → Celebration',
    'When a Salesforce opportunity is won, celebrate in Slack',
    'alerts',
    { type: 'event', label: 'On deal won', eventSource: 'salesforce' },
    [
      { serviceId: 'salesforce', action: 'Detect closed-won opp', icon: 'emoji_events' },
      { serviceId: 'slack', action: 'Post celebration to #wins', icon: 'celebration' },
    ],
    ['salesforce', 'slack'],
    ['popular', 'sales'],
  ),
  tpl(
    'salesforce-weekly-pipeline',
    'Weekly Pipeline Report',
    'Every Monday, summarize the sales pipeline',
    'reporting',
    { type: 'schedule', label: 'Every Monday at 9 AM', cron: '0 9 * * 1' },
    [
      { serviceId: 'salesforce', action: 'Get pipeline data', icon: 'waterfall_chart' },
      { serviceId: 'slack', action: 'Post pipeline summary', icon: 'bar_chart' },
    ],
    ['salesforce', 'slack'],
    ['sales', 'reporting'],
  ),

  // ── Multi-service ────────────────────────────────────────────────────
  tpl(
    'weekly-business-review',
    'Weekly Business Review',
    'Every Friday: deals closed + sprint progress + Slack highlights',
    'reporting',
    { type: 'schedule', label: 'Every Friday at 5 PM', cron: '0 17 * * 5' },
    [
      { serviceId: 'hubspot', action: 'Get weekly deal stats', icon: 'handshake' },
      { serviceId: 'trello', action: 'Get sprint progress', icon: 'view_kanban' },
      { serviceId: 'slack', action: 'Post weekly review', icon: 'summarize' },
    ],
    ['hubspot', 'trello', 'slack'],
    ['popular', 'reporting'],
  ),
  tpl(
    'customer-onboarding',
    'Customer Onboarding Flow',
    'New HubSpot contact → Trello card + welcome Slack + Gmail intro',
    'onboarding',
    { type: 'event', label: 'On new customer', eventSource: 'hubspot' },
    [
      { serviceId: 'hubspot', action: 'Detect new customer', icon: 'person_add' },
      { serviceId: 'trello', action: 'Create onboarding card', icon: 'add_card' },
      { serviceId: 'slack', action: 'Notify #customer-success', icon: 'chat' },
      { serviceId: 'gmail', action: 'Send welcome email', icon: 'mail' },
    ],
    ['hubspot', 'trello', 'slack', 'gmail'],
    ['onboarding'],
  ),
  tpl(
    'full-stack-deploy',
    'Full-Stack Deploy Pipeline',
    'GitHub release → Slack notification + Jira status update',
    'devops',
    { type: 'event', label: 'On release published', eventSource: 'github' },
    [
      { serviceId: 'github', action: 'Detect release', icon: 'rocket_launch' },
      { serviceId: 'jira', action: 'Move tickets to Done', icon: 'done_all' },
      { serviceId: 'slack', action: 'Announce release', icon: 'campaign' },
    ],
    ['github', 'jira', 'slack'],
    ['devops'],
  ),
  tpl(
    'daily-standup-prep',
    'Daily Standup Prep',
    "Every morning: yesterday's completed tasks + today's priorities",
    'productivity',
    { type: 'schedule', label: 'Daily at 8:30 AM', cron: '30 8 * * 1-5' },
    [
      { serviceId: 'jira', action: "Get yesterday's completed", icon: 'done' },
      { serviceId: 'jira', action: "Get today's priorities", icon: 'priority_high' },
      { serviceId: 'slack', action: 'Post standup prep', icon: 'groups' },
    ],
    ['jira', 'slack'],
    ['popular', 'productivity'],
  ),

  // ── SendGrid ─────────────────────────────────────────────────────────
  tpl(
    'sendgrid-bounce-alert',
    'Bounce Alert',
    'When bounce rate spikes, alert the marketing team',
    'marketing',
    { type: 'schedule', label: 'Every 4 hours', cron: '0 */4 * * *' },
    [
      { serviceId: 'sendgrid', action: 'Check bounce rate', icon: 'error' },
      { serviceId: 'slack', action: 'Alert if threshold exceeded', icon: 'warning' },
    ],
    ['sendgrid', 'slack'],
    ['marketing'],
  ),

  // ── Twilio ───────────────────────────────────────────────────────────
  tpl(
    'twilio-missed-call-slack',
    'Missed Call → Slack',
    'When a call is missed, post caller info to Slack',
    'alerts',
    { type: 'event', label: 'On missed call', eventSource: 'twilio' },
    [
      { serviceId: 'twilio', action: 'Detect missed call', icon: 'phone_missed' },
      { serviceId: 'slack', action: 'Post caller details', icon: 'chat' },
    ],
    ['twilio', 'slack'],
    ['communication'],
  ),

  // ── ClickUp ──────────────────────────────────────────────────────────
  tpl(
    'clickup-weekly-velocity',
    'Weekly Velocity Report',
    "Every Monday, show last week's task completion velocity",
    'reporting',
    { type: 'schedule', label: 'Every Monday at 9 AM', cron: '0 9 * * 1' },
    [
      { serviceId: 'clickup', action: 'Get completed tasks', icon: 'speed' },
      { serviceId: 'slack', action: 'Post velocity report', icon: 'bar_chart' },
    ],
    ['clickup', 'slack'],
    ['productivity', 'reporting'],
  ),

  // ── Airtable ─────────────────────────────────────────────────────────
  tpl(
    'airtable-new-record-slack',
    'New Record → Slack',
    'When a record is added to an Airtable base, notify Slack',
    'alerts',
    { type: 'event', label: 'On new record', eventSource: 'airtable' },
    [
      { serviceId: 'airtable', action: 'Detect new record', icon: 'playlist_add' },
      { serviceId: 'slack', action: 'Post record details', icon: 'chat' },
    ],
    ['airtable', 'slack'],
    ['productivity'],
  ),

  // ── Todoist ──────────────────────────────────────────────────────────
  tpl(
    'todoist-overdue-reminder',
    'Overdue Task Reminder',
    'Every morning, check for overdue tasks and remind via Slack',
    'productivity',
    { type: 'schedule', label: 'Daily at 9 AM', cron: '0 9 * * *' },
    [
      { serviceId: 'todoist', action: 'Find overdue tasks', icon: 'alarm' },
      { serviceId: 'slack', action: 'Post overdue list', icon: 'chat' },
    ],
    ['todoist', 'slack'],
    ['productivity'],
  ),
];

// ── Helpers ────────────────────────────────────────────────────────────

/** Get templates relevant to a specific service. */
export function getTemplatesForService(serviceId: string): AutomationTemplate[] {
  return TEMPLATE_CATALOG.filter((t) => t.requiredServices.includes(serviceId));
}

/** Get the most popular templates. */
export function getPopularTemplates(limit = 8): AutomationTemplate[] {
  return TEMPLATE_CATALOG.filter((t) => t.tags.includes('popular')).slice(0, limit);
}
