// Engine (Nodes) — Index
import { $ } from '../../components/helpers';
import { loadNodes } from './molecules';

export { loadNodes };

export function initNodesEvents() {
  $('nodes-refresh')?.addEventListener('click', () => loadNodes());
}
