// Projects View — Atoms (pure logic, types, constants, security predicates)
// Zero DOM, zero IPC, zero Tauri plugin imports

// ── Types ──────────────────────────────────────────────────────────────────

export interface FileEntry {
  name: string;
  path: string;
  isDirectory: boolean;
  children?: FileEntry[];
  expanded?: boolean;
  size?: number;
  modified?: number;
}

export interface ProjectFolder {
  name: string;
  path: string;
  addedAt: string;
}

export interface GitInfo {
  isRepo: boolean;
  branch?: string;
  remote?: string;
  dirty?: number;
  ahead?: number;
  behind?: number;
  lastCommit?: string;
  lastCommitDate?: string;
}

// ── B4: Sensitive Path Blocking ─────────────────────────────────────────────

/** Paths that should never be added as project folders or browsed into */
const SENSITIVE_PATH_PATTERNS: Array<{ pattern: RegExp; label: string }> = [
  { pattern: /[/\\]\.ssh(\/|\\|$)/i, label: 'SSH keys directory' },
  { pattern: /[/\\]\.gnupg(\/|\\|$)/i, label: 'GPG keyring directory' },
  { pattern: /[/\\]\.aws(\/|\\|$)/i, label: 'AWS credentials directory' },
  { pattern: /[/\\]\.kube(\/|\\|$)/i, label: 'Kubernetes config directory' },
  { pattern: /[/\\]\.docker(\/|\\|$)/i, label: 'Docker config directory' },
  { pattern: /[/\\]\.gnome-keyring(\/|\\|$)/i, label: 'GNOME keyring directory' },
  { pattern: /[/\\]\.password-store(\/|\\|$)/i, label: 'Password store directory' },
  { pattern: /[/\\]\.netrc$/i, label: 'netrc credentials file' },
  { pattern: /^\/etc(\/|$)/i, label: '/etc system config' },
  { pattern: /^\/root(\/|$)/i, label: '/root home directory' },
  { pattern: /^\/var\/log(\/|$)/i, label: 'System logs directory' },
  { pattern: /^\/proc(\/|$)/i, label: 'proc filesystem' },
  { pattern: /^\/sys(\/|$)/i, label: 'sys filesystem' },
  { pattern: /^\/dev(\/|$)/i, label: 'Device filesystem' },
  { pattern: /^C:\\\\Windows(\\\\|$)/i, label: 'Windows system directory' },
  { pattern: /^C:\\\\Users\\\\[^\\\\]+\\\\AppData(\\\\|$)/i, label: 'AppData directory' },
  { pattern: /[/\\]\.openclaw(\/|\\|$)/i, label: 'OpenClaw config (contains tokens)' },
  { pattern: /[/\\]\.config[/\\]himalaya(\/|\\|$)/i, label: 'Himalaya email config' },
];

/**
 * Check if a path matches any known sensitive location.
 * Returns the label of the matched pattern, or null if safe.
 */
export function isSensitivePath(pathStr: string): string | null {
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

/**
 * Validate that a file path is within the active project scope.
 * Prevents directory traversal and access to paths outside the project root.
 * Returns null if valid, or an error string if out-of-scope.
 */
export function isOutOfProjectScope(
  filePath: string,
  activeProjectRoot: string | null,
): string | null {
  if (!activeProjectRoot) return null; // no project selected — no scope to enforce
  const normFile = filePath.replace(/\\/g, '/').replace(/\/+$/, '');
  const normRoot = activeProjectRoot.replace(/\\/g, '/').replace(/\/+$/, '');

  // Must start with project root
  if (!normFile.startsWith(`${normRoot}/`) && normFile !== normRoot) {
    return `Path "${filePath}" is outside the active project scope "${activeProjectRoot}"`;
  }

  // Block traversal sequences even within the path
  if (/\/\.\.\//g.test(normFile) || normFile.endsWith('/..')) {
    return `Path contains directory traversal: "${filePath}"`;
  }

  return null;
}

// ── File icon & language helpers ───────────────────────────────────────────

export function getFileIcon(ext: string): string {
  const icons: Record<string, string> = {
    // Code files
    ts: 'code',
    tsx: 'code',
    js: 'javascript',
    jsx: 'javascript',
    py: 'code',
    rs: 'code',
    go: 'code',
    rb: 'code',
    c: 'code',
    cpp: 'code',
    h: 'code',
    java: 'code',
    kt: 'code',
    swift: 'code',
    sh: 'terminal',
    bash: 'terminal',
    zsh: 'terminal',
    sql: 'database',
    // Config/data
    json: 'data_object',
    yaml: 'settings',
    yml: 'settings',
    toml: 'settings',
    xml: 'code',
    svg: 'image',
    // Markup
    md: 'description',
    html: 'html',
    css: 'css',
    // Media
    png: 'image',
    jpg: 'image',
    jpeg: 'image',
    gif: 'image',
    webp: 'image',
    ico: 'image',
    mp3: 'audio_file',
    wav: 'audio_file',
    ogg: 'audio_file',
    mp4: 'video_file',
    webm: 'video_file',
    mov: 'video_file',
    // Archives
    zip: 'folder_zip',
    tar: 'folder_zip',
    gz: 'folder_zip',
    // Documents
    pdf: 'picture_as_pdf',
    doc: 'description',
    docx: 'description',
    txt: 'description',
  };
  return icons[ext] || 'insert_drive_file';
}

export function getLanguageClass(ext: string): string {
  const langMap: Record<string, string> = {
    ts: 'language-typescript',
    tsx: 'language-typescript',
    js: 'language-javascript',
    jsx: 'language-javascript',
    py: 'language-python',
    rs: 'language-rust',
    go: 'language-go',
    rb: 'language-ruby',
    html: 'language-html',
    css: 'language-css',
    json: 'language-json',
    yaml: 'language-yaml',
    yml: 'language-yaml',
    toml: 'language-toml',
    md: 'language-markdown',
    sh: 'language-bash',
    bash: 'language-bash',
    zsh: 'language-bash',
    sql: 'language-sql',
    xml: 'language-xml',
    svg: 'language-xml',
    c: 'language-c',
    cpp: 'language-cpp',
    h: 'language-c',
    java: 'language-java',
    kt: 'language-kotlin',
    swift: 'language-swift',
  };
  return langMap[ext] || 'language-plaintext';
}

// ── Path helpers ───────────────────────────────────────────────────────────

export function shortenPath(path: string): string {
  return path.replace(/^\/Users\/[^/]+/, '~').replace(/^\/home\/[^/]+/, '~');
}

export function shortenRemote(url: string): string {
  // git@github.com:user/repo.git → user/repo
  const sshMatch = url.match(/:([^/]+\/[^.]+)/);
  if (sshMatch) return sshMatch[1];
  // https://github.com/user/repo.git → user/repo
  const httpsMatch = url.match(/(?:github\.com|gitlab\.com|bitbucket\.org)[/:]([^/]+\/[^/.]+)/);
  if (httpsMatch) return httpsMatch[1];
  return url;
}

export function getDepth(path: string, rootPath: string): number {
  const rootParts = rootPath.split('/').length;
  const pathParts = path.split('/').length;
  return pathParts - rootParts;
}

export function getProjectRoot(filePath: string, projects: ProjectFolder[]): string | null {
  for (const p of projects) {
    if (filePath.startsWith(p.path)) return p.path;
  }
  return null;
}

// ── Persistence (localStorage) ─────────────────────────────────────────────

export function loadSavedProjects(): ProjectFolder[] {
  try {
    const raw = localStorage.getItem('paw-project-folders');
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

export function savePersistProjects(projects: ProjectFolder[]): void {
  localStorage.setItem('paw-project-folders', JSON.stringify(projects));
}

// ── Binary extension list ──────────────────────────────────────────────────

export const BINARY_EXTENSIONS = [
  'png',
  'jpg',
  'jpeg',
  'gif',
  'svg',
  'ico',
  'bmp',
  'webp',
  'woff',
  'woff2',
  'ttf',
  'eot',
  'otf',
  'mp3',
  'mp4',
  'wav',
  'ogg',
  'webm',
  'zip',
  'tar',
  'gz',
  'rar',
  'pdf',
  'exe',
  'dll',
  'so',
  'dylib',
  'o',
  'class',
  'pyc',
  'wasm',
  'db',
  'sqlite',
  'lock',
];
