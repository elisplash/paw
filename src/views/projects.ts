// Projects View — Browse local project folders as a file tree
// Uses Tauri filesystem APIs or falls back to a placeholder in browser

import { showToast } from '../components/toast';
import { logSecurityEvent } from '../db';

const $ = (id: string) => document.getElementById(id);

// ── Types ──────────────────────────────────────────────────────────────────

interface FileEntry {
  name: string;
  path: string;
  isDirectory: boolean;
  children?: FileEntry[];
  expanded?: boolean;
  size?: number;
  modified?: number;
}

interface ProjectFolder {
  name: string;
  path: string;
  addedAt: string;
}

// ── Module State ───────────────────────────────────────────────────────────

let _projects: ProjectFolder[] = [];
let _selectedFile: FileEntry | null = null;
let _fileTreeCache = new Map<string, FileEntry[]>(); // path → children
let _expandedPaths = new Set<string>();
let _tauriAvailable = false;
let _readDir: any = null;
let _readTextFile: any = null;
let _homeDir: any = null;
let _join: any = null;

// ── Tauri Detection ────────────────────────────────────────────────────────

async function initTauri(): Promise<boolean> {
  try {
    const fs = await import('@tauri-apps/plugin-fs');
    const path = await import('@tauri-apps/api/path');
    _readDir = fs.readDir;
    _readTextFile = fs.readTextFile;
    _homeDir = path.homeDir;
    _join = path.join;
    _tauriAvailable = true;
    return true;
  } catch {
    _tauriAvailable = false;
    return false;
  }
}

// ── Persistence ────────────────────────────────────────────────────────────

function loadSavedProjects(): ProjectFolder[] {
  try {
    const raw = localStorage.getItem('paw-project-folders');
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

function savePersistProjects(): void {
  localStorage.setItem('paw-project-folders', JSON.stringify(_projects));
}

// ── B4: Sensitive Path Blocking ─────────────────────────────────────────────

/** Paths that should never be added as project folders or browsed into */
const SENSITIVE_PATH_PATTERNS: Array<{ pattern: RegExp; label: string }> = [
  { pattern: /[/\\]\.ssh(\/|\\|$)/i,           label: 'SSH keys directory' },
  { pattern: /[/\\]\.gnupg(\/|\\|$)/i,         label: 'GPG keyring directory' },
  { pattern: /[/\\]\.aws(\/|\\|$)/i,           label: 'AWS credentials directory' },
  { pattern: /[/\\]\.kube(\/|\\|$)/i,          label: 'Kubernetes config directory' },
  { pattern: /[/\\]\.docker(\/|\\|$)/i,        label: 'Docker config directory' },
  { pattern: /[/\\]\.gnome-keyring(\/|\\|$)/i, label: 'GNOME keyring directory' },
  { pattern: /[/\\]\.password-store(\/|\\|$)/i,label: 'Password store directory' },
  { pattern: /[/\\]\.netrc$/i,                 label: 'netrc credentials file' },
  { pattern: /^\/etc(\/|$)/i,                  label: '/etc system config' },
  { pattern: /^\/root(\/|$)/i,                 label: '/root home directory' },
  { pattern: /^\/var\/log(\/|$)/i,             label: 'System logs directory' },
  { pattern: /^\/proc(\/|$)/i,                 label: 'proc filesystem' },
  { pattern: /^\/sys(\/|$)/i,                  label: 'sys filesystem' },
  { pattern: /^\/dev(\/|$)/i,                  label: 'Device filesystem' },
  { pattern: /^C:\\\\Windows(\\\\|$)/i,        label: 'Windows system directory' },
  { pattern: /^C:\\\\Users\\\\[^\\\\]+\\\\AppData(\\\\|$)/i, label: 'AppData directory' },
  { pattern: /[/\\]\.openclaw(\/|\\|$)/i,      label: 'OpenClaw config (contains tokens)' },
  { pattern: /[/\\]\.config[/\\]himalaya(\/|\\|$)/i, label: 'Himalaya email config' },
];

/**
 * Check if a path matches any known sensitive location.
 * Returns the label of the matched pattern, or null if safe.
 */
function isSensitivePath(pathStr: string): string | null {
  const normalized = pathStr.replace(/\\/g, '/');
  for (const { pattern, label } of SENSITIVE_PATH_PATTERNS) {
    if (pattern.test(normalized)) return label;
  }
  // Block home directory root itself (e.g. /home/user or /Users/user)
  if (/^(\/home\/[^/]+|\/Users\/[^/]+|~)$/.test(normalized)) {
    return 'Home directory root (too broad)';
  }
  // Block filesystem root
  if (normalized === '/' || normalized === 'C:\\' || normalized === 'C:/') {
    return 'Filesystem root';
  }
  return null;
}

// ── Public API ─────────────────────────────────────────────────────────────

export async function loadProjects(): Promise<void> {
  await initTauri();
  _projects = loadSavedProjects();
  renderProjectsSidebar();

  if (_projects.length === 0) {
    showProjectsEmpty();
  } else {
    // Auto-select first project
    await selectProject(_projects[0]);
  }
}

export async function addProjectFolder(path: string, name?: string): Promise<void> {
  // B4: Block sensitive directories
  const blocked = isSensitivePath(path);
  if (blocked) {
    showToast(`Blocked: "${path}" is a sensitive system directory (${blocked}). Cannot add as a project.`, 'error');
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
  if (_projects.some(p => p.path === path)) {
    showProjectsToast('Project already added', 'warning');
    return;
  }

  const folderName = name || path.split('/').pop() || path.split('\\').pop() || path;
  const project: ProjectFolder = {
    name: folderName,
    path,
    addedAt: new Date().toISOString(),
  };
  _projects.push(project);
  savePersistProjects();
  renderProjectsSidebar();
  await selectProject(project);
}

export async function removeProject(path: string): Promise<void> {
  _projects = _projects.filter(p => p.path !== path);
  _fileTreeCache.delete(path);
  savePersistProjects();
  renderProjectsSidebar();

  if (_projects.length > 0) {
    await selectProject(_projects[0]);
  } else {
    showProjectsEmpty();
  }
}

// ── File Tree Loading ──────────────────────────────────────────────────────

async function loadDirectoryContents(dirPath: string): Promise<FileEntry[]> {
  if (!_tauriAvailable || !_readDir) return [];

  try {
    const entries = await _readDir(dirPath);
    const result: FileEntry[] = [];

    for (const entry of entries) {
      if (!entry.name) continue;
      // Skip hidden files and common non-essential directories
      if (entry.name.startsWith('.') && entry.name !== '.env') continue;
      if (['node_modules', '__pycache__', '.git', 'target', 'dist', 'build', '.next', 'venv', '.venv'].includes(entry.name)) continue;

      const fullPath = await _join(dirPath, entry.name);

      // B4: Skip sensitive directories in file tree
      if (entry.isDirectory && isSensitivePath(fullPath)) continue;

      result.push({
        name: entry.name,
        path: fullPath,
        isDirectory: entry.isDirectory || false,
      });
    }

    // Sort: directories first, then alphabetical
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

  try {
    return await _readTextFile(filePath);
  } catch (e) {
    console.warn('[projects] Failed to read file:', filePath, e);
    return null;
  }
}

// ── Rendering ──────────────────────────────────────────────────────────────

function renderProjectsSidebar(): void {
  const sidebar = $('projects-sidebar-list');
  if (!sidebar) return;

  if (_projects.length === 0) {
    sidebar.innerHTML = `<div class="projects-sidebar-empty">No projects added yet.<br>Click <strong>Add Folder</strong> to browse a local project.</div>`;
    return;
  }

  sidebar.innerHTML = _projects.map(p => `
    <div class="projects-folder-item${_selectedFile && getProjectRoot(_selectedFile.path) === p.path ? ' active' : ''}" 
         data-path="${escapeAttr(p.path)}" title="${escapeAttr(p.path)}">
      <div class="projects-folder-row">
        <svg class="icon-sm" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
        </svg>
        <span class="projects-folder-name">${escapeHtml(p.name)}</span>
        <button class="btn-icon projects-remove-btn" data-remove="${escapeAttr(p.path)}" title="Remove project">
          <svg class="icon-xs" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
          </svg>
        </button>
      </div>
      <div class="projects-folder-path">${escapeHtml(shortenPath(p.path))}</div>
    </div>
  `).join('');

  // Bind clicks
  sidebar.querySelectorAll('.projects-folder-item').forEach(el => {
    el.addEventListener('click', async (e) => {
      if ((e.target as HTMLElement).closest('.projects-remove-btn')) return;
      const path = el.getAttribute('data-path');
      const project = _projects.find(p => p.path === path);
      if (project) await selectProject(project);
    });
  });

  sidebar.querySelectorAll('.projects-remove-btn').forEach(btn => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const path = btn.getAttribute('data-remove');
      if (path && confirm(`Remove "${_projects.find(p => p.path === path)?.name}" from projects?`)) {
        removeProject(path);
      }
    });
  });
}

async function selectProject(project: ProjectFolder): Promise<void> {
  const treeContainer = $('projects-file-tree');
  const viewer = $('projects-file-viewer');
  const empty = $('projects-empty');

  if (empty) empty.style.display = 'none';
  if (treeContainer) treeContainer.style.display = '';
  if (viewer) viewer.style.display = '';

  // Highlight in sidebar
  document.querySelectorAll('.projects-folder-item').forEach(el => {
    el.classList.toggle('active', el.getAttribute('data-path') === project.path);
  });

  // Load and render file tree
  const entries = await loadDirectoryContents(project.path);
  _expandedPaths.clear();
  _expandedPaths.add(project.path);
  
  if (treeContainer) {
    treeContainer.innerHTML = `<div class="tree-root" data-path="${escapeAttr(project.path)}">
      ${renderTreeEntries(entries, 0)}
    </div>`;
    bindTreeEvents(treeContainer);
  }

  // Show welcome in viewer
  if (viewer) {
    viewer.innerHTML = `
      <div class="projects-viewer-welcome">
        <div class="projects-viewer-welcome-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="48" height="48">
            <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
          </svg>
        </div>
        <div class="projects-viewer-welcome-title">${escapeHtml(project.name)}</div>
        <div class="projects-viewer-welcome-sub">${entries.filter(e => e.isDirectory).length} folders, ${entries.filter(e => !e.isDirectory).length} files</div>
        <div class="projects-viewer-welcome-path">${escapeHtml(project.path)}</div>
      </div>`;
  }

  _selectedFile = null;
}

function renderTreeEntries(entries: FileEntry[], depth: number): string {
  return entries.map(entry => {
    const indent = depth * 16;
    const isExpanded = _expandedPaths.has(entry.path);

    if (entry.isDirectory) {
      const childrenHtml = isExpanded && entry.children
        ? renderTreeEntries(entry.children, depth + 1) : '';
      return `
        <div class="tree-item tree-dir${isExpanded ? ' expanded' : ''}" data-path="${escapeAttr(entry.path)}" style="padding-left:${indent + 8}px">
          <svg class="tree-chevron" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="14" height="14">
            <polyline points="9 18 15 12 9 6"/>
          </svg>
          <svg class="tree-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16">
            ${isExpanded
              ? '<path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/><line x1="2" y1="10" x2="22" y2="10"/>'
              : '<path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>'}
          </svg>
          <span class="tree-name">${escapeHtml(entry.name)}</span>
        </div>
        <div class="tree-children${isExpanded ? ' expanded' : ''}" data-parent="${escapeAttr(entry.path)}">
          ${childrenHtml}
        </div>`;
    } else {
      const ext = entry.name.split('.').pop()?.toLowerCase() || '';
      const icon = getFileIcon(ext);
      return `
        <div class="tree-item tree-file${_selectedFile?.path === entry.path ? ' active' : ''}" data-path="${escapeAttr(entry.path)}" style="padding-left:${indent + 22}px">
          <svg class="tree-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16">
            ${icon}
          </svg>
          <span class="tree-name">${escapeHtml(entry.name)}</span>
          <span class="tree-ext">${ext ? `.${ext}` : ''}</span>
        </div>`;
    }
  }).join('');
}

function bindTreeEvents(container: HTMLElement): void {
  container.querySelectorAll('.tree-dir').forEach(el => {
    el.addEventListener('click', async () => {
      const dirPath = el.getAttribute('data-path');
      if (!dirPath) return;

      const isExpanded = _expandedPaths.has(dirPath);
      if (isExpanded) {
        _expandedPaths.delete(dirPath);
        el.classList.remove('expanded');
        const children = container.querySelector(`.tree-children[data-parent="${CSS.escape(dirPath)}"]`);
        if (children) {
          children.classList.remove('expanded');
          children.innerHTML = '';
        }
      } else {
        _expandedPaths.add(dirPath);
        el.classList.add('expanded');
        const entries = await loadDirectoryContents(dirPath);
        const children = container.querySelector(`.tree-children[data-parent="${CSS.escape(dirPath)}"]`);
        if (children) {
          children.classList.add('expanded');
          children.innerHTML = renderTreeEntries(entries, getDepth(dirPath));
          bindTreeEvents(children as HTMLElement);
        }
        // Update folder icon
        const iconSvg = el.querySelector('.tree-icon');
        if (iconSvg) {
          iconSvg.innerHTML = '<path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/><line x1="2" y1="10" x2="22" y2="10"/>';
        }
      }
    });
  });

  container.querySelectorAll('.tree-file').forEach(el => {
    el.addEventListener('click', async () => {
      const filePath = el.getAttribute('data-path');
      if (!filePath) return;

      // Highlight selection
      container.closest('#projects-file-tree')?.querySelectorAll('.tree-file.active').forEach(f => f.classList.remove('active'));
      el.classList.add('active');

      const fileName = filePath.split('/').pop() || filePath;
      _selectedFile = { name: fileName, path: filePath, isDirectory: false };
      await openFile(filePath);
    });
  });
}

async function openFile(filePath: string): Promise<void> {
  const viewer = $('projects-file-viewer');
  if (!viewer) return;

  const fileName = filePath.split('/').pop() || filePath;
  const ext = fileName.split('.').pop()?.toLowerCase() || '';

  // Check if it's a binary/non-text file
  const binaryExts = ['png', 'jpg', 'jpeg', 'gif', 'svg', 'ico', 'bmp', 'webp',
    'woff', 'woff2', 'ttf', 'eot', 'otf', 'mp3', 'mp4', 'wav', 'ogg', 'webm',
    'zip', 'tar', 'gz', 'rar', 'pdf', 'exe', 'dll', 'so', 'dylib', 'o', 'class',
    'pyc', 'wasm', 'db', 'sqlite', 'lock'];

  if (binaryExts.includes(ext)) {
    viewer.innerHTML = `
      <div class="projects-viewer-header">
        <span class="projects-viewer-filename">${escapeHtml(fileName)}</span>
        <span class="projects-viewer-path">${escapeHtml(shortenPath(filePath))}</span>
      </div>
      <div class="projects-viewer-binary">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="32" height="32">
          <path d="M13 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"/>
          <polyline points="13 2 13 9 20 9"/>
        </svg>
        <span>Binary file — cannot preview</span>
        <span class="projects-viewer-ext">.${ext}</span>
      </div>`;
    return;
  }

  // Show loading state
  viewer.innerHTML = `
    <div class="projects-viewer-header">
      <span class="projects-viewer-filename">${escapeHtml(fileName)}</span>
      <span class="projects-viewer-path">${escapeHtml(shortenPath(filePath))}</span>
    </div>
    <div class="projects-viewer-loading">Loading...</div>`;

  const content = await loadFileContent(filePath);
  if (content === null) {
    viewer.innerHTML = `
      <div class="projects-viewer-header">
        <span class="projects-viewer-filename">${escapeHtml(fileName)}</span>
      </div>
      <div class="projects-viewer-binary">
        <span>Could not read file</span>
      </div>`;
    return;
  }

  const lang = getLanguageClass(ext);
  const lines = content.split('\n');
  const lineNumbers = lines.map((_, i) => `<span class="line-num">${i + 1}</span>`).join('\n');
  const maxSize = 500_000; // 500KB max display
  const displayContent = content.length > maxSize
    ? content.slice(0, maxSize) + `\n\n... (truncated — ${(content.length / 1024).toFixed(0)}KB total)`
    : content;

  viewer.innerHTML = `
    <div class="projects-viewer-header">
      <span class="projects-viewer-filename">${escapeHtml(fileName)}</span>
      <span class="projects-viewer-path">${escapeHtml(shortenPath(filePath))}</span>
      <span class="projects-viewer-lines">${lines.length} lines</span>
    </div>
    <div class="projects-viewer-code">
      <div class="projects-line-numbers">${lineNumbers}</div>
      <pre class="projects-code-content"><code class="${lang}">${escapeHtml(displayContent)}</code></pre>
    </div>`;
}

function showProjectsEmpty(): void {
  const treeContainer = $('projects-file-tree');
  const viewer = $('projects-file-viewer');
  const empty = $('projects-empty');

  if (treeContainer) treeContainer.style.display = 'none';
  if (viewer) viewer.style.display = 'none';
  if (empty) empty.style.display = '';
}

// ── Add Folder Dialog ──────────────────────────────────────────────────────

export async function promptAddFolder(): Promise<void> {
  if (_tauriAvailable) {
    try {
      // Try Tauri dialog via runtime API (avoids static import of plugin-dialog)
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

  // Fallback: text prompt
  const path = prompt('Enter the full path to your project folder:');
  if (path && path.trim()) {
    await addProjectFolder(path.trim());
  }
}

// Also support adding workspace default path
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

// ── Event Binding ──────────────────────────────────────────────────────────

export function bindEvents(): void {
  const addBtn = $('projects-add-folder');
  const refreshBtn = $('projects-refresh-btn');

  addBtn?.addEventListener('click', () => promptAddFolder());

  refreshBtn?.addEventListener('click', async () => {
    _fileTreeCache.clear();
    const activePath = document.querySelector('.projects-folder-item.active')?.getAttribute('data-path');
    const project = _projects.find(p => p.path === activePath);
    if (project) {
      await selectProject(project);
    }
    showProjectsToast('Refreshed', 'success');
  });
}

// ── Helpers ────────────────────────────────────────────────────────────────

function getDepth(path: string): number {
  const root = document.querySelector('.tree-root')?.getAttribute('data-path');
  if (!root) return 0;
  const rootParts = root.split('/').length;
  const pathParts = path.split('/').length;
  return pathParts - rootParts;
}

function getProjectRoot(filePath: string): string | null {
  for (const p of _projects) {
    if (filePath.startsWith(p.path)) return p.path;
  }
  return null;
}

function getFileIcon(ext: string): string {
  const icons: Record<string, string> = {
    // Code files
    ts: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><text x="8" y="18" font-size="7" fill="currentColor" stroke="none" font-weight="bold">TS</text>',
    tsx: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><text x="7" y="18" font-size="6" fill="currentColor" stroke="none" font-weight="bold">TSX</text>',
    js: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><text x="9" y="18" font-size="7" fill="currentColor" stroke="none" font-weight="bold">JS</text>',
    jsx: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><text x="7" y="18" font-size="6" fill="currentColor" stroke="none" font-weight="bold">JSX</text>',
    py: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><text x="8" y="18" font-size="7" fill="currentColor" stroke="none" font-weight="bold">PY</text>',
    rs: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><text x="8" y="18" font-size="7" fill="currentColor" stroke="none" font-weight="bold">RS</text>',
    go: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><text x="8" y="18" font-size="7" fill="currentColor" stroke="none" font-weight="bold">GO</text>',
    // Config/data
    json: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><path d="M8 16s1-2 4-2 4 2 4 2"/>',
    yaml: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/>',
    yml: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/>',
    toml: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/>',
    // Markup
    md: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="8" y1="13" x2="16" y2="13"/><line x1="8" y1="17" x2="16" y2="17"/>',
    html: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><polyline points="8 13 10 16 8 19"/><line x1="12" y1="19" x2="16" y2="19"/>',
    css: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><text x="7" y="18" font-size="6" fill="currentColor" stroke="none" font-weight="bold">CSS</text>',
  };

  return icons[ext] || '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/>';
}

function getLanguageClass(ext: string): string {
  const langMap: Record<string, string> = {
    ts: 'language-typescript', tsx: 'language-typescript',
    js: 'language-javascript', jsx: 'language-javascript',
    py: 'language-python', rs: 'language-rust',
    go: 'language-go', rb: 'language-ruby',
    html: 'language-html', css: 'language-css',
    json: 'language-json', yaml: 'language-yaml', yml: 'language-yaml',
    toml: 'language-toml', md: 'language-markdown',
    sh: 'language-bash', bash: 'language-bash', zsh: 'language-bash',
    sql: 'language-sql', xml: 'language-xml', svg: 'language-xml',
    c: 'language-c', cpp: 'language-cpp', h: 'language-c',
    java: 'language-java', kt: 'language-kotlin', swift: 'language-swift',
  };
  return langMap[ext] || 'language-plaintext';
}

function shortenPath(path: string): string {
  // Replace home dir with ~
  return path.replace(/^\/Users\/[^/]+/, '~').replace(/^\/home\/[^/]+/, '~');
}

function escapeHtml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function escapeAttr(str: string): string {
  return str.replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

function showProjectsToast(msg: string, type: 'info' | 'success' | 'error' | 'warning' = 'info'): void {
  showToast(msg, type);
}
