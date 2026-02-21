// ─── Browser Sandbox · Atoms ───────────────────────────────────────────
// Pure types, constants, and functions for browser profiles, screenshots,
// workspaces, and network policy. No side effects.

// ── Types ──────────────────────────────────────────────────────────────

export interface BrowserProfile {
  id: string;
  name: string;
  user_data_dir: string;
  created_at: string;
  last_used: string;
  size_bytes: number;
}

export interface BrowserConfig {
  default_profile: string;
  profiles: BrowserProfile[];
  headless: boolean;
  auto_close_tabs: boolean;
  idle_timeout_secs: number;
}

export interface ScreenshotEntry {
  filename: string;
  path: string;
  size_bytes: number;
  created_at: string;
  base64_png?: string;
}

export interface WorkspaceInfo {
  agent_id: string;
  path: string;
  total_files: number;
  total_size_bytes: number;
  exists: boolean;
}

export interface WorkspaceFile {
  name: string;
  path: string;
  is_dir: boolean;
  size_bytes: number;
  modified_at: string;
}

export interface NetworkPolicy {
  enabled: boolean;
  allowed_domains: string[];
  blocked_domains: string[];
  log_requests: boolean;
  recent_requests: NetworkRequest[];
}

export interface NetworkRequest {
  url: string;
  domain: string;
  allowed: boolean;
  timestamp: string;
  tool_name: string;
}

// ── Constants ──────────────────────────────────────────────────────────

export const DEFAULT_BROWSER_CONFIG: BrowserConfig = {
  default_profile: 'default',
  profiles: [
    {
      id: 'default',
      name: 'Default',
      user_data_dir: '',
      created_at: '',
      last_used: '',
      size_bytes: 0,
    },
  ],
  headless: true,
  auto_close_tabs: true,
  idle_timeout_secs: 300,
};

/** Default safe domains for outbound network access */
export const DEFAULT_ALLOWED_DOMAINS = [
  'api.openai.com',
  'api.anthropic.com',
  'generativelanguage.googleapis.com',
  'openrouter.ai',
  'api.elevenlabs.io',
  'duckduckgo.com',
  'html.duckduckgo.com',
  'api.coinbase.com',
  'localhost',
] as const;

/** Default blocked domains (data exfiltration risk) */
export const DEFAULT_BLOCKED_DOMAINS = [
  'pastebin.com',
  'transfer.sh',
  'file.io',
  '0x0.st',
] as const;

// ── Pure Functions ─────────────────────────────────────────────────────

/** Format bytes as human-readable string */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

/** Validate a domain string */
export function isValidDomain(domain: string): boolean {
  if (!domain || domain.length > 253) return false;
  // Allow *.example.com wildcard
  const d = domain.startsWith('*.') ? domain.slice(2) : domain;
  return /^[a-zA-Z0-9]([a-zA-Z0-9-]*[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9-]*[a-zA-Z0-9])?)*$/.test(
    d,
  );
}

/** Extract domain from a URL */
export function extractDomain(url: string): string {
  return url
    .replace(/^https?:\/\//, '')
    .split('/')[0]
    .split(':')[0]
    .toLowerCase();
}

/** Get a relative time string from an ISO timestamp */
export function timeAgo(isoTimestamp: string): string {
  const now = Date.now();
  const then = new Date(isoTimestamp).getTime();
  const diffMs = now - then;
  const mins = Math.floor(diffMs / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d ago`;
  return new Date(isoTimestamp).toLocaleDateString();
}
