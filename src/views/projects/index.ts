// Projects View — Index (orchestration, state, event wiring, exports)

import { $ } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { logSecurityEvent } from '../../db';
import {
  type ProjectFolder,
  isSensitivePath,
  loadSavedProjects,
  savePersistProjects,
} from './atoms';
import {
  renderProjectsSidebar,
  selectProject,
  showProjectsEmpty,
  initTauriRefs,
  setModuleState,
  setOnRemoveProject,
} from './molecules';
import { initShellRefs, clearGitInfoCache } from './git';

// ── Public re-exports ──────────────────────────────────────────────────────

export type { FileEntry, ProjectFolder, GitInfo } from './atoms';
export { isSensitivePath, isOutOfProjectScope } from './atoms';
export { renderProjectsSidebar, showProjectsEmpty, selectProject } from './molecules';

// ── Module state ───────────────────────────────────────────────────────────

let _projects: ProjectFolder[] = [];
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const _fileTreeCache = new Map<string, any>();
const _expandedPaths = new Set<string>();
let _tauriAvailable = false;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
let _homeDir: any = null;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
let _join: any = null;

// ── Tauri detection ───────────────────────────────────────────────────────

async function initTauri(): Promise<boolean> {
  try {
    const fs = await import('@tauri-apps/plugin-fs');
    const path = await import('@tauri-apps/api/path');
    _homeDir = path.homeDir;
    _join = path.join;
    _tauriAvailable = true;
    initTauriRefs({
      readDir: fs.readDir,
      readTextFile: fs.readTextFile,
      join: path.join,
      available: true,
    });
    return true;
  } catch {
    _tauriAvailable = false;
    initTauriRefs({ readDir: null, readTextFile: null, join: null, available: false });
    return false;
  }
}

async function initShell(): Promise<boolean> {
  try {
    const shell = await import('@tauri-apps/plugin-shell');
    initShellRefs(shell.Command, true);
    return true;
  } catch {
    initShellRefs(null, false);
    return false;
  }
}

function syncModuleState(): void {
  setModuleState({
    projects: _projects,
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    fileTreeCache: _fileTreeCache as any,
    expandedPaths: _expandedPaths,
  });
}

// ── Public API ─────────────────────────────────────────────────────────────

export async function loadProjects(): Promise<void> {
  await initTauri();
  await initShell();
  _projects = loadSavedProjects();
  syncModuleState();
  setOnRemoveProject(removeProject);
  renderProjectsSidebar();

  if (_projects.length === 0) {
    showProjectsEmpty();
  } else {
    await selectProject(_projects[0]);
  }
}

export async function addProjectFolder(path: string, name?: string): Promise<void> {
  // B4: Block sensitive directories
  const blocked = isSensitivePath(path);
  if (blocked) {
    showToast(
      `Blocked: "${path}" is a sensitive system directory (${blocked}). Cannot add as a project.`,
      'error',
    );
    logSecurityEvent({
      eventType: 'security_policy',
      riskLevel: 'high',
      toolName: 'projects.addFolder',
      command: path,
      detail: `Blocked sensitive path: ${blocked}`,
      wasAllowed: false,
      matchedPattern: blocked,
    }).catch(() => {});
    return;
  }

  // Check if already added
  if (_projects.some((p) => p.path === path)) {
    showToast('Project already added', 'warning');
    return;
  }

  const folderName = name || path.split('/').pop() || path.split('\\').pop() || path;
  const project: ProjectFolder = {
    name: folderName,
    path,
    addedAt: new Date().toISOString(),
  };
  _projects.push(project);
  savePersistProjects(_projects);
  syncModuleState();
  renderProjectsSidebar();
  await selectProject(project);
}

export async function removeProject(path: string): Promise<void> {
  _projects = _projects.filter((p) => p.path !== path);
  _fileTreeCache.delete(path);
  savePersistProjects(_projects);
  syncModuleState();
  renderProjectsSidebar();

  if (_projects.length > 0) {
    await selectProject(_projects[0]);
  } else {
    showProjectsEmpty();
  }
}

// ── Add Folder Dialog ──────────────────────────────────────────────────────

export async function promptAddFolder(): Promise<void> {
  if (_tauriAvailable) {
    try {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const tauriDialog = (window as any).__TAURI__?.dialog;
      if (tauriDialog?.open) {
        const selected = await tauriDialog.open({
          directory: true,
          multiple: false,
          title: 'Select a project folder',
        });
        if (selected && typeof selected === 'string') {
          await addProjectFolder(selected);
          return;
        }
      }
    } catch (e) {
      console.warn('[projects] Dialog not available, falling back to text input', e);
    }
  }

  const path = prompt('Enter the full path to your project folder:');
  if (path && path.trim()) {
    await addProjectFolder(path.trim());
  }
}

export async function addWorkspaceFolder(): Promise<void> {
  if (!_tauriAvailable || !_homeDir || !_join) return;
  try {
    const home = await _homeDir();
    const wsPath = await _join(home, 'Documents', 'Paw');
    await addProjectFolder(wsPath, 'Paw Workspace');
  } catch (e) {
    console.warn('[projects] Failed to add workspace folder:', e);
  }
}

// ── Event binding ─────────────────────────────────────────────────────────

export function bindEvents(): void {
  const addBtn = $('projects-add-folder');
  const refreshBtn = $('projects-refresh-btn');

  addBtn?.addEventListener('click', () => promptAddFolder());

  refreshBtn?.addEventListener('click', async () => {
    _fileTreeCache.clear();
    clearGitInfoCache();
    const activePath = document
      .querySelector('.projects-folder-item.active')
      ?.getAttribute('data-path');
    const project = _projects.find((p) => p.path === activePath);
    if (project) {
      await selectProject(project);
    }
    showToast('Refreshed', 'success');
  });
}
