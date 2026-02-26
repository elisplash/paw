// src/views/integrations/atoms.ts — Pure types, constants, and helpers
//
// Atom-level: no DOM, no IPC, no side effects.

// ── Types ──────────────────────────────────────────────────────────────

export interface CredentialField {
  key: string; // "api_key"
  label: string; // "API Key"
  type: 'text' | 'password' | 'url' | 'select';
  placeholder?: string;
  required: boolean;
  helpText?: string;
}

export interface SetupStep {
  instruction: string;
  link?: string;
  tip?: string;
}

export interface SetupGuide {
  title: string;
  steps: SetupStep[];
  estimatedTime: string;
}

export type ServiceCategory =
  | 'communication'
  | 'development'
  | 'productivity'
  | 'crm'
  | 'commerce'
  | 'social'
  | 'cloud'
  | 'storage'
  | 'database'
  | 'analytics'
  | 'security'
  | 'ai'
  | 'voice'
  | 'content'
  | 'utility';

export interface ServiceDefinition {
  id: string;
  name: string;
  icon: string; // Material icon name
  color: string; // Brand accent color
  category: ServiceCategory;
  description: string;
  capabilities: string[];
  n8nNodeType: string; // e.g. "n8n-nodes-base.slack"
  credentialFields: CredentialField[];
  setupGuide: SetupGuide;
  queryExamples: string[];
  automationExamples: string[];
  docsUrl: string;
  popular: boolean;
  /** npm package name if this service needs a community node (not in n8n-nodes-base). */
  communityPackage?: string;
}

export interface ConnectedService {
  serviceId: string;
  connectedAt: string;
  lastUsed?: string;
  toolCount: number;
  status: 'connected' | 'error' | 'expired';
}

// ── Category metadata ──────────────────────────────────────────────────

export interface CategoryMeta {
  id: ServiceCategory;
  label: string;
  icon: string;
}

export const CATEGORIES: CategoryMeta[] = [
  { id: 'communication', label: 'Communication', icon: 'chat' },
  { id: 'development', label: 'Development', icon: 'code' },
  { id: 'productivity', label: 'Productivity', icon: 'edit_note' },
  { id: 'crm', label: 'CRM & Sales', icon: 'handshake' },
  { id: 'commerce', label: 'Commerce', icon: 'shopping_cart' },
  { id: 'social', label: 'Social Media', icon: 'share' },
  { id: 'cloud', label: 'Cloud', icon: 'cloud' },
  { id: 'storage', label: 'Storage & Files', icon: 'folder' },
  { id: 'database', label: 'Databases', icon: 'database' },
  { id: 'analytics', label: 'Analytics', icon: 'bar_chart' },
  { id: 'security', label: 'Security', icon: 'shield' },
  { id: 'ai', label: 'AI & ML', icon: 'psychology' },
  { id: 'voice', label: 'Voice & Video', icon: 'call' },
  { id: 'content', label: 'Content & CMS', icon: 'article' },
  { id: 'utility', label: 'Utilities', icon: 'build' },
];

// ── Sort options ───────────────────────────────────────────────────────

export type SortOption = 'popular' | 'a-z' | 'category';

// ── Pure helpers ───────────────────────────────────────────────────────

export function escHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

export function fuzzyMatch(query: string, text: string): boolean {
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  if (t.includes(q)) return true;
  // Simple fuzzy: all chars in order
  let qi = 0;
  for (let i = 0; i < t.length && qi < q.length; i++) {
    if (t[i] === q[qi]) qi++;
  }
  return qi === q.length;
}

export function filterServices(
  services: ServiceDefinition[],
  query: string,
  category: ServiceCategory | 'all',
): ServiceDefinition[] {
  let result = services;
  if (category !== 'all') {
    result = result.filter((s) => s.category === category);
  }
  if (query.trim()) {
    result = result.filter(
      (s) =>
        fuzzyMatch(query, s.name) ||
        fuzzyMatch(query, s.description) ||
        fuzzyMatch(query, s.category),
    );
  }
  return result;
}

export function sortServices(services: ServiceDefinition[], sort: SortOption): ServiceDefinition[] {
  const copy = [...services];
  switch (sort) {
    case 'popular':
      return copy.sort(
        (a, b) => (b.popular ? 1 : 0) - (a.popular ? 1 : 0) || a.name.localeCompare(b.name),
      );
    case 'a-z':
      return copy.sort((a, b) => a.name.localeCompare(b.name));
    case 'category':
      return copy.sort(
        (a, b) => a.category.localeCompare(b.category) || a.name.localeCompare(b.name),
      );
    default:
      return copy;
  }
}

export function categoryLabel(cat: ServiceCategory): string {
  return CATEGORIES.find((c) => c.id === cat)?.label ?? cat;
}

export function categoryIcon(cat: ServiceCategory): string {
  return CATEGORIES.find((c) => c.id === cat)?.icon ?? 'extension';
}
