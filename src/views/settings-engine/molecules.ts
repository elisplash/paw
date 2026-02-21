// Settings: Engine — DOM rendering + IPC

import { pawEngine, type EngineProviderConfig } from '../../engine';
import { setEngineMode } from '../../engine-bridge';
import { $, escHtml, confirmModal } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { KIND_LABELS, ID_LABELS } from './atoms';

/** Initialize engine settings UI — call once on app load. */
export function initEngineSettings(): void {
  const modeSelect = $('settings-engine-mode') as HTMLSelectElement | null;
  const configPanel = $('engine-config-panel');
  const providerKind = $('engine-provider-kind') as HTMLSelectElement | null;
  const apiKeyInput = $('engine-api-key') as HTMLInputElement | null;
  const modelInput = $('engine-model') as HTMLInputElement | null;
  const baseUrlInput = $('engine-base-url') as HTMLInputElement | null;
  const saveBtn = $('engine-save-btn');
  const saveStatus = $('engine-save-status');

  if (!modeSelect) return;

  // Pawz always runs in engine mode — force it
  modeSelect.value = 'engine';
  setEngineMode(true);
  if (configPanel) configPanel.style.display = '';

  // Mode toggle
  modeSelect.addEventListener('change', () => {
    const engineMode = modeSelect.value === 'engine';
    setEngineMode(engineMode);
    if (configPanel) configPanel.style.display = engineMode ? '' : 'none';

    if (saveStatus) {
      saveStatus.textContent = 'Reload the app to switch modes';
      saveStatus.style.color = 'var(--text-warning, orange)';
    }
  });

  // Provider kind change → auto-fill base URL and model from presets
  providerKind?.addEventListener('change', () => {
    const selected = providerKind.selectedOptions[0];
    const presetUrl = selected?.dataset.baseUrl;
    const presetModel = selected?.dataset.model;
    if (presetUrl !== undefined && baseUrlInput) {
      baseUrlInput.value = presetUrl;
    }
    if (presetModel && modelInput) {
      modelInput.value = presetModel;
    }
    // Clear API key for new provider
    if (apiKeyInput) apiKeyInput.value = '';
  });

  // Load current config from engine
  loadEngineConfig().catch(console.error);

  // Save button → adds/updates a provider
  saveBtn?.addEventListener('click', async () => {
    try {
      const selected = providerKind?.selectedOptions[0];
      const providerId = providerKind?.value || 'anthropic';
      const kind = (selected?.dataset.kind || providerId) as EngineProviderConfig['kind'];
      const apiKey = apiKeyInput?.value?.trim() || '';
      const model = modelInput?.value?.trim() || '';
      const baseUrl = baseUrlInput?.value?.trim() || undefined;

      if (!apiKey && kind !== 'ollama') {
        if (saveStatus) {
          saveStatus.textContent = 'API key is required';
          saveStatus.style.color = 'var(--text-danger, red)';
        }
        return;
      }

      const provider: EngineProviderConfig = {
        id: providerId,
        kind,
        api_key: apiKey,
        base_url: baseUrl,
        default_model: model || undefined,
      };

      await pawEngine.upsertProvider(provider);

      // Set as default if it's the first provider or if a model is specified
      if (model) {
        const config = await pawEngine.getConfig();
        // Only set as default if no default_model yet or this is the first provider
        if (!config.default_model || config.providers.length <= 1) {
          config.default_model = model;
          config.default_provider = providerId;
          await pawEngine.setConfig(config);
        }
      }

      if (saveStatus) {
        saveStatus.textContent = 'Saved! ✓';
        saveStatus.style.color = 'var(--text-success, green)';
        setTimeout(() => {
          if (saveStatus) saveStatus.textContent = '';
        }, 3000);
      }

      // Refresh provider list and clear the form
      await renderProvidersList();
      if (apiKeyInput) apiKeyInput.value = '';
      if (modelInput) modelInput.value = '';
      if (baseUrlInput) baseUrlInput.value = '';
      if (providerKind) providerKind.selectedIndex = 0;
    } catch (e) {
      console.error('[engine-settings] Save failed:', e);
      if (saveStatus) {
        saveStatus.textContent = `Error: ${e}`;
        saveStatus.style.color = 'var(--text-danger, red)';
      }
    }
  });
}

/** Render the list of currently configured providers. */
async function renderProvidersList(): Promise<void> {
  const list = $('engine-providers-list');
  if (!list) return;

  try {
    const config = await pawEngine.getConfig();
    const providers = config.providers ?? [];

    if (!providers.length) {
      list.innerHTML =
        '<div style="color:var(--text-muted);font-size:13px;padding:8px 0">No providers configured yet. Add one above.</div>';
      return;
    }

    list.innerHTML = `<label class="form-label" style="margin-top:12px">Configured Providers</label>${providers
      .map((p) => {
        const label = ID_LABELS[p.id] || KIND_LABELS[p.kind] || p.id;
        const isDefault = p.id === config.default_provider;
        return `<div class="engine-provider-row" style="display:flex;align-items:center;gap:8px;padding:6px 10px;margin:4px 0;background:var(--bg-secondary);border-radius:6px;font-size:13px">
          <span style="flex:1">
            <strong>${escHtml(label)}</strong>
            <span style="color:var(--text-muted);margin-left:6px">${escHtml(p.default_model ?? '')}${p.base_url ? ` · ${escHtml(p.base_url)}` : ''}</span>
            ${isDefault ? '<span style="color:var(--accent);margin-left:6px">★ default</span>' : ''}
          </span>
          <span style="color:var(--text-muted)">${p.api_key ? '<span class="ms ms-sm">key</span>' : p.kind === 'ollama' ? '<span class="ms ms-sm">home</span>' : '<span class="ms ms-sm">warning</span>'}</span>
          ${!isDefault ? `<button class="btn btn-ghost btn-sm engine-set-default" data-id="${escHtml(p.id)}" title="Set as default" style="padding:2px 6px">★</button>` : ''}
          <button class="btn btn-ghost btn-sm engine-edit-provider" data-id="${escHtml(p.id)}" title="Edit" style="padding:2px 6px">✎</button>
          <button class="btn btn-ghost btn-sm engine-remove-provider" data-id="${escHtml(p.id)}" title="Remove" style="padding:2px 6px;color:var(--text-danger,red)">✕</button>
        </div>`;
      })
      .join('')}`;

    // Wire remove buttons
    list.querySelectorAll('.engine-remove-provider').forEach((btn) => {
      btn.addEventListener('click', async () => {
        const id = (btn as HTMLElement).dataset.id!;
        if (!(await confirmModal(`Remove provider "${id}"?`))) return;
        try {
          await pawEngine.removeProvider(id);
          await renderProvidersList();
        } catch (e) {
          showToast(`Failed: ${e}`, 'error');
        }
      });
    });

    // Wire set-default buttons
    list.querySelectorAll('.engine-set-default').forEach((btn) => {
      btn.addEventListener('click', async () => {
        const id = (btn as HTMLElement).dataset.id!;
        try {
          const cfg = await pawEngine.getConfig();
          cfg.default_provider = id;
          const p = cfg.providers.find((pr) => pr.id === id);
          if (p?.default_model) cfg.default_model = p.default_model;
          await pawEngine.setConfig(cfg);
          await renderProvidersList();
        } catch (e) {
          showToast(`Failed: ${e}`, 'error');
        }
      });
    });

    // Wire edit buttons
    list.querySelectorAll('.engine-edit-provider').forEach((btn) => {
      btn.addEventListener('click', () => {
        const id = (btn as HTMLElement).dataset.id!;
        const p = providers.find((pr) => pr.id === id);
        if (!p) return;
        const providerKind = $('engine-provider-kind') as HTMLSelectElement | null;
        const apiKeyInput = $('engine-api-key') as HTMLInputElement | null;
        const modelInput = $('engine-model') as HTMLInputElement | null;
        const baseUrlInput = $('engine-base-url') as HTMLInputElement | null;
        // Select the right option by value
        if (providerKind) {
          // Try to find option matching provider id, fallback to kind
          const opt =
            Array.from(providerKind.options).find((o) => o.value === p.id) ??
            Array.from(providerKind.options).find((o) => o.value === p.kind);
          if (opt) providerKind.value = opt.value;
        }
        if (apiKeyInput) apiKeyInput.value = p.api_key;
        if (modelInput) modelInput.value = p.default_model ?? '';
        if (baseUrlInput) baseUrlInput.value = p.base_url ?? '';
      });
    });
  } catch (e) {
    console.debug('[engine-settings] Could not load providers:', e);
  }
}

/** Load existing engine config and populate the form. */
async function loadEngineConfig(): Promise<void> {
  try {
    const config = await pawEngine.getConfig();
    const providerKind = $('engine-provider-kind') as HTMLSelectElement | null;
    const modelInput = $('engine-model') as HTMLInputElement | null;

    // Show the default model in the model field as placeholder
    if (config.default_model && modelInput && !modelInput.value) {
      modelInput.placeholder = config.default_model;
    }

    // If only one provider, pre-fill the form for quick editing
    if (config.providers.length === 1) {
      const p = config.providers[0];
      const apiKeyInput = $('engine-api-key') as HTMLInputElement | null;
      const baseUrlInput = $('engine-base-url') as HTMLInputElement | null;
      if (providerKind) {
        const opt =
          Array.from(providerKind.options).find((o) => o.value === p.id) ??
          Array.from(providerKind.options).find((o) => o.value === p.kind);
        if (opt) providerKind.value = opt.value;
      }
      if (apiKeyInput) apiKeyInput.value = p.api_key;
      if (modelInput) modelInput.value = p.default_model ?? config.default_model ?? '';
      if (baseUrlInput) baseUrlInput.value = p.base_url ?? '';
    }

    // Render the providers list
    await renderProvidersList();
  } catch (e) {
    console.debug('[engine-settings] Could not load engine config:', e);
  }
}
