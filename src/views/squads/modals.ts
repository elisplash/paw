// src/views/squads/modals.ts — Squad modal handlers

import { pawEngine } from '../../engine';
import { showToast } from '../../components/toast';
import type { EngineSquad, EngineSquadMember } from '../../engine/atoms/types';
import { buildAgentOptions } from './atoms';
import { getSquads, setActiveSquadId, loadSquads } from './molecules';
import * as AgentsModule from '../agents';

const $ = (id: string) => document.getElementById(id);

/** Open the create modal with empty fields. */
export function openCreateModal(): void {
  const modal = $('squad-modal');
  if (!modal) return;
  modal.style.display = 'flex';
  const titleEl = $('squad-modal-title');
  if (titleEl) titleEl.textContent = 'New Squad';
  const nameInput = $('squad-form-name') as HTMLInputElement | null;
  const goalInput = $('squad-form-goal') as HTMLTextAreaElement | null;
  if (nameInput) nameInput.value = '';
  if (goalInput) goalInput.value = '';
  const saveBtn = $('squad-modal-save');
  if (saveBtn) saveBtn.textContent = 'Create';
  saveBtn?.setAttribute('data-mode', 'create');
  saveBtn?.removeAttribute('data-squad-id');
}

/** Open the edit modal pre-filled with squad data. */
export function openEditModal(squad: EngineSquad): void {
  const modal = $('squad-modal');
  if (!modal) return;
  modal.style.display = 'flex';
  const titleEl = $('squad-modal-title');
  if (titleEl) titleEl.textContent = 'Edit Squad';
  const nameInput = $('squad-form-name') as HTMLInputElement | null;
  const goalInput = $('squad-form-goal') as HTMLTextAreaElement | null;
  if (nameInput) nameInput.value = squad.name;
  if (goalInput) goalInput.value = squad.goal;
  const saveBtn = $('squad-modal-save');
  if (saveBtn) saveBtn.textContent = 'Save';
  saveBtn?.setAttribute('data-mode', 'edit');
  saveBtn?.setAttribute('data-squad-id', squad.id);
}

/** Open the add-member modal for a specific squad. */
export function openAddMemberModal(squad: EngineSquad): void {
  const modal = $('squad-member-modal');
  if (!modal) return;
  modal.style.display = 'flex';
  const agentSelect = $('squad-member-agent') as HTMLSelectElement | null;
  if (agentSelect) {
    const agents = AgentsModule.getAgents().map((a) => ({ id: a.id, name: a.name }));
    agentSelect.innerHTML = `<option value="">Select agent...</option>${buildAgentOptions(agents, squad.members)}`;
  }
  const roleSelect = $('squad-member-role') as HTMLSelectElement | null;
  if (roleSelect) roleSelect.value = 'member';
  const saveBtn = $('squad-member-save');
  saveBtn?.setAttribute('data-squad-id', squad.id);
}

/** Handle save from the create/edit modal. */
export async function handleSaveSquad(): Promise<void> {
  const saveBtn = $('squad-modal-save');
  const mode = saveBtn?.getAttribute('data-mode') || 'create';
  const nameInput = $('squad-form-name') as HTMLInputElement | null;
  const goalInput = $('squad-form-goal') as HTMLTextAreaElement | null;
  const name = nameInput?.value.trim() || '';
  const goal = goalInput?.value.trim() || '';

  if (!name) {
    showToast('Squad name is required', 'error');
    return;
  }

  try {
    if (mode === 'edit') {
      const squadId = saveBtn?.getAttribute('data-squad-id') || '';
      const existing = getSquads().find((s) => s.id === squadId);
      if (!existing) return;
      const updated: EngineSquad = { ...existing, name, goal };
      await pawEngine.squadUpdate(updated);
      showToast('Squad updated', 'success');
    } else {
      const id = crypto.randomUUID();
      const squad: EngineSquad = {
        id,
        name,
        goal,
        status: 'active',
        members: [],
        created_at: '',
        updated_at: '',
      };
      await pawEngine.squadCreate(squad);
      setActiveSquadId(id);
      showToast('Squad created', 'success');
    }
    closeModal();
    loadSquads();
  } catch (e) {
    showToast(`Failed to save squad: ${e}`, 'error');
  }
}

/** Handle adding a member from the member modal. */
export async function handleAddMember(): Promise<void> {
  const saveBtn = $('squad-member-save');
  const squadId = saveBtn?.getAttribute('data-squad-id') || '';
  const agentSelect = $('squad-member-agent') as HTMLSelectElement | null;
  const roleSelect = $('squad-member-role') as HTMLSelectElement | null;
  const agentId = agentSelect?.value || '';
  const role = roleSelect?.value || 'member';

  if (!agentId) {
    showToast('Select an agent', 'error');
    return;
  }

  try {
    const member: EngineSquadMember = { agent_id: agentId, role };
    await pawEngine.squadAddMember(squadId, member);
    showToast('Member added', 'success');
    closeMemberModal();
    loadSquads();
  } catch (e) {
    showToast(`Failed to add member: ${e}`, 'error');
  }
}

/** Close the create/edit modal. */
export function closeModal(): void {
  const modal = $('squad-modal');
  if (modal) modal.style.display = 'none';
}

/** Close the add-member modal. */
export function closeMemberModal(): void {
  const modal = $('squad-member-modal');
  if (modal) modal.style.display = 'none';
}
