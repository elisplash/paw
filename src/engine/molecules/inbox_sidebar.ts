// src/engine/molecules/inbox_sidebar.ts
// Phase 11.3 — Inbox context sidebar molecule (right panel).
// Re-parents the existing chat-mission-panel into the inbox layout's
// right column so all original features (context window, metrics, jobs,
// quick actions, prompts, automations, queries, agent selector) keep
// working with their existing JS wiring.

// ── Types ────────────────────────────────────────────────────────────────

export interface InboxSidebarController {
  /** Root DOM element (the wrapper div placed in the inbox grid) */
  el: HTMLElement;
  /** Show / hide the sidebar */
  toggle(open: boolean): void;
  /** Destroy + cleanup — returns mission panel to original parent */
  destroy(): void;
}

export interface InboxSidebarCallbacks {
  onRename: () => void;
  onDelete: () => void;
  onClear: () => void;
  onCompact: () => void;
  /** Optional: search inside conversation */
  onSearch?: (query: string) => void;
}

// ── Factory ──────────────────────────────────────────────────────────────

export function createInboxSidebar(callbacks: InboxSidebarCallbacks): InboxSidebarController {
  let destroyed = false;

  // Create the grid-column wrapper
  const root = document.createElement('div');
  root.className = 'inbox-sidebar';

  // Grab the existing mission panel from the DOM and move it in
  const missionPanel = document.getElementById('chat-mission-panel');
  let _originalParent: HTMLElement | null = null;

  if (missionPanel) {
    _originalParent = missionPanel.parentElement as HTMLElement | null;
    // Make it visible (inbox_controller hides it before we move it)
    missionPanel.style.display = '';
    root.appendChild(missionPanel);
  }

  // ── Wire quick-action buttons to callbacks ─────────────────────────────
  const renameBtn = document.getElementById('session-rename-btn');
  const deleteBtn = document.getElementById('session-delete-btn');
  const clearBtn = document.getElementById('session-clear-btn');
  const compactBtn = document.getElementById('session-compact-btn');

  const onRename = (e: Event) => {
    e.preventDefault();
    callbacks.onRename();
  };
  const onDelete = (e: Event) => {
    e.preventDefault();
    callbacks.onDelete();
  };
  const onClear = (e: Event) => {
    e.preventDefault();
    callbacks.onClear();
  };
  const onCompact = (e: Event) => {
    e.preventDefault();
    callbacks.onCompact();
  };

  renameBtn?.addEventListener('click', onRename);
  deleteBtn?.addEventListener('click', onDelete);
  clearBtn?.addEventListener('click', onClear);
  compactBtn?.addEventListener('click', onCompact);

  // ── Wire search-in-conversation ────────────────────────────────────────
  const searchInput = document.getElementById('mission-search-conv') as HTMLInputElement | null;
  let _searchDebounce: ReturnType<typeof setTimeout>;

  if (searchInput && callbacks.onSearch) {
    const onSearchCb = callbacks.onSearch;
    searchInput.addEventListener('input', () => {
      clearTimeout(_searchDebounce);
      _searchDebounce = setTimeout(() => onSearchCb(searchInput.value), 250);
    });
  }

  // ── Controller ─────────────────────────────────────────────────────────

  const ctrl: InboxSidebarController = {
    el: root,

    toggle(open: boolean) {
      root.style.display = open ? '' : 'none';
    },

    destroy() {
      if (destroyed) return;
      destroyed = true;

      // Remove event listeners
      renameBtn?.removeEventListener('click', onRename);
      deleteBtn?.removeEventListener('click', onDelete);
      clearBtn?.removeEventListener('click', onClear);
      compactBtn?.removeEventListener('click', onCompact);

      // Move mission panel back to its original parent
      if (missionPanel && _originalParent) {
        missionPanel.style.display = '';
        _originalParent.appendChild(missionPanel);
      }

      root.remove();
    },
  };

  return ctrl;
}
