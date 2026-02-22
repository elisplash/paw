// Extension View — Custom sidebar tab renderer for Extension-tier skills (Phase F.6).
// Renders skill_outputs as a full-page view (not just a widget card)
// and shows the skill's persistent KV storage.

import {
  pawEngine,
  type TomlSkillEntry,
  type SkillOutput,
  type SkillStorageItem,
} from '../../engine';
import { showToast } from '../../components/toast';
import { renderSkillWidgetCard } from '../../components/molecules/skill-widget';

// ── Types ──────────────────────────────────────────────────────────────

interface ExtensionTab {
  skillId: string;
  label: string;
  icon: string;
  layout: string;
}

// ── State ──────────────────────────────────────────────────────────────

let _extensionTabs: ExtensionTab[] = [];

// ── Public API ─────────────────────────────────────────────────────────

/**
 * Build extension tabs from TOML skills that have `[view]` sections.
 * Call this after loading TOML skills to register sidebar tabs.
 */
export function buildExtensionTabs(tomlSkills: TomlSkillEntry[]): ExtensionTab[] {
  _extensionTabs = tomlSkills
    .filter((s) => s.has_view && s.definition.enabled)
    .map((s) => ({
      skillId: s.definition.id,
      label: s.view_label || s.definition.name,
      icon: s.view_icon || 'extension',
      layout: 'widget',
    }));
  return _extensionTabs;
}

/** Get the list of registered extension tabs. */
export function getExtensionTabs(): ExtensionTab[] {
  return _extensionTabs;
}

/**
 * Render a full extension view page for a given skill ID.
 * Fetches skill outputs and storage, renders them in a full tab layout.
 */
export async function renderExtensionView(container: HTMLElement, skillId: string): Promise<void> {
  const tab = _extensionTabs.find((t) => t.skillId === skillId);
  const label = tab?.label || skillId;

  container.innerHTML = `<div style="display:flex;align-items:center;gap:8px;padding:12px;color:var(--text-muted)">
    <span class="wa-spinner"></span> Loading ${label}...
  </div>`;

  try {
    const [outputs, storage] = await Promise.all([
      pawEngine.listSkillOutputs(skillId),
      pawEngine.skillStoreList(skillId),
    ]);

    container.innerHTML = renderExtensionPage(skillId, label, outputs, storage);
    bindExtensionEvents(container, skillId);
  } catch (err) {
    container.innerHTML = `<div style="padding:24px">
      <p style="color:var(--accent-danger)">Failed to load extension: ${String(err)}</p>
    </div>`;
  }
}

// ── Internal renderers ─────────────────────────────────────────────────

function renderExtensionPage(
  skillId: string,
  label: string,
  outputs: SkillOutput[],
  storage: SkillStorageItem[],
): string {
  const widgetSection =
    outputs.length > 0
      ? `<div class="extension-widgets">
          <h3 style="font-size:15px;font-weight:600;margin:0 0 12px;display:flex;align-items:center;gap:6px">
            <span class="ms ms-sm">dashboard</span> Live Data
          </h3>
          <div class="extension-widget-grid">
            ${outputs.map((o) => renderSkillWidgetCard(o)).join('')}
          </div>
        </div>`
      : `<div style="text-align:center;padding:32px;color:var(--text-muted)">
          <span class="ms" style="font-size:48px;opacity:0.3">dashboard</span>
          <p style="margin:8px 0 0">No widget data yet. The agent will populate this when it runs.</p>
        </div>`;

  const storageSection =
    storage.length > 0
      ? `<div class="extension-storage" style="margin-top:24px">
          <h3 style="font-size:15px;font-weight:600;margin:0 0 12px;display:flex;align-items:center;gap:6px">
            <span class="ms ms-sm">database</span> Persistent Storage
            <span style="font-size:12px;font-weight:400;color:var(--text-muted)">(${storage.length} entries)</span>
          </h3>
          <div class="extension-storage-table-wrap" style="overflow-x:auto">
            <table class="extension-storage-table">
              <thead>
                <tr>
                  <th>Key</th>
                  <th>Value</th>
                  <th>Updated</th>
                </tr>
              </thead>
              <tbody>
                ${storage.map((item) => renderStorageRow(item)).join('')}
              </tbody>
            </table>
          </div>
        </div>`
      : '';

  return `
  <div class="extension-view-page" data-skill-id="${skillId}">
    <div class="extension-view-header">
      <h2 style="margin:0;font-size:20px;font-weight:700">${label}</h2>
      <div style="display:flex;gap:8px">
        <button class="btn btn-ghost btn-sm extension-refresh-btn">
          <span class="ms ms-sm">refresh</span> Refresh
        </button>
      </div>
    </div>
    ${widgetSection}
    ${storageSection}
  </div>`;
}

function renderStorageRow(item: SkillStorageItem): string {
  // Try to detect JSON values for nicer display
  let displayValue = item.value;
  try {
    const parsed = JSON.parse(item.value);
    if (typeof parsed === 'object') {
      displayValue = JSON.stringify(parsed, null, 2);
    }
  } catch {
    // Not JSON, use as-is
  }

  const isLong = displayValue.length > 100;
  const preview = isLong ? `${displayValue.substring(0, 100)}...` : displayValue;

  return `<tr>
    <td style="font-weight:600;font-family:monospace;font-size:12px">${item.key}</td>
    <td style="max-width:400px;overflow:hidden;text-overflow:ellipsis;white-space:${isLong ? 'nowrap' : 'pre-wrap'};font-size:12px">${preview}</td>
    <td style="font-size:11px;color:var(--text-muted);white-space:nowrap">${item.updated_at}</td>
  </tr>`;
}

// ── Event binding ──────────────────────────────────────────────────────

function bindExtensionEvents(container: HTMLElement, skillId: string): void {
  container.querySelector('.extension-refresh-btn')?.addEventListener('click', async () => {
    await renderExtensionView(container, skillId);
    showToast('Extension view refreshed', 'success');
  });
}
