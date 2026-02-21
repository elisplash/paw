// Settings: Tailscale — DOM rendering + IPC

import { pawEngine } from '../../engine';
import { showToast } from '../../components/toast';
import type { TailscaleStatus, TailscaleConfig } from '../../engine/atoms/types';
import { $, confirmModal } from '../../components/helpers';
import { esc, makeBtn } from './atoms';

export async function loadTailscaleSettings() {
  const container = $('settings-tailscale-content');
  if (!container) return;
  container.innerHTML = '<p style="color:var(--text-muted)">Checking Tailscale…</p>';

  let status: TailscaleStatus;
  let config: TailscaleConfig;
  try {
    [status, config] = await Promise.all([
      pawEngine.tailscaleStatus(),
      pawEngine.tailscaleGetConfig(),
    ]);
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
    : status.installed
      ? '<span class="ms ms-sm" style="color:var(--warning)">circle</span>'
      : '<span class="ms ms-sm" style="color:var(--error)">circle</span>';
  const stLabel = status.running
    ? 'Connected'
    : status.installed
      ? 'Installed (not running)'
      : 'Not installed';

  statusSection.innerHTML = `
    <h3 class="settings-subsection-title">Status</h3>
    <div style="display:grid;grid-template-columns:auto 1fr;gap:8px 16px;font-size:13px;max-width:480px">
      <span style="color:var(--text-muted)">State</span><span>${dot} ${esc(stLabel)}</span>
      ${status.hostname ? `<span style="color:var(--text-muted)">Hostname</span><span>${esc(status.hostname)}</span>` : ''}
      ${status.tailnet ? `<span style="color:var(--text-muted)">Tailnet</span><span>${esc(status.tailnet)}</span>` : ''}
      ${status.ip ? `<span style="color:var(--text-muted)">IP</span><span style="font-family:monospace">${esc(status.ip)}</span>` : ''}
      ${status.version ? `<span style="color:var(--text-muted)">Version</span><span>${esc(status.version)}</span>` : ''}
      <span style="color:var(--text-muted)">Serve</span><span>${status.serve_active ? `Active${status.serve_url ? ` — <a href="${esc(status.serve_url)}" target="_blank" style="color:var(--accent)">${esc(status.serve_url)}</a>` : ''}` : 'Inactive'}</span>
      <span style="color:var(--text-muted)">Funnel</span><span>${status.funnel_active ? `Active${status.funnel_url ? ` — <a href="${esc(status.funnel_url)}" target="_blank" style="color:var(--accent)">${esc(status.funnel_url)}</a>` : ''}` : 'Inactive'}</span>
    </div>`;
  container.appendChild(statusSection);

  if (!status.installed) {
    const install = document.createElement('div');
    install.style.cssText =
      'padding:16px;border:1px dashed var(--border);border-radius:8px;margin:12px 0';
    install.innerHTML = `<p style="color:var(--text-muted);margin:0 0 8px">Tailscale is not installed on this machine.</p>
      <a href="https://tailscale.com/download" target="_blank" class="btn btn-primary btn-sm">Download Tailscale</a>`;
    container.appendChild(install);
    return;
  }

  // ── Connection Control ───────────────────────────────────────────
  const connSection = document.createElement('div');
  connSection.className = 'settings-subsection';
  connSection.style.marginBottom = '20px';
  connSection.innerHTML = `<h3 class="settings-subsection-title">Connection</h3>`;

  const connBtns = document.createElement('div');
  connBtns.style.cssText = 'display:flex;gap:8px;flex-wrap:wrap;margin-top:8px';

  if (status.running) {
    const disconnectBtn = makeBtn('Disconnect', 'btn-ghost', async () => {
      try {
        await pawEngine.tailscaleDisconnect();
        showToast('Disconnected', 'success');
        loadTailscaleSettings();
      } catch (e: unknown) {
        showToast(`Failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
      }
    });
    connBtns.appendChild(disconnectBtn);
  } else {
    const connectBtn = makeBtn('Connect', 'btn-primary', async () => {
      try {
        await pawEngine.tailscaleConnect(config.auth_key || undefined);
        showToast('Connecting…', 'info');
        loadTailscaleSettings();
      } catch (e: unknown) {
        showToast(`Failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
      }
    });
    connBtns.appendChild(connectBtn);
  }
  connSection.appendChild(connBtns);
  container.appendChild(connSection);

  // ── Serve/Funnel Control ─────────────────────────────────────────
  const serveSection = document.createElement('div');
  serveSection.className = 'settings-subsection';
  serveSection.style.marginBottom = '20px';
  serveSection.innerHTML = `<h3 class="settings-subsection-title">Serve &amp; Funnel</h3>
    <p class="settings-section-desc" style="margin-bottom:12px">Expose Pawz via your Tailscale network (Serve) or to the public internet (Funnel).</p>`;

  const serveBtns = document.createElement('div');
  serveBtns.style.cssText = 'display:flex;gap:8px;flex-wrap:wrap';

  if (status.serve_active) {
    serveBtns.appendChild(
      makeBtn('Stop Serve', 'btn-ghost', async () => {
        try {
          await pawEngine.tailscaleServeStop();
          showToast('Serve stopped', 'success');
          loadTailscaleSettings();
        } catch (e: unknown) {
          showToast(`Failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
        }
      }),
    );
  } else {
    serveBtns.appendChild(
      makeBtn('Start Serve', 'btn-primary', async () => {
        try {
          await pawEngine.tailscaleServeStart(config.serve_port);
          showToast('Serve started', 'success');
          loadTailscaleSettings();
        } catch (e: unknown) {
          showToast(`Failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
        }
      }),
    );
  }

  if (status.funnel_active) {
    serveBtns.appendChild(
      makeBtn('Stop Funnel', 'btn-ghost', async () => {
        try {
          await pawEngine.tailscaleFunnelStop();
          showToast('Funnel stopped', 'success');
          loadTailscaleSettings();
        } catch (e: unknown) {
          showToast(`Failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
        }
      }),
    );
  } else {
    serveBtns.appendChild(
      makeBtn('Start Funnel (Public)', 'btn-primary', async () => {
        if (!(await confirmModal('Funnel exposes Pawz to the PUBLIC internet. Continue?'))) return;
        try {
          await pawEngine.tailscaleFunnelStart(config.serve_port);
          showToast('Funnel started', 'success');
          loadTailscaleSettings();
        } catch (e: unknown) {
          showToast(`Failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
        }
      }),
    );
  }

  serveSection.appendChild(serveBtns);
  container.appendChild(serveSection);

  // ── Configuration ────────────────────────────────────────────────
  const cfgSection = document.createElement('div');
  cfgSection.className = 'settings-subsection';
  cfgSection.innerHTML = `<h3 class="settings-subsection-title">Configuration</h3>`;

  const form = document.createElement('div');
  form.style.cssText = 'display:flex;flex-direction:column;gap:12px;max-width:400px;margin-top:8px';

  form.innerHTML = `
    <label style="font-size:12px;font-weight:600">Serve Port
      <input type="number" id="ts-serve-port" value="${config.serve_port}" min="1" max="65535"
        style="width:100%;margin-top:4px;padding:8px;border-radius:6px;border:1px solid var(--border);background:var(--bg-secondary);color:var(--text-primary);font-size:13px" />
    </label>
    <label style="font-size:12px;font-weight:600">Auth Key <span style="font-weight:normal;color:var(--text-muted)">(optional, for headless connect)</span>
      <input type="password" id="ts-auth-key" value="${esc(config.auth_key)}" placeholder="tskey-auth-..."
        style="width:100%;margin-top:4px;padding:8px;border-radius:6px;border:1px solid var(--border);background:var(--bg-secondary);color:var(--text-primary);font-size:13px" />
    </label>
    <label style="font-size:12px;font-weight:600">Hostname Override <span style="font-weight:normal;color:var(--text-muted)">(optional)</span>
      <input type="text" id="ts-hostname" value="${esc(config.hostname_override)}" placeholder="pawz-desktop"
        style="width:100%;margin-top:4px;padding:8px;border-radius:6px;border:1px solid var(--border);background:var(--bg-secondary);color:var(--text-primary);font-size:13px" />
    </label>
    <div style="display:flex;gap:8px;margin-top:4px">
      <button class="btn btn-primary btn-sm" id="ts-save-config">Save Config</button>
      <button class="btn btn-ghost btn-sm" id="ts-reload">Reload</button>
    </div>`;

  cfgSection.appendChild(form);
  container.appendChild(cfgSection);

  // Wire save
  form.querySelector('#ts-save-config')?.addEventListener('click', async () => {
    const port = parseInt((form.querySelector('#ts-serve-port') as HTMLInputElement).value) || 3100;
    const authKey = (form.querySelector('#ts-auth-key') as HTMLInputElement).value;
    const hostname = (form.querySelector('#ts-hostname') as HTMLInputElement).value;
    const updated: TailscaleConfig = {
      ...config,
      serve_port: port,
      auth_key: authKey,
      hostname_override: hostname,
    };
    try {
      await pawEngine.tailscaleSetConfig(updated);
      showToast('Config saved', 'success');
    } catch (e: unknown) {
      showToast(`Failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
    }
  });

  form.querySelector('#ts-reload')?.addEventListener('click', () => loadTailscaleSettings());
}
