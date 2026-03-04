// Canvas Tabs — Tab bar component for managing multiple dashboards.
// Follows VS Code tab pattern: click to switch, middle-click to close,
// overflow dropdown, context menu, and drag-to-reorder (future).

import { escHtml } from '../helpers';
import { pawEngine } from '../../engine';
import { showToast } from '../toast';
import type { DashboardTabRow, DashboardRow } from '../../engine/atoms/types';

// ── Types ─────────────────────────────────────────────────────────────

export interface TabInfo {
  tabId: string;
  dashboardId: string;
  name: string;
  icon: string;
  pinned: boolean;
  live: boolean;
  active: boolean;
}

export interface TabBarCallbacks {
  onActivate: (tabId: string, dashboardId: string) => void;
  onClose: (tabId: string) => void;
  onNew: () => void;
}

// ── State ─────────────────────────────────────────────────────────────

let _tabs: TabInfo[] = [];
let _callbacks: TabBarCallbacks | null = null;
let _windowId = 'main';

// ── Init ──────────────────────────────────────────────────────────────

/** Initialize the tab bar with callbacks. */
export function initTabBar(windowId: string, callbacks: TabBarCallbacks): void {
  _windowId = windowId;
  _callbacks = callbacks;
}

// ── Data Loading ──────────────────────────────────────────────────────

/** Load persisted tabs and resolve dashboard info. */
export async function loadTabs(): Promise<TabInfo[]> {
  try {
    const tabRows = await pawEngine.listTabs(_windowId);
    const dashboards = await pawEngine.listDashboards();
    const dashMap = new Map(dashboards.map((d) => [d.id, d]));

    _tabs = tabRows.map((t) => rowToTabInfo(t, dashMap));
    return _tabs;
  } catch (e) {
    console.warn('[canvas-tabs] Failed to load tabs:', e);
    return [];
  }
}

/** Convert a DashboardTabRow + dashboard metadata to TabInfo. */
function rowToTabInfo(tab: DashboardTabRow, dashMap: Map<string, DashboardRow>): TabInfo {
  const dash = dashMap.get(tab.dashboard_id);
  return {
    tabId: tab.id,
    dashboardId: tab.dashboard_id,
    name: dash?.name ?? 'Untitled',
    icon: dash?.icon ?? 'dashboard',
    pinned: dash?.pinned ?? false,
    live: !!dash?.refresh_interval,
    active: tab.active,
  };
}

// ── Rendering ─────────────────────────────────────────────────────────

/** Render the tab bar HTML. Returns empty string if no tabs. */
export function renderTabBar(): string {
  if (_tabs.length === 0) return '';

  const MAX_VISIBLE = 8;
  const visible = _tabs.slice(0, MAX_VISIBLE);
  const overflow = _tabs.slice(MAX_VISIBLE);

  const tabHtml = visible.map(renderTab).join('');

  const overflowHtml =
    overflow.length > 0
      ? `<div class="canvas-tab-overflow">
          <button class="btn btn-ghost btn-xs canvas-tab-overflow-btn" title="${overflow.length} more tabs">
            <span class="ms ms-sm">more_horiz</span>
          </button>
          <div class="canvas-tab-overflow-menu">${overflow.map(renderOverflowItem).join('')}</div>
        </div>`
      : '';

  return `
    <div class="canvas-tab-bar" data-window="${escHtml(_windowId)}">
      <div class="canvas-tab-list">${tabHtml}</div>
      ${overflowHtml}
      <button class="btn btn-ghost btn-xs canvas-tab-add" title="Open dashboard">
        <span class="ms ms-sm">add</span>
      </button>
    </div>
  `;
}

function renderTab(tab: TabInfo): string {
  const activeClass = tab.active ? ' canvas-tab-active' : '';
  const pinnedClass = tab.pinned ? ' canvas-tab-pinned' : '';
  const liveIndicator = tab.live
    ? '<span class="canvas-tab-live-dot" title="Live — auto-refreshing"></span>'
    : '';

  return `
    <div class="canvas-tab${activeClass}${pinnedClass}"
         data-tab-id="${escHtml(tab.tabId)}"
         data-dashboard-id="${escHtml(tab.dashboardId)}">
      <span class="ms ms-sm canvas-tab-icon">${escHtml(tab.icon)}</span>
      <span class="canvas-tab-label">${escHtml(tab.name)}</span>
      ${liveIndicator}
      ${!tab.pinned ? `<button class="canvas-tab-close" data-tab-id="${escHtml(tab.tabId)}" title="Close tab"><span class="ms ms-xs">close</span></button>` : ''}
    </div>
  `;
}

function renderOverflowItem(tab: TabInfo): string {
  return `
    <button class="canvas-tab-overflow-item" data-tab-id="${escHtml(tab.tabId)}" data-dashboard-id="${escHtml(tab.dashboardId)}">
      <span class="ms ms-sm">${escHtml(tab.icon)}</span>
      ${escHtml(tab.name)}
    </button>
  `;
}

// ── Event Wiring ──────────────────────────────────────────────────────

/** Wire up all tab bar event handlers. Call after DOM insert. */
export function wireTabEvents(): void {
  if (!_callbacks) return;
  const cbs = _callbacks;

  // Tab click → activate
  document.querySelectorAll<HTMLElement>('.canvas-tab').forEach((el) => {
    el.addEventListener('click', (e) => {
      const target = e.target as HTMLElement;
      // Don't activate if clicking close button
      if (target.closest('.canvas-tab-close')) return;

      const tabId = el.dataset.tabId ?? '';
      const dashId = el.dataset.dashboardId ?? '';
      activateTabLocal(tabId);
      cbs.onActivate(tabId, dashId);
    });

    // Middle-click → close
    el.addEventListener('auxclick', (e) => {
      if (e.button === 1) {
        const tabId = el.dataset.tabId ?? '';
        cbs.onClose(tabId);
      }
    });
  });

  // Close button click
  document.querySelectorAll<HTMLElement>('.canvas-tab-close').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const tabId = btn.dataset.tabId ?? '';
      cbs.onClose(tabId);
    });
  });

  // Add button → open dashboard picker
  const addBtn = document.querySelector('.canvas-tab-add');
  if (addBtn) {
    addBtn.addEventListener('click', () => cbs.onNew());
  }

  // Overflow item click
  document.querySelectorAll<HTMLElement>('.canvas-tab-overflow-item').forEach((btn) => {
    btn.addEventListener('click', () => {
      const tabId = btn.dataset.tabId ?? '';
      const dashId = btn.dataset.dashboardId ?? '';
      activateTabLocal(tabId);
      cbs.onActivate(tabId, dashId);
    });
  });

  // Overflow toggle
  const overflowBtn = document.querySelector('.canvas-tab-overflow-btn');
  if (overflowBtn) {
    overflowBtn.addEventListener('click', () => {
      const menu = document.querySelector('.canvas-tab-overflow-menu');
      menu?.classList.toggle('canvas-tab-overflow-open');
    });
  }
}

// ── Tab Operations ────────────────────────────────────────────────────

/** Locally activate a tab (update DOM classes). */
function activateTabLocal(tabId: string): void {
  document.querySelectorAll('.canvas-tab').forEach((el) => {
    el.classList.toggle('canvas-tab-active', el.getAttribute('data-tab-id') === tabId);
  });
  _tabs.forEach((t) => (t.active = t.tabId === tabId));
}

/** Add a tab and persist it. Returns the new tab info. */
export async function addTab(dashboardId: string, dashboard: DashboardRow): Promise<TabInfo> {
  const tabId = `tab-${Date.now().toString(36)}`;
  await pawEngine.openTab(tabId, dashboardId, _windowId);
  await pawEngine.activateTab(tabId, _windowId);

  const tab: TabInfo = {
    tabId,
    dashboardId,
    name: dashboard.name,
    icon: dashboard.icon,
    pinned: dashboard.pinned,
    live: !!dashboard.refresh_interval,
    active: true,
  };

  // Deactivate all others locally.
  _tabs.forEach((t) => (t.active = false));
  _tabs.push(tab);
  return tab;
}

/** Close a tab and persist the removal. */
export async function removeTab(tabId: string): Promise<void> {
  const idx = _tabs.findIndex((t) => t.tabId === tabId);
  if (idx === -1) return;

  const tab = _tabs[idx];
  if (tab.pinned) {
    showToast('Pinned tabs cannot be closed', 'warning');
    return;
  }

  await pawEngine.closeTab(tabId);
  _tabs.splice(idx, 1);

  // If the closed tab was active, activate the nearest tab.
  if (tab.active && _tabs.length > 0) {
    const next = _tabs[Math.min(idx, _tabs.length - 1)];
    next.active = true;
    await pawEngine.activateTab(next.tabId, _windowId);
    _callbacks?.onActivate(next.tabId, next.dashboardId);
  }
}

/** Get the currently active tab info. */
export function getActiveTab(): TabInfo | undefined {
  return _tabs.find((t) => t.active);
}

/** Get all current tabs. */
export function getTabs(): TabInfo[] {
  return _tabs;
}
