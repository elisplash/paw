// ─────────────────────────────────────────────────────────────────────────────
// Flow Visualization Engine — Panel Molecules
// Node properties panel, edge panel, and debug inspector rendering.
// ─────────────────────────────────────────────────────────────────────────────

import {
  type FlowGraph,
  type FlowNode,
  NODE_DEFAULTS,
} from './atoms';
import {
  getMoleculesState,
  getAvailableAgents,
  getSelectedEdgeIdLocal,
  setSelectedEdgeIdLocal,
  getDebugNodeStates,
  escAttr,
  formatDate,
} from './molecule-state';
import { renderGraph } from './canvas-molecules';
import { CRON_PRESETS, validateCron, describeCron, nextCronFire } from './cron-atoms';

// ── Node Properties Panel ──────────────────────────────────────────────────

export function renderNodePanel(
  container: HTMLElement,
  node: FlowNode | null,
  onUpdate: (patch: Partial<FlowNode>) => void,
  activeGraph?: FlowGraph | null,
  onGraphUpdate?: (patch: Partial<FlowGraph>) => void,
) {
  const _state = getMoleculesState();
  const _selectedEdgeId = getSelectedEdgeIdLocal();

  if (!node) {
    // Show flow-level properties if a graph is active
    if (activeGraph && onGraphUpdate) {
      const folderVal = escAttr(activeGraph.folder ?? '');
      const descVal = escAttr(activeGraph.description ?? '');

      container.innerHTML = `
        <div class="flow-panel">
          <div class="flow-panel-header">
            <span class="ms" style="color: var(--kinetic-red, #FF4D4D)">account_tree</span>
            <div>
              <div class="flow-panel-kind">FLOW PROPERTIES</div>
            </div>
          </div>
          <label class="flow-panel-field">
            <span>Name</span>
            <input type="text" class="flow-panel-input" data-flow-field="name" value="${escAttr(activeGraph.name)}" />
          </label>
          <label class="flow-panel-field">
            <span>Description</span>
            <textarea class="flow-panel-textarea" data-flow-field="description" rows="2" placeholder="Describe this flow…">${descVal}</textarea>
          </label>
          <label class="flow-panel-field">
            <span>Folder</span>
            <input type="text" class="flow-panel-input" data-flow-field="folder" value="${folderVal}" placeholder="(root)" />
          </label>
          <div class="flow-panel-section">
            <div class="flow-panel-section-label">Stats</div>
            <div class="flow-panel-pos">
              <span>${activeGraph.nodes.length} integrations</span>
              <span>${activeGraph.edges.length} edges</span>
            </div>
          </div>
          <div class="flow-panel-section">
            <div class="flow-panel-pos">
              <span>Created: ${formatDate(activeGraph.createdAt)}</span>
            </div>
            <div class="flow-panel-pos" style="margin-top: 2px">
              <span>Updated: ${formatDate(activeGraph.updatedAt)}</span>
            </div>
          </div>
        </div>
      `;

      container.querySelectorAll('[data-flow-field]').forEach((el) => {
        el.addEventListener('change', () => {
          const field = (el as HTMLElement).dataset.flowField!;
          const value = (el as HTMLInputElement).value;
          onGraphUpdate({ [field]: value } as Partial<FlowGraph>);
        });
      });
      return;
    }

    // Edge panel when an edge is selected
    if (_selectedEdgeId && activeGraph) {
      const edge = activeGraph.edges.find((ee) => ee.id === _selectedEdgeId);
      if (edge && onGraphUpdate) {
        const edgeLabel = escAttr(edge.label ?? '');
        const fromNode = activeGraph.nodes.find((n) => n.id === edge.from);
        const toNode = activeGraph.nodes.find((n) => n.id === edge.to);
        container.innerHTML = `
          <div class="flow-panel">
            <div class="flow-panel-header">
              <span class="ms" style="color: var(--accent)">arrow_forward</span>
              <div>
                <div class="flow-panel-kind">EDGE</div>
                <div class="flow-panel-label">${fromNode?.label ?? edge.from} → ${toNode?.label ?? edge.to}</div>
              </div>
            </div>
            <label class="flow-panel-field">
              <span>Kind</span>
              <select class="flow-panel-input" data-edge-field="kind">
                <option value="forward"${edge.kind === 'forward' ? ' selected' : ''}>Forward</option>
                <option value="reverse"${edge.kind === 'reverse' ? ' selected' : ''}>Reverse</option>
                <option value="bidirectional"${edge.kind === 'bidirectional' ? ' selected' : ''}>Bidirectional</option>
                <option value="error"${edge.kind === 'error' ? ' selected' : ''}>Error</option>
              </select>
            </label>
            <label class="flow-panel-field">
              <span>Label</span>
              <input type="text" class="flow-panel-input" data-edge-field="label" value="${edgeLabel}" placeholder="Optional label…" />
            </label>
            <label class="flow-panel-field">
              <span>Condition</span>
              <input type="text" class="flow-panel-input" data-edge-field="conditionExpr" value="${escAttr((edge as unknown as Record<string, unknown>).conditionExpr as string ?? '')}" placeholder="Expression for conditional routing" />
            </label>
            <div class="flow-panel-section" style="margin-top: 12px">
              <button class="flow-btn flow-btn-danger" data-edge-action="delete" style="width:100%">
                <span class="ms" style="font-size:14px">delete</span> Delete Edge
              </button>
            </div>
          </div>
        `;

        container.querySelectorAll('[data-edge-field]').forEach((el) => {
          el.addEventListener('change', () => {
            const field = (el as HTMLElement).dataset.edgeField!;
            const value = (el as HTMLInputElement).value;
            (edge as unknown as Record<string, unknown>)[field] = value;
            activeGraph.updatedAt = new Date().toISOString();
            onGraphUpdate({} as Partial<FlowGraph>);
            renderGraph();
          });
        });

        const deleteBtn = container.querySelector('[data-edge-action="delete"]');
        deleteBtn?.addEventListener('click', () => {
          activeGraph.edges = activeGraph.edges.filter((ee) => ee.id !== _selectedEdgeId);
          setSelectedEdgeIdLocal(null);
          _state?.setSelectedEdgeId(null);
          activeGraph.updatedAt = new Date().toISOString();
          onGraphUpdate({} as Partial<FlowGraph>);
          renderGraph();
          renderNodePanel(container, null, onUpdate, activeGraph, onGraphUpdate);
        });
        return;
      }
    }

    container.innerHTML =
      '<div class="flow-panel-empty"><span class="ms">touch_app</span><p>Select a node or edge to edit</p></div>';
    return;
  }

  const defaults = NODE_DEFAULTS[node.kind];
  const config = node.config ?? {};
  const promptVal = escAttr((config.prompt as string) ?? '');
  const modelVal = escAttr((config.model as string) ?? '');
  const conditionVal = escAttr((config.conditionExpr as string) ?? '');
  const transformVal = escAttr((config.transform as string) ?? '');
  const codeVal = escAttr((config.code as string) ?? '');
  const outputTarget = (config.outputTarget as string) ?? 'chat';
  const timeoutVal = (config.timeoutMs as number) ?? 120000;
  const _availableAgents = getAvailableAgents();

  let configFieldsHtml = '';

  if (
    node.kind === 'agent' ||
    node.kind === 'tool' ||
    node.kind === 'data' ||
    node.kind === 'trigger'
  ) {
    configFieldsHtml += `
      <label class="flow-panel-field">
        <span>Prompt</span>
        <textarea class="flow-panel-textarea" data-config="prompt" rows="3" placeholder="Instructions for this step…">${promptVal}</textarea>
      </label>
    `;
  }

  // Schedule section for trigger nodes
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

    configFieldsHtml += `
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

    configFieldsHtml += `
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

  if (node.kind === 'condition') {
    configFieldsHtml += `
      <label class="flow-panel-field">
        <span>Condition</span>
        <textarea class="flow-panel-textarea" data-config="conditionExpr" rows="2" placeholder="e.g. Does the input contain valid data?">${conditionVal}</textarea>
      </label>
    `;
  }

  if (node.kind === 'data') {
    configFieldsHtml += `
      <label class="flow-panel-field">
        <span>Transform</span>
        <textarea class="flow-panel-textarea" data-config="transform" rows="2" placeholder="e.g. Extract the top 3 results">${transformVal}</textarea>
      </label>
    `;
  }

  if (node.kind === 'code') {
    configFieldsHtml += `
      <label class="flow-panel-field">
        <span>JavaScript Code</span>
        <textarea class="flow-panel-textarea flow-panel-code" data-config="code" rows="8" placeholder="// Input available as: input (string), data (parsed JSON)
// Return a value or use console.log()
return input.toUpperCase();">${codeVal}</textarea>
        <span class="flow-panel-hint">Sandboxed: no window, document, fetch, eval.<br>Receives <code>input</code> (string) and <code>data</code> (parsed JSON).</span>
      </label>
    `;
  }

  if (node.kind === 'output') {
    configFieldsHtml += `
      <label class="flow-panel-field">
        <span>Output Target</span>
        <select class="flow-panel-select" data-config="outputTarget">
          ${['chat', 'log', 'store'].map((t) => `<option value="${t}"${outputTarget === t ? ' selected' : ''}>${t}</option>`).join('')}
        </select>
      </label>
    `;
  }

  if (node.kind === 'http') {
    const httpMethod = (config.httpMethod as string) ?? 'GET';
    const httpUrl = escAttr((config.httpUrl as string) ?? '');
    const httpHeaders = escAttr((config.httpHeaders as string) ?? '');
    const httpBody = escAttr((config.httpBody as string) ?? '');
    configFieldsHtml += `
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

  // Credential binding for HTTP and MCP-tool nodes
  if (node.kind === 'http' || node.kind === 'mcp-tool') {
    const credName = escAttr((config.credentialName as string) ?? '');
    const credType = (config.credentialType as string) ?? 'bearer';
    configFieldsHtml += `
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

  if (node.kind === 'mcp-tool') {
    const mcpToolName = escAttr((config.mcpToolName as string) ?? '');
    const mcpToolArgs = escAttr((config.mcpToolArgs as string) ?? '');
    configFieldsHtml += `
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

  if (node.kind === ('loop' as typeof node.kind)) {
    const loopOver = escAttr((config.loopOver as string) ?? '');
    const loopVar = escAttr((config.loopVar as string) ?? 'item');
    const loopMaxIter = (config.loopMaxIterations as number) ?? 100;
    configFieldsHtml += `
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

  // Group / sub-flow config
  if (node.kind === 'group') {
    const subFlowId = escAttr((config.subFlowId as string) ?? '');
    configFieldsHtml += `
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

  // Variable assignment — available on all node kinds
  {
    const setVarKey = escAttr((config.setVariableKey as string) ?? '');
    const setVarVal = escAttr((config.setVariable as string) ?? '');
    configFieldsHtml += `
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

  // Error node config
  if (node.kind === 'error') {
    const errorTargets = (config.errorTargets as string[]) ?? ['log'];
    configFieldsHtml += `
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

  // Retry config
  if (['agent', 'tool', 'data', 'code', 'http', 'mcp-tool', 'loop'].includes(node.kind)) {
    const maxRetries = (config.maxRetries as number) ?? 0;
    const retryDelay = (config.retryDelayMs as number) ?? 1000;
    const retryBackoff = (config.retryBackoff as number) ?? 2;
    configFieldsHtml += `
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

  // Timeout field
  if (['agent', 'tool', 'condition', 'data', 'code', 'http', 'mcp-tool', 'loop'].includes(node.kind)) {
    configFieldsHtml += `
      <label class="flow-panel-field">
        <span>Timeout (s)</span>
        <input type="number" class="flow-panel-input" data-config="timeoutMs" value="${timeoutVal / 1000}" min="5" max="600" step="5" />
      </label>
    `;
  }

  // Debug output inspector
  const debugState = getDebugNodeStates().get(node.id);
  const debugHtml = debugState
    ? `
    <div class="flow-panel-divider"></div>
    <div class="flow-panel-section">
      <span class="flow-panel-section-label">Debug Inspector</span>
      <div class="flow-panel-debug-status">
        <span class="flow-debug-badge flow-debug-badge-${debugState.status}">${debugState.status.toUpperCase()}</span>
      </div>
      ${
        debugState.input
          ? `
        <div class="flow-panel-debug-block">
          <span class="flow-panel-debug-label">Input</span>
          <pre class="flow-panel-debug-pre">${escAttr(debugState.input)}</pre>
        </div>
      `
          : ''
      }
      ${
        debugState.output
          ? `
        <div class="flow-panel-debug-block">
          <span class="flow-panel-debug-label">Output</span>
          <pre class="flow-panel-debug-pre">${escAttr(debugState.output)}</pre>
        </div>
      `
          : ''
      }
    </div>
  `
    : '';

  container.innerHTML = `
    <div class="flow-panel">
      <div class="flow-panel-header">
        <span class="ms" style="color:${defaults.color}">${defaults.icon}</span>
        <span class="flow-panel-kind">${node.kind.toUpperCase()}</span>
      </div>
      <label class="flow-panel-field">
        <span>Label</span>
        <input type="text" class="flow-panel-input" data-field="label" value="${escAttr(node.label)}" />
      </label>
      <label class="flow-panel-field">
        <span>Description</span>
        <input type="text" class="flow-panel-input" data-field="description" value="${escAttr(node.description ?? '')}" />
      </label>
      ${configFieldsHtml ? `<div class="flow-panel-divider"></div><div class="flow-panel-section"><span class="flow-panel-section-label">Execution Config</span></div>${configFieldsHtml}` : ''}
      <div class="flow-panel-divider"></div>
      <div class="flow-panel-section">
        <span class="flow-panel-section-label">Info</span>
        <div class="flow-panel-pos">
          <span>Status: <strong>${node.status}</strong></span>
          <span>x: ${node.x}  y: ${node.y}</span>
          <span>${node.width}×${node.height}</span>
        </div>
      </div>
      ${debugHtml}
    </div>
  `;

  // Bind direct node fields
  container.querySelectorAll('[data-field]').forEach((el) => {
    el.addEventListener('change', () => {
      const field = (el as HTMLElement).dataset.field!;
      const value = (el as HTMLInputElement).value;
      onUpdate({ [field]: value } as Partial<FlowNode>);
    });
  });

  // Bind config fields
  container.querySelectorAll('[data-config]').forEach((el) => {
    el.addEventListener('change', () => {
      const key = (el as HTMLElement).dataset.config!;
      let value: unknown = (el as HTMLInputElement).value;
      if (key === 'timeoutMs') value = Number(value) * 1000;
      if ((el as HTMLInputElement).type === 'checkbox') value = (el as HTMLInputElement).checked;
      const newConfig = { ...node.config, [key]: value };
      onUpdate({ config: newConfig });
    });
  });

  // Schedule preset dropdown
  const presetSelect = container.querySelector(
    '[data-schedule-preset]',
  ) as HTMLSelectElement | null;
  if (presetSelect) {
    presetSelect.addEventListener('change', () => {
      const val = presetSelect.value;
      if (!val) return;
      const cronInput = container.querySelector(
        '[data-config="schedule"]',
      ) as HTMLInputElement | null;
      if (cronInput) {
        cronInput.value = val;
        cronInput.dispatchEvent(new Event('change'));
      }
    });
  }

  // Error target checkboxes
  container.querySelectorAll('[data-error-target]').forEach((el) => {
    el.addEventListener('change', () => {
      const targets: string[] = [];
      container.querySelectorAll('[data-error-target]').forEach((cb) => {
        if ((cb as HTMLInputElement).checked) {
          targets.push((cb as HTMLElement).dataset.errorTarget!);
        }
      });
      const newConfig = { ...node.config, errorTargets: targets };
      onUpdate({ config: newConfig });
    });
  });
}
