// index.ts — Module state, wiring, and public API for the agents view
// Imports from sub-modules and provides the unified public interface

import { pawEngine, type BackendAgent } from '../../engine';
import { isEngineMode } from '../../engine-bridge';
import { listen } from '@tauri-apps/api/event';
import { type Agent, AVATAR_COLORS, SPRITE_AVATARS, DEFAULT_AVATAR, isAvatar } from './atoms';
import { renderAgents } from './molecules';
import { openAgentCreator, openAgentEditor } from './editor';
import { openMiniChat as _openMiniChat, _miniChats } from './mini-chat';
import { seedSoulFiles, refreshAvailableModels } from './helpers';
import { renderAgentDock } from './dock';

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
  _renderDock();
}

/** Thin wrapper that passes module state into the extracted dock renderer. */
function _renderDock() {
  renderAgentDock({
    getAgents: () => _agents,
    getMiniChatState: (id) => _miniChats.get(id),
    isMiniChatOpen: (id) => _miniChats.has(id),
    openMiniChat: (id) => openMiniChat(id),
  });
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
  _renderDock();

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
  _openMiniChat(agentId, () => _agents);
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
    _renderDock();

    // Notify main.ts to update chat header if this is the current agent
    if (_onProfileUpdated) _onProfileUpdated(agentId, agent);
    console.debug(`[agents] Profile updated for '${agentId}':`, agent.name, agent.avatar);
  }).catch((e) => console.warn('[agents] Failed to listen for profile updates:', e));
}

export function initAgents() {
  loadAgents();
  initProfileUpdateListener();
}

// ── Re-exports (maintain public interface for existing callers) ────────────

export { spriteAvatar, type Agent } from './atoms';
// closeMiniChat is not used externally but re-exported for completeness
export { closeMiniChat } from './mini-chat';
