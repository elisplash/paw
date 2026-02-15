// Paw — Main Application
// Wires OpenClaw gateway (WebSocket protocol v3) to the UI

import type { AppConfig, Message, InstallProgress, ChatMessage, Session, SkillEntry } from './types';
import { setGatewayConfig, probeHealth } from './api';
import { gateway } from './gateway';
import { initDb, listModes, saveMode, deleteMode, listDocs, saveDoc, getDoc, deleteDoc, listProjects, saveProject, deleteProject, listProjectFiles, saveProjectFile, deleteProjectFile, logCredentialActivity, getCredentialActivityLog } from './db';
import type { AgentMode } from './db';

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

let messages: Message[] = [];
let isLoading = false;
let currentSessionKey: string | null = null;
let sessions: Session[] = [];
let wsConnected = false;
let _streamingContent = '';  // accumulates deltas for current streaming response
let _streamingEl: HTMLElement | null = null;  // the live-updating DOM element
let _streamingRunId: string | null = null;
let _streamingResolve: ((text: string) => void) | null = null;  // resolves when agent run completes
let _streamingTimeout: ReturnType<typeof setTimeout> | null = null;

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
  skillsView, foundryView, settingsView,
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
  };
  const target = viewMap[viewName];
  if (target) target.classList.add('active');

  // Auto-load data when switching to a data view
  if (wsConnected) {
    switch (viewName) {
      case 'dashboard': loadDashboardCron(); break;
      case 'chat': loadSessions(); break;
      case 'channels': loadChannels(); break;
      case 'automations': loadCron(); break;
      case 'skills': loadSkills(); break;
      case 'foundry': loadModels(); loadModes(); loadAgents(); break;
      case 'memory': loadMemoryPalace(); loadMemory(); break;
      case 'build': loadBuildProjects(); loadSpaceCron('build'); break;
      case 'mail': loadMail(); loadSpaceCron('mail'); break;
      case 'settings': syncSettingsForm(); loadGatewayConfig(); loadSettingsLogs(); loadSettingsUsage(); loadSettingsPresence(); break;
      default: break;
    }
  }
  // Local-only views (no gateway needed)
  switch (viewName) {
    case 'content': loadContentDocs(); if (wsConnected) loadSpaceCron('content'); break;
    case 'research': loadResearchProjects(); if (wsConnected) loadSpaceCron('research'); break;
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

    const hello = await gateway.connect({ url: wsUrl, token: config.gateway.token });
    wsConnected = true;
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

    // Load agent name
    try {
      const agents = await gateway.listAgents();
      if (agents.agents?.length && chatAgentName) {
        const main = agents.agents.find(a => a.id === agents.defaultId) ?? agents.agents[0];
        chatAgentName.textContent = main.identity?.name ?? main.name ?? main.id;
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
  statusDot?.classList.add('connected');
  statusDot?.classList.remove('error');
  if (statusText) statusText.textContent = 'Connected';
});
gateway.on('_disconnected', () => {
  wsConnected = false;
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
  console.error('[main] Gateway reconnect exhausted — giving up. Refresh to retry.');
});

// ── Status check (fallback for polling) ────────────────────────────────────
async function checkGatewayStatus() {
  if (wsConnected || _connectInProgress || gateway.isConnecting) return;
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
    await gateway.configSet(parsed);
    alert('Configuration saved!');
  } catch (e) {
    alert(`Invalid config: ${e instanceof Error ? e.message : e}`);
  }
});

$('settings-reload-config')?.addEventListener('click', () => loadGatewayConfig());

// ══════════════════════════════════════════════════════════════════════════
// ═══ DATA VIEWS ════════════════════════════════════════════════════════════
// ══════════════════════════════════════════════════════════════════════════

// ── Sessions / Chat ────────────────────────────────────────────────────────
async function loadSessions() {
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
    if (currentSessionKey && !isLoading) await loadChatHistory(currentSessionKey);
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

function chatMsgToMessage(m: ChatMessage): Message {
  const ts = m.ts ?? m.timestamp;
  return {
    id: m.id ?? undefined,
    role: m.role as 'user' | 'assistant' | 'system',
    content: extractContent(m.content),
    timestamp: ts ? new Date(ts as string | number) : new Date(),
    toolCalls: m.toolCalls,
  };
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

$('new-chat-btn')?.addEventListener('click', () => {
  messages = [];
  currentSessionKey = null;
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

async function sendMessage() {
  const content = chatInput?.value.trim();
  if (!content || isLoading) return;

  addMessage({ role: 'user', content, timestamp: new Date() });
  if (chatInput) { chatInput.value = ''; chatInput.style.height = 'auto'; }
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

  try {
    const sessionKey = currentSessionKey ?? 'default';

    // Read selected mode's overrides (model, system prompt, thinking level)
    const modeSelect = $('chat-mode-select') as HTMLSelectElement | null;
    const selectedModeId = modeSelect?.value;
    let chatOpts: { model?: string; systemPrompt?: string; thinkingLevel?: string; temperature?: number } = {};
    if (selectedModeId) {
      const modes = await listModes();
      const mode = modes.find(m => m.id === selectedModeId);
      if (mode) {
        if (mode.model) chatOpts.model = mode.model;
        if (mode.system_prompt) chatOpts.systemPrompt = mode.system_prompt;
        if (mode.thinking_level) chatOpts.thinkingLevel = mode.thinking_level;
        if (mode.temperature > 0) chatOpts.temperature = mode.temperature;
      }
    }

    const result = await gateway.chatSend(sessionKey, content, chatOpts);
    console.log('[main] chat.send ack:', JSON.stringify(result).slice(0, 300));

    // Store the runId so we can filter events precisely
    if (result.runId) _streamingRunId = result.runId;
    if (result.sessionKey) currentSessionKey = result.sessionKey;

    // Now wait for the agent events to deliver the full response
    const finalText = await responsePromise;
    finalizeStreaming(finalText);
    // Refresh session list (but skip re-loading chat history — we already have it)
    loadSessions().catch(() => {});
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
  _streamingRunId = null;
  _streamingContent = '';
  // Hide abort button
  const abortBtn = $('chat-abort-btn');
  if (abortBtn) abortBtn.style.display = 'none';

  if (finalContent) {
    addMessage({ role: 'assistant', content: finalContent, timestamp: new Date(), toolCalls });
  }
}

function addMessage(message: Message) {
  messages.push(message);
  renderMessages();
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
  for (const msg of messages) {
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
    div.appendChild(time);

    if (msg.toolCalls?.length) {
      const badge = document.createElement('div');
      badge.className = 'tool-calls-badge';
      badge.textContent = `${msg.toolCalls.length} tool call${msg.toolCalls.length > 1 ? 's' : ''}`;
      div.appendChild(badge);
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

    // Route paw-research-* events to the Research view
    if (evtSession && evtSession.startsWith('paw-research-')) {
      if (!_researchStreaming) return;
      if (_researchRunId && runId && runId !== _researchRunId) return;

      if (stream === 'assistant' && data) {
        const delta = data.delta as string | undefined;
        if (delta) appendResearchDelta(delta);
      } else if (stream === 'lifecycle' && data) {
        const phase = data.phase as string | undefined;
        if (phase === 'start' && !_researchRunId && runId) _researchRunId = runId;
        if (phase === 'end' && _researchResolve) {
          _researchResolve(_researchContent);
          _researchResolve = null;
        }
      } else if (stream === 'tool' && data) {
        const tool = (data.name ?? data.tool) as string | undefined;
        const phase = data.phase as string | undefined;
        if (phase === 'start' && tool) appendResearchDelta(`\n\n▶ ${tool}...`);
      } else if (stream === 'error' && data) {
        const error = (data.message ?? data.error ?? '') as string;
        if (error) appendResearchDelta(`\n\nError: ${error}`);
        if (_researchResolve) { _researchResolve(_researchContent); _researchResolve = null; }
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

    // Route paw-create-* events to the Content view
    if (evtSession && evtSession.startsWith('paw-create-')) {
      if (!_contentStreaming) return;
      if (_contentStreamRunId && runId && runId !== _contentStreamRunId) return;
      if (stream === 'assistant' && data) {
        const delta = data.delta as string | undefined;
        if (delta) _contentStreamContent += delta;
      } else if (stream === 'lifecycle' && data) {
        const phase = data.phase as string | undefined;
        if (phase === 'start' && !_contentStreamRunId && runId) _contentStreamRunId = runId;
        if (phase === 'end' && _contentStreamResolve) { _contentStreamResolve(_contentStreamContent); _contentStreamResolve = null; }
      } else if (stream === 'error' && data) {
        const error = (data.message ?? data.error ?? '') as string;
        if (error) _contentStreamContent += `\n\nError: ${error}`;
        if (_contentStreamResolve) { _contentStreamResolve(_contentStreamContent); _contentStreamResolve = null; }
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
        console.log(`[main] Agent run ended: ${runId}`);
        if (_streamingResolve) {
          _streamingResolve(_streamingContent);
          _streamingResolve = null;
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

// Listen for chat events — only care about 'final' (assembled message).
// We skip 'delta' since agent events already handle real-time streaming.
gateway.on('chat', (payload: unknown) => {
  try {
    const evt = payload as Record<string, unknown>;
    const state = evt.state as string | undefined;

    // Skip delta events entirely — agent handler already processes deltas
    if (state !== 'final') return;

    const runId = evt.runId as string | undefined;
    const msg = evt.message as Record<string, unknown> | undefined;
    const chatEvtSession = evt.sessionKey as string | undefined;

    // Route paw-research-* final messages to research view
    if (chatEvtSession && chatEvtSession.startsWith('paw-research-')) {
      if (_researchStreaming && msg) {
        const text = extractContent(msg.content);
        if (text) {
          _researchContent = text;
          const liveContent = $('research-live-content');
          if (liveContent) liveContent.textContent = text;
          if (_researchResolve) { _researchResolve(text); _researchResolve = null; }
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
  if (!_channelSetupType || !wsConnected) return;

  // Mail IMAP setup is handled separately
  if (_channelSetupType === '__mail_imap__') {
    await saveMailImapSetup();
    return;
  }

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
    await gateway.configPatch({ channels: mergedChannels });

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
$('channel-setup-save')?.addEventListener('click', saveChannelSetup);
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
          await gateway.configPatch({ channels });
          showToast(`${chId} removed`, 'success');
          setTimeout(() => loadChannels(), 1000);
        } catch (e) {
          showToast(`Remove failed: ${e instanceof Error ? e.message : e}`, 'error');
        }
      });
    });
  } catch (e) {
    console.warn('Channels load failed:', e);
    if (loading) loading.style.display = 'none';
    if (empty) empty.style.display = 'flex';
  }
}
$('refresh-channels-btn')?.addEventListener('click', () => loadChannels());

// ── Mail — Email via Gmail Hooks + Himalaya ────────────────────────────────
// Gmail hooks: inbound push via GCP Pub/Sub → gateway hook → agent session
// Himalaya skill: agent CLI tool for IMAP/SMTP (read, send, reply)
// Mail accounts = hooks.gmail config + himalaya skill status

let _mailFolder = 'inbox';
let _mailGmailConfigured = false;
let _mailHimalayaReady = false;
let _mailMessages: { id: string; from: string; subject: string; snippet: string; date: Date; body?: string; sessionKey?: string; read?: boolean }[] = [];
let _mailSelectedId: string | null = null;

async function loadMail() {
  if (!wsConnected) return;
  try {
    // Check Gmail hooks config and Himalaya skill status in parallel
    const [cfgResult, skillsResult] = await Promise.all([
      gateway.configGet().catch(() => null),
      gateway.skillsStatus().catch(() => null),
    ]);

    // Gmail hooks status
    const cfg = cfgResult?.config as Record<string, unknown> | null;
    const hooks = cfg?.hooks as Record<string, unknown> | null;
    const gmail = hooks?.gmail as Record<string, unknown> | null;
    _mailGmailConfigured = !!(hooks?.enabled && gmail?.account);

    // Himalaya skill status
    const himalaya = skillsResult?.skills?.find(s => s.name === 'himalaya');
    _mailHimalayaReady = !!(himalaya?.eligible && !himalaya?.disabled);

    // Update accounts list (also reads himalaya config for configured accounts)
    await renderMailAccounts(gmail, himalaya ?? null);

    // Load inbox if we have any accounts configured (IMAP accounts or Gmail hooks)
    const hasAccounts = _mailAccounts.length > 0 || _mailGmailConfigured;
    if (hasAccounts) {
      await loadMailInbox();
    } else {
      _mailMessages = [];
      renderMailList();
      showMailEmpty(true);
    }
  } catch (e) {
    console.warn('[mail] Load failed:', e);
    showMailEmpty(true);
  }
}

interface MailPermissions {
  read: boolean;
  send: boolean;
  delete: boolean;
  manage: boolean;
}
let _mailAccounts: { name: string; email: string }[] = [];

/** Load permissions for a mail account from localStorage */
function loadMailPermissions(accountName: string): MailPermissions {
  try {
    const raw = localStorage.getItem(`mail-perms-${accountName}`);
    if (raw) return JSON.parse(raw);
  } catch { /* ignore */ }
  // Defaults: read + send on, delete + manage off
  return { read: true, send: true, delete: false, manage: false };
}

/** Save permissions for a mail account to localStorage */
function saveMailPermissions(accountName: string, perms: MailPermissions) {
  localStorage.setItem(`mail-perms-${accountName}`, JSON.stringify(perms));
}

/** Remove permissions when account is deleted */
function removeMailPermissions(accountName: string) {
  localStorage.removeItem(`mail-perms-${accountName}`);
}

async function renderMailAccounts(_gmail: Record<string, unknown> | null, himalaya: SkillEntry | null) {
  const list = $('mail-accounts-list');
  if (!list) return;
  list.innerHTML = '';
  _mailAccounts = [];

  // Read Himalaya config to find configured email accounts
  if (invoke) {
    try {
      const toml = await invoke<string>('read_himalaya_config');
      if (toml) {
        // Parse account names and emails from TOML
        const accountBlocks = toml.matchAll(/\[accounts\.([^\]]+)\][\s\S]*?email\s*=\s*"([^"]+)"/g);
        for (const match of accountBlocks) {
          _mailAccounts.push({ name: match[1], email: match[2] });
        }
      }
    } catch { /* no config yet */ }
  }

  // Show configured email accounts in vault style
  for (const acct of _mailAccounts) {
    const perms = loadMailPermissions(acct.name);
    const item = document.createElement('div');
    item.className = 'mail-vault-account';
    // Detect provider icon from email domain
    const domain = acct.email.split('@')[1] ?? '';
    let icon = 'M';
    if (domain.includes('gmail')) icon = 'G';
    else if (domain.includes('outlook') || domain.includes('hotmail') || domain.includes('live')) icon = 'O';
    else if (domain.includes('yahoo')) icon = 'Y';
    else if (domain.includes('icloud') || domain.includes('me.com')) icon = 'iC';
    else if (domain.includes('fastmail')) icon = 'FM';

    const permCount = [perms.read, perms.send, perms.delete, perms.manage].filter(Boolean).length;
    const permSummary = [perms.read && 'Read', perms.send && 'Send', perms.delete && 'Delete', perms.manage && 'Manage'].filter(Boolean).join(' · ') || 'No permissions';

    item.innerHTML = `
      <div class="mail-vault-header">
        <div class="mail-account-icon">${icon}</div>
        <div class="mail-account-info">
          <div class="mail-account-name">${escHtml(acct.email)}</div>
          <div class="mail-account-status connected">${permCount}/4 permissions active</div>
        </div>
        <button class="btn-icon mail-vault-expand" title="Manage permissions">▾</button>
      </div>
      <div class="mail-vault-details" style="display:none">
        <div class="mail-vault-perms">
          <label class="mail-vault-perm-row">
            <input type="checkbox" class="mail-vault-cb" data-perm="read" ${perms.read ? 'checked' : ''}>
            <span class="mail-vault-perm-icon">R</span>
            <span class="mail-vault-perm-name">Read emails</span>
          </label>
          <label class="mail-vault-perm-row">
            <input type="checkbox" class="mail-vault-cb" data-perm="send" ${perms.send ? 'checked' : ''}>
            <span class="mail-vault-perm-icon">S</span>
            <span class="mail-vault-perm-name">Send emails</span>
          </label>
          <label class="mail-vault-perm-row">
            <input type="checkbox" class="mail-vault-cb" data-perm="delete" ${perms.delete ? 'checked' : ''}>
            <span class="mail-vault-perm-icon">D</span>
            <span class="mail-vault-perm-name">Delete emails</span>
          </label>
          <label class="mail-vault-perm-row">
            <input type="checkbox" class="mail-vault-cb" data-perm="manage" ${perms.manage ? 'checked' : ''}>
            <span class="mail-vault-perm-icon">F</span>
            <span class="mail-vault-perm-name">Manage folders</span>
          </label>
        </div>
        <div class="mail-vault-perm-summary">${permSummary}</div>
        <div class="mail-vault-meta">
          <span class="mail-vault-meta-item">Stored locally at <code>~/.config/himalaya/</code> &mdash; password in OS keychain</span>
          <span class="mail-vault-meta-item">All actions logged in Chat</span>
        </div>
        <div class="mail-vault-actions">
          <button class="btn btn-ghost btn-sm mail-vault-revoke" data-account="${escAttr(acct.name)}">Revoke Access</button>
        </div>
      </div>
    `;
    list.appendChild(item);

    // Toggle expand/collapse
    const expandBtn = item.querySelector('.mail-vault-expand');
    const details = item.querySelector('.mail-vault-details') as HTMLElement;
    expandBtn?.addEventListener('click', () => {
      const open = details.style.display !== 'none';
      details.style.display = open ? 'none' : '';
      expandBtn.textContent = open ? '▾' : '▴';
    });

    // Wire permission toggles — instant save
    item.querySelectorAll('.mail-vault-cb').forEach(cb => {
      cb.addEventListener('change', () => {
        const updated: MailPermissions = {
          read: (item.querySelector('[data-perm="read"]') as HTMLInputElement)?.checked ?? true,
          send: (item.querySelector('[data-perm="send"]') as HTMLInputElement)?.checked ?? true,
          delete: (item.querySelector('[data-perm="delete"]') as HTMLInputElement)?.checked ?? false,
          manage: (item.querySelector('[data-perm="manage"]') as HTMLInputElement)?.checked ?? false,
        };
        saveMailPermissions(acct.name, updated);
        // Update summary display
        const count = [updated.read, updated.send, updated.delete, updated.manage].filter(Boolean).length;
        const summary = [updated.read && 'Read', updated.send && 'Send', updated.delete && 'Delete', updated.manage && 'Manage'].filter(Boolean).join(' · ') || 'No permissions';
        const statusEl = item.querySelector('.mail-account-status');
        const summaryEl = item.querySelector('.mail-vault-perm-summary');
        if (statusEl) statusEl.textContent = `${count}/4 permissions active`;
        if (summaryEl) summaryEl.textContent = summary;
        showToast(`Permissions updated for ${acct.email}`, 'info');
      });
    });

    // Wire revoke (remove) button
    item.querySelector('.mail-vault-revoke')?.addEventListener('click', async () => {
      if (!confirm(`Remove ${acct.email} and revoke all access?\n\nThis deletes the stored credentials from your device. Your email account is not affected.`)) return;
      try {
        if (invoke) await invoke('remove_himalaya_account', { accountName: acct.name });
        removeMailPermissions(acct.name);
        logCredentialActivity({
          accountName: acct.name,
          action: 'denied',
          detail: `Account revoked: ${acct.email} — credentials deleted from device`,
        });
        showToast(`${acct.email} revoked — credentials removed from this device`, 'success');
        loadMail();
      } catch (err) {
        showToast(`Remove failed: ${err instanceof Error ? err.message : err}`, 'error');
      }
    });
  }

  // Show Himalaya skill status if no accounts yet or skill isn't ready
  if (himalaya && (!himalaya.eligible || himalaya.disabled)) {
    const item = document.createElement('div');
    item.className = 'mail-account-item';
    const missingBins = himalaya.missing?.bins?.length;
    let statusLabel = 'Not installed';
    let statusClass = '';
    if (himalaya.disabled) { statusLabel = 'Disabled'; statusClass = 'muted'; }
    else if (missingBins) { statusLabel = 'Missing CLI'; statusClass = 'error'; }

    item.innerHTML = `
      <div class="mail-account-icon">H</div>
      <div class="mail-account-info">
        <div class="mail-account-name">Himalaya Skill</div>
        <div class="mail-account-status ${statusClass}">${statusLabel}</div>
      </div>
      ${himalaya.install?.length ? `<button class="btn btn-ghost btn-sm mail-himalaya-install">Install</button>` : ''}
      ${himalaya.disabled ? `<button class="btn btn-ghost btn-sm mail-himalaya-enable">Enable</button>` : ''}
    `;
    list.appendChild(item);

    item.querySelector('.mail-himalaya-install')?.addEventListener('click', async () => {
      const inst = himalaya.install?.[0];
      if (!inst) return;
      try {
        showToast('Installing Himalaya...', 'info');
        await gateway.skillsInstall(himalaya.name, inst.id);
        showToast('Himalaya installed!', 'success');
        loadMail();
      } catch (e) {
        showToast(`Install failed: ${e instanceof Error ? e.message : e}`, 'error');
      }
    });
    item.querySelector('.mail-himalaya-enable')?.addEventListener('click', async () => {
      try {
        await gateway.skillsUpdate(himalaya.skillKey ?? himalaya.name, { enabled: true });
        showToast('Himalaya enabled', 'success');
        loadMail();
      } catch (e) {
        showToast(`Enable failed: ${e instanceof Error ? e.message : e}`, 'error');
      }
    });
  }

  if (_mailAccounts.length === 0 && !himalaya) {
    list.innerHTML = '<div class="mail-no-accounts">No accounts connected</div>';
  }

  // Render activity log below accounts
  renderCredentialActivityLog();
}

async function renderCredentialActivityLog() {
  // Find or create the activity log section in the mail sidebar
  let logSection = $('mail-vault-activity');
  if (!logSection) {
    const accountsSection = document.querySelector('.mail-accounts-section');
    if (!accountsSection) return;
    logSection = document.createElement('div');
    logSection.id = 'mail-vault-activity';
    logSection.className = 'mail-vault-activity-section';
    accountsSection.after(logSection);
  }

  try {
    const entries = await getCredentialActivityLog(20);
    if (entries.length === 0) {
      logSection.innerHTML = `
        <div class="mail-vault-activity-header" id="mail-vault-activity-toggle">
          <svg class="icon-sm" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/></svg>
          Activity Log
          <span class="mail-vault-activity-count">0</span>
        </div>
        <div class="mail-vault-activity-empty">No credential activity yet</div>
      `;
      return;
    }

    const blocked = entries.filter(e => !e.was_allowed).length;
    logSection.innerHTML = `
      <div class="mail-vault-activity-header" id="mail-vault-activity-toggle">
        <svg class="icon-sm" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/></svg>
        Activity Log
        <span class="mail-vault-activity-count">${entries.length}${blocked ? ` · <span class="vault-blocked-count">${blocked} blocked</span>` : ''}</span>
        <span class="mail-vault-activity-chevron">▸</span>
      </div>
      <div class="mail-vault-activity-list" style="display:none">
        ${entries.map(e => {
          const icon = !e.was_allowed ? 'X' : e.action === 'send' ? 'S' : e.action === 'read' ? 'R' : e.action === 'delete' ? 'D' : e.action === 'manage' ? 'F' : '--';
          const cls = !e.was_allowed ? 'vault-log-blocked' : '';
          const time = e.timestamp ? new Date(e.timestamp + 'Z').toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }) : '';
          return `<div class="vault-log-entry ${cls}">
            <span class="vault-log-icon">${icon}</span>
            <div class="vault-log-body">
              <div class="vault-log-action">${escHtml(e.detail ?? e.action)}</div>
              <div class="vault-log-time">${time}${e.tool_name ? ' · ' + escHtml(e.tool_name) : ''}</div>
            </div>
          </div>`;
        }).join('')}
      </div>
    `;

    // Toggle expand
    $('mail-vault-activity-toggle')?.addEventListener('click', () => {
      const list = logSection!.querySelector('.mail-vault-activity-list') as HTMLElement | null;
      const chevron = logSection!.querySelector('.mail-vault-activity-chevron');
      if (list) {
        const open = list.style.display !== 'none';
        list.style.display = open ? 'none' : '';
        if (chevron) chevron.textContent = open ? '▸' : '▾';
      }
    });
  } catch {
    // DB not ready yet, skip
  }
}

async function loadMailInbox() {
  try {
    // List sessions — Gmail hook sessions use keys like "hook:gmail:<id>"
    const result = await gateway.listSessions({ limit: 100, includeDerivedTitles: true, includeLastMessage: true });
    const hookSessions = (result.sessions ?? []).filter(s => s.key.startsWith('hook:gmail:'));

    _mailMessages = hookSessions.map(s => {
      // Extract email metadata from the session's first message or label
      const label = s.label ?? s.displayName ?? s.key;
      // Try to parse from/subject from the label or derived title
      const fromMatch = label.match(/from\s+(.+?)(?:\n|$)/i);
      const subjMatch = label.match(/subject:\s*(.+?)(?:\n|$)/i);

      return {
        id: s.key,
        from: fromMatch?.[1] ?? 'Unknown sender',
        subject: subjMatch?.[1] ?? (label.slice(0, 80) || 'No subject'),
        snippet: (s as unknown as Record<string, unknown>).lastMessage
          ? extractContent(((s as unknown as Record<string, unknown>).lastMessage as Record<string, unknown>)?.content).slice(0, 120)
          : '',
        date: s.updatedAt ? new Date(s.updatedAt) : new Date(),
        sessionKey: s.key,
        read: true, // We don't track read state locally yet
      };
    }).sort((a, b) => b.date.getTime() - a.date.getTime());

    renderMailList();
    showMailEmpty(_mailMessages.length === 0);

    // Update inbox count
    const countEl = $('mail-inbox-count');
    if (countEl) countEl.textContent = String(_mailMessages.length);
  } catch (e) {
    console.warn('[mail] Inbox load failed:', e);
    _mailMessages = [];
    renderMailList();
    showMailEmpty(true);
  }
}

function showMailEmpty(show: boolean) {
  const empty = $('mail-empty');
  const items = $('mail-items');
  if (empty) {
    empty.style.display = show ? 'flex' : 'none';
    if (show) {
      const hasAccounts = _mailAccounts.length > 0;
      const mailIcon = `<div class="empty-icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z"/><polyline points="22,6 12,13 2,6"/></svg></div>`;

      if (hasAccounts && _mailHimalayaReady) {
        // Accounts configured and skill ready — just no inbox messages
        empty.innerHTML = `
          ${mailIcon}
          <div class="empty-title">Inbox is empty</div>
          <div class="empty-subtitle">No messages yet. Use Compose to send an email or ask your agent to check mail.</div>
          <button class="btn btn-ghost" id="mail-compose-cta" style="margin-top:16px">Compose Email</button>
        `;
        $('mail-compose-cta')?.addEventListener('click', () => {
          currentSessionKey = null;
          switchView('chat');
          if (chatInput) { chatInput.value = 'I want to compose a new email. Please help me draft it and use himalaya to send it when ready.'; chatInput.focus(); }
        });
      } else if (hasAccounts && !_mailHimalayaReady) {
        // Account configured but Himalaya skill not ready
        empty.innerHTML = `
          ${mailIcon}
          <div class="empty-title">Enable the Himalaya skill</div>
          <div class="empty-subtitle">Your email account is configured but the Himalaya skill needs to be installed or enabled for your agent to read and send emails.</div>
          <button class="btn btn-primary" id="mail-go-skills" style="margin-top:16px">Go to Skills</button>
        `;
        $('mail-go-skills')?.addEventListener('click', () => switchView('skills'));
      } else {
        // No accounts — show add account prompt
        empty.innerHTML = `
          ${mailIcon}
          <div class="empty-title">Connect your email</div>
          <div class="empty-subtitle">Add an email account so your agent can read, draft, and send emails on your behalf.</div>
          <button class="btn btn-primary" id="mail-setup-account" style="margin-top:16px">Add Email Account</button>
        `;
        $('mail-setup-account')?.addEventListener('click', () => openMailAccountSetup());
      }
    }
  }
  if (items) items.style.display = show ? 'none' : '';
}

function renderMailList() {
  const container = $('mail-items');
  if (!container) return;
  container.innerHTML = '';

  const filtered = _mailFolder === 'inbox' ? _mailMessages : [];
  // For now, only inbox has real data. Drafts/Sent/Agent are placeholders.

  if (_mailFolder !== 'inbox') {
    container.innerHTML = `<div style="padding:24px;text-align:center;color:var(--text-muted);font-size:13px">
      ${_mailFolder === 'agent' ? 'Agent-drafted emails will appear here when the agent writes emails for your review.' : 'No messages in this folder.'}
    </div>`;
    return;
  }

  for (const msg of filtered) {
    const item = document.createElement('div');
    item.className = `mail-item${msg.id === _mailSelectedId ? ' active' : ''}${!msg.read ? ' unread' : ''}`;
    item.innerHTML = `
      <div class="mail-item-sender">${escHtml(msg.from)}</div>
      <div class="mail-item-subject">${escHtml(msg.subject)}</div>
      <div class="mail-item-snippet">${escHtml(msg.snippet)}</div>
      <div class="mail-item-date">${formatMailDate(msg.date)}</div>
    `;
    item.addEventListener('click', () => openMailMessage(msg.id));
    container.appendChild(item);
  }
}

function formatMailDate(date: Date): string {
  const now = new Date();
  const diff = now.getTime() - date.getTime();
  if (diff < 86400000 && now.getDate() === date.getDate()) {
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }
  if (diff < 604800000) {
    return date.toLocaleDateString([], { weekday: 'short' });
  }
  return date.toLocaleDateString([], { month: 'short', day: 'numeric' });
}

async function openMailMessage(msgId: string) {
  _mailSelectedId = msgId;
  renderMailList(); // Re-render to highlight selected

  const msg = _mailMessages.find(m => m.id === msgId);
  const preview = $('mail-preview');
  if (!preview || !msg) return;

  // Load full message from session history
  let body = msg.snippet;
  if (msg.sessionKey) {
    try {
      const result = await gateway.chatHistory(msg.sessionKey);
      const msgs = result.messages ?? [];
      // First user message contains the email content from the hook
      const emailMsg = msgs.find(m => m.role === 'user');
      if (emailMsg) body = extractContent(emailMsg.content);
      // Agent response is the reply/processing
      const agentMsg = [...msgs].reverse().find(m => m.role === 'assistant');
      const agentReply = agentMsg ? extractContent(agentMsg.content) : null;

      preview.innerHTML = `
        <div class="mail-preview-header">
          <div class="mail-preview-from">${escHtml(msg.from)}</div>
          <div class="mail-preview-date">${msg.date.toLocaleString()}</div>
        </div>
        <div class="mail-preview-subject">${escHtml(msg.subject)}</div>
        <div class="mail-preview-body">${formatMarkdown(body)}</div>
        ${agentReply ? `
          <div class="mail-preview-agent-reply">
            <div class="mail-preview-agent-label">Agent Response</div>
            <div class="mail-preview-agent-body">${formatMarkdown(agentReply)}</div>
          </div>
        ` : ''}
        <div class="mail-preview-actions">
          ${_mailHimalayaReady ? `<button class="btn btn-primary btn-sm mail-reply-btn" data-session="${escAttr(msg.sessionKey ?? '')}">Reply via Agent</button>` : ''}
          <button class="btn btn-ghost btn-sm mail-open-session-btn" data-session="${escAttr(msg.sessionKey ?? '')}">Open in Chat</button>
        </div>
      `;

      // Wire action buttons
      preview.querySelector('.mail-reply-btn')?.addEventListener('click', () => {
        composeMailReply(msg);
      });
      preview.querySelector('.mail-open-session-btn')?.addEventListener('click', () => {
        if (msg.sessionKey) {
          currentSessionKey = msg.sessionKey;
          switchView('chat');
        }
      });
    } catch (e) {
      preview.innerHTML = `
        <div class="mail-preview-header">
          <div class="mail-preview-from">${escHtml(msg.from)}</div>
          <div class="mail-preview-date">${msg.date.toLocaleString()}</div>
        </div>
        <div class="mail-preview-subject">${escHtml(msg.subject)}</div>
        <div class="mail-preview-body">${escHtml(body)}</div>
      `;
    }
  }
}

function composeMailReply(msg: { from: string; subject: string; sessionKey?: string }) {
  // Switch to chat with a pre-filled message asking the agent to compose a reply
  const replyPrompt = `Please compose a reply to this email from ${msg.from} with subject "${msg.subject}". Use the himalaya skill to send it when I approve.`;
  if (msg.sessionKey) {
    currentSessionKey = msg.sessionKey;
  }
  switchView('chat');
  if (chatInput) {
    chatInput.value = replyPrompt;
    chatInput.focus();
  }
}

// Compose new email
$('mail-compose')?.addEventListener('click', () => {
  if (!_mailHimalayaReady) {
    showToast('Himalaya skill is required to send emails. Enable it in the Skills view.', 'error');
    return;
  }
  const prompt = 'I want to compose a new email. Please help me draft it and use himalaya to send it when ready.';
  currentSessionKey = null; // New session
  switchView('chat');
  if (chatInput) {
    chatInput.value = prompt;
    chatInput.focus();
  }
});

// Mail folder switching
document.querySelectorAll('.mail-folder').forEach(folder => {
  folder.addEventListener('click', () => {
    document.querySelectorAll('.mail-folder').forEach(f => f.classList.remove('active'));
    folder.classList.add('active');
    _mailFolder = folder.getAttribute('data-folder') ?? 'inbox';
    const titleEl = $('mail-folder-title');
    if (titleEl) {
      const labels: Record<string, string> = { inbox: 'Inbox', drafts: 'Drafts', sent: 'Sent', agent: 'Agent Drafts' };
      titleEl.textContent = labels[_mailFolder] ?? _mailFolder;
    }
    renderMailList();
    // Clear preview
    const preview = $('mail-preview');
    if (preview) preview.innerHTML = '<div class="mail-preview-empty">Select an email to read</div>';
    _mailSelectedId = null;
  });
});

// Refresh
$('mail-refresh')?.addEventListener('click', () => loadMail());

// Add account / Setup account — opens IMAP/SMTP email setup
$('mail-add-account')?.addEventListener('click', () => openMailAccountSetup());
$('mail-setup-account')?.addEventListener('click', () => openMailAccountSetup());

// Provider presets for auto-filling IMAP/SMTP servers
const EMAIL_PROVIDERS: Record<string, { name: string; icon: string; imap: string; imapPort: number; smtp: string; smtpPort: number; hint: string }> = {
  gmail: { name: 'Gmail', icon: 'G', imap: 'imap.gmail.com', imapPort: 993, smtp: 'smtp.gmail.com', smtpPort: 465, hint: 'Use an App Password — go to Google Account → Security → App Passwords' },
  outlook: { name: 'Outlook / Hotmail', icon: 'O', imap: 'outlook.office365.com', imapPort: 993, smtp: 'smtp.office365.com', smtpPort: 587, hint: 'Use your regular password, or an App Password if 2FA is on' },
  yahoo: { name: 'Yahoo Mail', icon: 'Y', imap: 'imap.mail.yahoo.com', imapPort: 993, smtp: 'smtp.mail.yahoo.com', smtpPort: 465, hint: 'Generate an App Password in Yahoo Account Settings → Security' },
  icloud: { name: 'iCloud Mail', icon: 'iC', imap: 'imap.mail.me.com', imapPort: 993, smtp: 'smtp.mail.me.com', smtpPort: 587, hint: 'Use an App-Specific Password from appleid.apple.com' },
  fastmail: { name: 'Fastmail', icon: 'FM', imap: 'imap.fastmail.com', imapPort: 993, smtp: 'smtp.fastmail.com', smtpPort: 465, hint: 'Use an App Password from Settings → Privacy & Security' },
  custom: { name: 'Other (IMAP/SMTP)', icon: '*', imap: '', imapPort: 993, smtp: '', smtpPort: 465, hint: 'Enter your mail server details manually' },
};

function openMailAccountSetup() {
  const title = $('channel-setup-title');
  const body = $('channel-setup-body');
  const modal = $('channel-setup-modal');
  const footer = $('channel-setup-save') as HTMLButtonElement | null;
  if (!title || !body || !modal || !footer) return;

  _channelSetupType = '__mail_imap__';
  title.textContent = 'Add Email Account';
  footer.style.display = '';
  footer.textContent = 'Connect Account';

  // Show provider picker first
  body.innerHTML = `
    <p class="channel-setup-desc">Choose your email provider to get started.</p>
    <div class="mail-provider-grid">
      ${Object.entries(EMAIL_PROVIDERS).map(([id, p]) => `
        <button class="mail-provider-btn" data-provider="${id}">
          <span class="mail-provider-icon">${p.icon}</span>
          <span class="mail-provider-name">${p.name}</span>
        </button>
      `).join('')}
    </div>
  `;
  footer.style.display = 'none'; // Hide save until provider is chosen

  // Wire provider selection
  body.querySelectorAll('.mail-provider-btn').forEach(btn => {
    btn.addEventListener('click', () => {
      const providerId = btn.getAttribute('data-provider') ?? 'custom';
      showMailAccountForm(providerId);
    });
  });

  modal.style.display = '';
}

function showMailAccountForm(providerId: string) {
  const provider = EMAIL_PROVIDERS[providerId] ?? EMAIL_PROVIDERS.custom;
  const body = $('channel-setup-body');
  const footer = $('channel-setup-save') as HTMLButtonElement | null;
  const title = $('channel-setup-title');
  if (!body || !footer) return;

  if (title) title.textContent = `Connect ${provider.name}`;
  footer.style.display = '';
  footer.textContent = 'Connect Account';

  const isCustom = providerId === 'custom';
  const needsAppPw = providerId === 'gmail' || providerId === 'yahoo' || providerId === 'icloud';

  body.innerHTML = `
    <div class="mail-setup-back" id="mail-setup-back">← Choose provider</div>
    ${provider.hint ? `<div class="mail-setup-hint"><svg class="icon-sm" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg> ${provider.hint}</div>` : ''}
    <div class="form-group">
      <label class="form-label" for="ch-field-mail-email">Email Address <span class="required">*</span></label>
      <input class="form-input" id="ch-field-mail-email" type="email" placeholder="you@${providerId === 'custom' ? 'example.com' : provider.imap.replace('imap.', '')}">
    </div>
    <div class="form-group">
      <label class="form-label" for="ch-field-mail-display">Display Name</label>
      <input class="form-input" id="ch-field-mail-display" type="text" placeholder="Your Name">
      <div class="form-hint">How your name appears in outgoing emails</div>
    </div>
    <div class="form-group">
      <label class="form-label" for="ch-field-mail-password">${needsAppPw ? 'App Password' : 'Password'} <span class="required">*</span></label>
      <input class="form-input" id="ch-field-mail-password" type="password" placeholder="${providerId === 'gmail' ? '16-character app password' : 'Password'}">
    </div>
    ${isCustom ? `
    <div class="form-row-2col">
      <div class="form-group">
        <label class="form-label" for="ch-field-mail-imap">IMAP Server <span class="required">*</span></label>
        <input class="form-input" id="ch-field-mail-imap" type="text" placeholder="imap.example.com" value="${escAttr(provider.imap)}">
      </div>
      <div class="form-group">
        <label class="form-label" for="ch-field-mail-imap-port">IMAP Port</label>
        <input class="form-input" id="ch-field-mail-imap-port" type="number" value="${provider.imapPort}">
      </div>
    </div>
    <div class="form-row-2col">
      <div class="form-group">
        <label class="form-label" for="ch-field-mail-smtp">SMTP Server <span class="required">*</span></label>
        <input class="form-input" id="ch-field-mail-smtp" type="text" placeholder="smtp.example.com" value="${escAttr(provider.smtp)}">
      </div>
      <div class="form-group">
        <label class="form-label" for="ch-field-mail-smtp-port">SMTP Port</label>
        <input class="form-input" id="ch-field-mail-smtp-port" type="number" value="${provider.smtpPort}">
      </div>
    </div>
    ` : `
    <input type="hidden" id="ch-field-mail-imap" value="${escAttr(provider.imap)}">
    <input type="hidden" id="ch-field-mail-imap-port" value="${provider.imapPort}">
    <input type="hidden" id="ch-field-mail-smtp" value="${escAttr(provider.smtp)}">
    <input type="hidden" id="ch-field-mail-smtp-port" value="${provider.smtpPort}">
    <div class="mail-setup-servers">
      <span>IMAP: ${provider.imap}:${provider.imapPort}</span>
      <span>SMTP: ${provider.smtp}:${provider.smtpPort}</span>
    </div>
    `}
    <input type="hidden" id="ch-field-mail-provider" value="${providerId}">

    <div class="mail-permissions-setup">
      <div class="mail-permissions-title">Agent permissions</div>
      <div class="mail-permissions-desc">Control what your agent can do with this account. You can change these any time from the Credential Vault.</div>
      <label class="mail-perm-toggle">
        <input type="checkbox" id="ch-field-perm-read" checked>
        <span class="mail-perm-label">Read emails</span>
        <span class="mail-perm-detail">List inbox, read messages, search</span>
      </label>
      <label class="mail-perm-toggle">
        <input type="checkbox" id="ch-field-perm-send" checked>
        <span class="mail-perm-label">Send emails</span>
        <span class="mail-perm-detail">Compose and send on your behalf</span>
      </label>
      <label class="mail-perm-toggle">
        <input type="checkbox" id="ch-field-perm-delete">
        <span class="mail-perm-label">Delete emails</span>
        <span class="mail-perm-detail">Move to trash, permanently delete</span>
      </label>
      <label class="mail-perm-toggle">
        <input type="checkbox" id="ch-field-perm-manage">
        <span class="mail-perm-label">Manage folders</span>
        <span class="mail-perm-detail">Create folders, move messages, flag</span>
      </label>
    </div>

    <div class="mail-security-info">
      <div class="mail-security-header">
        <svg class="icon-sm" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>
        How your credentials are stored &amp; used
      </div>
      <ul class="mail-security-list">
        <li><strong>OS keychain</strong> — your password is stored in the system keychain (macOS Keychain / libsecret on Linux), not in any file. The TOML config at <code>~/.config/himalaya/</code> only contains a reference.</li>
        <li><strong>Never sent to frontend</strong> — credential details are redacted before reaching the UI. The raw password stays in the Rust process and OS keychain only.</li>
        <li><strong>TLS in transit</strong> — connections to ${provider.imap || 'your mail server'} use TLS encryption</li>
        <li><strong>No cloud</strong> — Paw and OpenClaw are fully self-hosted; no third-party server ever sees your password</li>
        <li><strong>Permission-gated</strong> — the agent must pass your Credential Vault permissions before using email tools. Disabling a permission auto-blocks the tool.</li>
        <li><strong>Activity log</strong> — every agent email action (and every block) is recorded in a local SQLite audit log you can review in the Credential Vault</li>
        <li><strong>Revocable</strong> — ${needsAppPw ? "revoke the app password in your provider's security settings at any time to instantly cut off access" : 'change your password to instantly revoke access'}</li>
      </ul>
      <div class="mail-security-limitations">
        <strong>Note:</strong> For maximum security, use an App Password with limited scope instead of your main password. The OS keychain protects credentials at rest using your system's native encryption.
      </div>
    </div>
  `;

  // Wire back button
  $('mail-setup-back')?.addEventListener('click', () => openMailAccountSetup());
}

// Save handler for mail IMAP setup — called from saveChannelSetup
async function saveMailImapSetup() {
  const email = ($('ch-field-mail-email') as HTMLInputElement)?.value.trim();
  const password = ($('ch-field-mail-password') as HTMLInputElement)?.value.trim();
  const displayName = ($('ch-field-mail-display') as HTMLInputElement)?.value.trim();
  const imapHost = ($('ch-field-mail-imap') as HTMLInputElement)?.value.trim();
  const imapPort = parseInt(($('ch-field-mail-imap-port') as HTMLInputElement)?.value ?? '993', 10);
  const smtpHost = ($('ch-field-mail-smtp') as HTMLInputElement)?.value.trim();
  const smtpPort = parseInt(($('ch-field-mail-smtp-port') as HTMLInputElement)?.value ?? '465', 10);

  if (!email) { showToast('Email address is required', 'error'); return; }
  if (!password) { showToast('Password is required', 'error'); return; }
  if (!imapHost) { showToast('IMAP server is required', 'error'); return; }
  if (!smtpHost) { showToast('SMTP server is required', 'error'); return; }

  const saveBtn = $('channel-setup-save') as HTMLButtonElement | null;
  if (saveBtn) { saveBtn.disabled = true; saveBtn.textContent = 'Connecting...'; }

  try {
    // Derive a safe account name from the email
    const accountName = email.replace(/[^a-zA-Z0-9]/g, '-').toLowerCase();

    if (invoke) {
      await invoke('write_himalaya_config', {
        accountName,
        email,
        displayName: displayName || null,
        imapHost,
        imapPort,
        smtpHost,
        smtpPort,
        password,
      });
    } else {
      throw new Error('Tauri runtime not available — cannot write config');
    }

    // Save permissions alongside credentials
    const perms = {
      read: ($('ch-field-perm-read') as HTMLInputElement)?.checked ?? true,
      send: ($('ch-field-perm-send') as HTMLInputElement)?.checked ?? true,
      delete: ($('ch-field-perm-delete') as HTMLInputElement)?.checked ?? false,
      manage: ($('ch-field-perm-manage') as HTMLInputElement)?.checked ?? false,
    };
    saveMailPermissions(accountName, perms);

    // Log account creation in activity log
    const permList = [perms.read && 'read', perms.send && 'send', perms.delete && 'delete', perms.manage && 'manage'].filter(Boolean).join(', ');
    logCredentialActivity({
      accountName,
      action: 'approved',
      detail: `Account connected: ${email} (permissions: ${permList})`,
    });

    showToast(`${email} connected! Your agent can now read and send emails.`, 'success');
    closeChannelSetup();

    // Reload mail view to show the new account
    setTimeout(() => loadMail(), 500);
  } catch (e) {
    showToast(`Failed to connect: ${e instanceof Error ? e.message : e}`, 'error');
  } finally {
    if (saveBtn) { saveBtn.disabled = false; saveBtn.textContent = 'Connect Account'; }
  }
}

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

// ── Skills — Plugin Manager ────────────────────────────────────────────────
async function loadSkills() {
  const installed = $('skills-installed-list');
  const available = $('skills-available-list');
  const availableSection = $('skills-available-section');
  const empty = $('skills-empty');
  const loading = $('skills-loading');
  if (!wsConnected) return;

  if (loading) loading.style.display = '';
  if (empty) empty.style.display = 'none';
  if (installed) installed.innerHTML = '';
  if (available) available.innerHTML = '';
  if (availableSection) availableSection.style.display = 'none';

  try {
    const result = await gateway.skillsStatus();
    if (loading) loading.style.display = 'none';

    const skills = result.skills ?? [];
    if (!skills.length) {
      if (empty) empty.style.display = 'flex';
      return;
    }

    for (const skill of skills) {
      const card = document.createElement('div');
      card.className = 'skill-card';

      // Server returns disabled/eligible/missing/install — not installed/enabled
      const isEnabled = !skill.disabled;
      const hasMissingBins = (skill.missing?.bins?.length ?? 0) > 0
        || (skill.missing?.anyBins?.length ?? 0) > 0
        || (skill.missing?.os?.length ?? 0) > 0;
      const hasMissingEnv = (skill.missing?.env?.length ?? 0) > 0;
      const hasMissingConfig = (skill.missing?.config?.length ?? 0) > 0;
      const isInstalled = skill.always || (!hasMissingBins && !hasMissingEnv && !hasMissingConfig);
      const needsSetup = !hasMissingBins && (hasMissingEnv || hasMissingConfig);
      const hasEnvRequirements = (skill.requirements?.env?.length ?? 0) > 0;
      const installOptions = skill.install ?? [];

      if (needsSetup) card.className += ' needs-setup';

      const enabledClass = isEnabled ? 'enabled' : '';
      const statusLabel = isInstalled
        ? (isEnabled ? 'Enabled' : 'Disabled')
        : needsSetup ? 'Needs Setup' : 'Available';
      const statusClass = isInstalled
        ? (isEnabled ? 'connected' : 'muted')
        : needsSetup ? 'warning' : 'muted';

      // For the Install button, use the first install spec's ID and label
      const installSpecId = installOptions[0]?.id ?? '';
      const installLabel = installOptions[0]?.label ?? 'Install';

      // Encode skill data for config modal
      const skillDataAttr = escAttr(JSON.stringify({
        name: skill.name,
        skillKey: skill.skillKey ?? skill.name,
        description: skill.description ?? '',
        primaryEnv: skill.primaryEnv,
        requiredEnv: skill.requirements?.env ?? [],
        missingEnv: skill.missing?.env ?? [],
        homepage: skill.homepage,
      }));

      card.innerHTML = `
        <div class="skill-card-header">
          <span class="skill-card-name">${skill.emoji ? escHtml(skill.emoji) + ' ' : ''}${escHtml(skill.name)}</span>
          <span class="status-badge ${statusClass}">${statusLabel}</span>
        </div>
        <div class="skill-card-desc">${escHtml(skill.description ?? '')}</div>
        ${needsSetup ? `<div class="skill-config-missing">Needs API key${(skill.missing?.env?.length ?? 0) > 1 ? 's' : ''}: ${escHtml((skill.missing?.env ?? []).join(', '))}</div>` : ''}
        <div class="skill-card-footer">
          <div style="display:flex;align-items:center;gap:8px">
            ${skill.homepage ? `<a class="skill-card-link" href="${escAttr(skill.homepage)}" target="_blank">docs ↗</a>` : ''}
          </div>
          <div class="skill-card-actions">
            ${isInstalled ? `
              ${hasEnvRequirements ? `<button class="btn btn-ghost btn-sm skill-configure" data-skill='${skillDataAttr}' title="Configure">Configure</button>` : ''}
              <button class="skill-toggle ${enabledClass}" data-skill-key="${escAttr(skill.skillKey ?? skill.name)}" data-name="${escAttr(skill.name)}" data-enabled="${isEnabled}" title="${isEnabled ? 'Disable' : 'Enable'}"></button>
            ` : needsSetup ? `
              <button class="btn btn-primary btn-sm skill-configure" data-skill='${skillDataAttr}'>Setup</button>
            ` : installOptions.length > 0 ? `
              <button class="btn btn-primary btn-sm skill-install" data-name="${escAttr(skill.name)}" data-install-id="${escAttr(installSpecId)}">${escHtml(installLabel)}</button>
            ` : `
              <span class="status-badge muted">No installer</span>
            `}
          </div>
        </div>
      `;
      if (isInstalled) {
        installed?.appendChild(card);
      } else {
        if (availableSection) availableSection.style.display = '';
        available?.appendChild(card);
      }
    }

    wireSkillActions();
  } catch (e) {
    console.warn('Skills load failed:', e);
    if (loading) loading.style.display = 'none';
    if (empty) empty.style.display = 'flex';
    showSkillsToast(`Failed to load skills: ${e}`, 'error');
  }
}

function wireSkillActions() {
  // Install buttons
  document.querySelectorAll('.skill-install').forEach(btn => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const name = (btn as HTMLElement).dataset.name!;
      const installId = (btn as HTMLElement).dataset.installId!;
      if (!installId) {
        showSkillsToast(`No installer available for ${name}`, 'error');
        return;
      }
      (btn as HTMLButtonElement).disabled = true;
      (btn as HTMLButtonElement).textContent = 'Installing…';
      showSkillsToast(`Installing ${name}…`, 'info');
      try {
        await gateway.skillsInstall(name, installId);
        showSkillsToast(`${name} installed successfully!`, 'success');
        await loadSkills();
      } catch (e) {
        showSkillsToast(`Install failed for ${name}: ${e}`, 'error');
        (btn as HTMLButtonElement).disabled = false;
        (btn as HTMLButtonElement).textContent = 'Install';
      }
    });
  });

  // Enable/disable toggles
  document.querySelectorAll('.skill-toggle').forEach(btn => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const skillKey = (btn as HTMLElement).dataset.skillKey!;
      const name = (btn as HTMLElement).dataset.name ?? skillKey;
      const currentlyEnabled = (btn as HTMLElement).dataset.enabled === 'true';
      const newState = !currentlyEnabled;

      (btn as HTMLButtonElement).disabled = true;
      try {
        await gateway.skillsUpdate(skillKey, { enabled: newState });
        showSkillsToast(`${name} ${newState ? 'enabled' : 'disabled'}`, 'success');
        await loadSkills();
      } catch (e) {
        showSkillsToast(`Failed to ${newState ? 'enable' : 'disable'} ${name}: ${e}`, 'error');
        (btn as HTMLButtonElement).disabled = false;
      }
    });
  });

  // Configure / Setup buttons
  document.querySelectorAll('.skill-configure').forEach(btn => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const raw = (btn as HTMLElement).dataset.skill;
      if (!raw) return;
      try {
        const data = JSON.parse(raw);
        openSkillConfigModal(data);
      } catch { /* ignore parse errors */ }
    });
  });
}

let _skillsToastTimer: number | null = null;
function showSkillsToast(message: string, type: 'success' | 'error' | 'info') {
  const toast = $('skills-toast');
  if (!toast) return;
  toast.className = `skills-toast ${type}`;
  toast.textContent = message;
  toast.style.display = 'flex';

  if (_skillsToastTimer) clearTimeout(_skillsToastTimer);
  _skillsToastTimer = window.setTimeout(() => {
    toast.style.display = 'none';
    _skillsToastTimer = null;
  }, type === 'error' ? 8000 : 4000);
}

// ── Skill config modal ─────────────────────────────────────────────────────
interface SkillConfigData {
  name: string;
  skillKey: string;
  description: string;
  primaryEnv?: string;
  requiredEnv: string[];
  missingEnv: string[];
  homepage?: string;
}

let _activeSkillConfig: SkillConfigData | null = null;

function openSkillConfigModal(data: SkillConfigData) {
  const modal = $('skill-config-modal');
  const title = $('skill-config-title');
  const desc = $('skill-config-desc');
  const fields = $('skill-config-fields');
  if (!modal || !fields) return;

  _activeSkillConfig = data;

  if (title) title.textContent = `Configure ${data.name}`;
  if (desc) {
    const parts: string[] = [];
    if (data.description) parts.push(data.description);
    if (data.homepage) parts.push(`<a href="${escAttr(data.homepage)}" target="_blank" style="color:var(--accent)">View docs ↗</a>`);
    desc.innerHTML = parts.join(' — ');
  }

  // Build one input field per required env var
  const envVars = data.requiredEnv.length > 0 ? data.requiredEnv : (data.primaryEnv ? [data.primaryEnv] : []);
  fields.innerHTML = envVars.map(envName => {
    const isMissing = data.missingEnv.includes(envName);
    const isPrimary = envName === data.primaryEnv;
    return `
      <div class="skill-config-field">
        <label for="skill-env-${escAttr(envName)}">${escHtml(envName)}${isMissing ? ' <span style="color:var(--warning,#E8A317)">(not set)</span>' : ' <span style="color:var(--success)">✓</span>'}</label>
        <input type="password" id="skill-env-${escAttr(envName)}" class="form-input"
          data-env-name="${escAttr(envName)}"
          data-is-primary="${isPrimary}"
          placeholder="${isPrimary ? 'Enter your API key' : `Enter value for ${envName}`}"
          autocomplete="off" spellcheck="false">
        <div class="field-hint">${isPrimary ? 'This is the main API key for this skill.' : 'Required environment variable.'} Leave blank to keep current value.</div>
      </div>
    `;
  }).join('');

  modal.style.display = 'flex';
}

function closeSkillConfigModal() {
  const modal = $('skill-config-modal');
  if (modal) modal.style.display = 'none';
  _activeSkillConfig = null;
}

async function saveSkillConfig() {
  if (!_activeSkillConfig) return;
  const fields = $('skill-config-fields');
  if (!fields) return;

  const data = _activeSkillConfig;
  const inputs = fields.querySelectorAll<HTMLInputElement>('input[data-env-name]');

  // Collect values
  const env: Record<string, string> = {};
  let apiKey: string | undefined;

  inputs.forEach(input => {
    const envName = input.dataset.envName!;
    const value = input.value.trim();
    if (!value) return; // skip blank = keep current

    if (input.dataset.isPrimary === 'true') {
      apiKey = value;
    } else {
      env[envName] = value;
    }
  });

  // Nothing entered
  if (!apiKey && Object.keys(env).length === 0) {
    showSkillsToast('No values entered — nothing to save', 'info');
    return;
  }

  const saveBtn = $('skill-config-save') as HTMLButtonElement | null;
  if (saveBtn) { saveBtn.disabled = true; saveBtn.textContent = 'Saving…'; }

  try {
    const updates: { enabled?: boolean; apiKey?: string; env?: Record<string, string> } = {};
    if (apiKey) updates.apiKey = apiKey;
    if (Object.keys(env).length > 0) updates.env = env;

    await gateway.skillsUpdate(data.skillKey, updates);
    showSkillsToast(`${data.name} configured successfully!`, 'success');
    closeSkillConfigModal();
    await loadSkills();
  } catch (e) {
    showSkillsToast(`Failed to configure ${data.name}: ${e}`, 'error');
  } finally {
    if (saveBtn) { saveBtn.disabled = false; saveBtn.textContent = 'Save'; }
  }
}

$('skill-config-close')?.addEventListener('click', closeSkillConfigModal);
$('skill-config-cancel')?.addEventListener('click', closeSkillConfigModal);
$('skill-config-save')?.addEventListener('click', saveSkillConfig);
$('skill-config-modal')?.addEventListener('click', (e) => {
  if (e.target === e.currentTarget) closeSkillConfigModal();
});

$('refresh-skills-btn')?.addEventListener('click', () => loadSkills());

// Bins modal
$('skills-browse-bins')?.addEventListener('click', async () => {
  const backdrop = $('bins-modal-backdrop');
  const list = $('bins-list');
  const loading = $('bins-loading');
  const empty = $('bins-empty');
  if (!backdrop || !list) return;

  backdrop.style.display = 'flex';
  list.innerHTML = '';
  if (loading) loading.style.display = '';
  if (empty) empty.style.display = 'none';

  try {
    const result = await gateway.skillsBins();
    if (loading) loading.style.display = 'none';
    const bins = result.bins ?? [];
    if (!bins.length) {
      if (empty) empty.style.display = '';
      return;
    }

    for (const bin of bins) {
      const item = document.createElement('div');
      item.className = 'bins-item';
      item.innerHTML = `
        <span class="bins-item-name">${escHtml(bin)}</span>
        <button class="btn btn-primary btn-sm bins-item-install" data-name="${escAttr(bin)}">Install</button>
      `;
      list.appendChild(item);
    }

    // Wire bin install buttons
    list.querySelectorAll('.bins-item-install').forEach(btn => {
      btn.addEventListener('click', async () => {
        const name = (btn as HTMLElement).dataset.name!;
        (btn as HTMLButtonElement).disabled = true;
        (btn as HTMLButtonElement).textContent = 'Installing…';
        try {
          await gateway.skillsInstall(name, crypto.randomUUID());
          (btn as HTMLButtonElement).textContent = 'Installed';
          showSkillsToast(`${name} installed!`, 'success');
          loadSkills();
        } catch (e) {
          (btn as HTMLButtonElement).textContent = 'Failed';
          showSkillsToast(`Install failed: ${e}`, 'error');
          setTimeout(() => {
            (btn as HTMLButtonElement).textContent = 'Install';
            (btn as HTMLButtonElement).disabled = false;
          }, 2000);
        }
      });
    });
  } catch (e) {
    if (loading) loading.style.display = 'none';
    if (empty) { empty.style.display = ''; empty.textContent = `Failed to load bins: ${e}`; }
  }
});

$('bins-modal-close')?.addEventListener('click', () => {
  const backdrop = $('bins-modal-backdrop');
  if (backdrop) backdrop.style.display = 'none';
});

$('bins-modal-backdrop')?.addEventListener('click', (e) => {
  if (e.target === e.currentTarget) {
    (e.target as HTMLElement).style.display = 'none';
  }
});

$('bins-custom-install')?.addEventListener('click', async () => {
  const input = $('bins-custom-name') as HTMLInputElement | null;
  const btn = $('bins-custom-install') as HTMLButtonElement | null;
  if (!input || !btn) return;

  const name = input.value.trim();
  if (!name) { input.focus(); return; }

  btn.disabled = true;
  btn.textContent = 'Installing…';

  try {
    await gateway.skillsInstall(name, crypto.randomUUID());
    showSkillsToast(`${name} installed!`, 'success');
    input.value = '';
    loadSkills();
    // Close modal
    const backdrop = $('bins-modal-backdrop');
    if (backdrop) backdrop.style.display = 'none';
  } catch (e) {
    showSkillsToast(`Install failed for "${name}": ${e}`, 'error');
  } finally {
    btn.disabled = false;
    btn.textContent = 'Install';
  }
});

// ── Models / Foundry — Models + Agent Modes ────────────────────────────────
let _cachedModels: { id: string; name?: string; provider?: string; contextWindow?: number; reasoning?: boolean }[] = [];

async function loadModels() {
  const list = $('models-list');
  const empty = $('models-empty');
  const loading = $('models-loading');
  if (!wsConnected || !list) return;

  if (loading) loading.style.display = '';
  if (empty) empty.style.display = 'none';
  list.innerHTML = '';

  try {
    const result = await gateway.modelsList();
    if (loading) loading.style.display = 'none';

    const models = result.models ?? [];
    _cachedModels = models;
    if (!models.length) {
      if (empty) empty.style.display = 'flex';
      return;
    }

    for (const model of models) {
      const card = document.createElement('div');
      card.className = 'model-card';
      card.innerHTML = `
        <div class="model-card-header">
          <span class="model-card-name">${escHtml(model.name ?? model.id)}</span>
          ${model.provider ? `<span class="model-card-provider">${escHtml(model.provider)}</span>` : ''}
        </div>
        <div class="model-card-meta">
          ${model.contextWindow ? `<span>${model.contextWindow.toLocaleString()} tokens</span>` : ''}
          ${model.reasoning ? `<span class="model-card-badge">Reasoning</span>` : ''}
        </div>
      `;
      list.appendChild(card);
    }
  } catch (e) {
    console.warn('Models load failed:', e);
    if (loading) loading.style.display = 'none';
    if (empty) empty.style.display = 'flex';
  }
}
$('refresh-models-btn')?.addEventListener('click', () => { loadModels(); loadModes(); });

// Foundry tab switching
document.querySelectorAll('.foundry-tab').forEach(tab => {
  tab.addEventListener('click', () => {
    document.querySelectorAll('.foundry-tab').forEach(t => t.classList.remove('active'));
    tab.classList.add('active');
    const target = tab.getAttribute('data-foundry-tab');
    const modelsPanel = $('foundry-models-panel');
    const modesPanel = $('foundry-modes-panel');
    const agentsPanel = $('foundry-agents-panel');
    if (modelsPanel) modelsPanel.style.display = target === 'models' ? '' : 'none';
    if (modesPanel) modesPanel.style.display = target === 'modes' ? '' : 'none';
    if (agentsPanel) agentsPanel.style.display = target === 'agents' ? '' : 'none';
    // Auto-load agents when switching to agents tab
    if (target === 'agents') loadAgents();
  });
});

// ── Agent Modes ────────────────────────────────────────────────────────────
let _editingModeId: string | null = null;

async function loadModes() {
  const list = $('modes-list');
  const empty = $('modes-empty');
  if (!list) return;
  list.innerHTML = '';

  try {
    const modes = await listModes();
    if (!modes.length) {
      if (empty) empty.style.display = '';
      return;
    }
    if (empty) empty.style.display = 'none';

    for (const mode of modes) {
      const card = document.createElement('div');
      card.className = 'mode-card';
      card.style.borderLeftColor = mode.color || 'var(--accent)';
      card.innerHTML = `
        <div class="mode-card-icon" style="background:${mode.color}22">${mode.icon || mode.name?.charAt(0) || 'M'}</div>
        <div class="mode-card-info">
          <div class="mode-card-name">${escHtml(mode.name)}</div>
          <div class="mode-card-detail">${mode.model ? escHtml(mode.model) : 'Default model'} · ${mode.thinking_level || 'normal'} thinking</div>
        </div>
        ${mode.is_default ? '<span class="mode-card-default">Default</span>' : ''}
      `;
      card.addEventListener('click', () => editMode(mode));
      list.appendChild(card);
    }
  } catch (e) {
    console.warn('Modes load failed:', e);
  }
}

function editMode(mode?: AgentMode) {
  _editingModeId = mode?.id ?? null;
  const modal = $('mode-modal');
  const title = $('mode-modal-title');
  const deleteBtn = $('mode-modal-delete');
  if (!modal) return;
  modal.style.display = 'flex';
  if (title) title.textContent = mode ? 'Edit Agent Mode' : 'New Agent Mode';
  if (deleteBtn) deleteBtn.style.display = mode && !mode.is_default ? '' : 'none';

  // Populate model select
  const modelSelect = $('mode-form-model') as HTMLSelectElement;
  if (modelSelect) {
    modelSelect.innerHTML = '<option value="">Default model</option>';
    for (const m of _cachedModels) {
      const opt = document.createElement('option');
      opt.value = m.id;
      opt.textContent = m.name ?? m.id;
      if (mode?.model === m.id) opt.selected = true;
      modelSelect.appendChild(opt);
    }
  }

  // Fill form
  ($('mode-form-icon') as HTMLInputElement).value = mode?.icon ?? '';
  ($('mode-form-name') as HTMLInputElement).value = mode?.name ?? '';
  ($('mode-form-color') as HTMLInputElement).value = mode?.color ?? '#0073EA';
  ($('mode-form-prompt') as HTMLTextAreaElement).value = mode?.system_prompt ?? '';
  ($('mode-form-thinking') as HTMLSelectElement).value = mode?.thinking_level ?? 'normal';
  ($('mode-form-temp') as HTMLInputElement).value = String(mode?.temperature ?? 1);
  const tempVal = $('mode-form-temp-value');
  if (tempVal) tempVal.textContent = String(mode?.temperature ?? 1.0);
}

function hideModeModal() {
  const modal = $('mode-modal');
  if (modal) modal.style.display = 'none';
  _editingModeId = null;
}

$('modes-add-btn')?.addEventListener('click', () => editMode());
$('mode-modal-close')?.addEventListener('click', hideModeModal);
$('mode-modal-cancel')?.addEventListener('click', hideModeModal);

$('mode-form-temp')?.addEventListener('input', () => {
  const val = ($('mode-form-temp') as HTMLInputElement).value;
  const display = $('mode-form-temp-value');
  if (display) display.textContent = parseFloat(val).toFixed(1);
});

$('mode-modal-save')?.addEventListener('click', async () => {
  const name = ($('mode-form-name') as HTMLInputElement).value.trim();
  if (!name) { alert('Name is required'); return; }
  const id = _editingModeId ?? name.toLowerCase().replace(/\s+/g, '-').replace(/[^a-z0-9-]/g, '');
  await saveMode({
    id,
    name,
    icon: ($('mode-form-icon') as HTMLInputElement).value || '',
    color: ($('mode-form-color') as HTMLInputElement).value || '#0073EA',
    model: ($('mode-form-model') as HTMLSelectElement).value || null,
    system_prompt: ($('mode-form-prompt') as HTMLTextAreaElement).value,
    thinking_level: ($('mode-form-thinking') as HTMLSelectElement).value,
    temperature: parseFloat(($('mode-form-temp') as HTMLInputElement).value),
  });
  hideModeModal();
  loadModes();
});

$('mode-modal-delete')?.addEventListener('click', async () => {
  if (!_editingModeId || !confirm('Delete this mode?')) return;
  await deleteMode(_editingModeId);
  hideModeModal();
  loadModes();
});

// ══════════════════════════════════════════════════════════════════════════
// ═══ AGENTS — Multi-Agent Persona Management ═══════════════════════════════
// ══════════════════════════════════════════════════════════════════════════

let _agentsList: import('./types').AgentSummary[] = [];
let _currentAgentId: string | null = null;
let _editingAgentId: string | null = null;

/** Standard workspace files that OpenClaw agents use */
const AGENT_STANDARD_FILES: { name: string; label: string; description: string }[] = [
  { name: 'AGENTS.md',    label: 'Instructions',   description: 'Operating rules, priorities, memory usage guide' },
  { name: 'SOUL.md',      label: 'Persona',         description: 'Personality, tone, communication style, boundaries' },
  { name: 'USER.md',      label: 'About User',      description: 'Who the user is, how to address them, preferences' },
  { name: 'IDENTITY.md',  label: 'Identity',         description: 'Agent name, emoji, vibe/creature, avatar' },
  { name: 'TOOLS.md',     label: 'Tool Notes',       description: 'Notes about local tools and conventions' },
  { name: 'HEARTBEAT.md', label: 'Heartbeat',        description: 'Optional cron checklist (keep short to save tokens)' },
];

async function loadAgents() {
  const list = $('agents-list');
  const empty = $('agents-empty');
  const loading = $('agents-loading');
  if (!wsConnected || !list) return;

  if (loading) loading.style.display = '';
  if (empty) empty.style.display = 'none';
  list.innerHTML = '';

  try {
    const result = await gateway.listAgents();
    if (loading) loading.style.display = 'none';

    _agentsList = result.agents ?? [];
    if (!_agentsList.length) {
      if (empty) empty.style.display = 'flex';
      return;
    }

    const defaultId = result.defaultId;

    for (const agent of _agentsList) {
      const card = document.createElement('div');
      card.className = 'agent-card';
      const isDefault = agent.id === defaultId;
      const emoji = agent.identity?.emoji ?? agent.name?.charAt(0)?.toUpperCase() ?? 'A';
      const name = agent.identity?.name ?? agent.name ?? agent.id;
      const theme = agent.identity?.theme ?? '';
      card.innerHTML = `
        <div class="agent-card-avatar">${escHtml(emoji)}</div>
        <div class="agent-card-body">
          <div class="agent-card-name">${escHtml(name)}${isDefault ? ' <span class="agent-card-badge">Default</span>' : ''}</div>
          <div class="agent-card-id">${escHtml(agent.id)}</div>
          ${theme ? `<div class="agent-card-theme">${escHtml(theme)}</div>` : ''}
        </div>
        <svg class="agent-card-chevron" viewBox="0 0 24 24"><polyline points="9 18 15 12 9 6"/></svg>
      `;
      card.addEventListener('click', () => openAgentDetail(agent.id));
      list.appendChild(card);
    }
  } catch (e) {
    console.warn('Agents load failed:', e);
    if (loading) loading.style.display = 'none';
    if (empty) empty.style.display = 'flex';
  }
}

// ── Agent Detail View ──────────────────────────────────────────────────────
async function openAgentDetail(agentId: string) {
  _currentAgentId = agentId;
  const listView = $('agents-list-view');
  const detailView = $('agent-detail-view');
  if (listView) listView.style.display = 'none';
  if (detailView) detailView.style.display = '';

  // Populate header from cached agent list
  const agent = _agentsList.find(a => a.id === agentId);
  const emojiEl = $('agent-detail-emoji');
  const nameEl = $('agent-detail-name');
  const idEl = $('agent-detail-id');
  const deleteBtn = $('agent-detail-delete');
  if (emojiEl) emojiEl.textContent = agent?.identity?.emoji ?? agent?.name?.charAt(0)?.toUpperCase() ?? 'A';
  if (nameEl) nameEl.textContent = agent?.identity?.name ?? agent?.name ?? agentId;
  if (idEl) idEl.textContent = agentId;
  // Don't allow deleting the "main" agent
  if (deleteBtn) deleteBtn.style.display = agentId === 'main' ? 'none' : '';

  // Load agent files
  await loadAgentFiles(agentId);
}

function closeAgentDetail() {
  _currentAgentId = null;
  const listView = $('agents-list-view');
  const detailView = $('agent-detail-view');
  const editor = $('agent-file-editor');
  if (listView) listView.style.display = '';
  if (detailView) detailView.style.display = 'none';
  if (editor) editor.style.display = 'none';
}

$('agent-detail-back')?.addEventListener('click', closeAgentDetail);

// ── Agent Files ────────────────────────────────────────────────────────────
async function loadAgentFiles(agentId: string) {
  const grid = $('agent-files-list');
  const workspaceEl = $('agent-detail-workspace');
  if (!grid) return;
  grid.innerHTML = '<div class="view-loading">Loading files…</div>';

  try {
    const result = await gateway.agentFilesList(agentId);
    if (workspaceEl) workspaceEl.textContent = result.workspace || '—';

    const files = result.files ?? [];
    grid.innerHTML = '';

    // Render standard files first (even if they don't exist yet — show them as "create" cards)
    const existingPaths = new Set(files.map(f => f.path ?? f.name ?? ''));
    for (const sf of AGENT_STANDARD_FILES) {
      const exists = existingPaths.has(sf.name);
      const file = files.find(f => (f.path ?? f.name) === sf.name);
      const card = document.createElement('div');
      card.className = `agent-file-card ${exists ? '' : 'agent-file-card-new'}`;
      card.innerHTML = `
        <div class="agent-file-card-icon">${exists ? 'F' : '+'}</div>
        <div class="agent-file-card-body">
          <div class="agent-file-card-name">${escHtml(sf.name)}</div>
          <div class="agent-file-card-desc">${escHtml(sf.label)} — ${escHtml(sf.description)}</div>
          ${exists && file?.sizeBytes ? `<div class="agent-file-card-size">${formatBytes(file.sizeBytes)}</div>` : ''}
        </div>
      `;
      card.addEventListener('click', () => openAgentFileEditor(agentId, sf.name, exists));
      grid.appendChild(card);
    }

    // Render any extra files not in the standard list
    for (const file of files) {
      const path = file.path ?? file.name ?? '';
      if (AGENT_STANDARD_FILES.some(sf => sf.name === path)) continue;
      const card = document.createElement('div');
      card.className = 'agent-file-card';
      card.innerHTML = `
        <div class="agent-file-card-icon">F</div>
        <div class="agent-file-card-body">
          <div class="agent-file-card-name">${escHtml(path)}</div>
          ${file.sizeBytes ? `<div class="agent-file-card-size">${formatBytes(file.sizeBytes)}</div>` : ''}
        </div>
      `;
      card.addEventListener('click', () => openAgentFileEditor(agentId, path, true));
      grid.appendChild(card);
    }
  } catch (e) {
    console.warn('Agent files load failed:', e);
    grid.innerHTML = '<div class="empty-state"><div class="empty-title">Could not load files</div></div>';
  }
}

async function openAgentFileEditor(agentId: string, filePath: string, exists: boolean) {
  const editor = $('agent-file-editor');
  const pathEl = $('agent-file-editor-path');
  const content = $('agent-file-editor-content') as HTMLTextAreaElement | null;
  if (!editor || !content) return;

  editor.style.display = '';
  if (pathEl) pathEl.textContent = filePath;
  content.value = exists ? 'Loading…' : '';
  content.disabled = exists;
  content.dataset.agentId = agentId;
  content.dataset.filePath = filePath;

  if (exists) {
    try {
      const result = await gateway.agentFilesGet(filePath, agentId);
      content.value = result.content ?? '';
      content.disabled = false;
    } catch (e) {
      content.value = `Error loading file: ${e}`;
      content.disabled = false;
    }
  } else {
    // New file — pre-populate with a template for standard files
    const standard = AGENT_STANDARD_FILES.find(sf => sf.name === filePath);
    if (standard) {
      content.value = getAgentFileTemplate(filePath, agentId);
    }
    content.disabled = false;
  }

  // Scroll editor into view
  editor.scrollIntoView({ behavior: 'smooth', block: 'start' });
}

function getAgentFileTemplate(fileName: string, agentId: string): string {
  const templates: Record<string, string> = {
    'AGENTS.md': `# ${agentId} — Operating Instructions\n\n## Priorities\n1. Be helpful and accurate\n2. Use memory to remember context across sessions\n3. Follow the user's preferences defined in USER.md\n\n## Rules\n- Always check memory before answering questions about past conversations\n- Be concise unless asked for detail\n- Ask clarifying questions when intent is ambiguous\n`,
    'SOUL.md': `# ${agentId} — Persona\n\n## Personality\n- Friendly and professional\n- Direct and clear in communication\n- Proactive — anticipates needs\n\n## Tone\n- Warm but not overly casual\n- Confident without being arrogant\n\n## Boundaries\n- Always be honest about limitations\n- Never fabricate information\n`,
    'USER.md': `# About the User\n\n## How to address them\n- Use their first name\n\n## Preferences\n- Prefers concise responses\n- Likes code examples over lengthy explanations\n`,
    'IDENTITY.md': `# IDENTITY.md - Agent Identity\n\n- Name: ${agentId}\n- Creature: helpful assistant\n- Vibe: warm and capable\n`,
    'TOOLS.md': `# ${agentId} — Tool Notes\n\n## Available Tools\nThis agent has access to the standard OpenClaw tool set.\n\n## Conventions\n- Use the file system for persistent work\n- Use memory_store for important facts to remember\n`,
    'HEARTBEAT.md': `# ${agentId} — Heartbeat Checklist\n\n- [ ] Check for pending tasks\n- [ ] Review recent messages\n`,
  };
  return templates[fileName] ?? `# ${fileName}\n\n`;
}

$('agent-file-editor-save')?.addEventListener('click', async () => {
  const content = $('agent-file-editor-content') as HTMLTextAreaElement | null;
  if (!content?.dataset.filePath || !content?.dataset.agentId) return;
  try {
    await gateway.agentFilesSet(content.dataset.filePath, content.value, content.dataset.agentId);
    showToast('File saved', 'success');
    // Reload file list to reflect new file / size changes
    loadAgentFiles(content.dataset.agentId);
  } catch (e) {
    showToast(`Save failed: ${e instanceof Error ? e.message : e}`, 'error');
  }
});

$('agent-file-editor-close')?.addEventListener('click', () => {
  const editor = $('agent-file-editor');
  if (editor) editor.style.display = 'none';
});

$('agent-files-refresh')?.addEventListener('click', () => {
  if (_currentAgentId) loadAgentFiles(_currentAgentId);
});

// New custom file
$('agent-files-new')?.addEventListener('click', async () => {
  if (!_currentAgentId) return;
  const name = await promptModal('New File', 'File name (e.g. PROJECTS.md)…');
  if (!name) return;
  const fileName = name.endsWith('.md') ? name : name + '.md';
  openAgentFileEditor(_currentAgentId, fileName, false);
});

// ── Agent Create Modal ─────────────────────────────────────────────────────
function showAgentModal(agent?: import('./types').AgentSummary) {
  _editingAgentId = agent?.id ?? null;
  const modal = $('agent-modal');
  const title = $('agent-modal-title');
  const saveBtn = $('agent-modal-save');
  if (!modal) return;
  modal.style.display = 'flex';
  if (title) title.textContent = agent ? 'Edit Agent' : 'New Agent';
  if (saveBtn) saveBtn.textContent = agent ? 'Save Changes' : 'Create Agent';

  // Populate model select
  const modelSelect = $('agent-form-model') as HTMLSelectElement;
  if (modelSelect) {
    modelSelect.innerHTML = '<option value="">Default model</option>';
    for (const m of _cachedModels) {
      const opt = document.createElement('option');
      opt.value = m.id;
      opt.textContent = m.name ?? m.id;
      modelSelect.appendChild(opt);
    }
  }

  // Fill fields
  ($('agent-form-emoji') as HTMLInputElement).value = agent?.identity?.emoji ?? '';
  ($('agent-form-name') as HTMLInputElement).value = agent?.identity?.name ?? agent?.name ?? '';
  ($('agent-form-workspace') as HTMLInputElement).value = '';
}

function hideAgentModal() {
  const modal = $('agent-modal');
  if (modal) modal.style.display = 'none';
  _editingAgentId = null;
}

$('agents-create-btn')?.addEventListener('click', () => showAgentModal());
$('agent-modal-close')?.addEventListener('click', hideAgentModal);
$('agent-modal-cancel')?.addEventListener('click', hideAgentModal);

$('agent-modal-save')?.addEventListener('click', async () => {
  const name = ($('agent-form-name') as HTMLInputElement).value.trim();
  if (!name) { showToast('Name is required', 'error'); return; }
  const emoji = ($('agent-form-emoji') as HTMLInputElement).value || '';
  const workspace = ($('agent-form-workspace') as HTMLInputElement).value.trim() || undefined;
  const model = ($('agent-form-model') as HTMLSelectElement).value || undefined;

  try {
    if (_editingAgentId) {
      // Update existing
      await gateway.updateAgent({ agentId: _editingAgentId, name, workspace, model });
      showToast('Agent updated', 'success');
    } else {
      // Create new
      const result = await gateway.createAgent({ name, workspace, emoji });
      showToast(`Agent "${result.name}" created`, 'success');
      // Open the new agent detail
      hideAgentModal();
      await loadAgents();
      openAgentDetail(result.agentId);
      return;
    }
  } catch (e) {
    showToast(`Failed: ${e instanceof Error ? e.message : e}`, 'error');
    return;
  }
  hideAgentModal();
  loadAgents();
});

// ── Agent Edit / Delete buttons in detail view ─────────────────────────────
$('agent-detail-edit')?.addEventListener('click', () => {
  if (!_currentAgentId) return;
  const agent = _agentsList.find(a => a.id === _currentAgentId);
  showAgentModal(agent);
});

$('agent-detail-delete')?.addEventListener('click', async () => {
  if (!_currentAgentId || _currentAgentId === 'main') return;
  const agent = _agentsList.find(a => a.id === _currentAgentId);
  const name = agent?.identity?.name ?? agent?.name ?? _currentAgentId;
  if (!confirm(`Delete agent "${name}"? This will remove the agent and optionally its workspace files.`)) return;
  const deleteFiles = confirm('Also delete workspace files? (Cancel = keep files)');
  try {
    await gateway.deleteAgent(_currentAgentId, deleteFiles);
    showToast(`Agent "${name}" deleted`, 'success');
    closeAgentDetail();
    loadAgents();
  } catch (e) {
    showToast(`Delete failed: ${e instanceof Error ? e.message : e}`, 'error');
  }
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

// ── Memory (LanceDB) ───────────────────────────────────────────────────────
let _palaceInitialized = false;
let _palaceAvailable = false;
let _palaceSkipped = false;

async function loadMemoryPalace() {
  if (!wsConnected) return;

  // Check if memory-lancedb plugin is active in the gateway
  if (!_palaceInitialized) {
    _palaceInitialized = true;

    // memory-lancedb is a plugin, not a skill, so it won't appear in skillsStatus().
    // Instead, check if the config is written AND the gateway is running — if both
    // are true, the plugin is active (it registers on gateway startup).
    let configWritten = false;
    if (invoke) {
      try {
        configWritten = await invoke<boolean>('check_memory_configured');
      } catch { /* ignore */ }
    }

    if (configWritten) {
      // Config is present — check if gateway is actually running
      try {
        const healthy = invoke ? await invoke<boolean>('check_gateway_health', { port: null }) : false;
        if (healthy) {
          _palaceAvailable = true;
          console.log('[memory] Memory plugin configured and gateway is running');
        } else {
          console.log('[memory] Config written but gateway not running');
        }
      } catch {
        // If health check fails, still try — gateway might be starting up
        _palaceAvailable = false;
      }
    }

    initPalaceTabs();
    initPalaceRecall();
    initPalaceRemember();
    initPalaceGraph();
    initPalaceInstall();

    const banner = $('palace-install-banner');
    const filesDivider = $('palace-files-divider');

    if (!_palaceAvailable && !_palaceSkipped) {
      // Show setup banner
      if (banner) banner.style.display = 'flex';
      if (configWritten) {
        // Config is written but gateway hasn't picked it up or plugin failed
        // Show the form so users can update their settings, plus a restart note
        console.log('[memory] Config written but plugin not active — show form + restart option');
        const progressEl = $('palace-progress-text');
        const progressDiv = $('palace-install-progress');
        if (progressEl && progressDiv) {
          progressDiv.style.display = '';
          progressEl.textContent = 'Memory is configured but not active. Update settings or restart the gateway.';
        }
        // Pre-fill from settings if available
        if (invoke) {
          try {
            const existingUrl = await invoke<string | null>('get_embedding_base_url');
            const existingVersion = await invoke<string | null>('get_azure_api_version');
            const existingProvider = await invoke<string | null>('get_embedding_provider');
            const providerSel = $('palace-provider') as HTMLSelectElement | null;
            if (existingProvider && providerSel) providerSel.value = existingProvider;
            updateProviderFields();
            if (existingProvider === 'azure') {
              const baseUrlInput = $('palace-base-url') as HTMLInputElement | null;
              if (existingUrl && baseUrlInput && !baseUrlInput.value) baseUrlInput.value = existingUrl;
            } else {
              const openaiUrlInput = $('palace-base-url-openai') as HTMLInputElement | null;
              if (existingUrl && openaiUrlInput && !openaiUrlInput.value) openaiUrlInput.value = existingUrl;
            }
            const apiVersionInput = $('palace-api-version') as HTMLInputElement | null;
            if (existingVersion && apiVersionInput && !apiVersionInput.value) {
              apiVersionInput.value = existingVersion;
            }
          } catch { /* ignore */ }
        }
      }
    } else if (!_palaceAvailable && _palaceSkipped) {
      // Skipped — show files mode
      if (banner) banner.style.display = 'none';
      document.querySelectorAll('.palace-tab').forEach(t => t.classList.remove('active'));
      document.querySelectorAll('.palace-panel').forEach(p => (p as HTMLElement).style.display = 'none');
      document.querySelector('.palace-tab[data-palace-tab="files"]')?.classList.add('active');
      const fp = $('palace-files-panel');
      if (fp) fp.style.display = 'flex';
      if (filesDivider) filesDivider.style.display = 'none';
      const memoryListBelow = $('memory-list');
      if (memoryListBelow) memoryListBelow.style.display = 'none';
    } else {
      // Memory is available — full mode
      if (banner) banner.style.display = 'none';
      if (filesDivider) filesDivider.style.display = '';
      // Show settings gear so user can reconfigure endpoint/API key
      const settingsBtn = $('palace-settings');
      if (settingsBtn) settingsBtn.style.display = '';
    }
  }

  // Only load stats + sidebar when memory is actually available
  // (don't call CLI commands when plugin is misconfigured — they can hang)
  if (_palaceAvailable) {
    await loadPalaceStats();
    await loadPalaceSidebar();
  }
}

function updateProviderFields() {
  const sel = $('palace-provider') as HTMLSelectElement | null;
  const isAzure = sel?.value === 'azure';
  const azureFields = $('palace-azure-fields');
  const openaiEndpoint = $('palace-openai-endpoint-field');
  const apiVersionField = $('palace-api-version-field');
  const apiKeyInput = $('palace-api-key') as HTMLInputElement | null;
  const modelLabel = $('palace-model-label');
  const modelInput = $('palace-model-name') as HTMLInputElement | null;

  if (azureFields) azureFields.style.display = isAzure ? '' : 'none';
  if (openaiEndpoint) openaiEndpoint.style.display = isAzure ? 'none' : '';
  if (apiVersionField) apiVersionField.style.display = isAzure ? '' : 'none';
  if (apiKeyInput) apiKeyInput.placeholder = isAzure ? 'Azure API key' : 'sk-...';
  if (modelLabel) modelLabel.innerHTML = isAzure
    ? 'Deployment Name <span class="palace-api-hint">(defaults to text-embedding-3-small)</span>'
    : 'Model <span class="palace-api-hint">(defaults to text-embedding-3-small)</span>';
  if (modelInput) modelInput.placeholder = isAzure
    ? 'text-embedding-3-small' : 'text-embedding-3-small';
}

function getSelectedProvider(): string {
  return (($('palace-provider') as HTMLSelectElement)?.value) || 'openai';
}

function getBaseUrlForProvider(): string {
  const provider = getSelectedProvider();
  if (provider === 'azure') {
    return ($('palace-base-url') as HTMLInputElement)?.value?.trim() ?? '';
  }
  return ($('palace-base-url-openai') as HTMLInputElement)?.value?.trim() ?? '';
}

function initPalaceInstall() {
  // Provider dropdown — toggle fields on change
  $('palace-provider')?.addEventListener('change', updateProviderFields);
  // Set initial state
  updateProviderFields();

  // Settings gear — show the setup banner for reconfiguration
  $('palace-settings')?.addEventListener('click', async () => {
    const banner = $('palace-install-banner');
    if (!banner) return;
    banner.style.display = 'flex';
    // Pre-fill with existing settings
    if (invoke) {
      try {
        const existingUrl = await invoke<string | null>('get_embedding_base_url');
        const existingVersion = await invoke<string | null>('get_azure_api_version');
        const existingProvider = await invoke<string | null>('get_embedding_provider');
        const providerSel = $('palace-provider') as HTMLSelectElement | null;
        if (existingProvider && providerSel) providerSel.value = existingProvider;
        updateProviderFields();
        if (existingProvider === 'azure') {
          const baseUrlInput = $('palace-base-url') as HTMLInputElement | null;
          if (existingUrl && baseUrlInput) baseUrlInput.value = existingUrl;
        } else {
          const openaiUrlInput = $('palace-base-url-openai') as HTMLInputElement | null;
          if (existingUrl && openaiUrlInput) openaiUrlInput.value = existingUrl;
        }
        const apiVersionInput = $('palace-api-version') as HTMLInputElement | null;
        if (existingVersion && apiVersionInput) apiVersionInput.value = existingVersion;
      } catch { /* ignore */ }
    }
    // Update button text to indicate reconfiguration
    const btn = $('palace-install-btn') as HTMLButtonElement | null;
    if (btn) { btn.textContent = 'Save & Restart'; btn.disabled = false; }
    const progressDiv = $('palace-install-progress');
    if (progressDiv) progressDiv.style.display = 'none';
  });

  // ── Shared form reader & validator ──
  function readMemoryForm(): { apiKey: string; baseUrl: string; modelName: string; apiVersion: string; provider: string } | null {
    const apiKeyInput = $('palace-api-key') as HTMLInputElement | null;
    const provider = getSelectedProvider();
    let apiKey = apiKeyInput?.value?.trim() ?? '';
    let baseUrl = getBaseUrlForProvider();
    const modelName = ($('palace-model-name') as HTMLInputElement | null)?.value?.trim() ?? '';
    const apiVersion = ($('palace-api-version') as HTMLInputElement | null)?.value?.trim() ?? '';

    // Detect URL pasted into API key field
    if (apiKey.startsWith('http://') || apiKey.startsWith('https://')) {
      if (!baseUrl) {
        baseUrl = apiKey;
        apiKey = '';
        const targetId = provider === 'azure' ? 'palace-base-url' : 'palace-base-url-openai';
        const bi = $(targetId) as HTMLInputElement | null;
        if (bi) bi.value = baseUrl;
        if (apiKeyInput) { apiKeyInput.value = ''; apiKeyInput.style.borderColor = '#e44'; apiKeyInput.focus(); apiKeyInput.placeholder = 'Enter your API key here (not a URL)'; }
        return null;
      } else {
        if (apiKeyInput) { apiKeyInput.value = ''; apiKeyInput.style.borderColor = '#e44'; apiKeyInput.focus(); apiKeyInput.placeholder = 'This looks like a URL — enter your API key instead'; }
        return null;
      }
    }

    if (provider === 'azure' && !baseUrl) {
      const bi = $('palace-base-url') as HTMLInputElement | null;
      if (bi) { bi.style.borderColor = '#e44'; bi.focus(); bi.placeholder = 'Azure endpoint is required'; }
      return null;
    }

    if (!apiKey) {
      if (apiKeyInput) { apiKeyInput.style.borderColor = '#e44'; apiKeyInput.focus(); apiKeyInput.placeholder = 'API key is required'; }
      return null;
    }
    if (apiKeyInput) apiKeyInput.style.borderColor = '';
    return { apiKey, baseUrl, modelName, apiVersion, provider };
  }

  // ── Test Connection button ──
  $('palace-test-btn')?.addEventListener('click', async () => {
    const testBtn = $('palace-test-btn') as HTMLButtonElement | null;
    const progressDiv = $('palace-install-progress');
    const progressText = $('palace-progress-text') as HTMLElement | null;
    if (!testBtn || !invoke) return;

    const form = readMemoryForm();
    if (!form) return;

    testBtn.disabled = true;
    testBtn.textContent = 'Testing…';
    if (progressDiv) progressDiv.style.display = '';
    if (progressText) progressText.textContent = 'Testing embedding endpoint…';

    try {
      await invoke('test_embedding_connection', {
        apiKey: form.apiKey,
        baseUrl: form.baseUrl || null,
        model: form.modelName || null,
        apiVersion: form.apiVersion || null,
        provider: form.provider,
      });
      if (progressText) progressText.textContent = 'Connection test passed ✓';
    } catch (testErr: any) {
      const errMsg = typeof testErr === 'string' ? testErr : testErr?.message || String(testErr);
      if (progressText) progressText.textContent = `Connection test failed: ${errMsg}`;
    } finally {
      testBtn.disabled = false;
      testBtn.textContent = 'Test Connection';
    }
  });

  // ── Enable / Save button ──
  $('palace-install-btn')?.addEventListener('click', async () => {
    const btn = $('palace-install-btn') as HTMLButtonElement | null;
    const progressDiv = $('palace-install-progress');
    const progressText = $('palace-progress-text') as HTMLElement | null;
    if (!btn || !invoke) return;

    const form = readMemoryForm();
    if (!form) return;
    const { apiKey, baseUrl, modelName, apiVersion, provider } = form;

    btn.disabled = true;
    btn.textContent = 'Testing connection…';
    if (progressDiv) progressDiv.style.display = '';
    if (progressText) progressText.textContent = 'Testing embedding endpoint…';

    try {
      // Step 1: Test the embedding connection before saving
      try {
        await invoke('test_embedding_connection', {
          apiKey,
          baseUrl: baseUrl || null,
          model: modelName || null,
          apiVersion: apiVersion || null,
          provider,
        });
        if (progressText) progressText.textContent = 'Connection test passed ✓ Saving configuration…';
      } catch (testErr: any) {
        // Connection test failed — show the error and let user fix
        const errMsg = typeof testErr === 'string' ? testErr : testErr?.message || String(testErr);
        if (progressText) progressText.textContent = `Connection test failed: ${errMsg}`;
        btn.textContent = 'Retry';
        btn.disabled = false;
        return;
      }

      btn.textContent = 'Saving…';

      // Step 2: Write config to openclaw.json
      await invoke('enable_memory_plugin', {
        apiKey,
        baseUrl: baseUrl || null,
        model: modelName || null,
        apiVersion: apiVersion || null,
        provider,
      });

      if (progressText) progressText.textContent = 'Configuration saved! Restarting gateway…';

      // Restart gateway to pick up the new plugin config
      try {
        await invoke('stop_gateway');
        await new Promise(r => setTimeout(r, 4000));
        await invoke('start_gateway', { port: null });
        await new Promise(r => setTimeout(r, 5000));
      } catch (e) {
        console.warn('[memory] Gateway restart failed:', e);
      }

      // Re-check if memory plugin is now active
      // Config was just written and gateway restarted — check if it's healthy
      _palaceInitialized = false;
      _palaceAvailable = false;

      try {
        const healthy = await invoke<boolean>('check_gateway_health', { port: null });
        const configured = await invoke<boolean>('check_memory_configured');
        _palaceAvailable = healthy && configured;
      } catch { /* ignore */ }

      if (_palaceAvailable) {
        const banner = $('palace-install-banner');
        if (banner) banner.style.display = 'none';
        _palaceInitialized = false;
        await loadMemoryPalace();
        loadMemory();
      } else {
        if (progressText) {
          progressText.textContent = 'Configuration saved. The gateway may need a manual restart to activate the memory plugin.';
        }
        btn.textContent = 'Restart Gateway';
        btn.disabled = false;
        btn.onclick = async () => {
          btn.disabled = true;
          btn.textContent = 'Restarting…';
          try {
            await invoke('stop_gateway');
            await new Promise(r => setTimeout(r, 4000));
            await invoke('start_gateway', { port: null });
            await new Promise(r => setTimeout(r, 5000));
            _palaceInitialized = false;
            await loadMemoryPalace();
            loadMemory();
          } catch (e) {
            if (progressText) progressText.textContent = `Restart failed: ${e}`;
            btn.disabled = false;
            btn.textContent = 'Retry';
          }
        };
      }
    } catch (e) {
      if (progressText) progressText.textContent = `Error: ${e}`;
      btn.textContent = 'Retry';
      btn.disabled = false;
    }
  });

  // Skip button
  $('palace-skip-btn')?.addEventListener('click', () => {
    _palaceSkipped = true;
    const banner = $('palace-install-banner');
    if (banner) banner.style.display = 'none';

    document.querySelectorAll('.palace-tab').forEach(t => t.classList.remove('active'));
    document.querySelectorAll('.palace-panel').forEach(p => (p as HTMLElement).style.display = 'none');
    document.querySelector('.palace-tab[data-palace-tab="files"]')?.classList.add('active');
    const fp = $('palace-files-panel');
    if (fp) fp.style.display = 'flex';

    const filesDivider = $('palace-files-divider');
    if (filesDivider) filesDivider.style.display = 'none';
    const memoryListBelow = $('memory-list');
    if (memoryListBelow) memoryListBelow.style.display = 'none';
  });
}

async function loadPalaceStats() {
  const totalEl = $('palace-total');
  const typesEl = $('palace-types');
  const edgesEl = $('palace-graph-edges');
  if (!totalEl) return;

  if (!_palaceAvailable || !invoke) {
    // Show agent file count as fallback stats
    try {
      const result = await gateway.agentFilesList();
      const files = result.files ?? [];
      totalEl.textContent = String(files.length);
      if (typesEl) typesEl.textContent = 'files';
      if (edgesEl) edgesEl.textContent = '—';
    } catch {
      totalEl.textContent = '—';
      if (typesEl) typesEl.textContent = '—';
      if (edgesEl) edgesEl.textContent = '—';
    }
    return;
  }

  try {
    // Use openclaw ltm stats via Rust command
    const statsText = await invoke<string>('memory_stats');
    // Format: "Total memories: N"
    const countMatch = statsText.match(/(\d+)/);
    if (countMatch) {
      totalEl.textContent = countMatch[1];
    } else {
      totalEl.textContent = '0';
    }
    if (typesEl) typesEl.textContent = 'memories';
    if (edgesEl) edgesEl.textContent = '—'; // LanceDB doesn't have edges
  } catch (e) {
    console.warn('[memory] Stats load failed:', e);
    totalEl.textContent = '—';
    if (typesEl) typesEl.textContent = '—';
    if (edgesEl) edgesEl.textContent = '—';
  }
}

async function loadPalaceSidebar() {
  const list = $('palace-memory-list');
  if (!list) return;

  list.innerHTML = '';

  if (!_palaceAvailable || !invoke) {
    // Fall back to showing agent files
    try {
      const result = await gateway.agentFilesList();
      const files = result.files ?? [];
      if (!files.length) {
        list.innerHTML = `<div class="palace-list-empty">No agent files yet</div>`;
        return;
      }
      for (const file of files) {
        const displayName = file.path ?? file.name ?? 'unknown';
        const displaySize = file.sizeBytes ?? file.size;
        const card = document.createElement('div');
        card.className = 'palace-memory-card';
        card.innerHTML = `
          <span class="palace-memory-type">file</span>
          <div class="palace-memory-subject">${escHtml(displayName)}</div>
          <div class="palace-memory-preview">${displaySize ? formatBytes(displaySize) : 'Agent file'}</div>
        `;
        card.addEventListener('click', () => {
          document.querySelectorAll('.palace-tab').forEach(t => t.classList.remove('active'));
          document.querySelectorAll('.palace-panel').forEach(p => (p as HTMLElement).style.display = 'none');
          document.querySelector('.palace-tab[data-palace-tab="files"]')?.classList.add('active');
          const fp = $('palace-files-panel');
          if (fp) fp.style.display = 'flex';
          openMemoryFile(displayName);
        });
        list.appendChild(card);
      }
    } catch (e) {
      console.warn('Agent files load failed:', e);
      list.innerHTML = '<div class="palace-list-empty">Could not load files</div>';
    }
    return;
  }

  try {
    // Use openclaw ltm search via Rust command
    const jsonText = await invoke<string>('memory_search', { query: 'recent important information', limit: 20 });
    const memories: { id?: string; text?: string; category?: string; importance?: number; score?: number }[] = JSON.parse(jsonText);
    if (!memories.length) {
      list.innerHTML = '<div class="palace-list-empty">No memories yet</div>';
      return;
    }
    for (const mem of memories) {
      const card = document.createElement('div');
      card.className = 'palace-memory-card';
      card.innerHTML = `
        <span class="palace-memory-type">${escHtml(mem.category ?? 'other')}</span>
        <div class="palace-memory-subject">${escHtml((mem.text ?? '').slice(0, 60))}${(mem.text?.length ?? 0) > 60 ? '…' : ''}</div>
        <div class="palace-memory-preview">${mem.score != null ? `${(mem.score * 100).toFixed(0)}% match` : ''}</div>
      `;
      card.addEventListener('click', () => {
        if (mem.id) palaceRecallById(mem.id);
      });
      list.appendChild(card);
    }
  } catch (e) {
    console.warn('Memory sidebar load failed:', e);
    list.innerHTML = '<div class="palace-list-empty">Could not load memories</div>';
  }
}

async function palaceRecallById(memoryId: string) {
  const resultsEl = $('palace-recall-results');
  const emptyEl = $('palace-recall-empty');
  if (!resultsEl) return;

  // Switch to recall tab
  document.querySelectorAll('.palace-tab').forEach(t => t.classList.remove('active'));
  document.querySelectorAll('.palace-panel').forEach(p => (p as HTMLElement).style.display = 'none');
  document.querySelector('.palace-tab[data-palace-tab="recall"]')?.classList.add('active');
  const recallPanel = $('palace-recall-panel');
  if (recallPanel) recallPanel.style.display = 'flex';

  resultsEl.innerHTML = '<div style="padding:1rem;color:var(--text-secondary)">Loading…</div>';
  if (emptyEl) emptyEl.style.display = 'none';

  if (!invoke) {
    resultsEl.innerHTML = '<div style="padding:1rem;color:var(--danger)">Memory not available</div>';
    return;
  }

  try {
    // Use openclaw ltm search via Rust command
    const jsonText = await invoke<string>('memory_search', { query: memoryId, limit: 1 });
    const memories = JSON.parse(jsonText);
    resultsEl.innerHTML = '';
    if (Array.isArray(memories) && memories.length) {
      resultsEl.appendChild(renderRecallCard(memories[0]));
    } else {
      resultsEl.innerHTML = '<div style="padding:1rem;color:var(--text-secondary)">Memory not found</div>';
    }
  } catch (e) {
    resultsEl.innerHTML = `<div style="padding:1rem;color:var(--danger)">Error: ${escHtml(String(e))}</div>`;
  }
}

function renderRecallCard(mem: { id?: string; text?: string; category?: string; importance?: number; score?: number }): HTMLElement {
  const card = document.createElement('div');
  card.className = 'palace-result-card';

  const score = mem.score != null ? `<span class="palace-result-score">${(mem.score * 100).toFixed(0)}%</span>` : '';
  const importance = mem.importance != null ? `<span class="palace-result-tag">importance: ${mem.importance}</span>` : '';

  card.innerHTML = `
    <div class="palace-result-header">
      <span class="palace-result-type">${escHtml(mem.category ?? 'other')}</span>
      ${score}
    </div>
    <div class="palace-result-content">${escHtml(mem.text ?? '')}</div>
    <div class="palace-result-meta">
      ${importance}
    </div>
  `;
  return card;
}

// Palace tab switching
function initPalaceTabs() {
  document.querySelectorAll('.palace-tab').forEach(tab => {
    tab.addEventListener('click', () => {
      const target = (tab as HTMLElement).dataset.palaceTab;
      if (!target) return;

      document.querySelectorAll('.palace-tab').forEach(t => t.classList.remove('active'));
      tab.classList.add('active');

      document.querySelectorAll('.palace-panel').forEach(p => (p as HTMLElement).style.display = 'none');
      const panel = $(`palace-${target}-panel`);
      if (panel) panel.style.display = 'flex';
    });
  });
}

// Palace recall search
function initPalaceRecall() {
  const btn = $('palace-recall-btn');
  const input = $('palace-recall-input') as HTMLTextAreaElement | null;
  if (!btn || !input) return;

  btn.addEventListener('click', () => palaceRecallSearch());
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      palaceRecallSearch();
    }
  });
}

async function palaceRecallSearch() {
  const input = $('palace-recall-input') as HTMLTextAreaElement | null;
  const resultsEl = $('palace-recall-results');
  const emptyEl = $('palace-recall-empty');
  if (!input || !resultsEl) return;

  const query = input.value.trim();
  if (!query) return;

  resultsEl.innerHTML = '<div style="padding:1rem;color:var(--text-secondary)">Searching…</div>';
  if (emptyEl) emptyEl.style.display = 'none';

  if (!_palaceAvailable || !invoke) {
    resultsEl.innerHTML = `<div class="empty-state" style="padding:1rem;">
      <div class="empty-title">Memory not enabled</div>
      <div class="empty-subtitle" style="max-width:380px;line-height:1.6">
        Enable long-term memory in the Memory tab to use semantic recall.
      </div>
    </div>`;
    return;
  }

  try {
    // Use openclaw ltm search via Rust command
    const jsonText = await invoke<string>('memory_search', { query, limit: 10 });
    const memories: { id?: string; text?: string; category?: string; importance?: number; score?: number }[] = JSON.parse(jsonText);
    resultsEl.innerHTML = '';
    if (!memories.length) {
      if (emptyEl) emptyEl.style.display = 'flex';
      return;
    }
    for (const mem of memories) {
      resultsEl.appendChild(renderRecallCard(mem));
    }
  } catch (e) {
    resultsEl.innerHTML = `<div style="padding:1rem;color:var(--danger)">Recall failed: ${escHtml(String(e))}</div>`;
  }
}

// Palace remember form
function initPalaceRemember() {
  const btn = $('palace-remember-save');
  if (!btn) return;

  btn.addEventListener('click', async () => {
    const category = ($('palace-remember-type') as HTMLSelectElement | null)?.value ?? 'other';
    const content = ($('palace-remember-content') as HTMLTextAreaElement | null)?.value.trim() ?? '';
    const importanceStr = ($('palace-remember-importance') as HTMLSelectElement | null)?.value ?? '5';
    const importance = parseInt(importanceStr, 10) || 5;

    if (!content) {
      alert('Content is required.');
      return;
    }

    if (!_palaceAvailable) {
      alert('Memory not enabled. Enable long-term memory in the Memory tab first.');
      return;
    }

    btn.textContent = 'Saving…';
    (btn as HTMLButtonElement).disabled = true;

    try {
      // Call the Tauri command directly for reliable storage
      if (invoke) {
        await invoke('memory_store', {
          content,
          category,
          importance,
        });
      } else {
        // Fallback: ask agent to store (less reliable, for browser-only dev)
        const storeSessionKey = currentSessionKey ?? 'default';
        const storePrompt = `Please store this in long-term memory using memory_store: "${content.replace(/"/g, '\\"')}" with category "${category}" and importance ${importance}. Just confirm when done.`;
        await Promise.race([
          gateway.chatSend(storeSessionKey, storePrompt),
          new Promise<never>((_, reject) => setTimeout(() => reject(new Error('timeout')), 30000)),
        ]);
      }

      // Clear form
      if ($('palace-remember-content') as HTMLTextAreaElement) ($('palace-remember-content') as HTMLTextAreaElement).value = '';

      showToast('Memory saved!', 'success');
      await loadPalaceSidebar();
      await loadPalaceStats();
    } catch (e) {
      showToast(`Save failed: ${e instanceof Error ? e.message : e}`, 'error');
    } finally {
      btn.textContent = 'Save Memory';
      (btn as HTMLButtonElement).disabled = false;
    }
  });
}

// Palace knowledge graph visualization
function initPalaceGraph() {
  const renderBtn = $('palace-graph-render');
  if (!renderBtn) return;

  renderBtn.addEventListener('click', () => renderPalaceGraph());
}

async function renderPalaceGraph() {
  const canvas = $('palace-graph-canvas') as HTMLCanvasElement | null;
  const emptyEl = $('palace-graph-empty');
  if (!canvas) return;

  if (!_palaceAvailable) {
    if (emptyEl) {
      emptyEl.style.display = 'flex';
      emptyEl.innerHTML = `
        <div class="empty-title">Memory Map</div>
        <div class="empty-subtitle">Enable long-term memory to visualize stored knowledge</div>
      `;
    }
    return;
  }

  if (emptyEl) { emptyEl.style.display = 'flex'; emptyEl.textContent = 'Loading memory map…'; }

  if (!invoke) {
    if (emptyEl) { emptyEl.style.display = 'flex'; emptyEl.textContent = 'Memory not available.'; }
    return;
  }

  try {
    // Use openclaw ltm search via Rust command
    const jsonText = await invoke<string>('memory_search', { query: '*', limit: 50 });
    let memories: { id?: string; text?: string; category?: string; importance?: number; score?: number }[] = [];
    try { memories = JSON.parse(jsonText); } catch { /* empty */ }

    if (!memories.length) {
      if (emptyEl) { emptyEl.style.display = 'flex'; emptyEl.textContent = 'No memories to visualize.'; }
      return;
    }

    if (emptyEl) emptyEl.style.display = 'none';

    // Render bubble chart grouped by category
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const rect = canvas.parentElement?.getBoundingClientRect();
    canvas.width = rect?.width ?? 600;
    canvas.height = rect?.height ?? 400;

    const categoryColors: Record<string, string> = {
      other: '#676879', preference: '#0073EA', fact: '#00CA72',
      decision: '#FDAB3D', procedure: '#E44258', concept: '#A25DDC',
      code: '#579BFC', person: '#FF642E', project: '#CAB641',
    };

    // Group by category, place category clusters
    const groups = new Map<string, typeof memories>();
    for (const mem of memories) {
      const cat = mem.category ?? 'other';
      if (!groups.has(cat)) groups.set(cat, []);
      groups.get(cat)!.push(mem);
    }

    // Layout: distribute category centers in a circle
    const categories = Array.from(groups.entries());
    const cx = canvas.width / 2, cy = canvas.height / 2;
    const radius = Math.min(cx, cy) * 0.55;

    ctx.clearRect(0, 0, canvas.width, canvas.height);

    categories.forEach(([cat, mems], i) => {
      const angle = (i / categories.length) * Math.PI * 2 - Math.PI / 2;
      const groupX = cx + Math.cos(angle) * radius;
      const groupY = cy + Math.sin(angle) * radius;

      // Draw category label
      ctx.fillStyle = '#676879';
      ctx.font = 'bold 12px Figtree, sans-serif';
      ctx.textAlign = 'center';
      ctx.fillText(cat.toUpperCase(), groupX, groupY - 30 - mems.length * 2);

      // Draw bubbles for each memory
      mems.forEach((mem, j) => {
        const innerAngle = (j / mems.length) * Math.PI * 2;
        const spread = Math.min(25 + mems.length * 4, 60);
        const mx = groupX + Math.cos(innerAngle) * spread * (0.3 + Math.random() * 0.7);
        const my = groupY + Math.sin(innerAngle) * spread * (0.3 + Math.random() * 0.7);
        const size = 4 + (mem.importance ?? 5) * 0.8;
        const color = categoryColors[cat] ?? '#676879';

        ctx.beginPath();
        ctx.arc(mx, my, size, 0, Math.PI * 2);
        ctx.fillStyle = color;
        ctx.globalAlpha = 0.7;
        ctx.fill();
        ctx.globalAlpha = 1;
        ctx.strokeStyle = '#fff';
        ctx.lineWidth = 1.5;
        ctx.stroke();
      });

      // Count label
      ctx.fillStyle = categoryColors[cat] ?? '#676879';
      ctx.font = '11px Figtree, sans-serif';
      ctx.fillText(`${mems.length}`, groupX, groupY + 35 + mems.length * 2);
    });
  } catch (e) {
    console.warn('Graph render failed:', e);
    if (emptyEl) { emptyEl.style.display = 'flex'; emptyEl.textContent = 'Failed to load memory map.'; }
  }
}

// Palace refresh button
$('palace-refresh')?.addEventListener('click', async () => {
  _palaceInitialized = false;
  _palaceSkipped = false;
  await loadMemoryPalace();
  loadMemory();
});

// Palace sidebar search filter (local filter of visible cards)
$('palace-search')?.addEventListener('input', () => {
  const query = (($('palace-search') as HTMLInputElement)?.value ?? '').toLowerCase();
  document.querySelectorAll('.palace-memory-card').forEach(card => {
    const text = card.textContent?.toLowerCase() ?? '';
    (card as HTMLElement).style.display = text.includes(query) ? '' : 'none';
  });
});

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
  const sessionKey = 'paw-create-' + _activeDocId;

  _contentStreaming = true;
  _contentStreamContent = '';
  _contentStreamRunId = null;
  showToast('AI improving your text…', 'info');

  const done = new Promise<string>((resolve) => {
    _contentStreamResolve = resolve;
    setTimeout(() => resolve(_contentStreamContent || '(Timed out)'), 120_000);
  });

  try {
    const result = await gateway.chatSend(sessionKey, `Improve this text. Return only the improved version, no explanations:\n\n${body}`);
    if (result.runId) _contentStreamRunId = result.runId;

    const finalText = await done;
    if (finalText && bodyEl) {
      bodyEl.value = finalText;
      showToast('Text improved!', 'success');
    }
  } catch (e) {
    showToast(`Failed: ${e instanceof Error ? e.message : e}`, 'error');
  } finally {
    _contentStreaming = false;
    _contentStreamRunId = null;
    _contentStreamResolve = null;
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
let _activeResearchId: string | null = null;

// ── Build — streaming state ────────────────────────────────────────────────
let _buildStreaming = false;
let _buildStreamContent = '';
let _buildStreamRunId: string | null = null;
let _buildStreamResolve: ((text: string) => void) | null = null;

// ── Content — streaming state ──────────────────────────────────────────────
let _contentStreaming = false;
let _contentStreamContent = '';
let _contentStreamRunId: string | null = null;
let _contentStreamResolve: ((text: string) => void) | null = null;

// ── Research — Agent-powered research ──────────────────────────────────────
let _researchStreaming = false;
let _researchContent = '';
let _researchRunId: string | null = null;
let _researchResolve: ((text: string) => void) | null = null;

async function loadResearchProjects() {
  const list = $('research-project-list');
  const empty = $('research-empty');
  const workspace = $('research-workspace');
  if (!list) return;

  const projects = await listProjects('research');
  list.innerHTML = '';

  if (!projects.length && !_activeResearchId) {
    if (empty) empty.style.display = 'flex';
    if (workspace) workspace.style.display = 'none';
    return;
  }

  for (const p of projects) {
    const item = document.createElement('div');
    item.className = `studio-doc-item${p.id === _activeResearchId ? ' active' : ''}`;
    item.innerHTML = `
      <div class="studio-doc-title">${escHtml(p.name)}</div>
      <div class="studio-doc-meta">${new Date(p.updated_at).toLocaleDateString()}</div>
    `;
    item.addEventListener('click', () => openResearchProject(p.id));
    list.appendChild(item);
  }
}

async function openResearchProject(id: string) {
  _activeResearchId = id;
  const empty = $('research-empty');
  const workspace = $('research-workspace');
  if (empty) empty.style.display = 'none';
  if (workspace) workspace.style.display = '';
  await loadResearchFindings(id);
  loadResearchProjects();
}

async function loadResearchFindings(projectId: string) {
  const list = $('research-findings-list');
  const header = $('research-findings-header');
  if (!list) return;

  // Load findings saved as content docs linked to this project
  const allDocs = await listDocs();
  const findings = allDocs.filter(d => d.project_id === projectId && d.content_type === 'research-finding');
  const savedReports = allDocs.filter(d => d.project_id === projectId && d.content_type === 'research-report');
  list.innerHTML = '';

  // Show "View Saved Report" button if a report was previously generated
  if (savedReports.length) {
    const reportBtn = document.createElement('button');
    reportBtn.className = 'btn btn-ghost btn-sm';
    reportBtn.style.marginBottom = '8px';
    reportBtn.textContent = `View saved report (${new Date(savedReports[0].created_at).toLocaleDateString()})`;
    reportBtn.addEventListener('click', () => {
      const reportArea = $('research-report-area');
      const findingsArea = $('research-findings-area');
      const reportContent = $('research-report-content');
      if (reportArea) reportArea.style.display = '';
      if (findingsArea) findingsArea.style.display = 'none';
      if (reportContent) reportContent.innerHTML = formatResearchContent(savedReports[0].content);
    });
    list.appendChild(reportBtn);
  }

  if (findings.length) {
    if (header) header.style.display = 'flex';
    for (const f of findings) {
      list.appendChild(renderFindingCard(f));
    }
  } else {
    if (header) header.style.display = 'none';
  }
}

function renderFindingCard(doc: import('./db').ContentDoc): HTMLElement {
  const card = document.createElement('div');
  card.className = 'research-finding-card';
  card.innerHTML = `
    <div class="research-finding-header">
      <div class="research-finding-title">${escHtml(doc.title)}</div>
      <div class="research-finding-actions">
        <span class="research-finding-meta">${new Date(doc.created_at).toLocaleString()}</span>
        <button class="btn btn-ghost btn-xs research-finding-delete" data-id="${escAttr(doc.id)}" title="Remove">✕</button>
      </div>
    </div>
    <div class="research-finding-body">${formatResearchContent(doc.content)}</div>
  `;
  card.querySelector('.research-finding-delete')?.addEventListener('click', async (e) => {
    e.stopPropagation();
    await deleteDoc(doc.id);
    if (_activeResearchId) loadResearchFindings(_activeResearchId);
  });
  return card;
}

/** Render markdown-like text to HTML (used for chat messages and research findings) */
function formatMarkdown(text: string): string {
  // Fenced code blocks first (before escaping)
  let html = text.replace(/```(\w*)\n([\s\S]*?)```/g, (_m, lang, code) => {
    return `<pre class="code-block" data-lang="${escHtml(lang)}"><code>${escHtml(code.trimEnd())}</code></pre>`;
  });
  // Now escape everything else (except already-replaced code blocks)
  // Split on code blocks, escape non-code parts, rejoin
  const parts = html.split(/(<pre class="code-block"[\s\S]*?<\/pre>)/);
  html = parts.map((part, i) => {
    if (i % 2 === 1) return part; // code block — leave as is
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

function formatResearchContent(text: string): string {
  // Simple markdown-ish rendering: bold, headers, bullets, links
  return escHtml(text)
    .replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
    .replace(/^### (.+)$/gm, '<h4>$1</h4>')
    .replace(/^## (.+)$/gm, '<h3>$1</h3>')
    .replace(/^# (.+)$/gm, '<h2>$1</h2>')
    .replace(/^[-•] (.+)$/gm, '<div class="research-bullet">• $1</div>')
    .replace(/\n/g, '<br>');
}

async function runResearch() {
  if (!_activeResearchId || !wsConnected || _researchStreaming) return;
  const input = $('research-topic-input') as HTMLInputElement | null;
  const topic = input?.value.trim();
  if (!topic) return;

  const projectId = _activeResearchId;
  const sessionKey = 'paw-research-' + projectId;

  // Show live output
  _researchStreaming = true;
  _researchContent = '';
  _researchRunId = null;
  const liveArea = $('research-live');
  const liveContent = $('research-live-content');
  const runBtn = $('research-run-btn');
  if (liveArea) liveArea.style.display = '';
  if (liveContent) liveContent.innerHTML = '<div class="loading-dots"><span></span><span></span><span></span></div>';
  if (runBtn) runBtn.setAttribute('disabled', 'true');
  const label = $('research-live-label');
  if (label) label.textContent = 'Researching…';

  // Promise that resolves when the agent finishes
  const done = new Promise<string>((resolve) => {
    _researchResolve = resolve;
    setTimeout(() => resolve(_researchContent || '(Research timed out)'), 180_000);
  });

  try {
    const result = await gateway.chatSend(sessionKey,
      `Research this topic thoroughly. Browse the web, find multiple sources, and provide detailed findings with key insights, data points, and source URLs. Be comprehensive and structured.\n\nTopic: ${topic}`
    );
    if (result.runId) _researchRunId = result.runId;

    const finalText = await done;

    // Save finding to database
    const findingId = crypto.randomUUID();
    await saveDoc({
      id: findingId,
      project_id: projectId,
      title: topic,
      content: finalText,
      content_type: 'research-finding',
    });

    // Update UI
    if (liveArea) liveArea.style.display = 'none';
    if (input) input.value = '';
    await loadResearchFindings(projectId);
  } catch (e) {
    console.error('[research] Error:', e);
    if (liveContent) {
      liveContent.textContent = `Error: ${e instanceof Error ? e.message : e}`;
    }
  } finally {
    _researchStreaming = false;
    _researchRunId = null;
    _researchResolve = null;
    if (runBtn) runBtn.removeAttribute('disabled');
    if (label) label.textContent = 'Done';
  }
}

function appendResearchDelta(text: string) {
  _researchContent += text;
  const liveContent = $('research-live-content');
  if (liveContent) {
    liveContent.textContent = _researchContent;
    // Auto-scroll the live area
    liveContent.scrollTop = liveContent.scrollHeight;
  }
}

async function generateResearchReport() {
  if (!_activeResearchId || !wsConnected) return;

  const allDocs = await listDocs();
  const findings = allDocs.filter(d => d.project_id === _activeResearchId && d.content_type === 'research-finding');
  if (!findings.length) { alert('No findings yet — run some research first'); return; }

  const reportArea = $('research-report-area');
  const findingsArea = $('research-findings-area');
  const reportContent = $('research-report-content');
  if (reportArea) reportArea.style.display = '';
  if (findingsArea) findingsArea.style.display = 'none';
  if (reportContent) reportContent.innerHTML = '<div class="loading-dots"><span></span><span></span><span></span></div>';

  // Compile findings into a prompt
  const findingsText = findings.map((f, i) => `## Finding ${i + 1}: ${f.title}\n${f.content}`).join('\n\n---\n\n');

  const sessionKey = 'paw-research-' + _activeResearchId;

  // Temporarily capture deltas for the report
  const prevStreaming = _researchStreaming;
  _researchStreaming = true;
  _researchContent = '';

  const done = new Promise<string>((resolve) => {
    _researchResolve = resolve;
    setTimeout(() => resolve(_researchContent || '(Report generation timed out)'), 180_000);
  });

  try {
    const result = await gateway.chatSend(sessionKey,
      `Based on all the research findings below, write a comprehensive, well-structured report. Include an executive summary, key findings organized by theme, conclusions, and a list of sources. Use markdown formatting.\n\n${findingsText}`
    );
    if (result.runId) _researchRunId = result.runId;

    const reportText = await done;
    if (reportContent) reportContent.innerHTML = formatResearchContent(reportText);

    // Persist report to DB so it survives reload
    if (reportText && _activeResearchId) {
      const reportId = crypto.randomUUID();
      await saveDoc({
        id: reportId,
        project_id: _activeResearchId,
        title: `Research Report — ${new Date().toLocaleDateString()}`,
        content: reportText,
        content_type: 'research-report',
      });
      showToast('Report saved', 'success');
    }
  } catch (e) {
    if (reportContent) reportContent.textContent = `Error generating report: ${e instanceof Error ? e.message : e}`;
  } finally {
    _researchStreaming = prevStreaming;
    _researchRunId = null;
    _researchResolve = null;
  }
}

async function createNewResearch() {
  const name = await promptModal('Research project name:', 'My research project');
  if (!name) return;
  const id = crypto.randomUUID();
  await saveProject({ id, name, space: 'research' });
  openResearchProject(id);
  loadResearchProjects();
}

// Wire up research event handlers
$('research-new-project')?.addEventListener('click', createNewResearch);
$('research-create-first')?.addEventListener('click', createNewResearch);

$('research-run-btn')?.addEventListener('click', runResearch);
$('research-topic-input')?.addEventListener('keydown', (e) => {
  if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); runResearch(); }
});

$('research-abort-btn')?.addEventListener('click', () => {
  if (!_activeResearchId) return;
  gateway.chatAbort('paw-research-' + _activeResearchId).catch(console.warn);
  if (_researchResolve) {
    _researchResolve(_researchContent || '(Aborted)');
    _researchResolve = null;
  }
});

$('research-generate-report')?.addEventListener('click', generateResearchReport);

$('research-close-report')?.addEventListener('click', () => {
  const reportArea = $('research-report-area');
  const findingsArea = $('research-findings-area');
  if (reportArea) reportArea.style.display = 'none';
  if (findingsArea) findingsArea.style.display = '';
});

$('research-delete-project')?.addEventListener('click', async () => {
  if (!_activeResearchId) return;
  if (!confirm('Delete this research project and all its findings?')) return;
  // Delete associated findings
  const allDocs = await listDocs();
  for (const d of allDocs.filter(d => d.project_id === _activeResearchId)) {
    await deleteDoc(d.id);
  }
  await deleteProject(_activeResearchId);
  _activeResearchId = null;
  const workspace = $('research-workspace');
  const empty = $('research-empty');
  if (workspace) workspace.style.display = 'none';
  if (empty) empty.style.display = 'flex';
  loadResearchProjects();
});

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
function escAttr(s: string): string {
  return s.replace(/"/g, '&quot;').replace(/'/g, '&#39;');
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

// ── Settings: Logs viewer ──────────────────────────────────────────────────
async function loadSettingsLogs() {
  if (!wsConnected) return;
  const section = $('settings-logs-section');
  const output = $('settings-logs-output');
  const linesSelect = $('settings-logs-lines') as HTMLSelectElement | null;
  try {
    const lines = parseInt(linesSelect?.value ?? '100', 10);
    const result = await gateway.logsTail(lines);
    if (section) section.style.display = '';
    if (output) output.textContent = (result.lines ?? []).join('\n') || '(no logs)';
  } catch (e) {
    console.warn('[settings] Logs load failed:', e);
    if (section) section.style.display = 'none';
  }
}

$('settings-refresh-logs')?.addEventListener('click', () => loadSettingsLogs());

// ── Settings: Usage dashboard ──────────────────────────────────────────────
async function loadSettingsUsage() {
  if (!wsConnected) return;
  const section = $('settings-usage-section');
  const content = $('settings-usage-content');
  try {
    const [status, cost] = await Promise.all([
      gateway.usageStatus().catch(() => null),
      gateway.usageCost().catch(() => null),
    ]);
    if (!status && !cost) { if (section) section.style.display = 'none'; return; }
    if (section) section.style.display = '';
    let html = '';
    if (status?.total) {
      html += `<div class="usage-card">
        <div class="usage-card-label">Requests</div>
        <div class="usage-card-value">${status.total.requests?.toLocaleString() ?? '—'}</div>
      </div>
      <div class="usage-card">
        <div class="usage-card-label">Tokens</div>
        <div class="usage-card-value">${status.total.tokens?.toLocaleString() ?? '—'}</div>
        <div class="usage-card-sub">In: ${(status.total.inputTokens ?? 0).toLocaleString()} / Out: ${(status.total.outputTokens ?? 0).toLocaleString()}</div>
      </div>`;
    }
    if (cost?.totalCost != null) {
      html += `<div class="usage-card">
        <div class="usage-card-label">Cost</div>
        <div class="usage-card-value">$${cost.totalCost.toFixed(4)} ${cost.currency ?? ''}</div>
      </div>`;
    }
    if (status?.byModel) {
      html += '<div class="usage-models"><h4>By Model</h4>';
      for (const [model, data] of Object.entries(status.byModel)) {
        const d = data as { requests?: number; tokens?: number };
        html += `<div class="usage-model-row"><span class="usage-model-name">${escHtml(model)}</span><span>${(d.requests ?? 0).toLocaleString()} req / ${(d.tokens ?? 0).toLocaleString()} tok</span></div>`;
      }
      html += '</div>';
    }
    if (content) content.innerHTML = html || '<p style="color:var(--text-muted)">No usage data</p>';
  } catch (e) {
    console.warn('[settings] Usage load failed:', e);
    if (section) section.style.display = 'none';
  }
}

$('settings-refresh-usage')?.addEventListener('click', () => loadSettingsUsage());

// ── Settings: System presence ──────────────────────────────────────────────
async function loadSettingsPresence() {
  if (!wsConnected) return;
  const section = $('settings-presence-section');
  const list = $('settings-presence-list');
  try {
    const result = await gateway.systemPresence();
    const entries = result.entries ?? [];
    if (!entries.length) { if (section) section.style.display = 'none'; return; }
    if (section) section.style.display = '';
    if (list) {
      list.innerHTML = entries.map(e => {
        const name = e.client?.id ?? e.connId ?? 'Unknown';
        const platform = e.client?.platform ?? '';
        const role = e.role ?? '';
        return `
          <div class="presence-entry">
            <div class="presence-dot online"></div>
            <div class="presence-info">
              <div class="presence-name">${escHtml(name)}</div>
              <div class="presence-meta">${escHtml(role)} · ${escHtml(platform)}${e.connectedAt ? ' · ' + new Date(e.connectedAt).toLocaleString() : ''}</div>
            </div>
          </div>
        `;
      }).join('');
    }
  } catch (e) {
    console.warn('[settings] Presence load failed:', e);
    if (section) section.style.display = 'none';
  }
}

$('settings-refresh-presence')?.addEventListener('click', () => loadSettingsPresence());

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

gateway.on('exec.approval.requested', (payload: unknown) => {
  const evt = payload as Record<string, unknown>;
  const id = (evt.id ?? evt.approvalId) as string | undefined;
  const tool = (evt.tool ?? evt.name ?? '') as string;
  const desc = (evt.description ?? evt.message ?? `The agent wants to use tool: ${tool}`) as string;
  const args = evt.args as Record<string, unknown> | undefined;

  const modal = $('approval-modal');
  const descEl = $('approval-modal-desc');
  const detailsEl = $('approval-modal-details');
  if (!modal || !descEl) return;

  const sessionKey = (evt.sessionKey ?? '') as string;

  // ── Permission enforcement for mail/credential tools ──
  const mailPerm = classifyMailPermission(tool, args);
  if (mailPerm) {
    // Check if ANY account has this permission granted
    const anyAllowed = _mailAccounts.some(acct => {
      const perms = loadMailPermissions(acct.name);
      return perms[mailPerm.perm];
    });
    if (!anyAllowed) {
      // Auto-deny and log
      if (id) gateway.request('exec.approvals.resolve', { id, allowed: false }).catch(console.warn);
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
    // Permission granted — log it and continue to approval modal
    logCredentialActivity({
      action: mailPerm.perm,
      toolName: tool,
      detail: `Agent requested: ${tool}${args ? ' ' + JSON.stringify(args).slice(0, 120) : ''}`,
      sessionKey,
      wasAllowed: true,
    });
  }

  descEl.textContent = desc;
  if (detailsEl) {
    detailsEl.innerHTML = args
      ? `<pre class="code-block"><code>${escHtml(JSON.stringify(args, null, 2))}</code></pre>`
      : '';
  }
  modal.style.display = 'flex';

  // Resolve when user clicks Allow/Deny
  const cleanup = () => {
    modal.style.display = 'none';
    $('approval-allow-btn')?.removeEventListener('click', onAllow);
    $('approval-deny-btn')?.removeEventListener('click', onDeny);
    $('approval-modal-close')?.removeEventListener('click', onDeny);
  };
  const onAllow = () => {
    cleanup();
    if (id) gateway.request('exec.approvals.resolve', { id, allowed: true }).catch(console.warn);
    showToast('Tool approved', 'success');
  };
  const onDeny = () => {
    cleanup();
    if (id) gateway.request('exec.approvals.resolve', { id, allowed: false }).catch(console.warn);
    showToast('Tool denied', 'warning');
  };
  $('approval-allow-btn')?.addEventListener('click', onAllow);
  $('approval-deny-btn')?.addEventListener('click', onDeny);
  $('approval-modal-close')?.addEventListener('click', onDeny);
});

// ── Initialize ─────────────────────────────────────────────────────────────
document.addEventListener('DOMContentLoaded', async () => {
  try {
    console.log('[main] Paw starting...');

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
