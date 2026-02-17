// Settings: Voice — TTS, Talk Mode, Voice Wake

const $ = (id: string) => document.getElementById(id);

// ── TTS ─────────────────────────────────────────────────────────────────────

export async function loadVoiceSettings() {
  const container = $('settings-voice-content');
  if (!container) return;
  container.innerHTML = `<p style="color:var(--text-muted)">Voice settings (TTS, Talk Mode, Voice Wake) are coming soon to the Paw Engine.</p>`;
}

export function initVoiceSettings() {
  // All dynamic
}
