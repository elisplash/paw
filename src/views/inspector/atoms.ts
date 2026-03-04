// Inspector — Pure data types and helpers (no DOM)
// The Inspector is the "Agent X-Ray" panel showing real-time
// chain-of-thought, tool routing, memory recall, and context utilization.

// ── Data Types ────────────────────────────────────────────────────────

/** A single tool call entry in the timeline. */
export interface InspectorToolEntry {
  /** Unique tool call id */
  callId: string;
  /** Tool function name */
  name: string;
  /** Which round this was called in */
  round: number;
  /** Epoch ms when the call started */
  startedAt: number;
  /** Epoch ms when the call finished (null if in progress) */
  finishedAt: number | null;
  /** Duration in ms (computed on finish) */
  durationMs: number | null;
  /** Whether the tool succeeded */
  success: boolean | null;
  /** Truncated output (first N chars) */
  outputPreview: string | null;
  /** Tool tier classification */
  tier: string | null;
  /** Whether the tool was auto-approved */
  autoApproved: boolean;
}

/** A thinking trace chunk. */
export interface InspectorThinkingChunk {
  /** Accumulated thinking text */
  text: string;
  /** When this chunk arrived */
  timestamp: number;
}

/** Recalled memory entry shown in the Inspector. */
export interface InspectorMemoryEntry {
  /** Memory content (truncated) */
  content: string;
  /** Relevance score (0–1) */
  relevance: number;
  /** Decay weight (0–1) */
  decay: number;
}

/** Token usage breakdown for the context bar. */
export interface InspectorUsage {
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
}

/** Full state snapshot of the Inspector for a single agent run. */
export interface InspectorState {
  /** Session key being inspected */
  sessionId: string | null;
  /** Run ID currently being inspected */
  runId: string | null;
  /** Whether the Inspector panel is visible */
  isOpen: boolean;
  /** Current round number */
  currentRound: number;
  /** Max rounds configured for this agent */
  maxRounds: number;
  /** Total rounds completed (set on 'complete') */
  totalRounds: number | null;
  /** Tool call timeline */
  tools: InspectorToolEntry[];
  /** Currently loaded tool names */
  loadedTools: string[];
  /** Thinking trace chunks */
  thinking: InspectorThinkingChunk[];
  /** Memory recalls shown during this run */
  memories: InspectorMemoryEntry[];
  /** Token usage (updated on complete) */
  usage: InspectorUsage | null;
  /** Estimated context tokens (updated per round) */
  contextTokens: number | null;
  /** Model context window limit (for the progress bar) */
  contextLimit: number;
  /** Model name (set on complete) */
  model: string | null;
  /** Whether the agent is currently running */
  isRunning: boolean;
  /** Epoch ms when the run started */
  startedAt: number | null;
  /** Epoch ms when the run finished */
  finishedAt: number | null;
}

// ── Factory ───────────────────────────────────────────────────────────

/** Create a fresh InspectorState for a new run. */
export function createInspectorState(): InspectorState {
  return {
    sessionId: null,
    runId: null,
    isOpen: false,
    currentRound: 0,
    maxRounds: 12,
    totalRounds: null,
    tools: [],
    loadedTools: [],
    thinking: [],
    memories: [],
    usage: null,
    contextTokens: null,
    contextLimit: 128_000, // default, overridden by model
    model: null,
    isRunning: false,
    startedAt: null,
    finishedAt: null,
  };
}

// ── Pure Helpers ──────────────────────────────────────────────────────

/** Max chars of tool output to show in the preview. */
const OUTPUT_PREVIEW_LIMIT = 200;

/** Truncate tool output for display. */
export function truncateOutput(output: string): string {
  if (output.length <= OUTPUT_PREVIEW_LIMIT) return output;
  return `${output.slice(0, OUTPUT_PREVIEW_LIMIT)}\u2026`;
}

/** Format milliseconds as a human-readable duration. */
export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  const mins = Math.floor(ms / 60_000);
  const secs = Math.round((ms % 60_000) / 1000);
  return `${mins}m ${secs}s`;
}

/** Format a token count with K suffix for large numbers. */
export function formatTokens(count: number): string {
  if (count < 1000) return String(count);
  return `${(count / 1000).toFixed(1)}k`;
}

/** Calculate context utilization as a percentage (0–100). */
export function contextPercent(used: number, limit: number): number {
  if (limit <= 0) return 0;
  return Math.min(100, Math.round((used / limit) * 100));
}

/** Round progress as a percentage (0–100). */
export function roundPercent(current: number, max: number): number {
  if (max <= 0) return 0;
  return Math.min(100, Math.round((current / max) * 100));
}

/** CSS class for context utilization level. */
export function contextLevelClass(percent: number): string {
  if (percent >= 90) return 'inspector-ctx-critical';
  if (percent >= 70) return 'inspector-ctx-warn';
  return 'inspector-ctx-ok';
}

/** Icon for tool tier. */
export function tierIcon(tier: string | null): string {
  switch (tier) {
    case 'safe':
      return 'verified_user';
    case 'reversible':
      return 'undo';
    case 'external':
      return 'cloud';
    case 'dangerous':
      return 'warning';
    default:
      return 'help_outline';
  }
}

/** Status icon for a tool entry. */
export function toolStatusIcon(entry: InspectorToolEntry): string {
  if (entry.finishedAt === null) return 'hourglass_top'; // in progress
  return entry.success ? 'check_circle' : 'error';
}

/** Status CSS class for a tool entry. */
export function toolStatusClass(entry: InspectorToolEntry): string {
  if (entry.finishedAt === null) return 'inspector-tool-running';
  return entry.success ? 'inspector-tool-ok' : 'inspector-tool-fail';
}
