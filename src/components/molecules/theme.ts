// src/components/molecules/theme.ts
// Theme management — multi-theme support.

const THEME_KEY = 'paw-theme';

export type PawTheme = 'dark' | 'light' | 'midnight' | 'hacker' | 'ember' | 'arctic' | 'violet' | 'solarized';

export const THEMES: { id: PawTheme; label: string; icon: string; swatch: string }[] = [
  { id: 'dark',      label: 'Dark',          icon: 'dark_mode',      swatch: '#050505' },
  { id: 'light',     label: 'Light',         icon: 'light_mode',     swatch: '#F5F0EB' },
  { id: 'midnight',  label: 'Midnight Blue', icon: 'nights_stay',    swatch: '#0B1120' },
  { id: 'hacker',    label: 'Hacker',        icon: 'terminal',       swatch: '#000000' },
  { id: 'ember',     label: 'Ember',         icon: 'local_fire_department', swatch: '#1A1210' },
  { id: 'arctic',    label: 'Arctic',        icon: 'ac_unit',        swatch: '#EFF4F8' },
  { id: 'violet',    label: 'Violet Void',   icon: 'auto_awesome',   swatch: '#100818' },
  { id: 'solarized', label: 'Solarized',     icon: 'wb_twilight',    swatch: '#002B36' },
];

const ACCENT_MAP: Record<PawTheme, string> = {
  dark: '#FF4D4D',
  light: '#CC3333',
  midnight: '#4D9EFF',
  hacker: '#00FF41',
  ember: '#FF8C28',
  arctic: '#2E7DB5',
  violet: '#A78BFA',
  solarized: '#268BD2',
};

export function getTheme(): PawTheme {
  return (localStorage.getItem(THEME_KEY) as PawTheme) || 'dark';
}

export function setTheme(theme: PawTheme) {
  if (theme === 'dark') {
    document.documentElement.removeAttribute('data-theme');
  } else {
    document.documentElement.setAttribute('data-theme', theme);
  }
  localStorage.setItem(THEME_KEY, theme);

  // Sync settings grid active state
  updateSettingsGrid(theme);

  // Sync sidebar icon
  updateSidebarIcon(theme);
}

export function initTheme() {
  setTheme(getTheme());

  // Build settings page theme grid (if the container exists)
  buildSettingsGrid();

  // Wire sidebar button as quick dark/light toggle
  document.getElementById('sidebar-theme-toggle')?.addEventListener('click', () => {
    const cur = getTheme();
    const isLight = cur === 'light' || cur === 'arctic';
    setTheme(isLight ? 'dark' : 'light');
  });
}

// ── Settings page theme grid ─────────────────────────────────────────────

/** Build the theme picker grid inside #theme-grid on the settings page. */
export function buildSettingsGrid() {
  const container = document.getElementById('theme-grid');
  if (!container) return;

  const current = getTheme();

  container.innerHTML = THEMES.map(t => `
    <button class="theme-grid-item${t.id === current ? ' active' : ''}" data-theme="${t.id}">
      <span class="theme-grid-swatch" style="background:${t.swatch};box-shadow:inset 0 0 0 2px ${ACCENT_MAP[t.id]}"></span>
      <span class="theme-grid-label">${t.label}</span>
      <span class="theme-grid-icon ms" style="font-size:16px">${t.icon}</span>
    </button>
  `).join('');

  container.addEventListener('click', (e) => {
    const item = (e.target as HTMLElement).closest('.theme-grid-item') as HTMLElement | null;
    if (!item) return;
    const theme = item.dataset.theme as PawTheme;
    setTheme(theme);
  });
}

function updateSettingsGrid(theme: PawTheme) {
  const container = document.getElementById('theme-grid');
  if (!container) return;
  container.querySelectorAll('.theme-grid-item').forEach(el => {
    el.classList.toggle('active', (el as HTMLElement).dataset.theme === theme);
  });
}

// ── Sidebar icon sync ────────────────────────────────────────────────────

function updateSidebarIcon(theme: PawTheme) {
  const btn = document.getElementById('sidebar-theme-toggle');
  if (!btn) return;
  const icon = btn.querySelector('.nav-icon');
  if (icon) {
    const isLight = theme === 'light' || theme === 'arctic';
    icon.textContent = isLight ? 'light_mode' : 'dark_mode';
  }
}
