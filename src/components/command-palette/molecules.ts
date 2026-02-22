// molecules.ts — Command palette DOM rendering and event handling
// Uses atoms for pure logic, accesses DOM for rendering

import {
  buildPaletteItems,
  filterPaletteItems,
  clampIndex,
  type PaletteItem,
  type AgentInfo,
} from './atoms';

let _overlay: HTMLElement | null = null;
let _input: HTMLInputElement | null = null;
let _list: HTMLElement | null = null;
let _allItems: PaletteItem[] = [];
let _filteredItems: PaletteItem[] = [];
let _selectedIndex = 0;
let _onSelect: ((item: PaletteItem) => void) | null = null;
let _isOpen = false;

/** Create the palette DOM if it doesn't exist yet. */
function ensureDOM(): { overlay: HTMLElement; input: HTMLInputElement; list: HTMLElement } {
  if (_overlay && _input && _list) return { overlay: _overlay, input: _input, list: _list };

  const overlay = document.createElement('div');
  overlay.className = 'command-palette-overlay';
  overlay.addEventListener('click', (e) => {
    if (e.target === overlay) closePalette();
  });

  const container = document.createElement('div');
  container.className = 'command-palette';

  const input = document.createElement('input');
  input.className = 'command-palette-input';
  input.type = 'text';
  input.placeholder = 'Type to search agents, views…';
  input.addEventListener('input', () => onQueryChange(input.value));
  input.addEventListener('keydown', onInputKeydown);

  const list = document.createElement('div');
  list.className = 'command-palette-list';

  container.appendChild(input);
  container.appendChild(list);
  overlay.appendChild(container);
  document.body.appendChild(overlay);

  _overlay = overlay;
  _input = input;
  _list = list;

  return { overlay, input, list };
}

function onQueryChange(query: string) {
  _filteredItems = filterPaletteItems(_allItems, query);
  _selectedIndex = clampIndex(0, _filteredItems.length);
  renderList();
}

function onInputKeydown(e: KeyboardEvent) {
  if (e.key === 'ArrowDown') {
    e.preventDefault();
    _selectedIndex = clampIndex(_selectedIndex + 1, _filteredItems.length);
    renderList();
  } else if (e.key === 'ArrowUp') {
    e.preventDefault();
    _selectedIndex = clampIndex(_selectedIndex - 1, _filteredItems.length);
    renderList();
  } else if (e.key === 'Enter') {
    e.preventDefault();
    const item = _filteredItems[_selectedIndex];
    if (item) selectItem(item);
  } else if (e.key === 'Escape') {
    e.preventDefault();
    closePalette();
  }
}

function selectItem(item: PaletteItem) {
  closePalette();
  if (_onSelect) _onSelect(item);
}

function renderList() {
  if (!_list) return;
  const html = _filteredItems
    .map((item, i) => {
      const active = i === _selectedIndex ? ' active' : '';
      const kindBadge = item.kind === 'agent' ? 'Agent' : 'View';
      return `<div class="command-palette-item${active}" data-index="${i}">
        <span class="command-palette-item-icon">${item.icon ?? ''}</span>
        <span class="command-palette-item-label">${item.label}</span>
        <span class="command-palette-item-badge">${kindBadge}</span>
      </div>`;
    })
    .join('');
  _list.innerHTML = html;

  // Click handlers
  _list.querySelectorAll('.command-palette-item').forEach((el) => {
    el.addEventListener('click', () => {
      const idx = parseInt(el.getAttribute('data-index') ?? '0', 10);
      const item = _filteredItems[idx];
      if (item) selectItem(item);
    });
  });

  // Scroll active item into view
  const activeEl = _list.querySelector('.command-palette-item.active');
  if (activeEl) activeEl.scrollIntoView({ block: 'nearest' });
}

/** Open the command palette. */
export function openPalette(agents: AgentInfo[], onSelect: (item: PaletteItem) => void) {
  const { overlay, input } = ensureDOM();
  _allItems = buildPaletteItems(agents);
  _filteredItems = _allItems;
  _selectedIndex = 0;
  _onSelect = onSelect;
  _isOpen = true;

  overlay.classList.add('visible');
  input.value = '';
  renderList();
  // Focus after a microtask so the overlay transition starts
  requestAnimationFrame(() => input.focus());
}

/** Close the command palette. */
export function closePalette() {
  _isOpen = false;
  if (_overlay) _overlay.classList.remove('visible');
}

/** Whether the palette is currently open. */
export function isPaletteOpen(): boolean {
  return _isOpen;
}

/** Tear down DOM (useful for tests). */
export function destroyPalette() {
  if (_overlay) {
    _overlay.remove();
    _overlay = null;
    _input = null;
    _list = null;
  }
  _isOpen = false;
  _allItems = [];
  _filteredItems = [];
  _selectedIndex = 0;
  _onSelect = null;
}
