// Settings: Voice — Pure data catalogs (no DOM, no IPC)

export const GOOGLE_VOICES: { id: string; label: string; gender: string }[] = [
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

export const OPENAI_VOICES: { id: string; label: string; gender: string }[] = [
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

export const ELEVENLABS_VOICES: { id: string; label: string; gender: string }[] = [
  { id: 'EXAVITQu4vr4xnSDxMaL', label: 'Sarah', gender: 'F' },
  { id: 'IKne3meq5aSn9XLyUdCD', label: 'Charlie', gender: 'M' },
  { id: 'JBFqnCBsd6RMkjVDRZzb', label: 'George', gender: 'M' },
  { id: 'N2lVS1w4EtoT3dr4eOWO', label: 'Callum', gender: 'M' },
  { id: 'TX3LPaxmHKxFdv7VOQHJ', label: 'Liam', gender: 'M' },
  { id: 'XB0fDUnXU5powFXDhCwa', label: 'Charlotte', gender: 'F' },
  { id: 'Xb7hH8MSUJpSbSDYk0k2', label: 'Alice', gender: 'F' },
  { id: 'XrExE9yKIg1WjnnlVkGX', label: 'Matilda', gender: 'F' },
  { id: 'bIHbv24MWmeRgasZH58o', label: 'Will', gender: 'M' },
  { id: 'cgSgspJ2msm6clMCkdW9', label: 'Jessica', gender: 'F' },
  { id: 'cjVigY5qzO86Huf0OWal', label: 'Eric', gender: 'M' },
  { id: 'iP95p4xoKVk53GoZ742B', label: 'Chris', gender: 'M' },
  { id: 'nPczCjzI2devNBz1zQrb', label: 'Brian', gender: 'M' },
  { id: 'onwK4e9ZLuTAKqWW03F9', label: 'Daniel', gender: 'M' },
  { id: 'pFZP5JQG7iQjIQuC4Bku', label: 'Lily', gender: 'F' },
  { id: 'pqHfZKP75CvOlQylNhV4', label: 'Bill', gender: 'M' },
];

export const LANGUAGES = [
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

export function voicesForProvider(provider: string) {
  switch (provider) {
    case 'openai':
      return OPENAI_VOICES;
    case 'elevenlabs':
      return ELEVENLABS_VOICES;
    default:
      return GOOGLE_VOICES;
  }
}

export function providerHint(provider: string): string {
  switch (provider) {
    case 'openai':
      return 'Uses your OpenAI API key from Models settings. $15/1M characters.';
    case 'elevenlabs':
      return 'Uses your ElevenLabs API key (entered below). Premium neural voices.';
    default:
      return 'Uses your Google API key from Models settings. Chirp 3 HD voices are highest quality.';
  }
}
