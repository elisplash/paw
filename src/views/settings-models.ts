// Settings: Models & Providers
// Multi-provider management — add/edit/remove AI providers, set default model
// All config goes through the Paw engine (Tauri IPC). No gateway.

import { pawEngine, type EngineProviderConfig, type EngineConfig } from '../engine';
import { showToast } from '../components/toast';
import {
  isConnected, getEngineConfig, setEngineConfig,
  esc, formRow, selectInput, textInput, numberInput, saveReloadButtons
} from './settings-config';

const $ = (id: string) => document.getElementById(id);

// ── Provider Kinds ──────────────────────────────────────────────────────────

const PROVIDER_KINDS: Array<{ value: string; label: string }> = [
  { value: 'ollama', label: 'Ollama (local)' },
  { value: 'openai', label: 'OpenAI' },
  { value: 'anthropic', label: 'Anthropic' },
  { value: 'google', label: 'Google' },
  { value: 'openrouter', label: 'OpenRouter' },
  { value: 'custom', label: 'Custom / Compatible' },
];

const DEFAULT_BASE_URLS: Record<string, string> = {
  ollama: 'http://localhost:11434',
  openai: 'https://api.openai.com/v1',
  anthropic: 'https://api.anthropic.com',
  google: 'https://generativelanguage.googleapis.com/v1beta',
  openrouter: 'https://openrouter.ai/api/v1',
  custom: '',
};

const POPULAR_MODELS: Record<string, string[]> = {
  ollama: ['llama3.2:3b', 'llama3.1:8b', 'llama3.1:70b', 'mistral:7b', 'codellama:13b', 'deepseek-coder:6.7b', 'phi3:mini', 'qwen2.5:7b'],
  openai: ['gpt-4o', 'gpt-4o-mini', 'o1', 'o3-mini'],
  anthropic: ['claude-sonnet-4-20250514', 'claude-3-5-haiku-20241022', 'claude-opus-4-20250514'],
  google: ['gemini-2.0-flash', 'gemini-2.0-pro', 'gemini-1.5-flash-8b'],
  openrouter: ['meta-llama/llama-3.1-405b-instruct', 'anthropic/claude-sonnet-4-20250514'],
  custom: [],
};

const KIND_ICONS: Record<string, string> = {
  ollama: '🦙', openai: '🤖', anthropic: '🧠', google: '🔮', openrouter: '🌐', custom: '🔧',
};

// ── Render ──────────────────────────────────────────────────────────────────

export async function loadModelsSettings() {
  if (!isConnected()) return;
  const container = $('settings-models-content');
  if (!container) return;
  container.innerHTML = '<p style="color:var(--text-muted)">Loading…</p>';

  try {
    const config = await getEngineConfig();
    const providers = config.providers ?? [];

    container.innerHTML = '';

    // ── Provider Overview ────────────────────────────────────────────────
    const overviewSection = document.createElement('div');
    overviewSection.className = 'settings-subsection';
    overviewSection.innerHTML = `<h3 class="settings-subsection-title">Configured Providers</h3>
      <p class="settings-section-desc">All your AI providers. Agents can use any of these — add as many as you need.</p>`;

    if (providers.length === 0) {
      const empty = document.createElement('div');
      empty.style.cssText = 'padding:24px;text-align:center;border:1px dashed var(--border);border-radius:8px;margin:12px 0';
      empty.innerHTML = `<p style="color:var(--text-muted);margin:0 0 8px 0">No providers configured yet.</p>
        <p style="color:var(--text-muted);font-size:12px;margin:0">Add Ollama for local models, or connect OpenAI, Anthropic, Google, OpenRouter, and more.</p>`;
      overviewSection.appendChild(empty);
    } else {
      // Provider status table
      const table = document.createElement('table');
      table.style.cssText = 'width:100%;border-collapse:collapse;font-size:13px;margin:8px 0 16px 0';
      table.innerHTML = `<thead><tr style="text-align:left;border-bottom:1px solid var(--border)">
        <th style="padding:6px 12px 6px 0">Provider</th>
        <th style="padding:6px 12px">Type</th>
        <th style="padding:6px 12px">Endpoint</th>
        <th style="padding:6px 12px">Default Model</th>
        <th style="padding:6px 12px">Status</th>
      </tr></thead>`;
      const tbody = document.createElement('tbody');

      for (const p of providers) {
        const icon = KIND_ICONS[p.kind] ?? '🔧';
        const kindLabel = PROVIDER_KINDS.find(k => k.value === p.kind)?.label ?? p.kind;
        const endpoint = p.base_url || DEFAULT_BASE_URLS[p.kind] || '(default)';
        const hasKey = !!p.api_key;
        const isLocal = p.kind === 'ollama';
        const isDefault = p.id === config.default_provider;
        const statusBadge = hasKey
          ? '<span style="color:#4ade80">● Key set</span>'
          : isLocal
            ? '<span style="color:#60a5fa">● Local</span>'
            : '<span style="color:#fbbf24">● No key</span>';

        const row = document.createElement('tr');
        row.style.borderBottom = '1px solid var(--border-light, rgba(255,255,255,0.06))';
        row.innerHTML = `<td style="padding:6px 12px 6px 0;font-weight:600">${icon} ${esc(p.id)}${isDefault ? ' <span style="font-size:10px;color:var(--accent);font-weight:normal">★ default</span>' : ''}</td>
          <td style="padding:6px 12px;color:var(--text-muted)">${esc(kindLabel)}</td>
          <td style="padding:6px 12px;font-family:monospace;font-size:11px">${esc(String(endpoint))}</td>
          <td style="padding:6px 12px;font-family:monospace;font-size:11px">${esc(p.default_model ?? '—')}</td>
          <td style="padding:6px 12px">${statusBadge}</td>`;
        tbody.appendChild(row);
      }

      table.appendChild(tbody);
      overviewSection.appendChild(table);
    }

    container.appendChild(overviewSection);

    // ── Default Model / Provider ─────────────────────────────────────────
    const defaultSection = document.createElement('div');
    defaultSection.className = 'settings-subsection';
    defaultSection.style.marginTop = '20px';
    defaultSection.innerHTML = `<h3 class="settings-subsection-title">Default Model & Provider</h3>
      <p class="settings-section-desc">The model and provider used for conversations unless overridden per-agent.</p>`;

    // Default provider dropdown
    const providerOpts = [
      { value: '', label: '— auto (first available) —' },
      ...providers.map(p => ({ value: p.id, label: `${KIND_ICONS[p.kind] ?? ''} ${p.id}` }))
    ];
    const defProvRow = formRow('Default Provider', 'Which provider to use by default');
    const defProvSel = selectInput(providerOpts, config.default_provider ?? '');
    defProvSel.style.maxWidth = '320px';
    defProvRow.appendChild(defProvSel);
    defaultSection.appendChild(defProvRow);

    // Default model — build list from popular models of all providers
    const allModelOpts: Array<{ value: string; label: string }> = [
      { value: '', label: '— use provider default —' },
    ];
    for (const p of providers) {
      if (p.default_model) {
        allModelOpts.push({ value: p.default_model, label: `${p.default_model} (${p.id})` });
      }
      const popular = POPULAR_MODELS[p.kind] ?? [];
      for (const m of popular) {
        if (!allModelOpts.find(o => o.value === m)) {
          allModelOpts.push({ value: m, label: `${m} (${p.kind})` });
        }
      }
    }
    // Include current value if not in list
    if (config.default_model && !allModelOpts.find(o => o.value === config.default_model)) {
      allModelOpts.splice(1, 0, { value: config.default_model, label: config.default_model });
    }

    const defModelRow = formRow('Default Model', 'Model ID to use — or type a custom one');
    const defModelInp = textInput(config.default_model ?? '', 'gpt-4o, claude-sonnet-4-20250514, llama3.1:8b …');
    defModelInp.style.maxWidth = '400px';
    defModelInp.setAttribute('list', 'default-model-datalist');
    // Use datalist for suggestions but allow free-form input
    const datalist = document.createElement('datalist');
    datalist.id = 'default-model-datalist';
    for (const opt of allModelOpts) {
      if (!opt.value) continue;
      const o = document.createElement('option');
      o.value = opt.value;
      o.textContent = opt.label;
      datalist.appendChild(o);
    }
    defModelRow.appendChild(defModelInp);
    defModelRow.appendChild(datalist);
    defaultSection.appendChild(defModelRow);

    // Tool limits
    const roundsRow = formRow('Max Tool Rounds', 'How many tool-call rounds before stopping (default: 25)');
    const roundsInp = numberInput(config.max_tool_rounds ?? 25, { min: 1, max: 200, step: 1 });
    roundsInp.style.maxWidth = '120px';
    roundsRow.appendChild(roundsInp);
    defaultSection.appendChild(roundsRow);

    const timeoutRow = formRow('Tool Timeout (seconds)', 'Max time per tool execution (default: 120)');
    const timeoutInp = numberInput(config.tool_timeout_secs ?? 120, { min: 5, max: 3600, step: 5 });
    timeoutInp.style.maxWidth = '120px';
    timeoutRow.appendChild(timeoutInp);
    defaultSection.appendChild(timeoutRow);

    defaultSection.appendChild(saveReloadButtons(
      async () => {
        const updated: EngineConfig = {
          ...config,
          default_provider: defProvSel.value || undefined,
          default_model: defModelInp.value.trim() || undefined,
          max_tool_rounds: parseInt(roundsInp.value, 10) || 25,
          tool_timeout_secs: parseInt(timeoutInp.value, 10) || 120,
        };
        const ok = await setEngineConfig(updated);
        if (ok) loadModelsSettings();
      },
      () => loadModelsSettings()
    ));
    container.appendChild(defaultSection);

    // ── Provider Cards (edit/remove each) ────────────────────────────────
    const provHeader = document.createElement('div');
    provHeader.style.cssText = 'display:flex;justify-content:space-between;align-items:center;margin-top:24px';
    provHeader.innerHTML = `<h3 class="settings-subsection-title" style="margin:0">Manage Providers</h3>`;
    const addBtn = document.createElement('button');
    addBtn.className = 'btn btn-primary btn-sm';
    addBtn.textContent = '+ Add Provider';
    addBtn.addEventListener('click', () => toggleAddProviderForm());
    provHeader.appendChild(addBtn);
    container.appendChild(provHeader);

    // Inline add-provider form (hidden by default)
    container.appendChild(buildAddProviderForm(config));

    // Render each provider as a card
    for (const p of providers) {
      container.appendChild(renderProviderCard(p, config));
    }

  } catch (e) {
    container.innerHTML = `<p style="color:var(--danger)">Failed to load: ${esc(String(e))}</p>`;
  }
}

// ── Add Provider Form ───────────────────────────────────────────────────────

function buildAddProviderForm(config: EngineConfig): HTMLDivElement {
  const form = document.createElement('div');
  form.id = 'add-provider-form';
  form.style.cssText = 'display:none;margin-top:12px;padding:16px;border:1px solid var(--accent);border-radius:8px;background:var(--bg-secondary, rgba(255,255,255,0.03))';
  form.innerHTML = `<h4 style="margin:0 0 12px 0;font-size:14px">New Provider</h4>`;

  const idRow = formRow('Provider ID', 'Unique lowercase identifier (e.g. my-openai, ollama-local)');
  const idInp = textInput('', 'ollama');
  idInp.style.maxWidth = '240px';
  idRow.appendChild(idInp);
  form.appendChild(idRow);

  const kindRow = formRow('Provider Type');
  const kindSel = selectInput(PROVIDER_KINDS, 'ollama');
  kindSel.style.maxWidth = '260px';
  kindRow.appendChild(kindSel);
  form.appendChild(kindRow);

  const urlRow = formRow('Base URL', 'Leave blank for default');
  const urlInp = textInput('', 'http://localhost:11434');
  urlInp.style.maxWidth = '400px';
  urlRow.appendChild(urlInp);
  form.appendChild(urlRow);

  const keyRow = formRow('API Key', 'Leave blank for local providers like Ollama');
  const keyInp = textInput('', 'sk-…', 'password');
  keyInp.style.maxWidth = '320px';
  keyRow.appendChild(keyInp);
  form.appendChild(keyRow);

  const modelRow = formRow('Default Model', 'Optional default model for this provider');
  const modelInp = textInput('', 'gpt-4o');
  modelInp.style.maxWidth = '320px';
  modelRow.appendChild(modelInp);
  form.appendChild(modelRow);

  // Auto-fill URL when kind changes
  kindSel.addEventListener('change', () => {
    const kind = kindSel.value;
    if (!urlInp.value || Object.values(DEFAULT_BASE_URLS).includes(urlInp.value)) {
      urlInp.value = DEFAULT_BASE_URLS[kind] ?? '';
    }
    urlInp.placeholder = DEFAULT_BASE_URLS[kind] ?? '';
    // Auto-fill ID if empty
    if (!idInp.value) {
      idInp.value = kind;
    }
    // Suggest models
    const models = POPULAR_MODELS[kind] ?? [];
    if (models.length && !modelInp.value) {
      modelInp.placeholder = models[0];
    }
  });

  // Trigger initial fill
  kindSel.dispatchEvent(new Event('change'));

  const formBtns = document.createElement('div');
  formBtns.style.cssText = 'display:flex;gap:8px;margin-top:16px';
  const createBtn = document.createElement('button');
  createBtn.className = 'btn btn-primary';
  createBtn.textContent = 'Add Provider';
  createBtn.addEventListener('click', async () => {
    const id = idInp.value.trim();
    if (!id) {
      showToast('Enter a provider ID', 'error');
      return;
    }
    if (!/^[a-zA-Z][a-zA-Z0-9_-]*$/.test(id)) {
      showToast('ID must start with a letter (letters, numbers, hyphens)', 'error');
      return;
    }
    if (config.providers.some(p => p.id === id)) {
      showToast(`Provider "${id}" already exists`, 'error');
      return;
    }
    const provider: EngineProviderConfig = {
      id,
      kind: kindSel.value as EngineProviderConfig['kind'],
      api_key: keyInp.value.trim(),
      base_url: urlInp.value.trim() || undefined,
      default_model: modelInp.value.trim() || undefined,
    };
    try {
      createBtn.disabled = true;
      createBtn.textContent = 'Adding…';
      await pawEngine.upsertProvider(provider);
      showToast(`Provider "${id}" added`, 'success');
      loadModelsSettings();
    } catch (e) {
      showToast(`Failed: ${e instanceof Error ? e.message : e}`, 'error');
      createBtn.disabled = false;
      createBtn.textContent = 'Add Provider';
    }
  });
  const cancelBtn = document.createElement('button');
  cancelBtn.className = 'btn btn-ghost';
  cancelBtn.textContent = 'Cancel';
  cancelBtn.addEventListener('click', () => { form.style.display = 'none'; });
  formBtns.appendChild(createBtn);
  formBtns.appendChild(cancelBtn);
  form.appendChild(formBtns);

  return form;
}

function toggleAddProviderForm() {
  const form = document.getElementById('add-provider-form');
  if (!form) return;
  const visible = form.style.display !== 'none';
  form.style.display = visible ? 'none' : 'block';
  if (!visible) {
    const firstInput = form.querySelector('input') as HTMLInputElement | null;
    if (firstInput) firstInput.focus();
  }
}

// ── Provider Card ───────────────────────────────────────────────────────────

function renderProviderCard(provider: EngineProviderConfig, config: EngineConfig): HTMLDivElement {
  const card = document.createElement('div');
  card.className = 'settings-card';
  card.style.cssText = 'margin-top:12px;padding:16px;border:1px solid var(--border);border-radius:8px;';

  const icon = KIND_ICONS[provider.kind] ?? '🔧';
  const isDefault = provider.id === config.default_provider;

  // Header
  const header = document.createElement('div');
  header.style.cssText = 'display:flex;justify-content:space-between;align-items:center;margin-bottom:12px';
  const titleWrap = document.createElement('div');
  titleWrap.style.cssText = 'display:flex;align-items:center;gap:8px';
  titleWrap.innerHTML = `<span style="font-size:18px">${icon}</span>
    <strong style="font-size:14px">${esc(provider.id)}</strong>
    <span style="font-size:11px;color:var(--text-muted);background:var(--bg-tertiary,rgba(255,255,255,0.06));padding:2px 8px;border-radius:4px">${esc(provider.kind)}</span>
    ${isDefault ? '<span style="font-size:10px;color:var(--accent);background:rgba(var(--accent-rgb,99,102,241),0.15);padding:2px 8px;border-radius:4px">★ default</span>' : ''}`;
  header.appendChild(titleWrap);

  const actions = document.createElement('div');
  actions.style.cssText = 'display:flex;gap:6px';

  // Set as default button
  if (!isDefault) {
    const defBtn = document.createElement('button');
    defBtn.className = 'btn btn-ghost btn-sm';
    defBtn.textContent = 'Set Default';
    defBtn.addEventListener('click', async () => {
      try {
        const updated: EngineConfig = { ...config, default_provider: provider.id };
        await setEngineConfig(updated, true);
        showToast(`${provider.id} set as default provider`, 'success');
        loadModelsSettings();
      } catch (e) {
        showToast(`Failed: ${e instanceof Error ? e.message : e}`, 'error');
      }
    });
    actions.appendChild(defBtn);
  }

  // Delete button with confirm
  const delBtn = document.createElement('button');
  delBtn.className = 'btn btn-danger btn-sm';
  delBtn.textContent = 'Remove';
  let confirmPending = false;
  delBtn.addEventListener('click', async () => {
    if (!confirmPending) {
      confirmPending = true;
      delBtn.textContent = 'Confirm Remove?';
      delBtn.style.fontWeight = 'bold';
      setTimeout(() => {
        if (confirmPending) {
          confirmPending = false;
          delBtn.textContent = 'Remove';
          delBtn.style.fontWeight = '';
        }
      }, 4000);
      return;
    }
    confirmPending = false;
    delBtn.textContent = 'Removing…';
    delBtn.disabled = true;
    try {
      await pawEngine.removeProvider(provider.id);
      showToast(`Provider "${provider.id}" removed`, 'success');
      loadModelsSettings();
    } catch (e) {
      showToast(`Remove failed: ${e instanceof Error ? e.message : e}`, 'error');
      delBtn.textContent = 'Remove';
      delBtn.disabled = false;
    }
  });
  actions.appendChild(delBtn);
  header.appendChild(actions);
  card.appendChild(header);

  // Editable fields
  const kindRow = formRow('Provider Type');
  const kindSel = selectInput(PROVIDER_KINDS, provider.kind);
  kindSel.style.maxWidth = '260px';
  kindRow.appendChild(kindSel);
  card.appendChild(kindRow);

  const urlRow = formRow('Base URL');
  const urlInp = textInput(provider.base_url ?? '', DEFAULT_BASE_URLS[provider.kind] ?? 'https://api.example.com/v1');
  urlInp.style.maxWidth = '400px';
  urlRow.appendChild(urlInp);
  card.appendChild(urlRow);

  const keyRow = formRow('API Key');
  const keyInp = textInput(provider.api_key ?? '', 'sk-…', 'password');
  keyInp.style.maxWidth = '320px';
  keyRow.appendChild(keyInp);
  card.appendChild(keyRow);

  const modelRow = formRow('Default Model', 'Model used when no specific model is requested');
  const modelInp = textInput(provider.default_model ?? '', '');
  modelInp.style.maxWidth = '320px';
  // Add datalist with popular models for this kind
  const popular = POPULAR_MODELS[provider.kind] ?? [];
  if (popular.length) {
    const dlId = `models-${provider.id}`;
    const dl = document.createElement('datalist');
    dl.id = dlId;
    for (const m of popular) {
      const o = document.createElement('option');
      o.value = m;
      dl.appendChild(o);
    }
    modelInp.setAttribute('list', dlId);
    modelRow.appendChild(dl);
  }
  modelRow.appendChild(modelInp);
  card.appendChild(modelRow);

  // Popular models as quick-select chips
  if (popular.length) {
    const chipsWrap = document.createElement('div');
    chipsWrap.style.cssText = 'display:flex;flex-wrap:wrap;gap:4px;margin-top:4px';
    for (const m of popular.slice(0, 6)) {
      const chip = document.createElement('button');
      chip.className = 'btn btn-ghost btn-sm';
      chip.style.cssText = 'font-size:11px;padding:2px 8px;border-radius:12px;border:1px solid var(--border)';
      chip.textContent = m;
      chip.addEventListener('click', () => { modelInp.value = m; });
      chipsWrap.appendChild(chip);
    }
    card.appendChild(chipsWrap);
  }

  // Save / Reload
  card.appendChild(saveReloadButtons(
    async () => {
      const updated: EngineProviderConfig = {
        id: provider.id,
        kind: kindSel.value as EngineProviderConfig['kind'],
        api_key: keyInp.value.trim(),
        base_url: urlInp.value.trim() || undefined,
        default_model: modelInp.value.trim() || undefined,
      };
      try {
        await pawEngine.upsertProvider(updated);
        showToast(`Provider "${provider.id}" updated`, 'success');
        loadModelsSettings();
      } catch (e) {
        showToast(`Save failed: ${e instanceof Error ? e.message : e}`, 'error');
      }
    },
    () => loadModelsSettings()
  ));

  return card;
}

// ── Init ────────────────────────────────────────────────────────────────────

export function initModelsSettings() {
  // Nothing to bind — all dynamic
}
