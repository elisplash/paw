// Built In — Native Rust Engine Tools
// Shows all engine-compiled skills (EngineSkillStatus) categorised by readiness.
// These are the low-level system tools that integrations/n8n cannot do.

import { pawEngine, type EngineSkillStatus } from '../../engine';
import { isEngineMode } from '../../engine-bridge';
import { $, escHtml } from '../../components/helpers';
import { renderSkillCard, fromEngineSkill } from '../../components/molecules/skill-card';

// ── Atoms ──────────────────────────────────────────────────────────────

function msIcon(name: string, cls = ''): string {
  return `<span class="ms${cls ? ` ${cls}` : ''}">${name}</span>`;
}

// ── Hero stats ─────────────────────────────────────────────────────────

function updateHeroStats(total: number, ready: number, issues: number): void {
  const elTotal = $('builtin-stat-total');
  const elReady = $('builtin-stat-ready');
  const elIssues = $('builtin-stat-issues');
  if (elTotal) elTotal.textContent = String(total);
  if (elReady) elReady.textContent = String(ready);
  if (elIssues) elIssues.textContent = String(issues);
}

// ── Render ─────────────────────────────────────────────────────────────

function renderBuiltIn(skills: EngineSkillStatus[]): string {
  // Categorise all engine skills by status
  const ready = skills.filter((s) => s.enabled && s.is_ready);
  const needsCreds = skills.filter(
    (s) =>
      s.enabled &&
      !s.is_ready &&
      s.missing_binaries.length === 0 &&
      s.missing_credentials.length > 0,
  );
  const needsEnv = skills.filter(
    (s) =>
      s.enabled &&
      !s.is_ready &&
      s.missing_binaries.length === 0 &&
      s.missing_credentials.length === 0 &&
      s.missing_env_vars.length > 0,
  );
  const missingBinaries = skills.filter(
    (s) => s.enabled && !s.is_ready && s.missing_binaries.length > 0,
  );
  const disabled = skills.filter((s) => !s.enabled);
  const needsSetup = [...needsCreds, ...needsEnv];

  if (skills.length === 0) {
    return `
    <div style="text-align:center;padding:48px 24px">
      <span class="ms" style="font-size:48px;opacity:0.3;display:block;margin-bottom:12px">memory</span>
      <h3 style="margin:0 0 8px;font-size:16px;font-weight:600;color:var(--text-primary)">No built-in tools found</h3>
      <p style="color:var(--text-muted);font-size:13px;margin:0">
        The engine didn't report any native tools. This may indicate a connection issue.
      </p>
    </div>`;
  }

  let html = '';

  // ── Ready & Working ──────────────────────────────────────────────
  if (ready.length > 0) {
    html += `
    <div class="active-section">
      <div class="active-section-header">
        <span class="active-section-dot active-dot-ready"></span>
        <span class="active-section-title">Ready &amp; Working</span>
        <span class="active-section-count">${ready.length}</span>
      </div>
      <div class="skills-card-grid">`;

    for (const skill of ready) {
      const cardData = fromEngineSkill(skill);
      cardData.action = { type: 'none' };
      html += renderSkillCard(cardData);
    }

    html += '</div></div>';
  }

  // ── Needs Setup ──────────────────────────────────────────────────
  if (needsSetup.length > 0) {
    html += `
    <div class="active-section">
      <div class="active-section-header">
        <span class="active-section-dot active-dot-setup"></span>
        <span class="active-section-title">Needs Setup</span>
        <span class="active-section-count">${needsSetup.length}</span>
      </div>
      <p class="active-section-desc">These tools are enabled but need credentials or environment variables configured.</p>
      <div class="skills-card-grid">`;

    for (const skill of needsSetup) {
      const cardData = fromEngineSkill(skill);
      cardData.action = { type: 'none' };
      html += renderSkillCard(cardData);
    }

    html += '</div></div>';
  }

  // ── Platform Unavailable ─────────────────────────────────────────
  if (missingBinaries.length > 0) {
    html += `
    <div class="active-section active-section-collapsed">
      <div class="active-section-header active-section-toggle" id="builtin-unavailable-toggle">
        <span class="active-section-dot active-dot-unavail"></span>
        <span class="active-section-title">Platform Unavailable</span>
        <span class="active-section-count">${missingBinaries.length}</span>
        <span class="ms active-section-chevron ms-sm">expand_more</span>
      </div>
      <p class="active-section-desc">
        These tools require binaries not found on this system (e.g. macOS-only apps).
        They're enabled but can't run here. You can disable them to clean up this list.
      </p>
      <div class="active-section-body" id="builtin-unavailable-body" style="display:none">
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
            <button class="btn btn-ghost btn-sm builtin-disable-btn" data-skill="${escHtml(skill.id)}" title="Disable this tool">
              ${msIcon('visibility_off')} Disable
            </button>
          </div>`;
    }

    html += '</div></div></div>';
  }

  // ── Disabled ─────────────────────────────────────────────────────
  if (disabled.length > 0) {
    html += `
    <div class="active-section active-section-collapsed">
      <div class="active-section-header active-section-toggle" id="builtin-disabled-toggle">
        <span class="active-section-dot" style="background:var(--border)"></span>
        <span class="active-section-title">Disabled</span>
        <span class="active-section-count">${disabled.length}</span>
        <span class="ms active-section-chevron ms-sm">expand_more</span>
      </div>
      <div class="active-section-body" id="builtin-disabled-body" style="display:none">
        <div class="active-unavail-grid">`;

    for (const skill of disabled) {
      html += `
          <div class="active-unavail-row" data-skill-id="${escHtml(skill.id)}">
            <div class="active-unavail-info">
              <strong>${escHtml(skill.name)}</strong>
              <span style="font-size:11px;color:var(--text-muted)">${escHtml(skill.description)}</span>
            </div>
            <button class="btn btn-primary btn-sm builtin-enable-btn" data-skill="${escHtml(skill.id)}" title="Enable this tool">
              ${msIcon('visibility')} Enable
            </button>
          </div>`;
    }

    html += '</div></div></div>';
  }

  return html;
}

// ── Event binding ──────────────────────────────────────────────────────

function bindBuiltInEvents(): void {
  // Collapsible toggles
  const bindToggle = (toggleId: string, bodyId: string) => {
    const toggle = document.getElementById(toggleId);
    const body = document.getElementById(bodyId);
    if (toggle && body) {
      toggle.addEventListener('click', () => {
        const open = body.style.display !== 'none';
        body.style.display = open ? 'none' : 'block';
        const chevron = toggle.querySelector('.active-section-chevron');
        if (chevron) chevron.textContent = open ? 'expand_more' : 'expand_less';
        toggle.closest('.active-section')?.classList.toggle('active-section-collapsed', open);
      });
    }
  };

  bindToggle('builtin-unavailable-toggle', 'builtin-unavailable-body');
  bindToggle('builtin-disabled-toggle', 'builtin-disabled-body');

  // Disable buttons
  document.querySelectorAll('.builtin-disable-btn').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const skillId = (btn as HTMLElement).dataset.skill;
      if (!skillId) return;
      try {
        await pawEngine.skillSetEnabled(skillId, false);
        const row = (btn as HTMLElement).closest('.active-unavail-row');
        if (row) row.remove();
      } catch (e) {
        console.error('[built-in] Disable failed:', e);
      }
    });
  });

  // Enable buttons
  document.querySelectorAll('.builtin-enable-btn').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const skillId = (btn as HTMLElement).dataset.skill;
      if (!skillId) return;
      try {
        await pawEngine.skillSetEnabled(skillId, true);
        await loadBuiltIn(); // full reload to re-categorise
      } catch (e) {
        console.error('[built-in] Enable failed:', e);
      }
    });
  });
}

// ── Main loader ────────────────────────────────────────────────────────

export async function loadBuiltIn(): Promise<void> {
  const loading = $('builtin-loading');
  const list = $('builtin-list');

  if (!isEngineMode()) {
    if (loading) loading.textContent = 'Pawz engine is required.';
    if (list) list.innerHTML = '';
    return;
  }

  try {
    if (loading) loading.style.display = '';

    const skills = await pawEngine.skillsList();

    if (loading) loading.style.display = 'none';

    // Stats
    const ready = skills.filter((s) => s.enabled && s.is_ready).length;
    const issues = skills.filter((s) => s.enabled && !s.is_ready).length;
    updateHeroStats(skills.length, ready, issues);

    // Render
    if (list) {
      list.innerHTML = renderBuiltIn(skills);
      bindBuiltInEvents();
    }

    // Kinetic stagger on side panel cards
    const view = document.getElementById('builtin-view');
    if (view) {
      const cards = view.querySelectorAll(
        '.builtin-panel-card, .builtin-side-panel > .skills-panel-card',
      );
      cards.forEach((card, i) => {
        (card as HTMLElement).style.animationDelay = `${i * 60}ms`;
      });
    }

    // Refresh button
    $('refresh-builtin-btn')?.addEventListener('click', () => loadBuiltIn());
  } catch (e) {
    console.error('[built-in] Load failed:', e);
    if (loading) loading.textContent = `Failed to load: ${e}`;
  }
}
