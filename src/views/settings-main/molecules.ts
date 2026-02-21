// Settings View — Molecules (DOM rendering + IPC)

import {
  loadSecuritySettings,
  saveSecuritySettings,
  getSessionOverrideRemaining,
  resetSecuritySettings,
  type SecuritySettings,
} from '../../security';
import { getSecurityAuditLog, isEncryptionReady } from '../../db';
import { $, escHtml } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { isConnected } from '../../state/connection';
import { getBudgetLimit, setBudgetLimit, downloadFile, type ToolRule } from './atoms';

// ── State accessors (set by index.ts) ──────────────────────────────────────

interface MoleculesState {
  getToolRules: () => ToolRule[];
  setToolRules: (rules: ToolRule[]) => void;
  pushToolRule: (rule: ToolRule) => void;
  spliceToolRule: (idx: number) => void;
  getOverrideBannerInterval: () => ReturnType<typeof setInterval> | null;
  setOverrideBannerInterval: (v: ReturnType<typeof setInterval> | null) => void;
}

let _state: MoleculesState;

export function setMoleculesState(s: MoleculesState) {
  _state = s;
}

// ── Engine Status ──────────────────────────────────────────────────────────

export async function loadSettingsStatus() {
  if (!isConnected()) return;
  const section = $('settings-status-section');
  const content = $('settings-status-content');
  if (section) section.style.display = '';
  if (content)
    content.innerHTML =
      '<div class="status-card"><div class="status-card-label">Runtime</div><div class="status-card-value">Paw Engine (Tauri)</div></div>';
}

// ── Logs Viewer ────────────────────────────────────────────────────────────

export async function loadSettingsLogs() {
  if (!isConnected()) return;
  const section = $('settings-logs-section');
  const output = $('settings-logs-output');
  if (section) section.style.display = '';
  if (output)
    output.textContent = '(Engine logs viewer coming soon — check the Tauri console for now)';
}

// ── Usage Dashboard ────────────────────────────────────────────────────────

export async function loadSettingsUsage() {
  if (!isConnected()) return;
  const section = $('settings-usage-section');
  const content = $('settings-usage-content');
  if (section) section.style.display = '';
  if (content)
    content.innerHTML = `<div class="usage-empty-state">
    <p style="color:var(--text-secondary);margin:0 0 8px">Usage tracking coming soon to the Paw engine.</p>
    <p style="color:var(--text-muted);margin:0;font-size:12px">Token tracking is active in chat — send a message to see per-session estimates in the chat header.</p>
  </div>`;
}

// ── Budget Alert ───────────────────────────────────────────────────────────

// @ts-ignore: reserved for budget alert feature
function _checkBudgetAlert(currentCost: number) {
  void currentCost;
  const limit = getBudgetLimit();
  if (limit == null) return;
  const alertEl = $('budget-alert');
  if (!alertEl) return;

  if (currentCost >= limit) {
    alertEl.style.display = '';
    const text = $('budget-alert-text');
    if (text)
      text.textContent = `Budget limit reached: $${currentCost.toFixed(4)} / $${limit.toFixed(2)} — consider switching to a cheaper model or pausing automations`;
  } else if (currentCost >= limit * 0.8) {
    alertEl.style.display = '';
    const text = $('budget-alert-text');
    if (text)
      text.textContent = `Approaching budget: $${currentCost.toFixed(4)} / $${limit.toFixed(2)} (${((currentCost / limit) * 100).toFixed(0)}%)`;
  } else {
    alertEl.style.display = 'none';
  }
}

export function initBudgetSettings() {
  const input = $('budget-limit-input') as HTMLInputElement | null;
  const saveBtn = $('budget-limit-save');
  const clearBtn = $('budget-limit-clear');

  if (input) {
    const current = getBudgetLimit();
    if (current != null) input.value = current.toFixed(2);
  }

  saveBtn?.addEventListener('click', () => {
    const val = parseFloat((input as HTMLInputElement)?.value ?? '');
    if (isNaN(val) || val <= 0) {
      showToast('Enter a valid budget amount', 'error');
      return;
    }
    setBudgetLimit(val);
    showToast(`Budget alert set at $${val.toFixed(2)}`, 'success');
    loadSettingsUsage().catch(() => {});
  });

  clearBtn?.addEventListener('click', () => {
    setBudgetLimit(null);
    if (input) (input as HTMLInputElement).value = '';
    const alertEl = $('budget-alert');
    if (alertEl) alertEl.style.display = 'none';
    showToast('Budget alert cleared', 'info');
  });
}

// ── System Presence ────────────────────────────────────────────────────────

export async function loadSettingsPresence() {
  const section = $('settings-presence-section');
  if (section) section.style.display = 'none';
}

// ── Nodes View ─────────────────────────────────────────────────────────────

export async function loadSettingsNodes() {
  const section = $('settings-nodes-section');
  if (section) section.style.display = 'none';
}

// ── Device Pairing ─────────────────────────────────────────────────────────

export async function loadSettingsDevices() {
  const section = $('settings-devices-section');
  if (section) section.style.display = 'none';
}

// ── Onboarding Wizard ──────────────────────────────────────────────────────

export async function loadSettingsWizard() {
  const section = $('settings-wizard-section');
  if (section) section.style.display = 'none';
}

export async function startWizard() {
  showToast('Wizard not available in engine mode', 'info');
}

export async function wizardNext() {
  showToast('Wizard not available in engine mode', 'info');
}

export async function cancelWizard() {
  // no-op
}

// ── Self-Update ────────────────────────────────────────────────────────

export async function runUpdate() {
  showToast('Self-update not available in engine mode — use your package manager', 'info');
}

// ── Browser Control ────────────────────────────────────────────────────

export async function loadSettingsBrowser() {
  const section = $('settings-browser-section');
  if (section) section.style.display = 'none';
}

export async function startBrowser() {
  showToast('Browser control coming soon to the Paw engine', 'info');
}

export async function stopBrowser() {
  // no-op
}

// ── Exec Approvals Config ──────────────────────────────────────────────────

export function renderToolRules() {
  const list = $('approvals-tool-list');
  if (!list) return;

  const toolRules = _state.getToolRules();

  if (toolRules.length === 0) {
    list.innerHTML =
      '<div class="approvals-empty">No tool-specific rules yet. Click "Add rule" to create one.</div>';
    return;
  }

  list.innerHTML = toolRules
    .map(
      (rule, i) => `
    <div class="approvals-tool-row" data-idx="${i}">
      <div class="approvals-tool-name">${escHtml(rule.name)}</div>
      <div class="approvals-toggle-group">
        <button class="approvals-toggle-btn${rule.state === 'allow' ? ' active allow' : ''}" data-state="allow" data-idx="${i}" title="Always allow">
          <span class="ms" style="font-size:14px">check</span>
          Allow
        </button>
        <button class="approvals-toggle-btn${rule.state === 'ask' ? ' active ask' : ''}" data-state="ask" data-idx="${i}" title="Ask each time">
          <span class="ms" style="font-size:14px">help</span>
          Ask
        </button>
        <button class="approvals-toggle-btn${rule.state === 'deny' ? ' active deny' : ''}" data-state="deny" data-idx="${i}" title="Always block">
          <span class="ms" style="font-size:14px">close</span>
          Block
        </button>
      </div>
      <button class="approvals-remove-btn" data-idx="${i}" title="Remove rule">
        <span class="ms" style="font-size:14px">delete</span>
      </button>
    </div>
  `,
    )
    .join('');

  // Wire toggle buttons
  list.querySelectorAll('.approvals-toggle-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      const idx = parseInt(btn.getAttribute('data-idx') ?? '-1', 10);
      const state = btn.getAttribute('data-state') as 'allow' | 'ask' | 'deny';
      const rules = _state.getToolRules();
      if (idx < 0 || idx >= rules.length) return;
      rules[idx].state = state;
      _state.setToolRules(rules);
      renderToolRules();
    });
  });

  // Wire remove buttons
  list.querySelectorAll('.approvals-remove-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      const idx = parseInt(btn.getAttribute('data-idx') ?? '-1', 10);
      const rules = _state.getToolRules();
      if (idx < 0 || idx >= rules.length) return;
      _state.spliceToolRule(idx);
      renderToolRules();
    });
  });
}

export function addToolRule() {
  const promptModal = $('prompt-modal');
  const promptInput = $('prompt-modal-input') as HTMLInputElement | null;
  const promptTitle = $('prompt-modal-title');
  const promptOk = $('prompt-modal-ok');
  const promptClose = $('prompt-modal-close');
  const promptCancel = $('prompt-modal-cancel');
  if (promptModal && promptInput && promptOk && promptClose) {
    if (promptTitle) promptTitle.textContent = 'Add Tool Rule';
    promptInput.value = '';
    promptInput.placeholder = 'Tool name, e.g. brave_search';
    promptModal.style.display = 'flex';
    promptInput.focus();

    const cleanup = () => {
      promptModal.style.display = 'none';
      promptOk.removeEventListener('click', onOk);
      promptClose.removeEventListener('click', onCancel);
      if (promptCancel) promptCancel.removeEventListener('click', onCancel);
      promptInput.removeEventListener('keydown', onKey);
    };
    const onOk = () => {
      const name = promptInput.value.trim();
      cleanup();
      if (!name) return;
      const rules = _state.getToolRules();
      if (rules.some((r) => r.name === name)) {
        showToast(`"${name}" already has a rule`, 'info');
        return;
      }
      _state.pushToolRule({ name, state: 'ask' });
      renderToolRules();
    };
    const onCancel = () => cleanup();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Enter') onOk();
      if (e.key === 'Escape') onCancel();
    };
    promptOk.addEventListener('click', onOk);
    promptClose.addEventListener('click', onCancel);
    if (promptCancel) promptCancel.addEventListener('click', onCancel);
    promptInput.addEventListener('keydown', onKey);
  } else {
    const name = prompt('Tool name (e.g. brave_search):');
    if (!name?.trim()) return;
    const trimmed = name.trim();
    const rules = _state.getToolRules();
    if (rules.some((r) => r.name === trimmed)) {
      showToast(`"${trimmed}" already has a rule`, 'info');
      return;
    }
    _state.pushToolRule({ name: trimmed, state: 'ask' });
    renderToolRules();
  }
}

export async function loadSettingsApprovals() {
  const section = $('settings-approvals-section');
  if (section) section.style.display = '';
  renderToolRules();
}

export async function saveSettingsApprovals() {
  const policyRadio = document.querySelector(
    'input[name="approvals-policy"]:checked',
  ) as HTMLInputElement | null;
  const policy = policyRadio?.value ?? 'ask';

  const toolRules = _state.getToolRules();
  const allow = toolRules.filter((r) => r.state === 'allow').map((r) => r.name);
  const deny = toolRules.filter((r) => r.state === 'deny').map((r) => r.name);

  localStorage.setItem('paw-tool-approvals', JSON.stringify({ allow, deny, askPolicy: policy }));
  showToast('Approval rules saved locally', 'success');
}

// ── Security Audit Dashboard ───────────────────────────────────────────────

export function updateEncryptionStatus() {
  const bar = $('encryption-status-bar');
  const text = $('encryption-status-text');
  if (!bar || !text) return;

  const ready = isEncryptionReady();
  bar.className = `encryption-status-bar ${ready ? 'enc-active' : 'enc-inactive'}`;
  text.textContent = ready
    ? 'Database encryption active — sensitive fields encrypted with OS keychain key'
    : 'Encryption unavailable — credential storage is blocked until keychain is restored';
}

interface KeychainHealth {
  status: 'healthy' | 'degraded' | 'unavailable';
  db_key_ok: boolean;
  vault_key_ok: boolean;
  message: string;
  error: string | null;
}

/** Fetch keychain health from the Rust backend and update the UI indicator. */
export async function updateKeychainHealth(): Promise<void> {
  const bar = $('keychain-health-bar');
  const dot = $('keychain-health-dot');
  const text = $('keychain-health-text');
  const detail = $('keychain-health-detail');
  if (!bar || !dot || !text) return;

  try {
    const invoke = (await import('@tauri-apps/api/core')).invoke;
    const health = await invoke<KeychainHealth>('check_keychain_health');

    bar.style.display = 'flex';

    const statusClass =
      health.status === 'healthy'
        ? 'kc-healthy'
        : health.status === 'degraded'
          ? 'kc-degraded'
          : 'kc-unavailable';
    bar.className = `keychain-health-bar ${statusClass}`;
    text.textContent = health.message;
    if (detail) {
      detail.textContent = health.error ?? '';
      detail.style.display = health.error ? 'block' : 'none';
    }
  } catch (e) {
    bar.style.display = 'flex';
    bar.className = 'keychain-health-bar kc-unavailable';
    text.textContent = 'Unable to check keychain health';
    if (detail) {
      detail.textContent = String(e);
      detail.style.display = 'block';
    }
    console.error('[settings] Keychain health check failed:', e);
  }
}

export async function loadSecurityAudit() {
  const section = $('settings-audit-section');
  const tbody = $('audit-log-body');
  const emptyEl = $('audit-empty');
  const tableWrapper = $('audit-table-wrapper');
  if (!section || !tbody) return;

  section.style.display = '';
  updateEncryptionStatus();
  updateKeychainHealth();

  const filterType = ($('audit-filter-type') as HTMLSelectElement | null)?.value || undefined;
  const filterRisk = ($('audit-filter-risk') as HTMLSelectElement | null)?.value || '';
  const limit = parseInt(($('audit-filter-limit') as HTMLSelectElement | null)?.value || '100', 10);

  try {
    const entries = await getSecurityAuditLog(limit, filterType);

    const filtered = filterRisk ? entries.filter((e) => e.risk_level === filterRisk) : entries;

    const denied = entries.filter((e) => !e.was_allowed).length;
    const allowed = entries.filter((e) => e.was_allowed).length;
    const critical = entries.filter((e) => e.risk_level === 'critical').length;
    const deniedLabel = $('audit-score-denied-label');
    const allowedLabel = $('audit-score-allowed-label');
    const criticalLabel = $('audit-score-critical-label');
    if (deniedLabel) deniedLabel.textContent = `${denied} blocked`;
    if (allowedLabel) allowedLabel.textContent = `${allowed} allowed`;
    if (criticalLabel) criticalLabel.textContent = `${critical} critical`;

    if (filtered.length === 0) {
      tbody.innerHTML = '';
      if (emptyEl) emptyEl.style.display = '';
      if (tableWrapper) tableWrapper.style.display = 'none';
      return;
    }

    if (emptyEl) emptyEl.style.display = 'none';
    if (tableWrapper) tableWrapper.style.display = '';

    tbody.innerHTML = filtered
      .map((e) => {
        const time = e.timestamp ? new Date(`${e.timestamp}Z`).toLocaleString() : '—';
        const riskBadge = e.risk_level
          ? `<span class="audit-risk-badge risk-${escHtml(e.risk_level)}">${escHtml(e.risk_level)}</span>`
          : '<span class="audit-risk-badge">—</span>';
        const resultBadge = e.was_allowed
          ? '<span class="audit-result-badge allowed">✓ Allowed</span>'
          : '<span class="audit-result-badge denied">✕ Denied</span>';
        const eventLabel = e.event_type.replace(/_/g, ' ');
        return `<tr class="${e.was_allowed ? '' : 'audit-row-denied'}">
        <td class="audit-cell-time">${escHtml(time)}</td>
        <td class="audit-cell-event">${escHtml(eventLabel)}</td>
        <td>${riskBadge}</td>
        <td class="audit-cell-tool">${escHtml(e.tool_name ?? '—')}</td>
        <td class="audit-cell-detail" title="${escHtml(e.detail ?? '')}">${escHtml((e.detail ?? '').slice(0, 80))}${(e.detail?.length ?? 0) > 80 ? '…' : ''}</td>
        <td>${resultBadge}</td>
      </tr>`;
      })
      .join('');
  } catch (e) {
    console.warn('[settings] Audit log load failed:', e);
    if (emptyEl) {
      emptyEl.style.display = '';
      emptyEl.textContent = `Failed to load audit log: ${e}`;
    }
    if (tableWrapper) tableWrapper.style.display = 'none';
  }
}

export function exportAuditJSON() {
  const filterType = ($('audit-filter-type') as HTMLSelectElement | null)?.value || undefined;
  const limit = parseInt(($('audit-filter-limit') as HTMLSelectElement | null)?.value || '100', 10);
  getSecurityAuditLog(limit, filterType)
    .then((entries) => {
      const json = JSON.stringify(entries, null, 2);
      downloadFile('paw-security-audit.json', json, 'application/json');
    })
    .catch((e) => showToast(`Export failed: ${e}`, 'error'));
}

export function exportAuditCSV() {
  const filterType = ($('audit-filter-type') as HTMLSelectElement | null)?.value || undefined;
  const limit = parseInt(($('audit-filter-limit') as HTMLSelectElement | null)?.value || '100', 10);
  getSecurityAuditLog(limit, filterType)
    .then((entries) => {
      const headers = [
        'id',
        'timestamp',
        'event_type',
        'risk_level',
        'tool_name',
        'command',
        'detail',
        'session_key',
        'was_allowed',
        'matched_pattern',
      ];
      const rows = entries.map((e) =>
        headers
          .map((h) => {
            const val = (e as unknown as Record<string, unknown>)[h];
            const str = val == null ? '' : String(val);
            return `"${str.replace(/"/g, '""')}"`;
          })
          .join(','),
      );
      const csv = [headers.join(','), ...rows].join('\n');
      downloadFile('paw-security-audit.csv', csv, 'text/csv');
    })
    .catch((e) => showToast(`Export failed: ${e}`, 'error'));
}

// ── Security Policies (local settings) ─────────────────────────────────────

export function loadSecurityPolicies() {
  const settings = loadSecuritySettings();

  const autoDenyPriv = $('sec-auto-deny-priv') as HTMLInputElement | null;
  const autoDenyCritical = $('sec-auto-deny-critical') as HTMLInputElement | null;
  const requireType = $('sec-require-type') as HTMLInputElement | null;
  const readOnlyProjects = $('sec-read-only-projects') as HTMLInputElement | null;
  const allowlistEl = $('sec-allowlist') as HTMLTextAreaElement | null;
  const denylistEl = $('sec-denylist') as HTMLTextAreaElement | null;
  const rotationInterval = $('sec-token-rotation-interval') as HTMLSelectElement | null;
  const rotationStatus = $('sec-token-rotation-status');

  if (autoDenyPriv) autoDenyPriv.checked = settings.autoDenyPrivilegeEscalation;
  if (autoDenyCritical) autoDenyCritical.checked = settings.autoDenyCritical;
  if (requireType) requireType.checked = settings.requireTypeToCritical;
  if (readOnlyProjects) readOnlyProjects.checked = settings.readOnlyProjects;
  if (allowlistEl) allowlistEl.value = settings.commandAllowlist.join('\n');
  if (denylistEl) denylistEl.value = settings.commandDenylist.join('\n');
  if (rotationInterval) rotationInterval.value = String(settings.tokenRotationIntervalDays);
  if (rotationStatus) {
    if (settings.tokenRotationIntervalDays > 0) {
      rotationStatus.textContent = `Tokens older than ${settings.tokenRotationIntervalDays} days will be auto-rotated`;
    } else {
      rotationStatus.textContent = '';
    }
  }

  updateSessionOverrideBanner();
}

export function saveSecurityPolicies() {
  const autoDenyPriv = ($('sec-auto-deny-priv') as HTMLInputElement | null)?.checked ?? false;
  const autoDenyCritical =
    ($('sec-auto-deny-critical') as HTMLInputElement | null)?.checked ?? false;
  const requireType = ($('sec-require-type') as HTMLInputElement | null)?.checked ?? true;
  const readOnlyProjects =
    ($('sec-read-only-projects') as HTMLInputElement | null)?.checked ?? false;
  const allowlistRaw = ($('sec-allowlist') as HTMLTextAreaElement | null)?.value ?? '';
  const denylistRaw = ($('sec-denylist') as HTMLTextAreaElement | null)?.value ?? '';
  const tokenRotationIntervalDays = parseInt(
    ($('sec-token-rotation-interval') as HTMLSelectElement | null)?.value ?? '0',
    10,
  );

  const commandAllowlist = allowlistRaw
    .split('\n')
    .map((l) => l.trim())
    .filter(Boolean);
  const commandDenylist = denylistRaw
    .split('\n')
    .map((l) => l.trim())
    .filter(Boolean);

  for (const p of [...commandAllowlist, ...commandDenylist]) {
    try {
      new RegExp(p);
    } catch {
      showToast(`Invalid regex pattern: ${p}`, 'error');
      return;
    }
  }

  const existing = loadSecuritySettings();

  const settings: SecuritySettings = {
    autoDenyPrivilegeEscalation: autoDenyPriv,
    autoDenyCritical: autoDenyCritical,
    requireTypeToCritical: requireType,
    commandAllowlist,
    commandDenylist,
    sessionOverrideUntil: existing.sessionOverrideUntil,
    tokenRotationIntervalDays,
    readOnlyProjects,
  };
  saveSecuritySettings(settings);
  showToast('Security policies saved', 'success');
}

export function resetSecurityPolicies() {
  resetSecuritySettings()
    .then(() => {
      loadSecurityPolicies();
      showToast('Security policies reset to defaults', 'info');
    })
    .catch((e) => {
      console.warn('[settings] Failed to reset security settings:', e);
      showToast('Failed to reset security policies', 'error');
    });
}

// ── Session override banner management ─────────────────────────────────────

export function updateSessionOverrideBanner(): void {
  const banner = $('session-override-banner');
  const label = $('session-override-banner-label');
  if (!banner) return;

  const remaining = getSessionOverrideRemaining();
  if (remaining > 0) {
    const mins = Math.ceil(remaining / 60000);
    banner.style.display = 'flex';
    if (label)
      label.textContent = `Session override active — auto-approving all tools for ${mins} minute${mins !== 1 ? 's' : ''}`;

    if (!_state.getOverrideBannerInterval()) {
      const interval = setInterval(() => {
        const r = getSessionOverrideRemaining();
        if (r <= 0) {
          if (banner) banner.style.display = 'none';
          const cur = _state.getOverrideBannerInterval();
          if (cur) {
            clearInterval(cur);
            _state.setOverrideBannerInterval(null);
          }
          return;
        }
        const m = Math.ceil(r / 60000);
        if (label)
          label.textContent = `Session override active — auto-approving all tools for ${m} minute${m !== 1 ? 's' : ''}`;
      }, 30000);
      _state.setOverrideBannerInterval(interval);
    }
  } else {
    banner.style.display = 'none';
    const cur = _state.getOverrideBannerInterval();
    if (cur) {
      clearInterval(cur);
      _state.setOverrideBannerInterval(null);
    }
  }
}
