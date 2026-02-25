// PawzHub Marketplace — Orchestrator
// Replaces the old iframe approach with a native in-app marketplace.

import {
  pawEngine,
  type PawzHubEntry,
  type CommunitySkill,
} from '../../engine';
import { $ } from '../../components/helpers';
import {
  renderHeroSection,
  renderFeaturedSection,
  renderAllSkillsSection,
  renderCommunitySection,
  setReload,
  bindPawzHubEvents,
  bindCommunityEvents,
} from './molecules';

// ── Public entry point ─────────────────────────────────────────────────────

export async function loadPawzHub(): Promise<void> {
  const container = $('pawzhub-content');
  if (!container) return;

  // Wire reload reference so install buttons can refresh the view
  setReload(loadPawzHub);

  // Show loading state
  container.innerHTML = `<div style="display:flex;align-items:center;gap:8px;padding:24px;color:var(--text-muted)">
    <span class="wa-spinner"></span> Loading PawzHub Marketplace...
  </div>`;

  try {
    // Parallel fetch: registry + community skills
    const [entries, communitySkills] = await Promise.all([
      fetchRegistryEntries(),
      fetchCommunitySkills(),
    ]);

    // Render full page
    container.innerHTML =
      renderHeroSection() +
      `<div id="ph-registry-results">` +
      renderFeaturedSection(entries) +
      renderAllSkillsSection(entries) +
      `</div>` +
      renderCommunitySection(communitySkills);

    // Bind events
    const registryContainer = $('ph-registry-results');
    if (registryContainer) {
      bindPawzHubEvents(registryContainer);
    }
    bindCommunityEvents();

    // Wire install buttons for the initial render
    wireInitialInstallButtons(container, entries);
  } catch (err) {
    container.innerHTML = `
    <div style="text-align:center;padding:32px">
      <span class="ms" style="font-size:48px;opacity:0.3;display:block;margin-bottom:12px">cloud_off</span>
      <p style="color:var(--text-muted);font-size:14px;margin:0 0 12px">Failed to load PawzHub</p>
      <p style="color:var(--accent-danger);font-size:12px;margin:0 0 16px">${String(err)}</p>
      <button class="btn btn-primary" id="ph-retry-btn">Retry</button>
    </div>`;
    $('ph-retry-btn')?.addEventListener('click', () => loadPawzHub());
  }
}

// ── Data fetching helpers ──────────────────────────────────────────────────

async function fetchRegistryEntries(): Promise<PawzHubEntry[]> {
  try {
    return await pawEngine.pawzhubSearch('');
  } catch {
    console.warn('[PawzHub] Registry fetch failed, returning empty');
    return [];
  }
}

async function fetchCommunitySkills(): Promise<CommunitySkill[]> {
  try {
    return await pawEngine.communitySkillsList();
  } catch {
    console.warn('[PawzHub] Community skills list failed, returning empty');
    return [];
  }
}

// ── Wire initial install buttons ───────────────────────────────────────────

function wireInitialInstallButtons(container: HTMLElement, _entries: PawzHubEntry[]): void {
  // Unified card uses .uc-install-btn for all install actions.
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
        } else if (source && path) {
          await pawEngine.communitySkillInstall(source, path);
        }
        const { showToast } = await import('../../components/toast');
        showToast(`${name} installed!`, 'success');
        await loadPawzHub();
      } catch (err) {
        const { showToast } = await import('../../components/toast');
        showToast(`Install failed: ${err}`, 'error');
        btn.disabled = false;
        btn.innerHTML = `Install`;
      }
    });
  });
}
