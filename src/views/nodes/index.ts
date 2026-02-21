// Nodes — orchestration + public API
import { $ } from '../../components/helpers';
import { loadNodes } from './molecules';

export {
  loadPairingRequests,
  handleNodePairRequested,
  handleNodePairResolved,
  configureCallbacks,
} from './atoms';
export { loadNodes };

export function initNodesEvents() {
  $('nodes-refresh')?.addEventListener('click', () => loadNodes());
}
