// Nodes — DOM rendering + IPC

import { pawEngine } from '../../engine';
import { showToast } from '../../components/toast';
import { $ } from '../../components/helpers';
import { esc } from './atoms';

// ── Main loader ────────────────────────────────────────────────────────────
export async function loadNodes() {
  const container = $('nodes-list') ?? $('nodes-detail') ?? $('nodes-empty');
  // Try to find and use the main content area
  const parent = container?.parentElement ?? container;
  if (!parent) return;

  // If separate containers exist, unify into the parent
  const list = $('nodes-list');
  const detail = $('nodes-detail');
  const empty = $('nodes-empty');
  const loading = $('nodes-loading');
  if (loading) loading.style.display = 'none';
  if (empty) empty.style.display = 'none';
  if (detail) detail.style.display = 'none';

  // Use the list container as our main render target, or fall back
  const target = list ?? parent;
  target.innerHTML = '<p style="color:var(--text-muted)">Loading connections…</p>';
  target.style.display = '';

  try {
    // Gather status from engine
    const [status, config] = await Promise.all([pawEngine.status(), pawEngine.getConfig()]);

    let skillsInfo: Array<{
      name: string;
      icon: string;
      configured_credentials: string[];
      missing_credentials: string[];
    }> = [];
    try {
      skillsInfo = await pawEngine.skillsList();
    } catch {
      /* skills may not be loaded */
    }

    target.innerHTML = '';

    // ── Engine Status ──────────────────────────────────────────────────
    const engineSection = document.createElement('div');
    engineSection.style.cssText = 'margin-bottom:16px';
    const engineRunning =
      status && (status as unknown as Record<string, unknown>).running !== false;
    engineSection.innerHTML = `
      <h3 class="settings-subsection-title">Engine Status</h3>
      <div style="display:flex;gap:12px;align-items:center;padding:8px 0">
        <span style="font-size:24px"><span class="ms" style="color:${engineRunning ? 'var(--success)' : 'var(--danger)'}">circle</span></span>
        <div>
          <div style="font-weight:600;font-size:14px">Paw Engine</div>
          <div style="font-size:12px;color:var(--text-muted)">${engineRunning ? 'Running — Tauri IPC connected' : 'Not responding'}</div>
        </div>
      </div>
    `;
    target.appendChild(engineSection);

    // ── Providers ──────────────────────────────────────────────────────
    const provSection = document.createElement('div');
    provSection.style.cssText = 'margin-bottom:16px';
    provSection.innerHTML = '<h3 class="settings-subsection-title">Configured Providers</h3>';

    if (!config.providers.length) {
      provSection.innerHTML +=
        '<p style="color:var(--text-muted);font-size:13px;padding:4px 0">No providers configured. Go to Settings → Advanced to add Ollama or cloud providers.</p>';
    } else {
      for (const prov of config.providers) {
        const card = document.createElement('div');
        card.style.cssText =
          'display:flex;gap:10px;align-items:center;padding:8px 0;border-bottom:1px solid var(--border-light, rgba(255,255,255,0.06))';

        const kindIcons: Record<string, string> = {
          ollama: 'pets',
          openai: 'smart_toy',
          anthropic: 'psychology',
          google: 'auto_awesome',
          openrouter: 'language',
          custom: 'build',
        };
        const icon = `<span class="ms ms-sm">${kindIcons[prov.kind.toLowerCase()] ?? 'bolt'}</span>`;
        const isDefault = prov.id === config.default_provider;
        const hasKey =
          prov.kind.toLowerCase() === 'ollama' || (prov.api_key && prov.api_key.length > 0);
        const url =
          prov.base_url || (prov.kind.toLowerCase() === 'ollama' ? 'http://localhost:11434' : '—');

        card.innerHTML = `
          <span style="font-size:20px">${icon}</span>
          <div style="flex:1">
            <div style="font-weight:600;font-size:13px">${esc(prov.kind)}${isDefault ? ' <span style="color:var(--accent);font-size:11px">(default)</span>' : ''}</div>
            <div style="font-size:11px;color:var(--text-muted)">${esc(url)}</div>
          </div>
          <span style="font-size:11px;color:${hasKey ? 'var(--success)' : 'var(--warning)'}">${hasKey ? '● Key set' : '○ No key'}</span>
        `;

        // Test button for Ollama
        if (prov.kind.toLowerCase() === 'ollama') {
          const testBtn = document.createElement('button');
          testBtn.className = 'btn btn-sm';
          testBtn.textContent = 'Test';
          testBtn.addEventListener('click', async () => {
            testBtn.disabled = true;
            testBtn.textContent = '…';
            try {
              const testUrl = (prov.base_url || 'http://localhost:11434').replace(/\/$/, '');
              const resp = await fetch(`${testUrl}/api/tags`);
              if (resp.ok) {
                const data = (await resp.json()) as { models?: Array<{ name: string }> };
                const count = data.models?.length ?? 0;
                showToast(
                  `Ollama connected — ${count} model${count !== 1 ? 's' : ''} available`,
                  'success',
                );
              } else {
                showToast(`Ollama returned ${resp.status}`, 'error');
              }
            } catch (e) {
              showToast(`Cannot reach Ollama: ${e instanceof Error ? e.message : e}`, 'error');
            } finally {
              testBtn.disabled = false;
              testBtn.textContent = 'Test';
            }
          });
          card.appendChild(testBtn);
        }

        provSection.appendChild(card);
      }
    }
    target.appendChild(provSection);

    // ── Default Model ──────────────────────────────────────────────────
    if (config.default_model) {
      const modelSection = document.createElement('div');
      modelSection.style.cssText = 'margin-bottom:16px';
      modelSection.innerHTML = `
        <h3 class="settings-subsection-title">Active Model</h3>
        <div style="display:flex;gap:8px;align-items:center;padding:6px 0">
          <span style="font-size:16px"><span class="ms ms-sm">flag</span></span>
          <span style="font-weight:600;font-size:13px;font-family:var(--font-mono)">${esc(config.default_model)}</span>
          ${config.default_provider ? `<span style="font-size:11px;color:var(--text-muted)">via ${esc(config.default_provider)}</span>` : ''}
        </div>
      `;
      target.appendChild(modelSection);
    }

    // ── Skills Readiness ───────────────────────────────────────────────
    if (skillsInfo.length > 0) {
      const skillSection = document.createElement('div');
      skillSection.innerHTML = '<h3 class="settings-subsection-title">Skills</h3>';

      for (const skill of skillsInfo) {
        const ready = skill.missing_credentials.length === 0;
        const row = document.createElement('div');
        row.style.cssText = 'display:flex;gap:8px;align-items:center;padding:4px 0;font-size:13px';
        row.innerHTML = `
          <span>${esc(skill.icon)}</span>
          <span style="font-weight:600;min-width:80px">${esc(skill.name)}</span>
          <span style="color:${ready ? 'var(--success)' : 'var(--warning)'};font-size:11px">${ready ? '● Ready' : `○ Missing: ${skill.missing_credentials.join(', ')}`}</span>
        `;
        skillSection.appendChild(row);
      }
      target.appendChild(skillSection);
    }

    // ── Engine Config Summary ──────────────────────────────────────────
    const cfgSection = document.createElement('div');
    cfgSection.style.cssText = 'margin-top:12px';
    cfgSection.innerHTML = `
      <h3 class="settings-subsection-title">Engine Config</h3>
      <div style="font-family:var(--font-mono);font-size:11px;color:var(--text-muted);line-height:1.8">
        <div>max_tool_rounds: ${config.max_tool_rounds ?? '—'}</div>
        <div>tool_timeout_secs: ${config.tool_timeout_secs ?? '—'}</div>
        <div>providers: ${config.providers.length}</div>
        <div>default_model: ${config.default_model ? esc(config.default_model) : '(not set)'}</div>
      </div>
    `;
    target.appendChild(cfgSection);
  } catch (e) {
    target.innerHTML = `<p style="color:var(--danger)">Failed to load status: ${esc(String(e))}</p>`;
  }
}
