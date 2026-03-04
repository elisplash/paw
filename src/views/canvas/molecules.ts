// Canvas View — DOM rendering + IPC
// Renders a bento-grid of agent-generated components.

import { pawEngine } from '../../engine';
import { $, escHtml, promptModal } from '../../components/helpers';
import { showToast } from '../../components/toast';
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

function renderTable(data: Record<string, unknown>): string {
  const columns = dataArr(data, 'columns') as string[];
  const rows = dataArr(data, 'rows') as unknown[][];

  if (!columns.length) return '<p class="canvas-muted">No columns defined</p>';

  const thead = columns.map((c) => `<th>${escHtml(String(c))}</th>`).join('');
  const tbody = rows
    .slice(0, 50) // cap at 50 rows for performance
    .map(
      (row) =>
        `<tr>${(row as unknown[]).map((cell) => `<td>${escHtml(String(cell ?? ''))}</td>`).join('')}</tr>`,
    )
    .join('');

  return `
    <div class="canvas-table-wrap">
      <table class="canvas-table">
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

  const rows = entries
    .slice(-100) // last 100 entries
    .map((e) => {
      const time = dataStr(e, 'time');
      const text = dataStr(e, 'text');
      const level = dataStr(e, 'level', 'info');
      return `<div class="canvas-log-entry canvas-log-${escHtml(level)}">
        ${time ? `<span class="canvas-log-time">${escHtml(time)}</span>` : ''}
        <span class="canvas-log-text">${escHtml(text)}</span>
      </div>`;
    })
    .join('');

  return `<div class="canvas-log">${rows}</div>`;
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
      return `<button class="btn btn-sm btn-ghost canvas-action-btn">${escHtml(label)}</button>`;
    })
    .join('');

  return `
    <div class="canvas-card-content">
      <div class="canvas-card-body-text">${escHtml(body)}</div>
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
        <div class="canvas-progress-fill" style="width: ${pct}%"></div>
      </div>
      <div class="canvas-progress-meta">
        <span>${pct}%</span>
        ${eta ? `<span>ETA: ${escHtml(eta)}</span>` : ''}
      </div>
    </div>
  `;
}

function renderMarkdown(data: Record<string, unknown>): string {
  // Simple markdown rendering — just escape and preserve whitespace.
  // Full markdown can be added later without a library.
  const text = dataStr(data, 'text') || dataStr(data, 'body');
  return `<div class="canvas-markdown"><pre>${escHtml(text)}</pre></div>`;
}

function renderForm(data: Record<string, unknown>): string {
  const fields = dataArr(data, 'fields') as Record<string, unknown>[];
  if (!fields.length) return '<p class="canvas-muted">No form fields</p>';

  const inputs = fields
    .map((f) => {
      const name = dataStr(f, 'name');
      const label = dataStr(f, 'label', name);
      const type = dataStr(f, 'type', 'text');
      return `<div class="canvas-form-field">
        <label>${escHtml(label)}</label>
        <input type="${escHtml(type)}" name="${escHtml(name)}" class="input input-sm" />
      </div>`;
    })
    .join('');

  return `<div class="canvas-form">${inputs}<button class="btn btn-sm btn-primary canvas-form-submit">Submit</button></div>`;
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
    if (body) body.innerHTML = renderComponentBody(all[idx]);
  }
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
