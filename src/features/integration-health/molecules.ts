// src/features/integration-health/molecules.ts — Dashboard integration rendering
//
// Molecule-level: DOM-aware renderers for Today view integration cards.
// Connected Services strip, health warnings, smart suggestions.

import { invoke } from '@tauri-apps/api/core';
import {
  type ServiceHealth,
  type HealthSummary,
  type IntegrationSuggestion,
  type ChainRule,
  statusIcon,
  statusColor,
  statusLabel,
  computeHealthSummary,
  generateSuggestions,
} from './atoms';
import { kineticDot, type KineticStatus } from '../../components/kinetic-row';

// ── Connected Services Strip ───────────────────────────────────────────

export function renderConnectedStrip(services: ServiceHealth[]): string {
  if (!services.length) {
    return `
      <div class="ihealth-strip-empty">
        No services connected. <a href="#" data-nav="integrations" class="ihealth-link">Browse integrations</a>
      </div>`;
  }

  const chips = services
    .map((s) => {
      const color = statusColor(s.status);
      const icon = statusIcon(s.status);
      const kStatus: KineticStatus =
        s.status === 'healthy'
          ? 'healthy'
          : s.status === 'degraded'
            ? 'warning'
            : s.status === 'error' || s.status === 'expired'
              ? 'error'
              : 'idle';
      return `
      <div class="ihealth-chip ihealth-status-${s.status} k-row k-breathe k-materialise k-status-${kStatus}" data-service="${_esc(s.service)}" title="${_esc(s.serviceName)}: ${statusLabel(s.status)} — click to test connection">
        ${kineticDot()}
        <span class="ms" style="font-size:16px">${_esc(s.icon)}</span>
        <span class="ihealth-chip-name">${_esc(s.serviceName)}</span>
        <span class="ms ihealth-chip-status" style="color:${color};font-size:12px">${icon}</span>
        <span class="ms ihealth-chip-settings" style="font-size:13px;color:var(--text-muted);margin-left:2px;opacity:0;transition:opacity .15s" title="Connection settings">settings</span>
      </div>`;
    })
    .join('');

  return `<div class="ihealth-strip k-stagger">${chips}</div>\n<div id="ihealth-test-panel" class="ihealth-test-panel" style="display:none"></div>`;
}

// ── Health Warning Banner ──────────────────────────────────────────────

export function renderHealthWarning(summary: HealthSummary): string {
  if (!summary.needsAttention.length) return '';

  const count = summary.needsAttention.length;
  const items = summary.needsAttention
    .slice(0, 3)
    .map((s) => {
      const color = statusColor(s.status);
      const icon = statusIcon(s.status);
      const msg = s.message ?? statusLabel(s.status);
      return `
      <div class="ihealth-warn-item k-row k-materialise">
        <span class="ms" style="color:${color};font-size:16px">${icon}</span>
        <span><strong>${_esc(s.serviceName)}</strong>: ${_esc(msg)}</span>
        ${s.status === 'expired' ? `<button class="guardrail-btn guardrail-btn-approve ihealth-reauth-btn" data-service="${_esc(s.service)}"><span class="ms">refresh</span> Re-auth</button>` : ''}
      </div>`;
    })
    .join('');

  return `
    <div class="ihealth-warning">
      <div class="ihealth-warning-header">
        <span class="ms" style="color:var(--warning);font-size:18px">warning</span>
        <span>${count} integration${count !== 1 ? 's' : ''} need${count === 1 ? 's' : ''} attention</span>
      </div>
      ${items}
    </div>`;
}

// ── Smart Suggestions ──────────────────────────────────────────────────

export function renderSuggestions(suggestions: IntegrationSuggestion[]): string {
  if (!suggestions.length) return '';

  const cards = suggestions
    .map(
      (s) => `
    <div class="ihealth-suggestion k-row k-spring k-materialise" data-suggestion-id="${s.id}" data-action="${_esc(s.action)}">
      <span class="ms" style="font-size:20px;color:var(--accent)">${_esc(s.icon)}</span>
      <div class="ihealth-suggestion-text">
        <span>${_esc(s.text)}</span>
      </div>
      <button class="guardrail-btn guardrail-btn-edit ihealth-suggestion-btn" data-action="${_esc(s.action)}">
        ${_esc(s.actionLabel)}
      </button>
    </div>`,
    )
    .join('');

  return `
    <div class="ihealth-suggestions k-stagger">
      <div class="ihealth-suggestions-header">
        <span class="ms" style="font-size:18px;color:var(--accent)">auto_awesome</span>
        <span>Smart Suggestions</span>
      </div>
      ${cards}
    </div>`;
}

// ── Workflow Chain Rules ───────────────────────────────────────────────

export function renderChainRules(rules: ChainRule[]): string {
  if (!rules.length) {
    return `
      <div class="ihealth-chains-empty">
        <span class="ms">link</span>
        No workflow chains configured yet.
      </div>`;
  }

  const rows = rules
    .map(
      (r) => `
    <div class="ihealth-chain-row k-row k-spring k-materialise">
      <span class="ms" style="font-size:16px">link</span>
      <span class="ihealth-chain-name">${_esc(r.name)}</span>
      <span class="ihealth-chain-flow">
        ${_esc(r.trigger.service)} → ${_esc(r.then.service)}
      </span>
      <label class="ihealth-chain-toggle">
        <input type="checkbox" ${r.enabled ? 'checked' : ''} data-chain-id="${r.id}" />
        <span class="ihealth-chain-slider"></span>
      </label>
    </div>`,
    )
    .join('');

  return `
    <div class="ihealth-chains k-stagger">
      <div class="ihealth-chains-header">
        <span class="ms" style="font-size:18px;color:var(--accent)">link</span>
        <span>Workflow Chains</span>
      </div>
      ${rows}
    </div>`;
}

// ── IPC helpers ────────────────────────────────────────────────────────

export async function loadServiceHealth(): Promise<ServiceHealth[]> {
  try {
    return await invoke('engine_health_check_services');
  } catch {
    return [];
  }
}

export async function loadChainRules(): Promise<ChainRule[]> {
  try {
    return await invoke('engine_health_list_chains');
  } catch {
    return [];
  }
}

export async function toggleChainRule(id: string, enabled: boolean): Promise<void> {
  try {
    await invoke('engine_health_toggle_chain', { chainId: id, enabled });
  } catch {
    /* silent */
  }
}

// ── Connection Test Panel ──────────────────────────────────────────────

/** Open the connection test panel for a specific service. */
async function _openTestPanel(serviceId: string, container: HTMLElement): Promise<void> {
  const panel = container.querySelector('#ihealth-test-panel') as HTMLElement;
  if (!panel) return;

  const displayName = serviceId.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());

  // Show loading state
  panel.style.display = 'block';
  panel.innerHTML = `
    <div class="ihealth-test-header">
      <span class="ms" style="font-size:18px">settings</span>
      <span class="ihealth-test-title">${_esc(displayName)} — Connection</span>
      <button class="ihealth-test-close" title="Close"><span class="ms" style="font-size:16px">close</span></button>
    </div>
    <div class="ihealth-test-body" style="padding:12px 0;font-size:13px;color:var(--text-secondary)">
      <span class="ms ms-sm" style="animation:spin 1s linear infinite">sync</span> Loading credentials…
    </div>`;

  // Wire close
  panel.querySelector('.ihealth-test-close')?.addEventListener('click', () => {
    panel.style.display = 'none';
  });

  // Load stored credentials (masked)
  let creds: Record<string, string> = {};
  try {
    creds = await invoke<Record<string, string>>('engine_integrations_get_credentials', {
      serviceId,
    });
  } catch {
    /* no creds stored */
  }

  // Build connection URL display
  const urlValue = _deriveConnectionUrl(serviceId, creds);
  const hasCreds = Object.keys(creds).length > 0;

  // Build masked credential summary
  const credRows = Object.entries(creds)
    .map(([key, val]) => {
      const masked =
        key.toLowerCase().includes('token') ||
        key.toLowerCase().includes('secret') ||
        key.toLowerCase().includes('key') ||
        key.toLowerCase().includes('password')
          ? val
            ? `••••••••${val.slice(-4)}`
            : '(empty)'
          : val || '(empty)';
      const label = key.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
      return `
        <div class="ihealth-test-cred-row">
          <span class="ihealth-test-cred-label">${_esc(label)}</span>
          <span class="ihealth-test-cred-value">${_esc(masked)}</span>
        </div>`;
    })
    .join('');

  panel.innerHTML = `
    <div class="ihealth-test-header">
      <span class="ms" style="font-size:18px">settings</span>
      <span class="ihealth-test-title">${_esc(displayName)} — Connection</span>
      <button class="ihealth-test-close" title="Close"><span class="ms" style="font-size:16px">close</span></button>
    </div>
    <div class="ihealth-test-body">
      ${
        urlValue
          ? `
        <div class="ihealth-test-url-row">
          <span class="ms" style="font-size:15px;color:var(--accent)">link</span>
          <span class="ihealth-test-url-label">Connected URL</span>
          <code class="ihealth-test-url-value">${_esc(urlValue)}</code>
        </div>`
          : ''
      }
      ${
        hasCreds
          ? `
        <div class="ihealth-test-creds-section">
          <div class="ihealth-test-section-label"><span class="ms" style="font-size:14px">key</span> Stored Credentials</div>
          ${credRows}
        </div>`
          : `
        <div style="font-size:13px;color:var(--warning);display:flex;align-items:center;gap:6px;padding:8px 0">
          <span class="ms" style="font-size:16px">warning</span>
          No credentials found. Please configure this integration.
        </div>`
      }
      <div class="ihealth-test-actions">
        <button class="ihealth-test-btn ihealth-test-btn-primary" id="ihealth-run-test" ${!hasCreds ? 'disabled' : ''}>
          <span class="ms" style="font-size:15px">speed</span>
          Test Connection
        </button>
        <button class="ihealth-test-btn ihealth-test-btn-secondary" id="ihealth-go-settings">
          <span class="ms" style="font-size:15px">tune</span>
          Open Settings
        </button>
      </div>
      <div id="ihealth-test-result" style="display:none"></div>
    </div>`;

  // Wire close
  panel.querySelector('.ihealth-test-close')?.addEventListener('click', () => {
    panel.style.display = 'none';
  });

  // Wire test button
  panel.querySelector('#ihealth-run-test')?.addEventListener('click', async () => {
    const resultEl = panel.querySelector('#ihealth-test-result') as HTMLElement;
    const testBtn = panel.querySelector('#ihealth-run-test') as HTMLButtonElement;
    if (!resultEl || !testBtn) return;

    testBtn.disabled = true;
    testBtn.innerHTML =
      '<span class="ms" style="font-size:15px;animation:spin 1s linear infinite">sync</span> Testing…';
    resultEl.style.display = 'block';
    resultEl.innerHTML =
      '<span style="color:var(--text-secondary);font-size:12px">Connecting…</span>';

    try {
      const result = await invoke<{ success: boolean; message: string; details?: string }>(
        'engine_integrations_test_credentials',
        { serviceId, nodeType: '', credentials: creds },
      );

      if (result.success) {
        resultEl.innerHTML = `
          <div class="ihealth-test-result-ok">
            <span class="ms" style="color:var(--success);font-size:16px">check_circle</span>
            <span>${_esc(result.message)}</span>
            ${result.details ? `<span class="ihealth-test-result-details">${_esc(result.details)}</span>` : ''}
          </div>`;
      } else {
        resultEl.innerHTML = `
          <div class="ihealth-test-result-fail">
            <span class="ms" style="color:var(--error);font-size:16px">error</span>
            <span>${_esc(result.message)}</span>
          </div>`;
      }
    } catch (err) {
      resultEl.innerHTML = `
        <div class="ihealth-test-result-fail">
          <span class="ms" style="color:var(--error);font-size:16px">error</span>
          <span>Test failed: ${_esc(String(err))}</span>
        </div>`;
    }

    testBtn.disabled = false;
    testBtn.innerHTML = '<span class="ms" style="font-size:15px">speed</span> Test Connection';
  });

  // Wire settings navigation
  panel.querySelector('#ihealth-go-settings')?.addEventListener('click', () => {
    const navItem = document.querySelector('[data-view="integrations"]') as HTMLElement;
    navItem?.click();
  });
}

/** Derive the connection URL from stored credentials. */
function _deriveConnectionUrl(serviceId: string, creds: Record<string, string>): string {
  switch (serviceId) {
    case 'jira': {
      const domain = creds.domain || '';
      if (!domain) return '';
      const base = domain.startsWith('http') ? domain : `https://${domain}`;
      return `${base.replace(/\/$/, '')}/rest/api/3`;
    }
    case 'zendesk': {
      const sub = creds.subdomain || '';
      return sub ? `https://${sub}.zendesk.com/api/v2` : '';
    }
    case 'github':
      return 'https://api.github.com';
    case 'gitlab':
      return creds.base_url || creds.url || 'https://gitlab.com/api/v4';
    case 'slack':
      return 'https://slack.com/api';
    case 'discord':
      return 'https://discord.com/api/v10';
    case 'notion':
      return 'https://api.notion.com/v1';
    case 'linear':
      return 'https://api.linear.app/graphql';
    case 'hubspot':
      return 'https://api.hubapi.com';
    case 'stripe':
      return 'https://api.stripe.com/v1';
    case 'sendgrid':
      return 'https://api.sendgrid.com/v3';
    case 'twilio': {
      const sid = creds.account_sid || '';
      return sid ? `https://api.twilio.com/2010-04-01/Accounts/${sid}` : 'https://api.twilio.com';
    }
    case 'shopify': {
      const store = creds.store_name || creds.shop || '';
      return store ? `https://${store}.myshopify.com/admin/api` : '';
    }
    case 'clickup':
      return 'https://api.clickup.com/api/v2';
    case 'todoist':
      return 'https://api.todoist.com/rest/v2';
    case 'airtable':
      return 'https://api.airtable.com/v0';
    case 'trello':
      return 'https://api.trello.com/1';
    case 'telegram':
      return 'https://api.telegram.org';
    case 'pagerduty':
      return 'https://api.pagerduty.com';
    case 'microsoft-teams':
      return 'https://graph.microsoft.com/v1.0';
    default: {
      // Try common credential fields
      return creds.base_url || creds.url || creds.domain || '';
    }
  }
}

// ── Wire events ────────────────────────────────────────────────────────

export function wireDashboardEvents(container: HTMLElement): void {
  container.addEventListener('click', (e) => {
    const target = e.target as HTMLElement;

    // Integration chip → open connection test panel
    const chip = target.closest('.ihealth-chip[data-service]') as HTMLElement | null;
    if (chip) {
      const serviceId = chip.dataset.service;
      if (serviceId) {
        _openTestPanel(serviceId, container);
        return;
      }
    }

    // Re-auth button
    const reauth = target.closest('.ihealth-reauth-btn') as HTMLElement | null;
    if (reauth) {
      const service = reauth.dataset.service;
      if (service) {
        invoke('engine_health_trigger_reauth', { service }).catch(() => {});
      }
    }

    // Suggestion action
    const sugBtn = target.closest('.ihealth-suggestion-btn') as HTMLElement | null;
    if (sugBtn) {
      const action = sugBtn.dataset.action;
      if (action) {
        // Send action to chat as a message
        const chatInput = document.getElementById('chat-input') as HTMLTextAreaElement | null;
        if (chatInput) {
          chatInput.value = `Run ${action.replace(/_/g, ' ')}`;
          chatInput.dispatchEvent(new Event('input', { bubbles: true }));
        }
      }
    }

    // Navigation links
    const navLink = target.closest('[data-nav]') as HTMLElement | null;
    if (navLink) {
      e.preventDefault();
      const view = navLink.dataset.nav;
      if (view) {
        const navItem = document.querySelector(`[data-view="${view}"]`) as HTMLElement;
        navItem?.click();
      }
    }
  });

  // Chain toggles
  container.addEventListener('change', (e) => {
    const input = e.target as HTMLInputElement;
    if (input.dataset.chainId) {
      toggleChainRule(input.dataset.chainId, input.checked);
    }
  });
}

// ── Composite: full dashboard integration section ──────────────────────

export async function renderDashboardIntegrations(connectedServiceIds: string[]): Promise<string> {
  const health = await loadServiceHealth();
  const summary = computeHealthSummary(health);
  const suggestions = generateSuggestions(connectedServiceIds);
  const chains = await loadChainRules();

  return `
    <div class="ihealth-dashboard">
      ${renderConnectedStrip(health)}
      ${renderHealthWarning(summary)}
      ${renderSuggestions(suggestions)}
      ${chains.length > 0 ? renderChainRules(chains) : ''}
    </div>`;
}

// ── Internal helpers ───────────────────────────────────────────────────

function _esc(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
