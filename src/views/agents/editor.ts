// editor.ts — Agent editor modal + community skills management
// Depends on: atoms, creator (EditorCallbacks), agent-policies, engine, toast

import { pawEngine, type CommunitySkill } from '../../engine';
import {
  POLICY_PRESETS,
  type ToolPolicy,
  getAgentPolicy,
  setAgentPolicy,
} from '../../features/agent-policies';
import { escHtml, escAttr, confirmModal } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { type Agent, TOOL_GROUPS, SPRITE_AVATARS, spriteAvatar } from './atoms';
import { type EditorCallbacks } from './creator';

// Re-export so index.ts can import both from one place
export { openAgentCreator, type EditorCallbacks } from './creator';

// ── Extracted helpers for openAgentEditor ─────────────────────────────────

/** Build the full HTML template for the agent editor modal */
function buildEditorHtml(agent: Agent, availableModels: { id: string; name: string }[]): string {
  return `
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
          <button class="agent-tab" data-tab="skills">Tools</button>
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
              ${availableModels.map((m) => `<option value="${m.id}" ${agent.model === m.id ? 'selected' : ''}>${m.name}</option>`).join('')}
            </select>
            <div class="form-hint">Which AI model this agent uses</div>
          </div>
          
          <div class="form-group">
            <label class="form-label">Avatar</label>
            <div class="agent-avatar-picker">
              ${SPRITE_AVATARS.map(
                (s) =>
                  `<button class="agent-avatar-option ${agent.avatar === s ? 'selected' : ''}" data-avatar="${s}">${spriteAvatar(s, 36)}</button>`,
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
        
        <!-- Tools Tab -->
        <div class="agent-tab-content" id="tab-skills">
          <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:12px">
            <div class="form-hint" style="margin:0">Control which tools this agent can use</div>
            <div style="display:flex;gap:6px">
              ${Object.entries(POLICY_PRESETS)
                .map(
                  ([key, preset]) =>
                    `<button class="btn btn-ghost btn-xs agent-tool-preset" data-preset="${key}" title="${preset.description}">${preset.label}</button>`,
                )
                .join('')}
            </div>
          </div>
          <div id="agent-tool-groups">
            ${TOOL_GROUPS.map(
              (group) => `
              <div class="agent-tool-group">
                <div class="agent-tool-group-header">
                  <label class="agent-tool-group-toggle">
                    <input type="checkbox" class="agent-tool-group-check" data-group="${group.label}">
                    <span class="ms ms-sm">${group.icon}</span>
                    <strong>${group.label}</strong>
                    <span class="agent-tool-group-count" data-group-count="${group.label}"></span>
                  </label>
                </div>
                <div class="agent-tool-group-items">
                  ${group.tools
                    .map(
                      (t) => `
                    <label class="agent-skill-toggle agent-tool-item">
                      <input type="checkbox" data-tool="${t.id}">
                      <div class="agent-skill-info">
                        <div class="agent-skill-name">${t.name}</div>
                        <div class="agent-skill-desc">${t.desc}</div>
                      </div>
                    </label>
                  `,
                    )
                    .join('')}
                </div>
              </div>
            `,
            ).join('')}
          </div>

          <div id="agent-community-skills-section" style="margin-top:24px">
            <div style="font-size:13px;font-weight:600;margin-bottom:8px;display:flex;align-items:center;gap:6px">
              <span class="ms ms-sm">extension</span> Community Skills
            </div>
            <div class="form-hint" style="margin-bottom:12px">Enable or disable installed community skills for this agent. Install new skills from the <strong>Skills</strong> tab.</div>
            <div id="agent-community-skills-grid" class="agent-skills-grid">
              <div style="font-size:12px;color:var(--text-muted);padding:8px">Loading...</div>
            </div>
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
              ${agent.boundaries
                .map(
                  (b, i) => `
                <div class="agent-boundary-row">
                  <input type="text" class="form-input agent-boundary-input" value="${escAttr(b)}" data-index="${i}">
                  <button class="btn-icon agent-boundary-remove" data-index="${i}">×</button>
                </div>
              `,
                )
                .join('')}
            </div>
            <button class="btn btn-ghost btn-sm" id="agent-add-boundary">+ Add rule</button>
          </div>
          
          ${
            agent.id !== 'default'
              ? `
          <div class="agent-danger-zone">
            <button class="btn btn-ghost agent-delete-btn" style="color:var(--error)">Delete Agent</button>
          </div>
          `
              : ''
          }
        </div>
      </div>
      <div class="agent-modal-footer">
        <button class="btn btn-ghost agent-modal-cancel">Cancel</button>
        <button class="btn btn-primary" id="agent-edit-save">Save Changes</button>
      </div>
    </div>
  `;
}

/** Wire tool-group checkboxes, individual tool toggles, and preset buttons */
function wireToolPolicyUI(modal: HTMLElement, allToolIds: string[], agentId: string): void {
  const currentPolicy = getAgentPolicy(agentId);
  const isUnrestricted = currentPolicy.mode === 'unrestricted';
  const allowedSet = new Set<string>(isUnrestricted ? allToolIds : currentPolicy.allowed);

  function applyToolChecks(allowed: Set<string>) {
    modal.querySelectorAll<HTMLInputElement>('[data-tool]').forEach((cb) => {
      cb.checked = allowed.has(cb.getAttribute('data-tool')!);
    });
    updateGroupChecks();
  }

  function updateGroupChecks() {
    for (const group of TOOL_GROUPS) {
      const items = modal.querySelectorAll<HTMLInputElement>(`[data-tool]`);
      const groupToolIds = new Set(group.tools.map((t) => t.id));
      let checked = 0,
        total = 0;
      items.forEach((cb) => {
        const tid = cb.getAttribute('data-tool')!;
        if (groupToolIds.has(tid)) {
          total++;
          if (cb.checked) checked++;
        }
      });
      const groupCb = modal.querySelector<HTMLInputElement>(`[data-group="${group.label}"]`);
      if (groupCb) {
        groupCb.checked = checked === total;
        groupCb.indeterminate = checked > 0 && checked < total;
      }
      const countEl = modal.querySelector(`[data-group-count="${group.label}"]`);
      if (countEl) countEl.textContent = `(${checked}/${total})`;
    }
  }

  applyToolChecks(allowedSet);

  modal.querySelectorAll<HTMLInputElement>('.agent-tool-group-check').forEach((groupCb) => {
    groupCb.addEventListener('change', () => {
      const groupLabel = groupCb.getAttribute('data-group')!;
      const group = TOOL_GROUPS.find((g) => g.label === groupLabel);
      if (!group) return;
      const checked = groupCb.checked;
      group.tools.forEach((t) => {
        const cb = modal.querySelector<HTMLInputElement>(`[data-tool="${t.id}"]`);
        if (cb) cb.checked = checked;
      });
      updateGroupChecks();
    });
  });

  modal.querySelectorAll<HTMLInputElement>('[data-tool]').forEach((cb) => {
    cb.addEventListener('change', () => updateGroupChecks());
  });

  modal.querySelectorAll<HTMLButtonElement>('.agent-tool-preset').forEach((btn) => {
    btn.addEventListener('click', () => {
      const presetKey = btn.getAttribute('data-preset')!;
      const preset = POLICY_PRESETS[presetKey];
      if (!preset) return;
      if (preset.policy.mode === 'unrestricted') {
        applyToolChecks(new Set(allToolIds));
      } else if (preset.policy.mode === 'allowlist') {
        applyToolChecks(new Set(preset.policy.allowed));
      } else if (preset.policy.mode === 'denylist') {
        const denied = new Set(preset.policy.denied);
        applyToolChecks(new Set(allToolIds.filter((t) => !denied.has(t))));
      }
      modal.querySelectorAll('.agent-tool-preset').forEach((b) => b.classList.remove('active'));
      btn.classList.add('active');
    });
  });
}

/** Collect form values and persist agent edits + tool policy */
function saveAgentEdits(
  modal: HTMLElement,
  agent: Agent,
  personality: Agent['personality'],
  boundaries: string[],
  selectedAvatar: string,
  allToolIds: string[],
  cbs: EditorCallbacks,
  close: () => void,
): void {
  const name = (modal.querySelector('#agent-edit-name') as HTMLInputElement)?.value.trim();
  const bio = (modal.querySelector('#agent-edit-bio') as HTMLInputElement)?.value.trim();
  const model = (modal.querySelector('#agent-edit-model') as HTMLSelectElement)?.value;
  const systemPrompt = (
    modal.querySelector('#agent-edit-prompt') as HTMLTextAreaElement
  )?.value.trim();

  const selectedTools: string[] = [];
  modal.querySelectorAll<HTMLInputElement>('[data-tool]:checked').forEach((cb) => {
    const toolId = cb.getAttribute('data-tool');
    if (toolId) selectedTools.push(toolId);
  });

  const isAllSelected = selectedTools.length >= allToolIds.length;
  const newPolicy: ToolPolicy = isAllSelected
    ? {
        mode: 'unrestricted',
        allowed: [],
        denied: [],
        requireApprovalForUnlisted: false,
        alwaysRequireApproval: [],
      }
    : {
        mode: 'allowlist',
        allowed: selectedTools,
        denied: [],
        requireApprovalForUnlisted: false,
        alwaysRequireApproval: [],
      };

  if (!name) {
    showToast('Name is required', 'error');
    return;
  }

  agent.name = name;
  agent.bio = bio;
  agent.avatar = selectedAvatar;
  agent.model = model || 'default';
  agent.personality = personality;
  agent.skills = selectedTools;
  agent.boundaries = boundaries.filter((b) => b.trim());
  agent.systemPrompt = systemPrompt;

  setAgentPolicy(agent.id, newPolicy);
  cbs.onUpdated();

  pawEngine
    .createAgent({
      agent_id: agent.id,
      role: agent.bio || 'assistant',
      specialty: agent.template === 'general' ? 'general' : agent.template,
      model: agent.model !== 'default' ? agent.model : undefined,
      system_prompt: agent.systemPrompt,
      capabilities: isAllSelected ? [] : selectedTools,
    })
    .catch((e) => console.warn('[agents] Backend update failed:', e));

  close();
  showToast('Changes saved', 'success');
}

/** Wire boundary add/edit/remove UI and perform initial render */
function wireEditorBoundaries(modal: HTMLElement, boundaries: string[]): void {
  const renderBoundaries = () => {
    const container = modal.querySelector('#agent-boundaries');
    if (!container) return;
    container.innerHTML = boundaries
      .map(
        (b, i) => `
      <div class="agent-boundary-row">
        <input type="text" class="form-input agent-boundary-input" value="${escAttr(b)}" data-index="${i}">
        <button class="btn-icon agent-boundary-remove" data-index="${i}">×</button>
      </div>
    `,
      )
      .join('');

    container.querySelectorAll('.agent-boundary-input').forEach((input) => {
      input.addEventListener('change', (e) => {
        const idx = parseInt((e.target as HTMLInputElement).getAttribute('data-index') || '0');
        boundaries[idx] = (e.target as HTMLInputElement).value;
      });
    });

    container.querySelectorAll('.agent-boundary-remove').forEach((btn) => {
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

  renderBoundaries();
}

export function openAgentEditor(agentId: string, cbs: EditorCallbacks) {
  const agents = cbs.getAgents();
  const agent = agents.find((a) => a.id === agentId);
  if (!agent) return;

  const availableModels = cbs.getAvailableModels();
  const allToolIds = TOOL_GROUPS.flatMap((g) => g.tools.map((t) => t.id));

  const modal = document.createElement('div');
  modal.className = 'agent-modal';
  modal.innerHTML = buildEditorHtml(agent, availableModels);
  document.body.appendChild(modal);

  const personality = { ...agent.personality };
  const boundaries = [...agent.boundaries];
  let selectedAvatar = agent.avatar;

  // Load community skills for this agent
  loadAgentCommunitySkills(modal, agentId, cbs.getAgents());

  wireToolPolicyUI(modal, allToolIds, agent.id);

  // Tab switching
  modal.querySelectorAll('.agent-tab').forEach((tab) => {
    tab.addEventListener('click', () => {
      modal.querySelectorAll('.agent-tab').forEach((t) => t.classList.remove('active'));
      modal.querySelectorAll('.agent-tab-content').forEach((c) => c.classList.remove('active'));
      tab.classList.add('active');
      const tabId = tab.getAttribute('data-tab');
      modal.querySelector(`#tab-${tabId}`)?.classList.add('active');
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

  // Personality selection
  modal.querySelectorAll('.agent-personality-options').forEach((group) => {
    group.querySelectorAll('.agent-personality-btn').forEach((btn) => {
      btn.addEventListener('click', () => {
        group
          .querySelectorAll('.agent-personality-btn')
          .forEach((b) => b.classList.remove('selected'));
        btn.classList.add('selected');
        const key = group.getAttribute('data-key') as keyof typeof personality;
        const value = btn.getAttribute('data-value');
        if (key && value) (personality as Record<string, string>)[key] = value;
      });
    });
  });

  // Boundaries
  wireEditorBoundaries(modal, boundaries);

  // Delete
  modal.querySelector('.agent-delete-btn')?.addEventListener('click', async () => {
    if (await confirmModal(`Delete ${agent.name}? This cannot be undone.`)) {
      cbs.onDeleted(agentId);
      // Also remove from backend SQLite
      pawEngine
        .deleteAgent(agentId)
        .catch((e) => console.warn('[agents] Backend delete failed:', e));
      modal.remove();
      showToast(`${agent.name} deleted`, 'success');
    }
  });

  const close = () => modal.remove();
  modal.querySelector('.agent-modal-close')?.addEventListener('click', close);
  modal.querySelector('.agent-modal-cancel')?.addEventListener('click', close);
  modal.addEventListener('click', (e) => {
    if (e.target === modal) close();
  });

  modal.querySelector('#agent-edit-save')?.addEventListener('click', () => {
    saveAgentEdits(modal, agent, personality, boundaries, selectedAvatar, allToolIds, cbs, close);
  });
}

// ── Community Skills per-agent management ────────────────────────────────

async function loadAgentCommunitySkills(
  modal: HTMLElement,
  agentId: string,
  agents: Agent[],
): Promise<void> {
  const grid = modal.querySelector('#agent-community-skills-grid');
  if (!grid) return;

  try {
    const allSkills: CommunitySkill[] = await pawEngine.communitySkillsList();

    if (allSkills.length === 0) {
      grid.innerHTML = `<div style="font-size:12px;color:var(--text-muted);padding:8px">No community skills installed. Browse and install from the <strong>Skills</strong> tab.</div>`;
      return;
    }

    grid.innerHTML = allSkills
      .map((s) => {
        // Skill is assigned to this agent if agent_ids is empty (all agents) or includes this agent
        const isAssigned =
          !s.agent_ids || s.agent_ids.length === 0 || s.agent_ids.includes(agentId);
        return `
        <label class="agent-skill-toggle">
          <input type="checkbox" class="agent-community-toggle" data-community-skill="${escHtml(s.id)}" ${isAssigned ? 'checked' : ''}>
          <div class="agent-skill-info">
            <div class="agent-skill-name" style="display:flex;align-items:center;gap:4px">
              <span class="ms ms-sm" style="font-size:14px;color:var(--accent)">extension</span>
              ${escHtml(s.name)}
            </div>
            <div class="agent-skill-desc">${escHtml(s.description)}</div>
          </div>
        </label>`;
      })
      .join('');

    // Bind toggle events
    grid.querySelectorAll('.agent-community-toggle').forEach((el) => {
      el.addEventListener('change', async (e) => {
        const input = e.target as HTMLInputElement;
        const skillId = input.dataset.communitySkill!;
        const skill = allSkills.find((sk) => sk.id === skillId);
        if (!skill) return;

        try {
          const currentIds = skill.agent_ids || [];

          let newIds: string[];
          if (input.checked) {
            // Adding this agent
            if (currentIds.length === 0) {
              // Already global (all agents) — no change needed
              return;
            }
            newIds = [...currentIds, agentId].filter((v, i, a) => a.indexOf(v) === i);
          } else {
            // Removing this agent
            if (currentIds.length === 0) {
              // Was "all agents" — switch to explicit list of all EXCEPT this one
              const allAgentIds = agents.map((a) => a.id);
              newIds = allAgentIds.filter((id) => id !== agentId);
            } else {
              newIds = currentIds.filter((id) => id !== agentId);
            }
          }

          await pawEngine.communitySkillSetAgents(skillId, newIds);
          // Update local state so subsequent toggles see the correct agent_ids
          skill.agent_ids = newIds;
          const name = skill.name || skillId.split('/').pop() || skillId;
          showToast(`${name} ${input.checked ? 'enabled' : 'disabled'} for this agent`, 'success');
        } catch (err) {
          showToast(`Failed: ${err}`, 'error');
          input.checked = !input.checked;
        }
      });
    });
  } catch {
    grid.innerHTML = `<div style="font-size:12px;color:var(--text-muted);padding:8px">Could not load community skills</div>`;
  }
}
