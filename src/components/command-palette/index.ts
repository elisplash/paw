// index.ts — Command palette barrel + global keyboard binding + shortcuts overlay
// Wires atoms + molecules together and exports init function

import { openPalette, closePalette, isPaletteOpen } from './molecules';
import type { PaletteItem, SkillInfo } from './atoms';

export { openPalette, closePalette, isPaletteOpen } from './molecules';
export { destroyPalette } from './molecules';
export type { PaletteItem, PaletteItemKind, AgentInfo, SkillInfo } from './atoms';

type GetAgentsFn = () => { id: string; name: string; avatar: string }[];
type SwitchViewFn = (viewName: string) => void;
type SwitchAgentFn = (agentId: string) => Promise<void>;
type ActionFn = (action: string) => void;
type GetSkillsFn = () => SkillInfo[];

let _getAgents: GetAgentsFn = () => [];
let _switchView: SwitchViewFn = () => {};
let _switchAgent: SwitchAgentFn = async () => {};
let _onAction: ActionFn = () => {};
let _getSkills: GetSkillsFn = () => [];

function handleSelect(item: PaletteItem) {
  if (item.kind === 'view') {
    _switchView(item.payload);
  } else if (item.kind === 'agent') {
    _switchView('chat');
    void _switchAgent(item.payload);
  } else if (item.kind === 'action') {
    _onAction(item.payload);
  }
}

// ── Sidebar nav positions (1-9) – matches index.html order ──────────
const NAV_KEYS: Record<string, string> = {
  '1': 'today',
  '2': 'chat',
  '3': 'agents',
  '4': 'tasks',
  '5': 'mail',
  '6': 'channels',
  '7': 'skills',
  '8': 'pawzhub',
  '9': 'foundry',
};

// ── Keyboard Shortcuts Overlay ──────────────────────────────────────

let _shortcutsOverlay: HTMLElement | null = null;

const SHORTCUT_SECTIONS = [
  {
    title: 'Navigation',
    items: [
      { keys: ['1–9'], desc: 'Switch sidebar tab by position' },
      { keys: ['⌘ K'], desc: 'Open command palette' },
      { keys: ['⌘ ,'], desc: 'Open settings' },
      { keys: ['?'], desc: 'Show this help' },
    ],
  },
  {
    title: 'Actions',
    items: [
      { keys: ['⌘ N'], desc: 'New task / new chat (context-aware)' },
      { keys: ['Esc'], desc: 'Close palette / modal / overlay' },
      { keys: ['↑ ↓'], desc: 'Navigate palette items' },
      { keys: ['Enter'], desc: 'Select palette item' },
    ],
  },
  {
    title: 'Command Palette',
    items: [
      { keys: ['type name'], desc: 'Search agents, views, skills' },
      { keys: ['new task'], desc: 'Create a task' },
      { keys: ['new chat'], desc: 'Start a chat' },
      { keys: ['theme'], desc: 'Toggle dark/light mode' },
    ],
  },
];

function openShortcutsOverlay() {
  if (_shortcutsOverlay) {
    _shortcutsOverlay.classList.add('visible');
    return;
  }

  const overlay = document.createElement('div');
  overlay.className = 'shortcuts-overlay visible';
  overlay.addEventListener('click', (e) => {
    if (e.target === overlay) closeShortcutsOverlay();
  });

  const sections = SHORTCUT_SECTIONS.map(
    (s) => `
    <div class="shortcuts-section">
      <div class="shortcuts-section-title">${s.title}</div>
      ${s.items
        .map(
          (item) => `
        <div class="shortcuts-row">
          <span class="shortcuts-keys">${item.keys.map((k) => `<kbd>${k}</kbd>`).join(' ')}</span>
          <span class="shortcuts-desc">${item.desc}</span>
        </div>`,
        )
        .join('')}
    </div>`,
  ).join('');

  overlay.innerHTML = `
    <div class="shortcuts-dialog">
      <div class="shortcuts-header">
        <span class="shortcuts-title">Keyboard Shortcuts</span>
        <button class="btn-icon shortcuts-close">×</button>
      </div>
      <div class="shortcuts-body">${sections}</div>
      <div class="shortcuts-footer">
        Press <kbd>?</kbd> to toggle · <kbd>Esc</kbd> to close
      </div>
    </div>
  `;

  overlay.querySelector('.shortcuts-close')?.addEventListener('click', closeShortcutsOverlay);
  document.body.appendChild(overlay);
  _shortcutsOverlay = overlay;
}

function closeShortcutsOverlay() {
  _shortcutsOverlay?.classList.remove('visible');
}

function isShortcutsOpen(): boolean {
  return _shortcutsOverlay?.classList.contains('visible') ?? false;
}

// ── Global keydown handler ──────────────────────────────────────────

function isInputFocused(): boolean {
  const el = document.activeElement;
  if (!el) return false;
  const tag = el.tagName;
  return (
    tag === 'INPUT' ||
    tag === 'TEXTAREA' ||
    tag === 'SELECT' ||
    (el as HTMLElement).isContentEditable
  );
}

function onGlobalKeydown(e: KeyboardEvent) {
  // Cmd+K (Mac) or Ctrl+K (Win/Linux) — command palette toggle
  if (e.key === 'k' && (e.metaKey || e.ctrlKey)) {
    e.preventDefault();
    if (isPaletteOpen()) {
      closePalette();
    } else {
      const agents = _getAgents();
      const skills = _getSkills();
      openPalette(agents, handleSelect, skills);
    }
    return;
  }

  // Cmd+, → settings
  if (e.key === ',' && (e.metaKey || e.ctrlKey)) {
    e.preventDefault();
    _switchView('settings');
    return;
  }

  // Cmd+N → context-aware new (task if on tasks view, else new chat)
  if (e.key === 'n' && (e.metaKey || e.ctrlKey)) {
    e.preventDefault();
    _onAction('new-task');
    return;
  }

  // Escape → close shortcuts overlay
  if (e.key === 'Escape' && isShortcutsOpen()) {
    closeShortcutsOverlay();
    return;
  }

  // Don't intercept when typing in inputs
  if (isInputFocused() || isPaletteOpen()) return;

  // ? → toggle shortcuts overlay
  if (e.key === '?' || (e.key === '/' && e.shiftKey)) {
    e.preventDefault();
    if (isShortcutsOpen()) {
      closeShortcutsOverlay();
    } else {
      openShortcutsOverlay();
    }
    return;
  }

  // 1-9 → sidebar navigation (only when not in an input)
  if (NAV_KEYS[e.key] && !e.metaKey && !e.ctrlKey && !e.altKey) {
    e.preventDefault();
    _switchView(NAV_KEYS[e.key]);
    return;
  }
}

/** Initialise the command palette global shortcut. Call once from main.ts. */
export function initCommandPalette(deps: {
  getAgents: GetAgentsFn;
  switchView: SwitchViewFn;
  switchAgent: SwitchAgentFn;
  onAction?: ActionFn;
  getSkills?: GetSkillsFn;
}) {
  _getAgents = deps.getAgents;
  _switchView = deps.switchView;
  _switchAgent = deps.switchAgent;
  _onAction = deps.onAction ?? (() => {});
  _getSkills = deps.getSkills ?? (() => []);
  document.addEventListener('keydown', onGlobalKeydown);
}
