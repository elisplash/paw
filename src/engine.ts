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
  model_routing?: ModelRouting;
}

/** Model routing for multi-agent orchestration.
 *  Lets you assign different models for boss vs worker agents,
 *  per-specialty, or per-agent overrides. */
export interface ModelRouting {
  /** Model for the boss/orchestrator agent (powerful) */
  boss_model?: string;
  /** Default model for worker/sub-agents (cheaper/faster) */
  worker_model?: string;
  /** Per-specialty model overrides: e.g. { coder: 'gemini-2.5-pro' } */
  specialty_models?: Record<string, string>;
  /** Per-agent overrides (highest priority): e.g. { 'agent-123': 'gemini-2.5-pro' } */
  agent_models?: Record<string, string>;
}

export interface EngineChatRequest {
  session_id?: string;
  message: string;
  model?: string;
  system_prompt?: string;
  temperature?: number;
  provider_id?: string;
  tools_enabled?: boolean;
  agent_id?: string;
  /** Per-agent tool filter: only these tools will be available to the AI. */
  tool_filter?: string[];
  attachments?: Array<{ mimeType: string; content: string; name?: string }>;
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
  agent_id?: string;
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
  model?: string;
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

export interface OllamaReadyStatus {
  ollama_running: boolean;
  was_auto_started: boolean;
  model_available: boolean;
  was_auto_pulled: boolean;
  model_name: string;
  embedding_dims: number;
  error: string | null;
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
  category: string;
  enabled: boolean;
  required_credentials: EngineSkillCredentialField[];
  configured_credentials: string[];
  missing_credentials: string[];
  missing_binaries: string[];
  required_env_vars: string[];
  missing_env_vars: string[];
  install_hint: string;
  has_instructions: boolean;
  is_ready: boolean;
  tool_names: string[];
  /** Default instructions from builtin definition */
  default_instructions: string;
  /** Custom user-edited instructions (empty = using defaults) */
  custom_instructions: string;
}

// ── Trading ────────────────────────────────────────────────────────────

export interface TradeRecord {
  id: string;
  trade_type: 'trade' | 'transfer' | 'dex_swap';
  side: string | null;
  product_id: string | null;
  currency: string | null;
  amount: string;
  order_type: string | null;
  order_id: string | null;
  status: string;
  usd_value: string | null;
  to_address: string | null;
  reason: string;
  session_id: string | null;
  agent_id: string | null;
  created_at: string;
}

export interface TradingSummary {
  date: string;
  trade_count: number;
  transfer_count: number;
  dex_swap_count: number;
  buy_total_usd: number;
  sell_total_usd: number;
  transfer_total_usd: number;
  dex_volume_raw: number;
  dex_pairs: string[];
  net_pnl_usd: number;
  daily_spent_usd: number;
}

export interface TradingPolicy {
  auto_approve: boolean;
  max_trade_usd: number;
  max_daily_loss_usd: number;
  allowed_pairs: string[];
  allow_transfers: boolean;
  max_transfer_usd: number;
}

// ── Text-to-Speech ─────────────────────────────────────────────────────

export interface TtsConfig {
  provider: string;        // "google" | "openai"
  voice: string;           // e.g. "en-US-Chirp3-HD-Achernar" or "alloy"
  speed: number;           // 0.25–4.0
  language_code: string;   // e.g. "en-US"
  auto_speak: boolean;     // automatically speak new responses
}

// ── Tasks ──────────────────────────────────────────────────────────────

export type TaskStatus = 'inbox' | 'assigned' | 'in_progress' | 'review' | 'blocked' | 'done';
export type TaskPriority = 'low' | 'medium' | 'high' | 'urgent';

export interface EngineTask {
  id: string;
  title: string;
  description: string;
  status: TaskStatus;
  priority: TaskPriority;
  assigned_agent?: string;        // legacy single agent
  assigned_agents: TaskAgent[];   // multi-agent assignments
  session_id?: string;
  cron_schedule?: string;
  cron_enabled: boolean;
  last_run_at?: string;
  next_run_at?: string;
  created_at: string;
  updated_at: string;
}

export interface TaskAgent {
  agent_id: string;
  role: string;   // 'lead' | 'collaborator'
}

export interface EngineTaskActivity {
  id: string;
  task_id: string;
  kind: string;
  agent?: string;
  content: string;
  created_at: string;
}

// ── Orchestrator: Projects ─────────────────────────────────────────────

export interface EngineProject {
  id: string;
  title: string;
  goal: string;
  status: string;         // planning, running, paused, completed, failed
  boss_agent: string;
  agents: EngineProjectAgent[];
  created_at: string;
  updated_at: string;
}

export interface EngineProjectAgent {
  agent_id: string;
  role: string;           // boss, worker
  specialty: string;      // coder, researcher, designer, communicator, security, general
  status: string;         // idle, working, done, error
  current_task?: string;
  model?: string;         // per-agent model override
  system_prompt?: string; // custom system prompt for this agent
  capabilities?: string[]; // tool names this agent can use
}

export interface EngineProjectMessage {
  id: string;
  project_id: string;
  from_agent: string;
  to_agent?: string;
  kind: string;           // delegation, progress, result, error, message
  content: string;
  metadata?: string;
  created_at: string;
}

/** A backend-created agent (from project_agents table). */
export interface BackendAgent {
  project_id: string;
  agent_id: string;
  role: string;
  specialty: string;
  status: string;
  current_task?: string;
  model?: string;
  system_prompt?: string;
  capabilities?: string[];
}

// ── Telegram ───────────────────────────────────────────────────────────

export interface TelegramConfig {
  bot_token: string;
  enabled: boolean;
  dm_policy: string;
  allowed_users: number[];
  pending_users: TelegramPendingUser[];
  agent_id?: string;
  context_window?: number;
}

export interface TelegramPendingUser {
  user_id: number;
  username: string;
  first_name: string;
  requested_at: string;
}

export interface TelegramStatus {
  running: boolean;
  connected: boolean;
  bot_username?: string;
  bot_name?: string;
  message_count: number;
  last_message_at?: string;
  allowed_users: number[];
  pending_users: TelegramPendingUser[];
  dm_policy: string;
}

// ── Shared Channel Types ───────────────────────────────────────────────

export interface ChannelPendingUser {
  user_id: string;
  username: string;
  display_name: string;
  requested_at: string;
}

export interface ChannelStatus {
  running: boolean;
  connected: boolean;
  bot_name?: string;
  bot_id?: string;
  message_count: number;
  allowed_users: string[];
  pending_users: ChannelPendingUser[];
  dm_policy: string;
}

// ── Discord ────────────────────────────────────────────────────────────

export interface DiscordConfig {
  bot_token: string;
  enabled: boolean;
  dm_policy: string;
  allowed_users: string[];
  pending_users: ChannelPendingUser[];
  agent_id?: string;
  respond_to_mentions: boolean;
}

// ── IRC ────────────────────────────────────────────────────────────────

export interface IrcConfig {
  server: string;
  port: number;
  tls: boolean;
  nick: string;
  password?: string;
  channels_to_join: string[];
  enabled: boolean;
  dm_policy: string;
  allowed_users: string[];
  pending_users: ChannelPendingUser[];
  agent_id?: string;
  respond_in_channels: boolean;
}

// ── Slack ──────────────────────────────────────────────────────────────

export interface SlackConfig {
  bot_token: string;
  app_token: string;
  enabled: boolean;
  dm_policy: string;
  allowed_users: string[];
  pending_users: ChannelPendingUser[];
  agent_id?: string;
  respond_to_mentions: boolean;
}

// ── Matrix ─────────────────────────────────────────────────────────────

export interface MatrixConfig {
  homeserver: string;
  access_token: string;
  enabled: boolean;
  dm_policy: string;
  allowed_users: string[];
  pending_users: ChannelPendingUser[];
  agent_id?: string;
  respond_in_rooms: boolean;
}

// ── Mattermost ─────────────────────────────────────────────────────────

export interface MattermostConfig {
  server_url: string;
  token: string;
  enabled: boolean;
  dm_policy: string;
  allowed_users: string[];
  pending_users: ChannelPendingUser[];
  agent_id?: string;
  respond_to_mentions: boolean;
}

// ── Nextcloud Talk ─────────────────────────────────────────────────────

export interface NextcloudConfig {
  server_url: string;
  username: string;
  password: string;
  enabled: boolean;
  dm_policy: string;
  allowed_users: string[];
  pending_users: ChannelPendingUser[];
  agent_id?: string;
  respond_in_groups: boolean;
}

// ── Nostr ──────────────────────────────────────────────────────────────

export interface NostrConfig {
  private_key_hex: string;
  relays: string[];
  enabled: boolean;
  dm_policy: string;
  allowed_users: string[];
  pending_users: ChannelPendingUser[];
  agent_id?: string;
}

// ── Twitch ─────────────────────────────────────────────────────────────

export interface TwitchConfig {
  oauth_token: string;
  bot_username: string;
  channels_to_join: string[];
  enabled: boolean;
  dm_policy: string;
  allowed_users: string[];
  pending_users: ChannelPendingUser[];
  agent_id?: string;
  require_mention: boolean;
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
  async chatSend(sessionIdOrRequest: string | EngineChatRequest, message?: string): Promise<EngineChatResponse> {
    const request: EngineChatRequest = typeof sessionIdOrRequest === 'string'
      ? { session_id: sessionIdOrRequest, message: message ?? '' }
      : sessionIdOrRequest;
    return invoke<EngineChatResponse>('engine_chat_send', { request });
  }

  /** Abort a running chat (stub — backend support not yet implemented). */
  async chatAbort(_sessionId: string): Promise<void> {
    console.warn('[engine] chatAbort not yet implemented in backend');
  }

  /** Get chat history for a session. */
  async chatHistory(sessionId: string, limit?: number): Promise<EngineStoredMessage[]> {
    return invoke<EngineStoredMessage[]>('engine_chat_history', {
      sessionId,
      limit: limit ?? 200,
    });
  }

  // ── Sessions ─────────────────────────────────────────────────────────

  async sessionsList(limit?: number, agentId?: string): Promise<EngineSession[]> {
    return invoke<EngineSession[]>('engine_sessions_list', { limit: limit ?? 50, agentId: agentId ?? null });
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

  /** Compact a session: summarize old messages and replace them with a summary. */
  async sessionCompact(sessionId: string): Promise<{
    session_id: string;
    messages_before: number;
    messages_after: number;
    tokens_before: number;
    tokens_after: number;
    summary_length: number;
  }> {
    return invoke('engine_session_compact', { sessionId });
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

  /** Auto-detect Ollama on first run and add it as a provider. */
  async autoSetup(): Promise<{ action: string; model?: string; message?: string; available_models?: string[] }> {
    return invoke('engine_auto_setup');
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

  async embeddingStatus(): Promise<{ ollama_running: boolean; model_available: boolean; model_name: string; error?: string }> {
    return invoke('engine_embedding_status');
  }

  async embeddingPullModel(): Promise<string> {
    return invoke<string>('engine_embedding_pull_model');
  }

  async ensureEmbeddingReady(): Promise<OllamaReadyStatus> {
    return invoke<OllamaReadyStatus>('engine_ensure_embedding_ready');
  }

  async memoryBackfill(): Promise<{ success: number; failed: number }> {
    return invoke('engine_memory_backfill');
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

  async skillGetInstructions(skillId: string): Promise<string | null> {
    return invoke<string | null>('engine_skill_get_instructions', { skillId });
  }

  async skillSetInstructions(skillId: string, instructions: string): Promise<void> {
    return invoke('engine_skill_set_instructions', { skillId, instructions });
  }

  // ── Trading ──────────────────────────────────────────────────────────

  async tradingHistory(limit?: number): Promise<TradeRecord[]> {
    return invoke<TradeRecord[]>('engine_trading_history', { limit });
  }

  async tradingSummary(): Promise<TradingSummary> {
    return invoke<TradingSummary>('engine_trading_summary');
  }

  async tradingPolicyGet(): Promise<TradingPolicy> {
    return invoke<TradingPolicy>('engine_trading_policy_get');
  }

  async tradingPolicySet(policy: TradingPolicy): Promise<void> {
    return invoke('engine_trading_policy_set', { policy });
  }

  // ── Text-to-Speech ──────────────────────────────────────────────────

  /** Synthesize speech from text. Returns base64-encoded MP3 audio. */
  async ttsSpeak(text: string): Promise<string> {
    return invoke<string>('engine_tts_speak', { text });
  }

  /** Get TTS config */
  async ttsGetConfig(): Promise<TtsConfig> {
    return invoke<TtsConfig>('engine_tts_get_config');
  }

  /** Save TTS config */
  async ttsSetConfig(config: TtsConfig): Promise<void> {
    return invoke('engine_tts_set_config', { config });
  }

  // ── Tasks (Kanban Board) ─────────────────────────────────────────────

  async tasksList(): Promise<EngineTask[]> {
    return invoke<EngineTask[]>('engine_tasks_list');
  }

  async taskCreate(task: EngineTask): Promise<void> {
    return invoke('engine_task_create', { task });
  }

  async taskUpdate(task: EngineTask): Promise<void> {
    return invoke('engine_task_update', { task });
  }

  async taskDelete(taskId: string): Promise<void> {
    return invoke('engine_task_delete', { taskId });
  }

  async taskMove(taskId: string, newStatus: string): Promise<void> {
    return invoke('engine_task_move', { taskId, newStatus });
  }

  async taskActivity(taskId?: string, limit?: number): Promise<EngineTaskActivity[]> {
    return invoke<EngineTaskActivity[]>('engine_task_activity', { taskId, limit });
  }

  async taskSetAgents(taskId: string, agents: TaskAgent[]): Promise<void> {
    return invoke('engine_task_set_agents', { taskId, agents });
  }

  async taskRun(taskId: string): Promise<string> {
    return invoke<string>('engine_task_run', { taskId });
  }

  async tasksCronTick(): Promise<string[]> {
    return invoke<string[]>('engine_tasks_cron_tick');
  }

  // ── Telegram Bridge ──────────────────────────────────────────────────

  async telegramStart(): Promise<void> {
    return invoke('engine_telegram_start');
  }

  async telegramStop(): Promise<void> {
    return invoke('engine_telegram_stop');
  }

  async telegramStatus(): Promise<TelegramStatus> {
    return invoke<TelegramStatus>('engine_telegram_status');
  }

  async telegramGetConfig(): Promise<TelegramConfig> {
    return invoke<TelegramConfig>('engine_telegram_get_config');
  }

  async telegramSetConfig(config: TelegramConfig): Promise<void> {
    return invoke('engine_telegram_set_config', { config });
  }

  async telegramApproveUser(userId: number): Promise<void> {
    return invoke('engine_telegram_approve_user', { userId });
  }

  async telegramDenyUser(userId: number): Promise<void> {
    return invoke('engine_telegram_deny_user', { userId });
  }

  async telegramRemoveUser(userId: number): Promise<void> {
    return invoke('engine_telegram_remove_user', { userId });
  }

  // ── Discord Bridge ───────────────────────────────────────────────────

  async discordStart(): Promise<void> { return invoke('engine_discord_start'); }
  async discordStop(): Promise<void> { return invoke('engine_discord_stop'); }
  async discordStatus(): Promise<ChannelStatus> { return invoke<ChannelStatus>('engine_discord_status'); }
  async discordGetConfig(): Promise<DiscordConfig> { return invoke<DiscordConfig>('engine_discord_get_config'); }
  async discordSetConfig(config: DiscordConfig): Promise<void> { return invoke('engine_discord_set_config', { config }); }
  async discordApproveUser(userId: string): Promise<void> { return invoke('engine_discord_approve_user', { userId }); }
  async discordDenyUser(userId: string): Promise<void> { return invoke('engine_discord_deny_user', { userId }); }
  async discordRemoveUser(userId: string): Promise<void> { return invoke('engine_discord_remove_user', { userId }); }

  // ── IRC Bridge ───────────────────────────────────────────────────────

  async ircStart(): Promise<void> { return invoke('engine_irc_start'); }
  async ircStop(): Promise<void> { return invoke('engine_irc_stop'); }
  async ircStatus(): Promise<ChannelStatus> { return invoke<ChannelStatus>('engine_irc_status'); }
  async ircGetConfig(): Promise<IrcConfig> { return invoke<IrcConfig>('engine_irc_get_config'); }
  async ircSetConfig(config: IrcConfig): Promise<void> { return invoke('engine_irc_set_config', { config }); }
  async ircApproveUser(userId: string): Promise<void> { return invoke('engine_irc_approve_user', { userId }); }
  async ircDenyUser(userId: string): Promise<void> { return invoke('engine_irc_deny_user', { userId }); }
  async ircRemoveUser(userId: string): Promise<void> { return invoke('engine_irc_remove_user', { userId }); }

  // ── Slack Bridge ─────────────────────────────────────────────────────

  async slackStart(): Promise<void> { return invoke('engine_slack_start'); }
  async slackStop(): Promise<void> { return invoke('engine_slack_stop'); }
  async slackStatus(): Promise<ChannelStatus> { return invoke<ChannelStatus>('engine_slack_status'); }
  async slackGetConfig(): Promise<SlackConfig> { return invoke<SlackConfig>('engine_slack_get_config'); }
  async slackSetConfig(config: SlackConfig): Promise<void> { return invoke('engine_slack_set_config', { config }); }
  async slackApproveUser(userId: string): Promise<void> { return invoke('engine_slack_approve_user', { userId }); }
  async slackDenyUser(userId: string): Promise<void> { return invoke('engine_slack_deny_user', { userId }); }
  async slackRemoveUser(userId: string): Promise<void> { return invoke('engine_slack_remove_user', { userId }); }

  // ── Matrix Bridge ────────────────────────────────────────────────────

  async matrixStart(): Promise<void> { return invoke('engine_matrix_start'); }
  async matrixStop(): Promise<void> { return invoke('engine_matrix_stop'); }
  async matrixStatus(): Promise<ChannelStatus> { return invoke<ChannelStatus>('engine_matrix_status'); }
  async matrixGetConfig(): Promise<MatrixConfig> { return invoke<MatrixConfig>('engine_matrix_get_config'); }
  async matrixSetConfig(config: MatrixConfig): Promise<void> { return invoke('engine_matrix_set_config', { config }); }
  async matrixApproveUser(userId: string): Promise<void> { return invoke('engine_matrix_approve_user', { userId }); }
  async matrixDenyUser(userId: string): Promise<void> { return invoke('engine_matrix_deny_user', { userId }); }
  async matrixRemoveUser(userId: string): Promise<void> { return invoke('engine_matrix_remove_user', { userId }); }

  // ── Mattermost Bridge ────────────────────────────────────────────────

  async mattermostStart(): Promise<void> { return invoke('engine_mattermost_start'); }
  async mattermostStop(): Promise<void> { return invoke('engine_mattermost_stop'); }
  async mattermostStatus(): Promise<ChannelStatus> { return invoke<ChannelStatus>('engine_mattermost_status'); }
  async mattermostGetConfig(): Promise<MattermostConfig> { return invoke<MattermostConfig>('engine_mattermost_get_config'); }
  async mattermostSetConfig(config: MattermostConfig): Promise<void> { return invoke('engine_mattermost_set_config', { config }); }
  async mattermostApproveUser(userId: string): Promise<void> { return invoke('engine_mattermost_approve_user', { userId }); }
  async mattermostDenyUser(userId: string): Promise<void> { return invoke('engine_mattermost_deny_user', { userId }); }
  async mattermostRemoveUser(userId: string): Promise<void> { return invoke('engine_mattermost_remove_user', { userId }); }

  // ── Nextcloud Talk Bridge ────────────────────────────────────────────

  async nextcloudStart(): Promise<void> { return invoke('engine_nextcloud_start'); }
  async nextcloudStop(): Promise<void> { return invoke('engine_nextcloud_stop'); }
  async nextcloudStatus(): Promise<ChannelStatus> { return invoke<ChannelStatus>('engine_nextcloud_status'); }
  async nextcloudGetConfig(): Promise<NextcloudConfig> { return invoke<NextcloudConfig>('engine_nextcloud_get_config'); }
  async nextcloudSetConfig(config: NextcloudConfig): Promise<void> { return invoke('engine_nextcloud_set_config', { config }); }
  async nextcloudApproveUser(userId: string): Promise<void> { return invoke('engine_nextcloud_approve_user', { userId }); }
  async nextcloudDenyUser(userId: string): Promise<void> { return invoke('engine_nextcloud_deny_user', { userId }); }
  async nextcloudRemoveUser(userId: string): Promise<void> { return invoke('engine_nextcloud_remove_user', { userId }); }

  // ── Nostr Bridge ─────────────────────────────────────────────────────

  async nostrStart(): Promise<void> { return invoke('engine_nostr_start'); }
  async nostrStop(): Promise<void> { return invoke('engine_nostr_stop'); }
  async nostrStatus(): Promise<ChannelStatus> { return invoke<ChannelStatus>('engine_nostr_status'); }
  async nostrGetConfig(): Promise<NostrConfig> { return invoke<NostrConfig>('engine_nostr_get_config'); }
  async nostrSetConfig(config: NostrConfig): Promise<void> { return invoke('engine_nostr_set_config', { config }); }
  async nostrApproveUser(userId: string): Promise<void> { return invoke('engine_nostr_approve_user', { userId }); }
  async nostrDenyUser(userId: string): Promise<void> { return invoke('engine_nostr_deny_user', { userId }); }
  async nostrRemoveUser(userId: string): Promise<void> { return invoke('engine_nostr_remove_user', { userId }); }

  // ── Twitch Bridge ────────────────────────────────────────────────────

  async twitchStart(): Promise<void> { return invoke('engine_twitch_start'); }
  async twitchStop(): Promise<void> { return invoke('engine_twitch_stop'); }
  async twitchStatus(): Promise<ChannelStatus> { return invoke<ChannelStatus>('engine_twitch_status'); }
  async twitchGetConfig(): Promise<TwitchConfig> { return invoke<TwitchConfig>('engine_twitch_get_config'); }
  async twitchSetConfig(config: TwitchConfig): Promise<void> { return invoke('engine_twitch_set_config', { config }); }
  async twitchApproveUser(userId: string): Promise<void> { return invoke('engine_twitch_approve_user', { userId }); }
  async twitchDenyUser(userId: string): Promise<void> { return invoke('engine_twitch_deny_user', { userId }); }
  async twitchRemoveUser(userId: string): Promise<void> { return invoke('engine_twitch_remove_user', { userId }); }

  // ── Orchestrator: Projects ───────────────────────────────────────────

  async projectsList(): Promise<EngineProject[]> {
    return invoke<EngineProject[]>('engine_projects_list');
  }

  async projectCreate(project: EngineProject): Promise<void> {
    return invoke('engine_project_create', { project });
  }

  async projectUpdate(project: EngineProject): Promise<void> {
    return invoke('engine_project_update', { project });
  }

  async projectDelete(projectId: string): Promise<void> {
    return invoke('engine_project_delete', { projectId });
  }

  async projectSetAgents(projectId: string, agents: EngineProjectAgent[]): Promise<void> {
    return invoke('engine_project_set_agents', { projectId, agents });
  }

  /** List all agents across all projects (backend-created agents). */
  async listAllAgents(): Promise<BackendAgent[]> {
    return invoke<BackendAgent[]>('engine_list_all_agents');
  }

  /** Create a standalone agent (persisted in SQLite). */
  async createAgent(agent: {
    agent_id: string;
    role: string;
    specialty?: string;
    model?: string;
    system_prompt?: string;
    capabilities?: string[];
  }): Promise<void> {
    return invoke('engine_create_agent', {
      agentId: agent.agent_id,
      role: agent.role,
      specialty: agent.specialty ?? 'general',
      model: agent.model ?? null,
      systemPrompt: agent.system_prompt ?? null,
      capabilities: agent.capabilities ?? [],
    });
  }

  /** Delete a standalone agent by id. */
  async deleteAgent(agentId: string): Promise<void> {
    return invoke('engine_delete_agent', { agentId });
  }

  async projectMessages(projectId: string, limit?: number): Promise<EngineProjectMessage[]> {
    return invoke<EngineProjectMessage[]>('engine_project_messages', { projectId, limit });
  }

  async projectRun(projectId: string): Promise<string> {
    return invoke<string>('engine_project_run', { projectId });
  }
}

export const pawEngine = new PawEngineClient();
