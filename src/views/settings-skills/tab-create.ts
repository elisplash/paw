// My Skills — Create Tab
// Wraps the existing wizard.ts — moves the skill creation wizard into its own tab.

import { renderWizardSection, bindWizardEvents, setWizardReload } from './wizard';

// ── Render ─────────────────────────────────────────────────────────────

export function renderCreateTab(): string {
  return renderWizardSection();
}

// ── Event binding ──────────────────────────────────────────────────────

export function bindCreateTabEvents(reloadFn: () => Promise<void>): void {
  setWizardReload(reloadFn);
  bindWizardEvents();
}
