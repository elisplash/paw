// src/views/integrations/queries/catalog.ts — Curated query examples
//
// Data-only: no DOM, no IPC.

import type { ServiceQuery } from './atoms';

// ── Helper ─────────────────────────────────────────────────────────────

function q(
  id: string,
  question: string,
  serviceIds: string[],
  category: ServiceQuery['category'],
  icon: string,
  resultHint: string,
): ServiceQuery {
  return { id, question, serviceIds, category, icon, resultHint };
}

// ── Query Catalog ──────────────────────────────────────────────────────

export const QUERY_CATALOG: ServiceQuery[] = [
  // ── Sales / CRM ────────────────────────────────────────────────────
  q(
    'hubspot-deals-month',
    'How many deals did we close this month?',
    ['hubspot'],
    'sales',
    'payments',
    'Deal count, total value, top deal',
  ),
  q(
    'hubspot-contacts-week',
    'Show contacts added this week',
    ['hubspot'],
    'crm',
    'person_add',
    'New contact list with dates',
  ),
  q(
    'hubspot-deal-size',
    "What's the average deal size in Q1?",
    ['hubspot'],
    'sales',
    'analytics',
    'Average, min, max deal values',
  ),
  q(
    'hubspot-stale-deals',
    'Which deals have no activity in 14 days?',
    ['hubspot'],
    'sales',
    'warning',
    'Stale deals with last activity dates',
  ),
  q(
    'hubspot-pipeline',
    'Show me the current sales pipeline',
    ['hubspot'],
    'sales',
    'funnel_chart',
    'Pipeline stages with deal counts',
  ),
  q(
    'hubspot-companies',
    'List companies in California',
    ['hubspot'],
    'crm',
    'business',
    'Company list filtered by location',
  ),

  q(
    'salesforce-opps',
    'Show open opportunities over $50k',
    ['salesforce'],
    'sales',
    'trending_up',
    'Filtered opportunity list',
  ),
  q(
    'salesforce-leads',
    'How many leads this month?',
    ['salesforce'],
    'sales',
    'leaderboard',
    'Lead count by source',
  ),
  q(
    'salesforce-contacts',
    'List contacts from Acme Corp',
    ['salesforce'],
    'crm',
    'contacts',
    'Contact list with roles',
  ),

  // ── Project Management ──────────────────────────────────────────────
  q(
    'trello-in-progress',
    'What cards are in "In Progress"?',
    ['trello'],
    'projects',
    'view_kanban',
    'Cards with assignees and ages',
  ),
  q(
    'trello-overdue',
    'Show overdue Trello cards',
    ['trello'],
    'projects',
    'event_busy',
    'Overdue cards with due dates',
  ),
  q(
    'trello-boards',
    'List all my Trello boards',
    ['trello'],
    'projects',
    'dashboard',
    'Board names and card counts',
  ),
  q(
    'trello-stale',
    "Which cards haven't been updated in a week?",
    ['trello'],
    'projects',
    'history',
    'Stale card list with last activity',
  ),

  q(
    'jira-sprint',
    'How many open bugs in the current sprint?',
    ['jira'],
    'projects',
    'bug_report',
    'Bug count by priority',
  ),
  q(
    'jira-my-issues',
    'Show my assigned Jira issues',
    ['jira'],
    'projects',
    'assignment_ind',
    'Issue list with status',
  ),
  q(
    'jira-search',
    'Search for issues about "authentication"',
    ['jira'],
    'projects',
    'search',
    'Matching issues with summaries',
  ),

  q(
    'linear-cycle',
    'How many issues in the current cycle?',
    ['linear'],
    'projects',
    'timeline',
    'Issue breakdown by status',
  ),
  q(
    'linear-bugs',
    'Show bugs assigned to me in Linear',
    ['linear'],
    'projects',
    'bug_report',
    'Bug list with priorities',
  ),

  q(
    'asana-due-today',
    'What tasks are due today?',
    ['asana'],
    'projects',
    'today',
    'Tasks due today with assignees',
  ),
  q(
    'asana-overdue',
    'Show overdue Asana tasks',
    ['asana'],
    'projects',
    'event_busy',
    'Overdue tasks with original dates',
  ),

  q(
    'clickup-assigned',
    'What tasks are assigned to me?',
    ['clickup'],
    'projects',
    'assignment_ind',
    'Task list with statuses',
  ),
  q(
    'clickup-overdue',
    'Show overdue items in ClickUp',
    ['clickup'],
    'projects',
    'event_busy',
    'Overdue items with dates',
  ),

  q(
    'monday-due',
    'What items are due today on Monday?',
    ['monday'],
    'projects',
    'today',
    'Due items with board names',
  ),
  q(
    'monday-boards',
    'Show my Monday boards',
    ['monday'],
    'projects',
    'dashboard',
    'Board list with item counts',
  ),

  q(
    'todoist-today',
    'What tasks are due today in Todoist?',
    ['todoist'],
    'projects',
    'today',
    "Today's tasks with projects",
  ),
  q(
    'todoist-overdue',
    'Show overdue Todoist tasks',
    ['todoist'],
    'projects',
    'event_busy',
    'Overdue tasks list',
  ),

  // ── Communication ───────────────────────────────────────────────────
  q(
    'slack-unread',
    'How many unread Slack messages?',
    ['slack'],
    'communication',
    'mark_email_unread',
    'Unread counts by channel',
  ),
  q(
    'slack-mentions',
    'Show my Slack mentions today',
    ['slack'],
    'communication',
    'alternate_email',
    'Messages mentioning you',
  ),
  q(
    'slack-channels',
    'List all Slack channels',
    ['slack'],
    'communication',
    'tag',
    'Channel list with member counts',
  ),
  q(
    'slack-alice',
    'What did @alice say today?',
    ['slack'],
    'communication',
    'chat',
    'Messages from a specific user',
  ),

  q(
    'discord-unread',
    'Any new messages in #general?',
    ['discord'],
    'communication',
    'forum',
    'Unread message summary',
  ),
  q(
    'discord-online',
    'Who is online in the server?',
    ['discord'],
    'communication',
    'group',
    'Online member list',
  ),

  q(
    'telegram-messages',
    'Any new Telegram messages from my bot?',
    ['telegram'],
    'communication',
    'sms',
    'Recent bot messages',
  ),
  q(
    'telegram-members',
    'How many members in the group?',
    ['telegram'],
    'communication',
    'group',
    'Group member count',
  ),

  q(
    'teams-unread',
    'Any unread messages in Teams?',
    ['microsoft-teams'],
    'communication',
    'mark_email_unread',
    'Unread summary',
  ),
  q(
    'teams-channels',
    'List my Teams channels',
    ['microsoft-teams'],
    'communication',
    'tag',
    'Channel list',
  ),

  // ── Development ─────────────────────────────────────────────────────
  q(
    'github-open-prs',
    'How many open PRs in my repos?',
    ['github'],
    'development',
    'merge',
    'PR count by repo',
  ),
  q(
    'github-recent-commits',
    'Show recent commits on main',
    ['github'],
    'development',
    'commit',
    'Commit list with authors',
  ),
  q(
    'github-bugs',
    'List issues labeled "bug"',
    ['github'],
    'development',
    'bug_report',
    'Bug issues with assignees',
  ),
  q(
    'github-reviews',
    'Any PRs waiting for my review?',
    ['github'],
    'development',
    'rate_review',
    'PRs needing your review',
  ),

  // ── Data & Analytics ────────────────────────────────────────────────
  q(
    'gsheets-read',
    'Read the Sales summary sheet',
    ['google-sheets'],
    'analytics',
    'table_chart',
    'Sheet data in table format',
  ),
  q(
    'gsheets-rows',
    'How many rows in my tracker?',
    ['google-sheets'],
    'analytics',
    'tag',
    'Row count and column headers',
  ),

  q(
    'notion-search',
    'Search Notion for "meeting notes"',
    ['notion'],
    'analytics',
    'search',
    'Matching pages with snippets',
  ),
  q(
    'notion-databases',
    'List all Notion databases',
    ['notion'],
    'analytics',
    'database',
    'Database names and item counts',
  ),

  q(
    'airtable-records',
    'Show all records in my Projects base',
    ['airtable'],
    'analytics',
    'table_view',
    'Record list with fields',
  ),
  q(
    'airtable-rows',
    'How many rows in the tracker?',
    ['airtable'],
    'analytics',
    'tag',
    'Row count per table',
  ),

  // ── E-commerce & Support ────────────────────────────────────────────
  q(
    'shopify-orders',
    'How many orders today?',
    ['shopify'],
    'sales',
    'shopping_cart',
    'Order count and revenue',
  ),
  q(
    'shopify-stock',
    'Show low-stock products',
    ['shopify'],
    'sales',
    'inventory',
    'Products below threshold',
  ),
  q(
    'shopify-revenue',
    'Revenue this month?',
    ['shopify'],
    'sales',
    'payments',
    'Revenue total and comparison',
  ),

  q(
    'stripe-payments',
    'Show recent payments',
    ['stripe'],
    'sales',
    'receipt_long',
    'Payment list with amounts',
  ),
  q(
    'stripe-balance',
    "What's my Stripe balance?",
    ['stripe'],
    'sales',
    'account_balance',
    'Available and pending balance',
  ),

  q(
    'zendesk-open',
    'How many open support tickets?',
    ['zendesk'],
    'crm',
    'support_agent',
    'Open ticket count by priority',
  ),
  q(
    'zendesk-unassigned',
    'Show unassigned tickets',
    ['zendesk'],
    'crm',
    'person_off',
    'Unassigned ticket list',
  ),
  q(
    'zendesk-response-time',
    "What's the average response time?",
    ['zendesk'],
    'analytics',
    'timer',
    'Response time metrics',
  ),

  // ── Email ───────────────────────────────────────────────────────────
  q(
    'gmail-unread',
    'Any unread emails?',
    ['gmail'],
    'communication',
    'mark_email_unread',
    'Unread count by label',
  ),
  q(
    'gmail-today',
    'Show emails from today',
    ['gmail'],
    'communication',
    'inbox',
    "Today's email list",
  ),
  q(
    'gmail-meetings',
    'What meetings do I have tomorrow?',
    ['gmail'],
    'communication',
    'event',
    "Tomorrow's calendar events",
  ),

  q(
    'sendgrid-stats',
    'How many emails sent this week?',
    ['sendgrid'],
    'analytics',
    'send',
    'Send count and bounce rate',
  ),

  // ── Messaging ───────────────────────────────────────────────────────
  q(
    'twilio-sms',
    'How many SMS did I send today?',
    ['twilio'],
    'communication',
    'sms',
    'SMS count and delivery stats',
  ),
  q(
    'twilio-calls',
    'Show recent call logs',
    ['twilio'],
    'communication',
    'call',
    'Call list with durations',
  ),

  // ── Cross-service queries ───────────────────────────────────────────
  q(
    'x-deals-trello',
    "Which deals don't have a Trello card?",
    ['hubspot', 'trello'],
    'cross-service',
    'hub',
    'Untracked deals cross-reference',
  ),
  q(
    'x-team-slack',
    "Who hasn't posted in Slack this week?",
    ['slack'],
    'cross-service',
    'group_off',
    'Inactive team member list',
  ),
  q(
    'x-prs-trello',
    'Match GitHub PRs to Trello cards',
    ['github', 'trello'],
    'cross-service',
    'hub',
    'PR ↔ card mapping',
  ),
  q(
    'x-health-check',
    'Give me a full business health check',
    ['hubspot', 'trello', 'slack'],
    'cross-service',
    'monitoring',
    'Multi-service dashboard',
  ),
  q(
    'x-stale-all',
    'Show stale items across all connected services',
    ['hubspot', 'trello', 'jira'],
    'cross-service',
    'history',
    'Stale items from all services',
  ),
  q(
    'x-daily-summary',
    'Generate a daily summary of all services',
    ['slack', 'github', 'trello'],
    'cross-service',
    'summarize',
    'Comprehensive daily digest',
  ),
];

// ── Lookup helpers ─────────────────────────────────────────────────────

/** Get queries relevant to a specific service. */
export function getQueriesForService(serviceId: string): ServiceQuery[] {
  return QUERY_CATALOG.filter((q) => q.serviceIds.includes(serviceId));
}

/** Get popular / cross-service queries. */
export function getPopularQueries(): ServiceQuery[] {
  return QUERY_CATALOG.filter((q) => q.category === 'cross-service');
}
