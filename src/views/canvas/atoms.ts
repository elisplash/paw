// Canvas View — Pure helpers (no DOM, no IPC)

import type {
  CanvasComponentRow,
  CanvasComponentType,
  CanvasPosition,
} from '../../engine/atoms/types';

// ── Parsed Component ──────────────────────────────────────────────────

/** A canvas component with parsed JSON fields, ready for rendering. */
export interface ParsedCanvasComponent {
  id: string;
  sessionId: string | null;
  dashboardId: string | null;
  agentId: string;
  componentType: CanvasComponentType;
  title: string;
  data: Record<string, unknown>;
  position: CanvasPosition | null;
  createdAt: string;
  updatedAt: string;
}

/** Parse a raw backend row into a renderable component. */
export function parseComponent(row: CanvasComponentRow): ParsedCanvasComponent {
  let data: Record<string, unknown> = {};
  try {
    data = JSON.parse(row.data);
  } catch {
    data = { _raw: row.data };
  }

  let position: CanvasPosition | null = null;
  if (row.position) {
    try {
      position = JSON.parse(row.position);
    } catch {
      /* ignore malformed position */
    }
  }

  return {
    id: row.id,
    sessionId: row.session_id,
    dashboardId: row.dashboard_id,
    agentId: row.agent_id,
    componentType: row.component_type,
    title: row.title,
    data,
    position,
    createdAt: row.created_at,
    updatedAt: row.updated_at,
  };
}

// ── Grid Layout ───────────────────────────────────────────────────────

/** Default grid columns for bento layout. */
export const GRID_COLUMNS = 3;

/** Compute CSS grid properties from a CanvasPosition. */
export function gridStyle(pos: CanvasPosition | null): string {
  if (!pos) return '';
  const parts: string[] = [];
  if (pos.col) parts.push(`grid-column-start: ${pos.col}`);
  if (pos.width) parts.push(`grid-column-end: span ${pos.width}`);
  if (pos.row) parts.push(`grid-row-start: ${pos.row}`);
  if (pos.height) parts.push(`grid-row-end: span ${pos.height}`);
  return parts.join('; ');
}

// ── Component Type Metadata ───────────────────────────────────────────

/** Map component types to Material Symbol icons. */
export function componentIcon(type: CanvasComponentType): string {
  const icons: Record<CanvasComponentType, string> = {
    metric: 'speed',
    table: 'table_chart',
    chart: 'show_chart',
    log: 'receipt_long',
    kv: 'data_object',
    card: 'article',
    status: 'info',
    progress: 'hourglass_top',
    form: 'edit_note',
    markdown: 'description',
  };
  return icons[type] ?? 'widgets';
}

/** Human-readable label for a component type. */
export function componentLabel(type: CanvasComponentType): string {
  const labels: Record<CanvasComponentType, string> = {
    metric: 'Metric',
    table: 'Table',
    chart: 'Chart',
    log: 'Log',
    kv: 'Key-Value',
    card: 'Card',
    status: 'Status',
    progress: 'Progress',
    form: 'Form',
    markdown: 'Markdown',
  };
  return labels[type] ?? 'Widget';
}

// ── Data Helpers ──────────────────────────────────────────────────────

/** Safely extract a string field from component data. */
export function dataStr(data: Record<string, unknown>, key: string, fallback = ''): string {
  const v = data[key];
  return typeof v === 'string' ? v : fallback;
}

/** Safely extract a number field from component data. */
export function dataNum(data: Record<string, unknown>, key: string, fallback = 0): number {
  const v = data[key];
  return typeof v === 'number' ? v : fallback;
}

/** Safely extract an array field from component data. */
export function dataArr(data: Record<string, unknown>, key: string): unknown[] {
  const v = data[key];
  return Array.isArray(v) ? v : [];
}
