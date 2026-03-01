// index.ts — Module state, wiring, and public API for the agents view
// Imports from sub-modules and provides the unified public interface

import { pawEngine, type BackendAgent } from '../../engine';
import { isEngineMode } from '../../engine-bridge';
import { listen } from '@tauri-apps/api/event';
import { type Agent, AVATAR_COLORS, SPRITE_AVATARS, DEFAULT_AVATAR, isAvatar } from './atoms';
import { renderAgents } from './molecules';
import { openAgentCreator, openAgentEditor } from './editor';
import { seedSoulFiles, refreshAvailableModels } from './helpers';
import {
  initMiniHubSystem,
  openMiniHub,
  refreshDock,
  closeMiniHubByAgent,
} from '../../engine/organisms/mini_hub_orchestrator';
import {
  updateAgentsHeroStats,
  renderCapabilitiesList,
  renderActivityList,
  renderTemplateGrid,
  initAgentsKinetic,
  AGENT_TEMPLATE_CATALOG,
} from '../../components/agents-panel';

// ── Module state ────────────────────────────────────────────────────────────

let _agents: Agent[] = [];
let _selectedAgent: string | null = null;
let _availableModels: { id: string; name: string }[] = [
  { id: 'default', name: 'Default (Use account setting)' },
];

// Callbacks registered via configure()
let onSwitchView: ((view: string) => void) | null = null;
let onSetCurrentAgent: ((agentId: string | null) => void) | null = null;
let _onProfileUpdated: ((agentId: string, agent: Agent) => void) | null = null;

function startChatWithAgent(agentId: string) {
  _selectedAgent = agentId;
  onSetCurrentAgent?.(agentId);
  onSwitchView?.('chat');
}

function saveAgents() {
  // Persist all agents to localStorage (backend agents too so edits to name/avatar/personality survive reload)
  localStorage.setItem('paw-agents', JSON.stringify(_agents));
  refreshDock();
}

// Build the EditorCallbacks object to pass into editor functions
function makeEditorCallbacks() {
  return {
    getAgents: () => _agents,
    getAvailableModels: () => _availableModels,
    onCreated: (agent: Agent) => {
      _agents.push(agent);
      saveAgents();
      _renderAgents();
    },
    onUpdated: () => {
      saveAgents();
      _renderAgents();
    },
    onDeleted: (agentId: string) => {
      _agents = _agents.filter((a) => a.id !== agentId);
      saveAgents();
      _renderAgents();
    },
    seedSoulFiles,
  };
}

// Internal render helper that passes correct callbacks
function _renderAgents() {
  renderAgents(_agents, {
    onChat: (id) => startChatWithAgent(id),
    onMiniChat: (id) => openMiniChat(id),
    onEdit: (id) => openAgentEditor(id, makeEditorCallbacks()),
    onCreate: () => openAgentCreator(makeEditorCallbacks()),
  });

  // Update hero stats, side panel cards
  updateAgentsHeroStats(_agents);
  renderCapabilitiesList(_agents);
  renderActivityList(_agents);
}

// ── Public API ─────────────────────────────────────────────────────────────

export function configure(opts: {
  switchView: (view: string) => void;
  setCurrentAgent?: (agentId: string | null) => void;
}) {
  onSwitchView = opts.switchView;
  onSetCurrentAgent = opts.setCurrentAgent ?? null;
}

export async function loadAgents() {
  console.debug('[agents] loadAgents called');
  // Refresh available models from engine config (non-blocking)
  _availableModels = await refreshAvailableModels();
  // Load from localStorage (manually created agents)
  try {
    const stored = localStorage.getItem('paw-agents');
    _agents = stored ? JSON.parse(stored) : [];
    // Tag localStorage agents as local
    _agents.forEach((a) => {
      if (!a.source) a.source = 'local';
    });
    // Migrate ANY non-numeric avatar to a new Pawz Boi avatar
    let migrated = false;
    const usedNums = new Set<number>();
    _agents.forEach((a) => {
      if (!/^\d+$/.test(a.avatar)) {
        let num: number;
        do {
          num = Math.floor(Math.random() * 50) + 1;
        } while (usedNums.has(num));
        usedNums.add(num);
        a.avatar = String(num);
        migrated = true;
      }
    });
    if (migrated) localStorage.setItem('paw-agents', JSON.stringify(_agents));
    console.debug('[agents] Loaded from storage:', _agents.length, 'agents');
  } catch {
    _agents = [];
  }

  // Ensure there's always a default agent
  const existingDefault = _agents.find((a) => a.id === 'default');
  if (existingDefault && !isAvatar(existingDefault.avatar)) {
    existingDefault.avatar = DEFAULT_AVATAR;
    saveAgents();
  }
  if (!existingDefault) {
    _agents.unshift({
      id: 'default',
      name: 'Pawz',
      avatar: DEFAULT_AVATAR,
      color: AVATAR_COLORS[0],
      bio: 'Your main AI agent',
      model: 'default',
      template: 'general',
      personality: { tone: 'balanced', initiative: 'balanced', detail: 'balanced' },
      skills: ['web_search', 'web_fetch', 'read', 'write', 'exec'],
      boundaries: ['Ask before sending emails', 'No destructive git commands without permission'],
      createdAt: new Date().toISOString(),
      source: 'local',
    });
    saveAgents();
  }

  // Merge backend-created agents (from project_agents table)
  if (isEngineMode()) {
    try {
      const backendAgents: BackendAgent[] = await pawEngine.listAllAgents();
      console.debug('[agents] Backend agents:', backendAgents.length);
      const usedSprites = new Set(_agents.map((a) => a.avatar));
      function pickUniqueSprite(preferred: string): string {
        if (!usedSprites.has(preferred)) {
          usedSprites.add(preferred);
          return preferred;
        }
        const avail = SPRITE_AVATARS.find((s) => !usedSprites.has(s));
        if (avail) {
          usedSprites.add(avail);
          return avail;
        }
        return preferred; // fallback if all used
      }
      for (const ba of backendAgents) {
        // Skip if already in local list (by agent_id)
        if (_agents.find((a) => a.id === ba.agent_id)) continue;
        // Convert backend agent to Agent format — each gets a unique sprite
        const specialtySprite: Record<string, string> = {
          coder: '10',
          researcher: '15',
          designer: '20',
          communicator: '25',
          security: '30',
          general: '35',
          writer: '40',
          analyst: '45',
        };
        const preferredSprite = specialtySprite[ba.specialty] || '35';
        _agents.push({
          id: ba.agent_id,
          name: ba.agent_id.replace(/-/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase()),
          avatar: pickUniqueSprite(preferredSprite),
          color: AVATAR_COLORS[_agents.length % AVATAR_COLORS.length],
          bio: `${ba.role} — ${ba.specialty}`,
          model: ba.model || 'default',
          template: 'custom',
          personality: { tone: 'balanced', initiative: 'balanced', detail: 'balanced' },
          skills: ba.capabilities || [],
          boundaries: [],
          systemPrompt: ba.system_prompt,
          createdAt: new Date().toISOString(),
          source: 'backend',
          projectId: ba.project_id,
        });
      }
    } catch (e) {
      console.warn('[agents] Failed to load backend agents:', e);
    }
  }

  _renderAgents();

  // Initialize the mini-hub system (dock + popup hubs) with the current agents list
  initMiniHubSystem(() => _agents);

  // Seed soul files for all agents that don't have them yet (one-time migration)
  if (isEngineMode()) {
    for (const agent of _agents) {
      seedSoulFiles(agent);
    }
  }
}

export function getAgents(): Agent[] {
  return _agents;
}

export function getCurrentAgent(): Agent | null {
  return _agents.find((a) => a.id === _selectedAgent) || _agents[0] || null;
}

/** Set the selected agent by ID (used by main.ts agent dropdown). */
export function setSelectedAgent(agentId: string | null) {
  _selectedAgent = agentId;
}

/** Open a mini-chat popup for any agent (callable from outside the module). */
export function openMiniChat(agentId: string) {
  openMiniHub(agentId);
}

/** Register a callback for profile updates (called from main.ts) */
export function onProfileUpdated(cb: (agentId: string, agent: Agent) => void) {
  _onProfileUpdated = cb;
}

// ── Profile Update Event Listener ────────────────────────────────────────

let _profileUpdateListenerInitialized = false;

function initProfileUpdateListener() {
  if (_profileUpdateListenerInitialized) return;
  _profileUpdateListenerInitialized = true;

  listen<Record<string, string>>('agent-profile-updated', (event) => {
    const data = event.payload;
    const agentId = data.agent_id;
    if (!agentId) return;

    console.debug('[agents] Profile update event received:', data);

    const agent = _agents.find((a) => a.id === agentId);
    if (!agent) {
      console.warn(`[agents] update_profile: agent '${agentId}' not found`);
      return;
    }

    // Apply updates
    if (data.name) agent.name = data.name;
    if (data.avatar) agent.avatar = data.avatar;
    if (data.bio) agent.bio = data.bio;
    if (data.system_prompt) agent.systemPrompt = data.system_prompt;

    // Persist and re-render
    saveAgents();
    _renderAgents();
    refreshDock();

    // Notify main.ts to update chat header if this is the current agent
    if (_onProfileUpdated) _onProfileUpdated(agentId, agent);
    console.debug(`[agents] Profile updated for '${agentId}':`, agent.name, agent.avatar);
  }).catch((e) => console.warn('[agents] Failed to listen for profile updates:', e));
}

export function initAgents() {
  loadAgents();
  initProfileUpdateListener();

  // Render template marketplace + kinetic animations
  renderTemplateGrid((templateId) => _installTemplate(templateId));
  initAgentsKinetic();

  // ── Collapsible section toggles ────────────────────────────────────────
  document.querySelectorAll('.agents-section-toggle').forEach((toggle) => {
    toggle.addEventListener('click', (e) => {
      // Don't collapse when clicking view-toggle buttons inside the header
      if ((e.target as HTMLElement).closest('.agents-toggle-btns')) return;

      const bodyId = (toggle as HTMLElement).dataset.toggle;
      if (!bodyId) return;
      const body = document.getElementById(bodyId);
      if (!body) return;

      const isOpen = !body.classList.contains('agents-collapsed');
      body.classList.toggle('agents-collapsed', isOpen);
      const chevron = toggle.querySelector('.agents-section-chevron');
      if (chevron) chevron.textContent = isOpen ? 'expand_more' : 'expand_less';
    });
  });

  // Quick actions in side panel
  document.getElementById('agents-qa-create')?.addEventListener('click', () => {
    openAgentCreator(makeEditorCallbacks());
  });
  document.getElementById('agents-qa-export')?.addEventListener('click', () => {
    _exportFleet();
  });
  document.getElementById('agents-qa-import')?.addEventListener('click', () => {
    _importConfig();
  });
}

/** Install an agent from a template */
function _installTemplate(templateId: string) {
  const tpl = AGENT_TEMPLATE_CATALOG.find((t) => t.id === templateId);
  if (!tpl) return;

  // Create a new agent from the template
  const agent: Agent = {
    id: `${tpl.id}-${Date.now()}`,
    name: tpl.name,
    avatar: String(Math.floor(Math.random() * 25) + 1),
    color: AVATAR_COLORS[_agents.length % AVATAR_COLORS.length],
    bio: tpl.desc,
    model: tpl.model || 'default',
    template: 'custom',
    personality: tpl.personality as Agent['personality'],
    skills: tpl.skills,
    boundaries: [],
    systemPrompt: tpl.systemPrompt,
    createdAt: new Date().toISOString(),
    source: 'local',
  };

  _agents.push(agent);
  saveAgents();
  _renderAgents();

  // Flash the install button as feedback
  const btn = document.querySelector(`[data-tpl-id="${templateId}"]`) as HTMLElement | null;
  if (btn) {
    btn.textContent = '✓ Installed';
    btn.style.background = 'var(--accent)';
    btn.style.color = 'var(--bg-primary)';
    btn.style.borderColor = 'var(--accent)';
    setTimeout(() => {
      btn.innerHTML = '<span class="ms ms-sm">download</span> Install';
      btn.style.background = '';
      btn.style.color = '';
      btn.style.borderColor = '';
    }, 2000);
  }
}

/** Export all agents as JSON */
function _exportFleet() {
  const json = JSON.stringify(_agents, null, 2);
  const blob = new Blob([json], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = 'openpawz-fleet.json';
  a.click();
  URL.revokeObjectURL(url);
}

/** Import agents from a JSON file */
function _importConfig() {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = '.json';
  input.addEventListener('change', () => {
    const file = input.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      try {
        const imported = JSON.parse(reader.result as string);
        if (Array.isArray(imported)) {
          let count = 0;
          for (const a of imported) {
            if (a.id && a.name && !_agents.find((e) => e.id === a.id)) {
              _agents.push(a);
              count++;
            }
          }
          if (count > 0) {
            saveAgents();
            _renderAgents();
          }
        }
      } catch {
        console.warn('[agents] Failed to import config');
      }
    };
    reader.readAsText(file);
  });
  input.click();
}

// ── Re-exports (maintain public interface for existing callers) ────────────

export { spriteAvatar, type Agent } from './atoms';
export function closeMiniChat(agentId: string) {
  // Find and close by agent id — new system uses hub ids internally
  closeMiniHubByAgent(agentId);
}
