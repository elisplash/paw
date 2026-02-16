// Settings: Agent Defaults
// Configure agents.defaults — thinking, verbose, timeout, workspace, etc.
// ~180 lines

import {
  getConfig, patchConfig, getVal, isConnected,
  esc, formRow, selectInput, textInput, numberInput, toggleSwitch, saveReloadButtons
} from './settings-config';

const $ = (id: string) => document.getElementById(id);

// ── Option sets ─────────────────────────────────────────────────────────────

const THINKING_LEVELS = [
  { value: 'off', label: 'Off' }, { value: 'minimal', label: 'Minimal' },
  { value: 'low', label: 'Low' }, { value: 'medium', label: 'Medium' },
  { value: 'high', label: 'High' }, { value: 'xhigh', label: 'Extra High' },
];

const VERBOSE_MODES = [
  { value: 'off', label: 'Off' }, { value: 'on', label: 'On' }, { value: 'full', label: 'Full' },
];

const ELEVATED_MODES = [
  { value: 'off', label: 'Off' }, { value: 'on', label: 'On' },
  { value: 'ask', label: 'Ask' }, { value: 'full', label: 'Full' },
];

const TIME_FORMATS = [
  { value: 'auto', label: 'Auto' }, { value: '12', label: '12-hour' }, { value: '24', label: '24-hour' },
];

const TYPING_MODES = [
  { value: 'never', label: 'Never' }, { value: 'instant', label: 'Instant' },
  { value: 'thinking', label: 'While thinking' }, { value: 'message', label: 'While messaging' },
];

const STREAMING_MODES = [
  { value: 'off', label: 'Off (stream)' }, { value: 'on', label: 'On (block)' },
];

// ── Render ──────────────────────────────────────────────────────────────────

export async function loadAgentDefaultsSettings() {
  if (!isConnected()) return;
  const container = $('settings-agent-defaults-content');
  if (!container) return;
  container.innerHTML = '<p style="color:var(--text-muted)">Loading…</p>';

  try {
    const config = await getConfig();
    const d = (getVal(config, 'agents.defaults') ?? {}) as Record<string, any>;
    container.innerHTML = '';

    // ── AI Behaviour ─────────────────────────────────────────────────────
    const aiSection = document.createElement('div');
    aiSection.innerHTML = '<h3 class="settings-subsection-title">AI Behaviour</h3>';

    const thinkRow = formRow('Thinking Level', 'How much reasoning the model does before responding');
    const thinkSel = selectInput(THINKING_LEVELS, String(d.thinkingDefault ?? 'off'));
    thinkSel.style.maxWidth = '200px';
    thinkRow.appendChild(thinkSel);
    aiSection.appendChild(thinkRow);

    const verbRow = formRow('Verbose Output', 'Level of detail in agent responses');
    const verbSel = selectInput(VERBOSE_MODES, String(d.verboseDefault ?? 'off'));
    verbSel.style.maxWidth = '200px';
    verbRow.appendChild(verbSel);
    aiSection.appendChild(verbRow);

    const elevRow = formRow('Elevated Mode', 'Grant agent elevated tool permissions');
    const elevSel = selectInput(ELEVATED_MODES, String(d.elevatedDefault ?? 'off'));
    elevSel.style.maxWidth = '200px';
    elevRow.appendChild(elevSel);
    aiSection.appendChild(elevRow);

    const streamRow = formRow('Block Streaming', 'Wait for full response instead of streaming');
    const streamSel = selectInput(STREAMING_MODES, String(d.blockStreamingDefault ?? 'off'));
    streamSel.style.maxWidth = '200px';
    streamRow.appendChild(streamSel);
    aiSection.appendChild(streamRow);

    container.appendChild(aiSection);

    // ── Limits ───────────────────────────────────────────────────────────
    const limSection = document.createElement('div');
    limSection.innerHTML = '<h3 class="settings-subsection-title" style="margin-top:20px">Limits & Timeouts</h3>';

    const ctxRow = formRow('Context Tokens', 'Maximum context window size for conversations');
    const ctxInp = numberInput(d.contextTokens, { min: 1000, step: 1000, placeholder: '200000' });
    ctxInp.style.maxWidth = '160px';
    ctxRow.appendChild(ctxInp);
    limSection.appendChild(ctxRow);

    const concRow = formRow('Max Concurrent Agents', 'How many agents can run simultaneously');
    const concInp = numberInput(d.maxConcurrent, { min: 1, max: 50, placeholder: '5' });
    concInp.style.maxWidth = '120px';
    concRow.appendChild(concInp);
    limSection.appendChild(concRow);

    const timeRow = formRow('Timeout (seconds)', 'Max seconds per agent turn before timeout');
    const timeInp = numberInput(d.timeoutSeconds, { min: 10, step: 10, placeholder: '300' });
    timeInp.style.maxWidth = '140px';
    timeRow.appendChild(timeInp);
    limSection.appendChild(timeRow);

    const mediaRow = formRow('Media Max MB', 'Maximum file size for media attachments');
    const mediaInp = numberInput(d.mediaMaxMb, { min: 0, step: 1, placeholder: '25' });
    mediaInp.style.maxWidth = '120px';
    mediaRow.appendChild(mediaInp);
    limSection.appendChild(mediaRow);

    container.appendChild(limSection);

    // ── Workspace & Paths ────────────────────────────────────────────────
    const pathSection = document.createElement('div');
    pathSection.innerHTML = '<h3 class="settings-subsection-title" style="margin-top:20px">Workspace & Paths</h3>';

    const wsRow = formRow('Workspace Path', 'Default agent workspace directory');
    const wsInp = textInput(d.workspace ?? '', '/path/to/workspace');
    wsRow.appendChild(wsInp);
    pathSection.appendChild(wsRow);

    const repoRow = formRow('Repo Root', 'Root directory for git operations');
    const repoInp = textInput(d.repoRoot ?? '', '/path/to/repo');
    repoRow.appendChild(repoInp);
    pathSection.appendChild(repoRow);

    const { container: skipToggle, checkbox: skipCb } = toggleSwitch(
      d.skipBootstrap === true,
      'Skip Bootstrap (don\'t run bootstrap on agent start)'
    );
    pathSection.appendChild(skipToggle);

    container.appendChild(pathSection);

    // ── Display ──────────────────────────────────────────────────────────
    const dispSection = document.createElement('div');
    dispSection.innerHTML = '<h3 class="settings-subsection-title" style="margin-top:20px">Display Preferences</h3>';

    const tzRow = formRow('User Timezone', 'Timezone for timestamps (e.g. America/New_York)');
    const tzInp = textInput(d.userTimezone ?? '', 'America/New_York');
    tzInp.style.maxWidth = '260px';
    tzRow.appendChild(tzInp);
    dispSection.appendChild(tzRow);

    const tfRow = formRow('Time Format');
    const tfSel = selectInput(TIME_FORMATS, String(d.timeFormat ?? 'auto'));
    tfSel.style.maxWidth = '160px';
    tfRow.appendChild(tfSel);
    dispSection.appendChild(tfRow);

    const typRow = formRow('Typing Indicator', 'When to show typing indicator in channels');
    const typSel = selectInput(TYPING_MODES, String(d.typingMode ?? 'never'));
    typSel.style.maxWidth = '200px';
    typRow.appendChild(typSel);
    dispSection.appendChild(typRow);

    container.appendChild(dispSection);

    // ── Save ─────────────────────────────────────────────────────────────
    container.appendChild(saveReloadButtons(
      async () => {
        const patch: Record<string, unknown> = {
          thinkingDefault: thinkSel.value,
          verboseDefault: verbSel.value,
          elevatedDefault: elevSel.value,
          blockStreamingDefault: streamSel.value,
          contextTokens: parseInt(ctxInp.value) || undefined,
          maxConcurrent: parseInt(concInp.value) || undefined,
          timeoutSeconds: parseInt(timeInp.value) || undefined,
          mediaMaxMb: parseFloat(mediaInp.value) || undefined,
          workspace: wsInp.value || undefined,
          repoRoot: repoInp.value || undefined,
          skipBootstrap: skipCb.checked,
          userTimezone: tzInp.value || undefined,
          timeFormat: tfSel.value,
          typingMode: typSel.value,
        };
        // Remove undefined values
        for (const k of Object.keys(patch)) {
          if (patch[k] === undefined) delete patch[k];
        }
        await patchConfig({ agents: { defaults: patch } });
      },
      () => loadAgentDefaultsSettings()
    ));

  } catch (e) {
    container.innerHTML = `<p style="color:var(--danger)">Failed to load: ${esc(String(e))}</p>`;
  }
}

export function initAgentDefaultsSettings() {
  // All dynamic
}
