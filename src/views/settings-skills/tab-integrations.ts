// My Skills — Integrations Tab
// Shows TOML and built-in skills that have credentials (tier = 'integration').
// Re-uses the existing renderSkillsPage / card rendering from molecules.ts.

import type { EngineSkillStatus } from '../../engine';
import {
  renderSkillsPage,
  bindFilterEvents,
  bindSearchEvents,
  bindSkillEvents,
  setMoleculesState,
} from './molecules';

// ── Render ─────────────────────────────────────────────────────────────

export function renderIntegrationsTab(
  skills: EngineSkillStatus[],
  reloadFn: () => Promise<void>,
): string {
  // Filter to integrations: skills with credentials (tier = integration) or extensions
  const integrations = skills.filter(
    (s) =>
      s.tier === 'integration' || (s.required_credentials && s.required_credentials.length > 0),
  );

  if (integrations.length === 0) {
    return `
    <div style="text-align:center;padding:48px 24px">
      <span class="ms" style="font-size:48px;opacity:0.3;display:block;margin-bottom:12px">key</span>
      <h3 style="margin:0 0 8px;font-size:16px;font-weight:600;color:var(--text-primary)">No integrations</h3>
      <p style="color:var(--text-muted);font-size:13px;margin:0">
        Service integrations are now managed on the <strong>Integrations</strong> page.
      </p>
    </div>`;
  }

  // Reuse existing molecule rendering (category-grouped skill cards)
  setMoleculesState({ currentFilter: 'all', searchQuery: '', reloadFn });
  return renderSkillsPage(integrations);
}

// ── Event binding ──────────────────────────────────────────────────────

export function bindIntegrationsTabEvents(
  skills: EngineSkillStatus[],
  setFilter: (f: string) => void,
  setSearch: (q: string) => void,
): void {
  bindFilterEvents(skills, setFilter);
  bindSearchEvents(setSearch);
  bindSkillEvents();
}
