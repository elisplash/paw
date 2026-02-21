// Settings View — Index (orchestration, state, exports)
// Logs, Usage, Presence, Nodes, Devices, Exec Approvals, Security Policies

import { clearSessionOverride } from '../../security';
import { $ } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { isConnected } from '../../state/connection';
import type { ToolRule } from './atoms';
import {
  setMoleculesState,
  loadSettingsStatus,
  loadSettingsLogs,
  loadSettingsUsage,
  loadSettingsPresence,
  loadSettingsNodes,
  loadSettingsDevices,
  loadSettingsWizard,
  startWizard,
  wizardNext,
  cancelWizard,
  runUpdate,
  loadSettingsBrowser,
  startBrowser,
  stopBrowser,
  loadSettingsApprovals,
  saveSettingsApprovals,
  addToolRule,
  initBudgetSettings,
  loadSecurityAudit,
  exportAuditJSON,
  exportAuditCSV,
  loadSecurityPolicies,
  saveSecurityPolicies,
  resetSecurityPolicies,
  updateSessionOverrideBanner,
} from './molecules';

// ── Module state ───────────────────────────────────────────────────────────

let _usageRefreshInterval: ReturnType<typeof setInterval> | null = null;
let _toolRules: ToolRule[] = [];
let _overrideBannerInterval: ReturnType<typeof setInterval> | null = null;

// ── State bridge for molecules ─────────────────────────────────────────────

function initMoleculesState() {
  setMoleculesState({
    getToolRules: () => _toolRules,
    setToolRules: (rules: ToolRule[]) => {
      _toolRules = rules;
    },
    pushToolRule: (rule: ToolRule) => {
      _toolRules.push(rule);
    },
    spliceToolRule: (idx: number) => {
      _toolRules.splice(idx, 1);
    },
    getOverrideBannerInterval: () => _overrideBannerInterval,
    setOverrideBannerInterval: (v: ReturnType<typeof setInterval> | null) => {
      _overrideBannerInterval = v;
    },
  });
}

// ── Usage auto-refresh ─────────────────────────────────────────────────────

export function startUsageAutoRefresh() {
  stopUsageAutoRefresh();
  _usageRefreshInterval = setInterval(() => {
    if (isConnected()) loadSettingsUsage().catch(() => {});
  }, 30_000);
}

export function stopUsageAutoRefresh() {
  if (_usageRefreshInterval) {
    clearInterval(_usageRefreshInterval);
    _usageRefreshInterval = null;
  }
}

/** Clear the session-override banner interval (call on view unmount). */
export function stopOverrideBannerInterval() {
  if (_overrideBannerInterval) {
    clearInterval(_overrideBannerInterval);
    _overrideBannerInterval = null;
  }
}

// ── Token auto-rotation check (H4) ────────────────────────────────────────

export async function checkTokenAutoRotation(): Promise<void> {
  // Device token rotation not available in engine mode
}

// ── Initialize event listeners ─────────────────────────────────────────────

export function initSettings() {
  initMoleculesState();

  $('settings-refresh-status')?.addEventListener('click', () => loadSettingsStatus());
  $('settings-refresh-logs')?.addEventListener('click', () => loadSettingsLogs());
  $('settings-refresh-usage')?.addEventListener('click', () => loadSettingsUsage());
  $('settings-refresh-presence')?.addEventListener('click', () => loadSettingsPresence());
  $('settings-refresh-nodes')?.addEventListener('click', () => loadSettingsNodes());
  $('settings-refresh-devices')?.addEventListener('click', () => loadSettingsDevices());
  $('settings-refresh-approvals')?.addEventListener('click', () => loadSettingsApprovals());
  $('settings-save-approvals')?.addEventListener('click', () => saveSettingsApprovals());
  $('approvals-add-tool')?.addEventListener('click', () => addToolRule());
  // Wizard
  $('settings-wizard-start')?.addEventListener('click', () => startWizard());
  $('settings-wizard-next')?.addEventListener('click', () => wizardNext());
  $('settings-wizard-cancel')?.addEventListener('click', () => cancelWizard());
  // Update
  $('settings-update-run')?.addEventListener('click', () => runUpdate());
  // Browser
  $('settings-browser-start')?.addEventListener('click', () => startBrowser());
  $('settings-browser-stop')?.addEventListener('click', () => stopBrowser());
  $('settings-refresh-browser')?.addEventListener('click', () => loadSettingsBrowser());
  // Security audit
  $('audit-refresh')?.addEventListener('click', () => loadSecurityAudit());
  $('audit-export-json')?.addEventListener('click', () => exportAuditJSON());
  $('audit-export-csv')?.addEventListener('click', () => exportAuditCSV());
  $('audit-filter-type')?.addEventListener('change', () => loadSecurityAudit());
  $('audit-filter-risk')?.addEventListener('change', () => loadSecurityAudit());
  $('audit-filter-limit')?.addEventListener('change', () => loadSecurityAudit());
  // Security policies
  $('settings-save-security')?.addEventListener('click', () => saveSecurityPolicies());
  $('settings-reset-security')?.addEventListener('click', () => resetSecurityPolicies());
  // Session override cancel
  $('session-override-cancel')?.addEventListener('click', () => {
    clearSessionOverride();
    updateSessionOverrideBanner();
    showToast('Session override cancelled — approval modal restored', 'info');
  });
  // Budget
  initBudgetSettings();
}

// ── Load all settings data ─────────────────────────────────────────────────

export async function loadSettings() {
  initMoleculesState();

  loadSecurityPolicies(); // synchronous — reads from localStorage
  await Promise.all([
    loadSecurityAudit(),
    loadSettingsStatus(),
    loadSettingsLogs(),
    loadSettingsUsage(),
    loadSettingsPresence(),
    loadSettingsNodes(),
    loadSettingsDevices(),
    loadSettingsApprovals(),
    loadSettingsWizard(),
    loadSettingsBrowser(),
  ]);
  checkTokenAutoRotation().catch(() => {});
}

// ── Re-exports ─────────────────────────────────────────────────────────────

export { getBudgetLimit } from './atoms';
export {
  loadSettingsStatus,
  loadSettingsLogs,
  loadSettingsUsage,
  loadSettingsPresence,
  loadSettingsNodes,
  loadSettingsDevices,
  loadSettingsApprovals,
  loadSecurityAudit,
  loadSecurityPolicies,
  updateSessionOverrideBanner,
  updateEncryptionStatus,
} from './molecules';
