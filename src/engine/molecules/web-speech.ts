// src/engine/molecules/web-speech.ts
// Free, zero-config speech-to-text using the browser's built-in Web Speech API.
// No API key required. Works in WebKit (macOS/Tauri) and Chromium webviews.
// Falls back gracefully if the API is not available.

// ── Types ────────────────────────────────────────────────────────────────

/** Minimal subset of the SpeechRecognition API we use. */
interface SpeechRecognitionEvent {
  readonly results: SpeechRecognitionResultList;
  readonly resultIndex: number;
}

interface SpeechRecognitionResultList {
  readonly length: number;
  item(index: number): SpeechRecognitionResult;
  [index: number]: SpeechRecognitionResult;
}

interface SpeechRecognitionResult {
  readonly length: number;
  readonly isFinal: boolean;
  item(index: number): SpeechRecognitionAlternative;
  [index: number]: SpeechRecognitionAlternative;
}

interface SpeechRecognitionAlternative {
  readonly transcript: string;
  readonly confidence: number;
}

interface SpeechRecognitionInstance extends EventTarget {
  continuous: boolean;
  interimResults: boolean;
  lang: string;
  maxAlternatives: number;
  start(): void;
  stop(): void;
  abort(): void;
  onresult: ((event: SpeechRecognitionEvent) => void) | null;
  onerror: ((event: { error: string; message?: string }) => void) | null;
  onend: (() => void) | null;
  onstart: (() => void) | null;
  onspeechend: (() => void) | null;
}

interface SpeechRecognitionConstructor {
  new (): SpeechRecognitionInstance;
}

// ── Availability check ───────────────────────────────────────────────────

/**
 * Returns true if the browser's Web Speech API is available.
 * Works in WebKit (Tauri macOS) and Chromium-based webviews.
 */
export function isWebSpeechAvailable(): boolean {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const w = window as any;
  return !!(w.SpeechRecognition || w.webkitSpeechRecognition);
}

function getSpeechRecognition(): SpeechRecognitionConstructor | null {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const w = window as any;
  return w.SpeechRecognition || w.webkitSpeechRecognition || null;
}

// ── Controller ───────────────────────────────────────────────────────────

export interface WebSpeechController {
  /** Whether speech recognition is currently active. */
  isActive(): boolean;
  /** Start recognition. Rejects if API is unavailable or mic is denied. */
  start(): Promise<void>;
  /** Stop recognition and return the final transcript. */
  stop(): string;
  /** Abort without returning results. */
  abort(): void;
  /** Get the current transcript (may include interim results). */
  getTranscript(): string;
}

export interface WebSpeechOptions {
  /** Language code, e.g. "en-US". Default: navigator.language or "en-US". */
  lang?: string;
  /** Called on every interim or final result with the full accumulated transcript. */
  onResult?: (transcript: string, isFinal: boolean) => void;
  /** Called when recognition ends (naturally or via stop/abort). */
  onEnd?: (finalTranscript: string) => void;
  /** Called on error. */
  onError?: (error: string) => void;
  /** Whether to use continuous mode (keeps listening). Default: true. */
  continuous?: boolean;
  /** Max recording duration in ms before auto-stop. Default: 30000 (30s). */
  maxDurationMs?: number;
}

/**
 * Create a Web Speech API recognition controller.
 * Zero cost, zero config — uses the browser's built-in speech engine.
 */
export function createWebSpeech(options: WebSpeechOptions = {}): WebSpeechController {
  const {
    lang = navigator.language || 'en-US',
    onResult,
    onEnd,
    onError,
    continuous = true,
    maxDurationMs = 30_000,
  } = options;

  let recognition: SpeechRecognitionInstance | null = null;
  let active = false;
  let finalTranscript = '';
  let interimTranscript = '';
  let autoStopTimer: ReturnType<typeof setTimeout> | null = null;

  function clearTimer(): void {
    if (autoStopTimer) {
      clearTimeout(autoStopTimer);
      autoStopTimer = null;
    }
  }

  function cleanup(): void {
    clearTimer();
    active = false;
    if (recognition) {
      recognition.onresult = null;
      recognition.onerror = null;
      recognition.onend = null;
      recognition.onstart = null;
      recognition = null;
    }
  }

  async function start(): Promise<void> {
    if (active) return;

    const Ctor = getSpeechRecognition();
    if (!Ctor) {
      const msg = 'Web Speech API not available in this browser';
      onError?.(msg);
      throw new Error(msg);
    }

    recognition = new Ctor();
    recognition.continuous = continuous;
    recognition.interimResults = true;
    recognition.lang = lang;
    recognition.maxAlternatives = 1;

    finalTranscript = '';
    interimTranscript = '';

    recognition.onresult = (event: SpeechRecognitionEvent) => {
      let interim = '';
      for (let i = event.resultIndex; i < event.results.length; i++) {
        const result = event.results[i];
        if (result.isFinal) {
          finalTranscript += result[0].transcript;
        } else {
          interim += result[0].transcript;
        }
      }
      interimTranscript = interim;
      const full = finalTranscript + interimTranscript;
      onResult?.(full, interimTranscript.length === 0);
    };

    recognition.onerror = (event) => {
      // "no-speech" and "aborted" are non-fatal
      if (event.error === 'no-speech' || event.error === 'aborted') {
        return;
      }
      console.error('[web-speech] Error:', event.error, event.message);
      onError?.(event.error);
      cleanup();
    };

    recognition.onend = () => {
      const wasActive = active;
      cleanup();
      if (wasActive) {
        const result = finalTranscript.trim();
        onEnd?.(result);
      }
    };

    // Request mic permission by starting recognition
    try {
      recognition.start();
      active = true;

      // Auto-stop after max duration
      autoStopTimer = setTimeout(() => {
        autoStopTimer = null;
        if (active && recognition) {
          recognition.stop();
        }
      }, maxDurationMs);
    } catch (e) {
      cleanup();
      throw e;
    }
  }

  function stop(): string {
    clearTimer();
    if (recognition && active) {
      recognition.stop();
    }
    const result = (finalTranscript + interimTranscript).trim();
    return result;
  }

  function abort(): void {
    clearTimer();
    active = false; // prevent onEnd callback from firing transcript
    if (recognition) {
      recognition.abort();
    }
    cleanup();
  }

  return {
    isActive: () => active,
    start,
    stop,
    abort,
    getTranscript: () => (finalTranscript + interimTranscript).trim(),
  };
}
