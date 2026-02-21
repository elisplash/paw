// Projects View — Molecules (DOM rendering, file tree, file viewer)

import { $, escHtml, escAttr, confirmModal } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { logSecurityEvent } from '../../db';
import {
  type FileEntry,
  type ProjectFolder,
  isSensitivePath,
  isOutOfProjectScope,
  getFileIcon,
  getLanguageClass,
  shortenPath,
  getDepth,
  BINARY_EXTENSIONS,
} from './atoms';
import { getGitInfo, getCachedGitInfo, renderGitBanner, bindGitActions } from './git';

// ── Tauri FS state (set by index.ts) ──────────────────────────────────────

let _tauriAvailable = false;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
let _readDir: any = null;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
let _readTextFile: any = null;
// eslint-disable-next-line @typescript-eslint/no-explicit-any
let _join: any = null;

export function initTauriRefs(opts: {
  readDir: any; // eslint-disable-line @typescript-eslint/no-explicit-any
  readTextFile: any; // eslint-disable-line @typescript-eslint/no-explicit-any
  join: any; // eslint-disable-line @typescript-eslint/no-explicit-any
  available: boolean;
}): void {
  _readDir = opts.readDir;
  _readTextFile = opts.readTextFile;
  _join = opts.join;
  _tauriAvailable = opts.available;
}

// ── Module shared state refs (set by index.ts) ────────────────────────────

let _projects: ProjectFolder[] = [];
let _selectedFile: FileEntry | null = null;
let _fileTreeCache = new Map<string, FileEntry[]>();
let _expandedPaths = new Set<string>();
let _activeProjectRoot: string | null = null;

export function setModuleState(opts: {
  projects: ProjectFolder[];
  fileTreeCache: Map<string, FileEntry[]>;
  expandedPaths: Set<string>;
}): void {
  _projects = opts.projects;
  _fileTreeCache = opts.fileTreeCache;
  _expandedPaths = opts.expandedPaths;
}

export function setActiveProjectRoot(root: string | null): void {
  _activeProjectRoot = root;
}

export function getSelectedFile(): FileEntry | null {
  return _selectedFile;
}

export function setSelectedFile(f: FileEntry | null): void {
  _selectedFile = f;
}

// Callback for selectProject to allow index.ts to call removeProject
let _onRemoveProject: ((path: string) => void) | null = null;
export function setOnRemoveProject(fn: (path: string) => void): void {
  _onRemoveProject = fn;
}

// ── File system operations ────────────────────────────────────────────────

async function loadDirectoryContents(dirPath: string): Promise<FileEntry[]> {
  if (!_tauriAvailable || !_readDir) return [];

  const scopeErr = isOutOfProjectScope(dirPath, _activeProjectRoot);
  if (scopeErr) {
    console.warn(`[projects] Scope violation: ${scopeErr}`);
    logSecurityEvent({
      eventType: 'scope_violation',
      riskLevel: 'high',
      toolName: 'projects.readDir',
      command: dirPath,
      detail: scopeErr,
      wasAllowed: false,
      matchedPattern: 'project_scope',
    }).catch(() => {});
    return [];
  }

  try {
    const entries = await _readDir(dirPath);
    const result: FileEntry[] = [];

    for (const entry of entries) {
      if (!entry.name) continue;
      if (entry.name.startsWith('.') && entry.name !== '.env') continue;
      if (
        [
          'node_modules',
          '__pycache__',
          '.git',
          'target',
          'dist',
          'build',
          '.next',
          'venv',
          '.venv',
        ].includes(entry.name)
      )
        continue;

      const fullPath = await _join(dirPath, entry.name);

      // B4: Skip sensitive directories in file tree
      if (entry.isDirectory && isSensitivePath(fullPath)) continue;

      result.push({
        name: entry.name,
        path: fullPath,
        isDirectory: entry.isDirectory || false,
      });
    }

    result.sort((a, b) => {
      if (a.isDirectory !== b.isDirectory) return a.isDirectory ? -1 : 1;
      return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
    });

    _fileTreeCache.set(dirPath, result);
    return result;
  } catch (e) {
    console.warn('[projects] Failed to read directory:', dirPath, e);
    return [];
  }
}

async function loadFileContent(filePath: string): Promise<string | null> {
  if (!_tauriAvailable || !_readTextFile) return null;

  const scopeErr = isOutOfProjectScope(filePath, _activeProjectRoot);
  if (scopeErr) {
    console.warn(`[projects] Scope violation: ${scopeErr}`);
    logSecurityEvent({
      eventType: 'scope_violation',
      riskLevel: 'high',
      toolName: 'projects.readFile',
      command: filePath,
      detail: scopeErr,
      wasAllowed: false,
      matchedPattern: 'project_scope',
    }).catch(() => {});
    showToast('Access blocked: file is outside the active project scope', 'error');
    return null;
  }

  try {
    return await _readTextFile(filePath);
  } catch (e) {
    console.warn('[projects] Failed to read file:', filePath, e);
    return null;
  }
}

// ── Sidebar rendering ─────────────────────────────────────────────────────

export function renderProjectsSidebar(): void {
  const sidebar = $('projects-sidebar-list');
  if (!sidebar) return;

  if (_projects.length === 0) {
    sidebar.innerHTML = `<div class="projects-sidebar-empty">No projects added yet.<br>Click <strong>Add Folder</strong> to browse a local project.</div>`;
    return;
  }

  sidebar.innerHTML = _projects
    .map((p) => {
      const cached = getCachedGitInfo(p.path);
      const branchHint =
        cached?.isRepo && cached.branch
          ? `<span style="font-size:10px;color:var(--accent);font-family:var(--font-mono);opacity:0.8">${escHtml(cached.branch)}</span>`
          : '';
      const dirtyDot =
        cached?.isRepo && cached.dirty
          ? '<span style="color:var(--warning);font-size:8px;margin-left:2px">●</span>'
          : '';
      const isActive =
        _selectedFile &&
        _projects.some((proj) => proj.path === p.path && _selectedFile!.path.startsWith(proj.path));
      return `
    <div class="projects-folder-item${isActive ? ' active' : ''}" 
         data-path="${escAttr(p.path)}" title="${escAttr(p.path)}">
      <div class="projects-folder-row">
        <span class="ms ms-sm">folder</span>
        <span class="projects-folder-name">${escHtml(p.name)}</span>
        ${dirtyDot}
        <button class="btn-icon projects-remove-btn" data-remove="${escAttr(p.path)}" title="Remove project">
          <span class="ms" style="font-size:14px">close</span>
        </button>
      </div>
      <div class="projects-folder-path">${escHtml(shortenPath(p.path))}${branchHint ? ` · ${branchHint}` : ''}</div>
    </div>`;
    })
    .join('');

  // Bind clicks
  sidebar.querySelectorAll('.projects-folder-item').forEach((el) => {
    el.addEventListener('click', async (e) => {
      if ((e.target as HTMLElement).closest('.projects-remove-btn')) return;
      const path = el.getAttribute('data-path');
      const project = _projects.find((p) => p.path === path);
      if (project) await selectProject(project);
    });
  });

  sidebar.querySelectorAll('.projects-remove-btn').forEach((btn) => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const path = btn.getAttribute('data-remove');
      if (
        path &&
        (await confirmModal(
          `Remove "${_projects.find((p) => p.path === path)?.name}" from projects?`,
        ))
      ) {
        _onRemoveProject?.(path);
      }
    });
  });
}

// ── Project selection ─────────────────────────────────────────────────────

export async function selectProject(project: ProjectFolder): Promise<void> {
  // C1: Set per-project scope
  _activeProjectRoot = project.path;

  const treeContainer = $('projects-file-tree');
  const viewer = $('projects-file-viewer');
  const empty = $('projects-empty');

  if (empty) empty.style.display = 'none';
  if (treeContainer) treeContainer.style.display = '';
  if (viewer) viewer.style.display = '';

  // Highlight in sidebar
  document.querySelectorAll('.projects-folder-item').forEach((el) => {
    el.classList.toggle('active', el.getAttribute('data-path') === project.path);
  });

  // Load and render file tree
  const entries = await loadDirectoryContents(project.path);
  _expandedPaths.clear();
  _expandedPaths.add(project.path);

  if (treeContainer) {
    treeContainer.innerHTML = `<div class="tree-root" data-path="${escAttr(project.path)}">
      ${renderTreeEntries(entries, 0, project.path)}
    </div>`;
    bindTreeEvents(treeContainer, project.path);
  }

  // Gather git info (async, non-blocking for file tree)
  const gitInfo = await getGitInfo(project.path);

  // Show welcome + git info in viewer
  if (viewer) {
    const dirCount = entries.filter((e) => e.isDirectory).length;
    const fileCount = entries.filter((e) => !e.isDirectory).length;

    viewer.innerHTML = `
      <div class="projects-viewer-welcome">
        <div class="projects-viewer-welcome-icon">
          <span class="ms" style="font-size:48px">folder</span>
        </div>
        <div class="projects-viewer-welcome-title">${escHtml(project.name)}</div>
        <div class="projects-viewer-welcome-sub">${dirCount} folders, ${fileCount} files</div>
        <div class="projects-viewer-welcome-path">${escHtml(project.path)}</div>
      </div>
      ${renderGitBanner(gitInfo, project.path)}
    `;
    bindGitActions(viewer, project.path, async (path) => {
      const proj = _projects.find((p) => p.path === path);
      if (proj) await selectProject(proj);
    });
  }

  _selectedFile = null;
}

// ── File tree rendering ───────────────────────────────────────────────────

function renderTreeEntries(entries: FileEntry[], depth: number, rootPath: string): string {
  return entries
    .map((entry) => {
      const indent = depth * 16;
      const isExpanded = _expandedPaths.has(entry.path);

      if (entry.isDirectory) {
        const childrenHtml =
          isExpanded && entry.children
            ? renderTreeEntries(entry.children, depth + 1, rootPath)
            : '';
        return `
        <div class="tree-item tree-dir${isExpanded ? ' expanded' : ''}" data-path="${escAttr(entry.path)}" style="padding-left:${indent + 8}px">
          <span class="ms tree-chevron" style="font-size:14px">chevron_right</span>
          <span class="ms ms-sm tree-icon">${isExpanded ? 'folder_open' : 'folder'}</span>
          <span class="tree-name">${escHtml(entry.name)}</span>
        </div>
        <div class="tree-children${isExpanded ? ' expanded' : ''}" data-parent="${escAttr(entry.path)}">
          ${childrenHtml}
        </div>`;
      } else {
        const ext = entry.name.split('.').pop()?.toLowerCase() || '';
        const iconName = getFileIcon(ext);
        return `
        <div class="tree-item tree-file${_selectedFile?.path === entry.path ? ' active' : ''}" data-path="${escAttr(entry.path)}" style="padding-left:${indent + 22}px">
          <span class="ms ms-sm tree-icon">${iconName}</span>
          <span class="tree-name">${escHtml(entry.name)}</span>
          <span class="tree-ext">${ext ? `.${ext}` : ''}</span>
        </div>`;
      }
    })
    .join('');
}

function bindTreeEvents(container: HTMLElement, rootPath: string): void {
  container.querySelectorAll('.tree-dir').forEach((el) => {
    el.addEventListener('click', async () => {
      const dirPath = el.getAttribute('data-path');
      if (!dirPath) return;

      const isExpanded = _expandedPaths.has(dirPath);
      if (isExpanded) {
        _expandedPaths.delete(dirPath);
        el.classList.remove('expanded');
        const children = container.querySelector(
          `.tree-children[data-parent="${CSS.escape(dirPath)}"]`,
        );
        if (children) {
          children.classList.remove('expanded');
          children.innerHTML = '';
        }
      } else {
        _expandedPaths.add(dirPath);
        el.classList.add('expanded');
        const entries = await loadDirectoryContents(dirPath);
        const children = container.querySelector(
          `.tree-children[data-parent="${CSS.escape(dirPath)}"]`,
        );
        if (children) {
          children.classList.add('expanded');
          children.innerHTML = renderTreeEntries(entries, getDepth(dirPath, rootPath), rootPath);
          bindTreeEvents(children as HTMLElement, rootPath);
        }
        // Update folder icon
        const iconSvg = el.querySelector('.tree-icon');
        if (iconSvg) {
          iconSvg.innerHTML =
            '<path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/><line x1="2" y1="10" x2="22" y2="10"/>';
        }
      }
    });
  });

  container.querySelectorAll('.tree-file').forEach((el) => {
    el.addEventListener('click', async () => {
      const filePath = el.getAttribute('data-path');
      if (!filePath) return;

      // Highlight selection
      container
        .closest('#projects-file-tree')
        ?.querySelectorAll('.tree-file.active')
        .forEach((f) => f.classList.remove('active'));
      el.classList.add('active');

      const fileName = filePath.split('/').pop() || filePath;
      _selectedFile = { name: fileName, path: filePath, isDirectory: false };
      await openFile(filePath);
    });
  });
}

// ── File viewer ───────────────────────────────────────────────────────────

async function openFile(filePath: string): Promise<void> {
  const viewer = $('projects-file-viewer');
  if (!viewer) return;

  const fileName = filePath.split('/').pop() || filePath;
  const ext = fileName.split('.').pop()?.toLowerCase() || '';

  if (BINARY_EXTENSIONS.includes(ext)) {
    viewer.innerHTML = `
      <div class="projects-viewer-header">
        <span class="projects-viewer-filename">${escHtml(fileName)}</span>
        <span class="projects-viewer-path">${escHtml(shortenPath(filePath))}</span>
      </div>
      <div class="projects-viewer-binary">
        <span class="ms" style="font-size:32px">insert_drive_file</span>
        <span>Binary file — cannot preview</span>
        <span class="projects-viewer-ext">.${ext}</span>
      </div>`;
    return;
  }

  // Show loading state
  viewer.innerHTML = `
    <div class="projects-viewer-header">
      <span class="projects-viewer-filename">${escHtml(fileName)}</span>
      <span class="projects-viewer-path">${escHtml(shortenPath(filePath))}</span>
    </div>
    <div class="projects-viewer-loading">Loading...</div>`;

  const content = await loadFileContent(filePath);
  if (content === null) {
    viewer.innerHTML = `
      <div class="projects-viewer-header">
        <span class="projects-viewer-filename">${escHtml(fileName)}</span>
      </div>
      <div class="projects-viewer-binary">
        <span>Could not read file</span>
      </div>`;
    return;
  }

  const lang = getLanguageClass(ext);
  const lines = content.split('\n');
  const lineNumbers = lines.map((_, i) => `<span class="line-num">${i + 1}</span>`).join('\n');
  const maxSize = 500_000;
  const displayContent =
    content.length > maxSize
      ? `${content.slice(0, maxSize)}\n\n... (truncated — ${(content.length / 1024).toFixed(0)}KB total)`
      : content;

  viewer.innerHTML = `
    <div class="projects-viewer-header">
      <span class="projects-viewer-filename">${escHtml(fileName)}</span>
      <span class="projects-viewer-path">${escHtml(shortenPath(filePath))}</span>
      <span class="projects-viewer-lines">${lines.length} lines</span>
    </div>
    <div class="projects-viewer-code">
      <div class="projects-line-numbers">${lineNumbers}</div>
      <pre class="projects-code-content"><code class="${lang}">${escHtml(displayContent)}</code></pre>
    </div>`;
}

// ── Empty state ───────────────────────────────────────────────────────────

export function showProjectsEmpty(): void {
  const treeContainer = $('projects-file-tree');
  const viewer = $('projects-file-viewer');
  const empty = $('projects-empty');

  if (treeContainer) treeContainer.style.display = 'none';
  if (viewer) viewer.style.display = 'none';
  if (empty) {
    empty.style.display = '';
    empty.innerHTML = `
      <div class="empty-icon">
        <span class="ms" style="font-size:48px">folder</span>
      </div>
      <div class="empty-title">Your Projects</div>
      <div class="empty-subtitle" style="max-width:340px;text-align:center;line-height:1.6">
        Add a local folder to browse files, view git status, and give your agent
        project context. Works with any git repo or plain folder.
      </div>
      <div style="margin-top:16px;padding:12px 16px;border-radius:8px;background:var(--surface-2, rgba(255,255,255,0.04));font-size:12px;color:var(--text-muted);line-height:1.8;text-align:left;max-width:360px">
        <div style="font-weight:600;margin-bottom:4px;color:var(--text)">What Projects does:</div>
        <div><span class="ms ms-sm">folder</span> <strong>Browse</strong> — file tree with code viewer</div>
        <div><span class="ms ms-sm">commit</span> <strong>Git</strong> — branch, status, pull, push, commit</div>
        <div><span class="ms ms-sm">smart_toy</span> <strong>Agent context</strong> — your agent can read, edit, and run commands in project folders</div>
        <div><span class="ms ms-sm">lock</span> <strong>Scoped</strong> — agent access is confined to the project you select</div>
      </div>
    `;
  }
}
