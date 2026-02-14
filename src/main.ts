// Paw — Main Application
// Wires OpenClaw gateway (WebSocket protocol v3) to the UI

import type { AppConfig, Message, InstallProgress, ChatMessage, Session } from './types';
import { setGatewayConfig, probeHealth } from './api';
import { gateway } from './gateway';

// ── Global error handlers ──────────────────────────────────────────────────
window.addEventListener('unhandledrejection', (event) => {
  console.error('Unhandled promise rejection:', event.reason);
  event.preventDefault();
});
window.addEventListener('error', (event) => {
  console.error('Uncaught error:', event.error);
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
      case 'chat': loadSessions(); break;
      case 'channels': loadChannels(); break;
      case 'automations': loadCron(); break;
      case 'skills': loadSkills(); break;
      case 'foundry': loadModels(); break;
      case 'memory': loadMemory(); break;
      case 'settings': syncSettingsForm(); loadGatewayConfig(); break;
      default: break;
    }
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
  if (statusText) statusText.textContent = 'Disconnected';
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
    if (currentSessionKey) await loadChatHistory(currentSessionKey);
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

  // Prepare streaming UI — show an empty assistant bubble that will fill with deltas
  _streamingContent = '';
  _streamingRunId = null;
  showStreamingMessage();

  try {
    const sessionKey = currentSessionKey ?? 'default';
    const result = await gateway.chatSend(sessionKey, content);

    // Streaming may have already populated the bubble via agent events.
    // Use the final result as the canonical content (in case deltas were missed).
    const finalText = extractContent(result.text) || extractContent(result.response) || extractContent((result as unknown as Record<string, unknown>).content);
    finalizeStreaming(finalText || _streamingContent, result.toolCalls);

    if (result.sessionKey) currentSessionKey = result.sessionKey;
    loadSessions();
  } catch (error) {
    console.error('Chat error:', error);
    const errText = _streamingContent
      ? _streamingContent + '\n\n(Error: connection interrupted)'
      : `Error: ${error instanceof Error ? error.message : 'Failed to get response'}`;
    finalizeStreaming(errText);
  } finally {
    isLoading = false;
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
  if (chatMessages) chatMessages.scrollTop = chatMessages.scrollHeight;
}

/** Append a text delta to the streaming bubble */
function appendStreamingDelta(text: string) {
  _streamingContent += text;
  if (_streamingEl) {
    _streamingEl.textContent = _streamingContent;
    if (chatMessages) chatMessages.scrollTop = chatMessages.scrollHeight;
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
  if (messages.length === 0) {
    if (chatEmpty) chatEmpty.style.display = 'flex';
    chatMessages.querySelectorAll('.message').forEach(m => m.remove());
    return;
  }
  if (chatEmpty) chatEmpty.style.display = 'none';
  chatMessages.querySelectorAll('.message').forEach(m => m.remove());

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

    chatMessages.appendChild(div);
  }
  chatMessages.scrollTop = chatMessages.scrollHeight;
}

// Listen for streaming agent events — update chat bubble in real-time
gateway.on('agent', (payload: unknown) => {
  try {
    const evt = payload as import('./types').AgentEvent;
    // Only process events for the current streaming run
    if (!isLoading) return;
    if (_streamingRunId && evt.runId && evt.runId !== _streamingRunId) return;

    switch (evt.type) {
      case 'start':
        _streamingRunId = evt.runId ?? null;
        break;
      case 'delta':
        if (evt.content) {
          appendStreamingDelta(evt.content);
        }
        break;
      case 'tool-start':
        if (evt.tool && _streamingEl) {
          // Show tool use indicator inline
          appendStreamingDelta(`\n\n> Using ${evt.tool}...`);
        }
        break;
      case 'tool-done':
        // Could show tool result — for now just note completion
        break;
      case 'error':
        if (evt.error) {
          appendStreamingDelta(`\n\nError: ${evt.error}`);
        }
        break;
      // 'done' is handled by the chatSend response resolving
    }
  } catch (e) {
    console.warn('[main] Agent event handler error:', e);
  }
});

// ── Channels ───────────────────────────────────────────────────────────────
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
      const card = document.createElement('div');
      card.className = 'list-item';
      const statusClass = ch.linked ? 'connected' : (ch.configured ? 'warning' : 'muted');
      const statusLabel = ch.linked ? 'Linked' : (ch.configured ? 'Configured' : 'Not set up');
      card.innerHTML = `
        <div class="list-item-header">
          <span class="list-item-title">${escHtml(String(id))}</span>
          <span class="status-badge ${statusClass}">${statusLabel}</span>
        </div>
        ${ch.accounts ? `<div class="list-item-meta">${Object.keys(ch.accounts).length} account(s)</div>` : ''}
      `;
      list.appendChild(card);
    }
  } catch (e) {
    console.warn('Channels load failed:', e);
    if (loading) loading.style.display = 'none';
    if (empty) empty.style.display = 'flex';
  }
}
$('refresh-channels-btn')?.addEventListener('click', () => loadChannels());

// ── Automations / Cron ─────────────────────────────────────────────────────
async function loadCron() {
  const list = $('cron-list');
  const empty = $('cron-empty');
  const loading = $('cron-loading');
  if (!wsConnected || !list) return;

  if (loading) loading.style.display = '';
  if (empty) empty.style.display = 'none';
  list.innerHTML = '';

  try {
    const result = await gateway.cronList();
    if (loading) loading.style.display = 'none';

    const jobs = result.jobs ?? [];
    if (!jobs.length) {
      if (empty) empty.style.display = 'flex';
      return;
    }

    for (const job of jobs) {
      const card = document.createElement('div');
      card.className = 'list-item';
      const scheduleStr = typeof job.schedule === 'string' ? job.schedule : (job.schedule?.type ?? '');
      card.innerHTML = `
        <div class="list-item-header">
          <span class="list-item-title">${escHtml(job.label ?? job.id)}</span>
          <span class="status-badge ${job.enabled ? 'connected' : 'muted'}">${job.enabled ? 'Active' : 'Paused'}</span>
        </div>
        <div class="list-item-meta">${escHtml(scheduleStr)} ${job.prompt ? '— ' + escHtml(String(job.prompt)) : ''}</div>
        <div class="list-item-actions">
          <button class="btn btn-ghost btn-sm cron-run" data-id="${escAttr(job.id)}">Run Now</button>
          <button class="btn btn-ghost btn-sm cron-toggle" data-id="${escAttr(job.id)}" data-enabled="${job.enabled}">${job.enabled ? 'Pause' : 'Enable'}</button>
        </div>
      `;
      list.appendChild(card);
    }

    list.querySelectorAll('.cron-run').forEach(btn => {
      btn.addEventListener('click', async () => {
        const id = (btn as HTMLElement).dataset.id!;
        try { await gateway.cronRun(id); alert('Job triggered!'); }
        catch (e) { alert(`Failed: ${e}`); }
      });
    });
    list.querySelectorAll('.cron-toggle').forEach(btn => {
      btn.addEventListener('click', async () => {
        const id = (btn as HTMLElement).dataset.id!;
        const enabled = (btn as HTMLElement).dataset.enabled === 'true';
        try {
          await gateway.cronUpdate(id, { enabled: !enabled });
          loadCron();
        } catch (e) { alert(`Failed: ${e}`); }
      });
    });
  } catch (e) {
    console.warn('Cron load failed:', e);
    if (loading) loading.style.display = 'none';
    if (empty) empty.style.display = 'flex';
  }
}
$('add-cron-btn')?.addEventListener('click', () => {
  const label = prompt('Job name:');
  if (!label) return;
  const schedule = prompt('Cron schedule (e.g. "0 * * * *"):');
  if (!schedule) return;
  const prompt_ = prompt('Task prompt:');
  if (!prompt_) return;
  gateway.cronAdd({ label, schedule, prompt: prompt_, enabled: true })
    .then(() => loadCron())
    .catch(e => alert(`Failed: ${e}`));
});

// ── Skills ─────────────────────────────────────────────────────────────────
async function loadSkills() {
  const list = $('skills-list');
  const empty = $('skills-empty');
  const loading = $('skills-loading');
  if (!wsConnected || !list) return;

  if (loading) loading.style.display = '';
  if (empty) empty.style.display = 'none';
  list.innerHTML = '';

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
      card.className = 'list-item';
      card.innerHTML = `
        <div class="list-item-header">
          <span class="list-item-title">${escHtml(skill.name)}</span>
          <span class="status-badge ${skill.installed ? 'connected' : 'muted'}">${skill.installed ? 'Installed' : 'Available'}</span>
        </div>
        <div class="list-item-meta">${escHtml(skill.description ?? '')}</div>
      `;
      list.appendChild(card);
    }
  } catch (e) {
    console.warn('Skills load failed:', e);
    if (loading) loading.style.display = 'none';
    if (empty) empty.style.display = 'flex';
  }
}
$('refresh-skills-btn')?.addEventListener('click', () => loadSkills());

// ── Models / Foundry ───────────────────────────────────────────────────────
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
    if (!models.length) {
      if (empty) empty.style.display = 'flex';
      return;
    }

    for (const model of models) {
      const card = document.createElement('div');
      card.className = 'list-item';
      card.innerHTML = `
        <div class="list-item-header">
          <span class="list-item-title">${escHtml(model.name ?? model.id)}</span>
          <span class="list-item-tag">${escHtml(model.provider ?? '')}</span>
        </div>
        <div class="list-item-meta">${model.contextWindow ? `Context: ${model.contextWindow.toLocaleString()} tokens` : ''} ${model.reasoning ? '• Reasoning' : ''}</div>
      `;
      list.appendChild(card);
    }
  } catch (e) {
    console.warn('Models load failed:', e);
    if (loading) loading.style.display = 'none';
    if (empty) empty.style.display = 'flex';
  }
}
$('refresh-models-btn')?.addEventListener('click', () => loadModels());

// ── Memory / Agent Files ───────────────────────────────────────────────────
async function loadMemory() {
  const list = $('memory-list');
  const empty = $('memory-empty');
  const loading = $('memory-loading');
  const editor = $('memory-editor');
  if (!wsConnected || !list) return;

  if (loading) loading.style.display = '';
  if (empty) empty.style.display = 'none';
  if (editor) editor.style.display = 'none';
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
  if (!editor || !content) return;

  editor.style.display = '';
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
