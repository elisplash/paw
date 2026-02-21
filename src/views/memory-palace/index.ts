// Memory Palace — Index (state, orchestration, install flow, event wiring, exports)

import { pawEngine } from '../../engine';
import { $ } from '../../components/helpers';
import { isConnected } from '../../state/connection';
import {
  updateProviderFields,
  readMemoryForm,
  initPalaceTabs,
  initPalaceRecall,
  initPalaceRemember,
  loadPalaceStats,
  loadPalaceSidebar,
  palaceRecallById,
  exportMemories,
} from './molecules';
import { initPalaceGraph } from './graph';

// ── Re-exports ─────────────────────────────────────────────────────────────

export type { MemoryFormData, RecallCardData } from './atoms';
export { validateMemoryForm, CATEGORY_COLORS } from './atoms';
export {
  renderRecallCard,
  palaceRecallById,
  loadPalaceStats,
  loadPalaceSidebar,
} from './molecules';
export { renderPalaceGraph } from './graph';

// ── Module state ───────────────────────────────────────────────────────────

let _palaceInitialized = false;
let _palaceAvailable = false;
let _palaceSkipped = false;

export function setCurrentSessionKey(_key: string | null): void {
  // Reserved for session-aware memory queries
}

export function isPalaceAvailable(): boolean {
  return _palaceAvailable;
}

export function resetPalaceState(): void {
  _palaceInitialized = false;
  _palaceSkipped = false;
}

// ── Main loader ────────────────────────────────────────────────────────────

export async function loadMemoryPalace(): Promise<void> {
  if (!isConnected()) return;

  // Check if memory is available
  if (!_palaceInitialized) {
    _palaceInitialized = true;

    // Engine mode: memory is always available (SQLite-backed)
    try {
      const stats = await pawEngine.memoryStats();
      _palaceAvailable = true;
      console.debug('[memory] Engine mode — memory available, total:', stats.total_memories);
    } catch (e) {
      console.warn('[memory] Engine mode — memory check failed:', e);
      _palaceAvailable = true; // Still available, just might not have embeddings
    }
  }

  initPalaceTabs();
  initPalaceRecall();
  initPalaceRemember(async () => {
    await loadPalaceSidebar((id) => palaceRecallById(id));
    await loadPalaceStats();
  });
  initPalaceGraph();
  initPalaceInstall();

  const banner = $('palace-install-banner');
  const filesDivider = $('palace-files-divider');

  if (!_palaceAvailable && !_palaceSkipped) {
    if (banner) banner.style.display = 'flex';
  } else if (!_palaceAvailable && _palaceSkipped) {
    if (banner) banner.style.display = 'none';
    document.querySelectorAll('.palace-tab').forEach((t) => t.classList.remove('active'));
    document
      .querySelectorAll('.palace-panel')
      .forEach((p) => ((p as HTMLElement).style.display = 'none'));
    document.querySelector('.palace-tab[data-palace-tab="files"]')?.classList.add('active');
    const fp = $('palace-files-panel');
    if (fp) fp.style.display = 'flex';
    if (filesDivider) filesDivider.style.display = 'none';
    const memoryListBelow = $('memory-list');
    if (memoryListBelow) memoryListBelow.style.display = 'none';
  } else {
    if (banner) banner.style.display = 'none';
    if (filesDivider) filesDivider.style.display = '';
    const settingsBtn = $('palace-settings');
    if (settingsBtn) settingsBtn.style.display = 'none';
  }

  if (_palaceAvailable) {
    await loadPalaceStats();
    await loadPalaceSidebar((id) => palaceRecallById(id));
  }
}

// ── Install / Setup form ───────────────────────────────────────────────────

let _installBound = false;
function initPalaceInstall(): void {
  if (_installBound) return;
  _installBound = true;
  // Provider dropdown — toggle fields on change
  $('palace-provider')?.addEventListener('change', updateProviderFields);
  updateProviderFields();

  // Settings gear — show the setup banner for reconfiguration
  $('palace-settings')?.addEventListener('click', async () => {
    const banner = $('palace-install-banner');
    if (!banner) return;
    banner.style.display = 'flex';
    // Pre-fill with existing settings
    try {
      const existingProvider = await pawEngine.getEmbeddingProvider();
      const existingUrl = await pawEngine.getEmbeddingBaseUrl();
      const existingVersion = await pawEngine.getAzureApiVersion();
      const providerSel = $('palace-provider') as HTMLSelectElement | null;
      if (existingProvider && providerSel) providerSel.value = existingProvider;
      updateProviderFields();
      if (existingProvider === 'azure') {
        const baseUrlInput = $('palace-base-url') as HTMLInputElement | null;
        if (existingUrl && baseUrlInput) baseUrlInput.value = existingUrl;
      } else {
        const openaiUrlInput = $('palace-base-url-openai') as HTMLInputElement | null;
        if (existingUrl && openaiUrlInput) openaiUrlInput.value = existingUrl;
      }
      const apiVersionInput = $('palace-api-version') as HTMLInputElement | null;
      if (existingVersion && apiVersionInput) apiVersionInput.value = existingVersion;
    } catch {
      /* ignore */
    }
    // Update button text to indicate reconfiguration
    const btn = $('palace-install-btn') as HTMLButtonElement | null;
    if (btn) {
      btn.textContent = 'Save & Restart';
      btn.disabled = false;
    }
    const progressDiv = $('palace-install-progress');
    if (progressDiv) progressDiv.style.display = 'none';
  });

  // ── Test Connection button ──
  $('palace-test-btn')?.addEventListener('click', async () => {
    const testBtn = $('palace-test-btn') as HTMLButtonElement | null;
    const progressDiv = $('palace-install-progress');
    const progressText = $('palace-progress-text') as HTMLElement | null;
    if (!testBtn) return;

    const form = readMemoryForm();
    if (!form) return;

    testBtn.disabled = true;
    testBtn.textContent = 'Testing…';
    if (progressDiv) progressDiv.style.display = '';
    if (progressText) progressText.textContent = 'Testing embedding endpoint…';

    try {
      await pawEngine.testEmbeddingConnection({
        apiKey: form.apiKey,
        baseUrl: form.baseUrl || null,
        model: form.modelName || null,
        apiVersion: form.apiVersion || null,
        provider: form.provider,
      });
      if (progressText) progressText.textContent = 'Connection test passed ✓';
    } catch (testErr: unknown) {
      const errMsg =
        typeof testErr === 'string' ? testErr : (testErr as Error)?.message || String(testErr);
      if (progressText) progressText.textContent = `Connection test failed: ${errMsg}`;
    } finally {
      testBtn.disabled = false;
      testBtn.textContent = 'Test Connection';
    }
  });

  // ── Enable / Save button ──
  $('palace-install-btn')?.addEventListener('click', async () => {
    const btn = $('palace-install-btn') as HTMLButtonElement | null;
    const progressDiv = $('palace-install-progress');
    const progressText = $('palace-progress-text') as HTMLElement | null;
    if (!btn) return;

    const form = readMemoryForm();
    if (!form) return;
    const { apiKey, baseUrl, modelName, apiVersion, provider } = form;

    btn.disabled = true;
    btn.textContent = 'Testing connection…';
    if (progressDiv) progressDiv.style.display = '';
    if (progressText) progressText.textContent = 'Testing embedding endpoint…';

    try {
      // Step 1: Test the embedding connection before saving
      try {
        await pawEngine.testEmbeddingConnection({
          apiKey,
          baseUrl: baseUrl || null,
          model: modelName || null,
          apiVersion: apiVersion || null,
          provider,
        });
        if (progressText)
          progressText.textContent = 'Connection test passed ✓ Saving configuration…';
      } catch (testErr: unknown) {
        const errMsg =
          typeof testErr === 'string' ? testErr : (testErr as Error)?.message || String(testErr);
        if (progressText) progressText.textContent = `Connection test failed: ${errMsg}`;
        btn.textContent = 'Retry';
        btn.disabled = false;
        return;
      }

      btn.textContent = 'Saving…';

      // Step 2: Write config
      await pawEngine.enableMemoryPlugin({
        apiKey,
        baseUrl: baseUrl || null,
        model: modelName || null,
        apiVersion: apiVersion || null,
        provider,
      });

      if (progressText) progressText.textContent = 'Configuration saved! Reloading…';

      // Reload memory palace to pick up the new config
      _palaceInitialized = false;
      _palaceAvailable = false;

      try {
        const configured = await pawEngine.checkMemoryConfigured();
        _palaceAvailable = configured;
      } catch {
        /* ignore */
      }

      if (_palaceAvailable) {
        const banner = $('palace-install-banner');
        if (banner) banner.style.display = 'none';
        _palaceInitialized = false;
        await loadMemoryPalace();
      } else {
        if (progressText) {
          progressText.textContent =
            'Configuration saved. Memory plugin may need additional setup.';
        }
        btn.textContent = 'Retry';
        btn.disabled = false;
        btn.onclick = async () => {
          btn.disabled = true;
          btn.textContent = 'Checking…';
          try {
            _palaceInitialized = false;
            await loadMemoryPalace();
          } catch (e) {
            if (progressText) progressText.textContent = `Setup check failed: ${e}`;
            btn.disabled = false;
            btn.textContent = 'Retry';
          }
        };
      }
    } catch (e) {
      if (progressText) progressText.textContent = `Error: ${e}`;
      btn.textContent = 'Retry';
      btn.disabled = false;
    }
  });

  // Skip button
  $('palace-skip-btn')?.addEventListener('click', () => {
    _palaceSkipped = true;
    const banner = $('palace-install-banner');
    if (banner) banner.style.display = 'none';

    document.querySelectorAll('.palace-tab').forEach((t) => t.classList.remove('active'));
    document
      .querySelectorAll('.palace-panel')
      .forEach((p) => ((p as HTMLElement).style.display = 'none'));
    document.querySelector('.palace-tab[data-palace-tab="files"]')?.classList.add('active');
    const fp = $('palace-files-panel');
    if (fp) fp.style.display = 'flex';

    const filesDivider = $('palace-files-divider');
    if (filesDivider) filesDivider.style.display = 'none';
    const memoryListBelow = $('memory-list');
    if (memoryListBelow) memoryListBelow.style.display = 'none';
  });
}

// ── UI event wiring ────────────────────────────────────────────────────────

export function initPalaceEvents(): void {
  // Refresh button
  $('palace-refresh')?.addEventListener('click', async () => {
    _palaceInitialized = false;
    _palaceSkipped = false;
    await loadMemoryPalace();
  });

  // Export button
  $('palace-export')?.addEventListener('click', () => {
    exportMemories();
  });

  // Sidebar search filter
  $('palace-search')?.addEventListener('input', () => {
    const query = (($('palace-search') as HTMLInputElement)?.value ?? '').toLowerCase();
    document.querySelectorAll('.palace-memory-card').forEach((card) => {
      const text = card.textContent?.toLowerCase() ?? '';
      (card as HTMLElement).style.display = text.includes(query) ? '' : 'none';
    });
  });
}
