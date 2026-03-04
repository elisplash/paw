// Inspector — Event Bridge
// Routes EngineEvent data into the Inspector state mutations.
// This is a thin adapter between the engine event stream and the
// Inspector panel's state management functions.

import type { EngineEvent } from '../../engine/atoms/types';
import {
  inspectorNewRun,
  inspectorToolRequest,
  inspectorToolResult,
  inspectorThinkingDelta,
  inspectorComplete,
  inspectorError,
  getInspectorState,
} from './index';

/** Last seen run_id — used to detect new runs. */
let _lastRunId: string | null = null;

/**
 * Route an incoming EngineEvent to the Inspector.
 * Called from the engine bridge on every event.
 * Only processes events that the Inspector cares about.
 */
export function routeInspectorEvent(event: EngineEvent): void {
  // Detect new run by run_id change
  if (event.run_id && event.run_id !== _lastRunId) {
    _lastRunId = event.run_id;

    // Only start a new inspector run if not already tracking this run
    const state = getInspectorState();
    if (state.runId !== event.run_id) {
      inspectorNewRun(event.session_id, event.run_id);
    }
  }

  switch (event.kind) {
    case 'tool_request': {
      const toolName = event.tool_call?.function?.name ?? 'unknown';
      const callId = event.tool_call?.id ?? `call-${Date.now()}`;
      const round = event.round_number ?? getInspectorState().currentRound + 1;
      const tier = event.tool_tier ?? null;

      inspectorToolRequest(
        callId,
        toolName,
        round,
        tier,
        false, // autoApproved = false (tool_auto_approved is a separate event)
        event.loaded_tools,
        event.context_tokens,
      );
      break;
    }

    case 'tool_auto_approved': {
      // Mark the matching tool entry as auto-approved
      const state = getInspectorState();
      const callId = event.tool_call_id;
      if (callId) {
        const entry = state.tools.find((t) => t.callId === callId);
        if (entry) entry.autoApproved = true;
      }
      break;
    }

    case 'tool_result': {
      const callId = event.tool_call_id ?? '';
      const output = event.output ?? '';
      const success = event.success ?? false;
      inspectorToolResult(callId, output, success, event.duration_ms);
      break;
    }

    case 'thinking_delta': {
      if (event.text) {
        inspectorThinkingDelta(event.text);
      }
      break;
    }

    case 'complete': {
      inspectorComplete(event.total_rounds, event.max_rounds, event.usage, event.model);
      break;
    }

    case 'error': {
      inspectorError(event.message ?? 'Unknown error');
      break;
    }

    // delta, canvas_push, canvas_update — not relevant to Inspector
    default:
      break;
  }
}
