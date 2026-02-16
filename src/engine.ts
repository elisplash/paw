// Paw Agent Engine — TypeScript Client
// Drop-in alternative to gateway.ts that uses Tauri invoke() instead of WebSocket.
// No network, no port, no auth token, no reconnect logic.

import { invoke } from '@tauri-apps/api/core';

// ── Types matching the Rust engine ─────────────────────────────────────

export interface EngineProviderConfig {
  id: string;
  kind: 'openai' | 'anthropic' | 'google' | 'ollama' | 'openrouter' | 'custom';
  api_key: string;
  base_url?: string;
  default_model?: string;
}

export interface EngineConfig {
  providers: EngineProviderConfig[];
  default_provider?: string;
  default_model?: string;
  default_system_prompt?: string;
  max_tool_rounds: number;
  tool_timeout_secs: number;
}

export interface EngineChatRequest {
  session_id?: string;
  message: string;
  model?: string;
  system_prompt?: string;
  temperature?: number;
  provider_id?: string;
  tools_enabled?: boolean;
  attachments?: Array<{ mimeType: string; content: string }>;
}

export interface EngineChatResponse {
  run_id: string;
  session_id: string;
}

export interface EngineSession {
  id: string;
  label?: string;
  model: string;
  system_prompt?: string;
  created_at: string;
  updated_at: string;
  message_count: number;
}

export interface EngineStoredMessage {
  id: string;
  session_id: string;
  role: string;
  content: string;
  tool_calls_json?: string;
  tool_call_id?: string;
  name?: string;
  created_at: string;
}

export interface EngineEvent {
  kind: 'delta' | 'tool_request' | 'tool_result' | 'complete' | 'error';
  session_id: string;
  run_id: string;
  // delta
  text?: string;
  // tool_request
  tool_call?: { id: string; type: string; function: { name: string; arguments: string } };
  // tool_result
  tool_call_id?: string;
  output?: string;
  success?: boolean;
  // complete
  tool_calls_count?: number;
  usage?: { input_tokens: number; output_tokens: number; total_tokens: number };
  // error
  message?: string;
}

export interface EngineStatus {
  ready: boolean;
  providers: number;
  has_api_key: boolean;
  default_model?: string;
  default_provider?: string;
}

// ── Agent Files (Soul / Persona) ───────────────────────────────────────

export interface EngineAgentFile {
  agent_id: string;
  file_name: string;
  content: string;
  updated_at: string;
}

// ── Memory ─────────────────────────────────────────────────────────────

export interface EngineMemory {
  id: string;
  content: string;
  category: string;
  importance: number;
  created_at: string;
  score?: number;
}

export interface EngineMemoryConfig {
  embedding_base_url: string;
  embedding_model: string;
  embedding_dims: number;
  auto_recall: boolean;
  auto_capture: boolean;
  recall_limit: number;
  recall_threshold: number;
}

export interface EngineMemoryStats {
  total_memories: number;
  categories: [string, number][];
  has_embeddings: boolean;
}

// ── Skills ─────────────────────────────────────────────────────────────

export interface EngineSkillCredentialField {
  key: string;
  label: string;
  description: string;
  required: boolean;
  placeholder: string;
}

export interface EngineSkillStatus {
  id: string;
  name: string;
  description: string;
  icon: string;
  enabled: boolean;
  required_credentials: EngineSkillCredentialField[];
  configured_credentials: string[];
  missing_credentials: string[];
  is_ready: boolean;
  tool_names: string[];
}

// ── Engine Client ──────────────────────────────────────────────────────

class PawEngineClient {
  private _listeners: Map<string, Set<(event: EngineEvent) => void>> = new Map();
  private _tauriUnlisten: (() => void) | null = null;

  /** Start listening for engine events from the Rust backend. */
  async startListening(): Promise<void> {
    if (this._tauriUnlisten) return; // already listening

    const { listen } = await import('@tauri-apps/api/event');
    this._tauriUnlisten = await listen<EngineEvent>('engine-event', (event) => {
      const payload = event.payload;
      // Dispatch to registered listeners
      const handlers = this._listeners.get(payload.kind);
      if (handlers) {
        for (const h of handlers) {
          try { h(payload); } catch (e) { console.error('[engine] Event handler error:', e); }
        }
      }
      // Also dispatch to wildcard listeners
      const wildcardHandlers = this._listeners.get('*');
      if (wildcardHandlers) {
        for (const h of wildcardHandlers) {
          try { h(payload); } catch (e) { console.error('[engine] Wildcard handler error:', e); }
        }
      }
    }) as unknown as () => void;
  }

  /** Register a listener for engine events.
   *  @param kind - Event kind or '*' for all events
   *  @returns Unsubscribe function
   */
  on(kind: string, handler: (event: EngineEvent) => void): () => void {
    if (!this._listeners.has(kind)) {
      this._listeners.set(kind, new Set());
    }
    this._listeners.get(kind)!.add(handler);
    return () => this._listeners.get(kind)?.delete(handler);
  }

  /** Stop listening and clean up. */
  destroy(): void {
    if (this._tauriUnlisten) {
      this._tauriUnlisten();
      this._tauriUnlisten = null;
    }
    this._listeners.clear();
  }

  // ── Chat ─────────────────────────────────────────────────────────────

  /** Send a message and start an agent turn. Results stream via events. */
  async chatSend(request: EngineChatRequest): Promise<EngineChatResponse> {
    return invoke<EngineChatResponse>('engine_chat_send', { request });
  }

  /** Get chat history for a session. */
  async chatHistory(sessionId: string, limit?: number): Promise<EngineStoredMessage[]> {
    return invoke<EngineStoredMessage[]>('engine_chat_history', {
      sessionId,
      limit: limit ?? 200,
    });
  }

  // ── Sessions ─────────────────────────────────────────────────────────

  async sessionsList(limit?: number): Promise<EngineSession[]> {
    return invoke<EngineSession[]>('engine_sessions_list', { limit: limit ?? 50 });
  }

  async sessionRename(sessionId: string, label: string): Promise<void> {
    return invoke('engine_session_rename', { sessionId, label });
  }

  async sessionDelete(sessionId: string): Promise<void> {
    return invoke('engine_session_delete', { sessionId });
  }

  async sessionClear(sessionId: string): Promise<void> {
    return invoke('engine_session_clear', { sessionId });
  }

  // ── Config ───────────────────────────────────────────────────────────

  async getConfig(): Promise<EngineConfig> {
    return invoke<EngineConfig>('engine_get_config');
  }

  async setConfig(config: EngineConfig): Promise<void> {
    return invoke('engine_set_config', { config });
  }

  async upsertProvider(provider: EngineProviderConfig): Promise<void> {
    return invoke('engine_upsert_provider', { provider });
  }

  async removeProvider(providerId: string): Promise<void> {
    return invoke('engine_remove_provider', { providerId });
  }

  /** Check if the engine is configured and ready. */
  async status(): Promise<EngineStatus> {
    return invoke<EngineStatus>('engine_status');
  }

  /** Resolve a pending tool approval (HIL — Human In the Loop). */
  async approveTool(toolCallId: string, approved: boolean): Promise<void> {
    return invoke('engine_approve_tool', { toolCallId, approved });
  }

  // ── Agent Files (Soul / Persona) ─────────────────────────────────────

  async agentFileList(agentId?: string): Promise<EngineAgentFile[]> {
    return invoke<EngineAgentFile[]>('engine_agent_file_list', { agentId: agentId ?? 'default' });
  }

  async agentFileGet(fileName: string, agentId?: string): Promise<EngineAgentFile | null> {
    return invoke<EngineAgentFile | null>('engine_agent_file_get', {
      agentId: agentId ?? 'default',
      fileName,
    });
  }

  async agentFileSet(fileName: string, content: string, agentId?: string): Promise<void> {
    return invoke('engine_agent_file_set', {
      agentId: agentId ?? 'default',
      fileName,
      content,
    });
  }

  async agentFileDelete(fileName: string, agentId?: string): Promise<void> {
    return invoke('engine_agent_file_delete', {
      agentId: agentId ?? 'default',
      fileName,
    });
  }

  // ── Memory ───────────────────────────────────────────────────────────

  async memoryStore(content: string, category?: string, importance?: number): Promise<string> {
    return invoke<string>('engine_memory_store', { content, category, importance });
  }

  async memorySearch(query: string, limit?: number): Promise<EngineMemory[]> {
    return invoke<EngineMemory[]>('engine_memory_search', { query, limit });
  }

  async memoryStats(): Promise<EngineMemoryStats> {
    return invoke<EngineMemoryStats>('engine_memory_stats');
  }

  async memoryDelete(id: string): Promise<void> {
    return invoke('engine_memory_delete', { id });
  }

  async memoryList(limit?: number): Promise<EngineMemory[]> {
    return invoke<EngineMemory[]>('engine_memory_list', { limit });
  }

  async getMemoryConfig(): Promise<EngineMemoryConfig> {
    return invoke<EngineMemoryConfig>('engine_get_memory_config');
  }

  async setMemoryConfig(config: EngineMemoryConfig): Promise<void> {
    return invoke('engine_set_memory_config', { config });
  }

  async testEmbedding(): Promise<number> {
    return invoke<number>('engine_test_embedding');
  }

  // ── Skills (Credential Vault) ────────────────────────────────────────

  async skillsList(): Promise<EngineSkillStatus[]> {
    return invoke<EngineSkillStatus[]>('engine_skills_list');
  }

  async skillSetEnabled(skillId: string, enabled: boolean): Promise<void> {
    return invoke('engine_skill_set_enabled', { skillId, enabled });
  }

  async skillSetCredential(skillId: string, key: string, value: string): Promise<void> {
    return invoke('engine_skill_set_credential', { skillId, key, value });
  }

  async skillDeleteCredential(skillId: string, key: string): Promise<void> {
    return invoke('engine_skill_delete_credential', { skillId, key });
  }

  async skillRevokeAll(skillId: string): Promise<void> {
    return invoke('engine_skill_revoke_all', { skillId });
  }
}

export const pawEngine = new PawEngineClient();
