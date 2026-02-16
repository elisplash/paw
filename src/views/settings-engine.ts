// Settings — Paw Engine Configuration
// Manages the runtime mode toggle and AI provider settings.

import { pawEngine, type EngineProviderConfig } from '../engine';
import { setEngineMode } from '../engine-bridge';

const $ = (id: string) => document.getElementById(id);

/** Initialize engine settings UI — call once on app load. */
export function initEngineSettings(): void {
  const modeSelect = $('settings-engine-mode') as HTMLSelectElement | null;
  const configPanel = $('engine-config-panel');
  const providerKind = $('engine-provider-kind') as HTMLSelectElement | null;
  const apiKeyInput = $('engine-api-key') as HTMLInputElement | null;
  const modelInput = $('engine-model') as HTMLInputElement | null;
  const baseUrlInput = $('engine-base-url') as HTMLInputElement | null;
  const baseUrlGroup = $('engine-base-url-group');
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

    // Show reload prompt
    if (saveStatus) {
      saveStatus.textContent = 'Reload the app to switch modes';
      saveStatus.style.color = 'var(--text-warning, orange)';
    }
  });

  // Provider kind → show/hide base URL
  const updateBaseUrlVisibility = () => {
    const kind = providerKind?.value ?? '';
    const showUrl = kind === 'custom' || kind === 'ollama';
    if (baseUrlGroup) baseUrlGroup.style.display = showUrl ? '' : 'none';
  };
  providerKind?.addEventListener('change', updateBaseUrlVisibility);
  updateBaseUrlVisibility();

  // Load current config from engine
  loadEngineConfig().catch(console.error);

  // Save button
  saveBtn?.addEventListener('click', async () => {
    try {
      const kind = providerKind?.value || 'anthropic';
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
        id: kind,
        kind: kind as EngineProviderConfig['kind'],
        api_key: apiKey,
        base_url: baseUrl,
        default_model: model || undefined,
      };

      await pawEngine.upsertProvider(provider);

      // Also set it as the default model if specified
      if (model) {
        const config = await pawEngine.getConfig();
        config.default_model = model;
        config.default_provider = kind;
        await pawEngine.setConfig(config);
      }

      if (saveStatus) {
        saveStatus.textContent = 'Saved! ✓';
        saveStatus.style.color = 'var(--text-success, green)';
        setTimeout(() => { if (saveStatus) saveStatus.textContent = ''; }, 3000);
      }
    } catch (e) {
      console.error('[engine-settings] Save failed:', e);
      if (saveStatus) {
        saveStatus.textContent = `Error: ${e}`;
        saveStatus.style.color = 'var(--text-danger, red)';
      }
    }
  });
}

/** Load existing engine config and populate the form. */
async function loadEngineConfig(): Promise<void> {
  try {
    const config = await pawEngine.getConfig();
    const providerKind = $('engine-provider-kind') as HTMLSelectElement | null;
    const apiKeyInput = $('engine-api-key') as HTMLInputElement | null;
    const modelInput = $('engine-model') as HTMLInputElement | null;
    const baseUrlInput = $('engine-base-url') as HTMLInputElement | null;

    // Use the first configured provider
    if (config.providers.length > 0) {
      const p = config.providers[0];
      if (providerKind) providerKind.value = p.kind;
      if (apiKeyInput) apiKeyInput.value = p.api_key;
      if (modelInput) modelInput.value = p.default_model ?? config.default_model ?? '';
      if (baseUrlInput) baseUrlInput.value = p.base_url ?? '';
    } else if (config.default_model) {
      if (modelInput) modelInput.value = config.default_model;
    }
  } catch (e) {
    // Engine not available (e.g., running in browser dev mode)
    console.debug('[engine-settings] Could not load engine config:', e);
  }
}
