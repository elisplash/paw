// Settings: Advanced — Pure constants (no DOM, no IPC)

export const PROVIDER_KINDS: Array<{ value: string; label: string }> = [
  { value: 'ollama', label: 'Ollama (local)' },
  { value: 'openai', label: 'OpenAI' },
  { value: 'anthropic', label: 'Anthropic' },
  { value: 'google', label: 'Google' },
  { value: 'openrouter', label: 'OpenRouter' },
  { value: 'custom', label: 'Custom / Compatible' },
];

export const DEFAULT_BASE_URLS: Record<string, string> = {
  ollama: 'http://localhost:11434',
  openai: 'https://api.openai.com/v1',
  anthropic: 'https://api.anthropic.com',
  google: 'https://generativelanguage.googleapis.com/v1beta',
  openrouter: 'https://openrouter.ai/api/v1',
  custom: '',
};

export const POPULAR_MODELS: Record<string, string[]> = {
  ollama: [
    'llama3.2:3b',
    'llama3.1:8b',
    'llama3.1:70b',
    'mistral:7b',
    'codellama:13b',
    'deepseek-coder:6.7b',
    'phi3:mini',
    'qwen2.5:7b',
  ],
  openai: ['gpt-4o', 'gpt-4o-mini', 'o1', 'o3-mini'],
  anthropic: [
    'claude-sonnet-4-6',
    'claude-haiku-4-5-20251001',
    'claude-3-haiku-20240307',
    'claude-opus-4-6',
  ],
  google: ['gemini-3-flash', 'gemini-2.5-flash-lite', 'gemini-2.0-flash'],
  openrouter: ['meta-llama/llama-3.1-405b-instruct', 'anthropic/claude-sonnet-4-6'],
  custom: [],
};
