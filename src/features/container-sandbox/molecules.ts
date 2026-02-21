// ─── Container Sandbox · Molecules ─────────────────────────────────────
// Composed functions with side effects: Tauri IPC, localStorage.
// Builds on atoms for sandbox config management.

import {
  type SandboxConfig,
  type SandboxStatus,
  DEFAULT_SANDBOX_CONFIG,
  SANDBOX_PRESETS,
  validateSandboxConfig,
  describeSandboxConfig,
  assessCommandRisk,
} from './atoms';

const STORAGE_KEY = 'paw_sandbox_config';

// ── Config Persistence ─────────────────────────────────────────────────

/** Load sandbox config from localStorage */
export function loadSandboxConfig(): SandboxConfig {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      return { ...DEFAULT_SANDBOX_CONFIG, ...parsed };
    }
  } catch (e) {
    console.warn('[sandbox] Failed to load config:', e);
  }
  return { ...DEFAULT_SANDBOX_CONFIG };
}

/** Save sandbox config to localStorage */
export function saveSandboxConfig(config: SandboxConfig): void {
  const validation = validateSandboxConfig(config);
  if (!validation.valid) {
    console.error('[sandbox] Cannot save invalid config:', validation.errors);
    return;
  }
  localStorage.setItem(STORAGE_KEY, JSON.stringify(config));
}

/** Toggle sandbox on/off and persist */
export function toggleSandbox(enabled: boolean): SandboxConfig {
  const config = loadSandboxConfig();
  config.enabled = enabled;
  saveSandboxConfig(config);
  return config;
}

/** Apply a preset and persist */
export function applyPreset(presetName: keyof typeof SANDBOX_PRESETS): SandboxConfig {
  const preset = SANDBOX_PRESETS[presetName];
  if (!preset) {
    console.warn('[sandbox] Unknown preset:', presetName);
    return loadSandboxConfig();
  }
  const config = { ...preset };
  saveSandboxConfig(config);
  return config;
}

/** Reset to default config */
export function resetSandboxConfig(): SandboxConfig {
  const config = { ...DEFAULT_SANDBOX_CONFIG };
  saveSandboxConfig(config);
  return config;
}

// ── Status & Health ────────────────────────────────────────────────────

let _cachedStatus: SandboxStatus | null = null;

/** Get sandbox status (checks Docker availability via Tauri IPC) */
export async function getSandboxStatus(): Promise<SandboxStatus> {
  try {
    // @ts-ignore — Tauri invoke
    const { invoke } = window.__TAURI__.core;
    const available: boolean = await invoke('engine_sandbox_check');
    _cachedStatus = {
      dockerAvailable: available,
      lastChecked: Date.now(),
      containerCount: 0,
    };
  } catch {
    _cachedStatus = {
      dockerAvailable: false,
      lastChecked: Date.now(),
      containerCount: 0,
    };
  }
  return _cachedStatus;
}

/** Check if sandbox should be used for a given command */
export function shouldSandbox(command: string): { sandbox: boolean; reason: string } {
  const config = loadSandboxConfig();

  if (!config.enabled) {
    return { sandbox: false, reason: 'Sandbox is disabled' };
  }

  const risk = assessCommandRisk(command);

  // Always sandbox high/critical risk commands when enabled
  if (risk === 'critical' || risk === 'high') {
    return { sandbox: true, reason: `${risk} risk command detected` };
  }

  // Sandbox everything when enabled
  return { sandbox: true, reason: 'Sandbox mode is active' };
}

/** Get a human-readable description of current sandbox state */
export function getSandboxSummary(): string {
  const config = loadSandboxConfig();
  return describeSandboxConfig(config);
}

// ── Re-exports for convenience ─────────────────────────────────────────

export type { SandboxConfig, SandboxStatus, SandboxValidation } from './atoms';

export {
  DEFAULT_SANDBOX_CONFIG,
  SANDBOX_PRESETS,
  validateSandboxConfig,
  describeSandboxConfig,
  assessCommandRisk,
  formatMemoryLimit,
} from './atoms';
