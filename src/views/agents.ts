// Agents View — Create and manage AI agent personas
// Each agent has its own personality, skills, and memories

const $ = (id: string) => document.getElementById(id);

interface Agent {
  id: string;
  name: string;
  avatar: string; // emoji or initials
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
}

// Available models
const AVAILABLE_MODELS = [
  { id: 'default', name: 'Default (Use account setting)' },
  { id: 'claude-sonnet-4-20250514', name: 'Claude Sonnet 4' },
  { id: 'claude-opus-4-20250514', name: 'Claude Opus 4' },
  { id: 'gpt-4o', name: 'GPT-4o' },
  { id: 'gpt-4o-mini', name: 'GPT-4o Mini' },
  { id: 'gemini-2.0-flash', name: 'Gemini 2.0 Flash' },
  { id: 'gemini-2.5-pro', name: 'Gemini 2.5 Pro' },
];

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
  // Load from localStorage for now (could move to SQLite later)
  try {
    const stored = localStorage.getItem('paw-agents');
    _agents = stored ? JSON.parse(stored) : [];
    console.log('[agents] Loaded from storage:', _agents.length, 'agents');
  } catch {
    _agents = [];
  }

  // Ensure there's always a default agent
  if (_agents.length === 0) {
    _agents.push({
      id: 'default',
      name: 'Dave',
      avatar: '🧠',
      color: AVATAR_COLORS[0],
      bio: 'Your main AI assistant',
      model: 'default',
      template: 'general',
      personality: { tone: 'balanced', initiative: 'balanced', detail: 'balanced' },
      skills: ['web_search', 'web_fetch', 'read', 'write', 'exec'],
      boundaries: ['Ask before sending emails', 'No destructive git commands without permission'],
      createdAt: new Date().toISOString(),
    });
    saveAgents();
  }

  renderAgents();
}

function saveAgents() {
  localStorage.setItem('paw-agents', JSON.stringify(_agents));
}

function renderAgents() {
  console.log('[agents] renderAgents called');
  const grid = $('agents-grid');
  console.log('[agents] grid element:', grid);
  if (!grid) return;

  grid.innerHTML = _agents.map(agent => `
    <div class="agent-card" data-id="${agent.id}">
      <div class="agent-card-header">
        <div class="agent-avatar" style="background:${agent.color}">${agent.avatar}</div>
        <div class="agent-info">
          <div class="agent-name">${escHtml(agent.name)}</div>
          <div class="agent-template">${agent.template.charAt(0).toUpperCase() + agent.template.slice(1)}</div>
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
              <div class="agent-template-icon">🤖</div>
              <div class="agent-template-name">General</div>
              <div class="agent-template-desc">All-purpose assistant</div>
            </div>
            <div class="agent-template-card" data-template="research">
              <div class="agent-template-icon">🔬</div>
              <div class="agent-template-name">Research</div>
              <div class="agent-template-desc">Deep analysis</div>
            </div>
            <div class="agent-template-card" data-template="creative">
              <div class="agent-template-icon">🎨</div>
              <div class="agent-template-name">Creative</div>
              <div class="agent-template-desc">Writing & ideas</div>
            </div>
            <div class="agent-template-card" data-template="technical">
              <div class="agent-template-icon">💻</div>
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
            <button class="agent-avatar-option selected" data-avatar="🤖">🤖</button>
            <button class="agent-avatar-option" data-avatar="🧠">🧠</button>
            <button class="agent-avatar-option" data-avatar="🔬">🔬</button>
            <button class="agent-avatar-option" data-avatar="🎨">🎨</button>
            <button class="agent-avatar-option" data-avatar="💻">💻</button>
            <button class="agent-avatar-option" data-avatar="📚">📚</button>
            <button class="agent-avatar-option" data-avatar="🚀">🚀</button>
            <button class="agent-avatar-option" data-avatar="⚡">⚡</button>
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
  let selectedAvatar = '🤖';

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
      selectedAvatar = btn.getAttribute('data-avatar') || '🤖';
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

    const template = AGENT_TEMPLATES[selectedTemplate];
    const newAgent: Agent = {
      id: `agent-${Date.now()}`,
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
    };

    _agents.push(newAgent);
    saveAgents();
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
          <div class="agent-avatar-large" style="background:${agent.color}">${agent.avatar}</div>
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
              ${AVAILABLE_MODELS.map(m => `<option value="${m.id}" ${agent.model === m.id ? 'selected' : ''}>${m.name}</option>`).join('')}
            </select>
            <div class="form-hint">Which AI model this agent uses</div>
          </div>
          
          <div class="form-group">
            <label class="form-label">Avatar</label>
            <div class="agent-avatar-picker">
              ${['🤖','🧠','🔬','🎨','💻','📚','🚀','⚡','🎯','💡','🔥','✨'].map(a => 
                `<button class="agent-avatar-option ${agent.avatar === a ? 'selected' : ''}" data-avatar="${a}">${a}</button>`
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
      selectedAvatar = btn.getAttribute('data-avatar') || '🤖';
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

export function getAgents(): Agent[] {
  return _agents;
}

export function getCurrentAgent(): Agent | null {
  return _agents.find(a => a.id === _selectedAgent) || _agents[0] || null;
}

export function initAgents() {
  loadAgents();
}
