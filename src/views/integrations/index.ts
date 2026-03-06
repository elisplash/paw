// src/views/integrations/index.ts — Orchestration + public API
//
// Thin barrel: owns module state, re-exports public surface.

import type { ServiceDefinition, ConnectedService } from './atoms';
import {
  renderIntegrations,
  initMoleculesState,
  setNativeIntegrations,
  fetchOAuthAvailability,
} from './molecules';
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

/**
 * Normalize a backend ConnectedService to ensure camelCase properties.
 * The Rust backend may send snake_case (service_id) or camelCase (serviceId)
 * depending on whether the serde rename has been rebuilt.
 */
function _normalizeConnected(raw: Record<string, unknown>): ConnectedService {
  return {
    serviceId: (raw.serviceId ?? raw.service_id ?? '') as string,
    connectedAt: (raw.connectedAt ?? raw.connected_at ?? '') as string,
    lastUsed: (raw.lastUsed ?? raw.last_used ?? undefined) as string | undefined,
    toolCount: (raw.toolCount ?? raw.tool_count ?? 0) as number,
    status: (raw.status ?? 'connected') as 'connected' | 'error' | 'expired',
  };
}

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
      // All native skills — integration-tier AND CLI/productivity/media/etc.
      nativeSkills = skills;
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
      const raw = await invoke<Record<string, unknown>[]>('engine_integrations_get_connected');
      _connected.length = 0;
      _connected.push(...raw.map(_normalizeConnected));
    } catch (e) {
      console.warn('[integrations] Failed to fetch connected services:', e);
    }

    // ── Auto-detect Rust skill vault connections ──
    _autoDetectSkillConnections(nativeSkills);

    // ── Check which OAuth services have real Client IDs ──
    await fetchOAuthAvailability();
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
      const raw = await invoke<Record<string, unknown>[]>('engine_integrations_get_connected');
      _connected.length = 0;
      _connected.push(...raw.map(_normalizeConnected));
    } catch (e) {
      console.warn('[integrations] refreshConnected failed:', e);
    }

    // ── Auto-detect Rust skill vault connections ──
    // Same logic as loadIntegrations — skills configured via the Skills page
    // should also appear as connected in the Integration Hub.
    try {
      const nativeSkills = await pawEngine.skillsList();
      _autoDetectSkillConnections(nativeSkills);
    } catch {
      // Native skills not available — skip auto-detect
    }
  }
  renderIntegrations();
  updateIntegrationsHeroStats(_connected);
  renderHealthList(_connected);
}

// ── Auto-detect Skill Connections ──────────────────────────────────────

/** Skills configured via the Skills page should also appear as connected. */
function _autoDetectSkillConnections(nativeSkills: EngineSkillStatus[]): void {
  const skillToService: Record<string, string[]> = {
    google_workspace: ['gmail', 'google-sheets', 'google-calendar', 'google-docs', 'google-drive'],
    slack: ['slack'],
    discord: ['discord'],
    github: ['github'],
    trello: ['trello'],
    telegram: ['telegram'],
  };
  const connectedIds = new Set(_connected.map((c) => c.serviceId));
  for (const skill of nativeSkills) {
    if (!skill.is_ready) continue;
    const serviceIds = skillToService[skill.id];
    if (!serviceIds) continue;
    for (const sid of serviceIds) {
      if (connectedIds.has(sid)) continue;
      _connected.push({
        serviceId: sid,
        connectedAt: new Date().toISOString(),
        toolCount: skill.tool_names?.length ?? 0,
        status: 'connected',
      });
      connectedIds.add(sid);
    }
  }
}

// ── Quick Action Bindings ──────────────────────────────────────────────

let _quickActionsWired = false;

function _wireQuickActions(): void {
  if (_quickActionsWired) return;
  _quickActionsWired = true;

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

// ── Connected Drawer ───────────────────────────────────────────────────

// Listen for the custom event fired by the hero stat click
document.addEventListener('integrations:show-connected-drawer', () => {
  _showConnectedDrawer();
});

function _showConnectedDrawer(): void {
  if (_connected.length === 0) return;

  // Append to document.body — NOT inside .integrations-view.
  // The view's retroFadeIn animation uses `transform` which creates a
  // containing block that breaks `position: fixed` children.
  let drawer = document.getElementById('integrations-connected-drawer');
  let backdrop = document.getElementById('integrations-connected-backdrop');
  if (!drawer) {
    backdrop = document.createElement('div');
    backdrop.id = 'integrations-connected-backdrop';
    backdrop.className = 'connected-drawer-backdrop';
    document.body.appendChild(backdrop);

    drawer = document.createElement('div');
    drawer.id = 'integrations-connected-drawer';
    drawer.className = 'integrations-connected-drawer';
    document.body.appendChild(drawer);

    backdrop.addEventListener('click', () => {
      drawer!.classList.remove('open');
      backdrop!.classList.remove('open');
      drawer!.innerHTML = '';
    });
  }

  // Toggle: if already visible, close it
  if (drawer.classList.contains('open')) {
    drawer.classList.remove('open');
    if (backdrop) backdrop.classList.remove('open');
    drawer.innerHTML = '';
    return;
  }

  // Import the catalog for display names/icons
  const catalog = SERVICE_CATALOG;
  const items = _connected
    .map((c) => {
      const svc = catalog.find((s) => s.id === c.serviceId);
      const name = svc?.name ?? c.serviceId ?? 'Unknown';
      const icon = svc?.icon ?? 'extension';
      const color = svc?.color ?? 'var(--text-secondary)';
      const dotClass =
        c.status === 'error' ? 'error' : c.status === 'expired' ? 'warning' : 'healthy';
      const statusLabel =
        c.status === 'error' ? 'Error' : c.status === 'expired' ? 'Expired' : 'Connected';
      const tools = c.toolCount ?? 0;
      return `<div class="connected-drawer-item k-row k-spring" data-service-id="${c.serviceId}">
        <span class="ms connected-drawer-icon" style="color:${color}">${icon}</span>
        <div class="connected-drawer-info">
          <span class="connected-drawer-name">${name}</span>
          <span class="connected-drawer-meta">${tools} tool${tools !== 1 ? 's' : ''} · ${statusLabel}</span>
        </div>
        <span class="integrations-health-dot ${dotClass}"></span>
      </div>`;
    })
    .join('');

  drawer.innerHTML = `
    <div class="connected-drawer-header">
      <span class="ms">power</span>
      <span class="connected-drawer-title">${_connected.length} Connected Service${_connected.length !== 1 ? 's' : ''}</span>
      <button class="btn btn-ghost btn-sm connected-drawer-close">
        <span class="ms">close</span>
      </button>
    </div>
    <div class="connected-drawer-list">${items}</div>
  `;

  drawer.classList.add('open');
  if (backdrop) backdrop.classList.add('open');

  // Close button
  drawer.querySelector('.connected-drawer-close')?.addEventListener('click', () => {
    drawer!.classList.remove('open');
    if (backdrop) backdrop.classList.remove('open');
    drawer!.innerHTML = '';
  });

  // Click item to open detail
  drawer.querySelectorAll('.connected-drawer-item').forEach((el) => {
    el.addEventListener('click', () => {
      const sid = (el as HTMLElement).dataset.serviceId;
      if (sid) {
        const svc = catalog.find((s) => s.id === sid);
        if (svc) {
          _selectedService = svc;
          renderIntegrations();
        }
      }
      drawer!.classList.remove('open');
      if (backdrop) backdrop.classList.remove('open');
    });
  });
}

import { SERVICE_CATALOG } from './catalog';
export { SERVICE_CATALOG };
