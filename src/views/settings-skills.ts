// Pawz Skills — Full Skill Vault and Instruction Browser
// Shows all skills grouped by category with binary/env status, credentials, and install hints.

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
  toast.style.background = type === 'error' ? 'var(--bg-danger)' :
    type === 'success' ? 'var(--bg-success)' : 'var(--bg-info)';
  toast.style.color = '#fff';
  setTimeout(() => { toast.style.display = 'none'; }, type === 'error' ? 6000 : 3000);
}

const CATEGORY_META: Record<string, { label: string; icon: string; order: number }> = {
  Vault:         { label: 'Vault (Credentials)', icon: '🔐', order: 0 },
  Communication: { label: 'Communication',       icon: '💬', order: 1 },
  Productivity:  { label: 'Productivity',         icon: '📋', order: 2 },
  Api:           { label: 'API Integrations',     icon: '🔌', order: 3 },
  Development:   { label: 'Development',          icon: '🛠️', order: 4 },
  Media:         { label: 'Media',                icon: '🎬', order: 5 },
  SmartHome:     { label: 'Smart Home & IoT',     icon: '🏠', order: 6 },
  Cli:           { label: 'CLI Tools',            icon: '⌨️', order: 7 },
  System:        { label: 'System',               icon: '🖥️', order: 8 },
};

let _currentFilter: string = 'all';

/** Load and render the skills vault settings. */
export async function loadSkillsSettings(): Promise<void> {
  const loading = $('skills-vault-loading');
  const list = $('skills-vault-list');

  if (!isEngineMode()) {
    if (loading) loading.textContent = 'Pawz engine is required.';
    if (list) list.innerHTML = '';
    return;
  }

  try {
    if (loading) loading.style.display = '';
    const skills = await pawEngine.skillsList();
    if (loading) loading.style.display = 'none';
    if (list) list.innerHTML = renderSkillsPage(skills);
    bindFilterEvents(skills);
    bindSkillEvents(skills);
  } catch (e) {
    console.error('[skills-settings] Load failed:', e);
    if (loading) loading.textContent = `Failed to load skills: ${e}`;
  }
}

function renderSkillsPage(skills: EngineSkillStatus[]): string {
  if (skills.length === 0) return '<p style="color:var(--text-muted)">No skills available.</p>';

  const enabledCount = skills.filter(s => s.enabled).length;
  const readyCount = skills.filter(s => s.is_ready).length;

  // Summary bar
  const summary = `<div style="
    display:flex; gap:16px; align-items:center; margin-bottom:16px; padding:12px 16px;
    background:var(--bg-secondary); border-radius:10px; border:1px solid var(--border-color);
    flex-wrap:wrap;
  ">
    <span style="font-size:14px; font-weight:600; color:var(--text-primary);">
      ${skills.length} Skills
    </span>
    <span style="font-size:12px; color:var(--text-muted);">
      🟢 ${readyCount} ready &nbsp;·&nbsp; ⚡ ${enabledCount} enabled
    </span>
  </div>`;

  // Category filter tabs
  const categories = [...new Set(skills.map(s => s.category))];
  categories.sort((a, b) => (CATEGORY_META[a]?.order ?? 99) - (CATEGORY_META[b]?.order ?? 99));

  const tabs = `<div class="skills-filter-tabs" style="
    display:flex; gap:6px; margin-bottom:16px; flex-wrap:wrap;
  ">
    <button class="btn btn-sm skills-filter-btn ${_currentFilter === 'all' ? 'btn-primary' : 'btn-ghost'}" data-filter="all" style="font-size:12px;">All</button>
    <button class="btn btn-sm skills-filter-btn ${_currentFilter === 'enabled' ? 'btn-primary' : 'btn-ghost'}" data-filter="enabled" style="font-size:12px;">⚡ Enabled</button>
    ${categories.map(c => {
      const meta = CATEGORY_META[c] || { label: c, icon: '📦', order: 99 };
      const count = skills.filter(s => s.category === c).length;
      return `<button class="btn btn-sm skills-filter-btn ${_currentFilter === c ? 'btn-primary' : 'btn-ghost'}" data-filter="${escHtml(c)}" style="font-size:12px;">${meta.icon} ${meta.label} (${count})</button>`;
    }).join('')}
  </div>`;

  // Render filtered skills
  const filtered = _currentFilter === 'all' ? skills :
    _currentFilter === 'enabled' ? skills.filter(s => s.enabled) :
    skills.filter(s => s.category === _currentFilter);

  // Group by category
  const grouped: Record<string, EngineSkillStatus[]> = {};
  for (const s of filtered) {
    if (!grouped[s.category]) grouped[s.category] = [];
    grouped[s.category].push(s);
  }

  const sortedCats = Object.keys(grouped).sort((a, b) =>
    (CATEGORY_META[a]?.order ?? 99) - (CATEGORY_META[b]?.order ?? 99)
  );

  const sections = sortedCats.map(cat => {
    const meta = CATEGORY_META[cat] || { label: cat, icon: '📦', order: 99 };
    const cards = grouped[cat].map(s => renderSkillCard(s)).join('');
    return `
      <div class="skill-category-group" style="margin-bottom:20px;">
        <h3 style="font-size:14px; color:var(--text-secondary); margin:0 0 10px 4px; font-weight:600;">
          ${meta.icon} ${meta.label}
        </h3>
        ${cards}
      </div>`;
  }).join('');

  return summary + tabs + sections;
}

function renderSkillCard(s: EngineSkillStatus): string {
  // Status determination
  let statusIcon: string;
  let statusText: string;
  if (s.is_ready) {
    statusIcon = '🟢'; statusText = 'Ready';
  } else if (s.enabled && s.missing_binaries.length > 0) {
    statusIcon = '🔴'; statusText = 'Missing binaries';
  } else if (s.enabled && s.missing_credentials.length > 0) {
    statusIcon = '🟡'; statusText = 'Missing credentials';
  } else if (s.enabled && s.missing_env_vars.length > 0) {
    statusIcon = '🟡'; statusText = 'Missing env vars';
  } else if (s.enabled) {
    statusIcon = '🟡'; statusText = 'Setup incomplete';
  } else {
    statusIcon = '⚫'; statusText = 'Disabled';
  }

  const hasCreds = s.required_credentials.length > 0;
  const hasTools = s.tool_names.length > 0;

  // Badges
  const badges: string[] = [];
  if (s.has_instructions) badges.push('<span style="background:var(--bg-tertiary); color:var(--text-muted); font-size:10px; padding:1px 6px; border-radius:8px;">📖 Instruction</span>');
  if (hasTools) badges.push('<span style="background:var(--bg-tertiary); color:var(--text-muted); font-size:10px; padding:1px 6px; border-radius:8px;">🔧 Tools</span>');
  if (hasCreds) badges.push('<span style="background:var(--bg-tertiary); color:var(--text-muted); font-size:10px; padding:1px 6px; border-radius:8px;">🔑 Vault</span>');

  return `
  <div class="skill-vault-card" data-skill-id="${escHtml(s.id)}" style="
    border:1px solid var(--border-color);
    border-radius:10px;
    padding:14px 16px;
    margin-bottom:8px;
    background:var(--bg-secondary);
    ${s.enabled ? 'border-left:3px solid var(--accent-color);' : ''}
  ">
    <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:6px;">
      <div style="display:flex; align-items:center; gap:8px;">
        <span style="font-size:20px;">${escHtml(s.icon)}</span>
        <div>
          <strong style="font-size:14px;">${escHtml(s.name)}</strong>
          <span style="margin-left:8px; font-size:11px; color:var(--text-muted);">${statusIcon} ${statusText}</span>
        </div>
      </div>
      <div style="display:flex; gap:6px; align-items:center;">
        <label style="display:flex; align-items:center; gap:4px; cursor:pointer; font-size:12px;">
          <input type="checkbox" class="skill-enabled-toggle" data-skill="${escHtml(s.id)}" ${s.enabled ? 'checked' : ''} />
          Enable
        </label>
        ${hasCreds ? `<button class="btn btn-ghost btn-sm skill-revoke-btn" data-skill="${escHtml(s.id)}" title="Revoke all credentials" style="color:var(--text-danger); font-size:11px;">Revoke</button>` : ''}
      </div>
    </div>
    <p style="color:var(--text-secondary); font-size:12px; margin:0 0 8px 0;">${escHtml(s.description)}</p>
    <div style="display:flex; gap:6px; flex-wrap:wrap; margin-bottom:6px;">${badges.join('')}</div>
    ${hasTools ? `<div style="font-size:11px; color:var(--text-muted); margin-bottom:4px;">
      Tools: ${s.tool_names.map(t => `<code style="background:var(--bg-tertiary); padding:1px 5px; border-radius:4px; font-size:10px;">${escHtml(t)}</code>`).join(' ')}
    </div>` : ''}
    ${renderBinaryStatus(s)}
    ${renderEnvVarStatus(s)}
    ${renderCredentialFields(s)}
    ${renderAdvancedSection(s)}
  </div>`;
}

function renderBinaryStatus(s: EngineSkillStatus): string {
  if (s.missing_binaries.length === 0) return '';

  return `<div style="
    background:var(--bg-tertiary); border-radius:6px; padding:8px 10px; margin-bottom:6px; font-size:12px;
  ">
    <div style="color:var(--text-danger); margin-bottom:4px;">
      ⚠️ Missing binaries: ${s.missing_binaries.map(b => `<code style="font-weight:600;">${escHtml(b)}</code>`).join(', ')}
    </div>
    ${s.install_hint ? `<div style="color:var(--text-muted);">
      Install: <code style="background:var(--bg-secondary); padding:2px 6px; border-radius:4px; cursor:pointer; user-select:all;">${escHtml(s.install_hint)}</code>
    </div>` : ''}
  </div>`;
}

function renderEnvVarStatus(s: EngineSkillStatus): string {
  if (s.missing_env_vars.length === 0) return '';

  return `<div style="
    background:var(--bg-tertiary); border-radius:6px; padding:8px 10px; margin-bottom:6px; font-size:12px;
  ">
    <div style="color:var(--text-warning, #fbbf24);">
      ⚠️ Missing environment variables: ${s.missing_env_vars.map(v => `<code style="font-weight:600;">${escHtml(v)}</code>`).join(', ')}
    </div>
  </div>`;
}

function renderCredentialFields(skill: EngineSkillStatus): string {
  if (skill.required_credentials.length === 0) return '';

  const rows = skill.required_credentials.map(cred => {
    const isSet = skill.configured_credentials.includes(cred.key);
    const reqBadge = cred.required ? '<span style="color:var(--text-danger);font-size:11px;">*</span>' : '';

    return `
    <div class="skill-cred-row" style="display:flex; gap:8px; align-items:center; margin-bottom:6px;">
      <div style="min-width:140px;">
        <label style="font-size:11px; color:var(--text-secondary);">${escHtml(cred.label)} ${reqBadge}</label>
        <div style="font-size:10px; color:var(--text-muted);">${escHtml(cred.description)}</div>
      </div>
      <div style="flex:1; display:flex; gap:4px; align-items:center;">
        <input
          type="password"
          class="form-input skill-cred-input"
          data-skill="${escHtml(skill.id)}"
          data-key="${escHtml(cred.key)}"
          placeholder="${isSet ? '••••••••' : escHtml(cred.placeholder)}"
          style="font-size:11px; flex:1;"
        />
        <button class="btn btn-ghost btn-sm skill-cred-save" data-skill="${escHtml(skill.id)}" data-key="${escHtml(cred.key)}" style="font-size:10px; padding:3px 7px;">
          ${isSet ? 'Update' : 'Set'}
        </button>
        ${isSet ? `<button class="btn btn-ghost btn-sm skill-cred-delete" data-skill="${escHtml(skill.id)}" data-key="${escHtml(cred.key)}" style="font-size:10px; padding:3px 7px; color:var(--text-danger);">✕</button>` : ''}
      </div>
      ${isSet ? '<span style="color:var(--text-success); font-size:11px;">✓</span>' : '<span style="color:var(--text-muted); font-size:11px;">—</span>'}
    </div>`;
  }).join('');

  return `<div style="border-top:1px solid var(--border-color); padding-top:8px; margin-top:6px;">
    <div style="font-size:11px; font-weight:600; color:var(--text-secondary); margin-bottom:6px;">🔑 Credentials</div>
    ${rows}
  </div>`;
}

function renderAdvancedSection(s: EngineSkillStatus): string {
  const hasCustom = s.custom_instructions.length > 0;
  const currentText = hasCustom ? s.custom_instructions : s.default_instructions;

  if (!s.default_instructions && !hasCustom) return '';

  return `<div style="border-top:1px solid var(--border-color); padding-top:8px; margin-top:6px;">
    <details class="skill-advanced-toggle" data-skill="${escHtml(s.id)}">
      <summary style="font-size:11px; font-weight:600; color:var(--text-secondary); cursor:pointer; user-select:none; margin-bottom:6px;">
        ⚙️ Advanced — Agent Instructions
        ${hasCustom ? '<span style="color:var(--accent-color); font-size:10px; margin-left:6px;">customized</span>' : ''}
      </summary>
      <p style="font-size:10px; color:var(--text-muted); margin:0 0 6px 0;">
        These instructions are injected into the agent's system prompt when this skill is enabled. Edit to customize how the agent uses this skill.
      </p>
      <textarea
        class="form-input skill-instructions-editor"
        data-skill="${escHtml(s.id)}"
        style="width:100%; min-height:120px; font-size:11px; font-family:monospace; resize:vertical; line-height:1.5; background:var(--bg-tertiary); border:1px solid var(--border-color); border-radius:6px; padding:8px;"
        spellcheck="false"
      >${escHtml(currentText)}</textarea>
      <div style="display:flex; gap:6px; margin-top:6px;">
        <button class="btn btn-sm btn-primary skill-instructions-save" data-skill="${escHtml(s.id)}" style="font-size:11px; padding:4px 12px;">
          Save Instructions
        </button>
        ${hasCustom ? `<button class="btn btn-sm btn-ghost skill-instructions-reset" data-skill="${escHtml(s.id)}" style="font-size:11px; padding:4px 12px; color:var(--text-muted);">
          Reset to Default
        </button>` : ''}
      </div>
    </details>
  </div>`;
}

function bindFilterEvents(skills: EngineSkillStatus[]): void {
  document.querySelectorAll('.skills-filter-btn').forEach(el => {
    el.addEventListener('click', () => {
      const btn = el as HTMLButtonElement;
      _currentFilter = btn.dataset.filter || 'all';
      const list = $('skills-vault-list');
      if (list) {
        list.innerHTML = renderSkillsPage(skills);
        bindFilterEvents(skills);
        bindSkillEvents(skills);
      }
    });
  });
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

  // Save custom instructions
  document.querySelectorAll('.skill-instructions-save').forEach(el => {
    el.addEventListener('click', async () => {
      const btn = el as HTMLButtonElement;
      const skillId = btn.dataset.skill!;
      const textarea = document.querySelector(`.skill-instructions-editor[data-skill="${skillId}"]`) as HTMLTextAreaElement;
      const value = textarea?.value ?? '';

      try {
        await pawEngine.skillSetInstructions(skillId, value);
        showVaultToast(`Instructions saved for ${skillId}`, 'success');
        await loadSkillsSettings();
      } catch (err) {
        showVaultToast(`Failed: ${err}`, 'error');
      }
    });
  });

  // Reset instructions to default
  document.querySelectorAll('.skill-instructions-reset').forEach(el => {
    el.addEventListener('click', async () => {
      const btn = el as HTMLButtonElement;
      const skillId = btn.dataset.skill!;

      if (!confirm('Reset to default instructions? Your customizations will be lost.')) return;

      try {
        await pawEngine.skillSetInstructions(skillId, '');
        showVaultToast(`Instructions reset for ${skillId}`, 'success');
        await loadSkillsSettings();
      } catch (err) {
        showVaultToast(`Failed: ${err}`, 'error');
      }
    });
  });
}
