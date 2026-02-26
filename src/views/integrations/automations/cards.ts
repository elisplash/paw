// src/views/integrations/automations/cards.ts — Card HTML renderers
//
// Atom-level: pure functions that return HTML strings, no DOM or IPC.

import {
  checkRequirements,
  triggerLabel,
  statusBadge,
  type AutomationTemplate,
  type ActiveAutomation,
} from './atoms';
import { svcName, svcIcon, svcColor, formatDate } from './ipc';
import { escHtml } from '../atoms';

// ── Template card ──────────────────────────────────────────────────────

export function renderTemplateCard(t: AutomationTemplate, connectedIds: Set<string>): string {
  const { met: _met, missing } = checkRequirements(t, connectedIds);
  const ready = missing.length === 0;

  return `
    <div class="automation-card" data-template-id="${t.id}">
      <div class="automation-card-header">
        <div class="automation-card-services">
          ${t.requiredServices
            .map(
              (sid) => `
            <span class="automation-svc-dot ${connectedIds.has(sid) ? 'connected' : 'missing'}"
                  style="color: ${svcColor(sid)}" title="${svcName(sid)}">
              <span class="ms ms-sm">${svcIcon(sid)}</span>
            </span>
          `,
            )
            .join('')}
        </div>
        <span class="automation-card-trigger">${triggerLabel(t.trigger)}</span>
      </div>
      <h4 class="automation-card-name">${escHtml(t.name)}</h4>
      <p class="automation-card-desc">${escHtml(t.description)}</p>
      <div class="automation-card-steps">
        ${t.steps
          .map(
            (s) => `
          <span class="automation-step-chip">
            <span class="ms ms-xs">${s.icon}</span> ${escHtml(s.action)}
          </span>
        `,
          )
          .join('<span class="automation-step-arrow">→</span>')}
      </div>
      <div class="automation-card-footer">
        ${
          ready
            ? `<button class="btn btn-primary btn-sm automation-activate-btn" data-template-id="${t.id}">
              <span class="ms ms-sm">play_arrow</span> Activate
            </button>`
            : `<span class="automation-missing-label">
              <span class="ms ms-sm">link_off</span>
              Connect ${missing.map(svcName).join(', ')} first
            </span>`
        }
        <span class="automation-setup-time">${escHtml(t.estimatedSetup)}</span>
      </div>
    </div>
  `;
}

// ── Active automation card ─────────────────────────────────────────────

export function renderActiveCard(a: ActiveAutomation): string {
  const badge = statusBadge(a.status);
  return `
    <div class="automation-active-card" data-auto-id="${a.id}">
      <div class="automation-active-top">
        <span class="automation-active-status automation-status-${a.status}">
          <span class="ms ms-sm">${badge.icon}</span> ${badge.label}
        </span>
        <span class="automation-active-trigger">${triggerLabel(a.trigger)}</span>
      </div>
      <h4 class="automation-active-name">${escHtml(a.name)}</h4>
      <p class="automation-active-desc">${escHtml(a.description)}</p>
      <div class="automation-active-services">
        ${a.services
          .map(
            (sid) => `
          <span class="automation-svc-chip" style="--svc-color: ${svcColor(sid)}">
            <span class="ms ms-sm">${svcIcon(sid)}</span> ${svcName(sid)}
          </span>
        `,
          )
          .join('')}
      </div>
      <div class="automation-active-footer">
        <span class="automation-active-stats">
          ${
            a.lastRunAt
              ? `Last run: ${formatDate(a.lastRunAt)}
               ${
                 a.lastRunResult === 'success'
                   ? '<span class="ms ms-sm" style="color:var(--success)">check_circle</span>'
                   : a.lastRunResult === 'error'
                     ? '<span class="ms ms-sm" style="color:var(--danger)">error</span>'
                     : ''
               }
               ${a.lastRunDetails ? `· ${escHtml(a.lastRunDetails)}` : ''}`
              : 'Not yet run'
          }
          · ${a.runCount} runs
        </span>
        <div class="automation-active-actions">
          <button class="btn btn-ghost btn-xs auto-toggle-btn"
                  data-auto-id="${a.id}" data-action="${a.status === 'active' ? 'pause' : 'resume'}"
                  title="${a.status === 'active' ? 'Pause' : 'Resume'}">
            <span class="ms ms-sm">${a.status === 'active' ? 'pause' : 'play_arrow'}</span>
          </button>
          <button class="btn btn-ghost btn-xs auto-delete-btn"
                  data-auto-id="${a.id}" title="Delete">
            <span class="ms ms-sm">delete</span>
          </button>
        </div>
      </div>
    </div>
  `;
}
