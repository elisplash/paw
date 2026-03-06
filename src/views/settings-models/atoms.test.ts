import { describe, it, expect } from 'vitest';
import {
  buildAllKnownModels,
  getAvailableModelsList,
  PROVIDER_KINDS,
  DEFAULT_BASE_URLS,
  POPULAR_MODELS,
  TIER_LABELS,
  KIND_ICONS,
  SPECIALTIES,
} from './atoms';
import type { EngineProviderConfig } from '../../engine/atoms/types';

// ── buildAllKnownModels ────────────────────────────────────────────────

describe('buildAllKnownModels', () => {
  const fakeProvider = (kind: string, defaultModel?: string): EngineProviderConfig =>
    ({
      id: `p-${kind}`,
      kind,
      api_key: 'test',
      default_model: defaultModel,
    }) as EngineProviderConfig;

  it('includes default_model from provider', () => {
    const models = buildAllKnownModels([fakeProvider('openai', 'gpt-4o')]);
    expect(models).toContain('gpt-4o');
  });

  it('includes popular models for provider kind', () => {
    const models = buildAllKnownModels([fakeProvider('deepseek')]);
    expect(models).toContain('deepseek-chat');
    expect(models).toContain('deepseek-reasoner');
  });

  it('deduplicates models', () => {
    const models = buildAllKnownModels([fakeProvider('openai', 'gpt-4o')]);
    const gpt4oCount = models.filter((m) => m === 'gpt-4o').length;
    expect(gpt4oCount).toBe(1);
  });

  it('combines models from multiple providers', () => {
    const models = buildAllKnownModels([fakeProvider('openai'), fakeProvider('anthropic')]);
    expect(models).toContain('gpt-4o');
    expect(models).toContain('claude-opus-4-6');
  });

  it('returns empty for empty providers', () => {
    expect(buildAllKnownModels([])).toEqual([]);
  });

  it('handles unknown kind gracefully', () => {
    const models = buildAllKnownModels([fakeProvider('nonexistent', 'some-model')]);
    expect(models).toContain('some-model');
    // No popular models for unknown kind, so only the default
    expect(models).toHaveLength(1);
  });
});

// ── getAvailableModelsList ─────────────────────────────────────────────

describe('getAvailableModelsList', () => {
  it('is equivalent to buildAllKnownModels', () => {
    const providers = [{ id: 'p1', kind: 'ollama', api_key: '' } as EngineProviderConfig];
    expect(getAvailableModelsList(providers)).toEqual(buildAllKnownModels(providers));
  });
});

// ── Constants ──────────────────────────────────────────────────────────

describe('PROVIDER_KINDS', () => {
  it('has 11 providers', () => {
    expect(PROVIDER_KINDS).toHaveLength(11);
  });

  it('each entry has value and label', () => {
    for (const kind of PROVIDER_KINDS) {
      expect(kind.value).toBeDefined();
      expect(kind.label).toBeDefined();
    }
  });

  it('includes ollama, openai, anthropic', () => {
    const values = PROVIDER_KINDS.map((k) => k.value);
    expect(values).toContain('ollama');
    expect(values).toContain('openai');
    expect(values).toContain('anthropic');
  });
});

describe('DEFAULT_BASE_URLS', () => {
  it('has URL for every provider kind', () => {
    for (const kind of PROVIDER_KINDS) {
      expect(kind.value in DEFAULT_BASE_URLS).toBe(true);
    }
  });

  it('ollama defaults to localhost', () => {
    expect(DEFAULT_BASE_URLS.ollama).toContain('localhost');
  });

  it('custom has empty string', () => {
    expect(DEFAULT_BASE_URLS.custom).toBe('');
  });
});

describe('POPULAR_MODELS', () => {
  it('has models for core providers', () => {
    expect(POPULAR_MODELS.openai.length).toBeGreaterThan(0);
    expect(POPULAR_MODELS.anthropic.length).toBeGreaterThan(0);
    expect(POPULAR_MODELS.ollama.length).toBeGreaterThan(0);
    expect(POPULAR_MODELS.google.length).toBeGreaterThan(0);
  });
});

describe('TIER_LABELS', () => {
  it('has labels for anthropic models', () => {
    expect(TIER_LABELS.anthropic['claude-opus-4-6']).toContain('Flagship');
  });

  it('has labels for openai models', () => {
    expect(TIER_LABELS.openai['gpt-4o']).toContain('Best value');
  });
});

describe('KIND_ICONS', () => {
  it('has icon for every provider kind', () => {
    for (const kind of PROVIDER_KINDS) {
      expect(KIND_ICONS[kind.value]).toBeDefined();
    }
  });
});

describe('SPECIALTIES', () => {
  it('includes coder and general', () => {
    expect(SPECIALTIES).toContain('coder');
    expect(SPECIALTIES).toContain('general');
  });
});
