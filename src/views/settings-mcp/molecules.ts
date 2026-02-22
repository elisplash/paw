// Settings: MCP Servers — DOM rendering + IPC

import { pawEngine } from '../../engine';
import { showToast } from '../../components/toast';
import type { McpServerConfig, McpServerStatus } from '../../engine/atoms/types';
import { $ } from '../../components/helpers';
import { esc, makeBtn, inputStyle } from './atoms';

// ── Main loader ────────────────────────────────────────────────────────────

export async function loadMcpSettings() {
  const container = $('settings-mcp-content');
  if (!container) return;
  container.innerHTML = '<p style="color:var(--text-muted)">Loading MCP server configuration…</p>';

  let servers: McpServerConfig[];
  let statuses: McpServerStatus[];
  try {
    [servers, statuses] = await Promise.all([pawEngine.mcpListServers(), pawEngine.mcpStatus()]);
  } catch (e: unknown) {
    container.innerHTML = `<p style="color:var(--text-error)">Failed to load: ${esc(e instanceof Error ? e.message : String(e))}</p>`;
    return;
  }

  const statusMap = new Map(statuses.map((s) => [s.id, s]));
  container.innerHTML = '';

  // ── Connect All button ───────────────────────────────────────────
  const topBar = document.createElement('div');
  topBar.style.cssText = 'display:flex;gap:8px;margin-bottom:16px;flex-wrap:wrap';

  topBar.appendChild(
    makeBtn('Connect All Enabled', 'btn-primary', async () => {
      try {
        await pawEngine.mcpConnectAll();
        showToast('All MCP servers connected', 'success');
        loadMcpSettings();
      } catch (e: unknown) {
        showToast(`Connect errors: ${e instanceof Error ? e.message : String(e)}`, 'warning');
        loadMcpSettings();
      }
    }),
  );

  topBar.appendChild(makeBtn('Add Server', 'btn-ghost', () => showAddServerForm(container)));

  container.appendChild(topBar);

  // ── Server List ──────────────────────────────────────────────────
  if (servers.length === 0) {
    const empty = document.createElement('p');
    empty.style.cssText = 'color:var(--text-muted);font-size:13px;padding:24px 0';
    empty.textContent =
      'No MCP servers configured. Click "Add Server" to connect to an MCP tool server.';
    container.appendChild(empty);
    return;
  }

  for (const server of servers) {
    const status = statusMap.get(server.id);
    container.appendChild(renderServerCard(server, status));
  }
}

// ── Server Card ────────────────────────────────────────────────────────────

function renderServerCard(
  server: McpServerConfig,
  status: McpServerStatus | undefined,
): HTMLElement {
  const card = document.createElement('div');
  card.className = 'settings-subsection';
  card.style.cssText =
    'margin-bottom:16px;border:1px solid var(--border);border-radius:8px;padding:16px';

  const connected = status?.connected ?? false;
  const dot = connected
    ? '<span class="ms ms-sm" style="color:var(--success)">circle</span>'
    : '<span class="ms ms-sm" style="color:var(--text-muted)">circle</span>';
  const stLabel = connected ? `Connected (${status?.tool_count ?? 0} tools)` : 'Disconnected';
  const errorMsg = status?.error
    ? `<div style="color:var(--error);font-size:12px;margin-top:4px">${esc(status.error)}</div>`
    : '';

  const transport = server.transport === 'stdio' ? 'Stdio' : 'SSE';
  const endpoint =
    server.transport === 'stdio'
      ? `${esc(server.command)} ${server.args.map(esc).join(' ')}`
      : esc(server.url);

  card.innerHTML = `
    <div style="display:flex;justify-content:space-between;align-items:flex-start;gap:12px">
      <div>
        <h3 style="margin:0;font-size:14px;font-weight:600">${esc(server.name)}</h3>
        <div style="font-size:12px;color:var(--text-muted);margin-top:2px">
          ${dot} ${esc(stLabel)} · ${esc(transport)} · <code style="font-size:11px">${endpoint}</code>
        </div>
        ${errorMsg}
      </div>
      <div style="display:flex;gap:4px;flex-shrink:0">
        <span style="font-size:11px;padding:2px 8px;border-radius:4px;background:${server.enabled ? 'var(--success-bg, rgba(34,197,94,0.15))' : 'var(--bg-secondary)'};color:${server.enabled ? 'var(--success)' : 'var(--text-muted)'}">
          ${server.enabled ? 'Enabled' : 'Disabled'}
        </span>
      </div>
    </div>`;

  const btnRow = document.createElement('div');
  btnRow.style.cssText = 'display:flex;gap:6px;margin-top:12px;flex-wrap:wrap';

  if (connected) {
    btnRow.appendChild(
      makeBtn('Disconnect', 'btn-ghost', async () => {
        try {
          await pawEngine.mcpDisconnect(server.id);
          showToast(`Disconnected from ${server.name}`, 'success');
          loadMcpSettings();
        } catch (e: unknown) {
          showToast(`Failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
        }
      }),
    );
    btnRow.appendChild(
      makeBtn('Refresh Tools', 'btn-ghost', async () => {
        try {
          await pawEngine.mcpRefreshTools(server.id);
          showToast(`Refreshed tools for ${server.name}`, 'success');
          loadMcpSettings();
        } catch (e: unknown) {
          showToast(`Failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
        }
      }),
    );
  } else {
    btnRow.appendChild(
      makeBtn('Connect', 'btn-primary', async () => {
        try {
          await pawEngine.mcpConnect(server.id);
          showToast(`Connected to ${server.name}`, 'success');
          loadMcpSettings();
        } catch (e: unknown) {
          showToast(`Failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
        }
      }),
    );
  }

  btnRow.appendChild(makeBtn('Edit', 'btn-ghost', () => showEditServerForm(card, server)));

  btnRow.appendChild(
    makeBtn('Remove', 'btn-ghost', async () => {
      if (!confirm(`Remove MCP server "${server.name}"? This cannot be undone.`)) return;
      try {
        await pawEngine.mcpRemoveServer(server.id);
        showToast(`Removed ${server.name}`, 'success');
        loadMcpSettings();
      } catch (e: unknown) {
        showToast(`Failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
      }
    }),
  );

  card.appendChild(btnRow);
  return card;
}

// ── Add Server Form ────────────────────────────────────────────────────────

function showAddServerForm(container: HTMLElement) {
  // Remove existing form if any
  const existing = container.querySelector('#mcp-add-form');
  if (existing) {
    existing.remove();
    return;
  }

  const form = document.createElement('div');
  form.id = 'mcp-add-form';
  form.style.cssText =
    'border:1px solid var(--border);border-radius:8px;padding:16px;margin-bottom:16px;background:var(--bg-secondary)';

  form.innerHTML = `
    <h3 style="margin:0 0 12px;font-size:14px;font-weight:600">Add MCP Server</h3>
    <div style="display:flex;flex-direction:column;gap:10px;max-width:460px">
      <label style="font-size:12px;font-weight:600">Name
        <input type="text" id="mcp-add-name" placeholder="My MCP Server" style="${inputStyle}" />
      </label>
      <label style="font-size:12px;font-weight:600">Transport
        <select id="mcp-add-transport" style="${inputStyle}">
          <option value="stdio" selected>Stdio (local process)</option>
          <option value="sse">SSE (HTTP endpoint)</option>
        </select>
      </label>
      <div id="mcp-add-stdio-fields">
        <label style="font-size:12px;font-weight:600">Command
          <input type="text" id="mcp-add-command" placeholder="npx -y @modelcontextprotocol/server-filesystem" style="${inputStyle}" />
        </label>
        <label style="font-size:12px;font-weight:600;margin-top:8px;display:block">Arguments <span style="font-weight:normal;color:var(--text-muted)">(one per line)</span>
          <textarea id="mcp-add-args" rows="2" placeholder="/home/user/documents" style="${inputStyle};resize:vertical;font-family:monospace"></textarea>
        </label>
      </div>
      <div id="mcp-add-sse-fields" style="display:none">
        <label style="font-size:12px;font-weight:600">URL
          <input type="url" id="mcp-add-url" placeholder="http://localhost:8080/sse" style="${inputStyle}" />
        </label>
      </div>
      <div style="display:flex;gap:8px;margin-top:8px">
        <button class="btn btn-primary btn-sm" id="mcp-add-save">Add Server</button>
        <button class="btn btn-ghost btn-sm" id="mcp-add-cancel">Cancel</button>
      </div>
    </div>`;

  // Insert at top, after the button bar
  const topBar = container.querySelector('div');
  if (topBar && topBar.nextSibling) {
    container.insertBefore(form, topBar.nextSibling);
  } else {
    container.appendChild(form);
  }

  // Toggle transport fields
  const transportSelect = form.querySelector('#mcp-add-transport') as HTMLSelectElement;
  transportSelect.addEventListener('change', () => {
    const isStdio = transportSelect.value === 'stdio';
    (form.querySelector('#mcp-add-stdio-fields') as HTMLElement).style.display = isStdio
      ? ''
      : 'none';
    (form.querySelector('#mcp-add-sse-fields') as HTMLElement).style.display = isStdio
      ? 'none'
      : '';
  });

  form.querySelector('#mcp-add-cancel')?.addEventListener('click', () => form.remove());

  form.querySelector('#mcp-add-save')?.addEventListener('click', async () => {
    const name = (form.querySelector('#mcp-add-name') as HTMLInputElement).value.trim();
    if (!name) {
      showToast('Server name is required', 'error');
      return;
    }

    const transport = transportSelect.value as 'stdio' | 'sse';
    const command = (form.querySelector('#mcp-add-command') as HTMLInputElement).value.trim();
    const argsText = (form.querySelector('#mcp-add-args') as HTMLTextAreaElement).value.trim();
    const url = (form.querySelector('#mcp-add-url') as HTMLInputElement).value.trim();

    if (transport === 'stdio' && !command) {
      showToast('Command is required for Stdio transport', 'error');
      return;
    }
    if (transport === 'sse' && !url) {
      showToast('URL is required for SSE transport', 'error');
      return;
    }

    const config: McpServerConfig = {
      id: crypto.randomUUID(),
      name,
      transport,
      command: transport === 'stdio' ? command : '',
      args: transport === 'stdio' ? argsText.split('\n').filter((a) => a.trim()) : [],
      env: {},
      url: transport === 'sse' ? url : '',
      enabled: true,
    };

    try {
      await pawEngine.mcpSaveServer(config);
      showToast(`Added MCP server "${name}"`, 'success');
      form.remove();
      loadMcpSettings();
    } catch (e: unknown) {
      showToast(`Failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
    }
  });
}

// ── Edit Server Form ───────────────────────────────────────────────────────

function showEditServerForm(card: HTMLElement, server: McpServerConfig) {
  // Remove existing edit form
  const existing = card.querySelector('.mcp-edit-form');
  if (existing) {
    existing.remove();
    return;
  }

  const form = document.createElement('div');
  form.className = 'mcp-edit-form';
  form.style.cssText = 'margin-top:12px;padding-top:12px;border-top:1px solid var(--border)';

  form.innerHTML = `
    <div style="display:flex;flex-direction:column;gap:10px;max-width:460px">
      <label style="font-size:12px;font-weight:600">Name
        <input type="text" id="mcp-edit-name" value="${esc(server.name)}" style="${inputStyle}" />
      </label>
      <label style="font-size:12px;font-weight:600">Transport
        <select id="mcp-edit-transport" style="${inputStyle}">
          <option value="stdio" ${server.transport === 'stdio' ? 'selected' : ''}>Stdio (local process)</option>
          <option value="sse" ${server.transport === 'sse' ? 'selected' : ''}>SSE (HTTP endpoint)</option>
        </select>
      </label>
      <div id="mcp-edit-stdio-fields" style="${server.transport === 'sse' ? 'display:none' : ''}">
        <label style="font-size:12px;font-weight:600">Command
          <input type="text" id="mcp-edit-command" value="${esc(server.command)}" style="${inputStyle}" />
        </label>
        <label style="font-size:12px;font-weight:600;margin-top:8px;display:block">Arguments <span style="font-weight:normal;color:var(--text-muted)">(one per line)</span>
          <textarea id="mcp-edit-args" rows="2" style="${inputStyle};resize:vertical;font-family:monospace">${esc(server.args.join('\n'))}</textarea>
        </label>
      </div>
      <div id="mcp-edit-sse-fields" style="${server.transport === 'stdio' ? 'display:none' : ''}">
        <label style="font-size:12px;font-weight:600">URL
          <input type="url" id="mcp-edit-url" value="${esc(server.url)}" style="${inputStyle}" />
        </label>
      </div>
      <label style="font-size:12px;display:flex;align-items:center;gap:8px;cursor:pointer">
        <input type="checkbox" id="mcp-edit-enabled" ${server.enabled ? 'checked' : ''} />
        <span style="font-weight:600">Enabled</span>
      </label>
      <div style="display:flex;gap:8px;margin-top:4px">
        <button class="btn btn-primary btn-sm" id="mcp-edit-save">Save</button>
        <button class="btn btn-ghost btn-sm" id="mcp-edit-cancel">Cancel</button>
      </div>
    </div>`;

  card.appendChild(form);

  // Toggle transport fields
  const transportSelect = form.querySelector('#mcp-edit-transport') as HTMLSelectElement;
  transportSelect.addEventListener('change', () => {
    const isStdio = transportSelect.value === 'stdio';
    (form.querySelector('#mcp-edit-stdio-fields') as HTMLElement).style.display = isStdio
      ? ''
      : 'none';
    (form.querySelector('#mcp-edit-sse-fields') as HTMLElement).style.display = isStdio
      ? 'none'
      : '';
  });

  form.querySelector('#mcp-edit-cancel')?.addEventListener('click', () => form.remove());

  form.querySelector('#mcp-edit-save')?.addEventListener('click', async () => {
    const name = (form.querySelector('#mcp-edit-name') as HTMLInputElement).value.trim();
    if (!name) {
      showToast('Server name is required', 'error');
      return;
    }

    const transport = transportSelect.value as 'stdio' | 'sse';
    const command = (form.querySelector('#mcp-edit-command') as HTMLInputElement).value.trim();
    const argsText = (form.querySelector('#mcp-edit-args') as HTMLTextAreaElement).value.trim();
    const url = (form.querySelector('#mcp-edit-url') as HTMLInputElement).value.trim();
    const enabled = (form.querySelector('#mcp-edit-enabled') as HTMLInputElement).checked;

    const updated: McpServerConfig = {
      ...server,
      name,
      transport,
      command: transport === 'stdio' ? command : '',
      args: transport === 'stdio' ? argsText.split('\n').filter((a) => a.trim()) : [],
      url: transport === 'sse' ? url : '',
      enabled,
    };

    try {
      await pawEngine.mcpSaveServer(updated);
      showToast(`Updated MCP server "${name}"`, 'success');
      loadMcpSettings();
    } catch (e: unknown) {
      showToast(`Failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
    }
  });
}
