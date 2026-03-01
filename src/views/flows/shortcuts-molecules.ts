// ─────────────────────────────────────────────────────────────────────────────
// Flow Visualization Engine — Shortcuts Overlay Molecules (Phase 5.9)
// Keyboard shortcuts cheat sheet modal, toggled with "?".
// ─────────────────────────────────────────────────────────────────────────────

// ── Types ──────────────────────────────────────────────────────────────────

export interface ShortcutEntry {
  keys: string[]; // e.g. ['Ctrl', 'Z']
  label: string;
  category: ShortcutCategory;
}

export type ShortcutCategory = 'Navigation' | 'Editing' | 'Execution' | 'Debug';

// ── Shortcut Registry ──────────────────────────────────────────────────────

export const SHORTCUT_REGISTRY: ShortcutEntry[] = [
  // Navigation
  { keys: ['Space', 'Drag'], label: 'Pan canvas', category: 'Navigation' },
  { keys: ['Ctrl', '+'], label: 'Zoom in', category: 'Navigation' },
  { keys: ['Ctrl', '−'], label: 'Zoom out', category: 'Navigation' },
  { keys: ['Ctrl', '0'], label: 'Reset zoom', category: 'Navigation' },
  { keys: ['Ctrl', 'F'], label: 'Fit to view', category: 'Navigation' },
  { keys: ['M'], label: 'Toggle minimap', category: 'Navigation' },

  // Editing
  { keys: ['Ctrl', 'Z'], label: 'Undo', category: 'Editing' },
  { keys: ['Ctrl', 'Shift', 'Z'], label: 'Redo', category: 'Editing' },
  { keys: ['Ctrl', 'C'], label: 'Copy selection', category: 'Editing' },
  { keys: ['Ctrl', 'V'], label: 'Paste', category: 'Editing' },
  { keys: ['Ctrl', 'A'], label: 'Select all', category: 'Editing' },
  { keys: ['Delete'], label: 'Delete selection', category: 'Editing' },
  { keys: ['Ctrl', 'D'], label: 'Duplicate selection', category: 'Editing' },
  { keys: ['Ctrl', 'G'], label: 'Group selection', category: 'Editing' },

  // Execution
  { keys: ['F5'], label: 'Run flow', category: 'Execution' },
  { keys: ['Shift', 'F5'], label: 'Stop execution', category: 'Execution' },
  { keys: ['F6'], label: 'Run selected node', category: 'Execution' },
  { keys: ['Ctrl', 'Enter'], label: 'Run from cursor', category: 'Execution' },

  // Debug
  { keys: ['F9'], label: 'Toggle breakpoint', category: 'Debug' },
  { keys: ['F10'], label: 'Step over', category: 'Debug' },
  { keys: ['F11'], label: 'Step into sub-flow', category: 'Debug' },
  { keys: ['Ctrl', 'L'], label: 'Toggle data labels', category: 'Debug' },
  { keys: ['?'], label: 'Show shortcuts', category: 'Debug' },
];

const CATEGORIES: ShortcutCategory[] = ['Navigation', 'Editing', 'Execution', 'Debug'];

const CATEGORY_ICONS: Record<ShortcutCategory, string> = {
  Navigation: 'explore',
  Editing: 'edit',
  Execution: 'play_arrow',
  Debug: 'bug_report',
};

// ── State ──────────────────────────────────────────────────────────────────

let _overlayEl: HTMLElement | null = null;
let _keyHandler: ((e: KeyboardEvent) => void) | null = null;

// ── Public API ─────────────────────────────────────────────────────────────

/** Toggle shortcuts overlay visibility. */
export function toggleShortcutsOverlay(container: HTMLElement): void {
  if (_overlayEl) {
    hideShortcutsOverlay();
  } else {
    showShortcutsOverlay(container);
  }
}

/** Show the shortcuts overlay. */
export function showShortcutsOverlay(container: HTMLElement): void {
  if (_overlayEl) return;

  _overlayEl = document.createElement('div');
  _overlayEl.className = 'flow-shortcuts-modal-backdrop';
  _overlayEl.innerHTML = buildModalHTML('');
  container.appendChild(_overlayEl);

  // Focus search
  const input = _overlayEl.querySelector<HTMLInputElement>('.flow-shortcuts-search');
  input?.focus();

  // Search handler
  input?.addEventListener('input', () => {
    const body = _overlayEl?.querySelector('.flow-shortcuts-body');
    if (body) body.innerHTML = buildCategoryHTML(input.value.trim());
  });

  // Close on backdrop click
  _overlayEl.addEventListener('click', (e) => {
    if ((e.target as HTMLElement).classList.contains('flow-shortcuts-modal-backdrop')) {
      hideShortcutsOverlay();
    }
  });

  // Close on Escape
  _keyHandler = (e: KeyboardEvent) => {
    if (e.key === 'Escape') hideShortcutsOverlay();
  };
  document.addEventListener('keydown', _keyHandler);
}

/** Hide the shortcuts overlay. */
export function hideShortcutsOverlay(): void {
  if (_overlayEl) {
    _overlayEl.remove();
    _overlayEl = null;
  }
  if (_keyHandler) {
    document.removeEventListener('keydown', _keyHandler);
    _keyHandler = null;
  }
}

/** Check visibility state. */
export function isShortcutsVisible(): boolean {
  return _overlayEl != null;
}

/** Search shortcuts by fuzzy label match. */
export function searchShortcuts(query: string): ShortcutEntry[] {
  if (!query) return SHORTCUT_REGISTRY;
  const lower = query.toLowerCase();
  return SHORTCUT_REGISTRY.filter(
    (s) =>
      s.label.toLowerCase().includes(lower) ||
      s.keys.some((k) => k.toLowerCase().includes(lower)) ||
      s.category.toLowerCase().includes(lower),
  );
}

// ── HTML Builders ──────────────────────────────────────────────────────────

function buildModalHTML(filter: string): string {
  return `
    <div class="flow-shortcuts-modal">
      <div class="flow-shortcuts-header">
        <span class="ms" style="font-size:18px">keyboard</span>
        <span>Keyboard Shortcuts</span>
        <button class="flow-shortcuts-close" onclick="this.closest('.flow-shortcuts-modal-backdrop')?.remove()">
          <span class="ms">close</span>
        </button>
      </div>
      <div class="flow-shortcuts-search-wrap">
        <span class="ms" style="font-size:14px;opacity:.5">search</span>
        <input type="text" class="flow-shortcuts-search" placeholder="Search shortcuts…" value="${escHtml(filter)}" />
      </div>
      <div class="flow-shortcuts-body">
        ${buildCategoryHTML(filter)}
      </div>
    </div>
  `;
}

function buildCategoryHTML(filter: string): string {
  const matches = searchShortcuts(filter);
  return CATEGORIES.map((cat) => {
    const entries = matches.filter((s) => s.category === cat);
    if (entries.length === 0) return '';
    return `
      <div class="flow-shortcuts-category">
        <div class="flow-shortcuts-category-title">
          <span class="ms" style="font-size:14px">${CATEGORY_ICONS[cat]}</span>
          ${cat}
        </div>
        ${entries.map(renderShortcutRow).join('')}
      </div>
    `;
  }).join('');
}

function renderShortcutRow(entry: ShortcutEntry): string {
  const keysHTML = entry.keys.map((k) => `<kbd>${escHtml(k)}</kbd>`).join(' ');
  return `
    <div class="flow-shortcuts-row">
      <span class="flow-shortcuts-label">${escHtml(entry.label)}</span>
      <span class="flow-shortcuts-keys">${keysHTML}</span>
    </div>
  `;
}

function escHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
