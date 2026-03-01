// ── Engram: Model Capability Registry ───────────────────────────────────────
//
// Per-model capability fingerprints that eliminate ALL hardcoded model limits.
// Replaces:
//   - anthropic.rs: hardcoded max_tokens = 4096/8192
//   - agent.rs: hardcoded 16K context cap for channel agents
//   - types.rs: global default_context_window_tokens = 32_000
//   - state/index.ts: MODEL_CONTEXT_SIZES (frontend-only, display-only)
//
// Resolution strategy:
//   1. Try exact model name match
//   2. Try prefix match (handles date-suffixed IDs like claude-opus-4-6-20260115)
//   3. Fall back to conservative defaults

use crate::atoms::engram_types::{ModelCapabilities, ModelProvider, TokenizerType};
use std::sync::LazyLock;

/// Normalize a model name for matching.
/// Strips common suffixes (dates, preview tags) and lowercases.
pub fn normalize_model_name(model: &str) -> String {
    model
        .to_lowercase()
        .trim()
        // Strip date suffixes like -20250514, -20260115
        .trim_end_matches(|c: char| c == '-' || c.is_ascii_digit())
        // Strip common suffixes
        .trim_end_matches("-preview")
        .trim_end_matches("-latest")
        .trim_end_matches("-exp")
        .to_string()
}

/// Resolve model name to full capabilities.
///
/// This is the SINGLE source of truth for all model-specific parameters.
/// Every execution path (chat, tasks, orchestrator, swarm, flows, channels)
/// must use this instead of hardcoded values.
pub fn resolve_model_capabilities(model: &str) -> ModelCapabilities {
    let norm = normalize_model_name(model);

    // Try exact match first, then prefix match
    if let Some(caps) = try_exact_match(&norm) {
        return caps;
    }
    if let Some(caps) = try_prefix_match(&norm) {
        return caps;
    }

    // Unknown model — conservative defaults
    ModelCapabilities::default()
}

/// Convenience: get just the context window size for a model.
/// Replaces all `cfg.context_window_tokens` reads.
pub fn resolve_context_window(model: &str, fallback: usize) -> usize {
    let caps = resolve_model_capabilities(model);
    if caps.provider == ModelProvider::Unknown {
        fallback // use the user's configured fallback for truly unknown models
    } else {
        caps.context_window
    }
}

/// Convenience: get the max output tokens for a model.
/// Replaces hardcoded max_tokens in anthropic.rs.
pub fn resolve_max_output_tokens(model: &str) -> usize {
    resolve_model_capabilities(model).max_output_tokens
}

// ── Internal: Model Registry ────────────────────────────────────────────

/// A registry entry. Using a struct array instead of HashMap for prefix matching.
struct ModelEntry {
    prefix: &'static str,
    caps: ModelCapabilities,
}

/// The master registry. Ordered by specificity (longer prefixes first).
static REGISTRY: LazyLock<Vec<ModelEntry>> = LazyLock::new(|| {
    vec![
        // ═══════════════════════════════════════════════════════════════
        // Anthropic / Claude
        // ═══════════════════════════════════════════════════════════════
        ModelEntry {
            prefix: "claude-opus-4-6",
            caps: ModelCapabilities {
                context_window: 200_000,
                max_output_tokens: 32_768,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: true,
                supports_streaming: true,
                tokenizer: TokenizerType::Cl100kBase,
                rate_limit_rpm: Some(60),
                provider: ModelProvider::Anthropic,
            },
        },
        ModelEntry {
            prefix: "claude-sonnet-4-6",
            caps: ModelCapabilities {
                context_window: 200_000,
                max_output_tokens: 16_384,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: true,
                supports_streaming: true,
                tokenizer: TokenizerType::Cl100kBase,
                rate_limit_rpm: Some(60),
                provider: ModelProvider::Anthropic,
            },
        },
        ModelEntry {
            prefix: "claude-sonnet-4-5",
            caps: ModelCapabilities {
                context_window: 200_000,
                max_output_tokens: 16_384,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: true,
                supports_streaming: true,
                tokenizer: TokenizerType::Cl100kBase,
                rate_limit_rpm: Some(60),
                provider: ModelProvider::Anthropic,
            },
        },
        ModelEntry {
            prefix: "claude-haiku-4",
            caps: ModelCapabilities {
                context_window: 200_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::Cl100kBase,
                rate_limit_rpm: Some(120),
                provider: ModelProvider::Anthropic,
            },
        },
        ModelEntry {
            prefix: "claude-opus-4",
            caps: ModelCapabilities {
                context_window: 200_000,
                max_output_tokens: 32_768,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: true,
                supports_streaming: true,
                tokenizer: TokenizerType::Cl100kBase,
                rate_limit_rpm: Some(60),
                provider: ModelProvider::Anthropic,
            },
        },
        ModelEntry {
            prefix: "claude-sonnet-4",
            caps: ModelCapabilities {
                context_window: 200_000,
                max_output_tokens: 16_384,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: true,
                supports_streaming: true,
                tokenizer: TokenizerType::Cl100kBase,
                rate_limit_rpm: Some(60),
                provider: ModelProvider::Anthropic,
            },
        },
        ModelEntry {
            prefix: "claude-3-5-sonnet",
            caps: ModelCapabilities {
                context_window: 200_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::Cl100kBase,
                rate_limit_rpm: Some(60),
                provider: ModelProvider::Anthropic,
            },
        },
        ModelEntry {
            prefix: "claude-3-5-haiku",
            caps: ModelCapabilities {
                context_window: 200_000,
                max_output_tokens: 4_096,
                supports_tools: true,
                supports_vision: false,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::Cl100kBase,
                rate_limit_rpm: Some(120),
                provider: ModelProvider::Anthropic,
            },
        },
        ModelEntry {
            prefix: "claude-3-opus",
            caps: ModelCapabilities {
                context_window: 200_000,
                max_output_tokens: 4_096,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::Cl100kBase,
                rate_limit_rpm: Some(30),
                provider: ModelProvider::Anthropic,
            },
        },
        // Catch-all for any claude- prefix not matched above
        ModelEntry {
            prefix: "claude-",
            caps: ModelCapabilities {
                context_window: 200_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::Cl100kBase,
                rate_limit_rpm: Some(60),
                provider: ModelProvider::Anthropic,
            },
        },
        // ═══════════════════════════════════════════════════════════════
        // OpenAI / Codex
        // ═══════════════════════════════════════════════════════════════
        ModelEntry {
            prefix: "codex-5",
            caps: ModelCapabilities {
                context_window: 256_000,
                max_output_tokens: 65_536,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: true,
                supports_streaming: true,
                tokenizer: TokenizerType::O200kBase,
                rate_limit_rpm: Some(100),
                provider: ModelProvider::OpenAI,
            },
        },
        ModelEntry {
            prefix: "o4-mini",
            caps: ModelCapabilities {
                context_window: 200_000,
                max_output_tokens: 100_000,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: true,
                supports_streaming: true,
                tokenizer: TokenizerType::O200kBase,
                rate_limit_rpm: Some(100),
                provider: ModelProvider::OpenAI,
            },
        },
        ModelEntry {
            prefix: "o3-mini",
            caps: ModelCapabilities {
                context_window: 200_000,
                max_output_tokens: 100_000,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: true,
                supports_streaming: true,
                tokenizer: TokenizerType::O200kBase,
                rate_limit_rpm: Some(100),
                provider: ModelProvider::OpenAI,
            },
        },
        ModelEntry {
            prefix: "o3",
            caps: ModelCapabilities {
                context_window: 200_000,
                max_output_tokens: 100_000,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: true,
                supports_streaming: true,
                tokenizer: TokenizerType::O200kBase,
                rate_limit_rpm: Some(30),
                provider: ModelProvider::OpenAI,
            },
        },
        ModelEntry {
            prefix: "o1-pro",
            caps: ModelCapabilities {
                context_window: 200_000,
                max_output_tokens: 100_000,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: true,
                supports_streaming: true,
                tokenizer: TokenizerType::O200kBase,
                rate_limit_rpm: Some(10),
                provider: ModelProvider::OpenAI,
            },
        },
        ModelEntry {
            prefix: "o1-mini",
            caps: ModelCapabilities {
                context_window: 128_000,
                max_output_tokens: 65_536,
                supports_tools: true,
                supports_vision: false,
                supports_extended_thinking: true,
                supports_streaming: true,
                tokenizer: TokenizerType::O200kBase,
                rate_limit_rpm: Some(100),
                provider: ModelProvider::OpenAI,
            },
        },
        ModelEntry {
            prefix: "o1",
            caps: ModelCapabilities {
                context_window: 200_000,
                max_output_tokens: 100_000,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: true,
                supports_streaming: true,
                tokenizer: TokenizerType::O200kBase,
                rate_limit_rpm: Some(30),
                provider: ModelProvider::OpenAI,
            },
        },
        ModelEntry {
            prefix: "gpt-4o-mini",
            caps: ModelCapabilities {
                context_window: 128_000,
                max_output_tokens: 16_384,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::O200kBase,
                rate_limit_rpm: Some(500),
                provider: ModelProvider::OpenAI,
            },
        },
        ModelEntry {
            prefix: "gpt-4o",
            caps: ModelCapabilities {
                context_window: 128_000,
                max_output_tokens: 16_384,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::O200kBase,
                rate_limit_rpm: Some(500),
                provider: ModelProvider::OpenAI,
            },
        },
        ModelEntry {
            prefix: "gpt-4-turbo",
            caps: ModelCapabilities {
                context_window: 128_000,
                max_output_tokens: 4_096,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::Cl100kBase,
                rate_limit_rpm: Some(500),
                provider: ModelProvider::OpenAI,
            },
        },
        ModelEntry {
            prefix: "gpt-4",
            caps: ModelCapabilities {
                context_window: 8_192,
                max_output_tokens: 4_096,
                supports_tools: true,
                supports_vision: false,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::Cl100kBase,
                rate_limit_rpm: Some(500),
                provider: ModelProvider::OpenAI,
            },
        },
        ModelEntry {
            prefix: "gpt-3.5-turbo",
            caps: ModelCapabilities {
                context_window: 16_384,
                max_output_tokens: 4_096,
                supports_tools: true,
                supports_vision: false,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::Cl100kBase,
                rate_limit_rpm: Some(1000),
                provider: ModelProvider::OpenAI,
            },
        },
        // ═══════════════════════════════════════════════════════════════
        // Google / Gemini
        // ═══════════════════════════════════════════════════════════════
        ModelEntry {
            prefix: "gemini-3.1-pro",
            caps: ModelCapabilities {
                context_window: 2_097_152,
                max_output_tokens: 65_536,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: true,
                supports_streaming: true,
                tokenizer: TokenizerType::Gemini,
                rate_limit_rpm: Some(60),
                provider: ModelProvider::Google,
            },
        },
        ModelEntry {
            prefix: "gemini-3-pro",
            caps: ModelCapabilities {
                context_window: 1_048_576,
                max_output_tokens: 65_536,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: true,
                supports_streaming: true,
                tokenizer: TokenizerType::Gemini,
                rate_limit_rpm: Some(60),
                provider: ModelProvider::Google,
            },
        },
        ModelEntry {
            prefix: "gemini-3-flash",
            caps: ModelCapabilities {
                context_window: 1_048_576,
                max_output_tokens: 65_536,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::Gemini,
                rate_limit_rpm: Some(120),
                provider: ModelProvider::Google,
            },
        },
        ModelEntry {
            prefix: "gemini-2.5-pro",
            caps: ModelCapabilities {
                context_window: 1_048_576,
                max_output_tokens: 65_536,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: true,
                supports_streaming: true,
                tokenizer: TokenizerType::Gemini,
                rate_limit_rpm: Some(60),
                provider: ModelProvider::Google,
            },
        },
        ModelEntry {
            prefix: "gemini-2.5-flash",
            caps: ModelCapabilities {
                context_window: 1_048_576,
                max_output_tokens: 65_536,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::Gemini,
                rate_limit_rpm: Some(120),
                provider: ModelProvider::Google,
            },
        },
        ModelEntry {
            prefix: "gemini-2.0-flash",
            caps: ModelCapabilities {
                context_window: 1_048_576,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::Gemini,
                rate_limit_rpm: Some(120),
                provider: ModelProvider::Google,
            },
        },
        // Catch-all for gemini-
        ModelEntry {
            prefix: "gemini-",
            caps: ModelCapabilities {
                context_window: 1_048_576,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::Gemini,
                rate_limit_rpm: Some(60),
                provider: ModelProvider::Google,
            },
        },
        // ═══════════════════════════════════════════════════════════════
        // DeepSeek
        // ═══════════════════════════════════════════════════════════════
        ModelEntry {
            prefix: "deepseek-reasoner",
            caps: ModelCapabilities {
                context_window: 128_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: false,
                supports_extended_thinking: true,
                supports_streaming: true,
                tokenizer: TokenizerType::SentencePiece,
                rate_limit_rpm: Some(60),
                provider: ModelProvider::DeepSeek,
            },
        },
        ModelEntry {
            prefix: "deepseek-chat",
            caps: ModelCapabilities {
                context_window: 128_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: false,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::SentencePiece,
                rate_limit_rpm: Some(60),
                provider: ModelProvider::DeepSeek,
            },
        },
        ModelEntry {
            prefix: "deepseek-",
            caps: ModelCapabilities {
                context_window: 128_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: false,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::SentencePiece,
                rate_limit_rpm: Some(60),
                provider: ModelProvider::DeepSeek,
            },
        },
        // ═══════════════════════════════════════════════════════════════
        // Mistral
        // ═══════════════════════════════════════════════════════════════
        ModelEntry {
            prefix: "mistral-large",
            caps: ModelCapabilities {
                context_window: 128_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: false,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::SentencePiece,
                rate_limit_rpm: Some(60),
                provider: ModelProvider::Mistral,
            },
        },
        ModelEntry {
            prefix: "mixtral",
            caps: ModelCapabilities {
                context_window: 32_000,
                max_output_tokens: 4_096,
                supports_tools: true,
                supports_vision: false,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::SentencePiece,
                rate_limit_rpm: None,
                provider: ModelProvider::Mistral,
            },
        },
        ModelEntry {
            prefix: "mistral",
            caps: ModelCapabilities {
                context_window: 32_000,
                max_output_tokens: 4_096,
                supports_tools: true,
                supports_vision: false,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::SentencePiece,
                rate_limit_rpm: None,
                provider: ModelProvider::Mistral,
            },
        },
        // ═══════════════════════════════════════════════════════════════
        // xAI / Grok
        // ═══════════════════════════════════════════════════════════════
        ModelEntry {
            prefix: "grok-3",
            caps: ModelCapabilities {
                context_window: 131_072,
                max_output_tokens: 16_384,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: true,
                supports_streaming: true,
                tokenizer: TokenizerType::SentencePiece,
                rate_limit_rpm: Some(60),
                provider: ModelProvider::XAI,
            },
        },
        ModelEntry {
            prefix: "grok-2",
            caps: ModelCapabilities {
                context_window: 131_072,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: true,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::SentencePiece,
                rate_limit_rpm: Some(60),
                provider: ModelProvider::XAI,
            },
        },
        ModelEntry {
            prefix: "grok-",
            caps: ModelCapabilities {
                context_window: 131_072,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: false,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::SentencePiece,
                rate_limit_rpm: Some(60),
                provider: ModelProvider::XAI,
            },
        },
        // ═══════════════════════════════════════════════════════════════
        // Local / Ollama models
        // ═══════════════════════════════════════════════════════════════
        ModelEntry {
            prefix: "llama-4",
            caps: ModelCapabilities {
                context_window: 128_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: false,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::SentencePiece,
                rate_limit_rpm: None,
                provider: ModelProvider::Ollama,
            },
        },
        ModelEntry {
            prefix: "llama-3",
            caps: ModelCapabilities {
                context_window: 128_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: false,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::SentencePiece,
                rate_limit_rpm: None,
                provider: ModelProvider::Ollama,
            },
        },
        ModelEntry {
            prefix: "llama3.2",
            caps: ModelCapabilities {
                context_window: 8_192,
                max_output_tokens: 2_048,
                supports_tools: false,
                supports_vision: false,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::SentencePiece,
                rate_limit_rpm: None,
                provider: ModelProvider::Ollama,
            },
        },
        ModelEntry {
            prefix: "llama3.1",
            caps: ModelCapabilities {
                context_window: 128_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: false,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::SentencePiece,
                rate_limit_rpm: None,
                provider: ModelProvider::Ollama,
            },
        },
        ModelEntry {
            prefix: "qwen2.5",
            caps: ModelCapabilities {
                context_window: 128_000,
                max_output_tokens: 8_192,
                supports_tools: true,
                supports_vision: false,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::SentencePiece,
                rate_limit_rpm: None,
                provider: ModelProvider::Ollama,
            },
        },
        ModelEntry {
            prefix: "qwen",
            caps: ModelCapabilities {
                context_window: 32_000,
                max_output_tokens: 4_096,
                supports_tools: true,
                supports_vision: false,
                supports_extended_thinking: false,
                supports_streaming: true,
                tokenizer: TokenizerType::SentencePiece,
                rate_limit_rpm: None,
                provider: ModelProvider::Ollama,
            },
        },
    ]
});

fn try_exact_match(normalized: &str) -> Option<ModelCapabilities> {
    REGISTRY
        .iter()
        .find(|e| e.prefix == normalized)
        .map(|e| e.caps.clone())
}

fn try_prefix_match(normalized: &str) -> Option<ModelCapabilities> {
    // Registry is ordered with longer/more specific prefixes first,
    // so the first prefix match is the best one.
    REGISTRY
        .iter()
        .find(|e| normalized.starts_with(e.prefix))
        .map(|e| e.caps.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opus_4_6() {
        let caps = resolve_model_capabilities("claude-opus-4-6");
        assert_eq!(caps.context_window, 200_000);
        assert_eq!(caps.max_output_tokens, 32_768);
        assert!(caps.supports_extended_thinking);
        assert_eq!(caps.provider, ModelProvider::Anthropic);
    }

    #[test]
    fn test_opus_4_6_with_date_suffix() {
        let caps = resolve_model_capabilities("claude-opus-4-6-20260115");
        assert_eq!(caps.context_window, 200_000);
        assert_eq!(caps.provider, ModelProvider::Anthropic);
    }

    #[test]
    fn test_codex_5_3() {
        let caps = resolve_model_capabilities("codex-5.3");
        assert_eq!(caps.context_window, 256_000);
        assert_eq!(caps.max_output_tokens, 65_536);
        assert_eq!(caps.provider, ModelProvider::OpenAI);
    }

    #[test]
    fn test_gemini_3_1_pro() {
        let caps = resolve_model_capabilities("gemini-3.1-pro-preview");
        assert_eq!(caps.context_window, 2_097_152);
        assert!(caps.supports_vision);
        assert_eq!(caps.provider, ModelProvider::Google);
    }

    #[test]
    fn test_gpt4_small_context() {
        let caps = resolve_model_capabilities("gpt-4");
        assert_eq!(caps.context_window, 8_192);
        assert_eq!(caps.max_output_tokens, 4_096);
    }

    #[test]
    fn test_unknown_model_defaults() {
        let caps = resolve_model_capabilities("some-random-model");
        assert_eq!(caps.context_window, 32_000);
        assert_eq!(caps.max_output_tokens, 4_096);
        assert_eq!(caps.provider, ModelProvider::Unknown);
    }

    #[test]
    fn test_normalize_strips_date() {
        assert_eq!(
            normalize_model_name("claude-opus-4-6-20260115"),
            "claude-opus-4-6"
        );
    }

    #[test]
    fn test_normalize_strips_preview() {
        assert_eq!(
            normalize_model_name("gemini-3.1-pro-preview"),
            "gemini-3.1-pro"
        );
    }

    #[test]
    fn test_resolve_context_window_convenience() {
        assert_eq!(resolve_context_window("claude-opus-4-6", 32_000), 200_000);
        assert_eq!(resolve_context_window("unknown-model", 32_000), 32_000);
    }

    #[test]
    fn test_resolve_max_output() {
        assert_eq!(resolve_max_output_tokens("claude-opus-4-6"), 32_768);
        assert_eq!(resolve_max_output_tokens("gpt-4o"), 16_384);
    }

    #[test]
    fn test_local_models() {
        let caps = resolve_model_capabilities("llama-4");
        assert_eq!(caps.context_window, 128_000);
        assert_eq!(caps.provider, ModelProvider::Ollama);
        assert!(caps.rate_limit_rpm.is_none());
    }

    #[test]
    fn test_deepseek() {
        let caps = resolve_model_capabilities("deepseek-reasoner");
        assert_eq!(caps.context_window, 128_000);
        assert!(caps.supports_extended_thinking);
        assert_eq!(caps.provider, ModelProvider::DeepSeek);
    }
}
