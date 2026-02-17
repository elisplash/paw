// P4: Browser & Sandbox — Molecules (side effects: Tauri IPC, localStorage)
// Depends on atoms for types, talks to Rust backend via invoke().

import { invoke } from '@tauri-apps/api/core';
import type {
  BrowserProfile,
  AgentWorkspace,
  ScreenshotInfo,
  DomainPolicy,
} from './atoms';
import { DEFAULT_DOMAIN_POLICY } from './atoms';

// ── Browser Profile IPC ────────────────────────────────────────────────

export async function listBrowserProfiles(): Promise<BrowserProfile[]> {
  return invoke<BrowserProfile[]>('engine_browser_profiles_list');
}

export async function deleteBrowserProfile(agentId: string): Promise<void> {
  return invoke('engine_browser_profile_delete', { agentId });
}

// ── Screenshot Viewer IPC ──────────────────────────────────────────────

export async function listScreenshots(): Promise<ScreenshotInfo[]> {
  return invoke<ScreenshotInfo[]>('engine_screenshots_list');
}

export async function readScreenshot(filepath: string): Promise<string> {
  return invoke<string>('engine_screenshot_read', { filepath });
}

export async function deleteScreenshot(filepath: string): Promise<void> {
  return invoke('engine_screenshot_delete', { filepath });
}

// ── Per-Agent Workspace IPC ────────────────────────────────────────────

export async function listWorkspaces(): Promise<AgentWorkspace[]> {
  return invoke<AgentWorkspace[]>('engine_workspaces_list');
}

export async function ensureWorkspace(agentId: string): Promise<string> {
  return invoke<string>('engine_workspace_ensure', { agentId });
}

export async function deleteWorkspace(agentId: string): Promise<void> {
  return invoke('engine_workspace_delete', { agentId });
}

export async function getWorkspaceEnabled(agentId: string): Promise<boolean> {
  return invoke<boolean>('engine_workspace_get_enabled', { agentId });
}

export async function setWorkspaceEnabled(
  agentId: string,
  enabled: boolean,
): Promise<void> {
  return invoke('engine_workspace_set_enabled', { agentId, enabled });
}

// ── Domain Policy IPC ──────────────────────────────────────────────────

export async function getDomainPolicy(agentId: string): Promise<DomainPolicy> {
  return invoke<DomainPolicy>('engine_domain_policy_get', { agentId });
}

export async function setDomainPolicy(
  agentId: string,
  policy: DomainPolicy,
): Promise<void> {
  return invoke('engine_domain_policy_set', { agentId, policy });
}

// ── Composite Helpers ──────────────────────────────────────────────────

/** Get full agent isolation status: workspace + domain policy + browser profile. */
export async function getAgentIsolationStatus(agentId: string) {
  const [workspaceEnabled, domainPolicy, profiles] = await Promise.all([
    getWorkspaceEnabled(agentId),
    getDomainPolicy(agentId),
    listBrowserProfiles(),
  ]);

  const hasProfile = profiles.some((p) => p.agent_id === agentId);

  return {
    workspaceEnabled,
    domainPolicy,
    hasBrowserProfile: hasProfile,
    profile: profiles.find((p) => p.agent_id === agentId) ?? null,
  };
}

/** Quick setup: enable workspace + set domain policy for an agent. */
export async function configureAgentIsolation(
  agentId: string,
  options: {
    workspaceEnabled?: boolean;
    domainPolicy?: DomainPolicy;
  },
): Promise<void> {
  const promises: Promise<void>[] = [];

  if (options.workspaceEnabled !== undefined) {
    promises.push(setWorkspaceEnabled(agentId, options.workspaceEnabled));
  }
  if (options.domainPolicy) {
    promises.push(setDomainPolicy(agentId, options.domainPolicy));
  }

  await Promise.all(promises);
}

// ── Local Config (localStorage) ────────────────────────────────────────

const SCREENSHOT_VIEW_KEY = 'paw_screenshot_view_mode';

export type ScreenshotViewMode = 'grid' | 'list';

export function loadScreenshotViewMode(): ScreenshotViewMode {
  return (localStorage.getItem(SCREENSHOT_VIEW_KEY) as ScreenshotViewMode) || 'grid';
}

export function saveScreenshotViewMode(mode: ScreenshotViewMode): void {
  localStorage.setItem(SCREENSHOT_VIEW_KEY, mode);
}
