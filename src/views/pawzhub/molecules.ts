// PawzHub Marketplace — Molecules (DOM rendering + event binding)
// Native in-app marketplace replacing the old iframe approach.
// Uses the unified skill card component for consistent rendering.

import {
  pawEngine,
  type PawzHubEntry,
  type CommunitySkill,
  type DiscoveredSkill,
} from '../../engine';
import { $, escHtml, confirmModal } from '../../components/helpers';
import { showToast } from '../../components/toast';
import {
  renderSkillCard,
  fromPawzHubEntry,
  fromDiscoveredSkill,
  fromCommunitySkill,
} from '../../components/molecules/skill-card';
import {
  PAWZHUB_CATEGORIES,
  FEATURED_SKILL_IDS,
  POPULAR_REPOS,
  POPULAR_TAGS,
  msIcon,
} from './atoms';

// ── State ──────────────────────────────────────────────────────────────────

let _reloadFn: (() => Promise<void>) | null = null;
let _activeCategory = 'all';

export function setReload(fn: () => Promise<void>): void {
  _reloadFn = fn;
}

// ═══════════════════════════════════════════════════════════════════════════
//  PawzHub Registry — Main Section
// ═══════════════════════════════════════════════════════════════════════════

export function renderHeroSection(): string {
  const categoryButtons = PAWZHUB_CATEGORIES.map(
    (c) =>
      `<button class="btn btn-ghost btn-sm ph-category-btn${c === _activeCategory ? ' btn-primary' : ''}" data-category="${escHtml(c)}" style="border-radius:20px;padding:4px 14px;font-size:12px;border:1px solid var(--border-subtle);text-transform:capitalize">${escHtml(c === 'all' ? 'All' : c)}</button>`,
  ).join('');

  return `
  <div class="ph-search-bar" style="margin-bottom:16px">
    <div style="display:flex;gap:8px;align-items:center">
      <div style="flex:1;position:relative">
        <span style="position:absolute;left:10px;top:50%;transform:translateY(-50%);color:var(--text-muted);pointer-events:none">${msIcon('search')}</span>
        <input type="text" class="form-input" id="ph-search-input"
          placeholder="Search skills, integrations, MCP servers..."
          style="width:100%;font-size:14px;padding:10px 12px 10px 36px;border-radius:10px" />
      </div>
      <button class="btn btn-primary" id="ph-search-btn" style="padding:10px 20px;border-radius:10px;font-size:14px">
        Search
      </button>
    </div>
  </div>

  <div class="ph-categories" style="display:flex;gap:6px;flex-wrap:wrap;margin-bottom:16px">
    ${categoryButtons}
  </div>`;
}

// ── Featured Section ───────────────────────────────────────────────────────

export function renderFeaturedSection(entries: PawzHubEntry[]): string {
  const featured = entries.filter((e) => FEATURED_SKILL_IDS.includes(e.id));
  if (featured.length === 0) return '';

  return `
  <div class="ph-section" style="margin-bottom:24px">
    <h3 class="ph-section-title">
      ${msIcon('star')} Featured
      <span class="ph-section-count">${featured.length}</span>
    </h3>
    <div class="skills-card-grid">
      ${featured.map((e) => renderSkillCard(fromPawzHubEntry(e))).join('')}
    </div>
  </div>`;
}

// ── All Skills Section ─────────────────────────────────────────────────────

export function renderAllSkillsSection(entries: PawzHubEntry[]): string {
  if (entries.length === 0) {
    return `
    <div style="text-align:center;padding:32px">
      <span class="ms" style="font-size:48px;opacity:0.3;display:block;margin-bottom:12px">inventory_2</span>
      <p style="color:var(--text-muted);font-size:13px;margin:0">No skills found.</p>
    </div>`;
  }

  return `
  <div class="ph-section">
    <h3 class="ph-section-title">
      ${msIcon('apps')} All Skills
      <span class="ph-section-count">${entries.length}</span>
    </h3>
    <div class="skills-card-grid">
      ${entries.map((e) => renderSkillCard(fromPawzHubEntry(e))).join('')}
    </div>
  </div>`;
}

// ═══════════════════════════════════════════════════════════════════════════
//  Community Skills — Open Source Browser
// ═══════════════════════════════════════════════════════════════════════════

export function renderCommunitySection(installed: CommunitySkill[]): string {
  const tagButtons = POPULAR_TAGS.map(
    (t) =>
      `<button class="btn btn-ghost btn-sm ph-community-tag" data-query="${escHtml(t)}" style="border-radius:20px;padding:4px 14px;font-size:12px;border:1px solid var(--border-subtle)">${escHtml(t)}</button>`,
  ).join('');

  const repoButtons = POPULAR_REPOS.map(
    (r) =>
      `<button class="btn btn-ghost btn-sm ph-community-repo" data-source="${escHtml(r.source)}" style="border-radius:20px;padding:4px 14px;font-size:12px;border:1px solid var(--accent);color:var(--accent)">${escHtml(r.label)}</button>`,
  ).join('');

  const installedCards =
    installed.length > 0
      ? `
    <div class="ph-section" style="margin-top:20px">
      <h3 class="ph-section-title">
        ${msIcon('download')} Installed Community Skills
        <span class="ph-section-count">${installed.length}</span>
      </h3>
      <div class="skills-card-grid">
        ${installed.map((s) => {
          const cardData = fromCommunitySkill(s);
          // Override action with custom HTML for toggle + remove binding
          cardData.action = {
            type: 'custom',
            html: `<label class="skill-toggle-label"><input type="checkbox" class="ph-community-toggle" data-skill="${escHtml(s.id)}" ${s.enabled ? 'checked' : ''} /> Enable</label><button class="btn btn-ghost btn-sm ph-community-remove" data-skill="${escHtml(s.id)}" title="Remove">Remove</button>`,
          };
          return renderSkillCard(cardData);
        }).join('')}
      </div>
    </div>`
      : '';

  return `
  <div class="ph-section" style="margin-top:24px;padding-top:20px;border-top:1px solid var(--border-subtle)">
    <h3 class="ph-section-title">
      ${msIcon('public')} Community Skills
      <a href="https://skills.sh" target="_blank" class="ph-external-link">skills.sh</a>
    </h3>
    <p style="color:var(--text-muted);font-size:12px;margin:0 0 12px;max-width:500px">
      Open-source agent skills from the community. Search, browse repos, and install with one click.
    </p>

    <div style="display:flex;gap:8px;align-items:center;margin-bottom:10px">
      <div style="flex:1;position:relative">
        <span style="position:absolute;left:10px;top:50%;transform:translateY(-50%);color:var(--text-muted);pointer-events:none">${msIcon('search')}</span>
        <input type="text" class="form-input" id="ph-community-search"
          placeholder="Search community skills — marketing, trading..."
          style="width:100%;font-size:13px;padding:8px 10px 8px 34px;border-radius:8px" />
      </div>
      <button class="btn btn-primary btn-sm" id="ph-community-search-btn">Search</button>
    </div>

    <div style="display:flex;gap:6px;flex-wrap:wrap;margin-bottom:8px">
      ${tagButtons}
    </div>

    <div style="display:flex;gap:6px;flex-wrap:wrap;margin-top:8px;padding-top:8px;border-top:1px solid var(--border-subtle)">
      <span style="font-size:11px;color:var(--text-muted);display:flex;align-items:center;gap:4px">${msIcon('folder')} Browse repo:</span>
      ${repoButtons}
      <input type="text" class="form-input" id="ph-community-source"
        placeholder="owner/repo"
        style="font-size:12px;padding:4px 10px;border-radius:8px;width:140px;margin-left:4px" />
      <button class="btn btn-ghost btn-sm" id="ph-community-browse">${msIcon('search')} Browse</button>
    </div>

    <div id="ph-community-search-results" style="display:none;margin-top:14px"></div>
    <div id="ph-community-browse-results" style="display:none;margin-top:14px"></div>
  </div>
  ${installedCards}`;
}

// ═══════════════════════════════════════════════════════════════════════════
//  Search & Browse Handlers
// ═══════════════════════════════════════════════════════════════════════════

// ── PawzHub Registry search/browse ─────────────────────────────────────

export async function pawzhubSearch(query: string, container: HTMLElement): Promise<void> {
  container.innerHTML = `<div style="display:flex;align-items:center;gap:8px;padding:12px;color:var(--text-muted)">
    <span class="wa-spinner"></span> Searching PawzHub for "${escHtml(query)}"...
  </div>`;

  try {
    const entries = await pawEngine.pawzhubSearch(query);
    if (entries.length === 0) {
      container.innerHTML = `<div style="text-align:center;padding:24px">
        <span class="ms" style="font-size:36px;opacity:0.3;display:block;margin-bottom:8px">search_off</span>
        <p style="color:var(--text-muted);font-size:13px;margin:0">No results for "${escHtml(query)}"</p>
        <p style="color:var(--text-muted);font-size:12px;margin:4px 0 0">Try different keywords or browse by category.</p>
      </div>`;
      return;
    }
    container.innerHTML =
      `<div style="font-weight:600;font-size:13px;margin-bottom:10px">${entries.length} result${entries.length !== 1 ? 's' : ''} for "${escHtml(query)}"</div>` +
      `<div class="skills-card-grid">${entries.map((e) => renderSkillCard(fromPawzHubEntry(e))).join('')}</div>`;
    wireInstallButtons(container);
  } catch (err) {
    container.innerHTML = `<p style="color:var(--accent-danger);padding:12px">${msIcon('error')} ${escHtml(String(err))}</p>`;
  }
}

export async function pawzhubBrowse(category: string, container: HTMLElement): Promise<void> {
  _activeCategory = category;
  const label = category === 'all' ? 'all categories' : category;
  container.innerHTML = `<div style="display:flex;align-items:center;gap:8px;padding:12px;color:var(--text-muted)">
    <span class="wa-spinner"></span> Browsing ${escHtml(label)}...
  </div>`;

  try {
    const entries =
      category === 'all'
        ? await pawEngine.pawzhubSearch('')
        : await pawEngine.pawzhubBrowse(category);

    if (entries.length === 0) {
      container.innerHTML = `<div style="text-align:center;padding:24px">
        <span class="ms" style="font-size:36px;opacity:0.3;display:block;margin-bottom:8px">inventory_2</span>
        <p style="color:var(--text-muted);font-size:13px;margin:0">No skills in "${escHtml(label)}" yet.</p>
      </div>`;
      return;
    }
    container.innerHTML =
      renderFeaturedSection(entries) +
      renderAllSkillsSection(entries);
    wireInstallButtons(container);
  } catch (err) {
    container.innerHTML = `<p style="color:var(--accent-danger);padding:12px">${msIcon('error')} ${escHtml(String(err))}</p>`;
  }
}

// ── Community search/browse ────────────────────────────────────────────

async function communitySearch(query: string): Promise<void> {
  const results = $('ph-community-search-results');
  const browseResults = $('ph-community-browse-results');
  if (browseResults) browseResults.style.display = 'none';
  if (!results) return;

  results.style.display = 'block';
  results.innerHTML = `<div style="display:flex;align-items:center;gap:8px;padding:12px;color:var(--text-muted)">
    <span class="wa-spinner"></span> Searching for "${escHtml(query)}"...
  </div>`;

  try {
    const skills = await pawEngine.communitySkillsSearch(query);
    if (skills.length === 0) {
      results.innerHTML = `<div style="text-align:center;padding:16px">
        <p style="color:var(--text-muted);font-size:13px;margin:0">${msIcon('search_off')} No results for "${escHtml(query)}"</p>
      </div>`;
      return;
    }

    const byRepo = new Map<string, typeof skills>();
    for (const s of skills) {
      const existing = byRepo.get(s.source) || [];
      existing.push(s);
      byRepo.set(s.source, existing);
    }

    results.innerHTML =
      `<div style="font-weight:600;font-size:13px;margin-bottom:8px">${skills.length} skill${skills.length !== 1 ? 's' : ''} found from ${byRepo.size} repo${byRepo.size !== 1 ? 's' : ''}</div>` +
      `<div class="skills-card-grid">${skills.map((s) => renderSkillCard(fromDiscoveredSkill(s))).join('')}</div>`;
    wireCommunityInstallButtons(results, skills);
  } catch (err) {
    results.innerHTML = `<p style="color:var(--accent-danger);padding:12px">${msIcon('error')} ${escHtml(String(err))}</p>`;
  }
}

async function communityBrowse(source: string): Promise<void> {
  const results = $('ph-community-browse-results');
  const searchResults = $('ph-community-search-results');
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
    const header = `<div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:8px">
      <span style="font-weight:600;font-size:13px">${skills.length} skills in ${escHtml(source)}</span>
      ${
        notInstalled > 0
          ? `<button class="btn btn-primary btn-sm" id="ph-community-install-all" data-source="${escHtml(source)}">
            ${msIcon('download')} Install All (${notInstalled})
          </button>`
          : ''
      }
    </div>`;

    results.innerHTML = header + `<div class="skills-card-grid">${skills.map((s) => renderSkillCard(fromDiscoveredSkill(s))).join('')}</div>`;
    wireCommunityInstallButtons(results, skills);
  } catch (err) {
    results.innerHTML = `<p style="color:var(--accent-danger);padding:12px">${msIcon('error')} ${escHtml(String(err))}</p>`;
  }
}

// ═══════════════════════════════════════════════════════════════════════════
//  Install Button Wiring
// ═══════════════════════════════════════════════════════════════════════════

function wireInstallButtons(container: HTMLElement): void {
  const reload = () => (_reloadFn ? _reloadFn() : Promise.resolve());

  // Unified card uses .uc-install-btn for PawzHub registry installs
  container.querySelectorAll('.uc-install-btn').forEach((el) => {
    el.addEventListener('click', async () => {
      const btn = el as HTMLButtonElement;
      const skillId = btn.dataset.skillId!;
      const sourceRepo = btn.dataset.sourceRepo;
      const source = btn.dataset.source;
      const path = btn.dataset.path;
      const name = btn.dataset.name!;

      btn.disabled = true;
      btn.innerHTML = `<span class="wa-spinner" style="width:12px;height:12px"></span> Installing...`;

      try {
        if (sourceRepo) {
          await pawEngine.pawzhubInstall(skillId, sourceRepo);
          showToast(`${name} installed from PawzHub!`, 'success');
        } else if (source && path) {
          await pawEngine.communitySkillInstall(source, path);
          showToast(`${name} installed!`, 'success');
        }
        await reload();
      } catch (err) {
        showToast(`Install failed: ${err}`, 'error');
        btn.disabled = false;
        btn.innerHTML = `${msIcon('download')} Install`;
      }
    });
  });
}

function wireCommunityInstallButtons(container: HTMLElement, skills: DiscoveredSkill[]): void {
  // Individual install buttons are now handled by wireInstallButtons (unified card)
  wireInstallButtons(container);

  const reload = () => (_reloadFn ? _reloadFn() : Promise.resolve());

  // Install All button
  container.querySelector('#ph-community-install-all')?.addEventListener('click', async () => {
    const allBtn = $('ph-community-install-all') as HTMLButtonElement;
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

// ═══════════════════════════════════════════════════════════════════════════
//  Event Binding
// ═══════════════════════════════════════════════════════════════════════════

export function bindPawzHubEvents(container: HTMLElement): void {
  const searchInput = $('ph-search-input') as HTMLInputElement | null;

  // Registry search
  $('ph-search-btn')?.addEventListener('click', () => {
    if (searchInput?.value.trim()) pawzhubSearch(searchInput.value.trim(), container);
  });
  searchInput?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && searchInput.value.trim()) {
      pawzhubSearch(searchInput.value.trim(), container);
    }
  });

  // Category buttons
  document.querySelectorAll('.ph-category-btn').forEach((el) => {
    el.addEventListener('click', () => {
      document.querySelectorAll('.ph-category-btn').forEach((b) => b.classList.remove('btn-primary'));
      el.classList.add('btn-primary');
      const category = (el as HTMLElement).dataset.category!;
      pawzhubBrowse(category, container);
    });
  });
}

export function bindCommunityEvents(): void {
  const reload = () => (_reloadFn ? _reloadFn() : Promise.resolve());
  const searchInput = $('ph-community-search') as HTMLInputElement | null;

  // Community search
  $('ph-community-search-btn')?.addEventListener('click', () => {
    if (searchInput?.value.trim()) communitySearch(searchInput.value.trim());
  });
  searchInput?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && searchInput.value.trim()) {
      communitySearch(searchInput.value.trim());
    }
  });

  // Tag buttons
  document.querySelectorAll('.ph-community-tag').forEach((el) => {
    el.addEventListener('click', () => {
      const query = (el as HTMLElement).dataset.query!;
      if (searchInput) searchInput.value = query;
      communitySearch(query);
    });
  });

  // Repo browse button
  $('ph-community-browse')?.addEventListener('click', () => {
    const input = $('ph-community-source') as HTMLInputElement | null;
    if (input?.value.trim()) communityBrowse(input.value.trim());
  });

  // Quick browse repo buttons
  document.querySelectorAll('.ph-community-repo').forEach((el) => {
    el.addEventListener('click', () => {
      const source = (el as HTMLElement).dataset.source!;
      const input = $('ph-community-source') as HTMLInputElement | null;
      if (input) input.value = source;
      communityBrowse(source);
    });
  });

  // Enable/disable toggles for installed community skills
  document.querySelectorAll('.ph-community-toggle').forEach((el) => {
    el.addEventListener('change', async (e) => {
      const input = e.target as HTMLInputElement;
      const skillId = input.dataset.skill!;
      try {
        await pawEngine.communitySkillSetEnabled(skillId, input.checked);
        showToast(
          `${skillId.split('/').pop()} ${input.checked ? 'enabled' : 'disabled'}`,
          'success',
        );
        await reload();
      } catch (err) {
        showToast(`Failed: ${err}`, 'error');
        input.checked = !input.checked;
      }
    });
  });

  // Remove buttons
  document.querySelectorAll('.ph-community-remove').forEach((el) => {
    el.addEventListener('click', async () => {
      const btn = el as HTMLButtonElement;
      const skillId = btn.dataset.skill!;
      const name = skillId.split('/').pop() || skillId;

      if (!(await confirmModal(`Remove "${name}"? You can reinstall it later.`))) return;

      try {
        await pawEngine.communitySkillRemove(skillId);
        showToast(`${name} removed`, 'success');
        await reload();
      } catch (err) {
        showToast(`Failed: ${err}`, 'error');
      }
    });
  });
}
