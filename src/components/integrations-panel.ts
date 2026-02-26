// src/components/integrations-panel.ts — Side-panel data for integrations view
//
// Molecule-level: renders hero stats, health list, category breakdown.

import { SERVICE_CATALOG } from '../views/integrations/catalog';
import {
  CATEGORIES,
  type ConnectedService,
  type ServiceCategory,
} from '../views/integrations/atoms';

// ── Hero Stats ─────────────────────────────────────────────────────────

export function updateIntegrationsHeroStats(connected: ConnectedService[]): void {
  const totalEl = document.getElementById('integrations-stat-total');
  const connectedEl = document.getElementById('integrations-stat-connected');
  const toolsEl = document.getElementById('integrations-stat-tools');

  if (totalEl) totalEl.textContent = String(SERVICE_CATALOG.length);
  if (connectedEl) connectedEl.textContent = String(connected.length);
  if (toolsEl) {
    const total = connected.reduce((sum, c) => sum + c.toolCount, 0);
    toolsEl.textContent = String(total);
  }
}

// ── Connection Health List ─────────────────────────────────────────────

export function renderHealthList(connected: ConnectedService[]): void {
  const container = document.getElementById('integrations-health-list');
  if (!container) return;

  if (connected.length === 0) {
    container.innerHTML = '<div class="integrations-health-empty">No connections yet</div>';
    return;
  }

  container.innerHTML = connected
    .slice(0, 8)
    .map((c) => {
      const svc = SERVICE_CATALOG.find((s) => s.id === c.serviceId);
      const name = svc?.name ?? c.serviceId;
      const dotClass =
        c.status === 'error' ? 'error' : c.status === 'expired' ? 'warning' : 'healthy';
      const statusLabel = c.status === 'error' ? 'ERR' : c.status === 'expired' ? 'EXP' : 'OK';
      return `<div class="integrations-health-item">
        <span class="integrations-health-dot ${dotClass}"></span>
        <span class="integrations-health-name">${name}</span>
        <span class="integrations-health-status">${statusLabel}</span>
      </div>`;
    })
    .join('');
}

// ── Category Breakdown ─────────────────────────────────────────────────

export function renderCategoryBreakdown(): void {
  const container = document.getElementById('integrations-category-breakdown');
  if (!container) return;

  const counts = new Map<ServiceCategory, number>();
  for (const s of SERVICE_CATALOG) {
    counts.set(s.category, (counts.get(s.category) ?? 0) + 1);
  }

  const maxCount = Math.max(...counts.values(), 1);

  container.innerHTML = CATEGORIES.filter((cat) => (counts.get(cat.id) ?? 0) > 0)
    .sort((a, b) => (counts.get(b.id) ?? 0) - (counts.get(a.id) ?? 0))
    .map((cat) => {
      const count = counts.get(cat.id) ?? 0;
      const pct = Math.round((count / maxCount) * 100);
      return `<div class="integrations-cat-row">
        <span class="integrations-cat-row-label">${cat.label}</span>
        <div class="integrations-cat-row-bar">
          <div class="integrations-cat-row-fill" style="width: ${pct}%"></div>
        </div>
        <span class="integrations-cat-row-count">${count}</span>
      </div>`;
    })
    .join('');
}

// ── Kinetic Init ───────────────────────────────────────────────────────

export function initIntegrationsKinetic(): void {
  const sidePanel = document.querySelector('.integrations-side-panel');
  if (!sidePanel) return;

  const cards = sidePanel.querySelectorAll('.integrations-panel-card');
  cards.forEach((card, i) => {
    (card as HTMLElement).style.animationDelay = `${i * 60}ms`;
    card.classList.add('k-materialise');
  });
}
