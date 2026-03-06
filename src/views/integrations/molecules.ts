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
import { invoke } from '@tauri-apps/api/core';
import { loadAutomations, loadServiceTemplates } from './automations';
import { loadQueryPanel, loadServiceQueries, setQueryConnectedIds } from './queries';
import {
  mountCommunityBrowser,
  getRequiredPackage,
  displayName as communityDisplayName,
} from './community';
import { kineticStagger } from '../../components/kinetic-row';
import {
  pawEngine,
  type EngineSkillStatus,
  type McpServerConfig,
  type McpServerStatus,
} from '../../engine';
import { showToast } from '../../components/toast';

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

// ── Active integrations (MCP servers + native skills) ──────────────

let _mcpServers: McpServerConfig[] = [];
let _mcpStatuses: McpServerStatus[] = [];
let _nativeSkills: EngineSkillStatus[] = [];

export function setNativeIntegrations(
  skills: EngineSkillStatus[],
  mcpServers: McpServerConfig[],
  mcpStatuses: McpServerStatus[],
): void {
  _nativeSkills = skills;
  _mcpServers = mcpServers;
  _mcpStatuses = mcpStatuses;
}

// ── Filter / sort state ────────────────────────────────────────────────

let _searchQuery = '';
let _activeCategory: ServiceCategory | 'all' = 'all';
let _sortOption: SortOption = 'popular';
let _viewMode: 'grid' | 'list' | 'matrix' = 'matrix';
let _mainTab: 'services' | 'automations' | 'queries' | 'community' = 'services';

// ── OAuth availability cache ───────────────────────────────────────────
// Only services with real Client IDs (not placeholder REPLACE_WITH_) are
// in this set. Populated once on init via engine_oauth_services.

const _configuredOAuthIds = new Set<string>();

/**
 * Fetch which OAuth services are actually configured (have real client IDs)
 * and cache the result. Services without client IDs fall back to manual setup.
 */
export async function fetchOAuthAvailability(): Promise<void> {
  try {
    const services =
      await invoke<{ service_id: string; configured: boolean }[]>('engine_oauth_services');
    _configuredOAuthIds.clear();
    for (const svc of services) {
      if (svc.configured) {
        _configuredOAuthIds.add(svc.service_id);
      }
    }
  } catch (e) {
    console.warn('[integrations] Failed to fetch OAuth availability:', e);
  }
}

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
      _mainTab = (btn as HTMLElement).dataset.mainTab as
        | 'services'
        | 'automations'
        | 'queries'
        | 'community';
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
  _renderBuiltInSection(tabBody);
  _renderCards();
  _wireEvents();

  // If a service was pre-selected (e.g. from the Connected drawer), auto-open its detail
  const pending = _state.getSelectedService();
  if (pending) {
    const isConnected = _state.getConnected().some((c) => c.serviceId === pending.id);
    if (isConnected) {
      _renderConnectedDetail(pending);
    } else {
      _renderDetail(pending);
    }
    // Clear the selection so subsequent re-renders don't re-open the panel
    _state.setSelectedService(null);
  }
}

// ── Active integrations section (MCP servers) ──

function _renderNativeSection(tabBody: HTMLElement): void {
  const connectedMcp = _mcpStatuses.filter((s) => s.connected);

  if (connectedMcp.length === 0 && _mcpServers.length === 0) return;

  const sectionEl = document.createElement('div');
  sectionEl.className = 'native-integrations-section';

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

// ── Built-in tools section ─────────────────────────────────────────────

/** Map engine skill category to readable label */
function _skillCategoryLabel(cat: string): string {
  const map: Record<string, string> = {
    vault: 'Service Integrations',
    api: 'API Integrations',
    productivity: 'Productivity',
    development: 'Development',
    media: 'Media & Audio',
    smart_home: 'Smart Home & IoT',
    communication: 'Communication',
    cli: 'CLI Tools',
    system: 'System & Security',
  };
  return map[cat] ?? cat;
}

/** Pick a color for a skill based on category */
function _skillCategoryColor(cat: string): string {
  const map: Record<string, string> = {
    vault: '#6366f1',
    api: '#8b5cf6',
    productivity: '#3b82f6',
    development: '#10b981',
    media: '#f59e0b',
    smart_home: '#06b6d4',
    communication: '#ec4899',
    cli: '#64748b',
    system: '#ef4444',
  };
  return map[cat] ?? '#6366f1';
}

/** Icon mapping for skill emojis → material symbols */
function _skillMaterialIcon(emoji: string): string {
  const map: Record<string, string> = {
    '✈️': 'send',
    '🔌': 'power',
    '🪝': 'webhook',
    '🎮': 'sports_esports',
    '🪙': 'monetization_on',
    '📝': 'edit_note',
    '⏰': 'alarm',
    '✅': 'check_circle',
    '💎': 'diamond',
    '🐻': 'pets',
    '🧵': 'terminal',
    '📜': 'description',
    '🎙️': 'mic',
    '☁️': 'cloud',
    '🖼️': 'image',
    '🎞️': 'movie',
    '🗣️': 'record_voice_over',
    '💡': 'lightbulb',
    '🔊': 'volume_up',
    '🎛️': 'tune',
    '📱': 'phone_android',
    '📨': 'forward_to_inbox',
    '🌤️': 'wb_sunny',
    '📰': 'feed',
    '🔐': 'lock',
    '🎵': 'music_note',
    '📍': 'place',
    '👀': 'visibility',
    '🛡️': 'shield',
    '🧾': 'summarize',
    '🧲': 'gif_box',
    '📸': 'photo_camera',
    '🦄': 'swap_horiz',
    '☀️': 'light_mode',
  };
  return map[emoji] ?? 'extension';
}

function _renderBuiltInSection(tabBody: HTMLElement): void {
  if (_nativeSkills.length === 0) return;

  // Group skills by category
  const grouped: Record<string, EngineSkillStatus[]> = {};
  for (const skill of _nativeSkills) {
    const cat = skill.category;
    if (!grouped[cat]) grouped[cat] = [];
    grouped[cat].push(skill);
  }

  // Category order
  const catOrder = [
    'vault',
    'api',
    'productivity',
    'communication',
    'media',
    'smart_home',
    'development',
    'cli',
    'system',
  ];
  const sortedCats = catOrder.filter((c) => grouped[c]);

  const readyCount = _nativeSkills.filter((s) => s.is_ready).length;
  const enabledCount = _nativeSkills.filter((s) => s.enabled).length;

  let categoriesHtml = '';
  for (const cat of sortedCats) {
    const skills = grouped[cat];
    const catColor = _skillCategoryColor(cat);

    const cardsHtml = skills
      .map((skill) => {
        const icon = _skillMaterialIcon(skill.icon);
        const isReady = skill.is_ready;
        const isEnabled = skill.enabled;
        const hasMissingBins = skill.missing_binaries.length > 0;
        const hasMissingCreds = skill.missing_credentials.length > 0;

        let statusDot = 'native-status-offline';
        let statusText = 'Disabled';
        let statusIcon = 'radio_button_unchecked';
        if (isReady) {
          statusDot = 'native-status-active';
          statusText = `Ready · ${skill.tool_names.length || '∞'} tools`;
          statusIcon = 'check_circle';
        } else if (isEnabled && hasMissingBins) {
          statusDot = 'native-status-warning';
          statusText = 'Missing CLI';
          statusIcon = 'warning';
        } else if (isEnabled && hasMissingCreds) {
          statusDot = 'native-status-warning';
          statusText = 'Needs Setup';
          statusIcon = 'key';
        } else if (isEnabled) {
          statusDot = 'native-status-active';
          statusText = 'Enabled';
          statusIcon = 'toggle_on';
        }

        return `
        <div class="builtin-card k-row k-spring${isReady ? ' k-breathe' : ''}" data-builtin-id="${escHtml(skill.id)}">
          <div class="native-card-header">
            <span class="ms native-card-icon" style="color:${catColor}">${icon}</span>
            <div class="native-card-info">
              <span class="native-card-name">${escHtml(skill.name)}</span>
              <span class="native-card-desc">${escHtml(skill.description)}</span>
            </div>
            <div class="native-card-status ${statusDot}">
              <span class="ms ms-sm">${statusIcon}</span>
              <span>${statusText}</span>
            </div>
          </div>
        </div>`;
      })
      .join('');

    categoriesHtml += `
      <div class="builtin-category">
        <div class="builtin-category-header">
          <span class="builtin-cat-dot" style="background:${catColor}"></span>
          <span class="builtin-cat-label">${_skillCategoryLabel(cat)}</span>
          <span class="builtin-cat-count">${skills.length}</span>
        </div>
        <div class="native-cards-grid">${cardsHtml}</div>
      </div>`;
  }

  const sectionEl = document.createElement('div');
  sectionEl.className = 'native-integrations-section builtin-tools-section';
  sectionEl.innerHTML = `
    <div class="native-section-header">
      <span class="ms native-section-icon" style="color:var(--accent)">memory</span>
      <span class="native-section-title">Built-In Tools</span>
      <span class="native-section-badge">${readyCount} ready · ${enabledCount} enabled</span>
      <span class="native-section-sub">Native tools compiled into the app — no plugins needed</span>
      <button class="btn btn-ghost btn-sm builtin-toggle-btn" id="builtin-toggle-btn">
        <span class="ms ms-sm">expand_more</span>
      </button>
    </div>
    <div class="builtin-categories builtin-collapsed" id="builtin-categories">${categoriesHtml}</div>
  `;

  // Insert after MCP section (before toolbar), or before toolbar directly
  const mcpSection = tabBody.querySelector(
    '.native-integrations-section:not(.builtin-tools-section)',
  );
  const toolbar = tabBody.querySelector('.integrations-toolbar');
  if (mcpSection?.nextSibling) {
    tabBody.insertBefore(sectionEl, mcpSection.nextSibling);
  } else if (toolbar) {
    tabBody.insertBefore(sectionEl, toolbar);
  } else {
    tabBody.prepend(sectionEl);
  }

  // Stagger animate
  kineticStagger(sectionEl, '.builtin-card');

  // Toggle collapse
  const toggleBtn = document.getElementById('builtin-toggle-btn');
  const categoriesEl = document.getElementById('builtin-categories');
  if (toggleBtn && categoriesEl) {
    toggleBtn.addEventListener('click', () => {
      const isCollapsed = categoriesEl.classList.toggle('builtin-collapsed');
      toggleBtn.innerHTML = `<span class="ms ms-sm">${isCollapsed ? 'expand_more' : 'expand_less'}</span>`;
    });
  }

  // Wire card clicks
  sectionEl.querySelectorAll('.builtin-card').forEach((card) => {
    card.addEventListener('click', () => {
      const skillId = (card as HTMLElement).dataset.builtinId;
      const skill = _nativeSkills.find((s) => s.id === skillId);
      if (skill) _renderBuiltInDetail(skill);
    });
  });
}

// ── Built-in skill detail panel ────────────────────────────────────────

function _renderBuiltInDetail(skill: EngineSkillStatus): void {
  const panel = document.getElementById('integrations-detail');
  if (!panel) return;

  const catColor = _skillCategoryColor(skill.category);
  const icon = _skillMaterialIcon(skill.icon);
  const isEnabled = skill.enabled;
  const isReady = skill.is_ready;
  const hasMissingBins = skill.missing_binaries.length > 0;
  const hasMissingCreds = skill.missing_credentials.length > 0;

  // Build status HTML
  let statusHtml = '';
  if (isReady) {
    statusHtml = `<span class="integrations-status connected"><span class="ms ms-sm">check_circle</span> Ready${skill.tool_names.length ? ` · ${skill.tool_names.length} tools` : ''}</span>`;
  } else if (isEnabled && hasMissingBins) {
    statusHtml = `<span class="integrations-status" style="color:var(--warning)"><span class="ms ms-sm">warning</span> Missing: ${skill.missing_binaries.map(escHtml).join(', ')}</span>`;
  } else if (isEnabled && hasMissingCreds) {
    statusHtml = `<span class="integrations-status" style="color:var(--warning)"><span class="ms ms-sm">key</span> Needs credentials</span>`;
  }

  // Build credentials form
  let credsHtml = '';
  if (skill.required_credentials && skill.required_credentials.length > 0) {
    const fields = skill.required_credentials
      .map((cred) => {
        const isConfigured = skill.configured_credentials.includes(cred.key);
        return `
        <div class="builtin-cred-field">
          <label class="form-label">
            ${escHtml(cred.label)}
            ${cred.required ? '<span style="color:var(--error)">*</span>' : ''}
            ${isConfigured ? '<span class="ms ms-sm" style="color:var(--success);font-size:14px;vertical-align:middle">check_circle</span>' : ''}
          </label>
          <p class="form-hint" style="margin:0 0 4px;font-size:11px;color:var(--text-muted)">${escHtml(cred.description)}</p>
          <div class="builtin-cred-input-row">
            <input type="password" class="form-input builtin-cred-input"
                   data-skill-id="${escHtml(skill.id)}" data-cred-key="${escHtml(cred.key)}"
                   placeholder="${isConfigured ? '••••••••' : escHtml(cred.placeholder || '')}"
                   value="" />
            <button class="btn btn-sm btn-ghost builtin-cred-save" data-skill-id="${escHtml(skill.id)}" data-cred-key="${escHtml(cred.key)}">
              <span class="ms ms-sm">save</span>
            </button>
          </div>
        </div>`;
      })
      .join('');

    credsHtml = `
      <div class="integrations-detail-section">
        <h3><span class="ms ms-sm">key</span> Credentials</h3>
        <div class="builtin-creds-form">${fields}</div>
      </div>`;
  }

  // Install hint
  let installHtml = '';
  if (skill.install_hint) {
    installHtml = `
      <div class="integrations-detail-section">
        <h3><span class="ms ms-sm">download</span> Installation</h3>
        <div class="builtin-install-hint">
          <code>${escHtml(skill.install_hint)}</code>
        </div>
      </div>`;
  }

  // Missing binaries
  let binsHtml = '';
  if (hasMissingBins) {
    binsHtml = `
      <div class="integrations-detail-section">
        <h3><span class="ms ms-sm">warning</span> Missing CLI Tools</h3>
        <div class="builtin-missing-bins">
          ${skill.missing_binaries.map((b) => `<span class="builtin-bin-tag missing">${escHtml(b)}</span>`).join(' ')}
        </div>
        ${skill.install_hint ? `<p style="margin:8px 0 0;font-size:12px;color:var(--text-muted)">Install: <code>${escHtml(skill.install_hint)}</code></p>` : ''}
      </div>`;
  }

  // Tool names
  let toolsHtml = '';
  if (skill.tool_names.length > 0) {
    toolsHtml = `
      <div class="integrations-detail-section">
        <h3><span class="ms ms-sm">construction</span> Available Tools (${skill.tool_names.length})</h3>
        <div class="builtin-tools-list">
          ${skill.tool_names.map((t) => `<span class="builtin-tool-tag">${escHtml(t)}</span>`).join(' ')}
        </div>
      </div>`;
  }

  panel.style.display = 'block';
  panel.innerHTML = `
    <div class="integrations-detail-header">
      <button class="btn btn-ghost btn-sm integrations-detail-close" id="detail-close">
        <span class="ms">close</span>
      </button>
      <div class="integrations-detail-icon" style="background: ${catColor}15; color: ${catColor}">
        <span class="ms ms-lg">${icon}</span>
      </div>
      <h2>${escHtml(skill.name)}</h2>
      <span class="integrations-card-cat">${_skillCategoryLabel(skill.category)} · Built-In</span>
      <p>${escHtml(skill.description)}</p>
      ${statusHtml}
      <div class="builtin-enable-row" style="margin-top:12px">
        <label class="form-switch">
          <input type="checkbox" id="builtin-enable-toggle" ${isEnabled ? 'checked' : ''} />
          <span class="form-switch-slider"></span>
        </label>
        <span style="font-size:13px;color:var(--text-secondary)">${isEnabled ? 'Enabled' : 'Disabled'}</span>
      </div>
    </div>

    ${credsHtml}
    ${binsHtml}
    ${installHtml}
    ${toolsHtml}
  `;

  // Wire close
  document.getElementById('detail-close')?.addEventListener('click', () => {
    panel.style.display = 'none';
  });

  // Wire enable toggle
  const enableToggle = document.getElementById('builtin-enable-toggle') as HTMLInputElement;
  enableToggle?.addEventListener('change', async () => {
    try {
      await pawEngine.skillSetEnabled(skill.id, enableToggle.checked);
      showToast(`${skill.name} ${enableToggle.checked ? 'enabled' : 'disabled'}`, 'success');
      // Update local state and re-render
      skill.enabled = enableToggle.checked;
      _renderBuiltInDetail(skill);
    } catch (e) {
      showToast(`Failed: ${e}`, 'error');
      enableToggle.checked = !enableToggle.checked;
    }
  });

  // Wire credential save buttons
  panel.querySelectorAll('.builtin-cred-save').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const el = btn as HTMLElement;
      const skillId = el.dataset.skillId!;
      const key = el.dataset.credKey!;
      const input = panel.querySelector(
        `.builtin-cred-input[data-skill-id="${skillId}"][data-cred-key="${key}"]`,
      ) as HTMLInputElement;
      if (!input || !input.value.trim()) {
        showToast('Enter a value first', 'warning');
        return;
      }
      try {
        await pawEngine.skillSetCredential(skillId, key, input.value.trim());
        showToast(`${key} saved`, 'success');
        input.value = '';
        input.placeholder = '••••••••';
        // Mark as configured
        if (!skill.configured_credentials.includes(key)) {
          skill.configured_credentials.push(key);
        }
        skill.missing_credentials = skill.missing_credentials.filter((k) => k !== key);
        // Re-render to update status
        _renderBuiltInDetail(skill);
      } catch (e) {
        showToast(`Save failed: ${e}`, 'error');
      }
    });
  });
}

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
                  ? `<span class="matrix-on"><span class="integrations-live-dot"></span> ON</span>`
                  : '<span class="matrix-off">OFF</span>'
              }
            </div>
            <div class="matrix-row-action">
              ${
                isConnected
                  ? `<button class="btn btn-ghost btn-sm integrations-connect-btn" data-service-id="${s.id}">Edit</button>`
                  : `<button class="btn btn-ghost btn-sm integrations-connect-btn" data-service-id="${s.id}">${s.authType === 'oauth' && _configuredOAuthIds.has(s.id) ? 'Connect' : 'Setup'}</button>`
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
                <span class="integrations-live-dot"></span>
                Connected${conn ? ` · ${conn.toolCount ?? 0} tools` : ''}
              </span>
              <button class="btn btn-sm btn-ghost integrations-connect-btn" data-service-id="${s.id}">Edit</button>`
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
          : service.authType === 'oauth' && _configuredOAuthIds.has(service.id)
            ? `<div class="integrations-connect-options">
                <button class="btn btn-primary btn-sm" id="detail-oauth-btn">
                  <span class="ms ms-sm">link</span> Connect ${escHtml(service.name)}
                </button>
                <button class="btn btn-ghost btn-xs" id="detail-manual-btn" title="Manual API key setup">
                  <span class="ms ms-sm">key</span> Enter credentials manually
                </button>
              </div>`
            : service.authType === 'oauth' && !_configuredOAuthIds.has(service.id)
              ? `<div class="integrations-connect-options">
                  <div class="integrations-oauth-unavailable">
                    <span class="ms ms-sm">info</span>
                    <span>OAuth not configured for this build. Use API key setup below.</span>
                  </div>
                  <button class="btn btn-primary btn-sm" id="detail-connect-btn">
                    <span class="ms ms-sm">key</span> Setup ${escHtml(service.name)}
                  </button>
                </div>`
              : service.authType === 'n8n-oauth'
                ? `<div class="integrations-connect-options">
                  <button class="btn btn-primary btn-sm btn-n8n-oauth" id="detail-n8n-oauth-btn">
                    <span class="ms ms-sm">swap_horiz</span> Connect via n8n
                  </button>
                  <button class="btn btn-ghost btn-xs" id="detail-manual-btn" title="Manual API key setup">
                    <span class="ms ms-sm">key</span> Enter credentials manually
                  </button>
                </div>`
                : service.authType === 'rfc7591'
                  ? `<div class="integrations-connect-options">
                    <button class="btn btn-primary btn-sm btn-rfc7591" id="detail-rfc7591-btn">
                      <span class="ms ms-sm">auto_fix_high</span> Auto-connect ${escHtml(service.name)}
                    </button>
                    <button class="btn btn-ghost btn-xs" id="detail-manual-btn" title="Manual API key setup">
                      <span class="ms ms-sm">key</span> Enter credentials manually
                    </button>
                  </div>`
                  : `<button class="btn btn-primary btn-sm" id="detail-connect-btn">
                    <span class="ms ms-sm">power</span> Connect ${escHtml(service.name)}
                  </button>`
      }
    </div>

    <div id="community-package-banner"></div>

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

  // Wire connect button → open setup guide (API key path)
  document.getElementById('detail-connect-btn')?.addEventListener('click', () => {
    _openGuide(service);
  });

  // Wire OAuth connect button → launch OAuth PKCE flow
  document.getElementById('detail-oauth-btn')?.addEventListener('click', () => {
    _startOAuthFlow(service);
  });

  // Wire n8n OAuth delegation button → open n8n credential UI
  document.getElementById('detail-n8n-oauth-btn')?.addEventListener('click', () => {
    _startN8nOAuthFlow(service);
  });

  // Wire RFC 7591 auto-connect button → dynamic registration + PKCE
  document.getElementById('detail-rfc7591-btn')?.addEventListener('click', () => {
    _startRfc7591Flow(service);
  });

  // Wire manual fallback for OAuth services → same setup guide as API key
  document.getElementById('detail-manual-btn')?.addEventListener('click', () => {
    _openGuide(service);
  });

  // Show community package banner if applicable
  _renderCommunityBanner(service);
}

// ── Connected service detail (Edit view) ─────────────────────────────

/**
 * Render a detail panel for a connected service showing stored credentials,
 * connection URL, test button, and management actions. Mirrors the Dashboard's
 * "Jira — Connection" view.
 */
async function _renderConnectedDetail(service: ServiceDefinition): Promise<void> {
  const panel = document.getElementById('integrations-detail');
  if (!panel) return;

  const conn = _state.getConnected().find((c) => c.serviceId === service.id);

  panel.style.display = 'block';
  panel.innerHTML = `
    <div class="integrations-detail-header">
      <button class="btn btn-ghost btn-sm integrations-detail-close" id="detail-close">
        <span class="ms">close</span>
      </button>
      <div class="integrations-detail-icon" style="background: ${service.color}15; color: ${service.color}">
        <span class="ms ms-lg">${service.icon}</span>
      </div>
      <h2>${escHtml(service.name)} — Connection</h2>
      <span class="integrations-card-cat">${categoryLabel(service.category)}</span>
      <span class="integrations-status connected"><span class="ms ms-sm">check_circle</span> Connected${conn ? ` · ${conn.toolCount ?? 0} tools` : ''}</span>
    </div>
    <div class="integrations-detail-section" id="connected-creds-section">
      <div style="font-size:13px;color:var(--text-secondary);display:flex;align-items:center;gap:6px;padding:8px 0">
        <span class="ms ms-sm" style="animation:spin 1s linear infinite">sync</span> Loading credentials…
      </div>
    </div>
  `;

  // Wire close
  document.getElementById('detail-close')?.addEventListener('click', () => {
    panel.style.display = 'none';
    _state.setSelectedService(null);
  });

  // Fetch stored credentials
  let creds: Record<string, string> = {};
  try {
    creds = await invoke<Record<string, string>>('engine_integrations_get_credentials', {
      serviceId: service.id,
    });
  } catch {
    /* no creds stored */
  }

  // Derive connection URL
  const domain = creds.domain || creds.url || creds.base_url || '';
  let urlValue = '';
  if (domain) {
    const base = domain.startsWith('http') ? domain : `https://${domain}`;
    if (service.id === 'jira') {
      urlValue = `${base.replace(/\/$/, '')}/rest/api/3`;
    } else {
      urlValue = base;
    }
  }

  const hasCreds = Object.keys(creds).length > 0;

  // Build masked credential rows
  const credRows = Object.entries(creds)
    .map(([key, val]) => {
      const isSensitive =
        key.toLowerCase().includes('token') ||
        key.toLowerCase().includes('secret') ||
        key.toLowerCase().includes('key') ||
        key.toLowerCase().includes('password');
      const masked = isSensitive && val ? `••••••••${val.slice(-4)}` : val || '(empty)';
      const label = key.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
      return `
        <div class="svc-detail-conn-row" style="display:flex;justify-content:space-between;padding:6px 0;border-bottom:1px solid var(--border)">
          <span style="color:var(--text-secondary);font-size:13px">${escHtml(label)}</span>
          <span style="font-family:var(--font-mono);font-size:13px">${escHtml(masked)}</span>
        </div>`;
    })
    .join('');

  const credsSection = document.getElementById('connected-creds-section');
  if (!credsSection) return;

  credsSection.innerHTML = `
    ${
      urlValue
        ? `
      <div style="background:rgba(var(--accent-rgb,99,102,241),0.08);border:1px solid rgba(var(--accent-rgb,99,102,241),0.2);border-radius:8px;padding:10px 14px;display:flex;align-items:center;gap:8px;margin-bottom:12px">
        <span class="ms ms-sm" style="color:var(--accent)">link</span>
        <span style="font-size:13px;color:var(--text-secondary)">Connected URL</span>
        <code style="font-size:12px;color:var(--accent);flex:1;text-align:right">${escHtml(urlValue)}</code>
      </div>`
        : ''
    }

    ${
      hasCreds
        ? `
      <h3 style="display:flex;align-items:center;gap:6px;margin-bottom:8px">
        <span class="ms ms-sm">key</span> Stored Credentials
      </h3>
      ${credRows}
    `
        : `
      <div style="font-size:13px;color:var(--warning);display:flex;align-items:center;gap:6px;padding:8px 0">
        <span class="ms" style="font-size:16px">warning</span>
        No credentials found. Please reconfigure this integration.
      </div>
    `
    }

    <div style="display:flex;gap:8px;margin-top:16px">
      <button class="btn btn-primary btn-sm" id="connected-test-btn">
        <span class="ms ms-sm">speed</span> Test Connection
      </button>
      <button class="btn btn-ghost btn-sm" id="connected-reconfigure-btn">
        <span class="ms ms-sm">tune</span> Reconfigure
      </button>
      <button class="btn btn-ghost btn-sm" style="color:var(--danger,#ef4444)" id="connected-disconnect-btn">
        <span class="ms ms-sm">link_off</span> Disconnect
      </button>
    </div>
    <div id="connected-test-result" style="display:none;margin-top:12px"></div>
  `;

  // Wire test button
  document.getElementById('connected-test-btn')?.addEventListener('click', async () => {
    const resultEl = document.getElementById('connected-test-result');
    const testBtn = document.getElementById('connected-test-btn') as HTMLButtonElement;
    if (!resultEl || !testBtn) return;

    testBtn.disabled = true;
    testBtn.innerHTML =
      '<span class="ms ms-sm" style="animation:spin 1s linear infinite">sync</span> Testing…';
    resultEl.style.display = 'block';
    resultEl.innerHTML =
      '<span style="font-size:13px;color:var(--text-secondary)">Testing connection…</span>';

    try {
      const result = await invoke<{ success: boolean; message: string; details?: string }>(
        'engine_integrations_test_credentials',
        { serviceId: service.id, nodeType: service.n8nNodeType, credentials: creds },
      );
      if (result.success) {
        resultEl.innerHTML = `<div style="display:flex;align-items:center;gap:6px;color:var(--success,#22c55e);font-size:13px">
          <span class="ms ms-sm">check_circle</span> ${escHtml(result.message)}${result.details ? ` — ${escHtml(result.details)}` : ''}
        </div>`;
      } else {
        resultEl.innerHTML = `<div style="display:flex;align-items:center;gap:6px;color:var(--danger,#ef4444);font-size:13px">
          <span class="ms ms-sm">error</span> ${escHtml(result.message)}${result.details ? ` — ${escHtml(result.details)}` : ''}
        </div>`;
      }
    } catch (e) {
      resultEl.innerHTML = `<div style="color:var(--danger,#ef4444);font-size:13px"><span class="ms ms-sm">error</span> Test failed: ${escHtml(String(e))}</div>`;
    }
    testBtn.disabled = false;
    testBtn.innerHTML = '<span class="ms ms-sm">speed</span> Test Connection';
  });

  // Wire reconfigure → re-open setup guide
  document.getElementById('connected-reconfigure-btn')?.addEventListener('click', () => {
    _openGuide(service);
  });

  // Wire disconnect
  document.getElementById('connected-disconnect-btn')?.addEventListener('click', async () => {
    try {
      await invoke('engine_integrations_disconnect', { serviceId: service.id });
      showToast(`${service.name} disconnected`, 'info');
      panel.style.display = 'none';
      _state.setSelectedService(null);
      refreshConnected();
    } catch (e) {
      showToast(`Disconnect failed: ${e}`, 'error');
    }
  });
}

// ── Community package banner in detail panel ─────────────────────────

async function _renderCommunityBanner(service: ServiceDefinition): Promise<void> {
  const requiredPkg = getRequiredPackage(service.id, service.communityPackage);
  if (!requiredPkg) return;

  const banner = document.getElementById('community-package-banner');
  if (!banner) return;

  const pkgName = communityDisplayName(requiredPkg);

  // Check if already installed
  let isInstalled = false;
  try {
    const installed = await invoke<Array<{ packageName: string }>>(
      'engine_n8n_community_packages_list',
    );
    isInstalled = installed.some((p) => p.packageName === requiredPkg);
  } catch {
    // n8n not running — show banner anyway
  }

  if (isInstalled) {
    banner.innerHTML = `
      <div class="community-req-banner community-req-installed">
        <span class="ms ms-sm">check_circle</span>
        <span>Community package <strong>${escHtml(pkgName)}</strong> is installed — native node support active.</span>
      </div>`;
  } else {
    banner.innerHTML = `
      <div class="community-req-banner community-req-available">
        <span class="ms ms-sm">extension</span>
        <div class="community-req-text">
          <span>A dedicated community package <strong>${escHtml(requiredPkg)}</strong> is available for richer integration.</span>
          <span class="community-req-hint">Install it for native node support instead of generic HTTP requests.</span>
        </div>
        <button class="btn btn-sm community-req-install" id="community-req-install-btn">
          <span class="ms ms-sm">add_circle</span> Install
        </button>
      </div>`;

    document.getElementById('community-req-install-btn')?.addEventListener('click', async () => {
      const btn = document.getElementById('community-req-install-btn') as HTMLButtonElement;
      if (!btn) return;
      btn.disabled = true;
      btn.innerHTML = '<span class="ms ms-sm k-spin">progress_activity</span> Installing…';

      try {
        const { showToast: toast } = await import('../../components/toast');
        await invoke('engine_n8n_community_packages_install', { packageName: requiredPkg });
        toast(`Installed ${requiredPkg}`, 'success');

        // Auto-deploy MCP workflow
        try {
          await invoke('engine_n8n_deploy_mcp_workflow');
        } catch {
          /* best effort */
        }

        // Re-render banner as installed
        _renderCommunityBanner(service);
      } catch (e) {
        const err = e instanceof Error ? e.message : String(e);
        const { showToast: toast } = await import('../../components/toast');
        toast(`Install failed: ${err}`, 'error');
        btn.disabled = false;
        btn.innerHTML = '<span class="ms ms-sm">add_circle</span> Install';
      }
    });
  }
}

// ── OAuth PKCE flow ────────────────────────────────────────────────────

async function _startOAuthFlow(service: ServiceDefinition): Promise<void> {
  const panel = document.getElementById('integrations-detail');
  if (!panel) return;

  const btn = document.getElementById('detail-oauth-btn') as HTMLButtonElement | null;
  if (btn) {
    btn.disabled = true;
    btn.innerHTML =
      '<span class="ms ms-sm k-spin">progress_activity</span> Waiting for authorization…';
  }

  try {
    const result = await invoke<{
      service_id: string;
      success: boolean;
      scopes_granted: string[];
      error?: string;
    }>('engine_oauth_start', { serviceId: service.id });

    if (result.success) {
      // OAuth tokens stored — run the provisioning pipeline
      try {
        await invoke('engine_integrations_connect', {
          serviceId: service.id,
          toolCount: service.capabilities?.length ?? 1,
        });
      } catch (e) {
        console.warn('[oauth] connect:', e);
      }

      try {
        // Deploy MCP workflow for tool discovery
        await invoke('engine_n8n_deploy_mcp_workflow', {
          serviceId: service.id,
          serviceName: service.name,
          n8nNodeType: service.n8nNodeType,
        });
      } catch (e) {
        console.warn('[oauth] MCP deploy (non-fatal):', e);
      }

      // Show success + refresh
      const { showToast } = await import('../../components/toast');
      showToast(`Connected to ${service.name}!`, 'success');
      refreshConnected();
      _renderDetail(service);
    } else {
      // Show error
      const { showToast } = await import('../../components/toast');
      showToast(result.error ?? `Failed to connect ${service.name}`, 'error');
      if (btn) {
        btn.disabled = false;
        btn.innerHTML = `<span class="ms ms-sm">link</span> Connect ${escHtml(service.name)}`;
      }
    }
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    const { showToast } = await import('../../components/toast');
    showToast(`OAuth error: ${msg}`, 'error');
    if (btn) {
      btn.disabled = false;
      btn.innerHTML = `<span class="ms ms-sm">link</span> Connect ${escHtml(service.name)}`;
    }
  }
}

// ── n8n OAuth delegation flow ──────────────────────────────────────────

async function _startN8nOAuthFlow(service: ServiceDefinition): Promise<void> {
  const btn = document.getElementById('detail-n8n-oauth-btn') as HTMLButtonElement | null;
  if (btn) {
    btn.disabled = true;
    btn.innerHTML = '<span class="ms ms-sm k-spin">progress_activity</span> Opening n8n…';
  }

  try {
    const url = await invoke<string>('engine_oauth_n8n_url', {
      serviceId: service.id,
    });

    // Open n8n credential creation page in the default browser
    await invoke('plugin:opener|open_url', { url });

    showToast(
      `Complete the OAuth flow in n8n, then return here and click "Verify Connection"`,
      'info',
    );

    if (btn) {
      btn.disabled = false;
      btn.innerHTML = `<span class="ms ms-sm">check_circle</span> I've completed setup in n8n`;
      // Re-wire to verify connection
      btn.onclick = async () => {
        btn.disabled = true;
        btn.innerHTML = '<span class="ms ms-sm k-spin">progress_activity</span> Verifying…';
        try {
          await invoke('engine_integrations_connect', {
            serviceId: service.id,
            toolCount: service.capabilities?.length ?? 1,
          });
          showToast(`Connected to ${service.name} via n8n!`, 'success');
          refreshConnected();
          _renderDetail(service);
        } catch (e) {
          const msg = e instanceof Error ? e.message : String(e);
          showToast(`Verification failed: ${msg}`, 'error');
          btn.disabled = false;
          btn.innerHTML = `<span class="ms ms-sm">check_circle</span> I've completed setup in n8n`;
        }
      };
    }
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    showToast(`n8n OAuth error: ${msg}`, 'error');
    if (btn) {
      btn.disabled = false;
      btn.innerHTML = `<span class="ms ms-sm">swap_horiz</span> Connect via n8n`;
    }
  }
}

// ── RFC 7591 dynamic registration flow ─────────────────────────────────

async function _startRfc7591Flow(service: ServiceDefinition): Promise<void> {
  const btn = document.getElementById('detail-rfc7591-btn') as HTMLButtonElement | null;
  if (btn) {
    btn.disabled = true;
    btn.innerHTML =
      '<span class="ms ms-sm k-spin">progress_activity</span> Registering & authorizing…';
  }

  try {
    const result = await invoke<{
      service_id: string;
      success: boolean;
      scopes_granted: string[];
      error?: string;
    }>('engine_oauth_rfc7591_start', { serviceId: service.id });

    if (result.success) {
      try {
        await invoke('engine_integrations_connect', {
          serviceId: service.id,
          toolCount: service.capabilities?.length ?? 1,
        });
      } catch (e) {
        console.warn('[rfc7591] connect:', e);
      }

      showToast(`Auto-connected to ${service.name}!`, 'success');
      refreshConnected();
      _renderDetail(service);
    } else {
      showToast(result.error ?? `Failed to auto-connect ${service.name}`, 'error');
      if (btn) {
        btn.disabled = false;
        btn.innerHTML = `<span class="ms ms-sm">auto_fix_high</span> Auto-connect ${escHtml(service.name)}`;
      }
    }
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    showToast(`Auto-connect error: ${msg}`, 'error');
    if (btn) {
      btn.disabled = false;
      btn.innerHTML = `<span class="ms ms-sm">auto_fix_high</span> Auto-connect ${escHtml(service.name)}`;
    }
  }
}

// ── Setup guide launcher ───────────────────────────────────────────────

function _openGuide(service: ServiceDefinition): void {
  const panel = document.getElementById('integrations-detail');
  if (!panel) return;
  panel.style.display = 'block';

  // Check if a community package is needed and not yet installed
  const requiredPkg = getRequiredPackage(service.id, service.communityPackage);
  if (requiredPkg) {
    _showAutoInstallPrompt(panel, service, requiredPkg);
  } else {
    openSetupGuide(panel, service, {
      onSave: () => {
        refreshConnected();
      },
      onClose: () => _renderDetail(service),
    });
  }
}

/** Show a prompt to install a required community package before setup. */
async function _showAutoInstallPrompt(
  panel: HTMLElement,
  service: ServiceDefinition,
  requiredPkg: string,
): Promise<void> {
  // First check if already installed
  let alreadyInstalled = false;
  try {
    const installed = await invoke<Array<{ packageName: string }>>(
      'engine_n8n_community_packages_list',
    );
    alreadyInstalled = installed.some((p) => p.packageName === requiredPkg);
  } catch {
    // n8n not running — skip check, proceed to guide directly
    openSetupGuide(panel, service, {
      onSave: () => {
        refreshConnected();
      },
      onClose: () => _renderDetail(service),
    });
    return;
  }

  if (alreadyInstalled) {
    // Already installed — go straight to setup
    openSetupGuide(panel, service, {
      onSave: () => {
        refreshConnected();
      },
      onClose: () => _renderDetail(service),
    });
    return;
  }

  // Show install prompt
  const pkgDisplay = communityDisplayName(requiredPkg);
  panel.innerHTML = `
    <div class="setup-guide">
      <div class="setup-guide-header">
        <div class="setup-guide-icon" style="background: ${service.color}15; color: ${service.color}">
          <span class="ms ms-lg">${service.icon}</span>
        </div>
        <div class="setup-guide-title-wrap">
          <h2 class="setup-guide-title">Install ${escHtml(pkgDisplay)}</h2>
          <span class="setup-guide-time">
            <span class="ms ms-sm">extension</span>
            Community package required
          </span>
        </div>
        <button class="btn btn-ghost btn-sm setup-guide-close" id="auto-install-close">
          <span class="ms">close</span>
        </button>
      </div>

      <div class="community-req-banner community-req-available" style="margin:16px 0">
        <span class="ms">info</span>
        <div class="community-req-text">
          <span><strong>${escHtml(service.name)}</strong> requires the community package
            <code style="font-size:12px;padding:2px 6px;background:var(--bg-tertiary,rgba(255,255,255,0.05));border-radius:4px">${escHtml(requiredPkg)}</code></span>
          <span class="community-req-hint">This provides native n8n node support with richer actions and triggers.</span>
        </div>
      </div>

      <div class="setup-guide-actions">
        <button class="btn btn-primary" id="auto-install-btn">
          <span class="ms ms-sm">download</span>
          <span>Install &amp; Continue</span>
        </button>
        <button class="btn btn-ghost" id="auto-install-skip">
          Skip (use generic HTTP)
        </button>
      </div>

      <div id="auto-install-feedback" style="display:none"></div>
    </div>
  `;

  // Close button
  document.getElementById('auto-install-close')?.addEventListener('click', () => {
    _renderDetail(service);
  });

  // Skip button — proceed to guide without installing
  document.getElementById('auto-install-skip')?.addEventListener('click', () => {
    openSetupGuide(panel, service, {
      onSave: () => {
        refreshConnected();
      },
      onClose: () => _renderDetail(service),
    });
  });

  // Install button
  document.getElementById('auto-install-btn')?.addEventListener('click', async () => {
    const btn = document.getElementById('auto-install-btn') as HTMLButtonElement;
    const skipBtn = document.getElementById('auto-install-skip') as HTMLButtonElement;
    const feedback = document.getElementById('auto-install-feedback') as HTMLElement;
    if (!btn) return;

    btn.disabled = true;
    btn.innerHTML =
      '<span class="ms ms-sm k-spin">progress_activity</span> Installing… (this may take a minute)';
    if (skipBtn) skipBtn.style.display = 'none';
    if (feedback) {
      feedback.style.display = 'block';
      feedback.innerHTML =
        '<div class="setup-guide-fb setup-guide-fb-testing"><span class="ms ms-sm spin">progress_activity</span> Installing community package…</div>';
    }

    try {
      await invoke('engine_n8n_community_packages_install', { packageName: requiredPkg });

      // Auto-deploy MCP workflow
      try {
        await invoke('engine_n8n_deploy_mcp_workflow');
      } catch {
        /* best effort */
      }

      if (feedback) {
        feedback.innerHTML =
          '<div class="setup-guide-fb setup-guide-fb-success"><span class="ms ms-sm">check_circle</span> Package installed! Continuing to setup…</div>';
      }

      // Short delay so user sees the success, then proceed to setup guide
      setTimeout(() => {
        openSetupGuide(panel, service, {
          onSave: () => {
            refreshConnected();
          },
          onClose: () => _renderDetail(service),
        });
      }, 1200);
    } catch (e) {
      const err = e instanceof Error ? e.message : String(e);
      if (feedback) {
        feedback.innerHTML = `<div class="setup-guide-fb setup-guide-fb-error"><span class="ms ms-sm">error</span> Install failed: ${escHtml(err)}</div>`;
      }
      btn.disabled = false;
      btn.innerHTML = '<span class="ms ms-sm">download</span> <span>Retry Install</span>';
      if (skipBtn) skipBtn.style.display = '';
    }
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

  // Card clicks → detail (or connect button → guide/OAuth)
  document.getElementById('integrations-grid')?.addEventListener('click', (e) => {
    const target = e.target as HTMLElement;

    // If user clicked the "Connect" / "Setup" button directly, route by authType
    const connectBtn = target.closest('.integrations-connect-btn') as HTMLElement;
    if (connectBtn) {
      const sid = connectBtn.dataset.serviceId;
      const service = SERVICE_CATALOG.find((s) => s.id === sid);
      if (service) {
        _state.setSelectedService(service);

        // Connected services always go to the connection detail (Edit view),
        // regardless of auth type — shows stored creds, test, disconnect.
        const isConnected = _state.getConnected().some((c) => c.serviceId === sid);
        if (isConnected) {
          _renderConnectedDetail(service);
        } else if (service.authType === 'oauth') {
          // Show the detail panel which has the OAuth connect button
          _renderDetail(service);
        } else if (service.authType === 'n8n-oauth') {
          _renderDetail(service);
        } else if (service.authType === 'rfc7591') {
          _renderDetail(service);
        } else {
          // Not connected, API key → open setup guide
          _openGuide(service);
        }
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
