// Paw Agent Engine — Embedding Client
//
// Calls Ollama or OpenAI-compatible embedding APIs to produce vector
// representations of text. Used by the memory system for semantic search.

use crate::engine::types::*;
use log::{info, warn};
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use crate::atoms::error::EngineResult;

/// Track whether we've already tried to pull the model this session.
static MODEL_PULL_ATTEMPTED: AtomicBool = AtomicBool::new(false);

/// Embedding client — calls Ollama or OpenAI-compatible embedding API.
pub struct EmbeddingClient {
    client: Client,
    base_url: String,
    model: String,
}

impl EmbeddingClient {
    pub fn new(config: &MemoryConfig) -> Self {
        EmbeddingClient {
            client: Client::new(),
            base_url: config.embedding_base_url.clone(),
            model: config.embedding_model.clone(),
        }
    }

    /// Get embedding vector for a text string.
    /// Tries Ollama API format first, falls back to OpenAI format.
    /// On first failure, attempts to auto-pull the model from Ollama.
    pub async fn embed(&self, text: &str) -> EngineResult<Vec<f32>> {
        // Try Ollama format first (new /api/embed endpoint, then legacy /api/embeddings)
        let ollama_result = self.embed_ollama(text).await;
        if let Ok(vec) = ollama_result {
            return Ok(vec);
        }

        let ollama_err = ollama_result.unwrap_err();

        // If model not found, try auto-pulling it (once per session)
        let ollama_err_str = ollama_err.to_string();
        if (ollama_err_str.contains("not found") || ollama_err_str.contains("404") || ollama_err_str.contains("does not exist"))
            && !MODEL_PULL_ATTEMPTED.swap(true, Ordering::SeqCst)
        {
            info!("[memory] Model '{}' not found, attempting auto-pull...", self.model);
            match self.pull_model().await {
                Ok(()) => {
                    info!("[memory] Model '{}' pulled successfully, retrying embed", self.model);
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

        // Try OpenAI-compatible format: POST /v1/embeddings
        let openai_result = self.embed_openai(text).await;
        if let Ok(vec) = openai_result {
            return Ok(vec);
        }

        Err(format!(
            "Embedding failed. Ollama: {} | OpenAI: {}",
            ollama_err,
            openai_result.unwrap_err()
        ).into())
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

        let new_result = self.client.post(&new_url)
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
                            let vec: Vec<f32> = first.iter()
                                .filter_map(|v| v.as_f64().map(|f| f as f32))
                                .collect();
                            if !vec.is_empty() {
                                return Ok(vec);
                            }
                        }
                    }
                    // Some Ollama versions return singular "embedding" even on /api/embed
                    if let Some(embedding) = v["embedding"].as_array() {
                        let vec: Vec<f32> = embedding.iter()
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
                if status.as_u16() == 404 || body.contains("not found") || body.contains("does not exist") {
                    return Err(format!("Model '{}' not found — {}", self.model, body).into());
                }
                info!("[memory] New /api/embed returned {} — trying legacy endpoint", status);
            }
        }

        // ── Fall back to legacy /api/embeddings endpoint ──
        let legacy_url = format!("{}/api/embeddings", self.base_url.trim_end_matches('/'));
        let legacy_body = json!({
            "model": self.model,
            "prompt": text,
        });

        let resp = self.client.post(&legacy_url)
            .json(&legacy_body)
            .timeout(std::time::Duration::from_secs(60))
            .send()
            .await
            .map_err(|e| format!("Ollama not reachable at {} — is Ollama running? Error: {}", self.base_url, e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Ollama embed {} — {}", status, text).into());
        }

        let v: Value = resp.json().await?;

        let embedding = v["embedding"].as_array()
            .ok_or_else(|| "No 'embedding' array in Ollama response".to_string())?;

        let vec: Vec<f32> = embedding.iter()
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

        let resp = self.client.post(&url)
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

        let embedding = v["data"][0]["embedding"].as_array()
            .ok_or_else(|| "No 'data[0].embedding' array in OpenAI response".to_string())?;

        let vec: Vec<f32> = embedding.iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();

        if vec.is_empty() {
            return Err("Empty embedding vector from OpenAI format".into());
        }

        Ok(vec)
    }

    /// Check if the embedding service is reachable and the model works.
    pub async fn test_connection(&self) -> EngineResult<usize> {
        let vec = self.embed("test connection").await?;
        Ok(vec.len())
    }

    /// Check if Ollama is reachable.
    pub async fn check_ollama_running(&self) -> EngineResult<bool> {
        let url = format!("{}/api/tags", self.base_url.trim_end_matches('/'));
        match self.client.get(&url)
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
        let resp = self.client.get(&url)
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

        info!("[memory] Pulling model '{}' from Ollama (this may take a minute)...", self.model);

        let resp = self.client.post(&url)
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

        info!("[memory] Pulling model '{}' from Ollama (streaming)...", self.model);

        let resp = self.client.post(&url)
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
            if line.is_empty() { continue; }
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
