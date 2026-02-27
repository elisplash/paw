// src/engine/molecules/token_meter.ts
// Scoped token metering molecule.
// All functions receive a container/state snapshot rather than reading globals.
// Returns a TokenMeterController — instance-able for mini-hubs.

import { fmtK, estimateContextBreakdown } from '../atoms/chat';
import {
  MODEL_CONTEXT_SIZES,
  MODEL_COST_PER_TOKEN,
  COMPACTION_WARN_THRESHOLD,
} from '../../state/index';
import { refreshMissionPanel, initMissionPanel } from '../../components/chat-mission-panel';

// ── Types ────────────────────────────────────────────────────────────────

export interface TokenMeterState {
  sessionTokensUsed: number;
  sessionInputTokens: number;
  sessionOutputTokens: number;
  sessionCost: number;
  modelContextLimit: number;
  compactionDismissed: boolean;
  lastRecordedTotal: number;
  activeModelKey: string;
  sessionToolResultTokens: number;
  sessionToolCallCount: number;
  messageCount: number;
  messages: Array<{ content?: string }>;
}

export interface TokenMeterController {
  /** Reset all counters to zero and re-render. */
  reset(state: TokenMeterState): void;
  /** Update the meter display from current state. */
  update(state: TokenMeterState): void;
  /** Record a token usage payload and update state + display. */
  recordUsage(
    usage: Record<string, unknown> | undefined,
    state: TokenMeterState,
    getBudgetLimit: () => number | null,
  ): void;
  /** Update context limit when model changes. */
  updateContextLimitFromModel(modelName: string, state: TokenMeterState): void;
  /** Update the context breakdown popover. */
  updateBreakdownPopover(state: TokenMeterState): void;
  /** Toggle the breakdown popover visibility. */
  toggleBreakdown(): void;
}

// ── Factory ──────────────────────────────────────────────────────────────

/**
 * Create a scoped token meter controller bound to specific DOM elements.
 * All DOM lookups are scoped to the provided element IDs within the document,
 * or can be scoped to a container in a future mini-hub variant.
 */
export function createTokenMeter(selectors: {
  meterId: string;
  fillId: string;
  labelId: string;
  breakdownPanelId: string;
  compactionWarningId: string;
  compactionWarningTextId: string;
  budgetAlertId: string;
  budgetAlertTextId: string;
}): TokenMeterController {
  const $ = (id: string) => document.getElementById(id);

  function updateMeter(state: TokenMeterState): void {
    const meter = $(selectors.meterId);
    const fill = $(selectors.fillId);
    const label = $(selectors.labelId);
    if (!meter || !fill || !label) return;
    meter.style.display = '';

    if (state.sessionTokensUsed <= 0) {
      fill.style.width = '0%';
      fill.className = 'token-meter-fill';
      const lim =
        state.modelContextLimit >= 1000
          ? `${(state.modelContextLimit / 1000).toFixed(0)}k`
          : `${state.modelContextLimit}`;
      label.textContent = `0 / ${lim} tokens`;
      meter.title = 'Token tracking active — send a message to see usage';

      // Still update mission panel on zero state
      refreshMissionPanel({
        tokensUsed: 0,
        contextLimit: state.modelContextLimit,
        inputTokens: 0,
        outputTokens: 0,
        cost: 0,
        messageCount: state.messageCount,
      });
      return;
    }

    const pct = Math.min((state.sessionTokensUsed / state.modelContextLimit) * 100, 100);
    fill.style.width = `${pct}%`;
    fill.className =
      pct >= 80
        ? 'token-meter-fill danger'
        : pct >= 60
          ? 'token-meter-fill warning'
          : 'token-meter-fill';

    const used =
      state.sessionTokensUsed >= 1000
        ? `${(state.sessionTokensUsed / 1000).toFixed(1)}k`
        : `${state.sessionTokensUsed}`;
    const lim =
      state.modelContextLimit >= 1000
        ? `${(state.modelContextLimit / 1000).toFixed(0)}k`
        : `${state.modelContextLimit}`;
    const cost = state.sessionCost > 0 ? ` · $${state.sessionCost.toFixed(4)}` : '';
    label.textContent = `${used} / ${lim} tokens${cost}`;
    meter.title = 'Click for context breakdown';

    updateCompactionWarning(pct, state);
    updateBreakdownPopoverFn(state);

    // Update mission control side panel
    refreshMissionPanel({
      tokensUsed: state.sessionTokensUsed,
      contextLimit: state.modelContextLimit,
      inputTokens: state.sessionInputTokens,
      outputTokens: state.sessionOutputTokens,
      cost: state.sessionCost,
      messageCount: state.messageCount,
    });
  }

  function updateCompactionWarning(pct: number, state: TokenMeterState): void {
    const warning = $(selectors.compactionWarningId);
    if (!warning) return;
    if (pct >= COMPACTION_WARN_THRESHOLD * 100 && !state.compactionDismissed) {
      warning.style.display = '';
      const text = $(selectors.compactionWarningTextId);
      if (text) {
        text.textContent =
          pct >= 95
            ? `Context window ${pct.toFixed(0)}% full — messages will be compacted imminently`
            : `Context window ${pct.toFixed(0)}% full — older messages may be compacted soon`;
      }
    } else {
      warning.style.display = 'none';
    }
  }

  function updateBreakdownPopoverFn(state: TokenMeterState): void {
    const panel = $(selectors.breakdownPanelId);
    if (!panel || panel.style.display === 'none') return;

    const b = estimateContextBreakdown({
      sessionTokensUsed: state.sessionTokensUsed,
      modelContextLimit: state.modelContextLimit,
      sessionInputTokens: state.sessionInputTokens,
      sessionOutputTokens: state.sessionOutputTokens,
      sessionToolResultTokens: state.sessionToolResultTokens,
      messages: state.messages,
    });

    const fill = panel.querySelector('.ctx-breakdown-fill') as HTMLElement | null;
    const summary = panel.querySelector('.ctx-breakdown-summary') as HTMLElement | null;
    const rows = panel.querySelector('.ctx-breakdown-rows') as HTMLElement | null;
    const warn = panel.querySelector('.ctx-breakdown-warn') as HTMLElement | null;

    if (fill) {
      fill.style.width = `${b.pct}%`;
      fill.className =
        b.pct >= 80
          ? 'ctx-breakdown-fill danger'
          : b.pct >= 60
            ? 'ctx-breakdown-fill warning'
            : 'ctx-breakdown-fill';
    }
    if (summary) {
      summary.textContent = `${fmtK(b.total)} / ${fmtK(b.limit)} tokens \u2022 ${b.pct.toFixed(0)}%`;
    }
    if (rows) {
      rows.innerHTML =
        `<div class="ctx-row"><span class="ctx-row-header">System</span></div>` +
        `<div class="ctx-row"><span class="ctx-row-label">System Prompt</span><span class="ctx-row-value">${b.systemPct.toFixed(1)}%</span></div>` +
        `<div class="ctx-row"><span class="ctx-row-header">Conversation</span></div>` +
        `<div class="ctx-row"><span class="ctx-row-label">Messages</span><span class="ctx-row-value">${b.messagesPct.toFixed(1)}%</span></div>` +
        `<div class="ctx-row"><span class="ctx-row-label">Tool Results</span><span class="ctx-row-value">${b.toolResultsPct.toFixed(1)}%</span></div>` +
        `<div class="ctx-row"><span class="ctx-row-label">Output</span><span class="ctx-row-value">${b.outputPct.toFixed(1)}%</span></div>`;
    }
    if (warn) {
      warn.style.display = b.pct >= 60 ? '' : 'none';
      warn.textContent =
        b.pct >= 90
          ? 'Context nearly full — quality will degrade.'
          : b.pct >= 60
            ? 'Quality may decline as limit nears.'
            : '';
    }
  }

  function toggleBreakdownFn(): void {
    const panel = $(selectors.breakdownPanelId);
    if (!panel) return;
    const visible = panel.style.display !== 'none';
    if (visible) {
      panel.style.display = 'none';
    } else {
      panel.style.display = '';
      // Will need a state snapshot to update — caller should call updateBreakdownPopover after
    }
  }

  // ── Controller ─────────────────────────────────────────────────────────

  const controller: TokenMeterController = {
    reset(state: TokenMeterState): void {
      state.sessionTokensUsed = 0;
      state.sessionInputTokens = 0;
      state.sessionOutputTokens = 0;
      state.sessionCost = 0;
      state.lastRecordedTotal = 0;
      state.compactionDismissed = false;
      state.sessionToolResultTokens = 0;
      state.sessionToolCallCount = 0;
      updateMeter(state);
      initMissionPanel();
      const budgetAlert = $(selectors.budgetAlertId);
      if (budgetAlert) budgetAlert.style.display = 'none';
    },

    update(state: TokenMeterState): void {
      updateMeter(state);
    },

    recordUsage(
      usage: Record<string, unknown> | undefined,
      state: TokenMeterState,
      getBudgetLimit: () => number | null,
    ): void {
      if (!usage) return;
      const uAny = usage as Record<string, unknown>;
      const nested = uAny.response as Record<string, unknown> | undefined;
      const inner = (uAny.usage ?? nested?.usage ?? usage) as Record<string, unknown>;
      const totalTokens = (inner.totalTokens ??
        inner.total_tokens ??
        inner.totalTokenCount ??
        0) as number;
      const inputTokens = (inner.promptTokens ??
        inner.prompt_tokens ??
        inner.inputTokens ??
        inner.input_tokens ??
        inner.prompt_token_count ??
        0) as number;
      const outputTokens = (inner.completionTokens ??
        inner.completion_tokens ??
        inner.outputTokens ??
        inner.output_tokens ??
        inner.completion_token_count ??
        0) as number;

      if (totalTokens > 0 || inputTokens > 0 || outputTokens > 0) {
        state.sessionInputTokens = inputTokens;
        state.sessionOutputTokens += outputTokens;
        state.sessionTokensUsed = inputTokens + state.sessionOutputTokens;
        state.lastRecordedTotal = state.sessionTokensUsed;
      }

      const rate = MODEL_COST_PER_TOKEN[state.activeModelKey] ?? MODEL_COST_PER_TOKEN['default'];
      state.sessionCost += inputTokens * rate.input + outputTokens * rate.output;

      const budgetLimit = getBudgetLimit();
      if (budgetLimit != null && state.sessionCost >= budgetLimit * 0.8) {
        const budgetAlert = $(selectors.budgetAlertId);
        if (budgetAlert) {
          budgetAlert.style.display = '';
          const alertText = $(selectors.budgetAlertTextId);
          if (alertText) {
            alertText.textContent =
              state.sessionCost >= budgetLimit
                ? `Session budget exceeded: $${state.sessionCost.toFixed(4)} / $${budgetLimit.toFixed(2)}`
                : `Nearing session budget: $${state.sessionCost.toFixed(4)} / $${budgetLimit.toFixed(2)}`;
          }
        }
      }
      updateMeter(state);
    },

    updateContextLimitFromModel(modelName: string, state: TokenMeterState): void {
      const lower = modelName.toLowerCase();
      for (const [prefix, limit] of Object.entries(MODEL_CONTEXT_SIZES)) {
        if (lower.includes(prefix)) {
          if (state.modelContextLimit !== limit) {
            console.debug(
              `[token] Context limit: ${state.modelContextLimit.toLocaleString()} → ${limit.toLocaleString()} (${modelName})`,
            );
            state.modelContextLimit = limit;
            updateMeter(state);
          }
          state.activeModelKey = prefix;
          return;
        }
      }
    },

    updateBreakdownPopover(state: TokenMeterState): void {
      updateBreakdownPopoverFn(state);
    },

    toggleBreakdown(): void {
      toggleBreakdownFn();
    },
  };

  return controller;
}
