// PawzHub Marketplace — Atoms (pure data, constants, helpers)
// Zero DOM, zero IPC — shared across PawzHub view modules.

// ── Category metadata ──────────────────────────────────────────────────────

/** Categories for the PawzHub marketplace filter bar. */
export const PAWZHUB_CATEGORIES = [
  'all',
  'development',
  'productivity',
  'communication',
  'data',
  'devops',
  'finance',
  'marketing',
  'media',
  'research',
] as const;

export type PawzHubCategory = (typeof PAWZHUB_CATEGORIES)[number];

// ── Tier metadata ──────────────────────────────────────────────────────────

export const TIER_META: Record<string, { label: string; emoji: string; color: string }> = {
  skill: { label: 'Skill', emoji: '🔵', color: '#3b82f6' },
  integration: { label: 'Integration', emoji: '🟣', color: '#a855f7' },
  extension: { label: 'Extension', emoji: '🟡', color: '#eab308' },
  mcp: { label: 'MCP Server', emoji: '🔴', color: '#ef4444' },
};

// ── Featured Skills (curated list for the hero section) ────────────────────

/** Skill IDs to feature prominently at the top of PawzHub. */
export const FEATURED_SKILL_IDS = [
  'github',
  'discord',
  'n8n',
  'notion',
  'slack',
  'spotify',
  'hue',
  'home-assistant',
];

// ── Community Sources ──────────────────────────────────────────────────────

export const POPULAR_REPOS = [
  { source: 'vercel-labs/agent-skills', label: 'Vercel Agent Skills' },
  { source: 'anthropics/skills', label: 'Anthropic Skills' },
];

export const POPULAR_TAGS = [
  'marketing',
  'trading',
  'supabase',
  'writing',
  'coding',
  'data analysis',
  'devops',
  'design',
  'finance',
  'research',
];

// ── Helpers ────────────────────────────────────────────────────────────────

export function msIcon(name: string, size: string = 'ms-sm'): string {
  return `<span class="ms ${size}">${name}</span>`;
}

export function tierBadge(tier: string): string {
  const meta = TIER_META[tier] || TIER_META.skill;
  return `<span class="pawzhub-tier-badge" style="--tier-color:${meta.color}">${meta.emoji} ${meta.label}</span>`;
}

export function formatInstalls(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toString();
}
