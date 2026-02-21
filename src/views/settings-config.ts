// Settings Config — shared config read/write/cache utilities
// Used by all settings panels to read/write Paw engine config
// Each panel imports getConfig() and patchConfig() from here
//
// All reads/writes go through the Paw engine (Tauri IPC).
// No gateway, no WebSocket — direct Rust calls.

import { pawEngine, type EngineConfig } from '../engine';
import { showToast } from '../components/toast';

// ── Config Cache ───────────────────────────────────────────────────────────

let _configCache: Record<string, unknown> | null = null;
let _configLoading: Promise<Record<string, unknown>> | null = null;

/** Fetch config from engine (cached, deduped). Call invalidate() after writes. */
export async function getConfig(): Promise<Record<string, unknown>> {
  if (_configCache) return _configCache;
  if (_configLoading) return _configLoading;
  _configLoading = pawEngine
    .getConfig()
    .then((cfg) => {
      _configCache = cfg as unknown as Record<string, unknown>;
      _configLoading = null;
      return _configCache;
    })
    .catch((e) => {
      _configLoading = null;
      throw e;
    });
  return _configLoading;
}

/** Get raw typed engine config. */
export async function getEngineConfig(): Promise<EngineConfig> {
  return pawEngine.getConfig();
}

/** Save full engine config. */
export async function setEngineConfig(config: EngineConfig, silent = false): Promise<boolean> {
  try {
    await pawEngine.setConfig(config);
    _configCache = null;
    _configLoading = null;
    if (!silent) {
      const modelName = config.default_model;
      const toastMsg = modelName ? `Settings saved — active model: ${modelName}` : 'Settings saved';
      showToast(toastMsg, 'success');
    }
    // Refresh the chat header model label
    const refreshFn = (window as unknown as Record<string, unknown>).__refreshModelLabel as
      | (() => void)
      | undefined;
    if (refreshFn) refreshFn();
    return true;
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    showToast(`Save failed: ${msg}`, 'error');
    return false;
  }
}

/** Deep-get a config value by dot path. Returns undefined if missing. */
export function getVal(config: Record<string, unknown>, path: string): unknown {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return path.split('.').reduce((obj: any, key) => obj?.[key], config);
}

/** Build a nested patch object from a dot path + value. */
export function buildPatch(path: string, value: unknown): Record<string, unknown> {
  const keys = path.split('.');
  const patch: Record<string, unknown> = {};
  let current: Record<string, unknown> = patch;
  for (let i = 0; i < keys.length - 1; i++) {
    current[keys[i]] = {};
    current = current[keys[i]] as Record<string, unknown>;
  }
  current[keys[keys.length - 1]] = value;
  return patch;
}

/** Patch config — reads current engine config, merges the patch, and saves.
 *  Returns true on success, false on error.
 */
export async function patchConfig(
  patch: Record<string, unknown>,
  silent = false,
): Promise<boolean> {
  try {
    const current = await pawEngine.getConfig();
    const merged = deepMerge(current as unknown as Record<string, unknown>, patch);
    await pawEngine.setConfig(merged as unknown as EngineConfig);
    _configCache = null;
    _configLoading = null;
    if (!silent) showToast('Settings saved', 'success');
    return true;
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    showToast(`Save failed: ${msg}`, 'error');
    return false;
  }
}

/** Deep merge source into target. null values delete keys. */
function deepMerge(
  target: Record<string, unknown>,
  source: Record<string, unknown>,
): Record<string, unknown> {
  const result = { ...target };
  for (const [key, value] of Object.entries(source)) {
    if (value === null || value === undefined) {
      delete result[key];
    } else if (
      typeof value === 'object' &&
      !Array.isArray(value) &&
      typeof result[key] === 'object' &&
      !Array.isArray(result[key])
    ) {
      result[key] = deepMerge(
        result[key] as Record<string, unknown>,
        value as Record<string, unknown>,
      );
    } else {
      result[key] = value;
    }
  }
  return result;
}

/** Convenience: patch a single dot-path value */
export async function patchValue(path: string, value: unknown, silent = false): Promise<boolean> {
  return patchConfig(buildPatch(path, value), silent);
}

/** Delete a config key by dot path. */
export async function deleteConfigKey(path: string, silent = false): Promise<boolean> {
  return patchConfig(buildPatch(path, null), silent);
}

/** Invalidate cache (call after external config changes) */
export function invalidateConfigCache(): void {
  _configCache = null;
  _configLoading = null;
}

// ── Connected state ────────────────────────────────────────────────────────
// Engine is always "connected" — calling setConnected invalidates config cache.

export function setConnected(_connected: boolean) {
  invalidateConfigCache();
}

// ── UI Helpers ─────────────────────────────────────────────────────────────

/** Escape HTML */
export function esc(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

/** Create a labeled form row. Returns the container element. */
export function formRow(label: string, description?: string): HTMLDivElement {
  const row = document.createElement('div');
  row.className = 'form-group';
  const lbl = document.createElement('label');
  lbl.className = 'form-label';
  lbl.textContent = label;
  row.appendChild(lbl);
  if (description) {
    const desc = document.createElement('p');
    desc.className = 'form-hint';
    desc.style.cssText = 'margin:0 0 4px 0;font-size:11px;color:var(--text-muted)';
    desc.textContent = description;
    row.appendChild(desc);
  }
  return row;
}

/** Create a select dropdown with options. Returns the <select>. */
export function selectInput(
  options: Array<{ value: string; label: string }>,
  currentValue?: string,
): HTMLSelectElement {
  const sel = document.createElement('select');
  sel.className = 'form-input';
  for (const opt of options) {
    const o = document.createElement('option');
    o.value = opt.value;
    o.textContent = opt.label;
    if (opt.value === currentValue) o.selected = true;
    sel.appendChild(o);
  }
  return sel;
}

/** Create a text input. Returns the <input>. */
export function textInput(value?: string, placeholder?: string, type = 'text'): HTMLInputElement {
  const inp = document.createElement('input');
  inp.type = type;
  inp.className = 'form-input';
  if (value != null) inp.value = String(value);
  if (placeholder) inp.placeholder = placeholder;
  return inp;
}

/** Create a number input. */
export function numberInput(
  value?: number,
  opts?: { min?: number; max?: number; step?: number; placeholder?: string },
): HTMLInputElement {
  const inp = document.createElement('input');
  inp.type = 'number';
  inp.className = 'form-input';
  if (value != null) inp.value = String(value);
  if (opts?.min != null) inp.min = String(opts.min);
  if (opts?.max != null) inp.max = String(opts.max);
  if (opts?.step != null) inp.step = String(opts.step);
  if (opts?.placeholder) inp.placeholder = opts.placeholder;
  return inp;
}

/** Create a toggle switch. Returns { container, checkbox }. */
export function toggleSwitch(
  checked: boolean,
  label?: string,
): { container: HTMLLabelElement; checkbox: HTMLInputElement } {
  const container = document.createElement('label');
  container.className = 'security-toggle-row';
  container.style.cursor = 'pointer';
  const checkbox = document.createElement('input');
  checkbox.type = 'checkbox';
  checkbox.checked = checked;
  container.appendChild(checkbox);
  if (label) {
    const info = document.createElement('div');
    info.className = 'security-toggle-info';
    const lbl = document.createElement('div');
    lbl.className = 'security-toggle-label';
    lbl.textContent = label;
    info.appendChild(lbl);
    container.appendChild(info);
  }
  return { container, checkbox };
}

/** Create a "Save" + "Reload" button pair. */
export function saveReloadButtons(onSave: () => void, onReload: () => void): HTMLDivElement {
  const row = document.createElement('div');
  row.style.cssText = 'display:flex;gap:8px;margin-top:16px';
  const save = document.createElement('button');
  save.className = 'btn btn-primary';
  save.textContent = 'Save';
  save.addEventListener('click', onSave);
  const reload = document.createElement('button');
  reload.className = 'btn btn-ghost btn-sm';
  reload.textContent = 'Reload';
  reload.addEventListener('click', onReload);
  row.appendChild(save);
  row.appendChild(reload);
  return row;
}
