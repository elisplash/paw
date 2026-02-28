// Today View — DOM rendering + IPC (Command Center)

import { pawEngine } from '../../engine';
import { getAgents, loadAgents, setSelectedAgent } from '../agents';
import { switchView } from '../router';
import { $, escHtml } from '../../components/helpers';
import { showToast } from '../../components/toast';
import {
  type Task,
  getWeatherIcon,
  getGreeting,
  isToday,
  engineTaskToToday,
  filterTodayTasks,
  toggledStatus,
  formatTokens,
  formatCost,
  agentStatus,
  buildHeatmapData,
  buildCapabilityGroups,
  type CapabilityGroup,
} from './atoms';
import { renderSkillWidgets } from '../../components/molecules/skill-widget';
import type { SkillOutput, EngineSkillStatus } from '../../engine/atoms/types';
import { appState } from '../../state';
import {
  renderDashboardIntegrations,
  wireDashboardEvents,
  loadServiceHealth,
} from '../../features/integration-health';
import { heatmapStrip } from '../../components/molecules/data-viz';
import { isShowcaseActive, getShowcaseData } from '../../components/showcase';
import {
  kineticRow,
  kineticStagger,
  kineticDot,
  type KineticStatus,
} from '../../components/kinetic-row';

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

/** Save weather_location to engine config. */
async function saveWeatherLocation(location: string) {
  try {
    const { pawEngine: pe } = await import('../../engine');
    const cfg = await pe.getConfig();
    cfg.weather_location = location;
    await pe.setConfig(cfg);
  } catch (e) {
    console.warn('[today] Failed to save weather location:', e);
  }
}

/** Show inline prompt to set/change location. */
function showLocationEditor(weatherEl: HTMLElement, currentLocation: string) {
  const editor = document.createElement('div');
  editor.style.cssText = 'display:flex;gap:6px;align-items:center;margin-top:6px';
  const inp = document.createElement('input');
  inp.type = 'text';
  inp.value = currentLocation;
  inp.placeholder = 'Enter city (e.g. New York, London, Tokyo)';
  inp.style.cssText =
    'font-size:12px;padding:4px 8px;border-radius:6px;border:1px solid var(--border);background:var(--bg-secondary);color:var(--text);width:200px';
  const saveBtn = document.createElement('button');
  saveBtn.className = 'btn btn-sm';
  saveBtn.textContent = 'Save';
  saveBtn.style.cssText = 'font-size:11px;padding:2px 10px';
  const cancelBtn = document.createElement('button');
  cancelBtn.className = 'btn btn-ghost btn-sm';
  cancelBtn.textContent = 'Cancel';
  cancelBtn.style.cssText = 'font-size:11px;padding:2px 8px';
  editor.appendChild(inp);
  editor.appendChild(saveBtn);
  editor.appendChild(cancelBtn);
  weatherEl.appendChild(editor);
  inp.focus();

  const finish = async (save: boolean) => {
    if (save && inp.value.trim()) {
      await saveWeatherLocation(inp.value.trim());
      fetchWeather(); // re-fetch with new location
    } else {
      editor.remove();
    }
  };
  saveBtn.addEventListener('click', () => finish(true));
  cancelBtn.addEventListener('click', () => finish(false));
  inp.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') finish(true);
    if (e.key === 'Escape') finish(false);
  });
}

export async function fetchWeather() {
  const weatherEl = $('today-weather');
  if (!weatherEl) return;

  try {
    let json: string | null = null;

    if (invoke) {
      // Desktop: backend reads config + auto-detects location
      json = await invoke<string>('fetch_weather');
    } else {
      // Browser fallback — auto-detect via IP then fetch from Open-Meteo
      const ipResp = await fetch('https://ipapi.co/json/', {
        signal: AbortSignal.timeout(5000),
      });
      const ipData = await ipResp.json();
      const lat = ipData.latitude;
      const lon = ipData.longitude;
      if (!lat || !lon) throw new Error('Could not detect location');
      const wxResp = await fetch(
        `https://api.open-meteo.com/v1/forecast?latitude=${lat}&longitude=${lon}&current=temperature_2m,apparent_temperature,weather_code,wind_speed_10m,relative_humidity_2m&wind_speed_unit=kmh`,
        { signal: AbortSignal.timeout(8000) },
      );
      const wx = await wxResp.json();
      wx.location = { name: ipData.city ?? '', country: ipData.country_name ?? '' };
      json = JSON.stringify(wx);
    }

    if (!json) throw new Error('No weather data');

    const data = JSON.parse(json);
    const current = data.current;
    if (!current) throw new Error('No current weather');

    const tempC = current.temperature_2m ?? '--';
    const tempF = tempC !== '--' ? Math.round((tempC * 9) / 5 + 32) : '--';
    const code = String(current.weather_code ?? '');
    const feelsLikeC = current.apparent_temperature;
    const humidity = current.relative_humidity_2m;
    const windKmph = current.wind_speed_10m;
    const icon = getWeatherIcon(code);

    // WMO weather code to human-readable description
    const wmoDesc: Record<number, string> = {
      0: 'Clear sky',
      1: 'Mainly clear',
      2: 'Partly cloudy',
      3: 'Overcast',
      45: 'Fog',
      48: 'Depositing rime fog',
      51: 'Light drizzle',
      53: 'Moderate drizzle',
      55: 'Dense drizzle',
      56: 'Freezing drizzle',
      57: 'Dense freezing drizzle',
      61: 'Slight rain',
      63: 'Moderate rain',
      65: 'Heavy rain',
      66: 'Light freezing rain',
      67: 'Heavy freezing rain',
      71: 'Slight snow',
      73: 'Moderate snow',
      75: 'Heavy snow',
      77: 'Snow grains',
      80: 'Slight rain showers',
      81: 'Moderate rain showers',
      82: 'Violent rain showers',
      85: 'Slight snow showers',
      86: 'Heavy snow showers',
      95: 'Thunderstorm',
      96: 'Thunderstorm with slight hail',
      99: 'Thunderstorm with heavy hail',
    };
    const desc = wmoDesc[current.weather_code] ?? '';

    const loc = data.location;
    const location = loc ? `${loc.name ?? ''}${loc.country ? `, ${loc.country}` : ''}` : '';

    weatherEl.innerHTML = `
      <div class="today-weather-main">
        <span class="today-weather-icon">${icon}</span>
        <span class="today-weather-temp">${tempC}°C / ${tempF}°F</span>
      </div>
      <div class="today-weather-desc">${desc}</div>
      <div class="today-weather-details">
        ${feelsLikeC != null ? `<span>Feels like ${feelsLikeC}°C</span>` : ''}
        ${humidity != null ? `<span><span class="ms ms-sm">water_drop</span> ${humidity}%</span>` : ''}
        ${windKmph != null ? `<span><span class="ms ms-sm">air</span> ${windKmph} km/h</span>` : ''}
      </div>
      ${location ? `<div class="today-weather-location" id="weather-location-text" style="cursor:pointer;display:inline-flex;align-items:center;gap:4px" title="Click to change location"><span class="ms" style="font-size:14px">edit_location_alt</span> ${escHtml(location)}</div>` : ''}
    `;

    // Wire location click → inline editor
    const locEl = document.getElementById('weather-location-text');
    if (locEl) {
      locEl.addEventListener('click', () => {
        showLocationEditor(weatherEl, loc?.name ?? '');
      });
    }
  } catch (e) {
    console.warn('[today] Weather fetch failed:', e);
    weatherEl.innerHTML = `
      <div class="today-weather-main">
        <span class="today-weather-icon"><span class="ms ms-lg">cloud</span></span>
        <span class="today-weather-temp">--</span>
      </div>
      <div class="today-weather-desc" style="cursor:pointer" id="weather-set-location">
        Click to set your location
      </div>
    `;
    // Wire "set location" click
    const setEl = document.getElementById('weather-set-location');
    if (setEl) {
      setEl.addEventListener('click', () => {
        showLocationEditor(weatherEl, '');
      });
    }
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
    interface UnreadItem {
      from: string;
      subject: string;
      date: Date | null;
      source: 'himalaya';
    }
    const unreadItems: UnreadItem[] = [];

    // ── Himalaya IMAP emails ─────────────────────────────────────────
    let himalayaAccounts: { name: string; email: string }[] = [];
    try {
      const toml = await pawEngine.mailReadConfig();
      if (toml) {
        const accountBlocks = toml.matchAll(/\[accounts\.([^\]]+)\][\s\S]*?email\s*=\s*"([^"]+)"/g);
        for (const match of accountBlocks) {
          himalayaAccounts.push({ name: match[1], email: match[2] });
        }
      }
    } catch {
      /* no himalaya config */
    }
    if (himalayaAccounts.length === 0) {
      try {
        const raw = localStorage.getItem('mail-accounts-fallback');
        if (raw) himalayaAccounts = JSON.parse(raw);
      } catch {
        /* ignore */
      }
    }

    if (himalayaAccounts.length > 0) {
      try {
        const fetchPromise = pawEngine.mailFetchEmails(himalayaAccounts[0].name, 'INBOX', 10);
        const timeoutPromise = new Promise<never>((_, reject) =>
          setTimeout(() => reject(new Error('IMAP timeout')), 12000),
        );
        const jsonResult = await Promise.race([fetchPromise, timeoutPromise]);
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
        for (const env of envelopes) {
          if (!env.flags?.includes('Seen')) {
            unreadItems.push({
              from: env.from?.name || env.from?.addr || 'Unknown',
              subject: env.subject || '(No subject)',
              date: env.date ? new Date(env.date) : null,
              source: 'himalaya',
            });
          }
        }
      } catch (e) {
        console.warn('[today] Himalaya email fetch failed:', e);
      }
    }

    // ── No email sources configured ───────────────────────────────
    if (unreadItems.length === 0 && himalayaAccounts.length === 0) {
      emailsEl.innerHTML = `<div class="today-section-empty">Set up email in the <a href="#" class="today-link-mail">Mail</a> view to see messages here</div>`;
      emailsEl.querySelector('.today-link-mail')?.addEventListener('click', (e) => {
        e.preventDefault();
        const mailNav = document.querySelector('[data-view="mail"]') as HTMLElement;
        mailNav?.click();
      });
      return;
    }

    if (unreadItems.length === 0) {
      emailsEl.innerHTML = `<div class="today-section-empty"><span class="ms ms-sm">mark_email_read</span> No unread emails — you're all caught up!</div>`;
      return;
    }

    // Sort by date descending
    unreadItems.sort((a, b) => (b.date?.getTime() ?? 0) - (a.date?.getTime() ?? 0));

    emailsEl.innerHTML = unreadItems
      .slice(0, 8)
      .map((email) => {
        const timeStr = email.date
          ? email.date.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' })
          : '';
        return `
        <div class="today-email-item">
          <div class="today-email-from">${escHtml(email.from)}</div>
          <div class="today-email-subject">${escHtml(email.subject)}</div>
          ${timeStr ? `<div class="today-email-time">${timeStr}</div>` : ''}
        </div>
      `;
      })
      .join('');

    if (unreadItems.length > 8) {
      emailsEl.innerHTML += `<div class="today-email-more">+${unreadItems.length - 8} more unread</div>`;
    }
  } catch (e) {
    console.warn('[today] Email fetch failed:', e);
    emailsEl.innerHTML = `<div class="today-section-empty">Could not load emails — check Mail settings</div>`;
  }
}

// ── Calendar ──────────────────────────────────────────────────────────

export async function fetchCalendarEvents() {
  const calEl = $('today-calendar');
  if (!calEl) return;

  try {
    calEl.innerHTML = `<div class="today-section-empty">Connect a calendar integration via <a href="#" class="today-link-integrations">Integrations</a> to see events here</div>`;
    calEl.querySelector('.today-link-integrations')?.addEventListener('click', (e) => {
      e.preventDefault();
      switchView('integrations');
    });
  } catch (e) {
    console.warn('[today] Calendar fetch failed:', e);
    calEl.innerHTML = `<div class="today-section-empty">Could not load calendar</div>`;
  }
}

// ── Cached data for synchronous render ────────────────────────────────
let _skillOutputs: SkillOutput[] = [];
let _activeSkills: EngineSkillStatus[] = [];

/** Fetch skill outputs from backend and re-render if any found. */
export async function fetchSkillOutputs() {
  try {
    const outputs = await pawEngine.listSkillOutputs();
    _skillOutputs = outputs ?? [];

    const widgetContainer = document.getElementById('today-skill-widgets');
    if (widgetContainer) {
      widgetContainer.innerHTML = renderSkillWidgets(_skillOutputs);
    }
  } catch (e) {
    console.warn('[today] Skill outputs fetch failed:', e);
    _skillOutputs = [];
  }
}

/** Fetch enabled skills list and populate the Active Skills card. */
export async function fetchActiveSkills() {
  const container = $('cmd-skills-body');
  if (!container) return;

  // Showcase mode — render demo skill chips
  const showcase = isShowcaseActive() ? getShowcaseData() : null;
  if (showcase) {
    const countEl = $('cmd-skills-count');
    if (countEl) countEl.textContent = String(showcase.skillNames.length);
    container.innerHTML = `
      <div class="cmd-skills-grid k-stagger">
        ${showcase.skillNames.map((n) => `<span class="cmd-skill-chip k-row k-breathe k-materialise k-status-healthy">${kineticDot()} ${escHtml(n)}</span>`).join('')}
      </div>
    `;
    return;
  }

  try {
    const all = await pawEngine.skillsList();
    _activeSkills = all.filter((s) => s.enabled);

    const container = $('cmd-skills-body');
    if (!container) return;

    const countEl = $('cmd-skills-count');
    if (countEl) countEl.textContent = String(_activeSkills.length);

    if (_activeSkills.length === 0) {
      container.innerHTML = `<div class="today-section-empty">No skills enabled — add some in Settings → Skills</div>`;
      return;
    }

    const shown = _activeSkills.slice(0, 8);
    const remaining = _activeSkills.length - shown.length;

    container.innerHTML = `
      <div class="cmd-skills-grid k-stagger">
        ${shown
          .map((s) => {
            const kStatus: KineticStatus = s.is_ready ? 'healthy' : 'idle';
            return `<span class="cmd-skill-chip k-row k-breathe k-materialise k-status-${kStatus}" title="${escHtml(s.name)}">
                ${kineticDot()} ${escHtml(s.name)}
              </span>`;
          })
          .join('')}
      </div>
      ${remaining > 0 ? `<div class="cmd-skills-more">+ ${remaining} more</div>` : ''}
    `;
  } catch (e) {
    console.warn('[today] Skills list fetch failed:', e);
    if (container)
      container.innerHTML = `<div class="today-section-empty">Could not load skills</div>`;
  }
}

/** Populate the Capabilities card with grouped skill descriptions. */
export async function fetchCapabilities() {
  const container = $('cmd-capabilities-body');
  if (!container) return;

  try {
    // Use already-fetched skills if available, otherwise fetch
    let skills = _activeSkills;
    if (skills.length === 0) {
      const all = await pawEngine.skillsList();
      skills = all.filter((s) => s.enabled);
    }

    if (skills.length === 0) {
      container.innerHTML = `
        <div class="today-section-empty">
          Enable skills in Settings to unlock agent capabilities
        </div>
        <button class="btn btn-primary btn-sm capabilities-goto-skills" style="margin-top:8px">
          <span class="ms ms-sm">bolt</span> Browse Skills
        </button>
      `;
      container.querySelector('.capabilities-goto-skills')?.addEventListener('click', () => {
        switchView('settings-skills');
      });
      return;
    }

    const groups = buildCapabilityGroups(skills);
    container.innerHTML = renderCapabilityGroups(groups, skills.length);
  } catch (e) {
    console.warn('[today] Capabilities fetch failed:', e);
    container.innerHTML = `<div class="today-section-empty">Could not load capabilities</div>`;
  }
}

function renderCapabilityGroups(groups: CapabilityGroup[], totalSkills: number): string {
  const groupsHtml = groups
    .slice(0, 6)
    .map(
      (g) => `
      <div class="cap-group">
        <div class="cap-group-header">
          <span class="ms ms-sm">${g.icon}</span>
          <span class="cap-group-label">${escHtml(g.label)}</span>
        </div>
        <div class="cap-group-items">
          ${g.capabilities
            .slice(0, 3)
            .map((c) => `<span class="cap-item">${escHtml(c)}</span>`)
            .join('')}
          ${g.capabilities.length > 3 ? `<span class="cap-item cap-more">+${g.capabilities.length - 3} more</span>` : ''}
        </div>
      </div>`,
    )
    .join('');

  const moreGroups =
    groups.length > 6
      ? `<div class="cap-overflow">+${groups.length - 6} more categories</div>`
      : '';

  return `
    <div class="cap-summary">${totalSkills} skill${totalSkills !== 1 ? 's' : ''} across ${groups.length} ${groups.length !== 1 ? 'categories' : 'category'}</div>
    <div class="cap-groups">${groupsHtml}</div>
    ${moreGroups}
  `;
}

/** Populate the agent fleet status card. */
export async function fetchFleetStatus(retries = 3) {
  const container = $('cmd-fleet-body');
  if (!container) return;

  // Showcase mode — render demo agents
  const showcase = isShowcaseActive() ? getShowcaseData() : null;
  if (showcase) {
    container.innerHTML = `<div class="k-stagger">${showcase.agents
      .map((a) => {
        const status = agentStatus(a.lastUsed);
        const kStatus: KineticStatus = status === 'active' ? 'healthy' : 'idle';
        return `<div class="cmd-fleet-item k-row k-breathe k-materialise k-status-${kStatus}">
          ${kineticDot()}
          <span class="cmd-fleet-name">${escHtml(a.name)}</span>
          <span class="cmd-fleet-status">[${status}]</span>
        </div>`;
      })
      .join('')}</div>`;
    return;
  }

  try {
    let agents = getAgents();
    // Agents may not be loaded yet — retry with delay
    if (agents.length === 0 && retries > 0) {
      await new Promise((r) => setTimeout(r, 600));
      agents = getAgents();
    }
    // Still empty — force a full agent load (initAgents may not have finished)
    if (agents.length === 0) {
      try {
        await loadAgents();
        agents = getAgents();
      } catch (e) {
        console.warn('[today] loadAgents fallback failed:', e);
      }
    }
    if (agents.length === 0) {
      container.innerHTML = `<div class="today-section-empty">No agents configured — <a href="#" data-view="agents" style="color:var(--accent)">create one</a></div>`;
      container.querySelector('[data-view="agents"]')?.addEventListener('click', (e) => {
        e.preventDefault();
        switchView('agents');
      });
      return;
    }

    container.innerHTML = `<div class="k-stagger">${agents
      .map((a) => {
        const status = agentStatus(a.lastUsed);
        const kStatus: KineticStatus = status === 'active' ? 'healthy' : 'idle';
        return `<div class="cmd-fleet-item k-row k-breathe k-materialise k-status-${kStatus}" data-agent-id="${escHtml(a.id)}" title="Open chat with ${escHtml(a.name)}">
          ${kineticDot()}
          <span class="cmd-fleet-name">${escHtml(a.name)}</span>
          <span class="cmd-fleet-status">[${status}]</span>
        </div>`;
      })
      .join('')}</div>`;

    // Wire click → open full chat with the selected agent
    container.querySelectorAll<HTMLElement>('.cmd-fleet-item[data-agent-id]').forEach((el) => {
      el.addEventListener('click', () => {
        const agentId = el.dataset.agentId;
        if (agentId) {
          setSelectedAgent(agentId);
          switchView('chat');
        }
      });
    });
  } catch (e) {
    console.warn('[today] Fleet status failed:', e);
    container.innerHTML = `<div class="today-section-empty">Could not load agents — try refreshing</div>`;
  }
}

/** Populate the 30-day activity heatmap card. */
export async function fetchHeatmap() {
  const container = $('cmd-heatmap-body');
  if (!container) return;

  try {
    // Combine task activity + chat sessions for a complete picture
    const [taskItems, sessions] = await Promise.all([
      pawEngine.taskActivity(undefined, 500).catch(() => []),
      pawEngine.sessionsList(500).catch(() => []),
    ]);
    const days = buildHeatmapData(taskItems, sessions);
    container.innerHTML = heatmapStrip(days);
  } catch (e) {
    console.warn('[today] Heatmap fetch failed:', e);
    container.innerHTML = `<div class="today-section-empty">No activity data</div>`;
  }
}

export function renderToday() {
  const container = $('today-content');
  if (!container) return;

  const showcase = isShowcaseActive() ? getShowcaseData() : null;
  const tasks = showcase ? showcase.tasks : _state.getTasks();
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

  // Usage stats — showcase overrides or real data
  const tokensUsed = showcase ? showcase.tokenCount : appState.sessionTokensUsed;
  const cost = showcase ? showcase.cost : appState.sessionCost;

  container.innerHTML = `
    <div class="today-header bento-row">
      <div class="today-greeting-cell">
        <div class="today-label">MISSION CONTROL</div>
        <div class="today-greeting">${greeting}${userName ? `, ${escHtml(userName)}` : ''}</div>
        <div class="today-date">${dateStr}</div>
      </div>
      <div class="today-header-right">
        <div class="today-usage-strip">
          <span class="today-usage-item"><span class="today-usage-val" id="cmd-tokens">${formatTokens(tokensUsed)}</span> <span class="today-usage-lbl">tokens</span></span>
          <span class="today-usage-sep">·</span>
          <span class="today-usage-item"><span class="today-usage-val" id="cmd-cost">${formatCost(cost)}</span> <span class="today-usage-lbl">cost</span></span>
          <span class="today-usage-sep">·</span>
          <span class="today-usage-item"><span class="today-usage-val" id="cmd-input-tokens">${formatTokens(appState.sessionInputTokens)}</span> <span class="today-usage-lbl">in</span></span>
          <span class="today-usage-sep">·</span>
          <span class="today-usage-item"><span class="today-usage-val" id="cmd-output-tokens">${formatTokens(appState.sessionOutputTokens)}</span> <span class="today-usage-lbl">out</span></span>
        </div>
        <div class="today-weather-cell" id="today-weather">
          <span class="today-loading">…</span>
        </div>
      </div>
    </div>

    <div class="cmd-grid bento-grid">
      <!-- Row 1: Tasks + Calendar (your day) -->
      <div class="cmd-card bento-cell bento-span-6">
        <div class="today-card-header">
          <span class="today-card-title">TASKS</span>
          <span class="today-card-count">${pendingTasks.length}</span>
          <button class="btn btn-ghost btn-sm today-add-task-btn">+ Add</button>
        </div>
        <div class="today-card-body">
          <div class="today-tasks" id="today-tasks">
            ${
              pendingTasks.length === 0
                ? `<div class="today-section-empty">No tasks yet. Add one to get started!</div>`
                : pendingTasks
                    .map(
                      (task) => `
                <div class="today-task" data-id="${task.id}">
                  <input type="checkbox" class="today-task-check" ${task.done ? 'checked' : ''}>
                  <span class="today-task-text">${escHtml(task.text)}</span>
                  <button class="today-task-delete" title="Delete">×</button>
                </div>`,
                    )
                    .join('')
            }
          </div>
          ${completedToday.length > 0 ? `<div class="today-completed-label">${completedToday.length} completed today</div>` : ''}
        </div>
      </div>

      <div class="cmd-card bento-cell bento-span-6">
        <div class="today-card-header">
          <span class="today-card-title">CALENDAR</span>
        </div>
        <div class="today-card-body" id="today-calendar">
          <span class="today-loading">Loading…</span>
        </div>
      </div>

      <!-- Row 2: Inbox + Quick Actions -->
      <div class="cmd-card bento-cell bento-span-6">
        <div class="today-card-header">
          <span class="today-card-title">UNREAD MAIL</span>
        </div>
        <div class="today-card-body" id="today-emails">
          <span class="today-loading">Loading…</span>
        </div>
      </div>

      <div class="cmd-card bento-cell bento-span-6">
        <div class="today-card-header">
          <span class="today-card-title">QUICK ACTIONS</span>
        </div>
        <div class="today-card-body">
          <button class="today-quick-action" id="today-briefing-btn">
            ▸ Morning Briefing
          </button>
          <button class="today-quick-action" id="today-summarize-btn">
            ▸ Summarize Inbox
          </button>
          <button class="today-quick-action" id="today-schedule-btn">
            ▸ What's on today?
          </button>
        </div>
      </div>

      <!-- Row 3: Fleet + Skills + Activity -->
      <div class="cmd-card bento-cell bento-span-4">
        <div class="today-card-header">
          <span class="today-card-title">AGENT FLEET</span>
        </div>
        <div class="today-card-body" id="cmd-fleet-body">
          <span class="today-loading">Loading…</span>
        </div>
      </div>

      <div class="cmd-card bento-cell bento-span-4">
        <div class="today-card-header">
          <span class="today-card-title">SKILLS</span>
          <span class="today-card-count" id="cmd-skills-count">…</span>
        </div>
        <div class="today-card-body" id="cmd-skills-body">
          <span class="today-loading">Loading…</span>
        </div>
      </div>

      <div class="cmd-card bento-cell bento-span-4">
        <div class="today-card-header">
          <span class="today-card-title">ACTIVITY</span>
        </div>
        <div class="today-card-body" id="today-activity">
          <span class="today-loading">Loading…</span>
        </div>
      </div>

      <!-- Integrations (full width) -->
      <div class="cmd-card bento-cell bento-span-full">
        <div class="today-card-header">
          <span class="today-card-title">INTEGRATIONS</span>
          <span class="today-card-count" id="cmd-integrations-count">…</span>
        </div>
        <div class="today-card-body" id="cmd-integrations-body">
          <span class="today-loading">Loading…</span>
        </div>
      </div>

      <!-- Row 5: Heatmap + Capabilities -->
      <div class="cmd-card bento-cell bento-span-6">
        <div class="today-card-header">
          <span class="today-card-title">30-DAY HEATMAP</span>
        </div>
        <div class="today-card-body" id="cmd-heatmap-body">
          <span class="today-loading">Loading…</span>
        </div>
      </div>

      <div class="cmd-card bento-cell bento-span-6 capabilities-card">
        <div class="today-card-header">
          <span class="today-card-title">CAPABILITIES</span>
        </div>
        <div class="today-card-body" id="cmd-capabilities-body">
          <span class="today-loading">Loading…</span>
        </div>
      </div>

      <!-- Skill Widgets -->
      <div class="bento-cell bento-span-full" id="today-skill-widgets">
        ${renderSkillWidgets(_skillOutputs)}
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

  // ── Kinetic: apply spring hover to bento cards ──
  document.querySelectorAll('.cmd-card').forEach((card) => {
    kineticRow(card as HTMLElement, { spring: true, springCard: true });
  });

  // ── Kinetic: stagger-materialise skill chips and fleet items ──
  const fleetStagger = document.querySelector('#cmd-fleet-body .k-stagger');
  if (fleetStagger) kineticStagger(fleetStagger as HTMLElement, '.cmd-fleet-item');

  const skillsStagger = document.querySelector('#cmd-skills-body .k-stagger');
  if (skillsStagger) kineticStagger(skillsStagger as HTMLElement, '.cmd-skill-chip');

  // Integration health dashboard wiring
  loadIntegrationsDashboard();
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

// ── Task CRUD (engine-backed) ─────────────────────────────────────────

async function addTask(text: string) {
  try {
    await pawEngine.taskCreate({
      id: '',
      title: text,
      description: '',
      status: 'inbox',
      priority: 'medium',
      assigned_agents: [],
      cron_enabled: false,
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    });
    await reloadTodayTasks();
    showToast('Task added');
  } catch (e) {
    console.error('[today] addTask failed:', e);
    showToast('Failed to add task', 'error');
  }
}

async function toggleTask(taskId: string) {
  try {
    const tasks = await pawEngine.tasksList();
    const task = tasks.find((t) => t.id === taskId);
    if (!task) return;
    const newStatus = toggledStatus(task.status);
    await pawEngine.taskMove(taskId, newStatus);
    await reloadTodayTasks();
  } catch (e) {
    console.error('[today] toggleTask failed:', e);
    showToast('Failed to update task', 'error');
  }
}

async function deleteTask(taskId: string) {
  try {
    await pawEngine.taskDelete(taskId);
    await reloadTodayTasks();
  } catch (e) {
    console.error('[today] deleteTask failed:', e);
    showToast('Failed to delete task', 'error');
  }
}

/** Reload tasks from engine and update in-place (or full re-render if called standalone). */
export async function reloadTodayTasks(inPlace = false) {
  try {
    const all = await pawEngine.tasksList();
    const filtered = filterTodayTasks(all);
    const mapped = filtered.map(engineTaskToToday);
    _state.setTasks(mapped);

    if (inPlace) {
      // Update only the tasks section without destroying other cards' DOM
      const tasksContainer = $('today-tasks');
      if (!tasksContainer) return;

      const pendingTasks = mapped.filter((t) => !t.done);
      const completedToday = mapped.filter((t) => t.done && isToday(t.createdAt));

      tasksContainer.innerHTML =
        pendingTasks.length === 0
          ? `<div class="today-section-empty">No tasks yet. Add one to get started!</div>`
          : pendingTasks
              .map(
                (task) => `
            <div class="today-task" data-id="${task.id}">
              <input type="checkbox" class="today-task-check" ${task.done ? 'checked' : ''}>
              <span class="today-task-text">${escHtml(task.text)}</span>
              <button class="today-task-delete" title="Delete">×</button>
            </div>`,
              )
              .join('');

      // Update count badge
      const countEl = tasksContainer.closest('.cmd-card')?.querySelector('.today-card-count');
      if (countEl) countEl.textContent = String(pendingTasks.length);

      // Update completed label
      const parent = tasksContainer.parentElement;
      const existing = parent?.querySelector('.today-completed-label');
      if (existing) existing.remove();
      if (completedToday.length > 0 && parent) {
        parent.insertAdjacentHTML(
          'beforeend',
          `<div class="today-completed-label">${completedToday.length} completed today</div>`,
        );
      }

      // Re-bind task events
      tasksContainer.querySelectorAll('.today-task-check').forEach((checkbox) => {
        checkbox.addEventListener('change', (e) => {
          const taskEl = (e.target as HTMLElement).closest('.today-task');
          const taskId = taskEl?.getAttribute('data-id');
          if (taskId) toggleTask(taskId);
        });
      });
      tasksContainer.querySelectorAll('.today-task-delete').forEach((btn) => {
        btn.addEventListener('click', (e) => {
          const taskEl = (e.target as HTMLElement).closest('.today-task');
          const taskId = taskEl?.getAttribute('data-id');
          if (taskId) deleteTask(taskId);
        });
      });
    } else {
      // Full re-render (used when called standalone, e.g. from task CRUD)
      renderToday();
    }
  } catch (e) {
    console.error('[today] reloadTodayTasks failed:', e);
  }
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

// ── Integration Dashboard Loader ──────────────────────────────────────

async function loadIntegrationsDashboard() {
  const body = $('cmd-integrations-body');
  const countEl = $('cmd-integrations-count');
  if (!body) return;

  try {
    let health = await loadServiceHealth();

    // Fallback: if health check returned nothing, try listing connected IDs directly
    if (health.length === 0 && invoke) {
      try {
        const ids = await invoke<string[]>('engine_integrations_list_connected');
        if (ids && ids.length > 0) {
          health = ids.map(
            (id) =>
              ({
                service: id,
                serviceName: id.replace(/_/g, ' ').replace(/\b\w/g, (c: string) => c.toUpperCase()),
                icon: 'extension',
                status: 'ok',
                lastChecked: new Date().toISOString(),
                message: null,
                tokenExpiry: null,
                daysUntilExpiry: null,
                recentFailures: 0,
                todayActions: 0,
              }) as never,
          );
        }
      } catch {
        /* fallback failed, continue with empty */
      }
    }

    const connectedIds = health.map((h: { service: string }) => h.service);

    if (countEl) {
      countEl.textContent = health.length > 0 ? String(health.length) : '0';
    }

    if (health.length === 0) {
      body.innerHTML = `
        <div style="display:flex;align-items:center;gap:8px;font-size:13px;color:var(--text-secondary)">
          <span class="ms ms-sm">hub</span>
          <span>No services connected · <a href="#" class="integ-browse" style="color:var(--accent);text-decoration:none">Browse integrations</a></span>
        </div>`;
      body.querySelector('.integ-browse')?.addEventListener('click', (e) => {
        e.preventDefault();
        switchView('integrations');
      });
      return;
    }

    const html = await renderDashboardIntegrations(connectedIds);
    body.innerHTML = html;
    wireDashboardEvents(body);
  } catch {
    body.innerHTML = `
      <div style="display:flex;align-items:center;gap:8px;font-size:13px;color:var(--text-secondary)">
        <span class="ms ms-sm">hub</span>
        <span>25,000+ integrations available · <a href="#" class="integ-browse" style="color:var(--accent);text-decoration:none">Browse all</a></span>
      </div>`;
    body.querySelector('.integ-browse')?.addEventListener('click', (e) => {
      e.preventDefault();
      switchView('integrations');
    });
    if (countEl) countEl.textContent = '0';
  }
}
