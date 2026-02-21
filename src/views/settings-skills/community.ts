// Settings Skills — Community (community skills browser, search, browse, install)

import { pawEngine, type CommunitySkill, type DiscoveredSkill } from '../../engine';
import { $, escHtml, confirmModal } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { POPULAR_REPOS, POPULAR_TAGS, msIcon, formatInstalls } from './atoms';

// ── State ref ──────────────────────────────────────────────────────────────

let _reloadFn: (() => Promise<void>) | null = null;

export function setCommunityReload(fn: () => Promise<void>): void {
  _reloadFn = fn;
}

// ── Community section renderer ─────────────────────────────────────────────

export function renderCommunitySection(installed: CommunitySkill[]): string {
  const installedCards =
    installed.length > 0 ? installed.map((s) => renderCommunityCard(s)).join('') : '';

  const tagButtons = POPULAR_TAGS.map(
    (t) =>
      `<button class="btn btn-ghost btn-sm community-search-tag" data-query="${escHtml(t)}" style="border-radius:20px;padding:4px 14px;font-size:12px;border:1px solid var(--border-subtle)">${escHtml(t)}</button>`,
  ).join('');

  const repoButtons = POPULAR_REPOS.map(
    (r) =>
      `<button class="btn btn-ghost btn-sm community-quick-browse" data-source="${escHtml(r.source)}" style="border-radius:20px;padding:4px 14px;font-size:12px;border:1px solid var(--accent);color:var(--accent)">${escHtml(r.label)}</button>`,
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

  ${
    installed.length > 0
      ? `
  <div class="skill-category-group" style="margin-bottom:24px">
    <h3 class="skill-category-title" style="display:flex;align-items:center;gap:8px">
      ${msIcon('download')} Installed Community Skills
      <span style="font-size:12px;font-weight:400;color:var(--text-muted)">(${installed.length})</span>
    </h3>
    ${installedCards}
  </div>`
      : ''
  }`;
}

// ── Community card renderers ───────────────────────────────────────────────

function renderCommunityCard(s: CommunitySkill): string {
  const preview =
    s.instructions.length > 200 ? `${s.instructions.substring(0, 200)}...` : s.instructions;

  const agentLabel =
    !s.agent_ids || s.agent_ids.length === 0
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
      <span class="skill-badge">
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

function renderDiscoveredCard(skill: DiscoveredSkill): string {
  const installsBadge =
    skill.installs > 0
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
        ${
          skill.installed
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

// ── Browse & Search ────────────────────────────────────────────────────────

async function browseRepo(source: string): Promise<void> {
  const results = $('community-browse-results');
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

    const notInstalled = skills.filter((s) => !s.installed).length;
    const header = `<div style="display:flex;justify-content:space-between;align-items:center;padding:0 0 8px">
      <span style="font-weight:600;font-size:13px">${skills.length} skills found in ${escHtml(source)}</span>
      ${
        notInstalled > 0
          ? `<button class="btn btn-primary btn-sm" id="community-install-all" data-source="${escHtml(source)}">
            ${msIcon('download')} Install All (${notInstalled})
          </button>`
          : ''
      }
    </div>`;

    results.innerHTML = header + skills.map((s) => renderDiscoveredCard(s)).join('');
    wireInstallButtons(results, skills);
  } catch (err) {
    results.innerHTML = `<p style="color:var(--accent-danger);padding:12px">${msIcon('error')} ${escHtml(String(err))}</p>`;
  }
}

async function searchSkills(query: string): Promise<void> {
  const results = $('community-search-results');
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

    results.innerHTML = header + skills.map((s) => renderDiscoveredCard(s)).join('');
    wireInstallButtons(results, skills);
  } catch (err) {
    results.innerHTML = `<div style="padding:16px">
      <p style="color:var(--accent-danger);margin:0 0 6px">${msIcon('error')} ${escHtml(String(err))}</p>
      <p style="color:var(--text-muted);font-size:12px;margin:0">Try different keywords or browse a specific repo below.</p>
    </div>`;
  }
}

// ── Install button wiring ──────────────────────────────────────────────────

function wireInstallButtons(container: HTMLElement, skills: DiscoveredSkill[]): void {
  const reload = () => (_reloadFn ? _reloadFn() : Promise.resolve());

  container.querySelectorAll('.community-install-btn').forEach((el) => {
    el.addEventListener('click', async () => {
      const btn = el as HTMLButtonElement;
      const src = btn.dataset.source!;
      const path = btn.dataset.path!;
      const name = btn.dataset.name!;

      btn.disabled = true;
      btn.innerHTML = `<span class="wa-spinner" style="width:12px;height:12px"></span> Installing...`;

      try {
        await pawEngine.communitySkillInstall(src, path);
        showToast(`${name} installed and enabled!`, 'success');
        await reload();
      } catch (err) {
        showToast(`Install failed: ${err}`, 'error');
        btn.disabled = false;
        btn.innerHTML = `${msIcon('download')} Install`;
      }
    });
  });

  // Install All button
  container.querySelector('#community-install-all')?.addEventListener('click', async () => {
    const allBtn = $('community-install-all') as HTMLButtonElement;
    if (allBtn) {
      allBtn.disabled = true;
      allBtn.textContent = 'Installing...';
    }

    let installed = 0;
    for (const s of skills.filter((sk) => !sk.installed)) {
      try {
        await pawEngine.communitySkillInstall(s.source, s.path);
        installed++;
      } catch (err) {
        console.warn(`Failed to install ${s.name}:`, err);
      }
    }
    showToast(`${installed} skills installed!`, 'success');
    await reload();
  });
}

// ── Community event binding ────────────────────────────────────────────────

export function bindCommunityEvents(): void {
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
  document.querySelectorAll('.community-search-tag').forEach((el) => {
    el.addEventListener('click', () => {
      const query = (el as HTMLElement).dataset.query!;
      if (searchInput) searchInput.value = query;
      searchSkills(query);
    });
  });

  // Repo browse
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
  document.querySelectorAll('.community-quick-browse').forEach((el) => {
    el.addEventListener('click', () => {
      const source = (el as HTMLElement).dataset.source!;
      const input = $('community-skill-source') as HTMLInputElement | null;
      if (input) input.value = source;
      browseRepo(source);
    });
  });

  // Enable/disable toggles for installed community skills
  document.querySelectorAll('.community-enabled-toggle').forEach((el) => {
    el.addEventListener('change', async (e) => {
      const input = e.target as HTMLInputElement;
      const skillId = input.dataset.skill!;
      try {
        await pawEngine.communitySkillSetEnabled(skillId, input.checked);
        showToast(
          `${skillId.split('/').pop()} ${input.checked ? 'enabled' : 'disabled'}`,
          'success',
        );
        if (_reloadFn) await _reloadFn();
      } catch (err) {
        showToast(`Failed: ${err}`, 'error');
        input.checked = !input.checked;
      }
    });
  });

  // Remove buttons
  document.querySelectorAll('.community-remove-btn').forEach((el) => {
    el.addEventListener('click', async () => {
      const btn = el as HTMLButtonElement;
      const skillId = btn.dataset.skill!;
      const name = skillId.split('/').pop() || skillId;

      if (!(await confirmModal(`Remove "${name}"? You can reinstall it later.`))) return;

      try {
        await pawEngine.communitySkillRemove(skillId);
        showToast(`${name} removed`, 'success');
        if (_reloadFn) await _reloadFn();
      } catch (err) {
        showToast(`Failed: ${err}`, 'error');
      }
    });
  });
}
