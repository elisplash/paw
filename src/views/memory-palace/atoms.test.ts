import { describe, it, expect } from 'vitest';
import { validateMemoryForm, CATEGORY_COLORS, agentLabel, buildAgentFilterOptions } from './atoms';
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

// ── agentLabel ─────────────────────────────────────────────────────────

describe('agentLabel', () => {
  it('returns "system" for undefined', () => {
    expect(agentLabel(undefined)).toBe('system');
  });

  it('returns "system" for empty string', () => {
    expect(agentLabel('')).toBe('system');
  });

  it('returns the agent id when present', () => {
    expect(agentLabel('alice')).toBe('alice');
  });

  it('preserves agent id casing', () => {
    expect(agentLabel('ResearchBot')).toBe('ResearchBot');
  });
});

// ── buildAgentFilterOptions ────────────────────────────────────────────

describe('buildAgentFilterOptions', () => {
  it('returns empty string for empty list', () => {
    expect(buildAgentFilterOptions([])).toBe('');
  });

  it('builds options from agent ids', () => {
    const html = buildAgentFilterOptions(['alice', 'bob']);
    expect(html).toContain('value="alice"');
    expect(html).toContain('value="bob"');
  });

  it('deduplicates agent ids', () => {
    const html = buildAgentFilterOptions(['alice', 'alice', 'bob']);
    const matches = html.match(/value="alice"/g);
    expect(matches).toHaveLength(1);
  });

  it('filters out empty strings', () => {
    const html = buildAgentFilterOptions(['', 'alice', '']);
    expect(html).not.toContain('value=""');
    expect(html).toContain('value="alice"');
  });
});
