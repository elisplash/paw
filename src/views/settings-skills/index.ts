// Skills — Community .md Prompt Skills (skills.sh)
// Shows installed community skills + skills.sh browser.
// All native Rust tools moved to Built In page.

import { pawEngine } from '../../engine';
import { isEngineMode } from '../../engine-bridge';
import { $ } from '../../components/helpers';
import { renderCommunitySection, setCommunityReload, bindCommunityEvents } from './community';

// ── Re-exports (backward compat) ──────────────────────────────────────────

export { CATEGORY_META, SKILL_ICON_MAP, msIcon, skillIcon, formatInstalls } from './atoms';
export { POPULAR_REPOS, POPULAR_TAGS } from './atoms';

// ── Hero stats ─────────────────────────────────────────────────────────

function updateSkillsHeroStats(installed: number, enabled: number): void {
  const elInstalled = $('skills-stat-installed');
  const elEnabled = $('skills-stat-enabled');
  if (elInstalled) elInstalled.textContent = String(installed);
  if (elEnabled) elEnabled.textContent = String(enabled);
}

// ── Main data loader ───────────────────────────────────────────────────

export async function loadSkillsSettings(): Promise<void> {
  const loading = $('skills-vault-loading');
  const list = $('skills-vault-list');

  if (!isEngineMode()) {
    if (loading) loading.textContent = 'Pawz engine is required.';
    if (list) list.innerHTML = '';
    return;
  }

  // Set reload callback for community module
  setCommunityReload(loadSkillsSettings);

  try {
    if (loading) loading.style.display = '';

    // Fetch community skills
    const communitySkills = await pawEngine.communitySkillsList();

    if (loading) loading.style.display = 'none';

    // Hero stats
    const enabled = communitySkills.filter((s) => s.enabled).length;
    updateSkillsHeroStats(communitySkills.length, enabled);

    // Render community section (installed + browser)
    if (list) {
      list.innerHTML = renderCommunitySection(communitySkills);
      bindCommunityEvents();
    }

    // Quick actions in side panel
    $('skills-qa-refresh')?.addEventListener('click', () => loadSkillsSettings());
    $('skills-qa-browse-community')?.addEventListener('click', () => {
      const searchInput = document.getElementById(
        'community-skill-search',
      ) as HTMLInputElement | null;
      if (searchInput) {
        searchInput.focus();
        searchInput.scrollIntoView({ behavior: 'smooth', block: 'center' });
      }
    });

    // Refresh button in hero
    $('refresh-skills-btn')?.addEventListener('click', () => loadSkillsSettings());

    // Kinetic stagger on side panel cards
    const view = document.getElementById('skills-view');
    if (view) {
      const cards = view.querySelectorAll('.skills-panel-card');
      cards.forEach((card, i) => {
        (card as HTMLElement).style.animationDelay = `${i * 60}ms`;
      });
    }
  } catch (e) {
    console.error('[skills] Load failed:', e);
    if (loading) loading.textContent = `Failed to load skills: ${e}`;
  }
}
