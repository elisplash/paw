// src/components/molecules/action-receipt.ts — Inline Chat Action Receipts
//
// Molecule: renders compact receipt cards in the chat stream showing
// what an agent just did with an integration. Injected after tool calls complete.

import { invoke } from '@tauri-apps/api/core';
import {
  type IntegrationActionLog,
  translateAction,
  formatDuration,
  timeAgo,
} from '../../engine/atoms/action-labels';

// ── Types ──────────────────────────────────────────────────────────────

export interface ReceiptOptions {
  compact?: boolean; // ultra-compact (single line)
  showDuration?: boolean;
  showTimestamp?: boolean;
  showExternalLink?: boolean;
}

const DEFAULTS: ReceiptOptions = {
  compact: false,
  showDuration: true,
  showTimestamp: true,
  showExternalLink: true,
};

// ── Service Icon Map ───────────────────────────────────────────────────

const SERVICE_ICONS: Record<string, string> = {
  slack: 'tag',
  discord: 'forum',
  github: 'code',
  gmail: 'mail',
  hubspot: 'business_center',
  jira: 'bug_report',
  linear: 'linear_scale',
  trello: 'dashboard',
  notion: 'article',
  'google-sheets': 'table_chart',
  shopify: 'storefront',
  stripe: 'payments',
  salesforce: 'cloud',
  sendgrid: 'forward_to_inbox',
  twilio: 'sms',
  zendesk: 'support_agent',
  telegram: 'send',
  'google-calendar': 'calendar_month',
  'google-drive': 'folder',
};

function _serviceIcon(service: string): string {
  return SERVICE_ICONS[service] ?? 'extension';
}

// ── Render Functions ───────────────────────────────────────────────────

/**
 * Render a compact receipt card for a single integration action.
 */
export function renderReceipt(log: IntegrationActionLog, opts: ReceiptOptions = {}): string {
  const o = { ...DEFAULTS, ...opts };
  const statusIcon =
    log.status === 'success' ? 'check_circle' : log.status === 'failed' ? 'error' : 'pending';
  const statusColor =
    log.status === 'success'
      ? 'var(--success, #22c55e)'
      : log.status === 'failed'
        ? 'var(--danger, #ef4444)'
        : 'var(--warning, #f59e0b)';
  const svcIcon = _serviceIcon(log.service);

  if (o.compact) {
    return `
      <div class="action-receipt action-receipt--compact action-receipt--${log.status}">
        <span class="ms" style="color:${statusColor};font-size:14px">${statusIcon}</span>
        <span class="ms" style="font-size:14px">${svcIcon}</span>
        <span class="action-receipt-label">${_esc(log.actionLabel)}</span>
        ${o.showDuration && log.durationMs > 0 ? `<span class="action-receipt-duration">${formatDuration(log.durationMs)}</span>` : ''}
      </div>`;
  }

  const externalLink =
    o.showExternalLink && log.externalUrl
      ? `<a href="${_esc(log.externalUrl)}" target="_blank" rel="noopener" class="action-receipt-link" title="Open in ${_esc(log.serviceName)}"><span class="ms" style="font-size:14px">open_in_new</span></a>`
      : '';

  const errorSection =
    log.status === 'failed' && log.errorMessage
      ? `<div class="action-receipt-error"><span class="ms" style="font-size:13px">warning</span> ${_esc(log.errorMessage)}</div>`
      : '';

  return `
    <div class="action-receipt action-receipt--${log.status}">
      <div class="action-receipt-header">
        <span class="ms" style="color:${statusColor};font-size:16px">${statusIcon}</span>
        <span class="ms" style="font-size:16px">${svcIcon}</span>
        <div class="action-receipt-info">
          <span class="action-receipt-service">${_esc(log.serviceName)}</span>
          <span class="action-receipt-label">${_esc(log.actionLabel)}</span>
        </div>
        <div class="action-receipt-meta">
          ${o.showDuration && log.durationMs > 0 ? `<span class="action-receipt-duration">${formatDuration(log.durationMs)}</span>` : ''}
          ${o.showTimestamp ? `<span class="action-receipt-time">${timeAgo(log.timestamp)}</span>` : ''}
          ${externalLink}
        </div>
      </div>
      ${log.summary ? `<div class="action-receipt-summary">${_esc(log.summary)}</div>` : ''}
      ${errorSection}
    </div>`;
}

/**
 * Render a batch receipt — multiple actions grouped together.
 */
export function renderBatchReceipt(logs: IntegrationActionLog[]): string {
  if (!logs.length) return '';
  if (logs.length === 1) return renderReceipt(logs[0]);

  const successCount = logs.filter((l) => l.status === 'success').length;
  const failedCount = logs.filter((l) => l.status === 'failed').length;
  const services = [...new Set(logs.map((l) => l.serviceName))];

  const header = `${successCount} action${successCount !== 1 ? 's' : ''} completed${failedCount > 0 ? `, ${failedCount} failed` : ''} across ${services.join(', ')}`;

  const rows = logs.map((l) => renderReceipt(l, { compact: true })).join('');

  return `
    <div class="action-receipt-batch">
      <div class="action-receipt-batch-header">
        <span class="ms" style="font-size:16px;color:var(--accent)">receipt_long</span>
        <span>${header}</span>
      </div>
      <div class="action-receipt-batch-items">${rows}</div>
    </div>`;
}

// ── IPC: Record Action ─────────────────────────────────────────────────

/**
 * Record an integration action to the persistent log and return the
 * receipt HTML. Call this after a tool call completes.
 */
export async function recordAndRenderReceipt(
  service: string,
  action: string,
  input: Record<string, unknown>,
  output: unknown,
  status: 'success' | 'failed',
  durationMs: number,
  agent: string,
  errorMessage?: string,
): Promise<string> {
  const label = translateAction(service, action, input);
  const summary = typeof output === 'string' ? output : '';

  const log: IntegrationActionLog = {
    id: '', // backend assigns
    timestamp: new Date().toISOString(),
    service,
    serviceName: _serviceName(service),
    action,
    actionLabel: label,
    summary,
    agent,
    status,
    durationMs,
    input,
    output,
    errorMessage,
  };

  // Persist to backend
  try {
    await invoke('engine_action_log_record', { entry: log });
  } catch {
    /* non-fatal */
  }

  return renderReceipt(log);
}

// ── DOM insertion helper ───────────────────────────────────────────────

/**
 * Insert a receipt card into the chat stream after the current streaming message.
 */
export function injectReceiptIntoChat(html: string): void {
  const container = document.getElementById('chat-messages');
  if (!container) return;

  const wrapper = document.createElement('div');
  wrapper.className = 'action-receipt-wrapper';
  wrapper.innerHTML = html;
  container.appendChild(wrapper);
  container.scrollTop = container.scrollHeight;
}

// ── Helpers ────────────────────────────────────────────────────────────

function _esc(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function _serviceName(service: string): string {
  const names: Record<string, string> = {
    slack: 'Slack',
    discord: 'Discord',
    github: 'GitHub',
    gmail: 'Gmail',
    hubspot: 'HubSpot',
    jira: 'Jira',
    linear: 'Linear',
    trello: 'Trello',
    notion: 'Notion',
    'google-sheets': 'Google Sheets',
    shopify: 'Shopify',
    stripe: 'Stripe',
    salesforce: 'Salesforce',
    sendgrid: 'SendGrid',
    twilio: 'Twilio',
    zendesk: 'Zendesk',
    telegram: 'Telegram',
    'google-calendar': 'Google Calendar',
    'google-drive': 'Google Drive',
  };
  return names[service] ?? service.charAt(0).toUpperCase() + service.slice(1);
}
