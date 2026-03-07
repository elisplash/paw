// Skills — Community .md Prompt Skills (skills.sh)
// Shows installed community skills + skills.sh browser.
// All native Rust tools moved to Built In page.

import { pawEngine } from '../../engine';
import { isEngineMode } from '../../engine-bridge';
import { $, escHtml } from '../../components/helpers';
import { renderCommunitySection, setCommunityReload, bindCommunityEvents } from './community';
import { msIcon } from './atoms';

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

    // Load FORGE training stats into side panel (non-blocking)
    loadForgeStats();

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

// ── FORGE Training Stats (side panel) ──────────────────────────────────

async function loadForgeStats(): Promise<void> {
  const container = $('skills-forge-stats');
  if (!container) return;

  try {
    const [summary, domains] = await Promise.all([
      pawEngine.forgeCertSummary('default'),
      pawEngine.forgeListDomains('default'),
    ]);

    const total =
      summary.uncertified +
      summary.in_training +
      summary.certified +
      summary.expired +
      summary.failed;

    if (total === 0) {
      container.innerHTML = `<p style="color:var(--text-muted);font-size:12px">No procedural memories yet. FORGE certification stats will appear here as agents learn skills.</p>`;
      return;
    }

    const statsHtml = `
      <div style="display:flex;gap:12px;flex-wrap:wrap;margin-bottom:8px">
        <span style="font-size:12px;display:flex;align-items:center;gap:3px">
          ${msIcon('verified')} <strong>${summary.certified}</strong> certified
        </span>
        <span style="font-size:12px;display:flex;align-items:center;gap:3px">
          ${msIcon('model_training')} <strong>${summary.in_training}</strong> training
        </span>
        <span style="font-size:12px;display:flex;align-items:center;gap:3px;color:var(--text-muted)">
          ${summary.uncertified} uncertified
        </span>
      </div>`;

    const domainsHtml =
      domains.length > 0
        ? domains
            .map(
              (d) =>
                `<div style="display:flex;align-items:center;justify-content:space-between;padding:3px 0;font-size:12px">
                <span>${escHtml(d.domain)}</span>
                <span style="color:var(--text-muted)">${d.certified_skills}/${d.total_skills}</span>
              </div>`,
            )
            .join('')
        : '';

    container.innerHTML = statsHtml + domainsHtml;
  } catch {
    container.innerHTML = `<p style="color:var(--text-muted);font-size:12px">Could not load FORGE data.</p>`;
  }
}
