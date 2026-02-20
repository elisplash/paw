// Skills View — Plugin Manager
// Extracted from main.ts for maintainability
// NOTE: Skills management requires engine API (not yet implemented)

import { logSecurityEvent } from '../db';

const $ = (id: string) => document.getElementById(id);

// ── Module state ───────────────────────────────────────────────────────────
let wsConnected = false;

export function setWsConnected(connected: boolean) {
  wsConnected = connected;
}

// ── Helpers ────────────────────────────────────────────────────────────────
function escHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

function escAttr(s: string): string {
  return s.replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

let _skillsToastTimer: number | null = null;
function showSkillsToast(message: string, type: 'success' | 'error' | 'info') {
  const toast = $('skills-toast');
  if (!toast) return;
  toast.className = `skills-toast ${type}`;
  toast.textContent = message;
  toast.style.display = 'flex';

  if (_skillsToastTimer) clearTimeout(_skillsToastTimer);
  _skillsToastTimer = window.setTimeout(() => {
    toast.style.display = 'none';
    _skillsToastTimer = null;
  }, type === 'error' ? 8000 : 4000);
}

// ── Main loader ────────────────────────────────────────────────────────────
export async function loadSkills() {
  const installed = $('skills-installed-list');
  const available = $('skills-available-list');
  const availableSection = $('skills-available-section');
  const empty = $('skills-empty');
  const loading = $('skills-loading');
  if (!wsConnected) return;

  if (loading) loading.style.display = '';
  if (empty) empty.style.display = 'none';
  if (installed) installed.innerHTML = '';
  if (available) available.innerHTML = '';
  if (availableSection) availableSection.style.display = 'none';

  try {
    // Skills API not yet available in engine mode
    if (loading) loading.style.display = 'none';
    if (empty) {
      empty.style.display = 'flex';
      empty.innerHTML = '<div class="empty-title">Skills</div><div class="empty-subtitle">Skill management coming soon to the Paw engine</div>';
    }
    return;
    const result = { skills: [] } as { skills: any[] }; // stub
    if (false) { // unreachable — kept for type checker
    }
    const skills = result.skills ?? [];
    if (!skills.length) {
      if (empty) empty!.style.display = 'flex';
      return;
    }

    for (const skill of skills) {
      const card = document.createElement('div');
      card.className = 'skill-card';

      const isEnabled = !skill.disabled;
      const hasMissingBins = (skill.missing?.bins?.length ?? 0) > 0
        || (skill.missing?.anyBins?.length ?? 0) > 0
        || (skill.missing?.os?.length ?? 0) > 0;
      const hasMissingEnv = (skill.missing?.env?.length ?? 0) > 0;
      const hasMissingConfig = (skill.missing?.config?.length ?? 0) > 0;
      const isInstalled = skill.always || (!hasMissingBins && !hasMissingEnv && !hasMissingConfig);
      const needsSetup = !hasMissingBins && (hasMissingEnv || hasMissingConfig);
      const hasEnvRequirements = (skill.requirements?.env?.length ?? 0) > 0;
      const installOptions = skill.install ?? [];

      if (needsSetup) card.className += ' needs-setup';

      const statusLabel = isInstalled
        ? (isEnabled ? 'Enabled' : 'Disabled')
        : needsSetup ? 'Needs Setup' : 'Available';
      const statusClass = isInstalled
        ? (isEnabled ? 'connected' : 'muted')
        : needsSetup ? 'warning' : 'muted';

      const installSpecId = installOptions[0]?.id ?? '';
      const installLabel = installOptions[0]?.label ?? 'Install';

      const skillDataAttr = escAttr(JSON.stringify({
        name: skill.name,
        skillKey: skill.skillKey ?? skill.name,
        description: skill.description ?? '',
        primaryEnv: skill.primaryEnv,
        requiredEnv: skill.requirements?.env ?? [],
        missingEnv: skill.missing?.env ?? [],
        homepage: skill.homepage,
      }));

      card.innerHTML = `
        <div class="skill-card-header">
          <span class="skill-card-name">${skill.emoji ? escHtml(skill.emoji) + ' ' : ''}${escHtml(skill.name)}</span>
          <span class="status-badge ${statusClass}">${statusLabel}</span>
        </div>
        <div class="skill-card-desc">${escHtml(skill.description ?? '')}</div>
        ${needsSetup ? `<div class="skill-config-missing">Needs API key${(skill.missing?.env?.length ?? 0) > 1 ? 's' : ''}: ${escHtml((skill.missing?.env ?? []).join(', '))}</div>` : ''}
        <div class="skill-card-footer">
          <div style="display:flex;align-items:center;gap:8px">
            ${skill.homepage ? `<a class="skill-card-link" href="${escAttr(skill.homepage)}" target="_blank">docs ↗</a>` : ''}
          </div>
          <div class="skill-card-actions">
            ${isInstalled ? `
              ${hasEnvRequirements ? `<button class="btn btn-ghost btn-sm skill-configure" data-skill='${skillDataAttr}' title="Configure">Configure</button>` : ''}
              <button class="skill-toggle ${isEnabled ? 'enabled' : ''}" data-skill-key="${escAttr(skill.skillKey ?? skill.name)}" data-name="${escAttr(skill.name)}" data-enabled="${isEnabled}" title="${isEnabled ? 'Disable' : 'Enable'}"></button>
            ` : needsSetup ? `
              <button class="btn btn-primary btn-sm skill-configure" data-skill='${skillDataAttr}'>Setup</button>
            ` : installOptions.length > 0 ? `
              <button class="btn btn-primary btn-sm skill-install" data-name="${escAttr(skill.name)}" data-install-id="${escAttr(installSpecId)}">${escHtml(installLabel)}</button>
            ` : `
              <span class="status-badge muted">No installer</span>
            `}
          </div>
        </div>
      `;
      if (isInstalled) {
        installed?.appendChild(card);
      } else {
        if (availableSection) availableSection!.style.display = '';
        available?.appendChild(card);
      }
    }

    wireSkillActions();
  } catch (e) {
    console.warn('Skills load failed:', e);
    if (loading) loading.style.display = 'none';
    if (empty) empty.style.display = 'flex';
    showSkillsToast(`Failed to load skills: ${e}`, 'error');
  }
}

// ── B2: Skill Safety Confirmation ──────────────────────────────────────────

// ── H2: npm registry risk intelligence ─────────────────────────────────────

interface NpmPackageInfo {
  name: string;
  weeklyDownloads: number | null;
  lastPublishDate: string | null;
  lastPublishDays: number | null;
  deprecated: string | null;
  license: string | null;
  version: string | null;
  maintainerCount: number;
  hasTypes: boolean;
}

async function fetchNpmPackageInfo(packageName: string): Promise<NpmPackageInfo | null> {
  try {
    // Fetch package metadata from npm registry
    const resp = await fetch(`https://registry.npmjs.org/${encodeURIComponent(packageName)}`, {
      signal: AbortSignal.timeout(5000),
    });
    if (!resp.ok) return null;
    const data = await resp.json();

    const latest = data['dist-tags']?.latest;
    const latestVersion = latest ? data.versions?.[latest] : null;
    const time = data.time ?? {};
    const lastPublishDate = time[latest] ?? time.modified ?? null;

    let lastPublishDays: number | null = null;
    if (lastPublishDate) {
      lastPublishDays = Math.floor((Date.now() - new Date(lastPublishDate).getTime()) / 86400000);
    }

    const info: NpmPackageInfo = {
      name: data.name ?? packageName,
      weeklyDownloads: null,
      lastPublishDate,
      lastPublishDays,
      deprecated: latestVersion?.deprecated ?? null,
      license: latestVersion?.license ?? data.license ?? null,
      version: latest ?? null,
      maintainerCount: (data.maintainers ?? []).length,
      hasTypes: !!(latestVersion?.types || latestVersion?.typings),
    };

    // Fetch download counts (separate API)
    try {
      const dlResp = await fetch(`https://api.npmjs.org/downloads/point/last-week/${encodeURIComponent(packageName)}`, {
        signal: AbortSignal.timeout(3000),
      });
      if (dlResp.ok) {
        const dlData = await dlResp.json();
        info.weeklyDownloads = dlData.downloads ?? null;
      }
    } catch { /* non-critical — skip download count */ }

    return info;
  } catch (e) {
    console.warn('[skills] npm registry fetch failed:', e);
    return null;
  }
}

function buildRiskScoreHtml(info: NpmPackageInfo): string {
  const rows: string[] = [];

  if (info.version) {
    rows.push(`<span class="npm-risk-item">v${escHtml(info.version)}</span>`);
  }
  if (info.weeklyDownloads !== null) {
    const fmt = info.weeklyDownloads >= 1000
      ? `${(info.weeklyDownloads / 1000).toFixed(info.weeklyDownloads >= 10000 ? 0 : 1)}k`
      : String(info.weeklyDownloads);
    rows.push(`<span class="npm-risk-item">${fmt}/week</span>`);
  }
  if (info.lastPublishDays !== null) {
    const label = info.lastPublishDays > 365
      ? `${Math.floor(info.lastPublishDays / 365)}y ago`
      : `${info.lastPublishDays}d ago`;
    rows.push(`<span class="npm-risk-item">${label}</span>`);
  }
  if (info.license) {
    rows.push(`<span class="npm-risk-item">${escHtml(info.license)}</span>`);
  }
  if (info.maintainerCount > 0) {
    rows.push(`<span class="npm-risk-item">${info.maintainerCount} maintainer${info.maintainerCount > 1 ? 's' : ''}</span>`);
  }

  if (!rows.length) return '';
  return `<div class="npm-risk-score">${rows.join('')}</div>`;
}

/** H2: Post-install sandbox check — verify the skill after installation */
async function runPostInstallSandboxCheck(skillName: string): Promise<void> {
  try {
    const status = { skills: [] } as any; // stub — engine skills API coming soon
    const skills = status.skills ?? [];
    const installed = skills.find((s: Record<string, unknown>) =>
      (s.name as string)?.toLowerCase() === skillName.toLowerCase() ||
      (s.skillKey as string)?.toLowerCase() === skillName.toLowerCase()
    );
    if (!installed) return;

    const warnings: string[] = [];

    // Check for suspicious tool registrations
    const tools = (installed.tools ?? installed.capabilities ?? []) as string[];
    const dangerousTools = ['exec', 'shell', 'eval', 'spawn', 'process', 'system'];
    for (const t of tools) {
      const tStr = typeof t === 'string' ? t : JSON.stringify(t);
      for (const dt of dangerousTools) {
        if (tStr.toLowerCase().includes(dt)) {
          warnings.push(`Registers tool with suspicious name: "${tStr}"`);
          break;
        }
      }
    }

    // Check for network/filesystem capabilities
    const caps = (installed.requiredCapabilities ?? installed.permissions ?? []) as string[];
    for (const c of caps) {
      const cStr = typeof c === 'string' ? c : JSON.stringify(c);
      if (/network|http|fetch|socket/i.test(cStr)) {
        warnings.push(`Requests network access: ${cStr}`);
      }
      if (/filesystem|write|disk|file/i.test(cStr)) {
        warnings.push(`Requests filesystem write access: ${cStr}`);
      }
    }

    if (warnings.length > 0) {
      showSkillsToast(`Post-install check for ${skillName}: ${warnings[0]}`, 'error');
      logSecurityEvent({
        eventType: 'skill_sandbox_check',
        riskLevel: 'medium',
        toolName: skillName,
        command: `skills.sandbox_check ${skillName}`,
        detail: `Post-install warnings: ${warnings.join('; ')}`,
        wasAllowed: true,
      }).catch(() => {});
    }
  } catch {
    // Non-critical — skill may not expose this metadata
  }
}

/** Known safe / trusted skill packages (community-vetted) */
const KNOWN_SAFE_SKILLS = new Set([
  'web-search', 'web-browse', 'web-scrape',
  'memory', 'memory-lancedb',
  'filesystem', 'shell-exec',
  'git', 'github', 'gitlab',
  'docker', 'kubernetes',
  'postgres', 'mysql', 'sqlite', 'redis',
  'fetch', 'http', 'rest-api',
  'puppeteer', 'playwright',
  'slack', 'discord', 'telegram',
]);

/**
 * Show a safety confirmation dialog before installing a skill.
 * Returns true if user confirms, false if cancelled.
 */
async function showSkillSafetyConfirm(skillName: string, installId: string): Promise<boolean> {
  const isKnown = KNOWN_SAFE_SKILLS.has(skillName.toLowerCase());
  const isNpmPkg = installId.includes('/') || installId.startsWith('@') || installId.match(/^[a-z][\w-]*$/i);

  // Build safety checks
  const checks: Array<{ label: string; status: 'pass' | 'warn' | 'fail' }> = [];

  if (isKnown) {
    checks.push({ label: 'Known community skill', status: 'pass' });
  } else {
    checks.push({ label: 'Unrecognized skill — not in known-safe list', status: 'warn' });
  }

  if (isNpmPkg) {
    checks.push({ label: 'npm package — runs install scripts', status: 'warn' });
  }

  checks.push({ label: 'Will have access to agent tool calls', status: 'warn' });

  // ── H2: npm registry risk score ──
  let riskScoreHtml = '';
  if (isNpmPkg) {
    const npmInfo = await fetchNpmPackageInfo(installId.replace(/^@/, ''));
    if (npmInfo) {
      if (npmInfo.deprecated) {
        checks.unshift({ label: `DEPRECATED: ${npmInfo.deprecated}`, status: 'fail' });
      }
      if (npmInfo.weeklyDownloads !== null && npmInfo.weeklyDownloads < 100) {
        checks.push({ label: `Low download count (${npmInfo.weeklyDownloads}/week)`, status: 'warn' });
      } else if (npmInfo.weeklyDownloads !== null && npmInfo.weeklyDownloads > 10000) {
        checks.push({ label: `Popular (${(npmInfo.weeklyDownloads / 1000).toFixed(0)}k downloads/week)`, status: 'pass' });
      }
      if (npmInfo.lastPublishDays !== null) {
        if (npmInfo.lastPublishDays > 365) {
          checks.push({ label: `Last published ${Math.floor(npmInfo.lastPublishDays / 365)}y ago — may be unmaintained`, status: 'warn' });
        } else {
          checks.push({ label: `Last published ${npmInfo.lastPublishDays}d ago`, status: 'pass' });
        }
      }
      riskScoreHtml = buildRiskScoreHtml(npmInfo);
    }
  }

  const checksHtml = checks.map(c => {
    const icon = c.status === 'pass' ? '✓' : c.status === 'warn' ? '!' : '✗';
    return `<div class="skill-safety-check"><span class="check-${c.status}">${icon}</span> ${escHtml(c.label)}</div>`;
  }).join('');

  // Use the prompt modal for confirmation
  return new Promise<boolean>((resolve) => {
    const promptModal = $('prompt-modal');
    const promptInput = $('prompt-modal-input') as HTMLInputElement | null;
    const promptTitle = $('prompt-modal-title');
    const promptOk = $('prompt-modal-ok');
    const promptClose = $('prompt-modal-close');
    const promptCancel = $('prompt-modal-cancel');
    const promptBody = promptModal?.querySelector('.modal-body');

    if (!promptModal || !promptOk || !promptBody) {
      // Fallback: native confirm
      const ok = confirm(`Install skill "${skillName}"?\n\nThis will download and install a package that can execute code on your machine. Only install skills you trust.`);
      resolve(ok);
      return;
    }

    if (promptTitle) promptTitle.textContent = `Install "${skillName}"?`;
    
    // Replace the modal body temporarily
    const originalBody = promptBody.innerHTML;
    promptBody.innerHTML = `
      <div class="skill-safety-banner">
        <div class="safety-icon"><span class="ms">shield</span></div>
        <div>
          <strong>Skill Safety Review</strong>
          <p style="margin:4px 0 0">This will download and install <strong>${escHtml(skillName)}</strong> on your machine. Skills can execute code and access system resources.</p>
          <div class="skill-safety-checks">${checksHtml}</div>
          ${riskScoreHtml}
        </div>
      </div>
      <p style="font-size:12px;color:var(--text-muted);margin-top:12px">Only install skills from sources you trust. If unsure, review the skill's documentation first.</p>
    `;

    if (promptInput) promptInput.style.display = 'none';

    promptModal.style.display = 'flex';

    const cleanup = () => {
      promptModal.style.display = 'none';
      promptBody.innerHTML = originalBody;
      if (promptInput) promptInput.style.display = '';
      promptOk.removeEventListener('click', onOk);
      promptClose?.removeEventListener('click', onCancel);
      promptCancel?.removeEventListener('click', onCancel);
    };

    const onOk = () => { cleanup(); resolve(true); };
    const onCancel = () => { cleanup(); resolve(false); };

    promptOk.addEventListener('click', onOk);
    promptClose?.addEventListener('click', onCancel);
    promptCancel?.addEventListener('click', onCancel);
  });
}

function wireSkillActions() {
  // Install buttons
  document.querySelectorAll('.skill-install').forEach(btn => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const name = (btn as HTMLElement).dataset.name!;
      const installId = (btn as HTMLElement).dataset.installId!;
      if (!installId) {
        showSkillsToast(`No installer available for ${name}`, 'error');
        return;
      }

      // ── B2: Skill vetting — safety confirmation ──
      const safetyOk = await showSkillSafetyConfirm(name, installId);
      if (!safetyOk) {
        logSecurityEvent({
          eventType: 'skill_install',
          riskLevel: 'medium',
          toolName: name,
          command: `skills.install ${name}`,
          detail: `User cancelled skill install: ${name}`,
          wasAllowed: false,
        }).catch(() => {});
        return;
      }

      (btn as HTMLButtonElement).disabled = true;
      (btn as HTMLButtonElement).textContent = 'Installing…';
      showSkillsToast(`Installing ${name}…`, 'info');
      try {
        showSkillsToast('Skill installation coming soon to the Paw engine', 'info');
        return;
        void name; void installId; // stub
        logSecurityEvent({
          eventType: 'skill_install',
          toolName: name,
          command: `skills.install ${name}`,
          detail: `Skill installed: ${name}`,
          wasAllowed: true,
        }).catch(() => {});

        // ── H2: Post-install sandbox check ──
        runPostInstallSandboxCheck(name).catch(() => {});

        await loadSkills();
      } catch (e) {
        showSkillsToast(`Install failed for ${name}: ${e}`, 'error');
        (btn as HTMLButtonElement).disabled = false;
        (btn as HTMLButtonElement).textContent = 'Install';
      }
    });
  });

  // Enable/disable toggles
  document.querySelectorAll('.skill-toggle').forEach(btn => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const skillKey = (btn as HTMLElement).dataset.skillKey!;
      const name = (btn as HTMLElement).dataset.name ?? skillKey;
      const currentlyEnabled = (btn as HTMLElement).dataset.enabled === 'true';
      const newState = !currentlyEnabled;

      (btn as HTMLButtonElement).disabled = true;
      try {
        showSkillsToast('Skill management coming soon to the Paw engine', 'info');
        return;
        void skillKey; void newState; // stub
        await loadSkills();
      } catch (e) {
        showSkillsToast(`Failed to ${newState ? 'enable' : 'disable'} ${name}: ${e}`, 'error');
        (btn as HTMLButtonElement).disabled = false;
      }
    });
  });

  // Configure / Setup buttons
  document.querySelectorAll('.skill-configure').forEach(btn => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const raw = (btn as HTMLElement).dataset.skill;
      if (!raw) return;
      try {
        const data = JSON.parse(raw);
        openSkillConfigModal(data);
      } catch { /* ignore parse errors */ }
    });
  });
}

// ── Skill config modal ─────────────────────────────────────────────────────
interface SkillConfigData {
  name: string;
  skillKey: string;
  description: string;
  primaryEnv?: string;
  requiredEnv: string[];
  missingEnv: string[];
  homepage?: string;
}

let _activeSkillConfig: SkillConfigData | null = null;

function openSkillConfigModal(data: SkillConfigData) {
  const modal = $('skill-config-modal');
  const title = $('skill-config-title');
  const desc = $('skill-config-desc');
  const fields = $('skill-config-fields');
  if (!modal || !fields) return;

  _activeSkillConfig = data;

  if (title) title.textContent = `Configure ${data.name}`;
  if (desc) {
    const parts: string[] = [];
    if (data.description) parts.push(data.description);
    if (data.homepage) parts.push(`<a href="${escAttr(data.homepage)}" target="_blank" style="color:var(--accent)">View docs ↗</a>`);
    desc.innerHTML = parts.join(' — ');
  }

  const envVars = data.requiredEnv.length > 0 ? data.requiredEnv : (data.primaryEnv ? [data.primaryEnv] : []);
  fields.innerHTML = envVars.map(envName => {
    const isMissing = data.missingEnv.includes(envName);
    const isPrimary = envName === data.primaryEnv;
    return `
      <div class="skill-config-field">
        <label for="skill-env-${escAttr(envName)}">${escHtml(envName)}${isMissing ? ' <span style="color:var(--warning)">(not set)</span>' : ' <span style="color:var(--success)">✓</span>'}</label>
        <input type="password" id="skill-env-${escAttr(envName)}" class="form-input"
          data-env-name="${escAttr(envName)}"
          data-is-primary="${isPrimary}"
          placeholder="${isPrimary ? 'Enter your API key' : `Enter value for ${envName}`}"
          autocomplete="off" spellcheck="false">
        <div class="field-hint">${isPrimary ? 'This is the main API key for this skill.' : 'Required environment variable.'} Leave blank to keep current value.</div>
      </div>
    `;
  }).join('');

  modal.style.display = 'flex';
}

function closeSkillConfigModal() {
  const modal = $('skill-config-modal');
  if (modal) modal.style.display = 'none';
  _activeSkillConfig = null;
}

async function saveSkillConfig() {
  if (!_activeSkillConfig) return;
  const fields = $('skill-config-fields');
  if (!fields) return;

  const data = _activeSkillConfig;
  const inputs = fields.querySelectorAll<HTMLInputElement>('input[data-env-name]');

  const env: Record<string, string> = {};
  let apiKey: string | undefined;

  inputs.forEach(input => {
    const envName = input.dataset.envName!;
    const value = input.value.trim();
    if (!value) return;

    if (input.dataset.isPrimary === 'true') {
      apiKey = value;
    } else {
      env[envName] = value;
    }
  });

  if (!apiKey && Object.keys(env).length === 0) {
    showSkillsToast('No values entered — nothing to save', 'info');
    return;
  }

  const saveBtn = $('skill-config-save') as HTMLButtonElement | null;
  if (saveBtn) { saveBtn.disabled = true; saveBtn.textContent = 'Saving…'; }

  try {
    const updates: { enabled?: boolean; apiKey?: string; env?: Record<string, string> } = {};
    if (apiKey) updates.apiKey = apiKey;
    if (Object.keys(env).length > 0) updates.env = env;

    void updates; // stub — engine skills API coming soon
    showSkillsToast('Skill configuration coming soon to the Paw engine', 'info');
    return;
    showSkillsToast(`${data.name} configured successfully!`, 'success');
    closeSkillConfigModal();
    await loadSkills();
  } catch (e) {
    showSkillsToast(`Failed to configure ${data.name}: ${e}`, 'error');
  } finally {
    if (saveBtn) { saveBtn.disabled = false; saveBtn.textContent = 'Save'; }
  }
}

// ── Event wiring ───────────────────────────────────────────────────────────
export function initSkillsEvents() {
  $('skill-config-close')?.addEventListener('click', closeSkillConfigModal);
  $('skill-config-cancel')?.addEventListener('click', closeSkillConfigModal);
  $('skill-config-save')?.addEventListener('click', saveSkillConfig);
  $('skill-config-modal')?.addEventListener('click', (e) => {
    if (e.target === e.currentTarget) closeSkillConfigModal();
  });

  $('refresh-skills-btn')?.addEventListener('click', () => loadSkills());

  // Bins modal
  $('skills-browse-bins')?.addEventListener('click', async () => {
    const backdrop = $('bins-modal-backdrop');
    const list = $('bins-list');
    const loading = $('bins-loading');
    const empty = $('bins-empty');
    if (!backdrop || !list) return;

    backdrop.style.display = 'flex';
    list.innerHTML = '';
    if (loading) loading.style.display = '';
    if (empty) empty.style.display = 'none';

    try {
      const result = { bins: [] } as { bins: string[] }; // stub — engine skills API coming soon
      if (loading) loading.style.display = 'none';
      const bins = result.bins ?? [];
      if (!bins.length) {
        if (empty) empty.style.display = '';
        return;
      }

      for (const bin of bins) {
        const item = document.createElement('div');
        item.className = 'bins-item';
        item.innerHTML = `
          <span class="bins-item-name">${escHtml(bin)}</span>
          <button class="btn btn-primary btn-sm bins-item-install" data-name="${escAttr(bin)}">Install</button>
        `;
        list.appendChild(item);
      }

      list.querySelectorAll('.bins-item-install').forEach(btn => {
        btn.addEventListener('click', async () => {
          const name = (btn as HTMLElement).dataset.name!;
          (btn as HTMLButtonElement).disabled = true;
          (btn as HTMLButtonElement).textContent = 'Installing…';
          try {
            showSkillsToast('Skill installation coming soon to the Paw engine', 'info'); return;
            (btn as HTMLButtonElement).textContent = 'Installed';
            showSkillsToast(`${name} installed!`, 'success');
            loadSkills();
          } catch (e) {
            (btn as HTMLButtonElement).textContent = 'Failed';
            showSkillsToast(`Install failed: ${e}`, 'error');
            setTimeout(() => {
              (btn as HTMLButtonElement).textContent = 'Install';
              (btn as HTMLButtonElement).disabled = false;
            }, 2000);
          }
        });
      });
    } catch (e) {
      if (loading) loading.style.display = 'none';
      if (empty) { empty.style.display = ''; empty.textContent = `Failed to load bins: ${e}`; }
    }
  });

  $('bins-modal-close')?.addEventListener('click', () => {
    const backdrop = $('bins-modal-backdrop');
    if (backdrop) backdrop.style.display = 'none';
  });

  $('bins-modal-backdrop')?.addEventListener('click', (e) => {
    if (e.target === e.currentTarget) {
      (e.target as HTMLElement).style.display = 'none';
    }
  });

  $('bins-custom-install')?.addEventListener('click', async () => {
    const input = $('bins-custom-name') as HTMLInputElement | null;
    const btn = $('bins-custom-install') as HTMLButtonElement | null;
    if (!input || !btn) return;

    const name = input.value.trim();
    if (!name) { input.focus(); return; }

    btn.disabled = true;
    btn.textContent = 'Installing…';

    try {
      showSkillsToast('Skill installation coming soon to the Paw engine', 'info');
      return;
      input!.value = '';
      loadSkills();
      const backdrop = $('bins-modal-backdrop');
      if (backdrop) backdrop!.style.display = 'none';
    } catch (e) {
      showSkillsToast(`Install failed for "${name}": ${e}`, 'error');
    } finally {
      btn.disabled = false;
      btn.textContent = 'Install';
    }
  });
}
