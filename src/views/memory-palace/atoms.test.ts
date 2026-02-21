import { describe, it, expect } from 'vitest';
import { validateMemoryForm, CATEGORY_COLORS } from './atoms';
import type { MemoryFormInputs } from './atoms';

// ── validateMemoryForm ─────────────────────────────────────────────────

describe('validateMemoryForm', () => {
  const validInputs: MemoryFormInputs = {
    apiKey: 'sk-test-key',
    azureBaseUrl: '',
    openaiBaseUrl: 'https://api.openai.com/v1',
    modelName: 'gpt-4',
    apiVersion: '2024-01-01',
    provider: 'openai',
  };

  it('returns ok for valid OpenAI inputs', () => {
    const r = validateMemoryForm(validInputs);
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.data.apiKey).toBe('sk-test-key');
      expect(r.data.provider).toBe('openai');
    }
  });

  it('errors when URL pasted into API key field and no baseUrl', () => {
    const r = validateMemoryForm({
      ...validInputs,
      apiKey: 'https://api.example.com',
      openaiBaseUrl: '',
    });
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error.kind).toBe('url_in_key');
  });

  it('errors when URL pasted into API key field with existing baseUrl', () => {
    const r = validateMemoryForm({ ...validInputs, apiKey: 'https://api.example.com' });
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error.kind).toBe('url_in_key_dup');
  });

  it('errors when azure provider has no base URL', () => {
    const r = validateMemoryForm({
      ...validInputs,
      provider: 'azure',
      azureBaseUrl: '',
    });
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error.kind).toBe('azure_no_url');
  });

  it('errors when API key is empty', () => {
    const r = validateMemoryForm({ ...validInputs, apiKey: '' });
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error.kind).toBe('no_key');
  });
});

// ── CATEGORY_COLORS ────────────────────────────────────────────────────

describe('CATEGORY_COLORS', () => {
  it('has expected categories', () => {
    expect(CATEGORY_COLORS).toHaveProperty('preference');
    expect(CATEGORY_COLORS).toHaveProperty('fact');
    expect(CATEGORY_COLORS).toHaveProperty('code');
  });

  it('values are hex colors', () => {
    for (const color of Object.values(CATEGORY_COLORS)) {
      expect(color).toMatch(/^#[0-9A-Fa-f]{6}$/);
    }
  });
});
