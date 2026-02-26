// Skills — Active Tab
// Split view: Ready & Working (green) vs Needs Setup (amber) vs Platform Unavailable (grey).
// Integration-tier skills excluded — they live on the Integrations page.

import type { EngineSkillStatus, McpServerStatus } from '../../engine';
import { escHtml } from '../../components/helpers';
import {
  renderSkillCard,
  fromEngineSkill,
  type SkillCardData,
} from '../../components/molecules/skill-card';
import { msIcon } from './atoms';

// ── Types ──────────────────────────────────────────────────────────────

export interface ActiveTabData {
  skills: EngineSkillStatus[];
  mcpStatuses: McpServerStatus[];
}

// ── Render ─────────────────────────────────────────────────────────────

export function renderActiveTab(data: ActiveTabData): string {
  // Exclude integration-tier skills — those live on the Integrations page
  const enabledSkills = data.skills.filter((s) => s.enabled && s.tier !== 'integration');
  const connectedMcp = data.mcpStatuses.filter((s) => s.connected);

  // Categorise enabled skills
  const ready = enabledSkills.filter((s) => s.is_ready);
  const needsCreds = enabledSkills.filter(
    (s) => !s.is_ready && s.missing_binaries.length === 0 && s.missing_credentials.length > 0,
  );
  const needsEnv = enabledSkills.filter(
    (s) =>
      !s.is_ready &&
      s.missing_binaries.length === 0 &&
      s.missing_credentials.length === 0 &&
      s.missing_env_vars.length > 0,
  );
  const missingBinaries = enabledSkills.filter((s) => !s.is_ready && s.missing_binaries.length > 0);
  const needsSetup = [...needsCreds, ...needsEnv];

  if (enabledSkills.length === 0 && connectedMcp.length === 0) {
    return `
    <div style="text-align:center;padding:48px 24px">
      <span class="ms" style="font-size:48px;opacity:0.3;display:block;margin-bottom:12px">check_circle</span>
      <h3 style="margin:0 0 8px;font-size:16px;font-weight:600;color:var(--text-primary)">No active skills</h3>
      <p style="color:var(--text-muted);font-size:13px;margin:0 0 16px;max-width:400px;margin-inline:auto">
        Enable prompt skills from the <strong>Prompts</strong> tab to give your agent new abilities.
        For service integrations, visit the <strong>Integrations</strong> page.
      </p>
    </div>`;
  }

  let html = '';

  // ── Ready & Working section ──────────────────────────────────────
  if (ready.length > 0 || connectedMcp.length > 0) {
    html += `
    <div class="active-section">
      <div class="active-section-header">
        <span class="active-section-dot active-dot-ready"></span>
        <span class="active-section-title">Ready &amp; Working</span>
        <span class="active-section-count">${ready.length + connectedMcp.length}</span>
      </div>
      <div class="skills-card-grid">`;

    for (const skill of ready) {
      const cardData = fromEngineSkill(skill);
      cardData.action = { type: 'none' };
      html += renderSkillCard(cardData);
    }

    for (const mcp of connectedMcp) {
      const cardData: SkillCardData = {
        id: mcp.id,
        name: mcp.name ?? mcp.id,
        description: `${mcp.tool_count ?? 0} tool${(mcp.tool_count ?? 0) !== 1 ? 's' : ''} available`,
        icon: 'dns',
        tier: 'mcp',
        status: 'active',
        statusLabel: 'Connected',
        toolCount: mcp.tool_count ?? 0,
        hasWidget: false,
        hasMcp: true,
        verified: false,
        action: { type: 'none' },
        dataAttrs: { 'mcp-id': mcp.id },
      };
      html += renderSkillCard(cardData);
    }

    html += '</div></div>';
  }

  // ── Needs Setup section (missing credentials / env vars) ─────────
  if (needsSetup.length > 0) {
    html += `
    <div class="active-section">
      <div class="active-section-header">
        <span class="active-section-dot active-dot-setup"></span>
        <span class="active-section-title">Needs Setup</span>
        <span class="active-section-count">${needsSetup.length}</span>
      </div>
      <p class="active-section-desc">These skills are enabled but need credentials or environment variables configured before they can work.</p>
      <div class="skills-card-grid">`;

    for (const skill of needsSetup) {
      const cardData = fromEngineSkill(skill);
      cardData.action = { type: 'none' };
      html += renderSkillCard(cardData);
    }

    html += '</div></div>';
  }

  // ── Platform Unavailable section (missing binaries) ──────────────
  if (missingBinaries.length > 0) {
    html += `
    <div class="active-section active-section-collapsed">
      <div class="active-section-header active-section-toggle" id="active-unavailable-toggle">
        <span class="active-section-dot active-dot-unavail"></span>
        <span class="active-section-title">Platform Unavailable</span>
        <span class="active-section-count">${missingBinaries.length}</span>
        <span class="ms active-section-chevron ms-sm">expand_more</span>
      </div>
      <p class="active-section-desc">
        These skills require binaries not found on this system (e.g. macOS-only apps, or tools not installed).
        They're enabled but can't run here. You can disable them to clean up this list.
      </p>
      <div class="active-section-body" id="active-unavailable-body" style="display:none">
        <div class="active-unavail-grid">`;

    for (const skill of missingBinaries) {
      const missing = skill.missing_binaries.map((b) => escHtml(b)).join(', ');
      html += `
          <div class="active-unavail-row" data-skill-id="${escHtml(skill.id)}">
            <div class="active-unavail-info">
              <strong>${escHtml(skill.name)}</strong>
              <span class="active-unavail-missing">${msIcon('error')} Missing: <code>${missing}</code></span>
              ${skill.install_hint ? `<span class="active-unavail-hint">${msIcon('terminal')} ${escHtml(skill.install_hint)}</span>` : ''}
            </div>
            <button class="btn btn-ghost btn-sm active-disable-btn" data-skill="${escHtml(skill.id)}" title="Disable this skill">
              ${msIcon('visibility_off')} Disable
            </button>
          </div>`;
    }

    html += `
        </div>
      </div>
    </div>`;
  }

  return html;
}

// ── Event binding ──────────────────────────────────────────────────────

export function bindActiveTabEvents(): void {
  // Toggle collapsed section
  const toggle = document.getElementById('active-unavailable-toggle');
  const body = document.getElementById('active-unavailable-body');
  if (toggle && body) {
    toggle.addEventListener('click', () => {
      const open = body.style.display !== 'none';
      body.style.display = open ? 'none' : 'block';
      const chevron = toggle.querySelector('.active-section-chevron');
      if (chevron) chevron.textContent = open ? 'expand_more' : 'expand_less';
      toggle.closest('.active-section')?.classList.toggle('active-section-collapsed', open);
    });
  }

  // Disable buttons
  document.querySelectorAll('.active-disable-btn').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const skillId = (btn as HTMLElement).dataset.skill;
      if (!skillId) return;
      try {
        const { pawEngine } = await import('../../engine');
        await pawEngine.skillSetEnabled(skillId, false);
        // Remove the row from DOM immediately for snappy UX
        const row = (btn as HTMLElement).closest('.active-unavail-row');
        if (row) {
          row.remove();
          // Update count badge
          const remaining = document.querySelectorAll('.active-unavail-row').length;
          const countEl = document.querySelector(
            '#active-unavailable-toggle .active-section-count',
          );
          if (countEl) countEl.textContent = String(remaining);
          if (remaining === 0) {
            document
              .querySelector('#active-unavailable-toggle')
              ?.closest('.active-section')
              ?.remove();
          }
        }
      } catch (e) {
        console.error('[skills] Disable failed:', e);
      }
    });
  });
}
