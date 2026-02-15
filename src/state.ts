// Paw — Global Application State
// Centralized state that views can import

import type { AppConfig, Message, Session } from './types';

// ── App Configuration ──────────────────────────────────────────────────────
export let config: AppConfig = {
  configured: false,
  gateway: { url: '', token: '' },
};

export function setConfig(newConfig: AppConfig) {
  config = newConfig;
}

// ── Chat State ─────────────────────────────────────────────────────────────
export let messages: Message[] = [];
export let currentSessionKey: string | null = null;
export let sessions: Session[] = [];
export let isLoading = false;
export let wsConnected = false;

// Streaming state
export let streamingContent = '';
export let streamingEl: HTMLElement | null = null;
export let streamingRunId: string | null = null;
export let streamingResolve: ((text: string) => void) | null = null;
export let streamingTimeout: ReturnType<typeof setTimeout> | null = null;

export function setMessages(m: Message[]) { messages = m; }
export function setCurrentSessionKey(k: string | null) { currentSessionKey = k; }
export function setSessions(s: Session[]) { sessions = s; }
export function setIsLoading(l: boolean) { isLoading = l; }
export function setWsConnected(c: boolean) { wsConnected = c; }
export function setStreamingContent(c: string) { streamingContent = c; }
export function setStreamingEl(el: HTMLElement | null) { streamingEl = el; }
export function setStreamingRunId(id: string | null) { streamingRunId = id; }
export function setStreamingResolve(r: ((text: string) => void) | null) { streamingResolve = r; }
export function setStreamingTimeout(t: ReturnType<typeof setTimeout> | null) { streamingTimeout = t; }

// ── Build State ────────────────────────────────────────────────────────────
export let buildProjectId: string | null = null;
export let buildActiveFile: string | null = null;
export let buildOpenFiles: { id: string; path: string; content: string }[] = [];
export let buildStreaming = false;
export let buildStreamContent = '';
export let buildStreamRunId: string | null = null;
export let buildStreamResolve: ((text: string) => void) | null = null;

export function setBuildProjectId(id: string | null) { buildProjectId = id; }
export function setBuildActiveFile(f: string | null) { buildActiveFile = f; }
export function setBuildOpenFiles(files: { id: string; path: string; content: string }[]) { buildOpenFiles = files; }
export function setBuildStreaming(s: boolean) { buildStreaming = s; }
export function setBuildStreamContent(c: string) { buildStreamContent = c; }
export function setBuildStreamRunId(id: string | null) { buildStreamRunId = id; }
export function setBuildStreamResolve(r: ((text: string) => void) | null) { buildStreamResolve = r; }

// ── Content State ──────────────────────────────────────────────────────────
export let activeDocId: string | null = null;
export let contentStreaming = false;
export let contentStreamContent = '';
export let contentStreamRunId: string | null = null;
export let contentStreamResolve: ((text: string) => void) | null = null;

export function setActiveDocId(id: string | null) { activeDocId = id; }
export function setContentStreaming(s: boolean) { contentStreaming = s; }
export function setContentStreamContent(c: string) { contentStreamContent = c; }
export function setContentStreamRunId(id: string | null) { contentStreamRunId = id; }
export function setContentStreamResolve(r: ((text: string) => void) | null) { contentStreamResolve = r; }

// ── Research State ─────────────────────────────────────────────────────────
export let activeResearchId: string | null = null;
export let researchStreaming = false;
export let researchContent = '';
export let researchRunId: string | null = null;
export let researchResolve: ((text: string) => void) | null = null;

export function setActiveResearchId(id: string | null) { activeResearchId = id; }
export function setResearchStreaming(s: boolean) { researchStreaming = s; }
export function setResearchContent(c: string) { researchContent = c; }
export function setResearchRunId(id: string | null) { researchRunId = id; }
export function setResearchResolve(r: ((text: string) => void) | null) { researchResolve = r; }

// ── Helper to get port from URL ────────────────────────────────────────────
export function getPortFromUrl(url: string): number {
  if (!url) return 18789;
  try { return parseInt(new URL(url).port, 10) || 18789; }
  catch { return 18789; }
}
