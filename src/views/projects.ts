// Projects View — Browse local project folders as a file tree
// Uses Tauri filesystem APIs + shell plugin for git integration

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

interface GitInfo {
  isRepo: boolean;
  branch?: string;
  remote?: string;
  dirty?: number;     // count of changed files
  ahead?: number;     // commits ahead of remote
  behind?: number;    // commits behind remote
  lastCommit?: string; // short message
  lastCommitDate?: string;
}

// ── Module State ───────────────────────────────────────────────────────────

let _projects: ProjectFolder[] = [];
let _selectedFile: FileEntry | null = null;
let _fileTreeCache = new Map<string, FileEntry[]>(); // path → children
let _expandedPaths = new Set<string>();
let _tauriAvailable = false;
let _shellAvailable = false;
let _readDir: any = null;
let _readTextFile: any = null;
let _homeDir: any = null;
let _join: any = null;
let _shellCommand: any = null; // Command class from @tauri-apps/plugin-shell
let _gitInfoCache = new Map<string, GitInfo>(); // path → git info

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

async function initShell(): Promise<boolean> {
  try {
    const shell = await import('@tauri-apps/plugin-shell');
    _shellCommand = shell.Command;
    _shellAvailable = true;
    return true;
  } catch {
    _shellAvailable = false;
    return false;
  }
}

// ── Git helpers ────────────────────────────────────────────────────────────

/** Run a git command in a directory and return stdout (or null on error). */
async function gitExec(cwd: string, ...args: string[]): Promise<string | null> {
  if (!_shellAvailable || !_shellCommand) return null;
  try {
    const cmd = _shellCommand.create('git', args, { cwd });
    const result = await cmd.execute();
    if (result.code !== 0) return null;
    return (result.stdout as string).trim();
  } catch {
    return null;
  }
}

/** Gather git info for a project path. Cached until invalidated. */
async function getGitInfo(projectPath: string, forceRefresh = false): Promise<GitInfo> {
  if (!forceRefresh && _gitInfoCache.has(projectPath)) {
    return _gitInfoCache.get(projectPath)!;
  }

  const noGit: GitInfo = { isRepo: false };

  // Check if it's a git repo
  const topLevel = await gitExec(projectPath, 'rev-parse', '--show-toplevel');
  if (!topLevel) {
    _gitInfoCache.set(projectPath, noGit);
    return noGit;
  }

  const info: GitInfo = { isRepo: true };

  // Branch
  info.branch = await gitExec(projectPath, 'rev-parse', '--abbrev-ref', 'HEAD') ?? undefined;

  // Remote URL
  info.remote = await gitExec(projectPath, 'config', '--get', 'remote.origin.url') ?? undefined;

  // Dirty file count
  const statusOut = await gitExec(projectPath, 'status', '--porcelain');
  if (statusOut !== null) {
    info.dirty = statusOut === '' ? 0 : statusOut.split('\n').filter(l => l.trim()).length;
  }

  // Ahead/behind (only if upstream is set)
  const upstream = await gitExec(projectPath, 'rev-parse', '--abbrev-ref', '@{upstream}');
  if (upstream) {
    const abOut = await gitExec(projectPath, 'rev-list', '--left-right', '--count', `HEAD...@{upstream}`);
    if (abOut) {
      const [ahead, behind] = abOut.split(/\s+/).map(Number);
      info.ahead = ahead || 0;
      info.behind = behind || 0;
    }
  }

  // Last commit
  const logOut = await gitExec(projectPath, 'log', '-1', '--format=%s|||%ar');
  if (logOut) {
    const [msg, date] = logOut.split('|||');
    info.lastCommit = msg;
    info.lastCommitDate = date;
  }

  _gitInfoCache.set(projectPath, info);
  return info;
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

// ── Per-project scope guard (C1) ───────────────────────────────────────────
let _activeProjectRoot: string | null = null;

/**
 * Validate that a file path is within the active project scope.
 * Prevents directory traversal and access to paths outside the project root.
 * Returns null if valid, or an error string if out-of-scope.
 */
function isOutOfProjectScope(filePath: string): string | null {
  if (!_activeProjectRoot) return null; // no project selected — no scope to enforce
  const normFile = filePath.replace(/\\/g, '/').replace(/\/+$/, '');
  const normRoot = _activeProjectRoot.replace(/\\/g, '/').replace(/\/+$/, '');

  // Must start with project root
  if (!normFile.startsWith(normRoot + '/') && normFile !== normRoot) {
    return `Path "${filePath}" is outside the active project scope "${_activeProjectRoot}"`;
  }

  // Block traversal sequences even within the path
  if (/\/\.\.\//g.test(normFile) || normFile.endsWith('/..')) {
    return `Path contains directory traversal: "${filePath}"`;
  }

  return null;
}

// ── Public API ─────────────────────────────────────────────────────────────

export async function loadProjects(): Promise<void> {
  await initTauri();
  await initShell();
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

  // C1: Enforce per-project scope
  const scopeErr = isOutOfProjectScope(dirPath);
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

  // C1: Enforce per-project scope
  const scopeErr = isOutOfProjectScope(filePath);
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

// ── Rendering ──────────────────────────────────────────────────────────────

function renderProjectsSidebar(): void {
  const sidebar = $('projects-sidebar-list');
  if (!sidebar) return;

  if (_projects.length === 0) {
    sidebar.innerHTML = `<div class="projects-sidebar-empty">No projects added yet.<br>Click <strong>Add Folder</strong> to browse a local project.</div>`;
    return;
  }

  sidebar.innerHTML = _projects.map(p => {
    const cached = _gitInfoCache.get(p.path);
    const branchHint = cached?.isRepo && cached.branch
      ? `<span style="font-size:10px;color:var(--accent);font-family:var(--font-mono);opacity:0.8">${escapeHtml(cached.branch)}</span>`
      : '';
    const dirtyDot = cached?.isRepo && cached.dirty
      ? '<span style="color:var(--warning, #facc15);font-size:8px;margin-left:2px">●</span>'
      : '';
    return `
    <div class="projects-folder-item${_selectedFile && getProjectRoot(_selectedFile.path) === p.path ? ' active' : ''}" 
         data-path="${escapeAttr(p.path)}" title="${escapeAttr(p.path)}">
      <div class="projects-folder-row">
        <svg class="icon-sm" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
        </svg>
        <span class="projects-folder-name">${escapeHtml(p.name)}</span>
        ${dirtyDot}
        <button class="btn-icon projects-remove-btn" data-remove="${escapeAttr(p.path)}" title="Remove project">
          <svg class="icon-xs" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
          </svg>
        </button>
      </div>
      <div class="projects-folder-path">${escapeHtml(shortenPath(p.path))}${branchHint ? ' · ' + branchHint : ''}</div>
    </div>`;
  }).join('');

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
  // C1: Set per-project scope
  _activeProjectRoot = project.path;

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

  // Gather git info (async, non-blocking for file tree)
  const gitInfo = await getGitInfo(project.path);

  // Show welcome + git info in viewer
  if (viewer) {
    const dirCount = entries.filter(e => e.isDirectory).length;
    const fileCount = entries.filter(e => !e.isDirectory).length;

    viewer.innerHTML = `
      <div class="projects-viewer-welcome">
        <div class="projects-viewer-welcome-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" width="48" height="48">
            <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
          </svg>
        </div>
        <div class="projects-viewer-welcome-title">${escapeHtml(project.name)}</div>
        <div class="projects-viewer-welcome-sub">${dirCount} folders, ${fileCount} files</div>
        <div class="projects-viewer-welcome-path">${escapeHtml(project.path)}</div>
      </div>
      ${renderGitBanner(gitInfo, project.path)}
    `;
    bindGitActions(viewer, project.path);
  }

  _selectedFile = null;
}

// ── Git Banner & Actions ───────────────────────────────────────────────────

function renderGitBanner(git: GitInfo, projectPath: string): string {
  if (!git.isRepo) {
    return `
      <div class="git-banner git-banner--none" style="margin-top:12px;padding:10px 12px;border-radius:8px;background:var(--surface-2, rgba(255,255,255,0.04));font-size:12px;color:var(--text-muted)">
        <span style="opacity:0.6">📁 Not a git repository</span>
        <button class="btn btn-sm git-action" data-action="init" data-path="${escapeAttr(projectPath)}" style="margin-left:auto;font-size:11px">
          git init
        </button>
      </div>`;
  }

  const branchBadge = git.branch
    ? `<span style="font-weight:600;font-family:var(--font-mono);font-size:12px;background:var(--accent-alpha, rgba(99,102,241,0.15));color:var(--accent);padding:2px 8px;border-radius:4px">${escapeHtml(git.branch)}</span>`
    : '';

  const dirtyBadge = git.dirty !== undefined && git.dirty > 0
    ? `<span style="font-size:11px;color:var(--warning, #facc15)">● ${git.dirty} changed</span>`
    : `<span style="font-size:11px;color:var(--success, #4ade80)">● Clean</span>`;

  let syncBadge = '';
  if (git.ahead || git.behind) {
    const parts: string[] = [];
    if (git.ahead) parts.push(`↑${git.ahead}`);
    if (git.behind) parts.push(`↓${git.behind}`);
    syncBadge = `<span style="font-size:11px;color:var(--text-muted);font-family:var(--font-mono)">${parts.join(' ')}</span>`;
  }

  const remoteBadge = git.remote
    ? `<span style="font-size:11px;color:var(--text-muted);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:250px" title="${escapeAttr(git.remote)}">${escapeHtml(shortenRemote(git.remote))}</span>`
    : '';

  const lastCommitLine = git.lastCommit
    ? `<div style="font-size:11px;color:var(--text-muted);margin-top:6px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">
        Latest: ${escapeHtml(git.lastCommit)}${git.lastCommitDate ? ` <span style="opacity:0.6">(${escapeHtml(git.lastCommitDate)})</span>` : ''}
      </div>`
    : '';

  return `
    <div class="git-banner" style="margin-top:12px;padding:10px 12px;border-radius:8px;background:var(--surface-2, rgba(255,255,255,0.04))">
      <div style="display:flex;align-items:center;gap:8px;flex-wrap:wrap">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="16" height="16" style="flex-shrink:0;opacity:0.7">
          <circle cx="12" cy="12" r="4"/><line x1="1.05" y1="12" x2="7" y2="12"/><line x1="17.01" y1="12" x2="22.96" y2="12"/>
        </svg>
        ${branchBadge}
        ${dirtyBadge}
        ${syncBadge}
        ${remoteBadge}
      </div>
      ${lastCommitLine}
      <div class="git-actions" style="display:flex;gap:6px;margin-top:8px;flex-wrap:wrap">
        ${git.remote ? `<button class="btn btn-sm git-action" data-action="pull" data-path="${escapeAttr(projectPath)}">⬇ Pull</button>` : ''}
        ${git.remote ? `<button class="btn btn-sm git-action" data-action="push" data-path="${escapeAttr(projectPath)}">⬆ Push</button>` : ''}
        <button class="btn btn-sm git-action" data-action="commit" data-path="${escapeAttr(projectPath)}">💾 Commit</button>
        <button class="btn btn-sm git-action" data-action="status" data-path="${escapeAttr(projectPath)}" style="margin-left:auto;opacity:0.7;font-size:11px">↻ Refresh</button>
      </div>
    </div>`;
}

function shortenRemote(url: string): string {
  // git@github.com:user/repo.git → user/repo
  const sshMatch = url.match(/:([^/]+\/[^.]+)/);
  if (sshMatch) return sshMatch[1];
  // https://github.com/user/repo.git → user/repo
  const httpsMatch = url.match(/(?:github\.com|gitlab\.com|bitbucket\.org)[/:]([^/]+\/[^/.]+)/);
  if (httpsMatch) return httpsMatch[1];
  return url;
}

function bindGitActions(container: HTMLElement, projectPath: string): void {
  container.querySelectorAll('.git-action').forEach(btn => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const action = (btn as HTMLElement).dataset.action;
      const path = (btn as HTMLElement).dataset.path || projectPath;
      if (!action) return;

      const origText = btn.textContent;
      btn.textContent = '…';
      (btn as HTMLButtonElement).disabled = true;

      try {
        switch (action) {
          case 'pull': {
            const out = await gitExec(path, 'pull');
            if (out !== null) {
              showToast(out.includes('Already up to date') ? 'Already up to date' : 'Pull complete', 'success');
            } else {
              showToast('Pull failed — check remote & credentials', 'error');
            }
            break;
          }
          case 'push': {
            const out = await gitExec(path, 'push');
            if (out !== null) {
              showToast('Push complete', 'success');
            } else {
              showToast('Push failed — check remote & credentials', 'error');
            }
            break;
          }
          case 'commit': {
            const msg = prompt('Commit message:');
            if (!msg) break;
            // Stage all + commit
            const addOut = await gitExec(path, 'add', '-A');
            if (addOut === null) { showToast('git add failed', 'error'); break; }
            const commitOut = await gitExec(path, 'commit', '-m', msg);
            if (commitOut !== null) {
              showToast('Committed!', 'success');
            } else {
              showToast('Commit failed — nothing to commit?', 'error');
            }
            break;
          }
          case 'init': {
            const initOut = await gitExec(path, 'init');
            if (initOut !== null) {
              showToast('Initialized git repo', 'success');
            } else {
              showToast('git init failed', 'error');
            }
            break;
          }
          case 'status': {
            // Just refresh git info
            break;
          }
        }

        // Refresh git info
        _gitInfoCache.delete(path);
        const project = _projects.find(p => p.path === path);
        if (project) await selectProject(project);

      } catch (err) {
        showToast(`Git error: ${err instanceof Error ? err.message : err}`, 'error');
      } finally {
        btn.textContent = origText;
        (btn as HTMLButtonElement).disabled = false;
      }
    });
  });
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
  if (empty) {
    empty.style.display = '';
    empty.innerHTML = `
      <div class="empty-icon">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
          <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>
        </svg>
      </div>
      <div class="empty-title">Your Projects</div>
      <div class="empty-subtitle" style="max-width:340px;text-align:center;line-height:1.6">
        Add a local folder to browse files, view git status, and give your agent
        project context. Works with any git repo or plain folder.
      </div>
      <div style="margin-top:16px;padding:12px 16px;border-radius:8px;background:var(--surface-2, rgba(255,255,255,0.04));font-size:12px;color:var(--text-muted);line-height:1.8;text-align:left;max-width:360px">
        <div style="font-weight:600;margin-bottom:4px;color:var(--text)">What Projects does:</div>
        <div>📁 <strong>Browse</strong> — file tree with code viewer</div>
        <div>🔀 <strong>Git</strong> — branch, status, pull, push, commit</div>
        <div>🤖 <strong>Agent context</strong> — your agent can read, edit, and run commands in project folders</div>
        <div>🔒 <strong>Scoped</strong> — agent access is confined to the project you select</div>
      </div>
    `;
  }
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
    _gitInfoCache.clear();
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
