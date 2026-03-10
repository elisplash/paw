/**
 * main.ts — Pawz CODE Control App (Tauri desktop)
 *
 * All communication with the daemon goes through Tauri invoke() commands —
 * no direct fetch() calls to the daemon. The Rust backend handles HTTP.
 *
 * Panels:
 *   - Service status (connected/disconnected, model, provider, workspace)
 *   - Activity stats (active runs, memory, engram, protocols)
 *   - Protocol list
 *   - Controls (refresh, start/stop daemon, start-at-login, open config)
 *   - Log viewer
 */

import { invoke } from '@tauri-apps/api/core';

// ── Types ────────────────────────────────────────────────────────────────────

interface DaemonStatus {
  status: string;
  service: string;
  version?: string;
  model?: string;
  provider?: string;
  workspace_root?: string | null;
  active_runs?: number;
  memory_entries?: number;
  engram_entries?: number;
  protocols?: string[];
  max_rounds?: number;
}

// ── State ────────────────────────────────────────────────────────────────────

const POLL_INTERVAL_MS = 10_000;

let pollTimer: ReturnType<typeof setInterval> | null = null;
let startAtLoginEnabled = false;
let daemonRunning = false;

// ── DOM helpers ───────────────────────────────────────────────────────────────

function el(id: string): HTMLElement {
  return document.getElementById(id) as HTMLElement;
}

function setText(id: string, value: string): void {
  const e = el(id);
  if (e) e.textContent = value;
}

function log(msg: string): void {
  const box = el('log-box');
  if (!box) return;
  const line = document.createElement('div');
  line.textContent = `[${new Date().toLocaleTimeString()}] ${msg}`;
  box.appendChild(line);
  box.scrollTop = box.scrollHeight;
  while (box.children.length > 100) {
    box.removeChild(box.firstChild!);
  }
}

// ── get_status (via Tauri invoke) ─────────────────────────────────────────────

async function fetchStatus(): Promise<DaemonStatus | null> {
  try {
    const json = await invoke<string>('get_status');
    return JSON.parse(json) as DaemonStatus;
  } catch {
    return null;
  }
}

function applyStatus(data: DaemonStatus | null): void {
  const dot = el('status-dot');
  const statusText = el('status-text');

  if (!data) {
    dot?.classList.remove('connected');
    dot?.classList.add('disconnected');
    if (statusText) statusText.textContent = 'Disconnected';
    setText('info-model', '—');
    setText('info-provider', '—');
    setText('info-workspace', '—');
    setText('info-version', '—');
    setText('stat-runs', '—');
    setText('stat-memory', '—');
    setText('stat-engram', '—');
    setText('stat-protocols', '—');
    const protoList = el('protocols-list');
    if (protoList) protoList.textContent = 'Not connected';
    daemonRunning = false;
    updateDaemonButton();
    return;
  }

  dot?.classList.remove('disconnected');
  dot?.classList.add('connected');
  if (statusText) statusText.textContent = 'Connected';

  setText('info-model', data.model ?? '—');
  setText('info-provider', data.provider ?? '—');
  setText('info-workspace', data.workspace_root ?? '(not set)');
  setText('info-version', data.version ?? '—');
  setText('stat-runs', String(data.active_runs ?? 0));
  setText('stat-memory', String(data.memory_entries ?? 0));
  setText('stat-engram', String(data.engram_entries ?? 0));
  setText('stat-protocols', String((data.protocols ?? []).length));

  const protoList = el('protocols-list');
  if (protoList) {
    protoList.innerHTML = '';
    for (const p of data.protocols ?? []) {
      const tag = document.createElement('span');
      tag.className = 'tag';
      tag.textContent = p;
      protoList.appendChild(tag);
    }
    if ((data.protocols ?? []).length === 0) {
      protoList.textContent = 'None loaded';
    }
  }

  daemonRunning = true;
  updateDaemonButton();
}

async function refresh(): Promise<void> {
  const data = await fetchStatus();
  applyStatus(data);
  if (data) {
    log(`Refreshed — model: ${data.model ?? '?'}, runs: ${data.active_runs ?? 0}`);
  } else {
    log('Daemon unreachable');
  }
}

// ── Config Form ───────────────────────────────────────────────────────────────

function generateToken(): void {
  const array = new Uint8Array(32);
  crypto.getRandomValues(array);
  const token = Array.from(array, b => b.toString(16).padStart(2, '0')).join('');
  (el('cfg-token') as HTMLInputElement).value = token;
  // Also update the token display at the top
  (el('token-display') as HTMLInputElement).value = token;
  log('Generated new auth token');
}

function onProviderChange(): void {
  const provider = (el('cfg-provider') as HTMLSelectElement).value;
  const apiKeyField = el('field-api-key');
  const claudeBinaryField = el('field-claude-binary');

  // Hide API key field for claude_code and ollama
  if (provider === 'claude_code' || provider === 'ollama') {
    apiKeyField.style.display = 'none';
  } else {
    apiKeyField.style.display = 'block';
  }

  // Show Claude binary path field only for claude_code
  if (provider === 'claude_code') {
    claudeBinaryField.style.display = 'block';
  } else {
    claudeBinaryField.style.display = 'none';
  }
}

async function loadConfigForm(): Promise<void> {
  try {
    const raw = await invoke<string>('load_config');

    // Parse TOML manually (simple key=value parser)
    const lines = raw.split('\n');
    let provider = '';
    let apiKey = '';
    let model = '';
    let token = '';
    let claudeBinary = '';

    for (const line of lines) {
      const trimmed = line.trim();
      if (trimmed.startsWith('#')) continue;

      const match = trimmed.match(/^(\w+)\s*=\s*"([^"]*)"/);
      if (!match) continue;

      const [, key, value] = match;
      if (key === 'provider') provider = value;
      if (key === 'api_key') apiKey = value;
      if (key === 'model') model = value;
      if (key === 'auth_token') token = value;
      if (key === 'claude_binary_path') claudeBinary = value;
    }

    (el('cfg-provider') as HTMLSelectElement).value = provider || 'anthropic';
    (el('cfg-api-key') as HTMLInputElement).value = apiKey;
    (el('cfg-model') as HTMLInputElement).value = model;
    (el('cfg-token') as HTMLInputElement).value = token;
    (el('cfg-claude-binary') as HTMLInputElement).value = claudeBinary;

    // Also update the token display at the top
    (el('token-display') as HTMLInputElement).value = token;

    onProviderChange();
    log('Config loaded into form');
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    log(`⚠ Load config error: ${msg}`);
  }
}

async function saveConfigForm(): Promise<void> {
  const provider = (el('cfg-provider') as HTMLSelectElement).value;
  const apiKey = (el('cfg-api-key') as HTMLInputElement).value;
  const model = (el('cfg-model') as HTMLInputElement).value;
  const token = (el('cfg-token') as HTMLInputElement).value;
  const claudeBinary = (el('cfg-claude-binary') as HTMLInputElement).value;

  if (!token) {
    log('⚠ Auth token is required. Click Generate to create one.');
    return;
  }
  if (!model) {
    log('⚠ Model name is required.');
    return;
  }
  if ((provider === 'anthropic' || provider === 'openai') && !apiKey) {
    log(`⚠ API key is required for ${provider}`);
    return;
  }

  let toml = `# Pawz CODE Configuration\nport = 3941\nbind = "127.0.0.1"\nauth_token = "${token}"\nprovider = "${provider}"\n`;

  if (apiKey && provider !== 'claude_code' && provider !== 'ollama') {
    toml += `api_key = "${apiKey}"\n`;
  }

  toml += `model = "${model}"\n`;

  // Add claude_binary_path if provided and using claude_code
  if (provider === 'claude_code' && claudeBinary) {
    toml += `claude_binary_path = "${claudeBinary}"\n`;
  }

  try {
    const result = await invoke<string>('save_config', { content: toml });
    // Update the token display after saving
    (el('token-display') as HTMLInputElement).value = token;
    log(result);
    log('✓ Config saved successfully');
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    log(`⚠ Save config error: ${msg}`);
  }
}

// ── start_daemon / stop_daemon ────────────────────────────────────────────────

async function toggleDaemon(): Promise<void> {
  const btn = el('btn-daemon');
  if (btn) btn.setAttribute('disabled', 'true');

  try {
    if (daemonRunning) {
      const result = await invoke<string>('stop_daemon');
      log(result);
      daemonRunning = false;
    } else {
      // Try to find binary next to config dir, else ask user to provide path
      const binaryPath = await resolveDaemonBinaryPath();
      if (!binaryPath) {
        log('⚠ Cannot find pawz-code binary. Build with `cargo build --release` in pawz-code/server/');
        return;
      }
      const result = await invoke<string>('start_daemon', { binaryPath });
      log(result);
      daemonRunning = true;
      // Poll status after short delay to confirm it started
      setTimeout(() => void refresh(), 2000);
    }
    updateDaemonButton();
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    log(`⚠ Daemon toggle error: ${msg}`);
  } finally {
    if (btn) btn.removeAttribute('disabled');
  }
}

function updateDaemonButton(): void {
  const btn = el('btn-daemon');
  if (!btn) return;
  if (daemonRunning) {
    btn.textContent = '⏹ Stop Daemon';
    btn.classList.remove('btn-primary');
    btn.classList.add('btn-danger');
  } else {
    btn.textContent = '▶ Start Daemon';
    btn.classList.remove('btn-danger');
    btn.classList.add('btn-primary');
  }
}

/** Hardcoded daemon binary path for personal use. */
async function resolveDaemonBinaryPath(): Promise<string | null> {
  // Hardcoded to the standard build location
  return '/Users/elibury/Desktop/OpenPawz/pawz-code/server/target/release/pawz-code';
}

// ── toggle_start_at_login ─────────────────────────────────────────────────────

async function toggleStartAtLogin(): Promise<void> {
  const btn = el('btn-start-at-login');
  if (btn) btn.setAttribute('disabled', 'true');

  const newState = !startAtLoginEnabled;
  try {
    const binaryPath = await resolveDaemonBinaryPath();
    const result = await invoke<string>('toggle_start_at_login', {
      enable: newState,
      binaryPath: binaryPath ?? '',
    });
    startAtLoginEnabled = newState;
    updateStartAtLoginButton();
    log(result);
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : String(err);
    log(`⚠ Start-at-login error: ${msg}`);
  } finally {
    if (btn) btn.removeAttribute('disabled');
  }
}

function updateStartAtLoginButton(): void {
  const btn = el('btn-start-at-login');
  if (!btn) return;
  if (startAtLoginEnabled) {
    btn.textContent = '✓ Start at Login (On)';
    btn.classList.remove('btn-ghost');
    btn.classList.add('btn-primary');
  } else {
    btn.textContent = '⟳ Start at Login';
    btn.classList.remove('btn-primary');
    btn.classList.add('btn-ghost');
  }
}

// ── Footer time ───────────────────────────────────────────────────────────────

function updateFooterTime(): void {
  const e = el('footer-time');
  if (e) e.textContent = new Date().toLocaleTimeString();
}

// ── Copy token to clipboard ──────────────────────────────────────────────────

async function copyToken(): Promise<void> {
  const tokenDisplay = el('token-display') as HTMLInputElement;
  const token = tokenDisplay.value;

  if (!token) {
    log('⚠ No token to copy. Generate or load config first.');
    return;
  }

  try {
    await navigator.clipboard.writeText(token);
    log('✓ Auth token copied to clipboard!');
    log('→ Paste it in VS Code Settings → pawzCode.authToken');
  } catch (err) {
    log('⚠ Failed to copy to clipboard');
  }
}

// ── Expose to HTML onclick handlers ──────────────────────────────────────────

(window as unknown as Record<string, unknown>).refresh = refresh;
(window as unknown as Record<string, unknown>).generateToken = generateToken;
(window as unknown as Record<string, unknown>).onProviderChange = onProviderChange;
(window as unknown as Record<string, unknown>).loadConfigForm = loadConfigForm;
(window as unknown as Record<string, unknown>).saveConfigForm = saveConfigForm;
(window as unknown as Record<string, unknown>).toggleDaemon = toggleDaemon;
(window as unknown as Record<string, unknown>).toggleStartAtLogin = toggleStartAtLogin;
(window as unknown as Record<string, unknown>).copyToken = copyToken;

// ── Init ──────────────────────────────────────────────────────────────────────

async function init(): Promise<void> {
  log('Starting Pawz CODE control panel…');

  // Try to load config into form
  try {
    await loadConfigForm();
  } catch {
    log('No config found — use the form to create one');
  }

  await refresh();

  // Auto-start daemon if not running
  const status = await fetchStatus();
  if (!status) {
    log('Daemon not running — attempting auto-start…');
    try {
      const binaryPath = await resolveDaemonBinaryPath();
      if (binaryPath) {
        const result = await invoke<string>('start_daemon', { binaryPath });
        log(result);
        daemonRunning = true;
        updateDaemonButton();
        // Wait a bit then refresh status
        setTimeout(() => void refresh(), 2000);
      } else {
        log('⚠ Cannot auto-start: daemon binary not found');
        log('→ Build with: cd server && cargo build --release');
      }
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      log(`⚠ Auto-start failed: ${msg}`);
    }
  } else {
    log('✓ Daemon already running');
  }

  // Start polling
  if (pollTimer) clearInterval(pollTimer);
  pollTimer = setInterval(() => void refresh(), POLL_INTERVAL_MS);

  // Footer clock
  updateFooterTime();
  setInterval(updateFooterTime, 1000);
}

void init();
