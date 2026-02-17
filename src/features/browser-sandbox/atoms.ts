// P4: Browser & Sandbox — Atoms (pure types + functions)
// No side effects, no imports from Tauri or localStorage.

// ── Browser Profiles ───────────────────────────────────────────────────

export interface BrowserProfile {
  agent_id: string;
  profile_dir: string;
  created_at: string;
  last_used_at: string;
}

export function describeProfileAge(profile: BrowserProfile): string {
  if (!profile.last_used_at) return 'never used';
  const ms = Date.now() - new Date(profile.last_used_at).getTime();
  const mins = Math.floor(ms / 60_000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

export function formatProfileSize(profile: BrowserProfile): string {
  // Profile dirs can be large; we don't track size on the backend.
  // This is a placeholder for potential future enhancement.
  return profile.profile_dir;
}

// ── Per-Agent Workspaces ───────────────────────────────────────────────

export interface AgentWorkspace {
  agent_id: string;
  workspace_path: string;
  created_at: string;
  size_bytes: number | null;
}

export function formatWorkspaceSize(ws: AgentWorkspace): string {
  if (ws.size_bytes == null) return 'unknown';
  const bytes = ws.size_bytes;
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

export function describeWorkspaceAge(ws: AgentWorkspace): string {
  if (!ws.created_at) return 'unknown';
  const ms = Date.now() - new Date(ws.created_at).getTime();
  const days = Math.floor(ms / 86_400_000);
  if (days === 0) return 'today';
  if (days === 1) return 'yesterday';
  return `${days} days ago`;
}

// ── Screenshot Viewer ──────────────────────────────────────────────────

export interface ScreenshotInfo {
  filename: string;
  filepath: string;
  size_bytes: number;
  created_at: string;
}

export function formatScreenshotSize(info: ScreenshotInfo): string {
  const bytes = info.size_bytes;
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function extractTimestamp(filename: string): string {
  // Filenames are like: screenshot-20260217-143022.png
  const match = filename.match(/screenshot-(\d{4})(\d{2})(\d{2})-(\d{2})(\d{2})(\d{2})/);
  if (!match) return filename;
  const [, y, mo, d, h, mi, s] = match;
  return `${y}-${mo}-${d} ${h}:${mi}:${s}`;
}

export function screenshotDataUrl(base64: string): string {
  return `data:image/png;base64,${base64}`;
}

// ── Domain Policy ──────────────────────────────────────────────────────

export type DomainPolicyMode = 'allowall' | 'allowlist' | 'denylist';

export interface DomainPolicy {
  mode: DomainPolicyMode;
  domains: string[];
}

export const DEFAULT_DOMAIN_POLICY: DomainPolicy = {
  mode: 'allowall',
  domains: [],
};

/** Presets for common domain policies. */
export const DOMAIN_POLICY_PRESETS: Record<string, DomainPolicy> = {
  unrestricted: { mode: 'allowall', domains: [] },
  research_only: {
    mode: 'allowlist',
    domains: [
      '*.google.com',
      '*.duckduckgo.com',
      '*.wikipedia.org',
      '*.github.com',
      '*.stackoverflow.com',
      '*.arxiv.org',
    ],
  },
  no_social: {
    mode: 'denylist',
    domains: [
      '*.facebook.com',
      '*.twitter.com',
      '*.x.com',
      '*.instagram.com',
      '*.tiktok.com',
      '*.reddit.com',
    ],
  },
  sandbox: {
    mode: 'allowlist',
    domains: ['localhost', '127.0.0.1'],
  },
};

export function describeDomainPolicy(policy: DomainPolicy): string {
  switch (policy.mode) {
    case 'allowall':
      return 'Unrestricted — all domains allowed';
    case 'allowlist':
      return `Allowlist — ${policy.domains.length} domain${policy.domains.length !== 1 ? 's' : ''} allowed`;
    case 'denylist':
      return `Denylist — ${policy.domains.length} domain${policy.domains.length !== 1 ? 's' : ''} blocked`;
  }
}

/** Validate a domain pattern (e.g. *.example.com, example.com). */
export function isValidDomainPattern(pattern: string): boolean {
  if (!pattern) return false;
  // Allow *.domain.tld or domain.tld
  const re = /^(\*\.)?[a-zA-Z0-9]([a-zA-Z0-9-]*[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9-]*[a-zA-Z0-9])?)*$/;
  return re.test(pattern);
}

/** Check if a domain would be allowed by a policy (client-side preview). */
export function wouldAllow(domain: string, policy: DomainPolicy): boolean {
  if (policy.mode === 'allowall') return true;

  const domainLower = domain.toLowerCase();
  const matches = policy.domains.some((pattern) => {
    const p = pattern.toLowerCase();
    if (p.startsWith('*.')) {
      const suffix = p.slice(2);
      return domainLower === suffix || domainLower.endsWith(`.${suffix}`);
    }
    return domainLower === p;
  });

  return policy.mode === 'allowlist' ? matches : !matches;
}
