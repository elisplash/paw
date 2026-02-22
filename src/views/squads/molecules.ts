// src/views/squads/molecules.ts — Squads view list & detail logic

import { pawEngine } from '../../engine';
import { showToast } from '../../components/toast';
import type { EngineSquad } from '../../engine/atoms/types';
import { renderSquadCard, renderSquadDetail } from './atoms';
import { openEditModal, openAddMemberModal } from './modals';

let squads: EngineSquad[] = [];
let activeSquadId: string | null = null;

const $ = (id: string) => document.getElementById(id);

/** Return the current squads list (for use by modals). */
export function getSquads(): EngineSquad[] {
  return squads;
}

/** Set the active squad id (for use by modals after create). */
export function setActiveSquadId(id: string | null): void {
  activeSquadId = id;
}

/** Load all squads from the engine. */
export async function loadSquads(): Promise<void> {
  try {
    squads = await pawEngine.squadsList();
  } catch (e) {
    console.warn('[squads] Failed to load:', e);
    squads = [];
  }
  renderSquadList();
  if (activeSquadId) {
    const found = squads.find((s) => s.id === activeSquadId);
    if (found) renderDetail(found);
    else clearDetail();
  } else {
    clearDetail();
  }
}

function renderSquadList(): void {
  const listEl = $('squads-list');
  const emptyEl = $('squads-empty');
  if (!listEl) return;

  if (squads.length === 0) {
    listEl.innerHTML = '';
    if (emptyEl) emptyEl.style.display = 'flex';
    return;
  }
  if (emptyEl) emptyEl.style.display = 'none';
  listEl.innerHTML = squads.map((s) => renderSquadCard(s, s.id === activeSquadId)).join('');

  listEl.querySelectorAll('.squad-card').forEach((card) => {
    card.addEventListener('click', () => {
      const id = (card as HTMLElement).dataset.squadId;
      if (id) selectSquad(id);
    });
  });
}

function selectSquad(id: string): void {
  activeSquadId = id;
  const squad = squads.find((s) => s.id === id);
  if (!squad) return;
  renderSquadList();
  renderDetail(squad);
}

function renderDetail(squad: EngineSquad): void {
  const detailEl = $('squads-detail');
  if (!detailEl) return;
  detailEl.style.display = 'block';
  detailEl.innerHTML = renderSquadDetail(squad);

  $('squad-edit-btn')?.addEventListener('click', () => openEditModal(squad));
  $('squad-delete-btn')?.addEventListener('click', () => deleteSquad(squad.id));
  $('squad-add-member-btn')?.addEventListener('click', () => openAddMemberModal(squad));

  detailEl.querySelectorAll('.squad-remove-member').forEach((btn) => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const agentId = (btn as HTMLElement).dataset.agentId;
      if (!agentId) return;
      try {
        await pawEngine.squadRemoveMember(squad.id, agentId);
        showToast('Member removed', 'success');
        loadSquads();
      } catch (err) {
        showToast(`Failed to remove member: ${err}`, 'error');
      }
    });
  });
}

function clearDetail(): void {
  const detailEl = $('squads-detail');
  if (detailEl) {
    detailEl.style.display = 'none';
    detailEl.innerHTML = '';
  }
}

async function deleteSquad(squadId: string): Promise<void> {
  try {
    await pawEngine.squadDelete(squadId);
    showToast('Squad deleted', 'success');
    activeSquadId = null;
    loadSquads();
  } catch (e) {
    showToast(`Failed to delete squad: ${e}`, 'error');
  }
}
