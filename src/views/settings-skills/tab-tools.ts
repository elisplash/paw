// My Skills — Tools Tab
// Shows MCP servers (promoted from Settings) + prompt-only skills (tier = 'skill').

import { pawEngine, type EngineSkillStatus, type McpServerConfig, type McpServerStatus } from '../../engine';
import { escHtml, confirmModal } from '../../components/helpers';
import { showToast } from '../../components/toast';
import { msIcon } from './atoms';
import { renderSkillCard, fromMcpServer, fromEngineSkill } from '../../components/molecules/skill-card';

// ── Types ──────────────────────────────────────────────────────────────

export interface ToolsTabData {
  skills: EngineSkillStatus[];
  mcpServers: McpServerConfig[];
  mcpStatuses: McpServerStatus[];
}

// ── State ──────────────────────────────────────────────────────────────

let _reloadFn: (() => Promise<void>) | null = null;

export function setToolsReload(fn: () => Promise<void>): void {
  _reloadFn = fn;
}

// ── Render ─────────────────────────────────────────────────────────────

export function renderToolsTab(data: ToolsTabData): string {
  const promptSkills = data.skills.filter((s) => s.tier === 'skill');
  const statusMap = new Map(data.mcpStatuses.map((s) => [s.id, s]));

  let html = '';

  // ── MCP Servers section ──────────────────────────────────────────
  html += `
  <div class="tools-section">
    <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:12px">
      <h3 style="margin:0;font-size:15px;font-weight:600;display:flex;align-items:center;gap:6px">
        ${msIcon('dns')} MCP Servers
        <span style="font-size:12px;font-weight:400;color:var(--text-muted)">(${data.mcpServers.length})</span>
      </h3>
      <div style="display:flex;gap:6px">
        <button class="btn btn-primary btn-sm" id="tools-mcp-connect-all">
          ${msIcon('power')} Connect All
        </button>
        <button class="btn btn-ghost btn-sm" id="tools-mcp-add">
          ${msIcon('add')} Add Server
        </button>
      </div>
    </div>`;

  if (data.mcpServers.length === 0) {
    html += `
    <div style="text-align:center;padding:24px;border:1px dashed var(--border-subtle);border-radius:8px;margin-bottom:16px">
      <p style="color:var(--text-muted);font-size:13px;margin:0">
        No MCP servers configured. Add one to connect external tool servers.
      </p>
    </div>`;
  } else {
    html += '<div class="skills-card-grid">';
    for (const server of data.mcpServers) {
      const cardData = fromMcpServer(server, statusMap.get(server.id));
      // Override action with custom HTML for connect/disconnect + remove
      const connected = statusMap.get(server.id)?.connected ?? false;
      const primaryAction = connected
        ? `<button class="btn btn-ghost btn-sm tools-mcp-disconnect" data-server-id="${escHtml(server.id)}">Disconnect</button>
           <button class="btn btn-ghost btn-sm tools-mcp-refresh" data-server-id="${escHtml(server.id)}">Refresh</button>`
        : `<button class="btn btn-primary btn-sm tools-mcp-connect" data-server-id="${escHtml(server.id)}">Connect</button>`;
      cardData.action = {
        type: 'custom',
        html: `${primaryAction}<button class="btn btn-ghost btn-sm tools-mcp-remove" data-server-id="${escHtml(server.id)}" data-server-name="${escHtml(server.name)}">${msIcon('delete')}</button>`,
      };
      html += renderSkillCard(cardData);
    }
    html += '</div>';
  }

  html += '</div>';

  // ── Add Server Form (hidden by default) ──────────────────────────
  html += `<div id="tools-mcp-add-form" style="display:none"></div>`;

  // ── Prompt-only skills section ───────────────────────────────────
  html += `
  <div class="tools-section" style="margin-top:24px">
    <h3 style="margin:0 0 12px;font-size:15px;font-weight:600;display:flex;align-items:center;gap:6px">
      ${msIcon('description')} Prompt Skills
      <span style="font-size:12px;font-weight:400;color:var(--text-muted)">(${promptSkills.length})</span>
    </h3>`;

  if (promptSkills.length === 0) {
    html += `
    <div style="text-align:center;padding:24px;border:1px dashed var(--border-subtle);border-radius:8px">
      <p style="color:var(--text-muted);font-size:13px;margin:0">
        No prompt-only skills installed.
      </p>
    </div>`;
  } else {
    html += '<div class="skills-card-grid">';
    for (const skill of promptSkills) {
      const cardData = fromEngineSkill(skill);
      // Override action CSS class for tools-specific toggle binding
      cardData.action = {
        type: 'custom',
        html: `<label class="skill-toggle-label"><input type="checkbox" class="tools-skill-toggle" data-skill="${escHtml(skill.id)}" ${skill.enabled ? 'checked' : ''} /> Enable</label>`,
      };
      cardData.extraBadges = [`${msIcon('description')} Prompt`];
      html += renderSkillCard(cardData);
    }
    html += '</div>';
  }

  html += '</div>';
  return html;
}

// ── Add Server Form ────────────────────────────────────────────────────

function showAddForm(): void {
  const container = document.getElementById('tools-mcp-add-form');
  if (!container) return;

  if (container.style.display !== 'none') {
    container.style.display = 'none';
    container.innerHTML = '';
    return;
  }

  container.style.display = '';
  container.innerHTML = `
  <div style="border:1px solid var(--border-subtle);border-radius:8px;padding:16px;margin-bottom:16px;background:var(--bg-surface)">
    <h4 style="margin:0 0 12px;font-size:14px;font-weight:600">Add MCP Server</h4>
    <div style="display:flex;flex-direction:column;gap:10px;max-width:480px">
      <label style="font-size:12px;font-weight:600">Name
        <input type="text" class="form-input" id="tools-mcp-name" placeholder="My MCP Server" style="width:100%;margin-top:4px" />
      </label>
      <label style="font-size:12px;font-weight:600">Transport
        <select class="form-input" id="tools-mcp-transport" style="width:100%;margin-top:4px">
          <option value="stdio" selected>Stdio (local process)</option>
          <option value="sse">SSE (HTTP endpoint)</option>
        </select>
      </label>
      <div id="tools-mcp-stdio-fields">
        <label style="font-size:12px;font-weight:600">Command
          <input type="text" class="form-input" id="tools-mcp-command" placeholder="npx -y @modelcontextprotocol/server-filesystem" style="width:100%;margin-top:4px" />
        </label>
        <label style="font-size:12px;font-weight:600;margin-top:8px;display:block">Arguments (one per line)
          <textarea class="form-input" id="tools-mcp-args" rows="2" placeholder="/home/user/documents" style="width:100%;margin-top:4px;resize:vertical;font-family:monospace"></textarea>
        </label>
      </div>
      <div id="tools-mcp-sse-fields" style="display:none">
        <label style="font-size:12px;font-weight:600">URL
          <input type="url" class="form-input" id="tools-mcp-url" placeholder="http://localhost:8080/sse" style="width:100%;margin-top:4px" />
        </label>
      </div>
      <div style="display:flex;gap:8px;margin-top:8px">
        <button class="btn btn-primary btn-sm" id="tools-mcp-save">Add Server</button>
        <button class="btn btn-ghost btn-sm" id="tools-mcp-cancel">Cancel</button>
      </div>
    </div>
  </div>`;

  // Transport toggle
  const transportSelect = document.getElementById('tools-mcp-transport') as HTMLSelectElement;
  transportSelect?.addEventListener('change', () => {
    const isStdio = transportSelect.value === 'stdio';
    const stdioEl = document.getElementById('tools-mcp-stdio-fields');
    const sseEl = document.getElementById('tools-mcp-sse-fields');
    if (stdioEl) stdioEl.style.display = isStdio ? '' : 'none';
    if (sseEl) sseEl.style.display = isStdio ? 'none' : '';
  });

  document.getElementById('tools-mcp-cancel')?.addEventListener('click', () => {
    container.style.display = 'none';
    container.innerHTML = '';
  });

  document.getElementById('tools-mcp-save')?.addEventListener('click', async () => {
    const name = (document.getElementById('tools-mcp-name') as HTMLInputElement)?.value?.trim();
    if (!name) { showToast('Server name is required', 'error'); return; }

    const transport = (document.getElementById('tools-mcp-transport') as HTMLSelectElement)?.value as 'stdio' | 'sse';
    const command = (document.getElementById('tools-mcp-command') as HTMLInputElement)?.value?.trim() ?? '';
    const argsText = (document.getElementById('tools-mcp-args') as HTMLTextAreaElement)?.value?.trim() ?? '';
    const url = (document.getElementById('tools-mcp-url') as HTMLInputElement)?.value?.trim() ?? '';

    if (transport === 'stdio' && !command) { showToast('Command is required for Stdio', 'error'); return; }
    if (transport === 'sse' && !url) { showToast('URL is required for SSE', 'error'); return; }

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
      container.style.display = 'none';
      container.innerHTML = '';
      if (_reloadFn) await _reloadFn();
    } catch (e) {
      showToast(`Failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
    }
  });
}

// ── Event binding ──────────────────────────────────────────────────────

export function bindToolsTabEvents(): void {
  const reload = () => (_reloadFn ? _reloadFn() : Promise.resolve());

  // Connect All button
  document.getElementById('tools-mcp-connect-all')?.addEventListener('click', async () => {
    try {
      await pawEngine.mcpConnectAll();
      showToast('All MCP servers connected', 'success');
      await reload();
    } catch (e) {
      showToast(`Connect errors: ${e instanceof Error ? e.message : String(e)}`, 'warning');
      await reload();
    }
  });

  // Add Server button
  document.getElementById('tools-mcp-add')?.addEventListener('click', showAddForm);

  // Per-server connect buttons
  document.querySelectorAll('.tools-mcp-connect').forEach((el) => {
    el.addEventListener('click', async () => {
      const id = (el as HTMLElement).dataset.serverId!;
      try {
        await pawEngine.mcpConnect(id);
        showToast('Connected', 'success');
        await reload();
      } catch (e) {
        showToast(`Failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
      }
    });
  });

  // Per-server disconnect buttons
  document.querySelectorAll('.tools-mcp-disconnect').forEach((el) => {
    el.addEventListener('click', async () => {
      const id = (el as HTMLElement).dataset.serverId!;
      try {
        await pawEngine.mcpDisconnect(id);
        showToast('Disconnected', 'success');
        await reload();
      } catch (e) {
        showToast(`Failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
      }
    });
  });

  // Per-server refresh buttons
  document.querySelectorAll('.tools-mcp-refresh').forEach((el) => {
    el.addEventListener('click', async () => {
      const id = (el as HTMLElement).dataset.serverId!;
      try {
        await pawEngine.mcpRefreshTools(id);
        showToast('Tools refreshed', 'success');
        await reload();
      } catch (e) {
        showToast(`Failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
      }
    });
  });

  // Per-server remove buttons
  document.querySelectorAll('.tools-mcp-remove').forEach((el) => {
    el.addEventListener('click', async () => {
      const id = (el as HTMLElement).dataset.serverId!;
      const name = (el as HTMLElement).dataset.serverName ?? id;
      if (!(await confirmModal(`Remove MCP server "${name}"? This cannot be undone.`))) return;
      try {
        await pawEngine.mcpRemoveServer(id);
        showToast(`Removed ${name}`, 'success');
        await reload();
      } catch (e) {
        showToast(`Failed: ${e instanceof Error ? e.message : String(e)}`, 'error');
      }
    });
  });

  // Prompt skill enable/disable toggles
  document.querySelectorAll('.tools-skill-toggle').forEach((el) => {
    el.addEventListener('change', async (e) => {
      const input = e.target as HTMLInputElement;
      const skillId = input.dataset.skill!;
      try {
        await pawEngine.skillSetEnabled(skillId, input.checked);
        showToast(`${skillId} ${input.checked ? 'enabled' : 'disabled'}`, 'success');
        await reload();
      } catch (err) {
        showToast(`Failed: ${err}`, 'error');
        input.checked = !input.checked;
      }
    });
  });
}
