// Claw Desktop - Main Application

import { gateway, type Session, type CronJob, type Skill, type Node } from './gateway';

// Tauri API types
interface TauriWindow {
  __TAURI__?: {
    core: {
      invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
    };
    event: {
      listen: <T>(event: string, handler: (event: { payload: T }) => void) => Promise<() => void>;
    };
  };
}

const tauriWindow = window as unknown as TauriWindow;
const invoke = tauriWindow.__TAURI__?.core?.invoke;
const listen = tauriWindow.__TAURI__?.event?.listen;

interface Config {
  configured: boolean;
  gateway: {
    url: string;
    token: string;
  };
}

interface Message {
  role: 'user' | 'assistant';
  content: string;
  timestamp: Date;
}

interface InstallProgress {
  stage: string;
  percent: number;
  message: string;
}

// State
let config: Config = {
  configured: false,
  gateway: {
    url: 'http://localhost:5757',
    token: '',
  },
};

let messages: Message[] = [];
let isLoading = false;
let currentRunId: string | null = null;

// DOM Elements
const allViews = [
  'setup-view', 'install-view', 'manual-setup-view', 'chat-view',
  'channels-view', 'sessions-view', 'cron-view', 'skills-view',
  'nodes-view', 'config-view', 'logs-view'
].map(id => document.getElementById(id)).filter(Boolean) as HTMLElement[];

const statusDot = document.getElementById('status-dot');
const statusText = document.getElementById('status-text');
const chatMessages = document.getElementById('chat-messages');
const chatEmpty = document.getElementById('chat-empty');
const chatInput = document.getElementById('chat-input') as HTMLTextAreaElement;
const chatSend = document.getElementById('chat-send') as HTMLButtonElement;
const stopBtn = document.getElementById('stop-btn') as HTMLButtonElement;
const modelLabel = document.getElementById('model-label');

// Navigation
document.querySelectorAll('.nav-item').forEach((item) => {
  item.addEventListener('click', () => {
    const view = item.getAttribute('data-view');
    if (view && config.configured) {
      switchView(view);
      loadViewData(view);
    }
  });
});

function switchView(viewName: string) {
  document.querySelectorAll('.nav-item').forEach((item) => {
    item.classList.toggle('active', item.getAttribute('data-view') === viewName);
  });

  allViews.forEach((v) => v.classList.remove('active'));
  document.getElementById(`${viewName}-view`)?.classList.add('active');
}

function showView(viewId: string) {
  allViews.forEach((v) => v.classList.remove('active'));
  document.getElementById(viewId)?.classList.add('active');
}

// Load data for views
async function loadViewData(view: string) {
  switch (view) {
    case 'sessions':
      await loadSessions();
      break;
    case 'cron':
      await loadCronJobs();
      break;
    case 'skills':
      await loadSkills();
      break;
    case 'nodes':
      await loadNodes();
      break;
    case 'config':
      await loadConfig();
      break;
    case 'channels':
      await loadChannels();
      break;
  }
}

// Sessions
async function loadSessions() {
  const container = document.getElementById('sessions-list');
  if (!container) return;
  
  container.innerHTML = '<div class="loading-state">Loading...</div>';
  
  try {
    const sessions = await gateway.getSessions();
    
    if (sessions.length === 0) {
      container.innerHTML = '<div class="loading-state">No active sessions</div>';
      return;
    }
    
    container.innerHTML = sessions.map((s: Session) => `
      <div class="item-card" data-key="${s.key}">
        <div class="item-icon ${s.kind === 'main' ? 'active' : ''}">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>
          </svg>
        </div>
        <div class="item-content">
          <div class="item-title">${s.key}</div>
          <div class="item-desc">${s.kind} · ${s.model || 'default model'}</div>
        </div>
      </div>
    `).join('');
  } catch (e) {
    container.innerHTML = `<div class="loading-state">Error: ${e}</div>`;
  }
}

// Cron Jobs
async function loadCronJobs() {
  const container = document.getElementById('cron-list');
  if (!container) return;
  
  container.innerHTML = '<div class="loading-state">Loading...</div>';
  
  try {
    const jobs = await gateway.getCronJobs();
    
    if (jobs.length === 0) {
      container.innerHTML = '<div class="loading-state">No scheduled tasks</div>';
      return;
    }
    
    container.innerHTML = jobs.map((j: CronJob) => `
      <div class="item-card" data-id="${j.id}">
        <div class="item-icon ${j.enabled ? 'active' : ''}">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <circle cx="12" cy="12" r="10"/>
            <polyline points="12 6 12 12 16 14"/>
          </svg>
        </div>
        <div class="item-content">
          <div class="item-title">${j.name || j.id}</div>
          <div class="item-desc">${j.nextRun ? `Next: ${new Date(j.nextRun).toLocaleString()}` : 'Not scheduled'}</div>
        </div>
        <div class="item-toggle ${j.enabled ? 'active' : ''}" data-job-id="${j.id}"></div>
      </div>
    `).join('');
    
    // Toggle handlers
    container.querySelectorAll('.item-toggle').forEach(toggle => {
      toggle.addEventListener('click', async (e) => {
        e.stopPropagation();
        const jobId = (toggle as HTMLElement).dataset.jobId;
        const isActive = toggle.classList.contains('active');
        if (jobId) {
          await gateway.updateCronJob(jobId, { enabled: !isActive });
          toggle.classList.toggle('active');
        }
      });
    });
  } catch (e) {
    container.innerHTML = `<div class="loading-state">Error: ${e}</div>`;
  }
}

// Skills
async function loadSkills() {
  const container = document.getElementById('skills-list');
  if (!container) return;
  
  container.innerHTML = '<div class="loading-state">Loading...</div>';
  
  try {
    const skills = await gateway.getSkills();
    
    if (skills.length === 0) {
      container.innerHTML = '<div class="loading-state">No skills available</div>';
      return;
    }
    
    container.innerHTML = skills.map((s: Skill) => `
      <div class="item-card" data-id="${s.id}">
        <div class="item-icon ${s.enabled ? 'active' : ''}">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/>
          </svg>
        </div>
        <div class="item-content">
          <div class="item-title">${s.name}</div>
          <div class="item-desc">${s.description || 'No description'}</div>
        </div>
        <div class="item-toggle ${s.enabled ? 'active' : ''}" data-skill-id="${s.id}"></div>
      </div>
    `).join('');
    
    // Toggle handlers
    container.querySelectorAll('.item-toggle').forEach(toggle => {
      toggle.addEventListener('click', async (e) => {
        e.stopPropagation();
        const skillId = (toggle as HTMLElement).dataset.skillId;
        const isActive = toggle.classList.contains('active');
        if (skillId) {
          if (isActive) {
            await gateway.disableSkill(skillId);
          } else {
            await gateway.enableSkill(skillId);
          }
          toggle.classList.toggle('active');
        }
      });
    });
  } catch (e) {
    container.innerHTML = `<div class="loading-state">Error: ${e}</div>`;
  }
}

// Nodes
async function loadNodes() {
  const container = document.getElementById('nodes-list');
  if (!container) return;
  
  container.innerHTML = '<div class="loading-state">Loading...</div>';
  
  try {
    const nodes = await gateway.getNodes();
    
    if (nodes.length === 0) {
      container.innerHTML = '<div class="loading-state">No devices paired</div>';
      return;
    }
    
    container.innerHTML = nodes.map((n: Node) => `
      <div class="item-card">
        <div class="item-icon ${n.connected ? 'active' : ''}">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
            <rect x="5" y="2" width="14" height="20" rx="2"/>
            <line x1="12" y1="18" x2="12.01" y2="18"/>
          </svg>
        </div>
        <div class="item-content">
          <div class="item-title">${n.name}</div>
          <div class="item-desc">${n.connected ? 'Connected' : 'Offline'}${n.caps ? ` · ${n.caps.join(', ')}` : ''}</div>
        </div>
      </div>
    `).join('');
  } catch (e) {
    container.innerHTML = `<div class="loading-state">Error: ${e}</div>`;
  }
}

// Config
async function loadConfig() {
  const editor = document.getElementById('config-editor') as HTMLTextAreaElement;
  if (!editor) return;
  
  try {
    const cfg = await gateway.getConfig();
    editor.value = JSON.stringify(cfg, null, 2);
  } catch (e) {
    editor.value = `// Error loading config: ${e}`;
  }
}

// Channels
async function loadChannels() {
  try {
    const channels = await gateway.getChannels();
    channels.forEach(ch => {
      const card = document.querySelector(`.channel-card[data-type="${ch.type}"]`);
      if (card) {
        const status = card.querySelector('.channel-status');
        if (status) {
          status.textContent = ch.linked ? 'Connected' : ch.status;
          status.classList.toggle('connected', !!ch.linked);
        }
      }
    });
  } catch {
    // Channels API might not exist
  }
}

// Setup handlers
document.getElementById('setup-detect')?.addEventListener('click', async () => {
  if (statusText) statusText.textContent = 'Detecting...';
  
  try {
    const installed = invoke ? await invoke<boolean>('check_openclaw_installed') : false;
    
    if (installed) {
      const token = invoke ? await invoke<string | null>('get_gateway_token') : null;
      
      if (token) {
        config.configured = true;
        config.gateway.url = 'http://localhost:5757';
        config.gateway.token = token;
        saveConfig();
        
        if (invoke) await invoke('start_gateway');
        await new Promise(resolve => setTimeout(resolve, 2000));
        
        await connectGateway();
        return;
      }
    }
    
    showView('install-view');
  } catch (error) {
    console.error('Detection error:', error);
    showView('install-view');
  }
});

document.getElementById('setup-manual')?.addEventListener('click', () => {
  showView('manual-setup-view');
});

document.getElementById('setup-new')?.addEventListener('click', () => {
  showView('install-view');
});

document.getElementById('gateway-back')?.addEventListener('click', () => {
  showView('setup-view');
});

document.getElementById('install-back')?.addEventListener('click', () => {
  showView('setup-view');
});

// Install
document.getElementById('start-install')?.addEventListener('click', async () => {
  const progressBar = document.getElementById('install-progress-bar');
  const progressText = document.getElementById('install-progress-text');
  const installBtn = document.getElementById('start-install') as HTMLButtonElement;
  
  if (installBtn) {
    installBtn.disabled = true;
    installBtn.textContent = 'Installing...';
  }
  
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
      config.configured = true;
      config.gateway.url = 'http://localhost:5757';
      config.gateway.token = token;
      saveConfig();
      
      await new Promise(resolve => setTimeout(resolve, 1000));
      await connectGateway();
    }
  } catch (error) {
    console.error('Install error:', error);
    if (progressText) progressText.textContent = `Error: ${error}`;
    if (installBtn) {
      installBtn.disabled = false;
      installBtn.textContent = 'Retry';
    }
  }
});

// Gateway form
document.getElementById('gateway-form')?.addEventListener('submit', async (e) => {
  e.preventDefault();
  
  const url = (document.getElementById('gateway-url') as HTMLInputElement).value;
  const token = (document.getElementById('gateway-token') as HTMLInputElement).value;
  
  config.gateway = { url, token };
  config.configured = true;
  saveConfig();
  
  await connectGateway();
});

// Connect to gateway
async function connectGateway() {
  try {
    await gateway.connect(config.gateway);
    
    statusDot?.classList.add('connected');
    statusDot?.classList.remove('error');
    if (statusText) statusText.textContent = 'Connected';
    
    // Listen for chat events
    gateway.on('chat', handleChatEvent);
    gateway.on('disconnected', () => {
      statusDot?.classList.remove('connected');
      statusDot?.classList.add('error');
      if (statusText) statusText.textContent = 'Disconnected';
    });
    
    switchView('chat');
  } catch (error) {
    console.error('Connection error:', error);
    statusDot?.classList.remove('connected');
    statusDot?.classList.add('error');
    if (statusText) statusText.textContent = 'Connection failed';
    
    // Try simple HTTP health check
    try {
      const response = await fetch(`${config.gateway.url}/health`, { signal: AbortSignal.timeout(2000) });
      if (response.ok) {
        statusDot?.classList.add('connected');
        if (statusText) statusText.textContent = 'Connected (HTTP)';
        switchView('chat');
      }
    } catch {
      showView('setup-view');
    }
  }
}

function handleChatEvent(event: unknown) {
  const ev = event as { type?: string; content?: string; runId?: string };
  
  if (ev.type === 'content' && ev.content) {
    // Streaming content
    updateStreamingMessage(ev.content);
  } else if (ev.type === 'done') {
    finalizeStreamingMessage();
  }
}

// Config persistence
function saveConfig() {
  localStorage.setItem('claw-config', JSON.stringify(config));
}

function loadAppConfig() {
  const saved = localStorage.getItem('claw-config');
  if (saved) {
    try {
      config = JSON.parse(saved);
    } catch {
      // Invalid config
    }
  }
}

// Chat functionality
chatSend?.addEventListener('click', sendMessage);
chatInput?.addEventListener('keydown', (e) => {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault();
    sendMessage();
  }
});

chatInput?.addEventListener('input', () => {
  if (chatInput) {
    chatInput.style.height = 'auto';
    chatInput.style.height = Math.min(chatInput.scrollHeight, 120) + 'px';
  }
});

document.getElementById('new-chat-btn')?.addEventListener('click', () => {
  messages = [];
  renderMessages();
});

stopBtn?.addEventListener('click', async () => {
  try {
    await gateway.abortChat();
    isLoading = false;
    stopBtn.style.display = 'none';
    if (chatSend) chatSend.disabled = false;
  } catch (e) {
    console.error('Failed to abort:', e);
  }
});

// Save config
document.getElementById('save-config-btn')?.addEventListener('click', async () => {
  const editor = document.getElementById('config-editor') as HTMLTextAreaElement;
  if (!editor) return;
  
  try {
    const newConfig = JSON.parse(editor.value);
    await gateway.patchConfig(newConfig);
    alert('Configuration saved!');
  } catch (e) {
    alert(`Error: ${e}`);
  }
});

// Refresh buttons
document.getElementById('refresh-sessions')?.addEventListener('click', () => loadSessions());
document.getElementById('refresh-skills')?.addEventListener('click', () => loadSkills());
document.getElementById('refresh-nodes')?.addEventListener('click', () => loadNodes());

async function sendMessage() {
  const content = chatInput?.value.trim();
  if (!content || isLoading) return;

  addMessage({ role: 'user', content, timestamp: new Date() });
  if (chatInput) chatInput.value = '';
  if (chatInput) chatInput.style.height = 'auto';

  isLoading = true;
  if (chatSend) chatSend.disabled = true;
  if (stopBtn) stopBtn.style.display = 'flex';
  if (modelLabel) modelLabel.textContent = 'Thinking...';
  showLoading();

  try {
    // Try gateway WebSocket first
    if (gateway.isConnected()) {
      const result = await gateway.sendChat(content);
      currentRunId = result.runId;
      // Response will come via events
    } else {
      // Fallback to HTTP
      const response = await callGatewayHttp(content);
      hideLoading();
      addMessage({ role: 'assistant', content: response, timestamp: new Date() });
      isLoading = false;
      if (chatSend) chatSend.disabled = false;
      if (stopBtn) stopBtn.style.display = 'none';
      if (modelLabel) modelLabel.textContent = 'Ready';
    }
  } catch (error) {
    console.error('Error:', error);
    hideLoading();
    addMessage({
      role: 'assistant',
      content: `Error: ${error instanceof Error ? error.message : 'Failed to get response'}`,
      timestamp: new Date(),
    });
    isLoading = false;
    if (chatSend) chatSend.disabled = false;
    if (stopBtn) stopBtn.style.display = 'none';
    if (modelLabel) modelLabel.textContent = 'Ready';
  }
}

let streamingContent = '';

function updateStreamingMessage(content: string) {
  streamingContent += content;
  
  // Update or create streaming message
  let streamingMsg = document.getElementById('streaming-message');
  if (!streamingMsg) {
    hideLoading();
    streamingMsg = document.createElement('div');
    streamingMsg.className = 'message assistant';
    streamingMsg.id = 'streaming-message';
    streamingMsg.innerHTML = '<div class="message-content"></div>';
    chatMessages?.appendChild(streamingMsg);
  }
  
  const msgContent = streamingMsg.querySelector('.message-content');
  if (msgContent) msgContent.textContent = streamingContent;
  
  if (chatMessages) chatMessages.scrollTop = chatMessages.scrollHeight;
}

function finalizeStreamingMessage() {
  if (streamingContent) {
    addMessage({ role: 'assistant', content: streamingContent, timestamp: new Date() });
  }
  document.getElementById('streaming-message')?.remove();
  streamingContent = '';
  isLoading = false;
  currentRunId = null;
  if (chatSend) chatSend.disabled = false;
  if (stopBtn) stopBtn.style.display = 'none';
  if (modelLabel) modelLabel.textContent = 'Ready';
}

function addMessage(message: Message) {
  messages.push(message);
  renderMessages();
}

function renderMessages() {
  if (!chatMessages || !chatEmpty) return;
  
  if (messages.length === 0) {
    chatEmpty.style.display = 'flex';
    return;
  }

  chatEmpty.style.display = 'none';

  const existingMessages = chatMessages.querySelectorAll('.message:not(#streaming-message):not(#loading-message)');
  existingMessages.forEach((m) => m.remove());

  messages.forEach((msg) => {
    const div = document.createElement('div');
    div.className = `message ${msg.role}`;

    const content = document.createElement('div');
    content.className = 'message-content';
    content.textContent = msg.content;

    const time = document.createElement('div');
    time.className = 'message-time';
    time.textContent = msg.timestamp.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });

    div.appendChild(content);
    div.appendChild(time);
    chatMessages.appendChild(div);
  });

  chatMessages.scrollTop = chatMessages.scrollHeight;
}

function showLoading() {
  if (chatEmpty) chatEmpty.style.display = 'none';
  const loadingDiv = document.createElement('div');
  loadingDiv.className = 'message assistant';
  loadingDiv.id = 'loading-message';
  loadingDiv.innerHTML = `
    <div class="message-content">
      <div class="loading-dots">
        <span></span>
        <span></span>
        <span></span>
      </div>
    </div>
  `;
  chatMessages?.appendChild(loadingDiv);
  if (chatMessages) chatMessages.scrollTop = chatMessages.scrollHeight;
}

function hideLoading() {
  document.getElementById('loading-message')?.remove();
}

async function callGatewayHttp(userMessage: string): Promise<string> {
  const response = await fetch(`${config.gateway.url}/webchat/send`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ message: userMessage }),
  });

  if (!response.ok) {
    throw new Error(`Gateway error: ${response.status}`);
  }

  const data = await response.json();
  return data.response || data.message || data.content || 'No response';
}

// Initialize
document.addEventListener('DOMContentLoaded', async () => {
  loadAppConfig();

  if (config.configured) {
    await connectGateway();
  } else {
    showView('setup-view');
  }
});
