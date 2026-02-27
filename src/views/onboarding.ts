// Onboarding Wizard — first-run setup flow
//
// Steps:
//   1. Welcome — feature highlights
//   2. Choose Provider — Ollama (recommended) or cloud providers
//   3. API Key — enter key for cloud providers (skipped for Ollama)
//   4. Done — summary + launch

import { pawEngine } from '../engine';
import { showToast } from '../components/toast';
import type { EngineProviderConfig } from '../engine/atoms/types';

const $ = (id: string) => document.getElementById(id);

let _chosenProvider: string | null = null;
const TOTAL_STEPS = 4;

// ── Provider metadata ─────────────────────────────────────────────────

interface ProviderMeta {
  name: string;
  placeholder: string;
  helpHtml: string;
  defaultModel: string;
  baseUrl?: string;
}

const PROVIDER_META: Record<string, ProviderMeta> = {
  openai: {
    name: 'OpenAI',
    placeholder: 'sk-...',
    helpHtml: 'Get your key at <a href="https://platform.openai.com/api-keys" target="_blank" rel="noopener">platform.openai.com</a>',
    defaultModel: 'gpt-4o',
  },
  anthropic: {
    name: 'Anthropic',
    placeholder: 'sk-ant-...',
    helpHtml: 'Get your key at <a href="https://console.anthropic.com/settings/keys" target="_blank" rel="noopener">console.anthropic.com</a>',
    defaultModel: 'claude-sonnet-4-20250514',
  },
  google: {
    name: 'Google',
    placeholder: 'AIza...',
    helpHtml: 'Get your key at <a href="https://aistudio.google.com/apikey" target="_blank" rel="noopener">aistudio.google.com</a>',
    defaultModel: 'gemini-2.5-flash',
  },
  deepseek: {
    name: 'DeepSeek',
    placeholder: 'sk-...',
    helpHtml: 'Get your key at <a href="https://platform.deepseek.com/api_keys" target="_blank" rel="noopener">platform.deepseek.com</a>',
    defaultModel: 'deepseek-chat',
  },
  openrouter: {
    name: 'OpenRouter',
    placeholder: 'sk-or-...',
    helpHtml: 'Get your key at <a href="https://openrouter.ai/keys" target="_blank" rel="noopener">openrouter.ai</a>',
    defaultModel: 'anthropic/claude-sonnet-4-20250514',
    baseUrl: 'https://openrouter.ai/api/v1',
  },
  grok: {
    name: 'Grok',
    placeholder: 'xai-...',
    helpHtml: 'Get your key at <a href="https://console.x.ai" target="_blank" rel="noopener">console.x.ai</a>',
    defaultModel: 'grok-3',
    baseUrl: 'https://api.x.ai/v1',
  },
};

// ── Ollama download URLs per platform ─────────────────────────────────

function getOllamaDownloadUrl(): string {
  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes('mac')) return 'https://ollama.com/download/mac';
  if (ua.includes('win')) return 'https://ollama.com/download/windows';
  return 'https://ollama.com/download/linux';
}

// ── Step navigation ───────────────────────────────────────────────────

function showStep(step: number) {
  // Hide all steps
  for (let i = 1; i <= TOTAL_STEPS; i++) {
    const el = $(`wizard-step-${i}`);
    if (el) el.classList.toggle('wizard-hidden', i !== step);
  }

  // Update progress bar
  const fill = $('wizard-progress-fill');
  if (fill) fill.style.width = `${(step / TOTAL_STEPS) * 100}%`;
}

// ── Ollama detection ──────────────────────────────────────────────────

async function renderOllamaStatus() {
  const statusEl = $('wizard-ollama-status');
  if (!statusEl) return;

  statusEl.innerHTML = '<span class="today-loading">Detecting Ollama…</span>';

  // Check if providers already exist (autoSetup returns "providers_exist" if so)
  try {
    const cfg = await pawEngine.getConfig();
    const hasOllama = cfg.providers?.some(p => p.kind === 'ollama');

    if (hasOllama) {
      statusEl.innerHTML = `
        <div class="wizard-ollama-found">
          <span class="ms ms-sm" style="color: var(--color-success)">check_circle</span>
          <span>Ollama is running</span>
        </div>
        <button class="btn btn-primary" id="wizard-use-ollama">Use Ollama</button>
      `;
      $('wizard-use-ollama')?.addEventListener('click', () => {
        _chosenProvider = 'ollama';
        showStep(4);
        renderSummary('Ollama (local)', cfg.default_model || 'auto-detected');
      });
      return;
    }

    // Try auto-setup (will detect + configure Ollama if running)
    const result = await pawEngine.autoSetup();
    if (result.action === 'ollama_added') {
      statusEl.innerHTML = `
        <div class="wizard-ollama-found">
          <span class="ms ms-sm" style="color: var(--color-success)">check_circle</span>
          <span>Ollama detected — model: ${result.model || 'ready'}</span>
        </div>
        <button class="btn btn-primary" id="wizard-use-ollama">Use Ollama</button>
      `;
      $('wizard-use-ollama')?.addEventListener('click', () => {
        _chosenProvider = 'ollama';
        showStep(4);
        renderSummary('Ollama (local)', result.model || 'auto');
      });
    } else {
      // Ollama not found — show download link
      statusEl.innerHTML = `
        <div class="wizard-ollama-missing">
          <span class="ms ms-sm" style="color: var(--text-muted)">cloud_download</span>
          <span>Ollama not detected</span>
        </div>
        <div class="wizard-ollama-actions">
          <a href="${getOllamaDownloadUrl()}" target="_blank" rel="noopener" class="btn btn-primary" id="wizard-download-ollama">
            Download Ollama
          </a>
          <button class="btn btn-ghost btn-sm" id="wizard-retry-ollama">Retry Detection</button>
        </div>
        <div class="wizard-ollama-hint">Install Ollama, then click "Retry Detection"</div>
      `;
      $('wizard-retry-ollama')?.addEventListener('click', () => renderOllamaStatus());
    }
  } catch (e) {
    console.warn('[wizard] Ollama check failed:', e);
    statusEl.innerHTML = `
      <div class="wizard-ollama-missing">
        <span class="ms ms-sm">cloud_download</span>
        <span>Ollama not detected</span>
      </div>
      <a href="${getOllamaDownloadUrl()}" target="_blank" rel="noopener" class="btn btn-primary">
        Download Ollama
      </a>
      <button class="btn btn-ghost btn-sm" id="wizard-retry-ollama">Retry</button>
    `;
    $('wizard-retry-ollama')?.addEventListener('click', () => renderOllamaStatus());
  }
}

// ── API key step ──────────────────────────────────────────────────────

function showApiKeyStep(providerId: string) {
  const meta = PROVIDER_META[providerId];
  if (!meta) return;

  _chosenProvider = providerId;

  const titleEl = $('wizard-key-title');
  const subtitleEl = $('wizard-key-subtitle');
  const input = $('wizard-api-key') as HTMLInputElement | null;
  const helpEl = $('wizard-key-help');

  if (titleEl) titleEl.textContent = `Connect ${meta.name}`;
  if (subtitleEl) subtitleEl.textContent = 'Paste your API key below. It\'s stored locally and never leaves your machine.';
  if (input) {
    input.placeholder = meta.placeholder;
    input.value = '';
  }
  if (helpEl) helpEl.innerHTML = meta.helpHtml;

  const saveBtn = $('wizard-save-key') as HTMLButtonElement | null;
  if (saveBtn) saveBtn.disabled = true;

  showStep(3);
}

async function saveProviderKey() {
  const input = $('wizard-api-key') as HTMLInputElement | null;
  const apiKey = input?.value.trim();
  if (!apiKey || !_chosenProvider) return;

  const meta = PROVIDER_META[_chosenProvider];
  if (!meta) return;

  const saveBtn = $('wizard-save-key') as HTMLButtonElement | null;
  if (saveBtn) {
    saveBtn.disabled = true;
    saveBtn.textContent = 'Connecting…';
  }

  try {
    const provider: EngineProviderConfig = {
      id: _chosenProvider,
      kind: _chosenProvider as EngineProviderConfig['kind'],
      api_key: apiKey,
      base_url: meta.baseUrl,
      default_model: meta.defaultModel,
    };

    await pawEngine.upsertProvider(provider);

    // Set as default
    const cfg = await pawEngine.getConfig();
    cfg.default_provider = _chosenProvider;
    cfg.default_model = meta.defaultModel;
    await pawEngine.setConfig(cfg);

    showStep(4);
    renderSummary(meta.name, meta.defaultModel);
  } catch (e) {
    console.error('[wizard] Failed to save provider:', e);
    if (saveBtn) {
      saveBtn.disabled = false;
      saveBtn.textContent = 'Connect Provider';
    }
    showToast('Failed to save provider — check your API key', 'error');
  }
}

// ── Summary step ──────────────────────────────────────────────────────

function renderSummary(providerName: string, model: string) {
  const summaryEl = $('wizard-summary');
  const subtitleEl = $('wizard-done-subtitle');
  if (summaryEl) {
    summaryEl.innerHTML = `
      <div class="wizard-summary-item">
        <span class="ms ms-sm">hub</span>
        <span><strong>Provider:</strong> ${providerName}</span>
      </div>
      <div class="wizard-summary-item">
        <span class="ms ms-sm">psychology</span>
        <span><strong>Model:</strong> ${model}</span>
      </div>
      <div class="wizard-summary-item">
        <span class="ms ms-sm">tips_and_updates</span>
        <span>You can add more providers and change models anytime in Settings.</span>
      </div>
    `;
  }
  if (subtitleEl) {
    subtitleEl.textContent = 'OpenPawz is configured and ready to go.';
  }
}

// ── Finish + complete onboarding ──────────────────────────────────────

async function finishWizard() {
  try {
    await pawEngine.setOnboardingComplete();
  } catch (e) {
    console.warn('[wizard] Failed to mark onboarding complete:', e);
  }
  // Dispatch event so main.ts can continue startup
  window.dispatchEvent(new CustomEvent('wizard-complete'));
}

// ── Public API ────────────────────────────────────────────────────────

/** Check if the onboarding wizard should be shown. */
export async function shouldShowWizard(): Promise<boolean> {
  try {
    const complete = await pawEngine.isOnboardingComplete();
    if (complete) return false;

    // Also check if providers already exist (manual setup or auto-setup)
    const cfg = await pawEngine.getConfig();
    if (cfg.providers && cfg.providers.length > 0) {
      // User already has providers — mark onboarding complete and skip
      await pawEngine.setOnboardingComplete();
      return false;
    }

    return true;
  } catch {
    // If engine isn't ready, don't show wizard
    return false;
  }
}

/** Initialize and show the wizard. */
export function initWizard() {
  showStep(1);

  // Step 1: Get Started
  $('wizard-start')?.addEventListener('click', () => {
    showStep(2);
    renderOllamaStatus();
  });

  // Step 2: Back
  $('wizard-back-2')?.addEventListener('click', () => showStep(1));

  // Step 2: Cloud provider buttons
  document.querySelectorAll('.wizard-provider-btn').forEach(btn => {
    btn.addEventListener('click', () => {
      const providerId = (btn as HTMLElement).dataset.provider;
      if (providerId) showApiKeyStep(providerId);
    });
  });

  // Step 3: Back
  $('wizard-back-3')?.addEventListener('click', () => {
    showStep(2);
    renderOllamaStatus();
  });

  // Step 3: API key input enable/disable save button
  const apiInput = $('wizard-api-key') as HTMLInputElement | null;
  const saveBtn = $('wizard-save-key') as HTMLButtonElement | null;
  apiInput?.addEventListener('input', () => {
    if (saveBtn) saveBtn.disabled = !apiInput.value.trim();
  });
  apiInput?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && apiInput.value.trim()) saveProviderKey();
  });

  // Step 3: Save
  $('wizard-save-key')?.addEventListener('click', () => saveProviderKey());

  // Step 4: Finish
  $('wizard-finish')?.addEventListener('click', () => finishWizard());
}
