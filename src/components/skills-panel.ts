// skills-panel.ts — Skills side panel: kinetic init
// Hero stats are now managed directly in Skills and Built In index modules.

import { $ } from './helpers';

// ── Hero stat counters (kept for backward compat) ──────────────────────────

export function updateSkillsHeroStats(total: number, active: number, mcp: number): void {
  const elTotal = $('skills-stat-total') ?? $('skills-stat-installed');
  const elActive = $('skills-stat-active') ?? $('skills-stat-enabled');
  const elMcp = $('skills-stat-mcp');
  if (elTotal) elTotal.textContent = String(total);
  if (elActive) elActive.textContent = String(active);
  if (elMcp) elMcp.textContent = String(mcp);
}

// ── Quick Actions wiring (kept for backward compat) ────────────────────────

export function bindSkillsQuickActions(opts: {
  onRefresh: () => void;
  onCreateTab?: () => void;
}): void {
  $('skills-qa-refresh')?.addEventListener('click', opts.onRefresh);
  if (opts.onCreateTab) {
    $('skills-qa-create')?.addEventListener('click', opts.onCreateTab);
  }
  $('skills-qa-browse')?.addEventListener('click', () => {
    $('skills-qa-browse-community')?.click();
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
