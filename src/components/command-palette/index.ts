// index.ts — Command palette barrel + global keyboard binding
// Wires atoms + molecules together and exports init function

import { openPalette, closePalette, isPaletteOpen } from './molecules';
import type { PaletteItem } from './atoms';

export { openPalette, closePalette, isPaletteOpen } from './molecules';
export { destroyPalette } from './molecules';
export type { PaletteItem, PaletteItemKind, AgentInfo } from './atoms';

type GetAgentsFn = () => { id: string; name: string; avatar: string }[];
type SwitchViewFn = (viewName: string) => void;
type SwitchAgentFn = (agentId: string) => Promise<void>;

let _getAgents: GetAgentsFn = () => [];
let _switchView: SwitchViewFn = () => {};
let _switchAgent: SwitchAgentFn = async () => {};

function handleSelect(item: PaletteItem) {
  if (item.kind === 'view') {
    _switchView(item.payload);
  } else if (item.kind === 'agent') {
    _switchView('chat');
    void _switchAgent(item.payload);
  }
}

function onGlobalKeydown(e: KeyboardEvent) {
  // Cmd+K (Mac) or Ctrl+K (Win/Linux)
  if (e.key === 'k' && (e.metaKey || e.ctrlKey)) {
    e.preventDefault();
    if (isPaletteOpen()) {
      closePalette();
    } else {
      const agents = _getAgents();
      openPalette(agents, handleSelect);
    }
  }
}

/** Initialise the command palette global shortcut. Call once from main.ts. */
export function initCommandPalette(deps: {
  getAgents: GetAgentsFn;
  switchView: SwitchViewFn;
  switchAgent: SwitchAgentFn;
}) {
  _getAgents = deps.getAgents;
  _switchView = deps.switchView;
  _switchAgent = deps.switchAgent;
  document.addEventListener('keydown', onGlobalKeydown);
}
