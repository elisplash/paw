"""
Paw embedding provider hook for Memory Palace.

This module patches Memory Palace's embedding system to support
OpenAI-compatible APIs alongside Ollama. Installed by Paw/Claw Desktop.

How it works:
1. Reads `embedding_provider` from ~/.memory-palace/config.json
2. If "openai" → routes through paw_openai_embeddings.get_embedding_openai()
3. If "ollama" or unset → uses original Ollama path (unchanged behavior)

Applied by inserting one import line at the top of embeddings.py.
"""

import logging

logger = logging.getLogger(__name__)

_original_get_embedding = None
_patched = False


def install_hook():
    """
    Monkey-patch memory_palace.embeddings.get_embedding to support
    the configured embedding provider.

    Safe to call multiple times — only patches once.
    """
    global _original_get_embedding, _patched

    if _patched:
        return

    import memory_palace.embeddings as emb_module

    # Save original
    _original_get_embedding = emb_module.get_embedding

    def patched_get_embedding(text, model=None):
        """Route embedding calls based on configured provider."""
        from memory_palace.config import load_config
        config = load_config()
        provider = config.get("embedding_provider", "ollama")

        if provider == "openai":
            from memory_palace.paw_openai_embeddings import get_embedding_openai
            return get_embedding_openai(text, model=model)
        else:
            # Original Ollama path
            return _original_get_embedding(text, model=model)

    # Patch
    emb_module.get_embedding = patched_get_embedding

    # Also patch is_ollama_available for openai mode
    _original_is_ollama = emb_module.is_ollama_available

    def patched_is_ollama_available():
        from memory_palace.config import load_config
        config = load_config()
        provider = config.get("embedding_provider", "ollama")
        if provider == "openai":
            from memory_palace.paw_openai_embeddings import is_openai_available
            return is_openai_available()
        return _original_is_ollama()

    emb_module.is_ollama_available = patched_is_ollama_available

    # Patch get_active_embedding_model for openai mode
    _original_get_active = emb_module.get_active_embedding_model

    def patched_get_active_model():
        from memory_palace.config import load_config
        config = load_config()
        provider = config.get("embedding_provider", "ollama")
        if provider == "openai":
            return config.get("embedding_model") or "text-embedding-3-small"
        return _original_get_active()

    emb_module.get_active_embedding_model = patched_get_active_model

    _patched = True
    logger.info("Paw embedding hook installed (provider routing enabled)")
