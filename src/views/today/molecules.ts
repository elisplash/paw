// Today View — DOM rendering + IPC

import { pawEngine } from '../../engine';
import { getCurrentAgent, spriteAvatar } from '../agents';
import { switchView } from '../router';
import { $, escHtml } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { type Task, getWeatherIcon, getGreeting, getPawzMessage, isToday } from './atoms';

// ── Tauri bridge (no pawEngine equivalent for these commands) ──────────
interface TauriWindow {
  __TAURI__?: {
    core: { invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T> };
  };
}
const tauriWindow = window as unknown as TauriWindow;
const invoke = tauriWindow.__TAURI__?.core?.invoke;

// ── State bridge ──────────────────────────────────────────────────────

interface MoleculesState {
  getTasks: () => Task[];
  setTasks: (t: Task[]) => void;
  getRenderToday: () => () => void;
}

let _state: MoleculesState;

export function initMoleculesState() {
  return {
    setMoleculesState(s: MoleculesState) {
      _state = s;
    },
  };
}

// ── Weather ───────────────────────────────────────────────────────────

export async function fetchWeather() {
  const weatherEl = $('today-weather');
  if (!weatherEl) return;

  try {
    let json: string | null = null;

    if (invoke) {
      json = await invoke<string>('fetch_weather', { location: null });
    } else {
      const response = await fetch('https://wttr.in/?format=j1', {
        headers: { 'User-Agent': 'curl' },
        signal: AbortSignal.timeout(8000),
      });
      json = await response.text();
    }

    if (!json) throw new Error('No weather data');

    const data = JSON.parse(json);
    const current = data.current_condition?.[0];
    if (!current) throw new Error('No current weather');

    const tempC = current.temp_C ?? '--';
    const tempF = current.temp_F ?? '--';
    const desc = current.weatherDesc?.[0]?.value ?? '';
    const code = current.weatherCode ?? '';
    const feelsLikeC = current.FeelsLikeC;
    const humidity = current.humidity;
    const windKmph = current.windspeedKmph;
    const icon = getWeatherIcon(code);

    const area = data.nearest_area?.[0];
    const location = area
      ? `${area.areaName?.[0]?.value ?? ''}${area.country?.[0]?.value ? `, ${area.country[0].value}` : ''}`
      : '';

    weatherEl.innerHTML = `
      <div class="today-weather-main">
        <span class="today-weather-icon">${icon}</span>
        <span class="today-weather-temp">${tempC}°C / ${tempF}°F</span>
      </div>
      <div class="today-weather-desc">${desc}</div>
      <div class="today-weather-details">
        ${feelsLikeC ? `<span>Feels like ${feelsLikeC}°C</span>` : ''}
        ${humidity ? `<span><span class="ms ms-sm">water_drop</span> ${humidity}%</span>` : ''}
        ${windKmph ? `<span><span class="ms ms-sm">air</span> ${windKmph} km/h</span>` : ''}
      </div>
      ${location ? `<div class="today-weather-location">${escHtml(location)}</div>` : ''}
    `;
  } catch (e) {
    console.warn('[today] Weather fetch failed:', e);
    weatherEl.innerHTML = `
      <div class="today-weather-main">
        <span class="today-weather-icon"><span class="ms ms-lg">cloud</span></span>
        <span class="today-weather-temp">--</span>
      </div>
      <div class="today-weather-desc">Weather unavailable — check connection</div>
    `;
  }
}

// ── Emails ────────────────────────────────────────────────────────────

export async function fetchUnreadEmails() {
  const emailsEl = $('today-emails');
  if (!emailsEl) return;

  if (!invoke) {
    emailsEl.innerHTML = `<div class="today-section-empty">Email requires the desktop app</div>`;
    return;
  }

  try {
    let accounts: { name: string; email: string }[] = [];
    if (invoke) {
      try {
        const toml = await invoke<string>('read_himalaya_config');
        if (toml) {
          const accountBlocks = toml.matchAll(
            /\[accounts\.([^\]]+)\][\s\S]*?email\s*=\s*"([^"]+)"/g,
          );
          for (const match of accountBlocks) {
            accounts.push({ name: match[1], email: match[2] });
          }
        }
      } catch {
        /* no config yet */
      }
    }
    if (accounts.length === 0) {
      try {
        const raw = localStorage.getItem('mail-accounts-fallback');
        if (raw) accounts = JSON.parse(raw);
      } catch {
        /* ignore */
      }
    }

    if (accounts.length === 0) {
      emailsEl.innerHTML = `<div class="today-section-empty">Set up email in the <a href="#" class="today-link-mail">Mail</a> view to see messages here</div>`;
      emailsEl.querySelector('.today-link-mail')?.addEventListener('click', (e) => {
        e.preventDefault();
        const mailNav = document.querySelector('[data-view="mail"]') as HTMLElement;
        mailNav?.click();
      });
      return;
    }

    const accountName = accounts[0].name;
    const jsonResult = await invoke<string>('fetch_emails', {
      account: accountName,
      folder: 'INBOX',
      pageSize: 10,
    });

    interface EmailEnvelope {
      id: string;
      flags: string[];
      subject: string;
      from: { name?: string; addr: string };
      date: string;
    }

    let envelopes: EmailEnvelope[] = [];
    try {
      envelopes = JSON.parse(jsonResult);
    } catch {
      /* ignore */
    }

    const unread = envelopes.filter((e) => !e.flags?.includes('Seen'));

    if (unread.length === 0) {
      emailsEl.innerHTML = `<div class="today-section-empty"><span class="ms ms-sm">mark_email_read</span> No unread emails — you're all caught up!</div>`;
      return;
    }

    emailsEl.innerHTML = unread
      .slice(0, 8)
      .map((email) => {
        const from = email.from?.name || email.from?.addr || 'Unknown';
        const subject = email.subject || '(No subject)';
        const date = email.date ? new Date(email.date) : null;
        const timeStr = date
          ? date.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' })
          : '';
        return `
        <div class="today-email-item">
          <div class="today-email-from">${escHtml(from)}</div>
          <div class="today-email-subject">${escHtml(subject)}</div>
          ${timeStr ? `<div class="today-email-time">${timeStr}</div>` : ''}
        </div>
      `;
      })
      .join('');

    if (unread.length > 8) {
      emailsEl.innerHTML += `<div class="today-email-more">+${unread.length - 8} more unread</div>`;
    }
  } catch (e) {
    console.warn('[today] Email fetch failed:', e);
    emailsEl.innerHTML = `<div class="today-section-empty">Could not load emails — check Mail settings</div>`;
  }
}

// ── Dashboard Render ──────────────────────────────────────────────────

export function renderToday() {
  const container = $('today-content');
  if (!container) return;

  const tasks = _state.getTasks();
  const now = new Date();
  const dateStr = now.toLocaleDateString('en-US', {
    weekday: 'long',
    month: 'long',
    day: 'numeric',
  });
  const greeting = getGreeting();
  const userName = localStorage.getItem('paw-user-name') || '';

  const pendingTasks = tasks.filter((t) => !t.done);
  const completedToday = tasks.filter((t) => t.done && isToday(t.createdAt));

  const mainAgent = getCurrentAgent();
  const agentName = mainAgent?.name ?? 'Agent';
  const agentAvatar = mainAgent ? spriteAvatar(mainAgent.avatar, 48) : spriteAvatar('5', 48);

  container.innerHTML = `
    <div class="today-header">
      <div class="today-greeting">${greeting}${userName ? `, ${escHtml(userName)}` : ''}</div>
      <div class="today-date">${dateStr}</div>
    </div>
    
    <div class="today-grid">
      <div class="today-main">
        <!-- Agent Summary -->
        <div class="today-card today-dave-card">
          <div class="today-dave-header">
            <div class="today-dave-avatar">${agentAvatar}</div>
            <div class="today-dave-intro">
              <div class="today-dave-name">${escHtml(agentName)}</div>
              <div class="today-dave-role">Your AI Agent</div>
            </div>
          </div>
          <div class="today-dave-message" id="today-dave-message">
            ${getPawzMessage(pendingTasks.length, completedToday.length)}
          </div>
        </div>
        
        <!-- Weather -->
        <div class="today-card">
          <div class="today-card-header">
            <span class="today-card-icon"><span class="ms">partly_cloudy_day</span></span>
            <span class="today-card-title">Weather</span>
          </div>
          <div class="today-card-body" id="today-weather">
            <span class="today-loading">Loading...</span>
          </div>
        </div>
        
        <!-- Tasks -->
        <div class="today-card today-card-tasks">
          <div class="today-card-header">
            <span class="today-card-icon"><span class="ms">task_alt</span></span>
            <span class="today-card-title">Tasks</span>
            <span class="today-card-count">${pendingTasks.length}</span>
            <button class="btn btn-ghost btn-sm today-add-task-btn">+ Add</button>
          </div>
          <div class="today-card-body">
            <div class="today-tasks" id="today-tasks">
              ${
                pendingTasks.length === 0
                  ? `
                <div class="today-section-empty">No tasks yet. Add one to get started!</div>
              `
                  : pendingTasks
                      .map(
                        (task) => `
                <div class="today-task" data-id="${task.id}">
                  <input type="checkbox" class="today-task-check" ${task.done ? 'checked' : ''}>
                  <span class="today-task-text">${escHtml(task.text)}</span>
                  <button class="today-task-delete" title="Delete">×</button>
                </div>
              `,
                      )
                      .join('')
              }
            </div>
            ${
              completedToday.length > 0
                ? `
              <div class="today-completed-label">${completedToday.length} completed today</div>
            `
                : ''
            }
          </div>
        </div>
        
        <!-- Unread Emails -->
        <div class="today-card">
          <div class="today-card-header">
            <span class="today-card-icon"><span class="ms">mail</span></span>
            <span class="today-card-title">Unread Emails</span>
          </div>
          <div class="today-card-body" id="today-emails">
            <span class="today-loading">Loading...</span>
          </div>
        </div>
      </div>
      
      <div class="today-sidebar">
        <!-- Quick Actions -->
        <div class="today-card">
          <div class="today-card-header">
            <span class="today-card-icon"><span class="ms">bolt</span></span>
            <span class="today-card-title">Quick Actions</span>
          </div>
          <div class="today-card-body">
            <button class="today-quick-action" id="today-briefing-btn">
              <span class="ms">campaign</span> Morning Briefing
            </button>
            <button class="today-quick-action" id="today-summarize-btn">
              <span class="ms">summarize</span> Summarize Inbox
            </button>
            <button class="today-quick-action" id="today-schedule-btn">
              <span class="ms">calendar_today</span> What's on today?
            </button>
          </div>
        </div>
      </div>
    </div>
  `;

  bindEvents();
}

// ── Events ────────────────────────────────────────────────────────────

function bindEvents() {
  $('today-content')
    ?.querySelector('.today-add-task-btn')
    ?.addEventListener('click', () => {
      openAddTaskModal();
    });

  document.querySelectorAll('.today-task-check').forEach((checkbox) => {
    checkbox.addEventListener('change', (e) => {
      const taskEl = (e.target as HTMLElement).closest('.today-task');
      const taskId = taskEl?.getAttribute('data-id');
      if (taskId) toggleTask(taskId);
    });
  });

  document.querySelectorAll('.today-task-delete').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      const taskEl = (e.target as HTMLElement).closest('.today-task');
      const taskId = taskEl?.getAttribute('data-id');
      if (taskId) deleteTask(taskId);
    });
  });

  $('today-briefing-btn')?.addEventListener('click', () => triggerBriefing());
  $('today-summarize-btn')?.addEventListener('click', () => triggerInboxSummary());
  $('today-schedule-btn')?.addEventListener('click', () => triggerScheduleCheck());
}

// ── Task Modal ────────────────────────────────────────────────────────

function openAddTaskModal() {
  const modal = document.createElement('div');
  modal.className = 'today-modal';
  modal.innerHTML = `
    <div class="today-modal-dialog">
      <div class="today-modal-header">
        <span>Add Task</span>
        <button class="btn-icon today-modal-close">×</button>
      </div>
      <div class="today-modal-body">
        <input type="text" class="form-input" id="task-input" placeholder="What needs to be done?" autofocus>
      </div>
      <div class="today-modal-footer">
        <button class="btn btn-ghost today-modal-cancel">Cancel</button>
        <button class="btn btn-primary" id="task-submit">Add Task</button>
      </div>
    </div>
  `;
  document.body.appendChild(modal);

  const input = modal.querySelector('#task-input') as HTMLInputElement;
  input?.focus();

  const close = () => modal.remove();
  const submit = () => {
    const text = input?.value.trim();
    if (text) {
      addTask(text);
      close();
    }
  };

  modal.querySelector('.today-modal-close')?.addEventListener('click', close);
  modal.querySelector('.today-modal-cancel')?.addEventListener('click', close);
  modal.querySelector('#task-submit')?.addEventListener('click', submit);
  input?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') submit();
  });
  modal.addEventListener('click', (e) => {
    if (e.target === modal) close();
  });
}

// ── Task CRUD ─────────────────────────────────────────────────────────

function addTask(text: string) {
  const tasks = _state.getTasks();
  const task: Task = {
    id: `task-${Date.now()}`,
    text,
    done: false,
    createdAt: new Date().toISOString(),
  };
  tasks.unshift(task);
  _state.setTasks(tasks);
  saveTasks();
  renderToday();
  showToast('Task added');
}

function toggleTask(taskId: string) {
  const tasks = _state.getTasks();
  const task = tasks.find((t) => t.id === taskId);
  if (task) {
    task.done = !task.done;
    _state.setTasks(tasks);
    saveTasks();
    renderToday();
  }
}

function deleteTask(taskId: string) {
  const tasks = _state.getTasks().filter((t) => t.id !== taskId);
  _state.setTasks(tasks);
  saveTasks();
  renderToday();
}

function saveTasks() {
  localStorage.setItem('paw-tasks', JSON.stringify(_state.getTasks()));
}

// ── Quick Actions ─────────────────────────────────────────────────────

async function triggerBriefing() {
  showToast('Starting morning briefing...');
  switchView('chat');
  try {
    await pawEngine.chatSend(
      'main',
      'Give me a morning briefing: weather, any calendar events today, and summarize my unread emails.',
    );
  } catch {
    showToast('Failed to start briefing', 'error');
  }
}

async function triggerInboxSummary() {
  showToast('Summarizing inbox...');
  switchView('chat');
  try {
    await pawEngine.chatSend(
      'main',
      'Check my email inbox and summarize the important unread messages.',
    );
  } catch {
    showToast('Failed to summarize inbox', 'error');
  }
}

async function triggerScheduleCheck() {
  showToast('Checking schedule...');
  switchView('chat');
  try {
    await pawEngine.chatSend('main', 'What do I have scheduled for today? Check my calendar.');
  } catch {
    showToast('Failed to check schedule', 'error');
  }
}
