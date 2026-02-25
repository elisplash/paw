// My Skills — Tabbed Workspace Orchestrator
// Manages tab switching, data loading, and wiring between atomic tab modules.

import {
  pawEngine,
  type EngineSkillStatus,
  type TomlSkillEntry,
  type McpServerConfig,
  type McpServerStatus,
} from '../../engine';
import { isEngineMode } from '../../engine-bridge';
import { $ } from '../../components/helpers';

// Tab modules (atomic)
import { renderActiveTab, bindActiveTabEvents, type ActiveTabData } from './tab-active';
import { renderIntegrationsTab, bindIntegrationsTabEvents } from './tab-integrations';
import { renderToolsTab, bindToolsTabEvents, setToolsReload, type ToolsTabData } from './tab-tools';
import { renderExtensionsTab, bindExtensionsTabEvents } from './tab-extensions';
import { renderCreateTab, bindCreateTabEvents } from './tab-create';
import { renderSummaryBar, updateTabCounts } from './summary-bar';
import { setMoleculesState } from './molecules';

// ── Re-exports (backward compat) ──────────────────────────────────────────

export { CATEGORY_META, SKILL_ICON_MAP, msIcon, skillIcon, formatInstalls } from './atoms';
export { POPULAR_REPOS, POPULAR_TAGS } from './atoms';

// ── Tab state ──────────────────────────────────────────────────────────────

type SkillsTab = 'active' | 'integrations' | 'tools' | 'extensions' | 'create';
let _activeTab: SkillsTab = 'active';
let _currentFilter = 'all';
let _searchQuery = '';

// Cached data (shared across tabs, refreshed on load)
let _skills: EngineSkillStatus[] = [];
let _tomlSkills: TomlSkillEntry[] = [];
let _mcpServers: McpServerConfig[] = [];
let _mcpStatuses: McpServerStatus[] = [];

function setFilter(f: string): void {
  _currentFilter = f;
}
function setSearch(q: string): void {
  _searchQuery = q;
}

// ── Tab switching ──────────────────────────────────────────────────────────

function switchTab(tab: SkillsTab): void {
  _activeTab = tab;

  // Update tab bar active state
  document.querySelectorAll('.skills-tab').forEach((btn) => {
    btn.classList.toggle('active', (btn as HTMLElement).dataset.skillsTab === tab);
  });

  // Render the selected tab content
  renderCurrentTab();
}

function renderCurrentTab(): void {
  const list = $('skills-vault-list');
  if (!list) return;

  switch (_activeTab) {
    case 'active': {
      const data: ActiveTabData = { skills: _skills, mcpStatuses: _mcpStatuses };
      list.innerHTML = renderActiveTab(data);
      bindActiveTabEvents();
      break;
    }
    case 'integrations': {
      setMoleculesState({ currentFilter: _currentFilter, searchQuery: _searchQuery, reloadFn: loadSkillsSettings });
      list.innerHTML = renderIntegrationsTab(_skills, loadSkillsSettings);
      const integrations = _skills.filter(
        (s) => s.tier === 'integration' || (s.required_credentials && s.required_credentials.length > 0),
      );
      bindIntegrationsTabEvents(integrations, setFilter, setSearch);
      break;
    }
    case 'tools': {
      const data: ToolsTabData = { skills: _skills, mcpServers: _mcpServers, mcpStatuses: _mcpStatuses };
      list.innerHTML = renderToolsTab(data);
      bindToolsTabEvents();
      break;
    }
    case 'extensions': {
      list.innerHTML = renderExtensionsTab(_skills, _tomlSkills);
      bindExtensionsTabEvents();
      break;
    }
    case 'create': {
      list.innerHTML = renderCreateTab();
      bindCreateTabEvents(loadSkillsSettings);
      break;
    }
  }
}

// ── Tab bar event wiring ───────────────────────────────────────────────────

function bindTabBar(): void {
  document.querySelectorAll('.skills-tab').forEach((btn) => {
    btn.addEventListener('click', () => {
      const tab = (btn as HTMLElement).dataset.skillsTab as SkillsTab | undefined;
      if (tab) switchTab(tab);
    });
  });
}

// ── Main data loader ───────────────────────────────────────────────────────

export async function loadSkillsSettings(): Promise<void> {
  const loading = $('skills-vault-loading');
  const list = $('skills-vault-list');
  const summaryBar = $('skills-summary-bar');

  if (!isEngineMode()) {
    if (loading) loading.textContent = 'Pawz engine is required.';
    if (list) list.innerHTML = '';
    return;
  }

  // Set reload callbacks for modules that need them
  setToolsReload(loadSkillsSettings);

  try {
    if (loading) loading.style.display = '';

    // Fetch all data in parallel
    const [skills, , tomlSkills, mcpServers, mcpStatuses] = await Promise.all([
      pawEngine.skillsList(),
      pawEngine.communitySkillsList(), // pre-fetched for Phase 2 (PawzHub marketplace)
      pawEngine.tomlSkillsScan(),
      pawEngine.mcpListServers(),
      pawEngine.mcpStatus(),
    ]);

    if (loading) loading.style.display = 'none';

    // Cache data for tab rendering
    _skills = skills;
    _tomlSkills = tomlSkills;
    _mcpServers = mcpServers;
    _mcpStatuses = mcpStatuses;

    // Compute tab counts
    const integrationCount = skills.filter(
      (s) => s.tier === 'integration' || (s.required_credentials && s.required_credentials.length > 0),
    ).length;
    const promptSkills = skills.filter((s) => s.tier === 'skill');
    const toolCount = mcpServers.length + promptSkills.length;
    const extensionCount = skills.filter((s) => s.tier === 'extension' || s.has_widget).length;

    // Update summary bar
    if (summaryBar) {
      summaryBar.innerHTML = renderSummaryBar({ skills, mcpStatuses });
    }

    // Update tab counts
    updateTabCounts({ skills, mcpStatuses, integrationCount, toolCount, extensionCount });

    // Wire tab bar (only needs to happen once but is idempotent)
    bindTabBar();

    // Render the currently selected tab
    renderCurrentTab();
  } catch (e) {
    console.error('[my-skills] Load failed:', e);
    if (loading) loading.textContent = `Failed to load skills: ${e}`;
  }
}
