// src/views/integrations/index.ts — Orchestration + public API
//
// Thin barrel: owns module state, re-exports public surface.

import type { ServiceDefinition, ConnectedService } from './atoms';
import { renderIntegrations, initMoleculesState, setNativeIntegrations } from './molecules';
import {
  updateIntegrationsHeroStats,
  renderHealthList,
  renderCategoryBreakdown,
  initIntegrationsKinetic,
} from '../../components/integrations-panel';
import {
  pawEngine,
  type EngineSkillStatus,
  type McpServerConfig,
  type McpServerStatus,
} from '../../engine';
import { isEngineMode } from '../../engine-bridge';
import { invoke } from '@tauri-apps/api/core';

// ── Module state ───────────────────────────────────────────────────────

const _connected: ConnectedService[] = [];
let _selectedService: ServiceDefinition | null = null;

const { setMoleculesState } = initMoleculesState();
setMoleculesState({
  getConnected: () => _connected,
  setSelectedService: (s) => {
    _selectedService = s;
  },
  getSelectedService: () => _selectedService,
});

// ── Public API ─────────────────────────────────────────────────────────

export async function loadIntegrations(): Promise<void> {
  // Fetch native engine skills + MCP servers (the working integrations)
  let nativeSkills: EngineSkillStatus[] = [];
  let mcpServers: McpServerConfig[] = [];
  let mcpStatuses: McpServerStatus[] = [];

  if (isEngineMode()) {
    try {
      const [skills, servers, statuses] = await Promise.all([
        pawEngine.skillsList(),
        pawEngine.mcpListServers(),
        pawEngine.mcpStatus(),
      ]);
      // Native integrations = skills with credentials (tier=integration) that are enabled
      nativeSkills = skills.filter(
        (s) =>
          s.tier === 'integration' || (s.required_credentials && s.required_credentials.length > 0),
      );
      mcpServers = servers;
      mcpStatuses = statuses;

      // ── n8n MCP bridge: auto-ensure ready + fetch bridge status ──
      try {
        const mcpBridge = await invoke<{ connected: boolean; tool_count: number }>(
          'engine_n8n_mcp_status',
        );
        if (mcpBridge.connected && mcpBridge.tool_count > 0) {
          // Inject the n8n MCP bridge as a visible MCP server entry
          const n8nServer: McpServerConfig = {
            id: 'n8n',
            name: 'n8n Integrations (MCP Bridge)',
            transport: 'sse',
            command: '',
            args: [],
            env: {},
            url: '',
            enabled: true,
          };
          const n8nStatus: McpServerStatus = {
            id: 'n8n',
            name: 'n8n Integrations (MCP Bridge)',
            connected: true,
            error: null,
            tool_count: mcpBridge.tool_count,
          };
          // Only add if not already present from regular MCP list
          if (!mcpServers.some((s) => s.id === 'n8n')) {
            mcpServers.push(n8nServer);
          }
          if (!mcpStatuses.some((s) => s.id === 'n8n')) {
            mcpStatuses.push(n8nStatus);
          }
        }
      } catch (e) {
        // n8n MCP bridge not available — that's fine
        console.debug('[integrations] n8n MCP bridge not available:', e);
      }
    } catch (e) {
      console.warn('[integrations] Failed to fetch native skills:', e);
    }
  }

  // Pass native data into molecules for rendering
  setNativeIntegrations(nativeSkills, mcpServers, mcpStatuses);

  // Fetch connected services from backend
  if (isEngineMode()) {
    try {
      const details = await invoke<ConnectedService[]>('engine_integrations_get_connected');
      _connected.length = 0;
      _connected.push(...details);
    } catch (e) {
      console.warn('[integrations] Failed to fetch connected services:', e);
    }
  }
  renderIntegrations();

  // Side panel
  updateIntegrationsHeroStats(_connected);
  renderHealthList(_connected);
  renderCategoryBreakdown();
  initIntegrationsKinetic();

  // Wire quick actions
  _wireQuickActions();
}

export function getConnectedCount(): number {
  return _connected.length;
}

/** Re-fetch connected services from backend and re-render the view. */
export async function refreshConnected(): Promise<void> {
  if (isEngineMode()) {
    try {
      const details = await invoke<ConnectedService[]>('engine_integrations_get_connected');
      _connected.length = 0;
      _connected.push(...details);
    } catch (e) {
      console.warn('[integrations] refreshConnected failed:', e);
    }
  }
  renderIntegrations();
  updateIntegrationsHeroStats(_connected);
  renderHealthList(_connected);
}

// ── Quick Action Bindings ──────────────────────────────────────────────

function _wireQuickActions(): void {
  document.getElementById('integrations-qa-browse')?.addEventListener('click', () => {
    // Switch to services tab and clear filters
    renderIntegrations();
  });

  document.getElementById('integrations-qa-automations')?.addEventListener('click', () => {
    // Simulate clicking the Automations main tab
    const btn = document.querySelector(
      '.integrations-main-tab[data-main-tab="automations"]',
    ) as HTMLElement;
    btn?.click();
  });

  document.getElementById('integrations-qa-queries')?.addEventListener('click', () => {
    const btn = document.querySelector(
      '.integrations-main-tab[data-main-tab="queries"]',
    ) as HTMLElement;
    btn?.click();
  });
}

export { SERVICE_CATALOG } from './catalog';
