// Settings: Models & Providers — DOM rendering + IPC

import {
  pawEngine,
  type EngineProviderConfig,
  type EngineConfig,
  type ModelRouting,
} from '../../engine';
import { showToast } from '../../components/toast';
import { isConnected } from '../../state/connection';
import {
  getEngineConfig,
  setEngineConfig,
  esc,
  formRow,
  selectInput,
  textInput,
  saveReloadButtons,
} from '../settings-config';
import { $ } from '../../components/helpers';
import {
  PROVIDER_KINDS,
  DEFAULT_BASE_URLS,
  POPULAR_MODELS,
  KIND_ICONS,
  SPECIALTIES,
  TIER_LABELS,
  buildAllKnownModels,
  getAvailableModelsList,
} from './atoms';

// Re-export for external consumers
export { getAvailableModelsList };

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
      empty.style.cssText =
        'padding:24px;text-align:center;border:1px dashed var(--border);border-radius:8px;margin:12px 0';
      empty.innerHTML = `<p style="color:var(--text-muted);margin:0 0 8px 0">No providers configured yet.</p>
        <p style="color:var(--text-muted);font-size:12px;margin:0">Add Ollama for local models, or connect OpenAI, Anthropic, Google, OpenRouter, and more.</p>`;
      overviewSection.appendChild(empty);
    } else {
      // Provider status table
      const table = document.createElement('table');
      table.style.cssText =
        'width:100%;border-collapse:collapse;font-size:13px;margin:8px 0 16px 0';
      table.innerHTML = `<thead><tr style="text-align:left;border-bottom:1px solid var(--border)">
        <th style="padding:6px 12px 6px 0">Provider</th>
        <th style="padding:6px 12px">Type</th>
        <th style="padding:6px 12px">Endpoint</th>
        <th style="padding:6px 12px">Default Model</th>
        <th style="padding:6px 12px">Status</th>
      </tr></thead>`;
      const tbody = document.createElement('tbody');

      for (const p of providers) {
        const iconHtml = `<span class="ms ms-sm">${KIND_ICONS[p.kind] ?? 'build'}</span>`;
        const kindLabel = PROVIDER_KINDS.find((k) => k.value === p.kind)?.label ?? p.kind;
        const endpoint = p.base_url || DEFAULT_BASE_URLS[p.kind] || '(default)';
        const hasKey = !!p.api_key;
        const isLocal = p.kind === 'ollama';
        const isDefault = p.id === config.default_provider;
        const statusBadge = hasKey
          ? '<span style="color:var(--status-success)">● Key set</span>'
          : isLocal
            ? '<span style="color:var(--status-info)">● Local</span>'
            : '<span style="color:var(--status-warning)">● No key</span>';

        const row = document.createElement('tr');
        row.style.borderBottom = '1px solid var(--border-light, rgba(255,255,255,0.06))';
        row.innerHTML = `<td style="padding:6px 12px 6px 0;font-weight:600">${iconHtml} ${esc(p.id)}${isDefault ? ' <span style="font-size:10px;color:var(--accent);font-weight:normal">\u2605 default</span>' : ''}</td>
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
      ...providers.map((p) => ({ value: p.id, label: `${KIND_ICONS[p.kind] ?? ''} ${p.id}` })),
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
        if (!allModelOpts.find((o) => o.value === m)) {
          allModelOpts.push({ value: m, label: `${m} (${p.kind})` });
        }
      }
    }
    // Include current value if not in list
    if (config.default_model && !allModelOpts.find((o) => o.value === config.default_model)) {
      allModelOpts.splice(1, 0, { value: config.default_model, label: config.default_model });
    }

    const defModelRow = formRow('Default Model', 'Model ID to use — or type a custom one');
    const defModelInp = textInput(
      config.default_model ?? '',
      'gpt-4o, claude-sonnet-4-6, llama3.1:8b …',
    );
    defModelInp.style.maxWidth = '400px';
    defModelInp.setAttribute('list', 'default-model-datalist');
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

    defaultSection.appendChild(
      saveReloadButtons(
        async () => {
          const updated: EngineConfig = {
            ...config,
            default_provider: defProvSel.value || undefined,
            default_model: defModelInp.value.trim() || undefined,
          };
          const ok = await setEngineConfig(updated);
          if (ok) loadModelsSettings();
        },
        () => loadModelsSettings(),
      ),
    );
    container.appendChild(defaultSection);

    // ── Model Routing (Multi-Agent) ──────────────────────────────────────
    container.appendChild(buildModelRoutingSection(config, allModelOpts));

    // ── Available Models Reference ───────────────────────────────────────
    container.appendChild(buildAvailableModelsPanel(providers));

    // ── Provider Cards (edit/remove each) ────────────────────────────────
    const provHeader = document.createElement('div');
    provHeader.style.cssText =
      'display:flex;justify-content:space-between;align-items:center;margin-top:24px';
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

// ── Model Routing Section ───────────────────────────────────────────────────

function buildModelRoutingSection(
  config: EngineConfig,
  _allModelOpts: Array<{ value: string; label: string }>,
): HTMLDivElement {
  const section = document.createElement('div');
  section.className = 'settings-subsection';
  section.style.marginTop = '20px';
  section.innerHTML = `<h3 class="settings-subsection-title">Model Routing (Multi-Agent)</h3>
    <p class="settings-section-desc">Use different models for different roles. Enable <strong>Smart Auto-Tier</strong> to automatically use the cheapest model for simple tasks and upgrade for complex ones.</p>`;

  const routing = config.model_routing ?? {};

  // Build datalist for model suggestions
  const allKnownModels = buildAllKnownModels(config.providers ?? []);
  const dlId = 'routing-model-datalist';
  const dl = document.createElement('datalist');
  dl.id = dlId;
  for (const m of allKnownModels) {
    const o = document.createElement('option');
    o.value = m;
    dl.appendChild(o);
  }
  section.appendChild(dl);

  // ── Smart Auto-Tier Toggle ──
  const autoTierRow = formRow(
    'Smart Auto-Tier',
    'Automatically use cheap model for simple tasks, upgrade for complex ones',
  );
  const autoTierCheck = document.createElement('input');
  autoTierCheck.type = 'checkbox';
  autoTierCheck.checked = routing.auto_tier ?? false;
  autoTierCheck.style.cssText = 'width:18px;height:18px;cursor:pointer';
  const autoTierLabel = document.createElement('span');
  autoTierLabel.textContent = routing.auto_tier
    ? 'Enabled — saves cost on simple tasks'
    : 'Disabled — always uses default model';
  autoTierLabel.style.cssText = 'font-size:12px;color:var(--text-muted);margin-left:8px';
  autoTierCheck.addEventListener('change', () => {
    autoTierLabel.textContent = autoTierCheck.checked
      ? 'Enabled — saves cost on simple tasks'
      : 'Disabled — always uses default model';
    cheapRow.style.display = autoTierCheck.checked ? '' : 'none';
  });
  const autoTierWrap = document.createElement('div');
  autoTierWrap.style.cssText = 'display:flex;align-items:center';
  autoTierWrap.appendChild(autoTierCheck);
  autoTierWrap.appendChild(autoTierLabel);
  autoTierRow.appendChild(autoTierWrap);
  section.appendChild(autoTierRow);

  // Cheap Model (for auto-tier)
  const cheapRow = formRow(
    'Cheap Model (for simple tasks)',
    'Model used for greetings, status checks, single-tool calls',
  );
  const cheapInp = textInput(
    routing.cheap_model ?? '',
    'e.g. claude-3-haiku-20240307, gemini-2.0-flash',
  );
  cheapInp.style.maxWidth = '320px';
  cheapInp.setAttribute('list', dlId);
  cheapRow.appendChild(cheapInp);
  cheapRow.style.display = routing.auto_tier ? '' : 'none';
  section.appendChild(cheapRow);

  // Boss Model
  const bossRow = formRow(
    'Boss / Orchestrator Model',
    'Powerful model for the master agent that plans and delegates',
  );
  const bossInp = textInput(routing.boss_model ?? '', 'e.g. gemini-2.5-pro');
  bossInp.style.maxWidth = '320px';
  bossInp.setAttribute('list', dlId);
  bossRow.appendChild(bossInp);
  section.appendChild(bossRow);

  // Worker Model
  const workerRow = formRow(
    'Worker / Sub-Agent Model',
    'Cheaper/faster model for sub-agents executing tasks',
  );
  const workerInp = textInput(routing.worker_model ?? '', 'e.g. gemini-2.0-flash');
  workerInp.style.maxWidth = '320px';
  workerInp.setAttribute('list', dlId);
  workerRow.appendChild(workerInp);
  section.appendChild(workerRow);

  // Quick preset chips
  const presets = document.createElement('div');
  presets.style.cssText = 'display:flex;flex-wrap:wrap;gap:6px;margin:8px 0 16px 0';
  const presetOptions = [
    { label: 'Gemini Pro + Flash', boss: 'gemini-2.5-pro', worker: 'gemini-2.0-flash' },
    { label: 'Gemini Pro + Flash Lite', boss: 'gemini-2.5-pro', worker: 'gemini-2.0-flash-lite' },
    {
      label: 'Gemini Flash + Flash Lite',
      boss: 'gemini-2.5-flash',
      worker: 'gemini-2.0-flash-lite',
    },
    { label: 'GPT-4o + 4o-mini', boss: 'gpt-4o', worker: 'gpt-4o-mini' },
    { label: 'Claude Opus + Haiku', boss: 'claude-opus-4-6', worker: 'claude-haiku-4-5-20251001' },
  ];
  for (const p of presetOptions) {
    const chip = document.createElement('button');
    chip.className = 'btn btn-ghost btn-sm';
    chip.style.cssText =
      'font-size:11px;padding:3px 10px;border-radius:12px;border:1px solid var(--border)';
    chip.textContent = p.label;
    chip.addEventListener('click', () => {
      bossInp.value = p.boss;
      workerInp.value = p.worker;
    });
    presets.appendChild(chip);
  }
  section.appendChild(presets);

  // Specialty Overrides
  const specSection = document.createElement('div');
  specSection.style.cssText = 'margin-top:16px';
  specSection.innerHTML = `<div style="font-weight:600;font-size:13px;margin-bottom:8px">Per-Specialty Overrides <span style="font-weight:normal;color:var(--text-muted)">(optional)</span></div>
    <p style="font-size:12px;color:var(--text-muted);margin:0 0 8px 0">Assign specific models to agent specialties. Leave blank to use the Worker model.</p>`;

  const specModels: Record<string, string> = { ...(routing.specialty_models ?? {}) };
  const specGrid = document.createElement('div');
  specGrid.style.cssText =
    'display:grid;grid-template-columns:120px 1fr;gap:6px 12px;align-items:center';
  const specInputs: Record<string, HTMLInputElement> = {};

  for (const spec of SPECIALTIES) {
    const label = document.createElement('span');
    label.style.cssText = 'font-size:12px;text-transform:capitalize;color:var(--text-muted)';
    label.textContent = spec;
    specGrid.appendChild(label);

    const inp = textInput(specModels[spec] ?? '', `default (use worker model)`);
    inp.style.cssText = 'font-size:12px;padding:4px 8px;max-width:280px';
    inp.setAttribute('list', dlId);
    specInputs[spec] = inp;
    specGrid.appendChild(inp);
  }
  specSection.appendChild(specGrid);
  section.appendChild(specSection);

  // Save button
  section.appendChild(
    saveReloadButtons(
      async () => {
        const specialtyModels: Record<string, string> = {};
        for (const [spec, inp] of Object.entries(specInputs)) {
          const val = inp.value.trim();
          if (val) specialtyModels[spec] = val;
        }

        const newRouting: ModelRouting = {
          boss_model: bossInp.value.trim() || undefined,
          worker_model: workerInp.value.trim() || undefined,
          specialty_models: Object.keys(specialtyModels).length > 0 ? specialtyModels : undefined,
          agent_models: routing.agent_models,
          cheap_model: cheapInp.value.trim() || undefined,
          auto_tier: autoTierCheck.checked,
        };

        const updated: EngineConfig = {
          ...config,
          model_routing: newRouting,
        };
        const ok = await setEngineConfig(updated);
        if (ok) loadModelsSettings();
      },
      () => loadModelsSettings(),
    ),
  );

  return section;
}

// ── Available Models Panel ──────────────────────────────────────────────────

function buildAvailableModelsPanel(providers: EngineProviderConfig[]): HTMLDivElement {
  const section = document.createElement('div');
  section.className = 'settings-subsection';
  section.style.marginTop = '20px';

  section.innerHTML = `<h3 class="settings-subsection-title">Available Models</h3>
    <p class="settings-section-desc">All models available from your configured providers. Click any model to copy its ID — paste it into task model overrides, agent routing, etc.</p>`;

  if (providers.length === 0) {
    section.innerHTML += `<p style="color:var(--text-muted);font-size:13px;padding:12px 0">Add a provider above to see available models.</p>`;
    return section;
  }

  for (const p of providers) {
    const models = POPULAR_MODELS[p.kind] ?? [];
    if (!models.length) continue;

    const iconHtml2 = `<span class="ms ms-sm">${KIND_ICONS[p.kind] ?? 'build'}</span>`;
    const provBlock = document.createElement('div');
    provBlock.style.cssText = 'margin:12px 0 16px 0';

    const provTitle = document.createElement('div');
    provTitle.style.cssText =
      'font-weight:600;font-size:13px;margin-bottom:6px;display:flex;align-items:center;gap:6px';
    provTitle.innerHTML = `${iconHtml2} ${esc(p.id)} <span style="font-size:11px;color:var(--text-muted);font-weight:normal">${esc(p.kind)}</span>`;
    provBlock.appendChild(provTitle);

    if (p.default_model) {
      const activeTag = document.createElement('div');
      activeTag.style.cssText = 'font-size:11px;color:var(--text-muted);margin-bottom:6px';
      activeTag.innerHTML = `Currently using: <strong style="color:var(--accent)">${esc(p.default_model)}</strong>`;
      provBlock.appendChild(activeTag);
    }

    const chipsWrap = document.createElement('div');
    chipsWrap.style.cssText = 'display:flex;flex-wrap:wrap;gap:6px';

    const tierInfo = TIER_LABELS[p.kind] ?? {};

    for (const m of models) {
      const chip = document.createElement('button');
      chip.className = 'btn btn-ghost btn-sm';

      const isActive = m === p.default_model;
      const tier = tierInfo[m] ?? '';

      chip.style.cssText = `font-size:11px;padding:4px 10px;border-radius:6px;border:1px solid ${isActive ? 'var(--accent)' : 'var(--border)'};font-family:monospace;cursor:pointer;position:relative;${isActive ? 'background:rgba(var(--accent-rgb,99,102,241),0.15);color:var(--accent);font-weight:600' : ''}`;
      chip.textContent = m;
      if (tier) chip.title = tier;

      chip.addEventListener('click', () => {
        navigator.clipboard.writeText(m).then(() => {
          const orig = chip.textContent;
          chip.textContent = '✓ Copied!';
          chip.style.color = 'var(--status-success)';
          setTimeout(() => {
            chip.textContent = orig;
            chip.style.color = isActive ? 'var(--accent)' : '';
          }, 1200);
        });
      });

      chipsWrap.appendChild(chip);
    }
    provBlock.appendChild(chipsWrap);

    if (Object.keys(tierInfo).length > 0) {
      const legend = document.createElement('div');
      legend.style.cssText = 'font-size:10px;color:var(--text-muted);margin-top:6px';
      legend.textContent = 'Hover for model tier info. Click to copy model ID.';
      provBlock.appendChild(legend);
    }

    section.appendChild(provBlock);
  }

  return section;
}

// ── Add Provider Form ───────────────────────────────────────────────────────

function buildAddProviderForm(config: EngineConfig): HTMLDivElement {
  const form = document.createElement('div');
  form.id = 'add-provider-form';
  form.style.cssText =
    'display:none;margin-top:12px;padding:16px;border:1px solid var(--accent);border-radius:8px;background:var(--bg-secondary, rgba(255,255,255,0.03))';
  form.innerHTML = `<h4 style="margin:0 0 12px 0;font-size:14px">New Provider</h4>`;

  const idRow = formRow(
    'Provider ID',
    'Unique lowercase identifier (e.g. my-openai, ollama-local)',
  );
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

  kindSel.addEventListener('change', () => {
    const kind = kindSel.value;
    if (!urlInp.value || Object.values(DEFAULT_BASE_URLS).includes(urlInp.value)) {
      urlInp.value = DEFAULT_BASE_URLS[kind] ?? '';
    }
    urlInp.placeholder = DEFAULT_BASE_URLS[kind] ?? '';
    if (!idInp.value) {
      idInp.value = kind;
    }
    const models = POPULAR_MODELS[kind] ?? [];
    if (models.length && !modelInp.value) {
      modelInp.placeholder = models[0];
    }
  });

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
    if (config.providers.some((p) => p.id === id)) {
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
  cancelBtn.addEventListener('click', () => {
    form.style.display = 'none';
  });
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
  card.style.cssText =
    'margin-top:12px;padding:16px;border:1px solid var(--border);border-radius:8px;';

  const iconHtml3 = `<span class="ms ms-sm">${KIND_ICONS[provider.kind] ?? 'build'}</span>`;
  const isDefault = provider.id === config.default_provider;

  const header = document.createElement('div');
  header.style.cssText =
    'display:flex;justify-content:space-between;align-items:center;margin-bottom:12px';
  const titleWrap = document.createElement('div');
  titleWrap.style.cssText = 'display:flex;align-items:center;gap:8px';
  titleWrap.innerHTML = `${iconHtml3}
    <strong style="font-size:14px">${esc(provider.id)}</strong>
    <span style="font-size:11px;color:var(--text-muted);background:var(--bg-tertiary,rgba(255,255,255,0.06));padding:2px 8px;border-radius:4px">${esc(provider.kind)}</span>
    ${isDefault ? '<span style="font-size:10px;color:var(--accent);background:rgba(var(--accent-rgb,99,102,241),0.15);padding:2px 8px;border-radius:4px">★ default</span>' : ''}`;
  header.appendChild(titleWrap);

  const actions = document.createElement('div');
  actions.style.cssText = 'display:flex;gap:6px';

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

  const kindRow = formRow('Provider Type');
  const kindSel = selectInput(PROVIDER_KINDS, provider.kind);
  kindSel.style.maxWidth = '260px';
  kindRow.appendChild(kindSel);
  card.appendChild(kindRow);

  const urlRow = formRow('Base URL');
  const urlInp = textInput(
    provider.base_url ?? '',
    DEFAULT_BASE_URLS[provider.kind] ?? 'https://api.example.com/v1',
  );
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

  if (popular.length) {
    const chipsWrap = document.createElement('div');
    chipsWrap.style.cssText = 'display:flex;flex-wrap:wrap;gap:4px;margin-top:4px';
    for (const m of popular.slice(0, 6)) {
      const chip = document.createElement('button');
      chip.className = 'btn btn-ghost btn-sm';
      chip.style.cssText =
        'font-size:11px;padding:2px 8px;border-radius:12px;border:1px solid var(--border)';
      chip.textContent = m;
      chip.addEventListener('click', () => {
        modelInp.value = m;
      });
      chipsWrap.appendChild(chip);
    }
    card.appendChild(chipsWrap);
  }

  card.appendChild(
    saveReloadButtons(
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
          const modelMsg = updated.default_model ? ` — model: ${updated.default_model}` : '';
          showToast(`Provider "${provider.id}" updated${modelMsg}`, 'success');
          const refreshFn = (window as unknown as Record<string, unknown>).__refreshModelLabel as
            | (() => void)
            | undefined;
          if (refreshFn) refreshFn();
          loadModelsSettings();
        } catch (e) {
          showToast(`Save failed: ${e instanceof Error ? e.message : e}`, 'error');
        }
      },
      () => loadModelsSettings(),
    ),
  );

  return card;
}
