// src/views/integrations/detail.ts — Extended Service Detail Panel
//
// Molecule: renders the enriched detail slide-in for a connected service,
// showing health status, recent action history, credential info, and
// quick actions. Complements the base detail in molecules.ts.

import { invoke } from '@tauri-apps/api/core';
import type { ServiceDefinition, ConnectedService } from './atoms';
import {
  type IntegrationActionLog,
  formatDuration,
  timeAgo,
} from '../../engine/atoms/action-labels';
import {
  type ServiceHealth,
  statusIcon,
  statusColor,
  statusLabel,
  daysUntilExpiry,
} from '../../features/integration-health/atoms';

// ── Types ──────────────────────────────────────────────────────────────

export interface DetailViewState {
  service: ServiceDefinition;
  connected: ConnectedService | null;
  health: ServiceHealth | null;
  recentActions: IntegrationActionLog[];
}

// ── Renderers ──────────────────────────────────────────────────────────

/**
 * Render the extended detail section for a connected service.
 * Inject below the base detail panel's setup-guide section.
 */
export function renderServiceDetail(state: DetailViewState): string {
  const parts: string[] = [];

  // Health status card
  if (state.health) {
    parts.push(_renderHealthCard(state.health));
  }

  // Recent actions
  if (state.recentActions.length) {
    parts.push(_renderRecentActions(state.recentActions));
  }

  // Connection info
  if (state.connected) {
    parts.push(_renderConnectionInfo(state.connected));
  }

  // Quick actions
  parts.push(_renderQuickActions(state.service, !!state.connected));

  return parts.join('');
}

function _renderHealthCard(health: ServiceHealth): string {
  const icon = statusIcon(health.status);
  const color = statusColor(health.status);
  const label = statusLabel(health.status);

  let expiryInfo = '';
  if (health.tokenExpiry) {
    const days = daysUntilExpiry(health.tokenExpiry);
    if (days !== null && days <= 7) {
      expiryInfo = `<div class="svc-detail-expiry"><span class="ms" style="font-size:14px;color:var(--warning)">schedule</span> Token expires in ${days} day${days !== 1 ? 's' : ''}</div>`;
    }
  }

  return `
    <div class="integrations-detail-section">
      <h3><span class="ms ms-sm">monitor_heart</span> Health Status</h3>
      <div class="svc-detail-health">
        <div class="svc-detail-health-row">
          <span class="ms" style="color:${color};font-size:20px">${icon}</span>
          <span class="svc-detail-health-label">${label}</span>
          ${health.message ? `<span class="svc-detail-health-msg">${_esc(health.message)}</span>` : ''}
        </div>
        ${expiryInfo}
        <div class="svc-detail-health-meta">
          <span class="svc-detail-health-checked">Last checked: ${health.lastChecked ? timeAgo(health.lastChecked) : 'never'}</span>
          ${health.recentFailures ? `<span class="svc-detail-health-failures">${health.recentFailures} recent failure${health.recentFailures !== 1 ? 's' : ''}</span>` : ''}
          ${health.todayActions ? `<span class="svc-detail-health-actions">${health.todayActions} action${health.todayActions !== 1 ? 's' : ''} today</span>` : ''}
        </div>
      </div>
    </div>`;
}

function _renderRecentActions(actions: IntegrationActionLog[]): string {
  const rows = actions
    .slice(0, 8)
    .map((a) => {
      const statusCls =
        a.status === 'success' ? 'success' : a.status === 'failed' ? 'failed' : 'running';
      const statusMs =
        a.status === 'success' ? 'check_circle' : a.status === 'failed' ? 'error' : 'pending';

      return `
      <div class="svc-detail-action svc-detail-action--${statusCls}">
        <span class="ms" style="font-size:14px">${statusMs}</span>
        <span class="svc-detail-action-label">${_esc(a.actionLabel)}</span>
        <span class="svc-detail-action-time">${timeAgo(a.timestamp)}</span>
        ${a.durationMs > 0 ? `<span class="svc-detail-action-dur">${formatDuration(a.durationMs)}</span>` : ''}
      </div>`;
    })
    .join('');

  return `
    <div class="integrations-detail-section">
      <h3><span class="ms ms-sm">history</span> Recent Activity</h3>
      <div class="svc-detail-actions">${rows}</div>
    </div>`;
}

function _renderConnectionInfo(conn: ConnectedService): string {
  const statusMs =
    conn.status === 'connected' ? 'check_circle' : conn.status === 'expired' ? 'error' : 'warning';
  const statusClr =
    conn.status === 'connected'
      ? 'var(--success, #22c55e)'
      : conn.status === 'expired'
        ? 'var(--danger, #ef4444)'
        : 'var(--warning, #f59e0b)';

  return `
    <div class="integrations-detail-section">
      <h3><span class="ms ms-sm">link</span> Connection</h3>
      <div class="svc-detail-conn">
        <div class="svc-detail-conn-row">
          <span class="ms" style="color:${statusClr};font-size:16px">${statusMs}</span>
          <span>Status: <strong>${conn.status}</strong></span>
        </div>
        <div class="svc-detail-conn-row">
          <span class="ms" style="font-size:16px">calendar_today</span>
          <span>Connected: ${_formatDate(conn.connectedAt)}</span>
        </div>
        ${
          conn.lastUsed
            ? `
        <div class="svc-detail-conn-row">
          <span class="ms" style="font-size:16px">access_time</span>
          <span>Last used: ${timeAgo(conn.lastUsed)}</span>
        </div>`
            : ''
        }
        <div class="svc-detail-conn-row">
          <span class="ms" style="font-size:16px">build</span>
          <span>${conn.toolCount} tool${conn.toolCount !== 1 ? 's' : ''} available</span>
        </div>
      </div>
    </div>`;
}

function _renderQuickActions(service: ServiceDefinition, isConnected: boolean): string {
  if (!isConnected) return '';

  const examples = service.queryExamples.slice(0, 3);
  const queryButtons = examples
    .map(
      (q) =>
        `<button class="svc-detail-quick-btn" data-query="${_esc(q)}">
      <span class="ms" style="font-size:14px">chat</span> ${_esc(q)}
    </button>`,
    )
    .join('');

  return `
    <div class="integrations-detail-section">
      <h3><span class="ms ms-sm">bolt</span> Quick Actions</h3>
      <div class="svc-detail-quick">
        ${queryButtons}
        <button class="svc-detail-quick-btn svc-detail-quick-btn--danger" data-action="disconnect" data-service-id="${service.id}">
          <span class="ms" style="font-size:14px">link_off</span> Disconnect ${_esc(service.name)}
        </button>
      </div>
    </div>`;
}

// ── Data Loading ───────────────────────────────────────────────────────

/**
 * Load full detail state for a service from backend.
 */
export async function loadDetailState(
  service: ServiceDefinition,
  connected: ConnectedService | null,
): Promise<DetailViewState> {
  let health: ServiceHealth | null = null;
  let recentActions: IntegrationActionLog[] = [];

  try {
    const allHealth: ServiceHealth[] = await invoke('engine_health_check_services');
    health = allHealth.find((h) => h.service === service.id) ?? null;
  } catch {
    /* silent */
  }

  try {
    recentActions = await invoke('engine_action_log_list', {
      service: service.id,
      limit: 8,
    });
  } catch {
    /* silent */
  }

  return { service, connected, health, recentActions };
}

// ── Event Wiring ───────────────────────────────────────────────────────

/**
 * Wire events for the detail panel's extended section.
 */
export function wireDetailEvents(container: HTMLElement): void {
  container.addEventListener('click', (e) => {
    const target = e.target as HTMLElement;

    // Query quick action → send to chat
    const queryBtn = target.closest('.svc-detail-quick-btn[data-query]') as HTMLElement | null;
    if (queryBtn) {
      const query = queryBtn.dataset.query;
      if (query) {
        const chatInput = document.getElementById('chat-input') as HTMLTextAreaElement | null;
        if (chatInput) {
          chatInput.value = query;
          chatInput.dispatchEvent(new Event('input', { bubbles: true }));
          // Switch to chat view
          const chatNav = document.querySelector('[data-view="chat"]') as HTMLElement | null;
          chatNav?.click();
        }
      }
    }

    // Disconnect action
    const disconnectBtn = target.closest('[data-action="disconnect"]') as HTMLElement | null;
    if (disconnectBtn) {
      const serviceId = disconnectBtn.dataset.serviceId;
      if (serviceId) {
        _handleDisconnect(serviceId);
      }
    }
  });
}

async function _handleDisconnect(serviceId: string): Promise<void> {
  try {
    await invoke('engine_health_update_service', {
      service: serviceId,
      status: 'unknown',
      message: 'Disconnected by user',
    });
    // Refresh the view
    const event = new CustomEvent('integrations:refresh');
    document.dispatchEvent(event);
  } catch {
    /* silent */
  }
}

// ── Helpers ────────────────────────────────────────────────────────────

function _esc(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function _formatDate(iso: string): string {
  try {
    return new Date(iso).toLocaleDateString(undefined, {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
    });
  } catch {
    return iso;
  }
}
