// Paw Agent Engine — AI Provider Registry
// AnyProvider wraps Box<dyn AiProvider> so adding a new provider
// never requires modifying the factory enum — just implement the trait.

pub mod openai;
pub mod anthropic;
pub mod google;

pub use openai::OpenAiProvider;
pub use anthropic::AnthropicProvider;
pub use google::GoogleProvider;

use crate::engine::types::{Message, ToolDefinition, StreamChunk, ProviderConfig, ProviderKind};
use crate::atoms::traits::AiProvider;
use crate::atoms::error::EngineResult;

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
            ProviderKind::Google    => Box::new(GoogleProvider::new(config)),
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
    ) -> EngineResult<Vec<StreamChunk>> {
        self.0
            .chat_stream(messages, tools, model, temperature)
            .await
            
    }

    /// The ProviderKind discriminant of the underlying provider.
    pub fn kind(&self) -> ProviderKind {
        self.0.kind()
    }
}
