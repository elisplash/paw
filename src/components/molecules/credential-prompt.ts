// src/components/molecules/credential-prompt.ts — Reusable credential modal
//
// Phase 3: Can be triggered from chat, integrations view, or any context.
// Shows service info, credential fields, inline setup steps, test & save.

import { invoke } from '@tauri-apps/api/core';
import { escHtml } from './markdown';

// ── Types ──────────────────────────────────────────────────────────────

export interface CredentialRequest {
  service: string;
  serviceName: string;
  serviceIcon: string;
  serviceColor: string;
  fields: CredentialPromptField[];
  helpUrl?: string;
  helpSteps?: string[];
  /** If set, auto-retry this action after credentials are saved. */
  retryAction?: string;
  retryPayload?: Record<string, unknown>;
}

export interface CredentialPromptField {
  key: string;
  label: string;
  type: 'password' | 'url' | 'text';
  placeholder?: string;
  required: boolean;
}

export interface CredentialPromptResult {
  saved: boolean;
  serviceId: string;
  credentials?: Record<string, string>;
}

type PromptState = 'idle' | 'testing' | 'success' | 'error';

// ── Module state ───────────────────────────────────────────────────────

let _activeRequest: CredentialRequest | null = null;
let _state: PromptState = 'idle';
let _errorMessage = '';
let _successDetails = '';
let _resolve: ((result: CredentialPromptResult) => void) | null = null;

// ── Public API ─────────────────────────────────────────────────────────

/**
 * Show the credential prompt modal. Returns a promise that resolves
 * when the user saves or dismisses the modal.
 */
export function showCredentialPrompt(request: CredentialRequest): Promise<CredentialPromptResult> {
  return new Promise((resolve) => {
    _activeRequest = request;
    _state = 'idle';
    _errorMessage = '';
    _successDetails = '';
    _resolve = resolve;
    _render();
  });
}

/**
 * Build a CredentialRequest from a ServiceDefinition-like object.
 * Convenience for views that already have the service data.
 */
export function buildCredentialRequest(service: {
  id: string;
  name: string;
  icon: string;
  color: string;
  credentialFields: {
    key: string;
    label: string;
    type: string;
    placeholder?: string;
    required: boolean;
  }[];
  setupGuide?: { steps: { instruction: string; link?: string; tip?: string }[] };
  docsUrl?: string;
}): CredentialRequest {
  return {
    service: service.id,
    serviceName: service.name,
    serviceIcon: service.icon,
    serviceColor: service.color,
    fields: service.credentialFields.map((f) => ({
      key: f.key,
      label: f.label,
      type: f.type as 'password' | 'url' | 'text',
      placeholder: f.placeholder,
      required: f.required,
    })),
    helpUrl: service.docsUrl,
    helpSteps: service.setupGuide?.steps.map((s) => s.instruction),
  };
}

// ── Render ──────────────────────────────────────────────────────────────

function _render(): void {
  if (!_activeRequest) return;
  const req = _activeRequest;

  // Remove existing modal if any
  _removeModal();

  const overlay = document.createElement('div');
  overlay.id = 'credential-prompt-overlay';
  overlay.className = 'credential-prompt-overlay';
  overlay.innerHTML = `
    <div class="credential-prompt-modal">
      <div class="credential-prompt-header">
        <button class="btn btn-ghost btn-sm credential-prompt-close" id="cred-prompt-close">
          <span class="ms">close</span>
        </button>
        <div class="credential-prompt-icon" style="background: ${req.serviceColor}15; color: ${req.serviceColor}">
          <span class="ms ms-lg">${req.serviceIcon}</span>
        </div>
        <h2>Connect ${escHtml(req.serviceName)}</h2>
        <p class="credential-prompt-desc">Enter your credentials to connect this service to your agent.</p>
      </div>

      ${
        req.helpSteps && req.helpSteps.length > 0
          ? `
        <div class="credential-prompt-steps">
          <h4><span class="ms ms-sm">menu_book</span> Setup Steps</h4>
          <ol>
            ${req.helpSteps.map((s) => `<li>${escHtml(s)}</li>`).join('')}
          </ol>
          ${
            req.helpUrl
              ? `<a href="${escHtml(req.helpUrl)}" target="_blank" rel="noopener" class="credential-prompt-docs">
            <span class="ms ms-sm">open_in_new</span> Full documentation
          </a>`
              : ''
          }
        </div>
      `
          : ''
      }

      <div class="credential-prompt-fields" id="cred-prompt-fields">
        ${req.fields
          .map(
            (f) => `
          <div class="credential-prompt-field">
            <label for="cred-field-${f.key}">${escHtml(f.label)}${f.required ? ' *' : ''}</label>
            <div class="credential-prompt-input-wrap">
              <input
                type="${f.type === 'password' ? 'password' : 'text'}"
                id="cred-field-${f.key}"
                data-key="${f.key}"
                class="input credential-prompt-input"
                placeholder="${escHtml(f.placeholder ?? '')}"
                ${f.required ? 'required' : ''}
                autocomplete="off"
              />
              ${
                f.type === 'password'
                  ? `
                <button class="btn btn-ghost btn-xs cred-toggle-vis" data-field="${f.key}" title="Toggle visibility">
                  <span class="ms ms-sm">visibility</span>
                </button>
              `
                  : ''
              }
            </div>
          </div>
        `,
          )
          .join('')}
      </div>

      <div class="credential-prompt-status" id="cred-prompt-status">
        ${
          _state === 'testing'
            ? `
          <div class="credential-prompt-testing">
            <span class="ms ms-sm spin">progress_activity</span> Testing connection…
          </div>
        `
            : _state === 'success'
              ? `
          <div class="credential-prompt-success">
            <span class="ms ms-sm">check_circle</span> Connected!
            ${_successDetails ? `<span class="credential-prompt-details">${escHtml(_successDetails)}</span>` : ''}
          </div>
        `
              : _state === 'error'
                ? `
          <div class="credential-prompt-error">
            <span class="ms ms-sm">error</span> ${escHtml(_errorMessage || 'Connection failed')}
          </div>
        `
                : ''
        }
      </div>

      <div class="credential-prompt-actions">
        <button class="btn btn-ghost" id="cred-prompt-cancel">Cancel</button>
        <button class="btn btn-primary" id="cred-prompt-test"
                ${_state === 'testing' ? 'disabled' : ''}>
          <span class="ms ms-sm">${_state === 'success' ? 'check' : 'science'}</span>
          ${_state === 'success' ? 'Save & Close' : 'Test & Save'}
        </button>
      </div>
    </div>
  `;

  document.body.appendChild(overlay);
  _wireEvents(overlay, req);
}

// ── Events ─────────────────────────────────────────────────────────────

function _wireEvents(overlay: HTMLElement, req: CredentialRequest): void {
  // Close / cancel
  const close = () => {
    _removeModal();
    _resolve?.({ saved: false, serviceId: req.service });
    _resolve = null;
  };

  overlay.querySelector('#cred-prompt-close')?.addEventListener('click', close);
  overlay.querySelector('#cred-prompt-cancel')?.addEventListener('click', close);
  overlay.addEventListener('click', (e) => {
    if (e.target === overlay) close();
  });

  // Password visibility toggles
  overlay.querySelectorAll('.cred-toggle-vis').forEach((btn) => {
    btn.addEventListener('click', () => {
      const key = (btn as HTMLElement).dataset.field;
      const input = overlay.querySelector(`#cred-field-${key}`) as HTMLInputElement;
      if (!input) return;
      const isPassword = input.type === 'password';
      input.type = isPassword ? 'text' : 'password';
      const icon = btn.querySelector('.ms');
      if (icon) icon.textContent = isPassword ? 'visibility_off' : 'visibility';
    });
  });

  // Test & Save
  overlay.querySelector('#cred-prompt-test')?.addEventListener('click', async () => {
    if (_state === 'success') {
      // Already tested — just close with success
      const creds = _gatherCredentials(overlay, req);
      _removeModal();
      _resolve?.({ saved: true, serviceId: req.service, credentials: creds });
      _resolve = null;
      return;
    }

    // Validate required fields
    const creds = _gatherCredentials(overlay, req);
    const missing = req.fields.filter((f) => f.required && !creds[f.key]?.trim());
    if (missing.length > 0) {
      _state = 'error';
      _errorMessage = `Missing required: ${missing.map((f) => f.label).join(', ')}`;
      _updateStatus(overlay);
      return;
    }

    // Test connection
    _state = 'testing';
    _updateStatus(overlay);

    try {
      const result = await invoke<{ success: boolean; message: string; details?: string }>(
        'engine_integrations_test_credentials',
        { serviceId: req.service, credentials: creds },
      );

      if (result.success) {
        // Save credentials
        await invoke('engine_integrations_save_credentials', {
          serviceId: req.service,
          credentials: creds,
        });
        _state = 'success';
        _successDetails = result.details ?? result.message;
      } else {
        _state = 'error';
        _errorMessage = result.message;
      }
    } catch (err) {
      _state = 'error';
      _errorMessage = String(err);
    }

    _updateStatus(overlay);
  });
}

// ── Helpers ────────────────────────────────────────────────────────────

function _gatherCredentials(overlay: HTMLElement, req: CredentialRequest): Record<string, string> {
  const creds: Record<string, string> = {};
  for (const f of req.fields) {
    const input = overlay.querySelector(`#cred-field-${f.key}`) as HTMLInputElement;
    if (input) creds[f.key] = input.value;
  }
  return creds;
}

function _updateStatus(overlay: HTMLElement): void {
  const statusEl = overlay.querySelector('#cred-prompt-status');
  const testBtn = overlay.querySelector('#cred-prompt-test') as HTMLButtonElement;
  if (!statusEl) return;

  statusEl.innerHTML =
    _state === 'testing'
      ? `<div class="credential-prompt-testing">
        <span class="ms ms-sm spin">progress_activity</span> Testing connection…
      </div>`
      : _state === 'success'
        ? `<div class="credential-prompt-success">
          <span class="ms ms-sm">check_circle</span> Connected!
          ${_successDetails ? `<span class="credential-prompt-details">${escHtml(_successDetails)}</span>` : ''}
        </div>`
        : _state === 'error'
          ? `<div class="credential-prompt-error">
            <span class="ms ms-sm">error</span> ${escHtml(_errorMessage || 'Connection failed')}
          </div>`
          : '';

  if (testBtn) {
    testBtn.disabled = _state === 'testing';
    const icon = testBtn.querySelector('.ms');
    if (icon) icon.textContent = _state === 'success' ? 'check' : 'science';
    testBtn.childNodes.forEach((n) => {
      if (n.nodeType === Node.TEXT_NODE) {
        n.textContent = _state === 'success' ? ' Save & Close' : ' Test & Save';
      }
    });
  }
}

function _removeModal(): void {
  document.getElementById('credential-prompt-overlay')?.remove();
  _activeRequest = null;
  _state = 'idle';
}
