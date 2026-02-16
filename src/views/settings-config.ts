// Settings Config — shared config read/write/cache utilities
// Used by all settings panels to read/write OpenClaw gateway config
// Each panel imports getConfig() and patchConfig() from here

import { gateway } from '../gateway';
import { showToast } from '../components/toast';

// ── Config Cache ───────────────────────────────────────────────────────────

let _configCache: Record<string, unknown> | null = null;
let _configLoading: Promise<Record<string, unknown>> | null = null;

/** Fetch config from gateway (cached, deduped). Call invalidate() after writes. */
export async function getConfig(): Promise<Record<string, unknown>> {
  if (_configCache) return _configCache;
  if (_configLoading) return _configLoading;
  _configLoading = gateway.configGet().then(r => {
    _configCache = r.config as Record<string, unknown>;
    _configLoading = null;
    return _configCache;
  }).catch(e => {
    _configLoading = null;
    throw e;
  });
  return _configLoading;
}

/** Deep-get a config value by dot path. Returns undefined if missing. */
export function getVal(config: Record<string, unknown>, path: string): unknown {
  return path.split('.').reduce((obj: any, key) => obj?.[key], config);
}

/** Build a nested patch object from a dot path + value.
 *  e.g. buildPatch('agents.defaults.thinkingDefault', 'high')
 *  → { agents: { defaults: { thinkingDefault: 'high' } } }
 */
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

/** Deep-merge source into target (mutates target). Arrays are replaced, not merged. */
function deepMerge(target: Record<string, unknown>, source: Record<string, unknown>): Record<string, unknown> {
  for (const key of Object.keys(source)) {
    const sv = source[key];
    const tv = target[key];
    if (sv && typeof sv === 'object' && !Array.isArray(sv) && tv && typeof tv === 'object' && !Array.isArray(tv)) {
      deepMerge(tv as Record<string, unknown>, sv as Record<string, unknown>);
    } else {
      target[key] = sv;
    }
  }
  return target;
}

/** Patch config via read → deep-merge → configSet (writes full config).
 *  This ensures partial patches don't wipe the rest of the config.
 *  Invalidates cache on success. Shows toast on error.
 *  Returns true on success, false on error.
 */
export async function patchConfig(patch: Record<string, unknown>, silent = false): Promise<boolean> {
  try {
    // Read current full config
    const current = await getConfig();
    // Deep-clone current config to avoid mutating cache
    const merged = JSON.parse(JSON.stringify(current));
    // Deep-merge patch into clone
    deepMerge(merged, patch);
    // Write the full merged config via config.set (accepts object directly)
    await gateway.configSet(merged);
    _configCache = null; // invalidate
    if (!silent) {
      showToast('Settings saved', 'success');
    }
    return true;
  } catch (e) {
    showToast(`Save failed: ${e instanceof Error ? e.message : e}`, 'error');
    return false;
  }
}

/** Convenience: patch a single dot-path value */
export async function patchValue(path: string, value: unknown, silent = false): Promise<boolean> {
  return patchConfig(buildPatch(path, value), silent);
}

/** Delete a config key by dot path (e.g. 'models.providers.google').
 *  Reads full config, clones, removes the key, then applies the full config.
 *  This avoids sending null values to the gateway.
 */
export async function deleteConfigKey(path: string, silent = false): Promise<boolean> {
  try {
    const current = await getConfig();
    const merged = JSON.parse(JSON.stringify(current));
    const keys = path.split('.');
    let obj: Record<string, unknown> = merged;
    for (let i = 0; i < keys.length - 1; i++) {
      if (!obj[keys[i]] || typeof obj[keys[i]] !== 'object') return true; // path doesn't exist, nothing to delete
      obj = obj[keys[i]] as Record<string, unknown>;
    }
    delete obj[keys[keys.length - 1]];

    // Write full config minus the deleted key via config.set
    await gateway.configSet(merged);
    _configCache = null;
    if (!silent) {
      showToast('Removed', 'success');
    }
    return true;
  } catch (e) {
    showToast(`Delete failed: ${e instanceof Error ? e.message : e}`, 'error');
    return false;
  }
}

/** Invalidate cache (call after external config changes or on reconnect) */
export function invalidateConfigCache(): void {
  _configCache = null;
  _configLoading = null;
}

// ── Connected state ────────────────────────────────────────────────────────

let _wsConnected = false;

export function setConnected(connected: boolean) {
  _wsConnected = connected;
  if (!connected) invalidateConfigCache();
}

export function isConnected(): boolean {
  return _wsConnected;
}

// ── UI Helpers ─────────────────────────────────────────────────────────────

/** Escape HTML */
export function esc(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
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
export function selectInput(options: Array<{ value: string; label: string }>, currentValue?: string): HTMLSelectElement {
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
export function numberInput(value?: number, opts?: { min?: number; max?: number; step?: number; placeholder?: string }): HTMLInputElement {
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
export function toggleSwitch(checked: boolean, label?: string): { container: HTMLLabelElement; checkbox: HTMLInputElement } {
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
