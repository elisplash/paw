// ─────────────────────────────────────────────────────────────────────────────
// Flow Execution Engine — Variable Resolution Atoms
// Template variable resolution and loop array parsing.
// No DOM, no IPC — fully testable.
// ─────────────────────────────────────────────────────────────────────────────

// ── Variable Resolution ────────────────────────────────────────────────────

/**
 * Resolve `{{flow.key}}`, `{{env.KEY}}`, `{{input}}`, and `{{loop.index}}` / `{{loop.item}}`
 * template variables in a string.
 */
export function resolveVariables(
  template: string,
  context: {
    input?: string;
    variables?: Record<string, unknown>;
    loopIndex?: number;
    loopItem?: unknown;
    loopVar?: string;
    nodeOutputs?: Map<string, string>;
    vaultCredentials?: Record<string, string>;
  },
): string {
  if (!template) return template;

  let result = template;

  // {{input}} — upstream output
  if (context.input !== undefined) {
    result = result.replace(/\{\{input\}\}/g, context.input);
  }

  // {{flow.key}} — flow-level variables
  if (context.variables) {
    result = result.replace(/\{\{flow\.(\w+)\}\}/g, (_match, key: string) => {
      const val = context.variables![key];
      if (val === undefined) return `{{flow.${key}}}`;
      return typeof val === 'string' ? val : JSON.stringify(val);
    });
  }

  // {{env.KEY}} — environment variables (best-effort, limited in browser)
  result = result.replace(/\{\{env\.(\w+)\}\}/g, (_match, key: string) => {
    // In Tauri, env vars could be passed from Rust; in browser, check globalThis
    return `{{env.${key}}}`;
  });

  // {{vault.NAME}} — pre-loaded vault credentials (decrypted at run-start)
  if (context.vaultCredentials) {
    result = result.replace(/\{\{vault\.(\w[\w.-]*)\}\}/g, (_match, name: string) => {
      const val = context.vaultCredentials![name];
      if (val === undefined) return `{{vault.${name}}}`;
      return val;
    });
  }

  // {{loop.index}}, {{loop.item}}, {{loop.<var>}} — loop context
  if (context.loopIndex !== undefined) {
    result = result.replace(/\{\{loop\.index\}\}/g, String(context.loopIndex));
  }
  if (context.loopItem !== undefined) {
    const itemStr =
      typeof context.loopItem === 'string' ? context.loopItem : JSON.stringify(context.loopItem);
    result = result.replace(/\{\{loop\.item\}\}/g, itemStr);
    // Also support custom loop variable name
    if (context.loopVar && context.loopVar !== 'item') {
      result = result.replace(new RegExp(`\\{\\{loop\\.${context.loopVar}\\}\\}`, 'g'), itemStr);
    }
  }

  // {{nodeLabel.output}} — access specific node outputs by label
  if (context.nodeOutputs) {
    result = result.replace(/\{\{(\w[\w\s-]*)\.output\}\}/g, (_match, _label: string) => {
      // nodeOutputs is keyed by nodeId — label mapping would require graph context
      // For now, return the template as-is (resolved at execution time)
      return _match;
    });
  }

  return result;
}

// ── Loop Array Parsing ─────────────────────────────────────────────────────

/**
 * Parse a `loopOver` expression to extract an array from upstream data.
 * Supports: direct JSON array, dot-path access (e.g. "data.items"),
 * or newline-separated text.
 */
export function parseLoopArray(input: string, loopOver?: string): unknown[] {
  if (!input) return [];

  // Try parsing input as JSON first
  let data: unknown;
  try {
    data = JSON.parse(input);
  } catch {
    // Not JSON — treat as newline-separated text
    if (!loopOver) {
      return input.split('\n').filter((line) => line.trim());
    }
    data = input;
  }

  // If no loopOver expression, use data directly
  if (!loopOver || loopOver.trim() === '') {
    return Array.isArray(data) ? data : [data];
  }

  // Dot-path access: "items", "data.results", "response.data.list"
  const parts = loopOver.trim().split('.');
  let current: unknown = data;
  for (const part of parts) {
    if (current == null || typeof current !== 'object') return [data];
    current = (current as Record<string, unknown>)[part];
  }

  return Array.isArray(current) ? current : current != null ? [current] : [];
}
