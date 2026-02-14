// Claw Desktop - Main Application

import { getGatewayStatus, setGatewayConfig } from './api';

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

// DOM Elements
const setupView = document.getElementById('setup-view')!;
const manualSetupView = document.getElementById('manual-setup-view')!;
const chatView = document.getElementById('chat-view')!;
const agentsView = document.getElementById('agents-view')!;
const channelsView = document.getElementById('channels-view')!;
const memoryView = document.getElementById('memory-view')!;
const cronView = document.getElementById('cron-view')!;
const settingsView = document.getElementById('settings-view')!;
const statusDot = document.getElementById('status-dot')!;
const statusText = document.getElementById('status-text')!;
const chatMessages = document.getElementById('chat-messages')!;
const chatEmpty = document.getElementById('chat-empty')!;
const chatInput = document.getElementById('chat-input') as HTMLTextAreaElement;
const chatSend = document.getElementById('chat-send') as HTMLButtonElement;

const allViews = [setupView, manualSetupView, chatView, agentsView, channelsView, memoryView, cronView, settingsView];

// Navigation
document.querySelectorAll('.nav-item').forEach((item) => {
  item.addEventListener('click', () => {
    const view = item.getAttribute('data-view');
    if (view) switchView(view);
  });
});

function switchView(viewName: string) {
  // Don't allow navigation if not configured (except settings)
  if (!config.configured && viewName !== 'settings') {
    return;
  }

  // Update nav
  document.querySelectorAll('.nav-item').forEach((item) => {
    item.classList.toggle('active', item.getAttribute('data-view') === viewName);
  });

  // Hide all views
  allViews.forEach((v) => v.classList.remove('active'));

  // Show selected view
  switch (viewName) {
    case 'chat':
      chatView.classList.add('active');
      break;
    case 'agents':
      agentsView.classList.add('active');
      break;
    case 'channels':
      channelsView.classList.add('active');
      break;
    case 'memory':
      memoryView.classList.add('active');
      break;
    case 'cron':
      cronView.classList.add('active');
      break;
    case 'settings':
      settingsView.classList.add('active');
      syncSettingsForm();
      break;
  }
}

function showSetup() {
  allViews.forEach((v) => v.classList.remove('active'));
  setupView.classList.add('active');
}

function showManualSetup() {
  allViews.forEach((v) => v.classList.remove('active'));
  manualSetupView.classList.add('active');
}

// Setup handlers
document.getElementById('setup-detect')?.addEventListener('click', async () => {
  statusText.textContent = 'Detecting...';
  
  // Try to find config file and connect
  const status = await getGatewayStatus();
  
  if (status.running) {
    // Gateway found! Try to load token from default location
    config.configured = true;
    config.gateway.url = 'http://localhost:5757';
    saveConfig();
    
    statusDot.classList.add('connected');
    statusText.textContent = 'Connected';
    
    switchView('chat');
  } else {
    alert('No gateway detected. Make sure OpenClaw is running, or use Manual Setup.');
  }
});

document.getElementById('setup-manual')?.addEventListener('click', () => {
  showManualSetup();
});

document.getElementById('setup-new')?.addEventListener('click', () => {
  // Open OpenClaw docs in browser
  window.open('https://docs.openclaw.ai/getting-started', '_blank');
});

document.getElementById('gateway-back')?.addEventListener('click', () => {
  showSetup();
});

// Gateway form
document.getElementById('gateway-form')?.addEventListener('submit', async (e) => {
  e.preventDefault();
  
  const url = (document.getElementById('gateway-url') as HTMLInputElement).value;
  const token = (document.getElementById('gateway-token') as HTMLInputElement).value;
  
  // Test connection
  setGatewayConfig(url, token);
  const status = await getGatewayStatus();
  
  if (status.running) {
    config.configured = true;
    config.gateway = { url, token };
    saveConfig();
    
    statusDot.classList.add('connected');
    statusText.textContent = 'Connected';
    
    switchView('chat');
  } else {
    alert('Could not connect to gateway. Check URL and try again.');
  }
});

// Settings Form
function syncSettingsForm() {
  (document.getElementById('settings-gateway-url') as HTMLInputElement).value = config.gateway.url;
  (document.getElementById('settings-gateway-token') as HTMLInputElement).value = config.gateway.token;
}

document.getElementById('settings-save-gateway')?.addEventListener('click', async () => {
  const url = (document.getElementById('settings-gateway-url') as HTMLInputElement).value;
  const token = (document.getElementById('settings-gateway-token') as HTMLInputElement).value;
  
  setGatewayConfig(url, token);
  const status = await getGatewayStatus();
  
  if (status.running) {
    config.gateway = { url, token };
    saveConfig();
    
    statusDot.classList.add('connected');
    statusDot.classList.remove('error');
    statusText.textContent = 'Connected';
    
    alert('Settings saved!');
  } else {
    alert('Could not connect to gateway with these settings.');
  }
});

// Config persistence
function saveConfig() {
  localStorage.setItem('claw-config', JSON.stringify(config));
}

function loadConfig() {
  const saved = localStorage.getItem('claw-config');
  if (saved) {
    try {
      config = JSON.parse(saved);
      setGatewayConfig(config.gateway.url, config.gateway.token);
    } catch {
      // Invalid config, use defaults
    }
  }
}

// Chat functionality
chatSend.addEventListener('click', sendMessage);
chatInput.addEventListener('keydown', (e) => {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault();
    sendMessage();
  }
});

// Auto-resize textarea
chatInput.addEventListener('input', () => {
  chatInput.style.height = 'auto';
  chatInput.style.height = Math.min(chatInput.scrollHeight, 120) + 'px';
});

// New chat
document.getElementById('new-chat-btn')?.addEventListener('click', () => {
  messages = [];
  renderMessages();
});

async function sendMessage() {
  const content = chatInput.value.trim();
  if (!content || isLoading) return;

  // Add user message
  addMessage({ role: 'user', content, timestamp: new Date() });
  chatInput.value = '';
  chatInput.style.height = 'auto';

  isLoading = true;
  chatSend.disabled = true;
  showLoading();

  try {
    const response = await callGateway(content);
    hideLoading();
    addMessage({ role: 'assistant', content: response, timestamp: new Date() });
  } catch (error) {
    console.error('Error:', error);
    hideLoading();
    addMessage({
      role: 'assistant',
      content: `Error: ${error instanceof Error ? error.message : 'Failed to get response'}`,
      timestamp: new Date(),
    });
  } finally {
    isLoading = false;
    chatSend.disabled = false;
  }
}

function addMessage(message: Message) {
  messages.push(message);
  renderMessages();
}

function renderMessages() {
  if (messages.length === 0) {
    chatEmpty.style.display = 'flex';
    return;
  }

  chatEmpty.style.display = 'none';

  // Clear and re-render
  const existingMessages = chatMessages.querySelectorAll('.message');
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
  chatEmpty.style.display = 'none';
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
  chatMessages.appendChild(loadingDiv);
  chatMessages.scrollTop = chatMessages.scrollHeight;
}

function hideLoading() {
  const loading = document.getElementById('loading-message');
  if (loading) loading.remove();
}

async function callGateway(userMessage: string): Promise<string> {
  // Call gateway API
  const response = await fetch(`${config.gateway.url}/api/v1/chat`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Authorization': `Bearer ${config.gateway.token}`,
    },
    body: JSON.stringify({
      message: userMessage,
      history: messages.slice(-10).map((m) => ({
        role: m.role,
        content: m.content,
      })),
    }),
  });

  if (!response.ok) {
    // Try webchat endpoint
    const webchatResponse = await fetch(`${config.gateway.url}/webchat/send`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        message: userMessage,
      }),
    });

    if (!webchatResponse.ok) {
      throw new Error(`Gateway error: ${response.status}`);
    }

    const data = await webchatResponse.json();
    return data.response || data.message || 'No response';
  }

  const data = await response.json();
  return data.response || data.message || data.content || 'No response';
}

// Gateway status check
async function checkGatewayStatus() {
  const status = await getGatewayStatus();
  
  if (status.running) {
    statusDot.classList.add('connected');
    statusDot.classList.remove('error');
    statusText.textContent = 'Connected';
  } else {
    statusDot.classList.remove('connected');
    statusDot.classList.add('error');
    statusText.textContent = 'Disconnected';
  }
}

// Initialize
document.addEventListener('DOMContentLoaded', () => {
  loadConfig();

  if (config.configured) {
    switchView('chat');
    checkGatewayStatus();
  } else {
    showSetup();
  }

  // Periodic status check
  setInterval(checkGatewayStatus, 10000);
});
