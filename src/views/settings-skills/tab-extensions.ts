// My Skills — Extensions Tab
// Shows skills with [view] or [widget] sections (Extension tier TOML skills).
// Re-uses the extension-view.ts renderer for individual extension details.

import type { EngineSkillStatus, TomlSkillEntry } from '../../engine';
import { escHtml } from '../../components/helpers';
import { msIcon } from './atoms';
import { buildExtensionTabs, renderExtensionView } from './extension-view';
import { renderSkillCard, fromEngineSkill } from '../../components/molecules/skill-card';

// ── Render ─────────────────────────────────────────────────────────────

export function renderExtensionsTab(
  skills: EngineSkillStatus[],
  tomlSkills: TomlSkillEntry[],
): string {
  // Extension-tier skills: has_widget OR has_view OR tier === 'extension'
  const extensions = skills.filter(
    (s) => s.tier === 'extension' || s.has_widget,
  );

  // Build extension tabs from TOML skills with [view] sections
  const viewTabs = buildExtensionTabs(tomlSkills);

  if (extensions.length === 0 && viewTabs.length === 0) {
    return `
    <div style="text-align:center;padding:48px 24px">
      <span class="ms" style="font-size:48px;opacity:0.3;display:block;margin-bottom:12px">dashboard</span>
      <h3 style="margin:0 0 8px;font-size:16px;font-weight:600;color:var(--text-primary)">No extensions</h3>
      <p style="color:var(--text-muted);font-size:13px;margin:0;max-width:400px;margin-inline:auto">
        Extensions add dashboard widgets and custom views. Install them from <strong>PawzHub</strong> or
        create one with the <strong>Create</strong> tab.
      </p>
    </div>`;
  }

  let html = '';

  // View tabs section
  if (viewTabs.length > 0) {
    html += `
    <div style="margin-bottom:24px">
      <h3 style="margin:0 0 12px;font-size:15px;font-weight:600;display:flex;align-items:center;gap:6px">
        ${msIcon('tab')} Custom Views
        <span style="font-size:12px;font-weight:400;color:var(--text-muted)">(${viewTabs.length})</span>
      </h3>
      <div class="skills-card-grid">
        ${viewTabs
          .map(
            (tab) => `
          <div class="skill-card-compact extension-view-card" data-extension-id="${escHtml(tab.skillId)}" style="cursor:pointer">
            <div class="skill-card-compact-header">
              <span class="skill-card-icon">${msIcon(tab.icon)}</span>
              <div class="skill-card-compact-info">
                <strong class="skill-card-name">${escHtml(tab.label)}</strong>
                <span class="skill-status status-ready">
                  ${msIcon('check_circle')} Active
                </span>
              </div>
            </div>
            <div class="skill-badges-row">
              <span class="skill-tier-badge skill-tier-extension">Extension</span>
              <span class="skill-badge">${msIcon('tab')} Custom View</span>
            </div>
          </div>`,
          )
          .join('')}
      </div>
    </div>`;
  }

  // Widget extensions section
  const widgetExtensions = extensions.filter((s) => s.has_widget);
  if (widgetExtensions.length > 0) {
    html += `
    <div>
      <h3 style="margin:0 0 12px;font-size:15px;font-weight:600;display:flex;align-items:center;gap:6px">
        ${msIcon('dashboard')} Widget Extensions
        <span style="font-size:12px;font-weight:400;color:var(--text-muted)">(${widgetExtensions.length})</span>
      </h3>
      <div class="skills-card-grid">
        ${widgetExtensions.map((s) => {
          const cardData = fromEngineSkill(s);
          cardData.action = { type: 'none' };
          return renderSkillCard(cardData);
        }).join('')}
      </div>
    </div>`;
  }

  // Extension detail view (shown when user clicks a card)
  html += `<div id="extension-detail-view" style="display:none;margin-top:16px"></div>`;

  return html;
}

// ── Event binding ──────────────────────────────────────────────────────

export function bindExtensionsTabEvents(): void {
  // Click on extension view cards → open detail view
  document.querySelectorAll('.extension-view-card').forEach((el) => {
    el.addEventListener('click', async () => {
      const skillId = (el as HTMLElement).dataset.extensionId;
      if (!skillId) return;

      const detailView = document.getElementById('extension-detail-view');
      if (!detailView) return;

      detailView.style.display = '';
      await renderExtensionView(detailView, skillId);
    });
  });
}
