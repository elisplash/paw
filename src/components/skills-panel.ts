// skills-panel.ts — Skills side panel: hero stats, kinetic init, quick action wiring

import { $ } from './helpers';

// ── Hero stat counters ─────────────────────────────────────────────────────

export function updateSkillsHeroStats(total: number, active: number, mcp: number): void {
  const elTotal = $('skills-stat-total');
  const elActive = $('skills-stat-active');
  const elMcp = $('skills-stat-mcp');
  if (elTotal) elTotal.textContent = String(total);
  if (elActive) elActive.textContent = String(active);
  if (elMcp) elMcp.textContent = String(mcp);
}

// ── Quick Actions wiring ───────────────────────────────────────────────────

export function bindSkillsQuickActions(opts: {
  onRefresh: () => void;
  onCreateTab: () => void;
}): void {
  $('skills-qa-create')?.addEventListener('click', opts.onCreateTab);
  $('skills-qa-refresh')?.addEventListener('click', opts.onRefresh);
  $('skills-qa-browse')?.addEventListener('click', () => {
    // Switch to Prompts tab which has community browser
    const toolsTab = document.querySelector('.skills-tab[data-skills-tab="tools"]') as HTMLElement | null;
    toolsTab?.click();
  });
}

// ── Kinetic init ───────────────────────────────────────────────────────────

export function initSkillsKinetic(): void {
  const view = document.getElementById('skills-view');
  if (!view) return;

  // Stagger side panel cards
  const stagger = view.querySelector('.skills-side-panel');
  if (stagger) {
    const cards = stagger.querySelectorAll('.skills-panel-card');
    cards.forEach((card, i) => {
      (card as HTMLElement).style.animationDelay = `${i * 60}ms`;
    });
  }
}
