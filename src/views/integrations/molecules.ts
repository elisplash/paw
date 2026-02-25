// src/views/integrations/molecules.ts — DOM rendering + event wiring
//
// Molecule-level: builds HTML, binds events, calls IPC.

import {
  escHtml, filterServices, sortServices, categoryLabel,
  CATEGORIES,
  type ServiceDefinition, type ServiceCategory, type SortOption,
  type ConnectedService,
} from './atoms';
import { SERVICE_CATALOG } from './catalog';

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
  return { setMoleculesState: (s) => { _state = s; } };
}

// ── Filter / sort state ────────────────────────────────────────────────

let _searchQuery = '';
let _activeCategory: ServiceCategory | 'all' = 'all';
let _sortOption: SortOption = 'popular';
let _viewMode: 'grid' | 'list' = 'grid';

// ── Main render ────────────────────────────────────────────────────────

export function renderIntegrations(): void {
  const container = document.getElementById('integrations-content');
  if (!container) return;

  const connected = _state.getConnected();
  const totalCount = SERVICE_CATALOG.length;
  const connectedCount = connected.length;

  container.innerHTML = `
    <div class="integrations-header">
      <div class="integrations-hero">
        <h1 class="integrations-title">
          <span class="ms ms-lg">integration_instructions</span>
          Integrations
        </h1>
        <p class="integrations-subtitle">
          ${totalCount}+ services. One click.
          ${connectedCount > 0 ? `<span class="integrations-connected-badge">${connectedCount} connected</span>` : ''}
        </p>
      </div>
    </div>

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
        (c) => `<button class="integrations-cat-pill ${_activeCategory === c.id ? 'active' : ''}" data-cat="${c.id}">
          <span class="ms ms-sm">${c.icon}</span>${c.label}
        </button>`,
      ).join('')}
    </div>

    <div class="integrations-grid ${_viewMode === 'list' ? 'integrations-list-mode' : ''}"
         id="integrations-grid">
    </div>

    <div class="integrations-detail-panel" id="integrations-detail" style="display:none;">
    </div>
  `;

  _renderCards();
  _wireEvents();
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

  grid.innerHTML = ordered.map((s) => {
    const isConnected = connectedIds.has(s.id);
    const conn = connected.find((c) => c.serviceId === s.id);
    return `
      <div class="integrations-card ${isConnected ? 'integrations-card-connected' : ''}"
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
          ${isConnected
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
  }).join('');
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
      ${isConnected
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
          ${service.setupGuide.steps.map((step) => `
            <li>
              ${step.link ? `<a href="${escHtml(step.link)}" target="_blank" rel="noopener">${escHtml(step.instruction)}</a>` : escHtml(step.instruction)}
              ${step.tip ? `<div class="integrations-guide-tip"><span class="ms ms-sm">lightbulb</span> ${escHtml(step.tip)}</div>` : ''}
            </li>
          `).join('')}
        </ol>
      </div>
    </div>

    <div class="integrations-detail-section">
      <h3><span class="ms ms-sm">chat</span> Ask Your Agent</h3>
      <div class="integrations-examples">
        ${service.queryExamples.map((q) => `<div class="integrations-example-chip">"${escHtml(q)}"</div>`).join('')}
      </div>
    </div>

    <div class="integrations-detail-section">
      <h3><span class="ms ms-sm">auto_fix_high</span> Automations</h3>
      <div class="integrations-examples">
        ${service.automationExamples.map((a) => `<div class="integrations-example-chip">${escHtml(a)}</div>`).join('')}
      </div>
    </div>

    ${service.docsUrl ? `
    <div class="integrations-detail-section">
      <a href="${escHtml(service.docsUrl)}" target="_blank" rel="noopener" class="integrations-docs-link">
        <span class="ms ms-sm">open_in_new</span> API Documentation
      </a>
    </div>` : ''}
  `;

  // Wire detail close
  document.getElementById('detail-close')?.addEventListener('click', () => {
    panel.style.display = 'none';
    _state.setSelectedService(null);
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
    document.querySelectorAll('.integrations-cat-pill').forEach((p) => p.classList.remove('active'));
    btn.classList.add('active');
    _renderCards();
  });

  // View mode toggle
  document.querySelectorAll('.integrations-view-toggle button').forEach((btn) => {
    btn.addEventListener('click', () => {
      _viewMode = (btn as HTMLElement).dataset.viewmode as 'grid' | 'list';
      const grid = document.getElementById('integrations-grid');
      grid?.classList.toggle('integrations-list-mode', _viewMode === 'list');
      document.querySelectorAll('.integrations-view-toggle button').forEach((b) => b.classList.remove('active'));
      btn.classList.add('active');
    });
  });

  // Card clicks → detail
  document.getElementById('integrations-grid')?.addEventListener('click', (e) => {
    const card = (e.target as HTMLElement).closest('.integrations-card') as HTMLElement;
    if (!card) return;
    const sid = card.dataset.serviceId;
    const service = SERVICE_CATALOG.find((s) => s.id === sid);
    if (service) {
      _state.setSelectedService(service);
      _renderDetail(service);
    }
  });
}
