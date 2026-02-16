// Paw — Main Application
// Wires OpenClaw gateway (WebSocket protocol v3) to the UI

import type { AppConfig, Message, InstallProgress, ChatMessage, Session } from './types';
import { setGatewayConfig, probeHealth } from './api';
import { gateway, isLocalhostUrl } from './gateway';
// ── Inline Lucide-style SVG icons (avoids broken lucide package) ─────────────
const _icons: Record<string, string> = {
  'paperclip': '<path d="m16 6-8.414 8.586a2 2 0 0 0 2.829 2.829l8.414-8.586a4 4 0 1 0-5.657-5.657l-8.379 8.551a6 6 0 1 0 8.485 8.485l8.379-8.551"/>',
  'arrow-up': '<path d="m5 12 7-7 7 7"/><path d="M12 19V5"/>',
  'square': '<rect width="18" height="18" x="3" y="3" rx="2"/>',
  'rotate-ccw': '<path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/>',
  'x': '<path d="M18 6 6 18"/><path d="m6 6 12 12"/>',
  'image': '<rect width="18" height="18" x="3" y="3" rx="2" ry="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"/>',
  'file-text': '<path d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z"/><path d="M14 2v5a1 1 0 0 0 1 1h5"/><path d="M10 9H8"/><path d="M16 13H8"/><path d="M16 17H8"/>',
  'file': '<path d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z"/><path d="M14 2v5a1 1 0 0 0 1 1h5"/>',
  'wrench': '<path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.106-3.105c.32-.322.863-.22.983.218a6 6 0 0 1-8.259 7.057l-7.91 7.91a1 1 0 0 1-2.999-3l7.91-7.91a6 6 0 0 1 7.057-8.259c.438.12.54.662.219.984z"/>',
  'download': '<path d="M12 15V3"/><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m7 10 5 5 5-5"/>',
  'external-link': '<path d="M15 3h6v6"/><path d="M10 14 21 3"/><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>',
};
function icon(name: string, cls = ''): string {
  const inner = _icons[name] || '';
  return `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"${cls ? ` class="${cls}"` : ''}>${inner}</svg>`;
}
import { initDb, initDbEncryption, listModes, listDocs, saveDoc, getDoc, deleteDoc, listProjects, saveProject, listProjectFiles, saveProjectFile, deleteProjectFile, logCredentialActivity, logSecurityEvent } from './db';
import * as SettingsModule from './views/settings';
import * as ModelsSettings from './views/settings-models';
import * as EnvSettings from './views/settings-env';
import * as AgentDefaultsSettings from './views/settings-agent-defaults';
import * as SessionsSettings from './views/settings-sessions';
import * as VoiceSettings from './views/settings-voice';
import * as AdvancedSettings from './views/settings-advanced';
import { setConnected as setSettingsConnected, invalidateConfigCache } from './views/settings-config';
import * as AutomationsModule from './views/automations';
import * as MemoryPalaceModule from './views/memory-palace';
import * as MailModule from './views/mail';
import type { MailPermissions } from './views/mail';
import * as SkillsModule from './views/skills';
import * as FoundryModule from './views/foundry';
import * as ResearchModule from './views/research';
import * as NodesModule from './views/nodes';
import * as ProjectsModule from './views/projects';
import * as AgentsModule from './views/agents';
import * as TodayModule from './views/today';
import { classifyCommandRisk, isPrivilegeEscalation, loadSecuritySettings, matchesAllowlist, matchesDenylist, auditNetworkRequest, getSessionOverrideRemaining, isFilesystemWriteTool, activateSessionOverride, type RiskClassification } from './security';

// ── Global error handlers ──────────────────────────────────────────────────
function crashLog(msg: string) {
  try {
    const log = JSON.parse(localStorage.getItem('paw-crash-log') || '[]') as string[];
    log.push(`${new Date().toISOString()} ${msg}`);
    // Keep last 50 entries
    while (log.length > 50) log.shift();
    localStorage.setItem('paw-crash-log', JSON.stringify(log));
  } catch { /* localStorage might be full */ }
}
window.addEventListener('unhandledrejection', (event) => {
  const msg = event.reason?.message ?? event.reason ?? 'unknown';
  crashLog(`unhandledrejection: ${msg}`);
  console.error('Unhandled promise rejection:', msg);
  event.preventDefault();
});
window.addEventListener('error', (event) => {
  const msg = event.error?.message ?? event.message ?? 'unknown';
  crashLog(`error: ${msg}`);
  console.error('Uncaught error:', msg);
});

// ── Tauri bridge ───────────────────────────────────────────────────────────
interface TauriWindow {
  __TAURI__?: {
    core: { invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T> };
    event: { listen: <T>(event: string, handler: (event: { payload: T }) => void) => Promise<() => void> };
  };
}
const tauriWindow = window as unknown as TauriWindow;
const invoke = tauriWindow.__TAURI__?.core?.invoke;
const listen = tauriWindow.__TAURI__?.event?.listen;

// ── State ──────────────────────────────────────────────────────────────────
let config: AppConfig = {
  configured: false,
  gateway: { url: '', token: '' },
};

let messages: MessageWithAttachments[] = [];
let isLoading = false;
let currentSessionKey: string | null = null;
let sessions: Session[] = [];
let wsConnected = false;
let _streamingContent = '';  // accumulates deltas for current streaming response
let _streamingEl: HTMLElement | null = null;  // the live-updating DOM element
let _streamingRunId: string | null = null;
let _streamingResolve: ((text: string) => void) | null = null;  // resolves when agent run completes
let _streamingTimeout: ReturnType<typeof setTimeout> | null = null;
let _pendingAttachments: File[] = [];

// ── Token metering state ───────────────────────────────────────────────────
let _sessionTokensUsed = 0;         // accumulated tokens for current session
let _sessionInputTokens = 0;
let _sessionOutputTokens = 0;
let _sessionCost = 0;               // estimated session cost in USD
let _modelContextLimit = 128_000;   // default context window (will be updated from models.list)
const COMPACTION_WARN_THRESHOLD = 0.80; // warn at 80% context usage
let _compactionDismissed = false;
let _lastRecordedTotal = 0; // tracks last real usage recording to detect fallback need

// Rough per-token cost estimates (USD) for common model families
const _MODEL_COST_PER_TOKEN: Record<string, { input: number; output: number }> = {
  'gpt-4o':       { input: 2.5e-6,  output: 10e-6 },
  'gpt-4o-mini':  { input: 0.15e-6, output: 0.6e-6 },
  'gpt-4-turbo':  { input: 10e-6,   output: 30e-6 },
  'gpt-4':        { input: 30e-6,   output: 60e-6 },
  'gpt-3.5':      { input: 0.5e-6,  output: 1.5e-6 },
  'claude-3-opus':    { input: 15e-6,  output: 75e-6 },
  'claude-3.5-sonnet': { input: 3e-6,   output: 15e-6 },
  'claude-3-haiku':   { input: 0.25e-6, output: 1.25e-6 },
  'claude-sonnet':    { input: 3e-6,   output: 15e-6 },
  'claude-opus':      { input: 15e-6,  output: 75e-6 },
  'claude-haiku':     { input: 0.25e-6, output: 1.25e-6 },
  'default':          { input: 3e-6,   output: 15e-6 },
};
let _activeModelKey = 'default';

/** Extended Message type with optional attachments (gateway may include these) */
interface ChatAttachmentLocal {
  name: string;
  mimeType: string;
  url?: string;
  data?: string; // base64
}
interface MessageWithAttachments extends Message {
  attachments?: ChatAttachmentLocal[];
}

function getPortFromUrl(url: string): number {
  if (!url) return 18789;
  try { return parseInt(new URL(url).port, 10) || 18789; }
  catch { return 18789; }
}

// ── DOM refs ───────────────────────────────────────────────────────────────
const $ = (id: string) => document.getElementById(id);
const dashboardView = $('dashboard-view');
const setupView = $('setup-view');
const manualSetupView = $('manual-setup-view');
const installView = $('install-view');
const chatView = $('chat-view');
const buildView = $('build-view');
const codeView = $('code-view');
const contentView = $('content-view');
const mailView = $('mail-view');
const automationsView = $('automations-view');
const channelsView = $('channels-view');
const researchView = $('research-view');
const memoryView = $('memory-view');
const skillsView = $('skills-view');
const foundryView = $('foundry-view');
const settingsView = $('settings-view');
const nodesView = $('nodes-view');
const agentsView = $('agents-view');
const todayView = $('today-view');
const statusDot = $('status-dot');
const statusText = $('status-text');
const chatMessages = $('chat-messages');
const chatEmpty = $('chat-empty');
const chatInput = $('chat-input') as HTMLTextAreaElement | null;
const chatSend = $('chat-send') as HTMLButtonElement | null;
const chatSessionSelect = $('chat-session-select') as HTMLSelectElement | null;
const chatAgentName = $('chat-agent-name');
const modelLabel = $('model-label');

const allViews = [
  dashboardView, setupView, manualSetupView, installView,
  chatView, buildView, codeView, contentView, mailView,
  automationsView, channelsView, researchView, memoryView,
  skillsView, foundryView, settingsView, nodesView, agentsView, todayView,
].filter(Boolean);

// ── Navigation ─────────────────────────────────────────────────────────────
document.querySelectorAll('[data-view]').forEach((item) => {
  item.addEventListener('click', () => {
    const view = item.getAttribute('data-view');
    if (view) switchView(view);
  });
});

function switchView(viewName: string) {
  if (!config.configured && viewName !== 'settings') return;

  document.querySelectorAll('.nav-item').forEach((item) => {
    item.classList.toggle('active', item.getAttribute('data-view') === viewName);
  });
  allViews.forEach((v) => v?.classList.remove('active'));

  const viewMap: Record<string, HTMLElement | null> = {
    dashboard: dashboardView, chat: chatView, build: buildView, code: codeView,
    content: contentView, mail: mailView, automations: automationsView,
    channels: channelsView, research: researchView, memory: memoryView,
    skills: skillsView, foundry: foundryView, settings: settingsView,
    nodes: nodesView, agents: agentsView, today: todayView,
  };
  const target = viewMap[viewName];
  if (target) target.classList.add('active');

  // Auto-load data when switching to a data view
  if (wsConnected) {
    switch (viewName) {
      case 'dashboard': loadDashboardCron(); break;
      case 'chat': loadSessions(); break;
      case 'channels': loadChannels(); break;
      case 'automations': AutomationsModule.loadCron(); break;
      case 'agents': AgentsModule.loadAgents(); break;
      case 'today': TodayModule.loadToday(); break;
      case 'skills': SkillsModule.loadSkills(); break;
      case 'foundry': FoundryModule.loadModels(); FoundryModule.loadModes(); FoundryModule.loadAgents(); break;
      case 'nodes': NodesModule.loadNodes(); NodesModule.loadPairingRequests(); break;
      case 'memory': MemoryPalaceModule.loadMemoryPalace(); loadMemory(); break;
      case 'build': loadBuildProjects(); loadSpaceCron('build'); break;
      case 'mail': MailModule.loadMail(); loadSpaceCron('mail'); break;
      case 'settings': syncSettingsForm(); loadGatewayConfig(); SettingsModule.loadSettings(); SettingsModule.startUsageAutoRefresh(); loadActiveSettingsTab(); break;
      default: break;
    }
  }
  // Stop usage auto-refresh when leaving settings
  if (viewName !== 'settings') SettingsModule.stopUsageAutoRefresh();
  // Local-only views (no gateway needed)
  switch (viewName) {
    case 'content': loadContentDocs(); if (wsConnected) loadSpaceCron('content'); break;
    case 'research': ResearchModule.loadResearchProjects(); if (wsConnected) loadSpaceCron('research'); break;
    case 'code': ProjectsModule.loadProjects(); break;
    default: break;
  }
  if (viewName === 'settings') syncSettingsForm();
}

function showView(viewId: string) {
  allViews.forEach((v) => v?.classList.remove('active'));
  $(viewId)?.classList.add('active');
}

// ── Gateway connection ─────────────────────────────────────────────────────
let _connectInProgress = false;

async function connectGateway(): Promise<boolean> {
  if (_connectInProgress || gateway.isConnecting) {
    console.warn('[main] connectGateway called while already connecting, skipping');
    return false;
  }
  _connectInProgress = true;

  // Repair openclaw.json if it was corrupted by a previous Paw version
  if (invoke) {
    try {
      const repaired = await invoke<boolean>('repair_openclaw_config');
      if (repaired) console.log('[main] Repaired openclaw.json (fixed invalid config properties)');
    } catch { /* ignore — first run or no config yet */ }
  }

  try {
    const wsUrl = config.gateway.url.replace(/^http/, 'ws');
    const tokenLen = config.gateway.token?.length ?? 0;
    console.log(`[main] connectGateway() → url=${wsUrl} tokenLen=${tokenLen}`);

    if (!wsUrl || wsUrl === 'ws://' || wsUrl === 'ws://undefined') {
      console.error('[main] Invalid gateway URL:', config.gateway.url);
      return false;
    }

    // ── Security: block non-localhost gateway URLs ──
    if (!isLocalhostUrl(wsUrl)) {
      console.error(`[main] BLOCKED: non-localhost gateway URL "${wsUrl}"`);
      showToast('Security: gateway URL must be localhost. Connection blocked.', 'error');
      return false;
    }

    const hello = await gateway.connect({ url: wsUrl, token: config.gateway.token });
    wsConnected = true;
    setSettingsConnected(true);
    console.log('[main] Gateway connected:', hello);

    statusDot?.classList.add('connected');
    statusDot?.classList.remove('error');
    if (statusText) statusText.textContent = 'Connected';
    if (modelLabel) modelLabel.textContent = 'Connected';

    // Abort any running agent executions left over from other clients
    // (e.g. OpenClaw Chat). Stale runs can crash the gateway when
    // two operator clients compete for the same session.
    // Also delete paw-* internal sessions (memory palace background calls).
    try {
      const sessResult = await gateway.listSessions({ limit: 50 });
      const activeSessions = sessResult.sessions ?? [];
      for (const s of activeSessions) {
        if (s.key.startsWith('paw-')) {
          // Delete internal sessions so they don't clutter the UI
          try { await gateway.deleteSession(s.key); } catch { /* ignore */ }
        } else {
          try { await gateway.chatAbort(s.key); } catch { /* no running exec */ }
        }
      }
      const pawSessions = activeSessions.filter(s => s.key.startsWith('paw-'));
      if (pawSessions.length) console.log(`[main] Cleaned up ${pawSessions.length} internal paw-* session(s)`);
      if (activeSessions.length - pawSessions.length > 0) {
        console.log(`[main] Cleared ${activeSessions.length - pawSessions.length} session(s) of stale agent runs`);
      }
    } catch (e) {
      console.warn('[main] Session cleanup failed (non-critical):', e);
    }

    // Load agent name + identity
    try {
      const agents = await gateway.listAgents();
      if (agents.agents?.length && chatAgentName) {
        const main = agents.agents.find(a => a.id === agents.defaultId) ?? agents.agents[0];
        chatAgentName.textContent = main.identity?.name ?? main.name ?? main.id;

        // Fetch detailed identity (emoji, theme) for richer header display
        try {
          const identity = await gateway.getAgentIdentity(main.id);
          const emoji = identity.emoji ?? main.identity?.emoji;
          const name = identity.name ?? main.identity?.name ?? main.name ?? main.id;
          if (emoji) {
            chatAgentName.textContent = `${emoji} ${name}`;
          } else {
            chatAgentName.textContent = name;
          }
        } catch { /* identity detail not available, keep agents.list name */ }
      }
    } catch { /* non-critical */ }

    return true;
  } catch (e) {
    console.error('[main] WS connect failed:', e);
    wsConnected = false;
    statusDot?.classList.remove('connected');
    statusDot?.classList.add('error');
    if (statusText) statusText.textContent = 'Disconnected';
    if (modelLabel) modelLabel.textContent = 'Disconnected';
    return false;
  } finally {
    _connectInProgress = false;
  }
}

// Subscribe to gateway lifecycle events
gateway.on('_connected', () => {
  wsConnected = true;
  setSettingsConnected(true);
  SettingsModule.setWsConnected(true);
  MemoryPalaceModule.setWsConnected(true);
  MailModule.setWsConnected(true);
  SkillsModule.setWsConnected(true);
  FoundryModule.setWsConnected(true);
  ResearchModule.setWsConnected(true);
  NodesModule.setWsConnected(true);
  statusDot?.classList.add('connected');
  statusDot?.classList.remove('error');
  if (statusText) statusText.textContent = 'Connected';
  // Detect model context limit for token meter
  detectModelContextLimit().catch(() => {});
  // Show token meter immediately (even at 0/128k) so users know tracking is on
  updateTokenMeter();
});
gateway.on('_disconnected', () => {
  wsConnected = false;
  setSettingsConnected(false);
  invalidateConfigCache();
  SettingsModule.setWsConnected(false);
  MemoryPalaceModule.setWsConnected(false);
  MailModule.setWsConnected(false);
  SkillsModule.setWsConnected(false);
  FoundryModule.setWsConnected(false);
  ResearchModule.setWsConnected(false);
  NodesModule.setWsConnected(false);
  statusDot?.classList.remove('connected');
  statusDot?.classList.add('error');
  if (statusText) statusText.textContent = 'Reconnecting...';

  // Clean up any in-progress streaming — resolve the promise with what we have
  // Use a local ref to avoid double-resolution (catch block may also resolve)
  const resolve = _streamingResolve;
  if (resolve) {
    _streamingResolve = null;
    console.warn('[main] WS disconnected during streaming — finalizing with partial content');
    resolve(_streamingContent || '(Connection lost)');
  }
});

gateway.on('_reconnect_exhausted', () => {
  if (statusText) statusText.textContent = 'Connection lost';
  console.error('[main] Gateway reconnect exhausted — attempting watchdog restart...');
  watchdogRestart();
});

// ── Crash Watchdog (C4) ────────────────────────────────────────────────────
let _watchdogCrashCount = 0;
let _watchdogLastCrash = 0;
let _watchdogRestartInProgress = false;
const WATCHDOG_MAX_RESTARTS = 5;        // max consecutive restarts before giving up
const WATCHDOG_RESET_WINDOW = 120_000;  // reset crash count after 2 min of stability

/** Attempt to restart the gateway after a crash. */
async function watchdogRestart(): Promise<void> {
  if (_watchdogRestartInProgress || !invoke) return;
  _watchdogRestartInProgress = true;

  const now = Date.now();
  // Reset crash counter if it's been stable for a while
  if (now - _watchdogLastCrash > WATCHDOG_RESET_WINDOW) _watchdogCrashCount = 0;
  _watchdogCrashCount++;
  _watchdogLastCrash = now;

  console.warn(`[watchdog] Crash detected (#${_watchdogCrashCount}/${WATCHDOG_MAX_RESTARTS})`);

  // Log crash to security audit
  logSecurityEvent({
    eventType: 'gateway_crash',
    riskLevel: _watchdogCrashCount >= WATCHDOG_MAX_RESTARTS ? 'critical' : 'medium',
    detail: `Gateway crash #${_watchdogCrashCount} — ${_watchdogCrashCount >= WATCHDOG_MAX_RESTARTS ? 'max restarts exceeded' : 'attempting auto-restart'}`,
  });

  if (_watchdogCrashCount > WATCHDOG_MAX_RESTARTS) {
    showToast(`Gateway crashed ${_watchdogCrashCount} times. Please restart Paw manually.`, 'error');
    if (statusText) statusText.textContent = 'Gateway crashed';
    _watchdogRestartInProgress = false;
    return;
  }

  showToast(`Gateway stopped unexpectedly. Restarting... (attempt ${_watchdogCrashCount}/${WATCHDOG_MAX_RESTARTS})`, 'warning');
  if (statusText) statusText.textContent = 'Restarting gateway...';

  try {
    const port = getPortFromUrl(config.gateway.url);
    await invoke('start_gateway', { port }).catch((e: unknown) => {
      console.warn('[watchdog] start_gateway failed:', e);
    });

    // Wait for gateway to boot
    await new Promise(r => setTimeout(r, 3000));

    // Verify it's alive
    const alive = await invoke<boolean>('check_gateway_health', { port }).catch(() => false);
    if (alive) {
      console.log('[watchdog] Gateway restarted successfully, reconnecting...');
      wsConnected = false;
      await connectGateway();
      if (wsConnected) {
        showToast('Gateway recovered and reconnected', 'success');
      }
    } else {
      console.error('[watchdog] Gateway restart failed — not responding on port', port);
      if (statusText) statusText.textContent = 'Restart failed';
    }
  } catch (e) {
    console.error('[watchdog] Restart error:', e);
  } finally {
    _watchdogRestartInProgress = false;
  }
}

// ── Status check (fallback for polling) ────────────────────────────────────
let _wasConnected = false;  // track state transition for crash detection

async function checkGatewayStatus() {
  if (_connectInProgress || gateway.isConnecting) return;

  // If we were connected and now we're not, it's a crash/disconnect
  if (_wasConnected && !wsConnected) {
    console.warn('[watchdog] Detected gateway disconnect during poll — triggering restart');
    _wasConnected = false;
    watchdogRestart();
    return;
  }

  // Track connection state for next poll
  _wasConnected = wsConnected;

  if (wsConnected) return;
  try {
    const ok = await probeHealth();
    if (ok && !wsConnected && !_connectInProgress) {
      await connectGateway();
    }
  } catch {
    // Try TCP check via Tauri
    const port = getPortFromUrl(config.gateway.url);
    const tcpAlive = invoke ? await invoke<boolean>('check_gateway_health', { port }).catch(() => false) : false;
    if (tcpAlive && !wsConnected) {
      await connectGateway();
    } else {
      statusDot?.classList.remove('connected');
      statusDot?.classList.add('error');
      if (statusText) statusText.textContent = 'Disconnected';
    }
  }
}

// ── Setup / Detect / Install handlers ──────────────────────────────────────
$('setup-detect')?.addEventListener('click', async () => {
  if (statusText) statusText.textContent = 'Detecting...';
  try {
    const installed = invoke ? await invoke<boolean>('check_openclaw_installed') : false;
    if (installed) {
      const token = invoke ? await invoke<string | null>('get_gateway_token') : null;
      if (token) {
        const cfgPort = invoke ? await invoke<number>('get_gateway_port_setting').catch(() => 18789) : 18789;
        config.configured = true;
        config.gateway.url = `http://127.0.0.1:${cfgPort}`;
        config.gateway.token = token;
        saveConfig();

        // Probe first — only start gateway if nothing is listening
        if (invoke) {
          const alreadyRunning = await invoke<boolean>('check_gateway_health', { port: cfgPort }).catch(() => false);
          if (!alreadyRunning) {
            await invoke('start_gateway', { port: cfgPort }).catch((e: unknown) => {
              console.warn('Gateway start failed (may already be running):', e);
            });
            await new Promise(r => setTimeout(r, 2000));
          }
        }

        await connectGateway();
        switchView('dashboard');
        return;
      }
    }
    showView('install-view');
  } catch (error) {
    console.error('Detection error:', error);
    alert('Could not detect OpenClaw. Try manual setup or install.');
  }
});

$('setup-manual')?.addEventListener('click', () => showView('manual-setup-view'));
$('setup-new')?.addEventListener('click', () => showView('install-view'));
$('gateway-back')?.addEventListener('click', () => showView('setup-view'));
$('install-back')?.addEventListener('click', () => showView('setup-view'));

// ── Install OpenClaw ───────────────────────────────────────────────────────
$('start-install')?.addEventListener('click', async () => {
  const progressBar = $('install-progress-bar') as HTMLElement;
  const progressText = $('install-progress-text') as HTMLElement;
  const installBtn = $('start-install') as HTMLButtonElement;
  installBtn.disabled = true;
  installBtn.textContent = 'Installing...';

  try {
    if (listen) {
      await listen<InstallProgress>('install-progress', (event: { payload: InstallProgress }) => {
        const { percent, message } = event.payload;
        if (progressBar) progressBar.style.width = `${percent}%`;
        if (progressText) progressText.textContent = message;
      });
    }
    if (invoke) await invoke('install_openclaw');

    const token = invoke ? await invoke<string | null>('get_gateway_token') : null;
    if (token) {
      const cfgPort = invoke ? await invoke<number>('get_gateway_port_setting').catch(() => 18789) : 18789;
      config.configured = true;
      config.gateway.url = `http://127.0.0.1:${cfgPort}`;
      config.gateway.token = token;
      saveConfig();
      await new Promise(r => setTimeout(r, 1000));
      await connectGateway();
      switchView('dashboard');
    }
  } catch (error) {
    console.error('Install error:', error);
    if (progressText) progressText.textContent = `Error: ${error}`;
    installBtn.disabled = false;
    installBtn.textContent = 'Retry Installation';
  }
});

// ── Gateway form (manual) ──────────────────────────────────────────────────
$('gateway-form')?.addEventListener('submit', async (e) => {
  e.preventDefault();
  const url = ($('gateway-url') as HTMLInputElement).value;
  const token = ($('gateway-token') as HTMLInputElement).value;
  try {
    config.configured = true;
    config.gateway = { url, token };
    saveConfig();
    const ok = await connectGateway();
    if (ok) {
      switchView('dashboard');
    } else {
      throw new Error('Connection failed');
    }
  } catch {
    alert('Could not connect to gateway. Check URL and try again.');
  }
});

// ── Config persistence ─────────────────────────────────────────────────────
function saveConfig() {
  localStorage.setItem('claw-config', JSON.stringify(config));
  setGatewayConfig(config.gateway.url, config.gateway.token);
}

function loadConfigFromStorage() {
  const saved = localStorage.getItem('claw-config');
  if (saved) {
    try { config = JSON.parse(saved); } catch { /* invalid */ }
  }
  setGatewayConfig(config.gateway.url, config.gateway.token);
}

/** Read live port+token from ~/.openclaw/openclaw.json via Tauri and update config */
async function refreshConfigFromDisk(): Promise<boolean> {
  if (!invoke) {
    console.log('[main] refreshConfigFromDisk: no Tauri runtime');
    return false;
  }
  try {
    const installed = await invoke<boolean>('check_openclaw_installed').catch(() => false);
    console.log(`[main] refreshConfigFromDisk: installed=${installed}`);
    if (!installed) return false;

    const token = await invoke<string | null>('get_gateway_token').catch((e) => {
      console.warn('[main] get_gateway_token invoke failed:', e);
      return null;
    });
    const port = await invoke<number>('get_gateway_port_setting').catch(() => 18789);

    const tokenMasked = token
      ? (token.length > 8 ? `${token.slice(0, 4)}...${token.slice(-4)}` : '****')
      : '(null)';
    console.log(`[main] refreshConfigFromDisk: port=${port} token=${tokenMasked} (${token?.length ?? 0} chars)`);

    if (token) {
      config.configured = true;
      config.gateway.url = `http://127.0.0.1:${port}`;
      config.gateway.token = token;
      saveConfig();
      console.log(`[main] Config updated: url=${config.gateway.url}`);
      return true;
    } else {
      console.warn('[main] No token found in config file — check ~/.openclaw/openclaw.json gateway.auth.token');
    }
  } catch (e) {
    console.warn('[main] Failed to read config from disk:', e);
  }
  return false;
}

// ── Settings form ──────────────────────────────────────────────────────────
function syncSettingsForm() {
  const urlInput = $('settings-gateway-url') as HTMLInputElement;
  const tokenInput = $('settings-gateway-token') as HTMLInputElement;
  if (urlInput) urlInput.value = config.gateway.url;
  if (tokenInput) tokenInput.value = config.gateway.token;
}

$('settings-save-gateway')?.addEventListener('click', async () => {
  const url = ($('settings-gateway-url') as HTMLInputElement).value;
  const token = ($('settings-gateway-token') as HTMLInputElement).value;
  config.gateway = { url, token };
  config.configured = true;
  saveConfig();

  gateway.disconnect();
  wsConnected = false;
  const ok = await connectGateway();
  if (ok) {
    alert('Connected successfully!');
  } else {
    alert('Settings saved but could not connect to gateway.');
  }
});

// ── Settings tab switching ──────────────────────────────────────────────────

let _activeSettingsTab = 'general';

/** Load data for whichever settings tab is currently active. */
function loadActiveSettingsTab() {
  switch (_activeSettingsTab) {
    case 'models': ModelsSettings.loadModelsSettings(); break;
    case 'env': EnvSettings.loadEnvSettings(); break;
    case 'agent-defaults': AgentDefaultsSettings.loadAgentDefaultsSettings(); break;
    case 'sessions': SessionsSettings.loadSessionsSettings(); break;
    case 'voice': VoiceSettings.loadVoiceSettings(); break;
    case 'advanced': AdvancedSettings.loadAdvancedSettings(); break;
    default: break; // general + security load via existing SettingsModule
  }
}

function initSettingsTabs() {
  const bar = $('settings-tab-bar');
  if (!bar) return;
  bar.addEventListener('click', (e) => {
    const btn = (e.target as HTMLElement).closest('.settings-tab') as HTMLElement | null;
    if (!btn) return;
    const tab = btn.dataset.settingsTab;
    if (!tab || tab === _activeSettingsTab) return;

    // Toggle active class
    bar.querySelectorAll('.settings-tab').forEach(b => b.classList.remove('active'));
    btn.classList.add('active');

    // Toggle panel visibility
    document.querySelectorAll('.settings-tab-panel').forEach(p => {
      (p as HTMLElement).style.display = 'none';
    });
    const panel = $(`settings-panel-${tab}`);
    if (panel) panel.style.display = '';

    _activeSettingsTab = tab;
    loadActiveSettingsTab();
  });
}

// ── Config editor (Settings > OpenClaw Configuration) ──────────────────────
async function loadGatewayConfig() {
  const section = $('settings-config-section');
  const editor = $('settings-config-editor') as HTMLTextAreaElement | null;
  const versionEl = $('settings-gateway-version');
  if (!wsConnected) {
    if (section) section.style.display = 'none';
    return;
  }
  try {
    const result = await gateway.configGet();
    if (section) section.style.display = '';
    if (editor) editor.value = JSON.stringify(result.config, null, 2);

    // Warn if config contains redacted placeholders
    const redactWarn = $('settings-config-redact-warning');
    const configStr = editor?.value ?? '';
    if (configStr.includes('__OPENCLAW_REDACTED__')) {
      if (!redactWarn) {
        // Create warning banner above the editor
        const warn = document.createElement('div');
        warn.id = 'settings-config-redact-warning';
        warn.className = 'config-redact-warning';
        warn.textContent = 'Some values are redacted by the gateway. Do not save without restoring them or those fields will be corrupted.';
        editor?.parentElement?.insertBefore(warn, editor);
      } else {
        redactWarn.style.display = '';
      }
    } else if (redactWarn) {
      redactWarn.style.display = 'none';
    }

    try {
      const h = await gateway.getHealth();
      if (versionEl) versionEl.textContent = `Gateway: ${h.ts ? 'up since ' + new Date(h.ts).toLocaleString() : 'running'}`;
    } catch { /* ignore */ }
  } catch (e) {
    console.warn('Config load failed:', e);
    if (section) section.style.display = 'none';
  }
}

$('settings-save-config')?.addEventListener('click', async () => {
  const editor = $('settings-config-editor') as HTMLTextAreaElement;
  if (!editor) return;
  try {
    const parsed = JSON.parse(editor.value);

    // Safety check: the gateway redacts sensitive values as "__OPENCLAW_REDACTED__".
    // If the user saves without restoring them, the config gets corrupted
    // (e.g. maxTokens becomes a string instead of a number).
    const configStr = JSON.stringify(parsed);
    if (configStr.includes('__OPENCLAW_REDACTED__')) {
      const proceed = confirm(
        'Warning: This config contains redacted values ("__OPENCLAW_REDACTED__"). ' +
        'Saving will write these placeholder strings into your config, which can break ' +
        'fields like API keys and model settings.\n\n' +
        'Either restore the original values or click Cancel to abort.\n\n' +
        'Save anyway?'
      );
      if (!proceed) return;
    }

    await gateway.configWrite(parsed);
    alert('Configuration saved!');
  } catch (e) {
    alert(`Invalid config: ${e instanceof Error ? e.message : e}`);
  }
});

// Apply Config (validate + write + restart — safer than configSet)
$('settings-apply-config')?.addEventListener('click', async () => {
  const editor = $('settings-config-editor') as HTMLTextAreaElement;
  if (!editor) return;
  try {
    const parsed = JSON.parse(editor.value);
    const configStr = JSON.stringify(parsed);
    if (configStr.includes('__OPENCLAW_REDACTED__')) {
      const proceed = confirm(
        'Warning: This config contains redacted values ("__OPENCLAW_REDACTED__"). ' +
        'Applying will write these placeholder strings into your config.\n\n' +
        'Save anyway?'
      );
      if (!proceed) return;
    }
    const result = await gateway.configApplyRaw(JSON.stringify(parsed, null, 2));
    if (result.errors?.length) {
      alert(`Config applied with warnings:\n${result.errors.join('\n')}`);
    } else {
      showToast(`Config applied${result.restarted ? ' — gateway restarting' : ''}`, 'success');
    }
  } catch (e) {
    alert(`Apply failed: ${e instanceof Error ? e.message : e}`);
  }
});

$('settings-reload-config')?.addEventListener('click', () => loadGatewayConfig());

// View config schema
$('settings-view-schema')?.addEventListener('click', async () => {
  if (!wsConnected) return;
  try {
    const result = await gateway.configSchema();
    const editor = $('settings-config-editor') as HTMLTextAreaElement;
    if (editor) {
      editor.value = JSON.stringify(result.schema ?? result, null, 2);
      showToast('Schema loaded — showing available config keys', 'info');
    }
  } catch (e) {
    showToast(`Schema load failed: ${e instanceof Error ? e.message : e}`, 'error');
  }
});

// Wake Agent button (dashboard)
$('wake-agent-btn')?.addEventListener('click', async () => {
  if (!wsConnected) { showToast('Not connected to gateway', 'error'); return; }
  try {
    await gateway.wake();
    showToast('Agent woken', 'success');
  } catch (e) {
    showToast(`Wake failed: ${e instanceof Error ? e.message : e}`, 'error');
  }
});

// ══════════════════════════════════════════════════════════════════════════
// ═══ DATA VIEWS ════════════════════════════════════════════════════════════
// ══════════════════════════════════════════════════════════════════════════

// ── Sessions / Chat ────────────────────────────────────────────────────────
async function loadSessions(opts?: { skipHistory?: boolean }) {
  if (!wsConnected) return;
  try {
    const result = await gateway.listSessions({ limit: 50, includeDerivedTitles: true, includeLastMessage: true });
    // Filter out internal paw-* sessions (memory palace background calls)
    sessions = (result.sessions ?? []).filter(s => !s.key.startsWith('paw-'));
    renderSessionSelect();
    if (!currentSessionKey && sessions.length) {
      currentSessionKey = sessions[0].key;
    }
    // Don't reload chat history if we're in the middle of streaming
    // or if the caller explicitly asked to skip (e.g. after sendMessage which already has local messages)
    if (!opts?.skipHistory && currentSessionKey && !isLoading) await loadChatHistory(currentSessionKey);
    // Populate mode picker from local DB
    populateModeSelect().catch(() => {});
  } catch (e) { console.warn('Sessions load failed:', e); }
}

function renderSessionSelect() {
  if (!chatSessionSelect) return;
  chatSessionSelect.innerHTML = '';
  if (!sessions.length) {
    const opt = document.createElement('option');
    opt.value = '';
    opt.textContent = 'No sessions';
    chatSessionSelect.appendChild(opt);
    return;
  }
  for (const s of sessions) {
    const opt = document.createElement('option');
    opt.value = s.key;
    opt.textContent = s.label ?? s.displayName ?? s.key;
    if (s.key === currentSessionKey) opt.selected = true;
    chatSessionSelect.appendChild(opt);
  }
}

chatSessionSelect?.addEventListener('change', () => {
  const key = chatSessionSelect?.value;
  if (key) {
    currentSessionKey = key;
    // Reset token meter for new session
    _sessionTokensUsed = 0;
    _sessionInputTokens = 0;
    _sessionOutputTokens = 0;
    _sessionCost = 0;
    _lastRecordedTotal = 0;
    _compactionDismissed = false;
    updateTokenMeter();
    const ba1 = $('session-budget-alert');
    if (ba1) ba1.style.display = 'none';
    loadChatHistory(key);
  }
});

/** Populate the mode picker dropdown in the chat header */
async function populateModeSelect() {
  const sel = $('chat-mode-select') as HTMLSelectElement | null;
  if (!sel) return;
  const modes = await listModes();
  sel.innerHTML = '<option value="">Default</option>';
  for (const m of modes) {
    const opt = document.createElement('option');
    opt.value = m.id;
    opt.textContent = m.name;
    sel.appendChild(opt);
  }
}

async function loadChatHistory(sessionKey: string) {
  if (!wsConnected) return;
  try {
    const result = await gateway.chatHistory(sessionKey);
    messages = (result.messages ?? []).map(chatMsgToMessage);
    renderMessages();
  } catch (e) {
    console.warn('Chat history load failed:', e);
    messages = [];
    renderMessages();
  }
}

/** Extract readable text from Anthropic-style content blocks or plain strings */
function extractContent(content: unknown): string {
  if (typeof content === 'string') return content;
  if (Array.isArray(content)) {
    return content
      .filter((block: Record<string, unknown>) => block.type === 'text' && typeof block.text === 'string')
      .map((block: Record<string, unknown>) => block.text as string)
      .join('\n');
  }
  if (content && typeof content === 'object') {
    const obj = content as Record<string, unknown>;
    if (obj.type === 'text' && typeof obj.text === 'string') return obj.text;
  }
  if (content == null) return '';
  return String(content);
}

function chatMsgToMessage(m: ChatMessage): MessageWithAttachments {
  const ts = m.ts ?? m.timestamp;
  const result: MessageWithAttachments = {
    id: m.id ?? undefined,
    role: m.role as 'user' | 'assistant' | 'system',
    content: extractContent(m.content),
    timestamp: ts ? new Date(ts as string | number) : new Date(),
    toolCalls: m.toolCalls,
  };
  // Carry attachments through if present on the gateway message
  const raw = m as Record<string, unknown>;
  if (Array.isArray(raw.attachments) && raw.attachments.length > 0) {
    result.attachments = raw.attachments as ChatAttachmentLocal[];
  }
  return result;
}

// Chat send
chatSend?.addEventListener('click', sendMessage);
chatInput?.addEventListener('keydown', (e) => {
  if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendMessage(); }
});
chatInput?.addEventListener('input', () => {
  if (chatInput) {
    chatInput.style.height = 'auto';
    chatInput.style.height = Math.min(chatInput.scrollHeight, 120) + 'px';
  }
});

// ── Attachment picker ────────────────────────────────────────────────────────
const chatAttachBtn = $('chat-attach-btn');
const chatFileInput = $('chat-file-input') as HTMLInputElement | null;
const chatAttachmentPreview = $('chat-attachment-preview');

chatAttachBtn?.addEventListener('click', () => chatFileInput?.click());

chatFileInput?.addEventListener('change', () => {
  if (!chatFileInput.files) return;
  for (const file of Array.from(chatFileInput.files)) {
    _pendingAttachments.push(file);
  }
  chatFileInput.value = ''; // reset so same file can be re-selected
  renderAttachmentPreview();
});

/** Get the right icon name for a file type */
function fileTypeIcon(mimeType: string): string {
  if (mimeType.startsWith('image/')) return 'image';
  if (mimeType === 'application/pdf' || mimeType.startsWith('text/')) return 'file-text';
  return 'file';
}

function renderAttachmentPreview() {
  if (!chatAttachmentPreview) return;
  if (_pendingAttachments.length === 0) {
    chatAttachmentPreview.style.display = 'none';
    chatAttachmentPreview.innerHTML = '';
    return;
  }
  chatAttachmentPreview.style.display = 'flex';
  chatAttachmentPreview.innerHTML = '';
  for (let i = 0; i < _pendingAttachments.length; i++) {
    const file = _pendingAttachments[i];
    const chip = document.createElement('div');
    chip.className = 'attachment-chip';
    if (file.type.startsWith('image/')) {
      const img = document.createElement('img');
      img.src = URL.createObjectURL(file);
      img.className = 'attachment-chip-thumb';
      img.onload = () => URL.revokeObjectURL(img.src);
      chip.appendChild(img);
    } else {
      const iconWrap = document.createElement('span');
      iconWrap.className = 'attachment-chip-icon';
      iconWrap.innerHTML = icon(fileTypeIcon(file.type));
      chip.appendChild(iconWrap);
    }
    const meta = document.createElement('div');
    meta.className = 'attachment-chip-meta';
    const nameEl = document.createElement('span');
    nameEl.className = 'attachment-chip-name';
    nameEl.textContent = file.name.length > 24 ? file.name.slice(0, 21) + '...' : file.name;
    nameEl.title = file.name;
    meta.appendChild(nameEl);
    const sizeEl = document.createElement('span');
    sizeEl.className = 'attachment-chip-size';
    sizeEl.textContent = file.size < 1024 ? `${file.size} B` : file.size < 1048576 ? `${(file.size / 1024).toFixed(1)} KB` : `${(file.size / 1048576).toFixed(1)} MB`;
    meta.appendChild(sizeEl);
    chip.appendChild(meta);
    const removeBtn = document.createElement('button');
    removeBtn.className = 'attachment-chip-remove';
    removeBtn.innerHTML = icon('x');
    removeBtn.title = 'Remove';
    const idx = i;
    removeBtn.addEventListener('click', () => {
      _pendingAttachments.splice(idx, 1);
      renderAttachmentPreview();
    });
    chip.appendChild(removeBtn);
    chatAttachmentPreview.appendChild(chip);
  }
}

function clearPendingAttachments() {
  _pendingAttachments = [];
  renderAttachmentPreview();
}

$('new-chat-btn')?.addEventListener('click', () => {
  messages = [];
  currentSessionKey = null;
  // Reset token meter for new conversation
  _sessionTokensUsed = 0;
  _sessionInputTokens = 0;
  _sessionOutputTokens = 0;
  _sessionCost = 0;
  _lastRecordedTotal = 0;
  _compactionDismissed = false;
  updateTokenMeter();
  const ba2 = $('session-budget-alert');
  if (ba2) ba2.style.display = 'none';
  renderMessages();
  if (chatSessionSelect) chatSessionSelect.value = '';
});

// Abort running agent
$('chat-abort-btn')?.addEventListener('click', async () => {
  const key = currentSessionKey ?? 'default';
  try {
    await gateway.chatAbort(key, _streamingRunId ?? undefined);
    showToast('Agent stopped', 'info');
  } catch (e) {
    console.warn('[main] Abort failed:', e);
  }
  // Let the lifecycle 'end' event or timeout finalize the stream
});

// Session rename
$('session-rename-btn')?.addEventListener('click', async () => {
  if (!currentSessionKey || !wsConnected) return;
  const name = await promptModal('Rename session', 'New name…');
  if (!name) return;
  try {
    await gateway.patchSession(currentSessionKey, { label: name });
    showToast('Session renamed', 'success');
    await loadSessions();
  } catch (e) {
    showToast(`Rename failed: ${e instanceof Error ? e.message : e}`, 'error');
  }
});

// Session delete
$('session-delete-btn')?.addEventListener('click', async () => {
  if (!currentSessionKey || !wsConnected) return;
  if (!confirm('Delete this session? This cannot be undone.')) return;
  try {
    await gateway.deleteSession(currentSessionKey);
    currentSessionKey = null;
    messages = [];
    renderMessages();
    showToast('Session deleted', 'success');
    await loadSessions();
  } catch (e) {
    showToast(`Delete failed: ${e instanceof Error ? e.message : e}`, 'error');
  }
});

// Session clear history (reset)
$('session-clear-btn')?.addEventListener('click', async () => {
  if (!currentSessionKey || !wsConnected) return;
  if (!confirm('Clear all messages in this session? The session itself will remain.')) return;
  try {
    await gateway.resetSession(currentSessionKey);
    messages = [];
    renderMessages();
    showToast('Session history cleared', 'success');
  } catch (e) {
    showToast(`Clear failed: ${e instanceof Error ? e.message : e}`, 'error');
  }
});

// Session compact (compress storage)
$('session-compact-btn')?.addEventListener('click', async () => {
  if (!wsConnected) return;
  try {
    const result = await gateway.sessionsCompact(currentSessionKey ?? undefined);
    showToast(`Compacted${result.removed ? ` — removed ${result.removed} entries` : ''}`, 'success');
    // Reset token meter after compaction
    _sessionTokensUsed = 0;
    _sessionInputTokens = 0;
    _sessionOutputTokens = 0;
    _sessionCost = 0;
    _lastRecordedTotal = 0;
    _compactionDismissed = false;
    updateTokenMeter();
    const budgetAlert = $('session-budget-alert');
    if (budgetAlert) budgetAlert.style.display = 'none';
  } catch (e) {
    showToast(`Compact failed: ${e instanceof Error ? e.message : e}`, 'error');
  }
});

// ── Token Meter ────────────────────────────────────────────────────────────

/** Update the token meter bar + label in the chat header */
function updateTokenMeter() {
  const meter = $('token-meter');
  const fill = $('token-meter-fill');
  const label = $('token-meter-label');
  if (!meter || !fill || !label) return;

  // Always show the meter so users know tracking is active
  meter.style.display = '';

  if (_sessionTokensUsed <= 0) {
    fill.style.width = '0%';
    fill.className = 'token-meter-fill';
    const limit = _modelContextLimit >= 1000 ? `${(_modelContextLimit / 1000).toFixed(0)}k` : `${_modelContextLimit}`;
    label.textContent = `0 / ${limit} tokens`;
    meter.title = 'Token tracking active — send a message to see usage';
    return;
  }

  const pct = Math.min((_sessionTokensUsed / _modelContextLimit) * 100, 100);
  fill.style.width = `${pct}%`;

  // Color coding: green < 60%, yellow 60-80%, red > 80%
  if (pct >= 80) {
    fill.className = 'token-meter-fill danger';
  } else if (pct >= 60) {
    fill.className = 'token-meter-fill warning';
  } else {
    fill.className = 'token-meter-fill';
  }

  // Label: "12.4k / 128k tokens"
  const used = _sessionTokensUsed >= 1000 ? `${(_sessionTokensUsed / 1000).toFixed(1)}k` : `${_sessionTokensUsed}`;
  const limit = _modelContextLimit >= 1000 ? `${(_modelContextLimit / 1000).toFixed(0)}k` : `${_modelContextLimit}`;
  const costStr = _sessionCost > 0 ? ` · $${_sessionCost.toFixed(4)}` : '';
  label.textContent = `${used} / ${limit} tokens${costStr}`;
  meter.title = `Session tokens: ${_sessionTokensUsed.toLocaleString()} / ${_modelContextLimit.toLocaleString()} (In: ${_sessionInputTokens.toLocaleString()} / Out: ${_sessionOutputTokens.toLocaleString()}) — Est. cost: $${_sessionCost.toFixed(4)}`;

  // Compaction warning
  updateCompactionWarning(pct);
}

/** Show/hide compaction warning based on context fill percentage */
function updateCompactionWarning(pct: number) {
  const warning = $('compaction-warning');
  if (!warning) return;

  if (pct >= COMPACTION_WARN_THRESHOLD * 100 && !_compactionDismissed) {
    warning.style.display = '';
    const text = $('compaction-warning-text');
    if (text) {
      if (pct >= 95) {
        text.textContent = `Context window ${pct.toFixed(0)}% full — messages will be compacted imminently`;
      } else {
        text.textContent = `Context window ${pct.toFixed(0)}% full — older messages may be compacted soon`;
      }
    }
  } else {
    warning.style.display = 'none';
  }
}

/** Record token usage from a chat/agent event and update the meter */
function recordTokenUsage(usage: Record<string, unknown> | undefined) {
  if (!usage) return;
  // Try multiple paths — different providers nest usage differently
  const uAny = usage as Record<string, unknown>;
  const nested = (uAny.response as Record<string, unknown> | undefined);
  const inner = (uAny.usage ?? nested?.usage ?? usage) as Record<string, unknown>;
  const totalTokens = (inner.totalTokens ?? inner.total_tokens ?? inner.totalTokenCount ?? 0) as number;
  const inputTokens = (inner.promptTokens ?? inner.prompt_tokens ?? inner.inputTokens ?? inner.input_tokens ?? inner.prompt_token_count ?? 0) as number;
  const outputTokens = (inner.completionTokens ?? inner.completion_tokens ?? inner.outputTokens ?? inner.output_tokens ?? inner.completion_token_count ?? 0) as number;

  if (totalTokens > 0) {
    _sessionInputTokens += inputTokens;
    _sessionOutputTokens += outputTokens;
    _sessionTokensUsed += totalTokens;
    _lastRecordedTotal = _sessionTokensUsed;
  } else if (inputTokens > 0 || outputTokens > 0) {
    _sessionInputTokens += inputTokens;
    _sessionOutputTokens += outputTokens;
    _sessionTokensUsed += inputTokens + outputTokens;
    _lastRecordedTotal = _sessionTokensUsed;
  }

  // Estimate cost from token counts
  const rate = _MODEL_COST_PER_TOKEN[_activeModelKey] ?? _MODEL_COST_PER_TOKEN['default'];
  _sessionCost += inputTokens * rate.input + outputTokens * rate.output;

  // Check budget after each usage recording
  const budgetLimit = SettingsModule.getBudgetLimit();
  if (budgetLimit != null && _sessionCost >= budgetLimit * 0.8) {
    const budgetAlert = $('session-budget-alert');
    if (budgetAlert) {
      budgetAlert.style.display = '';
      const alertText = $('session-budget-alert-text');
      if (alertText) {
        if (_sessionCost >= budgetLimit) {
          alertText.textContent = `Session budget exceeded: $${_sessionCost.toFixed(4)} / $${budgetLimit.toFixed(2)}`;
        } else {
          alertText.textContent = `Nearing session budget: $${_sessionCost.toFixed(4)} / $${budgetLimit.toFixed(2)}`;
        }
      }
    }
  }

  updateTokenMeter();
}

/** Try to detect model context limit from models.list or health data */
async function detectModelContextLimit() {
  try {
    const result = await gateway.modelsList();
    const models = (result as unknown as Record<string, unknown>).models as Array<Record<string, unknown>> | undefined;
    if (models && models.length > 0) {
      // Find the active/default model's context window
      for (const m of models) {
        const ctx = (m.contextWindow ?? m.context_window ?? m.maxTokens ?? m.max_tokens) as number | undefined;
        if (ctx && ctx > 0) {
          _modelContextLimit = ctx;
          console.log(`[token-meter] Detected model context limit: ${ctx}`);
          // Detect model key for cost estimation
          const name = ((m.name ?? m.id ?? m.model ?? '') as string).toLowerCase();
          for (const key of Object.keys(_MODEL_COST_PER_TOKEN)) {
            if (key !== 'default' && name.includes(key)) {
              _activeModelKey = key;
              console.log(`[token-meter] Matched model cost key: ${key}`);
              break;
            }
          }
          break;
        }
      }
    }
  } catch {
    // Keep default
  }
}

// Dismiss compaction warning
$('compaction-warning-dismiss')?.addEventListener('click', () => {
  _compactionDismissed = true;
  const warning = $('compaction-warning');
  if (warning) warning.style.display = 'none';
});

async function sendMessage() {
  const content = chatInput?.value.trim();
  if (!content || isLoading) return;

  // Convert pending File[] to attachments (matching webchat format)
  const attachments: Array<{ type: string; mimeType: string; content: string }> = [];
  if (_pendingAttachments.length > 0) {
    for (const file of _pendingAttachments) {
      try {
        const base64 = await fileToBase64(file);
        attachments.push({
          type: 'image',
          mimeType: file.type || 'image/png',
          content: base64,
        });
      } catch (e) {
        console.error('[Chat] Failed to encode attachment:', file.name, e);
      }
    }
  }

  // Add user message with attachments to UI
  const userMsg: import('./types').Message = { role: 'user', content, timestamp: new Date() };
  if (attachments.length > 0) {
    (userMsg as unknown as Record<string, unknown>).attachments = attachments.map(a => ({
      mimeType: a.mimeType,
      data: a.content,
    }));
  }
  addMessage(userMsg);
  if (chatInput) { chatInput.value = ''; chatInput.style.height = 'auto'; }
  clearPendingAttachments();
  isLoading = true;
  if (chatSend) chatSend.disabled = true;

  // Prepare streaming UI
  _streamingContent = '';
  _streamingRunId = null;
  showStreamingMessage();

  // chat.send is async — it returns {runId, status:"started"} immediately.
  // The actual response arrives via 'agent' events (deltas) and 'chat' events.
  // We create a promise that resolves when the agent 'done' event fires.
  const responsePromise = new Promise<string>((resolve) => {
    _streamingResolve = resolve;
    // Safety: auto-resolve after 120s to prevent permanent hang
    _streamingTimeout = setTimeout(() => {
      console.warn('[main] Streaming timeout — auto-finalizing');
      resolve(_streamingContent || '(Response timed out)');
    }, 120_000);
  });

  // Declare chatOpts outside try — Safari/WebKit has TDZ bugs with let-in-try
  let chatOpts: {
    model?: string;
    thinkingLevel?: string;
    temperature?: number;
    attachments?: import('./types').ChatAttachment[];
    agentProfile?: Partial<import('./types').Agent>;
  } = {};

  try {
    const sessionKey = currentSessionKey ?? 'default';

    // Read selected mode's overrides (model, thinking level)
    const modeSelect = $('chat-mode-select') as HTMLSelectElement | null;
    const selectedModeId = modeSelect?.value;
    if (selectedModeId) {
      const modes = await listModes();
      const mode = modes.find(m => m.id === selectedModeId);
      if (mode) {
        if (mode.model) chatOpts.model = mode.model;
        if (mode.thinking_level) chatOpts.thinkingLevel = mode.thinking_level;
        if (mode.temperature > 0) chatOpts.temperature = mode.temperature;
      }
    }

    // -- Agent Profile Injection --
    // Get the current agent and inject its profile into the options
    const currentAgent = AgentsModule.getCurrentAgent();
    if (currentAgent) {
      // Use the agent's model if it's not the default
      if (currentAgent.model && currentAgent.model !== 'default') {
        chatOpts.model = currentAgent.model;
      }
      // Pass the full profile to be constructed into a system prompt by the gateway client
      (chatOpts as Record<string, unknown>).agentProfile = currentAgent;
      console.log(`[main] Injecting agent profile for "${currentAgent.name}"`, currentAgent);
    }
    
    // Include attachments if any
    if (attachments.length > 0) {
      chatOpts.attachments = attachments;
      console.log('[main] Sending attachments:', attachments.length, 'items, first mimeType:', attachments[0]?.mimeType, 'content length:', attachments[0]?.content?.length);
    }

    const result = await gateway.chatSend(sessionKey, content, chatOpts);
    console.log('[main] chat.send ack:', JSON.stringify(result).slice(0, 300));

    // Store the runId so we can filter events precisely
    if (result.runId) _streamingRunId = result.runId;
    if (result.sessionKey) currentSessionKey = result.sessionKey;

    // Try to extract usage from the send response itself
    const sendUsage = (result as unknown as Record<string, unknown>).usage as Record<string, unknown> | undefined;
    if (sendUsage) recordTokenUsage(sendUsage);

    // Some gateway modes return the full response in the chat.send ack
    // (non-streaming / sync mode). If so, resolve immediately.
    const resultAny = result as unknown as Record<string, unknown>;
    const ackText = result.text
      ?? (typeof resultAny.response === 'string' ? resultAny.response as string : null)
      ?? extractContent(resultAny.response);
    if (ackText && _streamingResolve) {
      console.log(`[main] chat.send ack contained response (${ackText.length} chars) — resolving`);
      appendStreamingDelta(ackText);
      _streamingResolve(ackText);
      _streamingResolve = null;
    }

    // Now wait for the agent events to deliver the full response
    const finalText = await responsePromise;
    finalizeStreaming(finalText);
    // Refresh session list only — don't re-load chat history since we already have
    // the local messages array up to date from the streaming pipeline
    loadSessions({ skipHistory: true }).catch(() => {});
  } catch (error) {
    console.error('Chat error:', error);
    // Don't show raw WS errors like "connection closed" — the disconnect handler
    // already resolved with partial content. Only finalize if not already done.
    if (_streamingEl) {
      const errMsg = error instanceof Error ? error.message : 'Failed to get response';
      finalizeStreaming(_streamingContent || `Error: ${errMsg}`);
    }
  } finally {
    isLoading = false;
    _streamingRunId = null;
    _streamingResolve = null;
    if (_streamingTimeout) { clearTimeout(_streamingTimeout); _streamingTimeout = null; }
    if (chatSend) chatSend.disabled = false;
  }
}

/** Show an empty assistant bubble for streaming content */
function showStreamingMessage() {
  if (chatEmpty) chatEmpty.style.display = 'none';
  const div = document.createElement('div');
  div.className = 'message assistant';
  div.id = 'streaming-message';
  const contentEl = document.createElement('div');
  contentEl.className = 'message-content';
  contentEl.innerHTML = '<div class="loading-dots"><span></span><span></span><span></span></div>';
  const time = document.createElement('div');
  time.className = 'message-time';
  time.textContent = new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  div.appendChild(contentEl);
  div.appendChild(time);
  chatMessages?.appendChild(div);
  _streamingEl = contentEl;
  // Show abort button
  const abortBtn = $('chat-abort-btn');
  if (abortBtn) abortBtn.style.display = '';
  scrollToBottom();
}

/** Append a text delta to the streaming bubble */
let _scrollRafPending = false;
function scrollToBottom() {
  if (_scrollRafPending || !chatMessages) return;
  _scrollRafPending = true;
  requestAnimationFrame(() => {
    if (chatMessages) chatMessages.scrollTop = chatMessages.scrollHeight;
    _scrollRafPending = false;
  });
}

function appendStreamingDelta(text: string) {
  _streamingContent += text;
  if (_streamingEl) {
    // During streaming: render markdown so user gets formatted output live
    _streamingEl.innerHTML = formatMarkdown(_streamingContent);
    scrollToBottom();
  }
}

/** Finalize streaming: replace the live bubble with a permanent message */
function finalizeStreaming(finalContent: string, toolCalls?: import('./types').ToolCall[]) {
  // Remove the streaming element
  $('streaming-message')?.remove();
  _streamingEl = null;
  const savedRunId = _streamingRunId;
  _streamingRunId = null;
  _streamingContent = '';
  // Hide abort button
  const abortBtn = $('chat-abort-btn');
  if (abortBtn) abortBtn.style.display = 'none';

  if (finalContent) {
    addMessage({ role: 'assistant', content: finalContent, timestamp: new Date(), toolCalls });

    // Fallback token estimation: if no real usage data came through events,
    // estimate from character count (~4 chars per token is a rough average).
    // This ensures the token meter always shows something useful.
    if (_sessionTokensUsed === 0 || _lastRecordedTotal === _sessionTokensUsed) {
      const userMsg = messages.filter(m => m.role === 'user').pop();
      const userChars = userMsg?.content?.length ?? 0;
      const assistantChars = finalContent.length;
      const estInput = Math.ceil(userChars / 4);
      const estOutput = Math.ceil(assistantChars / 4);
      _sessionInputTokens += estInput;
      _sessionOutputTokens += estOutput;
      _sessionTokensUsed += estInput + estOutput;
      // Estimate cost
      const rate = _MODEL_COST_PER_TOKEN[_activeModelKey] ?? _MODEL_COST_PER_TOKEN['default'];
      _sessionCost += estInput * rate.input + estOutput * rate.output;
      console.log(`[token-meter] Fallback estimate: ~${estInput + estOutput} tokens (${userChars + assistantChars} chars)`);
      updateTokenMeter();
    }
  } else {
    // No content received — try fetching the latest history from the gateway
    // to recover the response (it may have been stored server-side even though
    // the streaming events didn't deliver it).
    console.warn(`[main] finalizeStreaming called with empty content (runId=${savedRunId?.slice(0,12) ?? 'null'}). Fetching history fallback...`);
    const sk = currentSessionKey;
    if (sk) {
      gateway.chatHistory(sk).then(hist => {
        const msgs = hist.messages ?? [];
        // Find the last assistant message
        for (let i = msgs.length - 1; i >= 0; i--) {
          if (msgs[i].role === 'assistant') {
            const text = extractContent(msgs[i].content);
            if (text) {
              console.log(`[main] History fallback recovered ${text.length} chars`);
              addMessage({ role: 'assistant', content: text, timestamp: new Date() });
              return;
            }
            break;
          }
        }
        console.warn('[main] History fallback: no usable assistant message found');
        addMessage({ role: 'assistant', content: '*(No response received — the agent may not be configured or the model returned empty output)*', timestamp: new Date() });
      }).catch(e => {
        console.warn('[main] History fallback failed:', e);
        addMessage({ role: 'assistant', content: '*(No response received)*', timestamp: new Date() });
      });
    } else {
      addMessage({ role: 'assistant', content: '*(No response received)*', timestamp: new Date() });
    }
  }
}

function addMessage(message: MessageWithAttachments) {
  messages.push(message);
  renderMessages();
}

/** Find last index matching predicate */
function findLastIndex<T>(arr: T[], pred: (item: T) => boolean): number {
  for (let i = arr.length - 1; i >= 0; i--) {
    if (pred(arr[i])) return i;
  }
  return -1;
}

/** Retry a message: remove everything from the retried user message onward and resend */
function retryMessage(content: string) {
  if (isLoading || !content) return;
  // Remove last user message + any assistant reply after it
  const lastUserIdx = findLastIndex(messages, m => m.role === 'user');
  if (lastUserIdx >= 0) {
    messages.splice(lastUserIdx);
  }
  renderMessages();
  // Inject content into input and send
  if (chatInput) {
    chatInput.value = content;
    chatInput.style.height = 'auto';
  }
  sendMessage();
}

function renderMessages() {
  if (!chatMessages) return;
  // Remove only non-streaming message elements
  chatMessages.querySelectorAll('.message:not(#streaming-message)').forEach(m => m.remove());

  if (messages.length === 0) {
    if (chatEmpty) chatEmpty.style.display = 'flex';
    return;
  }
  if (chatEmpty) chatEmpty.style.display = 'none';

  // Build all nodes in a fragment to avoid repeated reflows
  const frag = document.createDocumentFragment();
  const lastUserIdx = findLastIndex(messages, m => m.role === 'user');
  const lastAssistantIdx = findLastIndex(messages, m => m.role === 'assistant');
  for (let i = 0; i < messages.length; i++) {
    const msg = messages[i];
    const div = document.createElement('div');
    div.className = `message ${msg.role}`;
    const contentEl = document.createElement('div');
    contentEl.className = 'message-content';
    // Render markdown for assistant messages, plain text for user
    if (msg.role === 'assistant' || msg.role === 'system') {
      contentEl.innerHTML = formatMarkdown(msg.content);
    } else {
      contentEl.textContent = msg.content;
    }
    const time = document.createElement('div');
    time.className = 'message-time';
    time.textContent = msg.timestamp.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    div.appendChild(contentEl);

    // Render image attachments inline if present
    if (msg.attachments?.length) {
      const attachStrip = document.createElement('div');
      attachStrip.className = 'message-attachments';
      for (const att of msg.attachments) {
        if (att.mimeType?.startsWith('image/')) {
          const card = document.createElement('div');
          card.className = 'message-attachment-card';
          const img = document.createElement('img');
          img.className = 'message-attachment-img';
          img.alt = att.name || 'attachment';
          if (att.url) {
            img.src = att.url;
          } else if (att.data) {
            img.src = `data:${att.mimeType};base64,${att.data}`;
          }
          card.appendChild(img);
          const overlay = document.createElement('div');
          overlay.className = 'message-attachment-overlay';
          overlay.innerHTML = icon('external-link');
          card.appendChild(overlay);
          card.addEventListener('click', () => window.open(img.src, '_blank'));
          if (att.name) {
            const label = document.createElement('div');
            label.className = 'message-attachment-label';
            label.textContent = att.name;
            card.appendChild(label);
          }
          attachStrip.appendChild(card);
        } else {
          const docChip = document.createElement('div');
          docChip.className = 'message-attachment-doc';
          const iconName = att.mimeType?.startsWith('text/') || att.mimeType === 'application/pdf' ? 'file-text' : 'file';
          docChip.innerHTML = `${icon(iconName)}<span>${att.name || 'file'}</span>`;
          attachStrip.appendChild(docChip);
        }
      }
      div.appendChild(attachStrip);
    }

    div.appendChild(time);

    if (msg.toolCalls?.length) {
      const badge = document.createElement('div');
      badge.className = 'tool-calls-badge';
      badge.innerHTML = `${icon('wrench')} ${msg.toolCalls.length} tool call${msg.toolCalls.length > 1 ? 's' : ''}`;
      div.appendChild(badge);
    }

    // Retry button on the last user message, and on the last assistant message if it errored
    const isLastUser = i === lastUserIdx;
    const isErroredAssistant = i === lastAssistantIdx && msg.content.startsWith('Error:');
    if ((isLastUser || isErroredAssistant) && !isLoading) {
      const retryBtn = document.createElement('button');
      retryBtn.className = 'message-retry-btn';
      retryBtn.title = 'Retry';
      retryBtn.innerHTML = `${icon('rotate-ccw')} Retry`;
      const retryContent = isLastUser ? msg.content : (lastUserIdx >= 0 ? messages[lastUserIdx].content : '');
      retryBtn.addEventListener('click', () => retryMessage(retryContent));
      div.appendChild(retryBtn);
    }

    frag.appendChild(div);
  }
  // Insert before any streaming message, or at end
  const streamingEl = $('streaming-message');
  if (streamingEl) {
    chatMessages.insertBefore(frag, streamingEl);
  } else {
    chatMessages.appendChild(frag);
  }
  scrollToBottom();
}

// Listen for streaming agent events — update chat bubble in real-time
// Actual format: { runId, stream: "assistant"|"lifecycle"|"tool", data: {...}, sessionKey, seq, ts }
gateway.on('agent', (payload: unknown) => {
  try {
    const evt = payload as Record<string, unknown>;
    const stream = evt.stream as string | undefined;
    const data = evt.data as Record<string, unknown> | undefined;
    const runId = evt.runId as string | undefined;
    const evtSession = evt.sessionKey as string | undefined;

    // Debug: log routing state for non-delta events
    if (stream !== 'assistant') {
      console.log(`[main] agent evt: stream=${stream} session=${evtSession} runId=${String(runId).slice(0,12)} isLoading=${isLoading} hasStreamingEl=${!!_streamingEl} streamingRunId=${_streamingRunId?.slice(0,12) ?? 'null'}`);
    }

    // Route paw-research-* events to the Research view
    if (evtSession && evtSession.startsWith('paw-research-')) {
      if (!ResearchModule.isStreaming()) return;
      if (ResearchModule.getRunId() && runId && runId !== ResearchModule.getRunId()) return;

      if (stream === 'assistant' && data) {
        const delta = data.delta as string | undefined;
        if (delta) ResearchModule.appendDelta(delta);
      } else if (stream === 'lifecycle' && data) {
        const phase = data.phase as string | undefined;
        if (phase === 'end') ResearchModule.resolveStream();
      } else if (stream === 'tool' && data) {
        const tool = (data.name ?? data.tool) as string | undefined;
        const phase = data.phase as string | undefined;
        if (phase === 'start' && tool) ResearchModule.appendDelta(`\n\n▶ ${tool}...`);
      } else if (stream === 'error' && data) {
        const error = (data.message ?? data.error ?? '') as string;
        if (error) ResearchModule.appendDelta(`\n\nError: ${error}`);
        ResearchModule.resolveStream();
      }
      return;
    }

    // Route paw-build-* events to the Build view
    if (evtSession && evtSession.startsWith('paw-build')) {
      if (!_buildStreaming) return;
      if (_buildStreamRunId && runId && runId !== _buildStreamRunId) return;
      if (stream === 'assistant' && data) {
        const delta = data.delta as string | undefined;
        if (delta) {
          _buildStreamContent += delta;
          const el = $('build-chat-messages');
          const lastMsg = el?.querySelector('.message.assistant:last-child .message-content');
          if (lastMsg) lastMsg.innerHTML = formatMarkdown(_buildStreamContent);
        }
      } else if (stream === 'lifecycle' && data) {
        const phase = data.phase as string | undefined;
        if (phase === 'start' && !_buildStreamRunId && runId) _buildStreamRunId = runId;
        if (phase === 'end' && _buildStreamResolve) { _buildStreamResolve(_buildStreamContent); _buildStreamResolve = null; }
      } else if (stream === 'error' && data) {
        const error = (data.message ?? data.error ?? '') as string;
        if (error) _buildStreamContent += `\n\nError: ${error}`;
        if (_buildStreamResolve) { _buildStreamResolve(_buildStreamContent); _buildStreamResolve = null; }
      }
      return;
    }

    // Filter: ignore other background paw-* sessions (e.g. memory)
    if (evtSession && evtSession.startsWith('paw-')) return;

    // Filter: only process during active send, and match runId if known
    if (!isLoading && !_streamingEl) return;
    if (_streamingRunId && runId && runId !== _streamingRunId) return;

    if (stream === 'assistant' && data) {
      // data.delta = incremental text, data.text = accumulated text so far
      const delta = data.delta as string | undefined;
      if (delta) {
        appendStreamingDelta(delta);
      }
    } else if (stream === 'lifecycle' && data) {
      const phase = data.phase as string | undefined;
      if (phase === 'start') {
        if (!_streamingRunId && runId) _streamingRunId = runId;
        console.log(`[main] Agent run started: ${runId}`);
      } else if (phase === 'end') {
        console.log(`[main] Agent run ended: ${runId} content-length=${_streamingContent.length}`, data ? JSON.stringify(data).slice(0, 500) : '(no data)');
        // Capture usage from lifecycle end event — try multiple paths
        const dAny = data as Record<string, unknown>;
        const dNested = (dAny.response as Record<string, unknown> | undefined);
        const agentUsage = (dAny.usage ?? dNested?.usage ?? data) as Record<string, unknown> | undefined;
        recordTokenUsage(agentUsage);
        // Also try root-level usage on the event itself
        const evtUsage = (evt as Record<string, unknown>).usage as Record<string, unknown> | undefined;
        if (evtUsage) recordTokenUsage(evtUsage);
        if (_streamingResolve) {
          // If we already have content from deltas, resolve immediately.
          // Otherwise give the 'chat' final event a 3s grace period to arrive
          // (the chat.final event carries the assembled response and may arrive
          // slightly after the lifecycle-end event).
          if (_streamingContent) {
            console.log('[main] Resolving with streamed content:', _streamingContent.length, 'chars');
            _streamingResolve(_streamingContent);
            _streamingResolve = null;
          } else {
            console.log('[main] No content at lifecycle end — waiting 3s for chat.final event...');
            const savedResolve = _streamingResolve;
            setTimeout(() => {
              // Only resolve if the chat.final handler hasn't already done it
              if (_streamingResolve === savedResolve && _streamingResolve) {
                console.warn('[main] Grace period expired — resolving with empty content');
                _streamingResolve(_streamingContent || '');
                _streamingResolve = null;
              }
            }, 3000);
          }
        }
      }
    } else if (stream === 'tool' && data) {
      const tool = (data.name ?? data.tool) as string | undefined;
      const phase = data.phase as string | undefined;
      if (phase === 'start' && tool) {
        console.log(`[main] Tool: ${tool}`);
        if (_streamingEl) appendStreamingDelta(`\n\n▶ ${tool}...`);
      }
    } else if (stream === 'error' && data) {
      const error = (data.message ?? data.error ?? '') as string;
      console.error(`[main] Agent error: ${error}`);
      crashLog(`agent-error: ${error}`);
      if (error && _streamingEl) appendStreamingDelta(`\n\nError: ${error}`);
      if (_streamingResolve) {
        _streamingResolve(_streamingContent);
        _streamingResolve = null;
      }
    }
  } catch (e) {
    console.warn('[main] Agent event handler error:', e);
  }
});

// Listen for chat events — handle 'final' (assembled message) and 'error' states.
// We skip 'delta' since agent events already handle real-time streaming.
gateway.on('chat', (payload: unknown) => {
  try {
    const evt = payload as Record<string, unknown>;
    const state = evt.state as string | undefined;

    // Handle error state — the agent/model returned an error (e.g. 401, rate limit)
    if (state === 'error') {
      const runId = evt.runId as string | undefined;
      const errorMsg = (evt.errorMessage ?? evt.error ?? 'Unknown error') as string;
      console.error(`[main] Chat error event (runId=${runId?.slice(0, 12)}):`, errorMsg);
      crashLog(`chat-error: ${errorMsg}`);

      // If we're streaming for this run, show the error in the bubble and
      // resolve the streaming promise so sendMessage() can finalize.
      if (isLoading || _streamingEl) {
        if (!_streamingRunId || (runId && runId === _streamingRunId)) {
          const errorContent = 'Error: ' + errorMsg;
          _streamingContent = errorContent;
          if (_streamingEl) {
            const errorSpan = document.createElement('span');
            errorSpan.className = 'chat-error-inline';
            errorSpan.textContent = errorContent;
            _streamingEl.innerHTML = '';
            _streamingEl.appendChild(errorSpan);
            scrollToBottom();
          }
          // Resolve the streaming promise with the error text so sendMessage()
          // flow calls finalizeStreaming() once with the error content.
          if (_streamingResolve) {
            _streamingResolve(errorContent);
            _streamingResolve = null;
          }
        }
      }
      return;
    }

    // Skip delta events entirely — agent handler already processes deltas
    if (state !== 'final') return;

    const runId = evt.runId as string | undefined;
    const msg = evt.message as Record<string, unknown> | undefined;
    const chatEvtSession = evt.sessionKey as string | undefined;

    // Route paw-research-* final messages to research view
    if (chatEvtSession && chatEvtSession.startsWith('paw-research-')) {
      if (ResearchModule.isStreaming() && msg) {
        const text = extractContent(msg.content);
        if (text) {
          ResearchModule.setContent(text);
          const liveContent = $('research-live-content');
          if (liveContent) liveContent.textContent = text;
          ResearchModule.resolveStream(text);
        }
      }
      return;
    }

    // Ignore other background paw-* sessions
    if (chatEvtSession && chatEvtSession.startsWith('paw-')) return;

    if (!isLoading && !_streamingEl) return;
    if (_streamingRunId && runId && runId !== _streamingRunId) return;

    if (msg) {
      // Final assembled message — use as canonical response
      const text = extractContent(msg.content);
      if (text) {
        console.log(`[main] Chat final (${text.length} chars)`);
        // If streaming hasn't captured the full text, replace with final
        _streamingContent = text;
        if (_streamingEl) {
          _streamingEl.innerHTML = formatMarkdown(text);
          scrollToBottom();
        }
        // Resolve the streaming promise since we have the final text
        if (_streamingResolve) {
          _streamingResolve(text);
          _streamingResolve = null;
        }
      }
      // Track token usage from chat final event
      const chatUsage = (msg.usage ?? evt.usage) as Record<string, unknown> | undefined;
      recordTokenUsage(chatUsage);
    } else {
      // chat.final with no message body — the agent produced no output.
      // Resolve the grace period immediately instead of waiting.
      console.warn(`[main] Chat final with no message body (runId=${runId?.slice(0,12)}) — agent produced no output`);
      if (_streamingResolve) {
        _streamingResolve(_streamingContent || '');
        _streamingResolve = null;
      }
    }
  } catch (e) {
    console.warn('[main] Chat event handler error:', e);
  }
});

// ── Channels — Connection Hub ──────────────────────────────────────────────
const CHANNEL_ICONS: Record<string, string> = {
  telegram: 'TG', discord: 'DC', whatsapp: 'WA', signal: 'SG', slack: 'SK',
};
const CHANNEL_CLASSES: Record<string, string> = {
  telegram: 'telegram', discord: 'discord', whatsapp: 'whatsapp', signal: 'signal', slack: 'slack',
};

// ── Channel Setup Definitions ──────────────────────────────────────────────
interface ChannelField {
  key: string;
  label: string;
  type: 'text' | 'password' | 'select' | 'toggle';
  placeholder?: string;
  hint?: string;
  required?: boolean;
  options?: { value: string; label: string }[];
  defaultValue?: string | boolean;
  sensitive?: boolean;
}

interface ChannelSetupDef {
  id: string;
  name: string;
  icon: string;
  description: string;
  fields: ChannelField[];
  /** Build the config patch from form values */
  buildConfig: (values: Record<string, string | boolean>) => Record<string, unknown>;
}

const CHANNEL_SETUPS: ChannelSetupDef[] = [
  {
    id: 'telegram',
    name: 'Telegram',
    icon: 'TG',
    description: 'Connect your agent to Telegram via a Bot token from @BotFather.',
    fields: [
      { key: 'botToken', label: 'Bot Token', type: 'password', placeholder: '123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11', hint: 'Get this from @BotFather on Telegram', required: true, sensitive: true },
      { key: 'dmPolicy', label: 'DM Policy', type: 'select', options: [
        { value: 'pairing', label: 'Pairing (approve via code)' },
        { value: 'allowlist', label: 'Allowlist only' },
        { value: 'open', label: 'Open (anyone can DM)' },
        { value: 'disabled', label: 'Disabled' },
      ], defaultValue: 'pairing' },
      { key: 'groupPolicy', label: 'Group Policy', type: 'select', options: [
        { value: 'allowlist', label: 'Allowlist only' },
        { value: 'open', label: 'Open (any group)' },
        { value: 'disabled', label: 'Disabled' },
      ], defaultValue: 'allowlist' },
      { key: 'allowFrom', label: 'Allowed Users', type: 'text', placeholder: 'User IDs or usernames, comma-separated', hint: 'Leave blank for pairing mode' },
    ],
    buildConfig: (v) => ({
      channels: { telegram: {
        enabled: true,
        botToken: v.botToken as string,
        dmPolicy: v.dmPolicy as string || 'pairing',
        groupPolicy: v.groupPolicy as string || 'allowlist',
        ...(v.allowFrom ? { allowFrom: (v.allowFrom as string).split(',').map(s => s.trim()).filter(Boolean) } : {}),
      }},
    }),
  },
  {
    id: 'discord',
    name: 'Discord',
    icon: 'DC',
    description: 'Connect to Discord with a bot token from the Developer Portal.',
    fields: [
      { key: 'token', label: 'Bot Token', type: 'password', placeholder: 'Your Discord bot token', hint: 'From discord.com/developers → Bot → Token', required: true, sensitive: true },
      { key: 'dmPolicy', label: 'DM Policy', type: 'select', options: [
        { value: 'pairing', label: 'Pairing (approve via code)' },
        { value: 'allowlist', label: 'Allowlist only' },
        { value: 'open', label: 'Open (anyone can DM)' },
        { value: 'disabled', label: 'Disabled' },
      ], defaultValue: 'pairing' },
      { key: 'groupPolicy', label: 'Server Policy', type: 'select', options: [
        { value: 'open', label: 'Open (respond in allowed channels)' },
        { value: 'allowlist', label: 'Allowlist only' },
        { value: 'disabled', label: 'Disabled' },
      ], defaultValue: 'open' },
      { key: 'allowFrom', label: 'Allowed Users', type: 'text', placeholder: 'Discord usernames, comma-separated', hint: 'Leave blank for pairing mode' },
    ],
    buildConfig: (v) => ({
      channels: { discord: {
        enabled: true,
        token: v.token as string,
        dm: {
          enabled: true,
          policy: v.dmPolicy as string || 'pairing',
          ...(v.allowFrom ? { allowFrom: (v.allowFrom as string).split(',').map(s => s.trim()).filter(Boolean) } : {}),
        },
        groupPolicy: v.groupPolicy as string || 'open',
      }},
    }),
  },
  {
    id: 'whatsapp',
    name: 'WhatsApp',
    icon: 'WA',
    description: 'Connect to WhatsApp via QR code pairing — no token needed.',
    fields: [
      { key: 'dmPolicy', label: 'DM Policy', type: 'select', options: [
        { value: 'pairing', label: 'Pairing (approve via code)' },
        { value: 'allowlist', label: 'Allowlist only' },
        { value: 'open', label: 'Open (anyone can DM)' },
        { value: 'disabled', label: 'Disabled' },
      ], defaultValue: 'pairing' },
      { key: 'groupPolicy', label: 'Group Policy', type: 'select', options: [
        { value: 'allowlist', label: 'Allowlist only' },
        { value: 'open', label: 'Open (any group)' },
        { value: 'disabled', label: 'Disabled' },
      ], defaultValue: 'allowlist' },
      { key: 'allowFrom', label: 'Allowed Phone Numbers', type: 'text', placeholder: '+15551234567, +15559876543', hint: 'E.164 format. Leave blank for pairing mode.' },
    ],
    buildConfig: (v) => ({
      channels: { whatsapp: {
        enabled: true,
        dmPolicy: v.dmPolicy as string || 'pairing',
        groupPolicy: v.groupPolicy as string || 'allowlist',
        ...(v.allowFrom ? { allowFrom: (v.allowFrom as string).split(',').map(s => s.trim()).filter(Boolean) } : {}),
      }},
    }),
  },
  {
    id: 'slack',
    name: 'Slack',
    icon: 'SK',
    description: 'Connect to Slack using Socket Mode (bot + app tokens).',
    fields: [
      { key: 'botToken', label: 'Bot Token', type: 'password', placeholder: 'xoxb-...', hint: 'OAuth Bot Token from Slack app settings', required: true, sensitive: true },
      { key: 'appToken', label: 'App Token', type: 'password', placeholder: 'xapp-...', hint: 'App-Level Token (connections:write scope)', required: true, sensitive: true },
      { key: 'dmPolicy', label: 'DM Policy', type: 'select', options: [
        { value: 'pairing', label: 'Pairing (approve via code)' },
        { value: 'allowlist', label: 'Allowlist only' },
        { value: 'open', label: 'Open (anyone can DM)' },
        { value: 'disabled', label: 'Disabled' },
      ], defaultValue: 'pairing' },
      { key: 'allowFrom', label: 'Allowed Users', type: 'text', placeholder: 'Slack usernames, comma-separated', hint: 'Leave blank for pairing mode' },
    ],
    buildConfig: (v) => ({
      channels: { slack: {
        enabled: true,
        botToken: v.botToken as string,
        appToken: v.appToken as string,
        mode: 'socket',
        dm: {
          enabled: true,
          policy: v.dmPolicy as string || 'pairing',
          ...(v.allowFrom ? { allowFrom: (v.allowFrom as string).split(',').map(s => s.trim()).filter(Boolean) } : {}),
        },
      }},
    }),
  },
  {
    id: 'signal',
    name: 'Signal',
    icon: 'SG',
    description: 'Connect to Signal via signal-cli. Requires signal-cli installed.',
    fields: [
      { key: 'account', label: 'Phone Number', type: 'text', placeholder: '+15551234567', hint: 'E.164 phone number registered with signal-cli', required: true },
      { key: 'cliPath', label: 'signal-cli Path', type: 'text', placeholder: 'signal-cli', hint: 'Path to signal-cli binary (default: signal-cli)', defaultValue: 'signal-cli' },
      { key: 'dmPolicy', label: 'DM Policy', type: 'select', options: [
        { value: 'pairing', label: 'Pairing (approve via code)' },
        { value: 'allowlist', label: 'Allowlist only' },
        { value: 'open', label: 'Open (anyone can DM)' },
        { value: 'disabled', label: 'Disabled' },
      ], defaultValue: 'pairing' },
      { key: 'allowFrom', label: 'Allowed Phone Numbers', type: 'text', placeholder: '+15559876543', hint: 'E.164 format. Leave blank for pairing mode.' },
    ],
    buildConfig: (v) => ({
      channels: { signal: {
        enabled: true,
        account: v.account as string,
        ...(v.cliPath && v.cliPath !== 'signal-cli' ? { cliPath: v.cliPath as string } : {}),
        dmPolicy: v.dmPolicy as string || 'pairing',
        ...(v.allowFrom ? { allowFrom: (v.allowFrom as string).split(',').map(s => s.trim()).filter(Boolean) } : {}),
      }},
    }),
  },
];

let _channelSetupType: string | null = null;

function openChannelSetup(channelType: string) {
  const def = CHANNEL_SETUPS.find(c => c.id === channelType);
  if (!def) return;
  _channelSetupType = channelType;

  const title = $('channel-setup-title');
  const body = $('channel-setup-body');
  const modal = $('channel-setup-modal');
  if (!title || !body || !modal) return;

  title.textContent = `Set Up ${def.name}`;

  let html = `<p class="channel-setup-desc">${escHtml(def.description)}</p>`;
  for (const field of def.fields) {
    html += `<div class="form-group">`;
    html += `<label class="form-label" for="ch-field-${field.key}">${escHtml(field.label)}${field.required ? ' <span class="required">*</span>' : ''}</label>`;

    if (field.type === 'select' && field.options) {
      html += `<select class="form-input" id="ch-field-${field.key}" data-ch-field="${field.key}">`;
      for (const opt of field.options) {
        const sel = opt.value === (field.defaultValue ?? '') ? ' selected' : '';
        html += `<option value="${escAttr(opt.value)}"${sel}>${escHtml(opt.label)}</option>`;
      }
      html += `</select>`;
    } else if (field.type === 'toggle') {
      const checked = field.defaultValue ? ' checked' : '';
      html += `<label class="toggle-label"><input type="checkbox" id="ch-field-${field.key}" data-ch-field="${field.key}"${checked}> Enabled</label>`;
    } else {
      const inputType = field.type === 'password' ? 'password' : 'text';
      const val = typeof field.defaultValue === 'string' ? ` value="${escAttr(field.defaultValue)}"` : '';
      html += `<input class="form-input" id="ch-field-${field.key}" data-ch-field="${field.key}" type="${inputType}" placeholder="${escAttr(field.placeholder ?? '')}"${val}>`;
    }

    if (field.hint) {
      html += `<div class="form-hint">${escHtml(field.hint)}</div>`;
    }
    html += `</div>`;
  }

  body.innerHTML = html;
  modal.style.display = '';
}

function closeChannelSetup() {
  const modal = $('channel-setup-modal');
  if (modal) modal.style.display = 'none';
  _channelSetupType = null;
}

async function saveChannelSetup() {
  console.log('[mail-debug] saveChannelSetup called, _channelSetupType=', _channelSetupType, 'mailType=', MailModule.getChannelSetupType(), 'wsConnected=', wsConnected);
  // Mail IMAP setup is handled by the mail module — check BEFORE the null guard
  // because openMailAccountSetup() sets mail.ts's _channelSetupType, not main.ts's.
  if (_channelSetupType === '__mail_imap__' || MailModule.getChannelSetupType() === '__mail_imap__') {
    console.log('[mail-debug] Routing to MailModule.saveMailImapSetup()');
    await MailModule.saveMailImapSetup();
    MailModule.clearChannelSetupType();
    _channelSetupType = null;
    return;
  }

  if (!_channelSetupType || !wsConnected) return;

  const def = CHANNEL_SETUPS.find(c => c.id === _channelSetupType);
  if (!def) return;

  // Gather field values
  const values: Record<string, string | boolean> = {};
  for (const field of def.fields) {
    const el = $(`ch-field-${field.key}`);
    if (!el) continue;
    if (field.type === 'toggle') {
      values[field.key] = (el as HTMLInputElement).checked;
    } else {
      values[field.key] = ((el as HTMLInputElement | HTMLSelectElement).value ?? '').trim();
    }
  }

  // Validate required fields
  for (const field of def.fields) {
    if (field.required && !values[field.key]) {
      showToast(`${field.label} is required`, 'error');
      $(`ch-field-${field.key}`)?.focus();
      return;
    }
  }

  const saveBtn = $('channel-setup-save') as HTMLButtonElement | null;
  if (saveBtn) { saveBtn.disabled = true; saveBtn.textContent = 'Saving...'; }

  try {
    // Build the config patch from the setup definition
    const patch = def.buildConfig(values);

    // First, get current config to deep-merge channels
    const current = await gateway.configGet();
    const currentChannels = (current.config as Record<string, unknown>)?.channels as Record<string, unknown> ?? {};
    const patchChannels = (patch as Record<string, unknown>).channels as Record<string, unknown>;

    // Merge: keep existing channel configs, add/replace this one
    const mergedChannels = { ...currentChannels, ...patchChannels };
    const fullConfig = JSON.parse(JSON.stringify(current.config));
    fullConfig.channels = mergedChannels;
    await gateway.configWrite(fullConfig);

    showToast(`${def.name} configured!`, 'success');
    closeChannelSetup();

    // Reload channels after a brief delay (gateway needs to initialize the channel)
    setTimeout(() => loadChannels(), 1500);
  } catch (e) {
    showToast(`Failed to save: ${e instanceof Error ? e.message : e}`, 'error');
  } finally {
    if (saveBtn) { saveBtn.disabled = false; saveBtn.textContent = 'Save & Connect'; }
  }
}

// Wire up channel setup UI
$('channel-setup-close')?.addEventListener('click', closeChannelSetup);
$('channel-setup-cancel')?.addEventListener('click', closeChannelSetup);
const _saveBtn = $('channel-setup-save');
console.log('[mail-debug] Binding save button, element found:', !!_saveBtn);
_saveBtn?.addEventListener('click', saveChannelSetup);
$('channel-setup-modal')?.addEventListener('click', (e) => {
  if ((e.target as HTMLElement).id === 'channel-setup-modal') closeChannelSetup();
});

// "Add Channel" button in header opens a picker
$('add-channel-btn')?.addEventListener('click', () => {
  // If there are no channels shown, the empty-state picker is already visible.
  // Otherwise, show a quick-pick via the setup modal with channel selection.
  const body = $('channel-setup-body');
  const title = $('channel-setup-title');
  const modal = $('channel-setup-modal');
  const footer = $('channel-setup-save') as HTMLButtonElement | null;
  if (!body || !title || !modal) return;

  _channelSetupType = null;
  title.textContent = 'Add Channel';
  if (footer) footer.style.display = 'none';

  let html = '<div class="channel-picker-grid">';
  for (const def of CHANNEL_SETUPS) {
    html += `<button class="channel-pick-btn" data-ch-pick="${def.id}">
      <span class="channel-pick-icon ${CHANNEL_CLASSES[def.id] ?? 'default'}">${def.icon}</span>
      <span>${escHtml(def.name)}</span>
    </button>`;
  }
  html += '</div>';
  body.innerHTML = html;

  // Wire picks
  body.querySelectorAll('[data-ch-pick]').forEach(btn => {
    btn.addEventListener('click', () => {
      if (footer) footer.style.display = '';
      openChannelSetup((btn as HTMLElement).dataset.chPick!);
    });
  });

  modal.style.display = '';
});

// Wire empty-state channel picker buttons
document.querySelectorAll('#channels-picker-empty .channel-pick-btn').forEach(btn => {
  btn.addEventListener('click', () => {
    const chType = (btn as HTMLElement).dataset.chType;
    if (chType) openChannelSetup(chType);
  });
});

async function loadChannels() {
  const list = $('channels-list');
  const empty = $('channels-empty');
  const loading = $('channels-loading');
  if (!wsConnected || !list) return;

  if (loading) loading.style.display = '';
  if (empty) empty.style.display = 'none';
  list.innerHTML = '';

  try {
    const result = await gateway.getChannelsStatus(true);
    if (loading) loading.style.display = 'none';

    const channels = result.channels ?? {};
    const keys = Object.keys(channels);
    if (!keys.length) {
      if (empty) empty.style.display = 'flex';
      return;
    }

    for (const id of keys) {
      const ch = channels[id];
      const lId = id.toLowerCase();
      const icon = CHANNEL_ICONS[lId] ?? '--';
      const cssClass = CHANNEL_CLASSES[lId] ?? 'default';
      const linked = ch.linked;
      const configured = ch.configured;
      const accounts = ch.accounts ? Object.keys(ch.accounts) : [];

      const card = document.createElement('div');
      card.className = 'channel-card';
      card.innerHTML = `
        <div class="channel-card-header">
          <div class="channel-card-icon ${cssClass}">${icon}</div>
          <div>
            <div class="channel-card-title">${escHtml(String(id))}</div>
            <div class="channel-card-status">
              <span class="status-dot ${linked ? 'connected' : (configured ? 'error' : '')}"></span>
              <span>${linked ? 'Connected' : (configured ? 'Disconnected' : 'Not configured')}</span>
            </div>
          </div>
        </div>
        ${accounts.length ? `<div class="channel-card-accounts">${accounts.map(a => escHtml(a)).join(', ')}</div>` : ''}
        <div class="channel-card-actions">
          ${!linked && configured ? `<button class="btn btn-primary btn-sm ch-login" data-ch="${escAttr(id)}">Login</button>` : ''}
          ${linked ? `<button class="btn btn-ghost btn-sm ch-logout" data-ch="${escAttr(id)}">Logout</button>` : ''}
          ${CHANNEL_SETUPS.find(c => c.id === lId) ? `<button class="btn btn-ghost btn-sm ch-edit" data-ch="${escAttr(lId)}">Edit</button>` : ''}
          ${configured ? `<button class="btn btn-ghost btn-sm ch-remove" data-ch="${escAttr(lId)}" title="Remove channel">Remove</button>` : ''}
          <button class="btn btn-ghost btn-sm ch-refresh-single" data-ch="${escAttr(id)}">Refresh</button>
        </div>
      `;
      list.appendChild(card);
    }

    // Wire up login/logout buttons
    list.querySelectorAll('.ch-login').forEach(btn => {
      btn.addEventListener('click', async () => {
        const chId = (btn as HTMLElement).dataset.ch!;
        try {
          (btn as HTMLButtonElement).disabled = true;
          (btn as HTMLButtonElement).textContent = 'Logging in...';
          await gateway.startWebLogin(chId);
          await gateway.waitWebLogin(chId, 120_000);
          loadChannels();
        } catch (e) {
          alert(`Login failed: ${e instanceof Error ? e.message : e}`);
          loadChannels();
        }
      });
    });
    list.querySelectorAll('.ch-logout').forEach(btn => {
      btn.addEventListener('click', async () => {
        const chId = (btn as HTMLElement).dataset.ch!;
        if (!confirm(`Disconnect ${chId}?`)) return;
        try { await gateway.logoutChannel(chId); loadChannels(); }
        catch (e) { alert(`Logout failed: ${e}`); }
      });
    });
    list.querySelectorAll('.ch-refresh-single').forEach(btn => {
      btn.addEventListener('click', () => loadChannels());
    });
    list.querySelectorAll('.ch-edit').forEach(btn => {
      btn.addEventListener('click', () => {
        const chId = (btn as HTMLElement).dataset.ch!;
        openChannelSetup(chId);
      });
    });
    list.querySelectorAll('.ch-remove').forEach(btn => {
      btn.addEventListener('click', async () => {
        const chId = (btn as HTMLElement).dataset.ch!;
        if (!confirm(`Remove ${chId} channel configuration? This will disconnect the channel.`)) return;
        try {
          const current = await gateway.configGet();
          const cfg = current.config as Record<string, unknown>;
          const channels = { ...(cfg?.channels as Record<string, unknown> ?? {}) };
          delete channels[chId];
          const fullConfig = JSON.parse(JSON.stringify(cfg));
          fullConfig.channels = channels;
          await gateway.configWrite(fullConfig);
          showToast(`${chId} removed`, 'success');
          setTimeout(() => loadChannels(), 1000);
        } catch (e) {
          showToast(`Remove failed: ${e instanceof Error ? e.message : e}`, 'error');
        }
      });
    });

    // Populate channel send dropdown
    const sendSection = $('channel-send-section');
    const sendTarget = $('channel-send-target') as HTMLSelectElement | null;
    const connectedKeys = keys.filter(k => channels[k].linked);
    if (connectedKeys.length && sendSection && sendTarget) {
      sendSection.style.display = '';
      sendTarget.innerHTML = connectedKeys.map(k => `<option value="${escAttr(k)}">${escHtml(k)}</option>`).join('');
    } else if (sendSection) {
      sendSection.style.display = 'none';
    }
  } catch (e) {
    console.warn('Channels load failed:', e);
    if (loading) loading.style.display = 'none';
    if (empty) empty.style.display = 'flex';
  }
}
$('refresh-channels-btn')?.addEventListener('click', () => loadChannels());

// Direct channel send
$('channel-send-btn')?.addEventListener('click', async () => {
  const target = ($('channel-send-target') as HTMLSelectElement)?.value;
  const msgInput = $('channel-send-message') as HTMLInputElement;
  const message = msgInput?.value.trim();
  if (!target || !message || !wsConnected) return;
  try {
    await gateway.send({ channelId: target, message });
    showToast(`Sent to ${target}`, 'success');
    if (msgInput) msgInput.value = '';
  } catch (e) {
    showToast(`Send failed: ${e instanceof Error ? e.message : e}`, 'error');
  }
});

// ── Automations / Cron — Card Board ────────────────────────────────────────
async function loadCron() {
  const activeCards = $('cron-active-cards');
  const pausedCards = $('cron-paused-cards');
  const historyCards = $('cron-history-cards');
  const empty = $('cron-empty');
  const loading = $('cron-loading');
  const activeCount = $('cron-active-count');
  const pausedCount = $('cron-paused-count');
  const board = document.querySelector('.auto-board') as HTMLElement | null;
  if (!wsConnected) return;

  if (loading) loading.style.display = '';
  if (empty) empty.style.display = 'none';
  if (board) board.style.display = 'grid';
  if (activeCards) activeCards.innerHTML = '';
  if (pausedCards) pausedCards.innerHTML = '';
  if (historyCards) historyCards.innerHTML = '';

  try {
    const result = await gateway.cronList();
    if (loading) loading.style.display = 'none';

    const jobs = result.jobs ?? [];
    if (!jobs.length) {
      if (empty) empty.style.display = 'flex';
      if (board) board.style.display = 'none';
      return;
    }

    let active = 0, paused = 0;
    for (const job of jobs) {
      const scheduleStr = typeof job.schedule === 'string' ? job.schedule : (job.schedule?.type ?? '');
      const card = document.createElement('div');
      card.className = 'auto-card';
      card.innerHTML = `
        <div class="auto-card-title">${escHtml(job.label ?? job.id)}</div>
        <div class="auto-card-schedule">${escHtml(scheduleStr)}</div>
        ${job.prompt ? `<div class="auto-card-prompt">${escHtml(String(job.prompt))}</div>` : ''}
        <div class="auto-card-actions">
          <button class="btn btn-ghost btn-sm cron-run" data-id="${escAttr(job.id)}">▶ Run</button>
          <button class="btn btn-ghost btn-sm cron-toggle" data-id="${escAttr(job.id)}" data-enabled="${job.enabled}">${job.enabled ? '⏸ Pause' : '▶ Enable'}</button>
          <button class="btn btn-ghost btn-sm cron-delete" data-id="${escAttr(job.id)}">Delete</button>
        </div>
      `;
      if (job.enabled) {
        active++;
        activeCards?.appendChild(card);
      } else {
        paused++;
        pausedCards?.appendChild(card);
      }
    }
    if (activeCount) activeCount.textContent = String(active);
    if (pausedCount) pausedCount.textContent = String(paused);

    // Wire card actions
    const wireActions = (container: HTMLElement | null) => {
      if (!container) return;
      container.querySelectorAll('.cron-run').forEach(btn => {
        btn.addEventListener('click', async () => {
          const id = (btn as HTMLElement).dataset.id!;
          try { await gateway.cronRun(id); alert('Job triggered!'); }
          catch (e) { alert(`Failed: ${e}`); }
        });
      });
      container.querySelectorAll('.cron-toggle').forEach(btn => {
        btn.addEventListener('click', async () => {
          const id = (btn as HTMLElement).dataset.id!;
          const enabled = (btn as HTMLElement).dataset.enabled === 'true';
          try { await gateway.cronUpdate(id, { enabled: !enabled }); loadCron(); }
          catch (e) { alert(`Failed: ${e}`); }
        });
      });
      container.querySelectorAll('.cron-delete').forEach(btn => {
        btn.addEventListener('click', async () => {
          const id = (btn as HTMLElement).dataset.id!;
          if (!confirm('Delete this automation?')) return;
          try { await gateway.cronRemove(id); loadCron(); }
          catch (e) { alert(`Failed: ${e}`); }
        });
      });
    };
    wireActions(activeCards);
    wireActions(pausedCards);

    // Load run history
    try {
      const runs = await gateway.cronRuns(undefined, 20);
      if (runs.runs?.length && historyCards) {
        for (const run of runs.runs.slice(0, 10)) {
          const histCard = document.createElement('div');
          histCard.className = 'auto-card';
          const statusClass = run.status === 'success' ? 'success' : (run.status === 'running' ? 'running' : 'failed');
          histCard.innerHTML = `
            <div class="auto-card-time">${run.startedAt ? new Date(run.startedAt).toLocaleString() : ''}</div>
            <div class="auto-card-title">${escHtml(run.jobLabel ?? run.jobId ?? 'Job')}</div>
            <span class="auto-card-status ${statusClass}">${run.status ?? 'unknown'}</span>
          `;
          historyCards.appendChild(histCard);
        }
      }
    } catch { /* run history not available */ }
  } catch (e) {
    console.warn('Cron load failed:', e);
    if (loading) loading.style.display = 'none';
    if (empty) empty.style.display = 'flex';
    if (board) board.style.display = 'none';
  }
}

// Cron modal logic
function showCronModal() {
  const modal = $('cron-modal');
  if (modal) modal.style.display = 'flex';
  // Reset form
  const label = $('cron-form-label') as HTMLInputElement;
  const schedule = $('cron-form-schedule') as HTMLInputElement;
  const prompt_ = $('cron-form-prompt') as HTMLTextAreaElement;
  const preset = $('cron-form-schedule-preset') as HTMLSelectElement;
  if (label) label.value = '';
  if (schedule) schedule.value = '';
  if (prompt_) prompt_.value = '';
  if (preset) preset.value = '';
}
function hideCronModal() {
  const modal = $('cron-modal');
  if (modal) modal.style.display = 'none';
}

$('add-cron-btn')?.addEventListener('click', showCronModal);
$('cron-empty-add')?.addEventListener('click', showCronModal);
$('cron-modal-close')?.addEventListener('click', hideCronModal);
$('cron-modal-cancel')?.addEventListener('click', hideCronModal);

$('cron-form-schedule-preset')?.addEventListener('change', () => {
  const preset = ($('cron-form-schedule-preset') as HTMLSelectElement).value;
  const scheduleInput = $('cron-form-schedule') as HTMLInputElement;
  if (preset && scheduleInput) scheduleInput.value = preset;
});

$('cron-modal-save')?.addEventListener('click', async () => {
  const label = ($('cron-form-label') as HTMLInputElement).value.trim();
  const schedule = ($('cron-form-schedule') as HTMLInputElement).value.trim();
  const prompt_ = ($('cron-form-prompt') as HTMLTextAreaElement).value.trim();
  if (!label || !schedule || !prompt_) { alert('All fields required'); return; }
  try {
    await gateway.cronAdd({ label, schedule, prompt: prompt_, enabled: true });
    hideCronModal();
    loadCron();
  } catch (e) { alert(`Failed: ${e}`); }
});

// ── Memory / Agent Files — Split View ──────────────────────────────────────
async function loadMemory() {
  const list = $('memory-list');
  const empty = $('memory-empty');
  const loading = $('memory-loading');
  const editorPanel = $('memory-editor');
  if (!wsConnected || !list) return;

  if (loading) loading.style.display = '';
  if (empty) empty.style.display = 'none';
  if (editorPanel) editorPanel.style.display = 'none';
  list.innerHTML = '';

  try {
    const result = await gateway.agentFilesList();
    if (loading) loading.style.display = 'none';

    const files = result.files ?? [];
    if (!files.length) {
      if (empty) empty.style.display = 'flex';
      return;
    }

    for (const file of files) {
      const card = document.createElement('div');
      card.className = 'list-item list-item-clickable';
      const displayName = file.path ?? file.name ?? 'unknown';
      const displaySize = file.sizeBytes ?? file.size;
      card.innerHTML = `
        <div class="list-item-header">
          <span class="list-item-title">${escHtml(displayName)}</span>
          <span class="list-item-meta">${displaySize ? formatBytes(displaySize) : ''}</span>
        </div>
      `;
      card.addEventListener('click', () => openMemoryFile(displayName));
      list.appendChild(card);
    }
  } catch (e) {
    console.warn('Memory load failed:', e);
    if (loading) loading.style.display = 'none';
    if (empty) empty.style.display = 'flex';
  }
}

async function openMemoryFile(filePath: string) {
  const editor = $('memory-editor');
  const content = $('memory-editor-content') as HTMLTextAreaElement | null;
  const pathEl = $('memory-editor-path');
  const empty = $('memory-empty');
  if (!editor || !content) return;

  editor.style.display = '';
  if (empty) empty.style.display = 'none';
  if (pathEl) pathEl.textContent = filePath;
  content.value = 'Loading...';
  content.disabled = true;

  try {
    const result = await gateway.agentFilesGet(filePath);
    content.value = result.content ?? '';
    content.disabled = false;
    content.dataset.filePath = filePath;
  } catch (e) {
    content.value = `Error loading file: ${e}`;
  }
}

$('memory-editor-save')?.addEventListener('click', async () => {
  const content = $('memory-editor-content') as HTMLTextAreaElement | null;
  if (!content?.dataset.filePath) return;
  try {
    await gateway.agentFilesSet(content.dataset.filePath, content.value);
    alert('File saved!');
  } catch (e) {
    alert(`Save failed: ${e}`);
  }
});

$('memory-editor-close')?.addEventListener('click', () => {
  const editor = $('memory-editor');
  if (editor) editor.style.display = 'none';
});

$('refresh-memory-btn')?.addEventListener('click', () => loadMemory());

// ── Dashboard Cron Widget ──────────────────────────────────────────────────
async function loadDashboardCron() {
  const section = $('dashboard-cron-section');
  const list = $('dashboard-cron-list');
  const empty = $('dashboard-cron-empty');
  if (!wsConnected || !list) return;

  list.innerHTML = '';
  if (empty) empty.style.display = 'none';

  try {
    const result = await gateway.cronList();
    const jobs = result.jobs ?? [];

    if (!jobs.length) {
      if (section) section.style.display = '';
      if (empty) empty.style.display = 'flex';
      return;
    }

    if (section) section.style.display = '';

    for (const job of jobs.slice(0, 8)) {
      const card = document.createElement('div');
      card.className = 'dash-cron-card';
      const scheduleStr = typeof job.schedule === 'string' ? job.schedule : (job.schedule?.type ?? '');
      card.innerHTML = `
        <span class="dash-cron-dot ${job.enabled ? 'active' : 'paused'}"></span>
        <div class="dash-cron-info">
          <div class="dash-cron-name">${escHtml(job.label ?? job.id)}</div>
          <div class="dash-cron-schedule">${escHtml(scheduleStr)}</div>
        </div>
      `;
      card.addEventListener('click', () => switchView('automations'));
      list.appendChild(card);
    }
  } catch (e) {
    console.warn('Dashboard cron load failed:', e);
    if (section) section.style.display = 'none';
  }
}

// ── Space Cron Mini-Widgets ────────────────────────────────────────────────
async function loadSpaceCron(space: string) {
  const widget = $(`${space}-cron-widget`);
  const count = $(`${space}-cron-count`);
  const items = $(`${space}-cron-items`);
  if (!wsConnected || !widget) return;

  widget.style.display = 'none';
  if (items) items.innerHTML = '';

  try {
    const result = await gateway.cronList();
    const jobs = result.jobs ?? [];
    if (!jobs.length) return;

    // Filter jobs contextually by space keywords
    const keywords: Record<string, string[]> = {
      build: ['build', 'deploy', 'compile', 'test', 'ci', 'lint'],
      content: ['content', 'write', 'publish', 'draft', 'blog', 'post', 'article'],
      mail: ['mail', 'email', 'send', 'newsletter', 'digest', 'notify', 'inbox'],
      research: ['research', 'scrape', 'crawl', 'monitor', 'fetch', 'analyze', 'report'],
    };
    const spaceKeywords = keywords[space] ?? [];

    const matched = jobs.filter(job => {
      const label = (job.label ?? '').toLowerCase();
      const prompt = (typeof job.prompt === 'string' ? job.prompt : '').toLowerCase();
      return spaceKeywords.some(kw => label.includes(kw) || prompt.includes(kw));
    });

    // If no keyword matches, show all active jobs as a fallback (max 3)
    const display = matched.length ? matched : jobs.filter(j => j.enabled).slice(0, 3);
    if (!display.length) return;

    widget.style.display = '';
    if (count) count.textContent = String(display.length);

    for (const job of display.slice(0, 5)) {
      const item = document.createElement('div');
      item.className = 'space-cron-item';
      const scheduleStr = typeof job.schedule === 'string' ? job.schedule : (job.schedule?.type ?? '');
      item.innerHTML = `
        <span class="dash-cron-dot ${job.enabled ? 'active' : 'paused'}"></span>
        <span class="space-cron-name">${escHtml(job.label ?? job.id)}</span>
        <span class="space-cron-schedule">${escHtml(scheduleStr)}</span>
      `;
      item.addEventListener('click', () => switchView('automations'));
      items?.appendChild(item);
    }
  } catch (e) {
    console.warn(`Space cron (${space}) load failed:`, e);
  }
}
// ══════════════════════════════════════════════════════════════════════════
// ═══ LOCAL APPLICATION SPACES ═══════════════════════════════════════════
// ══════════════════════════════════════════════════════════════════════════

// ── Content / Create Studio ────────────────────────────────────────────────
let _activeDocId: string | null = null;

async function loadContentDocs() {
  const list = $('content-doc-list');
  const empty = $('content-empty');
  const toolbar = $('content-toolbar');
  const body = $('content-body') as HTMLTextAreaElement | null;
  const wordCount = $('content-word-count');
  if (!list) return;

  const docs = await listDocs();
  list.innerHTML = '';

  if (!docs.length && !_activeDocId) {
    if (empty) empty.style.display = 'flex';
    if (toolbar) toolbar.style.display = 'none';
    if (body) body.style.display = 'none';
    if (wordCount) wordCount.style.display = 'none';
    return;
  }

  for (const doc of docs) {
    const item = document.createElement('div');
    item.className = `studio-doc-item${doc.id === _activeDocId ? ' active' : ''}`;
    item.innerHTML = `
      <div class="studio-doc-title">${escHtml(doc.title || 'Untitled')}</div>
      <div class="studio-doc-meta">${doc.word_count} words · ${new Date(doc.updated_at).toLocaleDateString()}</div>
    `;
    item.addEventListener('click', () => openContentDoc(doc.id));
    list.appendChild(item);
  }
}

async function openContentDoc(docId: string) {
  const doc = await getDoc(docId);
  if (!doc) return;
  _activeDocId = docId;

  const empty = $('content-empty');
  const toolbar = $('content-toolbar');
  const body = $('content-body') as HTMLTextAreaElement;
  const titleInput = $('content-title') as HTMLInputElement;
  const typeSelect = $('content-type') as HTMLSelectElement;
  const wordCount = $('content-word-count');

  if (empty) empty.style.display = 'none';
  if (toolbar) toolbar.style.display = 'flex';
  if (body) { body.style.display = ''; body.value = doc.content; }
  if (titleInput) titleInput.value = doc.title;
  if (typeSelect) typeSelect.value = doc.content_type;
  if (wordCount) {
    wordCount.style.display = '';
    wordCount.textContent = `${doc.word_count} words`;
  }
  loadContentDocs();
}

async function createNewDoc() {
  const id = crypto.randomUUID();
  await saveDoc({ id, title: 'Untitled document', content: '', content_type: 'markdown' });
  await openContentDoc(id);
}

$('content-new-doc')?.addEventListener('click', createNewDoc);
$('content-create-first')?.addEventListener('click', createNewDoc);

$('content-save')?.addEventListener('click', async () => {
  if (!_activeDocId) return;
  const title = ($('content-title') as HTMLInputElement).value.trim() || 'Untitled';
  const content = ($('content-body') as HTMLTextAreaElement).value;
  const contentType = ($('content-type') as HTMLSelectElement).value;
  await saveDoc({ id: _activeDocId, title, content, content_type: contentType });
  const wordCount = $('content-word-count');
  if (wordCount) wordCount.textContent = `${content.split(/\s+/).filter(Boolean).length} words`;
  loadContentDocs();
});

$('content-body')?.addEventListener('input', () => {
  const body = $('content-body') as HTMLTextAreaElement;
  const wordCount = $('content-word-count');
  if (wordCount && body) {
    wordCount.textContent = `${body.value.split(/\s+/).filter(Boolean).length} words`;
  }
});

$('content-ai-improve')?.addEventListener('click', async () => {
  if (!_activeDocId || !wsConnected) { showToast('Connect to gateway first', 'error'); return; }
  const bodyEl = $('content-body') as HTMLTextAreaElement;
  const body = bodyEl?.value.trim();
  if (!body) return;

  const btn = $('content-ai-improve') as HTMLButtonElement | null;
  if (btn) btn.disabled = true;
  showToast('AI improving your text…', 'info');

  try {
    // Direct agent run (sessionless) — no chat history needed for one-shot improve
    const run = await gateway.agent({ prompt: `Improve this text. Return only the improved version, no explanations:\n\n${body}` });
    // Wait for the agent to finish and return the full result
    const result = await gateway.agentWait(run.runId, 120_000);
    if (result.text && bodyEl) {
      bodyEl.value = result.text;
      showToast('Text improved!', 'success');
    } else {
      showToast('Agent returned no text', 'error');
    }
  } catch (e) {
    showToast(`Failed: ${e instanceof Error ? e.message : e}`, 'error');
  } finally {
    if (btn) btn.disabled = false;
  }
});

$('content-delete-doc')?.addEventListener('click', async () => {
  if (!_activeDocId) return;
  if (!confirm('Delete this document?')) return;
  await deleteDoc(_activeDocId);
  _activeDocId = null;
  loadContentDocs();
});

// ── Research Notebook ──────────────────────────────────────────────────────

// ── Build — streaming state ────────────────────────────────────────────────
let _buildStreaming = false;
let _buildStreamContent = '';
let _buildStreamRunId: string | null = null;
let _buildStreamResolve: ((text: string) => void) | null = null;

// ── Build IDE ──────────────────────────────────────────────────────────────
let _buildProjectId: string | null = null;
let _buildOpenFiles: { id: string; path: string; content: string }[] = [];
let _buildActiveFile: string | null = null;
let _buildSaveTimer: ReturnType<typeof setTimeout> | null = null;

$('build-new-project')?.addEventListener('click', async () => {
  const name = await promptModal('Project name:', 'My project');
  if (!name) return;
  const id = crypto.randomUUID();
  await saveProject({ id, name, space: 'build' });
  _buildProjectId = id;
  _buildOpenFiles = [];
  _buildActiveFile = null;
  await loadBuildProjects();
  loadBuildProject();
});

async function loadBuildProjects() {
  const sel = $('build-project-select') as HTMLSelectElement | null;
  if (!sel) return;
  const projects = await listProjects('build');
  sel.innerHTML = '<option value="">No project</option>';
  for (const p of projects) {
    const opt = document.createElement('option');
    opt.value = p.id;
    opt.textContent = p.name;
    if (p.id === _buildProjectId) opt.selected = true;
    sel.appendChild(opt);
  }
  // Auto-select first project if none selected
  if (!_buildProjectId && projects.length) {
    _buildProjectId = projects[0].id;
    sel.value = projects[0].id;
    await loadBuildProject();
  }
}

$('build-project-select')?.addEventListener('change', async () => {
  const sel = $('build-project-select') as HTMLSelectElement;
  _buildProjectId = sel?.value || null;
  _buildOpenFiles = [];
  _buildActiveFile = null;
  if (_buildProjectId) {
    await loadBuildProject();
  } else {
    const empty = $('build-empty');
    if (empty) empty.style.display = 'flex';
    const editor = $('build-code-editor') as HTMLTextAreaElement;
    if (editor) editor.style.display = 'none';
    updateBuildTabs();
    updateBuildFileList();
  }
});

async function loadBuildProject() {
  if (!_buildProjectId) return;
  const empty = $('build-empty');

  // Load files from SQLite
  const dbFiles = await listProjectFiles(_buildProjectId);
  _buildOpenFiles = dbFiles.map(f => ({ id: f.id, path: f.path, content: f.content ?? '' }));
  _buildActiveFile = _buildOpenFiles[0]?.path ?? null;

  if (!_buildOpenFiles.length) {
    if (empty) empty.style.display = 'flex';
    const editor = $('build-code-editor') as HTMLTextAreaElement;
    if (editor) editor.style.display = 'none';
  } else {
    if (empty) empty.style.display = 'none';
    const editor = $('build-code-editor') as HTMLTextAreaElement;
    if (editor && _buildActiveFile) {
      editor.style.display = '';
      editor.value = _buildOpenFiles[0]?.content ?? '';
    }
  }

  updateBuildTabs();
  updateBuildFileList();
}

$('build-add-file')?.addEventListener('click', async () => {
  if (!_buildProjectId) { showToast('Create a project first', 'warning'); return; }
  const filename = await promptModal('File name', 'e.g. index.html');
  if (!filename) return;

  // Check for duplicate
  if (_buildOpenFiles.some(f => f.path === filename)) {
    showToast('File already exists', 'warning');
    return;
  }

  const fileId = crypto.randomUUID();
  // Persist immediately
  await saveProjectFile({ id: fileId, project_id: _buildProjectId, path: filename, content: '' });

  _buildOpenFiles.push({ id: fileId, path: filename, content: '' });
  _buildActiveFile = filename;

  const empty = $('build-empty');
  const editor = $('build-code-editor') as HTMLTextAreaElement;
  if (empty) empty.style.display = 'none';
  if (editor) { editor.style.display = ''; editor.value = ''; }

  updateBuildTabs();
  updateBuildFileList();
  showToast(`Created ${filename}`, 'success');
});

function updateBuildTabs() {
  const tabs = $('build-tabs');
  if (!tabs) return;
  tabs.innerHTML = '';
  for (const f of _buildOpenFiles) {
    const tab = document.createElement('div');
    tab.className = `ide-tab${f.path === _buildActiveFile ? ' active' : ''}`;
    tab.innerHTML = `<span>${escHtml(f.path)}</span><span class="ide-tab-close">✕</span>`;
    tab.querySelector('span:first-child')?.addEventListener('click', () => {
      _buildActiveFile = f.path;
      const editor = $('build-code-editor') as HTMLTextAreaElement;
      if (editor) editor.value = f.content;
      updateBuildTabs();
    });
    tab.querySelector('.ide-tab-close')?.addEventListener('click', async (e) => {
      e.stopPropagation();
      // Delete from DB
      await deleteProjectFile(f.id);
      _buildOpenFiles = _buildOpenFiles.filter(x => x.path !== f.path);
      if (_buildActiveFile === f.path) {
        _buildActiveFile = _buildOpenFiles[0]?.path ?? null;
        const editor = $('build-code-editor') as HTMLTextAreaElement;
        if (editor) editor.value = _buildActiveFile ? _buildOpenFiles[0].content : '';
        if (!_buildActiveFile) { editor.style.display = 'none'; const empty = $('build-empty'); if (empty) empty.style.display = 'flex'; }
      }
      updateBuildTabs();
      updateBuildFileList();
    });
    tabs.appendChild(tab);
  }
}

function updateBuildFileList() {
  const fileList = $('build-file-list');
  if (!fileList) return;
  fileList.innerHTML = '';
  for (const f of _buildOpenFiles) {
    const item = document.createElement('div');
    item.className = `ide-file-item${f.path === _buildActiveFile ? ' active' : ''}`;
    item.textContent = f.path;
    item.addEventListener('click', () => {
      _buildActiveFile = f.path;
      const editor = $('build-code-editor') as HTMLTextAreaElement;
      if (editor) { editor.style.display = ''; editor.value = f.content; }
      updateBuildTabs();
      updateBuildFileList();
    });
    fileList.appendChild(item);
  }
  if (!_buildOpenFiles.length) {
    fileList.innerHTML = '<div class="ide-file-empty">No files yet</div>';
  }
}

// Auto-save file content as user types (debounced 500ms)
$('build-code-editor')?.addEventListener('input', () => {
  if (!_buildActiveFile || !_buildProjectId) return;
  const editor = $('build-code-editor') as HTMLTextAreaElement;
  const file = _buildOpenFiles.find(f => f.path === _buildActiveFile);
  if (file && editor) {
    file.content = editor.value;
    // Debounce save to SQLite
    if (_buildSaveTimer) clearTimeout(_buildSaveTimer);
    _buildSaveTimer = setTimeout(() => {
      saveProjectFile({ id: file.id, project_id: _buildProjectId!, path: file.path, content: file.content })
        .catch(e => console.warn('[build] Auto-save failed:', e));
    }, 500);
  }
});

// Build chat — send to agent in build context
$('build-chat-send')?.addEventListener('click', async () => {
  const input = $('build-chat-input') as HTMLTextAreaElement;
  const msgList = $('build-chat-messages');
  if (!input?.value.trim() || !wsConnected) return;

  const userMsg = input.value.trim();
  input.value = '';

  // Show user message
  const userDiv = document.createElement('div');
  userDiv.className = 'message user';
  userDiv.innerHTML = `<div class="message-content">${escHtml(userMsg)}</div>`;
  msgList?.appendChild(userDiv);

  // Provide context about open files
  let context = userMsg;
  if (_buildOpenFiles.length) {
    const fileContext = _buildOpenFiles.map(f => `--- ${f.path} ---\n${f.content}`).join('\n\n');
    context = `[Build context]\nOpen files:\n${fileContext}\n\n[User instruction]: ${userMsg}`;
  }

  // Show streaming assistant bubble
  const agentDiv = document.createElement('div');
  agentDiv.className = 'message assistant';
  agentDiv.innerHTML = `<div class="message-content"><div class="loading-dots"><span></span><span></span><span></span></div></div>`;
  msgList?.appendChild(agentDiv);

  _buildStreaming = true;
  _buildStreamContent = '';
  _buildStreamRunId = null;

  const done = new Promise<string>((resolve) => {
    _buildStreamResolve = resolve;
    setTimeout(() => resolve(_buildStreamContent || '(Timed out)'), 120_000);
  });

  try {
    const sessionKey = _buildProjectId ? `paw-build-${_buildProjectId}` : 'paw-build';
    const result = await gateway.chatSend(sessionKey, context);
    if (result.runId) _buildStreamRunId = result.runId;

    const finalText = await done;
    const contentEl = agentDiv.querySelector('.message-content');
    if (contentEl) contentEl.innerHTML = formatMarkdown(finalText);
  } catch (e) {
    const contentEl = agentDiv.querySelector('.message-content');
    if (contentEl) contentEl.innerHTML = `Error: ${escHtml(e instanceof Error ? e.message : String(e))}`;
  } finally {
    _buildStreaming = false;
    _buildStreamRunId = null;
    _buildStreamResolve = null;
  }
});

// ── Helpers ────────────────────────────────────────────────────────────────

// Tauri 2 WKWebView (macOS) does not support window.prompt() — it returns null.
// This custom modal replaces all prompt() usage in the app.
function promptModal(title: string, placeholder?: string): Promise<string | null> {
  return new Promise(resolve => {
    const overlay = $('prompt-modal');
    const titleEl = $('prompt-modal-title');
    const input = $('prompt-modal-input') as HTMLInputElement | null;
    const okBtn = $('prompt-modal-ok');
    const cancelBtn = $('prompt-modal-cancel');
    const closeBtn = $('prompt-modal-close');
    if (!overlay || !input) { resolve(null); return; }

    if (titleEl) titleEl.textContent = title;
    input.placeholder = placeholder ?? '';
    input.value = '';
    overlay.style.display = 'flex';
    input.focus();

    function cleanup() {
      overlay!.style.display = 'none';
      okBtn?.removeEventListener('click', onOk);
      cancelBtn?.removeEventListener('click', onCancel);
      closeBtn?.removeEventListener('click', onCancel);
      input?.removeEventListener('keydown', onKey);
      overlay?.removeEventListener('click', onBackdrop);
    }
    function onOk() {
      const val = input!.value.trim();
      cleanup();
      resolve(val || null);
    }
    function onCancel() { cleanup(); resolve(null); }
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Enter') { e.preventDefault(); onOk(); }
      else if (e.key === 'Escape') { e.preventDefault(); onCancel(); }
    }
    function onBackdrop(e: MouseEvent) {
      if (e.target === overlay) onCancel();
    }

    okBtn?.addEventListener('click', onOk);
    cancelBtn?.addEventListener('click', onCancel);
    closeBtn?.addEventListener('click', onCancel);
    input.addEventListener('keydown', onKey);
    overlay.addEventListener('click', onBackdrop);
  });
}

function escHtml(s: string): string {
  const d = document.createElement('div');
  d.textContent = s;
  return d.innerHTML;
}

/** Convert a File to base64 data string */
function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result as string;
      // Strip data URL prefix to get raw base64
      const base64 = result.split(',')[1] || result;
      resolve(base64);
    };
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
}
function escAttr(s: string): string {
  return s.replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

/** Render markdown-like text to HTML (used for chat messages) */
function formatMarkdown(text: string): string {
  let html = text.replace(/```(\w*)\n([\s\S]*?)```/g, (_m, lang, code) => {
    return `<pre class="code-block" data-lang="${escHtml(lang)}"><code>${escHtml(code.trimEnd())}</code></pre>`;
  });
  const parts = html.split(/(<pre class="code-block"[\s\S]*?<\/pre>)/);
  html = parts.map((part, i) => {
    if (i % 2 === 1) return part;
    return escHtml(part)
      .replace(/`([^`]+)`/g, '<code class="inline-code">$1</code>')
      .replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
      .replace(/\*(.*?)\*/g, '<em>$1</em>')
      .replace(/^### (.+)$/gm, '<h4>$1</h4>')
      .replace(/^## (.+)$/gm, '<h3>$1</h3>')
      .replace(/^# (.+)$/gm, '<h2>$1</h2>')
      .replace(/^[-•] (.+)$/gm, '<div class="md-bullet">• $1</div>')
      .replace(/^\d+\. (.+)$/gm, '<div class="md-bullet">$&</div>')
      .replace(/\[([^\]]+)\]\((https?:\/\/[^)]+)\)/g, '<a href="$2" target="_blank" rel="noopener">$1</a>')
      .replace(/\n/g, '<br>');
  }).join('');
  return html;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return bytes + ' B';
  if (bytes < 1048576) return (bytes / 1024).toFixed(1) + ' KB';
  return (bytes / 1048576).toFixed(1) + ' MB';
}

// ── Global Toast Notification ──────────────────────────────────────────────
function showToast(message: string, type: 'info' | 'success' | 'error' | 'warning' = 'info', durationMs = 3500) {
  const container = $('global-toast');
  if (!container) return;
  container.textContent = message;
  container.className = `global-toast toast-${type}`;
  container.style.display = '';
  container.style.opacity = '1';
  // Auto dismiss
  setTimeout(() => {
    container.style.opacity = '0';
    setTimeout(() => { container.style.display = 'none'; }, 300);
  }, durationMs);
}

// Settings view is now in src/views/settings.ts

// ── Exec Approval event handler ────────────────────────────────────────────
// Maps himalaya CLI subcommands to permission categories
const HIMALAYA_PERM_MAP: Record<string, keyof MailPermissions> = {
  'envelope list': 'read', 'envelope get': 'read', 'envelope watch': 'read',
  'message read': 'read', 'message get': 'read', 'message list': 'read',
  'message write': 'send', 'message send': 'send', 'message reply': 'send', 'message forward': 'send',
  'message delete': 'delete', 'message remove': 'delete', 'flag remove': 'delete',
  'folder create': 'manage', 'folder delete': 'manage', 'folder rename': 'manage',
  'message move': 'manage', 'message copy': 'manage', 'flag add': 'manage', 'flag set': 'manage',
};

/** Classify a tool invocation's required permission. Returns null if not a mail tool. */
function classifyMailPermission(toolName: string, args?: Record<string, unknown>): { perm: keyof MailPermissions; label: string } | null {
  const name = (toolName || '').toLowerCase();
  if (!name.includes('himalaya')) return null;

  // Try to match against known subcommands from args or tool name
  const argsStr = args ? JSON.stringify(args).toLowerCase() : '';
  for (const [sub, perm] of Object.entries(HIMALAYA_PERM_MAP)) {
    if (name.includes(sub) || argsStr.includes(sub)) {
      return { perm, label: sub };
    }
  }
  // Fallback heuristics
  if (name.includes('send') || argsStr.includes('send')) return { perm: 'send', label: 'send' };
  if (name.includes('delete') || argsStr.includes('delete')) return { perm: 'delete', label: 'delete' };
  if (name.includes('move') || name.includes('folder')) return { perm: 'manage', label: 'manage' };
  // Default: treat as read
  return { perm: 'read', label: 'read' };
}

// ── Node gateway events ────────────────────────────────────────────────────
gateway.on('node.pair.requested', (payload: unknown) => {
  NodesModule.handleNodePairRequested(payload);
});
gateway.on('node.pair.resolved', (payload: unknown) => {
  NodesModule.handleNodePairResolved(payload);
});
gateway.on('node.invoke.result', (payload: unknown) => {
  const evt = payload as { nodeId?: string; command?: string; result?: unknown; error?: string };
  console.log('[main] node.invoke.result:', evt);
  // Refresh node list in case state changed
  if (wsConnected && nodesView?.classList.contains('active')) {
    NodesModule.loadNodes();
  }
});
gateway.on('node.event', (payload: unknown) => {
  const evt = payload as { nodeId?: string; event?: string; data?: unknown };
  console.log('[main] node.event:', evt);
  // Refresh node list (a node may have connected/disconnected)
  if (wsConnected && nodesView?.classList.contains('active')) {
    NodesModule.loadNodes();
  }
});

// ── Device pairing events ──────────────────────────────────────────────────
gateway.on('device.pair.requested', (payload: unknown) => {
  console.log('[main] device.pair.requested:', payload);
  // Refresh devices list if settings is open
  if (wsConnected && settingsView?.classList.contains('active')) {
    SettingsModule.loadSettingsDevices();
  }
  showToast('New device pairing request — check Settings', 'info');
});
gateway.on('device.pair.resolved', (payload: unknown) => {
  console.log('[main] device.pair.resolved:', payload);
  if (wsConnected && settingsView?.classList.contains('active')) {
    SettingsModule.loadSettingsDevices();
  }
});

gateway.on('exec.approval.requested', (payload: unknown) => {
  const evt = payload as Record<string, unknown>;
  const id = (evt.id ?? evt.approvalId) as string | undefined;
  const tool = (evt.tool ?? evt.name ?? '') as string;
  const desc = (evt.description ?? evt.message ?? `The agent wants to use tool: ${tool}`) as string;
  const args = evt.args as Record<string, unknown> | undefined;

  const modal = $('approval-modal');
  const modalCard = $('approval-modal-card');
  const modalTitle = $('approval-modal-title');
  const descEl = $('approval-modal-desc');
  const detailsEl = $('approval-modal-details');
  const riskBanner = $('approval-risk-banner');
  const riskIcon = $('approval-risk-icon');
  const riskLabel = $('approval-risk-label');
  const riskReason = $('approval-risk-reason');
  const typeConfirm = $('approval-type-confirm');
  const typeInput = $('approval-type-input') as HTMLInputElement | null;
  const allowBtn = $('approval-allow-btn') as HTMLButtonElement | null;
  if (!modal || !descEl) return;

  const sessionKey = (evt.sessionKey ?? '') as string;

  // ── Permission enforcement for mail/credential tools ──
  const mailPerm = classifyMailPermission(tool, args);
  if (mailPerm) {
    const anyAllowed = MailModule.getMailAccounts().some(acct => {
      const perms = MailModule.loadMailPermissions(acct.name);
      return perms[mailPerm.perm];
    });
    if (!anyAllowed) {
      if (id) gateway.execApprovalResolve(id, false).catch(console.warn);
      logCredentialActivity({
        action: 'blocked',
        toolName: tool,
        detail: `Blocked: "${mailPerm.label}" permission is disabled in Credential Vault`,
        sessionKey,
        wasAllowed: false,
      });
      showToast(`Blocked: your Credential Vault doesn't allow "${mailPerm.label}" — update permissions in Mail sidebar`, 'warning');
      return;
    }
    logCredentialActivity({
      action: mailPerm.perm,
      toolName: tool,
      detail: `Agent requested: ${tool}${args ? ' ' + JSON.stringify(args).slice(0, 120) : ''}`,
      sessionKey,
      wasAllowed: true,
    });
  }

  // ── Security: Risk classification ──
  const secSettings = loadSecuritySettings();
  const risk: RiskClassification | null = classifyCommandRisk(tool, args);

  // Build a command string for allowlist/denylist matching
  const cmdStr = args
    ? Object.values(args).filter(v => typeof v === 'string').join(' ')
    : tool;

  // ── Network request auditing (C5) ──
  const netAudit = auditNetworkRequest(tool, args);
  if (netAudit.isNetworkRequest) {
    const targetStr = netAudit.targets.length > 0 ? netAudit.targets.join(', ') : '(unknown destination)';
    logSecurityEvent({
      eventType: 'network_request',
      riskLevel: netAudit.isExfiltration ? 'critical' : (netAudit.allTargetsLocal ? null : 'medium'),
      toolName: tool,
      command: cmdStr,
      detail: `Outbound request → ${targetStr}${netAudit.isExfiltration ? ' [EXFILTRATION SUSPECTED]' : ''}${netAudit.allTargetsLocal ? ' (localhost)' : ''}`,
      sessionKey,
      wasAllowed: true, // will be updated by allow/deny below
      matchedPattern: netAudit.isExfiltration ? `exfiltration:${netAudit.exfiltrationReason}` : 'network_tool',
    });
  }

  // ── Session override: "Allow all for this session" (C3) ──
  const overrideRemaining = getSessionOverrideRemaining();
  if (overrideRemaining > 0) {
    // Session override is active — auto-approve (but still deny critical privilege escalation)
    if (!(secSettings.autoDenyPrivilegeEscalation && isPrivilegeEscalation(tool, args))) {
      if (id) gateway.execApprovalResolve(id, true).catch(console.warn);
      const minsLeft = Math.ceil(overrideRemaining / 60000);
      logCredentialActivity({ action: 'approved', toolName: tool, detail: `Session override active (${minsLeft}min remaining): ${tool}`, sessionKey, wasAllowed: true });
      logSecurityEvent({ eventType: 'session_override', riskLevel: risk?.level ?? null, toolName: tool, command: cmdStr, detail: `Session override auto-approved (${minsLeft}min remaining)`, sessionKey, wasAllowed: true, matchedPattern: 'session_override' });
      return;
    }
  }

  // ── Read-only project mode: block filesystem writes (H3) ──
  if (secSettings.readOnlyProjects) {
    const writeCheck = isFilesystemWriteTool(tool, args);
    if (writeCheck.isWrite) {
      if (id) gateway.execApprovalResolve(id, false).catch(console.warn);
      logCredentialActivity({ action: 'blocked', toolName: tool, detail: `Blocked: filesystem write tool in read-only mode${writeCheck.targetPath ? ` → ${writeCheck.targetPath}` : ''}`, sessionKey, wasAllowed: false });
      logSecurityEvent({ eventType: 'auto_deny', riskLevel: 'medium', toolName: tool, command: cmdStr, detail: `Read-only mode: filesystem write blocked${writeCheck.targetPath ? ` → ${writeCheck.targetPath}` : ''}`, sessionKey, wasAllowed: false, matchedPattern: 'read_only_mode' });
      showToast('Blocked: filesystem writes are disabled (read-only project mode)', 'warning');
      return;
    }
  }

  // ── Auto-deny: privilege escalation ──
  if (secSettings.autoDenyPrivilegeEscalation && isPrivilegeEscalation(tool, args)) {
    if (id) gateway.execApprovalResolve(id, false).catch(console.warn);
    logCredentialActivity({ action: 'blocked', toolName: tool, detail: `Auto-denied: privilege escalation command (sudo/su/doas/pkexec)`, sessionKey, wasAllowed: false });
    logSecurityEvent({ eventType: 'auto_deny', riskLevel: 'critical', toolName: tool, command: cmdStr, detail: 'Privilege escalation auto-denied', sessionKey, wasAllowed: false, matchedPattern: 'privilege_escalation' });
    showToast('Auto-denied: privilege escalation command blocked by security policy', 'warning');
    return;
  }

  // ── Auto-deny: all critical-risk commands ──
  if (secSettings.autoDenyCritical && risk?.level === 'critical') {
    if (id) gateway.execApprovalResolve(id, false).catch(console.warn);
    logCredentialActivity({ action: 'blocked', toolName: tool, detail: `Auto-denied: critical risk — ${risk.label}: ${risk.reason}`, sessionKey, wasAllowed: false });
    logSecurityEvent({ eventType: 'auto_deny', riskLevel: 'critical', toolName: tool, command: cmdStr, detail: `${risk.label}: ${risk.reason}`, sessionKey, wasAllowed: false, matchedPattern: risk.matchedPattern });
    showToast(`Auto-denied: ${risk.label} — ${risk.reason}`, 'warning');
    return;
  }

  // ── Auto-deny: command denylist ──
  if (secSettings.commandDenylist.length > 0 && matchesDenylist(cmdStr, secSettings.commandDenylist)) {
    if (id) gateway.execApprovalResolve(id, false).catch(console.warn);
    logCredentialActivity({ action: 'blocked', toolName: tool, detail: `Auto-denied: matched command denylist pattern`, sessionKey, wasAllowed: false });
    logSecurityEvent({ eventType: 'auto_deny', riskLevel: risk?.level ?? null, toolName: tool, command: cmdStr, detail: 'Matched command denylist', sessionKey, wasAllowed: false, matchedPattern: 'denylist' });
    showToast('Auto-denied: command matched your denylist', 'warning');
    return;
  }

  // ── Auto-approve: command allowlist (only if no risk detected) ──
  if (!risk && secSettings.commandAllowlist.length > 0 && matchesAllowlist(cmdStr, secSettings.commandAllowlist)) {
    if (id) gateway.execApprovalResolve(id, true).catch(console.warn);
    logCredentialActivity({ action: 'approved', toolName: tool, detail: `Auto-approved: matched command allowlist pattern`, sessionKey, wasAllowed: true });
    logSecurityEvent({ eventType: 'auto_allow', toolName: tool, command: cmdStr, detail: 'Matched command allowlist', sessionKey, wasAllowed: true, matchedPattern: 'allowlist' });
    return;
  }

  // ── Configure modal appearance based on risk ──
  const isDangerous = risk && (risk.level === 'critical' || risk.level === 'high');
  const isCritical = risk?.level === 'critical';

  // Reset modal state
  modalCard?.classList.remove('danger-modal');
  riskBanner?.classList.remove('risk-critical', 'risk-high', 'risk-medium');
  if (riskBanner) riskBanner.style.display = 'none';
  if (typeConfirm) typeConfirm.style.display = 'none';
  if (typeInput) typeInput.value = '';
  if (allowBtn) { allowBtn.disabled = false; allowBtn.textContent = 'Allow'; }
  if (modalTitle) modalTitle.textContent = 'Tool Approval Required';

  if (risk) {
    // Show danger modal variant
    if (isDangerous) {
      modalCard?.classList.add('danger-modal');
      if (modalTitle) modalTitle.textContent = '⚠ Dangerous Command Detected';
    }

    // Show risk banner
    if (riskBanner && riskLabel && riskReason && riskIcon) {
      riskBanner.style.display = 'flex';
      riskBanner.classList.add(`risk-${risk.level}`);
      riskLabel.textContent = `${risk.level.toUpperCase()}: ${risk.label}`;
      riskReason.textContent = risk.reason;
      riskIcon.textContent = isCritical ? '☠' : risk.level === 'high' ? '⚠' : '⚡';
    }

    // Type-to-confirm for critical commands
    if (isCritical && secSettings.requireTypeToCritical && typeConfirm && typeInput && allowBtn) {
      typeConfirm.style.display = 'block';
      allowBtn.disabled = true;
      allowBtn.textContent = 'Type ALLOW first';
      const onTypeInput = () => {
        const val = typeInput.value.trim().toUpperCase();
        allowBtn.disabled = val !== 'ALLOW';
        allowBtn.textContent = val === 'ALLOW' ? 'Allow' : 'Type ALLOW first';
      };
      typeInput.addEventListener('input', onTypeInput);
      // Store cleanup ref
      (typeInput as unknown as Record<string, unknown>)._secCleanup = onTypeInput;
    }
  }

  descEl.textContent = desc;

  // ── Network audit banner (C5) ──
  const netBanner = $('approval-network-banner');
  if (netBanner) netBanner.style.display = 'none';
  if (netAudit.isNetworkRequest && netBanner) {
    netBanner.style.display = 'block';
    const targetStr = netAudit.targets.length > 0 ? netAudit.targets.join(', ') : 'unknown destination';
    if (netAudit.isExfiltration) {
      netBanner.className = 'network-banner network-exfiltration';
      netBanner.innerHTML = `<strong>⚠ Possible Data Exfiltration</strong><br>Outbound data transfer detected → ${escHtml(targetStr)}`;
    } else if (!netAudit.allTargetsLocal) {
      netBanner.className = 'network-banner network-external';
      netBanner.innerHTML = `<strong>🌐 External Network Request</strong><br>Destination: ${escHtml(targetStr)}`;
    } else {
      netBanner.className = 'network-banner network-local';
      netBanner.innerHTML = `<strong>🔒 Localhost Request</strong><br>Destination: ${escHtml(targetStr)}`;
    }
  }

  if (detailsEl) {
    detailsEl.innerHTML = args
      ? `<pre class="code-block"><code>${escHtml(JSON.stringify(args, null, 2))}</code></pre>`
      : '';
  }
  modal.style.display = 'flex';

  // Resolve when user clicks Allow/Deny
  const cleanup = () => {
    modal.style.display = 'none';
    // Remove type-input listener
    if (typeInput) {
      const fn = (typeInput as unknown as Record<string, unknown>)._secCleanup as (() => void) | undefined;
      if (fn) typeInput.removeEventListener('input', fn);
    }
    $('approval-allow-btn')?.removeEventListener('click', onAllow);
    $('approval-deny-btn')?.removeEventListener('click', onDeny);
    $('approval-modal-close')?.removeEventListener('click', onDeny);
  };
  const onAllow = () => {
    cleanup();
    if (id) gateway.execApprovalResolve(id, true).catch(console.warn);
    const riskNote = risk ? ` (${risk.level}: ${risk.label})` : '';
    logCredentialActivity({ action: 'approved', toolName: tool, detail: `User approved${riskNote}: ${tool}${args ? ' ' + JSON.stringify(args).slice(0, 120) : ''}`, sessionKey, wasAllowed: true });
    logSecurityEvent({ eventType: 'exec_approval', riskLevel: risk?.level ?? null, toolName: tool, command: cmdStr, detail: `User approved${riskNote}`, sessionKey, wasAllowed: true, matchedPattern: risk?.matchedPattern });
    showToast('Tool approved', 'success');
  };
  const onDeny = () => {
    cleanup();
    if (id) gateway.execApprovalResolve(id, false).catch(console.warn);
    const riskNote = risk ? ` (${risk.level}: ${risk.label})` : '';
    logCredentialActivity({ action: 'denied', toolName: tool, detail: `User denied${riskNote}: ${tool}${args ? ' ' + JSON.stringify(args).slice(0, 120) : ''}`, sessionKey, wasAllowed: false });
    logSecurityEvent({ eventType: 'exec_approval', riskLevel: risk?.level ?? null, toolName: tool, command: cmdStr, detail: `User denied${riskNote}`, sessionKey, wasAllowed: false, matchedPattern: risk?.matchedPattern });
    showToast('Tool denied', 'warning');
  };
  $('approval-allow-btn')?.addEventListener('click', onAllow);
  $('approval-deny-btn')?.addEventListener('click', onDeny);
  $('approval-modal-close')?.addEventListener('click', onDeny);

  // ── Session override dropdown (C3) ──
  const overrideBtn = $('session-override-btn');
  const overrideMenu = $('session-override-menu');
  if (overrideBtn && overrideMenu) {
    const toggleMenu = (e: Event) => {
      e.stopPropagation();
      overrideMenu.style.display = overrideMenu.style.display === 'none' ? 'flex' : 'none';
    };
    overrideBtn.addEventListener('click', toggleMenu);
    overrideMenu.querySelectorAll('.session-override-opt').forEach(opt => {
      opt.addEventListener('click', () => {
        const mins = parseInt((opt as HTMLElement).dataset.minutes ?? '30', 10);
        activateSessionOverride(mins);
        overrideMenu.style.display = 'none';
        // Auto-approve this request too
        cleanup();
        if (id) gateway.execApprovalResolve(id, true).catch(console.warn);
        logCredentialActivity({ action: 'approved', toolName: tool, detail: `Session override activated (${mins}min): ${tool}`, sessionKey, wasAllowed: true });
        logSecurityEvent({ eventType: 'session_override', riskLevel: risk?.level ?? null, toolName: tool, command: cmdStr, detail: `Session override activated (${mins}min)`, sessionKey, wasAllowed: true, matchedPattern: 'session_override' });
        showToast(`Session override active for ${mins} minutes — all tool requests auto-approved`, 'info');
      });
    });
  }
});

// ── Additional gateway events ──────────────────────────────────────────────
gateway.on('exec.approval.resolved', (payload: unknown) => {
  const evt = payload as { id?: string; tool?: string; allowed?: boolean };
  console.log('[main] exec.approval.resolved:', evt);
  // Close modal if it was for this approval
  const modal = $('approval-modal');
  if (modal?.style.display !== 'none') {
    modal!.style.display = 'none';
  }
});

gateway.on('presence', (payload: unknown) => {
  console.log('[main] presence:', payload);
  // Refresh presence list if settings is open
  if (wsConnected && settingsView?.classList.contains('active')) {
    SettingsModule.loadSettingsPresence();
  }
});

gateway.on('cron', (payload: unknown) => {
  const evt = payload as { jobId?: string; status?: string; label?: string };
  console.log('[main] cron event:', evt);
  // Refresh automations if that view is open
  const autoView = $('automations-view');
  if (wsConnected && autoView?.classList.contains('active')) {
    loadCron();
  }
});

gateway.on('shutdown', (_payload: unknown) => {
  console.warn('[main] Gateway shutdown event received');
  showToast('Gateway is shutting down…', 'warning');
});

// ── Initialize ─────────────────────────────────────────────────────────────
document.addEventListener('DOMContentLoaded', async () => {
  try {
    console.log('[main] Paw starting...');

    // Render inline SVG icons in static HTML buttons
    for (const el of document.querySelectorAll<HTMLElement>('[data-icon]')) {
      const name = el.dataset.icon;
      if (name) el.innerHTML = icon(name);
    }

    // Check for crash log from previous run
    try {
      const prevLog = localStorage.getItem('paw-crash-log');
      if (prevLog) {
        const entries = JSON.parse(prevLog) as string[];
        if (entries.length) {
          console.warn(`[main] Previous crash log (${entries.length} entries):`);
          entries.slice(-5).forEach(e => console.warn('  ', e));
        }
      }
    } catch { /* ignore */ }
    crashLog('startup');

    // Initialise local SQLite database
    await initDb().catch(e => console.warn('[main] DB init failed:', e));

    // C2: Initialise field-level encryption key from OS keychain
    await initDbEncryption().catch(e => console.warn('[main] DB encryption init failed:', e));

    // Initialize Memory Palace module events
    MemoryPalaceModule.initPalaceEvents();

    // Handle palace-open-file event from memory-palace module
    window.addEventListener('palace-open-file', (e: Event) => {
      const filePath = (e as CustomEvent).detail as string;
      openMemoryFile(filePath);
    });

    // Initialize Mail module
    MailModule.configure({
      switchView,
      setCurrentSession: (key) => { currentSessionKey = key; },
      getChatInput: () => chatInput,
      closeChannelSetup,
    });
    MailModule.initMailEvents();

    // Initialize Skills module events
    SkillsModule.initSkillsEvents();

    // Initialize Foundry module events
    FoundryModule.configure({ promptModal });
    FoundryModule.initFoundryEvents();

    // Initialize Research module events
    ResearchModule.configure({ promptModal });
    ResearchModule.initResearchEvents();

    // Initialize Agents module
    AgentsModule.configure({
      switchView,
      setCurrentAgent: (agentId) => { console.log('[main] Agent selected:', agentId); },
    });
    AgentsModule.initAgents();

    // Initialize Nodes module events
    NodesModule.initNodesEvents();
    NodesModule.configureCallbacks({
      onCommandResult: (command, result) => {
        console.log(`[main] Node command result: ${command}`, result);
      },
    });

    // Initialize Settings module (wires approvals save/refresh/add-rule buttons)
    SettingsModule.initSettings();

    // Initialize new settings tab modules
    initSettingsTabs();
    ModelsSettings.initModelsSettings();
    EnvSettings.initEnvSettings();
    AgentDefaultsSettings.initAgentDefaultsSettings();
    SessionsSettings.initSessionsSettings();
    VoiceSettings.initVoiceSettings();
    AdvancedSettings.initAdvancedSettings();

    // Initialize Projects module events
    ProjectsModule.bindEvents();

    loadConfigFromStorage();
    console.log(`[main] After loadConfigFromStorage: configured=${config.configured} url="${config.gateway.url}" tokenLen=${config.gateway.token?.length ?? 0}`);

    // Always try to read live config from disk — it may have a newer token/port
    const freshConfig = await refreshConfigFromDisk();
    console.log(`[main] After refreshConfigFromDisk: fresh=${freshConfig} configured=${config.configured} url="${config.gateway.url}" tokenLen=${config.gateway.token?.length ?? 0}`);

    if (config.configured && config.gateway.token) {
      switchView('dashboard');

      // Probe first, then connect
      const port = getPortFromUrl(config.gateway.url);
      console.log(`[main] Config ready, probing gateway on port ${port}...`);

      const running = invoke
        ? await invoke<boolean>('check_gateway_health', { port }).catch(() => false)
        : await probeHealth().catch(() => false);

      console.log(`[main] Gateway probe: running=${running}`);

      if (running) {
        console.log('[main] Gateway running, connecting...');
        await connectGateway();
      } else {
        // Gateway not running — try to start it, then connect
        if (invoke) {
          console.log('[main] Gateway not responding, attempting to start...');
          await invoke('start_gateway', { port }).catch((e: unknown) => {
            console.warn('[main] Gateway start failed:', e);
          });
          // Wait for gateway to boot up
          console.log('[main] Waiting 3s for gateway to start...');
          await new Promise(r => setTimeout(r, 3000));
          console.log('[main] Retrying connection after gateway start...');
          await connectGateway();
        } else {
          console.warn('[main] No Tauri runtime — cannot start gateway');
          if (statusText) statusText.textContent = 'Disconnected';
        }
      }
    } else {
      console.log(`[main] Not configured or no token, showing setup. configured=${config.configured} hasToken=${!!config.gateway.token}`);
      showView('setup-view');
    }

    // Poll status every 15s — reconnect if WS dropped
    setInterval(() => {
      checkGatewayStatus().catch(e => console.warn('[main] Status poll error:', e));
    }, 15_000);

    console.log('[main] Paw initialized');
  } catch (e) {
    console.error('[main] Init error:', e);
    showView('setup-view');
  }
});
