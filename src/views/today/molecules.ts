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
  getPawzMessage,
  buildCapabilityGroups,
} from './atoms';
import { renderSkillWidgets } from '../../components/molecules/skill-widget';
import type {
  SkillOutput,
  EngineSkillStatus,
  TelemetryDailySummary,
  TelemetryModelBreakdown,
  EngineEvent,
} from '../../engine/atoms/types';
import { appState } from '../../state';
import {
  renderDashboardIntegrations,
  wireDashboardEvents,
  loadServiceHealth,
} from '../../features/integration-health';
import { sparkline } from '../../components/molecules/data-viz';
import { isShowcaseActive, getShowcaseData } from '../../components/showcase';
import {
  kineticRow,
  kineticStagger,
  kineticDot,
  type KineticStatus,
} from '../../components/kinetic-row';
import { createHeroLogo, type HeroLogoInstance } from '../../components/hero-logo';
import { renderPalaceGraphInto } from '../memory-palace/graph';

// ── Skeleton loading helper ──────────────────────────────────────────
function skelLines(n = 3): string {
  return Array.from(
    { length: n },
    (_, i) =>
      `<div class="today-skel${i === 1 ? ' today-skel-short' : i === 2 ? ' today-skel-wider' : ''}"></div>`,
  ).join('');
}

// ── Recall time-ago helper ────────────────────────────────────────────
function _timeAgo(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const min = Math.floor(diff / 60_000);
  if (min < 1) return 'just now';
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h ago`;
  return `${Math.floor(hr / 24)}d ago`;
}

// ── Hero logo + Engram brain instances ─────────────────────────────────
let _heroLogo: HeroLogoInstance | null = null;
let _recallUnsub: (() => void) | null = null;
let _engramCategories: [string, number][] = [];

// ── Tauri bridge (lazy — resolves at call time, not module load) ──────
// The @tauri-apps/api/core invoke is always available in the Tauri
// desktop app. Using a lazy getter avoids the race condition where
// window.__TAURI__ isn't injected yet when the module first loads.
function getInvoke() {
  const w = window as unknown as Record<string, unknown>;
  return w.__TAURI__
    ? (
        w.__TAURI__ as {
          core: {
            invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
          };
        }
      ).core.invoke
    : undefined;
}

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
  inp.className = 'form-input';
  inp.style.cssText =
    'font-size:12px;padding:4px 8px;border-radius:6px;border:1px solid var(--border);background:var(--bg-secondary);color:var(--text);width:200px;outline:none';
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

    const invoke = getInvoke();
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

  const invoke = getInvoke();
  if (!invoke) {
    emailsEl.innerHTML = `<div class="today-section-empty">Email requires the desktop app</div>`;
    return;
  }

  try {
    interface UnreadItem {
      from: string;
      subject: string;
      date: Date | null;
      source: 'himalaya' | 'google';
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

    // ── Gmail API emails ─────────────────────────────────────────────
    let hasGmail = false;
    try {
      const gmailMessages = await pawEngine.gmailInbox(10);
      if (gmailMessages.length > 0) hasGmail = true;
      for (const gm of gmailMessages) {
        if (!gm.read) {
          unreadItems.push({
            from: gm.from,
            subject: gm.subject,
            date: gm.date ? new Date(gm.date) : null,
            source: 'google',
          });
        }
      }
    } catch (e) {
      console.warn('[today] Gmail email fetch failed:', e);
    }

    // ── No email sources configured ───────────────────────────────
    if (unreadItems.length === 0 && himalayaAccounts.length === 0 && !hasGmail) {
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

interface CalendarEvent {
  id: string;
  summary: string;
  start: string;
  end: string;
  location: string | null;
  allDay: boolean;
}

export async function fetchCalendarEvents() {
  const calEl = $('today-calendar');
  if (!calEl) return;

  const invoke = getInvoke();
  if (!invoke) {
    calEl.innerHTML = `<div class="today-section-empty">Calendar requires the desktop app</div>`;
    return;
  }

  try {
    const events = await invoke<CalendarEvent[]>('engine_calendar_events_today');

    if (events.length === 0) {
      // Check if Google Calendar is connected at all
      let connected: string[] = [];
      try {
        connected = await invoke<string[]>('engine_integrations_list_connected');
      } catch {
        /* ignore */
      }
      const calCountEl = document.getElementById('today-calendar-count');
      if (calCountEl) calCountEl.textContent = '0';

      if (!connected.includes('google-calendar') && !connected.includes('google-workspace')) {
        calEl.innerHTML = `<div class="today-section-empty">Connect a calendar integration via <a href="#" class="today-link-integrations">Integrations</a> to see events here</div>`;
        calEl.querySelector('.today-link-integrations')?.addEventListener('click', (e) => {
          e.preventDefault();
          switchView('integrations');
        });
      } else {
        calEl.innerHTML = `<div class="today-section-empty"><span class="ms ms-sm">event_available</span> No events today</div>`;
      }
      return;
    }

    calEl.innerHTML = events
      .map((ev) => {
        let timeStr = '';
        if (ev.allDay) {
          timeStr = 'All day';
        } else if (ev.start) {
          try {
            const d = new Date(ev.start);
            timeStr = d.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' });
            if (ev.end) {
              const end = new Date(ev.end);
              timeStr += ` – ${end.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' })}`;
            }
          } catch {
            timeStr = '';
          }
        }
        return `
        <div class="today-cal-event">
          <div class="today-cal-time">${timeStr || 'TBD'}</div>
          <div class="today-cal-details">
            <div class="today-cal-summary">${escHtml(ev.summary)}</div>
            ${ev.location ? `<div class="today-cal-location"><span class="ms ms-xs">place</span>${escHtml(ev.location)}</div>` : ''}
          </div>
        </div>`;
      })
      .join('');
    const calCountEl = document.getElementById('today-calendar-count');
    if (calCountEl) calCountEl.textContent = String(events.length);
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
      ${
        buildCapabilityGroups(_activeSkills).length > 0
          ? `<div class="cmd-skills-cats">${buildCapabilityGroups(_activeSkills)
              .map((g) => escHtml(g.label))
              .join(' · ')}</div>`
          : ''
      }
    `;
    const skillsStagger = container.querySelector('.k-stagger');
    if (skillsStagger) kineticStagger(skillsStagger as HTMLElement, '.cmd-skill-chip');
  } catch (e) {
    console.warn('[today] Skills list fetch failed:', e);
    if (container)
      container.innerHTML = `<div class="today-section-empty">Could not load skills</div>`;
  }
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
    const fleetStagger = container.querySelector('.k-stagger');
    if (fleetStagger) kineticStagger(fleetStagger as HTMLElement, '.cmd-fleet-item');
  } catch (e) {
    console.warn('[today] Fleet status failed:', e);
    container.innerHTML = `<div class="today-section-empty">Could not load agents — try refreshing</div>`;
  }
}

// ── Time-ago formatter ───────────────────────────────────────────────
function timeAgo(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

/** Fetch Engram memory stats and render the ENGRAM card stats column. */
export async function fetchEngramStats() {
  const countEl = $('engram-memory-count');
  const statsCol = $('engram-stats-col');

  try {
    const stats = await pawEngine.memoryStats();
    _engramCategories = stats.categories;

    if (countEl) countEl.textContent = stats.total_memories.toLocaleString();

    if (statsCol) {
      const topCats = stats.categories.slice(0, 8);
      const embBadge = stats.has_embeddings
        ? `<div class="engram-embed-badge"><span class="ms ms-xs">hub</span> vector search active</div>`
        : '';
      statsCol.innerHTML = `
        <div>
          <div class="engram-stat-total">${stats.total_memories.toLocaleString()}</div>
          <div class="engram-stat-label">memories stored</div>
          ${embBadge}
        </div>
        <div class="engram-categories">
          ${topCats.map(([cat, count]) => `<span class="engram-cat-chip">${escHtml(cat)}<span class="engram-cat-count">${count}</span></span>`).join('')}
        </div>
        <div class="engram-hint">Ctrl+M · click brain to store</div>
      `;
    }
  } catch (e) {
    console.warn('[today] Engram stats failed:', e);
    if (countEl) countEl.textContent = '—';
    if (statsCol) {
      statsCol.innerHTML = `
        <div class="engram-stat-total">—</div>
        <div class="engram-stat-label">memory engine offline</div>
        <div class="engram-hint">Configure in Settings → Memory</div>
      `;
    }
  }
}

// ── Recall ────────────────────────────────────────────────────────────
/** No-op export — recall is user-triggered, not auto-fetched on load. */
export async function fetchRecall() {
  // intentionally empty — card is populated from localStorage on hydration
}

/** Compile recent data and stream an AI recap into the RECALL card. */
async function _generateRecall() {
  const btn = $('recall-btn') as HTMLButtonElement | null;
  const out = $('recall-output');
  if (!out || !btn) return;

  // Cancel any in-flight run (delta + complete + error unsubs are all in _recallUnsub)
  _recallUnsub?.();
  _recallUnsub = null;

  btn.disabled = true;
  btn.textContent = '⟳ Generating…';
  out.innerHTML = '<span class="recall-cursor"></span>';

  try {
    // ── Gather data ──────────────────────────────────────────────────
    const [sessions, mems] = await Promise.all([
      pawEngine
        .sessionsList(8)
        .catch(() => [] as Awaited<ReturnType<typeof pawEngine.sessionsList>>),
      pawEngine.memoryList(30).catch(() => [] as Awaited<ReturnType<typeof pawEngine.memoryList>>),
    ]);

    // Sample last few user messages from the 3 most recent sessions
    const recentMsgs: string[] = [];
    for (const s of sessions.slice(0, 3)) {
      try {
        const hist = await pawEngine.chatHistory(s.id, 6);
        const userMsgs = hist.filter((m) => m.role === 'user').slice(-2);
        if (userMsgs.length) {
          const label = s.label || 'Session';
          recentMsgs.push(`[${label}] ${userMsgs.map((m) => m.content.slice(0, 120)).join(' / ')}`);
        }
      } catch {
        // non-fatal — skip this session
      }
    }

    // Today's memories
    const todayStr = new Date().toDateString();
    const todayMems = mems
      .filter((m) => new Date(m.created_at).toDateString() === todayStr)
      .slice(0, 10)
      .map((m) => `• ${m.content.slice(0, 100)}`);

    const lines = [
      'Write a concise, friendly 2–3 sentence recap of what this user has been working on today.',
      'Be specific — use their actual topics. No bullet points. Write in second person ("You\'ve been…").',
      '',
      sessions.length
        ? `Recent sessions: ${sessions.map((s) => s.label || 'Untitled').join(', ')}`
        : '',
      recentMsgs.length ? `Recent messages:\n${recentMsgs.join('\n')}` : '',
      todayMems.length ? `Memories captured today:\n${todayMems.join('\n')}` : '',
    ].filter(Boolean);

    if (lines.length <= 2) {
      out.innerHTML =
        '<div class="recall-empty">No activity found yet — start a chat or save some memories!</div>';
      btn.disabled = false;
      btn.textContent = '↺ Recap';
      return;
    }

    const prompt = lines.join('\n');

    // ── Stream response ──────────────────────────────────────────────
    // Use a fresh session ID every invocation so there is zero conversation
    // history — a fixed session accumulates prior recap exchanges as context
    // and makes the LLM produce inconsistent follow-up responses.
    const sessionId = crypto.randomUUID();
    let accumulated = '';

    const cleanup = () => {
      _recallUnsub?.();
      _recallUnsub = null;
    };

    // Filter by session_id (known before chatSend) to avoid the run_id
    // race where _recallRunId is still null when the first delta fires.
    const unDelta = pawEngine.on('delta', (ev: EngineEvent) => {
      if (ev.session_id !== sessionId || !ev.text) return;
      accumulated += ev.text;
      out.innerHTML = `${escHtml(accumulated)}<span class="recall-cursor"></span>`;
    });

    const unComplete = pawEngine.on('complete', (ev: EngineEvent) => {
      if (ev.session_id !== sessionId) return;
      // ev.text is the full assembled response from Rust — use it as the
      // authoritative final text instead of accumulated deltas, which can
      // be incomplete if any delta events were dropped or arrived out of order.
      const finalText = ev.text || accumulated;
      cleanup();
      out.innerHTML = escHtml(finalText);
      if (finalText) {
        localStorage.setItem('paw-recall-text', finalText);
        localStorage.setItem('paw-recall-ts', new Date().toISOString());
      }
      btn.disabled = false;
      btn.textContent = '↺ Recap';
    });

    const unError = pawEngine.on('error', (ev: EngineEvent) => {
      if (ev.session_id !== sessionId) return;
      cleanup();
      out.innerHTML = accumulated
        ? escHtml(accumulated)
        : '<div class="recall-empty">Could not generate recap — check your AI provider.</div>';
      btn.disabled = false;
      btn.textContent = '↺ Recap';
    });

    // Bundle all three so a subsequent click cancels them atomically
    _recallUnsub = () => {
      unDelta();
      unComplete();
      unError();
    };

    await pawEngine.chatSend({
      session_id: sessionId,
      message: prompt,
      system_prompt:
        'You are a concise daily recap assistant. Summarize what the user has been doing today.',
      tools_enabled: false,
      temperature: 0.4,
    });
  } catch (e) {
    console.warn('[today] Recall generation failed:', e);
    _recallUnsub?.();
    _recallUnsub = null;
    out.innerHTML =
      '<div class="recall-empty">Could not generate recap — check your AI provider.</div>';
    btn.disabled = false;
    btn.textContent = '↺ Recap';
  }
}

/** Fetch and render the last 5 chat sessions. */
export async function fetchRecentSessions() {
  const container = $('today-sessions');
  const countEl = $('today-sessions-count');
  if (!container) return;

  try {
    const sessions = await pawEngine.sessionsList(6);

    if (sessions.length === 0) {
      if (countEl) countEl.textContent = '0';
      container.innerHTML = `<div class="today-section-empty">No sessions yet — start a chat to begin</div>`;
      return;
    }

    if (countEl) countEl.textContent = String(sessions.length);
    const agents = getAgents();

    container.innerHTML = sessions
      .slice(0, 5)
      .map((s) => {
        const agentName = s.agent_id
          ? (agents.find((a) => a.id === s.agent_id)?.name ?? null)
          : null;
        const label = s.label || 'Untitled Session';
        const rawModel = s.model ?? '';
        const modelShort = rawModel.includes('/') ? rawModel.split('/').pop()! : rawModel;
        const meta = [
          agentName,
          modelShort,
          `${s.message_count} msg${s.message_count !== 1 ? 's' : ''}`,
        ]
          .filter(Boolean)
          .join(' · ');
        return `
          <div class="today-session-item" data-session-id="${escHtml(s.id)}">
            <div class="today-session-dot"></div>
            <div class="today-session-info">
              <div class="today-session-label">${escHtml(label)}</div>
              <div class="today-session-meta">${escHtml(meta)}</div>
            </div>
            <div class="today-session-time">${timeAgo(s.updated_at)}</div>
          </div>`;
      })
      .join('');

    container.querySelectorAll<HTMLElement>('.today-session-item').forEach((el) => {
      el.addEventListener('click', () => {
        const sessionId = el.dataset.sessionId;
        if (sessionId) {
          appState.currentSessionKey = sessionId;
          switchView('chat');
        }
      });
    });
  } catch (e) {
    console.warn('[today] Recent sessions failed:', e);
    container.innerHTML = `<div class="today-section-empty">Could not load sessions</div>`;
  }
}

// ── Quick Memory Modal ────────────────────────────────────────────────────

function showQuickMemoryModal() {
  document.querySelector('.qmem-overlay')?.remove();

  const stored = _engramCategories.map(([c]) => c);
  const defaults = ['general', 'fact', 'task', 'preference', 'code', 'research', 'note'];
  const allCats = [...new Set([...defaults, ...stored])];

  const overlay = document.createElement('div');
  overlay.className = 'qmem-overlay';
  overlay.innerHTML = `
    <div class="qmem-modal" role="dialog" aria-label="Store Memory">
      <div class="qmem-header">
        <span class="ms ms-sm">psychology</span>
        <span class="qmem-title">Store a Memory in Engram</span>
        <button class="qmem-close" aria-label="Close">×</button>
      </div>
      <div class="qmem-body">
        <textarea class="qmem-textarea" id="qmem-content"
          placeholder="What do you want Engram to remember? (Ctrl+Enter to save)"></textarea>
        <div class="qmem-row">
          <select class="qmem-select" id="qmem-category">
            ${allCats.map((c) => `<option value="${escHtml(c)}">${escHtml(c)}</option>`).join('')}
          </select>
          <div class="qmem-importance">
            <span>importance</span>
            <input type="range" id="qmem-importance" min="1" max="10" value="5">
            <span id="qmem-importance-val">5</span>
          </div>
        </div>
      </div>
      <div class="qmem-footer">
        <span class="qmem-hint">Ctrl+Enter to store</span>
        <button class="btn btn-ghost" id="qmem-cancel">Cancel</button>
        <button class="btn btn-primary" id="qmem-submit">Store Memory</button>
      </div>
    </div>
  `;
  document.body.appendChild(overlay);
  requestAnimationFrame(() => overlay.classList.add('visible'));

  const textarea = overlay.querySelector<HTMLTextAreaElement>('#qmem-content')!;
  const categoryEl = overlay.querySelector<HTMLSelectElement>('#qmem-category')!;
  const importanceEl = overlay.querySelector<HTMLInputElement>('#qmem-importance')!;
  const importanceVal = overlay.querySelector<HTMLSpanElement>('#qmem-importance-val')!;
  const submitBtn = overlay.querySelector<HTMLButtonElement>('#qmem-submit')!;

  textarea.focus();
  importanceEl.addEventListener('input', () => {
    importanceVal.textContent = importanceEl.value;
  });

  const close = () => {
    overlay.classList.remove('visible');
    setTimeout(() => overlay.remove(), 155);
  };

  const submit = async () => {
    const content = textarea.value.trim();
    if (!content) {
      textarea.focus();
      return;
    }
    submitBtn.disabled = true;
    submitBtn.textContent = 'Storing…';
    try {
      await pawEngine.memoryStore(content, categoryEl.value, parseInt(importanceEl.value));
      showToast('Memory stored in Engram', 'success');
      close();
      fetchEngramStats().catch(() => {});
    } catch (e) {
      console.error('[today] memoryStore failed:', e);
      showToast('Failed to store memory', 'error');
      submitBtn.disabled = false;
      submitBtn.textContent = 'Store Memory';
    }
  };

  overlay.querySelector('.qmem-close')?.addEventListener('click', close);
  overlay.querySelector('#qmem-cancel')?.addEventListener('click', close);
  submitBtn.addEventListener('click', submit);
  textarea.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') close();
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) submit();
  });
  overlay.addEventListener('click', (e) => {
    if (e.target === overlay) close();
  });

  const onEsc = (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      close();
      document.removeEventListener('keydown', onEsc);
    }
  };
  document.addEventListener('keydown', onEsc);
}

/** Populate the TELEMETRY card — 14-day trends + today's headline stats + model breakdown. */
export async function fetchTelemetry() {
  const container = $('cmd-telemetry-body');
  if (!container) return;

  // Build date range: last 14 days
  const today = new Date();
  const fmt = (d: Date) => d.toISOString().slice(0, 10);
  const start = new Date(today);
  start.setDate(start.getDate() - 13);

  try {
    const [range, modelBreakdown] = await Promise.all([
      pawEngine.getMetricsRange(fmt(start), fmt(today)).catch(() => [] as TelemetryDailySummary[]),
      pawEngine.getModelBreakdown(fmt(today)).catch(() => [] as TelemetryModelBreakdown[]),
    ]);

    // Build a complete 14-day map (fill in zeroes for missing days)
    const dayMap = new Map<string, TelemetryDailySummary>();
    range.forEach((r) => dayMap.set(r.date, r));
    const allDays: TelemetryDailySummary[] = [];
    for (let i = 0; i < 14; i++) {
      const d = new Date(start);
      d.setDate(d.getDate() + i);
      const key = fmt(d);
      allDays.push(
        dayMap.get(key) ?? {
          date: key,
          input_tokens: 0,
          output_tokens: 0,
          cost_usd: 0,
          tool_calls: 0,
          tool_duration_ms: 0,
          llm_duration_ms: 0,
          total_duration_ms: 0,
          rounds: 0,
          turn_count: 0,
        },
      );
    }

    const todayData = dayMap.get(fmt(today)) ?? allDays[allDays.length - 1];
    const totalTokensToday = todayData.input_tokens + todayData.output_tokens;
    const avgLlmMs =
      todayData.turn_count > 0 ? Math.round(todayData.llm_duration_ms / todayData.turn_count) : 0;

    // Sparkline data arrays
    const costData = allDays.map((d) => d.cost_usd);
    const tokenData = allDays.map((d) => d.input_tokens + d.output_tokens);
    const maxCost = Math.max(...costData, 0.0001);
    const maxTokens = Math.max(...tokenData, 1);

    // Build day-label ticks (first and last only)
    const tickFirst = new Date(start).toLocaleDateString('en-US', {
      month: 'short',
      day: 'numeric',
    });
    const tickLast = today.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });

    // Model breakdown bars
    const totalCostBreakdown = modelBreakdown.reduce((s, m) => s + m.cost_usd, 0) || 0.0001;
    const modelBars = modelBreakdown
      .sort((a, b) => b.cost_usd - a.cost_usd)
      .slice(0, 5)
      .map((m) => {
        const pct = ((m.cost_usd / totalCostBreakdown) * 100).toFixed(0);
        const rawModel = m.model ?? 'unknown';
        const modelLabel = rawModel.includes('/') ? rawModel.split('/').pop()! : rawModel;
        const modelShort = modelLabel.length > 22 ? `${modelLabel.slice(0, 20)}…` : modelLabel;
        return `
          <div class="telem-model-row">
            <span class="telem-model-name" title="${escHtml(rawModel)}">${escHtml(modelShort)}</span>
            <div class="telem-model-bar-wrap">
              <div class="telem-model-bar" style="width:${pct}%"></div>
            </div>
            <span class="telem-model-pct">${pct}%</span>
          </div>`;
      })
      .join('');

    container.innerHTML = `
      <div class="telem-grid">

        <!-- Col 1: Today stats -->
        <div class="telem-col telem-col-stats">
          <div class="telem-col-label">TODAY</div>
          <div class="telem-stat">
            <span class="telem-stat-val" id="telem-turns">—</span>
            <span class="telem-stat-lbl">turns</span>
          </div>
          <div class="telem-stat">
            <span class="telem-stat-val" id="telem-tools">—</span>
            <span class="telem-stat-lbl">tool calls</span>
          </div>
          <div class="telem-stat">
            <span class="telem-stat-val" id="telem-tokens">—</span>
            <span class="telem-stat-lbl">tokens</span>
          </div>
          <div class="telem-stat">
            <span class="telem-stat-val" id="telem-latency">—</span>
            <span class="telem-stat-lbl">avg latency</span>
          </div>
        </div>

        <!-- Col 2: Sparklines -->
        <div class="telem-col telem-col-charts">
          <div class="telem-chart-row">
            <span class="telem-chart-label">cost / day</span>
            <div class="telem-chart-wrap">
              ${sparkline(costData, 'var(--accent,#d4654a)', 260, 36)}
            </div>
            <span class="telem-chart-peak">${formatCost(maxCost)}</span>
          </div>
          <div class="telem-chart-ticks">
            <span>${escHtml(tickFirst)}</span>
            <span>14d</span>
            <span>${escHtml(tickLast)}</span>
          </div>
          <div class="telem-chart-row" style="margin-top:8px">
            <span class="telem-chart-label">tokens / day</span>
            <div class="telem-chart-wrap">
              ${sparkline(tokenData, 'var(--kinetic-sage,#8fb0a0)', 260, 36)}
            </div>
            <span class="telem-chart-peak">${formatTokens(maxTokens)}</span>
          </div>
        </div>

        <!-- Col 3: Model breakdown -->
        <div class="telem-col telem-col-models">
          <div class="telem-col-label">MODELS TODAY</div>
          ${modelBars || `<div class="today-section-empty" style="text-align:left">No model usage today</div>`}
        </div>

      </div>
    `;

    // Animate count-up for today's stats
    const { animateCountUp } = await import('../../components/molecules/data-viz');
    const turnsEl = document.getElementById('telem-turns');
    const toolsEl = document.getElementById('telem-tools');
    const tokensEl = document.getElementById('telem-tokens');
    const latencyEl = document.getElementById('telem-latency');

    if (turnsEl) animateCountUp(turnsEl, todayData.turn_count, 700);
    if (toolsEl) animateCountUp(toolsEl, todayData.tool_calls, 700);
    if (tokensEl) {
      // Use formatTokens for the final value but animate raw number
      const t = totalTokensToday;
      animateCountUp(
        {
          set textContent(v: string) {
            tokensEl.textContent = formatTokens(Number(v));
          },
        } as unknown as HTMLElement,
        t,
        700,
      );
      // Just set directly — animateCountUp works on integers, formatting separately
      tokensEl.textContent = formatTokens(t);
    }
    if (latencyEl) {
      latencyEl.textContent = avgLlmMs > 0 ? `${(avgLlmMs / 1000).toFixed(1)}s` : '—';
    }
  } catch (e) {
    console.warn('[today] Telemetry fetch failed:', e);
    container.innerHTML = `<div class="today-section-empty">No telemetry data yet — start a chat to generate metrics</div>`;
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
  const userAvatar = localStorage.getItem('paw-user-avatar') || '';

  const pendingTasks = tasks.filter((t) => !t.done);
  const completedToday = tasks.filter((t) => t.done && isToday(t.updatedAt));

  // Usage stats — showcase overrides or real data
  const cost = showcase ? showcase.cost : appState.sessionCost;

  // Build avatar HTML — profile picture or initials or default icon
  const avatarInner = userAvatar
    ? `<img src="${escHtml(userAvatar)}" alt="" class="today-avatar-img">`
    : userName
      ? `<span class="today-avatar-initials">${escHtml(userName.charAt(0).toUpperCase())}</span>`
      : '<span class="ms ms-sm">person</span>';

  container.innerHTML = `
    <div class="today-header bento-row">
      <div class="today-greeting-cell">
        <div class="today-profile-row">
          <button class="today-avatar" id="today-avatar" title="Upload profile picture">
            ${avatarInner}
            <span class="today-avatar-overlay"><span class="ms ms-xs">photo_camera</span></span>
          </button>
          <div class="today-profile-text">
            <div class="today-label">MISSION CONTROL</div>
            <div class="today-greeting">${greeting}${userName ? `, <span class="today-user-name" id="today-user-name" title="Click to edit">${escHtml(userName)}</span>` : '<button class="today-set-name-btn" id="today-set-name">Set your name</button>'}</div>
            <div class="today-date">${dateStr}</div>
            <div class="today-pawz-msg">${escHtml(getPawzMessage(pendingTasks.length, completedToday.length))}</div>
          </div>
        </div>
      </div>
      <div class="today-tesseract-cell" id="today-tesseract"></div>
      <div class="today-header-right">
        <div class="today-usage-strip">
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
          <span class="today-card-count" id="today-calendar-count">…</span>
        </div>
        <div class="today-card-body" id="today-calendar">
          ${skelLines(3)}
        </div>
      </div>

      <!-- Row 2: Recall + Recent Sessions -->
      <div class="cmd-card bento-cell bento-span-6 recall-card" id="recall-card">
        <div class="today-card-header">
          <span class="today-card-title">RECALL</span>
          <button class="btn btn-ghost btn-sm" id="recall-btn">↺ Recap</button>
        </div>
        <div class="today-card-body recall-body" id="recall-body">
          <div class="recall-output" id="recall-output"></div>
        </div>
      </div>

      <div class="cmd-card bento-cell bento-span-6">
        <div class="today-card-header">
          <span class="today-card-title">RECENT SESSIONS</span>
          <span class="today-card-count" id="today-sessions-count">…</span>
        </div>
        <div class="today-card-body" id="today-sessions">
          ${skelLines(3)}
        </div>
      </div>

      <!-- Row 3: Engram + Quick Actions -->
      <div class="cmd-card bento-cell bento-span-8 engram-card" id="engram-card">
        <div class="today-card-header">
          <span class="today-card-title">ENGRAM</span>
          <span class="today-card-count" id="engram-memory-count">…</span>
          <button class="btn btn-ghost btn-sm" id="engram-store-btn">+ Memory</button>
        </div>
        <div class="engram-card-body">
          <div class="engram-brain-wrap" id="engram-brain-wrap"></div>
          <div class="engram-stats-col" id="engram-stats-col">
            ${skelLines(3)}
          </div>
        </div>
      </div>

      <div class="cmd-card bento-cell bento-span-4">
        <div class="today-card-header">
          <span class="today-card-title">QUICK ACTIONS</span>
        </div>
        <div class="today-card-body">
          <button class="today-quick-action" id="today-new-chat-btn">
            ▸ New Chat
          </button>
          <button class="today-quick-action" id="today-research-btn">
            ▸ Research
          </button>
          <button class="today-quick-action" id="today-orchestrate-btn">
            ▸ Orchestration
          </button>
          <button class="today-quick-action" id="today-memory-vault-btn">
            ▸ Memory Vault
          </button>
        </div>
      </div>

      <!-- Row 4: Fleet + Skills + Activity -->
      <div class="cmd-card bento-cell bento-span-4">
        <div class="today-card-header">
          <span class="today-card-title">AGENT FLEET</span>
        </div>
        <div class="today-card-body" id="cmd-fleet-body">
          ${skelLines(3)}
        </div>
      </div>

      <div class="cmd-card bento-cell bento-span-4">
        <div class="today-card-header">
          <span class="today-card-title">SKILLS</span>
          <span class="today-card-count" id="cmd-skills-count">…</span>
        </div>
        <div class="today-card-body" id="cmd-skills-body">
          ${skelLines(3)}
        </div>
      </div>

      <div class="cmd-card bento-cell bento-span-4">
        <div class="today-card-header">
          <span class="today-card-title">ACTIVITY</span>
        </div>
        <div class="today-card-body" id="today-activity">
          ${skelLines(3)}
        </div>
      </div>

      <!-- Integrations (full width) -->
      <div class="cmd-card bento-cell bento-span-full">
        <div class="today-card-header">
          <span class="today-card-title">INTEGRATIONS</span>
          <span class="today-card-count" id="cmd-integrations-count">…</span>
        </div>
        <div class="today-card-body" id="cmd-integrations-body">
          ${skelLines(2)}
        </div>
      </div>

      <!-- Row 5: Telemetry (full width) -->
      <div class="cmd-card bento-cell bento-span-full">
        <div class="today-card-header">
          <span class="today-card-title">TELEMETRY</span>
          <span class="today-card-count" id="cmd-telemetry-label" style="margin-left:auto;font-size:10px;opacity:0.5">14-day</span>
        </div>
        <div class="today-card-body" style="max-height:none" id="cmd-telemetry-body">
          ${skelLines(2)}
        </div>
      </div>

      <!-- Skill Widgets -->
      <div class="bento-cell bento-span-full" id="today-skill-widgets">
        ${renderSkillWidgets(_skillOutputs)}
      </div>
    </div>
  `;

  // ── Hydrate the hero logo ──
  const logoCell = $('today-tesseract');
  if (logoCell) {
    _heroLogo?.destroy();
    _heroLogo = createHeroLogo(logoCell);
  }

  // ── Hydrate the Engram knowledge graph ──
  const brainWrap = $('engram-brain-wrap');
  if (brainWrap) {
    void renderPalaceGraphInto(brainWrap);
  }

  // ── Hydrate the Recall card with last stored recap ──
  const recallOut = $('recall-output');
  if (recallOut) {
    const storedText = localStorage.getItem('paw-recall-text');
    const storedTs = localStorage.getItem('paw-recall-ts');
    if (storedText) {
      const ago = storedTs ? _timeAgo(storedTs) : '';
      recallOut.innerHTML = `${escHtml(storedText)}${ago ? `<div class="recall-ts">Last recap ${ago}</div>` : ''}`;
    } else {
      recallOut.innerHTML =
        '<div class="recall-empty">Hit ↺ Recap to see what you\'ve been up to</div>';
    }
  }

  bindEvents();
}

// ── Inline name editor ────────────────────────────────────────────────

function showNameEditor(anchor: HTMLElement) {
  // Replace the name span/button with an inline input
  const current = localStorage.getItem('paw-user-name') || '';
  const wrapper = document.createElement('span');
  wrapper.className = 'today-name-editor';
  const input = document.createElement('input');
  input.type = 'text';
  input.value = current;
  input.placeholder = 'Your name';
  input.className = 'today-name-input';
  input.maxLength = 40;
  wrapper.appendChild(input);

  const save = () => {
    const name = input.value.trim();
    if (name) {
      localStorage.setItem('paw-user-name', name);
      showToast(`Welcome, ${name}!`, 'success');
    } else {
      localStorage.removeItem('paw-user-name');
    }
    _state.getRenderToday()();
  };

  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') save();
    if (e.key === 'Escape') _state.getRenderToday()();
  });
  input.addEventListener('blur', save);

  anchor.replaceWith(wrapper);
  input.focus();
  input.select();
}

// ── Events ────────────────────────────────────────────────────────────

/** Resize an image file to a small avatar data URL (max 128×128, JPEG 85%). */
function resizeAvatarImage(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    // Use FileReader to get a data URL first (more reliable in Tauri WebView
    // than URL.createObjectURL which can have CORS/blob restrictions)
    const reader = new FileReader();
    reader.onerror = () => reject(new Error('Failed to read file'));
    reader.onload = () => {
      const img = new Image();
      img.onload = () => {
        const MAX = 128;
        let w = img.width;
        let h = img.height;
        if (w > h) {
          if (w > MAX) {
            h = Math.round(h * (MAX / w));
            w = MAX;
          }
        } else {
          if (h > MAX) {
            w = Math.round(w * (MAX / h));
            h = MAX;
          }
        }
        const canvas = document.createElement('canvas');
        canvas.width = w;
        canvas.height = h;
        const ctx = canvas.getContext('2d');
        if (!ctx) {
          reject(new Error('Canvas not supported'));
          return;
        }
        ctx.drawImage(img, 0, 0, w, h);
        // Use image/png as fallback — some WebViews don't support image/jpeg
        let dataUrl = canvas.toDataURL('image/jpeg', 0.85);
        if (!dataUrl || dataUrl === 'data:,') {
          dataUrl = canvas.toDataURL('image/png');
        }
        resolve(dataUrl);
      };
      img.onerror = () => reject(new Error('Failed to decode image'));
      img.src = reader.result as string;
    };
    reader.readAsDataURL(file);
  });
}

function bindEvents() {
  // ── Profile avatar upload ───────────────────────────────────────────
  $('today-avatar')?.addEventListener('click', () => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'image/png,image/jpeg,image/gif,image/webp';
    // Tauri WebView requires the input to be in the DOM for programmatic click
    input.style.display = 'none';
    document.body.appendChild(input);
    input.addEventListener('change', async () => {
      const file = input.files?.[0];
      // Clean up the hidden input from DOM
      input.remove();
      if (!file) return;
      if (file.size > 5 * 1024 * 1024) {
        showToast('Image must be under 5 MB', 'error');
        return;
      }
      try {
        const dataUrl = await resizeAvatarImage(file);
        localStorage.setItem('paw-user-avatar', dataUrl);
        showToast('Profile picture updated', 'success');
        _state.getRenderToday()();
      } catch (e) {
        console.error('[today] Avatar upload failed:', e);
        showToast('Failed to save profile picture', 'error');
      }
    });
    input.click();
  });

  // ── Inline name editing ─────────────────────────────────────────────
  const nameEl = document.getElementById('today-user-name');
  if (nameEl) {
    nameEl.addEventListener('click', () => showNameEditor(nameEl));
  }
  const setNameBtn = document.getElementById('today-set-name');
  if (setNameBtn) {
    setNameBtn.addEventListener('click', () => showNameEditor(setNameBtn));
  }

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

  // ── Quick Actions ───────────────────────────────────────────────────────
  $('today-new-chat-btn')?.addEventListener('click', () => switchView('chat'));
  $('today-research-btn')?.addEventListener('click', () => switchView('research'));
  $('today-orchestrate-btn')?.addEventListener('click', () => switchView('orchestrator'));
  $('today-memory-vault-btn')?.addEventListener('click', () => switchView('memory-palace'));

  // ── Recall card ─────────────────────────────────────────────────────────
  $('recall-btn')?.addEventListener('click', () => void _generateRecall());

  // ── Engram card ─────────────────────────────────────────────────────────
  // Graph handles its own click/hover interactions; modal only via + Memory btn
  $('engram-store-btn')?.addEventListener('click', showQuickMemoryModal);

  // ── Kinetic: apply spring hover to bento cards ──
  document.querySelectorAll('.cmd-card').forEach((card) => {
    kineticRow(card as HTMLElement, { spring: true, springCard: true });
  });
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
      const completedToday = mapped.filter((t) => t.done && isToday(t.updatedAt));

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

// ── Integration Dashboard Loader ──────────────────────────────────────

export async function loadIntegrationsDashboard() {
  const body = $('cmd-integrations-body');
  const countEl = $('cmd-integrations-count');
  if (!body) return;

  try {
    let health = await loadServiceHealth();

    // Fallback: if health check returned nothing, try listing connected IDs directly
    const invoke = getInvoke();
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

    const html = await renderDashboardIntegrations(connectedIds, health);
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
