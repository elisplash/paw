// Settings: Voice — DOM rendering, sliders, TTS test, talk mode

import { pawEngine, type TtsConfig } from '../../engine';
import { showToast } from '../../components/toast';
import { $ } from '../../components/helpers';
import { LANGUAGES, voicesForProvider, providerHint } from './atoms';

// ── State bridge ──────────────────────────────────────────────────────

interface MoleculesState {
  getConfig: () => TtsConfig;
  setConfig: (c: TtsConfig) => void;
}

let _state: MoleculesState;

export function initMoleculesState() {
  return {
    setMoleculesState(s: MoleculesState) {
      _state = s;
    },
  };
}

// ── Helpers ─────────────────────────────────────────────────────────

function buildFormConfig(): TtsConfig {
  return {
    provider: ($('tts-provider') as HTMLSelectElement)?.value || 'google',
    voice: ($('tts-voice') as HTMLSelectElement)?.value || 'en-US-Chirp3-HD-Achernar',
    speed: parseFloat(($('tts-speed') as HTMLInputElement)?.value || '1.0'),
    language_code: ($('tts-language') as HTMLSelectElement)?.value || 'en-US',
    auto_speak: ($('tts-auto-speak') as HTMLInputElement)?.checked ?? false,
    elevenlabs_api_key: ($('tts-elevenlabs-key') as HTMLInputElement)?.value || '',
    elevenlabs_model:
      ($('tts-elevenlabs-model') as HTMLSelectElement)?.value || 'eleven_multilingual_v2',
    stability: parseFloat(($('tts-stability') as HTMLInputElement)?.value || '0.5'),
    similarity_boost: parseFloat(($('tts-similarity') as HTMLInputElement)?.value || '0.75'),
  };
}

// ── Render settings form ──────────────────────────────────────────────

export function renderVoiceForm(container: HTMLElement) {
  const config = _state.getConfig();
  const voices = voicesForProvider(config.provider);
  const isEL = config.provider === 'elevenlabs';
  const isGoogle = config.provider === 'google';

  container.innerHTML = `
    <div class="settings-form">

      <!-- TTS Provider -->
      <div class="form-group">
        <label class="form-label">TTS Provider</label>
        <select class="form-input" id="tts-provider">
          <option value="google" ${config.provider === 'google' ? 'selected' : ''}>Google Cloud TTS</option>
          <option value="openai" ${config.provider === 'openai' ? 'selected' : ''}>OpenAI TTS</option>
          <option value="elevenlabs" ${config.provider === 'elevenlabs' ? 'selected' : ''}>ElevenLabs</option>
        </select>
        <div class="form-hint" id="tts-provider-hint">${providerHint(config.provider)}</div>
      </div>

      <!-- ElevenLabs-specific settings -->
      <div id="tts-elevenlabs-group" style="${isEL ? '' : 'display:none'}">
        <div class="form-group">
          <label class="form-label">ElevenLabs API Key</label>
          <input type="password" class="form-input" id="tts-elevenlabs-key" placeholder="xi-..." value="${config.elevenlabs_api_key || ''}">
          <div class="form-hint">Get your API key from elevenlabs.io</div>
        </div>
        <div class="form-group">
          <label class="form-label">Model</label>
          <select class="form-input" id="tts-elevenlabs-model">
            <option value="eleven_multilingual_v2" ${config.elevenlabs_model === 'eleven_multilingual_v2' ? 'selected' : ''}>Multilingual v2 (best quality)</option>
            <option value="eleven_turbo_v2_5" ${config.elevenlabs_model === 'eleven_turbo_v2_5' ? 'selected' : ''}>Turbo v2.5 (fastest)</option>
            <option value="eleven_monolingual_v1" ${config.elevenlabs_model === 'eleven_monolingual_v1' ? 'selected' : ''}>English v1</option>
          </select>
        </div>
        <div class="form-group">
          <label class="form-label">Stability: <span id="tts-stability-val">${config.stability.toFixed(2)}</span></label>
          <input type="range" class="form-range" id="tts-stability" min="0" max="1" step="0.05" value="${config.stability}">
          <div class="form-hint">Lower = more expressive/variable, higher = more consistent</div>
        </div>
        <div class="form-group">
          <label class="form-label">Clarity + Similarity: <span id="tts-similarity-val">${config.similarity_boost.toFixed(2)}</span></label>
          <input type="range" class="form-range" id="tts-similarity" min="0" max="1" step="0.05" value="${config.similarity_boost}">
          <div class="form-hint">Higher = closer to original voice, lower = more creative</div>
        </div>
      </div>

      <!-- Language (Google only) -->
      <div class="form-group" id="tts-language-group" style="${isGoogle ? '' : 'display:none'}">
        <label class="form-label">Language</label>
        <select class="form-input" id="tts-language">
          ${LANGUAGES.map((l) => `<option value="${l.code}" ${config.language_code === l.code ? 'selected' : ''}>${l.label}</option>`).join('')}
        </select>
      </div>

      <!-- Voice -->
      <div class="form-group">
        <label class="form-label">Voice</label>
        <select class="form-input" id="tts-voice">
          ${voices.map((v) => `<option value="${v.id}" ${config.voice === v.id ? 'selected' : ''}>${v.label} (${v.gender})</option>`).join('')}
        </select>
      </div>

      <!-- Speed -->
      <div class="form-group">
        <label class="form-label">Speed: <span id="tts-speed-val">${config.speed.toFixed(1)}x</span></label>
        <input type="range" class="form-range" id="tts-speed" min="0.5" max="2.0" step="0.1" value="${config.speed}">
        <div class="form-hint">0.5x (slow) → 2.0x (fast)</div>
      </div>

      <!-- Auto-speak -->
      <div class="form-group">
        <label class="form-label toggle-label">
          <input type="checkbox" id="tts-auto-speak" ${config.auto_speak ? 'checked' : ''}>
          <span>Auto-speak new responses</span>
        </label>
        <div class="form-hint">Automatically read aloud every new assistant message</div>
      </div>

      <!-- Actions -->
      <div class="form-group" style="display:flex;gap:12px;align-items:center">
        <button class="btn btn-primary" id="tts-save">Save</button>
        <button class="btn btn-ghost" id="tts-test">
          <span class="ms">volume_up</span> Test Voice
        </button>
      </div>

      <!-- Talk Mode section -->
      <div style="margin-top:32px;padding-top:24px;border-top:1px solid var(--border)">
        <h3 style="margin:0 0 8px 0;font-size:14px;color:var(--text)">Talk Mode</h3>
        <p class="form-hint" style="margin:0 0 16px 0">Hold-to-talk or toggle continuous voice conversation. Your speech is transcribed via OpenAI Whisper, sent to your agent, and the response is read aloud automatically.</p>
        <div class="form-group" style="display:flex;gap:12px;align-items:center">
          <button class="btn btn-ghost" id="talk-mode-btn">
            <span class="ms">mic</span> Start Talk Mode
          </button>
          <span class="form-hint" id="talk-mode-status"></span>
        </div>
      </div>
    </div>
  `;

  bindFormEvents();
}

// ── Form events ───────────────────────────────────────────────────────

function bindFormEvents() {
  const providerSelect = $('tts-provider') as HTMLSelectElement;
  providerSelect?.addEventListener('change', () => {
    const provider = providerSelect.value;
    const voiceSelect = $('tts-voice') as HTMLSelectElement;
    const langGroup = $('tts-language-group');
    const elGroup = $('tts-elevenlabs-group');
    const hint = $('tts-provider-hint');
    if (!voiceSelect) return;

    const voices = voicesForProvider(provider);
    voiceSelect.innerHTML = voices
      .map((v) => `<option value="${v.id}">${v.label} (${v.gender})</option>`)
      .join('');

    if (langGroup) langGroup.style.display = provider === 'google' ? '' : 'none';
    if (elGroup) elGroup.style.display = provider === 'elevenlabs' ? '' : 'none';
    if (hint) hint.textContent = providerHint(provider);
  });

  const speedSlider = $('tts-speed') as HTMLInputElement;
  const speedVal = $('tts-speed-val');
  speedSlider?.addEventListener('input', () => {
    if (speedVal) speedVal.textContent = `${parseFloat(speedSlider.value).toFixed(1)}x`;
  });

  const stabilitySlider = $('tts-stability') as HTMLInputElement;
  const stabilityVal = $('tts-stability-val');
  stabilitySlider?.addEventListener('input', () => {
    if (stabilityVal) stabilityVal.textContent = parseFloat(stabilitySlider.value).toFixed(2);
  });

  const similaritySlider = $('tts-similarity') as HTMLInputElement;
  const similarityVal = $('tts-similarity-val');
  similaritySlider?.addEventListener('input', () => {
    if (similarityVal) similarityVal.textContent = parseFloat(similaritySlider.value).toFixed(2);
  });

  $('tts-save')?.addEventListener('click', async () => {
    const config = buildFormConfig();
    _state.setConfig(config);
    try {
      await pawEngine.ttsSetConfig(config);
      showToast('Voice settings saved', 'success');
    } catch (e) {
      showToast(`Failed to save: ${e instanceof Error ? e.message : e}`, 'error');
    }
  });

  $('tts-test')?.addEventListener('click', async () => {
    const btn = $('tts-test') as HTMLButtonElement;
    if (!btn) return;
    btn.disabled = true;
    btn.innerHTML = `<span class="ms">hourglass_top</span> Generating...`;
    try {
      const config = buildFormConfig();
      _state.setConfig(config);
      await pawEngine.ttsSetConfig(config);
      const base64Audio = await pawEngine.ttsSpeak(
        'Hello! I am your Pawz assistant. This is a test of the text to speech system.',
      );
      const audioBytes = Uint8Array.from(atob(base64Audio), (c) => c.charCodeAt(0));
      const blob = new Blob([audioBytes], { type: 'audio/mp3' });
      const url = URL.createObjectURL(blob);
      const audio = new Audio(url);
      audio.addEventListener('ended', () => URL.revokeObjectURL(url));
      audio.play();
      btn.innerHTML = `<span class="ms">volume_up</span> Test Voice`;
    } catch (e) {
      showToast(`TTS test failed: ${e instanceof Error ? e.message : e}`, 'error');
      btn.innerHTML = `<span class="ms">volume_up</span> Test Voice`;
    } finally {
      btn.disabled = false;
    }
  });

  $('talk-mode-btn')?.addEventListener('click', () => toggleTalkMode());
}

// ═══ Talk Mode ═══════════════════════════════════════════════════════════

let _talkModeActive = false;
let _mediaRecorder: MediaRecorder | null = null;
let _audioStream: MediaStream | null = null;
let _talkAudio: HTMLAudioElement | null = null;

async function toggleTalkMode() {
  if (_talkModeActive) {
    stopTalkMode();
  } else {
    await startTalkMode();
  }
}

async function startTalkMode() {
  const btn = $('talk-mode-btn') as HTMLButtonElement;
  const status = $('talk-mode-status');
  if (!btn) return;

  try {
    _audioStream = await navigator.mediaDevices.getUserMedia({
      audio: { echoCancellation: true, noiseSuppression: true, sampleRate: 16000 },
    });

    _talkModeActive = true;
    btn.innerHTML = `<span class="ms">mic_off</span> Stop Talk Mode`;
    btn.classList.add('btn-danger');
    if (status) status.textContent = 'Listening...';

    startRecordingCycle();
  } catch (e) {
    showToast('Microphone access denied — Talk Mode requires mic permission', 'error');
    console.error('[talk] Mic access error:', e);
  }
}

function stopTalkMode() {
  _talkModeActive = false;
  if (_mediaRecorder && _mediaRecorder.state !== 'inactive') {
    _mediaRecorder.stop();
  }
  _mediaRecorder = null;
  if (_audioStream) {
    _audioStream.getTracks().forEach((t) => t.stop());
    _audioStream = null;
  }
  if (_talkAudio) {
    _talkAudio.pause();
    _talkAudio = null;
  }
  const btn = $('talk-mode-btn') as HTMLButtonElement;
  const status = $('talk-mode-status');
  if (btn) {
    btn.innerHTML = `<span class="ms">mic</span> Start Talk Mode`;
    btn.classList.remove('btn-danger');
  }
  if (status) status.textContent = '';
}

function startRecordingCycle() {
  if (!_talkModeActive || !_audioStream) return;

  const status = $('talk-mode-status');

  const mimeType = MediaRecorder.isTypeSupported('audio/webm;codecs=opus')
    ? 'audio/webm;codecs=opus'
    : MediaRecorder.isTypeSupported('audio/webm')
      ? 'audio/webm'
      : 'audio/ogg';

  _mediaRecorder = new MediaRecorder(_audioStream, { mimeType });
  const chunks: Blob[] = [];

  _mediaRecorder.ondataavailable = (e) => {
    if (e.data.size > 0) chunks.push(e.data);
  };

  _mediaRecorder.onstop = async () => {
    if (!_talkModeActive) return;
    if (chunks.length === 0) {
      startRecordingCycle();
      return;
    }

    const blob = new Blob(chunks, { type: mimeType });

    if (blob.size < 8000) {
      if (status) status.textContent = 'Listening...';
      startRecordingCycle();
      return;
    }

    if (status) status.textContent = 'Transcribing...';

    try {
      const base64 = await blobToBase64(blob);

      const transcript = await pawEngine.ttsTranscribe(base64, mimeType);
      if (!transcript.trim()) {
        if (status) status.textContent = 'Listening...';
        startRecordingCycle();
        return;
      }

      if (status)
        status.textContent = `You: "${transcript.substring(0, 60)}${transcript.length > 60 ? '...' : ''}"`;

      const { engineChatSend } = await import('../../engine/molecules/bridge');
      const { appState } = await import('../../state/index');
      const sessionKey = appState.currentSessionKey || 'default';
      const response = await engineChatSend(sessionKey, transcript);

      if (!_talkModeActive) return;

      const responseText =
        typeof response === 'string'
          ? response
          : ((response as Record<string, unknown>)?.content as string) || '';

      if (responseText && _talkModeActive) {
        if (status) status.textContent = 'Speaking...';
        try {
          const audioB64 = await pawEngine.ttsSpeak(responseText);
          const audioBytes = Uint8Array.from(atob(audioB64), (c) => c.charCodeAt(0));
          const audioBlob = new Blob([audioBytes], { type: 'audio/mp3' });
          const url = URL.createObjectURL(audioBlob);
          _talkAudio = new Audio(url);
          _talkAudio.addEventListener('ended', () => {
            URL.revokeObjectURL(url);
            _talkAudio = null;
            if (_talkModeActive) {
              if (status) status.textContent = 'Listening...';
              startRecordingCycle();
            }
          });
          _talkAudio.addEventListener('error', () => {
            URL.revokeObjectURL(url);
            _talkAudio = null;
            if (_talkModeActive) startRecordingCycle();
          });
          _talkAudio.play();
          return;
        } catch (e) {
          console.warn('[talk] TTS failed, continuing:', e);
        }
      }
    } catch (e) {
      console.error('[talk] Cycle error:', e);
      if (status) status.textContent = 'Error — retrying...';
    }

    if (_talkModeActive) {
      setTimeout(() => startRecordingCycle(), 500);
    }
  };

  _mediaRecorder.start();

  setTimeout(() => {
    if (_mediaRecorder && _mediaRecorder.state === 'recording') {
      _mediaRecorder.stop();
    }
  }, 8000);
}

function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = reader.result as string;
      resolve(result.split(',')[1] || result);
    };
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(blob);
  });
}
