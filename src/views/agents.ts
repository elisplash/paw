// Agents View — Create and manage AI agent personas
// Each agent has its own personality, skills, and memories
// Supports both localStorage agents (manually created) and backend agents (created by orchestrator)

import { pawEngine, type BackendAgent } from '../engine';
import { isEngineMode } from '../engine-bridge';
import { listen } from '@tauri-apps/api/event';

const $ = (id: string) => document.getElementById(id);

/**
 * Seed initial soul files for a new agent so it knows who it is from the first conversation.
 * Only writes files that don't already exist to avoid overwriting user edits.
 */
async function seedSoulFiles(agent: Agent): Promise<void> {
  try {
    const existing = await pawEngine.agentFileList(agent.id);
    const existingNames = new Set(existing.map(f => f.file_name));

    if (!existingNames.has('IDENTITY.md')) {
      const personality = agent.personality;
      const personalityDesc = [
        personality.tone !== 'balanced' ? `Tone: ${personality.tone}` : '',
        personality.initiative !== 'balanced' ? `Initiative: ${personality.initiative}` : '',
        personality.detail !== 'balanced' ? `Detail level: ${personality.detail}` : '',
      ].filter(Boolean).join(', ');

      const identity = [
        `# ${agent.name}`,
        '',
        `## Identity`,
        `- **Name**: ${agent.name}`,
        `- **Agent ID**: ${agent.id}`,
        `- **Role**: ${agent.bio || 'AI assistant'}`,
        agent.template !== 'general' && agent.template !== 'custom' ? `- **Specialty**: ${agent.template}` : '',
        personalityDesc ? `- **Personality**: ${personalityDesc}` : '',
        '',
        agent.boundaries.length > 0 ? `## Boundaries\n${agent.boundaries.map(b => `- ${b}`).join('\n')}` : '',
        '',
        agent.systemPrompt ? `## Custom Instructions\n${agent.systemPrompt}` : '',
      ].filter(Boolean).join('\n');

      await pawEngine.agentFileSet('IDENTITY.md', identity.trim(), agent.id);
    }

    if (!existingNames.has('SOUL.md')) {
      const soul = [
        `# Soul`,
        '',
        `Write your personality, values, and communication style here.`,
        `Use \`soul_write\` to update this file as you develop your voice.`,
      ].join('\n');
      await pawEngine.agentFileSet('SOUL.md', soul, agent.id);
    }

    if (!existingNames.has('USER.md')) {
      const user = [
        `# About the User`,
        '',
        `Record what you learn about the user here — their name, preferences, projects, etc.`,
        `Use \`soul_write\` to update this file when you learn new things.`,
      ].join('\n');
      await pawEngine.agentFileSet('USER.md', user, agent.id);
    }

    console.log(`[agents] Seeded soul files for ${agent.name} (${agent.id})`);
  } catch (e) {
    console.warn(`[agents] Failed to seed soul files for ${agent.id}:`, e);
  }
}

export interface Agent {
  id: string;
  name: string;
  avatar: string; // avatar ID (e.g. '5') or legacy emoji
  color: string;
  bio: string;
  model: string; // AI model to use
  template: 'general' | 'research' | 'creative' | 'technical' | 'custom';
  personality: {
    tone: 'casual' | 'balanced' | 'formal';
    initiative: 'reactive' | 'balanced' | 'proactive';
    detail: 'brief' | 'balanced' | 'thorough';
  };
  skills: string[];
  boundaries: string[];
  systemPrompt?: string; // Custom instructions
  createdAt: string;
  lastUsed?: string;
  source?: 'local' | 'backend'; // Where this agent comes from
  projectId?: string;           // If backend-created, which project
}

// Model list — dynamically loaded from engine config
let _availableModels: { id: string; name: string }[] = [
  { id: 'default', name: 'Default (Use account setting)' },
];

/** Fetch configured models from the engine and populate the model picker. */
async function refreshAvailableModels() {
  try {
    const config = await pawEngine.getConfig();
    const models: { id: string; name: string }[] = [
      { id: 'default', name: 'Default (Use account setting)' },
    ];
    // Add each provider's default model, plus well-known models per provider kind
    const WELL_KNOWN: Record<string, { id: string; name: string }[]> = {
      google: [
        { id: 'gemini-2.5-pro', name: 'Gemini 2.5 Pro' },
        { id: 'gemini-2.5-flash', name: 'Gemini 2.5 Flash' },
        { id: 'gemini-2.0-flash', name: 'Gemini 2.0 Flash' },
      ],
      anthropic: [
        { id: 'claude-sonnet-4-6', name: 'Claude Sonnet 4.6 ($3/$15)' },
        { id: 'claude-haiku-4-5-20251001', name: 'Claude Haiku 4.5 ($1/$5)' },
        { id: 'claude-3-haiku-20240307', name: 'Claude Haiku 3 ($0.25/$1.25) cheapest' },
        { id: 'claude-opus-4-6', name: 'Claude Opus 4.6 ($5/$25)' },
        { id: 'claude-sonnet-4-5-20250929', name: 'Claude Sonnet 4.5 (agentic)' },
      ],
      openai: [
        { id: 'gpt-4o', name: 'GPT-4o' },
        { id: 'gpt-4o-mini', name: 'GPT-4o Mini' },
        { id: 'o1', name: 'o1' },
        { id: 'o3-mini', name: 'o3-mini' },
      ],
      openrouter: [],
      ollama: [],
      custom: [],
    };
    const seen = new Set<string>(['default']);
    for (const p of config.providers ?? []) {
      // Provider's own default model
      if (p.default_model && !seen.has(p.default_model)) {
        seen.add(p.default_model);
        models.push({ id: p.default_model, name: `${p.default_model} (${p.kind})` });
      }
      // Well-known models for this provider kind
      for (const wk of WELL_KNOWN[p.kind] ?? []) {
        if (!seen.has(wk.id)) {
          seen.add(wk.id);
          models.push(wk);
        }
      }
    }
    // Also add the global default model if set
    if (config.default_model && !seen.has(config.default_model)) {
      models.push({ id: config.default_model, name: `${config.default_model} (default)` });
    }
    _availableModels = models;
  } catch (e) {
    console.warn('[agents] Could not load models from engine config:', e);
  }
}

// Available skills
const AVAILABLE_SKILLS = [
  { id: 'web_search', name: 'Web Search', desc: 'Search the internet' },
  { id: 'web_fetch', name: 'Web Fetch', desc: 'Read web pages' },
  { id: 'browser', name: 'Browser', desc: 'Control a web browser' },
  { id: 'read', name: 'Read Files', desc: 'Read local files' },
  { id: 'write', name: 'Write Files', desc: 'Create and edit files' },
  { id: 'exec', name: 'Run Commands', desc: 'Execute shell commands' },
  { id: 'image', name: 'Image Analysis', desc: 'Analyze images' },
  { id: 'memory_store', name: 'Memory', desc: 'Remember information' },
  { id: 'cron', name: 'Scheduling', desc: 'Set reminders and schedules' },
  { id: 'message', name: 'Messaging', desc: 'Send messages' },
];

// Default agent templates
const AGENT_TEMPLATES: Record<string, Partial<Agent>> = {
  general: {
    bio: 'A helpful all-purpose assistant',
    personality: { tone: 'balanced', initiative: 'balanced', detail: 'balanced' },
    skills: ['web_search', 'web_fetch', 'read', 'write'],
  },
  research: {
    bio: 'Deep research and analysis specialist',
    personality: { tone: 'formal', initiative: 'proactive', detail: 'thorough' },
    skills: ['web_search', 'web_fetch', 'read', 'write', 'browser'],
  },
  creative: {
    bio: 'Writing, brainstorming, and creative projects',
    personality: { tone: 'casual', initiative: 'proactive', detail: 'balanced' },
    skills: ['web_search', 'read', 'write', 'image'],
  },
  technical: {
    bio: 'Code, debugging, and technical problem-solving',
    personality: { tone: 'balanced', initiative: 'reactive', detail: 'thorough' },
    skills: ['read', 'write', 'edit', 'exec', 'web_search'],
  },
  custom: {
    bio: '',
    personality: { tone: 'balanced', initiative: 'balanced', detail: 'balanced' },
    skills: [],
  },
};

const AVATAR_COLORS = [
  '#0073EA', '#10b981', '#8b5cf6', '#f59e0b', '#ec4899', '#06b6d4', '#ef4444'
];

// ── Avatars ────────────────────────────────────────────────────────────────
// Pawz Boi avatar set (96×96 PNGs in /src/assets/avatars/)
const SPRITE_AVATARS = Array.from({ length: 50 }, (_, i) => String(i + 1));

/** Default avatar for the main Pawz agent */
const DEFAULT_AVATAR = '5';

/** Check if avatar string is a numeric avatar ID vs a legacy emoji */
function isAvatar(avatar: string): boolean {
  return /^\d+$/.test(avatar);
}

/** Render an agent avatar as an <img> or legacy emoji <span> */
export function spriteAvatar(avatar: string, size = 32): string {
  if (isAvatar(avatar)) {
    return `<img src="/src/assets/avatars/${avatar}.png" alt="" width="${size}" height="${size}" style="display:block;border-radius:50%">`;
  }
  // Legacy emoji fallback
  return `<span style="font-size:${Math.round(size * 0.7)}px;line-height:1">${avatar}</span>`;
}

let _agents: Agent[] = [];
let _selectedAgent: string | null = null;

// Callbacks
let onSwitchView: ((view: string) => void) | null = null;
let onSetCurrentAgent: ((agentId: string | null) => void) | null = null;

export function configure(opts: {
  switchView: (view: string) => void;
  setCurrentAgent?: (agentId: string | null) => void;
}) {
  onSwitchView = opts.switchView;
  onSetCurrentAgent = opts.setCurrentAgent ?? null;
}

export async function loadAgents() {
  console.log('[agents] loadAgents called');
  // Refresh available models from engine config (non-blocking)
  await refreshAvailableModels();
  // Load from localStorage (manually created agents)
  try {
    const stored = localStorage.getItem('paw-agents');
    _agents = stored ? JSON.parse(stored) : [];
    // Tag localStorage agents as local
    _agents.forEach(a => { if (!a.source) a.source = 'local'; });
    // Migrate ANY non-numeric avatar to a new Pawz Boi avatar
    let migrated = false;
    const usedNums = new Set<number>();
    _agents.forEach(a => {
      if (!/^\d+$/.test(a.avatar)) {
        let num: number;
        do { num = Math.floor(Math.random() * 50) + 1; } while (usedNums.has(num));
        usedNums.add(num);
        a.avatar = String(num);
        migrated = true;
      }
    });
    if (migrated) localStorage.setItem('paw-agents', JSON.stringify(_agents));
    console.log('[agents] Loaded from storage:', _agents.length, 'agents');
  } catch {
    _agents = [];
  }

  // Ensure there's always a default agent
  const existingDefault = _agents.find(a => a.id === 'default');
  if (existingDefault && !/^\d+$/.test(existingDefault.avatar)) {
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
      console.log('[agents] Backend agents:', backendAgents.length);
      const usedSprites = new Set(_agents.map(a => a.avatar));
      function pickUniqueSprite(preferred: string): string {
        if (!usedSprites.has(preferred)) { usedSprites.add(preferred); return preferred; }
        const avail = SPRITE_AVATARS.find(s => !usedSprites.has(s));
        if (avail) { usedSprites.add(avail); return avail; }
        return preferred; // fallback if all used
      }
      for (const ba of backendAgents) {
        // Skip if already in local list (by agent_id)
        if (_agents.find(a => a.id === ba.agent_id)) continue;
        // Convert backend agent to Agent format — each gets a unique sprite
        const specialtySprite: Record<string, string> = {
          coder: '10', researcher: '15', designer: '20', communicator: '25',
          security: '30', general: '35', writer: '40', analyst: '45',
        };
        const preferredSprite = specialtySprite[ba.specialty] || '35';
        _agents.push({
          id: ba.agent_id,
          name: ba.agent_id.replace(/-/g, ' ').replace(/\b\w/g, c => c.toUpperCase()),
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

  renderAgents();
  renderAgentDock();

  // Seed soul files for all agents that don't have them yet (one-time migration)
  if (isEngineMode()) {
    for (const agent of _agents) {
      seedSoulFiles(agent);
    }
  }
}

function saveAgents() {
  // Persist all agents to localStorage (backend agents too so edits to name/avatar/personality survive reload)
  localStorage.setItem('paw-agents', JSON.stringify(_agents));
  renderAgentDock();
}

function renderAgents() {
  console.log('[agents] renderAgents called');
  const grid = $('agents-grid');
  console.log('[agents] grid element:', grid);
  if (!grid) return;

  grid.innerHTML = _agents.map(agent => `
    <div class="agent-card${agent.source === 'backend' ? ' agent-card-backend' : ''}" data-id="${agent.id}">
      <div class="agent-card-header">
        <div class="agent-avatar" style="background:${agent.color}">${spriteAvatar(agent.avatar, 48)}</div>
        <div class="agent-info">
          <div class="agent-name">${escHtml(agent.name)}</div>
          <div class="agent-template">${agent.source === 'backend' ? 'AI-Created' : agent.template.charAt(0).toUpperCase() + agent.template.slice(1)}</div>
        </div>
        <button class="btn-icon agent-menu-btn" title="Options">⋮</button>
      </div>
      <div class="agent-bio">${escHtml(agent.bio)}</div>
      <div class="agent-stats">
        <span class="agent-stat">${agent.skills.length} skills</span>
        <span class="agent-stat">${agent.boundaries.length} rules</span>
      </div>
      <div class="agent-actions">
        <button class="btn btn-primary btn-sm agent-chat-btn">Chat</button>
        <button class="btn btn-ghost btn-sm agent-minichat-btn" title="Open mini chat window"><span class="ms ms-sm">chat</span></button>
        <button class="btn btn-ghost btn-sm agent-edit-btn">Edit</button>
      </div>
    </div>
  `).join('') + `
    <div class="agent-card agent-card-new" id="agent-card-new">
      <div class="agent-card-new-icon">+</div>
      <div class="agent-card-new-label">Create Agent</div>
    </div>
  `;

  // Bind events
  grid.querySelectorAll('.agent-chat-btn').forEach(btn => {
    btn.addEventListener('click', (e) => {
      const card = (e.target as HTMLElement).closest('.agent-card');
      const id = card?.getAttribute('data-id');
      if (id) startChatWithAgent(id);
    });
  });

  grid.querySelectorAll('.agent-minichat-btn').forEach(btn => {
    btn.addEventListener('click', (e) => {
      const card = (e.target as HTMLElement).closest('.agent-card');
      const id = card?.getAttribute('data-id');
      if (id) openMiniChat(id);
    });
  });

  grid.querySelectorAll('.agent-edit-btn').forEach(btn => {
    btn.addEventListener('click', (e) => {
      const card = (e.target as HTMLElement).closest('.agent-card');
      const id = card?.getAttribute('data-id');
      if (id) openAgentEditor(id);
    });
  });

  $('agent-card-new')?.addEventListener('click', () => openAgentCreator());
  $('agents-create-btn')?.addEventListener('click', () => openAgentCreator());
}

function startChatWithAgent(agentId: string) {
  _selectedAgent = agentId;
  onSetCurrentAgent?.(agentId);
  onSwitchView?.('chat');
}

function openAgentCreator() {
  const modal = document.createElement('div');
  modal.className = 'agent-modal';
  modal.innerHTML = `
    <div class="agent-modal-dialog">
      <div class="agent-modal-header">
        <span>New Agent</span>
        <button class="btn-icon agent-modal-close">×</button>
      </div>
      <div class="agent-modal-body">
        <div class="agent-templates">
          <div class="agent-template-label">Start from a template</div>
          <div class="agent-template-grid">
            <div class="agent-template-card selected" data-template="general">
              <div class="agent-template-icon"><span class="ms">smart_toy</span></div>
              <div class="agent-template-name">General</div>
              <div class="agent-template-desc">All-purpose assistant</div>
            </div>
            <div class="agent-template-card" data-template="research">
              <div class="agent-template-icon"><span class="ms">biotech</span></div>
              <div class="agent-template-name">Research</div>
              <div class="agent-template-desc">Deep analysis</div>
            </div>
            <div class="agent-template-card" data-template="creative">
              <div class="agent-template-icon"><span class="ms">palette</span></div>
              <div class="agent-template-name">Creative</div>
              <div class="agent-template-desc">Writing & ideas</div>
            </div>
            <div class="agent-template-card" data-template="technical">
              <div class="agent-template-icon"><span class="ms">code</span></div>
              <div class="agent-template-name">Technical</div>
              <div class="agent-template-desc">Code & debugging</div>
            </div>
          </div>
        </div>
        
        <div class="form-group">
          <label class="form-label">Name</label>
          <input type="text" class="form-input" id="agent-create-name" placeholder="Give your agent a name">
        </div>
        
        <div class="form-group">
          <label class="form-label">Bio</label>
          <input type="text" class="form-input" id="agent-create-bio" placeholder="What is this agent for?">
        </div>
        
        <div class="form-group">
          <label class="form-label">Avatar</label>
          <div class="agent-avatar-picker" id="agent-avatar-picker">
            ${SPRITE_AVATARS.map((s, i) =>
              `<button class="agent-avatar-option${i === 0 ? ' selected' : ''}" data-avatar="${s}">${spriteAvatar(s, 36)}</button>`
            ).join('')}
          </div>
        </div>
      </div>
      <div class="agent-modal-footer">
        <button class="btn btn-ghost agent-modal-cancel">Cancel</button>
        <button class="btn btn-primary" id="agent-create-submit">Create Agent</button>
      </div>
    </div>
  `;
  document.body.appendChild(modal);

  let selectedTemplate = 'general';
  let selectedAvatar = SPRITE_AVATARS[0];

  // Template selection
  modal.querySelectorAll('.agent-template-card').forEach(card => {
    card.addEventListener('click', () => {
      modal.querySelectorAll('.agent-template-card').forEach(c => c.classList.remove('selected'));
      card.classList.add('selected');
      selectedTemplate = card.getAttribute('data-template') || 'general';
      
      // Update bio placeholder
      const template = AGENT_TEMPLATES[selectedTemplate];
      const bioInput = modal.querySelector('#agent-create-bio') as HTMLInputElement;
      if (bioInput && template?.bio) bioInput.placeholder = template.bio;
    });
  });

  // Avatar selection
  modal.querySelectorAll('.agent-avatar-option').forEach(btn => {
    btn.addEventListener('click', () => {
      modal.querySelectorAll('.agent-avatar-option').forEach(b => b.classList.remove('selected'));
      btn.classList.add('selected');
      selectedAvatar = btn.getAttribute('data-avatar') || SPRITE_AVATARS[0];
    });
  });

  const close = () => modal.remove();
  modal.querySelector('.agent-modal-close')?.addEventListener('click', close);
  modal.querySelector('.agent-modal-cancel')?.addEventListener('click', close);
  modal.addEventListener('click', (e) => { if (e.target === modal) close(); });

  modal.querySelector('#agent-create-submit')?.addEventListener('click', () => {
    const name = (modal.querySelector('#agent-create-name') as HTMLInputElement)?.value.trim();
    const bio = (modal.querySelector('#agent-create-bio') as HTMLInputElement)?.value.trim();
    
    if (!name) {
      showToast('Please enter a name', 'error');
      return;
    }

    const agentSlug = name.toLowerCase().replace(/\s+/g, '-').replace(/[^a-z0-9-]/g, '');
    const agentId = `agent-${agentSlug}-${Date.now()}`;
    const template = AGENT_TEMPLATES[selectedTemplate];
    const newAgent: Agent = {
      id: agentId,
      name,
      avatar: selectedAvatar,
      color: AVATAR_COLORS[_agents.length % AVATAR_COLORS.length],
      bio: bio || template?.bio || '',
      model: 'default',
      template: selectedTemplate as Agent['template'],
      personality: template?.personality || { tone: 'balanced', initiative: 'balanced', detail: 'balanced' },
      skills: template?.skills || [],
      boundaries: [],
      createdAt: new Date().toISOString(),
      source: 'local',
    };

    _agents.push(newAgent);
    saveAgents();

    // Also persist to backend SQLite so agents survive across devices
    pawEngine.createAgent({
      agent_id: agentId,
      role: template?.bio || 'assistant',
      specialty: selectedTemplate === 'general' ? 'general' : selectedTemplate,
      model: undefined,
      system_prompt: undefined,
      capabilities: template?.skills || [],
    }).catch(e => console.warn('[agents] Backend persist failed:', e));

    // Seed initial soul files so the agent knows who it is
    seedSoulFiles(newAgent);

    renderAgents();
    close();
    showToast(`${name} created!`, 'success');
  });
}

function openAgentEditor(agentId: string) {
  const agent = _agents.find(a => a.id === agentId);
  if (!agent) return;

  const modal = document.createElement('div');
  modal.className = 'agent-modal';
  modal.innerHTML = `
    <div class="agent-modal-dialog agent-modal-large">
      <div class="agent-modal-header">
        <div class="agent-modal-header-left">
          <div class="agent-avatar-large" style="background:${agent.color}">${spriteAvatar(agent.avatar, 36)}</div>
          <span>Edit ${escHtml(agent.name)}</span>
        </div>
        <button class="btn-icon agent-modal-close">×</button>
      </div>
      <div class="agent-modal-body">
        <div class="agent-editor-tabs">
          <button class="agent-tab active" data-tab="basics">Basics</button>
          <button class="agent-tab" data-tab="personality">Personality</button>
          <button class="agent-tab" data-tab="skills">Skills</button>
          <button class="agent-tab" data-tab="advanced">Advanced</button>
        </div>
        
        <!-- Basics Tab -->
        <div class="agent-tab-content active" id="tab-basics">
          <div class="form-group">
            <label class="form-label">Name</label>
            <input type="text" class="form-input" id="agent-edit-name" value="${escAttr(agent.name)}">
          </div>
          
          <div class="form-group">
            <label class="form-label">Bio</label>
            <input type="text" class="form-input" id="agent-edit-bio" value="${escAttr(agent.bio)}" placeholder="What is this agent for?">
          </div>
          
          <div class="form-group">
            <label class="form-label">Model</label>
            <select class="form-input" id="agent-edit-model">
              ${_availableModels.map(m => `<option value="${m.id}" ${agent.model === m.id ? 'selected' : ''}>${m.name}</option>`).join('')}
            </select>
            <div class="form-hint">Which AI model this agent uses</div>
          </div>
          
          <div class="form-group">
            <label class="form-label">Avatar</label>
            <div class="agent-avatar-picker">
              ${SPRITE_AVATARS.map(s =>
                `<button class="agent-avatar-option ${agent.avatar === s ? 'selected' : ''}" data-avatar="${s}">${spriteAvatar(s, 36)}</button>`
              ).join('')}
            </div>
          </div>
        </div>
        
        <!-- Personality Tab -->
        <div class="agent-tab-content" id="tab-personality">
          <div class="agent-personality-grid">
            <div class="agent-personality-row">
              <span class="agent-personality-label">Tone</span>
              <div class="agent-personality-options" data-key="tone">
                <button class="agent-personality-btn ${agent.personality.tone === 'casual' ? 'selected' : ''}" data-value="casual">Casual</button>
                <button class="agent-personality-btn ${agent.personality.tone === 'balanced' ? 'selected' : ''}" data-value="balanced">Balanced</button>
                <button class="agent-personality-btn ${agent.personality.tone === 'formal' ? 'selected' : ''}" data-value="formal">Formal</button>
              </div>
            </div>
            <div class="agent-personality-row">
              <span class="agent-personality-label">Initiative</span>
              <div class="agent-personality-options" data-key="initiative">
                <button class="agent-personality-btn ${agent.personality.initiative === 'reactive' ? 'selected' : ''}" data-value="reactive">Wait for asks</button>
                <button class="agent-personality-btn ${agent.personality.initiative === 'balanced' ? 'selected' : ''}" data-value="balanced">Balanced</button>
                <button class="agent-personality-btn ${agent.personality.initiative === 'proactive' ? 'selected' : ''}" data-value="proactive">Proactive</button>
              </div>
            </div>
            <div class="agent-personality-row">
              <span class="agent-personality-label">Detail</span>
              <div class="agent-personality-options" data-key="detail">
                <button class="agent-personality-btn ${agent.personality.detail === 'brief' ? 'selected' : ''}" data-value="brief">Brief</button>
                <button class="agent-personality-btn ${agent.personality.detail === 'balanced' ? 'selected' : ''}" data-value="balanced">Balanced</button>
                <button class="agent-personality-btn ${agent.personality.detail === 'thorough' ? 'selected' : ''}" data-value="thorough">Thorough</button>
              </div>
            </div>
          </div>
        </div>
        
        <!-- Skills Tab -->
        <div class="agent-tab-content" id="tab-skills">
          <div class="form-hint" style="margin-bottom:16px">Choose which tools this agent can use</div>
          <div class="agent-skills-grid">
            ${AVAILABLE_SKILLS.map(s => `
              <label class="agent-skill-toggle">
                <input type="checkbox" data-skill="${s.id}" ${agent.skills.includes(s.id) ? 'checked' : ''}>
                <div class="agent-skill-info">
                  <div class="agent-skill-name">${s.name}</div>
                  <div class="agent-skill-desc">${s.desc}</div>
                </div>
              </label>
            `).join('')}
          </div>
        </div>
        
        <!-- Advanced Tab -->
        <div class="agent-tab-content" id="tab-advanced">
          <div class="form-group">
            <label class="form-label">Custom Instructions</label>
            <textarea class="form-input agent-system-prompt" id="agent-edit-prompt" placeholder="Add custom instructions for this agent...">${escHtml(agent.systemPrompt || '')}</textarea>
            <div class="form-hint">These instructions are added to every conversation with this agent</div>
          </div>
          
          <div class="form-group">
            <label class="form-label">Boundaries & Rules</label>
            <div class="agent-boundaries" id="agent-boundaries">
              ${agent.boundaries.map((b, i) => `
                <div class="agent-boundary-row">
                  <input type="text" class="form-input agent-boundary-input" value="${escAttr(b)}" data-index="${i}">
                  <button class="btn-icon agent-boundary-remove" data-index="${i}">×</button>
                </div>
              `).join('')}
            </div>
            <button class="btn btn-ghost btn-sm" id="agent-add-boundary">+ Add rule</button>
          </div>
          
          ${agent.id !== 'default' ? `
          <div class="agent-danger-zone">
            <button class="btn btn-ghost agent-delete-btn" style="color:var(--error)">Delete Agent</button>
          </div>
          ` : ''}
        </div>
      </div>
      <div class="agent-modal-footer">
        <button class="btn btn-ghost agent-modal-cancel">Cancel</button>
        <button class="btn btn-primary" id="agent-edit-save">Save Changes</button>
      </div>
    </div>
  `;
  document.body.appendChild(modal);

  const personality = { ...agent.personality };
  let boundaries = [...agent.boundaries];
  let selectedAvatar = agent.avatar;

  // Tab switching
  modal.querySelectorAll('.agent-tab').forEach(tab => {
    tab.addEventListener('click', () => {
      modal.querySelectorAll('.agent-tab').forEach(t => t.classList.remove('active'));
      modal.querySelectorAll('.agent-tab-content').forEach(c => c.classList.remove('active'));
      tab.classList.add('active');
      const tabId = tab.getAttribute('data-tab');
      modal.querySelector(`#tab-${tabId}`)?.classList.add('active');
    });
  });

  // Avatar selection
  modal.querySelectorAll('.agent-avatar-option').forEach(btn => {
    btn.addEventListener('click', () => {
      modal.querySelectorAll('.agent-avatar-option').forEach(b => b.classList.remove('selected'));
      btn.classList.add('selected');
      selectedAvatar = btn.getAttribute('data-avatar') || SPRITE_AVATARS[0];
    });
  });

  // Personality selection
  modal.querySelectorAll('.agent-personality-options').forEach(group => {
    group.querySelectorAll('.agent-personality-btn').forEach(btn => {
      btn.addEventListener('click', () => {
        group.querySelectorAll('.agent-personality-btn').forEach(b => b.classList.remove('selected'));
        btn.classList.add('selected');
        const key = group.getAttribute('data-key') as keyof typeof personality;
        const value = btn.getAttribute('data-value');
        if (key && value) (personality as Record<string, string>)[key] = value;
      });
    });
  });

  // Boundaries
  const renderBoundaries = () => {
    const container = modal.querySelector('#agent-boundaries');
    if (!container) return;
    container.innerHTML = boundaries.map((b, i) => `
      <div class="agent-boundary-row">
        <input type="text" class="form-input agent-boundary-input" value="${escAttr(b)}" data-index="${i}">
        <button class="btn-icon agent-boundary-remove" data-index="${i}">×</button>
      </div>
    `).join('');
    
    container.querySelectorAll('.agent-boundary-input').forEach(input => {
      input.addEventListener('change', (e) => {
        const idx = parseInt((e.target as HTMLInputElement).getAttribute('data-index') || '0');
        boundaries[idx] = (e.target as HTMLInputElement).value;
      });
    });
    
    container.querySelectorAll('.agent-boundary-remove').forEach(btn => {
      btn.addEventListener('click', (e) => {
        const idx = parseInt((e.target as HTMLElement).getAttribute('data-index') || '0');
        boundaries.splice(idx, 1);
        renderBoundaries();
      });
    });
  };

  modal.querySelector('#agent-add-boundary')?.addEventListener('click', () => {
    boundaries.push('');
    renderBoundaries();
  });

  // Delete
  modal.querySelector('.agent-delete-btn')?.addEventListener('click', () => {
    if (confirm(`Delete ${agent.name}? This cannot be undone.`)) {
      _agents = _agents.filter(a => a.id !== agentId);
      saveAgents();
      // Also remove from backend SQLite
      pawEngine.deleteAgent(agentId).catch(e => console.warn('[agents] Backend delete failed:', e));
      renderAgents();
      modal.remove();
      showToast(`${agent.name} deleted`, 'success');
    }
  });

  const close = () => modal.remove();
  modal.querySelector('.agent-modal-close')?.addEventListener('click', close);
  modal.querySelector('.agent-modal-cancel')?.addEventListener('click', close);
  modal.addEventListener('click', (e) => { if (e.target === modal) close(); });

  modal.querySelector('#agent-edit-save')?.addEventListener('click', () => {
    const name = (modal.querySelector('#agent-edit-name') as HTMLInputElement)?.value.trim();
    const bio = (modal.querySelector('#agent-edit-bio') as HTMLInputElement)?.value.trim();
    const model = (modal.querySelector('#agent-edit-model') as HTMLSelectElement)?.value;
    const systemPrompt = (modal.querySelector('#agent-edit-prompt') as HTMLTextAreaElement)?.value.trim();
    
    // Collect selected skills
    const skills: string[] = [];
    modal.querySelectorAll('.agent-skill-toggle input:checked').forEach(input => {
      const skill = (input as HTMLInputElement).getAttribute('data-skill');
      if (skill) skills.push(skill);
    });
    
    if (!name) {
      showToast('Name is required', 'error');
      return;
    }

    agent.name = name;
    agent.bio = bio;
    agent.avatar = selectedAvatar;
    agent.model = model || 'default';
    agent.personality = personality;
    agent.skills = skills;
    agent.boundaries = boundaries.filter(b => b.trim());
    agent.systemPrompt = systemPrompt;
    
    saveAgents();

    // Sync changes to backend SQLite
    pawEngine.createAgent({
      agent_id: agent.id,
      role: agent.bio || 'assistant',
      specialty: agent.template === 'general' ? 'general' : agent.template,
      model: agent.model !== 'default' ? agent.model : undefined,
      system_prompt: agent.systemPrompt,
      capabilities: agent.skills,
    }).catch(e => console.warn('[agents] Backend update failed:', e));

    renderAgents();
    close();
    showToast('Changes saved', 'success');
  });

  renderBoundaries();
}

function showToast(message: string, type: 'success' | 'error' = 'success') {
  const toast = document.createElement('div');
  toast.className = `toast toast-${type}`;
  toast.textContent = message;
  document.body.appendChild(toast);
  setTimeout(() => toast.classList.add('show'), 10);
  setTimeout(() => {
    toast.classList.remove('show');
    setTimeout(() => toast.remove(), 300);
  }, 3000);
}

function escHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

function escAttr(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/"/g, '&quot;');
}

// ═══ Mini-Chat Popup System (FB Messenger–style) ═══════════════════════════

interface MiniChatWindow {
  agentId: string;
  agent: Agent;
  sessionId: string | null;
  el: HTMLElement;
  messagesEl: HTMLElement;
  inputEl: HTMLInputElement;
  isMinimized: boolean;
  isStreaming: boolean;
  streamingContent: string;
  streamingEl: HTMLElement | null;
  runId: string | null;
  unreadCount: number;
  unlistenDelta: (() => void) | null;
  unlistenComplete: (() => void) | null;
  unlistenError: (() => void) | null;
}

const _miniChats: Map<string, MiniChatWindow> = new Map();
const MINI_CHAT_WIDTH = 320;
const MINI_CHAT_GAP = 12;
const DOCK_RESERVED = 72; /* 48px dock + 12px right margin + 12px gap */

function getMiniChatOffset(index: number): number {
  return DOCK_RESERVED + index * (MINI_CHAT_WIDTH + MINI_CHAT_GAP);
}

function repositionMiniChats() {
  let idx = 0;
  for (const mc of _miniChats.values()) {
    mc.el.style.right = `${getMiniChatOffset(idx)}px`;
    idx++;
  }
}

function openMiniChat(agentId: string) {
  // If already open, just un-minimize
  const existing = _miniChats.get(agentId);
  if (existing) {
    if (existing.isMinimized) toggleMinimizeMiniChat(agentId);
    existing.inputEl.focus();
    return;
  }

  const agent = _agents.find(a => a.id === agentId);
  if (!agent) return;

  const el = document.createElement('div');
  el.className = 'mini-chat';
  el.style.right = `${getMiniChatOffset(_miniChats.size)}px`;
  el.innerHTML = `
    <div class="mini-chat-header" style="background:${agent.color}">
      <div class="mini-chat-avatar">${spriteAvatar(agent.avatar, 24)}</div>
      <div class="mini-chat-name">${escHtml(agent.name)}</div>
      <div class="mini-chat-controls">
        <button class="mini-chat-btn mini-chat-minimize" title="Minimize">—</button>
        <button class="mini-chat-btn mini-chat-close" title="Close">×</button>
      </div>
    </div>
    <div class="mini-chat-body">
      <div class="mini-chat-messages"></div>
      <div class="mini-chat-input-row">
        <input type="text" class="mini-chat-input" placeholder="Message ${escAttr(agent.name)}…">
        <button class="mini-chat-send"><span class="ms ms-sm">send</span></button>
      </div>
    </div>
  `;
  document.body.appendChild(el);

  const messagesEl = el.querySelector('.mini-chat-messages') as HTMLElement;
  const inputEl = el.querySelector('.mini-chat-input') as HTMLInputElement;

  const mc: MiniChatWindow = {
    agentId,
    agent,
    sessionId: null,
    el,
    messagesEl,
    inputEl,
    isMinimized: false,
    isStreaming: false,
    streamingContent: '',
    streamingEl: null,
    runId: null,
    unreadCount: 0,
    unlistenDelta: null,
    unlistenComplete: null,
    unlistenError: null,
  };
  _miniChats.set(agentId, mc);

  // Set up engine event listeners for this chat
  setupMiniChatListeners(mc);

  // Header drag/minimize
  el.querySelector('.mini-chat-minimize')?.addEventListener('click', () => toggleMinimizeMiniChat(agentId));
  el.querySelector('.mini-chat-close')?.addEventListener('click', () => closeMiniChat(agentId));
  el.querySelector('.mini-chat-header')?.addEventListener('dblclick', () => toggleMinimizeMiniChat(agentId));

  // Send on Enter or button
  inputEl.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      sendMiniChatMessage(agentId);
    }
  });
  el.querySelector('.mini-chat-send')?.addEventListener('click', () => sendMiniChatMessage(agentId));

  // Animate in
  requestAnimationFrame(() => el.classList.add('mini-chat-visible'));
  updateDockActive(agentId, true);
  updateDockBadge(agentId, 0);
  inputEl.focus();
}

/** Lightweight markdown → HTML for mini-chat bubbles (bold, italic, code, links) */
function miniChatMd(raw: string): string {
  let s = escHtml(raw);
  // Code blocks: ```...```
  s = s.replace(/```([\s\S]*?)```/g, '<pre class="mini-chat-code">$1</pre>');
  // Inline code: `...`
  s = s.replace(/`([^`]+)`/g, '<code>$1</code>');
  // Bold: **...**
  s = s.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
  // Italic: *...*
  s = s.replace(/(?<!\*)\*(?!\*)(.+?)(?<!\*)\*(?!\*)/g, '<em>$1</em>');
  // Links: [text](url)
  s = s.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" target="_blank" rel="noopener">$1</a>');
  // Newlines
  s = s.replace(/\n/g, '<br>');
  return s;
}

function setupMiniChatListeners(mc: MiniChatWindow) {
  // Listen for delta events
  mc.unlistenDelta = pawEngine.on('delta', (event) => {
    if (!mc.runId || event.run_id !== mc.runId) return;
    mc.streamingContent += event.text || '';
    if (mc.streamingEl) {
      // During streaming: plain text for speed, markdown on finalize
      mc.streamingEl.textContent = mc.streamingContent;
      mc.messagesEl.scrollTop = mc.messagesEl.scrollHeight;
    }
  });

  // Listen for completion — only finalize on the FINAL completion (no pending tool calls).
  // Intermediate completions (tool_calls_count > 0) mean the agent is still working
  // through tool rounds and more deltas/responses will follow.
  mc.unlistenComplete = pawEngine.on('complete', (event) => {
    if (!mc.runId || event.run_id !== mc.runId) return;
    if (event.tool_calls_count && event.tool_calls_count > 0) return;
    finalizeMiniChatStreaming(mc);
  });

  // Listen for errors
  mc.unlistenError = pawEngine.on('error', (event) => {
    if (!mc.runId || event.run_id !== mc.runId) return;
    mc.streamingContent += `\nError: ${event.message || 'Error'}`;
    finalizeMiniChatStreaming(mc);
  });
}

function finalizeMiniChatStreaming(mc: MiniChatWindow) {
  mc.isStreaming = false;
  mc.runId = null;
  if (mc.streamingEl) {
    // Render final content with markdown formatting
    mc.streamingEl.innerHTML = miniChatMd(mc.streamingContent);
    mc.streamingEl.classList.remove('mini-chat-streaming');
    mc.streamingEl = null;
  }
  // Track unread when minimized
  if (mc.isMinimized) {
    mc.unreadCount++;
    updateMiniChatBadge(mc);
    updateDockBadge(mc.agentId, mc.unreadCount);
  }
  mc.inputEl.disabled = false;
  mc.inputEl.focus();
}

async function sendMiniChatMessage(agentId: string) {
  const mc = _miniChats.get(agentId);
  if (!mc || mc.isStreaming) return;

  const text = mc.inputEl.value.trim();
  if (!text) return;

  mc.inputEl.value = '';
  mc.isStreaming = true;
  mc.streamingContent = '';
  mc.inputEl.disabled = true;

  // Add user message bubble
  const userBubble = document.createElement('div');
  userBubble.className = 'mini-chat-msg mini-chat-msg-user';
  userBubble.textContent = text;
  mc.messagesEl.appendChild(userBubble);

  // Add assistant streaming bubble
  const asstBubble = document.createElement('div');
  asstBubble.className = 'mini-chat-msg mini-chat-msg-assistant mini-chat-streaming';
  asstBubble.innerHTML = '<span class="mini-chat-dots">···</span>';
  mc.messagesEl.appendChild(asstBubble);
  mc.streamingEl = asstBubble;
  mc.messagesEl.scrollTop = mc.messagesEl.scrollHeight;

  try {
    // Build agent system prompt
    const parts: string[] = [];
    if (mc.agent.name) parts.push(`You are ${mc.agent.name}.`);
    if (mc.agent.bio) parts.push(mc.agent.bio);
    if (mc.agent.systemPrompt) parts.push(mc.agent.systemPrompt);
    const systemPrompt = parts.length > 0 ? parts.join(' ') : undefined;

    const resolvedModel = (mc.agent.model && mc.agent.model !== 'default') ? mc.agent.model : undefined;

    const request = {
      session_id: mc.sessionId || undefined,
      message: text,
      model: resolvedModel,
      system_prompt: systemPrompt,
      tools_enabled: true,
      agent_id: mc.agentId,
    };

    const result = await pawEngine.chatSend(request);
    mc.runId = result.run_id;
    mc.sessionId = result.session_id;
  } catch (e) {
    console.error('[mini-chat] Send error:', e);
    asstBubble.textContent = `Error: ${e instanceof Error ? e.message : 'Failed to send'}`;
    asstBubble.classList.remove('mini-chat-streaming');
    mc.isStreaming = false;
    mc.streamingEl = null;
    mc.inputEl.disabled = false;
  }
}

function toggleMinimizeMiniChat(agentId: string) {
  const mc = _miniChats.get(agentId);
  if (!mc) return;
  mc.isMinimized = !mc.isMinimized;
  mc.el.classList.toggle('mini-chat-minimized', mc.isMinimized);
  if (!mc.isMinimized) {
    mc.unreadCount = 0;
    updateMiniChatBadge(mc);
    updateDockBadge(agentId, 0);
    mc.messagesEl.scrollTop = mc.messagesEl.scrollHeight;
  }
}

function closeMiniChat(agentId: string) {
  const mc = _miniChats.get(agentId);
  if (!mc) return;
  // Cleanup listeners
  mc.unlistenDelta?.();
  mc.unlistenComplete?.();
  mc.unlistenError?.();
  mc.el.classList.remove('mini-chat-visible');
  setTimeout(() => mc.el.remove(), 200);
  _miniChats.delete(agentId);
  repositionMiniChats();
  updateDockBadge(agentId, 0);
  updateDockActive(agentId, false);
}

export function getAgents(): Agent[] {
  return _agents;
}

export function getCurrentAgent(): Agent | null {
  return _agents.find(a => a.id === _selectedAgent) || _agents[0] || null;
}

/** Set the selected agent by ID (used by main.ts agent dropdown). */
export function setSelectedAgent(agentId: string | null) {
  _selectedAgent = agentId;
}

/** Open a mini-chat popup for any agent (callable from outside the module). */
export { openMiniChat };

// ═══ Mini-Chat Badge Helpers ═════════════════════════════════════════════════

/** Update the header badge inside a mini-chat window */
function updateMiniChatBadge(mc: MiniChatWindow) {
  let badge = mc.el.querySelector('.mini-chat-unread') as HTMLElement | null;
  if (mc.unreadCount > 0) {
    if (!badge) {
      badge = document.createElement('span');
      badge.className = 'mini-chat-unread';
      mc.el.querySelector('.mini-chat-name')?.appendChild(badge);
    }
    badge.textContent = mc.unreadCount > 9 ? '9+' : String(mc.unreadCount);
    badge.style.display = '';
  } else if (badge) {
    badge.style.display = 'none';
  }
}

/** Update the dock tray badge for a specific agent */
function updateDockBadge(agentId: string, count: number) {
  const dockItem = document.querySelector(`.agent-dock-item[data-agent-id="${agentId}"]`);
  if (!dockItem) return;
  let badge = dockItem.querySelector('.agent-dock-badge') as HTMLElement | null;
  if (count > 0) {
    if (!badge) {
      badge = document.createElement('span');
      badge.className = 'agent-dock-badge';
      dockItem.appendChild(badge);
    }
    badge.textContent = count > 9 ? '9+' : String(count);
    badge.style.display = '';
  } else if (badge) {
    badge.style.display = 'none';
  }
}

/** Set active ring on dock item when mini-chat is open */
function updateDockActive(agentId: string, active: boolean) {
  const dockItem = document.querySelector(`.agent-dock-item[data-agent-id="${agentId}"]`);
  if (dockItem) dockItem.classList.toggle('agent-dock-active', active);
}

// ═══ Global Agent Dock / Tray (FB Messenger–style) ══════════════════════════
// Persistent bar of agent avatar circles at the bottom-right of the screen.
// Clicking an avatar opens or focuses that agent's mini-chat popup.

let _dockEl: HTMLElement | null = null;

/**
 * Render or refresh the floating agent dock tray.
 * Called after agents load and whenever agents list changes.
 */
export function renderAgentDock() {
  // Create dock container if needed
  if (!_dockEl) {
    _dockEl = document.createElement('div');
    _dockEl.id = 'agent-dock';
    _dockEl.className = 'agent-dock';
    document.body.appendChild(_dockEl);
  }

  const agents = _agents.filter(a => a.id !== 'default'); // Don't show default Dave in dock
  if (agents.length === 0) {
    _dockEl.style.display = 'none';
    return;
  }
  _dockEl.style.display = '';

  _dockEl.innerHTML = agents.map(a => {
    const isOpen = _miniChats.has(a.id);
    const mc = _miniChats.get(a.id);
    const unread = mc?.unreadCount ?? 0;
    return `
      <div class="agent-dock-item${isOpen ? ' agent-dock-active' : ''}" data-agent-id="${a.id}" title="${escAttr(a.name)}">
        <div class="agent-dock-avatar">${spriteAvatar(a.avatar, 40)}</div>
        ${unread > 0 ? `<span class="agent-dock-badge">${unread > 9 ? '9+' : unread}</span>` : ''}
      </div>
    `;
  }).join('');

  // Bind click events
  _dockEl.querySelectorAll('.agent-dock-item').forEach(item => {
    item.addEventListener('click', () => {
      const agentId = (item as HTMLElement).dataset.agentId;
      if (agentId) openMiniChat(agentId);
    });
  });
}

// ═══ Profile Update Event Listener ═══════════════════════════════════════════
// Listens for 'agent-profile-updated' Tauri events emitted by the update_profile tool.
// Updates the agent in _agents + localStorage + re-renders UI in real-time.

let _profileUpdateListenerInitialized = false;

function initProfileUpdateListener() {
  if (_profileUpdateListenerInitialized) return;
  _profileUpdateListenerInitialized = true;

  listen<Record<string, string>>('agent-profile-updated', (event) => {
    const data = event.payload;
    const agentId = data.agent_id;
    if (!agentId) return;

    console.log('[agents] Profile update event received:', data);

    const agent = _agents.find(a => a.id === agentId);
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
    renderAgents();
    renderAgentDock();

    // Notify main.ts to update chat header if this is the current agent
    if (_onProfileUpdated) _onProfileUpdated(agentId, agent);
    console.log(`[agents] Profile updated for '${agentId}':`, agent.name, agent.avatar);
  }).catch(e => console.warn('[agents] Failed to listen for profile updates:', e));
}

/** Callback to notify main.ts when a profile is updated */
let _onProfileUpdated: ((agentId: string, agent: Agent) => void) | null = null;

/** Register a callback for profile updates (called from main.ts) */
export function onProfileUpdated(cb: (agentId: string, agent: Agent) => void) {
  _onProfileUpdated = cb;
}

export function initAgents() {
  loadAgents();
  initProfileUpdateListener();
}
