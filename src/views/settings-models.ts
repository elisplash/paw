// Settings: Models & Providers
// CRUD for AI model providers + model definitions + default model selection
// ~250 lines — focused on models.providers config path

import { gateway } from '../gateway';
import { showToast } from '../components/toast';
import {
  getConfig, patchConfig, getVal, isConnected,
  esc, formRow, selectInput, textInput, saveReloadButtons
} from './settings-config';

const $ = (id: string) => document.getElementById(id);

// ── API Types ──────────────────────────────────────────────────────────────

const API_TYPES = [
  { value: 'openai-completions', label: 'OpenAI Completions' },
  { value: 'openai-responses', label: 'OpenAI Responses' },
  { value: 'anthropic-messages', label: 'Anthropic Messages' },
  { value: 'google-generative-ai', label: 'Google Generative AI' },
  { value: 'github-copilot', label: 'GitHub Copilot' },
  { value: 'bedrock-converse-stream', label: 'AWS Bedrock' },
  { value: 'ollama', label: 'Ollama' },
];

// ── Render ──────────────────────────────────────────────────────────────────

export async function loadModelsSettings() {
  if (!isConnected()) return;
  const container = $('settings-models-content');
  if (!container) return;
  container.innerHTML = '<p style="color:var(--text-muted)">Loading…</p>';

  try {
    const config = await getConfig();
    const providers = (getVal(config, 'models.providers') ?? {}) as Record<string, any>;
    const defaultModel = getVal(config, 'agents.defaults.model.primary') as string | undefined;
    const fallbacks = (getVal(config, 'agents.defaults.model.fallbacks') ?? []) as string[];

    // Also fetch resolved models for the dropdown
    let modelChoices: Array<{ id: string; name?: string }> = [];
    try {
      const res = await gateway.modelsList();
      modelChoices = (res.models ?? []) as Array<{ id: string; name?: string }>;
    } catch { /* offline — use provider models instead */ }

    container.innerHTML = '';

    // ── Default Model Selection ──────────────────────────────────────────
    const defaultSection = document.createElement('div');
    defaultSection.className = 'settings-subsection';
    defaultSection.innerHTML = `<h3 class="settings-subsection-title">Default Model</h3>
      <p class="settings-section-desc">Primary model used for all conversations unless overridden per-agent.</p>`;

    const modelOpts = modelChoices.map(m => ({ value: m.id, label: m.name ?? m.id }));
    if (defaultModel && !modelOpts.find(o => o.value === defaultModel)) {
      modelOpts.unshift({ value: defaultModel, label: defaultModel });
    }
    modelOpts.unshift({ value: '', label: '— select —' });

    const primaryRow = formRow('Primary Model');
    const primarySel = selectInput(modelOpts, defaultModel ?? '');
    primarySel.style.maxWidth = '320px';
    primaryRow.appendChild(primarySel);
    defaultSection.appendChild(primaryRow);

    const fallbackRow = formRow('Fallback Models', 'Comma-separated list of fallback model IDs');
    const fallbackInp = textInput(fallbacks.join(', '), 'provider/model, provider/model');
    fallbackInp.style.maxWidth = '400px';
    fallbackRow.appendChild(fallbackInp);
    defaultSection.appendChild(fallbackRow);

    defaultSection.appendChild(saveReloadButtons(
      async () => {
        const primary = primarySel.value || undefined;
        const fb = fallbackInp.value.split(',').map(s => s.trim()).filter(Boolean);
        const patch: Record<string, unknown> = { agents: { defaults: { model: { primary, fallbacks: fb } } } };
        await patchConfig(patch);
      },
      () => loadModelsSettings()
    ));
    container.appendChild(defaultSection);

    // ── Provider Cards ───────────────────────────────────────────────────
    const provHeader = document.createElement('div');
    provHeader.style.cssText = 'display:flex;justify-content:space-between;align-items:center;margin-top:24px';
    provHeader.innerHTML = `<h3 class="settings-subsection-title" style="margin:0">Providers</h3>`;
    const addBtn = document.createElement('button');
    addBtn.className = 'btn btn-primary btn-sm';
    addBtn.textContent = '+ Add Provider';
    addBtn.addEventListener('click', () => addProvider(container));
    provHeader.appendChild(addBtn);
    container.appendChild(provHeader);

    const providerNames = Object.keys(providers);
    if (providerNames.length === 0) {
      const empty = document.createElement('p');
      empty.style.cssText = 'color:var(--text-muted);padding:12px 0';
      empty.textContent = 'No providers configured. Click "Add Provider" to get started.';
      container.appendChild(empty);
    }

    for (const name of providerNames) {
      const prov = providers[name];
      if (!prov || typeof prov !== 'object') continue;
      container.appendChild(renderProviderCard(name, prov));
    }

  } catch (e) {
    container.innerHTML = `<p style="color:var(--danger)">Failed to load: ${esc(String(e))}</p>`;
  }
}

function renderProviderCard(name: string, prov: Record<string, unknown>): HTMLDivElement {
  const card = document.createElement('div');
  card.className = 'settings-card';
  card.style.cssText = 'margin-top:12px;padding:16px;border:1px solid var(--border);border-radius:8px;';

  // Header
  const header = document.createElement('div');
  header.style.cssText = 'display:flex;justify-content:space-between;align-items:center;margin-bottom:12px';
  const title = document.createElement('strong');
  title.textContent = name;
  title.style.fontSize = '14px';
  header.appendChild(title);

  const actions = document.createElement('div');
  actions.style.cssText = 'display:flex;gap:6px';
  const delBtn = document.createElement('button');
  delBtn.className = 'btn btn-danger btn-sm';
  delBtn.textContent = 'Remove';
  delBtn.addEventListener('click', async () => {
    if (!confirm(`Remove provider "${name}"? This will delete all its models.`)) return;
    await patchConfig({ models: { providers: { [name]: null } } });
    loadModelsSettings();
  });
  actions.appendChild(delBtn);
  header.appendChild(actions);
  card.appendChild(header);

  // Fields
  const baseUrlRow = formRow('Base URL');
  const baseUrlInp = textInput(String(prov.baseUrl ?? ''), 'https://api.example.com/v1');
  baseUrlInp.style.maxWidth = '400px';
  baseUrlRow.appendChild(baseUrlInp);
  card.appendChild(baseUrlRow);

  const apiKeyRow = formRow('API Key');
  const apiKeyInp = textInput(String(prov.apiKey ?? ''), 'sk-…', 'password');
  apiKeyInp.style.maxWidth = '320px';
  apiKeyRow.appendChild(apiKeyInp);
  card.appendChild(apiKeyRow);

  const apiRow = formRow('API Type');
  const apiSel = selectInput(
    [{ value: '', label: '— inherit —' }, ...API_TYPES],
    String(prov.api ?? '')
  );
  apiSel.style.maxWidth = '260px';
  apiRow.appendChild(apiSel);
  card.appendChild(apiRow);

  // Models summary
  const models = Array.isArray(prov.models) ? prov.models : [];
  const modelsInfo = document.createElement('p');
  modelsInfo.style.cssText = 'font-size:12px;color:var(--text-muted);margin-top:8px';
  modelsInfo.textContent = models.length
    ? `${models.length} model(s): ${models.map((m: any) => m.name ?? m.id ?? '?').join(', ')}`
    : 'No models defined';
  card.appendChild(modelsInfo);

  // Save
  card.appendChild(saveReloadButtons(
    async () => {
      const patch: Record<string, unknown> = {};
      if (baseUrlInp.value) patch.baseUrl = baseUrlInp.value;
      if (apiKeyInp.value) patch.apiKey = apiKeyInp.value;
      if (apiSel.value) patch.api = apiSel.value;
      await patchConfig({ models: { providers: { [name]: patch } } });
    },
    () => loadModelsSettings()
  ));

  return card;
}

async function addProvider(_container: HTMLElement) {
  // Simple prompt for provider name
  const name = window.prompt?.('Provider name (e.g. google, openai, anthropic):')?.trim();
  if (!name) return;
  if (!/^[a-zA-Z][a-zA-Z0-9_-]*$/.test(name)) {
    showToast('Provider name must start with a letter and contain only letters, numbers, hyphens, underscores', 'error');
    return;
  }
  const ok = await patchConfig({
    models: { providers: { [name]: { baseUrl: '', models: [] } } }
  }, true);
  if (ok) {
    showToast(`Provider "${name}" added`, 'success');
    loadModelsSettings();
  }
}

// ── Init ────────────────────────────────────────────────────────────────────

export function initModelsSettings() {
  // Nothing to bind — all dynamic
}
