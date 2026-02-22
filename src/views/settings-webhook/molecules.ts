// Settings: Webhook — DOM rendering + IPC

import { pawEngine } from '../../engine';
import { showToast } from '../../components/toast';
import type { ChannelStatus, WebhookConfig } from '../../engine/atoms/types';
import { $, confirmModal } from '../../components/helpers';
import { esc, makeBtn } from './atoms';

export async function loadWebhookSettings() {
  const container = $('settings-webhook-content');
  if (!container) return;
  container.innerHTML = '<p style="color:var(--text-muted)">Loading webhook configuration…</p>';

  let status: ChannelStatus;
  let config: WebhookConfig;
  try {
    [status, config] = await Promise.all([pawEngine.webhookStatus(), pawEngine.webhookGetConfig()]);
  } catch (e: unknown) {
    container.innerHTML = `<p style="color:var(--text-error)">Failed to load: ${esc(e instanceof Error ? e.message : String(e))}</p>`;
    return;
  }

  container.innerHTML = '';

  // ── Status Card ──────────────────────────────────────────────────
  const statusSection = document.createElement('div');
  statusSection.className = 'settings-subsection';
  statusSection.style.marginBottom = '20px';

  const dot = status.running
    ? '<span class="ms ms-sm" style="color:var(--success)">circle</span>'
    : '<span class="ms ms-sm" style="color:var(--text-muted)">circle</span>';
  const stLabel = status.running ? 'Running' : 'Stopped';

  statusSection.innerHTML = `
    <h3 class="settings-subsection-title">Status</h3>
    <div style="display:grid;grid-template-columns:auto 1fr;gap:8px 16px;font-size:13px;max-width:480px">
      <span style="color:var(--text-muted)">State</span><span>${dot} ${esc(stLabel)}</span>
      <span style="color:var(--text-muted)">Endpoint</span><span style="font-family:monospace">${esc(config.bind_address)}:${config.port}</span>
      <span style="color:var(--text-muted)">Messages</span><span>${status.message_count}</span>
    </div>`;
  container.appendChild(statusSection);

  // ── Start / Stop ─────────────────────────────────────────────────
  const controlSection = document.createElement('div');
  controlSection.className = 'settings-subsection';
  controlSection.style.marginBottom = '20px';
  controlSection.innerHTML = `<h3 class="settings-subsection-title">Server Control</h3>`;

  const controlBtns = document.createElement('div');
  controlBtns.style.cssText = 'display:flex;gap:8px;flex-wrap:wrap;margin-top:8px';

  if (status.running) {
    controlBtns.appendChild(
      makeBtn('Stop Server', 'btn-ghost', async () => {
        try {
          await pawEngine.webhookStop();
          showToast('Webhook server stopped', 'success');
          loadWebhookSettings();
        } catch (e: unknown) {
          showToast(`Failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
        }
      }),
    );
  } else {
    controlBtns.appendChild(
      makeBtn('Start Server', 'btn-primary', async () => {
        try {
          await pawEngine.webhookStart();
          showToast('Webhook server started', 'success');
          loadWebhookSettings();
        } catch (e: unknown) {
          showToast(`Failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
        }
      }),
    );
  }

  controlSection.appendChild(controlBtns);
  container.appendChild(controlSection);

  // ── Configuration ────────────────────────────────────────────────
  const cfgSection = document.createElement('div');
  cfgSection.className = 'settings-subsection';
  cfgSection.style.marginBottom = '20px';
  cfgSection.innerHTML = `<h3 class="settings-subsection-title">Configuration</h3>`;

  const form = document.createElement('div');
  form.style.cssText = 'display:flex;flex-direction:column;gap:12px;max-width:400px;margin-top:8px';

  const inputStyle =
    'width:100%;margin-top:4px;padding:8px;border-radius:6px;border:1px solid var(--border);background:var(--bg-secondary);color:var(--text-primary);font-size:13px';

  form.innerHTML = `
    <label style="font-size:12px;font-weight:600">Bind Address
      <input type="text" id="wh-bind" value="${esc(config.bind_address)}" placeholder="127.0.0.1"
        style="${inputStyle}" />
    </label>
    <label style="font-size:12px;font-weight:600">Port
      <input type="number" id="wh-port" value="${config.port}" min="1" max="65535"
        style="${inputStyle}" />
    </label>
    <label style="font-size:12px;font-weight:600">Auth Token
      <div style="display:flex;gap:6px;margin-top:4px">
        <input type="password" id="wh-token" value="${esc(config.auth_token)}" readonly
          style="${inputStyle};flex:1;font-family:monospace" />
        <button class="btn btn-ghost btn-sm" id="wh-show-token" title="Toggle visibility" style="white-space:nowrap">Show</button>
        <button class="btn btn-ghost btn-sm" id="wh-copy-token" title="Copy to clipboard" style="white-space:nowrap">Copy</button>
        <button class="btn btn-ghost btn-sm" id="wh-regen-token" title="Regenerate token" style="white-space:nowrap">Regen</button>
      </div>
    </label>
    <label style="font-size:12px;font-weight:600">Default Agent ID <span style="font-weight:normal;color:var(--text-muted)">(used when URL omits agent_id)</span>
      <input type="text" id="wh-agent-id" value="${esc(config.default_agent_id)}" placeholder="default"
        style="${inputStyle}" />
    </label>
    <label style="font-size:12px;font-weight:600">Rate Limit <span style="font-weight:normal;color:var(--text-muted)">(requests/min per IP, 0 = unlimited)</span>
      <input type="number" id="wh-rate" value="${config.rate_limit_per_minute}" min="0" max="10000"
        style="${inputStyle}" />
    </label>
    <label style="font-size:12px;display:flex;align-items:center;gap:8px;cursor:pointer">
      <input type="checkbox" id="wh-dangerous" ${config.allow_dangerous_tools ? 'checked' : ''} />
      <span style="font-weight:600">Allow dangerous tools</span>
      <span style="color:var(--warning);font-size:11px">⚠️ Lets webhook-triggered agents run shell commands, file I/O, etc.</span>
    </label>
    <div style="display:flex;gap:8px;margin-top:4px">
      <button class="btn btn-primary btn-sm" id="wh-save-config">Save Config</button>
      <button class="btn btn-ghost btn-sm" id="wh-reload">Reload</button>
    </div>`;

  cfgSection.appendChild(form);
  container.appendChild(cfgSection);

  // Wire token actions
  const tokenInput = form.querySelector('#wh-token') as HTMLInputElement;
  form.querySelector('#wh-show-token')?.addEventListener('click', () => {
    const isHidden = tokenInput.type === 'password';
    tokenInput.type = isHidden ? 'text' : 'password';
    (form.querySelector('#wh-show-token') as HTMLButtonElement).textContent = isHidden
      ? 'Hide'
      : 'Show';
  });

  form.querySelector('#wh-copy-token')?.addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(config.auth_token);
      showToast('Token copied', 'success');
    } catch {
      showToast('Failed to copy', 'error');
    }
  });

  form.querySelector('#wh-regen-token')?.addEventListener('click', async () => {
    if (
      !(await confirmModal('Regenerate the webhook auth token? Existing integrations will break.'))
    )
      return;
    try {
      const newToken = await pawEngine.webhookRegenerateToken();
      config.auth_token = newToken;
      tokenInput.value = newToken;
      showToast('Token regenerated', 'success');
    } catch (e: unknown) {
      showToast(`Failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
    }
  });

  // Wire save
  form.querySelector('#wh-save-config')?.addEventListener('click', async () => {
    const bindAddress = (form.querySelector('#wh-bind') as HTMLInputElement).value || '127.0.0.1';
    const port = parseInt((form.querySelector('#wh-port') as HTMLInputElement).value) || 3940;
    const agentId = (form.querySelector('#wh-agent-id') as HTMLInputElement).value || 'default';
    const rate = parseInt((form.querySelector('#wh-rate') as HTMLInputElement).value) || 0;
    const dangerous = (form.querySelector('#wh-dangerous') as HTMLInputElement).checked;
    const updated: WebhookConfig = {
      ...config,
      bind_address: bindAddress,
      port,
      default_agent_id: agentId,
      rate_limit_per_minute: rate,
      allow_dangerous_tools: dangerous,
    };
    try {
      await pawEngine.webhookSetConfig(updated);
      showToast('Webhook config saved', 'success');
    } catch (e: unknown) {
      showToast(`Failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
    }
  });

  form.querySelector('#wh-reload')?.addEventListener('click', () => loadWebhookSettings());

  // ── curl Example ─────────────────────────────────────────────────
  const exampleSection = document.createElement('div');
  exampleSection.className = 'settings-subsection';
  exampleSection.innerHTML = `<h3 class="settings-subsection-title">Usage Example</h3>`;

  const endpoint = `http://${config.bind_address === '0.0.0.0' ? 'localhost' : config.bind_address}:${config.port}`;
  const agentPath = config.default_agent_id ? config.default_agent_id : '<agent_id>';
  const curlCmd = `curl -X POST ${endpoint}/webhook/${agentPath} \\
  -H "Authorization: Bearer ${config.auth_token}" \\
  -H "Content-Type: application/json" \\
  -d '{"message": "Hello from my automation!"}'`;

  const codeBlock = document.createElement('pre');
  codeBlock.style.cssText =
    'background:var(--bg-secondary);border:1px solid var(--border);border-radius:8px;padding:16px;font-size:12px;overflow-x:auto;line-height:1.5;color:var(--text-primary);font-family:monospace;white-space:pre-wrap;word-break:break-all';
  codeBlock.textContent = curlCmd;

  const copyBtn = makeBtn('Copy', 'btn-ghost', async () => {
    try {
      await navigator.clipboard.writeText(curlCmd);
      showToast('Copied to clipboard', 'success');
    } catch {
      showToast('Failed to copy', 'error');
    }
  });
  copyBtn.style.marginTop = '8px';

  exampleSection.appendChild(codeBlock);
  exampleSection.appendChild(copyBtn);
  container.appendChild(exampleSection);
}
