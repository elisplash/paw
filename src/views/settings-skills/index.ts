// Settings Skills — Index (orchestration, state, exports)

import { pawEngine } from '../../engine';
import { isEngineMode } from '../../engine-bridge';
import { $ } from '../../components/helpers';
import {
  renderSkillsPage,
  bindFilterEvents,
  bindSkillEvents,
  setMoleculesState,
} from './molecules';
import {
  renderCommunitySection,
  renderPawzHubSection,
  bindCommunityEvents,
  bindPawzHubEvents,
  setCommunityReload,
} from './community';
import { renderWizardSection, bindWizardEvents, setWizardReload } from './wizard';
import { buildExtensionTabs, renderExtensionView } from './extension-view';

// ── Re-exports ─────────────────────────────────────────────────────────────

export { CATEGORY_META, SKILL_ICON_MAP, msIcon, skillIcon, formatInstalls } from './atoms';
export { POPULAR_REPOS, POPULAR_TAGS } from './atoms';

// ── Module state ───────────────────────────────────────────────────────────

let _currentFilter = 'all';

function setFilter(f: string): void {
  _currentFilter = f;
}

// ── Main loader ────────────────────────────────────────────────────────────

export async function loadSkillsSettings(): Promise<void> {
  const loading = $('skills-vault-loading');
  const list = $('skills-vault-list');

  if (!isEngineMode()) {
    if (loading) loading.textContent = 'Pawz engine is required.';
    if (list) list.innerHTML = '';
    return;
  }

  // Wire reload callbacks
  setMoleculesState({ currentFilter: _currentFilter, reloadFn: loadSkillsSettings });
  setCommunityReload(loadSkillsSettings);
  setWizardReload(loadSkillsSettings);

  try {
    if (loading) loading.style.display = '';
    const [skills, communitySkills, tomlSkills] = await Promise.all([
      pawEngine.skillsList(),
      pawEngine.communitySkillsList(),
      pawEngine.tomlSkillsScan(),
    ]);
    if (loading) loading.style.display = 'none';

    // Build extension tabs from TOML skills with [view] sections
    const extTabs = buildExtensionTabs(tomlSkills);

    // Update molecules state with current filter
    setMoleculesState({ currentFilter: _currentFilter, reloadFn: loadSkillsSettings });

    // Render extension section (if any extensions have views)
    const extensionSection =
      extTabs.length > 0
        ? `<div class="skills-section extensions-section">
            <h2 style="font-size:17px;font-weight:700;margin:0 0 12px;display:flex;align-items:center;gap:8px">
              <span class="ms ms-sm">extension</span> Extensions
              <span style="font-size:12px;font-weight:400;color:var(--text-muted)">${extTabs.length} installed</span>
            </h2>
            <div class="extension-tabs-row" style="display:flex;gap:8px;flex-wrap:wrap;margin-bottom:16px">
              ${extTabs.map((t) => `<button class="btn btn-sm btn-ghost extension-tab-btn" data-ext-skill="${t.skillId}"><span class="ms ms-sm">${t.icon}</span> ${t.label}</button>`).join('')}
            </div>
            <div id="extension-view-container"></div>
          </div>`
        : '';

    if (list)
      list.innerHTML =
        renderWizardSection() +
        renderPawzHubSection() +
        renderCommunitySection(communitySkills) +
        extensionSection +
        renderSkillsPage(skills);
    bindFilterEvents(skills, setFilter);
    bindSkillEvents();
    bindCommunityEvents();
    bindPawzHubEvents();
    bindWizardEvents();
    bindExtensionTabEvents();
  } catch (e) {
    console.error('[skills-settings] Load failed:', e);
    if (loading) loading.textContent = `Failed to load skills: ${e}`;
  }
}

// ── Extension tab event binding ────────────────────────────────────────────

function bindExtensionTabEvents(): void {
  document.querySelectorAll('.extension-tab-btn').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const skillId = (btn as HTMLElement).dataset.extSkill;
      if (!skillId) return;

      // Highlight active tab
      document
        .querySelectorAll('.extension-tab-btn')
        .forEach((b) => b.classList.remove('btn-primary'));
      btn.classList.add('btn-primary');
      btn.classList.remove('btn-ghost');

      const container = document.getElementById('extension-view-container');
      if (container) {
        await renderExtensionView(container, skillId);
      }
    });
  });
}
