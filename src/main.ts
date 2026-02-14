// Paw — Main Application
// Wires OpenClaw gateway (WebSocket protocol v3) to the UI

import type { AppConfig, Message, InstallProgress, ChatMessage, Session } from './types';
import { setGatewayConfig, probeHealth } from './api';
import { gateway } from './gateway';
import { initDb, listModes, saveMode, deleteMode, listDocs, saveDoc, getDoc, deleteDoc, listProjects, saveProject, deleteProject } from './db';
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
      case 'foundry': loadModels(); loadModes(); break;
      case 'memory': loadMemoryPalace(); loadMemory(); break;
      case 'build': loadSpaceCron('build'); break;
      case 'mail': loadSpaceCron('mail'); break;
      case 'settings': syncSettingsForm(); loadGatewayConfig(); break;
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
    try {
      const sessResult = await gateway.listSessions({ limit: 50 });
      const activeSessions = sessResult.sessions ?? [];
      for (const s of activeSessions) {
        try {
          await gateway.chatAbort(s.key);
        } catch { /* no running exec on this session — that's fine */ }
      }
      if (activeSessions.length) {
        console.log(`[main] Cleared ${activeSessions.length} session(s) of stale agent runs`);
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
    sessions = result.sessions ?? [];
    renderSessionSelect();
    if (!currentSessionKey && sessions.length) {
      currentSessionKey = sessions[0].key;
    }
    // Don't reload chat history if we're in the middle of streaming
    if (currentSessionKey && !isLoading) await loadChatHistory(currentSessionKey);
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
    const result = await gateway.chatSend(sessionKey, content);
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
    _streamingEl.textContent = _streamingContent;
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
    contentEl.textContent = msg.content;
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
          _streamingEl.textContent = text;
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
  telegram: '✈️', discord: '🎮', whatsapp: '💬', signal: '🔒', slack: '💼',
};
const CHANNEL_CLASSES: Record<string, string> = {
  telegram: 'telegram', discord: 'discord', whatsapp: 'whatsapp', signal: 'signal', slack: 'slack',
};

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
      const icon = CHANNEL_ICONS[lId] ?? '📡';
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
  } catch (e) {
    console.warn('Channels load failed:', e);
    if (loading) loading.style.display = 'none';
    if (empty) empty.style.display = 'flex';
  }
}
$('refresh-channels-btn')?.addEventListener('click', () => loadChannels());

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
          <button class="btn btn-ghost btn-sm cron-delete" data-id="${escAttr(job.id)}">🗑</button>
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

      const enabledClass = skill.enabled ? 'enabled' : '';
      const statusLabel = skill.installed
        ? (skill.enabled ? 'Enabled' : 'Disabled')
        : 'Available';
      const statusClass = skill.installed
        ? (skill.enabled ? 'connected' : 'muted')
        : 'muted';

      card.innerHTML = `
        <div class="skill-card-header">
          <span class="skill-card-name">${escHtml(skill.label ?? skill.name)}</span>
          <span class="status-badge ${statusClass}">${statusLabel}</span>
        </div>
        <div class="skill-card-desc">${escHtml(skill.description ?? '')}</div>
        <div class="skill-card-footer">
          <div style="display:flex;align-items:center;gap:8px">
            <span class="skill-card-version">${skill.version ? 'v' + escHtml(skill.version) : ''}</span>
            ${skill.homepage ? `<a class="skill-card-link" href="${escAttr(skill.homepage)}" target="_blank">docs ↗</a>` : ''}
          </div>
          <div class="skill-card-actions">
            ${skill.installed ? `
              <button class="skill-toggle ${enabledClass}" data-name="${escAttr(skill.name)}" data-enabled="${skill.enabled}" title="${skill.enabled ? 'Disable' : 'Enable'}"></button>
              <button class="btn btn-ghost btn-sm skill-update" data-name="${escAttr(skill.name)}">Update</button>
            ` : `
              <button class="btn btn-primary btn-sm skill-install" data-name="${escAttr(skill.name)}">Install</button>
            `}
          </div>
        </div>
      `;
      if (skill.installed) {
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
      const installId = crypto.randomUUID();
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
      const name = (btn as HTMLElement).dataset.name!;
      const currentlyEnabled = (btn as HTMLElement).dataset.enabled === 'true';
      const newState = !currentlyEnabled;

      (btn as HTMLButtonElement).disabled = true;
      try {
        await gateway.skillsUpdate(name, { enabled: newState });
        showSkillsToast(`${name} ${newState ? 'enabled' : 'disabled'}`, 'success');
        await loadSkills();
      } catch (e) {
        showSkillsToast(`Failed to ${newState ? 'enable' : 'disable'} ${name}: ${e}`, 'error');
        (btn as HTMLButtonElement).disabled = false;
      }
    });
  });

  // Update buttons
  document.querySelectorAll('.skill-update').forEach(btn => {
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const name = (btn as HTMLElement).dataset.name!;
      (btn as HTMLButtonElement).disabled = true;
      (btn as HTMLButtonElement).textContent = 'Updating…';
      showSkillsToast(`Updating ${name}…`, 'info');
      try {
        await gateway.skillsUpdate(name, { update: true });
        showSkillsToast(`${name} updated!`, 'success');
        await loadSkills();
      } catch (e) {
        showSkillsToast(`Update failed for ${name}: ${e}`, 'error');
        (btn as HTMLButtonElement).disabled = false;
        (btn as HTMLButtonElement).textContent = 'Update';
      }
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
          (btn as HTMLButtonElement).textContent = '✓ Installed';
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
    if (modelsPanel) modelsPanel.style.display = target === 'models' ? '' : 'none';
    if (modesPanel) modesPanel.style.display = target === 'modes' ? '' : 'none';
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
        <div class="mode-card-icon" style="background:${mode.color}22">${mode.icon || '🤖'}</div>
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
  ($('mode-form-icon') as HTMLInputElement).value = mode?.icon ?? '🤖';
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
    icon: ($('mode-form-icon') as HTMLInputElement).value || '🤖',
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
          <span class="list-item-title">📄 ${escHtml(displayName)}</span>
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

// ── Memory Palace ──────────────────────────────────────────────────────────
let _palaceInitialized = false;
let _palaceAvailable = false;
let _palaceSkipped = false;

async function loadMemoryPalace() {
  if (!wsConnected) return;

  // Check if memory-palace is installed and registered as a gateway skill
  if (!_palaceInitialized) {
    _palaceInitialized = true;

    // Check via Tauri if palace is installed + registered as MCP skill
    if (invoke) {
      try {
        const healthy = await invoke<boolean>('check_palace_health');
        _palaceAvailable = healthy;
        if (healthy) {
          console.log('[palace] Memory Palace installed and registered as MCP skill');
        }
      } catch {
        _palaceAvailable = false;
      }
    }

    // Fallback: check via gateway skills API
    if (!_palaceAvailable) {
      try {
        const status = await gateway.skillsStatus();
        const skills = status.skills ?? [];
        _palaceAvailable = skills.some(
          (s: { name: string; installed?: boolean; enabled?: boolean }) =>
            s.name.toLowerCase().includes('memory') && s.installed !== false && s.enabled !== false,
        );
      } catch {
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
      // Show install banner
      if (banner) banner.style.display = 'flex';
      // Check Python availability for the banner
      if (invoke) {
        try {
          const hasPython = await invoke<boolean>('check_python_installed');
          const reqEl = $('palace-req-python');
          const statusEl = $('palace-req-python-status');
          if (reqEl) reqEl.classList.add(hasPython ? 'ready' : 'missing');
          if (statusEl) statusEl.textContent = hasPython ? '✓ found' : '✗ not found';
        } catch { /* ignore */ }

        // Check Ollama availability
        try {
          const hasOllama = await invoke<boolean>('check_ollama_installed');
          const reqEl = $('palace-req-ollama');
          const statusEl = $('palace-req-ollama-status');
          if (reqEl) reqEl.classList.add(hasOllama ? 'ready' : 'missing');
          if (statusEl) statusEl.textContent = hasOllama ? '✓ found' : '✗ not found — install from ollama.ai';
        } catch { /* ignore */ }
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
      // Palace is available — full mode
      if (banner) banner.style.display = 'none';
      if (filesDivider) filesDivider.style.display = '';
    }
  }

  // Load palace stats + memory list via gateway
  await loadPalaceStats();
  await loadPalaceSidebar();
}

function initPalaceInstall() {
  // Install button
  $('palace-install-btn')?.addEventListener('click', async () => {
    const btn = $('palace-install-btn') as HTMLButtonElement | null;
    const progress = $('palace-install-progress');
    const progressBar = $('palace-progress-bar') as HTMLElement | null;
    const progressText = $('palace-progress-text') as HTMLElement | null;
    if (!btn || !invoke) return;

    btn.disabled = true;
    btn.textContent = 'Installing…';
    if (progress) progress.style.display = '';

    try {
      // Listen for progress events
      if (listen) {
        await listen<{ percent: number; message: string }>('palace-install-progress', (event: { payload: { percent: number; message: string } }) => {
          const { percent, message } = event.payload;
          if (progressBar) progressBar.style.width = `${percent}%`;
          if (progressText) progressText.textContent = message;
        });
      }

      await invoke('install_palace');

      // Check if it's now registered as a gateway skill
      await new Promise(r => setTimeout(r, 1000));
      const healthy = await invoke<boolean>('check_palace_health').catch(() => false);

      if (healthy) {
        _palaceAvailable = true;
        _palaceInitialized = false; // Re-initialize to load palace mode
        const banner = $('palace-install-banner');
        if (banner) banner.style.display = 'none';
        await loadMemoryPalace();
        loadMemory();
      } else {
        // Installed but not yet registered — might need gateway restart
        let logTail = '';
        try {
          logTail = await invoke<string>('get_palace_log');
        } catch { /* ignore */ }

        if (progressText) {
          progressText.textContent = logTail
            ? 'Installed but skill registration may need a gateway restart. See log below:'
            : 'Installed! You may need to restart the gateway for the skill to activate.';
        }

        // Show log output if available
        if (logTail && progress) {
          let logEl = document.getElementById('palace-log-output');
          if (!logEl) {
            logEl = document.createElement('pre');
            logEl.id = 'palace-log-output';
            logEl.style.cssText = 'max-height:140px;overflow:auto;background:#f0f0f0;border-radius:6px;padding:8px 10px;font-size:11px;color:#444;margin-top:8px;text-align:left;white-space:pre-wrap;word-break:break-all;';
            progress.appendChild(logEl);
          }
          logEl.textContent = logTail;
        }

        btn.textContent = 'Refresh';
        btn.disabled = false;
        btn.onclick = () => {
          _palaceInitialized = false;
          loadMemoryPalace();
        };
      }
    } catch (e) {
      // Get palace.log for diagnostics on error too
      let logTail = '';
      try {
        if (invoke) logTail = await invoke<string>('get_palace_log');
      } catch { /* ignore */ }

      if (progressText) progressText.textContent = `Error: ${e}`;

      if (logTail && progress) {
        let logEl = document.getElementById('palace-log-output');
        if (!logEl) {
          logEl = document.createElement('pre');
          logEl.id = 'palace-log-output';
          logEl.style.cssText = 'max-height:140px;overflow:auto;background:#f0f0f0;border-radius:6px;padding:8px 10px;font-size:11px;color:#444;margin-top:8px;text-align:left;white-space:pre-wrap;word-break:break-all;';
          progress.appendChild(logEl);
        }
        logEl.textContent = logTail;
      }

      btn.textContent = 'Retry';
      btn.disabled = false;
    }
  });

  // Skip button
  $('palace-skip-btn')?.addEventListener('click', () => {
    _palaceSkipped = true;
    const banner = $('palace-install-banner');
    if (banner) banner.style.display = 'none';

    // Switch to files mode
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

  if (!_palaceAvailable) {
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
    const result = await gateway.chatSend('paw-memory', 'Use memory_stats to show current statistics. Return only raw JSON.', { thinking: 'minimal' });
    const text = typeof result === 'string' ? result : (result as { text?: string }).text ?? '';
    // Try to parse stats from the response
    const jsonMatch = text.match(/\{[\s\S]*"total"[\s\S]*\}/);
    if (jsonMatch) {
      const stats = JSON.parse(jsonMatch[0]);
      totalEl.textContent = String(stats.total ?? stats.total_memories ?? '0');
      if (typesEl) typesEl.textContent = String(stats.types ?? stats.type_count ?? '—');
      if (edgesEl) edgesEl.textContent = String(stats.edges ?? stats.total_edges ?? '—');
    }
  } catch {
    totalEl.textContent = '—';
    if (typesEl) typesEl.textContent = '—';
    if (edgesEl) edgesEl.textContent = '—';
  }
}

async function loadPalaceSidebar() {
  const list = $('palace-memory-list');
  if (!list) return;

  list.innerHTML = '';

  if (!_palaceAvailable) {
    // Fall back to showing agent files as memory entries in the sidebar
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
          // Switch to Files tab and open this file
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
    const result = await gateway.chatSend('paw-memory', 'Use memory_recent with limit 20. Return only raw JSON.', { thinking: 'minimal' });
    const text = typeof result === 'string' ? result : (result as { text?: string }).text ?? '';
    const jsonMatch = text.match(/\[[\s\S]*\]/);
    if (jsonMatch) {
      const memories: { id?: string; type?: string; subject?: string; content?: string; created_at?: string }[] = JSON.parse(jsonMatch[0]);
      for (const mem of memories) {
        const card = document.createElement('div');
        card.className = 'palace-memory-card';
        card.innerHTML = `
          <span class="palace-memory-type">${escHtml(mem.type ?? 'note')}</span>
          <div class="palace-memory-subject">${escHtml(mem.subject ?? 'Untitled')}</div>
          <div class="palace-memory-preview">${escHtml((mem.content ?? '').slice(0, 80))}${(mem.content?.length ?? 0) > 80 ? '…' : ''}</div>
          <div class="palace-memory-meta">${mem.created_at ? new Date(mem.created_at).toLocaleDateString() : ''}</div>
        `;
        card.addEventListener('click', () => {
          if (mem.id) palaceRecallById(mem.id);
        });
        list.appendChild(card);
      }
    }
  } catch (e) {
    console.warn('Palace sidebar load failed:', e);
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

  try {
    const result = await gateway.chatSend('paw-memory', `Use memory_get with id "${memoryId}". Return only raw JSON.`, { thinking: 'minimal' });
    const text = typeof result === 'string' ? result : (result as { text?: string }).text ?? '';
    const jsonMatch = text.match(/\{[\s\S]*\}/);
    if (jsonMatch) {
      const mem = JSON.parse(jsonMatch[0]);
      resultsEl.innerHTML = '';
      resultsEl.appendChild(renderRecallCard(mem));
    }
  } catch (e) {
    resultsEl.innerHTML = `<div style="padding:1rem;color:var(--danger)">Error: ${escHtml(String(e))}</div>`;
  }
}

function renderRecallCard(mem: { type?: string; subject?: string; content?: string; score?: number; created_at?: string; tags?: string[]; edges?: { type?: string; target_subject?: string }[] }): HTMLElement {
  const card = document.createElement('div');
  card.className = 'palace-result-card';

  const score = mem.score != null ? `<span class="palace-result-score">${(mem.score * 100).toFixed(0)}%</span>` : '';
  const tags = (mem.tags ?? []).map(t => `<span class="palace-result-tag">${escHtml(t)}</span>`).join('');
  const edges = (mem.edges ?? []).map(e =>
    `<span class="palace-result-edge ${escAttr(e.type ?? '')}">${escHtml(e.type ?? 'link')} → ${escHtml(e.target_subject ?? '?')}</span>`,
  ).join('');

  card.innerHTML = `
    <div class="palace-result-header">
      <span class="palace-result-type">${escHtml(mem.type ?? 'note')}</span>
      ${score}
    </div>
    <div class="palace-result-subject">${escHtml(mem.subject ?? 'Untitled')}</div>
    <div class="palace-result-content">${escHtml(mem.content ?? '')}</div>
    <div class="palace-result-meta">
      ${mem.created_at ? new Date(mem.created_at).toLocaleDateString() : ''}
      ${tags ? `<div class="palace-result-tags">${tags}</div>` : ''}
    </div>
    ${edges ? `<div class="palace-result-edges">${edges}</div>` : ''}
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

  if (!_palaceAvailable) {
    resultsEl.innerHTML = `<div class="empty-state" style="padding:1rem;">
      <div class="empty-title">Semantic recall requires Memory Palace</div>
      <div class="empty-subtitle" style="max-width:380px;line-height:1.6">
        Memory Palace is a separate MCP skill that adds semantic search, knowledge graphs, and auto-linking.<br><br>
        <strong>To set up:</strong><br>
        1. Install from <a href="https://github.com/jeffpierce/memory-palace" target="_blank" style="color:var(--accent)">github.com/jeffpierce/memory-palace</a><br>
        2. Run the MCP server alongside your gateway<br>
        3. Register it as a skill in your OpenClaw config<br><br>
        Meanwhile, use the <strong>Files</strong> tab to view and edit agent files directly.
      </div>
    </div>`;
    return;
  }

  try {
    const result = await gateway.chatSend('paw-memory', `Use memory_recall with query "${query.replace(/"/g, '\\"')}" and n_results 10. Return only raw JSON array.`, { thinking: 'minimal' });
    const text = typeof result === 'string' ? result : (result as { text?: string }).text ?? '';
    const jsonMatch = text.match(/\[[\s\S]*\]/);

    if (jsonMatch) {
      const memories: { type?: string; subject?: string; content?: string; score?: number; created_at?: string; tags?: string[]; edges?: { type?: string; target_subject?: string }[] }[] = JSON.parse(jsonMatch[0]);
      resultsEl.innerHTML = '';
      if (!memories.length) {
        if (emptyEl) emptyEl.style.display = 'flex';
        return;
      }
      for (const mem of memories) {
        resultsEl.appendChild(renderRecallCard(mem));
      }
    } else {
      resultsEl.innerHTML = `<div style="padding:1rem;color:var(--text-secondary)">${escHtml(text.slice(0, 500))}</div>`;
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
    const type = ($('palace-remember-type') as HTMLSelectElement | null)?.value ?? 'note';
    const subject = ($('palace-remember-subject') as HTMLInputElement | null)?.value.trim() ?? '';
    const content = ($('palace-remember-content') as HTMLTextAreaElement | null)?.value.trim() ?? '';
    const project = ($('palace-remember-project') as HTMLInputElement | null)?.value.trim() ?? '';
    const keywords = ($('palace-remember-keywords') as HTMLInputElement | null)?.value.trim() ?? '';
    const foundational = ($('palace-remember-foundational') as HTMLInputElement | null)?.checked ?? false;

    if (!subject || !content) {
      alert('Subject and content are required.');
      return;
    }

    if (!_palaceAvailable) {
      alert('Memory Palace skill not installed.\n\nInstall it from github.com/jeffpierce/memory-palace, run the MCP server, and register it as a skill in your OpenClaw config.\n\nMeanwhile, use the Files tab to manage agent files directly.');
      return;
    }

    btn.textContent = 'Saving…';
    (btn as HTMLButtonElement).disabled = true;

    try {
      const params = [
        `content: "${content.replace(/"/g, '\\"')}"`,
        `memory_type: "${type}"`,
        `subject: "${subject.replace(/"/g, '\\"')}"`,
        foundational ? 'foundational: true' : '',
        project ? `metadata: { project: "${project.replace(/"/g, '\\"')}" }` : '',
        keywords ? `tags: [${keywords.split(',').map(k => `"${k.trim()}"`).join(', ')}]` : '',
      ].filter(Boolean).join(', ');

      await gateway.chatSend('paw-memory', `Use memory_remember with ${params}. Confirm when saved.`, { thinking: 'minimal' });

      // Clear form
      if ($('palace-remember-subject') as HTMLInputElement) ($('palace-remember-subject') as HTMLInputElement).value = '';
      if ($('palace-remember-content') as HTMLTextAreaElement) ($('palace-remember-content') as HTMLTextAreaElement).value = '';
      if ($('palace-remember-project') as HTMLInputElement) ($('palace-remember-project') as HTMLInputElement).value = '';
      if ($('palace-remember-keywords') as HTMLInputElement) ($('palace-remember-keywords') as HTMLInputElement).value = '';
      if ($('palace-remember-foundational') as HTMLInputElement) ($('palace-remember-foundational') as HTMLInputElement).checked = false;

      alert('Memory saved!');
      await loadPalaceSidebar();
      await loadPalaceStats();
    } catch (e) {
      alert(`Save failed: ${e}`);
    } finally {
      btn.textContent = '💾 Save Memory';
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
        <div class="empty-title">Knowledge Graph</div>
        <div class="empty-subtitle">Requires <a href="https://github.com/jeffpierce/memory-palace" target="_blank" style="color:var(--accent)">Memory Palace</a> skill for graph visualization</div>
      `;
    }
    return;
  }

  if (emptyEl) { emptyEl.style.display = 'flex'; emptyEl.textContent = 'Loading graph…'; }

  try {
    // Get recent memories with their edges for graph data
    const result = await gateway.chatSend('paw-memory', 'Use memory_recent with limit 50. Include edges for each memory. Return only raw JSON array with fields: id, subject, type, edges (array of {target_id, target_subject, type}).', { thinking: 'minimal' });
    const text = typeof result === 'string' ? result : (result as { text?: string }).text ?? '';
    const jsonMatch = text.match(/\[[\s\S]*\]/);

    if (!jsonMatch) {
      if (emptyEl) { emptyEl.style.display = 'flex'; emptyEl.textContent = 'No graph data available.'; }
      return;
    }

    const memories: { id?: string; subject?: string; type?: string; edges?: { target_id?: string; target_subject?: string; type?: string }[] }[] = JSON.parse(jsonMatch[0]);
    if (!memories.length) {
      if (emptyEl) { emptyEl.style.display = 'flex'; emptyEl.textContent = 'No memories to visualize.'; }
      return;
    }

    if (emptyEl) emptyEl.style.display = 'none';

    // Render force-directed graph on canvas
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const rect = canvas.parentElement?.getBoundingClientRect();
    canvas.width = rect?.width ?? 600;
    canvas.height = rect?.height ?? 400;

    // Build node + edge lists
    const nodeMap = new Map<string, { x: number; y: number; vx: number; vy: number; subject: string; type: string }>();
    const edgeList: { from: string; to: string; type: string }[] = [];

    for (const mem of memories) {
      const id = mem.id ?? mem.subject ?? Math.random().toString();
      if (!nodeMap.has(id)) {
        nodeMap.set(id, {
          x: Math.random() * canvas.width,
          y: Math.random() * canvas.height,
          vx: 0, vy: 0,
          subject: mem.subject ?? '?',
          type: mem.type ?? 'note',
        });
      }
      for (const edge of mem.edges ?? []) {
        const targetId = edge.target_id ?? edge.target_subject ?? '';
        if (targetId) {
          if (!nodeMap.has(targetId)) {
            nodeMap.set(targetId, {
              x: Math.random() * canvas.width,
              y: Math.random() * canvas.height,
              vx: 0, vy: 0,
              subject: edge.target_subject ?? '?',
              type: 'note',
            });
          }
          edgeList.push({ from: id, to: targetId, type: edge.type ?? 'related' });
        }
      }
    }

    const nodes = Array.from(nodeMap.entries());
    const typeColors: Record<string, string> = {
      note: '#0073EA', fact: '#00CA72', decision: '#FDAB3D',
      procedure: '#E44258', concept: '#A25DDC', code: '#579BFC',
    };

    // Simple force simulation (50 iterations)
    for (let iter = 0; iter < 50; iter++) {
      // Repulsion between all nodes
      for (let i = 0; i < nodes.length; i++) {
        for (let j = i + 1; j < nodes.length; j++) {
          const a = nodes[i][1], b = nodes[j][1];
          let dx = a.x - b.x, dy = a.y - b.y;
          const dist = Math.sqrt(dx * dx + dy * dy) || 1;
          const force = 2000 / (dist * dist);
          dx *= force / dist; dy *= force / dist;
          a.vx += dx; a.vy += dy;
          b.vx -= dx; b.vy -= dy;
        }
      }
      // Attraction along edges
      for (const edge of edgeList) {
        const a = nodeMap.get(edge.from), b = nodeMap.get(edge.to);
        if (!a || !b) continue;
        let dx = b.x - a.x, dy = b.y - a.y;
        const dist = Math.sqrt(dx * dx + dy * dy) || 1;
        const force = (dist - 100) * 0.01;
        dx *= force / dist; dy *= force / dist;
        a.vx += dx; a.vy += dy;
        b.vx -= dx; b.vy -= dy;
      }
      // Center gravity
      for (const [, node] of nodes) {
        node.vx += (canvas.width / 2 - node.x) * 0.001;
        node.vy += (canvas.height / 2 - node.y) * 0.001;
      }
      // Apply velocities
      for (const [, node] of nodes) {
        node.x += node.vx * 0.3;
        node.y += node.vy * 0.3;
        node.vx *= 0.8;
        node.vy *= 0.8;
        // Clamp to bounds
        node.x = Math.max(30, Math.min(canvas.width - 30, node.x));
        node.y = Math.max(30, Math.min(canvas.height - 30, node.y));
      }
    }

    // Draw
    ctx.clearRect(0, 0, canvas.width, canvas.height);

    // Edges
    ctx.strokeStyle = '#D0D4E4';
    ctx.lineWidth = 1;
    for (const edge of edgeList) {
      const a = nodeMap.get(edge.from), b = nodeMap.get(edge.to);
      if (!a || !b) continue;
      ctx.beginPath();
      ctx.moveTo(a.x, a.y);
      ctx.lineTo(b.x, b.y);
      ctx.stroke();
    }

    // Nodes
    for (const [, node] of nodes) {
      const color = typeColors[node.type] ?? '#676879';
      ctx.beginPath();
      ctx.arc(node.x, node.y, 8, 0, Math.PI * 2);
      ctx.fillStyle = color;
      ctx.fill();
      ctx.strokeStyle = '#fff';
      ctx.lineWidth = 2;
      ctx.stroke();

      // Label
      ctx.fillStyle = '#323338';
      ctx.font = '11px Figtree, sans-serif';
      ctx.textAlign = 'center';
      const label = node.subject.length > 20 ? node.subject.slice(0, 18) + '…' : node.subject;
      ctx.fillText(label, node.x, node.y - 14);
    }
  } catch (e) {
    console.warn('Graph render failed:', e);
    if (emptyEl) { emptyEl.style.display = 'flex'; emptyEl.textContent = 'Failed to load graph data.'; }
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
  if (!_activeDocId || !wsConnected) { alert('Connect to gateway first'); return; }
  const body = ($('content-body') as HTMLTextAreaElement).value.trim();
  if (!body) return;
  const sessionKey = 'paw-create-' + _activeDocId;
  try {
    const result = await gateway.chatSend(sessionKey, `Improve this text. Return only the improved version, no explanations:\n\n${body}`);
    if (result.runId) {
      // Wait for response via events — simplified
      alert('AI improvement request sent. Check Chat for the response.');
    }
  } catch (e) {
    alert(`Failed: ${e}`);
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

async function loadResearchProjects() {
  const list = $('research-project-list');
  const empty = $('research-empty');
  const tabs = $('research-tabs');
  if (!list) return;

  const projects = await listProjects('research');
  list.innerHTML = '';

  if (!projects.length && !_activeResearchId) {
    if (empty) empty.style.display = 'flex';
    if (tabs) tabs.style.display = 'none';
    hideResearchPanels();
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

function hideResearchPanels() {
  [$('research-sources'), $('research-findings'), $('research-report')].forEach(p => {
    if (p) p.style.display = 'none';
  });
}

function openResearchProject(id: string) {
  _activeResearchId = id;
  const empty = $('research-empty');
  const tabs = $('research-tabs');
  if (empty) empty.style.display = 'none';
  if (tabs) tabs.style.display = 'flex';
  // Show sources panel by default
  switchResearchTab('sources');
  loadResearchProjects();
}

function switchResearchTab(tab: string) {
  document.querySelectorAll('.research-tab').forEach(t => t.classList.toggle('active', t.getAttribute('data-tab') === tab));
  hideResearchPanels();
  const panel = $(`research-${tab}`);
  if (panel) panel.style.display = '';
}

document.querySelectorAll('.research-tab').forEach(tab => {
  tab.addEventListener('click', () => {
    const t = tab.getAttribute('data-tab');
    if (t) switchResearchTab(t);
  });
});

async function createNewResearch() {
  const name = prompt('Research project name:');
  if (!name) return;
  const id = crypto.randomUUID();
  await saveProject({ id, name, space: 'research' });
  openResearchProject(id);
  loadResearchProjects();
}

$('research-new-project')?.addEventListener('click', createNewResearch);
$('research-create-first')?.addEventListener('click', createNewResearch);

$('research-add-source')?.addEventListener('click', () => {
  const url = prompt('Source URL:');
  if (!url) return;
  const list = $('research-sources-list');
  if (!list) return;
  const item = document.createElement('div');
  item.className = 'research-item';
  item.innerHTML = `
    <div class="research-item-title">${escHtml(url)}</div>
    <div class="research-item-meta">Added ${new Date().toLocaleDateString()}</div>
  `;
  list.appendChild(item);
});

$('research-agent-find')?.addEventListener('click', async () => {
  if (!_activeResearchId || !wsConnected) { alert('Connect to gateway first'); return; }
  const topic = prompt('What should the agent research?');
  if (!topic) return;
  alert('Research request sent. Check Chat for findings.');
  gateway.chatSend('paw-research-' + _activeResearchId, `Research this topic thoroughly and provide key findings with sources: ${topic}`).catch(console.warn);
});

$('research-delete-project')?.addEventListener('click', async () => {
  if (!_activeResearchId) return;
  if (!confirm('Delete this research project?')) return;
  await deleteProject(_activeResearchId);
  _activeResearchId = null;
  loadResearchProjects();
});

// ── Build IDE ──────────────────────────────────────────────────────────────
let _buildProjectId: string | null = null;
let _buildOpenFiles: { path: string; content: string }[] = [];
let _buildActiveFile: string | null = null;

$('build-new-project')?.addEventListener('click', async () => {
  const name = prompt('Project name:');
  if (!name) return;
  const id = crypto.randomUUID();
  await saveProject({ id, name, space: 'build' });
  _buildProjectId = id;
  loadBuildProject();
});

async function loadBuildProject() {
  if (!_buildProjectId) return;
  const fileList = $('build-file-list');
  const empty = $('build-empty');
  /* editor loaded on demand */

  if (empty) empty.style.display = 'none';

  // For now, show project files from DB (simplified)
  if (fileList) {
    fileList.innerHTML = '';
    // TODO: Load actual files from project_files table
    const placeholder = document.createElement('div');
    placeholder.className = 'ide-file-empty';
    placeholder.textContent = 'Create files with the + button';
    fileList.appendChild(placeholder);
  }
}

$('build-add-file')?.addEventListener('click', () => {
  if (!_buildProjectId) { alert('Create a project first'); return; }
  const filename = prompt('File name (e.g. index.html):');
  if (!filename) return;
  const _fl = $('build-file-list'); void _fl;
  const editor = $('build-code-editor') as HTMLTextAreaElement;
  const empty = $('build-empty');

  _buildOpenFiles.push({ path: filename, content: '' });
  _buildActiveFile = filename;

  if (empty) empty.style.display = 'none';
  if (editor) { editor.style.display = ''; editor.value = ''; }

  updateBuildTabs();
  updateBuildFileList();
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
    tab.querySelector('.ide-tab-close')?.addEventListener('click', (e) => {
      e.stopPropagation();
      _buildOpenFiles = _buildOpenFiles.filter(x => x.path !== f.path);
      if (_buildActiveFile === f.path) {
        _buildActiveFile = _buildOpenFiles[0]?.path ?? null;
        const editor = $('build-code-editor') as HTMLTextAreaElement;
        if (editor) editor.value = _buildActiveFile ? _buildOpenFiles[0].content : '';
        if (!_buildActiveFile) { editor.style.display = 'none'; const empty = $('build-empty'); if (empty) empty.style.display = 'flex'; }
      }
      updateBuildTabs();
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

// Save file content as user types
$('build-code-editor')?.addEventListener('input', () => {
  if (!_buildActiveFile) return;
  const editor = $('build-code-editor') as HTMLTextAreaElement;
  const file = _buildOpenFiles.find(f => f.path === _buildActiveFile);
  if (file && editor) file.content = editor.value;
});

// Build chat — send to agent in build context
$('build-chat-send')?.addEventListener('click', async () => {
  const input = $('build-chat-input') as HTMLTextAreaElement;
  const messages = $('build-chat-messages');
  if (!input?.value.trim() || !wsConnected) return;

  const userMsg = input.value.trim();
  input.value = '';

  // Show user message
  const userDiv = document.createElement('div');
  userDiv.className = 'message user';
  userDiv.innerHTML = `<div class="message-content">${escHtml(userMsg)}</div>`;
  messages?.appendChild(userDiv);

  // Provide context about open files
  let context = userMsg;
  if (_buildOpenFiles.length) {
    const fileContext = _buildOpenFiles.map(f => `--- ${f.path} ---\n${f.content}`).join('\n\n');
    context = `[Build context]\nOpen files:\n${fileContext}\n\n[User instruction]: ${userMsg}`;
  }

  try {
    const sessionKey = _buildProjectId ? `paw-build-${_buildProjectId}` : 'paw-build';
    await gateway.chatSend(sessionKey, context);
    // Response will come via events
    const agentDiv = document.createElement('div');
    agentDiv.className = 'message assistant';
    agentDiv.innerHTML = `<div class="message-content">Request sent — check Chat view for the full response.</div>`;
    messages?.appendChild(agentDiv);
  } catch (e) {
    const errDiv = document.createElement('div');
    errDiv.className = 'message assistant';
    errDiv.innerHTML = `<div class="message-content">Error: ${escHtml(e instanceof Error ? e.message : String(e))}</div>`;
    messages?.appendChild(errDiv);
  }
});

// ── Helpers ────────────────────────────────────────────────────────────────
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
