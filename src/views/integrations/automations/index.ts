// src/views/integrations/automations/index.ts — Orchestration + public API
//
// Thin barrel: owns module state, re-exports.

import type { ActiveAutomation } from './atoms';
import {
  renderAutomations,
  renderServiceTemplates,
  setServiceFilter,
  initAutomationsMoleculesState,
} from './molecules';

// ── Module state ───────────────────────────────────────────────────────

let _activeAutomations: ActiveAutomation[] = [];
let _connectedIds: Set<string> = new Set();

const { setAutomationsMoleculesState } = initAutomationsMoleculesState();
setAutomationsMoleculesState({
  getConnectedIds: () => _connectedIds,
  getActive: () => _activeAutomations,
  setActive: (a) => {
    _activeAutomations = a;
  },
});

// ── Public API ─────────────────────────────────────────────────────────

export function setConnectedIds(ids: Set<string>): void {
  _connectedIds = ids;
}

export function loadAutomations(container: HTMLElement): void {
  // TODO Phase 3+: fetch active automations from backend
  renderAutomations(container);
}

export function loadServiceTemplates(container: HTMLElement, serviceId: string): void {
  renderServiceTemplates(container, serviceId);
}

export function getActiveCount(): number {
  return _activeAutomations.length;
}

export { setServiceFilter };
export type { ActiveAutomation } from './atoms';
export { TEMPLATE_CATALOG } from './templates';
