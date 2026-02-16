// Settings: Models & Providers
// CRUD for AI model providers + model definitions + default model selection
// Uses inline forms instead of window.prompt/confirm (blocked in Tauri WebView)
// All writes go through gateway config.patch (RFC 7386 merge semantics)

import { gateway } from '../gateway';
import { showToast } from '../components/toast';
import {
  getConfig, patchConfig, deleteConfigKey, getVal, isConnected,
  esc, formRow, selectInput, textInput, toggleSwitch, saveReloadButtons
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

    // ── Service Routing Table ────────────────────────────────────────────
    // Shows a visual map of all configured providers and their endpoints
    const routeSection = document.createElement('div');
    routeSection.className = 'settings-subsection';
    routeSection.innerHTML = `<h3 class="settings-subsection-title">Service Routing</h3>
      <p class="settings-section-desc">Active provider endpoints from gateway config.</p>`;

    // Use gateway-reported providers (single source of truth)
    const allProviders = providers;

    const routeTable = document.createElement('table');
    routeTable.style.cssText = 'width:100%;border-collapse:collapse;font-size:13px;margin:8px 0 16px 0';
    routeTable.innerHTML = `<thead><tr style="text-align:left;border-bottom:1px solid var(--border)">
      <th style="padding:6px 12px 6px 0">Provider</th>
      <th style="padding:6px 12px">Endpoint</th>
      <th style="padding:6px 12px">API</th>
      <th style="padding:6px 12px">Status</th>
    </tr></thead>`;
    const tbody = document.createElement('tbody');

    // Always show gateway
    const gatewayRow = document.createElement('tr');
    gatewayRow.style.borderBottom = '1px solid var(--border-light, rgba(255,255,255,0.06))';
    gatewayRow.innerHTML = `<td style="padding:6px 12px 6px 0;font-weight:600">🔌 Gateway</td>
      <td style="padding:6px 12px;font-family:monospace;font-size:12px">ws://127.0.0.1:18789</td>
      <td style="padding:6px 12px;color:var(--text-muted)">WebSocket v3</td>
      <td style="padding:6px 12px">${isConnected() ? '<span style="color:#4ade80">● Connected</span>' : '<span style="color:#ef4444">● Disconnected</span>'}</td>`;
    tbody.appendChild(gatewayRow);

    for (const [provName, prov] of Object.entries(allProviders)) {
      if (!prov || typeof prov !== 'object') continue;
      const url = (prov as any).baseUrl || '(default)';
      const api = (prov as any).api || '—';
      const hasKey = !!(prov as any).apiKey;
      const row = document.createElement('tr');
      row.style.borderBottom = '1px solid var(--border-light, rgba(255,255,255,0.06))';
      row.innerHTML = `<td style="padding:6px 12px 6px 0;font-weight:600">${esc(provName)}</td>
        <td style="padding:6px 12px;font-family:monospace;font-size:12px">${esc(String(url))}</td>
        <td style="padding:6px 12px">${esc(String(api))}</td>
        <td style="padding:6px 12px">${hasKey ? '🔑 Key set' : url.includes('127.0.0.1') || url.includes('localhost') ? '🏠 Local' : '—'}</td>`;
      tbody.appendChild(row);
    }

    routeTable.appendChild(tbody);
    routeSection.appendChild(routeTable);
    container.appendChild(routeSection);

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

    // ── Model Aliases ────────────────────────────────────────────────────
    const aliasSection = document.createElement('div');
    aliasSection.style.marginTop = '20px';
    aliasSection.innerHTML = `<h3 class="settings-subsection-title">Model Aliases</h3>
      <p class="settings-section-desc">Short names to reference models in prompts (e.g. "use sonnet"). One model ID per alias.</p>`;

    const aliasModels = (getVal(config, 'agents.defaults.models') ?? {}) as Record<string, any>;
    const aliasTableBody = document.createElement('div');

    const renderAliasRows = () => {
      aliasTableBody.innerHTML = '';
      const entries = Object.entries(aliasModels);
      if (entries.length === 0) {
        const hint = document.createElement('p');
        hint.style.cssText = 'color:var(--text-muted);font-size:12px;margin:4px 0';
        hint.textContent = 'No aliases configured yet.';
        aliasTableBody.appendChild(hint);
      }
      for (const [modelId, val] of entries) {
        const alias = (val && typeof val === 'object') ? (val as Record<string, unknown>).alias ?? '' : '';
        const row = document.createElement('div');
        row.style.cssText = 'display:flex;gap:8px;align-items:center;margin-bottom:6px';
        const modelInp = textInput(modelId, 'anthropic/claude-sonnet-4-5');
        modelInp.style.flex = '1';
        modelInp.readOnly = true;
        modelInp.style.opacity = '0.7';
        const aliasInp = textInput(String(alias), 'sonnet');
        aliasInp.style.maxWidth = '140px';
        aliasInp.dataset.model = modelId;
        const rmBtn = document.createElement('button');
        rmBtn.className = 'btn btn-ghost btn-sm';
        rmBtn.textContent = '✕';
        rmBtn.style.color = 'var(--danger)';
        rmBtn.onclick = () => { delete aliasModels[modelId]; renderAliasRows(); };
        row.appendChild(modelInp);
        row.appendChild(aliasInp);
        row.appendChild(rmBtn);
        aliasTableBody.appendChild(row);
      }
    };
    renderAliasRows();
    aliasSection.appendChild(aliasTableBody);

    // Inline add-alias form (no window.prompt)
    const addAliasRow = document.createElement('div');
    addAliasRow.style.cssText = 'display:flex;gap:8px;align-items:center;margin-top:8px';
    const addAliasModelInp = textInput('', 'anthropic/claude-haiku-4-5');
    addAliasModelInp.style.flex = '1';
    const addAliasBtn = document.createElement('button');
    addAliasBtn.className = 'btn btn-ghost btn-sm';
    addAliasBtn.textContent = '+ Add';
    addAliasBtn.onclick = () => {
      const modelId = addAliasModelInp.value.trim();
      if (!modelId) {
        showToast('Enter a model ID first', 'error');
        return;
      }
      aliasModels[modelId] = { alias: '' };
      addAliasModelInp.value = '';
      renderAliasRows();
    };
    addAliasRow.appendChild(addAliasModelInp);
    addAliasRow.appendChild(addAliasBtn);
    aliasSection.appendChild(addAliasRow);

    aliasSection.appendChild(saveReloadButtons(
      async () => {
        // Read aliases from rendered inputs
        const modelsMap: Record<string, unknown> = {};
        aliasTableBody.querySelectorAll('input[data-model]').forEach((inp) => {
          const el = inp as HTMLInputElement;
          const modelId = el.dataset.model!;
          modelsMap[modelId] = { alias: el.value.trim() || undefined };
        });
        await patchConfig({ agents: { defaults: { models: modelsMap } } });
      },
      () => loadModelsSettings()
    ));
    container.appendChild(aliasSection);

    // ── Prompt Caching ───────────────────────────────────────────────────
    const cacheSection = document.createElement('div');
    cacheSection.style.marginTop = '20px';
    cacheSection.innerHTML = `<h3 class="settings-subsection-title">Prompt Caching</h3>
      <p class="settings-section-desc">Cache system prompts and stable context for up to 90% token discount on reuse (Claude 3.5+ Sonnet).</p>`;

    const cacheConf = (getVal(config, 'agents.defaults.cache') ?? {}) as Record<string, any>;

    const { container: cacheToggle, checkbox: cacheCb } = toggleSwitch(
      cacheConf.enabled === true,
      'Enable Prompt Caching'
    );
    cacheSection.appendChild(cacheToggle);

    const ttlRow = formRow('Cache TTL', 'How long cached prompts stay valid (e.g. 5m, 30m, 24h)');
    const ttlInp = textInput(cacheConf.ttl ?? '', '5m');
    ttlInp.style.maxWidth = '120px';
    ttlRow.appendChild(ttlInp);
    cacheSection.appendChild(ttlRow);

    const cachePrioRow = formRow('Priority');
    const cachePrioSel = selectInput(
      [{ value: 'high', label: 'High (maximize caching)' }, { value: 'low', label: 'Low (balance cost/speed)' }],
      cacheConf.priority ?? 'high'
    );
    cachePrioSel.style.maxWidth = '260px';
    cachePrioRow.appendChild(cachePrioSel);
    cacheSection.appendChild(cachePrioRow);

    cacheSection.appendChild(saveReloadButtons(
      async () => {
        await patchConfig({
          agents: { defaults: { cache: {
            enabled: cacheCb.checked,
            ttl: ttlInp.value || undefined,
            priority: cachePrioSel.value,
          } } }
        });
      },
      () => loadModelsSettings()
    ));
    container.appendChild(cacheSection);

    // ── Provider Cards ───────────────────────────────────────────────────
    const provHeader = document.createElement('div');
    provHeader.style.cssText = 'display:flex;justify-content:space-between;align-items:center;margin-top:24px';
    provHeader.innerHTML = `<h3 class="settings-subsection-title" style="margin:0">Providers</h3>`;
    const addBtn = document.createElement('button');
    addBtn.className = 'btn btn-primary btn-sm';
    addBtn.textContent = '+ Add Provider';
    addBtn.addEventListener('click', () => toggleAddProviderForm(container));
    provHeader.appendChild(addBtn);
    container.appendChild(provHeader);

    // Inline add-provider form (hidden by default)
    const addForm = document.createElement('div');
    addForm.id = 'add-provider-form';
    addForm.style.cssText = 'display:none;margin-top:12px;padding:16px;border:1px solid var(--accent);border-radius:8px;background:var(--bg-secondary, rgba(255,255,255,0.03))';
    addForm.innerHTML = `<h4 style="margin:0 0 12px 0;font-size:14px">New Provider</h4>`;

    const nameRow = formRow('Provider Name', 'Lowercase identifier (e.g. google, openai, ollama)');
    const nameInp = textInput('', 'ollama');
    nameInp.style.maxWidth = '240px';
    nameInp.id = 'add-provider-name';
    nameRow.appendChild(nameInp);
    addForm.appendChild(nameRow);

    const newUrlRow = formRow('Base URL');
    const newUrlInp = textInput('', 'http://127.0.0.1:11434');
    newUrlInp.style.maxWidth = '400px';
    newUrlInp.id = 'add-provider-url';
    newUrlRow.appendChild(newUrlInp);
    addForm.appendChild(newUrlRow);

    const newKeyRow = formRow('API Key', 'Leave blank for local providers like Ollama');
    const newKeyInp = textInput('', 'sk-…', 'password');
    newKeyInp.style.maxWidth = '320px';
    newKeyInp.id = 'add-provider-key';
    newKeyRow.appendChild(newKeyInp);
    addForm.appendChild(newKeyRow);

    const newApiRow = formRow('API Type');
    const newApiSel = selectInput(
      [{ value: '', label: '— select —' }, ...API_TYPES],
      ''
    );
    newApiSel.style.maxWidth = '260px';
    newApiSel.id = 'add-provider-api';
    newApiRow.appendChild(newApiSel);
    addForm.appendChild(newApiRow);

    const formBtns = document.createElement('div');
    formBtns.style.cssText = 'display:flex;gap:8px;margin-top:16px';
    const createBtn = document.createElement('button');
    createBtn.className = 'btn btn-primary';
    createBtn.textContent = 'Create Provider';
    createBtn.addEventListener('click', async () => {
      const name = nameInp.value.trim();
      if (!name) {
        showToast('Enter a provider name', 'error');
        return;
      }
      if (!/^[a-zA-Z][a-zA-Z0-9_-]*$/.test(name)) {
        showToast('Name must start with a letter (letters, numbers, hyphens only)', 'error');
        return;
      }
      const provObj: Record<string, unknown> = { models: [] };
      if (newUrlInp.value.trim()) provObj.baseUrl = newUrlInp.value.trim();
      if (newKeyInp.value.trim()) provObj.apiKey = newKeyInp.value.trim();
      if (newApiSel.value) provObj.api = newApiSel.value;

      try {
        createBtn.disabled = true;
        createBtn.textContent = 'Creating…';
        // Write through gateway config.patch — validated, hash-protected, triggers restart
        const ok = await patchConfig({ models: { providers: { [name]: provObj } } });
        if (ok) loadModelsSettings();
      } catch (e) {
        showToast(`Failed: ${e instanceof Error ? e.message : e}`, 'error');
        createBtn.disabled = false;
        createBtn.textContent = 'Create Provider';
      }
    });
    const cancelBtn = document.createElement('button');
    cancelBtn.className = 'btn btn-ghost';
    cancelBtn.textContent = 'Cancel';
    cancelBtn.addEventListener('click', () => { addForm.style.display = 'none'; });
    formBtns.appendChild(createBtn);
    formBtns.appendChild(cancelBtn);
    addForm.appendChild(formBtns);
    container.appendChild(addForm);

    const providerNames = Object.keys(providers);
    if (providerNames.length === 0) {
      const empty = document.createElement('p');
      empty.style.cssText = 'color:var(--text-muted);padding:12px 0';
      empty.textContent = 'No providers configured. Click "+ Add Provider" to get started.';
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

function toggleAddProviderForm(_container: HTMLElement) {
  const form = document.getElementById('add-provider-form');
  if (!form) return;
  const visible = form.style.display !== 'none';
  form.style.display = visible ? 'none' : 'block';
  if (!visible) {
    // Clear + focus name field
    const nameInp = document.getElementById('add-provider-name') as HTMLInputElement | null;
    if (nameInp) { nameInp.value = ''; nameInp.focus(); }
    const urlInp = document.getElementById('add-provider-url') as HTMLInputElement | null;
    if (urlInp) urlInp.value = '';
    const keyInp = document.getElementById('add-provider-key') as HTMLInputElement | null;
    if (keyInp) keyInp.value = '';
    const apiSel = document.getElementById('add-provider-api') as HTMLSelectElement | null;
    if (apiSel) apiSel.value = '';
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
  let confirmPending = false;
  delBtn.addEventListener('click', async () => {
    if (!confirmPending) {
      // First click: change to confirm state
      confirmPending = true;
      delBtn.textContent = 'Confirm Remove?';
      delBtn.style.fontWeight = 'bold';
      // Auto-reset after 4 seconds
      setTimeout(() => {
        if (confirmPending) {
          confirmPending = false;
          delBtn.textContent = 'Remove';
          delBtn.style.fontWeight = '';
        }
      }, 4000);
      return;
    }
    // Second click: actually delete via gateway config.patch (null = delete in RFC 7386)
    confirmPending = false;
    delBtn.textContent = 'Removing…';
    delBtn.disabled = true;
    try {
      const ok = await deleteConfigKey(`models.providers.${name}`);
      if (ok) loadModelsSettings();
    } catch (e) {
      showToast(`Remove failed: ${e instanceof Error ? e.message : e}`, 'error');
      delBtn.textContent = 'Remove';
      delBtn.disabled = false;
    }
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

  // Save — via gateway config.patch
  card.appendChild(saveReloadButtons(
    async () => {
      const patch: Record<string, unknown> = {};
      if (baseUrlInp.value) patch.baseUrl = baseUrlInp.value;
      if (apiKeyInp.value) patch.apiKey = apiKeyInp.value;
      if (apiSel.value) patch.api = apiSel.value;
      // Preserve models array from original
      if (models.length) patch.models = models;
      await patchConfig({ models: { providers: { [name]: patch } } });
    },
    () => loadModelsSettings()
  ));

  return card;
}

// ── Init ────────────────────────────────────────────────────────────────────

export function initModelsSettings() {
  // Nothing to bind — all dynamic
}
