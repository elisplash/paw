// src/views/integrations/community/atoms.ts — Pure types, constants, and helpers
//
// Atom-level: no DOM, no IPC, no side effects.

// ── Types ──────────────────────────────────────────────────────────────

/** A community package from the npm registry (ncnodes search). */
export interface CommunityPackage {
  package_name: string;
  description: string;
  author: string;
  version: string;
  weekly_downloads: number;
  last_updated: string;
  repository_url: string;
  keywords: string[];
}

/** An installed community package (from n8n REST API). */
export interface InstalledPackage {
  packageName: string;
  installedVersion: string;
  installedNodes: Array<{ name: string; type: string }>;
}

// ── n8n Credential Schema Types ────────────────────────────────────────

/** A field definition from n8n's credential type schema. */
export interface N8nCredentialSchemaField {
  name: string;
  display_name: string;
  field_type: string; // "string", "number", "boolean", "options"
  required: boolean;
  default_value: string | null;
  placeholder: string | null;
  description: string | null;
  options: string[];
  is_secret: boolean;
}

/** Schema for a specific n8n credential type. */
export interface N8nCredentialSchema {
  credential_type: string;
  display_name: string;
  fields: N8nCredentialSchemaField[];
}

/** Credential info for a package's nodes (returned by backend). */
export interface PackageCredentialInfo {
  package_name: string;
  credential_types: N8nCredentialSchema[];
}

export type CommunityTab = 'browse' | 'installed';
export type CommunitySortOption = 'downloads' | 'updated' | 'a-z';

// ── Constants ──────────────────────────────────────────────────────────

export const SORT_OPTIONS: Array<{ value: CommunitySortOption; label: string }> = [
  { value: 'downloads', label: 'Most Downloaded' },
  { value: 'updated', label: 'Recently Updated' },
  { value: 'a-z', label: 'A–Z' },
];

export const DEBOUNCE_MS = 350;

// ── Pure helpers ───────────────────────────────────────────────────────

export function escHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

/** Format download count: 12345 → "12.3k" */
export function formatDownloads(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}

/** Format ISO date to relative: "3 months ago", "2 days ago" */
export function relativeDate(iso: string): string {
  const now = Date.now();
  const then = new Date(iso).getTime();
  if (isNaN(then)) return iso;
  const diffMs = now - then;
  const mins = Math.floor(diffMs / 60_000);
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d ago`;
  const months = Math.floor(days / 30);
  if (months < 12) return `${months}mo ago`;
  const years = Math.floor(months / 12);
  return `${years}y ago`;
}

/** Sort packages by the chosen option. */
export function sortPackages(
  pkgs: CommunityPackage[],
  sort: CommunitySortOption,
): CommunityPackage[] {
  const copy = [...pkgs];
  switch (sort) {
    case 'downloads':
      return copy.sort((a, b) => b.weekly_downloads - a.weekly_downloads);
    case 'updated':
      return copy.sort(
        (a, b) => new Date(b.last_updated).getTime() - new Date(a.last_updated).getTime(),
      );
    case 'a-z':
      return copy.sort((a, b) => a.package_name.localeCompare(b.package_name));
    default:
      return copy;
  }
}

/** Check if a package is in the installed list. */
export function isInstalled(pkg: CommunityPackage, installed: InstalledPackage[]): boolean {
  return installed.some((i) => i.packageName === pkg.package_name);
}

/** Strip the n8n-nodes- prefix for display. */
export function displayName(packageName: string): string {
  return packageName
    .replace(/^@[^/]+\//, '') // strip scope
    .replace(/^n8n-nodes-/, '')
    .replace(/-/g, ' ')
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

// ── Community Package Requirements ─────────────────────────────────────
//
// Maps service IDs to the npm package name they need from the community.
// Services using n8n-nodes-base.httpRequest as a fallback often have a
// dedicated community node that provides richer, native integration.
//
// This map grows over time. Only well-known, maintained packages are listed.

export const COMMUNITY_PACKAGE_MAP: Record<string, string> = {
  // Messaging & Communication
  whatsapp: 'n8n-nodes-whatsapp-buttons',
  telegram: 'n8n-nodes-telegram-trigger',
  twilio: 'n8n-nodes-twilio-extended',

  // Browser & Scraping
  puppeteer: 'n8n-nodes-puppeteer',
  browserless: 'n8n-nodes-browserless',
  playwright: '@nicklason/n8n-nodes-playwright',

  // Databases & Caching
  redis: 'n8n-nodes-redis',
  mongodb: 'n8n-nodes-mongodb',
  dynamodb: 'n8n-nodes-dynamodb',

  // Cloud Storage
  minio: 'n8n-nodes-minio',
  backblaze: 'n8n-nodes-backblaze',

  // DevOps & Infra
  docker: 'n8n-nodes-docker',
  kubernetes: 'n8n-nodes-kubernetes',
  portainer: 'n8n-nodes-portainer',

  // AI & ML
  'openai-advanced': 'n8n-nodes-openai',
  langchain: '@n8n/n8n-nodes-langchain',

  // Productivity
  'google-calendar-advanced': 'n8n-nodes-google-calendar',
  raindrop: 'n8n-nodes-raindrop',
  rss: 'n8n-nodes-rss-feed-trigger',

  // Social
  mastodon: 'n8n-nodes-mastodon',
  bluesky: 'n8n-nodes-bluesky',

  // Home Automation
  'home-assistant': 'n8n-nodes-home-assistant',
  mqtt: 'n8n-nodes-mqtt',

  // CRM & Marketing
  lemlist: 'n8n-nodes-lemlist',

  // Finance
  plaid: 'n8n-nodes-plaid',
};

/** Look up the community package needed for a service (if any). */
export function getRequiredPackage(serviceId: string, communityPackage?: string): string | null {
  // Explicit per-service override takes priority
  if (communityPackage) return communityPackage;
  // Then check the static map
  return COMMUNITY_PACKAGE_MAP[serviceId] ?? null;
}
