// My Skills — Summary Bar
// Renders the at-a-glance stats bar at the top of My Skills workspace.

import type { EngineSkillStatus, McpServerStatus } from '../../engine';
import { msIcon } from './atoms';

export interface SummaryData {
  skills: EngineSkillStatus[];
  mcpStatuses: McpServerStatus[];
}

export function renderSummaryBar(data: SummaryData): string {
  const enabled = data.skills.filter((s) => s.enabled);
  const needSetup = data.skills.filter(
    (s) => s.enabled && !s.is_ready && s.missing_credentials.length > 0,
  );
  const mcpConnected = data.mcpStatuses.filter((s) => s.connected);
  const widgets = data.skills.filter((s) => s.enabled && s.has_widget);

  return `
  <div class="skills-summary-items">
    <span class="skills-summary-item">
      ${msIcon('check_circle')} <strong>${enabled.length}</strong> active
    </span>
    ${
      needSetup.length > 0
        ? `<span class="skills-summary-item skills-summary-warning">
            ${msIcon('warning')} <strong>${needSetup.length}</strong> need setup
          </span>`
        : ''
    }
    <span class="skills-summary-item">
      ${msIcon('dns')} <strong>${mcpConnected.length}</strong> MCP server${mcpConnected.length !== 1 ? 's' : ''}
    </span>
    ${
      widgets.length > 0
        ? `<span class="skills-summary-item">
            ${msIcon('dashboard')} <strong>${widgets.length}</strong> widget${widgets.length !== 1 ? 's' : ''}
          </span>`
        : ''
    }
  </div>`;
}

/** Update tab counts in the tab bar. */
export function updateTabCounts(data: SummaryData & { integrationCount: number; toolCount: number; extensionCount: number }): void {
  const setCount = (id: string, count: number) => {
    const el = document.getElementById(id);
    if (el) el.textContent = count > 0 ? `(${count})` : '';
  };

  const activeCount = data.skills.filter((s) => s.enabled).length +
    data.mcpStatuses.filter((s) => s.connected).length;

  setCount('tab-count-active', activeCount);
  setCount('tab-count-integrations', data.integrationCount);
  setCount('tab-count-tools', data.toolCount);
  setCount('tab-count-extensions', data.extensionCount);
}
