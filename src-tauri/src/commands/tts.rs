// commands/tts.rs — Text-to-Speech commands + Google/OpenAI helper implementations.

use crate::commands::state::EngineState;
use crate::engine::types::*;
use log::info;
use tauri::State;

/// TTS configuration stored in DB as JSON under key "tts_config"
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TtsConfig {
    pub provider: String,        // "google" | "openai" | "elevenlabs"
    pub voice: String,           // e.g. "en-US-Chirp3-HD-Achernar" or "alloy" or ElevenLabs voice_id
    pub speed: f64,              // 0.25–4.0
    pub language_code: String,   // e.g. "en-US"
    pub auto_speak: bool,        // automatically speak new responses
    #[serde(default)]
    pub elevenlabs_api_key: String,  // ElevenLabs API key (separate from provider keys)
    #[serde(default = "default_elevenlabs_model")]
    pub elevenlabs_model: String,    // "eleven_multilingual_v2" | "eleven_turbo_v2_5"
    #[serde(default = "default_stability")]
    pub stability: f64,              // 0.0–1.0 (ElevenLabs voice stability)
    #[serde(default = "default_similarity")]
    pub similarity_boost: f64,       // 0.0–1.0 (ElevenLabs clarity + similarity)
}

fn default_elevenlabs_model() -> String { "eleven_multilingual_v2".into() }
fn default_stability() -> f64 { 0.5 }
fn default_similarity() -> f64 { 0.75 }

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            provider: "google".into(),
            voice: "en-US-Chirp3-HD-Achernar".into(),
            speed: 1.0,
            language_code: "en-US".into(),
            auto_speak: false,
            elevenlabs_api_key: String::new(),
            elevenlabs_model: "eleven_multilingual_v2".into(),
            stability: 0.5,
            similarity_boost: 0.75,
        }
    }
}

/// Synthesize speech from text. Returns base64-encoded MP3 audio.
#[tauri::command]
pub async fn engine_tts_speak(
    state: State<'_, EngineState>,
    text: String,
) -> Result<String, String> {
    if text.trim().is_empty() {
        return Err("No text to speak".into());
    }

    // Load TTS config from DB
    let tts_config: TtsConfig = {
        let store = &state.store;
        match store.get_config("tts_config") {
            Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_default(),
            _ => TtsConfig::default(),
        }
    };

    // Find the provider's API key from engine config
    // Extract needed values before any async calls (MutexGuard is !Send)
    let (openai_provider_info, google_key) = {
        let config = state.config.lock();
        let openai = config.providers.iter().find(|p| p.kind == ProviderKind::OpenAI);
        let google = config.providers.iter().find(|p| p.kind == ProviderKind::Google);
        (
            openai.map(|p| (p.api_key.clone(), p.base_url.clone().unwrap_or_else(|| "https://api.openai.com/v1".into()))),
            google.map(|p| p.api_key.clone()),
        )
    };

    match tts_config.provider.as_str() {
        "openai" => {
            let (api_key, base_url) = openai_provider_info
                .ok_or("No OpenAI provider configured — add one in Settings → Models")?;
            tts_openai(&api_key, &base_url, &text, &tts_config).await
        }
        "elevenlabs" => {
            if tts_config.elevenlabs_api_key.is_empty() {
                return Err("No ElevenLabs API key configured — add one in Settings → Voice & TTS".into());
            }
            tts_elevenlabs(&tts_config.elevenlabs_api_key, &text, &tts_config).await
        }
        _ => {
            // Default: Google Cloud TTS
            let api_key = google_key
                .ok_or("No Google provider configured — add one in Settings → Models")?;
            tts_google(&api_key, &text, &tts_config).await
        }
    }
}

/// Get TTS config
#[tauri::command]
pub fn engine_tts_get_config(
    state: State<'_, EngineState>,
) -> Result<TtsConfig, String> {
    match state.store.get_config("tts_config") {
        Ok(Some(json)) => serde_json::from_str(&json).map_err(|e| e.to_string()),
        _ => Ok(TtsConfig::default()),
    }
}

/// Save TTS config
#[tauri::command]
pub fn engine_tts_set_config(
    state: State<'_, EngineState>,
    config: TtsConfig,
) -> Result<(), String> {
    let json = serde_json::to_string(&config).map_err(|e| e.to_string())?;
    state.store.set_config("tts_config", &json)?;
    info!("[tts] Config saved: provider={}, voice={}", config.provider, config.voice);
    Ok(())
}

/// Google Cloud TTS — calls texttospeech.googleapis.com/v1/text:synthesize
async fn tts_google(api_key: &str, text: &str, config: &TtsConfig) -> Result<String, String> {
    // Strip markdown for cleaner speech
    let clean = strip_markdown(text);
    if clean.trim().is_empty() {
        return Err("No speakable text after stripping markdown".into());
    }

    // Google TTS has a 5000 byte limit per request — chunk if needed
    let chunks = chunk_text(&clean, 4800);
    let client = reqwest::Client::new();
    let mut all_audio = Vec::new();

    for chunk in &chunks {
        let body = serde_json::json!({
            "input": { "text": chunk },
            "voice": {
                "languageCode": config.language_code,
                "name": config.voice
            },
            "audioConfig": {
                "audioEncoding": "MP3",
                "speakingRate": config.speed,
                "effectsProfileId": ["headphone-class-device"]
            }
        });

        let resp = client
            .post(format!(
                "https://texttospeech.googleapis.com/v1/text:synthesize?key={}",
                api_key
            ))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Google TTS request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Google TTS error ({}): {}", status, body));
        }

        let result: serde_json::Value = resp.json().await
            .map_err(|e| format!("Google TTS JSON parse error: {}", e))?;

        if let Some(audio) = result["audioContent"].as_str() {
            // Decode and accumulate raw audio bytes
            let bytes = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                audio,
            ).map_err(|e| format!("Base64 decode error: {}", e))?;
            all_audio.extend_from_slice(&bytes);
        } else {
            return Err("Google TTS: no audioContent in response".into());
        }
    }

    // Re-encode combined audio as base64
    Ok(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &all_audio,
    ))
}

/// OpenAI TTS — calls /v1/audio/speech
async fn tts_openai(api_key: &str, base_url: &str, text: &str, config: &TtsConfig) -> Result<String, String> {
    let clean = strip_markdown(text);
    if clean.trim().is_empty() {
        return Err("No speakable text after stripping markdown".into());
    }

    // OpenAI TTS has a 4096 char limit
    let chunks = chunk_text(&clean, 4000);
    let client = reqwest::Client::new();
    let mut all_audio = Vec::new();

    for chunk in &chunks {
        let body = serde_json::json!({
            "model": "tts-1",
            "input": chunk,
            "voice": config.voice,
            "speed": config.speed,
            "response_format": "mp3"
        });

        let resp = client
            .post(format!("{}/audio/speech", base_url.trim_end_matches('/')))
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("OpenAI TTS request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("OpenAI TTS error ({}): {}", status, body));
        }

        let bytes = resp.bytes().await
            .map_err(|e| format!("OpenAI TTS read error: {}", e))?;
        all_audio.extend_from_slice(&bytes);
    }

    Ok(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &all_audio,
    ))
}

/// ElevenLabs TTS — calls api.elevenlabs.io/v1/text-to-speech/{voice_id}
async fn tts_elevenlabs(api_key: &str, text: &str, config: &TtsConfig) -> Result<String, String> {
    let clean = strip_markdown(text);
    if clean.trim().is_empty() {
        return Err("No speakable text after stripping markdown".into());
    }

    // ElevenLabs has a 5000 char limit per request
    let chunks = chunk_text(&clean, 4800);
    let client = reqwest::Client::new();
    let mut all_audio = Vec::new();

    for chunk in &chunks {
        let body = serde_json::json!({
            "text": chunk,
            "model_id": config.elevenlabs_model,
            "voice_settings": {
                "stability": config.stability,
                "similarity_boost": config.similarity_boost,
                "speed": config.speed,
            }
        });

        let resp = client
            .post(format!(
                "https://api.elevenlabs.io/v1/text-to-speech/{}",
                config.voice
            ))
            .header("xi-api-key", api_key)
            .header("Accept", "audio/mpeg")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("ElevenLabs TTS request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("ElevenLabs TTS error ({}): {}", status, body));
        }

        let bytes = resp.bytes().await
            .map_err(|e| format!("ElevenLabs TTS read error: {}", e))?;
        all_audio.extend_from_slice(&bytes);
    }

    info!("[tts] ElevenLabs synthesized {} chunks, {} bytes total", chunks.len(), all_audio.len());
    Ok(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &all_audio,
    ))
}

/// Transcribe audio (base64-encoded) to text using OpenAI Whisper API.
/// Used by Talk Mode for speech-to-text.
#[tauri::command]
pub async fn engine_tts_transcribe(
    state: State<'_, EngineState>,
    audio_base64: String,
    mime_type: String,
) -> Result<String, String> {
    if audio_base64.is_empty() {
        return Err("No audio data provided".into());
    }

    // Find OpenAI provider for Whisper API
    let (api_key, base_url) = {
        let config = state.config.lock();
        config.providers.iter()
            .find(|p| p.kind == ProviderKind::OpenAI)
            .map(|p| (p.api_key.clone(), p.base_url.clone().unwrap_or_else(|| "https://api.openai.com/v1".into())))
            .ok_or("No OpenAI provider configured — Talk Mode requires OpenAI Whisper")?
    };

    // Decode base64 audio
    let audio_bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &audio_base64,
    ).map_err(|e| format!("Audio decode error: {}", e))?;

    // Determine file extension from mime type
    let ext = match mime_type.as_str() {
        "audio/webm" | "audio/webm;codecs=opus" => "webm",
        "audio/ogg" | "audio/ogg;codecs=opus" => "ogg",
        "audio/mp4" => "mp4",
        "audio/wav" | "audio/wave" => "wav",
        "audio/mpeg" | "audio/mp3" => "mp3",
        _ => "webm",
    };

    // Build multipart form
    let file_part = reqwest::multipart::Part::bytes(audio_bytes)
        .file_name(format!("audio.{}", ext))
        .mime_str(&mime_type)
        .map_err(|e| format!("MIME error: {}", e))?;

    let form = reqwest::multipart::Form::new()
        .text("model", "whisper-1")
        .text("language", "en")
        .part("file", file_part);

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/audio/transcriptions", base_url.trim_end_matches('/')))
        .header("Authorization", format!("Bearer {}", api_key))
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Whisper API request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Whisper API error ({}): {}", status, body));
    }

    let result: serde_json::Value = resp.json().await
        .map_err(|e| format!("Whisper API JSON parse error: {}", e))?;

    result["text"].as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Whisper API: no text in response".into())
}
fn strip_markdown(text: &str) -> String {
    let mut out = text.to_string();
    // Remove code blocks
    while let Some(start) = out.find("```") {
        if let Some(end) = out[start + 3..].find("```") {
            out.replace_range(start..start + 3 + end + 3, " ");
        } else {
            break;
        }
    }
    // Remove inline code
    out = out.replace('`', "");
    // Remove bold/italic markers
    out = out.replace("**", "").replace("__", "").replace('*', "").replace('_', " ");
    // Remove headers
    out = out.lines().map(|l| {
        let trimmed = l.trim_start();
        if trimmed.starts_with('#') {
            trimmed.trim_start_matches('#').trim_start()
        } else {
            l
        }
    }).collect::<Vec<_>>().join("\n");
    // Remove links [text](url) → text
    let mut result = String::new();
    let mut chars = out.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' {
            let mut link_text = String::new();
            let mut found_close = false;
            for cc in chars.by_ref() {
                if cc == ']' {
                    found_close = true;
                    break;
                }
                link_text.push(cc);
            }
            if found_close {
                // Skip (url) part
                if chars.peek() == Some(&'(') {
                    chars.next();
                    for cc in chars.by_ref() {
                        if cc == ')' { break; }
                    }
                }
                result.push_str(&link_text);
            } else {
                result.push('[');
                result.push_str(&link_text);
            }
        } else {
            result.push(c);
        }
    }
    // Remove bullet points
    result = result.lines().map(|l| {
        let trimmed = l.trim_start();
        if trimmed.starts_with("- ") || trimmed.starts_with("• ") {
            &trimmed[2..]
        } else {
            l
        }
    }).collect::<Vec<_>>().join("\n");
    // Collapse whitespace
    while result.contains("  ") {
        result = result.replace("  ", " ");
    }
    result.trim().to_string()
}

/// Split text into chunks of max `max_bytes` length, breaking at sentence boundaries
fn chunk_text(text: &str, max_bytes: usize) -> Vec<String> {
    if text.len() <= max_bytes {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    for sentence in text.split_inclusive(['.', '!', '?', '\n']) {
        if current.len() + sentence.len() > max_bytes && !current.is_empty() {
            chunks.push(current.clone());
            current.clear();
        }
        current.push_str(sentence);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}
