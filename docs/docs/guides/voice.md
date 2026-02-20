---
sidebar_position: 5
title: Voice & TTS
---

# Voice & TTS

Pawz supports text-to-speech so agents can speak their responses aloud.

## Setup

Go to **Settings → Voice** to configure TTS.

## Providers

### Google Cloud TTS

No API key needed — uses the free web endpoint.

**Chirp 3 HD voices:**
Puck, Charon, Kore, Fenrir, Leda, Orus, Zephyr, Aoede, Callirhoe, Autonoe

**Neural2 voices:**
en-US-Neural2-A through F

**Journey voices:**
en-US-Journey-D, en-US-Journey-F, en-US-Journey-O

### OpenAI TTS

Requires an OpenAI API key.

**Voices:** alloy, ash, coral, echo, fable, nova, onyx, sage, shimmer

### ElevenLabs

Requires an `ELEVENLABS_API_KEY`.

**Voices:** Sarah, Charlie, George, Callum, Liam, Charlotte, Alice, Matilda, Will, Jessica, Eric, Chris, Brian, Daniel, Lily, Bill

**Models:**
| Model | Best for |
|-------|----------|
| `eleven_multilingual_v2` | Multi-language, highest quality |
| `eleven_turbo_v2_5` | Low latency, English-focused |
| `eleven_monolingual_v1` | English only, legacy |

**Extra settings:**
- **Stability** (0–1, default 0.5) — higher = more consistent
- **Similarity boost** (0–1, default 0.75) — higher = closer to reference voice

## Settings

| Setting | Default | Description |
|---------|---------|-------------|
| **Provider** | — | Google / OpenAI / ElevenLabs |
| **Voice** | — | Voice name from the selected provider |
| **Speed** | 1.0 | Playback speed multiplier |
| **Language** | — | Language code (13 supported) |
| **Auto-speak** | Off | Automatically speak every response |

## Talk mode

Click the microphone icon in the chat header to enter talk mode. Your speech is transcribed and sent to the agent, and the response is spoken back.

Requires either:
- **Whisper Local** skill (install `whisper` binary)
- **Whisper API** skill (OpenAI API key)
