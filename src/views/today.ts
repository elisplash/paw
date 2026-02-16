// Today View — Daily briefing with weather, calendar, tasks, and unread emails

import { gateway } from '../gateway';

const $ = (id: string) => document.getElementById(id);

interface Task {
  id: string;
  text: string;
  done: boolean;
  dueDate?: string;
  createdAt: string;
}

let _tasks: Task[] = [];

export function configure(_opts: Record<string, unknown>) {
  // Future: callbacks for navigation etc
}

export async function loadToday() {
  console.log('[today] loadToday called');
  loadTasks();
  renderToday();
  
  // Fetch live data
  await Promise.all([
    fetchWeather(),
    fetchUnreadEmails(),
  ]);
}

function loadTasks() {
  try {
    const stored = localStorage.getItem('paw-tasks');
    _tasks = stored ? JSON.parse(stored) : [];
  } catch {
    _tasks = [];
  }
}

function saveTasks() {
  localStorage.setItem('paw-tasks', JSON.stringify(_tasks));
}

async function fetchWeather() {
  const weatherEl = $('today-weather');
  if (!weatherEl) return;
  
  try {
    // Simple weather fetch - use proxy-friendly format
    const response = await fetch('https://wttr.in/?format=j1');
    const data = await response.json();
    const current = data?.current_condition?.[0];
    if (current) {
      const temp = current.temp_F || current.temp_C;
      const desc = current.weatherDesc?.[0]?.value || '';
      const icon = getWeatherIcon(current.weatherCode);
      weatherEl.innerHTML = `
        <div class="today-weather-main">
          <span class="today-weather-icon">${icon}</span>
          <span class="today-weather-temp">${temp}°F</span>
        </div>
        <div class="today-weather-desc">${desc}</div>
      `;
    } else {
      throw new Error('No weather data');
    }
  } catch {
    weatherEl.innerHTML = '<span class="today-weather-unavailable">Weather unavailable</span>';
  }
}

function getWeatherIcon(code: string): string {
  const c = parseInt(code);
  if (c === 113) return '☀️';
  if (c === 116) return '⛅';
  if ([119, 122].includes(c)) return '☁️';
  if ([143, 248, 260].includes(c)) return '🌫️';
  if ([176, 263, 266, 293, 296, 299, 302, 305, 308, 311, 314, 353, 356, 359].includes(c)) return '🌧️';
  if ([179, 182, 185, 281, 284, 317, 320, 323, 326, 329, 332, 335, 338, 350, 362, 365, 368, 371, 374, 377].includes(c)) return '🌨️';
  if ([200, 386, 389, 392, 395].includes(c)) return '⛈️';
  return '🌤️';
}

async function fetchUnreadEmails() {
  const emailsEl = $('today-emails');
  if (!emailsEl) return;
  
  // This would integrate with mail module - for now show placeholder
  emailsEl.innerHTML = `
    <div class="today-section-empty">Connect email in Mail view to see unread messages</div>
  `;
}

function renderToday() {
  const container = $('today-content');
  if (!container) return;

  const now = new Date();
  const dateStr = now.toLocaleDateString('en-US', { weekday: 'long', month: 'long', day: 'numeric' });
  const greeting = getGreeting();

  const pendingTasks = _tasks.filter(t => !t.done);
  const completedToday = _tasks.filter(t => t.done && isToday(t.createdAt));

  container.innerHTML = `
    <div class="today-header">
      <div class="today-greeting">${greeting}, Eli</div>
      <div class="today-date">${dateStr}</div>
    </div>
    
    <div class="today-grid">
      <div class="today-main">
        <!-- Dave's Summary -->
        <div class="today-card today-dave-card">
          <div class="today-dave-header">
            <div class="today-dave-avatar">🧠</div>
            <div class="today-dave-intro">
              <div class="today-dave-name">Dave</div>
              <div class="today-dave-role">Your AI Assistant</div>
            </div>
          </div>
          <div class="today-dave-message" id="today-dave-message">
            ${getDaveMessage(pendingTasks.length, completedToday.length)}
          </div>
        </div>
        
        <!-- Weather -->
        <div class="today-card">
          <div class="today-card-header">
            <span class="today-card-icon">☀️</span>
            <span class="today-card-title">Weather</span>
          </div>
          <div class="today-card-body" id="today-weather">
            <span class="today-loading">Loading...</span>
          </div>
        </div>
        
        <!-- Tasks -->
        <div class="today-card today-card-tasks">
          <div class="today-card-header">
            <span class="today-card-icon">✅</span>
            <span class="today-card-title">Tasks</span>
            <span class="today-card-count">${pendingTasks.length}</span>
            <button class="btn btn-ghost btn-sm today-add-task-btn">+ Add</button>
          </div>
          <div class="today-card-body">
            <div class="today-tasks" id="today-tasks">
              ${pendingTasks.length === 0 ? `
                <div class="today-section-empty">No tasks yet. Add one to get started!</div>
              ` : pendingTasks.map(task => `
                <div class="today-task" data-id="${task.id}">
                  <input type="checkbox" class="today-task-check" ${task.done ? 'checked' : ''}>
                  <span class="today-task-text">${escHtml(task.text)}</span>
                  <button class="today-task-delete" title="Delete">×</button>
                </div>
              `).join('')}
            </div>
            ${completedToday.length > 0 ? `
              <div class="today-completed-label">${completedToday.length} completed today</div>
            ` : ''}
          </div>
        </div>
        
        <!-- Unread Emails -->
        <div class="today-card">
          <div class="today-card-header">
            <span class="today-card-icon">📧</span>
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
            <span class="today-card-icon">⚡</span>
            <span class="today-card-title">Quick Actions</span>
          </div>
          <div class="today-card-body">
            <button class="today-quick-action" id="today-briefing-btn">
              <span>🎙️</span> Morning Briefing
            </button>
            <button class="today-quick-action" id="today-summarize-btn">
              <span>📝</span> Summarize Inbox
            </button>
            <button class="today-quick-action" id="today-schedule-btn">
              <span>📅</span> What's on today?
            </button>
          </div>
        </div>
      </div>
    </div>
  `;

  bindEvents();
}

function bindEvents() {
  // Add task button
  $('today-content')?.querySelector('.today-add-task-btn')?.addEventListener('click', () => {
    openAddTaskModal();
  });

  // Task checkboxes
  document.querySelectorAll('.today-task-check').forEach(checkbox => {
    checkbox.addEventListener('change', (e) => {
      const taskEl = (e.target as HTMLElement).closest('.today-task');
      const taskId = taskEl?.getAttribute('data-id');
      if (taskId) toggleTask(taskId);
    });
  });

  // Task delete buttons
  document.querySelectorAll('.today-task-delete').forEach(btn => {
    btn.addEventListener('click', (e) => {
      const taskEl = (e.target as HTMLElement).closest('.today-task');
      const taskId = taskEl?.getAttribute('data-id');
      if (taskId) deleteTask(taskId);
    });
  });

  // Quick actions
  $('today-briefing-btn')?.addEventListener('click', () => triggerBriefing());
  $('today-summarize-btn')?.addEventListener('click', () => triggerInboxSummary());
  $('today-schedule-btn')?.addEventListener('click', () => triggerScheduleCheck());
}

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
  input?.addEventListener('keydown', (e) => { if (e.key === 'Enter') submit(); });
  modal.addEventListener('click', (e) => { if (e.target === modal) close(); });
}

function addTask(text: string) {
  const task: Task = {
    id: `task-${Date.now()}`,
    text,
    done: false,
    createdAt: new Date().toISOString(),
  };
  _tasks.unshift(task);
  saveTasks();
  renderToday();
  showToast('Task added');
}

function toggleTask(taskId: string) {
  const task = _tasks.find(t => t.id === taskId);
  if (task) {
    task.done = !task.done;
    saveTasks();
    setTimeout(() => renderToday(), 300); // Delay for animation
  }
}

function deleteTask(taskId: string) {
  _tasks = _tasks.filter(t => t.id !== taskId);
  saveTasks();
  renderToday();
}

async function triggerBriefing() {
  showToast('Starting morning briefing...');
  try {
    await gateway.chatSend('main', 'Give me a morning briefing: weather, any calendar events today, and summarize my unread emails.');
  } catch {
    showToast('Failed to start briefing', 'error');
  }
}

async function triggerInboxSummary() {
  showToast('Summarizing inbox...');
  try {
    await gateway.chatSend('main', 'Check my email inbox and summarize the important unread messages.');
  } catch {
    showToast('Failed to summarize inbox', 'error');
  }
}

async function triggerScheduleCheck() {
  showToast('Checking schedule...');
  try {
    await gateway.chatSend('main', 'What do I have scheduled for today? Check my calendar.');
  } catch {
    showToast('Failed to check schedule', 'error');
  }
}

function getGreeting(): string {
  const hour = new Date().getHours();
  if (hour < 12) return 'Good morning';
  if (hour < 17) return 'Good afternoon';
  return 'Good evening';
}

function getDaveMessage(pendingTasks: number, completedToday: number): string {
  const hour = new Date().getHours();
  const day = new Date().toLocaleDateString('en-US', { weekday: 'long' });
  
  let message = '';
  
  // Time-based opener
  if (hour < 12) {
    message = `Happy ${day}! Ready to make today count? `;
  } else if (hour < 17) {
    message = `Hope your ${day} is going well. `;
  } else {
    message = `Winding down this ${day}. `;
  }
  
  // Task-based context
  if (completedToday > 0 && pendingTasks === 0) {
    message += `You crushed it — ${completedToday} task${completedToday > 1 ? 's' : ''} done and nothing pending! 🎉`;
  } else if (completedToday > 0) {
    message += `Nice progress! ${completedToday} down, ${pendingTasks} to go.`;
  } else if (pendingTasks > 0) {
    message += `You've got ${pendingTasks} task${pendingTasks > 1 ? 's' : ''} lined up. Let's knock them out.`;
  } else {
    message += `No tasks on the board yet. Add something or hit Morning Briefing to get started.`;
  }
  
  return message;
}

function isToday(dateStr: string): boolean {
  const date = new Date(dateStr);
  const today = new Date();
  return date.toDateString() === today.toDateString();
}

function showToast(message: string, type: 'success' | 'error' = 'success') {
  const toast = document.createElement('div');
  toast.className = `toast toast-${type}`;
  toast.textContent = message;
  document.body.appendChild(toast);
  setTimeout(() => toast.classList.add('show'), 10);
  setTimeout(() => {
    toast.classList.remove('show');
    setTimeout(() => toast.remove(), 300);
  }, 3000);
}

function escHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

export function initToday() {
  // Called on app startup
}
