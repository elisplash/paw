import { describe, it, expect } from 'vitest';
import {
  voicesForProvider,
  providerHint,
  GOOGLE_VOICES,
  OPENAI_VOICES,
  ELEVENLABS_VOICES,
  LANGUAGES,
} from './atoms';

describe('voicesForProvider', () => {
  it('returns OpenAI voices', () => {
    expect(voicesForProvider('openai')).toBe(OPENAI_VOICES);
  });

  it('returns ElevenLabs voices', () => {
    expect(voicesForProvider('elevenlabs')).toBe(ELEVENLABS_VOICES);
  });

  it('defaults to Google voices', () => {
    expect(voicesForProvider('google')).toBe(GOOGLE_VOICES);
    expect(voicesForProvider('unknown')).toBe(GOOGLE_VOICES);
  });
});

describe('providerHint', () => {
  it('returns OpenAI hint', () => {
    expect(providerHint('openai')).toContain('OpenAI');
  });

  it('returns ElevenLabs hint', () => {
    expect(providerHint('elevenlabs')).toContain('ElevenLabs');
  });

  it('returns Google hint by default', () => {
    expect(providerHint('google')).toContain('Google');
  });
});

describe('Voice catalogs', () => {
  it('GOOGLE_VOICES has entries', () => {
    expect(GOOGLE_VOICES.length).toBeGreaterThan(10);
  });

  it('OPENAI_VOICES has entries', () => {
    expect(OPENAI_VOICES.length).toBeGreaterThan(5);
  });

  it('ELEVENLABS_VOICES has entries', () => {
    expect(ELEVENLABS_VOICES.length).toBeGreaterThan(10);
  });

  it('LANGUAGES has common languages', () => {
    const codes = LANGUAGES.map((l) => l.code);
    expect(codes).toContain('en-US');
    expect(codes).toContain('es-ES');
    expect(codes).toContain('ja-JP');
  });
});
