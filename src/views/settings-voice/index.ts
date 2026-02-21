// Settings: Voice — Orchestration, state, exports

import { pawEngine, type TtsConfig } from '../../engine';
import { $ } from '../../components/helpers';
import { initMoleculesState, renderVoiceForm } from './molecules';

// ── State ─────────────────────────────────────────────────────────────

let _config: TtsConfig = {
  provider: 'google',
  voice: 'en-US-Chirp3-HD-Achernar',
  speed: 1.0,
  language_code: 'en-US',
  auto_speak: false,
  elevenlabs_api_key: '',
  elevenlabs_model: 'eleven_multilingual_v2',
  stability: 0.5,
  similarity_boost: 0.75,
};

// ── State bridge ──────────────────────────────────────────────────────

const { setMoleculesState } = initMoleculesState();
setMoleculesState({
  getConfig: () => _config,
  setConfig: (c: TtsConfig) => {
    _config = c;
  },
});

// ── Public API ────────────────────────────────────────────────────────

export async function loadVoiceSettings() {
  const container = $('settings-voice-content');
  if (!container) return;

  try {
    _config = await pawEngine.ttsGetConfig();
  } catch (e) {
    console.warn('[voice] Failed to load TTS config, using defaults:', e);
  }

  renderVoiceForm(container);
}

export function initVoiceSettings() {
  // All dynamic — loaded when tab is opened
}

// ── Re-exports ────────────────────────────────────────────────────────

export { GOOGLE_VOICES, OPENAI_VOICES, ELEVENLABS_VOICES, LANGUAGES } from './atoms';
