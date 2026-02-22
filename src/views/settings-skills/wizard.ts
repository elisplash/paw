// Settings Skills — Wizard (Phase F.5: Skill creation wizard)
// Step-by-step form for generating pawz-skill.toml manifests.

import { pawEngine, type WizardFormData } from '../../engine';
import { $ } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { msIcon } from './atoms';

// ── State ──────────────────────────────────────────────────────────────

let _step = 0;
let _reloadFn: (() => Promise<void>) | null = null;

const STEPS = ['Basic Info', 'Credentials', 'Instructions', 'Widget', 'MCP Server', 'Review'];

const CATEGORIES = [
  'api',
  'cli',
  'communication',
  'development',
  'media',
  'productivity',
  'smart_home',
  'system',
  'vault',
];

const WIDGET_TYPES = ['status', 'metric', 'table', 'log', 'kv'];
const FIELD_TYPES = ['text', 'number', 'badge', 'datetime', 'percentage', 'currency'];

export function setWizardReload(fn: () => Promise<void>): void {
  _reloadFn = fn;
}

// ── Section renderer ───────────────────────────────────────────────────

export function renderWizardSection(): string {
  return `
  <div class="wizard-hero" style="background:linear-gradient(135deg, var(--bg-surface) 0%, color-mix(in srgb, #22c55e 8%, var(--bg-surface)) 100%);border:1px solid var(--border-subtle);border-radius:12px;padding:24px 28px;margin-bottom:24px">
    <div style="display:flex;align-items:center;gap:10px;margin-bottom:6px">
      <span style="font-size:28px">${msIcon('add_circle', 'ms-lg')}</span>
      <h2 style="margin:0;font-size:20px;font-weight:700;letter-spacing:-0.02em">Create a Skill</h2>
      <span style="font-size:11px;color:#22c55e;padding:2px 8px;border:1px solid #22c55e;border-radius:12px">Wizard</span>
    </div>
    <p style="color:var(--text-muted);font-size:13px;margin:0 0 16px;max-width:600px">
      Build a new skill with a step-by-step wizard. Generate a <code>pawz-skill.toml</code> manifest,
      install it locally, or publish to PawzHub.
    </p>
    <button class="btn btn-primary" id="wizard-open-btn" style="padding:10px 20px;border-radius:10px;font-size:14px">
      ${msIcon('add')} New Skill
    </button>
    <div id="wizard-container" style="display:none;margin-top:20px"></div>
  </div>`;
}

// ── Wizard stepper + content ───────────────────────────────────────────

function renderStepIndicator(): string {
  return `<div class="wizard-steps" style="display:flex;gap:4px;margin-bottom:20px">
    ${STEPS.map(
      (s, i) =>
        `<div class="wizard-step-indicator${i === _step ? ' wizard-step-active' : ''}${i < _step ? ' wizard-step-done' : ''}" style="flex:1;text-align:center;padding:6px 0;font-size:11px;border-bottom:2px solid ${i === _step ? 'var(--accent)' : i < _step ? '#22c55e' : 'var(--border-subtle)'};color:${i === _step ? 'var(--accent)' : i < _step ? '#22c55e' : 'var(--text-muted)'}">
          ${i < _step ? `${msIcon('check_circle')} ` : ''}${s}
        </div>`,
    ).join('')}
  </div>`;
}

function renderStep0(): string {
  const catOptions = CATEGORIES.map(
    (c) => `<option value="${c}">${c.replace('_', ' ')}</option>`,
  ).join('');
  return `
  <div class="wizard-step-content">
    <h3 style="margin:0 0 12px;font-size:15px">${msIcon('edit')} Basic Information</h3>
    <div style="display:grid;grid-template-columns:1fr 1fr;gap:12px">
      <label class="wizard-field">
        <span>Skill ID *</span>
        <input type="text" class="form-input" id="wiz-id" placeholder="my-skill" pattern="[a-zA-Z0-9_-]+" />
      </label>
      <label class="wizard-field">
        <span>Name *</span>
        <input type="text" class="form-input" id="wiz-name" placeholder="My Skill" />
      </label>
      <label class="wizard-field">
        <span>Version *</span>
        <input type="text" class="form-input" id="wiz-version" value="1.0.0" />
      </label>
      <label class="wizard-field">
        <span>Author *</span>
        <input type="text" class="form-input" id="wiz-author" placeholder="your-username" />
      </label>
      <label class="wizard-field">
        <span>Category</span>
        <select class="form-input" id="wiz-category">${catOptions}</select>
      </label>
      <label class="wizard-field">
        <span>Icon (Material Symbol)</span>
        <input type="text" class="form-input" id="wiz-icon" placeholder="extension" />
      </label>
    </div>
    <label class="wizard-field" style="margin-top:12px">
      <span>Description * <small style="color:var(--text-muted)">(max 500 chars)</small></span>
      <textarea class="form-input" id="wiz-description" rows="3" maxlength="500" placeholder="What does this skill do?"></textarea>
    </label>
    <label class="wizard-field" style="margin-top:8px">
      <span>Install Hint <small style="color:var(--text-muted)">(optional)</small></span>
      <input type="text" class="form-input" id="wiz-install-hint" placeholder="Get your API key at..." />
    </label>
  </div>`;
}

function renderStep1(): string {
  return `
  <div class="wizard-step-content">
    <h3 style="margin:0 0 12px;font-size:15px">${msIcon('key')} Credentials</h3>
    <p style="color:var(--text-muted);font-size:12px;margin:0 0 12px">
      Add API keys or tokens this skill needs. Skills with credentials are "Integrations" (Tier 2).
    </p>
    <div id="wiz-credentials-list"></div>
    <button class="btn btn-ghost btn-sm" id="wiz-add-credential" style="margin-top:8px">
      ${msIcon('add')} Add Credential
    </button>
  </div>`;
}

function renderCredentialRow(index: number): string {
  return `
  <div class="wizard-credential-row" data-index="${index}" style="border:1px solid var(--border-subtle);border-radius:8px;padding:12px;margin-bottom:8px">
    <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:8px">
      <span style="font-size:12px;font-weight:600">Credential #${index + 1}</span>
      <button class="btn btn-ghost btn-sm wiz-remove-credential" data-index="${index}" style="color:var(--accent-danger);font-size:11px">${msIcon('delete')} Remove</button>
    </div>
    <div style="display:grid;grid-template-columns:1fr 1fr;gap:8px">
      <label class="wizard-field">
        <span>Key *</span>
        <input type="text" class="form-input wiz-cred-key" placeholder="API_KEY" data-index="${index}" />
      </label>
      <label class="wizard-field">
        <span>Label *</span>
        <input type="text" class="form-input wiz-cred-label" placeholder="API Key" data-index="${index}" />
      </label>
      <label class="wizard-field">
        <span>Description</span>
        <input type="text" class="form-input wiz-cred-desc" placeholder="Your API key from..." data-index="${index}" />
      </label>
      <label class="wizard-field">
        <span>Placeholder</span>
        <input type="text" class="form-input wiz-cred-placeholder" placeholder="sk-..." data-index="${index}" />
      </label>
    </div>
    <label style="display:flex;align-items:center;gap:6px;margin-top:6px;font-size:12px">
      <input type="checkbox" class="wiz-cred-required" data-index="${index}" checked /> Required
    </label>
  </div>`;
}

function renderStep2(): string {
  return `
  <div class="wizard-step-content">
    <h3 style="margin:0 0 12px;font-size:15px">${msIcon('description')} Instructions</h3>
    <p style="color:var(--text-muted);font-size:12px;margin:0 0 12px">
      System prompt text injected when this skill is enabled. Tell the agent what tools are available and how to use them.
    </p>
    <textarea class="form-input" id="wiz-instructions" rows="8" placeholder="You have access to the Notion API via these tools..."></textarea>
  </div>`;
}

function renderStep3(): string {
  const typeOptions = WIDGET_TYPES.map((t) => `<option value="${t}">${t}</option>`).join('');
  return `
  <div class="wizard-step-content">
    <h3 style="margin:0 0 12px;font-size:15px">${msIcon('dashboard')} Dashboard Widget</h3>
    <p style="color:var(--text-muted);font-size:12px;margin:0 0 12px">
      Optional. Define a widget card that appears on the Today dashboard.
    </p>
    <label style="display:flex;align-items:center;gap:6px;margin-bottom:12px;font-size:13px">
      <input type="checkbox" id="wiz-widget-enable" /> Enable dashboard widget
    </label>
    <div id="wiz-widget-config" style="display:none">
      <div style="display:grid;grid-template-columns:1fr 1fr 1fr;gap:12px">
        <label class="wizard-field">
          <span>Widget Type</span>
          <select class="form-input" id="wiz-widget-type">${typeOptions}</select>
        </label>
        <label class="wizard-field">
          <span>Title</span>
          <input type="text" class="form-input" id="wiz-widget-title" placeholder="Widget Title" />
        </label>
        <label class="wizard-field">
          <span>Refresh Interval</span>
          <input type="text" class="form-input" id="wiz-widget-refresh" placeholder="10m" />
        </label>
      </div>
      <div style="margin-top:12px">
        <span style="font-size:12px;font-weight:600">Fields</span>
        <div id="wiz-widget-fields-list" style="margin-top:6px"></div>
        <button class="btn btn-ghost btn-sm" id="wiz-add-widget-field" style="margin-top:6px">
          ${msIcon('add')} Add Field
        </button>
      </div>
    </div>
  </div>`;
}

function renderWidgetFieldRow(index: number): string {
  const fieldTypeOptions = FIELD_TYPES.map((t) => `<option value="${t}">${t}</option>`).join('');
  return `
  <div class="wizard-widget-field-row" data-index="${index}" style="display:flex;gap:8px;align-items:end;margin-bottom:6px">
    <label class="wizard-field" style="flex:1">
      <span>Key</span>
      <input type="text" class="form-input wiz-wf-key" placeholder="field_key" data-index="${index}" />
    </label>
    <label class="wizard-field" style="flex:1">
      <span>Label</span>
      <input type="text" class="form-input wiz-wf-label" placeholder="Field Label" data-index="${index}" />
    </label>
    <label class="wizard-field" style="flex:1">
      <span>Type</span>
      <select class="form-input wiz-wf-type" data-index="${index}">${fieldTypeOptions}</select>
    </label>
    <button class="btn btn-ghost btn-sm wiz-remove-widget-field" data-index="${index}" style="color:var(--accent-danger);margin-bottom:2px">${msIcon('delete')}</button>
  </div>`;
}

function renderStep4(): string {
  return `
  <div class="wizard-step-content">
    <h3 style="margin:0 0 12px;font-size:15px">${msIcon('dns')} MCP Server</h3>
    <p style="color:var(--text-muted);font-size:12px;margin:0 0 12px">
      Optional. Bundle an MCP server that auto-registers on install. Credentials are injected as environment variables.
    </p>
    <label style="display:flex;align-items:center;gap:6px;margin-bottom:12px;font-size:13px">
      <input type="checkbox" id="wiz-mcp-enable" /> Enable MCP server
    </label>
    <div id="wiz-mcp-config" style="display:none">
      <div style="display:grid;grid-template-columns:1fr 1fr;gap:12px">
        <label class="wizard-field">
          <span>Command</span>
          <input type="text" class="form-input" id="wiz-mcp-command" placeholder="npx" />
        </label>
        <label class="wizard-field">
          <span>Transport</span>
          <select class="form-input" id="wiz-mcp-transport">
            <option value="stdio" selected>stdio</option>
            <option value="sse">SSE</option>
          </select>
        </label>
        <label class="wizard-field" style="grid-column:span 2">
          <span>Args <small style="color:var(--text-muted)">(comma-separated)</small></span>
          <input type="text" class="form-input" id="wiz-mcp-args" placeholder="-y, @modelcontextprotocol/server-github" />
        </label>
        <label class="wizard-field" style="grid-column:span 2">
          <span>URL <small style="color:var(--text-muted)">(for SSE transport only)</small></span>
          <input type="text" class="form-input" id="wiz-mcp-url" placeholder="http://localhost:3000/sse" />
        </label>
      </div>
    </div>
  </div>`;
}

function renderStep5(): string {
  return `
  <div class="wizard-step-content">
    <h3 style="margin:0 0 12px;font-size:15px">${msIcon('preview')} Review & Generate</h3>
    <p style="color:var(--text-muted);font-size:12px;margin:0 0 12px">
      Preview the generated TOML manifest. You can install it locally or publish to PawzHub.
    </p>
    <div style="display:flex;gap:8px;margin-bottom:12px">
      <button class="btn btn-primary" id="wiz-generate-btn">${msIcon('code')} Generate TOML</button>
    </div>
    <div id="wiz-preview" style="display:none">
      <pre id="wiz-toml-output" style="background:var(--bg-surface);border:1px solid var(--border-subtle);border-radius:8px;padding:12px;font-size:12px;max-height:400px;overflow:auto;white-space:pre-wrap;margin:0 0 12px"></pre>
      <div style="display:flex;gap:8px">
        <button class="btn btn-primary" id="wiz-install-btn">${msIcon('download')} Install Locally</button>
        <button class="btn btn-ghost" id="wiz-publish-btn">${msIcon('cloud_upload')} Publish to PawzHub</button>
        <button class="btn btn-ghost" id="wiz-copy-btn">${msIcon('content_copy')} Copy TOML</button>
      </div>
    </div>
  </div>`;
}

// ── Collect form data ──────────────────────────────────────────────────

let _credentialCount = 0;
let _widgetFieldCount = 0;

function collectFormData(): WizardFormData {
  const val = (id: string) => ($(id) as HTMLInputElement)?.value?.trim() ?? '';
  const checked = (id: string) => ($(id) as HTMLInputElement)?.checked ?? false;

  const credentials = [];
  for (let i = 0; i < _credentialCount; i++) {
    const row = document.querySelector(`.wizard-credential-row[data-index="${i}"]`);
    if (!row) continue;
    const key = (row.querySelector('.wiz-cred-key') as HTMLInputElement)?.value?.trim() ?? '';
    const label = (row.querySelector('.wiz-cred-label') as HTMLInputElement)?.value?.trim() ?? '';
    if (!key || !label) continue;
    credentials.push({
      key,
      label,
      description: (row.querySelector('.wiz-cred-desc') as HTMLInputElement)?.value?.trim() ?? '',
      required: (row.querySelector('.wiz-cred-required') as HTMLInputElement)?.checked ?? false,
      placeholder:
        (row.querySelector('.wiz-cred-placeholder') as HTMLInputElement)?.value?.trim() ?? '',
    });
  }

  let widget: WizardFormData['widget'] = null;
  if (checked('wiz-widget-enable')) {
    const fields = [];
    for (let i = 0; i < _widgetFieldCount; i++) {
      const row = document.querySelector(`.wizard-widget-field-row[data-index="${i}"]`);
      if (!row) continue;
      const key = (row.querySelector('.wiz-wf-key') as HTMLInputElement)?.value?.trim() ?? '';
      const label = (row.querySelector('.wiz-wf-label') as HTMLInputElement)?.value?.trim() ?? '';
      const ft = (row.querySelector('.wiz-wf-type') as HTMLSelectElement)?.value ?? 'text';
      if (key && label) fields.push({ key, label, field_type: ft });
    }
    widget = {
      widget_type: val('wiz-widget-type') || 'status',
      title: val('wiz-widget-title'),
      refresh: val('wiz-widget-refresh'),
      fields,
    };
  }

  let mcp: WizardFormData['mcp'] = null;
  if (checked('wiz-mcp-enable')) {
    const argsRaw = val('wiz-mcp-args');
    const args = argsRaw
      ? argsRaw
          .split(',')
          .map((a) => a.trim())
          .filter(Boolean)
      : [];
    mcp = {
      command: val('wiz-mcp-command'),
      args,
      transport: val('wiz-mcp-transport') || 'stdio',
      url: val('wiz-mcp-url'),
    };
  }

  return {
    id: val('wiz-id'),
    name: val('wiz-name'),
    version: val('wiz-version') || '1.0.0',
    author: val('wiz-author'),
    category: val('wiz-category') || 'api',
    icon: val('wiz-icon'),
    description: val('wiz-description'),
    install_hint: val('wiz-install-hint'),
    instructions: val('wiz-instructions'),
    credentials,
    widget,
    mcp,
  };
}

// ── Wizard rendering orchestration ─────────────────────────────────────

function renderCurrentStep(): string {
  const stepFns = [renderStep0, renderStep1, renderStep2, renderStep3, renderStep4, renderStep5];
  const nav = `<div style="display:flex;justify-content:space-between;align-items:center;margin-top:16px">
    <button class="btn btn-ghost btn-sm" id="wiz-prev" ${_step === 0 ? 'disabled' : ''}>${msIcon('arrow_back')} Previous</button>
    ${
      _step < STEPS.length - 1
        ? `<button class="btn btn-primary btn-sm" id="wiz-next">${msIcon('arrow_forward')} Next</button>`
        : ''
    }
  </div>`;
  return renderStepIndicator() + stepFns[_step]() + nav;
}

function renderWizard(): void {
  const container = $('wizard-container');
  if (!container) return;
  container.style.display = 'block';
  container.innerHTML = renderCurrentStep();
  bindStepEvents();
}

// ── Step-specific event binding ────────────────────────────────────────

function bindStepEvents(): void {
  // Navigation
  $('wiz-prev')?.addEventListener('click', () => {
    if (_step > 0) {
      _step--;
      renderWizard();
    }
  });
  $('wiz-next')?.addEventListener('click', () => {
    if (_step < STEPS.length - 1) {
      _step++;
      renderWizard();
    }
  });

  // Step 1: Credentials
  if (_step === 1) {
    $('wiz-add-credential')?.addEventListener('click', () => {
      const list = $('wiz-credentials-list');
      if (!list) return;
      list.insertAdjacentHTML('beforeend', renderCredentialRow(_credentialCount));
      _credentialCount++;
      bindCredentialRemoveButtons();
    });
    bindCredentialRemoveButtons();
  }

  // Step 3: Widget
  if (_step === 3) {
    const enableCb = $('wiz-widget-enable') as HTMLInputElement;
    const config = $('wiz-widget-config');
    enableCb?.addEventListener('change', () => {
      if (config) config.style.display = enableCb.checked ? '' : 'none';
    });
    $('wiz-add-widget-field')?.addEventListener('click', () => {
      const list = $('wiz-widget-fields-list');
      if (!list) return;
      list.insertAdjacentHTML('beforeend', renderWidgetFieldRow(_widgetFieldCount));
      _widgetFieldCount++;
      bindWidgetFieldRemoveButtons();
    });
    bindWidgetFieldRemoveButtons();
  }

  // Step 4: MCP
  if (_step === 4) {
    const enableCb = $('wiz-mcp-enable') as HTMLInputElement;
    const config = $('wiz-mcp-config');
    enableCb?.addEventListener('change', () => {
      if (config) config.style.display = enableCb.checked ? '' : 'none';
    });
  }

  // Step 5: Review
  if (_step === 5) {
    $('wiz-generate-btn')?.addEventListener('click', async () => {
      const form = collectFormData();
      try {
        const toml = await pawEngine.wizardGenerateToml(form);
        const preview = $('wiz-preview');
        const output = $('wiz-toml-output');
        if (preview) preview.style.display = '';
        if (output) output.textContent = toml;
      } catch (err) {
        showToast(`Generation failed: ${err}`, 'error');
      }
    });

    $('wiz-install-btn')?.addEventListener('click', async () => {
      const form = collectFormData();
      try {
        const toml = await pawEngine.wizardGenerateToml(form);
        await pawEngine.tomlSkillInstall(form.id, toml);
        showToast(`${form.name} installed!`, 'success');
        // Reset wizard
        _step = 0;
        _credentialCount = 0;
        _widgetFieldCount = 0;
        const container = $('wizard-container');
        if (container) container.style.display = 'none';
        if (_reloadFn) await _reloadFn();
      } catch (err) {
        showToast(`Install failed: ${err}`, 'error');
      }
    });

    $('wiz-publish-btn')?.addEventListener('click', async () => {
      const form = collectFormData();
      try {
        const toml = await pawEngine.wizardGenerateToml(form);
        const url = await pawEngine.wizardPublishUrl(form.id, toml);
        window.open(url, '_blank');
      } catch (err) {
        showToast(`Publish failed: ${err}`, 'error');
      }
    });

    $('wiz-copy-btn')?.addEventListener('click', () => {
      const output = $('wiz-toml-output');
      if (output?.textContent) {
        navigator.clipboard.writeText(output.textContent);
        showToast('TOML copied to clipboard', 'success');
      }
    });
  }
}

function bindCredentialRemoveButtons(): void {
  document.querySelectorAll('.wiz-remove-credential').forEach((el) => {
    el.addEventListener('click', () => {
      (el as HTMLElement).closest('.wizard-credential-row')?.remove();
    });
  });
}

function bindWidgetFieldRemoveButtons(): void {
  document.querySelectorAll('.wiz-remove-widget-field').forEach((el) => {
    el.addEventListener('click', () => {
      (el as HTMLElement).closest('.wizard-widget-field-row')?.remove();
    });
  });
}

// ── Public event binding ───────────────────────────────────────────────

export function bindWizardEvents(): void {
  $('wizard-open-btn')?.addEventListener('click', () => {
    _step = 0;
    _credentialCount = 0;
    _widgetFieldCount = 0;
    renderWizard();
  });
}
