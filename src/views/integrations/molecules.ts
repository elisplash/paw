// src/views/integrations/molecules.ts — DOM rendering + event wiring
//
// Molecule-level: builds HTML, binds events, calls IPC.

import {
  escHtml,
  filterServices,
  sortServices,
  categoryLabel,
  CATEGORIES,
  type ServiceDefinition,
  type ServiceCategory,
  type SortOption,
  type ConnectedService,
} from './atoms';
import { SERVICE_CATALOG } from './catalog';
import { openSetupGuide } from './setup-guide';
import { refreshConnected } from './index';
import { loadAutomations, loadServiceTemplates } from './automations';
import { loadQueryPanel, loadServiceQueries, setQueryConnectedIds } from './queries';
import { mountCommunityBrowser } from './community';
import { kineticStagger, kineticDot } from '../../components/kinetic-row';
import type { EngineSkillStatus, McpServerConfig, McpServerStatus } from '../../engine';

// ── Module state (set by index.ts) ─────────────────────────────────────

interface MoleculesState {
  getConnected: () => ConnectedService[];
  setSelectedService: (s: ServiceDefinition | null) => void;
  getSelectedService: () => ServiceDefinition | null;
}

let _state: MoleculesState = {
  getConnected: () => [],
  setSelectedService: () => {},
  getSelectedService: () => null,
};

export function initMoleculesState(): { setMoleculesState: (s: MoleculesState) => void } {
  return {
    setMoleculesState: (s) => {
      _state = s;
    },
  };
}

// ── Native integrations (engine skills + MCP) ──────────────────────────

let _mcpServers: McpServerConfig[] = [];
let _mcpStatuses: McpServerStatus[] = [];

export function setNativeIntegrations(
  _skills: EngineSkillStatus[],
  mcpServers: McpServerConfig[],
  mcpStatuses: McpServerStatus[],
): void {
  // _skills param kept for backward compat but native cards moved to Built In page
  _mcpServers = mcpServers;
  _mcpStatuses = mcpStatuses;
}

// ── Filter / sort state ────────────────────────────────────────────────

let _searchQuery = '';
let _activeCategory: ServiceCategory | 'all' = 'all';
let _sortOption: SortOption = 'popular';
let _viewMode: 'grid' | 'list' | 'matrix' = 'matrix';
let _mainTab: 'services' | 'automations' | 'queries' | 'community' = 'services';

// ── Main render ────────────────────────────────────────────────────────

export function renderIntegrations(): void {
  const container = document.getElementById('integrations-content');
  if (!container) return;

  container.innerHTML = `
    <div class="integrations-header">
      <div class="integrations-main-tabs">
        <button class="integrations-main-tab ${_mainTab === 'services' ? 'active' : ''}" data-main-tab="services">
          <span class="ms ms-sm">extension</span> Services
        </button>
        <button class="integrations-main-tab ${_mainTab === 'automations' ? 'active' : ''}" data-main-tab="automations">
          <span class="ms ms-sm">auto_fix_high</span> Automations
        </button>
        <button class="integrations-main-tab ${_mainTab === 'queries' ? 'active' : ''}" data-main-tab="queries">
          <span class="ms ms-sm">psychology</span> Queries
        </button>
        <button class="integrations-main-tab ${_mainTab === 'community' ? 'active' : ''}" data-main-tab="community">
          <span class="ms ms-sm">explore</span> Community
        </button>
      </div>
    </div>
    <div id="integrations-tab-body"></div>
  `;

  // Wire main tab switching
  container.querySelectorAll('.integrations-main-tab').forEach((btn) => {
    btn.addEventListener('click', () => {
      _mainTab = (btn as HTMLElement).dataset.mainTab as 'services' | 'automations' | 'queries' | 'community';
      renderIntegrations();
    });
  });

  const tabBody = container.querySelector('#integrations-tab-body') as HTMLElement;
  if (_mainTab === 'automations') {
    tabBody.innerHTML = '<div class="automations-panel"></div>';
    loadAutomations(tabBody.querySelector('.automations-panel')!);
  } else if (_mainTab === 'queries') {
    tabBody.innerHTML = '<div class="queries-panel"></div>';
    setQueryConnectedIds(new Set(_state.getConnected().map((c) => c.serviceId)));
    loadQueryPanel(tabBody.querySelector('.queries-panel')!);
  } else if (_mainTab === 'community') {
    tabBody.innerHTML = '<div class="community-panel"></div>';
    mountCommunityBrowser(tabBody.querySelector('.community-panel')!);
  } else {
    _renderServicesTab(tabBody);
  }
}

/** Render the services sub-tab with toolbar, categories, grid, and detail panel. */
function _renderServicesTab(tabBody: HTMLElement): void {
  const totalCount = SERVICE_CATALOG.length;

  tabBody.innerHTML = `
    <div class="integrations-toolbar">
      <div class="integrations-search-wrap">
        <span class="ms ms-sm">search</span>
        <input type="text" class="integrations-search" id="integrations-search"
               placeholder="Search ${totalCount}+ services…"
               value="${escHtml(_searchQuery)}" />
      </div>
      <div class="integrations-controls">
        <select class="integrations-sort" id="integrations-sort">
          <option value="popular" ${_sortOption === 'popular' ? 'selected' : ''}>Popular</option>
          <option value="a-z" ${_sortOption === 'a-z' ? 'selected' : ''}>A–Z</option>
          <option value="category" ${_sortOption === 'category' ? 'selected' : ''}>Category</option>
        </select>
        <div class="integrations-view-toggle">
          <button class="btn btn-ghost btn-sm ${_viewMode === 'matrix' ? 'active' : ''}"
                  data-viewmode="matrix" title="Matrix view">
            <span class="ms ms-sm">table_chart</span>
          </button>
          <button class="btn btn-ghost btn-sm ${_viewMode === 'grid' ? 'active' : ''}"
                  data-viewmode="grid" title="Grid view">
            <span class="ms ms-sm">grid_view</span>
          </button>
          <button class="btn btn-ghost btn-sm ${_viewMode === 'list' ? 'active' : ''}"
                  data-viewmode="list" title="List view">
            <span class="ms ms-sm">view_list</span>
          </button>
        </div>
      </div>
    </div>

    <div class="integrations-categories" id="integrations-categories">
      <button class="integrations-cat-pill ${_activeCategory === 'all' ? 'active' : ''}" data-cat="all">All</button>
      ${CATEGORIES.map(
        (
          c,
        ) => `<button class="integrations-cat-pill ${_activeCategory === c.id ? 'active' : ''}" data-cat="${c.id}">
          <span class="ms ms-sm">${c.icon}</span>${c.label}
        </button>`,
      ).join('')}
    </div>

    <div class="integrations-grid ${_viewMode === 'list' ? 'integrations-list-mode' : ''} ${_viewMode === 'matrix' ? 'integrations-matrix-mode' : ''}"
         id="integrations-grid">
    </div>

    <div class="integrations-detail-panel" id="integrations-detail" style="display:none;">
    </div>
  `;

  _renderNativeSection(tabBody);
  _renderCards();
  _wireEvents();
}

// ── Active integrations section (MCP servers only — native Rust tools live on Built In page) ──

function _renderNativeSection(tabBody: HTMLElement): void {
  const connectedMcp = _mcpStatuses.filter((s) => s.connected);

  if (connectedMcp.length === 0 && _mcpServers.length === 0) return;

  const sectionEl = document.createElement('div');
  sectionEl.className = 'native-integrations-section';

  // MCP server cards only (native Rust skills moved to Built In page)
  let cardsHtml = '';
  for (const server of _mcpServers) {
    const status = _mcpStatuses.find((s) => s.id === server.id);
    const isConnected = status?.connected ?? false;
    const toolCount = status?.tool_count ?? 0;

    cardsHtml += `
      <div class="native-card k-row k-spring ${isConnected ? 'k-breathe' : ''}">
        <div class="native-card-header">
          <span class="ms native-card-icon">dns</span>
          <div class="native-card-info">
            <span class="native-card-name">${escHtml(server.name)}</span>
            <span class="native-card-desc">MCP Server · ${escHtml(server.transport)}</span>
          </div>
          <div class="native-card-status ${isConnected ? 'native-status-active' : 'native-status-offline'}">
            <span class="ms ms-sm">${isConnected ? 'check_circle' : 'radio_button_unchecked'}</span>
            <span>${isConnected ? `Connected · ${toolCount} tools` : 'Offline'}</span>
          </div>
        </div>
      </div>`;
  }

  sectionEl.innerHTML = `
    <div class="native-section-header">
      <span class="ms native-section-icon">dns</span>
      <span class="native-section-title">MCP Servers</span>
      <span class="native-section-badge">${connectedMcp.length}/${_mcpServers.length} connected</span>
      <span class="native-section-sub">External tool providers via Model Context Protocol</span>
    </div>
    <div class="native-cards-grid">${cardsHtml}</div>
  `;

  // Insert before the toolbar
  const toolbar = tabBody.querySelector('.integrations-toolbar');
  if (toolbar) {
    tabBody.insertBefore(sectionEl, toolbar);
  } else {
    tabBody.prepend(sectionEl);
  }

  // Stagger animate
  kineticStagger(sectionEl, '.native-card');
}

// ── Card rendering ─────────────────────────────────────────────────────

function _renderCards(): void {
  const grid = document.getElementById('integrations-grid');
  if (!grid) return;

  const filtered = filterServices(SERVICE_CATALOG, _searchQuery, _activeCategory);
  const sorted = sortServices(filtered, _sortOption);
  const connected = _state.getConnected();
  const connectedIds = new Set(connected.map((c) => c.serviceId));

  if (sorted.length === 0) {
    grid.innerHTML = `
      <div class="integrations-empty">
        <span class="ms ms-lg">search_off</span>
        <p>No services match "${escHtml(_searchQuery)}"</p>
      </div>`;
    return;
  }

  // Pin connected services at top
  const pinned = sorted.filter((s) => connectedIds.has(s.id));
  const rest = sorted.filter((s) => !connectedIds.has(s.id));
  const ordered = [...pinned, ...rest];

  // Matrix view — compact 2-column service rows
  if (_viewMode === 'matrix') {
    grid.innerHTML = `
      <div class="matrix-grid">
        ${ordered
          .map((s) => {
            const isConnected = connectedIds.has(s.id);
            const conn = connected.find((c) => c.serviceId === s.id);
            return `<div class="matrix-row-card k-row k-spring${isConnected ? ` k-breathe k-status-${conn?.status === 'error' ? 'error' : conn?.status === 'expired' ? 'warning' : 'healthy'}` : ' k-status-idle'}" data-service-id="${s.id}">
            <span class="ms matrix-row-icon" style="color:${s.color}">${s.icon}</span>
            <div class="matrix-row-info">
              <span class="matrix-row-name">${escHtml(s.name)}</span>
              <span class="matrix-row-cat">${categoryLabel(s.category)}</span>
            </div>
            <div class="matrix-row-status">
              ${
                isConnected
                  ? `<span class="matrix-on">${kineticDot()} ON</span>`
                  : '<span class="matrix-off">OFF</span>'
              }
            </div>
            <div class="matrix-row-action">
              ${
                isConnected
                  ? `<button class="btn btn-ghost btn-sm integrations-card-btn" data-service-id="${s.id}">▸</button>`
                  : `<button class="btn btn-ghost btn-sm integrations-connect-btn" data-service-id="${s.id}">Setup</button>`
              }
            </div>
          </div>`;
          })
          .join('')}
      </div>
      <div class="matrix-footer">Showing ${ordered.length} of ${SERVICE_CATALOG.length} services</div>`;

    // Stagger rows
    const matrixGrid = grid.querySelector('.matrix-grid');
    if (matrixGrid) kineticStagger(matrixGrid as HTMLElement, '.matrix-row-card');
    return;
  }

  // Grid / List card view (existing)
  grid.innerHTML = ordered
    .map((s) => {
      const isConnected = connectedIds.has(s.id);
      const conn = connected.find((c) => c.serviceId === s.id);
      return `
      <div class="integrations-card k-row k-spring ${isConnected ? 'integrations-card-connected k-breathe k-oscillate k-status-healthy' : 'k-status-idle'}"
           data-service-id="${s.id}"
           style="--accent: ${s.color}">
        <div class="integrations-card-icon" style="background: ${s.color}15; color: ${s.color}">
          <span class="ms">${s.icon}</span>
        </div>
        <div class="integrations-card-body">
          <div class="integrations-card-name">${escHtml(s.name)}</div>
          <div class="integrations-card-cat">${categoryLabel(s.category)}</div>
          <div class="integrations-card-desc">${escHtml(s.description)}</div>
        </div>
        <div class="integrations-card-footer">
          ${
            isConnected
              ? `<span class="integrations-status connected">
                <span class="ms ms-sm">check_circle</span>
                Connected${conn ? ` · ${conn.toolCount} tools` : ''}
              </span>`
              : `<button class="btn btn-sm btn-ghost integrations-connect-btn" data-service-id="${s.id}">
                Connect
              </button>`
          }
        </div>
      </div>`;
    })
    .join('');

  // Apply staggered materialise to visible cards
  kineticStagger(grid, '.integrations-card');
}

// ── Detail panel ───────────────────────────────────────────────────────

function _renderDetail(service: ServiceDefinition): void {
  const panel = document.getElementById('integrations-detail');
  if (!panel) return;

  const connected = _state.getConnected();
  const isConnected = connected.some((c) => c.serviceId === service.id);

  panel.style.display = 'block';
  panel.innerHTML = `
    <div class="integrations-detail-header">
      <button class="btn btn-ghost btn-sm integrations-detail-close" id="detail-close">
        <span class="ms">close</span>
      </button>
      <div class="integrations-detail-icon" style="background: ${service.color}15; color: ${service.color}">
        <span class="ms ms-lg">${service.icon}</span>
      </div>
      <h2>${escHtml(service.name)}</h2>
      <span class="integrations-card-cat">${categoryLabel(service.category)}</span>
      <p>${escHtml(service.description)}</p>
      ${
        isConnected
          ? '<span class="integrations-status connected"><span class="ms ms-sm">check_circle</span> Connected</span>'
          : `<button class="btn btn-primary btn-sm" id="detail-connect-btn">
            <span class="ms ms-sm">power</span> Connect ${escHtml(service.name)}
          </button>`
      }
    </div>

    <div class="integrations-detail-section">
      <h3><span class="ms ms-sm">auto_awesome</span> What Your Agent Can Do</h3>
      <ul class="integrations-capabilities">
        ${service.capabilities.map((c) => `<li><span class="ms ms-sm">check</span> ${escHtml(c)}</li>`).join('')}
      </ul>
    </div>

    <div class="integrations-detail-section">
      <h3><span class="ms ms-sm">menu_book</span> Setup Guide</h3>
      <div class="integrations-guide">
        <div class="integrations-guide-time">
          <span class="ms ms-sm">schedule</span>
          ${escHtml(service.setupGuide.estimatedTime)}
        </div>
        <ol class="integrations-guide-steps">
          ${service.setupGuide.steps
            .map(
              (step) => `
            <li>
              ${step.link ? `<a href="${escHtml(step.link)}" target="_blank" rel="noopener">${escHtml(step.instruction)}</a>` : escHtml(step.instruction)}
              ${step.tip ? `<div class="integrations-guide-tip"><span class="ms ms-sm">lightbulb</span> ${escHtml(step.tip)}</div>` : ''}
            </li>
          `,
            )
            .join('')}
        </ol>
      </div>
    </div>

    <div class="integrations-detail-section">
      <h3><span class="ms ms-sm">psychology</span> Ask Your Agent</h3>
      <div id="detail-svc-queries"></div>
    </div>

    <div class="integrations-detail-section">
      <h3><span class="ms ms-sm">auto_fix_high</span> Automation Templates</h3>
      <div id="detail-svc-templates"></div>
    </div>

    ${
      service.docsUrl
        ? `
    <div class="integrations-detail-section">
      <a href="${escHtml(service.docsUrl)}" target="_blank" rel="noopener" class="integrations-docs-link">
        <span class="ms ms-sm">open_in_new</span> API Documentation
      </a>
    </div>`
        : ''
    }
  `;

  // Render service-specific query examples
  const queryContainer = document.getElementById('detail-svc-queries');
  if (queryContainer) {
    setQueryConnectedIds(new Set(_state.getConnected().map((c) => c.serviceId)));
    loadServiceQueries(queryContainer, service.id);
  }

  // Render service-specific automation templates
  const tplContainer = document.getElementById('detail-svc-templates');
  if (tplContainer) loadServiceTemplates(tplContainer, service.id);

  // Wire detail close
  document.getElementById('detail-close')?.addEventListener('click', () => {
    panel.style.display = 'none';
    _state.setSelectedService(null);
  });

  // Wire connect button → open setup guide
  document.getElementById('detail-connect-btn')?.addEventListener('click', () => {
    _openGuide(service);
  });
}

// ── Setup guide launcher ───────────────────────────────────────────────

function _openGuide(service: ServiceDefinition): void {
  const panel = document.getElementById('integrations-detail');
  if (!panel) return;
  panel.style.display = 'block';
  openSetupGuide(panel, service, {
    onSave: () => {
      // Re-fetch connected list from backend so cards show ON/Connected
      refreshConnected();
    },
    onClose: () => _renderDetail(service),
  });
}

// ── Event wiring ───────────────────────────────────────────────────────

function _wireEvents(): void {
  // Search
  const searchInput = document.getElementById('integrations-search') as HTMLInputElement;
  searchInput?.addEventListener('input', () => {
    _searchQuery = searchInput.value;
    _renderCards();
  });

  // Sort
  const sortSelect = document.getElementById('integrations-sort') as HTMLSelectElement;
  sortSelect?.addEventListener('change', () => {
    _sortOption = sortSelect.value as SortOption;
    _renderCards();
  });

  // Category pills
  document.getElementById('integrations-categories')?.addEventListener('click', (e) => {
    const btn = (e.target as HTMLElement).closest('.integrations-cat-pill') as HTMLElement;
    if (!btn) return;
    _activeCategory = (btn.dataset.cat ?? 'all') as ServiceCategory | 'all';
    document
      .querySelectorAll('.integrations-cat-pill')
      .forEach((p) => p.classList.remove('active'));
    btn.classList.add('active');
    _renderCards();
  });

  // View mode toggle
  document.querySelectorAll('.integrations-view-toggle button').forEach((btn) => {
    btn.addEventListener('click', () => {
      _viewMode = (btn as HTMLElement).dataset.viewmode as 'grid' | 'list' | 'matrix';
      const grid = document.getElementById('integrations-grid');
      if (grid) {
        grid.classList.toggle('integrations-list-mode', _viewMode === 'list');
        grid.classList.toggle('integrations-matrix-mode', _viewMode === 'matrix');
      }
      document
        .querySelectorAll('.integrations-view-toggle button')
        .forEach((b) => b.classList.remove('active'));
      btn.classList.add('active');
      _renderCards(); // re-render for matrix vs card
    });
  });

  // Card clicks → detail (or connect button → guide)
  document.getElementById('integrations-grid')?.addEventListener('click', (e) => {
    const target = e.target as HTMLElement;

    // If user clicked the "Connect" button directly, open the guide
    const connectBtn = target.closest('.integrations-connect-btn') as HTMLElement;
    if (connectBtn) {
      const sid = connectBtn.dataset.serviceId;
      const service = SERVICE_CATALOG.find((s) => s.id === sid);
      if (service) {
        _state.setSelectedService(service);
        _openGuide(service);
      }
      return;
    }

    // Otherwise open the detail panel
    const card = (target.closest('.integrations-card') ??
      target.closest('.matrix-row-card')) as HTMLElement;
    if (!card) return;
    const sid = card.dataset.serviceId;
    const service = SERVICE_CATALOG.find((s) => s.id === sid);
    if (service) {
      _state.setSelectedService(service);
      _renderDetail(service);
    }
  });
}
