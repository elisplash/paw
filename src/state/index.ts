// src/state/index.ts — Global application state singleton.
// All mutable UI state lives here so it can be shared across
// chat_controller, event_bus, channels, and main without circular deps.

import type { AppConfig, Session, ToolCall } from '../types';
import type { ModelPricingRow } from '../db';

// ── Extended message type ──────────────────────────────────────────────────
export interface ChatAttachmentLocal {
  name?: string;
  mimeType: string;
  url?: string;
  data?: string; // base64
}

export interface MessageWithAttachments {
  id?: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: Date;
  toolCalls?: ToolCall[];
  attachments?: ChatAttachmentLocal[];
  thinkingContent?: string;
}

// ── Token metering constants ───────────────────────────────────────────────
// Defaults are baked-in as fallbacks. DB overrides (model_pricing table)
// are merged on top at startup via loadModelPricing().
export const COMPACTION_WARN_THRESHOLD = 0.8;

const DEFAULT_CONTEXT_SIZES: Record<string, number> = {
  // Gemini
  'gemini-3.1-pro-preview': 1_048_576,
  'gemini-3-pro-preview': 1_048_576,
  'gemini-3-flash-preview': 1_048_576,
  'gemini-2.5-pro': 1_048_576,
  'gemini-2.5-flash': 1_048_576,
  'gemini-2.5-flash-lite': 1_048_576,
  'gemini-2.0-flash': 1_048_576,
  'gemini-2.0-pro': 1_048_576,
  // OpenAI
  'gpt-4o': 128_000,
  'gpt-4o-mini': 128_000,
  'gpt-4-turbo': 128_000,
  'gpt-4': 8_192,
  'gpt-3.5-turbo': 16_385,
  o1: 200_000,
  'o1-mini': 128_000,
  'o1-pro': 200_000,
  o3: 200_000,
  'o3-mini': 200_000,
  'o4-mini': 200_000,
  // Anthropic
  'claude-opus-4': 200_000,
  'claude-sonnet-4': 200_000,
  'claude-haiku-4': 200_000,
  'claude-sonnet-4-5': 200_000,
  'claude-3-5-sonnet': 200_000,
  'claude-3-5-haiku': 200_000,
  'claude-3-opus': 200_000,
  // DeepSeek
  'deepseek-chat': 128_000,
  'deepseek-reasoner': 128_000,
  // Llama
  'llama-3': 128_000,
  'llama-4': 128_000,
};

const DEFAULT_COST_PER_TOKEN: Record<string, { input: number; output: number }> = {
  'gpt-4o': { input: 2.5e-6, output: 10e-6 },
  'gpt-4o-mini': { input: 0.15e-6, output: 0.6e-6 },
  'gpt-4-turbo': { input: 10e-6, output: 30e-6 },
  'gpt-4': { input: 30e-6, output: 60e-6 },
  'gpt-3.5': { input: 0.5e-6, output: 1.5e-6 },
  'claude-opus-4': { input: 5e-6, output: 25e-6 },
  'claude-sonnet-4': { input: 3e-6, output: 15e-6 },
  'claude-haiku-4': { input: 1e-6, output: 5e-6 },
  'claude-sonnet-4-5': { input: 3e-6, output: 15e-6 },
  'claude-3-5-sonnet': { input: 3e-6, output: 15e-6 },
  'claude-3-5-haiku': { input: 1e-6, output: 5e-6 },
  'claude-3-opus': { input: 15e-6, output: 75e-6 },
  default: { input: 3e-6, output: 15e-6 },
};

// Mutable maps — start with defaults, merged with DB overrides at init
export let MODEL_CONTEXT_SIZES: Record<string, number> = { ...DEFAULT_CONTEXT_SIZES };
export let MODEL_COST_PER_TOKEN: Record<string, { input: number; output: number }> = {
  ...DEFAULT_COST_PER_TOKEN,
};

/**
 * Merge DB model_pricing overrides on top of built-in defaults.
 * Call once after DB init. Safe to call multiple times (idempotent).
 */
export function applyModelPricingOverrides(rows: ModelPricingRow[]): void {
  // Reset to defaults then layer overrides
  MODEL_CONTEXT_SIZES = { ...DEFAULT_CONTEXT_SIZES };
  MODEL_COST_PER_TOKEN = { ...DEFAULT_COST_PER_TOKEN };
  for (const row of rows) {
    if (row.context_size != null) {
      MODEL_CONTEXT_SIZES[row.model_key] = row.context_size;
    }
    if (row.cost_input != null && row.cost_output != null) {
      MODEL_COST_PER_TOKEN[row.model_key] = { input: row.cost_input, output: row.cost_output };
    }
  }
}

// ── Per-session stream state ───────────────────────────────────────────────
export interface StreamState {
  content: string;
  thinkingContent: string;
  el: HTMLElement | null;
  runId: string | null;
  resolve: ((text: string) => void) | null;
  timeout: ReturnType<typeof setTimeout> | null;
  agentId: string | null;
  /** Set to true after onToken has fired for this run to prevent double-counting */
  tokenRecorded: boolean;
  /** Timestamp when this stream was created (for stale cleanup). */
  createdAt: number;
}

export function createStreamState(agentId?: string | null): StreamState {
  return {
    content: '',
    thinkingContent: '',
    el: null,
    runId: null,
    resolve: null,
    timeout: null,
    agentId: agentId ?? null,
    tokenRecorded: false,
    createdAt: Date.now(),
  };
}

/** Max age (ms) before a stream entry is considered stale and evictable. */
const STREAM_MAX_AGE_MS = 10 * 60 * 1000; // 10 minutes (matches streaming timeout)

/** Hard cap on concurrent stream entries. */
const STREAM_MAX_ENTRIES = 50;

/**
 * Remove stale stream entries that were never cleaned up (e.g. error paths).
 * Called automatically when a new stream is registered.
 */
export function sweepStaleStreams(): number {
  const now = Date.now();
  let swept = 0;
  for (const [key, ss] of appState.activeStreams) {
    if (now - ss.createdAt > STREAM_MAX_AGE_MS) {
      if (ss.timeout) clearTimeout(ss.timeout);
      if (ss.resolve) ss.resolve(ss.content || '');
      appState.activeStreams.delete(key);
      swept++;
    }
  }
  // Hard cap: if still over limit, evict oldest entries
  if (appState.activeStreams.size > STREAM_MAX_ENTRIES) {
    const sorted = [...appState.activeStreams.entries()].sort(
      (a, b) => a[1].createdAt - b[1].createdAt,
    );
    while (sorted.length > STREAM_MAX_ENTRIES) {
      const [key, ss] = sorted.shift()!;
      if (ss.timeout) clearTimeout(ss.timeout);
      if (ss.resolve) ss.resolve(ss.content || '');
      appState.activeStreams.delete(key);
      swept++;
    }
  }
  return swept;
}

// ── Mutable singleton state ────────────────────────────────────────────────
export const appState = {
  // Core config (loaded from localStorage)
  config: { configured: false } as AppConfig,

  // Chat
  messages: [] as MessageWithAttachments[],
  isLoading: false,
  currentSessionKey: null as string | null,
  sessions: [] as Session[],
  wsConnected: false,

  // Streaming pipeline — session-keyed for concurrent isolation
  activeStreams: new Map<string, StreamState>(),

  // Legacy convenience accessors (delegate to current session's stream)
  get streamingContent(): string {
    const s = appState.activeStreams.get(appState.currentSessionKey ?? '');
    return s?.content ?? '';
  },
  set streamingContent(v: string) {
    const key = appState.currentSessionKey ?? '';
    const s = appState.activeStreams.get(key);
    if (s) s.content = v;
  },
  get streamingEl(): HTMLElement | null {
    const s = appState.activeStreams.get(appState.currentSessionKey ?? '');
    return s?.el ?? null;
  },
  set streamingEl(v: HTMLElement | null) {
    const key = appState.currentSessionKey ?? '';
    const s = appState.activeStreams.get(key);
    if (s) s.el = v;
  },
  get streamingRunId(): string | null {
    const s = appState.activeStreams.get(appState.currentSessionKey ?? '');
    return s?.runId ?? null;
  },
  set streamingRunId(v: string | null) {
    const key = appState.currentSessionKey ?? '';
    const s = appState.activeStreams.get(key);
    if (s) s.runId = v;
  },
  get streamingResolve(): ((text: string) => void) | null {
    const s = appState.activeStreams.get(appState.currentSessionKey ?? '');
    return s?.resolve ?? null;
  },
  set streamingResolve(v: ((text: string) => void) | null) {
    const key = appState.currentSessionKey ?? '';
    const s = appState.activeStreams.get(key);
    if (s) s.resolve = v;
  },
  get streamingAgentId(): string | null {
    const s = appState.activeStreams.get(appState.currentSessionKey ?? '');
    return s?.agentId ?? null;
  },
  set streamingAgentId(v: string | null) {
    const key = appState.currentSessionKey ?? '';
    const s = appState.activeStreams.get(key);
    if (s) s.agentId = v;
  },

  // Attachments
  pendingAttachments: [] as File[],

  // Token metering (per session)
  sessionTokensUsed: 0,
  sessionInputTokens: 0,
  sessionOutputTokens: 0,
  sessionCost: 0,
  modelContextLimit: 128_000,
  compactionDismissed: false,
  lastRecordedTotal: 0,
  activeModelKey: 'default',

  // Context breakdown tracking
  sessionToolResultTokens: 0,
  sessionToolCallCount: 0,

  // TTS
  ttsAudio: null as HTMLAudioElement | null,
  ttsActiveBtn: null as HTMLButtonElement | null,

  // Scroll de-bounce
  scrollRafPending: false,
};

// ── Per-agent session map ──────────────────────────────────────────────────
// Remembers which session belongs to which agent, persisted to localStorage.
export const agentSessionMap: Map<string, string> = (() => {
  try {
    const stored = localStorage.getItem('paw_agent_sessions');
    return stored
      ? new Map<string, string>(JSON.parse(stored) as [string, string][])
      : new Map<string, string>();
  } catch {
    return new Map<string, string>();
  }
})();

export function persistAgentSessionMap(): void {
  try {
    localStorage.setItem('paw_agent_sessions', JSON.stringify([...agentSessionMap.entries()]));
  } catch {
    /* ignore */
  }
}
