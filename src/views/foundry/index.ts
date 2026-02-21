// Foundry — orchestration, state, public API

import { saveMode, deleteMode } from '../../db';
import { $, confirmModal } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { initMoleculesState, loadModels, loadModes, editMode, hideModeModal } from './molecules';

// ── State ──────────────────────────────────────────────────────────────────
let _cachedModels: {
  id: string;
  name?: string;
  provider?: string;
  contextWindow?: number;
  reasoning?: boolean;
}[] = [];
let _editingModeId: string | null = null;

// ── State bridge ───────────────────────────────────────────────────────────
const { setMoleculesState } = initMoleculesState();
setMoleculesState({
  getCachedModels: () => _cachedModels,
  setCachedModels: (m) => {
    _cachedModels = m;
  },
  getEditingModeId: () => _editingModeId,
  setEditingModeId: (id) => {
    _editingModeId = id;
  },
});

// ── Public API ─────────────────────────────────────────────────────────────
export function getCachedModels() {
  return _cachedModels;
}

// ── Event wiring ───────────────────────────────────────────────────────────
export function initFoundryEvents() {
  $('refresh-models-btn')?.addEventListener('click', () => {
    loadModels();
    loadModes();
  });

  // Foundry tab switching (Models / Chat Modes)
  document.querySelectorAll('.foundry-tab').forEach((tab) => {
    tab.addEventListener('click', () => {
      document.querySelectorAll('.foundry-tab').forEach((t) => t.classList.remove('active'));
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
    if (!name) {
      showToast('Name is required', 'error');
      return;
    }
    const id =
      _editingModeId ??
      name
        .toLowerCase()
        .replace(/\s+/g, '-')
        .replace(/[^a-z0-9-]/g, '');
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
    if (!_editingModeId || !(await confirmModal('Delete this mode?'))) return;
    await deleteMode(_editingModeId);
    hideModeModal();
    loadModes();
  });
}

export { loadModels, loadModes };
