// Paw Agent Engine — AI Provider Registry
// AnyProvider wraps Box<dyn AiProvider> so adding a new provider
// never requires modifying the factory enum — just implement the trait.

pub mod anthropic;
pub mod google;
pub mod openai;

pub use anthropic::AnthropicProvider;
pub use google::GoogleProvider;
pub use openai::OpenAiProvider;

use crate::atoms::error::EngineResult;
use crate::atoms::traits::{AiProvider, ModelInfo};
use crate::engine::types::{Message, ProviderConfig, ProviderKind, StreamChunk, ToolDefinition};

// ── Provider factory ───────────────────────────────────────────────────────────

/// Type-erased AI provider.  Callers hold `AnyProvider` and call `.chat_stream()`
/// without knowing which concrete backend is in use.
pub struct AnyProvider(Box<dyn AiProvider>);

impl AnyProvider {
    /// Construct the right concrete provider from a `ProviderConfig`.
    ///
    /// ┌─────────────────────────────────────────────────────────────────┐
    /// │  To add a NEW OpenAI-compatible provider (e.g. DeepSeek):       │
    /// │    • Add the ProviderKind variant.                               │
    /// │    • Add its default_base_url().                                 │
    /// │    • No change needed here — the `_` arm handles it.            │
    /// │                                                                  │
    /// │  To add a provider with a UNIQUE wire format:                   │
    /// │    • Create engine/providers/{name}.rs + impl AiProvider.        │
    /// │    • Add a match arm below.                                      │
    /// └─────────────────────────────────────────────────────────────────┘
    pub fn from_config(config: &ProviderConfig) -> Self {
        let provider: Box<dyn AiProvider> = match config.kind {
            ProviderKind::Anthropic => Box::new(AnthropicProvider::new(config)),
            ProviderKind::Google => Box::new(GoogleProvider::new(config)),
            // Azure AI Foundry hosts heterogeneous models.  If the Target URI
            // contains "/anthropic" it's the Anthropic proxy and needs the
            // native Anthropic wire format (Messages API), not OpenAI's.
            ProviderKind::AzureFoundry
                if config
                    .base_url
                    .as_deref()
                    .is_some_and(|u| u.contains("/anthropic")) =>
            {
                Box::new(AnthropicProvider::new(config))
            }
            // All OpenAI-compatible variants:
            // OpenAI, Ollama, OpenRouter, Custom, DeepSeek, Grok, Mistral, Moonshot
            _ => Box::new(OpenAiProvider::new(config)),
        };
        AnyProvider(provider)
    }

    /// Chat completion with SSE streaming.
    /// Returns `Err(String)` so existing callers in agent_loop.rs / commands.rs
    /// need zero changes.
    pub async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        model: &str,
        temperature: Option<f64>,
        thinking_level: Option<&str>,
    ) -> EngineResult<Vec<StreamChunk>> {
        self.0
            .chat_stream(messages, tools, model, temperature, thinking_level)
            .await
            .map_err(|e| crate::atoms::error::EngineError::Other(e.to_string()))
    }

    /// The ProviderKind discriminant of the underlying provider.
    pub fn kind(&self) -> ProviderKind {
        self.0.kind()
    }

    /// List available models from the provider.
    pub async fn list_models(&self) -> EngineResult<Vec<ModelInfo>> {
        self.0
            .list_models()
            .await
            .map_err(|e| crate::atoms::error::EngineError::Other(e.to_string()))
    }
}
