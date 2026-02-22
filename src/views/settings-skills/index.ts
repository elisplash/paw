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
    const skills = await pawEngine.skillsList();
    const communitySkills = await pawEngine.communitySkillsList();
    if (loading) loading.style.display = 'none';

    // Update molecules state with current filter
    setMoleculesState({ currentFilter: _currentFilter, reloadFn: loadSkillsSettings });

    if (list)
      list.innerHTML =
        renderWizardSection() +
        renderPawzHubSection() +
        renderCommunitySection(communitySkills) +
        renderSkillsPage(skills);
    bindFilterEvents(skills, setFilter);
    bindSkillEvents();
    bindCommunityEvents();
    bindPawzHubEvents();
    bindWizardEvents();
  } catch (e) {
    console.error('[skills-settings] Load failed:', e);
    if (loading) loading.textContent = `Failed to load skills: ${e}`;
  }
}
