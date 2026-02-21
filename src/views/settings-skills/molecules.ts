// Settings Skills — Molecules (DOM rendering, event binding for built-in skills)

import { pawEngine, type EngineSkillStatus } from '../../engine';
import { escHtml, confirmModal } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { CATEGORY_META, msIcon, skillIcon } from './atoms';

// ── State ref (set by index.ts) ────────────────────────────────────────────

let _currentFilter = 'all';
let _reloadFn: (() => Promise<void>) | null = null;

export function setMoleculesState(opts: {
  currentFilter: string;
  reloadFn: () => Promise<void>;
}): void {
  _currentFilter = opts.currentFilter;
  _reloadFn = opts.reloadFn;
}

export function getCurrentFilter(): string {
  return _currentFilter;
}

// ── Main page renderer ─────────────────────────────────────────────────────

export function renderSkillsPage(skills: EngineSkillStatus[]): string {
  if (skills.length === 0) return '<p style="color:var(--text-muted)">No skills available.</p>';

  const enabledCount = skills.filter((s) => s.enabled).length;
  const readyCount = skills.filter((s) => s.is_ready).length;

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
  const categories = [...new Set(skills.map((s) => s.category))];
  categories.sort((a, b) => (CATEGORY_META[a]?.order ?? 99) - (CATEGORY_META[b]?.order ?? 99));

  const tabs = `<div class="skills-filter-tabs">
    <button class="btn btn-sm skills-filter-btn ${_currentFilter === 'all' ? 'btn-primary' : 'btn-ghost'}" data-filter="all">All</button>
    <button class="btn btn-sm skills-filter-btn ${_currentFilter === 'enabled' ? 'btn-primary' : 'btn-ghost'}" data-filter="enabled">${msIcon('bolt')} Enabled</button>
    <span style="display:inline-block;width:1px;height:20px;background:var(--border-subtle);margin:0 6px;vertical-align:middle"></span>
    <button class="btn btn-sm skills-filter-btn ${_currentFilter === 'tier:skill' ? 'btn-primary' : 'btn-ghost'}" data-filter="tier:skill">${msIcon('description')} Skills (${skills.filter((s) => s.tier === 'skill').length})</button>
    <button class="btn btn-sm skills-filter-btn ${_currentFilter === 'tier:integration' ? 'btn-primary' : 'btn-ghost'}" data-filter="tier:integration">${msIcon('key')} Integrations (${skills.filter((s) => s.tier === 'integration').length})</button>
    <span style="display:inline-block;width:1px;height:20px;background:var(--border-subtle);margin:0 6px;vertical-align:middle"></span>
    ${categories
      .map((c) => {
        const meta = CATEGORY_META[c] || { label: c, icon: 'extension', order: 99 };
        const count = skills.filter((s) => s.category === c).length;
        return `<button class="btn btn-sm skills-filter-btn ${_currentFilter === c ? 'btn-primary' : 'btn-ghost'}" data-filter="${escHtml(c)}">${msIcon(meta.icon)} ${escHtml(meta.label)} (${count})</button>`;
      })
      .join('')}
  </div>`;

  // Render filtered skills
  const filtered =
    _currentFilter === 'all'
      ? skills
      : _currentFilter === 'enabled'
        ? skills.filter((s) => s.enabled)
        : _currentFilter.startsWith('tier:')
          ? skills.filter((s) => s.tier === _currentFilter.slice(5))
          : skills.filter((s) => s.category === _currentFilter);

  // Group by category
  const grouped: Record<string, EngineSkillStatus[]> = {};
  for (const s of filtered) {
    if (!grouped[s.category]) grouped[s.category] = [];
    grouped[s.category].push(s);
  }

  const sortedCats = Object.keys(grouped).sort(
    (a, b) => (CATEGORY_META[a]?.order ?? 99) - (CATEGORY_META[b]?.order ?? 99),
  );

  const sections = sortedCats
    .map((cat) => {
      const meta = CATEGORY_META[cat] || { label: cat, icon: 'extension', order: 99 };
      const cards = grouped[cat].map((s) => renderSkillCard(s)).join('');
      return `
      <div class="skill-category-group">
        <h3 class="skill-category-title">
          ${msIcon(meta.icon)} ${escHtml(meta.label)}
        </h3>
        ${cards}
      </div>`;
    })
    .join('');

  return summary + tabs + sections;
}

// ── Tier badge ─────────────────────────────────────────────────────────────

const TIER_CONFIG: Record<string, { label: string; icon: string; color: string }> = {
  skill: { label: 'Skill', icon: 'description', color: 'var(--text-muted)' },
  integration: { label: 'Integration', icon: 'key', color: 'var(--accent)' },
  extension: { label: 'Extension', icon: 'widgets', color: '#c084fc' },
};

function tierBadge(tier: string): string {
  const cfg = TIER_CONFIG[tier] || TIER_CONFIG.skill;
  return `<span class="skill-badge" style="border-color:${cfg.color};color:${cfg.color}">${msIcon(cfg.icon)} ${cfg.label}</span>`;
}

// ── Skill card ─────────────────────────────────────────────────────────────

function renderSkillCard(s: EngineSkillStatus): string {
  let statusIcon: string;
  let statusText: string;
  let statusClass: string;
  if (s.is_ready) {
    statusIcon = 'check_circle';
    statusText = 'Ready';
    statusClass = 'status-ready';
  } else if (s.enabled && s.missing_binaries.length > 0) {
    statusIcon = 'error';
    statusText = 'Missing binaries';
    statusClass = 'status-error';
  } else if (s.enabled && s.missing_credentials.length > 0) {
    statusIcon = 'warning';
    statusText = 'Missing credentials';
    statusClass = 'status-warn';
  } else if (s.enabled && s.missing_env_vars.length > 0) {
    statusIcon = 'warning';
    statusText = 'Missing env vars';
    statusClass = 'status-warn';
  } else if (s.enabled) {
    statusIcon = 'warning';
    statusText = 'Setup incomplete';
    statusClass = 'status-warn';
  } else {
    statusIcon = 'radio_button_unchecked';
    statusText = 'Disabled';
    statusClass = 'status-off';
  }

  const hasCreds = s.required_credentials.length > 0;
  const hasTools = s.tool_names.length > 0;

  const badges: string[] = [tierBadge(s.tier)];
  if (hasTools) badges.push(`<span class="skill-badge">${msIcon('build')} Tools</span>`);
  if (hasCreds) badges.push(`<span class="skill-badge">${msIcon('vpn_key')} Vault</span>`);

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
    ${
      hasTools
        ? `<div class="skill-tools-row">
      Tools: ${s.tool_names.map((t) => `<code class="skill-tool-tag">${escHtml(t)}</code>`).join(' ')}
    </div>`
        : ''
    }
    ${renderBinaryStatus(s)}
    ${renderEnvVarStatus(s)}
    ${renderCredentialFields(s)}
    ${renderAdvancedSection(s)}
  </div>`;
}

// ── Status badges ──────────────────────────────────────────────────────────

function renderBinaryStatus(s: EngineSkillStatus): string {
  if (s.missing_binaries.length === 0) return '';

  return `<div class="skill-status-block skill-status-danger">
    <div class="skill-status-msg">
      ${msIcon('error')} Missing binaries: ${s.missing_binaries.map((b) => `<code class="skill-code-tag">${escHtml(b)}</code>`).join(', ')}
    </div>
    ${
      s.install_hint
        ? `<div class="skill-status-hint">
      Install: <code class="skill-code-tag skill-code-copy">${escHtml(s.install_hint)}</code>
    </div>`
        : ''
    }
  </div>`;
}

function renderEnvVarStatus(s: EngineSkillStatus): string {
  if (s.missing_env_vars.length === 0) return '';

  return `<div class="skill-status-block skill-status-warn">
    <div class="skill-status-msg">
      ${msIcon('warning')} Missing environment variables: ${s.missing_env_vars.map((v) => `<code class="skill-code-tag">${escHtml(v)}</code>`).join(', ')}
    </div>
  </div>`;
}

// ── Credential fields ──────────────────────────────────────────────────────

function renderCredentialFields(skill: EngineSkillStatus): string {
  if (skill.required_credentials.length === 0) return '';

  const rows = skill.required_credentials
    .map((cred) => {
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
    })
    .join('');

  return `<div class="skill-cred-section">
    <div class="skill-section-title">${msIcon('key')} Credentials</div>
    ${rows}
  </div>`;
}

// ── Advanced section ───────────────────────────────────────────────────────

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
        ${
          hasCustom
            ? `<button class="btn btn-sm btn-ghost skill-instructions-reset" data-skill="${escHtml(s.id)}">
          ${msIcon('restart_alt')} Reset to Default
        </button>`
            : ''
        }
      </div>
    </details>
  </div>`;
}

// ── Filter event binding ───────────────────────────────────────────────────

export function bindFilterEvents(
  _skills: EngineSkillStatus[],
  setFilter: (f: string) => void,
): void {
  document.querySelectorAll('.skills-filter-btn').forEach((el) => {
    el.addEventListener('click', () => {
      const btn = el as HTMLButtonElement;
      const filter = btn.dataset.filter || 'all';
      setFilter(filter);
      _currentFilter = filter;
      // Delegate to reload which re-renders everything
      if (_reloadFn) _reloadFn();
    });
  });
}

// ── Skill event binding ────────────────────────────────────────────────────

export function bindSkillEvents(): void {
  const reload = () => (_reloadFn ? _reloadFn() : Promise.resolve());

  // Enable/disable toggles
  document.querySelectorAll('.skill-enabled-toggle').forEach((el) => {
    el.addEventListener('change', async (e) => {
      const input = e.target as HTMLInputElement;
      const skillId = input.dataset.skill!;
      try {
        await pawEngine.skillSetEnabled(skillId, input.checked);
        showToast(`${skillId} ${input.checked ? 'enabled' : 'disabled'}`, 'success');
        await reload();
      } catch (err) {
        showToast(`Failed: ${err}`, 'error');
        input.checked = !input.checked;
      }
    });
  });

  // Save credential buttons
  document.querySelectorAll('.skill-cred-save').forEach((el) => {
    el.addEventListener('click', async () => {
      const btn = el as HTMLButtonElement;
      const skillId = btn.dataset.skill!;
      const key = btn.dataset.key!;
      const input = document.querySelector(
        `.skill-cred-input[data-skill="${skillId}"][data-key="${key}"]`,
      ) as HTMLInputElement;
      const value = input?.value?.trim();
      if (!value) {
        showToast('Enter a value first', 'info');
        return;
      }

      try {
        await pawEngine.skillSetCredential(skillId, key, value);
        showToast(`${key} saved securely`, 'success');
        input.value = '';
        await reload();
      } catch (err) {
        showToast(`Failed: ${err}`, 'error');
      }
    });
  });

  // Delete credential buttons
  document.querySelectorAll('.skill-cred-delete').forEach((el) => {
    el.addEventListener('click', async () => {
      const btn = el as HTMLButtonElement;
      const skillId = btn.dataset.skill!;
      const key = btn.dataset.key!;

      try {
        await pawEngine.skillDeleteCredential(skillId, key);
        showToast(`${key} removed`, 'success');
        await reload();
      } catch (err) {
        showToast(`Failed: ${err}`, 'error');
      }
    });
  });

  // Revoke all buttons
  document.querySelectorAll('.skill-revoke-btn').forEach((el) => {
    el.addEventListener('click', async () => {
      const btn = el as HTMLButtonElement;
      const skillId = btn.dataset.skill!;

      if (!(await confirmModal(`Revoke ALL credentials for ${skillId}? This can't be undone.`)))
        return;

      try {
        await pawEngine.skillRevokeAll(skillId);
        showToast(`All ${skillId} credentials revoked`, 'success');
        await reload();
      } catch (err) {
        showToast(`Failed: ${err}`, 'error');
      }
    });
  });

  // Save custom instructions
  document.querySelectorAll('.skill-instructions-save').forEach((el) => {
    el.addEventListener('click', async () => {
      const btn = el as HTMLButtonElement;
      const skillId = btn.dataset.skill!;
      const textarea = document.querySelector(
        `.skill-instructions-editor[data-skill="${skillId}"]`,
      ) as HTMLTextAreaElement;
      const value = textarea?.value ?? '';

      try {
        await pawEngine.skillSetInstructions(skillId, value);
        showToast(`Instructions saved for ${skillId}`, 'success');
        await reload();
      } catch (err) {
        showToast(`Failed: ${err}`, 'error');
      }
    });
  });

  // Reset instructions to default
  document.querySelectorAll('.skill-instructions-reset').forEach((el) => {
    el.addEventListener('click', async () => {
      const btn = el as HTMLButtonElement;
      const skillId = btn.dataset.skill!;

      if (!(await confirmModal('Reset to default instructions? Your customizations will be lost.')))
        return;

      try {
        await pawEngine.skillSetInstructions(skillId, '');
        showToast(`Instructions reset for ${skillId}`, 'success');
        await reload();
      } catch (err) {
        showToast(`Failed: ${err}`, 'error');
      }
    });
  });
}
