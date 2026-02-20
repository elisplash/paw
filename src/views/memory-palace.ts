// Memory Palace View — LanceDB-backed long-term memory
// Extracted from main.ts for maintainability

import { pawEngine } from '../engine';

const $ = (id: string) => document.getElementById(id);

// ── Tauri bridge ───────────────────────────────────────────────────────────
interface TauriWindow {
  __TAURI__?: {
    core: { invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T> };
  };
}
const tauriWindow = window as unknown as TauriWindow;
const invoke = tauriWindow.__TAURI__?.core?.invoke;

// ── Module state ───────────────────────────────────────────────────────────
let _palaceInitialized = false;
let _palaceAvailable = false;
let _palaceSkipped = false;
let wsConnected = false;

export function setWsConnected(connected: boolean) {
  wsConnected = connected;
}

export function setCurrentSessionKey(_key: string | null) {
  // Reserved for session-aware memory queries
}

export function isPalaceAvailable(): boolean {
  return _palaceAvailable;
}

export function resetPalaceState() {
  _palaceInitialized = false;
  _palaceSkipped = false;
}

// ── Helpers ────────────────────────────────────────────────────────────────
function escHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

function showToast(message: string, type: 'info' | 'success' | 'error' | 'warning' = 'info', durationMs = 3500) {
  const existing = document.querySelector('.toast-notification');
  if (existing) existing.remove();

  const toast = document.createElement('div');
  toast.className = `toast-notification toast-${type}`;
  toast.textContent = message;
  document.body.appendChild(toast);

  requestAnimationFrame(() => toast.classList.add('show'));
  setTimeout(() => {
    toast.classList.remove('show');
    setTimeout(() => toast.remove(), 300);
  }, durationMs);
}

// ── Main loader ────────────────────────────────────────────────────────────
export async function loadMemoryPalace() {
  if (!wsConnected) return;

  // Check if memory is available
  if (!_palaceInitialized) {
    _palaceInitialized = true;

    // Engine mode: memory is always available (SQLite-backed)
    // Check if embedding is configured for semantic search
    try {
      const stats = await pawEngine.memoryStats();
      _palaceAvailable = true;
      console.log('[memory] Engine mode — memory available, total:', stats.total_memories);
    } catch (e) {
      console.warn('[memory] Engine mode — memory check failed:', e);
      _palaceAvailable = true; // Still available, just might not have embeddings
    }
  }

  initPalaceTabs();
  initPalaceRecall();
  initPalaceRemember();
  initPalaceGraph();
  initPalaceInstall();

  const banner = $('palace-install-banner');
  const filesDivider = $('palace-files-divider');

  if (!_palaceAvailable && !_palaceSkipped) {
    // Show setup banner
    if (banner) banner.style.display = 'flex';
    } else if (!_palaceAvailable && _palaceSkipped) {
      // Skipped — show files mode
      if (banner) banner.style.display = 'none';
      document.querySelectorAll('.palace-tab').forEach(t => t.classList.remove('active'));
      document.querySelectorAll('.palace-panel').forEach(p => (p as HTMLElement).style.display = 'none');
      document.querySelector('.palace-tab[data-palace-tab="files"]')?.classList.add('active');
      const fp = $('palace-files-panel');
      if (fp) fp.style.display = 'flex';
      if (filesDivider) filesDivider.style.display = 'none';
      const memoryListBelow = $('memory-list');
      if (memoryListBelow) memoryListBelow.style.display = 'none';
    } else {
      // Memory is available — full mode
      if (banner) banner.style.display = 'none';
      if (filesDivider) filesDivider.style.display = '';
      // Settings gear visibility
      const settingsBtn = $('palace-settings');
      if (settingsBtn) settingsBtn.style.display = 'none';
    }

  // Only load stats + sidebar when memory is actually available
  // (don't call CLI commands when plugin is misconfigured — they can hang)
  if (_palaceAvailable) {
    await loadPalaceStats();
    await loadPalaceSidebar();
  }
}

// ── Provider fields toggle ─────────────────────────────────────────────────
function updateProviderFields() {
  const sel = $('palace-provider') as HTMLSelectElement | null;
  const isAzure = sel?.value === 'azure';
  const azureFields = $('palace-azure-fields');
  const openaiEndpoint = $('palace-openai-endpoint-field');
  const apiVersionField = $('palace-api-version-field');
  const apiKeyInput = $('palace-api-key') as HTMLInputElement | null;
  const modelLabelEl = $('palace-model-label');
  const modelInput = $('palace-model-name') as HTMLInputElement | null;

  if (azureFields) azureFields.style.display = isAzure ? '' : 'none';
  if (openaiEndpoint) openaiEndpoint.style.display = isAzure ? 'none' : '';
  if (apiVersionField) apiVersionField.style.display = isAzure ? '' : 'none';
  if (apiKeyInput) apiKeyInput.placeholder = isAzure ? 'Azure API key' : 'sk-...';
  if (modelLabelEl) modelLabelEl.innerHTML = isAzure
    ? 'Deployment Name <span class="palace-api-hint">(defaults to text-embedding-3-small)</span>'
    : 'Model <span class="palace-api-hint">(defaults to text-embedding-3-small)</span>';
  if (modelInput) modelInput.placeholder = isAzure
    ? 'text-embedding-3-small' : 'text-embedding-3-small';
}

function getSelectedProvider(): string {
  return (($('palace-provider') as HTMLSelectElement)?.value) || 'openai';
}

function getBaseUrlForProvider(): string {
  const provider = getSelectedProvider();
  if (provider === 'azure') {
    return ($('palace-base-url') as HTMLInputElement)?.value?.trim() ?? '';
  }
  return ($('palace-base-url-openai') as HTMLInputElement)?.value?.trim() ?? '';
}

// ── Install / Setup form ───────────────────────────────────────────────────
function initPalaceInstall() {
  // Provider dropdown — toggle fields on change
  $('palace-provider')?.addEventListener('change', updateProviderFields);
  // Set initial state
  updateProviderFields();

  // Settings gear — show the setup banner for reconfiguration
  $('palace-settings')?.addEventListener('click', async () => {
    const banner = $('palace-install-banner');
    if (!banner) return;
    banner.style.display = 'flex';
    // Pre-fill with existing settings
    if (invoke) {
      try {
        const existingUrl = await invoke<string | null>('get_embedding_base_url');
        const existingVersion = await invoke<string | null>('get_azure_api_version');
        const existingProvider = await invoke<string | null>('get_embedding_provider');
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
      } catch { /* ignore */ }
    }
    // Update button text to indicate reconfiguration
    const btn = $('palace-install-btn') as HTMLButtonElement | null;
    if (btn) { btn.textContent = 'Save & Restart'; btn.disabled = false; }
    const progressDiv = $('palace-install-progress');
    if (progressDiv) progressDiv.style.display = 'none';
  });

  // ── Shared form reader & validator ──
  function readMemoryForm(): { apiKey: string; baseUrl: string; modelName: string; apiVersion: string; provider: string } | null {
    const apiKeyInput = $('palace-api-key') as HTMLInputElement | null;
    const provider = getSelectedProvider();
    let apiKey = apiKeyInput?.value?.trim() ?? '';
    let baseUrl = getBaseUrlForProvider();
    const modelName = ($('palace-model-name') as HTMLInputElement | null)?.value?.trim() ?? '';
    const apiVersion = ($('palace-api-version') as HTMLInputElement | null)?.value?.trim() ?? '';

    // Detect URL pasted into API key field
    if (apiKey.startsWith('http://') || apiKey.startsWith('https://')) {
      if (!baseUrl) {
        baseUrl = apiKey;
        apiKey = '';
        const targetId = provider === 'azure' ? 'palace-base-url' : 'palace-base-url-openai';
        const bi = $(targetId) as HTMLInputElement | null;
        if (bi) bi.value = baseUrl;
        if (apiKeyInput) { apiKeyInput.value = ''; apiKeyInput.style.borderColor = 'var(--error)'; apiKeyInput.focus(); apiKeyInput.placeholder = 'Enter your API key here (not a URL)'; }
        return null;
      } else {
        if (apiKeyInput) { apiKeyInput.value = ''; apiKeyInput.style.borderColor = 'var(--error)'; apiKeyInput.focus(); apiKeyInput.placeholder = 'This looks like a URL — enter your API key instead'; }
        return null;
      }
    }

    if (provider === 'azure' && !baseUrl) {
      const bi = $('palace-base-url') as HTMLInputElement | null;
      if (bi) { bi.style.borderColor = 'var(--error)'; bi.focus(); bi.placeholder = 'Azure endpoint is required'; }
      return null;
    }

    if (!apiKey) {
      if (apiKeyInput) { apiKeyInput.style.borderColor = 'var(--error)'; apiKeyInput.focus(); apiKeyInput.placeholder = 'API key is required'; }
      return null;
    }
    if (apiKeyInput) apiKeyInput.style.borderColor = '';
    return { apiKey, baseUrl, modelName, apiVersion, provider };
  }

  // ── Test Connection button ──
  $('palace-test-btn')?.addEventListener('click', async () => {
    const testBtn = $('palace-test-btn') as HTMLButtonElement | null;
    const progressDiv = $('palace-install-progress');
    const progressText = $('palace-progress-text') as HTMLElement | null;
    if (!testBtn || !invoke) return;

    const form = readMemoryForm();
    if (!form) return;

    testBtn.disabled = true;
    testBtn.textContent = 'Testing…';
    if (progressDiv) progressDiv.style.display = '';
    if (progressText) progressText.textContent = 'Testing embedding endpoint…';

    try {
      await invoke('test_embedding_connection', {
        apiKey: form.apiKey,
        baseUrl: form.baseUrl || null,
        model: form.modelName || null,
        apiVersion: form.apiVersion || null,
        provider: form.provider,
      });
      if (progressText) progressText.textContent = 'Connection test passed ✓';
    } catch (testErr: unknown) {
      const errMsg = typeof testErr === 'string' ? testErr : (testErr as Error)?.message || String(testErr);
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
    if (!btn || !invoke) return;

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
        await invoke('test_embedding_connection', {
          apiKey,
          baseUrl: baseUrl || null,
          model: modelName || null,
          apiVersion: apiVersion || null,
          provider,
        });
        if (progressText) progressText.textContent = 'Connection test passed ✓ Saving configuration…';
      } catch (testErr: unknown) {
        // Connection test failed — show the error and let user fix
        const errMsg = typeof testErr === 'string' ? testErr : (testErr as Error)?.message || String(testErr);
        if (progressText) progressText.textContent = `Connection test failed: ${errMsg}`;
        btn.textContent = 'Retry';
        btn.disabled = false;
        return;
      }

      btn.textContent = 'Saving…';

      // Step 2: Write config to openclaw.json
      await invoke('enable_memory_plugin', {
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
        const configured = await invoke<boolean>('check_memory_configured');
        _palaceAvailable = configured;
      } catch { /* ignore */ }

      if (_palaceAvailable) {
        const banner = $('palace-install-banner');
        if (banner) banner.style.display = 'none';
        _palaceInitialized = false;
        await loadMemoryPalace();
      } else {
        if (progressText) {
          progressText.textContent = 'Configuration saved. Memory plugin may need additional setup.';
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

    document.querySelectorAll('.palace-tab').forEach(t => t.classList.remove('active'));
    document.querySelectorAll('.palace-panel').forEach(p => (p as HTMLElement).style.display = 'none');
    document.querySelector('.palace-tab[data-palace-tab="files"]')?.classList.add('active');
    const fp = $('palace-files-panel');
    if (fp) fp.style.display = 'flex';

    const filesDivider = $('palace-files-divider');
    if (filesDivider) filesDivider.style.display = 'none';
    const memoryListBelow = $('memory-list');
    if (memoryListBelow) memoryListBelow.style.display = 'none';
  });
}

// ── Embedding Status Banner ────────────────────────────────────────────────
async function renderEmbeddingStatus(stats: { total_memories: number; has_embeddings: boolean }) {
  // Remove old banner if any
  const old = $('palace-embedding-banner');
  if (old) old.remove();

  try {
    const status = await pawEngine.embeddingStatus();
    const statsEl = $('palace-stats');
    if (!statsEl) return;

    const banner = document.createElement('div');
    banner.id = 'palace-embedding-banner';
    banner.style.cssText = 'margin:8px 0;padding:10px 14px;border-radius:8px;font-size:12px;line-height:1.5';

    if (!status.ollama_running) {
      // Ollama not running
      banner.style.background = 'var(--warning-bg, rgba(234,179,8,0.1))';
      banner.style.border = '1px solid var(--warning-border, rgba(234,179,8,0.3))';
      banner.innerHTML = `
        <div style="display:flex;align-items:center;gap:8px">
          <span style="font-size:16px"><span class="ms ms-sm">warning</span></span>
          <div>
            <strong>Ollama not running</strong> — semantic memory search is disabled.
            <div style="color:var(--text-muted);margin-top:2px">
              Start Ollama to enable AI-powered memory search.
              Memory will fallback to keyword matching.
            </div>
          </div>
        </div>`;
    } else if (!status.model_available) {
      // Ollama running but model not pulled
      banner.style.background = 'var(--info-bg, rgba(59,130,246,0.1))';
      banner.style.border = '1px solid var(--info-border, rgba(59,130,246,0.3))';
      banner.innerHTML = `
        <div style="display:flex;align-items:center;gap:8px">
          <span style="font-size:16px"><span class="ms ms-sm">inventory_2</span></span>
          <div style="flex:1">
            <strong>Embedding model needed</strong> — <code style="font-size:11px;background:var(--bg-tertiary,rgba(255,255,255,0.06));padding:1px 5px;border-radius:3px">${escHtml(status.model_name)}</code> not found.
            <div style="color:var(--text-muted);margin-top:2px">
              Pull the model to enable semantic memory search (~275 MB download).
            </div>
          </div>
          <button class="btn btn-primary btn-sm" id="palace-pull-model-btn" style="white-space:nowrap">Pull Model</button>
        </div>
        <div id="palace-pull-progress" style="display:none;margin-top:6px;color:var(--text-muted)"></div>`;

      // Insert before wiring events
      statsEl.after(banner);
      $('palace-pull-model-btn')?.addEventListener('click', async () => {
        const btn = $('palace-pull-model-btn') as HTMLButtonElement | null;
        const prog = $('palace-pull-progress');
        if (btn) { btn.disabled = true; btn.textContent = 'Pulling...'; }
        if (prog) { prog.style.display = ''; prog.textContent = 'Downloading model... this may take a minute.'; }
        try {
          const result = await pawEngine.embeddingPullModel();
          if (prog) prog.textContent = `✓ ${result}`;
          if (btn) btn.textContent = '✓ Done';
          showToast('Embedding model ready!', 'success');
          // Refresh stats
          setTimeout(() => loadPalaceStats(), 1000);
        } catch (e) {
          if (prog) prog.textContent = `✗ Failed: ${e}`;
          if (btn) { btn.disabled = false; btn.textContent = 'Retry'; }
          showToast(`Pull failed: ${e}`, 'error');
        }
      });
      return; // Already inserted
    } else if (!stats.has_embeddings && stats.total_memories > 0) {
      // Ollama ready, model available, but existing memories have no vectors
      banner.style.background = 'var(--info-bg, rgba(59,130,246,0.1))';
      banner.style.border = '1px solid var(--info-border, rgba(59,130,246,0.3))';
      banner.innerHTML = `
        <div style="display:flex;align-items:center;gap:8px">
          <span style="font-size:16px"><span class="ms ms-sm">sync</span></span>
          <div style="flex:1">
            <strong>Embeddings ready</strong> — ${stats.total_memories} memories need vectors for semantic search.
          </div>
          <button class="btn btn-primary btn-sm" id="palace-backfill-btn" style="white-space:nowrap">Embed All</button>
        </div>
        <div id="palace-backfill-progress" style="display:none;margin-top:6px;color:var(--text-muted)"></div>`;

      statsEl.after(banner);
      $('palace-backfill-btn')?.addEventListener('click', async () => {
        const btn = $('palace-backfill-btn') as HTMLButtonElement | null;
        const prog = $('palace-backfill-progress');
        if (btn) { btn.disabled = true; btn.textContent = 'Embedding...'; }
        if (prog) { prog.style.display = ''; prog.textContent = 'Generating embeddings for existing memories...'; }
        try {
          const result = await pawEngine.memoryBackfill();
          if (prog) prog.textContent = `✓ ${result.success} embedded${result.failed > 0 ? `, ${result.failed} failed` : ''}`;
          if (btn) btn.textContent = '✓ Done';
          showToast(`Embedded ${result.success} memories`, 'success');
          setTimeout(() => loadPalaceStats(), 1000);
        } catch (e) {
          if (prog) prog.textContent = `✗ Failed: ${e}`;
          if (btn) { btn.disabled = false; btn.textContent = 'Retry'; }
          showToast(`Backfill failed: ${e}`, 'error');
        }
      });
      return;
    } else if (status.ollama_running && status.model_available) {
      // All good — semantic search is active!
      banner.style.background = 'var(--success-bg, rgba(34,197,94,0.08))';
      banner.style.border = '1px solid var(--success-border, rgba(34,197,94,0.2))';
      banner.innerHTML = `
        <div style="display:flex;align-items:center;gap:6px">
          <span style="font-size:14px">✓</span>
          <span>Semantic search active — <code style="font-size:11px;background:var(--bg-tertiary,rgba(255,255,255,0.06));padding:1px 5px;border-radius:3px">${escHtml(status.model_name)}</code> via Ollama</span>
        </div>`;
    } else {
      return; // Nothing to show
    }

    statsEl.after(banner);
  } catch (e) {
    console.warn('[memory] Embedding status check failed:', e);
  }
}

// ── Stats loader ───────────────────────────────────────────────────────────
async function loadPalaceStats() {
  const totalEl = $('palace-total');
  const typesEl = $('palace-types');
  const edgesEl = $('palace-graph-edges');
  if (!totalEl) return;

  try {
    const stats = await pawEngine.memoryStats();
    totalEl.textContent = String(stats.total_memories);
    if (typesEl) {
      const catCount = stats.categories.length;
      typesEl.textContent = catCount > 0 ? String(catCount) : '0';
      typesEl.title = stats.categories.length > 0
        ? stats.categories.map(([c, n]) => `${c}: ${n}`).join(', ')
        : '';
    }
    if (edgesEl) edgesEl.textContent = stats.has_embeddings ? '✓' : '✗';

    // Show embedding status banner
    await renderEmbeddingStatus(stats);
  } catch (e) {
    console.warn('[memory] Engine stats failed:', e);
    totalEl.textContent = '—';
    if (typesEl) typesEl.textContent = '—';
    if (edgesEl) edgesEl.textContent = '—';
  }
}

// ── Sidebar loader ─────────────────────────────────────────────────────────
async function loadPalaceSidebar() {
  const list = $('palace-memory-list');
  if (!list) return;

  list.innerHTML = '';

  try {
    const memories = await pawEngine.memoryList(20);
    if (!memories.length) {
      list.innerHTML = '<div class="palace-list-empty">No memories yet</div>';
      return;
    }
    for (const mem of memories) {
      const card = document.createElement('div');
      card.className = 'palace-memory-card';
      card.innerHTML = `
        <span class="palace-memory-type">${escHtml(mem.category)}</span>
        <div class="palace-memory-subject">${escHtml(mem.content.slice(0, 60))}${mem.content.length > 60 ? '…' : ''}</div>
        <div class="palace-memory-preview">${mem.score != null ? `${(mem.score * 100).toFixed(0)}% match` : `importance: ${mem.importance}`}</div>
      `;
      card.addEventListener('click', () => palaceRecallById(mem.id));
      list.appendChild(card);
    }
  } catch (e) {
    console.warn('[memory] Sidebar load failed:', e);
    list.innerHTML = '<div class="palace-list-empty">Could not load memories</div>';
  }
}

// ── Recall by ID ───────────────────────────────────────────────────────────
async function palaceRecallById(memoryId: string) {
  const resultsEl = $('palace-recall-results');
  const emptyEl = $('palace-recall-empty');
  if (!resultsEl) return;

  // Switch to recall tab
  document.querySelectorAll('.palace-tab').forEach(t => t.classList.remove('active'));
  document.querySelectorAll('.palace-panel').forEach(p => (p as HTMLElement).style.display = 'none');
  document.querySelector('.palace-tab[data-palace-tab="recall"]')?.classList.add('active');
  const recallPanel = $('palace-recall-panel');
  if (recallPanel) recallPanel.style.display = 'flex';

  resultsEl.innerHTML = '<div style="padding:1rem;color:var(--text-secondary)">Loading…</div>';
  if (emptyEl) emptyEl.style.display = 'none';

  try {
    const memories = await pawEngine.memorySearch(memoryId, 1);
    resultsEl.innerHTML = '';
    if (memories.length) {
      resultsEl.appendChild(renderRecallCard({ id: memories[0].id, text: memories[0].content, category: memories[0].category, importance: memories[0].importance, score: memories[0].score }));
    } else {
      resultsEl.innerHTML = '<div style="padding:1rem;color:var(--text-secondary)">Memory not found</div>';
    }
  } catch (e) {
    resultsEl.innerHTML = `<div style="padding:1rem;color:var(--danger)">Error: ${escHtml(String(e))}</div>`;
  }
}

// ── Recall card renderer ───────────────────────────────────────────────────
function renderRecallCard(mem: { id?: string; text?: string; category?: string; importance?: number; score?: number }): HTMLElement {
  const card = document.createElement('div');
  card.className = 'palace-result-card';

  const score = mem.score != null ? `<span class="palace-result-score">${(mem.score * 100).toFixed(0)}%</span>` : '';
  const importance = mem.importance != null ? `<span class="palace-result-tag">importance: ${mem.importance}</span>` : '';

  card.innerHTML = `
    <div class="palace-result-header">
      <span class="palace-result-type">${escHtml(mem.category ?? 'other')}</span>
      ${score}
    </div>
    <div class="palace-result-content">${escHtml(mem.text ?? '')}</div>
    <div class="palace-result-meta">
      ${importance}
    </div>
  `;
  return card;
}

// ── Tab switching ──────────────────────────────────────────────────────────
function initPalaceTabs() {
  document.querySelectorAll('.palace-tab').forEach(tab => {
    tab.addEventListener('click', () => {
      const target = (tab as HTMLElement).dataset.palaceTab;
      if (!target) return;

      document.querySelectorAll('.palace-tab').forEach(t => t.classList.remove('active'));
      tab.classList.add('active');

      document.querySelectorAll('.palace-panel').forEach(p => (p as HTMLElement).style.display = 'none');
      const panel = $(`palace-${target}-panel`);
      if (panel) panel.style.display = 'flex';
    });
  });
}

// ── Recall search ──────────────────────────────────────────────────────────
function initPalaceRecall() {
  const btn = $('palace-recall-btn');
  const input = $('palace-recall-input') as HTMLTextAreaElement | null;
  if (!btn || !input) return;

  btn.addEventListener('click', () => palaceRecallSearch());
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      palaceRecallSearch();
    }
  });
}

async function palaceRecallSearch() {
  const input = $('palace-recall-input') as HTMLTextAreaElement | null;
  const resultsEl = $('palace-recall-results');
  const emptyEl = $('palace-recall-empty');
  if (!input || !resultsEl) return;

  const query = input.value.trim();
  if (!query) return;

  resultsEl.innerHTML = '<div style="padding:1rem;color:var(--text-secondary)">Searching…</div>';
  if (emptyEl) emptyEl.style.display = 'none';

  try {
    const memories = await pawEngine.memorySearch(query, 10);
    resultsEl.innerHTML = '';
    if (!memories.length) {
      if (emptyEl) emptyEl.style.display = 'flex';
      return;
    }
    for (const mem of memories) {
      resultsEl.appendChild(renderRecallCard({ id: mem.id, text: mem.content, category: mem.category, importance: mem.importance, score: mem.score }));
    }
  } catch (e) {
    resultsEl.innerHTML = `<div style="padding:1rem;color:var(--danger)">Recall failed: ${escHtml(String(e))}</div>`;
  }
}

// ── Remember form ──────────────────────────────────────────────────────────
function initPalaceRemember() {
  const btn = $('palace-remember-save');
  if (!btn) return;

  btn.addEventListener('click', async () => {
    const category = ($('palace-remember-type') as HTMLSelectElement | null)?.value ?? 'other';
    const content = ($('palace-remember-content') as HTMLTextAreaElement | null)?.value.trim() ?? '';
    const importanceStr = ($('palace-remember-importance') as HTMLSelectElement | null)?.value ?? '5';
    const importance = parseInt(importanceStr, 10) || 5;

    if (!content) {
      alert('Content is required.');
      return;
    }

    btn.textContent = 'Saving…';
    (btn as HTMLButtonElement).disabled = true;

    try {
      await pawEngine.memoryStore(content, category, importance);

      // Clear form
      if ($('palace-remember-content') as HTMLTextAreaElement) ($('palace-remember-content') as HTMLTextAreaElement).value = '';

      showToast('Memory saved!', 'success');
      await loadPalaceSidebar();
      await loadPalaceStats();
    } catch (e) {
      showToast(`Save failed: ${e instanceof Error ? e.message : e}`, 'error');
    } finally {
      btn.textContent = 'Save Memory';
      (btn as HTMLButtonElement).disabled = false;
    }
  });
}

// ── Knowledge graph visualization ──────────────────────────────────────────
function initPalaceGraph() {
  const renderBtn = $('palace-graph-render');
  if (!renderBtn) return;

  renderBtn.addEventListener('click', () => renderPalaceGraph());
}

async function renderPalaceGraph() {
  const canvas = $('palace-graph-canvas') as HTMLCanvasElement | null;
  const emptyEl = $('palace-graph-empty');
  if (!canvas) return;

  if (emptyEl) { emptyEl.style.display = 'flex'; emptyEl.textContent = 'Loading memory map…'; }

  let memories: { id?: string; text?: string; category?: string; importance?: number; score?: number }[] = [];

  try {
    const engineMems = await pawEngine.memoryList(50);
    memories = engineMems.map(m => ({ id: m.id, text: m.content, category: m.category, importance: m.importance, score: m.score }));
  } catch (e) {
    console.warn('Graph load failed:', e);
    if (emptyEl) { emptyEl.style.display = 'flex'; emptyEl.textContent = 'Failed to load memory map.'; }
    return;
  }

  if (!memories.length) {
    if (emptyEl) { emptyEl.style.display = 'flex'; emptyEl.textContent = 'No memories to visualize.'; }
    return;
  }

  if (emptyEl) emptyEl.style.display = 'none';

  // Render bubble chart grouped by category
  const ctx = canvas.getContext('2d');
  if (!ctx) return;

  const rect = canvas.parentElement?.getBoundingClientRect();
  canvas.width = rect?.width ?? 600;
  canvas.height = rect?.height ?? 400;

  const categoryColors: Record<string, string> = {
    other: '#676879', preference: '#0073EA', fact: '#00CA72',
    decision: '#FDAB3D', procedure: '#E44258', concept: '#A25DDC',
    code: '#579BFC', person: '#FF642E', project: '#CAB641',
  };

  // Group by category, place category clusters
  const groups = new Map<string, typeof memories>();
  for (const mem of memories) {
    const cat = mem.category ?? 'other';
    if (!groups.has(cat)) groups.set(cat, []);
    groups.get(cat)!.push(mem);
  }

  // Layout: distribute category centers in a circle
  const categories = Array.from(groups.entries());
  const cx = canvas.width / 2, cy = canvas.height / 2;
  const radius = Math.min(cx, cy) * 0.55;

  ctx.clearRect(0, 0, canvas.width, canvas.height);

  categories.forEach(([cat, mems], i) => {
    const angle = (i / categories.length) * Math.PI * 2 - Math.PI / 2;
    const groupX = cx + Math.cos(angle) * radius;
    const groupY = cy + Math.sin(angle) * radius;

    // Draw category label
    ctx.fillStyle = '#676879';
    ctx.font = 'bold 12px Figtree, sans-serif';
    ctx.textAlign = 'center';
    ctx.fillText(cat.toUpperCase(), groupX, groupY - 30 - mems.length * 2);

    // Draw bubbles for each memory
    mems.forEach((mem, j) => {
      const innerAngle = (j / mems.length) * Math.PI * 2;
      const spread = Math.min(25 + mems.length * 4, 60);
      const mx = groupX + Math.cos(innerAngle) * spread * (0.3 + Math.random() * 0.7);
      const my = groupY + Math.sin(innerAngle) * spread * (0.3 + Math.random() * 0.7);
      const size = 4 + (mem.importance ?? 5) * 0.8;
      const color = categoryColors[cat] ?? '#676879';

      ctx.beginPath();
      ctx.arc(mx, my, size, 0, Math.PI * 2);
      ctx.fillStyle = color;
      ctx.globalAlpha = 0.7;
      ctx.fill();
      ctx.globalAlpha = 1;
      ctx.strokeStyle = '#fff';
      ctx.lineWidth = 1.5;
      ctx.stroke();
    });

    // Count label
    ctx.fillStyle = categoryColors[cat] ?? '#676879';
    ctx.font = '11px Figtree, sans-serif';
    ctx.fillText(`${mems.length}`, groupX, groupY + 35 + mems.length * 2);
  });
}

// ── Memory Export ───────────────────────────────────────────────────────────

/** Export all memories as a JSON file download */
async function exportMemories() {
  const btn = $('palace-export') as HTMLButtonElement | null;
  if (btn) btn.disabled = true;

  try {
    const engineMems = await pawEngine.memoryList(500);
    const memories = engineMems.map(m => ({ id: m.id, content: m.content, category: m.category, importance: m.importance, created_at: m.created_at }));

    if (!memories.length) {
      showToast('No memories to export', 'info');
      return;
    }

    // Build export payload with metadata
    const exportData = {
      exportedAt: new Date().toISOString(),
      source: 'Paw Desktop — Memory Export',
      totalMemories: memories.length,
      memories,
    };

    const blob = new Blob([JSON.stringify(exportData, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `paw-memories-${new Date().toISOString().slice(0, 10)}.json`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);

    showToast(`Exported ${memories.length} memories`, 'success');
  } catch (e) {
    showToast(`Export failed: ${e instanceof Error ? e.message : e}`, 'error');
  } finally {
    if (btn) btn.disabled = false;
  }
}

// ── UI event wiring ────────────────────────────────────────────────────────
export function initPalaceEvents() {
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

  // Sidebar search filter (local filter of visible cards)
  $('palace-search')?.addEventListener('input', () => {
    const query = (($('palace-search') as HTMLInputElement)?.value ?? '').toLowerCase();
    document.querySelectorAll('.palace-memory-card').forEach(card => {
      const text = card.textContent?.toLowerCase() ?? '';
      (card as HTMLElement).style.display = text.includes(query) ? '' : 'none';
    });
  });
}
