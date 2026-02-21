// creator.ts — New agent creation modal
// Extracted from editor.ts; depends on atoms, toast, engine

import { pawEngine } from '../../engine';
import { showToast } from '../../components/toast';
import { type Agent, AGENT_TEMPLATES, SPRITE_AVATARS, AVATAR_COLORS, spriteAvatar } from './atoms';

export interface EditorCallbacks {
  /** Called after a new agent is created — push to array + persist + re-render */
  onCreated: (agent: Agent) => void;
  /** Called after an agent is updated in-place — persist + re-render */
  onUpdated: () => void;
  /** Called after an agent is deleted — remove from array + persist + re-render */
  onDeleted: (agentId: string) => void;
  /** Seed initial soul files for a new agent */
  seedSoulFiles: (agent: Agent) => void;
  /** Current agents list (read-only reference for counts/lookup) */
  getAgents: () => Agent[];
  /** Available models for the model picker */
  getAvailableModels: () => { id: string; name: string }[];
}

export function openAgentCreator(cbs: EditorCallbacks) {
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
            ${SPRITE_AVATARS.map(
              (s, i) =>
                `<button class="agent-avatar-option${i === 0 ? ' selected' : ''}" data-avatar="${s}">${spriteAvatar(s, 36)}</button>`,
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
  modal.querySelectorAll('.agent-template-card').forEach((card) => {
    card.addEventListener('click', () => {
      modal.querySelectorAll('.agent-template-card').forEach((c) => c.classList.remove('selected'));
      card.classList.add('selected');
      selectedTemplate = card.getAttribute('data-template') || 'general';

      // Update bio placeholder
      const template = AGENT_TEMPLATES[selectedTemplate];
      const bioInput = modal.querySelector('#agent-create-bio') as HTMLInputElement;
      if (bioInput && template?.bio) bioInput.placeholder = template.bio;
    });
  });

  // Avatar selection
  modal.querySelectorAll('.agent-avatar-option').forEach((btn) => {
    btn.addEventListener('click', () => {
      modal.querySelectorAll('.agent-avatar-option').forEach((b) => b.classList.remove('selected'));
      btn.classList.add('selected');
      selectedAvatar = btn.getAttribute('data-avatar') || SPRITE_AVATARS[0];
    });
  });

  const close = () => modal.remove();
  modal.querySelector('.agent-modal-close')?.addEventListener('click', close);
  modal.querySelector('.agent-modal-cancel')?.addEventListener('click', close);
  modal.addEventListener('click', (e) => {
    if (e.target === modal) close();
  });

  modal.querySelector('#agent-create-submit')?.addEventListener('click', () => {
    const name = (modal.querySelector('#agent-create-name') as HTMLInputElement)?.value.trim();
    const bio = (modal.querySelector('#agent-create-bio') as HTMLInputElement)?.value.trim();

    if (!name) {
      showToast('Please enter a name', 'error');
      return;
    }

    const agentSlug = name
      .toLowerCase()
      .replace(/\s+/g, '-')
      .replace(/[^a-z0-9-]/g, '');
    const agentId = `agent-${agentSlug}-${Date.now()}`;
    const template = AGENT_TEMPLATES[selectedTemplate];
    const agents = cbs.getAgents();
    const newAgent: Agent = {
      id: agentId,
      name,
      avatar: selectedAvatar,
      color: AVATAR_COLORS[agents.length % AVATAR_COLORS.length],
      bio: bio || template?.bio || '',
      model: 'default',
      template: selectedTemplate as Agent['template'],
      personality: template?.personality || {
        tone: 'balanced',
        initiative: 'balanced',
        detail: 'balanced',
      },
      skills: template?.skills || [],
      boundaries: [],
      createdAt: new Date().toISOString(),
      source: 'local',
    };

    cbs.onCreated(newAgent);

    // Also persist to backend SQLite so agents survive across devices
    pawEngine
      .createAgent({
        agent_id: agentId,
        role: template?.bio || 'assistant',
        specialty: selectedTemplate === 'general' ? 'general' : selectedTemplate,
        model: undefined,
        system_prompt: undefined,
        capabilities: template?.skills || [],
      })
      .catch((e) => console.warn('[agents] Backend persist failed:', e));

    // Seed initial soul files so the agent knows who it is
    cbs.seedSoulFiles(newAgent);

    close();
    showToast(`${name} created!`, 'success');
  });
}
