// ─────────────────────────────────────────────────────────────────────────────
// Flow Execution Engine — Chat Reporter
// Renders flow execution progress into the chat messages area.
// Shows step-by-step progress with status indicators.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowExecEvent, FlowOutputEntry } from './executor-atoms';
import { type FlowNodeKind, NODE_DEFAULTS } from './atoms';

// ── Types ──────────────────────────────────────────────────────────────────

export interface FlowChatReporterController {
  /** Handle an execution event — render into chat */
  handleEvent: (event: FlowExecEvent) => void;
  /** Get the container element for this run's report */
  getElement: () => HTMLElement;
  /** Destroy the reporter */
  destroy: () => void;
}

// ── Status Icons ───────────────────────────────────────────────────────────

const STATUS_ICONS: Record<string, string> = {
  idle: 'radio_button_unchecked',
  running: 'pending',
  success: 'check_circle',
  error: 'error',
  paused: 'pause_circle',
};

const STATUS_COLORS: Record<string, string> = {
  idle: 'var(--text-muted)',
  running: 'var(--accent)',
  success: 'var(--success)',
  error: 'var(--error)',
  paused: 'var(--warning)',
};

// ── Factory ────────────────────────────────────────────────────────────────

/**
 * Create a chat reporter that renders flow execution events into a DOM element.
 * The element can be appended to the chat messages area.
 */
export function createFlowChatReporter(): FlowChatReporterController {
  const root = document.createElement('div');
  root.className = 'flow-report';

  // Header (set on run-start)
  const header = document.createElement('div');
  header.className = 'flow-report-header';
  root.appendChild(header);

  // Steps container
  const stepsContainer = document.createElement('div');
  stepsContainer.className = 'flow-report-steps';
  root.appendChild(stepsContainer);

  // Summary (set on run-complete)
  const summary = document.createElement('div');
  summary.className = 'flow-report-summary';
  summary.style.display = 'none';
  root.appendChild(summary);

  // Track step elements by nodeId
  const stepElements = new Map<string, HTMLElement>();

  function handleEvent(event: FlowExecEvent): void {
    switch (event.type) {
      case 'run-start':
        renderRunStart(event.graphName, event.totalSteps);
        break;
      case 'step-start':
        renderStepStart(event.nodeId, event.nodeLabel, event.nodeKind, event.stepIndex, event.runId);
        break;
      case 'step-progress':
        renderStepProgress(event.nodeId, event.delta);
        break;
      case 'step-complete':
        renderStepComplete(event.nodeId, event.output, event.durationMs);
        break;
      case 'step-error':
        renderStepError(event.nodeId, event.error, event.durationMs);
        break;
      case 'run-complete':
        renderRunComplete(event.status, event.totalDurationMs, event.outputLog);
        break;
      case 'run-paused':
        renderPaused(event.stepIndex);
        break;
      case 'run-aborted':
        renderAborted();
        break;
    }
  }

  function renderRunStart(name: string, totalSteps: number): void {
    header.innerHTML = `
      <div class="flow-report-title">
        <span class="ms" style="color:var(--accent)">account_tree</span>
        <strong>Running Flow: ${escHtml(name)}</strong>
        <span class="flow-report-step-count">${totalSteps} steps</span>
      </div>
      <div class="flow-report-progress">
        <div class="flow-report-progress-bar"></div>
      </div>
    `;
    stepsContainer.innerHTML = '';
    summary.style.display = 'none';
    stepElements.clear();
  }

  function renderStepStart(
    nodeId: string,
    label: string,
    kind: FlowNodeKind,
    stepIndex: number,
    _runId: string,
  ): void {
    const defaults = NODE_DEFAULTS[kind];
    const stepEl = document.createElement('div');
    stepEl.className = 'flow-report-step flow-report-step-running';
    stepEl.dataset.nodeId = nodeId;

    stepEl.innerHTML = `
      <div class="flow-report-step-header">
        <span class="ms flow-report-step-status" style="color:${STATUS_COLORS.running}">${STATUS_ICONS.running}</span>
        <span class="ms flow-report-step-icon" style="color:${defaults.color}">${defaults.icon}</span>
        <span class="flow-report-step-label">${escHtml(label)}</span>
        <span class="flow-report-step-kind">${kind}</span>
        <span class="flow-report-step-duration"></span>
      </div>
      <div class="flow-report-step-output"></div>
    `;

    stepsContainer.appendChild(stepEl);
    stepElements.set(nodeId, stepEl);

    // Update progress bar
    updateProgressBar(stepIndex + 1);

    // Scroll into view
    stepEl.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
  }

  function renderStepProgress(nodeId: string, delta: string): void {
    const stepEl = stepElements.get(nodeId);
    if (!stepEl) return;

    const outputEl = stepEl.querySelector('.flow-report-step-output');
    if (!outputEl) return;

    // Append delta text
    outputEl.textContent = (outputEl.textContent ?? '') + delta;

    // Scroll
    stepEl.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
  }

  function renderStepComplete(nodeId: string, output: string, durationMs: number): void {
    const stepEl = stepElements.get(nodeId);
    if (!stepEl) return;

    stepEl.classList.remove('flow-report-step-running');
    stepEl.classList.add('flow-report-step-success');

    // Update status icon
    const statusIcon = stepEl.querySelector('.flow-report-step-status');
    if (statusIcon) {
      (statusIcon as HTMLElement).style.color = STATUS_COLORS.success;
      statusIcon.textContent = STATUS_ICONS.success;
    }

    // Update duration
    const durationEl = stepEl.querySelector('.flow-report-step-duration');
    if (durationEl) durationEl.textContent = formatDuration(durationMs);

    // Update output (may already have streamed content)
    const outputEl = stepEl.querySelector('.flow-report-step-output');
    if (outputEl && output) {
      // Truncate long outputs
      const preview = output.length > 500 ? `${output.slice(0, 500)}…` : output;
      outputEl.textContent = preview;
    }
  }

  function renderStepError(nodeId: string, error: string, durationMs: number): void {
    const stepEl = stepElements.get(nodeId);
    if (!stepEl) return;

    stepEl.classList.remove('flow-report-step-running');
    stepEl.classList.add('flow-report-step-error');

    const statusIcon = stepEl.querySelector('.flow-report-step-status');
    if (statusIcon) {
      (statusIcon as HTMLElement).style.color = STATUS_COLORS.error;
      statusIcon.textContent = STATUS_ICONS.error;
    }

    const durationEl = stepEl.querySelector('.flow-report-step-duration');
    if (durationEl) durationEl.textContent = formatDuration(durationMs);

    const outputEl = stepEl.querySelector('.flow-report-step-output');
    if (outputEl) {
      outputEl.innerHTML = `<span class="flow-report-error">${escHtml(error)}</span>`;
    }
  }

  function renderRunComplete(
    status: string,
    totalDurationMs: number,
    outputLog: FlowOutputEntry[],
  ): void {
    const successCount = outputLog.filter((e) => e.status === 'success').length;
    const errorCount = outputLog.filter((e) => e.status === 'error').length;

    const statusColor = status === 'success' ? STATUS_COLORS.success : STATUS_COLORS.error;
    const statusIcon = status === 'success' ? STATUS_ICONS.success : STATUS_ICONS.error;

    summary.style.display = '';
    summary.innerHTML = `
      <div class="flow-report-summary-row">
        <span class="ms" style="color:${statusColor}">${statusIcon}</span>
        <strong>${status === 'success' ? 'Flow completed' : 'Flow finished with errors'}</strong>
        <span class="flow-report-summary-duration">${formatDuration(totalDurationMs)}</span>
      </div>
      <div class="flow-report-summary-stats">
        ${successCount > 0 ? `<span class="flow-report-stat-ok">${successCount} passed</span>` : ''}
        ${errorCount > 0 ? `<span class="flow-report-stat-err">${errorCount} failed</span>` : ''}
      </div>
    `;

    // Fill progress bar to 100%
    const progressBar = header.querySelector('.flow-report-progress-bar') as HTMLElement | null;
    if (progressBar) {
      progressBar.style.width = '100%';
      progressBar.style.background = statusColor;
    }
  }

  function renderPaused(_stepIndex: number): void {
    const progressBar = header.querySelector('.flow-report-progress-bar') as HTMLElement | null;
    if (progressBar) progressBar.style.background = STATUS_COLORS.paused;
  }

  function renderAborted(): void {
    summary.style.display = '';
    summary.innerHTML = `
      <div class="flow-report-summary-row">
        <span class="ms" style="color:${STATUS_COLORS.error}">cancel</span>
        <strong>Flow aborted</strong>
      </div>
    `;
  }

  function updateProgressBar(completedSteps: number): void {
    const progressBar = header.querySelector('.flow-report-progress-bar') as HTMLElement | null;
    const stepCount = header.querySelector('.flow-report-step-count');
    const total = stepCount ? parseInt(stepCount.textContent ?? '0') : 0;
    if (progressBar && total > 0) {
      progressBar.style.width = `${(completedSteps / total) * 100}%`;
    }
  }

  return {
    handleEvent,
    getElement: () => root,
    destroy: () => {
      root.remove();
      stepElements.clear();
    },
  };
}

// ── Helpers ────────────────────────────────────────────────────────────────

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  return `${(ms / 60_000).toFixed(1)}m`;
}

function escHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}
