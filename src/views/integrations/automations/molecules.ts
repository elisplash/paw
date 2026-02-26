// src/views/integrations/automations/molecules.ts — DOM rendering
//
// Molecule-level: builds HTML, binds events, calls IPC.

import {
  filterTemplates,
  sortAutomations,
  TEMPLATE_CATEGORIES,
  type ActiveAutomation,
  type AutomationStatus,
  type TemplateCategory,
} from './atoms';
import { TEMPLATE_CATALOG, getTemplatesForService } from './templates';
import { svcName, activateTemplate, toggleAutomation, deleteAutomation } from './ipc';
import { renderTemplateCard, renderActiveCard } from './cards';
import { escHtml } from '../atoms';

// ── Module state (set by index.ts) ─────────────────────────────────────

interface MoleculesState {
  getConnectedIds: () => Set<string>;
  getActive: () => ActiveAutomation[];
  setActive: (a: ActiveAutomation[]) => void;
}

let _state: MoleculesState = {
  getConnectedIds: () => new Set(),
  getActive: () => [],
  setActive: () => {},
};

export function initAutomationsMoleculesState(): {
  setAutomationsMoleculesState: (s: MoleculesState) => void;
} {
  return {
    setAutomationsMoleculesState: (s) => {
      _state = s;
    },
  };
}

// ── Filter state ───────────────────────────────────────────────────────

let _tab: 'templates' | 'active' = 'templates';
let _searchQuery = '';
let _activeCategory: TemplateCategory | 'all' = 'all';
let _serviceFilter: string | undefined;

// ── Main render ────────────────────────────────────────────────────────

export function renderAutomations(container: HTMLElement): void {
  const active = _state.getActive();

  container.innerHTML = `
    <div class="automations-header">
      <h2><span class="ms ms-lg">auto_fix_high</span> Automations</h2>
      <p class="automations-subtitle">
        ${TEMPLATE_CATALOG.length} templates · ${active.length} active
      </p>
    </div>

    <div class="automations-tabs">
      <button class="automations-tab ${_tab === 'templates' ? 'active' : ''}" data-tab="templates">
        <span class="ms ms-sm">library_books</span> Templates
      </button>
      <button class="automations-tab ${_tab === 'active' ? 'active' : ''}" data-tab="active">
        <span class="ms ms-sm">play_circle</span> Active (${active.length})
      </button>
    </div>

    <div class="automations-body" id="automations-body"></div>
  `;

  _renderTab(container);
  _wireTabEvents(container);
}

// ── Tab rendering ──────────────────────────────────────────────────────

function _renderTab(container: HTMLElement): void {
  const body = container.querySelector('#automations-body') as HTMLElement;
  if (!body) return;

  if (_tab === 'templates') {
    _renderTemplatesTab(body);
  } else {
    _renderActiveTab(body);
  }
}

// ── Templates tab ──────────────────────────────────────────────────────

function _renderTemplatesTab(body: HTMLElement): void {
  const filtered = filterTemplates(TEMPLATE_CATALOG, {
    serviceId: _serviceFilter,
    category: _activeCategory,
    query: _searchQuery,
  });
  const connectedIds = _state.getConnectedIds();

  body.innerHTML = `
    <div class="automations-toolbar">
      <div class="automations-search-wrap">
        <span class="ms ms-sm">search</span>
        <input type="text" class="automations-search" id="auto-search"
               placeholder="Search templates…" value="${escHtml(_searchQuery)}" />
      </div>
    </div>

    <div class="automations-cat-pills" id="auto-cat-pills">
      <button class="integrations-cat-pill ${_activeCategory === 'all' ? 'active' : ''}" data-cat="all">All</button>
      ${TEMPLATE_CATEGORIES.map(
        (c) => `
        <button class="integrations-cat-pill ${_activeCategory === c.id ? 'active' : ''}" data-cat="${c.id}">
          <span class="ms ms-sm">${c.icon}</span>${c.label}
        </button>
      `,
      ).join('')}
    </div>

    <div class="automations-template-grid" id="auto-template-grid">
      ${
        filtered.length === 0
          ? `<div class="integrations-empty">
            <span class="ms ms-lg">search_off</span>
            <p>No templates match your search</p>
          </div>`
          : filtered.map((t) => renderTemplateCard(t, connectedIds)).join('')
      }
    </div>
  `;

  _wireTemplateEvents(body);
}

// ── Active automations tab ─────────────────────────────────────────────

function _renderActiveTab(body: HTMLElement): void {
  const active = sortAutomations(_state.getActive());

  if (active.length === 0) {
    body.innerHTML = `
      <div class="automations-empty-active">
        <span class="ms ms-xl">auto_fix_high</span>
        <h3>No active automations yet</h3>
        <p>Activate a template or ask your agent to build one.</p>
        <button class="btn btn-ghost btn-sm" id="switch-to-templates">
          <span class="ms ms-sm">library_books</span> Browse templates
        </button>
      </div>
    `;
    body.querySelector('#switch-to-templates')?.addEventListener('click', () => {
      _tab = 'templates';
      const parent = body.closest('.automations-panel') ?? body.parentElement!;
      renderAutomations(parent as HTMLElement);
    });
    return;
  }

  body.innerHTML = `
    <div class="automations-active-list">
      ${active.map((a) => renderActiveCard(a)).join('')}
    </div>
  `;

  _wireActiveEvents(body);
}

// ── Event wiring ───────────────────────────────────────────────────────

function _wireTabEvents(container: HTMLElement): void {
  container.querySelectorAll('.automations-tab').forEach((btn) => {
    btn.addEventListener('click', () => {
      _tab = (btn as HTMLElement).dataset.tab as 'templates' | 'active';
      renderAutomations(container);
    });
  });
}

function _wireTemplateEvents(body: HTMLElement): void {
  // Search
  const search = body.querySelector('#auto-search') as HTMLInputElement;
  search?.addEventListener('input', () => {
    _searchQuery = search.value;
    _renderTemplatesTab(body);
  });

  // Category pills
  body.querySelector('#auto-cat-pills')?.addEventListener('click', (e) => {
    const btn = (e.target as HTMLElement).closest('.integrations-cat-pill') as HTMLElement;
    if (!btn) return;
    _activeCategory = (btn.dataset.cat ?? 'all') as TemplateCategory | 'all';
    _renderTemplatesTab(body);
  });

  // Activate buttons
  body.querySelectorAll('.automation-activate-btn').forEach((btn) => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const tid = (btn as HTMLElement).dataset.templateId;
      const tpl = TEMPLATE_CATALOG.find((t) => t.id === tid);
      if (!tpl) return;
      try {
        const result = await activateTemplate(tpl);
        _state.setActive([..._state.getActive(), result]);
        _tab = 'active';
        const panel = document.querySelector('.automations-panel') as HTMLElement;
        if (panel) renderAutomations(panel);
      } catch (err) {
        console.error('Failed to activate template:', err);
      }
    });
  });
}

function _wireActiveEvents(body: HTMLElement): void {
  body.querySelectorAll('.auto-toggle-btn').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const id = (btn as HTMLElement).dataset.autoId!;
      const action = (btn as HTMLElement).dataset.action as 'pause' | 'resume';
      try {
        await toggleAutomation(id, action);
        const active = _state
          .getActive()
          .map((a) =>
            a.id !== id
              ? a
              : { ...a, status: (action === 'pause' ? 'paused' : 'active') as AutomationStatus },
          );
        _state.setActive(active);
        _renderActiveTab(body);
      } catch (err) {
        console.error('Failed to toggle automation:', err);
      }
    });
  });

  body.querySelectorAll('.auto-delete-btn').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const id = (btn as HTMLElement).dataset.autoId!;
      try {
        await deleteAutomation(id);
        _state.setActive(_state.getActive().filter((a) => a.id !== id));
        _renderActiveTab(body);
      } catch (err) {
        console.error('Failed to delete automation:', err);
      }
    });
  });
}

// ── Public: render templates for a specific service ────────────────────

export function renderServiceTemplates(container: HTMLElement, serviceId: string): void {
  const templates = getTemplatesForService(serviceId);
  const connectedIds = _state.getConnectedIds();

  if (templates.length === 0) {
    container.innerHTML = `
      <div class="automation-svc-empty">
        <p>No templates yet for this service.</p>
        <p class="automation-svc-hint">Ask your agent: "Set up an automation for ${svcName(serviceId)}"</p>
      </div>
    `;
    return;
  }

  container.innerHTML = `
    <div class="automation-svc-templates">
      ${templates.map((t) => renderTemplateCard(t, connectedIds)).join('')}
    </div>
  `;

  // Wire activate buttons
  container.querySelectorAll('.automation-activate-btn').forEach((btn) => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const tid = (btn as HTMLElement).dataset.templateId;
      const tpl = TEMPLATE_CATALOG.find((t) => t.id === tid);
      if (!tpl) return;
      try {
        const result = await activateTemplate(tpl);
        _state.setActive([..._state.getActive(), result]);
        _tab = 'active';
        const panel = document.querySelector('.automations-panel') as HTMLElement;
        if (panel) renderAutomations(panel);
      } catch (err) {
        console.error('Failed to activate template:', err);
      }
    });
  });
}

// ── Public: set service filter ─────────────────────────────────────────

export function setServiceFilter(serviceId: string | undefined): void {
  _serviceFilter = serviceId;
}
