// Settings: Voice — TTS Configuration

import { pawEngine, TtsConfig } from '../engine';
import { showToast } from '../components/toast';

const $ = (id: string) => document.getElementById(id);

// ── Google Cloud TTS voice catalog ──────────────────────────────────────

const GOOGLE_VOICES: { id: string; label: string; gender: string }[] = [
  // Chirp 3 HD (latest, highest quality)
  { id: 'en-US-Chirp3-HD-Achernar', label: 'Achernar (Chirp 3 HD)', gender: 'F' },
  { id: 'en-US-Chirp3-HD-Aoede', label: 'Aoede (Chirp 3 HD)', gender: 'F' },
  { id: 'en-US-Chirp3-HD-Charon', label: 'Charon (Chirp 3 HD)', gender: 'M' },
  { id: 'en-US-Chirp3-HD-Fenrir', label: 'Fenrir (Chirp 3 HD)', gender: 'M' },
  { id: 'en-US-Chirp3-HD-Kore', label: 'Kore (Chirp 3 HD)', gender: 'F' },
  { id: 'en-US-Chirp3-HD-Leda', label: 'Leda (Chirp 3 HD)', gender: 'F' },
  { id: 'en-US-Chirp3-HD-Orus', label: 'Orus (Chirp 3 HD)', gender: 'M' },
  { id: 'en-US-Chirp3-HD-Puck', label: 'Puck (Chirp 3 HD)', gender: 'M' },
  { id: 'en-US-Chirp3-HD-Sulafat', label: 'Sulafat (Chirp 3 HD)', gender: 'F' },
  { id: 'en-US-Chirp3-HD-Zephyr', label: 'Zephyr (Chirp 3 HD)', gender: 'F' },
  // Neural2
  { id: 'en-US-Neural2-A', label: 'Neural2-A', gender: 'M' },
  { id: 'en-US-Neural2-C', label: 'Neural2-C', gender: 'F' },
  { id: 'en-US-Neural2-D', label: 'Neural2-D', gender: 'M' },
  { id: 'en-US-Neural2-F', label: 'Neural2-F', gender: 'F' },
  { id: 'en-US-Neural2-H', label: 'Neural2-H', gender: 'F' },
  { id: 'en-US-Neural2-J', label: 'Neural2-J', gender: 'M' },
  // Journey
  { id: 'en-US-Journey-D', label: 'Journey-D', gender: 'M' },
  { id: 'en-US-Journey-F', label: 'Journey-F', gender: 'F' },
  { id: 'en-US-Journey-O', label: 'Journey-O', gender: 'F' },
];

const OPENAI_VOICES: { id: string; label: string; gender: string }[] = [
  { id: 'alloy', label: 'Alloy', gender: 'N' },
  { id: 'ash', label: 'Ash', gender: 'M' },
  { id: 'coral', label: 'Coral', gender: 'F' },
  { id: 'echo', label: 'Echo', gender: 'M' },
  { id: 'fable', label: 'Fable', gender: 'M' },
  { id: 'nova', label: 'Nova', gender: 'F' },
  { id: 'onyx', label: 'Onyx', gender: 'M' },
  { id: 'sage', label: 'Sage', gender: 'F' },
  { id: 'shimmer', label: 'Shimmer', gender: 'F' },
];

const LANGUAGES = [
  { code: 'en-US', label: 'English (US)' },
  { code: 'en-GB', label: 'English (UK)' },
  { code: 'en-AU', label: 'English (AU)' },
  { code: 'es-ES', label: 'Spanish' },
  { code: 'fr-FR', label: 'French' },
  { code: 'de-DE', label: 'German' },
  { code: 'it-IT', label: 'Italian' },
  { code: 'pt-BR', label: 'Portuguese (BR)' },
  { code: 'ja-JP', label: 'Japanese' },
  { code: 'ko-KR', label: 'Korean' },
  { code: 'zh-CN', label: 'Chinese (Mandarin)' },
  { code: 'hi-IN', label: 'Hindi' },
  { code: 'ar-XA', label: 'Arabic' },
];

let _config: TtsConfig = {
  provider: 'google',
  voice: 'en-US-Chirp3-HD-Achernar',
  speed: 1.0,
  language_code: 'en-US',
  auto_speak: false,
};

// ── TTS ─────────────────────────────────────────────────────────────────────

export async function loadVoiceSettings() {
  const container = $('settings-voice-content');
  if (!container) return;

  // Load config from backend
  try {
    _config = await pawEngine.ttsGetConfig();
  } catch (e) {
    console.warn('[voice] Failed to load TTS config, using defaults:', e);
  }

  const voices = _config.provider === 'openai' ? OPENAI_VOICES : GOOGLE_VOICES;

  container.innerHTML = `
    <div class="settings-form">
      <div class="form-group">
        <label class="form-label">TTS Provider</label>
        <select class="form-input" id="tts-provider">
          <option value="google" ${_config.provider === 'google' ? 'selected' : ''}>Google Cloud TTS</option>
          <option value="openai" ${_config.provider === 'openai' ? 'selected' : ''}>OpenAI TTS</option>
        </select>
        <div class="form-hint" id="tts-provider-hint">${
          _config.provider === 'google'
            ? 'Uses your Google API key from Models settings. Chirp 3 HD voices are highest quality.'
            : 'Uses your OpenAI API key from Models settings. $15/1M characters.'
        }</div>
      </div>

      <div class="form-group" id="tts-language-group" style="${_config.provider === 'openai' ? 'display:none' : ''}">
        <label class="form-label">Language</label>
        <select class="form-input" id="tts-language">
          ${LANGUAGES.map(l => `<option value="${l.code}" ${_config.language_code === l.code ? 'selected' : ''}>${l.label}</option>`).join('')}
        </select>
      </div>

      <div class="form-group">
        <label class="form-label">Voice</label>
        <select class="form-input" id="tts-voice">
          ${voices.map(v => `<option value="${v.id}" ${_config.voice === v.id ? 'selected' : ''}>${v.label} (${v.gender})</option>`).join('')}
        </select>
      </div>

      <div class="form-group">
        <label class="form-label">Speed: <span id="tts-speed-val">${_config.speed.toFixed(1)}x</span></label>
        <input type="range" class="form-range" id="tts-speed" min="0.5" max="2.0" step="0.1" value="${_config.speed}">
        <div class="form-hint">0.5x (slow) → 2.0x (fast)</div>
      </div>

      <div class="form-group">
        <label class="form-label toggle-label">
          <input type="checkbox" id="tts-auto-speak" ${_config.auto_speak ? 'checked' : ''}>
          <span>Auto-speak new responses</span>
        </label>
        <div class="form-hint">Automatically read aloud every new assistant message</div>
      </div>

      <div class="form-group" style="display:flex;gap:12px;align-items:center">
        <button class="btn btn-primary" id="tts-save">Save</button>
        <button class="btn btn-ghost" id="tts-test">
          <span class="ms">volume_up</span> Test Voice
        </button>
      </div>
    </div>
  `;

  // ── Event listeners ──

  const providerSelect = $('tts-provider') as HTMLSelectElement;
  providerSelect?.addEventListener('change', () => {
    const provider = providerSelect.value;
    const voiceSelect = $('tts-voice') as HTMLSelectElement;
    const langGroup = $('tts-language-group');
    const hint = $('tts-provider-hint');
    if (!voiceSelect) return;

    const voices = provider === 'openai' ? OPENAI_VOICES : GOOGLE_VOICES;
    voiceSelect.innerHTML = voices.map(v =>
      `<option value="${v.id}">${v.label} (${v.gender})</option>`
    ).join('');

    if (langGroup) langGroup.style.display = provider === 'openai' ? 'none' : '';
    if (hint) hint.textContent = provider === 'google'
      ? 'Uses your Google API key from Models settings. Chirp 3 HD voices are highest quality.'
      : 'Uses your OpenAI API key from Models settings. $15/1M characters.';
  });

  const speedSlider = $('tts-speed') as HTMLInputElement;
  const speedVal = $('tts-speed-val');
  speedSlider?.addEventListener('input', () => {
    if (speedVal) speedVal.textContent = `${parseFloat(speedSlider.value).toFixed(1)}x`;
  });

  $('tts-save')?.addEventListener('click', async () => {
    _config = {
      provider: ($ ('tts-provider') as HTMLSelectElement)?.value || 'google',
      voice: ($('tts-voice') as HTMLSelectElement)?.value || 'en-US-Chirp3-HD-Achernar',
      speed: parseFloat(($('tts-speed') as HTMLInputElement)?.value || '1.0'),
      language_code: ($('tts-language') as HTMLSelectElement)?.value || 'en-US',
      auto_speak: ($('tts-auto-speak') as HTMLInputElement)?.checked ?? false,
    };
    try {
      await pawEngine.ttsSetConfig(_config);
      showToast('Voice settings saved', 'success');
    } catch (e) {
      showToast('Failed to save: ' + (e instanceof Error ? e.message : e), 'error');
    }
  });

  $('tts-test')?.addEventListener('click', async () => {
    const btn = $('tts-test') as HTMLButtonElement;
    if (!btn) return;
    btn.disabled = true;
    btn.innerHTML = `<span class="ms">hourglass_top</span> Generating...`;
    try {
      // Save current form state first
      _config = {
        provider: ($('tts-provider') as HTMLSelectElement)?.value || 'google',
        voice: ($('tts-voice') as HTMLSelectElement)?.value || 'en-US-Chirp3-HD-Achernar',
        speed: parseFloat(($('tts-speed') as HTMLInputElement)?.value || '1.0'),
        language_code: ($('tts-language') as HTMLSelectElement)?.value || 'en-US',
        auto_speak: ($('tts-auto-speak') as HTMLInputElement)?.checked ?? false,
      };
      await pawEngine.ttsSetConfig(_config);
      const base64Audio = await pawEngine.ttsSpeak('Hello! I am your Pawz assistant. This is a test of the text to speech system.');
      const audioBytes = Uint8Array.from(atob(base64Audio), c => c.charCodeAt(0));
      const blob = new Blob([audioBytes], { type: 'audio/mp3' });
      const url = URL.createObjectURL(blob);
      const audio = new Audio(url);
      audio.addEventListener('ended', () => URL.revokeObjectURL(url));
      audio.play();
      btn.innerHTML = `<span class="ms">volume_up</span> Test Voice`;
    } catch (e) {
      showToast('TTS test failed: ' + (e instanceof Error ? e.message : e), 'error');
      btn.innerHTML = `<span class="ms">volume_up</span> Test Voice`;
    } finally {
      btn.disabled = false;
    }
  });
}

export function initVoiceSettings() {
  // All dynamic — loaded when tab is opened
}
