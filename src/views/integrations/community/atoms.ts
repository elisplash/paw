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
export function isInstalled(
  pkg: CommunityPackage,
  installed: InstalledPackage[],
): boolean {
  return installed.some((i) => i.packageName === pkg.package_name);
}

/** Strip the n8n-nodes- prefix for display. */
export function displayName(packageName: string): string {
  return packageName
    .replace(/^@[^/]+\//, '')  // strip scope
    .replace(/^n8n-nodes-/, '')
    .replace(/-/g, ' ')
    .replace(/\b\w/g, (c) => c.toUpperCase());
}
