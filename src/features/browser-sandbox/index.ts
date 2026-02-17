// P4: Browser & Sandbox — Feature barrel export

export {
  // Types
  type BrowserProfile,
  type AgentWorkspace,
  type ScreenshotInfo,
  type DomainPolicy,
  type DomainPolicyMode,
  // Constants
  DEFAULT_DOMAIN_POLICY,
  DOMAIN_POLICY_PRESETS,
  // Pure functions
  describeProfileAge,
  formatProfileSize,
  formatWorkspaceSize,
  describeWorkspaceAge,
  formatScreenshotSize,
  extractTimestamp,
  screenshotDataUrl,
  describeDomainPolicy,
  isValidDomainPattern,
  wouldAllow,
} from './atoms';

export {
  // Browser profiles
  listBrowserProfiles,
  deleteBrowserProfile,
  // Screenshots
  listScreenshots,
  readScreenshot,
  deleteScreenshot,
  // Workspaces
  listWorkspaces,
  ensureWorkspace,
  deleteWorkspace,
  getWorkspaceEnabled,
  setWorkspaceEnabled,
  // Domain policy
  getDomainPolicy,
  setDomainPolicy,
  // Composite
  getAgentIsolationStatus,
  configureAgentIsolation,
  // Local config
  type ScreenshotViewMode,
  loadScreenshotViewMode,
  saveScreenshotViewMode,
} from './molecules';
