// ─── Browser Sandbox · Molecules ───────────────────────────────────────
// Composed functions with side effects: Tauri IPC calls.
// Builds on atoms for browser profile, screenshot, workspace and network management.

import type {
  BrowserConfig,
  BrowserProfile,
  ScreenshotEntry,
  WorkspaceInfo,
  WorkspaceFile,
  NetworkPolicy,
} from './atoms';

// Use dynamic import to avoid circular dep if engine barrel re-exports
const getEngine = async () => {
  const { pawEngine } = await import('../../engine/molecules/ipc_client');
  return pawEngine;
};

// ── Browser Profile Management ─────────────────────────────────────────

/** Load browser configuration from backend */
export async function loadBrowserConfig(): Promise<BrowserConfig> {
  const engine = await getEngine();
  return engine.browserGetConfig();
}

/** Save browser configuration to backend */
export async function saveBrowserConfig(config: BrowserConfig): Promise<void> {
  const engine = await getEngine();
  return engine.browserSetConfig(config);
}

/** Create a new named browser profile */
export async function createBrowserProfile(name: string): Promise<BrowserProfile> {
  const engine = await getEngine();
  return engine.browserCreateProfile(name);
}

/** Delete a browser profile by ID */
export async function deleteBrowserProfile(profileId: string): Promise<void> {
  const engine = await getEngine();
  return engine.browserDeleteProfile(profileId);
}

// ── Screenshot Management ──────────────────────────────────────────────

/** List all available screenshots */
export async function listScreenshots(): Promise<ScreenshotEntry[]> {
  const engine = await getEngine();
  return engine.screenshotsList();
}

/** Get a screenshot's base64 PNG data */
export async function getScreenshot(filename: string): Promise<ScreenshotEntry> {
  const engine = await getEngine();
  return engine.screenshotGet(filename);
}

/** Delete a screenshot */
export async function deleteScreenshot(filename: string): Promise<void> {
  const engine = await getEngine();
  return engine.screenshotDelete(filename);
}

// ── Per-Agent Workspace Management ─────────────────────────────────────

/** List all agent workspaces with stats */
export async function listWorkspaces(): Promise<WorkspaceInfo[]> {
  const engine = await getEngine();
  return engine.workspacesList();
}

/** List files in an agent's workspace */
export async function listWorkspaceFiles(
  agentId: string,
  subdir?: string,
): Promise<WorkspaceFile[]> {
  const engine = await getEngine();
  return engine.workspaceFiles(agentId, subdir);
}

/** Delete an agent's workspace */
export async function deleteWorkspace(agentId: string): Promise<void> {
  const engine = await getEngine();
  return engine.workspaceDelete(agentId);
}

// ── Network Policy Management ──────────────────────────────────────────

/** Load outbound network policy */
export async function loadNetworkPolicy(): Promise<NetworkPolicy> {
  const engine = await getEngine();
  return engine.networkGetPolicy();
}

/** Save outbound network policy */
export async function saveNetworkPolicy(policy: NetworkPolicy): Promise<void> {
  const engine = await getEngine();
  return engine.networkSetPolicy(policy);
}

/** Check if a URL is allowed by the network policy */
export async function checkNetworkUrl(url: string): Promise<[boolean, string]> {
  const engine = await getEngine();
  return engine.networkCheckUrl(url);
}
