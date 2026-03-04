// Inspector — Drawer component (state management + panel shell)
// Renders the collapsible Inspector panel alongside the chat view.

import { $ } from '../../components/helpers';
import {
  type InspectorState,
  type InspectorToolEntry,
  createInspectorState,
  truncateOutput,
  formatTokens,
  contextPercent,
  roundPercent,
  contextLevelClass,
} from './atoms';
import { renderToolTimeline } from './tool-timeline';
import { renderThinkingTrace } from './thinking-trace';

// ── State ─────────────────────────────────────────────────────────────

let _state: InspectorState = createInspectorState();

/** Get current inspector state (read-only snapshot). */
export function getInspectorState(): Readonly<InspectorState> {
  return _state;
}

// ── Toggle ────────────────────────────────────────────────────────────

/** Toggle the Inspector panel open/closed. */
export function toggleInspector(): void {
  _state.isOpen = !_state.isOpen;
  const panel = $('inspector-panel');
  if (panel) {
    panel.classList.toggle('inspector-open', _state.isOpen);
  }
  // Also toggle body class for layout shifts
  document.body.classList.toggle('inspector-visible', _state.isOpen);
  if (_state.isOpen) renderInspector();
}

/** Open the Inspector panel. */
export function openInspector(): void {
  if (!_state.isOpen) toggleInspector();
}

/** Close the Inspector panel. */
export function closeInspector(): void {
  if (_state.isOpen) toggleInspector();
}

/** Check if the Inspector is open. */
export function isInspectorOpen(): boolean {
  return _state.isOpen;
}

// ── State Mutations (called from bridge/event handler) ────────────────

/** Reset state for a new agent run. */
export function inspectorNewRun(sessionId: string, runId: string): void {
  _state = createInspectorState();
  _state.sessionId = sessionId;
  _state.runId = runId;
  _state.isOpen = document.body.classList.contains('inspector-visible');
  _state.isRunning = true;
  _state.startedAt = Date.now();
  if (_state.isOpen) renderInspector();
}

/** Record a tool request. */
export function inspectorToolRequest(
  callId: string,
  toolName: string,
  round: number,
  tier: string | null,
  autoApproved: boolean,
  loadedTools?: string[],
  contextTokens?: number,
): void {
  _state.currentRound = Math.max(_state.currentRound, round);

  const entry: InspectorToolEntry = {
    callId,
    name: toolName,
    round,
    startedAt: Date.now(),
    finishedAt: null,
    durationMs: null,
    success: null,
    outputPreview: null,
    tier,
    autoApproved,
  };
  _state.tools.push(entry);

  if (loadedTools) _state.loadedTools = loadedTools;
  if (contextTokens !== undefined) _state.contextTokens = contextTokens;

  if (_state.isOpen) renderInspector();
}

/** Record a tool result. */
export function inspectorToolResult(
  callId: string,
  output: string,
  success: boolean,
  durationMs?: number,
): void {
  const entry = _state.tools.find((t) => t.callId === callId);
  if (entry) {
    entry.finishedAt = Date.now();
    entry.success = success;
    entry.outputPreview = truncateOutput(output);
    entry.durationMs = durationMs ?? entry.finishedAt - entry.startedAt;
  }
  if (_state.isOpen) renderInspector();
}

/** Record a thinking delta. */
export function inspectorThinkingDelta(text: string): void {
  // Append to last chunk or create new one
  const last = _state.thinking.length > 0 ? _state.thinking[_state.thinking.length - 1] : undefined;
  if (last && Date.now() - last.timestamp < 500) {
    last.text += text;
  } else {
    _state.thinking.push({ text, timestamp: Date.now() });
  }
  if (_state.isOpen) renderInspector();
}

/** Record run completion. */
export function inspectorComplete(
  totalRounds?: number,
  maxRounds?: number,
  usage?: { input_tokens: number; output_tokens: number; total_tokens: number },
  model?: string,
): void {
  _state.isRunning = false;
  _state.finishedAt = Date.now();
  if (totalRounds !== undefined) _state.totalRounds = totalRounds;
  if (maxRounds !== undefined) _state.maxRounds = maxRounds;
  if (totalRounds !== undefined) _state.currentRound = totalRounds;
  if (usage) {
    _state.usage = {
      inputTokens: usage.input_tokens,
      outputTokens: usage.output_tokens,
      totalTokens: usage.total_tokens,
    };
  }
  if (model) _state.model = model;
  if (_state.isOpen) renderInspector();
}

/** Record an error. */
export function inspectorError(message: string): void {
  _state.isRunning = false;
  _state.finishedAt = Date.now();

  // Add as a failed tool entry so it shows in the timeline
  _state.tools.push({
    callId: `error-${Date.now()}`,
    name: 'error',
    round: _state.currentRound,
    startedAt: Date.now(),
    finishedAt: Date.now(),
    durationMs: 0,
    success: false,
    outputPreview: truncateOutput(message),
    tier: null,
    autoApproved: false,
  });

  if (_state.isOpen) renderInspector();
}

/** Update the context window limit (from model metadata). */
export function setInspectorContextLimit(limit: number): void {
  _state.contextLimit = limit;
}

// ── Render ────────────────────────────────────────────────────────────

/** Render the full Inspector panel contents. */
export function renderInspector(): void {
  const panel = $('inspector-panel');
  if (!panel) return;

  const s = _state;
  const rPct = roundPercent(s.currentRound, s.maxRounds);
  const ctxPct = s.contextTokens ? contextPercent(s.contextTokens, s.contextLimit) : 0;
  const ctxClass = contextLevelClass(ctxPct);
  const totalDuration = s.startedAt ? (s.finishedAt ?? Date.now()) - s.startedAt : 0;

  panel.innerHTML = `
    <div class="inspector-header">
      <span class="inspector-title">
        <span class="ms ms-sm">biotech</span> Inspector
        ${s.isRunning ? '<span class="inspector-live-dot"></span>' : ''}
      </span>
      <button class="btn btn-ghost btn-xs inspector-close-btn" title="Close (Ctrl+Shift+I)">
        <span class="ms ms-sm">close</span>
      </button>
    </div>

    <div class="inspector-body">
      ${renderRoundCounter(s.currentRound, s.maxRounds, rPct)}
      ${s.model ? `<div class="inspector-model"><span class="ms ms-xs">smart_toy</span> ${s.model}</div>` : ''}
      ${renderContextBar(s.contextTokens, s.contextLimit, ctxPct, ctxClass)}
      ${s.usage ? renderUsageBreakdown(s.usage.inputTokens, s.usage.outputTokens) : ''}
      ${renderLoadedTools(s.loadedTools)}
      ${renderToolTimeline(s.tools, totalDuration)}
      ${renderThinkingTrace(s.thinking)}
    </div>
  `;

  wireInspectorEvents(panel);
}

// ── Section Renderers ─────────────────────────────────────────────────

function renderRoundCounter(current: number, max: number, pct: number): string {
  if (current === 0 && max === 0) return '';
  const circumference = 2 * Math.PI * 18; // r=18
  const offset = circumference - (pct / 100) * circumference;

  return `
    <div class="inspector-section inspector-rounds">
      <svg class="inspector-ring" width="44" height="44" viewBox="0 0 44 44">
        <circle cx="22" cy="22" r="18" fill="none" stroke="var(--border)" stroke-width="3"/>
        <circle cx="22" cy="22" r="18" fill="none" stroke="var(--accent)" stroke-width="3"
          stroke-dasharray="${circumference}" stroke-dashoffset="${offset}"
          stroke-linecap="round" transform="rotate(-90 22 22)"/>
      </svg>
      <div class="inspector-round-label">
        <span class="inspector-round-num">Round ${current}</span>
        <span class="inspector-round-max">/ max ${max}</span>
      </div>
      <div class="inspector-round-pct">${pct}%</div>
    </div>
  `;
}

function renderContextBar(
  tokens: number | null,
  limit: number,
  pct: number,
  levelClass: string,
): string {
  if (!tokens) return '';
  return `
    <div class="inspector-section">
      <div class="inspector-section-title">
        <span class="ms ms-xs">memory</span> Context Usage
      </div>
      <div class="inspector-ctx-bar ${levelClass}">
        <div class="inspector-ctx-fill" style="width:${pct}%"></div>
      </div>
      <div class="inspector-ctx-label">${pct}% (${formatTokens(tokens)} / ${formatTokens(limit)})</div>
    </div>
  `;
}

function renderUsageBreakdown(input: number, output: number): string {
  return `
    <div class="inspector-section">
      <div class="inspector-section-title">
        <span class="ms ms-xs">token</span> Token Usage
      </div>
      <div class="inspector-usage-grid">
        <span class="inspector-usage-label">Input:</span>
        <span class="inspector-usage-val">${formatTokens(input)}</span>
        <span class="inspector-usage-label">Output:</span>
        <span class="inspector-usage-val">${formatTokens(output)}</span>
        <span class="inspector-usage-label">Total:</span>
        <span class="inspector-usage-val">${formatTokens(input + output)}</span>
      </div>
    </div>
  `;
}

function renderLoadedTools(tools: string[]): string {
  if (tools.length === 0) return '';
  return `
    <div class="inspector-section">
      <div class="inspector-section-title">
        <span class="ms ms-xs">build</span> Tools Loaded (${tools.length})
      </div>
      <div class="inspector-tool-chips">
        ${tools.map((t) => `<span class="inspector-chip">${t}</span>`).join('')}
      </div>
    </div>
  `;
}

// ── Event Wiring ──────────────────────────────────────────────────────

function wireInspectorEvents(panel: HTMLElement): void {
  const closeBtn = panel.querySelector('.inspector-close-btn');
  if (closeBtn) {
    closeBtn.addEventListener('click', () => closeInspector());
  }
}
