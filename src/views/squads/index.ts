// src/views/squads/index.ts — Squads view entry point

export { loadSquads, getSquads, setActiveSquadId } from './molecules';
export {
  openCreateModal,
  handleSaveSquad,
  handleAddMember,
  closeModal,
  closeMemberModal,
} from './modals';

import { loadSquads } from './molecules';
import {
  openCreateModal,
  handleSaveSquad,
  handleAddMember,
  closeModal,
  closeMemberModal,
} from './modals';

const $ = (id: string) => document.getElementById(id);

// ── Tauri event listeners for real-time squad updates ──────────────────

interface TauriWindow {
  __TAURI__?: {
    event: {
      listen: <T>(event: string, handler: (event: { payload: T }) => void) => Promise<() => void>;
    };
  };
}

function bindTauriEvents(): void {
  const tw = window as unknown as TauriWindow;
  const listen = tw.__TAURI__?.event?.listen;
  if (!listen) return;

  // Refresh squad view when agents send messages
  listen('agent-message', () => {
    const squadView = $('squads-view');
    if (squadView && squadView.classList.contains('active')) loadSquads();
  });

  // Refresh when squads are created/updated/disbanded
  listen('squad-updated', () => {
    const squadView = $('squads-view');
    if (squadView && squadView.classList.contains('active')) loadSquads();
  });

  // Show swarm activity status in real time
  listen<{ agent_id: string; squad_id: string; status: string; summary?: string; error?: string }>(
    'swarm-activity',
    (event) => {
      const statusEl = $('squad-swarm-status');
      const detailEl = $('squad-swarm-detail');
      if (!statusEl || !detailEl) return;
      const { agent_id, status } = event.payload;
      if (status === 'waking') {
        statusEl.style.display = 'flex';
        detailEl.textContent = `${agent_id} is thinking...`;
      } else if (status === 'completed') {
        detailEl.textContent = `${agent_id} responded`;
        // Refresh messages to show the new response
        const squadView = $('squads-view');
        if (squadView && squadView.classList.contains('active')) loadSquads();
        // Hide status after a brief delay
        setTimeout(() => {
          statusEl.style.display = 'none';
        }, 3000);
      } else if (status === 'error') {
        detailEl.textContent = `${agent_id} failed`;
        setTimeout(() => {
          statusEl.style.display = 'none';
        }, 5000);
      }
    },
  );
}

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
bindTauriEvents();
