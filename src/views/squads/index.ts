// src/views/squads/index.ts — Squads view entry point

export { loadSquads, getSquads, setActiveSquadId } from './molecules';
export {
  openCreateModal,
  handleSaveSquad,
  handleAddMember,
  closeModal,
  closeMemberModal,
} from './modals';

import {
  openCreateModal,
  handleSaveSquad,
  handleAddMember,
  closeModal,
  closeMemberModal,
} from './modals';

const $ = (id: string) => document.getElementById(id);

/** Wire DOM events once on module load. */
function bindEvents(): void {
  $('squads-create-btn')?.addEventListener('click', openCreateModal);
  $('squads-empty-create')?.addEventListener('click', openCreateModal);
  $('squad-modal-save')?.addEventListener('click', handleSaveSquad);
  $('squad-modal-cancel')?.addEventListener('click', closeModal);
  $('squad-modal-close')?.addEventListener('click', closeModal);
  $('squad-member-save')?.addEventListener('click', handleAddMember);
  $('squad-member-cancel')?.addEventListener('click', closeMemberModal);
  $('squad-member-close')?.addEventListener('click', closeMemberModal);
}

// Auto-bind on import
bindEvents();
