// Canvas View — Orchestration, state, event subscriptions

import { type ParsedCanvasComponent } from './atoms';
import {
  initMoleculesState,
  fetchCanvasComponents,
  fetchDashboardComponents,
  renderCanvas,
  pushComponent,
  updateComponent,
} from './molecules';
import { pawEngine } from '../../engine';
import type { CanvasComponent, CanvasComponentPatch, EngineEvent } from '../../engine/atoms/types';
import { staggerCards } from '../../components/animations';
import {
  initTabBar,
  loadTabs,
  renderTabBar,
  wireTabEvents,
  addTab,
  removeTab,
  getActiveTab,
} from '../../components/molecules/canvas-tabs';

// ── State ─────────────────────────────────────────────────────────────

let _components: ParsedCanvasComponent[] = [];
let _sessionId: string | null = null;
let _dashboardId: string | null = null;
let _subscribed = false;

// ── State bridge ──────────────────────────────────────────────────────

const { setMoleculesState } = initMoleculesState();
setMoleculesState({
  getComponents: () => _components,
  setComponents: (c: ParsedCanvasComponent[]) => {
    _components = c;
  },
  getSessionId: () => _sessionId,
  getDashboardId: () => _dashboardId,
  getTabBarHtml: () => renderTabBar(),
  wireTabBar: () => wireTabEvents(),
});

// ── Init Tab Bar ──────────────────────────────────────────────────────

initTabBar('main', {
  onActivate: async (_tabId, dashboardId) => {
    _dashboardId = dashboardId;
    _sessionId = null;
    await fetchDashboardComponents(dashboardId);
    renderCanvas();
    staggerCards('.canvas-card');
  },
  onClose: async (tabId) => {
    await removeTab(tabId);
    const active = getActiveTab();
    if (active) {
      _dashboardId = active.dashboardId;
      _sessionId = null;
      await fetchDashboardComponents(active.dashboardId);
    } else {
      _dashboardId = null;
      _components = [];
    }
    renderCanvas();
  },
  onNew: async () => {
    // Open dashboard picker — list saved dashboards
    try {
      const dashboards = await pawEngine.listDashboards();
      if (dashboards.length === 0) {
        console.debug('[canvas] No saved dashboards to open');
        return;
      }
      // Open the first available dashboard as a new tab (future: show picker UI)
      const dash = dashboards[0];
      const tab = await addTab(dash.id, dash);
      _dashboardId = tab.dashboardId;
      _sessionId = null;
      await fetchDashboardComponents(tab.dashboardId);
      renderCanvas();
      staggerCards('.canvas-card');
    } catch (e) {
      console.error('[canvas] Failed to open new tab:', e);
    }
  },
});

// ── Public API ────────────────────────────────────────────────────────

/** Set the active session and load its canvas. */
export async function loadCanvas(sessionId?: string): Promise<void> {
  if (sessionId) {
    _sessionId = sessionId;
    _dashboardId = null;
  }
  console.debug('[canvas] loadCanvas called, session:', _sessionId);

  // Ensure event subscriptions are wired
  if (!_subscribed) {
    subscribeToEvents();
    _subscribed = true;
  }

  // Load tabs from persistence
  await loadTabs();

  // Fetch persisted components for this session
  await fetchCanvasComponents();

  // Render the full view
  renderCanvas();

  // Animate cards in
  staggerCards('.canvas-card');
}

/** Load a specific dashboard (called from sidebar or agent event). */
export async function loadDashboard(dashboardId: string): Promise<void> {
  _dashboardId = dashboardId;
  _sessionId = null;

  if (!_subscribed) {
    subscribeToEvents();
    _subscribed = true;
  }

  // Open as a tab if not already open
  const dash = await pawEngine.getDashboard(dashboardId);
  if (dash) {
    await addTab(dashboardId, dash);
  }

  await fetchDashboardComponents(dashboardId);
  renderCanvas();
  staggerCards('.canvas-card');
}

/** Set the session ID without loading (used when switching sessions). */
export function setSession(sessionId: string): void {
  _sessionId = sessionId;
  _dashboardId = null;
}

// ── Engine Event Subscriptions ────────────────────────────────────────

function subscribeToEvents(): void {
  // Live push: agent added a new component
  pawEngine.on('canvas_push', (event: EngineEvent) => {
    const componentId = event.component_id as string;
    const component = event.component as CanvasComponent;

    // Match by session OR dashboard
    const matchSession = _sessionId && event.session_id === _sessionId;
    const ev = event as unknown as Record<string, unknown>;
    const matchDashboard = _dashboardId && ev.dashboard_id === _dashboardId;
    if (matchSession || matchDashboard) {
      pushComponent(componentId, component);
    }
  });

  // Live update: agent patched an existing component
  pawEngine.on('canvas_update', (event: EngineEvent) => {
    const componentId = event.component_id as string;
    const patch = event.patch as CanvasComponentPatch;

    const matchSession = _sessionId && event.session_id === _sessionId;
    const ev = event as unknown as Record<string, unknown>;
    const matchDashboard = _dashboardId && ev.dashboard_id === _dashboardId;
    if (matchSession || matchDashboard) {
      updateComponent(componentId, patch);
    }
  });

  // Dashboard saved → could auto-open as tab
  pawEngine.on(
    'dashboard-saved' as never,
    (() => {
      loadTabs(); // Refresh tab list
    }) as (event: EngineEvent) => void,
  );

  // Dashboard load (from agent tool)
  pawEngine.on(
    'dashboard-load' as never,
    ((event: unknown) => {
      const ev = event as Record<string, unknown>;
      const dashId = ev.dashboard_id as string;
      if (dashId) loadDashboard(dashId);
    }) as (event: EngineEvent) => void,
  );
}
