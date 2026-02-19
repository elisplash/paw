// Shared helper functions

export const $ = (id: string) => document.getElementById(id);

export function escHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

// ── Model Picker Helpers ──────────────────────────────────────────────

/** Well-known models grouped by provider kind */
const POPULAR_MODELS: Record<string, string[]> = {
  ollama: [
    'llama3.2:3b', 'llama3.2:1b', 'llama3.1:8b', 'llama3.1:70b', 'llama3.3:70b',
    'mistral:7b', 'mixtral:8x7b', 'codellama:13b', 'codellama:34b',
    'deepseek-coder:6.7b', 'deepseek-coder-v2:16b',
    'phi3:mini', 'phi3:medium', 'qwen2.5:7b', 'qwen2.5:32b', 'qwen2.5:72b',
    'gemma2:9b', 'gemma2:27b', 'command-r:35b',
  ],
  openai: [
    'gpt-4o', 'gpt-4o-mini', 'gpt-4.1', 'gpt-4.1-mini', 'gpt-4.1-nano',
    'o1', 'o1-mini', 'o3', 'o3-mini', 'o4-mini',
  ],
  anthropic: [
    'claude-opus-4-6', 'claude-sonnet-4-6', 'claude-haiku-4-5-20251001',
    'claude-sonnet-4-5-20250929',
  ],
  google: [
    'gemini-2.5-pro', 'gemini-2.5-flash',
    'gemini-2.0-flash', 'gemini-2.0-flash-lite',
    'gemini-1.5-pro', 'gemini-1.5-flash', 'gemini-1.5-flash-8b',
  ],
  openrouter: [
    'anthropic/claude-sonnet-4-6', 'anthropic/claude-haiku-4-5-20251001',
    'openai/gpt-4o', 'openai/gpt-4o-mini',
    'google/gemini-2.5-pro', 'google/gemini-2.5-flash',
    'meta-llama/llama-3.1-405b-instruct', 'meta-llama/llama-3.1-70b-instruct',
    'deepseek/deepseek-chat', 'deepseek/deepseek-r1',
    'mistralai/mistral-large', 'qwen/qwen-2.5-72b-instruct',
  ],
  custom: ['deepseek-chat', 'deepseek-reasoner'],
};

const KIND_LABELS: Record<string, string> = {
  ollama: '🦙 Ollama', openai: '🤖 OpenAI', anthropic: '🧠 Anthropic',
  google: '🔮 Google', openrouter: '🌐 OpenRouter', custom: '🔧 Custom',
};

interface ProviderInfo {
  id: string;
  kind: string;
  default_model?: string;
}

/**
 * Populate a <select> element with model options grouped by provider.
 * @param select  The <select> element to populate
 * @param providers  Array of configured providers
 * @param options  Configuration options
 */
export function populateModelSelect(
  select: HTMLSelectElement,
  providers: ProviderInfo[],
  options: {
    /** Text for the first option (empty value). If null, no default option is added. */
    defaultLabel?: string | null;
    /** Currently selected model value */
    currentValue?: string;
    /** Whether to include the current default model info in the default label */
    showDefaultModel?: string;
  } = {},
): void {
  const { defaultLabel = '(use default)', currentValue = '', showDefaultModel } = options;

  // Save scroll position
  const prevValue = currentValue || select.value;

  select.innerHTML = '';

  // Add the default/empty option
  if (defaultLabel !== null) {
    const defaultOpt = document.createElement('option');
    defaultOpt.value = defaultLabel === 'Default Model' ? 'default' : '';
    defaultOpt.textContent = showDefaultModel
      ? `${defaultLabel} — ${showDefaultModel}`
      : defaultLabel ?? '(use default)';
    select.appendChild(defaultOpt);
  }

  // Group models by provider
  const seen = new Set<string>();
  for (const provider of providers) {
    const kind = provider.kind || 'custom';
    const models: string[] = [];

    // Provider's configured default model first
    if (provider.default_model && !seen.has(provider.default_model)) {
      seen.add(provider.default_model);
      models.push(provider.default_model);
    }

    // Popular models for this provider kind
    for (const m of (POPULAR_MODELS[kind] ?? [])) {
      if (!seen.has(m)) {
        seen.add(m);
        models.push(m);
      }
    }

    if (models.length === 0) continue;

    const group = document.createElement('optgroup');
    group.label = KIND_LABELS[kind] ?? `${kind}`;
    for (const m of models) {
      const opt = document.createElement('option');
      opt.value = m;
      opt.textContent = m;
      group.appendChild(opt);
    }
    select.appendChild(group);
  }

  // If the previously selected value still exists, restore it
  if (prevValue) {
    const exists = Array.from(select.options).some(o => o.value === prevValue);
    if (exists) {
      select.value = prevValue;
    } else if (prevValue && prevValue !== 'default' && prevValue !== '') {
      // The user's model isn't in our list — add it as a custom entry
      const customGroup = document.createElement('optgroup');
      customGroup.label = '📌 Current';
      const opt = document.createElement('option');
      opt.value = prevValue;
      opt.textContent = prevValue;
      customGroup.appendChild(opt);
      // Insert after the default option
      if (select.children.length > 1) {
        select.insertBefore(customGroup, select.children[1]);
      } else {
        select.appendChild(customGroup);
      }
      select.value = prevValue;
    }
  }
}

export function escAttr(s: string): string {
  return escHtml(s).replace(/\n/g, '&#10;');
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function formatMarkdown(text: string): string {
  // Very simple markdown-ish rendering for chat/research
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
    .replace(/\*(.+?)\*/g, '<em>$1</em>')
    .replace(/`([^`]+)`/g, '<code>$1</code>')
    .replace(/^### (.+)$/gm, '<h4>$1</h4>')
    .replace(/^## (.+)$/gm, '<h3>$1</h3>')
    .replace(/^# (.+)$/gm, '<h2>$1</h2>')
    .replace(/\n/g, '<br>');
}

// Tauri 2 WKWebView (macOS) does not support window.prompt() — it returns null.
// This custom modal replaces all prompt() usage in the app.
export function promptModal(title: string, placeholder?: string): Promise<string | null> {
  return new Promise(resolve => {
    const overlay = $('prompt-modal');
    const titleEl = $('prompt-modal-title');
    const input = $('prompt-modal-input') as HTMLInputElement | null;
    const okBtn = $('prompt-modal-ok');
    const cancelBtn = $('prompt-modal-cancel');
    const closeBtn = $('prompt-modal-close');
    if (!overlay || !input) { resolve(null); return; }

    if (titleEl) titleEl.textContent = title;
    input.placeholder = placeholder ?? '';
    input.value = '';
    overlay.style.display = 'flex';
    input.focus();

    function cleanup() {
      overlay!.style.display = 'none';
      okBtn?.removeEventListener('click', onOk);
      cancelBtn?.removeEventListener('click', onCancel);
      closeBtn?.removeEventListener('click', onCancel);
      input?.removeEventListener('keydown', onKey);
      overlay?.removeEventListener('click', onBackdrop);
    }
    function onOk() {
      const val = input!.value.trim();
      cleanup();
      resolve(val || null);
    }
    function onCancel() { cleanup(); resolve(null); }
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Enter') { e.preventDefault(); onOk(); }
      else if (e.key === 'Escape') { e.preventDefault(); onCancel(); }
    }
    function onBackdrop(e: MouseEvent) {
      if (e.target === overlay) onCancel();
    }

    okBtn?.addEventListener('click', onOk);
    cancelBtn?.addEventListener('click', onCancel);
    closeBtn?.addEventListener('click', onCancel);
    input.addEventListener('keydown', onKey);
    overlay.addEventListener('click', onBackdrop);
  });
}
