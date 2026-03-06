// Paw Agent Engine — Embedding Client
//
// Provider-agnostic embedding layer.  Supports Ollama, OpenAI,
// Google (Gemini), and any OpenAI-compatible endpoint.  The active
// backend is controlled by `EmbeddingProvider` in `MemoryConfig`.
//
// When the provider is `Auto` (default) the legacy cascade is kept:
//   Ollama → OpenAI-at-same-URL → user's chat provider fallback
// so existing setups keep working without configuration changes.

use crate::atoms::error::EngineResult;
use crate::engine::types::*;
use log::{info, warn};
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};

/// Track whether we've already tried to pull the model this session.
static MODEL_PULL_ATTEMPTED: AtomicBool = AtomicBool::new(false);

/// Circuit breaker: once the provider returns "OperationNotSupported" (e.g.
/// Azure chat model that cannot embed), skip the provider fallback for the
/// rest of this process to avoid spamming 400s.
static PROVIDER_EMBED_UNSUPPORTED: AtomicBool = AtomicBool::new(false);

/// Optional fallback to an OpenAI-compatible provider when Ollama is not running.
#[derive(Clone, Debug)]
pub struct OpenAiFallback {
    pub api_key: String,
    pub base_url: String,
    /// Model for embeddings (e.g. "text-embedding-3-small")
    pub embedding_model: String,
    /// Model for classification/PII scanning (e.g. "gpt-4.1-mini")
    pub chat_model: String,
}

/// Embedding client — calls Ollama, OpenAI, Google, or any compatible API.
pub struct EmbeddingClient {
    client: Client,
    /// Which backend to use (auto, ollama, openai, google, provider).
    provider: EmbeddingProvider,
    base_url: String,
    model: String,
    /// If set, used as a fallback when the primary is unreachable.
    openai_fallback: Option<OpenAiFallback>,
}

impl EmbeddingClient {
    pub fn new(config: &MemoryConfig) -> Self {
        EmbeddingClient {
            client: Client::new(),
            provider: config.embedding_provider.clone(),
            base_url: config.embedding_base_url.clone(),
            model: config.embedding_model.clone(),
            openai_fallback: None,
        }
    }

    /// Set an OpenAI-compatible provider as fallback when Ollama is unreachable.
    pub fn with_openai_fallback(mut self, fallback: OpenAiFallback) -> Self {
        self.openai_fallback = Some(fallback);
        self
    }

    /// The model name used for embeddings.
    pub fn model_name(&self) -> &str {
        &self.model
    }

    /// Get embedding vector for a text string.
    ///
    /// Routing depends on `EmbeddingProvider`:
    /// - **Auto** (default): Ollama → OpenAI-at-same-URL → user provider fallback
    /// - **Ollama**: Ollama only, then provider fallback
    /// - **OpenAI**: Direct OpenAI embeddings API, then provider fallback
    /// - **Google**: Google `textembedding-004` via Gemini, then provider fallback
    /// - **Provider**: User's configured chat provider for embeddings
    ///
    /// Every path still ends with the keyword-search safety net in the
    /// memory layer, so we never lose memories.
    pub async fn embed(&self, text: &str) -> EngineResult<Vec<f32>> {
        // Safety truncation: nomic-embed-text context is 8192 tokens (~6K chars).
        // Truncate rather than fail for oversized inputs.
        // Use floor_char_boundary to avoid panicking on multi-byte chars (e.g. em dash —)
        let safe_text: &str = &text[..text.floor_char_boundary(6000)];

        match self.provider {
            EmbeddingProvider::Ollama => self.embed_route_ollama(safe_text).await,
            EmbeddingProvider::OpenAI => self.embed_route_openai(safe_text).await,
            EmbeddingProvider::Google => self.embed_route_google(safe_text).await,
            EmbeddingProvider::Provider => self.embed_route_provider(safe_text).await,
            EmbeddingProvider::Auto => self.embed_route_auto(safe_text).await,
        }
    }

    // ── Route: Auto (legacy cascade) ─────────────────────────────────────
    async fn embed_route_auto(&self, text: &str) -> EngineResult<Vec<f32>> {
        // Try Ollama format first (new /api/embed endpoint, then legacy /api/embeddings)
        let ollama_result = self.embed_ollama(text).await;
        if let Ok(vec) = ollama_result {
            return Ok(vec);
        }

        let ollama_err = ollama_result.unwrap_err();

        // If model not found, try auto-pulling it (once per session)
        let ollama_err_str = ollama_err.to_string();
        if (ollama_err_str.contains("not found")
            || ollama_err_str.contains("404")
            || ollama_err_str.contains("does not exist"))
            && !MODEL_PULL_ATTEMPTED.swap(true, Ordering::SeqCst)
        {
            info!(
                "[memory] Model '{}' not found, attempting auto-pull...",
                self.model
            );
            match self.pull_model().await {
                Ok(()) => {
                    info!(
                        "[memory] Model '{}' pulled successfully, retrying embed",
                        self.model
                    );
                    let retry = self.embed_ollama(text).await;
                    if let Ok(vec) = retry {
                        return Ok(vec);
                    }
                }
                Err(e) => {
                    warn!("[memory] Auto-pull failed: {}", e);
                }
            }
        }

        // Try OpenAI-compatible format at the same base_url: POST /v1/embeddings
        let openai_result = self.embed_openai(text).await;
        if let Ok(vec) = openai_result {
            return Ok(vec);
        }
        let openai_err = openai_result.unwrap_err();

        // ── Fallback: use the user's configured provider ─────────────
        if let Some(ref fb) = self.openai_fallback {
            // Circuit breaker: skip if we already know this provider can't embed
            if PROVIDER_EMBED_UNSUPPORTED.load(Ordering::Relaxed) {
                return Err(format!(
                    "Embedding failed. Ollama: {} | Same-URL OpenAI: {} | Provider fallback: skipped (model does not support embeddings)",
                    ollama_err, openai_err
                ).into());
            }
            info!("[memory] Ollama unavailable, falling back to provider for embeddings");
            let fb_result = self.embed_openai_provider(text, fb).await;
            if let Ok(vec) = fb_result {
                return Ok(vec);
            }
            let fb_err = fb_result.unwrap_err();
            // Detect "OperationNotSupported" and flip the circuit breaker
            let fb_err_str = fb_err.to_string();
            if fb_err_str.contains("OperationNotSupported")
                || fb_err_str.contains("does not work with the specified model")
            {
                warn!("[memory] Provider does not support embeddings — disabling provider fallback for this session");
                PROVIDER_EMBED_UNSUPPORTED.store(true, Ordering::Relaxed);
            }
            return Err(format!(
                "Embedding failed. Ollama: {} | Same-URL OpenAI: {} | Provider fallback: {}",
                ollama_err, openai_err, fb_err
            )
            .into());
        }

        Err(format!(
            "Embedding failed. Ollama: {} | OpenAI: {}",
            ollama_err, openai_err
        )
        .into())
    }

    // ── Route: Ollama only ───────────────────────────────────────────────
    async fn embed_route_ollama(&self, text: &str) -> EngineResult<Vec<f32>> {
        let result = self.embed_ollama(text).await;
        if let Ok(vec) = result {
            return Ok(vec);
        }
        let err = result.unwrap_err();

        // Auto-pull once on model-not-found
        let err_str = err.to_string();
        if (err_str.contains("not found")
            || err_str.contains("404")
            || err_str.contains("does not exist"))
            && !MODEL_PULL_ATTEMPTED.swap(true, Ordering::SeqCst)
        {
            info!(
                "[memory] Model '{}' not found, attempting auto-pull...",
                self.model
            );
            if self.pull_model().await.is_ok() {
                let retry = self.embed_ollama(text).await;
                if let Ok(vec) = retry {
                    return Ok(vec);
                }
            }
        }

        // Fall back to provider if available
        if let Some(ref fb) = self.openai_fallback {
            info!("[memory] Ollama failed, falling back to provider for embeddings");
            return self.embed_openai_provider(text, fb).await;
        }
        Err(err)
    }

    // ── Route: OpenAI direct ─────────────────────────────────────────────
    async fn embed_route_openai(&self, text: &str) -> EngineResult<Vec<f32>> {
        // Use the configured base_url with /v1/embeddings (or provider fallback)
        if let Some(ref fb) = self.openai_fallback {
            let result = self.embed_openai_provider(text, fb).await;
            if result.is_ok() {
                return result;
            }
            warn!(
                "[memory] OpenAI provider embedding failed: {}",
                result.as_ref().unwrap_err()
            );
        }
        // Try base_url as OpenAI-compatible endpoint
        self.embed_openai(text).await
    }

    // ── Route: Google (Gemini) ───────────────────────────────────────────
    async fn embed_route_google(&self, text: &str) -> EngineResult<Vec<f32>> {
        if let Some(ref fb) = self.openai_fallback {
            let result = self.embed_google(text, fb).await;
            if result.is_ok() {
                return result;
            }
            let err = result.unwrap_err();
            warn!("[memory] Google embedding failed: {}", err);
            // Fall back to OpenAI provider format (some Google proxies use it)
            let openai_result = self.embed_openai_provider(text, fb).await;
            if openai_result.is_ok() {
                return openai_result;
            }
            return Err(err);
        }
        Err("Google embedding requires an API key — configure a Google provider".into())
    }

    // ── Route: Use whatever chat provider is configured ──────────────────
    async fn embed_route_provider(&self, text: &str) -> EngineResult<Vec<f32>> {
        if let Some(ref fb) = self.openai_fallback {
            info!("[memory] Using configured chat provider for embeddings");
            return self.embed_openai_provider(text, fb).await;
        }
        Err("No chat provider configured — set up a provider in Settings first".into())
    }

    /// Ollama current API: POST /api/embed { model, input } → { embeddings: [[f32...]] }
    /// Falls back to legacy: POST /api/embeddings { model, prompt } → { embedding: [f32...] }
    async fn embed_ollama(&self, text: &str) -> EngineResult<Vec<f32>> {
        // ── Try new /api/embed endpoint first (Ollama 0.4+) ──
        let new_url = format!("{}/api/embed", self.base_url.trim_end_matches('/'));
        let new_body = json!({
            "model": self.model,
            "input": text,
        });

        let new_result = self
            .client
            .post(&new_url)
            .json(&new_body)
            .timeout(std::time::Duration::from_secs(60))
            .send()
            .await;

        if let Ok(resp) = new_result {
            if resp.status().is_success() {
                if let Ok(v) = resp.json::<Value>().await {
                    // New format returns { embeddings: [[f32...], ...] }
                    if let Some(embeddings) = v["embeddings"].as_array() {
                        if let Some(first) = embeddings.first().and_then(|e| e.as_array()) {
                            let vec: Vec<f32> = first
                                .iter()
                                .filter_map(|v| v.as_f64().map(|f| f as f32))
                                .collect();
                            if !vec.is_empty() {
                                return Ok(vec);
                            }
                        }
                    }
                    // Some Ollama versions return singular "embedding" even on /api/embed
                    if let Some(embedding) = v["embedding"].as_array() {
                        let vec: Vec<f32> = embedding
                            .iter()
                            .filter_map(|v| v.as_f64().map(|f| f as f32))
                            .collect();
                        if !vec.is_empty() {
                            return Ok(vec);
                        }
                    }
                }
            } else {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                if status.as_u16() == 404
                    || body.contains("not found")
                    || body.contains("does not exist")
                {
                    return Err(format!("Model '{}' not found — {}", self.model, body).into());
                }
                info!(
                    "[memory] New /api/embed returned {} — trying legacy endpoint",
                    status
                );
            }
        }

        // ── Fall back to legacy /api/embeddings endpoint ──
        let legacy_url = format!("{}/api/embeddings", self.base_url.trim_end_matches('/'));
        let legacy_body = json!({
            "model": self.model,
            "prompt": text,
        });

        let resp = self
            .client
            .post(&legacy_url)
            .json(&legacy_body)
            .timeout(std::time::Duration::from_secs(60))
            .send()
            .await
            .map_err(|e| {
                format!(
                    "Ollama not reachable at {} — is Ollama running? Error: {}",
                    self.base_url, e
                )
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Ollama embed {} — {}", status, text).into());
        }

        let v: Value = resp.json().await?;

        let embedding = v["embedding"]
            .as_array()
            .ok_or_else(|| "No 'embedding' array in Ollama response".to_string())?;

        let vec: Vec<f32> = embedding
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();

        if vec.is_empty() {
            return Err("Empty embedding vector from Ollama".into());
        }

        Ok(vec)
    }

    /// OpenAI-compatible format: POST /v1/embeddings { model, input }
    async fn embed_openai(&self, text: &str) -> EngineResult<Vec<f32>> {
        let url = format!("{}/v1/embeddings", self.base_url.trim_end_matches('/'));
        let body = json!({
            "model": self.model,
            "input": text,
        });

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("OpenAI embed {} — {}", status, text).into());
        }

        let v: Value = resp.json().await?;

        let embedding = v["data"][0]["embedding"]
            .as_array()
            .ok_or_else(|| "No 'data[0].embedding' array in OpenAI response".to_string())?;

        let vec: Vec<f32> = embedding
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();

        if vec.is_empty() {
            return Err("Empty embedding vector from OpenAI format".into());
        }

        Ok(vec)
    }

    /// Call the user's configured OpenAI provider for embeddings.
    async fn embed_openai_provider(
        &self,
        text: &str,
        fb: &OpenAiFallback,
    ) -> EngineResult<Vec<f32>> {
        let base = fb.base_url.trim_end_matches('/');
        let url = if base.contains(".azure.com") {
            // Azure: embeddings endpoint with api-version
            if base.contains('?') {
                format!("{}/embeddings", base)
            } else {
                format!("{}/embeddings?api-version=2024-05-01-preview", base)
            }
        } else {
            format!("{}/embeddings", base)
        };

        let body = json!({
            "model": fb.embedding_model,
            "input": text,
        });

        let mut req = self
            .client
            .post(&url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(30));

        // Azure uses api-key header; standard OpenAI uses Bearer token
        if fb.base_url.contains(".azure.com") {
            req = req.header("api-key", &fb.api_key);
        } else {
            req = req.bearer_auth(&fb.api_key);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("OpenAI provider embed request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("OpenAI provider embed {} — {}", status, text).into());
        }

        let v: Value = resp.json().await?;
        let embedding = v["data"][0]["embedding"]
            .as_array()
            .ok_or_else(|| "No 'data[0].embedding' in OpenAI provider response".to_string())?;

        let vec: Vec<f32> = embedding
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();

        if vec.is_empty() {
            return Err("Empty embedding vector from OpenAI provider".into());
        }

        info!("[memory] OpenAI provider embedding OK ({} dims)", vec.len());
        Ok(vec)
    }

    /// Google Gemini embedding: POST models/{model}:embedContent
    /// https://ai.google.dev/gemini-api/docs/embeddings
    async fn embed_google(&self, text: &str, fb: &OpenAiFallback) -> EngineResult<Vec<f32>> {
        let model = if fb.embedding_model.is_empty() {
            "text-embedding-004"
        } else {
            &fb.embedding_model
        };
        let base = fb.base_url.trim_end_matches('/');
        let url = format!("{}/models/{}:embedContent?key={}", base, model, fb.api_key);

        let body = json!({
            "model": format!("models/{}", model),
            "content": {
                "parts": [{ "text": text }]
            }
        });

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| format!("Google embed request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            return Err(format!("Google embed {} — {}", status, body_text).into());
        }

        let v: Value = resp.json().await?;
        let values = v["embedding"]["values"]
            .as_array()
            .ok_or_else(|| "No 'embedding.values' in Google response".to_string())?;

        let vec: Vec<f32> = values
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();

        if vec.is_empty() {
            return Err("Empty embedding vector from Google".into());
        }

        info!("[memory] Google embedding OK ({} dims)", vec.len());
        Ok(vec)
    }

    /// Check if the embedding service is reachable and the model works.
    pub async fn test_connection(&self) -> EngineResult<usize> {
        let vec = self.embed("test connection").await?;
        Ok(vec.len())
    }

    /// Send a classification prompt to the LLM via Ollama generate endpoint.
    /// Used for Layer 2 PII scanning during consolidation.
    /// Returns the raw text response from the model.
    pub async fn classify_text(&self, prompt: &str) -> EngineResult<String> {
        // Try Ollama /api/generate endpoint
        let url = format!("{}/api/generate", self.base_url.trim_end_matches('/'));
        let body = json!({
            "model": self.model,
            "prompt": prompt,
            "stream": false,
            "options": {
                "temperature": 0.0,
                "num_predict": 256,
            }
        });

        let ollama_result = self
            .client
            .post(&url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await;

        match ollama_result {
            Ok(resp) if resp.status().is_success() => {
                let v: Value = resp.json().await?;
                let response_text = v["response"].as_str().unwrap_or("").trim().to_string();
                if !response_text.is_empty() {
                    return Ok(response_text);
                }
            }
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                warn!("[memory] Ollama classify returned {} — {}", status, text);
            }
            Err(e) => {
                warn!("[memory] Ollama classify unreachable: {}", e);
            }
        }

        // ── Fallback: use the user's configured OpenAI provider ──────
        if let Some(ref fb) = self.openai_fallback {
            info!("[memory] Ollama unavailable, falling back to OpenAI provider for PII classify");
            return self.classify_text_openai(prompt, fb).await;
        }

        Err(
            "LLM classify request failed: Ollama not reachable and no provider fallback configured"
                .into(),
        )
    }

    /// Classify text using the user's configured OpenAI-compatible provider.
    async fn classify_text_openai(
        &self,
        prompt: &str,
        fb: &OpenAiFallback,
    ) -> EngineResult<String> {
        let base = fb.base_url.trim_end_matches('/');
        let url = if base.contains(".azure.com") {
            if base.contains('?') {
                format!("{}/chat/completions", base)
            } else {
                format!("{}/chat/completions?api-version=2024-05-01-preview", base)
            }
        } else {
            format!("{}/chat/completions", base)
        };

        let body = json!({
            "model": fb.chat_model,
            "messages": [
                { "role": "user", "content": prompt }
            ],
            "temperature": 0.0,
            "max_tokens": 256,
        });

        let mut req = self
            .client
            .post(&url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(30));

        if fb.base_url.contains(".azure.com") {
            req = req.header("api-key", &fb.api_key);
        } else {
            req = req.bearer_auth(&fb.api_key);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("OpenAI provider classify failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("OpenAI provider classify {} — {}", status, text).into());
        }

        let v: Value = resp.json().await?;
        let text = v["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string();

        if text.is_empty() {
            return Err("Empty response from OpenAI provider classify".into());
        }

        Ok(text)
    }

    /// Check if Ollama is reachable.
    pub async fn check_ollama_running(&self) -> EngineResult<bool> {
        let url = format!("{}/api/tags", self.base_url.trim_end_matches('/'));
        match self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    /// Check if the configured model is available in Ollama.
    pub async fn check_model_available(&self) -> EngineResult<bool> {
        let url = format!("{}/api/tags", self.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err("Ollama returned an error".into());
        }

        let v: Value = resp.json().await?;

        if let Some(models) = v["models"].as_array() {
            let model_base = self.model.split(':').next().unwrap_or(&self.model);
            for m in models {
                if let Some(name) = m["name"].as_str() {
                    let name_base = name.split(':').next().unwrap_or(name);
                    if name_base == model_base || name == self.model {
                        return Ok(true);
                    }
                }
                if let Some(name) = m["model"].as_str() {
                    let name_base = name.split(':').next().unwrap_or(name);
                    if name_base == model_base || name == self.model {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }

    /// Pull a model from Ollama. Blocks until download completes.
    pub async fn pull_model(&self) -> EngineResult<()> {
        let url = format!("{}/api/pull", self.base_url.trim_end_matches('/'));
        let body = json!({
            "name": self.model,
            "stream": false,
        });

        info!(
            "[memory] Pulling model '{}' from Ollama (this may take a minute)...",
            self.model
        );

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(600))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Pull failed {} — {}", status, text).into());
        }

        let v: Value = resp.json().await.unwrap_or(json!({}));
        let status = v["status"].as_str().unwrap_or("unknown");
        info!("[memory] Model pull complete: {}", status);
        Ok(())
    }

    /// Pull a model from Ollama with streaming progress.
    /// Calls `on_progress` with (status, completed_bytes, total_bytes) for each update.
    pub async fn pull_model_streaming<F>(&self, mut on_progress: F) -> EngineResult<()>
    where
        F: FnMut(&str, u64, u64),
    {
        let url = format!("{}/api/pull", self.base_url.trim_end_matches('/'));
        let body = json!({
            "name": self.model,
            "stream": true,
        });

        info!(
            "[memory] Pulling model '{}' from Ollama (streaming)...",
            self.model
        );

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .timeout(std::time::Duration::from_secs(600))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Pull failed {} — {}", status, text).into());
        }

        let body_text = resp.text().await?;
        for line in body_text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(line) {
                let status = v["status"].as_str().unwrap_or("downloading");
                let completed = v["completed"].as_u64().unwrap_or(0);
                let total = v["total"].as_u64().unwrap_or(0);
                on_progress(status, completed, total);
            }
        }

        info!("[memory] Model '{}' pull complete", self.model);
        Ok(())
    }
}
