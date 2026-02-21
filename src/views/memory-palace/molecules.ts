// Memory Palace — Molecules (DOM rendering, IPC interaction)

import { pawEngine } from '../../engine';
import { $, escHtml } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { type RecallCardData, type MemoryFormInputs, validateMemoryForm } from './atoms';

// ── Provider fields toggle ─────────────────────────────────────────────────

export function updateProviderFields(): void {
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
  if (modelLabelEl)
    modelLabelEl.innerHTML = isAzure
      ? 'Deployment Name <span class="palace-api-hint">(defaults to text-embedding-3-small)</span>'
      : 'Model <span class="palace-api-hint">(defaults to text-embedding-3-small)</span>';
  if (modelInput)
    modelInput.placeholder = isAzure ? 'text-embedding-3-small' : 'text-embedding-3-small';
}

function getSelectedProvider(): string {
  return ($('palace-provider') as HTMLSelectElement)?.value || 'openai';
}

export function getBaseUrlForProvider(): string {
  const provider = getSelectedProvider();
  if (provider === 'azure') {
    return ($('palace-base-url') as HTMLInputElement)?.value?.trim() ?? '';
  }
  return ($('palace-base-url-openai') as HTMLInputElement)?.value?.trim() ?? '';
}

// ── Form reader ────────────────────────────────────────────────────────────

/** Read form DOM values and validate via pure function. Returns data or null (with DOM feedback). */
export function readMemoryForm(): {
  apiKey: string;
  baseUrl: string;
  modelName: string;
  apiVersion: string;
  provider: string;
} | null {
  const apiKeyInput = $('palace-api-key') as HTMLInputElement | null;
  const provider = getSelectedProvider();
  const inputs: MemoryFormInputs = {
    apiKey: apiKeyInput?.value?.trim() ?? '',
    azureBaseUrl: ($('palace-base-url') as HTMLInputElement)?.value?.trim() ?? '',
    openaiBaseUrl: ($('palace-base-url-openai') as HTMLInputElement)?.value?.trim() ?? '',
    modelName: ($('palace-model-name') as HTMLInputElement)?.value?.trim() ?? '',
    apiVersion: ($('palace-api-version') as HTMLInputElement)?.value?.trim() ?? '',
    provider,
  };

  const result = validateMemoryForm(inputs);

  if (result.ok) {
    if (apiKeyInput) apiKeyInput.style.borderColor = '';
    return result.data;
  }

  // DOM feedback for validation errors
  const err = result.error;
  if (err.kind === 'url_in_key') {
    const targetId = provider === 'azure' ? 'palace-base-url' : 'palace-base-url-openai';
    const bi = $(targetId) as HTMLInputElement | null;
    if (bi) bi.value = err.swapUrl;
    if (apiKeyInput) {
      apiKeyInput.value = '';
      apiKeyInput.style.borderColor = 'var(--error)';
      apiKeyInput.focus();
      apiKeyInput.placeholder = 'Enter your API key here (not a URL)';
    }
  } else if (err.kind === 'url_in_key_dup') {
    if (apiKeyInput) {
      apiKeyInput.value = '';
      apiKeyInput.style.borderColor = 'var(--error)';
      apiKeyInput.focus();
      apiKeyInput.placeholder = 'This looks like a URL — enter your API key instead';
    }
  } else if (err.kind === 'azure_no_url') {
    const bi = $('palace-base-url') as HTMLInputElement | null;
    if (bi) {
      bi.style.borderColor = 'var(--error)';
      bi.focus();
      bi.placeholder = 'Azure endpoint is required';
    }
  } else if (err.kind === 'no_key') {
    if (apiKeyInput) {
      apiKeyInput.style.borderColor = 'var(--error)';
      apiKeyInput.focus();
      apiKeyInput.placeholder = 'API key is required';
    }
  }
  return null;
}

// ── Embedding Status Banner ────────────────────────────────────────────────

export async function renderEmbeddingStatus(stats: {
  total_memories: number;
  has_embeddings: boolean;
}): Promise<void> {
  // Remove old banner if any
  const old = $('palace-embedding-banner');
  if (old) old.remove();

  try {
    const status = await pawEngine.embeddingStatus();
    const statsEl = $('palace-stats');
    if (!statsEl) return;

    const banner = document.createElement('div');
    banner.id = 'palace-embedding-banner';
    banner.style.cssText =
      'margin:8px 0;padding:10px 14px;border-radius:8px;font-size:12px;line-height:1.5';

    if (!status.ollama_running) {
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

      statsEl.after(banner);
      $('palace-pull-model-btn')?.addEventListener('click', async () => {
        const btn = $('palace-pull-model-btn') as HTMLButtonElement | null;
        const prog = $('palace-pull-progress');
        if (btn) {
          btn.disabled = true;
          btn.textContent = 'Pulling...';
        }
        if (prog) {
          prog.style.display = '';
          prog.textContent = 'Downloading model... this may take a minute.';
        }
        try {
          const result = await pawEngine.embeddingPullModel();
          if (prog) prog.textContent = `✓ ${result}`;
          if (btn) btn.textContent = '✓ Done';
          showToast('Embedding model ready!', 'success');
          loadPalaceStats();
        } catch (e) {
          if (prog) prog.textContent = `✗ Failed: ${e}`;
          if (btn) {
            btn.disabled = false;
            btn.textContent = 'Retry';
          }
          showToast(`Pull failed: ${e}`, 'error');
        }
      });
      return;
    } else if (!stats.has_embeddings && stats.total_memories > 0) {
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
        if (btn) {
          btn.disabled = true;
          btn.textContent = 'Embedding...';
        }
        if (prog) {
          prog.style.display = '';
          prog.textContent = 'Generating embeddings for existing memories...';
        }
        try {
          const result = await pawEngine.memoryBackfill();
          if (prog)
            prog.textContent = `✓ ${result.success} embedded${result.failed > 0 ? `, ${result.failed} failed` : ''}`;
          if (btn) btn.textContent = '✓ Done';
          showToast(`Embedded ${result.success} memories`, 'success');
          loadPalaceStats();
        } catch (e) {
          if (prog) prog.textContent = `✗ Failed: ${e}`;
          if (btn) {
            btn.disabled = false;
            btn.textContent = 'Retry';
          }
          showToast(`Backfill failed: ${e}`, 'error');
        }
      });
      return;
    } else if (status.ollama_running && status.model_available) {
      banner.style.background = 'var(--success-bg, rgba(34,197,94,0.08))';
      banner.style.border = '1px solid var(--success-border, rgba(34,197,94,0.2))';
      banner.innerHTML = `
        <div style="display:flex;align-items:center;gap:6px">
          <span style="font-size:14px">✓</span>
          <span>Semantic search active — <code style="font-size:11px;background:var(--bg-tertiary,rgba(255,255,255,0.06));padding:1px 5px;border-radius:3px">${escHtml(status.model_name)}</code> via Ollama</span>
        </div>`;
    } else {
      return;
    }

    statsEl.after(banner);
  } catch (e) {
    console.warn('[memory] Embedding status check failed:', e);
  }
}

// ── Stats loader ───────────────────────────────────────────────────────────

export async function loadPalaceStats(): Promise<void> {
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
      typesEl.title =
        stats.categories.length > 0
          ? stats.categories.map(([c, n]) => `${c}: ${n}`).join(', ')
          : '';
    }
    if (edgesEl) edgesEl.textContent = stats.has_embeddings ? '✓' : '✗';

    await renderEmbeddingStatus(stats);
  } catch (e) {
    console.warn('[memory] Engine stats failed:', e);
    totalEl.textContent = '—';
    if (typesEl) typesEl.textContent = '—';
    if (edgesEl) edgesEl.textContent = '—';
  }
}

// ── Sidebar loader ─────────────────────────────────────────────────────────

export async function loadPalaceSidebar(onRecall?: (id: string) => void): Promise<void> {
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
      card.addEventListener('click', () => {
        if (onRecall) onRecall(mem.id);
        else palaceRecallById(mem.id);
      });
      list.appendChild(card);
    }
  } catch (e) {
    console.warn('[memory] Sidebar load failed:', e);
    list.innerHTML = '<div class="palace-list-empty">Could not load memories</div>';
  }
}

// ── Recall by ID ───────────────────────────────────────────────────────────

export async function palaceRecallById(memoryId: string): Promise<void> {
  const resultsEl = $('palace-recall-results');
  const emptyEl = $('palace-recall-empty');
  if (!resultsEl) return;

  // Switch to recall tab
  document.querySelectorAll('.palace-tab').forEach((t) => t.classList.remove('active'));
  document
    .querySelectorAll('.palace-panel')
    .forEach((p) => ((p as HTMLElement).style.display = 'none'));
  document.querySelector('.palace-tab[data-palace-tab="recall"]')?.classList.add('active');
  const recallPanel = $('palace-recall-panel');
  if (recallPanel) recallPanel.style.display = 'flex';

  resultsEl.innerHTML = '<div style="padding:1rem;color:var(--text-secondary)">Loading…</div>';
  if (emptyEl) emptyEl.style.display = 'none';

  try {
    const memories = await pawEngine.memorySearch(memoryId, 1);
    resultsEl.innerHTML = '';
    if (memories.length) {
      resultsEl.appendChild(
        renderRecallCard({
          id: memories[0].id,
          text: memories[0].content,
          category: memories[0].category,
          importance: memories[0].importance,
          score: memories[0].score,
        }),
      );
    } else {
      resultsEl.innerHTML =
        '<div style="padding:1rem;color:var(--text-secondary)">Memory not found</div>';
    }
  } catch (e) {
    resultsEl.innerHTML = `<div style="padding:1rem;color:var(--danger)">Error: ${escHtml(String(e))}</div>`;
  }
}

// ── Recall card renderer ───────────────────────────────────────────────────

export function renderRecallCard(mem: RecallCardData): HTMLElement {
  const card = document.createElement('div');
  card.className = 'palace-result-card';

  const score =
    mem.score != null
      ? `<span class="palace-result-score">${(mem.score * 100).toFixed(0)}%</span>`
      : '';
  const importance =
    mem.importance != null
      ? `<span class="palace-result-tag">importance: ${mem.importance}</span>`
      : '';

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

let _tabsBound = false;
export function initPalaceTabs(): void {
  if (_tabsBound) return;
  _tabsBound = true;
  document.querySelectorAll('.palace-tab').forEach((tab) => {
    tab.addEventListener('click', () => {
      const target = (tab as HTMLElement).dataset.palaceTab;
      if (!target) return;

      document.querySelectorAll('.palace-tab').forEach((t) => t.classList.remove('active'));
      tab.classList.add('active');

      document
        .querySelectorAll('.palace-panel')
        .forEach((p) => ((p as HTMLElement).style.display = 'none'));
      const panel = $(`palace-${target}-panel`);
      if (panel) panel.style.display = 'flex';
    });
  });
}

// ── Recall search ──────────────────────────────────────────────────────────

let _recallBound = false;
export function initPalaceRecall(): void {
  if (_recallBound) return;
  _recallBound = true;
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

async function palaceRecallSearch(): Promise<void> {
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
      resultsEl.appendChild(
        renderRecallCard({
          id: mem.id,
          text: mem.content,
          category: mem.category,
          importance: mem.importance,
          score: mem.score,
        }),
      );
    }
  } catch (e) {
    resultsEl.innerHTML = `<div style="padding:1rem;color:var(--danger)">Recall failed: ${escHtml(String(e))}</div>`;
  }
}

// ── Remember form ──────────────────────────────────────────────────────────

let _rememberBound = false;
export function initPalaceRemember(onSaved?: () => Promise<void>): void {
  if (_rememberBound) return;
  _rememberBound = true;
  const btn = $('palace-remember-save');
  if (!btn) return;

  btn.addEventListener('click', async () => {
    const category = ($('palace-remember-type') as HTMLSelectElement | null)?.value ?? 'other';
    const content =
      ($('palace-remember-content') as HTMLTextAreaElement | null)?.value.trim() ?? '';
    const importanceStr =
      ($('palace-remember-importance') as HTMLSelectElement | null)?.value ?? '5';
    const importance = parseInt(importanceStr, 10) || 5;

    if (!content) {
      showToast('Content is required.', 'error');
      return;
    }

    btn.textContent = 'Saving…';
    (btn as HTMLButtonElement).disabled = true;

    try {
      await pawEngine.memoryStore(content, category, importance);

      if ($('palace-remember-content') as HTMLTextAreaElement)
        ($('palace-remember-content') as HTMLTextAreaElement).value = '';

      showToast('Memory saved!', 'success');
      if (onSaved) await onSaved();
    } catch (e) {
      showToast(`Save failed: ${e instanceof Error ? e.message : e}`, 'error');
    } finally {
      btn.textContent = 'Save Memory';
      (btn as HTMLButtonElement).disabled = false;
    }
  });
}

// ── Memory Export ───────────────────────────────────────────────────────────

export async function exportMemories(): Promise<void> {
  const btn = $('palace-export') as HTMLButtonElement | null;
  if (btn) btn.disabled = true;

  try {
    const engineMems = await pawEngine.memoryList(500);
    const memories = engineMems.map((m) => ({
      id: m.id,
      content: m.content,
      category: m.category,
      importance: m.importance,
      created_at: m.created_at,
    }));

    if (!memories.length) {
      showToast('No memories to export', 'info');
      return;
    }

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
