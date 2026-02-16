// Settings: Environment Variables
// CRUD for env.vars + shell env toggle
// ~120 lines

import {
  getConfig, patchConfig, getVal, isConnected,
  esc, formRow, textInput, toggleSwitch, numberInput, saveReloadButtons
} from './settings-config';

const $ = (id: string) => document.getElementById(id);

// ── Render ──────────────────────────────────────────────────────────────────

export async function loadEnvSettings() {
  if (!isConnected()) return;
  const container = $('settings-env-content');
  if (!container) return;
  container.innerHTML = '<p style="color:var(--text-muted)">Loading…</p>';

  try {
    const config = await getConfig();
    const vars = (getVal(config, 'env.vars') ?? {}) as Record<string, string>;
    const shellEnabled = getVal(config, 'env.shellEnv.enabled') as boolean | undefined;
    const shellTimeout = getVal(config, 'env.shellEnv.timeoutMs') as number | undefined;

    container.innerHTML = '';

    // ── Shell Env ────────────────────────────────────────────────────────
    const shellSection = document.createElement('div');
    shellSection.style.cssText = 'margin-bottom:16px';
    const { container: shellToggle, checkbox: shellCb } = toggleSwitch(
      shellEnabled !== false,
      'Inherit shell environment variables'
    );
    shellSection.appendChild(shellToggle);

    const timeoutRow = formRow('Shell env timeout (ms)');
    const timeoutInp = numberInput(shellTimeout ?? 5000, { min: 0, step: 500, placeholder: '5000' });
    timeoutInp.style.maxWidth = '140px';
    timeoutRow.appendChild(timeoutInp);
    shellSection.appendChild(timeoutRow);
    container.appendChild(shellSection);

    // ── Variables Table ──────────────────────────────────────────────────
    const tableHeader = document.createElement('div');
    tableHeader.style.cssText = 'display:flex;justify-content:space-between;align-items:center;margin-bottom:8px';
    tableHeader.innerHTML = '<h3 class="settings-subsection-title" style="margin:0">Environment Variables</h3>';
    const addBtn = document.createElement('button');
    addBtn.className = 'btn btn-primary btn-sm';
    addBtn.textContent = '+ Add Variable';
    tableHeader.appendChild(addBtn);
    container.appendChild(tableHeader);

    const table = document.createElement('div');
    table.className = 'env-table';
    table.style.cssText = 'display:flex;flex-direction:column;gap:6px';
    const rows: Array<{ key: HTMLInputElement; val: HTMLInputElement; row: HTMLElement }> = [];

    function addRow(key = '', value = '') {
      const row = document.createElement('div');
      row.style.cssText = 'display:flex;gap:8px;align-items:center';
      const keyInp = textInput(key, 'KEY');
      keyInp.style.cssText = 'flex:1;max-width:200px;font-family:var(--font-mono);font-size:12px';
      const valInp = textInput(value, 'value', key.toLowerCase().includes('key') || key.toLowerCase().includes('secret') || key.toLowerCase().includes('token') ? 'password' : 'text');
      valInp.style.cssText = 'flex:2;font-family:var(--font-mono);font-size:12px';
      const delBtn = document.createElement('button');
      delBtn.className = 'btn btn-danger btn-sm';
      delBtn.textContent = '✕';
      delBtn.title = 'Remove';
      delBtn.addEventListener('click', () => {
        row.remove();
        const idx = rows.findIndex(r => r.row === row);
        if (idx !== -1) rows.splice(idx, 1);
      });
      row.appendChild(keyInp);
      row.appendChild(valInp);
      row.appendChild(delBtn);
      table.appendChild(row);
      rows.push({ key: keyInp, val: valInp, row });
    }

    // Populate existing vars
    for (const [k, v] of Object.entries(vars)) {
      addRow(k, String(v));
    }

    addBtn.addEventListener('click', () => addRow());
    container.appendChild(table);

    // Empty state
    if (Object.keys(vars).length === 0) {
      const hint = document.createElement('p');
      hint.style.cssText = 'color:var(--text-muted);font-size:12px;padding:8px 0';
      hint.textContent = 'No environment variables set. Add API keys and custom variables here.';
      container.appendChild(hint);
    }

    // ── Save ─────────────────────────────────────────────────────────────
    container.appendChild(saveReloadButtons(
      async () => {
        const newVars: Record<string, string | null> = {};
        // First null-out all old keys (to delete removed ones)
        for (const oldKey of Object.keys(vars)) {
          newVars[oldKey] = null;
        }
        // Then set current rows
        for (const r of rows) {
          const k = r.key.value.trim();
          const v = r.val.value;
          if (k) newVars[k] = v;
        }
        const patch: Record<string, unknown> = {
          env: {
            vars: newVars,
            shellEnv: { enabled: shellCb.checked, timeoutMs: parseInt(timeoutInp.value) || 5000 }
          }
        };
        const ok = await patchConfig(patch);
        if (ok) loadEnvSettings(); // reload to reflect deletions
      },
      () => loadEnvSettings()
    ));

  } catch (e) {
    container.innerHTML = `<p style="color:var(--danger)">Failed to load: ${esc(String(e))}</p>`;
  }
}

export function initEnvSettings() {
  // All dynamic — nothing to bind
}
