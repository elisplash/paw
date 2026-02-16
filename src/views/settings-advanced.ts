// Settings: Advanced — Gateway, Logging, Updates, Sandbox, Hooks
// ~230 lines

import {
  getConfig, patchConfig, getVal, isConnected,
  esc, formRow, selectInput, textInput, numberInput, toggleSwitch, saveReloadButtons
} from './settings-config';

const $ = (id: string) => document.getElementById(id);

// ── Option sets ─────────────────────────────────────────────────────────────

const LOG_LEVELS = [
  { value: 'error', label: 'Error' }, { value: 'warn', label: 'Warn' },
  { value: 'info', label: 'Info' }, { value: 'debug', label: 'Debug' },
  { value: 'trace', label: 'Trace' },
];

const LOG_STYLES = [
  { value: 'text', label: 'Plain text' }, { value: 'json', label: 'JSON' },
];

const UPDATE_CHANNELS = [
  { value: 'stable', label: 'Stable' },
  { value: 'beta', label: 'Beta' },
  { value: 'none', label: 'Disabled' },
];

const SANDBOX_MODES = [
  { value: 'off', label: 'Off' },
  { value: 'docker', label: 'Docker' },
  { value: 'nsjail', label: 'nsjail' },
  { value: 'firejail', label: 'Firejail' },
];

// ── Render ──────────────────────────────────────────────────────────────────

export async function loadAdvancedSettings() {
  if (!isConnected()) return;
  const container = $('settings-advanced-content');
  if (!container) return;
  container.innerHTML = '<p style="color:var(--text-muted)">Loading…</p>';

  try {
    const config = await getConfig();
    container.innerHTML = '';

    // ── Gateway Network ──────────────────────────────────────────────────
    const netSection = document.createElement('div');
    netSection.innerHTML = '<h3 class="settings-subsection-title">Gateway Network</h3>';
    const gw = (getVal(config, 'gateway') ?? {}) as Record<string, any>;

    const portRow = formRow('Port', 'Gateway listen port');
    const portInp = numberInput(gw.port, { min: 1, max: 65535, placeholder: '3578' });
    portInp.style.maxWidth = '120px';
    portRow.appendChild(portInp);
    netSection.appendChild(portRow);

    const bindRow = formRow('Bind Address', 'Interface to bind (0.0.0.0 = all)');
    const bindInp = textInput(gw.bind ?? '', '0.0.0.0');
    bindInp.style.maxWidth = '200px';
    bindRow.appendChild(bindInp);
    netSection.appendChild(bindRow);

    // TLS
    const { container: tlsToggle, checkbox: tlsCb } = toggleSwitch(gw.tls?.enabled === true, 'Enable TLS');
    netSection.appendChild(tlsToggle);

    const certRow = formRow('TLS Certificate Path');
    const certInp = textInput(gw.tls?.cert ?? '', '/path/to/cert.pem');
    certRow.appendChild(certInp);
    netSection.appendChild(certRow);

    const keyRow = formRow('TLS Key Path');
    const keyInp = textInput(gw.tls?.key ?? '', '/path/to/key.pem');
    keyRow.appendChild(keyInp);
    netSection.appendChild(keyRow);

    container.appendChild(netSection);

    // ── Auth ─────────────────────────────────────────────────────────────
    const authSection = document.createElement('div');
    authSection.innerHTML = '<h3 class="settings-subsection-title" style="margin-top:20px">Authentication</h3>';
    const auth = (getVal(config, 'auth') ?? {}) as Record<string, any>;

    const { container: authToggle, checkbox: authCb } = toggleSwitch(auth.enabled !== false, 'Require Auth Token');
    authSection.appendChild(authToggle);

    const tokenRow = formRow('Auth Token', 'Gateway access token');
    const tokenInp = textInput(auth.token ?? '', '(auto-generated)', 'password');
    tokenRow.appendChild(tokenInp);
    authSection.appendChild(tokenRow);

    container.appendChild(authSection);

    // ── Logging ──────────────────────────────────────────────────────────
    const logSection = document.createElement('div');
    logSection.innerHTML = '<h3 class="settings-subsection-title" style="margin-top:20px">Logging</h3>';
    const log = (getVal(config, 'logging') ?? {}) as Record<string, any>;

    const lvlRow = formRow('Log Level');
    const lvlSel = selectInput(LOG_LEVELS, log.level ?? 'info');
    lvlSel.style.maxWidth = '160px';
    lvlRow.appendChild(lvlSel);
    logSection.appendChild(lvlRow);

    const styleRow = formRow('Log Style');
    const styleSel = selectInput(LOG_STYLES, log.style ?? 'text');
    styleSel.style.maxWidth = '160px';
    styleRow.appendChild(styleSel);
    logSection.appendChild(styleRow);

    const fileRow = formRow('Log File Path', 'Leave empty for stdout only');
    const fileInp = textInput(log.file ?? '', '/var/log/openclaw.log');
    fileRow.appendChild(fileInp);
    logSection.appendChild(fileRow);

    container.appendChild(logSection);

    // ── Updates ──────────────────────────────────────────────────────────
    const updSection = document.createElement('div');
    updSection.innerHTML = '<h3 class="settings-subsection-title" style="margin-top:20px">Auto Updates</h3>';
    const upd = (getVal(config, 'updates') ?? {}) as Record<string, any>;

    const chanRow = formRow('Update Channel');
    const chanSel = selectInput(UPDATE_CHANNELS, upd.channel ?? 'stable');
    chanSel.style.maxWidth = '160px';
    chanRow.appendChild(chanSel);
    updSection.appendChild(chanRow);

    const { container: autoToggle, checkbox: autoCb } = toggleSwitch(
      upd.autoInstall !== false, 'Auto-install updates'
    );
    updSection.appendChild(autoToggle);

    container.appendChild(updSection);

    // ── Sandbox ──────────────────────────────────────────────────────────
    const sandSection = document.createElement('div');
    sandSection.innerHTML = '<h3 class="settings-subsection-title" style="margin-top:20px">Sandbox</h3>';
    const sand = (getVal(config, 'sandbox') ?? {}) as Record<string, any>;

    const modeRow = formRow('Sandbox Mode', 'Isolate agent code execution');
    const modeSel = selectInput(SANDBOX_MODES, sand.mode ?? 'off');
    modeSel.style.maxWidth = '180px';
    modeRow.appendChild(modeSel);
    sandSection.appendChild(modeRow);

    const imgRow = formRow('Docker Image', 'Custom image for Docker sandbox');
    const imgInp = textInput(sand.dockerImage ?? '', 'openclaw/sandbox:latest');
    imgRow.appendChild(imgInp);
    sandSection.appendChild(imgRow);

    container.appendChild(sandSection);

    // ── Hooks ────────────────────────────────────────────────────────────
    const hookSection = document.createElement('div');
    hookSection.innerHTML = '<h3 class="settings-subsection-title" style="margin-top:20px">Webhooks</h3>';
    const hooks = (getVal(config, 'hooks') ?? {}) as Record<string, any>;

    const whRow = formRow('Webhook URL', 'POST events to this URL');
    const whInp = textInput(hooks.url ?? '', 'https://example.com/webhook');
    whRow.appendChild(whInp);
    hookSection.appendChild(whRow);

    const secretRow = formRow('Webhook Secret', 'HMAC signing secret');
    const secretInp = textInput(hooks.secret ?? '', '', 'password');
    secretRow.appendChild(secretInp);
    hookSection.appendChild(secretRow);

    container.appendChild(hookSection);

    // ── Save all ─────────────────────────────────────────────────────────
    container.appendChild(saveReloadButtons(
      async () => {
        const patch: Record<string, unknown> = {
          gateway: {
            port: parseInt(portInp.value) || undefined,
            bind: bindInp.value || undefined,
            tls: {
              enabled: tlsCb.checked,
              cert: certInp.value || undefined,
              key: keyInp.value || undefined,
            },
          },
          auth: {
            enabled: authCb.checked,
            token: tokenInp.value || undefined,
          },
          logging: {
            level: lvlSel.value,
            style: styleSel.value,
            file: fileInp.value || undefined,
          },
          updates: {
            channel: chanSel.value,
            autoInstall: autoCb.checked,
          },
          sandbox: {
            mode: modeSel.value,
            dockerImage: imgInp.value || undefined,
          },
          hooks: {
            url: whInp.value || undefined,
            secret: secretInp.value || undefined,
          },
        };
        await patchConfig(patch);
      },
      () => loadAdvancedSettings()
    ));

  } catch (e) {
    container.innerHTML = `<p style="color:var(--danger)">Failed to load: ${esc(String(e))}</p>`;
  }
}

export function initAdvancedSettings() {
  // All dynamic
}
