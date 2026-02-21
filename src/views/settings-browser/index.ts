// Settings: Browser & Sandbox — Orchestration, state, exports
// All data goes through Tauri IPC. No gateway.

import { pawEngine } from '../../engine';
import { $ } from '../../components/helpers';
import {
  initMoleculesState,
  renderProfiles,
  renderScreenshots,
  renderWorkspaces,
  renderNetworkPolicy,
} from './molecules';

// ── State bridge ──────────────────────────────────────────────────────

const { setMoleculesState } = initMoleculesState();
setMoleculesState({
  getLoadBrowserSettings: () => loadBrowserSettings,
});

// ── Load ──────────────────────────────────────────────────────────────

export async function loadBrowserSettings() {
  const container = $('settings-browser-sandbox-content');
  if (!container) return;
  container.innerHTML =
    '<p style="color:var(--text-muted)">Loading browser &amp; sandbox config…</p>';

  try {
    const [browserConfig, screenshots, workspaces, networkPolicy] = await Promise.all([
      pawEngine.browserGetConfig(),
      pawEngine.screenshotsList(),
      pawEngine.workspacesList(),
      pawEngine.networkGetPolicy(),
    ]);

    container.innerHTML = '';

    renderProfiles(container, browserConfig);
    renderScreenshots(container, screenshots);
    renderWorkspaces(container, workspaces);
    renderNetworkPolicy(container, networkPolicy);
  } catch (e) {
    container.innerHTML = `<p style="color:var(--error)">Failed to load browser &amp; sandbox settings: ${e}</p>`;
  }
}
