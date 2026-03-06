// Canvas View — DOM rendering + IPC
// Renders a bento-grid of agent-generated components.

import { pawEngine } from '../../engine';
import { $, escHtml, promptModal } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { formatMarkdown } from '../../components/molecules/markdown';
import {
  type ParsedCanvasComponent,
  parseComponent,
  gridStyle,
  componentIcon,
  dataStr,
  dataNum,
  dataArr,
  GRID_COLUMNS,
} from './atoms';
import type {
  CanvasComponentRow,
  CanvasComponent,
  CanvasComponentPatch,
} from '../../engine/atoms/types';
import { renderSvgChart } from '../../components/molecules/canvas-chart';

// ── State bridge (set by index.ts) ────────────────────────────────────

interface MoleculesState {
  getComponents: () => ParsedCanvasComponent[];
  setComponents: (c: ParsedCanvasComponent[]) => void;
  getSessionId: () => string | null;
  getDashboardId: () => string | null;
  getDashboardName: () => string | null;
  getTabBarHtml: () => string;
  wireTabBar: () => void;
  onSave: (name: string) => Promise<void>;
  onRename: (name: string) => Promise<void>;
  onPin: () => Promise<void>;
  onPopOut: () => Promise<void>;
  onOpenDashboard: (dashboardId: string) => Promise<void>;
  onDelete: () => Promise<void>;
}

let _state: MoleculesState;

export function initMoleculesState() {
  return {
    setMoleculesState(s: MoleculesState) {
      _state = s;
    },
  };
}

// ── Fetch ─────────────────────────────────────────────────────────────

/** Load canvas components for the current session from the backend. */
export async function fetchCanvasComponents(): Promise<void> {
  const sid = _state.getSessionId();
  try {
    // If we have a session, load by session; otherwise load recent components
    const rows = sid
      ? await pawEngine.canvasListBySession(sid)
      : await pawEngine.canvasListRecent(50);
    _state.setComponents(rows.map(parseComponent));
  } catch (e) {
    console.warn('[canvas] Failed to load components:', e);
  }
}

/** Load canvas components for a saved dashboard. */
export async function fetchDashboardComponents(dashboardId: string): Promise<void> {
  try {
    const rows = await pawEngine.canvasListByDashboard(dashboardId);
    _state.setComponents(rows.map(parseComponent));
  } catch (e) {
    console.warn('[canvas] Failed to load dashboard components:', e);
  }
}

// ── Full Render ───────────────────────────────────────────────────────

/** Render the entire canvas view. */
export function renderCanvas(): void {
  const container = $('canvas-view');
  if (!container) return;

  const components = _state.getComponents();
  const isEmpty = components.length === 0;
  const dashName = _state.getDashboardName();
  const dashId = _state.getDashboardId();
  const hasDashboard = !!dashId;

  const tabBarHtml = _state.getTabBarHtml();

  const titleText = dashName ? escHtml(dashName) : 'Canvas';

  container.innerHTML = `
    ${tabBarHtml}
    <div class="canvas-header">
      <div class="canvas-header-left">
        <h2><span class="ms">dashboard_customize</span> ${titleText}</h2>
      </div>
      <div class="canvas-toolbar">
        <button class="btn btn-ghost btn-sm" id="canvas-open-btn" title="Open saved dashboard">
          <span class="ms ms-sm">folder_open</span> Open
        </button>
        ${
          !isEmpty
            ? `<button class="btn btn-ghost btn-sm" id="canvas-save-btn" title="${hasDashboard ? 'Saved' : 'Save as dashboard'}">
                <span class="ms ms-sm">${hasDashboard ? 'check_circle' : 'save'}</span> ${hasDashboard ? 'Saved' : 'Save'}
              </button>`
            : ''
        }
        ${
          hasDashboard
            ? `<button class="btn btn-ghost btn-sm" id="canvas-rename-btn" title="Rename dashboard">
                <span class="ms ms-sm">edit</span> Rename
              </button>
              <button class="btn btn-ghost btn-sm" id="canvas-pin-btn" title="Pin/Unpin dashboard">
                <span class="ms ms-sm">push_pin</span> Pin
              </button>
              <button class="btn btn-ghost btn-sm" id="canvas-popout-btn" title="Open in new window">
                <span class="ms ms-sm">open_in_new</span>
              </button>
              <button class="btn btn-ghost btn-sm canvas-toolbar-danger" id="canvas-delete-btn" title="Delete dashboard">
                <span class="ms ms-sm">delete</span>
              </button>`
            : ''
        }
        ${
          !isEmpty && !hasDashboard
            ? `<button class="btn btn-ghost btn-sm" id="canvas-clear-btn" title="Clear canvas">
                <span class="ms ms-sm">delete_sweep</span> Clear
              </button>`
            : ''
        }
      </div>
    </div>
    <div class="canvas-body">
      ${isEmpty ? renderEmptyState() : renderGrid(components)}
    </div>
    <div class="canvas-dashboard-picker" id="canvas-dashboard-picker" style="display:none"></div>
  `;

  _state.wireTabBar();
  wireEvents();
  activateLiveWidgets();
}

// ── Empty State ───────────────────────────────────────────────────────

function renderEmptyState(): string {
  return `
    <div class="canvas-empty">
      <span class="ms ms-xl">dashboard_customize</span>
      <h3>No canvas components yet</h3>
      <p>Ask an agent to visualize data and components will appear here in real-time.</p>
      <p class="canvas-empty-hint">Try: "Show me a dashboard of my project status"</p>
    </div>
  `;
}

// ── Bento Grid ────────────────────────────────────────────────────────

function renderGrid(components: ParsedCanvasComponent[]): string {
  const cards = components
    .map((c) => {
      const style = gridStyle(c.position);
      return `
      <div class="canvas-card" data-component-id="${escHtml(c.id)}"${style ? ` style="${style}"` : ''}>
        <div class="canvas-card-header">
          <span class="ms ms-sm">${componentIcon(c.componentType)}</span>
          <span class="canvas-card-title">${escHtml(c.title)}</span>
          <button class="btn btn-ghost btn-xs canvas-card-remove" data-id="${escHtml(c.id)}" title="Remove">
            <span class="ms ms-sm">close</span>
          </button>
        </div>
        <div class="canvas-card-body">
          ${renderComponentBody(c)}
        </div>
      </div>
    `;
    })
    .join('');

  return `<div class="canvas-grid" style="grid-template-columns: repeat(${GRID_COLUMNS}, 1fr)">${cards}</div>`;
}

// ── Component Renderers ───────────────────────────────────────────────

function renderComponentBody(c: ParsedCanvasComponent): string {
  switch (c.componentType) {
    case 'metric':
      return renderMetric(c.data);
    case 'table':
      return renderTable(c.data);
    case 'chart':
      return renderChart(c.data);
    case 'log':
      return renderLog(c.data);
    case 'kv':
      return renderKv(c.data);
    case 'card':
      return renderCard(c.data);
    case 'status':
      return renderStatus(c.data);
    case 'progress':
      return renderProgress(c.data);
    case 'markdown':
      return renderMarkdown(c.data);
    case 'form':
      return renderForm(c.data);
    case 'timeline':
      return renderTimeline(c.data);
    case 'checklist':
      return renderChecklist(c.data, c.id);
    case 'gauge':
      return renderGauge(c.data, c.id);
    case 'countdown':
      return renderCountdown(c.data, c.id);
    case 'image':
      return renderImage(c.data);
    case 'embed':
      return renderEmbed(c.data);
    default:
      return `<pre class="canvas-raw">${escHtml(JSON.stringify(c.data, null, 2))}</pre>`;
  }
}

function renderMetric(data: Record<string, unknown>): string {
  const value = dataStr(data, 'value', '—');
  const unit = dataStr(data, 'unit');
  const change = dataStr(data, 'change');
  const trend = dataStr(data, 'trend');
  const detail = dataStr(data, 'detail');

  const trendIcon =
    trend === 'up' ? 'trending_up' : trend === 'down' ? 'trending_down' : 'trending_flat';
  const trendClass =
    trend === 'up' ? 'canvas-trend-up' : trend === 'down' ? 'canvas-trend-down' : '';

  return `
    <div class="canvas-metric">
      <div class="canvas-metric-value">${escHtml(value)}${unit ? `<span class="canvas-metric-unit">${escHtml(unit)}</span>` : ''}</div>
      ${change ? `<div class="canvas-metric-change ${trendClass}"><span class="ms ms-sm">${trendIcon}</span> ${escHtml(change)}</div>` : ''}
      ${detail ? `<div class="canvas-metric-detail">${escHtml(detail)}</div>` : ''}
    </div>
  `;
}

/** Convert a cell value to a display string — handles objects, arrays, and primitives. */
function cellToString(cell: unknown): string {
  if (cell == null) return '';
  if (typeof cell === 'string' || typeof cell === 'number' || typeof cell === 'boolean')
    return String(cell);
  if (Array.isArray(cell)) return cell.map(cellToString).join(', ');
  if (typeof cell === 'object') {
    // Try common label-like keys first
    const obj = cell as Record<string, unknown>;
    for (const k of ['name', 'label', 'title', 'value', 'text', 'id']) {
      if (typeof obj[k] === 'string' || typeof obj[k] === 'number') return String(obj[k]);
    }
    return JSON.stringify(cell);
  }
  return String(cell);
}

function renderTable(data: Record<string, unknown>): string {
  const rawColumns = dataArr(data, 'columns') as unknown[];
  let rows = dataArr(data, 'rows') as unknown[][];

  if (!rawColumns.length) return '<p class="canvas-muted">No columns defined</p>';

  // Normalise columns: accept strings OR objects like {key, label, header, name}
  const columns = rawColumns.map((c) => {
    if (typeof c === 'string') return { key: c, label: c };
    if (typeof c === 'object' && c !== null) {
      const obj = c as Record<string, unknown>;
      const label = String(obj.label ?? obj.header ?? obj.name ?? obj.key ?? obj.title ?? '');
      const key = String(obj.key ?? obj.field ?? obj.id ?? label);
      return { key, label };
    }
    return { key: String(c), label: String(c) };
  });

  // If rows are objects (not arrays), convert using column keys
  if (
    rows.length > 0 &&
    !Array.isArray(rows[0]) &&
    typeof rows[0] === 'object' &&
    rows[0] !== null
  ) {
    const objRows = rows as unknown as Record<string, unknown>[];
    const normalise = (s: string) => s.toLowerCase().replace(/[\s_-]+/g, '');
    rows = objRows.map((r) => {
      const keyMap = new Map(Object.keys(r).map((k) => [normalise(k), k]));
      return columns.map((col) => {
        // Try exact key first, then normalised fuzzy match
        if (col.key in r) return r[col.key];
        const norm = normalise(col.key);
        const actualKey = keyMap.get(norm);
        if (actualKey !== undefined) return r[actualKey];
        // Try label as fallback
        const normLabel = normalise(col.label);
        const labelKey = keyMap.get(normLabel);
        return labelKey !== undefined ? r[labelKey] : undefined;
      });
    });
  }

  const thead = columns
    .map(
      (c, i) =>
        `<th data-col-index="${i}" class="canvas-th-sortable">${escHtml(c.label)} <span class="canvas-sort-icon"></span></th>`,
    )
    .join('');

  if (!rows.length) {
    return `
      <div class="canvas-table-wrap">
        <table class="canvas-table"><thead><tr>${thead}</tr></thead></table>
        <p class="canvas-muted">No data yet</p>
      </div>`;
  }

  const tbody = rows
    .slice(0, 50)
    .map(
      (row) =>
        `<tr>${(row as unknown[]).map((cell) => `<td>${escHtml(cellToString(cell))}</td>`).join('')}</tr>`,
    )
    .join('');

  return `
    <div class="canvas-table-wrap">
      <table class="canvas-table" data-canvas-sortable>
        <thead><tr>${thead}</tr></thead>
        <tbody>${tbody}</tbody>
      </table>
      ${rows.length > 50 ? `<p class="canvas-muted">${rows.length - 50} more rows…</p>` : ''}
    </div>
  `;
}

function renderChart(data: Record<string, unknown>): string {
  return renderSvgChart(data);
}

function renderLog(data: Record<string, unknown>): string {
  const entries = dataArr(data, 'entries') as Record<string, unknown>[];
  if (!entries.length) return '<p class="canvas-muted">No log entries</p>';

  // Determine which levels are present so we can render filter buttons
  const levels = [...new Set(entries.map((e) => dataStr(e, 'level', 'info')))];

  const rows = entries
    .slice(-100) // last 100 entries
    .map((e) => {
      const time = dataStr(e, 'time');
      const text = dataStr(e, 'text');
      const level = dataStr(e, 'level', 'info');
      return `<div class="canvas-log-entry canvas-log-${escHtml(level)}" data-log-level="${escHtml(level)}">
        ${time ? `<span class="canvas-log-time">${escHtml(time)}</span>` : ''}
        <span class="canvas-log-text">${escHtml(text)}</span>
      </div>`;
    })
    .join('');

  const filters =
    levels.length > 1
      ? `<div class="canvas-log-filters">${levels.map((l) => `<button class="btn btn-xs canvas-log-filter-btn canvas-log-filter-active" data-log-filter="${escHtml(l)}">${escHtml(l)}</button>`).join('')}</div>`
      : '';

  return `<div class="canvas-log">${filters}${rows}</div>`;
}

function renderKv(data: Record<string, unknown>): string {
  const pairs = dataArr(data, 'pairs') as Record<string, unknown>[];
  if (!pairs.length) return '<p class="canvas-muted">No data</p>';

  const rows = pairs
    .map((p) => {
      const key = dataStr(p, 'key');
      const value = dataStr(p, 'value');
      return `<div class="canvas-kv-row"><span class="canvas-kv-key">${escHtml(key)}</span><span class="canvas-kv-value">${escHtml(value)}</span></div>`;
    })
    .join('');

  return `<div class="canvas-kv">${rows}</div>`;
}

function renderCard(data: Record<string, unknown>): string {
  const body = dataStr(data, 'body');
  const actions = dataArr(data, 'actions') as Record<string, unknown>[];

  const actionBtns = actions
    .map((a) => {
      const label = dataStr(a, 'label', 'Action');
      const action = dataStr(a, 'action', label);
      return `<button class="btn btn-sm btn-ghost canvas-action-btn" data-canvas-action="${escHtml(action)}">${escHtml(label)}</button>`;
    })
    .join('');

  return `
    <div class="canvas-card-content">
      <div class="canvas-card-body-text">${formatMarkdown(body)}</div>
      ${actionBtns ? `<div class="canvas-card-actions">${actionBtns}</div>` : ''}
    </div>
  `;
}

function renderStatus(data: Record<string, unknown>): string {
  const icon = dataStr(data, 'icon', 'info');
  const text = dataStr(data, 'text');
  const badge = dataStr(data, 'badge');
  const level = dataStr(data, 'level', 'ok');
  const dotClass =
    level === 'error'
      ? 'status-error'
      : level === 'warning' || level === 'warn'
        ? 'status-warning'
        : level === 'idle'
          ? 'status-idle'
          : '';

  return `
    <div class="canvas-status">
      <span class="canvas-status-dot ${dotClass}"></span>
      <span class="ms ms-sm">${escHtml(icon)}</span>
      <span class="canvas-status-text">${escHtml(text)}</span>
      ${badge ? `<span class="canvas-status-badge">${escHtml(badge)}</span>` : ''}
    </div>
  `;
}

function renderProgress(data: Record<string, unknown>): string {
  const label = dataStr(data, 'label');
  const pct = Math.min(100, Math.max(0, dataNum(data, 'percentage', 0)));
  const eta = dataStr(data, 'eta');

  return `
    <div class="canvas-progress">
      <div class="canvas-progress-label">${escHtml(label)}</div>
      <div class="canvas-progress-bar">
        <div class="canvas-progress-fill canvas-progress-animate" data-target-width="${pct}" style="width: 0%"></div>
      </div>
      <div class="canvas-progress-meta">
        <span>${pct}%</span>
        ${eta ? `<span>ETA: ${escHtml(eta)}</span>` : ''}
      </div>
    </div>
  `;
}

function renderMarkdown(data: Record<string, unknown>): string {
  const text = dataStr(data, 'text') || dataStr(data, 'body');
  return `<div class="canvas-markdown">${formatMarkdown(text)}</div>`;
}

function renderForm(data: Record<string, unknown>): string {
  const fields = dataArr(data, 'fields') as Record<string, unknown>[];
  if (!fields.length) return '<p class="canvas-muted">No form fields</p>';

  const inputs = fields
    .map((f) => {
      const name = dataStr(f, 'name');
      const label = dataStr(f, 'label', name);
      const type = dataStr(f, 'type', 'text');
      const placeholder = dataStr(f, 'placeholder');
      const required = f.required === true;
      return `<div class="canvas-form-field">
        <label>${escHtml(label)}${required ? ' <span class="canvas-form-required">*</span>' : ''}</label>
        <input type="${escHtml(type)}" name="${escHtml(name)}" class="input input-sm" ${placeholder ? `placeholder="${escHtml(placeholder)}"` : ''} ${required ? 'required' : ''} />
      </div>`;
    })
    .join('');

  return `<form class="canvas-form" data-canvas-form>${inputs}<button type="submit" class="btn btn-sm btn-primary canvas-form-submit">Submit</button></form>`;
}

// ── Timeline ──────────────────────────────────────────────────────────

function renderTimeline(data: Record<string, unknown>): string {
  // Accept 'events', 'items', 'entries', 'steps', or 'milestones' as the array key
  let events = dataArr(data, 'events') as Record<string, unknown>[];
  if (!events.length) events = dataArr(data, 'items') as Record<string, unknown>[];
  if (!events.length) events = dataArr(data, 'entries') as Record<string, unknown>[];
  if (!events.length) events = dataArr(data, 'steps') as Record<string, unknown>[];
  if (!events.length) events = dataArr(data, 'milestones') as Record<string, unknown>[];
  if (!events.length) return '<p class="canvas-muted">No timeline events</p>';

  const items = events
    .map((ev, i) => {
      const label = dataStr(ev, 'label', `Step ${i + 1}`);
      const time = dataStr(ev, 'time');
      const detail = dataStr(ev, 'detail');
      const status = dataStr(ev, 'status', 'pending'); // done | active | pending
      const dotClass =
        status === 'done'
          ? 'canvas-tl-done'
          : status === 'active'
            ? 'canvas-tl-active'
            : 'canvas-tl-pending';
      return `<div class="canvas-tl-item ${dotClass}">
        <div class="canvas-tl-dot"></div>
        <div class="canvas-tl-content">
          <div class="canvas-tl-label">${escHtml(label)}</div>
          ${time ? `<div class="canvas-tl-time">${escHtml(time)}</div>` : ''}
          ${detail ? `<div class="canvas-tl-detail">${escHtml(detail)}</div>` : ''}
        </div>
      </div>`;
    })
    .join('');

  return `<div class="canvas-timeline">${items}</div>`;
}

// ── Checklist ─────────────────────────────────────────────────────────

function renderChecklist(data: Record<string, unknown>, componentId: string): string {
  const items = dataArr(data, 'items') as Record<string, unknown>[];
  if (!items.length) return '<p class="canvas-muted">No checklist items</p>';

  const total = items.length;
  const done = items.filter((it) => it.checked === true || it.done === true).length;
  const pct = Math.round((done / total) * 100);

  const rows = items
    .map((it, i) => {
      const label = dataStr(it, 'label', `Item ${i + 1}`);
      const checked = it.checked === true || it.done === true;
      return `<div class="canvas-cl-item${checked ? ' canvas-cl-done' : ''}" data-cl-index="${i}" data-cl-component="${escHtml(componentId)}" role="button" tabindex="0" style="cursor:pointer">
        <span class="canvas-cl-check">${checked ? '&#10003;' : ''}</span>
        <span class="canvas-cl-label">${escHtml(label)}</span>
      </div>`;
    })
    .join('');

  return `
    <div class="canvas-checklist" data-checklist-id="${escHtml(componentId)}">
      <div class="canvas-cl-progress">
        <div class="canvas-cl-progress-bar">
          <div class="canvas-cl-progress-fill" style="width: ${pct}%"></div>
        </div>
        <span class="canvas-cl-progress-text">${done}/${total}</span>
      </div>
      ${rows}
    </div>
  `;
}

// ── Gauge ─────────────────────────────────────────────────────────────

function renderGauge(data: Record<string, unknown>, componentId: string): string {
  const value = dataNum(data, 'value', 0);
  const max = dataNum(data, 'max', 100);
  const min = dataNum(data, 'min', 0);
  const label = dataStr(data, 'label');
  const unit = dataStr(data, 'unit');
  const level = dataStr(data, 'level'); // ok | warning | error

  const pct = Math.min(1, Math.max(0, (value - min) / (max - min || 1)));
  const radius = 60;
  const circumference = Math.PI * radius; // half-circle
  const offset = circumference * (1 - pct);

  const strokeColor =
    level === 'error'
      ? 'var(--kinetic-red, #D4654A)'
      : level === 'warning'
        ? 'var(--kinetic-gold, #C4A962)'
        : 'var(--accent, var(--kinetic-sage, #8FB0A0))';

  return `
    <div class="canvas-gauge" data-gauge-id="${escHtml(componentId)}">
      <svg viewBox="0 0 160 100" class="canvas-gauge-svg">
        <path d="M 20 80 A 60 60 0 0 1 140 80" fill="none" stroke="var(--border)" stroke-width="8" stroke-linecap="round"/>
        <path d="M 20 80 A 60 60 0 0 1 140 80" fill="none" stroke="${strokeColor}" stroke-width="8" stroke-linecap="round"
              stroke-dasharray="${circumference}" stroke-dashoffset="${offset}"
              class="canvas-gauge-arc"/>
      </svg>
      <div class="canvas-gauge-value">${escHtml(String(value))}${unit ? `<span class="canvas-gauge-unit">${escHtml(unit)}</span>` : ''}</div>
      ${label ? `<div class="canvas-gauge-label">${escHtml(label)}</div>` : ''}
    </div>
  `;
}

// ── Countdown ─────────────────────────────────────────────────────────

function renderCountdown(data: Record<string, unknown>, componentId: string): string {
  const target = dataStr(data, 'target'); // ISO date string
  const label = dataStr(data, 'label');
  const format = dataStr(data, 'format', 'dhms'); // d | hms | dhms

  if (!target) return '<p class="canvas-muted">No target date set</p>';

  // Render static placeholder — the animate-on-mount wiring will start the ticker
  return `
    <div class="canvas-countdown" data-countdown-id="${escHtml(componentId)}" data-target="${escHtml(target)}" data-format="${escHtml(format)}">
      ${label ? `<div class="canvas-countdown-label">${escHtml(label)}</div>` : ''}
      <div class="canvas-countdown-digits">
        <div class="canvas-cd-unit"><span class="canvas-cd-num" data-cd="d">--</span><span class="canvas-cd-lbl">DAYS</span></div>
        <div class="canvas-cd-sep">:</div>
        <div class="canvas-cd-unit"><span class="canvas-cd-num" data-cd="h">--</span><span class="canvas-cd-lbl">HRS</span></div>
        <div class="canvas-cd-sep">:</div>
        <div class="canvas-cd-unit"><span class="canvas-cd-num" data-cd="m">--</span><span class="canvas-cd-lbl">MIN</span></div>
        <div class="canvas-cd-sep">:</div>
        <div class="canvas-cd-unit"><span class="canvas-cd-num" data-cd="s">--</span><span class="canvas-cd-lbl">SEC</span></div>
      </div>
    </div>
  `;
}

// ── Image ─────────────────────────────────────────────────────────────

function renderImage(data: Record<string, unknown>): string {
  const src = dataStr(data, 'src') || dataStr(data, 'url');
  const alt = dataStr(data, 'alt', 'Image');
  const caption = dataStr(data, 'caption');

  if (!src) return '<p class="canvas-muted">No image source</p>';

  return `
    <div class="canvas-image">
      <img src="${escHtml(src)}" alt="${escHtml(alt)}" class="canvas-image-img" loading="lazy" />
      ${caption ? `<div class="canvas-image-caption">${escHtml(caption)}</div>` : ''}
    </div>
  `;
}

// ── Embed (Sandboxed HTML/CSS/JS — enables three.js, anime.js, D3, etc.) ──

function renderEmbed(data: Record<string, unknown>): string {
  const html = dataStr(data, 'html');
  const css = dataStr(data, 'css');
  const js = dataStr(data, 'js');
  const height = dataNum(data, 'height', 300);
  const libraries = dataArr(data, 'libraries') as string[];

  if (!html && !js) return '<p class="canvas-muted">No embed content</p>';

  // Build a self-contained HTML document for the sandboxed iframe
  const libTags = libraries.map((lib) => `<script src="${lib}"><\/script>`).join('\n');

  const doc = `<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
${libTags}
<style>
* { margin: 0; padding: 0; box-sizing: border-box; }
body {
  background: transparent;
  color: #e0ddd8;
  font-family: 'JetBrains Mono', 'SF Mono', monospace;
  overflow: hidden;
}
${css || ''}
</style>
</head>
<body>
${html || ''}
${js ? `<script>${js}<\/script>` : ''}
</body>
</html>`;

  // Use srcdoc with a strict sandbox — allow-scripts but NOT allow-same-origin
  // This prevents the iframe from accessing the parent's DOM, cookies, or storage
  const encoded = doc.replace(/"/g, '&quot;');

  return `
    <div class="canvas-embed">
      <iframe
        srcdoc="${encoded}"
        sandbox="allow-scripts"
        class="canvas-embed-frame"
        style="height: ${Math.min(Math.max(height, 100), 800)}px"
        loading="lazy"
      ></iframe>
    </div>
  `;
}

// ── Live Update (incremental DOM patch) ───────────────────────────────

/** Add a new component to the live canvas without full re-render. */
export function pushComponent(id: string, comp: CanvasComponent): void {
  const row: CanvasComponentRow = {
    id,
    session_id: _state.getSessionId(),
    dashboard_id: null,
    agent_id: 'default',
    component_type: comp.component_type,
    title: comp.title,
    data: JSON.stringify(comp.data),
    position: comp.position ? JSON.stringify(comp.position) : null,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
  };
  const parsed = parseComponent(row);
  const all = _state.getComponents();
  all.push(parsed);
  _state.setComponents(all);

  // If the grid exists, append the card; otherwise full render
  const grid = document.querySelector('.canvas-grid');
  if (grid) {
    const style = gridStyle(parsed.position);
    const cardHtml = `
      <div class="canvas-card" data-component-id="${escHtml(parsed.id)}"${style ? ` style="${style}"` : ''}>
        <div class="canvas-card-header">
          <span class="ms ms-sm">${componentIcon(parsed.componentType)}</span>
          <span class="canvas-card-title">${escHtml(parsed.title)}</span>
          <button class="btn btn-ghost btn-xs canvas-card-remove" data-id="${escHtml(parsed.id)}" title="Remove">
            <span class="ms ms-sm">close</span>
          </button>
        </div>
        <div class="canvas-card-body">
          ${renderComponentBody(parsed)}
        </div>
      </div>
    `;
    grid.insertAdjacentHTML('beforeend', cardHtml);
    wireCardRemove(parsed.id);
    activateLiveWidgets();
  } else {
    renderCanvas();
  }
}

/** Update an existing component in the live canvas. */
export function updateComponent(id: string, patch: CanvasComponentPatch): void {
  const all = _state.getComponents();
  const idx = all.findIndex((c) => c.id === id);
  if (idx === -1) return;

  if (patch.title) all[idx].title = patch.title;
  if (patch.data) all[idx].data = patch.data;
  if (patch.position) all[idx].position = patch.position;
  all[idx].updatedAt = new Date().toISOString();
  _state.setComponents(all);

  // Patch the card DOM in-place
  const card = document.querySelector(`[data-component-id="${id}"]`);
  if (card) {
    if (patch.title) {
      const titleEl = card.querySelector('.canvas-card-title');
      if (titleEl) titleEl.textContent = patch.title;
    }
    const body = card.querySelector('.canvas-card-body');
    if (body) {
      body.innerHTML = renderComponentBody(all[idx]);
      activateLiveWidgets(card as HTMLElement);
    }
  }
}

// ── Live Widget Activation (countdowns, gauge animations) ─────────────

/** Active countdown intervals — tracked for cleanup. */
const _countdownIntervals: Map<string, ReturnType<typeof setInterval>> = new Map();

/** Track elements that have already been wired for interactivity. */
const _wiredElements = new WeakSet<Element>();

/**
 * Activate dynamic canvas widgets (countdown tickers, gauge entrance animations).
 * Called after full render and after individual push/update.
 */
function activateLiveWidgets(scope?: HTMLElement): void {
  const root = scope ?? document;

  // ── Countdown tickers ───────────────────────────────────────────────
  root.querySelectorAll<HTMLElement>('.canvas-countdown[data-target]').forEach((el) => {
    const id = el.dataset.countdownId ?? '';
    if (_countdownIntervals.has(id)) return; // already ticking

    const target = new Date(el.dataset.target ?? '').getTime();
    if (isNaN(target)) return;

    const tick = () => {
      const now = Date.now();
      const diff = Math.max(0, target - now);
      const d = Math.floor(diff / 86400000);
      const h = Math.floor((diff % 86400000) / 3600000);
      const m = Math.floor((diff % 3600000) / 60000);
      const s = Math.floor((diff % 60000) / 1000);

      const dEl = el.querySelector<HTMLElement>('[data-cd="d"]');
      const hEl = el.querySelector<HTMLElement>('[data-cd="h"]');
      const mEl = el.querySelector<HTMLElement>('[data-cd="m"]');
      const sEl = el.querySelector<HTMLElement>('[data-cd="s"]');
      if (dEl) dEl.textContent = String(d).padStart(2, '0');
      if (hEl) hEl.textContent = String(h).padStart(2, '0');
      if (mEl) mEl.textContent = String(m).padStart(2, '0');
      if (sEl) sEl.textContent = String(s).padStart(2, '0');

      if (diff === 0) {
        clearInterval(_countdownIntervals.get(id)!);
        _countdownIntervals.delete(id);
        el.classList.add('canvas-countdown-done');
      }
    };

    tick(); // immediate first tick
    _countdownIntervals.set(id, setInterval(tick, 1000));
  });

  // ── Gauge entrance animation ────────────────────────────────────────
  root.querySelectorAll<SVGElement>('.canvas-gauge-arc').forEach((arc) => {
    const parent = arc.closest('.canvas-gauge');
    if (!parent || parent.classList.contains('canvas-gauge-animated')) return;
    parent.classList.add('canvas-gauge-animated');

    const finalOffset = arc.style.strokeDashoffset || arc.getAttribute('stroke-dashoffset') || '0';
    const dashArray = arc.getAttribute('stroke-dasharray') || '188';
    // Start from fully hidden and animate to the target
    arc.setAttribute('stroke-dashoffset', dashArray);
    requestAnimationFrame(() => {
      arc.style.transition = 'stroke-dashoffset 0.8s cubic-bezier(0.34, 1.56, 0.64, 1)';
      arc.style.strokeDashoffset = finalOffset;
    });
  });

  // ── Progress bar entrance animation ─────────────────────────────────
  root.querySelectorAll<HTMLElement>('.canvas-progress-animate').forEach((fill) => {
    if (fill.classList.contains('canvas-progress-animated')) return;
    fill.classList.add('canvas-progress-animated');
    const target = fill.dataset.targetWidth ?? '0';
    requestAnimationFrame(() => {
      fill.style.transition = 'width 0.6s cubic-bezier(0.34, 1.56, 0.64, 1)';
      fill.style.width = `${target}%`;
    });
  });

  // ── Checklist toggle ────────────────────────────────────────────────
  root.querySelectorAll<HTMLElement>('.canvas-cl-item').forEach((item) => {
    if (item.dataset.wired) return;
    item.dataset.wired = '1';
    item.addEventListener('click', () => {
      const isDone = item.classList.toggle('canvas-cl-done');
      const chk = item.querySelector('.canvas-cl-check');
      if (chk) chk.innerHTML = isDone ? '&#10003;' : '';

      // Update progress bar
      const checklist = item.closest('.canvas-checklist');
      if (checklist) {
        const items = checklist.querySelectorAll('.canvas-cl-item');
        const doneCount = checklist.querySelectorAll('.canvas-cl-done').length;
        const pct = Math.round((doneCount / items.length) * 100);
        const fill = checklist.querySelector<HTMLElement>('.canvas-cl-progress-fill');
        const txt = checklist.querySelector<HTMLElement>('.canvas-cl-progress-text');
        if (fill) fill.style.width = `${pct}%`;
        if (txt) txt.textContent = `${doneCount}/${items.length}`;
      }
    });
  });

  // ── Form submit ─────────────────────────────────────────────────────
  root.querySelectorAll<HTMLFormElement>('[data-canvas-form]').forEach((form) => {
    if (_wiredElements.has(form)) return;
    _wiredElements.add(form);
    form.addEventListener('submit', (e) => {
      e.preventDefault();
      const formData = new FormData(form);
      const values: Record<string, string> = {};
      formData.forEach((v, k) => {
        values[k] = String(v);
      });

      // Dispatch event for external handlers (agents, automation)
      document.dispatchEvent(new CustomEvent('canvas:form-submit', { detail: values }));
      showToast('Form submitted', 'success');

      // Visual feedback — briefly highlight submit button
      const btn = form.querySelector<HTMLButtonElement>('.canvas-form-submit');
      if (btn) {
        btn.textContent = 'Submitted ✓';
        btn.disabled = true;
        setTimeout(() => {
          btn.textContent = 'Submit';
          btn.disabled = false;
        }, 2000);
      }
    });
  });

  // ── Card action buttons ─────────────────────────────────────────────
  root.querySelectorAll<HTMLElement>('.canvas-action-btn').forEach((btn) => {
    if (btn.dataset.wired) return;
    btn.dataset.wired = '1';
    btn.addEventListener('click', () => {
      const action = btn.dataset.canvasAction ?? btn.textContent ?? 'action';
      document.dispatchEvent(new CustomEvent('canvas:action', { detail: { action } }));
      showToast(`Action: ${action}`, 'info');
    });
  });

  // ── Table column sorting ────────────────────────────────────────────
  root.querySelectorAll<HTMLTableElement>('[data-canvas-sortable]').forEach((table) => {
    if (_wiredElements.has(table)) return;
    _wiredElements.add(table);
    const headers = table.querySelectorAll<HTMLElement>('.canvas-th-sortable');
    headers.forEach((th) => {
      th.style.cursor = 'pointer';
      th.addEventListener('click', () => {
        const colIdx = parseInt(th.dataset.colIndex ?? '0', 10);
        const tbody = table.querySelector('tbody');
        if (!tbody) return;

        // Determine sort direction
        const asc = th.dataset.sortDir !== 'asc';
        th.dataset.sortDir = asc ? 'asc' : 'desc';

        // Reset other headers
        headers.forEach((h) => {
          if (h !== th) {
            h.dataset.sortDir = '';
            const icon = h.querySelector('.canvas-sort-icon');
            if (icon) icon.textContent = '';
          }
        });
        const icon = th.querySelector('.canvas-sort-icon');
        if (icon) icon.textContent = asc ? ' ▲' : ' ▼';

        // Sort rows
        const rows = Array.from(tbody.querySelectorAll('tr'));
        rows.sort((a, b) => {
          const aText = a.children[colIdx]?.textContent ?? '';
          const bText = b.children[colIdx]?.textContent ?? '';
          const aNum = parseFloat(aText);
          const bNum = parseFloat(bText);
          if (!isNaN(aNum) && !isNaN(bNum)) return asc ? aNum - bNum : bNum - aNum;
          return asc ? aText.localeCompare(bText) : bText.localeCompare(aText);
        });
        rows.forEach((r) => tbody.appendChild(r));
      });
    });
  });

  // ── Log level filtering ─────────────────────────────────────────────
  root.querySelectorAll<HTMLElement>('.canvas-log-filter-btn').forEach((btn) => {
    if (btn.dataset.wired) return;
    btn.dataset.wired = '1';
    btn.addEventListener('click', () => {
      btn.classList.toggle('canvas-log-filter-active');
      const logContainer = btn.closest('.canvas-log');
      if (!logContainer) return;

      // Get all active filter levels
      const activeFilters = Array.from(
        logContainer.querySelectorAll<HTMLElement>('.canvas-log-filter-active'),
      ).map((b) => b.dataset.logFilter ?? '');

      // Show/hide entries based on active filters
      logContainer.querySelectorAll<HTMLElement>('.canvas-log-entry').forEach((entry) => {
        const entryLevel = entry.dataset.logLevel ?? 'info';
        entry.style.display =
          activeFilters.length === 0 || activeFilters.includes(entryLevel) ? '' : 'none';
      });
    });
  });
}

/** Clean up countdown intervals (call when leaving canvas view). */
export function cleanupLiveWidgets(): void {
  for (const [, interval] of _countdownIntervals) {
    clearInterval(interval);
  }
  _countdownIntervals.clear();
}

// ── Event Wiring ──────────────────────────────────────────────────────

function wireEvents(): void {
  // Clear button
  const clearBtn = $('canvas-clear-btn');
  if (clearBtn) {
    clearBtn.addEventListener('click', async () => {
      const sid = _state.getSessionId();
      if (!sid) return;
      try {
        await pawEngine.canvasClearSession(sid);
        _state.setComponents([]);
        renderCanvas();
        showToast('Canvas cleared', 'success');
      } catch (e) {
        showToast('Failed to clear canvas', 'error');
        console.error('[canvas] Clear failed:', e);
      }
    });
  }

  // Save button — save current session canvas as a named dashboard
  const saveBtn = $('canvas-save-btn');
  if (saveBtn) {
    saveBtn.addEventListener('click', async () => {
      if (_state.getDashboardId()) {
        showToast('Dashboard already saved', 'info');
        return;
      }
      const name = await promptModal('Save Dashboard', 'Dashboard name');
      if (!name) return;
      try {
        await _state.onSave(name);
        showToast(`Dashboard "${name}" saved`, 'success');
      } catch (e) {
        showToast('Failed to save dashboard', 'error');
        console.error('[canvas] Save failed:', e);
      }
    });
  }

  // Rename button
  const renameBtn = $('canvas-rename-btn');
  if (renameBtn) {
    renameBtn.addEventListener('click', async () => {
      const current = _state.getDashboardName() ?? '';
      const name = await promptModal('Rename Dashboard', current);
      if (!name) return;
      try {
        await _state.onRename(name);
        showToast(`Renamed to "${name}"`, 'success');
      } catch (e) {
        showToast('Failed to rename', 'error');
        console.error('[canvas] Rename failed:', e);
      }
    });
  }

  // Pin button
  const pinBtn = $('canvas-pin-btn');
  if (pinBtn) {
    pinBtn.addEventListener('click', async () => {
      try {
        await _state.onPin();
      } catch (e) {
        showToast('Failed to toggle pin', 'error');
        console.error('[canvas] Pin failed:', e);
      }
    });
  }

  // Pop-out button
  const popBtn = $('canvas-popout-btn');
  if (popBtn) {
    popBtn.addEventListener('click', async () => {
      try {
        await _state.onPopOut();
      } catch (e) {
        showToast('Failed to pop out', 'error');
        console.error('[canvas] Pop-out failed:', e);
      }
    });
  }

  // Delete dashboard button
  const deleteBtn = $('canvas-delete-btn');
  if (deleteBtn) {
    deleteBtn.addEventListener('click', async () => {
      const name = _state.getDashboardName() ?? 'this dashboard';
      if (!confirm(`Delete "${name}" and all its components? This cannot be undone.`)) return;
      try {
        await _state.onDelete();
        showToast(`"${name}" deleted`, 'success');
      } catch (e) {
        showToast('Failed to delete dashboard', 'error');
        console.error('[canvas] Delete failed:', e);
      }
    });
  }

  // Open dashboard picker
  const openBtn = $('canvas-open-btn');
  if (openBtn) {
    openBtn.addEventListener('click', () => toggleDashboardPicker());
  }

  // Per-card remove buttons
  _state.getComponents().forEach((c) => wireCardRemove(c.id));
}

/** Toggle the dashboard picker dropdown. */
async function toggleDashboardPicker(): Promise<void> {
  const picker = $('canvas-dashboard-picker');
  if (!picker) return;

  // Close if already open
  if (picker.style.display !== 'none') {
    picker.style.display = 'none';
    return;
  }

  try {
    const dashboards = await pawEngine.listDashboards();
    const templates = await pawEngine.listTemplates();

    if (!dashboards.length && !templates.length) {
      picker.innerHTML = `<div class="canvas-picker-empty">No saved dashboards or templates yet</div>`;
      picker.style.display = 'block';
      return;
    }

    let html = '';

    if (dashboards.length) {
      html += `<div class="canvas-picker-section">
        <div class="canvas-picker-section-label"><span class="ms ms-sm">folder</span> Saved Dashboards</div>
        ${dashboards
          .map(
            (d) => `
          <button class="canvas-picker-item" data-dashboard-id="${escHtml(d.id)}">
            <span class="ms ms-sm">${escHtml(d.icon || 'dashboard')}</span>
            <span class="canvas-picker-name">${escHtml(d.name)}</span>
            ${d.pinned ? '<span class="ms ms-xs">push_pin</span>' : ''}
          </button>
        `,
          )
          .join('')}
      </div>`;
    }

    if (templates.length) {
      html += `<div class="canvas-picker-section">
        <div class="canvas-picker-section-label"><span class="ms ms-sm">auto_awesome</span> Templates</div>
        ${templates
          .map(
            (t) => `
          <button class="canvas-picker-item canvas-picker-template" data-template-id="${escHtml(t.id)}">
            <span class="ms ms-sm">${escHtml(t.icon || 'widgets')}</span>
            <span class="canvas-picker-name">${escHtml(t.name)}</span>
            <span class="canvas-picker-desc">${escHtml(t.description)}</span>
          </button>
        `,
          )
          .join('')}
      </div>`;
    }

    picker.innerHTML = html;
    picker.style.display = 'block';

    // Wire dashboard items
    picker.querySelectorAll<HTMLElement>('[data-dashboard-id]').forEach((btn) => {
      btn.addEventListener('click', async () => {
        const id = btn.dataset.dashboardId ?? '';
        picker.style.display = 'none';
        await _state.onOpenDashboard(id);
      });
    });

    // Wire template items (future: create from template)
    picker.querySelectorAll<HTMLElement>('[data-template-id]').forEach((btn) => {
      btn.addEventListener('click', () => {
        picker.style.display = 'none';
        showToast('Template support coming soon', 'info');
      });
    });

    // Close on outside click
    const dismiss = (e: MouseEvent) => {
      if (
        !picker.contains(e.target as Node) &&
        (e.target as HTMLElement)?.id !== 'canvas-open-btn'
      ) {
        picker.style.display = 'none';
        document.removeEventListener('click', dismiss);
      }
    };
    setTimeout(() => document.addEventListener('click', dismiss), 0);
  } catch (e) {
    console.error('[canvas] Failed to load dashboard picker:', e);
    showToast('Failed to load dashboards', 'error');
  }
}

function wireCardRemove(componentId: string): void {
  const btn = document.querySelector(`.canvas-card-remove[data-id="${componentId}"]`);
  if (btn) {
    btn.addEventListener('click', async () => {
      try {
        await pawEngine.canvasDeleteComponent(componentId);
        const all = _state.getComponents().filter((c) => c.id !== componentId);
        _state.setComponents(all);
        const card = document.querySelector(`[data-component-id="${componentId}"]`);
        card?.remove();
        if (!all.length) renderCanvas(); // switch to empty state
        showToast('Component removed', 'success');
      } catch (e) {
        showToast('Failed to remove component', 'error');
        console.error('[canvas] Remove failed:', e);
      }
    });
  }
}
