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
      <div class="ihealth-chip ihealth-status-${s.status} k-row k-breathe k-materialise k-status-${kStatus}" title="${_esc(s.serviceName)}: ${statusLabel(s.status)}">
        ${kineticDot()}
        <span class="ms" style="font-size:16px">${_esc(s.icon)}</span>
        <span class="ihealth-chip-name">${_esc(s.serviceName)}</span>
        <span class="ms ihealth-chip-status" style="color:${color};font-size:12px">${icon}</span>
      </div>`;
    })
    .join('');

  return `<div class="ihealth-strip k-stagger">${chips}</div>`;
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

// ── Wire events ────────────────────────────────────────────────────────

export function wireDashboardEvents(container: HTMLElement): void {
  container.addEventListener('click', (e) => {
    const target = e.target as HTMLElement;

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
