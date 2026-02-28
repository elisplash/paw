// ─────────────────────────────────────────────────────────────────────────────
// Flow Visualization Engine — List Molecules
// Flow list sidebar: flow items, folders, drag-and-drop reordering.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowGraph } from './atoms';
import { formatDate, escAttr } from './molecule-state';

// Track which folders are collapsed
const _collapsedFolders = new Set<string>();

function renderFlowItem(g: FlowGraph, activeId: string | null): string {
  return `
    <div class="flow-list-item${g.id === activeId ? ' active' : ''}" data-flow-id="${g.id}" draggable="true">
      <span class="ms flow-list-icon">account_tree</span>
      <div class="flow-list-meta">
        <div class="flow-list-name">${g.name}</div>
        <div class="flow-list-date">${formatDate(g.updatedAt)}</div>
      </div>
      <button class="flow-list-del" data-del-id="${g.id}" title="Delete"><span class="ms">close</span></button>
    </div>`;
}

export function renderFlowList(
  container: HTMLElement,
  graphs: FlowGraph[],
  activeId: string | null,
  onSelect: (id: string) => void,
  onDelete: (id: string) => void,
  onNew: () => void,
  onMoveToFolder?: (flowId: string, folder: string) => void,
) {
  // Group flows by folder
  const folders = new Map<string, FlowGraph[]>();
  const rootFlows: FlowGraph[] = [];

  for (const g of graphs) {
    const folder = g.folder?.trim();
    if (folder) {
      if (!folders.has(folder)) folders.set(folder, []);
      folders.get(folder)!.push(g);
    } else {
      rootFlows.push(g);
    }
  }

  const sortedFolders = [...folders.entries()].sort((a, b) => a[0].localeCompare(b[0]));

  let foldersHtml = '';
  for (const [folderName, flows] of sortedFolders) {
    const collapsed = _collapsedFolders.has(folderName);
    foldersHtml += `
      <div class="flow-folder" data-folder="${escAttr(folderName)}">
        <div class="flow-folder-header" data-folder-toggle="${escAttr(folderName)}">
          <span class="ms flow-folder-chevron">${collapsed ? 'chevron_right' : 'expand_more'}</span>
          <span class="ms flow-folder-icon">folder</span>
          <span class="flow-folder-name">${folderName}</span>
          <span class="flow-folder-count">${flows.length}</span>
        </div>
        ${collapsed ? '' : `<div class="flow-folder-items">${flows.map((g) => renderFlowItem(g, activeId)).join('')}</div>`}
      </div>`;
  }

  const rootHtml =
    rootFlows.length > 0
      ? rootFlows.map((g) => renderFlowItem(g, activeId)).join('')
      : sortedFolders.length === 0
        ? '<div class="flow-list-empty">No flows yet.<br>Create one or use <code>/flow</code> in Chat.</div>'
        : '';

  container.innerHTML = `
    <div class="flow-list-header">
      <h3>Flows</h3>
      <div class="flow-list-actions">
        <button class="flow-list-new-btn" data-action="new-folder" title="New Folder"><span class="ms">create_new_folder</span></button>
        <button class="flow-list-new-btn" title="New Flow"><span class="ms">add</span></button>
      </div>
    </div>
    <div class="flow-list-items">
      ${foldersHtml}
      ${rootHtml}
    </div>
  `;

  // Wire new flow button
  container
    .querySelector('.flow-list-new-btn:not([data-action])')
    ?.addEventListener('click', onNew);

  // Wire new folder button
  container.querySelector('[data-action="new-folder"]')?.addEventListener('click', () => {
    const name = prompt('Folder name:');
    if (!name?.trim()) return;
    onNew();
  });

  // Wire folder toggles
  container.querySelectorAll('[data-folder-toggle]').forEach((header) => {
    header.addEventListener('click', () => {
      const folder = (header as HTMLElement).dataset.folderToggle!;
      if (_collapsedFolders.has(folder)) _collapsedFolders.delete(folder);
      else _collapsedFolders.add(folder);
      renderFlowList(container, graphs, activeId, onSelect, onDelete, onNew, onMoveToFolder);
    });
  });

  // Wire flow item clicks & deletes
  container.querySelectorAll('.flow-list-item').forEach((el) => {
    const id = (el as HTMLElement).dataset.flowId!;
    el.addEventListener('click', (e) => {
      if ((e.target as HTMLElement).closest('.flow-list-del')) return;
      onSelect(id);
    });
  });
  container.querySelectorAll('.flow-list-del').forEach((btn) => {
    btn.addEventListener('click', () => {
      const id = (btn as HTMLElement).dataset.delId!;
      onDelete(id);
    });
  });

  // Drag-and-drop: flows can be dragged into folders
  container.querySelectorAll('.flow-list-item[draggable]').forEach((item) => {
    item.addEventListener('dragstart', (e) => {
      const ev = e as DragEvent;
      const flowId = (item as HTMLElement).dataset.flowId!;
      ev.dataTransfer?.setData('text/plain', flowId);
      (item as HTMLElement).classList.add('flow-list-dragging');
    });
    item.addEventListener('dragend', () => {
      (item as HTMLElement).classList.remove('flow-list-dragging');
      container
        .querySelectorAll('.flow-folder-drop-target')
        .forEach((f) => f.classList.remove('flow-folder-drop-target'));
    });
  });

  // Folder drop targets
  container.querySelectorAll('.flow-folder').forEach((folder) => {
    const folderName = (folder as HTMLElement).dataset.folder!;
    folder.addEventListener('dragover', (e) => {
      (e as DragEvent).preventDefault();
      (folder as HTMLElement).classList.add('flow-folder-drop-target');
    });
    folder.addEventListener('dragleave', () => {
      (folder as HTMLElement).classList.remove('flow-folder-drop-target');
    });
    folder.addEventListener('drop', (e) => {
      (e as DragEvent).preventDefault();
      (folder as HTMLElement).classList.remove('flow-folder-drop-target');
      const flowId = (e as DragEvent).dataTransfer?.getData('text/plain');
      if (flowId && onMoveToFolder) onMoveToFolder(flowId, folderName);
    });
  });

  // Drop on root area
  const listItems = container.querySelector('.flow-list-items');
  if (listItems) {
    listItems.addEventListener('dragover', (e) => {
      const ev = e as DragEvent;
      if (!(ev.target as HTMLElement).closest('.flow-folder')) {
        ev.preventDefault();
      }
    });
    listItems.addEventListener('drop', (e) => {
      const ev = e as DragEvent;
      if ((ev.target as HTMLElement).closest('.flow-folder')) return;
      ev.preventDefault();
      const flowId = ev.dataTransfer?.getData('text/plain');
      if (flowId && onMoveToFolder) onMoveToFolder(flowId, '');
    });
  }
}
