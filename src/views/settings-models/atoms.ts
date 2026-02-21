// Settings: Models & Providers — Pure constants & helpers (no DOM, no IPC)

import type { EngineProviderConfig } from '../../engine';

// ── Provider Kinds ──────────────────────────────────────────────────────────

export const PROVIDER_KINDS: Array<{ value: string; label: string }> = [
  { value: 'ollama', label: 'Ollama (local)' },
  { value: 'openai', label: 'OpenAI' },
  { value: 'anthropic', label: 'Anthropic' },
  { value: 'google', label: 'Google' },
  { value: 'deepseek', label: 'DeepSeek' },
  { value: 'grok', label: 'xAI (Grok)' },
  { value: 'mistral', label: 'Mistral' },
  { value: 'moonshot', label: 'Moonshot / Kimi' },
  { value: 'openrouter', label: 'OpenRouter' },
  { value: 'custom', label: 'Custom / Compatible' },
];

export const DEFAULT_BASE_URLS: Record<string, string> = {
  ollama: 'http://localhost:11434',
  openai: 'https://api.openai.com/v1',
  anthropic: 'https://api.anthropic.com',
  google: 'https://generativelanguage.googleapis.com/v1beta',
  deepseek: 'https://api.deepseek.com/v1',
  grok: 'https://api.x.ai/v1',
  mistral: 'https://api.mistral.ai/v1',
  moonshot: 'https://api.moonshot.cn/v1',
  openrouter: 'https://openrouter.ai/api/v1',
  custom: '',
};

export const POPULAR_MODELS: Record<string, string[]> = {
  ollama: [
    'llama3.2:3b',
    'llama3.2:1b',
    'llama3.1:8b',
    'llama3.1:70b',
    'llama3.3:70b',
    'mistral:7b',
    'mixtral:8x7b',
    'codellama:13b',
    'codellama:34b',
    'deepseek-coder:6.7b',
    'deepseek-coder-v2:16b',
    'phi3:mini',
    'phi3:medium',
    'qwen2.5:7b',
    'qwen2.5:32b',
    'qwen2.5:72b',
    'gemma2:9b',
    'gemma2:27b',
    'command-r:35b',
  ],
  openai: [
    'gpt-4o',
    'gpt-4o-mini',
    'gpt-4.1',
    'gpt-4.1-mini',
    'gpt-4.1-nano',
    'o1',
    'o1-mini',
    'o3',
    'o3-mini',
    'o4-mini',
  ],
  anthropic: [
    'claude-opus-4-6',
    'claude-sonnet-4-6',
    'claude-haiku-4-5-20251001',
    'claude-sonnet-4-5-20250929',
    'claude-3-haiku-20240307',
  ],
  google: [
    'gemini-2.5-pro',
    'gemini-2.5-flash',
    'gemini-2.0-flash',
    'gemini-2.0-flash-lite',
    'gemini-1.5-pro',
    'gemini-1.5-flash',
    'gemini-1.5-flash-8b',
  ],
  openrouter: [
    'anthropic/claude-sonnet-4-6',
    'anthropic/claude-haiku-4-5-20251001',
    'anthropic/claude-3-haiku-20240307',
    'openai/gpt-4o',
    'openai/gpt-4o-mini',
    'google/gemini-2.5-pro',
    'google/gemini-2.5-flash',
    'meta-llama/llama-3.1-405b-instruct',
    'meta-llama/llama-3.1-70b-instruct',
    'deepseek/deepseek-chat',
    'deepseek/deepseek-r1',
    'mistralai/mistral-large',
    'qwen/qwen-2.5-72b-instruct',
  ],
  deepseek: ['deepseek-chat', 'deepseek-reasoner'],
  grok: ['grok-3', 'grok-3-mini', 'grok-2', 'grok-2-mini'],
  mistral: [
    'mistral-large-latest',
    'mistral-medium-latest',
    'mistral-small-latest',
    'codestral-latest',
    'open-mistral-nemo',
    'mistral-embed',
  ],
  moonshot: ['moonshot-v1-8k', 'moonshot-v1-32k', 'moonshot-v1-128k'],
  custom: ['deepseek-chat', 'deepseek-reasoner'],
};

export const KIND_ICONS: Record<string, string> = {
  ollama: 'pets',
  openai: 'smart_toy',
  anthropic: 'psychology',
  google: 'auto_awesome',
  deepseek: 'explore',
  grok: 'bolt',
  mistral: 'air',
  moonshot: 'dark_mode',
  openrouter: 'language',
  custom: 'build',
};

export const SPECIALTIES = [
  'coder',
  'researcher',
  'designer',
  'communicator',
  'security',
  'general',
];

export const TIER_LABELS: Record<string, Record<string, string>> = {
  anthropic: {
    'claude-opus-4-6': 'Flagship — $5/$25 per MTok — complex reasoning, coding agents',
    'claude-sonnet-4-6': 'Best value — $3/$15 per MTok — general purpose workhorse',
    'claude-haiku-4-5-20251001': 'Fast + cheap — $1/$5 per MTok — bulk, cron jobs, summaries',
    'claude-3-haiku-20240307': 'Cheapest — $0.25/$1.25 per MTok — cron jobs, simple tasks',
    'claude-sonnet-4-5-20250929': 'Agentic — strong for computer use tasks',
  },
  openai: {
    'gpt-4o': 'Best value — multimodal',
    'gpt-4o-mini': 'Fast + cheap',
    'gpt-4.1': 'Latest flagship',
    'gpt-4.1-mini': 'Latest mini',
    'gpt-4.1-nano': 'Cheapest',
    o1: 'Deep reasoning',
    'o1-mini': 'Reasoning — cheaper',
    o3: 'Latest reasoning',
    'o3-mini': 'Reasoning — fast',
    'o4-mini': 'Latest reasoning mini',
  },
  google: {
    'gemini-2.5-pro': 'Flagship — best reasoning',
    'gemini-2.5-flash': 'Best value — fast + smart',
    'gemini-2.0-flash': 'Previous gen — fast',
    'gemini-2.0-flash-lite': 'Cheapest',
    'gemini-1.5-pro': 'Previous flagship',
    'gemini-1.5-flash': 'Previous fast',
    'gemini-1.5-flash-8b': 'Smallest',
  },
  deepseek: {
    'deepseek-chat': 'V3 — best value — general purpose',
    'deepseek-reasoner': 'R1 — deep reasoning + math',
  },
  grok: {
    'grok-3': 'Flagship — strongest reasoning',
    'grok-3-mini': 'Fast reasoning — think budget',
    'grok-2': 'Previous flagship',
    'grok-2-mini': 'Previous — fast + cheap',
  },
  mistral: {
    'mistral-large-latest': 'Flagship — best reasoning',
    'mistral-medium-latest': 'Balanced — cost-effective',
    'mistral-small-latest': 'Fast + cheap',
    'codestral-latest': 'Code-specialized',
    'open-mistral-nemo': 'Lightweight open model',
    'mistral-embed': 'Embedding model',
  },
  moonshot: {
    'moonshot-v1-8k': '8K context — fast',
    'moonshot-v1-32k': '32K context — balanced',
    'moonshot-v1-128k': '128K context — long documents',
  },
};

/** Build model list dynamically from all configured providers */
export function buildAllKnownModels(providers: EngineProviderConfig[]): string[] {
  const seen = new Set<string>();
  const models: string[] = [];
  const addModel = (m: string) => {
    if (m && !seen.has(m)) {
      seen.add(m);
      models.push(m);
    }
  };
  for (const p of providers) {
    if (p.default_model) addModel(p.default_model);
  }
  for (const p of providers) {
    for (const m of POPULAR_MODELS[p.kind] ?? []) addModel(m);
  }
  return models;
}

/** Exported so other views (tasks, agents) can get the dynamic model list */
export function getAvailableModelsList(providers: EngineProviderConfig[]): string[] {
  return buildAllKnownModels(providers);
}
