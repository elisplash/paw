// Settings: Advanced — DOM rendering + IPC

import { pawEngine, type EngineProviderConfig } from '../../engine';
import { showToast } from '../../components/toast';
import { isConnected } from '../../state/connection';
import {
  esc,
  formRow,
  selectInput,
  textInput,
  numberInput,
  saveReloadButtons,
} from '../settings-config';
import { $ } from '../../components/helpers';
import { PROVIDER_KINDS, DEFAULT_BASE_URLS, POPULAR_MODELS } from './atoms';

// ── Render ──────────────────────────────────────────────────────────────────

export async function loadAdvancedSettings() {
  if (!isConnected()) return;
  const container = $('settings-advanced-content');
  if (!container) return;
  container.innerHTML = '<p style="color:var(--text-muted)">Loading engine config…</p>';

  try {
    const config = await pawEngine.getConfig();
    container.innerHTML = '';

    // ── Ollama Quick Setup ───────────────────────────────────────────────
    const ollamaSection = document.createElement('div');
    ollamaSection.className = 'settings-subsection';

    const hasOllama = config.providers.some((p) => p.kind === 'ollama');

    ollamaSection.innerHTML = `
      <h3 class="settings-subsection-title"><span class="ms ms-sm">pets</span> Ollama (Local AI)</h3>
      <p class="form-hint" style="margin:0 0 12px;font-size:12px;color:var(--text-muted)">
        Run AI models on your own machine — free, private, no API key needed.
        ${!hasOllama ? '<br><strong style="color:var(--warning)">Not configured yet.</strong> Install Ollama from <a href="https://ollama.ai" target="_blank" style="color:var(--accent)">ollama.ai</a> then add it below.' : ''}
      </p>
    `;

    const ollamaProvider = config.providers.find((p) => p.kind === 'ollama');

    const ollamaUrlRow = formRow(
      'Ollama URL',
      'Where Ollama is running (default: http://localhost:11434)',
    );
    const ollamaUrlInp = textInput(
      ollamaProvider?.base_url ?? 'http://localhost:11434',
      'http://localhost:11434',
    );
    ollamaUrlInp.style.maxWidth = '320px';
    ollamaUrlRow.appendChild(ollamaUrlInp);
    ollamaSection.appendChild(ollamaUrlRow);

    const ollamaModelRow = formRow('Default Model', 'Which Ollama model to use');
    const ollamaModelInp = textInput(ollamaProvider?.default_model ?? '', 'llama3.2:3b');
    ollamaModelInp.style.maxWidth = '280px';
    ollamaModelRow.appendChild(ollamaModelInp);
    ollamaSection.appendChild(ollamaModelRow);

    // Popular models hint
    const modelsHint = document.createElement('div');
    modelsHint.style.cssText = 'display:flex;flex-wrap:wrap;gap:4px;margin:4px 0 8px 0';
    for (const m of POPULAR_MODELS.ollama) {
      const chip = document.createElement('button');
      chip.className = 'btn btn-sm';
      chip.textContent = m;
      chip.style.cssText = 'font-size:11px;padding:2px 8px;border-radius:12px';
      chip.addEventListener('click', () => {
        ollamaModelInp.value = m;
      });
      modelsHint.appendChild(chip);
    }
    ollamaSection.appendChild(modelsHint);

    // Test connection button
    const testRow = document.createElement('div');
    testRow.style.cssText = 'display:flex;gap:8px;align-items:center;margin:8px 0';
    const testBtn = document.createElement('button');
    testBtn.className = 'btn btn-sm';
    testBtn.textContent = 'Test Connection';
    const testStatus = document.createElement('span');
    testStatus.style.cssText = 'font-size:12px;color:var(--text-muted)';
    testBtn.addEventListener('click', async () => {
      testStatus.textContent = 'Testing…';
      testStatus.style.color = 'var(--text-muted)';
      try {
        const url = ollamaUrlInp.value.replace(/\/+$/, '');
        const resp = await fetch(`${url}/api/tags`);
        if (resp.ok) {
          const data = await resp.json();
          const models = (data.models ?? []) as Array<{ name: string }>;
          testStatus.textContent = `Connected! ${models.length} model${models.length !== 1 ? 's' : ''} available: ${models.map((m) => m.name).join(', ')}`;
          testStatus.style.color = 'var(--success)';
        } else {
          testStatus.textContent = `Ollama responded with ${resp.status}`;
          testStatus.style.color = 'var(--error)';
        }
      } catch (e) {
        testStatus.textContent = `Cannot reach Ollama — is it running? (${e instanceof Error ? e.message : e})`;
        testStatus.style.color = 'var(--error)';
      }
    });
    testRow.appendChild(testBtn);
    testRow.appendChild(testStatus);
    ollamaSection.appendChild(testRow);

    // Save Ollama button
    const ollamaSaveRow = document.createElement('div');
    ollamaSaveRow.style.cssText = 'margin:8px 0 16px';
    const ollamaSaveBtn = document.createElement('button');
    ollamaSaveBtn.className = 'btn btn-primary btn-sm';
    ollamaSaveBtn.textContent = hasOllama ? 'Update Ollama' : 'Add Ollama';
    ollamaSaveBtn.addEventListener('click', async () => {
      try {
        const provider: EngineProviderConfig = {
          id: 'ollama',
          kind: 'ollama',
          api_key: '', // Ollama doesn't need an API key
          base_url: ollamaUrlInp.value || 'http://localhost:11434',
          default_model: ollamaModelInp.value || undefined,
        };
        await pawEngine.upsertProvider(provider);

        // If this is the only provider, set it as default
        const freshCfg = await pawEngine.getConfig();
        if (freshCfg.providers.length === 1 || !freshCfg.default_provider) {
          freshCfg.default_provider = 'ollama';
          if (ollamaModelInp.value && !freshCfg.default_model) {
            freshCfg.default_model = ollamaModelInp.value;
          }
          await pawEngine.setConfig(freshCfg);
        }

        showToast(hasOllama ? 'Ollama updated' : 'Ollama added!', 'success');
        loadAdvancedSettings();
      } catch (e) {
        showToast(`Failed: ${e instanceof Error ? e.message : e}`, 'error');
      }
    });
    ollamaSaveRow.appendChild(ollamaSaveBtn);
    ollamaSection.appendChild(ollamaSaveRow);

    container.appendChild(ollamaSection);

    // ── All Providers ────────────────────────────────────────────────────
    const provSection = document.createElement('div');
    provSection.innerHTML =
      '<h3 class="settings-subsection-title" style="margin-top:20px">AI Providers</h3>';

    if (config.providers.length === 0) {
      const empty = document.createElement('p');
      empty.style.cssText = 'color:var(--text-muted);font-size:13px';
      empty.textContent =
        'No providers configured. Add Ollama above, or add a cloud provider below.';
      provSection.appendChild(empty);
    }

    for (const prov of config.providers) {
      const card = document.createElement('div');
      card.style.cssText =
        'padding:10px 12px;border:1px solid var(--border-color);border-radius:8px;margin-bottom:8px';
      const kindLabel = PROVIDER_KINDS.find((k) => k.value === prov.kind)?.label ?? prov.kind;
      card.innerHTML = `
        <div style="display:flex;justify-content:space-between;align-items:center">
          <div>
            <strong>${esc(kindLabel)}</strong>
            <span style="color:var(--text-muted);font-size:12px;margin-left:8px">${esc(prov.id)}</span>
            ${prov.default_model ? `<span style="color:var(--text-muted);font-size:11px;margin-left:8px">${esc(prov.default_model)}</span>` : ''}
          </div>
          <div style="display:flex;gap:4px">
            ${config.default_provider === prov.id ? '<span class="badge" style="font-size:10px;background:var(--accent);color:white">default</span>' : ''}
          </div>
        </div>
        ${prov.base_url ? `<div style="color:var(--text-muted);font-size:11px;margin-top:2px;font-family:monospace">${esc(prov.base_url)}</div>` : ''}
      `;

      const actRow = document.createElement('div');
      actRow.style.cssText = 'display:flex;gap:6px;margin-top:6px';

      if (config.default_provider !== prov.id) {
        const defaultBtn = document.createElement('button');
        defaultBtn.className = 'btn btn-sm';
        defaultBtn.textContent = 'Make Default';
        defaultBtn.addEventListener('click', async () => {
          const cfg = await pawEngine.getConfig();
          cfg.default_provider = prov.id;
          await pawEngine.setConfig(cfg);
          showToast(`${kindLabel} is now the default provider`, 'success');
          loadAdvancedSettings();
        });
        actRow.appendChild(defaultBtn);
      }

      const removeBtn = document.createElement('button');
      removeBtn.className = 'btn btn-sm';
      removeBtn.textContent = 'Remove';
      removeBtn.style.color = 'var(--danger)';
      let removePending = false;
      removeBtn.addEventListener('click', async () => {
        if (!removePending) {
          removePending = true;
          removeBtn.textContent = 'Confirm Remove?';
          setTimeout(() => {
            if (removePending) {
              removePending = false;
              removeBtn.textContent = 'Remove';
            }
          }, 4000);
          return;
        }
        removePending = false;
        try {
          await pawEngine.removeProvider(prov.id);
          showToast(`${kindLabel} removed`, 'success');
          loadAdvancedSettings();
        } catch (e) {
          showToast(`Remove failed: ${e}`, 'error');
        }
      });
      actRow.appendChild(removeBtn);

      card.appendChild(actRow);
      provSection.appendChild(card);
    }

    // ── Add New Provider ─────────────────────────────────────────────────
    const addSection = document.createElement('details');
    addSection.style.cssText = 'margin-top:12px';
    const addSummary = document.createElement('summary');
    addSummary.style.cssText = 'cursor:pointer;color:var(--accent);font-size:13px';
    addSummary.textContent = '+ Add Cloud Provider';
    addSection.appendChild(addSummary);

    const addBody = document.createElement('div');
    addBody.style.cssText =
      'margin-top:8px;display:flex;flex-direction:column;gap:6px;padding:12px;border:1px solid var(--border-color);border-radius:8px';

    const kindRow = formRow('Provider Type');
    const kindSel = selectInput(
      PROVIDER_KINDS.filter((k) => k.value !== 'ollama'),
      'anthropic',
    );
    kindSel.style.maxWidth = '220px';
    kindRow.appendChild(kindSel);
    addBody.appendChild(kindRow);

    const newKeyRow = formRow('API Key');
    const newKeyInp = textInput('', 'sk-...', 'password');
    newKeyRow.appendChild(newKeyInp);
    addBody.appendChild(newKeyRow);

    const newModelRow = formRow('Default Model', 'Leave blank to auto-detect');
    const newModelInp = textInput('', '');
    newModelRow.appendChild(newModelInp);
    addBody.appendChild(newModelRow);

    const newUrlRow = formRow('Base URL', 'Only needed for custom/self-hosted providers');
    const newUrlInp = textInput('', DEFAULT_BASE_URLS.openai);
    newUrlRow.appendChild(newUrlInp);
    addBody.appendChild(newUrlRow);

    // Update model hints when kind changes
    kindSel.addEventListener('change', () => {
      const kind = kindSel.value;
      newUrlInp.placeholder = DEFAULT_BASE_URLS[kind] ?? '';
      const models = POPULAR_MODELS[kind] ?? [];
      newModelInp.placeholder = models[0] ?? '';
    });

    const addBtn = document.createElement('button');
    addBtn.className = 'btn btn-primary btn-sm';
    addBtn.textContent = 'Add Provider';
    addBtn.style.alignSelf = 'flex-start';
    addBtn.addEventListener('click', async () => {
      const kind = kindSel.value;
      const apiKey = newKeyInp.value.trim();
      if (!apiKey && kind !== 'ollama') {
        showToast('API key is required', 'error');
        return;
      }
      try {
        const provider: EngineProviderConfig = {
          id: kind,
          kind: kind as EngineProviderConfig['kind'],
          api_key: apiKey,
          base_url: newUrlInp.value.trim() || undefined,
          default_model: newModelInp.value.trim() || undefined,
        };
        await pawEngine.upsertProvider(provider);
        showToast(
          `${PROVIDER_KINDS.find((k) => k.value === kind)?.label ?? kind} added`,
          'success',
        );
        loadAdvancedSettings();
      } catch (e) {
        showToast(`Failed: ${e}`, 'error');
      }
    });
    addBody.appendChild(addBtn);
    addSection.appendChild(addBody);
    provSection.appendChild(addSection);

    container.appendChild(provSection);

    // ── Engine Defaults ──────────────────────────────────────────────────
    const engSection = document.createElement('div');
    engSection.innerHTML =
      '<h3 class="settings-subsection-title" style="margin-top:20px">Engine Settings</h3>';

    const modelRow = formRow('Default Model', 'The model used when no override is set');
    const modelInp = textInput(config.default_model ?? '', 'gpt-4o');
    modelInp.style.maxWidth = '280px';
    modelRow.appendChild(modelInp);
    engSection.appendChild(modelRow);

    const providerRow = formRow('Default Provider', 'Which provider to use by default');
    const providerOpts = [
      { value: '', label: '(auto-detect from model)' },
      ...config.providers.map((p) => ({
        value: p.id,
        label: `${PROVIDER_KINDS.find((k) => k.value === p.kind)?.label ?? p.kind} (${p.id})`,
      })),
    ];
    const providerSel = selectInput(providerOpts, config.default_provider ?? '');
    providerSel.style.maxWidth = '280px';
    providerRow.appendChild(providerSel);
    engSection.appendChild(providerRow);

    const roundsRow = formRow(
      'Max Tool Rounds',
      'How many tool call rounds before stopping (default: 20)',
    );
    const roundsInp = numberInput(config.max_tool_rounds, { min: 1, max: 100, placeholder: '20' });
    roundsInp.style.maxWidth = '120px';
    roundsRow.appendChild(roundsInp);
    engSection.appendChild(roundsRow);

    const timeoutRow = formRow('Tool Timeout (seconds)', 'Max seconds for a single tool execution');
    const timeoutInp = numberInput(config.tool_timeout_secs, {
      min: 5,
      step: 5,
      placeholder: '120',
    });
    timeoutInp.style.maxWidth = '140px';
    timeoutRow.appendChild(timeoutInp);
    engSection.appendChild(timeoutRow);

    const concurrencyRow = formRow(
      'Max Concurrent Runs',
      'How many agent runs (chat + cron + tasks) can execute in parallel. Chat always gets priority. Increase if you have multiple providers or a high rate limit.',
    );
    const concurrencyInp = numberInput(config.max_concurrent_runs ?? 4, {
      min: 1,
      max: 20,
      placeholder: '4',
    });
    concurrencyInp.style.maxWidth = '120px';
    concurrencyRow.appendChild(concurrencyInp);
    engSection.appendChild(concurrencyRow);

    const budgetRow = formRow(
      'Daily Budget (USD)',
      'Estimated daily spend limit. Agent stops when exceeded. Set to 0 to disable.',
    );
    const budgetInp = numberInput(config.daily_budget_usd ?? 10, {
      min: 0,
      step: 1,
      placeholder: '10',
    });
    budgetInp.style.maxWidth = '120px';
    budgetRow.appendChild(budgetInp);
    engSection.appendChild(budgetRow);

    const contextRow = formRow(
      'Context Window (tokens)',
      'How much conversation history the agent sees. Higher = better topic tracking but more cost per turn. Models support 128K-1M, so this is conservative. Default: 32,000.',
    );
    const contextInp = numberInput(config.context_window_tokens ?? 32000, {
      min: 4000,
      max: 1000000,
      step: 4000,
      placeholder: '32000',
    });
    contextInp.style.maxWidth = '140px';
    contextRow.appendChild(contextInp);
    engSection.appendChild(contextRow);

    container.appendChild(engSection);

    // ── System Prompt ────────────────────────────────────────────────────
    const promptSection = document.createElement('div');
    promptSection.innerHTML =
      '<h3 class="settings-subsection-title" style="margin-top:20px">Default System Prompt</h3>';

    const promptArea = document.createElement('textarea');
    promptArea.className = 'form-input';
    promptArea.style.cssText =
      'width:100%;min-height:120px;font-family:var(--font-mono);font-size:12px;resize:vertical';
    promptArea.value = config.default_system_prompt ?? '';
    promptArea.placeholder = 'You are a helpful AI assistant...';
    promptSection.appendChild(promptArea);

    container.appendChild(promptSection);

    // ── Save All ─────────────────────────────────────────────────────────
    container.appendChild(
      saveReloadButtons(
        async () => {
          try {
            const cfg = await pawEngine.getConfig();
            cfg.default_model = modelInp.value.trim() || undefined;
            cfg.default_provider = providerSel.value || undefined;
            cfg.max_tool_rounds = parseInt(roundsInp.value) || 20;
            cfg.tool_timeout_secs = parseInt(timeoutInp.value) || 120;
            cfg.max_concurrent_runs = parseInt(concurrencyInp.value) || 4;
            cfg.daily_budget_usd = parseFloat(budgetInp.value) || 0;
            cfg.context_window_tokens = parseInt(contextInp.value) || 32000;
            cfg.default_system_prompt = promptArea.value.trim() || undefined;
            await pawEngine.setConfig(cfg);
            showToast('Engine settings saved', 'success');
          } catch (e) {
            showToast(`Save failed: ${e instanceof Error ? e.message : e}`, 'error');
          }
        },
        () => loadAdvancedSettings(),
      ),
    );
  } catch (e) {
    container.innerHTML = `<p style="color:var(--danger)">Failed to load engine config: ${esc(String(e))}</p>`;
  }
}
