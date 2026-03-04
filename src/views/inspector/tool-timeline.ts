// Inspector — Tool Call Timeline Renderer
// Renders a chronological list of tool calls with duration bars,
// success/failure indicators, and expandable output previews.

import { escHtml } from '../../components/helpers';
import {
  type InspectorToolEntry,
  formatDuration,
  toolStatusIcon,
  toolStatusClass,
  tierIcon,
} from './atoms';

// ── Render ────────────────────────────────────────────────────────────

/** Render the tool call timeline section. */
export function renderToolTimeline(tools: InspectorToolEntry[], totalDurationMs: number): string {
  if (tools.length === 0) return '';

  const totalTools = tools.length;
  const finishedTools = tools.filter((t) => t.finishedAt !== null).length;
  const failedTools = tools.filter((t) => t.success === false).length;
  const totalToolTime = tools.reduce((sum, t) => sum + (t.durationMs ?? 0), 0);

  return `
    <div class="inspector-section inspector-timeline">
      <details class="inspector-collapsible" open>
        <summary class="inspector-section-title">
          <span class="ms ms-xs">timeline</span>
          Tool Calls (${finishedTools}/${totalTools}${failedTools ? `, ${failedTools} failed` : ''}, ${formatDuration(totalToolTime)} total)
        </summary>
        <div class="inspector-timeline-list">
          ${tools.map((t) => renderToolEntry(t, totalDurationMs)).join('')}
        </div>
      </details>
    </div>
  `;
}

// ── Individual Entry ──────────────────────────────────────────────────

function renderToolEntry(entry: InspectorToolEntry, totalDuration: number): string {
  const statusIcon = toolStatusIcon(entry);
  const statusClass = toolStatusClass(entry);
  const tIcon = tierIcon(entry.tier);
  const dur = entry.durationMs !== null ? formatDuration(entry.durationMs) : '…';

  // Duration bar width proportional to total run time
  const barWidth =
    totalDuration > 0 && entry.durationMs
      ? Math.max(2, Math.round((entry.durationMs / totalDuration) * 100))
      : 0;

  const hasOutput = entry.outputPreview && entry.outputPreview.length > 0;

  return `
    <div class="inspector-tool-entry ${statusClass}">
      <div class="inspector-tool-row">
        <span class="ms ms-xs inspector-tool-status">${statusIcon}</span>
        <span class="inspector-tool-name">${escHtml(entry.name)}</span>
        <span class="ms ms-xs inspector-tool-tier" title="${entry.tier ?? 'unknown'}">${tIcon}</span>
        ${entry.autoApproved ? '<span class="inspector-tool-auto" title="Auto-approved">auto</span>' : ''}
        <span class="inspector-tool-round">R${entry.round}</span>
        <span class="inspector-tool-dur">${dur}</span>
      </div>
      ${barWidth > 0 ? `<div class="inspector-tool-bar"><div class="inspector-tool-bar-fill" style="width:${barWidth}%"></div></div>` : ''}
      ${hasOutput ? renderOutputPreview(entry) : ''}
    </div>
  `;
}

// ── Output Preview ────────────────────────────────────────────────────

function renderOutputPreview(entry: InspectorToolEntry): string {
  if (!entry.outputPreview) return '';
  return `
    <details class="inspector-tool-output">
      <summary class="inspector-tool-output-toggle">Output</summary>
      <pre class="inspector-tool-output-pre">${escHtml(entry.outputPreview)}</pre>
    </details>
  `;
}
