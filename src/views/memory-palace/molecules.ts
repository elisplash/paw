// Memory Palace — Molecules (DOM rendering, IPC interaction)

import { pawEngine } from '../../engine';
import { $, escHtml, confirmModal } from '../../components/helpers';
import { showToast } from '../../components/toast';
import {
  type RecallCardData,
  type MemoryFormInputs,
  validateMemoryForm,
  agentLabel,
} from './atoms';
import { renderPalaceGraph } from './graph';

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
    const memConfig = await pawEngine.getMemoryConfig();
    const embProvider = memConfig.embedding_provider ?? 'auto';
    const isCloudProvider =
      embProvider === 'openai' || embProvider === 'google' || embProvider === 'provider';

    const status = await pawEngine.embeddingStatus();
    const statsEl = $('palace-stats');
    if (!statsEl) return;

    const banner = document.createElement('div');
    banner.id = 'palace-embedding-banner';
    banner.style.cssText =
      'margin:8px 0;padding:10px 14px;border-radius:8px;font-size:12px;line-height:1.5';

    if (!isCloudProvider && !status.ollama_running) {
      banner.style.background = 'var(--warning-bg, rgba(234,179,8,0.1))';
      banner.style.border = '1px solid var(--warning-border, rgba(234,179,8,0.3))';
      banner.innerHTML = `
        <div style="display:flex;align-items:center;gap:8px">
          <span style="font-size:16px"><span class="ms ms-sm">warning</span></span>
          <div>
            <strong>Ollama not running</strong> — semantic memory search is disabled.
            <div style="color:var(--text-muted);margin-top:2px">
              Start Ollama or switch to a cloud embedding provider in Settings → Agent Defaults.
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
    } else if (isCloudProvider || (status.ollama_running && status.model_available)) {
      const provLabel = isCloudProvider
        ? embProvider === 'provider'
          ? 'your chat provider'
          : embProvider
        : 'Ollama';
      banner.style.background = 'var(--success-bg, rgba(34,197,94,0.08))';
      banner.style.border = '1px solid var(--success-border, rgba(34,197,94,0.2))';
      banner.innerHTML = `
        <div style="display:flex;align-items:center;gap:6px">
          <span style="font-size:14px">✓</span>
          <span>Semantic search active — <code style="font-size:11px;background:var(--bg-tertiary,rgba(255,255,255,0.06));padding:1px 5px;border-radius:3px">${escHtml(status.model_name || embProvider)}</code> via ${escHtml(provLabel)}</span>
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

  // Read agent filter
  const agentFilter = ($('palace-agent-filter') as HTMLSelectElement)?.value ?? '';

  try {
    const memories = await pawEngine.memoryList(50);
    // Apply client-side agent filter
    const filtered = agentFilter
      ? memories.filter((m) => (m.agent_id || '') === agentFilter)
      : memories;

    // Populate agent filter dropdown with known agents
    const agentFilterEl = $('palace-agent-filter') as HTMLSelectElement | null;
    if (agentFilterEl && agentFilterEl.options.length <= 2) {
      const agentIds = [
        ...new Set(memories.map((m) => m.agent_id || '').filter((id) => id.length > 0)),
      ];
      for (const id of agentIds) {
        const opt = document.createElement('option');
        opt.value = id;
        opt.textContent = id;
        agentFilterEl.appendChild(opt);
      }
      // Restore selection
      if (agentFilter) agentFilterEl.value = agentFilter;
    }

    if (!filtered.length) {
      list.innerHTML = '<div class="palace-list-empty">No memories yet</div>';
      return;
    }
    for (const mem of filtered) {
      const card = document.createElement('div');
      card.className = 'palace-memory-card';
      const agentTag = mem.agent_id
        ? `<span class="palace-memory-agent">${escHtml(mem.agent_id)}</span>`
        : '<span class="palace-memory-agent system">system</span>';
      card.innerHTML = `
        <div class="palace-memory-card-top">
          <span class="palace-memory-type">${escHtml(mem.category)}</span>
          ${agentTag}
          <button class="btn-icon palace-sidebar-delete" data-memory-id="${escHtml(mem.id)}" title="Delete"><span class="ms ms-sm">close</span></button>
        </div>
        <div class="palace-memory-subject">${escHtml(mem.content.slice(0, 60))}${mem.content.length > 60 ? '…' : ''}</div>
        <div class="palace-memory-preview">${mem.score != null ? `${(mem.score * 100).toFixed(0)}% match` : `importance: ${mem.importance}`}</div>
      `;
      // Click card = recall
      card.addEventListener('click', (e) => {
        if ((e.target as HTMLElement).closest('.palace-sidebar-delete')) return;
        if (onRecall) onRecall(mem.id);
        else palaceRecallById(mem.id);
      });
      // Delete button on sidebar card
      const delBtn = card.querySelector('.palace-sidebar-delete');
      if (delBtn) {
        delBtn.addEventListener('click', async (e) => {
          e.stopPropagation();
          if (!(await confirmModal('Delete this memory?', 'Delete Memory'))) return;
          try {
            await pawEngine.memoryDelete(mem.id);
            showToast('Memory deleted', 'success');
            card.remove();
            await loadPalaceStats();
          } catch (err) {
            showToast(`Failed to delete: ${err}`, 'error');
          }
        });
      }
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
          agent_id: memories[0].agent_id,
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
  const agent = `<span class="palace-result-tag palace-result-agent">${escHtml(agentLabel(mem.agent_id))}</span>`;
  const deleteBtn = mem.id
    ? `<button class="btn-icon palace-delete-memory" data-memory-id="${escHtml(mem.id)}" title="Delete memory"><span class="ms ms-sm">delete</span></button>`
    : '';

  card.innerHTML = `
    <div class="palace-result-header">
      <span class="palace-result-type">${escHtml(mem.category ?? 'other')}</span>
      ${score}
      ${deleteBtn}
    </div>
    <div class="palace-result-content">${escHtml(mem.text ?? '')}</div>
    <div class="palace-result-meta">
      ${importance}
      ${agent}
    </div>
  `;

  // Wire delete button
  const delEl = card.querySelector('.palace-delete-memory');
  if (delEl && mem.id) {
    delEl.addEventListener('click', async (e) => {
      e.stopPropagation();
      const memId = (delEl as HTMLElement).dataset.memoryId;
      if (!memId) return;
      if (!(await confirmModal('Delete this memory?', 'Delete Memory'))) return;
      try {
        await pawEngine.memoryDelete(memId);
        showToast('Memory deleted', 'success');
        card.remove();
        await loadPalaceStats();
        await loadPalaceSidebar();
      } catch (err) {
        showToast(`Failed to delete: ${err}`, 'error');
      }
    });
  }

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
      activatePalaceTab(target);
    });
  });
}

/** Programmatically switch to a palace tab (recall, graph, remember, files, forge). */
export function activatePalaceTab(target: string): void {
  document.querySelectorAll('.palace-tab').forEach((t) => t.classList.remove('active'));
  document.querySelector(`.palace-tab[data-palace-tab="${target}"]`)?.classList.add('active');

  document
    .querySelectorAll('.palace-panel')
    .forEach((p) => ((p as HTMLElement).style.display = 'none'));
  const panel = $(`palace-${target}-panel`);
  if (panel) panel.style.display = 'flex';

  // Auto-render graph when Map tab is activated
  if (target === 'graph') {
    renderPalaceGraph();
  }

  // Auto-render forge data when Forge tab is activated
  if (target === 'forge') {
    renderPalaceForge();
  }
}

/** Reset tabs to the default Recall tab. Called on view load. */
export function resetPalaceTabs(): void {
  activatePalaceTab('recall');
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
          agent_id: mem.agent_id,
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

// ── FORGE Certification View ───────────────────────────────────────────────

export async function renderPalaceForge(): Promise<void> {
  const container = $('palace-forge-content');
  const emptyEl = $('palace-forge-empty');
  if (!container) return;

  if (emptyEl) {
    emptyEl.style.display = 'flex';
    emptyEl.querySelector('.empty-subtitle')!.textContent = 'Loading certification data…';
  }

  // Read agent filter if available
  const agentFilter = ($('palace-agent-filter') as HTMLSelectElement)?.value ?? '';
  const agentId = agentFilter || 'default';

  try {
    const [summary, domains] = await Promise.all([
      pawEngine.forgeCertSummary(agentId),
      pawEngine.forgeListDomains(agentId),
    ]);

    const total =
      summary.uncertified +
      summary.in_training +
      summary.certified +
      summary.expired +
      summary.failed;

    if (total === 0 && domains.length === 0) {
      if (emptyEl) {
        emptyEl.style.display = 'flex';
        emptyEl.querySelector('.empty-subtitle')!.textContent =
          'No FORGE training data yet. Certified procedural memories will appear here.';
      }
      return;
    }

    if (emptyEl) emptyEl.style.display = 'none';

    // Build stats row
    const statsHtml = `
      <div class="palace-forge-stats" style="display:flex;gap:20px;flex-wrap:wrap;padding:16px 0;border-bottom:1px solid var(--border-subtle)">
        <div style="text-align:center">
          <div style="font-size:24px;font-weight:700;color:#22c55e">${summary.certified}</div>
          <div style="font-size:11px;color:var(--text-muted)">Certified</div>
        </div>
        <div style="text-align:center">
          <div style="font-size:24px;font-weight:700;color:#eab308">${summary.in_training}</div>
          <div style="font-size:11px;color:var(--text-muted)">Training</div>
        </div>
        <div style="text-align:center">
          <div style="font-size:24px;font-weight:700;color:var(--text-muted)">${summary.uncertified}</div>
          <div style="font-size:11px;color:var(--text-muted)">Uncertified</div>
        </div>
        <div style="text-align:center">
          <div style="font-size:24px;font-weight:700;color:#f97316">${summary.expired}</div>
          <div style="font-size:11px;color:var(--text-muted)">Expired</div>
        </div>
        <div style="text-align:center">
          <div style="font-size:24px;font-weight:700;color:#ef4444">${summary.failed}</div>
          <div style="font-size:11px;color:var(--text-muted)">Failed</div>
        </div>
      </div>`;

    // Build domains list
    let domainsHtml = '';
    if (domains.length > 0) {
      const domainCards = await Promise.all(
        domains.map(async (d) => {
          const pct =
            d.total_skills > 0 ? Math.round((d.certified_skills / d.total_skills) * 100) : 0;
          let skillsHtml = '';
          try {
            const tree = await pawEngine.forgeDomainTree(agentId, d.domain);
            skillsHtml = tree
              .map(
                (node) =>
                  `<div style="display:flex;align-items:center;gap:8px;padding:4px 0;font-size:12px">
                    <span class="ms ms-sm" style="font-size:14px;color:${statusColor(node.certification_status)}">${statusIcon(node.certification_status)}</span>
                    <span style="flex:1">${escHtml(node.trigger)}</span>
                    <span style="color:var(--text-muted);font-size:11px">${escHtml(node.skill_tree_path)}</span>
                    <span style="color:var(--text-muted);font-size:11px">${Math.round(node.success_rate * 100)}%</span>
                  </div>`,
              )
              .join('');
          } catch {
            // skill tree fetch failed — show domain only
          }

          return `
            <div style="border:1px solid var(--border-subtle);border-radius:8px;padding:12px;margin-bottom:8px">
              <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:8px">
                <div style="display:flex;align-items:center;gap:6px">
                  <span class="ms ms-sm">domain</span>
                  <strong style="font-size:14px">${escHtml(d.domain)}</strong>
                </div>
                <span style="font-size:12px;color:var(--text-muted)">${d.certified_skills}/${d.total_skills} certified (${pct}%)</span>
              </div>
              <div style="height:4px;background:var(--bg-tertiary,rgba(255,255,255,0.06));border-radius:2px;overflow:hidden;margin-bottom:8px">
                <div style="height:100%;width:${pct}%;background:#22c55e;border-radius:2px;transition:width 0.3s"></div>
              </div>
              ${skillsHtml}
            </div>`;
        }),
      );
      domainsHtml = `<div style="padding-top:12px">${domainCards.join('')}</div>`;
    }

    container.innerHTML = statsHtml + domainsHtml;
  } catch (e) {
    console.warn('[memory] Forge data load failed:', e);
    if (emptyEl) {
      emptyEl.style.display = 'flex';
      emptyEl.querySelector('.empty-subtitle')!.textContent = 'Failed to load FORGE data.';
    }
  }
}

function statusColor(status: string): string {
  switch (status) {
    case 'certified':
      return '#22c55e';
    case 'in_training':
      return '#eab308';
    case 'expired':
      return '#f97316';
    case 'failed':
      return '#ef4444';
    default:
      return 'var(--text-muted)';
  }
}

function statusIcon(status: string): string {
  switch (status) {
    case 'certified':
      return 'verified';
    case 'in_training':
      return 'model_training';
    case 'expired':
      return 'schedule';
    case 'failed':
      return 'cancel';
    default:
      return 'radio_button_unchecked';
  }
}
