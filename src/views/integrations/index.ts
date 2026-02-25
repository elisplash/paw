// src/views/integrations/index.ts — Orchestration + public API
//
// Thin barrel: owns module state, re-exports public surface.

import type { ServiceDefinition, ConnectedService } from './atoms';
import { renderIntegrations, initMoleculesState } from './molecules';

// ── Module state ───────────────────────────────────────────────────────

let _connected: ConnectedService[] = [];
let _selectedService: ServiceDefinition | null = null;

const { setMoleculesState } = initMoleculesState();
setMoleculesState({
  getConnected: () => _connected,
  setSelectedService: (s) => { _selectedService = s; },
  getSelectedService: () => _selectedService,
});

// ── Public API ─────────────────────────────────────────────────────────

export async function loadIntegrations(): Promise<void> {
  // TODO Phase 2.5+: fetch connected services from backend
  // _connected = await pawEngine.engine_n8n_get_connected_services();
  renderIntegrations();
}

export function getConnectedCount(): number {
  return _connected.length;
}

export { SERVICE_CATALOG } from './catalog';
