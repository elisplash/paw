// Settings — Skills (Credential Vault)
// Manages skill enable/disable, credential configuration, and vault revocation.

import { pawEngine, type EngineSkillStatus } from '../engine';
import { isEngineMode } from '../engine-bridge';

const $ = (id: string) => document.getElementById(id);

function escHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

function showVaultToast(message: string, type: 'success' | 'error' | 'info') {
  const toast = $('skills-vault-toast');
  if (!toast) return;
  toast.textContent = message;
  toast.style.display = 'block';
  toast.style.background = type === 'error' ? 'var(--bg-danger, #f87171)' :
    type === 'success' ? 'var(--bg-success, #34d399)' : 'var(--bg-info, #60a5fa)';
  toast.style.color = '#fff';
  setTimeout(() => { toast.style.display = 'none'; }, type === 'error' ? 6000 : 3000);
}

/** Load and render the skills vault settings. */
export async function loadSkillsSettings(): Promise<void> {
  const loading = $('skills-vault-loading');
  const list = $('skills-vault-list');

  if (!isEngineMode()) {
    if (loading) loading.textContent = 'Skills vault is only available in Engine mode. Switch to Engine in General settings.';
    if (list) list.innerHTML = '';
    return;
  }

  try {
    if (loading) loading.style.display = '';
    const skills = await pawEngine.skillsList();
    if (loading) loading.style.display = 'none';
    if (list) list.innerHTML = renderSkillsList(skills);
    bindSkillEvents(skills);
  } catch (e) {
    console.error('[skills-settings] Load failed:', e);
    if (loading) loading.textContent = `Failed to load skills: ${e}`;
  }
}

function renderSkillsList(skills: EngineSkillStatus[]): string {
  if (skills.length === 0) return '<p style="color:var(--text-muted)">No skills available.</p>';

  return skills.map(s => {
    const statusIcon = s.is_ready ? '🟢' : s.enabled ? '🟡' : '⚫';
    const statusText = s.is_ready ? 'Ready' : s.enabled ? 'Missing credentials' : 'Disabled';

    return `
    <div class="skill-vault-card" data-skill-id="${escHtml(s.id)}" style="
      border:1px solid var(--border-color);
      border-radius:10px;
      padding:16px;
      margin-bottom:12px;
      background:var(--bg-secondary);
    ">
      <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:10px;">
        <div>
          <span style="font-size:20px; margin-right:8px;">${escHtml(s.icon)}</span>
          <strong style="font-size:15px;">${escHtml(s.name)}</strong>
          <span style="margin-left:10px; font-size:12px; color:var(--text-muted);">${statusIcon} ${statusText}</span>
        </div>
        <div style="display:flex; gap:6px; align-items:center;">
          <label style="display:flex; align-items:center; gap:6px; cursor:pointer; font-size:13px;">
            <input type="checkbox" class="skill-enabled-toggle" data-skill="${escHtml(s.id)}" ${s.enabled ? 'checked' : ''} />
            Enable
          </label>
          <button class="btn btn-ghost btn-sm skill-revoke-btn" data-skill="${escHtml(s.id)}" title="Revoke all credentials" style="color:var(--text-danger, red); font-size:12px;">Revoke All</button>
        </div>
      </div>
      <p style="color:var(--text-secondary); font-size:13px; margin:0 0 12px 0;">${escHtml(s.description)}</p>
      <div style="font-size:12px; color:var(--text-muted); margin-bottom:8px;">
        Tools: ${s.tool_names.map(t => `<code style="background:var(--bg-tertiary); padding:1px 5px; border-radius:4px;">${escHtml(t)}</code>`).join(' ')}
      </div>
      ${renderCredentialFields(s)}
    </div>`;
  }).join('');
}

function renderCredentialFields(skill: EngineSkillStatus): string {
  if (skill.required_credentials.length === 0) return '';

  const rows = skill.required_credentials.map(cred => {
    const isSet = skill.configured_credentials.includes(cred.key);
    const reqBadge = cred.required ? '<span style="color:var(--text-danger, red);font-size:11px;">*</span>' : '';

    return `
    <div class="skill-cred-row" style="display:flex; gap:8px; align-items:center; margin-bottom:6px;">
      <div style="min-width:150px;">
        <label style="font-size:12px; color:var(--text-secondary);">${escHtml(cred.label)} ${reqBadge}</label>
        <div style="font-size:11px; color:var(--text-muted);">${escHtml(cred.description)}</div>
      </div>
      <div style="flex:1; display:flex; gap:4px; align-items:center;">
        <input
          type="password"
          class="form-input skill-cred-input"
          data-skill="${escHtml(skill.id)}"
          data-key="${escHtml(cred.key)}"
          placeholder="${isSet ? '••••••••' : escHtml(cred.placeholder)}"
          style="font-size:12px; flex:1;"
        />
        <button class="btn btn-ghost btn-sm skill-cred-save" data-skill="${escHtml(skill.id)}" data-key="${escHtml(cred.key)}" style="font-size:11px; padding:4px 8px;">
          ${isSet ? 'Update' : 'Set'}
        </button>
        ${isSet ? `<button class="btn btn-ghost btn-sm skill-cred-delete" data-skill="${escHtml(skill.id)}" data-key="${escHtml(cred.key)}" style="font-size:11px; padding:4px 8px; color:var(--text-danger, red);">✕</button>` : ''}
      </div>
      ${isSet ? '<span style="color:var(--text-success, green); font-size:12px;">✓</span>' : '<span style="color:var(--text-muted); font-size:12px;">—</span>'}
    </div>`;
  }).join('');

  return `<div style="border-top:1px solid var(--border-color); padding-top:10px; margin-top:8px;">
    <div style="font-size:12px; font-weight:600; color:var(--text-secondary); margin-bottom:8px;">Credentials</div>
    ${rows}
  </div>`;
}

function bindSkillEvents(_skills: EngineSkillStatus[]): void {
  // Enable/disable toggles
  document.querySelectorAll('.skill-enabled-toggle').forEach(el => {
    el.addEventListener('change', async (e) => {
      const input = e.target as HTMLInputElement;
      const skillId = input.dataset.skill!;
      try {
        await pawEngine.skillSetEnabled(skillId, input.checked);
        showVaultToast(`${skillId} ${input.checked ? 'enabled' : 'disabled'}`, 'success');
        // Reload to refresh status
        await loadSkillsSettings();
      } catch (err) {
        showVaultToast(`Failed: ${err}`, 'error');
        input.checked = !input.checked;
      }
    });
  });

  // Save credential buttons
  document.querySelectorAll('.skill-cred-save').forEach(el => {
    el.addEventListener('click', async () => {
      const btn = el as HTMLButtonElement;
      const skillId = btn.dataset.skill!;
      const key = btn.dataset.key!;
      const input = document.querySelector(`.skill-cred-input[data-skill="${skillId}"][data-key="${key}"]`) as HTMLInputElement;
      const value = input?.value?.trim();
      if (!value) { showVaultToast('Enter a value first', 'info'); return; }

      try {
        await pawEngine.skillSetCredential(skillId, key, value);
        showVaultToast(`${key} saved securely`, 'success');
        input.value = '';
        await loadSkillsSettings();
      } catch (err) {
        showVaultToast(`Failed: ${err}`, 'error');
      }
    });
  });

  // Delete credential buttons
  document.querySelectorAll('.skill-cred-delete').forEach(el => {
    el.addEventListener('click', async () => {
      const btn = el as HTMLButtonElement;
      const skillId = btn.dataset.skill!;
      const key = btn.dataset.key!;

      try {
        await pawEngine.skillDeleteCredential(skillId, key);
        showVaultToast(`${key} removed`, 'success');
        await loadSkillsSettings();
      } catch (err) {
        showVaultToast(`Failed: ${err}`, 'error');
      }
    });
  });

  // Revoke all buttons
  document.querySelectorAll('.skill-revoke-btn').forEach(el => {
    el.addEventListener('click', async () => {
      const btn = el as HTMLButtonElement;
      const skillId = btn.dataset.skill!;

      if (!confirm(`Revoke ALL credentials for ${skillId}? This can\'t be undone.`)) return;

      try {
        await pawEngine.skillRevokeAll(skillId);
        showVaultToast(`All ${skillId} credentials revoked`, 'success');
        await loadSkillsSettings();
      } catch (err) {
        showVaultToast(`Failed: ${err}`, 'error');
      }
    });
  });
}
