// src/features/integration-guardrails/molecules.ts — DOM rendering + event wiring
//
// Molecule-level: renders confirmation cards, rate-limit warnings,
// dry-run plans, and audit log viewer. Calls IPC for persistence.

import { invoke } from '@tauri-apps/api/core';
import {
  type IntegrationRiskLevel,
  type DryRunPlan,
  type DryRunStep,
  type CredentialUsageLog,
  type AgentServicePermission,
  type AccessLevel,
  riskMeta,
  checkRateLimit,
  bumpRateLimit,
  accessMeta,
} from './atoms';

// ── Confirmation card (injected into chat) ─────────────────────────────

export interface ConfirmationRequest {
  id: string;
  service: string;
  serviceName: string;
  action: string;
  risk: IntegrationRiskLevel;
  target?: string;
  preview?: string;
}

/**
 * Render a confirmation card HTML string for a chat message.
 * Returns the HTML; caller injects into chat DOM.
 */
export function renderConfirmationCard(req: ConfirmationRequest): string {
  const meta = riskMeta(req.risk);
  const targetHtml = req.target
    ? `<div class="guardrail-confirm-target">${_esc(req.target)}</div>`
    : '';
  const previewHtml = req.preview
    ? `<div class="guardrail-confirm-preview">${_esc(req.preview)}</div>`
    : '';

  return `
    <div class="guardrail-confirm-card ${meta.cssClass}" data-confirm-id="${req.id}">
      <div class="guardrail-confirm-header">
        <span class="ms" style="color:${meta.color}">${meta.icon}</span>
        <span class="guardrail-confirm-title">Agent wants to: <strong>${_esc(req.action)}</strong></span>
        <span class="guardrail-service-badge">${_esc(req.serviceName)}</span>
      </div>
      ${targetHtml}
      ${previewHtml}
      <div class="guardrail-confirm-actions">
        <button class="guardrail-btn guardrail-btn-approve" data-action="approve" data-confirm-id="${req.id}">
          <span class="ms">check</span> Confirm
        </button>
        ${
          req.risk === 'hard'
            ? ''
            : `
        <button class="guardrail-btn guardrail-btn-edit" data-action="edit" data-confirm-id="${req.id}">
          <span class="ms">edit</span> Edit
        </button>`
        }
        <button class="guardrail-btn guardrail-btn-cancel" data-action="cancel" data-confirm-id="${req.id}">
          <span class="ms">close</span> Cancel
        </button>
      </div>
    </div>`;
}

// ── Rate limit warning ─────────────────────────────────────────────────

export function renderRateLimitWarning(
  service: string,
  serviceName: string,
  remaining: number,
  limit: number,
): string {
  const pct = Math.round(((limit - remaining) / limit) * 100);
  const isExhausted = remaining <= 0;

  return `
    <div class="guardrail-rate-warning ${isExhausted ? 'exhausted' : ''}">
      <div class="guardrail-rate-header">
        <span class="ms">${isExhausted ? 'error' : 'speed'}</span>
        <span>${isExhausted ? 'Rate limit reached' : 'Approaching rate limit'} — ${_esc(serviceName)}</span>
      </div>
      <div class="guardrail-rate-bar">
        <div class="guardrail-rate-fill" style="width:${pct}%"></div>
      </div>
      <div class="guardrail-rate-info">
        ${remaining} / ${limit} actions remaining in window
      </div>
      ${
        isExhausted
          ? `
      <div class="guardrail-rate-actions">
        <button class="guardrail-btn guardrail-btn-approve" data-rate-service="${_esc(service)}" data-rate-action="bump">
          <span class="ms">add</span> Allow 20 more
        </button>
        <button class="guardrail-btn guardrail-btn-cancel" data-rate-service="${_esc(service)}" data-rate-action="wait">
          <span class="ms">pause</span> Wait
        </button>
      </div>`
          : ''
      }
    </div>`;
}

// ── Dry-run plan ───────────────────────────────────────────────────────

export function renderDryRunPlan(plan: DryRunPlan): string {
  const stepsHtml = plan.steps.map((s) => _renderPlanStep(s)).join('');
  const highCount = plan.highRiskCount;

  return `
    <div class="guardrail-dryrun" data-plan-id="${plan.id}">
      <div class="guardrail-dryrun-header">
        <span class="ms">playlist_play</span>
        <span>Planned actions (${plan.totalActions} steps${highCount > 0 ? `, ${highCount} high-risk` : ''})</span>
      </div>
      <ol class="guardrail-dryrun-steps">${stepsHtml}</ol>
      <div class="guardrail-dryrun-actions">
        <button class="guardrail-btn guardrail-btn-approve" data-plan-id="${plan.id}" data-plan-action="run-all">
          <span class="ms">play_arrow</span> Run all
        </button>
        <button class="guardrail-btn guardrail-btn-edit" data-plan-id="${plan.id}" data-plan-action="step">
          <span class="ms">skip_next</span> Step-by-step
        </button>
        <button class="guardrail-btn guardrail-btn-cancel" data-plan-id="${plan.id}" data-plan-action="cancel">
          <span class="ms">close</span> Cancel
        </button>
      </div>
    </div>`;
}

function _renderPlanStep(step: DryRunStep): string {
  const meta = riskMeta(step.risk);
  const preview = step.preview ? ` — <em>${_esc(step.preview)}</em>` : '';
  return `
    <li class="guardrail-step ${meta.cssClass}">
      <span class="ms" style="color:${meta.color};font-size:16px">${meta.icon}</span>
      <strong>${_esc(step.service)}</strong>: ${_esc(step.action)}
      ${step.target ? `→ ${_esc(step.target)}` : ''}${preview}
    </li>`;
}

// ── Audit log viewer ───────────────────────────────────────────────────

export function renderAuditLog(logs: CredentialUsageLog[]): string {
  if (!logs.length) {
    return `<div class="guardrail-audit-empty">
      <span class="ms">history</span> No credential usage logged yet.
    </div>`;
  }

  const rows = logs
    .slice(0, 100)
    .map((log) => {
      const resultIcon =
        log.result === 'success' ? 'check_circle' : log.result === 'denied' ? 'block' : 'error';
      const resultColor =
        log.result === 'success'
          ? 'var(--success)'
          : log.result === 'denied'
            ? 'var(--warning)'
            : 'var(--danger)';
      const ts = new Date(log.timestamp).toLocaleString();
      return `
      <tr class="guardrail-audit-row">
        <td>${_esc(ts)}</td>
        <td>${_esc(log.agent)}</td>
        <td>${_esc(log.service)}</td>
        <td>${_esc(log.action)}</td>
        <td>${_esc(log.accessLevel)}</td>
        <td>${log.approved ? 'Auto' : 'Manual'}</td>
        <td><span class="ms" style="color:${resultColor};font-size:16px">${resultIcon}</span></td>
      </tr>`;
    })
    .join('');

  return `
    <div class="guardrail-audit">
      <div class="guardrail-audit-header">
        <span class="ms">history</span>
        <span>Integration Access Log (last ${Math.min(logs.length, 100)} entries)</span>
        <button class="guardrail-btn guardrail-btn-cancel guardrail-audit-clear" data-audit-action="clear">
          <span class="ms">delete_sweep</span> Clear
        </button>
      </div>
      <table class="guardrail-audit-table">
        <thead>
          <tr>
            <th>Time</th>
            <th>Agent</th>
            <th>Service</th>
            <th>Action</th>
            <th>Access</th>
            <th>Approval</th>
            <th>Result</th>
          </tr>
        </thead>
        <tbody>${rows}</tbody>
      </table>
    </div>`;
}

// ── Agent permission editor ────────────────────────────────────────────

export function renderPermissionEditor(
  agentId: string,
  permissions: AgentServicePermission[],
  availableServices: string[],
): string {
  const rows = availableServices
    .map((svc) => {
      const perm = permissions.find((p) => p.service === svc);
      const current: AccessLevel = perm?.access ?? 'read';
      const meta = accessMeta(current);

      const options = (['none', 'read', 'write', 'full'] as AccessLevel[])
        .map(
          (level) =>
            `<option value="${level}" ${level === current ? 'selected' : ''}>${accessMeta(level).label}</option>`,
        )
        .join('');

      return `
      <div class="guardrail-perm-row" data-agent="${_esc(agentId)}" data-service="${_esc(svc)}">
        <span class="ms" style="color:${meta.color}">${meta.icon}</span>
        <span class="guardrail-perm-service">${_esc(svc)}</span>
        <select class="guardrail-perm-select" data-agent="${_esc(agentId)}" data-service="${_esc(svc)}">
          ${options}
        </select>
      </div>`;
    })
    .join('');

  return `
    <div class="guardrail-permissions">
      <div class="guardrail-perm-header">
        <span class="ms">admin_panel_settings</span>
        <span>Service Permissions</span>
      </div>
      <div class="guardrail-perm-list">${rows}</div>
    </div>`;
}

// ── Event wiring ───────────────────────────────────────────────────────

/** Pending confirmation resolve callbacks. */
const _pendingConfirms = new Map<string, { resolve: (approved: boolean) => void }>();

/**
 * Show a confirmation card and wait for user response.
 * Returns true if approved, false if cancelled.
 */
export async function requestConfirmation(req: ConfirmationRequest): Promise<boolean> {
  // Check rate limit
  const rl = checkRateLimit(req.service);
  if (!rl.allowed) {
    // Inject rate limit warning into chat
    _injectIntoChatArea(
      renderRateLimitWarning(req.service, req.serviceName, rl.remaining, rl.limit),
    );
    return false;
  }

  // Log the action attempt
  await _logAction(req.service, req.action, 'pending');

  return new Promise((resolve) => {
    _pendingConfirms.set(req.id, { resolve });
    _injectIntoChatArea(renderConfirmationCard(req));
  });
}

/**
 * Wire event listeners on the chat container for confirmation/rate/plan buttons.
 * Call once after chat DOM is ready.
 */
export function wireGuardrailEvents(container: HTMLElement): void {
  container.addEventListener('click', (e) => {
    const target = e.target as HTMLElement;
    const btn = target.closest('[data-confirm-id]') as HTMLElement | null;
    const rateBtn = target.closest('[data-rate-service]') as HTMLElement | null;
    const planBtn = target.closest('[data-plan-id]') as HTMLElement | null;
    const auditBtn = target.closest('[data-audit-action]') as HTMLElement | null;

    // Confirmation actions
    if (btn) {
      const id = btn.dataset.confirmId!;
      const action = btn.dataset.action;
      const pending = _pendingConfirms.get(id);
      if (pending) {
        pending.resolve(action === 'approve');
        _pendingConfirms.delete(id);
        _removeCard(container, `[data-confirm-id="${id}"].guardrail-confirm-card`);
      }
    }

    // Rate limit actions
    if (rateBtn) {
      const svc = rateBtn.dataset.rateService!;
      const action = rateBtn.dataset.rateAction;
      if (action === 'bump') {
        bumpRateLimit(svc, 20);
        _removeCard(container, `.guardrail-rate-warning`);
      }
    }

    // Plan actions
    if (planBtn && planBtn.dataset.planAction) {
      const planId = planBtn.dataset.planId!;
      const action = planBtn.dataset.planAction;
      if (action === 'cancel') {
        _removeCard(container, `[data-plan-id="${planId}"].guardrail-dryrun`);
      }
      // 'run-all' and 'step' events are emitted for the engine to handle
      container.dispatchEvent(
        new CustomEvent('guardrail:plan', {
          detail: { planId, action },
          bubbles: true,
        }),
      );
    }

    // Audit clear
    if (auditBtn) {
      invoke('engine_guardrails_clear_audit').catch(() => {});
      const auditEl = container.querySelector('.guardrail-audit');
      if (auditEl)
        auditEl.innerHTML =
          '<div class="guardrail-audit-empty"><span class="ms">history</span> Log cleared.</div>';
    }
  });

  // Permission select changes
  container.addEventListener('change', (e) => {
    const select = e.target as HTMLSelectElement;
    if (!select.classList.contains('guardrail-perm-select')) return;
    const agent = select.dataset.agent!;
    const service = select.dataset.service!;
    const access = select.value as AccessLevel;
    invoke('engine_guardrails_set_permission', { agentId: agent, service, access }).catch(() => {});
  });
}

// ── Internal helpers ───────────────────────────────────────────────────

function _esc(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function _injectIntoChatArea(html: string): void {
  const chatArea = document.querySelector('.chat-messages, #chat-messages');
  if (!chatArea) return;
  const div = document.createElement('div');
  div.className = 'guardrail-message';
  div.innerHTML = html;
  chatArea.appendChild(div);
  chatArea.scrollTop = chatArea.scrollHeight;
}

function _removeCard(container: HTMLElement, selector: string): void {
  const card = container.querySelector(selector);
  if (card) {
    card.classList.add('guardrail-fade-out');
    setTimeout(() => card.remove(), 300);
  }
}

async function _logAction(service: string, action: string, result: string): Promise<void> {
  try {
    await invoke('engine_guardrails_log_action', { service, action, result });
  } catch {
    /* silent */
  }
}
