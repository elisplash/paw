// src/views/integrations/setup-guide.ts — Setup guide renderer (molecule)
//
// Renders a step-by-step credential setup guide with inline credential
// fields, "Test & Save" button, and success/error feedback.

import { invoke } from '@tauri-apps/api/core';
import { escHtml, type ServiceDefinition, type CredentialField } from './atoms';

// ── Types ──────────────────────────────────────────────────────────────

export type GuideState = 'idle' | 'testing' | 'success' | 'error';

export interface GuideResult {
  success: boolean;
  message: string;
  details?: string; // e.g. "47 contacts · 12 deals"
}

interface GuideCallbacks {
  onSave: (serviceId: string, credentials: Record<string, string>) => void;
  onClose: () => void;
}

// ── Module state ───────────────────────────────────────────────────────

let _guideState: GuideState = 'idle';
let _callbacks: GuideCallbacks = { onSave: () => {}, onClose: () => {} };

export function setGuideCallbacks(cb: GuideCallbacks): void {
  _callbacks = cb;
}

/** Convenience: set callbacks and render in one call. */
export function openSetupGuide(
  container: HTMLElement,
  service: ServiceDefinition,
  callbacks: GuideCallbacks,
): void {
  _callbacks = callbacks;
  renderSetupGuide(container, service);
}

// ── Render the full guide into a container ─────────────────────────────

export function renderSetupGuide(container: HTMLElement, service: ServiceDefinition): void {
  _guideState = 'idle';

  const guide = service.setupGuide;
  const fields = service.credentialFields;

  container.innerHTML = `
    <div class="setup-guide">
      <div class="setup-guide-header">
        <div class="setup-guide-icon" style="background: ${service.color}15; color: ${service.color}">
          <span class="ms ms-lg">${service.icon}</span>
        </div>
        <div class="setup-guide-title-wrap">
          <h2 class="setup-guide-title">${escHtml(guide.title)}</h2>
          <span class="setup-guide-time">
            <span class="ms ms-sm">schedule</span>
            ${escHtml(guide.estimatedTime)}
          </span>
        </div>
        <button class="btn btn-ghost btn-sm setup-guide-close" id="guide-close">
          <span class="ms">close</span>
        </button>
      </div>

      <ol class="setup-guide-steps">
        ${guide.steps
          .map(
            (step, i) => `
          <li class="setup-guide-step" data-step="${i + 1}">
            <div class="setup-guide-step-num">${i + 1}</div>
            <div class="setup-guide-step-body">
              ${
                step.link
                  ? `<a href="${escHtml(step.link)}" target="_blank" rel="noopener" class="setup-guide-step-link">
                    ${escHtml(step.instruction)}
                    <span class="ms ms-sm">open_in_new</span>
                  </a>`
                  : `<span>${escHtml(step.instruction)}</span>`
              }
              ${
                step.tip
                  ? `<div class="setup-guide-tip">
                    <span class="ms ms-sm">lightbulb</span>
                    ${escHtml(step.tip)}
                  </div>`
                  : ''
              }
            </div>
          </li>
        `,
          )
          .join('')}
      </ol>

      <div class="setup-guide-credentials">
        <h3 class="setup-guide-cred-title">
          <span class="ms ms-sm">key</span> Your Credentials
        </h3>
        ${_renderFields(fields)}
      </div>

      <div class="setup-guide-actions">
        <button class="btn btn-primary" id="guide-test-save">
          <span class="ms ms-sm">verified</span>
          <span class="guide-btn-label">Test &amp; Save</span>
        </button>
        <button class="btn btn-ghost" id="guide-cancel">Cancel</button>
      </div>

      <div class="setup-guide-feedback" id="guide-feedback" style="display:none;"></div>
    </div>
  `;

  _wireGuideEvents(container, service);
}

// ── Credential input fields ────────────────────────────────────────────

function _renderFields(fields: CredentialField[]): string {
  return fields
    .map(
      (f) => `
    <div class="setup-guide-field">
      <label class="setup-guide-label" for="cred-${f.key}">
        ${escHtml(f.label)}
        ${f.required ? '<span class="setup-guide-required">*</span>' : ''}
      </label>
      ${f.helpText ? `<div class="setup-guide-help">${escHtml(f.helpText)}</div>` : ''}
      <div class="setup-guide-input-wrap">
        <input
          type="${f.type === 'password' ? 'password' : 'text'}"
          id="cred-${f.key}"
          class="setup-guide-input"
          data-cred-key="${f.key}"
          placeholder="${escHtml(f.placeholder ?? '')}"
          ${f.required ? 'required' : ''}
          autocomplete="off"
          spellcheck="false"
        />
        ${
          f.type === 'password'
            ? `<button class="btn btn-ghost btn-xs setup-guide-toggle-vis"
                    data-target="cred-${f.key}" title="Toggle visibility">
              <span class="ms ms-sm">visibility_off</span>
            </button>`
            : ''
        }
      </div>
    </div>
  `,
    )
    .join('');
}

// ── Feedback rendering ─────────────────────────────────────────────────

function _renderFeedback(state: GuideState, result: GuideResult | null): string {
  switch (state) {
    case 'testing':
      return `
        <div class="setup-guide-fb setup-guide-fb-testing">
          <span class="ms ms-sm spin">progress_activity</span>
          Testing connection…
        </div>`;
    case 'success':
      return `
        <div class="setup-guide-fb setup-guide-fb-success">
          <span class="ms ms-sm">check_circle</span>
          <div>
            <strong>${escHtml(result?.message ?? 'Connected!')}</strong>
            ${result?.details ? `<div class="setup-guide-fb-details">${escHtml(result.details)}</div>` : ''}
          </div>
        </div>`;
    case 'error':
      return `
        <div class="setup-guide-fb setup-guide-fb-error">
          <span class="ms ms-sm">error</span>
          <div>
            <strong>${escHtml(result?.message ?? 'Connection failed')}</strong>
            ${result?.details ? `<div class="setup-guide-fb-details">${escHtml(result.details)}</div>` : ''}
          </div>
        </div>`;
    default:
      return '';
  }
}

// ── Collect credential values ──────────────────────────────────────────

function _collectCredentials(container: HTMLElement): Record<string, string> {
  const creds: Record<string, string> = {};
  container.querySelectorAll<HTMLInputElement>('.setup-guide-input').forEach((input) => {
    const key = input.dataset.credKey;
    if (key) creds[key] = input.value.trim();
  });
  return creds;
}

function _validateCredentials(container: HTMLElement, fields: CredentialField[]): boolean {
  let valid = true;
  for (const f of fields) {
    if (!f.required) continue;
    const input = container.querySelector<HTMLInputElement>(`#cred-${f.key}`);
    if (!input || !input.value.trim()) {
      input?.classList.add('setup-guide-input-error');
      valid = false;
    } else {
      input?.classList.remove('setup-guide-input-error');
    }
  }
  return valid;
}

// ── Test connection via IPC ────────────────────────────────────────────

async function _testCredentials(
  serviceId: string,
  nodeType: string,
  credentials: Record<string, string>,
): Promise<GuideResult> {
  try {
    const result = await invoke<{ success: boolean; message: string; details?: string }>(
      'engine_integrations_test_credentials',
      { serviceId, nodeType, credentials },
    );
    return result;
  } catch (err) {
    return {
      success: false,
      message: 'Connection test failed',
      details: String(err),
    };
  }
}

// ── Event wiring ───────────────────────────────────────────────────────

function _wireGuideEvents(container: HTMLElement, service: ServiceDefinition): void {
  // Close button
  container.querySelector('#guide-close')?.addEventListener('click', () => {
    _callbacks.onClose();
  });

  // Cancel button
  container.querySelector('#guide-cancel')?.addEventListener('click', () => {
    _callbacks.onClose();
  });

  // Password visibility toggle
  container.querySelectorAll('.setup-guide-toggle-vis').forEach((btn) => {
    btn.addEventListener('click', () => {
      const target = (btn as HTMLElement).dataset.target;
      if (!target) return;
      const input = container.querySelector<HTMLInputElement>(`#${target}`);
      if (!input) return;
      const isNowVisible = input.type === 'password';
      input.type = isNowVisible ? 'text' : 'password';
      const icon = btn.querySelector('.ms');
      if (icon) icon.textContent = isNowVisible ? 'visibility' : 'visibility_off';
    });
  });

  // Clear error on input
  container.querySelectorAll<HTMLInputElement>('.setup-guide-input').forEach((input) => {
    input.addEventListener('input', () => {
      input.classList.remove('setup-guide-input-error');
    });
  });

  // Test & Save
  container.querySelector('#guide-test-save')?.addEventListener('click', async () => {
    if (_guideState === 'testing') return;

    if (!_validateCredentials(container, service.credentialFields)) return;

    const credentials = _collectCredentials(container);
    const feedback = container.querySelector('#guide-feedback') as HTMLElement;
    const btn = container.querySelector('#guide-test-save') as HTMLButtonElement;

    // Show testing state
    _guideState = 'testing';
    if (feedback) {
      feedback.style.display = 'block';
      feedback.innerHTML = _renderFeedback('testing', null);
    }
    if (btn) {
      btn.disabled = true;
      btn.querySelector('.guide-btn-label')!.textContent = 'Testing…';
    }

    // Call backend
    const result = await _testCredentials(service.id, service.n8nNodeType, credentials);

    if (result.success) {
      _guideState = 'success';
      if (feedback) feedback.innerHTML = _renderFeedback('success', result);
      if (btn) {
        btn.querySelector('.guide-btn-label')!.textContent = 'Saved!';
        btn.classList.add('btn-success');
      }

      // ── Wire: connect service + bridge credentials → skill vault ──
      try {
        // Mark service as connected (updates connected list & health monitor)
        await invoke('engine_integrations_connect', {
          serviceId: service.id,
          toolCount: service.capabilities?.length ?? 1,
        });
        // Bridge integration creds → skill vault & auto-enable skill
        await invoke('engine_integrations_provision', {
          serviceId: service.id,
        });
      } catch (e) {
        console.warn('[setup-guide] Post-save wiring:', e);
      }

      // Notify parent
      _callbacks.onSave(service.id, credentials);
    } else {
      _guideState = 'error';
      if (feedback) feedback.innerHTML = _renderFeedback('error', result);
      if (btn) {
        btn.disabled = false;
        btn.querySelector('.guide-btn-label')!.textContent = 'Test & Save';
      }
    }
  });
}
