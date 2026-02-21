// Settings: Agent Defaults — DOM rendering + IPC

import { pawEngine } from '../../engine';
import { showToast } from '../../components/toast';
import { isConnected } from '../../state/connection';
import {
  esc,
  formRow,
  selectInput,
  textInput,
  numberInput,
  toggleSwitch,
  saveReloadButtons,
} from '../settings-config';
import { $ } from '../../components/helpers';

// ── Render ──────────────────────────────────────────────────────────────────

export async function loadAgentDefaultsSettings() {
  if (!isConnected()) return;
  const container = $('settings-agent-defaults-content');
  if (!container) return;
  container.innerHTML = '<p style="color:var(--text-muted)">Loading…</p>';

  try {
    const config = await pawEngine.getConfig();
    const memConfig = await pawEngine.getMemoryConfig();
    container.innerHTML = '';

    // ── Default Model & Provider ─────────────────────────────────────────
    const modelSection = document.createElement('div');
    modelSection.innerHTML = '<h3 class="settings-subsection-title">Model & Provider</h3>';

    const modelRow = formRow('Default Model', 'The AI model used for new conversations');
    const modelInp = textInput(config.default_model ?? '', 'gpt-4o');
    modelInp.style.maxWidth = '280px';
    modelRow.appendChild(modelInp);
    modelSection.appendChild(modelRow);

    // Quick-pick model chips from configured providers
    if (config.providers.length > 0) {
      const chipsRow = document.createElement('div');
      chipsRow.style.cssText = 'display:flex;flex-wrap:wrap;gap:4px;margin:4px 0 8px 0';
      for (const prov of config.providers) {
        if (prov.default_model) {
          const chip = document.createElement('button');
          chip.className = 'btn btn-sm';
          chip.textContent = `${prov.default_model} (${prov.kind})`;
          chip.style.cssText = 'font-size:11px;padding:2px 8px;border-radius:12px';
          chip.addEventListener('click', () => {
            modelInp.value = prov.default_model!;
          });
          chipsRow.appendChild(chip);
        }
      }
      if (chipsRow.children.length > 0) modelSection.appendChild(chipsRow);
    }

    const providerRow = formRow(
      'Default Provider',
      'Which provider to use when auto-detection fails',
    );
    const providerOpts = [
      { value: '', label: '(auto-detect from model name)' },
      ...config.providers.map((p) => ({ value: p.id, label: `${p.kind} (${p.id})` })),
    ];
    const providerSel = selectInput(providerOpts, config.default_provider ?? '');
    providerSel.style.maxWidth = '280px';
    providerRow.appendChild(providerSel);
    modelSection.appendChild(providerRow);

    container.appendChild(modelSection);

    // ── Tool Execution ───────────────────────────────────────────────────
    const toolSection = document.createElement('div');
    toolSection.innerHTML =
      '<h3 class="settings-subsection-title" style="margin-top:20px">Tool Execution</h3>';

    const roundsRow = formRow(
      'Max Tool Rounds',
      'How many tool call rounds before the agent stops (default: 20)',
    );
    const roundsInp = numberInput(config.max_tool_rounds, { min: 1, max: 100, placeholder: '20' });
    roundsInp.style.maxWidth = '120px';
    roundsRow.appendChild(roundsInp);
    toolSection.appendChild(roundsRow);

    const timeoutRow = formRow(
      'Tool Timeout (seconds)',
      'Max seconds for a single tool execution (default: 120)',
    );
    const timeoutInp = numberInput(config.tool_timeout_secs, {
      min: 5,
      step: 5,
      placeholder: '120',
    });
    timeoutInp.style.maxWidth = '140px';
    timeoutRow.appendChild(timeoutInp);
    toolSection.appendChild(timeoutRow);

    const tzRow = formRow(
      'User Timezone',
      'IANA timezone (e.g. America/Chicago, America/New_York, Europe/London). Used for agent time awareness.',
    );
    const tzInp = document.createElement('input');
    tzInp.className = 'form-input';
    tzInp.type = 'text';
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    tzInp.value = (config as any).user_timezone ?? 'America/Chicago';
    tzInp.placeholder = 'America/Chicago';
    tzInp.style.maxWidth = '240px';
    tzRow.appendChild(tzInp);
    toolSection.appendChild(tzRow);

    container.appendChild(toolSection);

    // ── System Prompt ────────────────────────────────────────────────────
    const promptSection = document.createElement('div');
    promptSection.innerHTML =
      '<h3 class="settings-subsection-title" style="margin-top:20px">Default System Prompt</h3>';
    promptSection.innerHTML +=
      '<p class="form-hint" style="margin:0 0 8px;font-size:11px;color:var(--text-muted)">Base instructions prepended to every conversation. Agent soul files (SOUL.md, IDENTITY.md, etc.) are appended on top of this.</p>';

    const promptArea = document.createElement('textarea');
    promptArea.className = 'form-input';
    promptArea.style.cssText =
      'width:100%;min-height:140px;font-family:var(--font-mono);font-size:12px;resize:vertical';
    promptArea.value = config.default_system_prompt ?? '';
    promptArea.placeholder = 'You are a helpful AI assistant. You have access to tools...';
    promptSection.appendChild(promptArea);

    container.appendChild(promptSection);

    // ── Memory Defaults ──────────────────────────────────────────────────
    const memSection = document.createElement('div');
    memSection.innerHTML =
      '<h3 class="settings-subsection-title" style="margin-top:20px">Memory Defaults</h3>';

    const { container: recallToggle, checkbox: recallCb } = toggleSwitch(
      memConfig.auto_recall,
      'Auto-recall relevant memories before each turn',
    );
    memSection.appendChild(recallToggle);

    const { container: captureToggle, checkbox: captureCb } = toggleSwitch(
      memConfig.auto_capture,
      'Auto-capture facts from conversations',
    );
    memSection.appendChild(captureToggle);

    const recallLimitRow = formRow('Recall Limit', 'Max memories to inject per turn');
    const recallLimitInp = numberInput(memConfig.recall_limit, {
      min: 1,
      max: 50,
      placeholder: '5',
    });
    recallLimitInp.style.maxWidth = '100px';
    recallLimitRow.appendChild(recallLimitInp);
    memSection.appendChild(recallLimitRow);

    container.appendChild(memSection);

    // ── Embedding Configuration ──────────────────────────────────────────
    const embSection = document.createElement('div');
    embSection.innerHTML =
      '<h3 class="settings-subsection-title" style="margin-top:20px">Embedding (Semantic Search)</h3>';
    embSection.innerHTML +=
      '<p class="form-hint" style="margin:0 0 8px;font-size:11px;color:var(--text-muted)">Ollama runs locally and powers semantic memory search. The embedding model converts text to vectors for similarity matching. Pawz will auto-start Ollama and pull the model if needed.</p>';

    // Auto-Setup button (prominent)
    const autoSetupRow = document.createElement('div');
    autoSetupRow.style.cssText =
      'display:flex;align-items:center;gap:10px;margin:0 0 12px 0;padding:10px 14px;border:1px solid var(--border-color);border-radius:8px;background:var(--bg-secondary, rgba(255,255,255,0.03))';
    const autoSetupBtn = document.createElement('button');
    autoSetupBtn.className = 'btn btn-primary btn-sm';
    autoSetupBtn.textContent = 'Auto-Setup Ollama Embeddings';
    autoSetupBtn.style.whiteSpace = 'nowrap';
    const autoSetupStatus = document.createElement('span');
    autoSetupStatus.style.cssText = 'font-size:12px;color:var(--text-muted);line-height:1.4';
    autoSetupStatus.textContent =
      'Checks Ollama, starts it if needed, and pulls the embedding model';
    autoSetupRow.appendChild(autoSetupBtn);
    autoSetupRow.appendChild(autoSetupStatus);
    embSection.appendChild(autoSetupRow);

    autoSetupBtn.addEventListener('click', async () => {
      autoSetupBtn.disabled = true;
      autoSetupBtn.textContent = '⏳ Setting up…';
      autoSetupStatus.textContent = 'Starting Ollama and checking embedding model…';
      autoSetupStatus.style.color = 'var(--text-muted)';
      try {
        // Save current form values first
        const mc = await pawEngine.getMemoryConfig();
        mc.embedding_base_url = embUrlInp.value.trim() || 'http://localhost:11434';
        mc.embedding_model = embModelInp.value.trim() || 'nomic-embed-text';
        mc.embedding_dims = parseInt(embDimsInp.value) || 768;
        await pawEngine.setMemoryConfig(mc);

        const result = await pawEngine.ensureEmbeddingReady();
        if (result.error) {
          autoSetupStatus.textContent = `✗ ${result.error}`;
          autoSetupStatus.style.color = 'var(--text-danger)';
        } else {
          let msg = `✓ Ready! ${result.model_name} — ${result.embedding_dims} dimensions`;
          if (result.was_auto_started) msg += ' (Ollama auto-started)';
          if (result.was_auto_pulled) msg += ' (model auto-pulled)';
          autoSetupStatus.textContent = msg;
          autoSetupStatus.style.color = 'var(--text-success)';
          // Update dims field with actual value
          if (result.embedding_dims > 0) embDimsInp.value = String(result.embedding_dims);
          showToast('Semantic memory is ready!', 'success');
        }
      } catch (e) {
        const err = e instanceof Error ? e.message : String(e);
        autoSetupStatus.textContent = `✗ ${err}`;
        autoSetupStatus.style.color = 'var(--text-danger)';
      } finally {
        autoSetupBtn.disabled = false;
        autoSetupBtn.textContent = 'Auto-Setup Ollama Embeddings';
      }
    });

    const embUrlRow = formRow(
      'Ollama URL',
      'Where Ollama is running (default: http://localhost:11434)',
    );
    const embUrlInp = textInput(
      memConfig.embedding_base_url || 'http://localhost:11434',
      'http://localhost:11434',
    );
    embUrlInp.style.maxWidth = '320px';
    embUrlRow.appendChild(embUrlInp);
    embSection.appendChild(embUrlRow);

    const embModelRow = formRow('Embedding Model', 'Ollama model for generating embeddings');
    const embModelInp = textInput(
      memConfig.embedding_model || 'nomic-embed-text',
      'nomic-embed-text',
    );
    embModelInp.style.maxWidth = '220px';
    embModelRow.appendChild(embModelInp);
    embSection.appendChild(embModelRow);

    // Model quick-picks
    const modelChipsRow = document.createElement('div');
    modelChipsRow.style.cssText = 'display:flex;flex-wrap:wrap;gap:4px;margin:2px 0 8px 0';
    for (const m of [
      'nomic-embed-text',
      'all-minilm',
      'mxbai-embed-large',
      'snowflake-arctic-embed',
    ]) {
      const chip = document.createElement('button');
      chip.className = 'btn btn-sm';
      chip.textContent = m;
      chip.style.cssText = 'font-size:11px;padding:2px 8px;border-radius:12px';
      chip.addEventListener('click', () => {
        embModelInp.value = m;
      });
      modelChipsRow.appendChild(chip);
    }
    embSection.appendChild(modelChipsRow);

    const embDimsRow = formRow(
      'Embedding Dimensions',
      'Auto-detected when you run Test or Auto-Setup',
    );
    const embDimsInp = numberInput(memConfig.embedding_dims || 768, {
      min: 64,
      max: 4096,
      placeholder: '768',
    });
    embDimsInp.style.maxWidth = '120px';
    embDimsRow.appendChild(embDimsInp);
    embSection.appendChild(embDimsRow);

    // Status / test button
    const embStatusRow = document.createElement('div');
    embStatusRow.style.cssText = 'display:flex;align-items:center;gap:8px;margin:10px 0';
    const testBtn = document.createElement('button');
    testBtn.className = 'btn btn-sm';
    testBtn.textContent = 'Test Connection';
    const backfillBtn = document.createElement('button');
    backfillBtn.className = 'btn btn-sm';
    backfillBtn.textContent = 'Backfill Embeddings';
    backfillBtn.title = 'Embed any memories that were stored without vectors';
    const statusSpan = document.createElement('span');
    statusSpan.style.cssText = 'font-size:12px;color:var(--text-muted)';
    embStatusRow.appendChild(testBtn);
    embStatusRow.appendChild(backfillBtn);
    embStatusRow.appendChild(statusSpan);
    embSection.appendChild(embStatusRow);

    testBtn.addEventListener('click', async () => {
      testBtn.disabled = true;
      statusSpan.textContent = 'Testing...';
      statusSpan.style.color = 'var(--text-muted)';
      try {
        // Save current values first so the test uses them
        const mc = await pawEngine.getMemoryConfig();
        mc.embedding_base_url = embUrlInp.value.trim() || 'http://localhost:11434';
        mc.embedding_model = embModelInp.value.trim() || 'nomic-embed-text';
        mc.embedding_dims = parseInt(embDimsInp.value) || 768;
        await pawEngine.setMemoryConfig(mc);

        const dims = await pawEngine.testEmbedding();
        statusSpan.textContent = `✓ Connected — ${dims} dimensions`;
        statusSpan.style.color = 'var(--text-success)';
        if (dims > 0) embDimsInp.value = String(dims);
      } catch (e) {
        const err = e instanceof Error ? e.message : String(e);
        statusSpan.textContent = `✗ ${err}`;
        statusSpan.style.color = 'var(--text-danger)';
      } finally {
        testBtn.disabled = false;
      }
    });

    backfillBtn.addEventListener('click', async () => {
      backfillBtn.disabled = true;
      statusSpan.textContent = 'Backfilling…';
      statusSpan.style.color = 'var(--text-muted)';
      try {
        const result = await pawEngine.memoryBackfill();
        statusSpan.textContent = `✓ Backfill: ${result.success} embedded, ${result.failed} failed`;
        statusSpan.style.color =
          result.failed > 0 ? 'var(--text-warning, orange)' : 'var(--text-success)';
      } catch (e) {
        statusSpan.textContent = `✗ Backfill failed: ${e instanceof Error ? e.message : e}`;
        statusSpan.style.color = 'var(--text-danger)';
      } finally {
        backfillBtn.disabled = false;
      }
    });

    // Check Ollama status on load
    (async () => {
      try {
        const embStatus = await pawEngine.embeddingStatus();
        if (embStatus.ollama_running && embStatus.model_available) {
          statusSpan.textContent = `✓ Ollama running, ${embStatus.model_name} available`;
          statusSpan.style.color = 'var(--text-success)';
          autoSetupStatus.textContent = `✓ Ollama is running and ${embStatus.model_name} is available`;
          autoSetupStatus.style.color = 'var(--text-success)';
        } else if (embStatus.ollama_running) {
          statusSpan.textContent = `Ollama running but ${embStatus.model_name} not pulled yet`;
          statusSpan.style.color = 'var(--text-warning, orange)';
          autoSetupStatus.textContent = `Ollama is running but ${embStatus.model_name} needs to be pulled — click Auto-Setup`;
          autoSetupStatus.style.color = 'var(--text-warning, orange)';
        } else {
          statusSpan.textContent = 'Ollama not detected — click Auto-Setup to start it';
          statusSpan.style.color = 'var(--text-warning, orange)';
        }
      } catch {
        /* ignore */
      }
    })();

    container.appendChild(embSection);

    // ── Save ─────────────────────────────────────────────────────────────
    container.appendChild(
      saveReloadButtons(
        async () => {
          try {
            // Save engine config
            const cfg = await pawEngine.getConfig();
            cfg.default_model = modelInp.value.trim() || undefined;
            cfg.default_provider = providerSel.value || undefined;
            cfg.max_tool_rounds = parseInt(roundsInp.value) || 20;
            cfg.tool_timeout_secs = parseInt(timeoutInp.value) || 120;
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            (cfg as any).user_timezone = tzInp.value.trim() || 'America/Chicago';
            cfg.default_system_prompt = promptArea.value.trim() || undefined;
            await pawEngine.setConfig(cfg);

            // Save memory config (including embedding settings)
            const mc = await pawEngine.getMemoryConfig();
            mc.auto_recall = recallCb.checked;
            mc.auto_capture = captureCb.checked;
            mc.recall_limit = parseInt(recallLimitInp.value) || 5;
            mc.embedding_base_url = embUrlInp.value.trim() || 'http://localhost:11434';
            mc.embedding_model = embModelInp.value.trim() || 'nomic-embed-text';
            mc.embedding_dims = parseInt(embDimsInp.value) || 768;
            await pawEngine.setMemoryConfig(mc);

            showToast('Agent defaults saved', 'success');
          } catch (e) {
            showToast(`Save failed: ${e instanceof Error ? e.message : e}`, 'error');
          }
        },
        () => loadAgentDefaultsSettings(),
      ),
    );
  } catch (e) {
    container.innerHTML = `<p style="color:var(--danger)">Failed to load: ${esc(String(e))}</p>`;
  }
}
