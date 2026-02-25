// src/components/molecules/integration-feed.ts — Integration Activity Feed
//
// Molecule-level: renders activity feed, receipt cards, and pulse summary.
// Reusable across Today view, agent sessions, and integrations detail panel.

import { invoke } from '@tauri-apps/api/core';
import {
  type IntegrationActionLog,
  type ActionStats,
  type OutputCardType,
  computeStats,
  formatDuration,
  timeAgo,
  detectOutputType,
} from '../../engine/atoms/action-labels';

// ── Integration Pulse (Today view summary) ─────────────────────────────

export function renderPulse(stats: ActionStats): string {
  if (stats.total === 0) {
    return `
      <div class="intfeed-pulse intfeed-pulse-empty">
        <div class="intfeed-pulse-header">
          <span class="ms">electric_bolt</span>
          <span>Integration Pulse</span>
        </div>
        <div class="intfeed-pulse-body">No integration activity today.</div>
      </div>`;
  }

  const serviceRows = Object.entries(stats.byService)
    .sort(([, a], [, b]) => b.count - a.count)
    .slice(0, 5)
    .map(([_svc, s]) => {
      const icon = s.failed > 0 ? 'error' : 'check_circle';
      const color = s.failed > 0 ? 'var(--danger)' : 'var(--success)';
      const failText = s.failed > 0 ? ` · ${s.failed} failed` : '';
      return `
        <div class="intfeed-pulse-row">
          <span class="ms" style="color:${color};font-size:16px">${icon}</span>
          <span class="intfeed-pulse-service">${_esc(s.label)}</span>
          <span class="intfeed-pulse-count">${s.count} action${s.count !== 1 ? 's' : ''}${failText}</span>
        </div>`;
    })
    .join('');

  return `
    <div class="intfeed-pulse">
      <div class="intfeed-pulse-header">
        <span class="ms">electric_bolt</span>
        <span>Integration Pulse</span>
      </div>
      <div class="intfeed-pulse-summary">
        Today: <strong>${stats.total}</strong> action${stats.total !== 1 ? 's' : ''}${stats.failed > 0 ? ` · <span style="color:var(--danger)">${stats.failed} failed</span>` : ''}
      </div>
      <div class="intfeed-pulse-services">${serviceRows}</div>
    </div>`;
}

// ── Activity Feed (scrollable list) ────────────────────────────────────

export function renderFeed(
  actions: IntegrationActionLog[],
  options: { limit?: number; showAgent?: boolean } = {},
): string {
  const limit = options.limit ?? 20;
  const showAgent = options.showAgent ?? true;
  const display = actions.slice(0, limit);

  if (!display.length) {
    return `
      <div class="intfeed-empty">
        <span class="ms">history</span>
        <span>No integration activity yet.</span>
      </div>`;
  }

  const entries = display.map((a) => _renderEntry(a, showAgent)).join('');
  const moreHtml = actions.length > limit
    ? `<div class="intfeed-more">+ ${actions.length - limit} more actions</div>`
    : '';

  return `
    <div class="intfeed-list">
      ${entries}
      ${moreHtml}
    </div>`;
}

function _renderEntry(action: IntegrationActionLog, showAgent: boolean): string {
  const statusIcon = action.status === 'success'
    ? 'check_circle'
    : action.status === 'failed'
      ? 'error'
      : 'pending';
  const statusColor = action.status === 'success'
    ? 'var(--success, #22c55e)'
    : action.status === 'failed'
      ? 'var(--danger, #ef4444)'
      : 'var(--warning, #f59e0b)';

  const agentHtml = showAgent
    ? `<span class="intfeed-entry-agent">via ${_esc(action.agent)}</span>`
    : '';
  const errorHtml = action.errorMessage
    ? `<div class="intfeed-entry-error">${_esc(action.errorMessage)}</div>`
    : '';
  const linkHtml = action.externalUrl
    ? `<a class="intfeed-entry-link" href="${_esc(action.externalUrl)}" target="_blank" rel="noopener">Open <span class="ms" style="font-size:14px">open_in_new</span></a>`
    : '';
  const durHtml = action.durationMs > 0
    ? `<span class="intfeed-entry-dur">${formatDuration(action.durationMs)}</span>`
    : '';

  return `
    <div class="intfeed-entry intfeed-status-${action.status}" data-action-id="${action.id}">
      <div class="intfeed-entry-main">
        <span class="ms intfeed-entry-icon" style="color:${statusColor}">${statusIcon}</span>
        <div class="intfeed-entry-content">
          <div class="intfeed-entry-label">
            <strong>${_esc(action.serviceName)}</strong> · ${_esc(action.actionLabel)}
          </div>
          ${action.summary ? `<div class="intfeed-entry-summary">${_esc(action.summary)}</div>` : ''}
          ${errorHtml}
        </div>
        <div class="intfeed-entry-meta">
          <span class="intfeed-entry-time">${timeAgo(action.timestamp)}</span>
          ${durHtml}
          ${linkHtml}
        </div>
      </div>
      ${agentHtml}
    </div>`;
}

// ── In-chat receipt card (compact) ─────────────────────────────────────

export function renderReceipt(action: IntegrationActionLog): string {
  const icon = action.status === 'success' ? 'check_circle' : 'error';
  const color = action.status === 'success' ? 'var(--success)' : 'var(--danger)';
  const linkHtml = action.externalUrl
    ? `<a class="intfeed-receipt-link" href="${_esc(action.externalUrl)}" target="_blank" rel="noopener">Open <span class="ms" style="font-size:14px">open_in_new</span></a>`
    : '';

  return `
    <div class="intfeed-receipt intfeed-status-${action.status}">
      <span class="ms" style="color:${color};font-size:18px">${icon}</span>
      <span class="intfeed-receipt-text">
        <strong>${_esc(action.serviceName)}</strong> · ${_esc(action.actionLabel)}
      </span>
      ${action.summary ? `<span class="intfeed-receipt-summary">${_esc(action.summary)}</span>` : ''}
      ${linkHtml}
    </div>`;
}

// ── Rich output card (table / summary / timeline / notification) ───────

export function renderOutputCard(
  action: IntegrationActionLog,
  cardType?: OutputCardType,
): string {
  const type = cardType ?? detectOutputType(action.action, action.output);
  switch (type) {
    case 'table': return _renderTableCard(action);
    case 'summary': return _renderSummaryCard(action);
    case 'timeline': return _renderTimelineCard(action);
    case 'notification': return _renderNotificationCard(action);
  }
}

function _renderTableCard(action: IntegrationActionLog): string {
  const items = _extractItems(action.output);
  const columns = items.length > 0 ? Object.keys(items[0]).slice(0, 5) : [];
  const headerHtml = columns.map((c) => `<th>${_esc(_titleCase(c))}</th>`).join('');
  const rowsHtml = items.slice(0, 8).map((item) => {
    const cells = columns.map((c) => `<td>${_esc(String(item[c] ?? ''))}</td>`).join('');
    return `<tr>${cells}</tr>`;
  }).join('');
  const moreHtml = items.length > 8 ? `<div class="intfeed-card-more">+ ${items.length - 8} more</div>` : '';

  return `
    <div class="intfeed-output-card intfeed-card-table">
      <div class="intfeed-card-header">
        <span class="ms">table_chart</span>
        <strong>${_esc(action.serviceName)}</strong> · ${_esc(action.actionLabel)}
      </div>
      <table class="intfeed-card-table-el">
        <thead><tr>${headerHtml}</tr></thead>
        <tbody>${rowsHtml}</tbody>
      </table>
      ${moreHtml}
      ${_renderCardActions(action)}
    </div>`;
}

function _renderSummaryCard(action: IntegrationActionLog): string {
  return `
    <div class="intfeed-output-card intfeed-card-summary">
      <div class="intfeed-card-header">
        <span class="ms">summarize</span>
        <strong>${_esc(action.serviceName)}</strong> · ${_esc(action.actionLabel)}
      </div>
      <div class="intfeed-card-body">
        ${action.summary ? _esc(action.summary) : '<em>No summary available.</em>'}
      </div>
      ${_renderCardActions(action)}
    </div>`;
}

function _renderTimelineCard(action: IntegrationActionLog): string {
  const items = _extractItems(action.output);
  const entriesHtml = items.slice(0, 10).map((item) => {
    const ts = item.timestamp ?? item.date ?? item.created_at ?? '';
    const text = item.text ?? item.message ?? item.description ?? item.title ?? '';
    return `
      <div class="intfeed-tl-entry">
        ${ts ? `<span class="intfeed-tl-time">${_esc(String(ts))}</span>` : ''}
        <span class="intfeed-tl-text">${_esc(String(text))}</span>
      </div>`;
  }).join('');

  return `
    <div class="intfeed-output-card intfeed-card-timeline">
      <div class="intfeed-card-header">
        <span class="ms">timeline</span>
        <strong>${_esc(action.serviceName)}</strong> · ${_esc(action.actionLabel)}
      </div>
      <div class="intfeed-tl-list">${entriesHtml || '<em>No timeline data.</em>'}</div>
      ${_renderCardActions(action)}
    </div>`;
}

function _renderNotificationCard(action: IntegrationActionLog): string {
  const items = _extractItems(action.output);
  const messagesHtml = items.slice(0, 6).map((item) => {
    const from = item.from ?? item.user ?? item.sender ?? '';
    const text = item.text ?? item.message ?? item.content ?? '';
    const ts = item.timestamp ?? item.date ?? '';
    return `
      <div class="intfeed-notif-msg">
        ${from ? `<strong>${_esc(String(from))}</strong>: ` : ''}
        <span>${_esc(String(text))}</span>
        ${ts ? `<span class="intfeed-notif-time">${_esc(String(ts))}</span>` : ''}
      </div>`;
  }).join('');

  return `
    <div class="intfeed-output-card intfeed-card-notification">
      <div class="intfeed-card-header">
        <span class="ms">notifications</span>
        <strong>${_esc(action.serviceName)}</strong> · ${_esc(action.actionLabel)}
      </div>
      <div class="intfeed-notif-list">${messagesHtml || '<em>No messages.</em>'}</div>
      ${_renderCardActions(action)}
    </div>`;
}

function _renderCardActions(action: IntegrationActionLog): string {
  const linkBtn = action.externalUrl
    ? `<a class="guardrail-btn guardrail-btn-edit" href="${_esc(action.externalUrl)}" target="_blank" rel="noopener">
         <span class="ms">open_in_new</span> View in ${_esc(action.serviceName)}
       </a>`
    : '';

  return `
    <div class="intfeed-card-actions">
      ${linkBtn}
      <button class="guardrail-btn" data-feed-action="follow-up" data-action-id="${action.id}">
        <span class="ms">chat</span> Ask follow-up
      </button>
    </div>`;
}

// ── IPC helpers ────────────────────────────────────────────────────────

export async function loadRecentActions(
  limit: number = 20,
  service?: string,
): Promise<IntegrationActionLog[]> {
  try {
    return await invoke('engine_action_log_list', {
      limit,
      service: service ?? null,
    });
  } catch {
    return [];
  }
}

export async function loadActionStats(): Promise<ActionStats> {
  try {
    const actions: IntegrationActionLog[] = await invoke('engine_action_log_list', {
      limit: 200,
      service: null,
    });
    return computeStats(actions);
  } catch {
    return { total: 0, success: 0, failed: 0, running: 0, byService: {} };
  }
}

// ── Internal helpers ───────────────────────────────────────────────────

function _esc(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

function _titleCase(s: string): string {
  return s.replace(/[_-]/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
}

function _extractItems(output: unknown): Record<string, unknown>[] {
  if (Array.isArray(output)) return output as Record<string, unknown>[];
  if (output && typeof output === 'object') {
    const o = output as Record<string, unknown>;
    if (Array.isArray(o.items)) return o.items as Record<string, unknown>[];
    if (Array.isArray(o.results)) return o.results as Record<string, unknown>[];
    if (Array.isArray(o.data)) return o.data as Record<string, unknown>[];
  }
  return [];
}
