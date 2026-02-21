// Settings: Environment — DOM rendering + IPC

import { pawEngine } from '../../engine';
import { showToast } from '../../components/toast';
import { isConnected } from '../../state/connection';
import { esc, textInput } from '../settings-config';
import { $ } from '../../components/helpers';

// ── Render ──────────────────────────────────────────────────────────────────

export async function loadEnvSettings() {
  if (!isConnected()) return;
  const container = $('settings-env-content');
  if (!container) return;
  container.innerHTML = '<p style="color:var(--text-muted)">Loading…</p>';

  try {
    container.innerHTML = '';

    // ── System Environment ───────────────────────────────────────────────
    const sysSection = document.createElement('div');
    sysSection.style.cssText = 'margin-bottom:16px';
    sysSection.innerHTML = `
      <h3 class="settings-subsection-title">System Environment</h3>
      <p class="form-hint" style="margin:0 0 8px;font-size:12px;color:var(--text-muted)">
        Paw inherits environment variables from your system automatically.
        Set variables in your shell profile (<code>~/.bashrc</code>, <code>~/.zshrc</code>, etc.)
        or your desktop environment. Changes take effect on app restart.
      </p>
    `;
    container.appendChild(sysSection);

    // ── Provider API Keys ────────────────────────────────────────────────
    const provSection = document.createElement('div');
    provSection.innerHTML =
      '<h3 class="settings-subsection-title" style="margin-top:16px">Provider API Keys</h3>';
    provSection.innerHTML +=
      '<p class="form-hint" style="margin:0 0 8px;font-size:12px;color:var(--text-muted)">API keys are stored in the engine config and encrypted at rest. Manage providers in Settings → Advanced.</p>';

    const config = await pawEngine.getConfig();
    if (config.providers.length === 0) {
      const empty = document.createElement('p');
      empty.style.cssText = 'color:var(--text-muted);font-size:13px;padding:8px 0';
      empty.textContent =
        'No providers configured yet. Go to Settings → Advanced to add providers.';
      provSection.appendChild(empty);
    } else {
      for (const prov of config.providers) {
        const row = document.createElement('div');
        row.style.cssText = 'display:flex;gap:8px;align-items:center;margin-bottom:6px';

        const label = document.createElement('span');
        label.style.cssText = 'min-width:100px;font-weight:600;font-size:13px';
        label.textContent = prov.kind.charAt(0).toUpperCase() + prov.kind.slice(1);
        row.appendChild(label);

        const keyInp = textInput(
          prov.api_key,
          prov.kind === 'ollama' ? '(not required)' : 'sk-...',
          'password',
        );
        keyInp.style.cssText = 'flex:1;font-family:var(--font-mono);font-size:12px';
        row.appendChild(keyInp);

        const saveBtn = document.createElement('button');
        saveBtn.className = 'btn btn-sm btn-primary';
        saveBtn.textContent = 'Save';
        saveBtn.addEventListener('click', async () => {
          try {
            const updated = { ...prov, api_key: keyInp.value };
            await pawEngine.upsertProvider(updated);
            showToast(`${prov.kind} API key updated`, 'success');
          } catch (e) {
            showToast(`Failed: ${e instanceof Error ? e.message : e}`, 'error');
          }
        });
        row.appendChild(saveBtn);

        provSection.appendChild(row);
      }
    }
    container.appendChild(provSection);

    // ── Skill Credentials ────────────────────────────────────────────────
    const skillSection = document.createElement('div');
    skillSection.innerHTML =
      '<h3 class="settings-subsection-title" style="margin-top:20px">Skill Credentials</h3>';
    skillSection.innerHTML +=
      '<p class="form-hint" style="margin:0 0 8px;font-size:12px;color:var(--text-muted)">Credentials for enabled skills (email, Slack, GitHub, etc.) are managed in Skills settings. Stored encrypted in the local vault.</p>';

    try {
      const skills = await pawEngine.skillsList();
      const configured = skills.filter((s) => s.configured_credentials.length > 0);

      if (configured.length === 0) {
        const hint = document.createElement('p');
        hint.style.cssText = 'color:var(--text-muted);font-size:13px;padding:4px 0';
        hint.textContent =
          'No skill credentials configured yet. Enable skills and add credentials in the Skills view.';
        skillSection.appendChild(hint);
      } else {
        for (const skill of configured) {
          const row = document.createElement('div');
          row.style.cssText =
            'display:flex;gap:8px;align-items:center;margin-bottom:4px;padding:6px 0;border-bottom:1px solid var(--border-light, rgba(255,255,255,0.06))';
          row.innerHTML = `
            <span style="font-size:16px">${esc(skill.icon)}</span>
            <span style="font-weight:600;font-size:13px;min-width:80px">${esc(skill.name)}</span>
            <span style="color:var(--text-muted);font-size:12px">${skill.configured_credentials.length} credential${skill.configured_credentials.length !== 1 ? 's' : ''} stored</span>
            ${skill.missing_credentials.length > 0 ? `<span style="color:var(--warning);font-size:11px">Missing: ${skill.missing_credentials.join(', ')}</span>` : '<span style="color:var(--success);font-size:11px">Ready</span>'}
          `;
          skillSection.appendChild(row);
        }
      }
    } catch {
      // Skills may not be available
      const hint = document.createElement('p');
      hint.style.cssText = 'color:var(--text-muted);font-size:12px;padding:4px 0';
      hint.textContent = 'Could not load skill credentials.';
      skillSection.appendChild(hint);
    }

    container.appendChild(skillSection);

    // ── Common Environment Variables Guide ────────────────────────────────
    const guideSection = document.createElement('div');
    guideSection.innerHTML = `
      <h3 class="settings-subsection-title" style="margin-top:20px">Common Environment Variables</h3>
      <p class="form-hint" style="margin:0 0 8px;font-size:12px;color:var(--text-muted)">
        Set these in your shell profile if needed. Paw picks them up automatically.
      </p>
      <div style="font-family:var(--font-mono);font-size:11px;color:var(--text-muted);line-height:1.8">
        <div><code>OPENAI_API_KEY</code> — OpenAI API key (alternative to provider config)</div>
        <div><code>ANTHROPIC_API_KEY</code> — Anthropic API key</div>
        <div><code>GOOGLE_API_KEY</code> — Google AI API key</div>
        <div><code>OLLAMA_HOST</code> — Ollama server URL (default: http://localhost:11434)</div>
        <div><code>GITHUB_TOKEN</code> — GitHub personal access token for the GitHub skill</div>
        <div><code>SLACK_TOKEN</code> — Slack bot token for the Slack skill</div>
        <div><code>PATH</code> — System PATH (for tools like git, docker, etc.)</div>
      </div>
    `;
    container.appendChild(guideSection);
  } catch (e) {
    container.innerHTML = `<p style="color:var(--danger)">Failed to load: ${esc(String(e))}</p>`;
  }
}
