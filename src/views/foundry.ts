// Foundry View — Models & Chat Modes
// Models: shows configured providers and their available models
// Chat Modes: named presets (model + system prompt + temperature) for the chat mode selector

import { listModes, saveMode, deleteMode } from '../db';
import type { AgentMode } from '../db';
import { pawEngine } from '../engine';

const $ = (id: string) => document.getElementById(id);

// ── Module state ───────────────────────────────────────────────────────────
let _cachedModels: { id: string; name?: string; provider?: string; contextWindow?: number; reasoning?: boolean }[] = [];
let _editingModeId: string | null = null;

export function setWsConnected(_connected: boolean) { /* engine is always connected */ }

export function getCachedModels() {
  return _cachedModels;
}

// ── Helpers ────────────────────────────────────────────────────────────────
function escHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

// ── Models ─────────────────────────────────────────────────────────────────
export async function loadModels() {
  const list = $('models-list');
  const empty = $('models-empty');
  const loading = $('models-loading');
  if (!list) return;

  if (loading) loading.style.display = '';
  if (empty) empty.style.display = 'none';
  list.innerHTML = '';

  try {
    const config = await pawEngine.getConfig();
    if (loading) loading.style.display = 'none';

    // Reset cached models on each load to avoid duplicates
    _cachedModels = [];

    const providers = config.providers ?? [];
    if (!providers.length) {
      if (empty) empty.style.display = 'flex';
      return;
    }

    const KIND_ICONS: Record<string, string> = {
      ollama: 'pets', openai: 'smart_toy', anthropic: 'psychology', google: 'auto_awesome', openrouter: 'language', custom: 'build',
      deepseek: 'explore', grok: 'bolt', mistral: 'air', moonshot: 'dark_mode',
    };

    for (const p of providers) {
      const iconName = KIND_ICONS[p.kind] ?? 'build';
      const iconHtml = `<span class="ms ms-sm">${iconName}</span>`;
      const isDefault = p.id === config.default_provider;
      const card = document.createElement('div');
      card.className = 'model-card';
      card.innerHTML = `
        <div class="model-card-header">
          <span class="model-card-name">${iconHtml} ${escHtml(p.id)}</span>
          <span class="model-card-provider">${escHtml(p.kind)}${isDefault ? ' \u00b7 Default' : ''}</span>
        </div>
        <div class="model-card-meta">
          ${p.default_model ? `<span>Model: ${escHtml(p.default_model)}</span>` : '<span>No default model</span>'}
          ${p.base_url ? `<span>${escHtml(p.base_url)}</span>` : ''}
          <span>${p.api_key ? '<span class="ms ms-sm">key</span> Key set' : p.kind === 'ollama' ? '<span class="ms ms-sm">home</span> Local' : '<span class="ms ms-sm">warning</span> No key'}</span>
        </div>
      `;

      // Cache for mode editor model picker
      if (p.default_model) {
        _cachedModels.push({ id: p.default_model, name: p.default_model, provider: p.kind });
      }

      list.appendChild(card);
    }

    // Show global default model info
    if (config.default_model) {
      const infoCard = document.createElement('div');
      infoCard.className = 'model-card';
      infoCard.style.borderLeft = '3px solid var(--accent)';
      infoCard.innerHTML = `
        <div class="model-card-header">
          <span class="model-card-name"><span class="ms ms-sm">star</span> Default Model</span>
        </div>
        <div class="model-card-meta">
          <span>${escHtml(config.default_model)}</span>
          <span>Used when no agent or mode overrides the model</span>
        </div>
      `;
      list.appendChild(infoCard);
    }
  } catch (e) {
    console.warn('Models load failed:', e);
    if (loading) loading.style.display = 'none';
    if (empty) empty.style.display = 'flex';
  }
}

// ── Agent Modes ────────────────────────────────────────────────────────────
export async function loadModes() {
  const list = $('modes-list');
  const empty = $('modes-empty');
  if (!list) return;
  list.innerHTML = '';

  try {
    const modes = await listModes();
    if (!modes.length) {
      if (empty) empty.style.display = '';
      return;
    }
    if (empty) empty.style.display = 'none';

    for (const mode of modes) {
      const card = document.createElement('div');
      card.className = 'mode-card';
      card.style.borderLeftColor = mode.color || 'var(--accent)';
      card.innerHTML = `
        <div class="mode-card-icon" style="background:${mode.color}22">${mode.icon || mode.name?.charAt(0) || 'M'}</div>
        <div class="mode-card-info">
          <div class="mode-card-name">${escHtml(mode.name)}</div>
          <div class="mode-card-detail">${mode.model ? escHtml(mode.model) : 'Default model'} · ${mode.thinking_level || 'normal'} thinking</div>
        </div>
        ${mode.is_default ? '<span class="mode-card-default">Default</span>' : ''}
      `;
      card.addEventListener('click', () => editMode(mode));
      list.appendChild(card);
    }
  } catch (e) {
    console.warn('Modes load failed:', e);
  }
}

function editMode(mode?: AgentMode) {
  _editingModeId = mode?.id ?? null;
  const modal = $('mode-modal');
  const title = $('mode-modal-title');
  const deleteBtn = $('mode-modal-delete');
  if (!modal) return;
  modal.style.display = 'flex';
  if (title) title.textContent = mode ? 'Edit Agent Mode' : 'New Agent Mode';
  if (deleteBtn) deleteBtn.style.display = mode && !mode.is_default ? '' : 'none';

  const modelSelect = $('mode-form-model') as HTMLSelectElement;
  if (modelSelect) {
    modelSelect.innerHTML = '<option value="">Default model</option>';
    for (const m of _cachedModels) {
      const opt = document.createElement('option');
      opt.value = m.id;
      opt.textContent = m.name ?? m.id;
      if (mode?.model === m.id) opt.selected = true;
      modelSelect.appendChild(opt);
    }
  }

  ($('mode-form-icon') as HTMLInputElement).value = mode?.icon ?? '';
  ($('mode-form-name') as HTMLInputElement).value = mode?.name ?? '';
  ($('mode-form-color') as HTMLInputElement).value = mode?.color ?? '#0073EA';
  ($('mode-form-prompt') as HTMLTextAreaElement).value = mode?.system_prompt ?? '';
  ($('mode-form-thinking') as HTMLSelectElement).value = mode?.thinking_level ?? 'normal';
  ($('mode-form-temp') as HTMLInputElement).value = String(mode?.temperature ?? 1);
  const tempVal = $('mode-form-temp-value');
  if (tempVal) tempVal.textContent = String(mode?.temperature ?? 1.0);
}

function hideModeModal() {
  const modal = $('mode-modal');
  if (modal) modal.style.display = 'none';
  _editingModeId = null;
}

// ── Event wiring ───────────────────────────────────────────────────────────
export function initFoundryEvents() {
  $('refresh-models-btn')?.addEventListener('click', () => { loadModels(); loadModes(); });

  // Foundry tab switching (Models / Chat Modes)
  document.querySelectorAll('.foundry-tab').forEach(tab => {
    tab.addEventListener('click', () => {
      document.querySelectorAll('.foundry-tab').forEach(t => t.classList.remove('active'));
      tab.classList.add('active');
      const target = tab.getAttribute('data-foundry-tab');
      const modelsPanel = $('foundry-models-panel');
      const modesPanel = $('foundry-modes-panel');
      if (modelsPanel) modelsPanel.style.display = target === 'models' ? '' : 'none';
      if (modesPanel) modesPanel.style.display = target === 'modes' ? '' : 'none';
    });
  });

  // Mode modal
  $('modes-add-btn')?.addEventListener('click', () => editMode());
  $('mode-modal-close')?.addEventListener('click', hideModeModal);
  $('mode-modal-cancel')?.addEventListener('click', hideModeModal);

  $('mode-form-temp')?.addEventListener('input', () => {
    const val = ($('mode-form-temp') as HTMLInputElement).value;
    const display = $('mode-form-temp-value');
    if (display) display.textContent = parseFloat(val).toFixed(1);
  });

  $('mode-modal-save')?.addEventListener('click', async () => {
    const name = ($('mode-form-name') as HTMLInputElement).value.trim();
    if (!name) { alert('Name is required'); return; }
    const id = _editingModeId ?? name.toLowerCase().replace(/\s+/g, '-').replace(/[^a-z0-9-]/g, '');
    await saveMode({
      id,
      name,
      icon: ($('mode-form-icon') as HTMLInputElement).value || '',
      color: ($('mode-form-color') as HTMLInputElement).value || '#0073EA',
      model: ($('mode-form-model') as HTMLSelectElement).value || null,
      system_prompt: ($('mode-form-prompt') as HTMLTextAreaElement).value,
      thinking_level: ($('mode-form-thinking') as HTMLSelectElement).value,
      temperature: parseFloat(($('mode-form-temp') as HTMLInputElement).value),
    });
    hideModeModal();
    loadModes();
  });

  $('mode-modal-delete')?.addEventListener('click', async () => {
    if (!_editingModeId || !confirm('Delete this mode?')) return;
    await deleteMode(_editingModeId);
    hideModeModal();
    loadModes();
  });
}

