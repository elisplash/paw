"""
OpenAI-compatible embedding provider for Memory Palace.
Drop-in replacement for the Ollama embedding path.

Supports any OpenAI-compatible API: OpenAI, Azure, Foundry,
Together, Fireworks, local vLLM, etc.

Installed by Paw/Claw Desktop. Not part of upstream memory-palace.
"""

import logging
import math
import time
import requests
from typing import List, Optional

logger = logging.getLogger(__name__)

# text-embedding-3-small context: 8191 tokens ≈ 30000 chars
DEFAULT_MAX_EMBEDDING_CHARS = 8000

# Retry configuration
EMBEDDING_MAX_RETRIES = 3
EMBEDDING_RETRY_BASE_DELAY = 1.0  # seconds


def _get_openai_config():
    """Load OpenAI embedding config from memory-palace config."""
    from memory_palace.config import load_config
    config = load_config()
    return {
        "api_key": config.get("openai_api_key") or "",
        "base_url": config.get("openai_base_url", "https://api.openai.com/v1").rstrip("/"),
        "model": config.get("embedding_model") or "text-embedding-3-small",
    }


def _truncate_for_embedding(text: str, max_chars: int = DEFAULT_MAX_EMBEDDING_CHARS) -> str:
    """Truncate text to fit within the embedding model's context window."""
    if len(text) <= max_chars:
        return text
    marker = "\n[TRUNCATED FOR EMBEDDING]"
    truncated = text[:max_chars - len(marker)] + marker
    logger.info(
        "Truncated embedding text from %d to %d chars (limit: %d)",
        len(text), len(truncated), max_chars,
    )
    return truncated


def get_embedding_openai(text: str, model: Optional[str] = None) -> Optional[List[float]]:
    """
    Get embedding vector using an OpenAI-compatible API.

    Compatible with: OpenAI, Azure OpenAI, Foundry, Together, Fireworks,
    local vLLM, LiteLLM proxy, etc.

    Args:
        text: Text to embed
        model: Model override (defaults to config value)

    Returns:
        List of floats, or None on failure
    """
    if not text or not text.strip():
        return None

    text = _truncate_for_embedding(text)

    cfg = _get_openai_config()
    api_key = cfg["api_key"]
    base_url = cfg["base_url"]
    model = model or cfg["model"]

    if not api_key:
        logger.error("OpenAI API key not configured. Set openai_api_key in ~/.memory-palace/config.json")
        return None

    headers = {
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json",
    }

    last_error = None

    for attempt in range(EMBEDDING_MAX_RETRIES):
        try:
            timeout = 30 if attempt == 0 else 60

            response = requests.post(
                f"{base_url}/embeddings",
                headers=headers,
                json={
                    "model": model,
                    "input": text,
                },
                timeout=timeout,
            )

            # Handle rate limits with retry
            if response.status_code == 429:
                retry_after = float(response.headers.get("Retry-After", EMBEDDING_RETRY_BASE_DELAY * (2 ** attempt)))
                logger.warning("Rate limited, retrying in %.1fs", retry_after)
                time.sleep(retry_after)
                continue

            if response.status_code != 200:
                error_body = ""
                try:
                    error_body = response.json().get("error", {}).get("message", response.text[:200])
                except Exception:
                    error_body = response.text[:200]
                last_error = f"HTTP {response.status_code}: {error_body}"
                logger.warning(
                    "OpenAI embedding error (attempt %d/%d): %s",
                    attempt + 1, EMBEDDING_MAX_RETRIES, last_error,
                )
                if attempt < EMBEDDING_MAX_RETRIES - 1:
                    time.sleep(EMBEDDING_RETRY_BASE_DELAY * (2 ** attempt))
                continue

            data = response.json()
            embeddings = data.get("data", [])
            if embeddings and len(embeddings) > 0:
                embedding = embeddings[0].get("embedding")
                if embedding and len(embedding) > 0:
                    if attempt > 0:
                        logger.info("Embedding succeeded on attempt %d/%d", attempt + 1, EMBEDDING_MAX_RETRIES)
                    return embedding

            last_error = "empty embedding in response"
            logger.warning(
                "OpenAI returned empty embedding (attempt %d/%d)",
                attempt + 1, EMBEDDING_MAX_RETRIES,
            )

        except requests.exceptions.ConnectionError as e:
            last_error = f"connection error: {e}"
            logger.warning("OpenAI connection failed (attempt %d/%d): %s", attempt + 1, EMBEDDING_MAX_RETRIES, e)
        except requests.exceptions.Timeout:
            last_error = "timeout"
            logger.warning("OpenAI embedding timed out (attempt %d/%d)", attempt + 1, EMBEDDING_MAX_RETRIES)
        except requests.exceptions.RequestException as e:
            last_error = str(e)
            logger.warning("OpenAI request failed (attempt %d/%d): %s", attempt + 1, EMBEDDING_MAX_RETRIES, e)
        except (KeyError, ValueError) as e:
            last_error = f"malformed response: {e}"
            logger.warning("Malformed OpenAI response (attempt %d/%d): %s", attempt + 1, EMBEDDING_MAX_RETRIES, e)

        if attempt < EMBEDDING_MAX_RETRIES - 1:
            delay = EMBEDDING_RETRY_BASE_DELAY * (2 ** attempt)
            logger.info("Retrying embedding in %.1fs...", delay)
            time.sleep(delay)

    logger.error(
        "OpenAI embedding failed after %d attempts. Last error: %s. Text length: %d chars",
        EMBEDDING_MAX_RETRIES, last_error, len(text),
    )
    return None


def is_openai_available() -> bool:
    """Check if the OpenAI embedding API is reachable with the configured key."""
    cfg = _get_openai_config()
    if not cfg["api_key"]:
        return False
    try:
        response = requests.get(
            f"{cfg['base_url']}/models",
            headers={"Authorization": f"Bearer {cfg['api_key']}"},
            timeout=10,
        )
        return response.status_code == 200
    except requests.exceptions.RequestException:
        return False
