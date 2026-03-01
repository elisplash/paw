// ─────────────────────────────────────────────────────────────────────────────
// Panel Config Fields — Per-kind configuration field HTML generators
// Extracted from panel-molecules.ts to keep each module focused.
// ─────────────────────────────────────────────────────────────────────────────

import type { FlowNode } from './atoms';
import { getAvailableAgents, escAttr } from './molecule-state';
import { CRON_PRESETS, validateCron, describeCron, nextCronFire } from './cron-atoms';

/**
 * Build the HTML string for per-kind config fields in the node panel.
 * Returns the concatenated HTML for all applicable config sections.
 */
export function buildConfigFieldsHtml(node: FlowNode): string {
  const config = node.config ?? {};
  const promptVal = escAttr((config.prompt as string) ?? '');
  const modelVal = escAttr((config.model as string) ?? '');
  const conditionVal = escAttr((config.conditionExpr as string) ?? '');
  const transformVal = escAttr((config.transform as string) ?? '');
  const codeVal = escAttr((config.code as string) ?? '');
  const outputTarget = (config.outputTarget as string) ?? 'chat';
  const timeoutVal = (config.timeoutMs as number) ?? 120000;
  const _availableAgents = getAvailableAgents();

  let html = '';

  // ── Prompt field (agent, tool, data, trigger) ────────────────────────────

  if (
    node.kind === 'agent' ||
    node.kind === 'tool' ||
    node.kind === 'data' ||
    node.kind === 'trigger'
  ) {
    html += `
      <label class="flow-panel-field">
        <span>Prompt</span>
        <textarea class="flow-panel-textarea" data-config="prompt" rows="3" placeholder="Instructions for this step…">${promptVal}</textarea>
      </label>
    `;
  }

  // ── Schedule section (trigger) ───────────────────────────────────────────

  if (node.kind === 'trigger') {
    const scheduleVal = (config.schedule as string) ?? '';
    const scheduleEnabled = (config.scheduleEnabled as boolean) ?? false;
    const cronError = scheduleVal ? validateCron(scheduleVal) : null;
    const cronDesc = scheduleVal && !cronError ? describeCron(scheduleVal) : '';
    const nextFire = scheduleVal && !cronError ? nextCronFire(scheduleVal) : null;
    const nextFireStr = nextFire ? nextFire.toLocaleString() : '';

    const presetOptionsHtml = CRON_PRESETS.map(
      (p) =>
        `<option value="${p.value}"${scheduleVal === p.value ? ' selected' : ''}>${p.label}</option>`,
    ).join('');

    html += `
      <div class="flow-panel-schedule">
        <div class="flow-panel-schedule-header">
          <span class="ms" style="font-size:16px">schedule</span>
          <span>Schedule</span>
          <label class="flow-panel-toggle">
            <input type="checkbox" data-config="scheduleEnabled" ${scheduleEnabled ? 'checked' : ''} />
            <span>${scheduleEnabled ? 'On' : 'Off'}</span>
          </label>
        </div>
        <label class="flow-panel-field">
          <span>Preset</span>
          <select class="flow-panel-select" data-schedule-preset>
            <option value="">Custom…</option>
            ${presetOptionsHtml}
          </select>
        </label>
        <label class="flow-panel-field">
          <span>Cron Expression</span>
          <input type="text" class="flow-panel-input flow-panel-cron-input" data-config="schedule" value="${escAttr(scheduleVal)}" placeholder="* * * * *" spellcheck="false" />
          ${cronError ? `<span class="flow-panel-cron-error">${cronError}</span>` : ''}
          ${cronDesc ? `<span class="flow-panel-cron-desc">${cronDesc}</span>` : ''}
        </label>
        ${nextFireStr ? `<div class="flow-panel-cron-next"><span class="ms" style="font-size:14px">event_upcoming</span> Next: ${nextFireStr}</div>` : ''}
      </div>
    `;
  }

  // ── Agent / Tool fields ──────────────────────────────────────────────────

  if (node.kind === 'agent' || node.kind === 'tool') {
    const rawAgentId = (config.agentId as string) ?? '';
    const agentOptions =
      _availableAgents.length > 0
        ? [
            { id: '', name: '— Select Agent —' },
            { id: 'default', name: 'Default' },
            ..._availableAgents,
          ]
            .map(
              (a) =>
                `<option value="${escAttr(a.id)}"${a.id === rawAgentId ? ' selected' : ''}>${a.name}</option>`,
            )
            .join('')
        : `<option value="">default</option>`;

    html += `
      <label class="flow-panel-field">
        <span>Agent</span>
        <select class="flow-panel-select" data-config="agentId">
          ${agentOptions}
        </select>
      </label>
      <label class="flow-panel-field">
        <span>Model</span>
        <input type="text" class="flow-panel-input" data-config="model" value="${modelVal}" placeholder="inherit from agent" />
      </label>
    `;
  }

  // ── Condition ────────────────────────────────────────────────────────────

  if (node.kind === 'condition') {
    html += `
      <label class="flow-panel-field">
        <span>Condition</span>
        <textarea class="flow-panel-textarea" data-config="conditionExpr" rows="2" placeholder="e.g. Does the input contain valid data?">${conditionVal}</textarea>
      </label>
    `;
  }

  // ── Data transform ───────────────────────────────────────────────────────

  if (node.kind === 'data') {
    html += `
      <label class="flow-panel-field">
        <span>Transform</span>
        <textarea class="flow-panel-textarea" data-config="transform" rows="2" placeholder="e.g. Extract the top 3 results">${transformVal}</textarea>
      </label>
    `;
  }

  // ── Code ─────────────────────────────────────────────────────────────────

  if (node.kind === 'code') {
    html += `
      <label class="flow-panel-field">
        <span>JavaScript Code</span>
        <textarea class="flow-panel-textarea flow-panel-code" data-config="code" rows="8" placeholder="// Input available as: input (string), data (parsed JSON)
// Return a value or use console.log()
return input.toUpperCase();">${codeVal}</textarea>
        <span class="flow-panel-hint">Sandboxed: no window, document, fetch, eval.<br>Receives <code>input</code> (string) and <code>data</code> (parsed JSON).</span>
      </label>
    `;
  }

  // ── Output target ────────────────────────────────────────────────────────

  if (node.kind === 'output') {
    html += `
      <label class="flow-panel-field">
        <span>Output Target</span>
        <select class="flow-panel-select" data-config="outputTarget">
          ${['chat', 'log', 'store'].map((t) => `<option value="${t}"${outputTarget === t ? ' selected' : ''}>${t}</option>`).join('')}
        </select>
      </label>
    `;
  }

  // ── HTTP request ─────────────────────────────────────────────────────────

  if (node.kind === 'http') {
    const httpMethod = (config.httpMethod as string) ?? 'GET';
    const httpUrl = escAttr((config.httpUrl as string) ?? '');
    const httpHeaders = escAttr((config.httpHeaders as string) ?? '');
    const httpBody = escAttr((config.httpBody as string) ?? '');
    html += `
      <label class="flow-panel-field">
        <span>Method</span>
        <select class="flow-panel-select" data-config="httpMethod">
          ${['GET', 'POST', 'PUT', 'PATCH', 'DELETE'].map((m) => `<option value="${m}"${httpMethod === m ? ' selected' : ''}>${m}</option>`).join('')}
        </select>
      </label>
      <label class="flow-panel-field">
        <span>URL</span>
        <input type="text" class="flow-panel-input" data-config="httpUrl" value="${httpUrl}" placeholder="https://api.example.com/endpoint" />
      </label>
      <label class="flow-panel-field">
        <span>Headers (JSON)</span>
        <textarea class="flow-panel-textarea" data-config="httpHeaders" rows="2" placeholder='{"Content-Type": "application/json"}'>${httpHeaders}</textarea>
      </label>
      <label class="flow-panel-field">
        <span>Body</span>
        <textarea class="flow-panel-textarea" data-config="httpBody" rows="3" placeholder="Request body — use {{input}} for upstream output">${httpBody}</textarea>
      </label>
      <span class="flow-panel-hint">Use <code>{{input}}</code> in URL, headers, or body to inject upstream output.</span>
    `;
  }

  // ── Credential binding (HTTP / MCP-tool) ─────────────────────────────────

  if (node.kind === 'http' || node.kind === 'mcp-tool') {
    const credName = escAttr((config.credentialName as string) ?? '');
    const credType = (config.credentialType as string) ?? 'bearer';
    html += `
      <div class="flow-panel-retry-config" style="margin-top: 8px">
        <div class="flow-panel-retry-header">
          <span class="ms" style="font-size:14px;color:var(--kinetic-gold)">key</span>
          <span>Credential</span>
        </div>
        <label class="flow-panel-field">
          <span>Credential Name</span>
          <input type="text" class="flow-panel-input" data-config="credentialName" value="${credName}" placeholder="e.g. openai-key, github-token" />
        </label>
        <label class="flow-panel-field">
          <span>Type</span>
          <select class="flow-panel-input" data-config="credentialType">
            <option value="bearer"${credType === 'bearer' ? ' selected' : ''}>Bearer Token</option>
            <option value="api-key"${credType === 'api-key' ? ' selected' : ''}>API Key (header)</option>
            <option value="basic"${credType === 'basic' ? ' selected' : ''}>Basic Auth</option>
            <option value="oauth2"${credType === 'oauth2' ? ' selected' : ''}>OAuth2</option>
          </select>
        </label>
        <span class="flow-panel-hint">Or use <code>{{vault.name}}</code> directly in headers/args.</span>
      </div>
    `;
  }

  // ── MCP-tool ─────────────────────────────────────────────────────────────

  if (node.kind === 'mcp-tool') {
    const mcpToolName = escAttr((config.mcpToolName as string) ?? '');
    const mcpToolArgs = escAttr((config.mcpToolArgs as string) ?? '');
    html += `
      <label class="flow-panel-field">
        <span>Tool Name</span>
        <input type="text" class="flow-panel-input" data-config="mcpToolName" value="${mcpToolName}" placeholder="e.g. search_web, read_file" />
      </label>
      <label class="flow-panel-field">
        <span>Arguments (JSON)</span>
        <textarea class="flow-panel-textarea" data-config="mcpToolArgs" rows="3" placeholder='{"query": "{{input}}"}'>${mcpToolArgs}</textarea>
      </label>
      <span class="flow-panel-hint">Use <code>{{input}}</code> in arguments to inject upstream output.</span>
    `;
  }

  // ── Loop / Iteration ─────────────────────────────────────────────────────

  if (node.kind === ('loop' as typeof node.kind)) {
    const loopOver = escAttr((config.loopOver as string) ?? '');
    const loopVar = escAttr((config.loopVar as string) ?? 'item');
    const loopMaxIter = (config.loopMaxIterations as number) ?? 100;
    html += `
      <div class="flow-panel-loop-config">
        <div class="flow-panel-retry-header">
          <span class="ms" style="font-size:14px;color:var(--kinetic-gold)">repeat</span>
          <span>Loop / Iteration</span>
        </div>
        <label class="flow-panel-field">
          <span>Loop Over</span>
          <input type="text" class="flow-panel-input" data-config="loopOver" value="${loopOver}" placeholder="e.g. data.items, results" />
        </label>
        <label class="flow-panel-field">
          <span>Item Variable</span>
          <input type="text" class="flow-panel-input" data-config="loopVar" value="${loopVar}" placeholder="item" />
        </label>
        <label class="flow-panel-field">
          <span>Max Iterations</span>
          <input type="number" class="flow-panel-input" data-config="loopMaxIterations" value="${loopMaxIter}" min="1" max="1000" step="1" />
        </label>
        <span class="flow-panel-hint">Use <code>{{loop.index}}</code> and <code>{{loop.item}}</code> in downstream prompts.</span>
      </div>
    `;
  }

  // ── Group / Sub-flow ─────────────────────────────────────────────────────

  if (node.kind === 'group') {
    const subFlowId = escAttr((config.subFlowId as string) ?? '');
    html += `
      <div class="flow-panel-retry-config" style="margin-top: 8px">
        <div class="flow-panel-retry-header">
          <span class="ms" style="font-size:14px;color:var(--kinetic-purple, #A855F7)">account_tree</span>
          <span>Sub-flow</span>
        </div>
        <label class="flow-panel-field">
          <span>Sub-flow ID</span>
          <input type="text" class="flow-panel-input" data-config="subFlowId" value="${subFlowId}" placeholder="Paste flow ID to execute" />
        </label>
        <span class="flow-panel-hint">The selected flow will be executed with upstream input. Max 5 levels of nesting.</span>
      </div>
    `;
  }

  // ── Squad ────────────────────────────────────────────────────────────────

  if (node.kind === ('squad' as typeof node.kind)) {
    const squadId = escAttr((config.squadId as string) ?? '');
    const squadObj = escAttr((config.squadObjective as string) ?? '');
    const squadTimeout = ((config.squadTimeoutMs as number) ?? 300000) / 1000;
    const squadRounds = (config.squadMaxRounds as number) ?? 5;
    html += `
      <div class="flow-panel-retry-config" style="margin-top: 8px">
        <div class="flow-panel-retry-header">
          <span class="ms" style="font-size:14px;color:var(--kinetic-purple, #A855F7)">groups</span>
          <span>Squad</span>
        </div>
        <label class="flow-panel-field">
          <span>Squad ID</span>
          <input type="text" class="flow-panel-input" data-config="squadId" value="${squadId}" placeholder="Select or enter squad ID" />
        </label>
        <label class="flow-panel-field">
          <span>Objective</span>
          <textarea class="flow-panel-textarea" data-config="squadObjective" rows="2" placeholder="Task or goal for the squad (uses upstream input if empty)">${squadObj}</textarea>
        </label>
        <label class="flow-panel-field">
          <span>Timeout (s)</span>
          <input type="number" class="flow-panel-input" data-config="squadTimeoutMs" value="${squadTimeout}" min="10" max="600" step="10" />
        </label>
        <label class="flow-panel-field">
          <span>Max Rounds</span>
          <input type="number" class="flow-panel-input" data-config="squadMaxRounds" value="${squadRounds}" min="1" max="20" step="1" />
        </label>
        <span class="flow-panel-hint">The squad will discuss and converge on a result within the round limit.</span>
      </div>
    `;
  }

  // ── Memory Write ─────────────────────────────────────────────────────────

  if (node.kind === ('memory' as typeof node.kind)) {
    const memSrc = (config.memorySource as string) ?? 'output';
    const memContent = escAttr((config.memoryContent as string) ?? '');
    const memCat = (config.memoryCategory as string) ?? 'insight';
    const memImp = (config.memoryImportance as number) ?? 0.5;
    const memAgent = escAttr((config.memoryAgentId as string) ?? '');
    const categories = [
      'insight',
      'fact',
      'preference',
      'summary',
      'conversation',
      'task_result',
      'error_log',
      'custom',
    ];
    html += `
      <div class="flow-panel-retry-config" style="margin-top: 8px">
        <div class="flow-panel-retry-header">
          <span class="ms" style="font-size:14px;color:var(--kinetic-sage, #6B8E6B)">save</span>
          <span>Memory Write</span>
        </div>
        <label class="flow-panel-field">
          <span>Content Source</span>
          <select class="flow-panel-select" data-config="memorySource">
            <option value="output"${memSrc === 'output' ? ' selected' : ''}>Node Output</option>
            <option value="custom"${memSrc === 'custom' ? ' selected' : ''}>Custom Text</option>
          </select>
        </label>
        <label class="flow-panel-field">
          <span>Custom Content</span>
          <textarea class="flow-panel-textarea" data-config="memoryContent" rows="2" placeholder="Custom content to store (when source is Custom)">${memContent}</textarea>
        </label>
        <label class="flow-panel-field">
          <span>Category</span>
          <select class="flow-panel-select" data-config="memoryCategory">
            ${categories.map((c) => `<option value="${c}"${memCat === c ? ' selected' : ''}>${c.replace('_', ' ')}</option>`).join('')}
          </select>
        </label>
        <label class="flow-panel-field">
          <span>Importance (0–1)</span>
          <input type="number" class="flow-panel-input" data-config="memoryImportance" value="${memImp}" min="0" max="1" step="0.1" />
        </label>
        <label class="flow-panel-field">
          <span>Agent ID (scope)</span>
          <input type="text" class="flow-panel-input" data-config="memoryAgentId" value="${memAgent}" placeholder="Optional — scopes memory to agent" />
        </label>
        <span class="flow-panel-hint">Stores information in long-term memory for future recall.</span>
      </div>
    `;
  }

  // ── Memory Recall ────────────────────────────────────────────────────────

  if (node.kind === ('memory-recall' as typeof node.kind)) {
    const mqSrc = (config.memoryQuerySource as string) ?? 'input';
    const mqQuery = escAttr((config.memoryQuery as string) ?? '');
    const mqLimit = (config.memoryLimit as number) ?? 5;
    const mqThreshold = (config.memoryThreshold as number) ?? 0.3;
    const mqFormat = (config.memoryOutputFormat as string) ?? 'text';
    const mqAgent = escAttr((config.memoryAgentId as string) ?? '');
    html += `
      <div class="flow-panel-retry-config" style="margin-top: 8px">
        <div class="flow-panel-retry-header">
          <span class="ms" style="font-size:14px;color:var(--kinetic-gold, #DAA520)">manage_search</span>
          <span>Memory Recall</span>
        </div>
        <label class="flow-panel-field">
          <span>Query Source</span>
          <select class="flow-panel-select" data-config="memoryQuerySource">
            <option value="input"${mqSrc === 'input' ? ' selected' : ''}>Upstream Input</option>
            <option value="custom"${mqSrc === 'custom' ? ' selected' : ''}>Custom Query</option>
          </select>
        </label>
        <label class="flow-panel-field">
          <span>Custom Query</span>
          <input type="text" class="flow-panel-input" data-config="memoryQuery" value="${mqQuery}" placeholder="Search query (when source is Custom)" />
        </label>
        <label class="flow-panel-field">
          <span>Max Results</span>
          <input type="number" class="flow-panel-input" data-config="memoryLimit" value="${mqLimit}" min="1" max="50" step="1" />
        </label>
        <label class="flow-panel-field">
          <span>Min Relevance (0–1)</span>
          <input type="number" class="flow-panel-input" data-config="memoryThreshold" value="${mqThreshold}" min="0" max="1" step="0.05" />
        </label>
        <label class="flow-panel-field">
          <span>Output Format</span>
          <select class="flow-panel-select" data-config="memoryOutputFormat">
            <option value="text"${mqFormat === 'text' ? ' selected' : ''}>Text (numbered list)</option>
            <option value="json"${mqFormat === 'json' ? ' selected' : ''}>JSON (array)</option>
          </select>
        </label>
        <label class="flow-panel-field">
          <span>Agent ID (scope)</span>
          <input type="text" class="flow-panel-input" data-config="memoryAgentId" value="${mqAgent}" placeholder="Optional — scopes search to agent" />
        </label>
        <span class="flow-panel-hint">Searches long-term memory and provides results to downstream nodes.</span>
      </div>
    `;
  }

  // ── Variable assignment (all node kinds) ─────────────────────────────────

  {
    const setVarKey = escAttr((config.setVariableKey as string) ?? '');
    const setVarVal = escAttr((config.setVariable as string) ?? '');
    html += `
      <div class="flow-panel-retry-config" style="margin-top: 8px">
        <div class="flow-panel-retry-header">
          <span class="ms" style="font-size:14px">data_object</span>
          <span>Set Variable</span>
        </div>
        <label class="flow-panel-field">
          <span>Variable Name</span>
          <input type="text" class="flow-panel-input" data-config="setVariableKey" value="${setVarKey}" placeholder="e.g. summary, lastResult" />
        </label>
        <label class="flow-panel-field">
          <span>Value Expression</span>
          <input type="text" class="flow-panel-input" data-config="setVariable" value="${setVarVal}" placeholder="Leave empty to use node output" />
        </label>
        <span class="flow-panel-hint">Access via <code>{{flow.name}}</code> in downstream prompts.</span>
      </div>
    `;
  }

  // ── Error handler ────────────────────────────────────────────────────────

  if (node.kind === 'error') {
    const errorTargets = (config.errorTargets as string[]) ?? ['log'];
    html += `
      <div class="flow-panel-error-config">
        <div class="flow-panel-error-header">
          <span class="ms" style="font-size:16px;color:var(--kinetic-red)">error</span>
          <span>Error Handler</span>
        </div>
        <label class="flow-panel-field">
          <span>Notify via</span>
          <div class="flow-panel-error-targets">
            ${['log', 'toast', 'chat']
              .map(
                (t) => `
              <label class="flow-panel-error-target">
                <input type="checkbox" data-error-target="${t}" ${errorTargets.includes(t) ? 'checked' : ''} />
                <span>${t === 'log' ? 'Console Log' : t === 'toast' ? 'Toast Alert' : 'Chat Message'}</span>
              </label>
            `,
              )
              .join('')}
          </div>
        </label>
        <label class="flow-panel-field">
          <span>Custom Message</span>
          <textarea class="flow-panel-textarea" data-config="prompt" rows="2" placeholder="Optional error message template…">${promptVal}</textarea>
        </label>
      </div>
    `;
  }

  // ── Retry config ─────────────────────────────────────────────────────────

  if (
    [
      'agent',
      'tool',
      'data',
      'code',
      'http',
      'mcp-tool',
      'loop',
      'squad',
      'memory',
      'memory-recall',
    ].includes(node.kind)
  ) {
    const maxRetries = (config.maxRetries as number) ?? 0;
    const retryDelay = (config.retryDelayMs as number) ?? 1000;
    const retryBackoff = (config.retryBackoff as number) ?? 2;
    html += `
      <div class="flow-panel-retry-config">
        <div class="flow-panel-retry-header">
          <span class="ms" style="font-size:14px">replay</span>
          <span>Retry on Error</span>
        </div>
        <label class="flow-panel-field">
          <span>Max Retries</span>
          <input type="number" class="flow-panel-input" data-config="maxRetries" value="${maxRetries}" min="0" max="10" step="1" />
        </label>
        <label class="flow-panel-field">
          <span>Delay (ms)</span>
          <input type="number" class="flow-panel-input" data-config="retryDelayMs" value="${retryDelay}" min="100" max="60000" step="100" />
        </label>
        <label class="flow-panel-field">
          <span>Backoff ×</span>
          <input type="number" class="flow-panel-input" data-config="retryBackoff" value="${retryBackoff}" min="1" max="10" step="0.5" />
        </label>
      </div>
    `;
  }

  // ── Timeout ──────────────────────────────────────────────────────────────

  if (
    [
      'agent',
      'tool',
      'condition',
      'data',
      'code',
      'http',
      'mcp-tool',
      'loop',
      'squad',
      'memory',
      'memory-recall',
    ].includes(node.kind)
  ) {
    html += `
      <label class="flow-panel-field">
        <span>Timeout (s)</span>
        <input type="number" class="flow-panel-input" data-config="timeoutMs" value="${timeoutVal / 1000}" min="5" max="600" step="5" />
      </label>
    `;
  }

  return html;
}
