// src/engine/molecules/tts.ts
// TTS/STT molecule extracted from chat_controller.ts.
// Instance-able: all functions receive references rather than using globals.
// Owns: speakMessage, autoSpeakIfEnabled, talk mode (record → transcribe).
//
// Talk mode supports two tiers:
//   1. Free (Web Speech API) — zero-config, runs in the browser engine, no API key.
//   2. Enhanced (Whisper) — requires an OpenAI/Google API key for higher accuracy.
// The free tier is used by default; Whisper is used when `stt_provider` is "whisper".

import { pawEngine } from '../../engine';
import { showToast } from '../../components/toast';
import { isWebSpeechAvailable, createWebSpeech, type WebSpeechController } from './web-speech';

// ── Types ────────────────────────────────────────────────────────────────

export interface TtsState {
  ttsAudio: HTMLAudioElement | null;
  ttsActiveBtn: HTMLButtonElement | null;
}

export interface TalkModeController {
  /** Whether talk mode is currently recording. */
  isActive(): boolean;
  /** Toggle talk mode on/off. */
  toggle(): Promise<void>;
  /** Start recording. */
  start(): Promise<void>;
  /** Stop recording (triggers transcription). */
  stop(): void;
  /** Full cleanup: stop recording, release stream. */
  cleanup(): void;
}

// ── Speak message ────────────────────────────────────────────────────────

/**
 * Speak a message using TTS.
 * Toggles playback if the same button is clicked again.
 * Scoped: operates on the provided TtsState, not globals.
 */
export async function speakMessage(
  text: string,
  btn: HTMLButtonElement,
  state: TtsState,
): Promise<void> {
  // Toggle off if same button
  if (state.ttsAudio && state.ttsActiveBtn === btn) {
    state.ttsAudio.pause();
    state.ttsAudio = null;
    btn.innerHTML = `<span class="ms">volume_up</span>`;
    btn.classList.remove('tts-playing');
    state.ttsActiveBtn = null;
    return;
  }
  // Stop any other playback
  if (state.ttsAudio) {
    state.ttsAudio.pause();
    state.ttsAudio = null;
    if (state.ttsActiveBtn) {
      state.ttsActiveBtn.innerHTML = `<span class="ms">volume_up</span>`;
      state.ttsActiveBtn.classList.remove('tts-playing');
    }
  }
  btn.innerHTML = `<span class="ms">hourglass_top</span>`;
  btn.classList.add('tts-loading');
  try {
    const base64Audio = await pawEngine.ttsSpeak(text);
    const audioBytes = Uint8Array.from(atob(base64Audio), (c) => c.charCodeAt(0));
    const blob = new Blob([audioBytes], { type: 'audio/mp3' });
    const url = URL.createObjectURL(blob);
    state.ttsAudio = new Audio(url);
    state.ttsActiveBtn = btn;
    btn.innerHTML = `<span class="ms">stop_circle</span>`;
    btn.classList.remove('tts-loading');
    btn.classList.add('tts-playing');
    state.ttsAudio.addEventListener('ended', () => {
      btn.innerHTML = `<span class="ms">volume_up</span>`;
      btn.classList.remove('tts-playing');
      URL.revokeObjectURL(url);
      state.ttsAudio = null;
      state.ttsActiveBtn = null;
    });
    state.ttsAudio.addEventListener('error', () => {
      btn.innerHTML = `<span class="ms">volume_up</span>`;
      btn.classList.remove('tts-playing');
      URL.revokeObjectURL(url);
      state.ttsAudio = null;
      state.ttsActiveBtn = null;
    });
    state.ttsAudio.play();
  } catch (e) {
    console.error('[tts] Error:', e);
    btn.innerHTML = `<span class="ms">volume_up</span>`;
    btn.classList.remove('tts-loading', 'tts-playing');
    showToast(e instanceof Error ? e.message : 'TTS failed — check Voice settings', 'error');
  }
}

/**
 * Auto-speak if the TTS config has auto_speak enabled.
 * Scoped: operates on the provided TtsState.
 */
export async function autoSpeakIfEnabled(text: string, state: TtsState): Promise<void> {
  try {
    const cfg = await pawEngine.ttsGetConfig();
    if (!cfg.auto_speak) return;
    const base64Audio = await pawEngine.ttsSpeak(text);
    const audioBytes = Uint8Array.from(atob(base64Audio), (c) => c.charCodeAt(0));
    const blob = new Blob([audioBytes], { type: 'audio/mp3' });
    const url = URL.createObjectURL(blob);
    if (state.ttsAudio) state.ttsAudio.pause();
    state.ttsAudio = new Audio(url);
    state.ttsAudio.addEventListener('ended', () => {
      URL.revokeObjectURL(url);
      state.ttsAudio = null;
    });
    state.ttsAudio.play();
  } catch (e) {
    console.warn('[tts] Auto-speak failed:', e);
  }
}

// ── Talk Mode (voice-to-text) ────────────────────────────────────────────

/**
 * Determine which STT backend to use.
 * Checks the TTS config for `stt_provider` field; defaults to "browser"
 * if the Web Speech API is available, otherwise falls back to "whisper".
 */
async function resolveSTTProvider(): Promise<'browser' | 'whisper'> {
  try {
    const cfg = await pawEngine.ttsGetConfig();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const stt = (cfg as any).stt_provider as string | undefined;
    if (stt === 'whisper') return 'whisper';
    if (stt === 'browser') return 'browser';
  } catch {
    // Config unavailable — use browser if possible
  }
  return isWebSpeechAvailable() ? 'browser' : 'whisper';
}

/**
 * Create a scoped talk mode controller.
 * Uses the **free Web Speech API** by default (zero config, zero cost).
 * Falls back to the Whisper backend when explicitly configured or when
 * the Web Speech API is not available.
 *
 * @param getTargetInput — Returns the textarea to inject transcript into.
 * @param getTalkBtn — Returns the talk mode button element.
 * @param maxDurationMs — Max recording duration before auto-stop (default: 30s).
 */
export function createTalkMode(
  getTargetInput: () => HTMLTextAreaElement | null,
  getTalkBtn: () => HTMLElement | null,
  maxDurationMs = 30_000,
): TalkModeController {
  // ── Shared state (both tiers) ──────────────────────────────────────────
  let active = false;

  // ── Whisper-specific state ─────────────────────────────────────────────
  let mediaRecorder: MediaRecorder | null = null;
  let audioStream: MediaStream | null = null;
  let talkTimeout: ReturnType<typeof setTimeout> | null = null;

  // ── Web Speech state ───────────────────────────────────────────────────
  let webSpeech: WebSpeechController | null = null;

  // ── UI helpers ─────────────────────────────────────────────────────────

  function setButtonState(icon: string, title: string, activeClass = false): void {
    const btn = getTalkBtn();
    if (!btn) return;
    btn.innerHTML = `<span class="ms">${icon}</span>`;
    btn.title = title;
    if (activeClass) {
      btn.classList.add('talk-active');
    } else {
      btn.classList.remove('talk-active');
    }
  }

  function resetButton(): void {
    setButtonState('mic', 'Talk Mode — click to speak');
  }

  // ── Inject transcript into textarea ────────────────────────────────────

  function injectTranscript(text: string): void {
    const chatInput = getTargetInput();
    if (!chatInput || !text.trim()) return;
    chatInput.value = text;
    chatInput.style.height = 'auto';
    chatInput.style.height = `${Math.min(chatInput.scrollHeight, 120)}px`;
    chatInput.focus();
  }

  // ── Cleanup ────────────────────────────────────────────────────────────

  function cleanup(): void {
    if (talkTimeout) {
      clearTimeout(talkTimeout);
      talkTimeout = null;
    }
    active = false;
    mediaRecorder = null;
    if (audioStream) {
      audioStream.getTracks().forEach((t) => t.stop());
      audioStream = null;
    }
    if (webSpeech) {
      webSpeech.abort();
      webSpeech = null;
    }
    resetButton();
  }

  // ── Web Speech API (free tier) ─────────────────────────────────────────

  async function startBrowser(): Promise<void> {
    if (!isWebSpeechAvailable()) {
      showToast('Web Speech API not available — configure Whisper in Voice settings', 'error');
      return;
    }

    const chatInput = getTargetInput();

    active = true;
    setButtonState('stop_circle', 'Stop dictation', true);

    webSpeech = createWebSpeech({
      maxDurationMs,
      onResult: (transcript, _isFinal) => {
        // Live preview: show transcript as you speak
        if (chatInput) {
          chatInput.value = transcript;
          chatInput.style.height = 'auto';
          chatInput.style.height = `${Math.min(chatInput.scrollHeight, 120)}px`;
        }
      },
      onEnd: (finalTranscript) => {
        active = false;
        webSpeech = null;
        if (finalTranscript) {
          injectTranscript(finalTranscript);
        } else {
          showToast('No speech detected — try again', 'info');
        }
        resetButton();
      },
      onError: (error) => {
        active = false;
        webSpeech = null;
        if (error === 'not-allowed') {
          showToast('Microphone access denied', 'error');
        } else {
          showToast(`Speech recognition error: ${error}`, 'error');
        }
        resetButton();
      },
    });

    try {
      await webSpeech.start();
    } catch {
      showToast('Microphone access denied', 'error');
      cleanup();
    }
  }

  function stopBrowser(): void {
    if (webSpeech) {
      const transcript = webSpeech.stop();
      // onEnd callback will handle the rest
      if (!transcript) {
        // If stop() returns empty, onEnd might not fire with useful data
        active = false;
        webSpeech = null;
        resetButton();
      }
    }
  }

  // ── Whisper backend (enhanced tier) ────────────────────────────────────

  async function startWhisper(): Promise<void> {
    const btn = getTalkBtn();
    if (!btn) return;

    try {
      audioStream = await navigator.mediaDevices.getUserMedia({
        audio: { echoCancellation: true, noiseSuppression: true, sampleRate: 16000 },
      });

      active = true;
      setButtonState('stop_circle', 'Stop recording', true);

      const mimeType = MediaRecorder.isTypeSupported('audio/webm;codecs=opus')
        ? 'audio/webm;codecs=opus'
        : MediaRecorder.isTypeSupported('audio/webm')
          ? 'audio/webm'
          : 'audio/ogg';

      mediaRecorder = new MediaRecorder(audioStream, { mimeType });
      const chunks: Blob[] = [];

      mediaRecorder.ondataavailable = (e) => {
        if (e.data.size > 0) chunks.push(e.data);
      };

      mediaRecorder.onstop = async () => {
        // Stop audio tracks
        if (audioStream) {
          audioStream.getTracks().forEach((t) => t.stop());
          audioStream = null;
        }
        active = false;
        mediaRecorder = null;

        if (chunks.length === 0) {
          resetButton();
          return;
        }

        const blob = new Blob(chunks, { type: mimeType });
        if (blob.size < 4000) {
          showToast('Recording too short — try again', 'info');
          resetButton();
          return;
        }

        setButtonState('hourglass_top', 'Transcribing...');

        try {
          const reader = new FileReader();
          const base64 = await new Promise<string>((resolve, reject) => {
            reader.onload = () => resolve((reader.result as string).split(',')[1] || '');
            reader.onerror = () => reject(reader.error);
            reader.readAsDataURL(blob);
          });

          const transcript = await pawEngine.ttsTranscribe(base64, mimeType);
          if (transcript.trim()) {
            injectTranscript(transcript);
          } else {
            showToast('No speech detected — try again', 'info');
          }
        } catch (e) {
          console.error('[talk] Transcription error:', e);
          showToast(`Transcription failed: ${e instanceof Error ? e.message : e}`, 'error');
        } finally {
          resetButton();
        }
      };

      mediaRecorder.start();

      // Auto-stop after max duration
      talkTimeout = setTimeout(() => {
        talkTimeout = null;
        if (mediaRecorder && mediaRecorder.state === 'recording') {
          mediaRecorder.stop();
        }
      }, maxDurationMs);
    } catch (e) {
      showToast('Microphone access denied', 'error');
      console.error('[talk] Mic error:', e);
      cleanup();
    }
  }

  function stopWhisper(): void {
    if (talkTimeout) {
      clearTimeout(talkTimeout);
      talkTimeout = null;
    }
    if (mediaRecorder && mediaRecorder.state === 'recording') {
      mediaRecorder.stop();
    }
  }

  // ── Public API (delegates to browser or whisper tier) ──────────────────

  async function start(): Promise<void> {
    const provider = await resolveSTTProvider();
    if (provider === 'browser') {
      await startBrowser();
    } else {
      await startWhisper();
    }
  }

  function stop(): void {
    if (webSpeech) {
      stopBrowser();
    } else {
      stopWhisper();
    }
  }

  async function toggle(): Promise<void> {
    if (active) {
      stop();
    } else {
      await start();
    }
  }

  return {
    isActive: () => active,
    toggle,
    start,
    stop,
    cleanup,
  };
}
