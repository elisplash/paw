// Pawz Skills — Full Skill Vault and Instruction Browser
// Shows all skills grouped by category with binary/env status, credentials, and install hints.

import { pawEngine, type EngineSkillStatus, type CommunitySkill, type DiscoveredSkill } from '../engine';
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
  Vault:         { label: 'Vault (Credentials)', icon: 'enhanced_encryption', order: 0 },
  Communication: { label: 'Communication',       icon: 'forum',               order: 1 },
  Productivity:  { label: 'Productivity',         icon: 'task_alt',            order: 2 },
  Api:           { label: 'API Integrations',     icon: 'api',                 order: 3 },
  Development:   { label: 'Development',          icon: 'code',                order: 4 },
  Media:         { label: 'Media',                icon: 'movie',               order: 5 },
  SmartHome:     { label: 'Smart Home & IoT',     icon: 'home',                order: 6 },
  Cli:           { label: 'CLI Tools',            icon: 'terminal',            order: 7 },
  System:        { label: 'System',               icon: 'settings',            order: 8 },
};

/** Map skill icon names (emoji fallback from backend) to Material Symbols */
const SKILL_ICON_MAP: Record<string, string> = {
  '📧': 'mail', '✉️': 'mail', '💬': 'chat', '🔔': 'notifications',
  '📋': 'assignment', '📝': 'edit_note', '📅': 'calendar_today',
  '🔌': 'power', '🌐': 'language', '🔗': 'link',
  '🛠️': 'build', '💻': 'code', '🔧': 'build',
  '🎬': 'movie', '🎵': 'music_note', '📸': 'photo_camera', '🎙️': 'mic',
  '🏠': 'home', '💡': 'lightbulb',
  '⌨️': 'terminal', '🖥️': 'computer', '📦': 'inventory_2',
  '🔐': 'lock', '🔑': 'key', '🐙': 'code', '📊': 'analytics',
  '🤖': 'smart_toy', '⚡': 'bolt', '🔍': 'search',
};

function msIcon(name: string, size: string = 'ms-sm'): string {
  return `<span class="ms ${size}">${name}</span>`;
}

function skillIcon(raw: string): string {
  const mapped = SKILL_ICON_MAP[raw];
  return mapped ? msIcon(mapped) : msIcon('extension');
}

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
    const communitySkills = await pawEngine.communitySkillsList();
    if (loading) loading.style.display = 'none';
    if (list) list.innerHTML = renderCommunitySection(communitySkills) + renderSkillsPage(skills);
    bindFilterEvents(skills);
    bindSkillEvents(skills);
    bindCommunityEvents(communitySkills);
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
  const summary = `<div class="skills-summary-bar">
    <span class="skills-summary-count">${skills.length} Skills</span>
    <span class="skills-summary-status">
      ${msIcon('check_circle')} ${readyCount} ready
      <span class="skills-summary-sep">·</span>
      ${msIcon('bolt')} ${enabledCount} enabled
    </span>
  </div>`;

  // Category filter tabs
  const categories = [...new Set(skills.map(s => s.category))];
  categories.sort((a, b) => (CATEGORY_META[a]?.order ?? 99) - (CATEGORY_META[b]?.order ?? 99));

  const tabs = `<div class="skills-filter-tabs">
    <button class="btn btn-sm skills-filter-btn ${_currentFilter === 'all' ? 'btn-primary' : 'btn-ghost'}" data-filter="all">All</button>
    <button class="btn btn-sm skills-filter-btn ${_currentFilter === 'enabled' ? 'btn-primary' : 'btn-ghost'}" data-filter="enabled">${msIcon('bolt')} Enabled</button>
    ${categories.map(c => {
      const meta = CATEGORY_META[c] || { label: c, icon: 'extension', order: 99 };
      const count = skills.filter(s => s.category === c).length;
      return `<button class="btn btn-sm skills-filter-btn ${_currentFilter === c ? 'btn-primary' : 'btn-ghost'}" data-filter="${escHtml(c)}">${msIcon(meta.icon)} ${escHtml(meta.label)} (${count})</button>`;
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
    const meta = CATEGORY_META[cat] || { label: cat, icon: 'extension', order: 99 };
    const cards = grouped[cat].map(s => renderSkillCard(s)).join('');
    return `
      <div class="skill-category-group">
        <h3 class="skill-category-title">
          ${msIcon(meta.icon)} ${escHtml(meta.label)}
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
  let statusClass: string;
  if (s.is_ready) {
    statusIcon = 'check_circle'; statusText = 'Ready'; statusClass = 'status-ready';
  } else if (s.enabled && s.missing_binaries.length > 0) {
    statusIcon = 'error'; statusText = 'Missing binaries'; statusClass = 'status-error';
  } else if (s.enabled && s.missing_credentials.length > 0) {
    statusIcon = 'warning'; statusText = 'Missing credentials'; statusClass = 'status-warn';
  } else if (s.enabled && s.missing_env_vars.length > 0) {
    statusIcon = 'warning'; statusText = 'Missing env vars'; statusClass = 'status-warn';
  } else if (s.enabled) {
    statusIcon = 'warning'; statusText = 'Setup incomplete'; statusClass = 'status-warn';
  } else {
    statusIcon = 'radio_button_unchecked'; statusText = 'Disabled'; statusClass = 'status-off';
  }

  const hasCreds = s.required_credentials.length > 0;
  const hasTools = s.tool_names.length > 0;

  // Badges
  const badges: string[] = [];
  if (s.has_instructions) badges.push(`<span class="skill-badge">${msIcon('description')} Instruction</span>`);
  if (hasTools) badges.push(`<span class="skill-badge">${msIcon('build')} Tools</span>`);
  if (hasCreds) badges.push(`<span class="skill-badge">${msIcon('key')} Vault</span>`);

  return `
  <div class="skill-vault-card${s.enabled ? ' skill-enabled' : ''}" data-skill-id="${escHtml(s.id)}">
    <div class="skill-card-header">
      <div class="skill-card-identity">
        <span class="skill-card-icon">${skillIcon(s.icon)}</span>
        <div>
          <strong class="skill-card-name">${escHtml(s.name)}</strong>
          <span class="skill-status ${statusClass}">${msIcon(statusIcon)} ${statusText}</span>
        </div>
      </div>
      <div class="skill-card-actions">
        <label class="skill-toggle-label">
          <input type="checkbox" class="skill-enabled-toggle" data-skill="${escHtml(s.id)}" ${s.enabled ? 'checked' : ''} />
          Enable
        </label>
        ${hasCreds ? `<button class="btn btn-ghost btn-sm skill-revoke-btn" data-skill="${escHtml(s.id)}" title="Revoke all credentials">Revoke</button>` : ''}
      </div>
    </div>
    <p class="skill-card-desc">${escHtml(s.description)}</p>
    <div class="skill-badges-row">${badges.join('')}</div>
    ${hasTools ? `<div class="skill-tools-row">
      Tools: ${s.tool_names.map(t => `<code class="skill-tool-tag">${escHtml(t)}</code>`).join(' ')}
    </div>` : ''}
    ${renderBinaryStatus(s)}
    ${renderEnvVarStatus(s)}
    ${renderCredentialFields(s)}
    ${renderAdvancedSection(s)}
  </div>`;
}

function renderBinaryStatus(s: EngineSkillStatus): string {
  if (s.missing_binaries.length === 0) return '';

  return `<div class="skill-status-block skill-status-danger">
    <div class="skill-status-msg">
      ${msIcon('error')} Missing binaries: ${s.missing_binaries.map(b => `<code class="skill-code-tag">${escHtml(b)}</code>`).join(', ')}
    </div>
    ${s.install_hint ? `<div class="skill-status-hint">
      Install: <code class="skill-code-tag skill-code-copy">${escHtml(s.install_hint)}</code>
    </div>` : ''}
  </div>`;
}

function renderEnvVarStatus(s: EngineSkillStatus): string {
  if (s.missing_env_vars.length === 0) return '';

  return `<div class="skill-status-block skill-status-warn">
    <div class="skill-status-msg">
      ${msIcon('warning')} Missing environment variables: ${s.missing_env_vars.map(v => `<code class="skill-code-tag">${escHtml(v)}</code>`).join(', ')}
    </div>
  </div>`;
}

function renderCredentialFields(skill: EngineSkillStatus): string {
  if (skill.required_credentials.length === 0) return '';

  const rows = skill.required_credentials.map(cred => {
    const isSet = skill.configured_credentials.includes(cred.key);
    const reqBadge = cred.required ? '<span class="skill-required">*</span>' : '';

    return `
    <div class="skill-cred-row">
      <div class="skill-cred-label">
        <label class="skill-cred-name">${escHtml(cred.label)} ${reqBadge}</label>
        <span class="skill-cred-desc">${escHtml(cred.description)}</span>
      </div>
      <div class="skill-cred-field">
        <input
          type="password"
          class="form-input skill-cred-input"
          data-skill="${escHtml(skill.id)}"
          data-key="${escHtml(cred.key)}"
          placeholder="${isSet ? '••••••••' : escHtml(cred.placeholder)}"
        />
        <button class="btn btn-ghost btn-sm skill-cred-save" data-skill="${escHtml(skill.id)}" data-key="${escHtml(cred.key)}">
          ${isSet ? 'Update' : 'Set'}
        </button>
        ${isSet ? `<button class="btn btn-ghost btn-sm skill-cred-delete" data-skill="${escHtml(skill.id)}" data-key="${escHtml(cred.key)}">${msIcon('close')}</button>` : ''}
      </div>
      <span class="skill-cred-status">${isSet ? msIcon('check_circle') : msIcon('remove')}</span>
    </div>`;
  }).join('');

  return `<div class="skill-cred-section">
    <div class="skill-section-title">${msIcon('key')} Credentials</div>
    ${rows}
  </div>`;
}

function renderAdvancedSection(s: EngineSkillStatus): string {
  const hasCustom = s.custom_instructions.length > 0;
  const currentText = hasCustom ? s.custom_instructions : s.default_instructions;

  if (!s.default_instructions && !hasCustom) return '';

  return `<div class="skill-advanced-section">
    <details class="skill-advanced-toggle" data-skill="${escHtml(s.id)}">
      <summary class="skill-advanced-summary">
        ${msIcon('tune')} Advanced — Agent Instructions
        ${hasCustom ? '<span class="skill-customized-badge">customized</span>' : ''}
      </summary>
      <p class="skill-advanced-hint">
        These instructions are injected into the agent's system prompt when this skill is enabled. Edit to customize how the agent uses this skill.
      </p>
      <textarea
        class="form-input skill-instructions-editor"
        data-skill="${escHtml(s.id)}"
        spellcheck="false"
      >${escHtml(currentText)}</textarea>
      <div class="skill-advanced-actions">
        <button class="btn btn-sm btn-primary skill-instructions-save" data-skill="${escHtml(s.id)}">
          ${msIcon('save')} Save Instructions
        </button>
        ${hasCustom ? `<button class="btn btn-sm btn-ghost skill-instructions-reset" data-skill="${escHtml(s.id)}">
          ${msIcon('restart_alt')} Reset to Default
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

// ════════════════════════════════════════════════════════════════════════
// ██  Community Skills (skills.sh ecosystem)  ███████████████████████████
// ════════════════════════════════════════════════════════════════════════

const POPULAR_REPOS = [
  { source: 'vercel-labs/agent-skills', label: 'Vercel Agent Skills' },
  { source: 'anthropics/skills', label: 'Anthropic Skills' },
];

const POPULAR_TAGS = [
  'marketing', 'trading', 'supabase', 'writing', 'coding',
  'data analysis', 'devops', 'design', 'finance', 'research',
];

function renderCommunitySection(installed: CommunitySkill[]): string {
  const installedCards = installed.length > 0
    ? installed.map(s => renderCommunityCard(s)).join('')
    : '';

  const tagButtons = POPULAR_TAGS.map(t =>
    `<button class="btn btn-ghost btn-sm community-search-tag" data-query="${escHtml(t)}" style="border-radius:20px;padding:4px 14px;font-size:12px;border:1px solid var(--border-subtle)">${escHtml(t)}</button>`
  ).join('');

  const repoButtons = POPULAR_REPOS.map(r =>
    `<button class="btn btn-ghost btn-sm community-quick-browse" data-source="${escHtml(r.source)}" style="border-radius:20px;padding:4px 14px;font-size:12px;border:1px solid var(--accent);color:var(--accent)">${escHtml(r.label)}</button>`
  ).join('');

  return `
  <div class="community-skills-hero" style="background:linear-gradient(135deg, var(--bg-surface) 0%, color-mix(in srgb, var(--accent) 10%, var(--bg-surface)) 100%);border:1px solid var(--border-subtle);border-radius:12px;padding:24px 28px;margin-bottom:24px">
    <div style="display:flex;align-items:center;gap:10px;margin-bottom:6px">
      <span style="font-size:28px">${msIcon('public', 'ms-lg')}</span>
      <h2 style="margin:0;font-size:20px;font-weight:700;letter-spacing:-0.02em">Community Skills</h2>
      <a href="https://skills.sh" target="_blank" style="font-size:11px;color:var(--accent);text-decoration:none;padding:2px 8px;border:1px solid var(--accent);border-radius:12px;margin-left:4px">skills.sh</a>
    </div>
    <p style="color:var(--text-muted);font-size:13px;margin:0 0 16px;max-width:600px">
      Search and install open-source agent skills that teach your agents new capabilities.
      Skills work across all channels — WhatsApp, Telegram, Discord, and more.
    </p>

    <div style="display:flex;gap:8px;align-items:center;margin-bottom:14px">
      <div style="flex:1;position:relative">
        <span style="position:absolute;left:10px;top:50%;transform:translateY(-50%);color:var(--text-muted);pointer-events:none">${msIcon('search')}</span>
        <input type="text" class="form-input" id="community-skill-search"
          placeholder="Search skills — try marketing, trading, supabase..."
          style="width:100%;font-size:14px;padding:10px 12px 10px 36px;border-radius:10px" />
      </div>
      <button class="btn btn-primary" id="community-skill-search-btn" style="padding:10px 20px;border-radius:10px;font-size:14px">
        Search
      </button>
    </div>

    <div style="display:flex;gap:6px;flex-wrap:wrap;margin-bottom:8px">
      ${tagButtons}
    </div>

    <div style="display:flex;gap:6px;flex-wrap:wrap;margin-top:8px;padding-top:8px;border-top:1px solid var(--border-subtle)">
      <span style="font-size:11px;color:var(--text-muted);display:flex;align-items:center;gap:4px">${msIcon('folder')} Browse repo:</span>
      ${repoButtons}
      <span style="font-size:11px;color:var(--text-muted);display:flex;align-items:center;margin-left:4px">or type</span>
      <input type="text" class="form-input" id="community-skill-source"
        placeholder="owner/repo"
        style="font-size:12px;padding:4px 10px;border-radius:8px;width:160px" />
      <button class="btn btn-ghost btn-sm" id="community-skill-browse" style="border-radius:8px;font-size:12px">
        ${msIcon('search')} Browse
      </button>
    </div>

    <div id="community-search-results" style="display:none;margin-top:16px"></div>
    <div id="community-browse-results" style="display:none;margin-top:16px"></div>
  </div>

  ${installed.length > 0 ? `
  <div class="skill-category-group" style="margin-bottom:24px">
    <h3 class="skill-category-title" style="display:flex;align-items:center;gap:8px">
      ${msIcon('download')} Installed Community Skills
      <span style="font-size:12px;font-weight:400;color:var(--text-muted)">(${installed.length})</span>
    </h3>
    ${installedCards}
  </div>` : ''}`;
}

function renderCommunityCard(s: CommunitySkill): string {
  const preview = s.instructions.length > 200
    ? s.instructions.substring(0, 200) + '...'
    : s.instructions;

  const agentLabel = (!s.agent_ids || s.agent_ids.length === 0)
    ? `${msIcon('groups')} All Agents`
    : `${msIcon('person')} ${s.agent_ids.join(', ')}`;

  return `
  <div class="skill-vault-card${s.enabled ? ' skill-enabled' : ''}" data-community-id="${escHtml(s.id)}">
    <div class="skill-card-header">
      <div class="skill-card-identity">
        <span class="skill-card-icon">${msIcon('extension')}</span>
        <div>
          <strong class="skill-card-name">${escHtml(s.name)}</strong>
          <span class="skill-status ${s.enabled ? 'status-ready' : 'status-off'}">
            ${msIcon(s.enabled ? 'check_circle' : 'radio_button_unchecked')} ${s.enabled ? 'Enabled' : 'Disabled'}
          </span>
        </div>
      </div>
      <div class="skill-card-actions">
        <label class="skill-toggle-label">
          <input type="checkbox" class="community-enabled-toggle" data-skill="${escHtml(s.id)}" ${s.enabled ? 'checked' : ''} />
          Enable
        </label>
        <button class="btn btn-ghost btn-sm community-remove-btn" data-skill="${escHtml(s.id)}" title="Remove">Remove</button>
      </div>
    </div>
    <p class="skill-card-desc">${escHtml(s.description)}</p>
    <div class="skill-badges-row">
      <span class="skill-badge">${msIcon('public')} Community</span>
      <span class="skill-badge">${msIcon('description')} Instruction</span>
      <span class="skill-badge community-agent-badge" data-skill="${escHtml(s.id)}" style="cursor:pointer;user-select:none" title="Click to change agent assignment">
        ${agentLabel}
      </span>
    </div>
    <div style="font-size:12px;color:var(--text-muted);margin-top:4px">
      Source: <code style="font-size:11px">${escHtml(s.source)}</code>
    </div>
    <details style="margin-top:8px">
      <summary style="cursor:pointer;font-size:12px;color:var(--text-muted)">
        ${msIcon('visibility')} Preview instructions
      </summary>
      <pre style="font-size:11px;background:var(--bg-surface);border-radius:6px;padding:8px;margin:6px 0 0;max-height:200px;overflow:auto;white-space:pre-wrap">${escHtml(preview)}</pre>
    </details>
  </div>`;
}

function formatInstalls(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toString();
}

function renderDiscoveredCard(skill: DiscoveredSkill): string {
  const installsBadge = skill.installs > 0
    ? `<span style="font-size:11px;color:var(--text-muted);display:flex;align-items:center;gap:3px">${msIcon('download')} ${formatInstalls(skill.installs)}</span>`
    : '';

  return `
  <div class="skill-vault-card" style="opacity:${skill.installed ? '0.6' : '1'}">
    <div class="skill-card-header">
      <div class="skill-card-identity">
        <span class="skill-card-icon">${msIcon('extension')}</span>
        <div>
          <strong class="skill-card-name">${escHtml(skill.name)}</strong>
          <span class="skill-status ${skill.installed ? 'status-ready' : 'status-off'}">
            ${skill.installed ? `${msIcon('check_circle')} Installed` : `${msIcon('cloud_download')} Available`}
          </span>
        </div>
      </div>
      <div class="skill-card-actions" style="display:flex;align-items:center;gap:10px">
        ${installsBadge}
        ${skill.installed
          ? `<span style="font-size:12px;color:var(--text-muted)">Already installed</span>`
          : `<button class="btn btn-primary btn-sm community-install-btn" data-source="${escHtml(skill.source)}" data-path="${escHtml(skill.path)}" data-name="${escHtml(skill.name)}">
              ${msIcon('download')} Install
            </button>`
        }
      </div>
    </div>
    <p class="skill-card-desc">${escHtml(skill.description || 'No description')}</p>
    <div style="font-size:11px;color:var(--text-muted);margin-top:2px">
      <a href="https://skills.sh/${escHtml(skill.id)}" target="_blank" style="color:var(--accent);text-decoration:none">
        ${msIcon('open_in_new')} View on skills.sh
      </a>
      <span style="margin-left:8px">${escHtml(skill.source)}</span>
    </div>
  </div>`;
}

function bindCommunityEvents(_installed: CommunitySkill[]): void {
  // ── Keyword search ──────────────────────────────────────────────────
  const searchInput = $('community-skill-search') as HTMLInputElement | null;

  $('community-skill-search-btn')?.addEventListener('click', () => {
    if (searchInput?.value.trim()) searchSkills(searchInput.value.trim());
  });

  searchInput?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && searchInput.value.trim()) {
      searchSkills(searchInput.value.trim());
    }
  });

  // Tag buttons
  document.querySelectorAll('.community-search-tag').forEach(el => {
    el.addEventListener('click', () => {
      const query = (el as HTMLElement).dataset.query!;
      if (searchInput) searchInput.value = query;
      searchSkills(query);
    });
  });

  // ── Repo browse ─────────────────────────────────────────────────────
  $('community-skill-browse')?.addEventListener('click', () => {
    const input = $('community-skill-source') as HTMLInputElement | null;
    if (input?.value.trim()) browseRepo(input.value.trim());
  });

  ($('community-skill-source') as HTMLInputElement)?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      const input = e.target as HTMLInputElement;
      if (input.value.trim()) browseRepo(input.value.trim());
    }
  });

  // Quick browse buttons
  document.querySelectorAll('.community-quick-browse').forEach(el => {
    el.addEventListener('click', () => {
      const source = (el as HTMLElement).dataset.source!;
      const input = $('community-skill-source') as HTMLInputElement | null;
      if (input) input.value = source;
      browseRepo(source);
    });
  });

  // ── Installed skill management ──────────────────────────────────────
  // Enable/disable toggles
  document.querySelectorAll('.community-enabled-toggle').forEach(el => {
    el.addEventListener('change', async (e) => {
      const input = e.target as HTMLInputElement;
      const skillId = input.dataset.skill!;
      try {
        await pawEngine.communitySkillSetEnabled(skillId, input.checked);
        showVaultToast(`${skillId.split('/').pop()} ${input.checked ? 'enabled' : 'disabled'}`, 'success');
        await loadSkillsSettings();
      } catch (err) {
        showVaultToast(`Failed: ${err}`, 'error');
        input.checked = !input.checked;
      }
    });
  });

  // Remove buttons
  document.querySelectorAll('.community-remove-btn').forEach(el => {
    el.addEventListener('click', async () => {
      const btn = el as HTMLButtonElement;
      const skillId = btn.dataset.skill!;
      const name = skillId.split('/').pop() || skillId;

      if (!confirm(`Remove "${name}"? You can reinstall it later.`)) return;

      try {
        await pawEngine.communitySkillRemove(skillId);
        showVaultToast(`${name} removed`, 'success');
        await loadSkillsSettings();
      } catch (err) {
        showVaultToast(`Failed: ${err}`, 'error');
      }
    });
  });

  // Agent assignment badges
  document.querySelectorAll('.community-agent-badge').forEach(el => {
    el.addEventListener('click', async () => {
      const badge = el as HTMLElement;
      const skillId = badge.dataset.skill!;
      const skill = _installed.find(s => s.id === skillId);
      if (!skill) return;

      try {
        const agents = await pawEngine.listAllAgents();
        const agentIds = agents.map(a => a.agent_id);

        const currentIds = skill.agent_ids || [];
        const isAll = currentIds.length === 0;

        // Build a simple prompt with agent options
        const choices = ['All Agents', ...agentIds];
        const currentLabel = isAll ? 'All Agents' : currentIds.join(', ');
        const choice = prompt(
          `Assign "${skill.name}" to which agents?\n\nCurrent: ${currentLabel}\nAvailable: ${choices.join(', ')}\n\nEnter comma-separated agent IDs, or "all" for all agents:`
        );

        if (choice === null) return; // cancelled

        const newIds = choice.trim().toLowerCase() === 'all' || choice.trim() === ''
          ? []
          : choice.split(',').map(s => s.trim()).filter(Boolean);

        await pawEngine.communitySkillSetAgents(skillId, newIds);
        const label = newIds.length === 0 ? 'all agents' : newIds.join(', ');
        showVaultToast(`${skill.name} → ${label}`, 'success');
        await loadSkillsSettings();
      } catch (err) {
        showVaultToast(`Failed: ${err}`, 'error');
      }
    });
  });
}

async function browseRepo(source: string): Promise<void> {
  const results = $('community-browse-results');
  // Hide search results when browsing
  const searchResults = $('community-search-results');
  if (searchResults) searchResults.style.display = 'none';

  if (!results) return;

  results.style.display = 'block';
  results.innerHTML = `<div style="display:flex;align-items:center;gap:8px;padding:12px;color:var(--text-muted)">
    <span class="wa-spinner"></span> Browsing ${escHtml(source)}...
  </div>`;

  try {
    const skills = await pawEngine.communitySkillsBrowse(source);

    if (skills.length === 0) {
      results.innerHTML = `<p style="color:var(--text-muted);padding:12px">No skills found in ${escHtml(source)}.</p>`;
      return;
    }

    const notInstalled = skills.filter(s => !s.installed).length;
    const header = `<div style="display:flex;justify-content:space-between;align-items:center;padding:0 0 8px">
      <span style="font-weight:600;font-size:13px">${skills.length} skills found in ${escHtml(source)}</span>
      ${notInstalled > 0
        ? `<button class="btn btn-primary btn-sm" id="community-install-all" data-source="${escHtml(source)}">
            ${msIcon('download')} Install All (${notInstalled})
          </button>`
        : ''}
    </div>`;

    results.innerHTML = header + skills.map(s => renderDiscoveredCard(s)).join('');
    wireInstallButtons(results, skills);
  } catch (err) {
    results.innerHTML = `<p style="color:var(--accent-danger);padding:12px">${msIcon('error')} ${escHtml(String(err))}</p>`;
  }
}

async function searchSkills(query: string): Promise<void> {
  const results = $('community-search-results');
  // Hide repo browse results when searching
  const browseResults = $('community-browse-results');
  if (browseResults) browseResults.style.display = 'none';

  if (!results) return;

  results.style.display = 'block';
  results.innerHTML = `<div style="display:flex;align-items:center;gap:8px;padding:12px;color:var(--text-muted)">
    <span class="wa-spinner"></span> Searching for "${escHtml(query)}" skills...
  </div>`;

  try {
    const skills = await pawEngine.communitySkillsSearch(query);

    if (skills.length === 0) {
      results.innerHTML = `<div style="padding:16px;text-align:center">
        <p style="color:var(--text-muted);margin:0 0 8px">${msIcon('search_off')} No skills found for "${escHtml(query)}"</p>
        <p style="color:var(--text-muted);font-size:12px;margin:0">Try different keywords or browse a specific repo below.</p>
      </div>`;
      return;
    }

    // Group results by repo for clarity
    const byRepo = new Map<string, typeof skills>();
    for (const s of skills) {
      const existing = byRepo.get(s.source) || [];
      existing.push(s);
      byRepo.set(s.source, existing);
    }

    const header = `<div style="display:flex;justify-content:space-between;align-items:center;padding:0 0 8px">
      <span style="font-weight:600;font-size:14px">${msIcon('check_circle')} ${skills.length} skills found for "${escHtml(query)}"</span>
      <span style="font-size:12px;color:var(--text-muted)">from ${byRepo.size} ${byRepo.size === 1 ? 'repo' : 'repos'}</span>
    </div>`;

    results.innerHTML = header + skills.map(s => renderDiscoveredCard(s)).join('');

    // Wire install buttons
    wireInstallButtons(results, skills);
  } catch (err) {
    results.innerHTML = `<div style="padding:16px">
      <p style="color:var(--accent-danger);margin:0 0 6px">${msIcon('error')} ${escHtml(String(err))}</p>
      <p style="color:var(--text-muted);font-size:12px;margin:0">Try different keywords or browse a specific repo below.</p>
    </div>`;
  }
}

/** Wire install buttons inside a results container. Shared by search and browse. */
function wireInstallButtons(container: HTMLElement, skills: DiscoveredSkill[]): void {
  container.querySelectorAll('.community-install-btn').forEach(el => {
    el.addEventListener('click', async () => {
      const btn = el as HTMLButtonElement;
      const src = btn.dataset.source!;
      const path = btn.dataset.path!;
      const name = btn.dataset.name!;

      btn.disabled = true;
      btn.innerHTML = `<span class="wa-spinner" style="width:12px;height:12px"></span> Installing...`;

      try {
        await pawEngine.communitySkillInstall(src, path);
        showVaultToast(`${name} installed and enabled!`, 'success');
        await loadSkillsSettings();
      } catch (err) {
        showVaultToast(`Install failed: ${err}`, 'error');
        btn.disabled = false;
        btn.innerHTML = `${msIcon('download')} Install`;
      }
    });
  });

  // Install All button (if present)
  container.querySelector('#community-install-all')?.addEventListener('click', async () => {
    const allBtn = $('community-install-all') as HTMLButtonElement;
    if (allBtn) { allBtn.disabled = true; allBtn.textContent = 'Installing...'; }

    let installed = 0;
    for (const s of skills.filter(sk => !sk.installed)) {
      try {
        await pawEngine.communitySkillInstall(s.source, s.path);
        installed++;
      } catch (err) {
        console.warn(`Failed to install ${s.name}:`, err);
      }
    }
    showVaultToast(`${installed} skills installed!`, 'success');
    await loadSkillsSettings();
  });
}
