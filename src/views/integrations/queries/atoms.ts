// src/views/integrations/queries/atoms.ts — Query types & pure helpers
//
// Atom-level: no DOM, no IPC, no side-effects.

// ── Types ──────────────────────────────────────────────────────────────

/** A query that an agent can run against a connected service. */
export interface ServiceQuery {
  /** Unique query ID (e.g. 'hubspot-deals-this-month'). */
  id: string;
  /** Human-readable question. */
  question: string;
  /** Service(s) needed to answer. */
  serviceIds: string[];
  /** Category for grouping in UI. */
  category: QueryCategory;
  /** Material Symbols icon name. */
  icon: string;
  /** Brief description of what data is returned. */
  resultHint: string;
}

/** Result of an executed query returned by the backend. */
export interface QueryResult {
  queryId: string;
  status: 'success' | 'error' | 'partial';
  /** Human-readable formatted answer. */
  formatted: string;
  /** Optional structured data for rich rendering. */
  data?: QueryData;
  /** Attention items highlighted by the agent. */
  highlights?: QueryHighlight[];
  /** ISO timestamp of when the query ran. */
  executedAt: string;
}

/** Structured data payload for rich query results. */
export interface QueryData {
  type: 'table' | 'summary' | 'list' | 'kpi';
  columns?: string[];
  rows?: string[][];
  items?: string[];
  kpis?: { label: string; value: string; trend?: 'up' | 'down' | 'flat' }[];
}

/** An attention item in query results. */
export interface QueryHighlight {
  severity: 'info' | 'warning' | 'urgent';
  icon: string;
  message: string;
}

export type QueryCategory =
  | 'sales'
  | 'projects'
  | 'communication'
  | 'crm'
  | 'development'
  | 'analytics'
  | 'cross-service';

// ── Category metadata ──────────────────────────────────────────────────

export interface QueryCategoryMeta {
  id: QueryCategory;
  label: string;
  icon: string;
}

export const QUERY_CATEGORIES: QueryCategoryMeta[] = [
  { id: 'sales', label: 'Sales', icon: 'payments' },
  { id: 'projects', label: 'Projects', icon: 'assignment' },
  { id: 'communication', label: 'Comms', icon: 'forum' },
  { id: 'crm', label: 'CRM', icon: 'contacts' },
  { id: 'development', label: 'Dev', icon: 'code' },
  { id: 'analytics', label: 'Analytics', icon: 'analytics' },
  { id: 'cross-service', label: 'Cross-Service', icon: 'hub' },
];

// ── Pure helpers ───────────────────────────────────────────────────────

/** Filter queries by service and/or text search. */
export function filterQueries(
  queries: ServiceQuery[],
  opts: { serviceId?: string; category?: QueryCategory | 'all'; query?: string },
): ServiceQuery[] {
  let result = queries;

  if (opts.serviceId) {
    result = result.filter((q) => q.serviceIds.includes(opts.serviceId!));
  }
  if (opts.category && opts.category !== 'all') {
    result = result.filter((q) => q.category === opts.category);
  }
  if (opts.query) {
    const lower = opts.query.toLowerCase();
    result = result.filter(
      (q) => q.question.toLowerCase().includes(lower) || q.resultHint.toLowerCase().includes(lower),
    );
  }
  return result;
}

/** Get queries that require multiple services. */
export function getCrossServiceQueries(queries: ServiceQuery[]): ServiceQuery[] {
  return queries.filter((q) => q.serviceIds.length > 1);
}

/** Check if all services required by a query are connected. */
export function isQueryReady(query: ServiceQuery, connectedIds: Set<string>): boolean {
  return query.serviceIds.every((id) => connectedIds.has(id));
}

/** Severity badge for query highlights. */
export function highlightBadge(severity: QueryHighlight['severity']): {
  icon: string;
  label: string;
  cssClass: string;
} {
  switch (severity) {
    case 'urgent':
      return { icon: 'error', label: 'Urgent', cssClass: 'highlight-urgent' };
    case 'warning':
      return { icon: 'warning', label: 'Attention', cssClass: 'highlight-warning' };
    default:
      return { icon: 'info', label: 'Info', cssClass: 'highlight-info' };
  }
}
