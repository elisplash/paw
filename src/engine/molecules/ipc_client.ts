// src/engine/molecules/ipc_client.ts
// PawEngineClient — wraps every Tauri invoke() call for the engine.
// Extracted from engine.ts.

import { invoke } from '@tauri-apps/api/core';
import type {
  EngineConfig,
  EngineProviderConfig,
  EngineChatRequest,
  EngineChatResponse,
  EngineSession,
  EngineStoredMessage,
  EngineEvent,
  EngineStatus,
  EngineAgentFile,
  EngineMemory,
  EngineMemoryConfig,
  EngineMemoryStats,
  OllamaReadyStatus,
  EngineSkillStatus,
  CommunitySkill,
  DiscoveredSkill,
  TomlSkillEntry,
  PawzHubEntry,
  WizardFormData,
  SkillStorageItem,
  TradeRecord,
  TradingSummary,
  TradingPolicy,
  Position,
  TtsConfig,
  EngineTask,
  EngineTaskActivity,
  TaskAgent,
  EngineProject,
  EngineProjectAgent,
  EngineProjectMessage,
  BackendAgent,
  TelegramConfig,
  TelegramStatus,
  ChannelStatus,
  DiscordConfig,
  IrcConfig,
  SlackConfig,
  MatrixConfig,
  MattermostConfig,
  NextcloudConfig,
  NostrConfig,
  TwitchConfig,
  WhatsAppConfig,
  BrowserConfig,
  BrowserProfile,
  ScreenshotEntry,
  WorkspaceInfo,
  WorkspaceFile,
  NetworkPolicy,
  TailscaleStatus,
  TailscaleConfig,
  WebhookConfig,
  McpServerConfig,
  McpServerStatus,
  SkillOutput,
  EngineSquad,
  EngineSquadMember,
  EngineAgentMessage,
} from '../atoms/types';

export class PawEngineClient {
  private _listeners: Map<string, Set<(event: EngineEvent) => void>> = new Map();
  private _tauriUnlisten: (() => void) | null = null;

  /** Start listening for engine events from the Rust backend. */
  async startListening(): Promise<void> {
    if (this._tauriUnlisten) return;
    const { listen } = await import('@tauri-apps/api/event');
    this._tauriUnlisten = (await listen<EngineEvent>('engine-event', (event) => {
      const payload = event.payload;
      const handlers = this._listeners.get(payload.kind);
      if (handlers) {
        for (const h of handlers) {
          try {
            h(payload);
          } catch (e) {
            console.error('[engine] Event handler error:', e);
          }
        }
      }
      const wildcardHandlers = this._listeners.get('*');
      if (wildcardHandlers) {
        for (const h of wildcardHandlers) {
          try {
            h(payload);
          } catch (e) {
            console.error('[engine] Wildcard handler error:', e);
          }
        }
      }
    })) as unknown as () => void;
  }

  on(kind: string, handler: (event: EngineEvent) => void): () => void {
    if (!this._listeners.has(kind)) {
      this._listeners.set(kind, new Set());
    }
    this._listeners.get(kind)!.add(handler);
    return () => this._listeners.get(kind)?.delete(handler);
  }

  destroy(): void {
    if (this._tauriUnlisten) {
      this._tauriUnlisten();
      this._tauriUnlisten = null;
    }
    this._listeners.clear();
  }

  // ── Chat ─────────────────────────────────────────────────────────────

  async chatSend(
    sessionIdOrRequest: string | EngineChatRequest,
    message?: string,
  ): Promise<EngineChatResponse> {
    const request: EngineChatRequest =
      typeof sessionIdOrRequest === 'string'
        ? { session_id: sessionIdOrRequest, message: message ?? '' }
        : sessionIdOrRequest;
    return invoke<EngineChatResponse>('engine_chat_send', { request });
  }

  async chatAbort(sessionId: string): Promise<void> {
    return invoke<void>('engine_chat_abort', { sessionId });
  }

  async chatHistory(sessionId: string, limit?: number): Promise<EngineStoredMessage[]> {
    return invoke<EngineStoredMessage[]>('engine_chat_history', { sessionId, limit: limit ?? 200 });
  }

  // ── Sessions ─────────────────────────────────────────────────────────

  async sessionsList(limit?: number, agentId?: string): Promise<EngineSession[]> {
    return invoke<EngineSession[]>('engine_sessions_list', {
      limit: limit ?? 50,
      agentId: agentId ?? null,
    });
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

  async sessionCleanup(maxAgeSecs?: number, excludeId?: string): Promise<number> {
    return invoke<number>('engine_session_cleanup', {
      maxAgeSecs: maxAgeSecs ?? 3600,
      excludeId: excludeId ?? null,
    });
  }

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

  async status(): Promise<EngineStatus> {
    return invoke<EngineStatus>('engine_status');
  }

  async autoSetup(): Promise<{
    action: string;
    model?: string;
    message?: string;
    available_models?: string[];
  }> {
    return invoke('engine_auto_setup');
  }

  async approveTool(toolCallId: string, approved: boolean): Promise<void> {
    return invoke('engine_approve_tool', { toolCallId, approved });
  }

  // ── Agent Files ──────────────────────────────────────────────────────

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
    return invoke('engine_agent_file_set', { agentId: agentId ?? 'default', fileName, content });
  }

  async agentFileDelete(fileName: string, agentId?: string): Promise<void> {
    return invoke('engine_agent_file_delete', { agentId: agentId ?? 'default', fileName });
  }

  // ── Memory ───────────────────────────────────────────────────────────

  async memoryStore(
    content: string,
    category?: string,
    importance?: number,
    agentId?: string,
  ): Promise<string> {
    return invoke<string>('engine_memory_store', { content, category, importance, agentId });
  }

  async memorySearch(query: string, limit?: number, agentId?: string): Promise<EngineMemory[]> {
    return invoke<EngineMemory[]>('engine_memory_search', { query, limit, agentId });
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

  async embeddingStatus(): Promise<{
    ollama_running: boolean;
    model_available: boolean;
    model_name: string;
    error?: string;
  }> {
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

  // ── Embedding config (legacy Tauri commands) ─────────────────────────

  async getEmbeddingProvider(): Promise<string | null> {
    return invoke<string | null>('get_embedding_provider');
  }

  async getEmbeddingBaseUrl(): Promise<string | null> {
    return invoke<string | null>('get_embedding_base_url');
  }

  async getAzureApiVersion(): Promise<string | null> {
    return invoke<string | null>('get_azure_api_version');
  }

  async testEmbeddingConnection(params: {
    apiKey: string;
    baseUrl: string | null;
    model: string | null;
    apiVersion: string | null;
    provider: string;
  }): Promise<void> {
    return invoke('test_embedding_connection', params);
  }

  async enableMemoryPlugin(params: {
    apiKey: string;
    baseUrl: string | null;
    model: string | null;
    apiVersion: string | null;
    provider: string;
  }): Promise<void> {
    return invoke('enable_memory_plugin', params);
  }

  async checkMemoryConfigured(): Promise<boolean> {
    return invoke<boolean>('check_memory_configured');
  }

  // ── Skills ───────────────────────────────────────────────────────────

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

  // ── Google OAuth2 ──
  async googleOAuthConnect(): Promise<string> {
    return invoke<string>('engine_google_oauth_connect');
  }

  async googleOAuthStatus(): Promise<string | null> {
    return invoke<string | null>('engine_google_oauth_status');
  }

  async googleOAuthDisconnect(): Promise<void> {
    return invoke('engine_google_oauth_disconnect');
  }

  async googleOAuthHasBundled(): Promise<boolean> {
    return invoke<boolean>('engine_google_oauth_has_bundled');
  }

  // ── Google Gmail (Mail UI) ──────────────────────────────────────────

  async googleGmailList(
    query?: string,
    maxResults?: number,
  ): Promise<
    {
      id: string;
      from: string;
      subject: string;
      snippet: string;
      date: string;
      read: boolean;
      labels: string[];
    }[]
  > {
    return invoke('engine_google_gmail_list', {
      query: query ?? null,
      maxResults: maxResults ?? null,
    });
  }

  async googleGmailRead(messageId: string): Promise<{
    id: string;
    from: string;
    to: string;
    subject: string;
    date: string;
    body: string;
    snippet: string;
    labels: string[];
  }> {
    return invoke('engine_google_gmail_read', { messageId });
  }

  async googleGmailSend(to: string, subject: string, body: string): Promise<string> {
    return invoke<string>('engine_google_gmail_send', { to, subject, body });
  }

  async skillGetInstructions(skillId: string): Promise<string | null> {
    return invoke<string | null>('engine_skill_get_instructions', { skillId });
  }

  async skillSetInstructions(skillId: string, instructions: string): Promise<void> {
    return invoke('engine_skill_set_instructions', { skillId, instructions });
  }

  // ── Community Skills (skills.sh) ──────────────────────────────────

  async communitySkillsList(): Promise<CommunitySkill[]> {
    return invoke<CommunitySkill[]>('engine_community_skills_list');
  }

  async communitySkillsBrowse(source: string): Promise<DiscoveredSkill[]> {
    return invoke<DiscoveredSkill[]>('engine_community_skills_browse', { source });
  }

  async communitySkillsSearch(query: string): Promise<DiscoveredSkill[]> {
    return invoke<DiscoveredSkill[]>('engine_community_skills_search', { query });
  }

  async communitySkillInstall(source: string, skillPath: string): Promise<CommunitySkill> {
    return invoke<CommunitySkill>('engine_community_skill_install', { source, skillPath });
  }

  async communitySkillRemove(skillId: string): Promise<void> {
    return invoke('engine_community_skill_remove', { skillId });
  }

  async communitySkillSetEnabled(skillId: string, enabled: boolean): Promise<void> {
    return invoke('engine_community_skill_set_enabled', { skillId, enabled });
  }

  async communitySkillSetAgents(skillId: string, agentIds: string[]): Promise<void> {
    return invoke('engine_community_skill_set_agents', { skillId, agentIds });
  }

  // ── TOML Manifest Skills (Phase F.1) ─────────────────────────────────

  async tomlSkillsScan(): Promise<TomlSkillEntry[]> {
    return invoke<TomlSkillEntry[]>('engine_toml_skills_scan');
  }

  async tomlSkillInstall(skillId: string, tomlContent: string): Promise<string> {
    return invoke<string>('engine_toml_skill_install', { skillId, tomlContent });
  }

  async tomlSkillUninstall(skillId: string): Promise<void> {
    return invoke('engine_toml_skill_uninstall', { skillId });
  }

  // ── Skill Outputs (Phase F.2 — Dashboard Widgets) ────────────────────

  async listSkillOutputs(skillId?: string, agentId?: string): Promise<SkillOutput[]> {
    return invoke<SkillOutput[]>('engine_list_skill_outputs', { skillId, agentId });
  }

  // ── PawzHub Registry (Phase F.4) ─────────────────────────────────────

  async pawzhubSearch(query: string): Promise<PawzHubEntry[]> {
    return invoke<PawzHubEntry[]>('engine_pawzhub_search', { query });
  }

  async pawzhubBrowse(category: string): Promise<PawzHubEntry[]> {
    return invoke<PawzHubEntry[]>('engine_pawzhub_browse', { category });
  }

  async pawzhubInstall(skillId: string, sourceRepo: string): Promise<string> {
    return invoke<string>('engine_pawzhub_install', { skillId, sourceRepo });
  }

  // ── Skill Wizard (Phase F.5) ─────────────────────────────────────────

  async wizardGenerateToml(form: WizardFormData): Promise<string> {
    return invoke<string>('engine_wizard_generate_toml', { form });
  }

  async wizardPublishUrl(skillId: string, tomlContent: string): Promise<string> {
    return invoke<string>('engine_wizard_publish_url', { skillId, tomlContent });
  }

  // ── Skill Storage (Phase F.6) ────────────────────────────────────────

  async skillStoreList(skillId: string): Promise<SkillStorageItem[]> {
    return invoke<SkillStorageItem[]>('engine_skill_store_list', { skillId });
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

  async positionsList(status?: string): Promise<Position[]> {
    return invoke<Position[]>('engine_positions_list', { status: status ?? null });
  }

  async positionClose(id: string): Promise<void> {
    return invoke('engine_position_close', { id });
  }

  async positionUpdateTargets(
    id: string,
    stopLossPct: number,
    takeProfitPct: number,
  ): Promise<void> {
    return invoke('engine_position_update_targets', { id, stopLossPct, takeProfitPct });
  }

  // ── Text-to-Speech ───────────────────────────────────────────────────

  async ttsSpeak(text: string): Promise<string> {
    return invoke<string>('engine_tts_speak', { text });
  }

  async ttsGetConfig(): Promise<TtsConfig> {
    return invoke<TtsConfig>('engine_tts_get_config');
  }

  async ttsSetConfig(config: TtsConfig): Promise<void> {
    return invoke('engine_tts_set_config', { config });
  }

  async ttsTranscribe(audioBase64: string, mimeType: string): Promise<string> {
    return invoke<string>('engine_tts_transcribe', { audioBase64, mimeType });
  }

  // ── Tasks ────────────────────────────────────────────────────────────

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

  // ── Telegram ────────────────────────────────────────────────────────

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

  // ── Discord ─────────────────────────────────────────────────────────

  async discordStart(): Promise<void> {
    return invoke('engine_discord_start');
  }
  async discordStop(): Promise<void> {
    return invoke('engine_discord_stop');
  }
  async discordStatus(): Promise<ChannelStatus> {
    return invoke<ChannelStatus>('engine_discord_status');
  }
  async discordGetConfig(): Promise<DiscordConfig> {
    return invoke<DiscordConfig>('engine_discord_get_config');
  }
  async discordSetConfig(config: DiscordConfig): Promise<void> {
    return invoke('engine_discord_set_config', { config });
  }
  async discordApproveUser(userId: string): Promise<void> {
    return invoke('engine_discord_approve_user', { userId });
  }
  async discordDenyUser(userId: string): Promise<void> {
    return invoke('engine_discord_deny_user', { userId });
  }
  async discordRemoveUser(userId: string): Promise<void> {
    return invoke('engine_discord_remove_user', { userId });
  }

  // ── IRC ──────────────────────────────────────────────────────────────

  async ircStart(): Promise<void> {
    return invoke('engine_irc_start');
  }
  async ircStop(): Promise<void> {
    return invoke('engine_irc_stop');
  }
  async ircStatus(): Promise<ChannelStatus> {
    return invoke<ChannelStatus>('engine_irc_status');
  }
  async ircGetConfig(): Promise<IrcConfig> {
    return invoke<IrcConfig>('engine_irc_get_config');
  }
  async ircSetConfig(config: IrcConfig): Promise<void> {
    return invoke('engine_irc_set_config', { config });
  }
  async ircApproveUser(userId: string): Promise<void> {
    return invoke('engine_irc_approve_user', { userId });
  }
  async ircDenyUser(userId: string): Promise<void> {
    return invoke('engine_irc_deny_user', { userId });
  }
  async ircRemoveUser(userId: string): Promise<void> {
    return invoke('engine_irc_remove_user', { userId });
  }

  // ── Slack ────────────────────────────────────────────────────────────

  async slackStart(): Promise<void> {
    return invoke('engine_slack_start');
  }
  async slackStop(): Promise<void> {
    return invoke('engine_slack_stop');
  }
  async slackStatus(): Promise<ChannelStatus> {
    return invoke<ChannelStatus>('engine_slack_status');
  }
  async slackGetConfig(): Promise<SlackConfig> {
    return invoke<SlackConfig>('engine_slack_get_config');
  }
  async slackSetConfig(config: SlackConfig): Promise<void> {
    return invoke('engine_slack_set_config', { config });
  }
  async slackApproveUser(userId: string): Promise<void> {
    return invoke('engine_slack_approve_user', { userId });
  }
  async slackDenyUser(userId: string): Promise<void> {
    return invoke('engine_slack_deny_user', { userId });
  }
  async slackRemoveUser(userId: string): Promise<void> {
    return invoke('engine_slack_remove_user', { userId });
  }

  // ── Matrix ───────────────────────────────────────────────────────────

  async matrixStart(): Promise<void> {
    return invoke('engine_matrix_start');
  }
  async matrixStop(): Promise<void> {
    return invoke('engine_matrix_stop');
  }
  async matrixStatus(): Promise<ChannelStatus> {
    return invoke<ChannelStatus>('engine_matrix_status');
  }
  async matrixGetConfig(): Promise<MatrixConfig> {
    return invoke<MatrixConfig>('engine_matrix_get_config');
  }
  async matrixSetConfig(config: MatrixConfig): Promise<void> {
    return invoke('engine_matrix_set_config', { config });
  }
  async matrixApproveUser(userId: string): Promise<void> {
    return invoke('engine_matrix_approve_user', { userId });
  }
  async matrixDenyUser(userId: string): Promise<void> {
    return invoke('engine_matrix_deny_user', { userId });
  }
  async matrixRemoveUser(userId: string): Promise<void> {
    return invoke('engine_matrix_remove_user', { userId });
  }

  // ── Mattermost ───────────────────────────────────────────────────────

  async mattermostStart(): Promise<void> {
    return invoke('engine_mattermost_start');
  }
  async mattermostStop(): Promise<void> {
    return invoke('engine_mattermost_stop');
  }
  async mattermostStatus(): Promise<ChannelStatus> {
    return invoke<ChannelStatus>('engine_mattermost_status');
  }
  async mattermostGetConfig(): Promise<MattermostConfig> {
    return invoke<MattermostConfig>('engine_mattermost_get_config');
  }
  async mattermostSetConfig(config: MattermostConfig): Promise<void> {
    return invoke('engine_mattermost_set_config', { config });
  }
  async mattermostApproveUser(userId: string): Promise<void> {
    return invoke('engine_mattermost_approve_user', { userId });
  }
  async mattermostDenyUser(userId: string): Promise<void> {
    return invoke('engine_mattermost_deny_user', { userId });
  }
  async mattermostRemoveUser(userId: string): Promise<void> {
    return invoke('engine_mattermost_remove_user', { userId });
  }

  // ── Nextcloud Talk ───────────────────────────────────────────────────

  async nextcloudStart(): Promise<void> {
    return invoke('engine_nextcloud_start');
  }
  async nextcloudStop(): Promise<void> {
    return invoke('engine_nextcloud_stop');
  }
  async nextcloudStatus(): Promise<ChannelStatus> {
    return invoke<ChannelStatus>('engine_nextcloud_status');
  }
  async nextcloudGetConfig(): Promise<NextcloudConfig> {
    return invoke<NextcloudConfig>('engine_nextcloud_get_config');
  }
  async nextcloudSetConfig(config: NextcloudConfig): Promise<void> {
    return invoke('engine_nextcloud_set_config', { config });
  }
  async nextcloudApproveUser(userId: string): Promise<void> {
    return invoke('engine_nextcloud_approve_user', { userId });
  }
  async nextcloudDenyUser(userId: string): Promise<void> {
    return invoke('engine_nextcloud_deny_user', { userId });
  }
  async nextcloudRemoveUser(userId: string): Promise<void> {
    return invoke('engine_nextcloud_remove_user', { userId });
  }

  // ── Nostr ────────────────────────────────────────────────────────────

  async nostrStart(): Promise<void> {
    return invoke('engine_nostr_start');
  }
  async nostrStop(): Promise<void> {
    return invoke('engine_nostr_stop');
  }
  async nostrStatus(): Promise<ChannelStatus> {
    return invoke<ChannelStatus>('engine_nostr_status');
  }
  async nostrGetConfig(): Promise<NostrConfig> {
    return invoke<NostrConfig>('engine_nostr_get_config');
  }
  async nostrSetConfig(config: NostrConfig): Promise<void> {
    return invoke('engine_nostr_set_config', { config });
  }
  async nostrApproveUser(userId: string): Promise<void> {
    return invoke('engine_nostr_approve_user', { userId });
  }
  async nostrDenyUser(userId: string): Promise<void> {
    return invoke('engine_nostr_deny_user', { userId });
  }
  async nostrRemoveUser(userId: string): Promise<void> {
    return invoke('engine_nostr_remove_user', { userId });
  }

  // ── Twitch ───────────────────────────────────────────────────────────

  async twitchStart(): Promise<void> {
    return invoke('engine_twitch_start');
  }
  async twitchStop(): Promise<void> {
    return invoke('engine_twitch_stop');
  }
  async twitchStatus(): Promise<ChannelStatus> {
    return invoke<ChannelStatus>('engine_twitch_status');
  }
  async twitchGetConfig(): Promise<TwitchConfig> {
    return invoke<TwitchConfig>('engine_twitch_get_config');
  }
  async twitchSetConfig(config: TwitchConfig): Promise<void> {
    return invoke('engine_twitch_set_config', { config });
  }
  async twitchApproveUser(userId: string): Promise<void> {
    return invoke('engine_twitch_approve_user', { userId });
  }
  async twitchDenyUser(userId: string): Promise<void> {
    return invoke('engine_twitch_deny_user', { userId });
  }
  async twitchRemoveUser(userId: string): Promise<void> {
    return invoke('engine_twitch_remove_user', { userId });
  }

  // ── WhatsApp ─────────────────────────────────────────────────────────

  async whatsappStart(): Promise<void> {
    return invoke('engine_whatsapp_start');
  }
  async whatsappStop(): Promise<void> {
    return invoke('engine_whatsapp_stop');
  }
  async whatsappStatus(): Promise<ChannelStatus> {
    return invoke<ChannelStatus>('engine_whatsapp_status');
  }
  async whatsappGetConfig(): Promise<WhatsAppConfig> {
    return invoke<WhatsAppConfig>('engine_whatsapp_get_config');
  }
  async whatsappSetConfig(config: WhatsAppConfig): Promise<void> {
    return invoke('engine_whatsapp_set_config', { config });
  }
  async whatsappApproveUser(userId: string): Promise<void> {
    return invoke('engine_whatsapp_approve_user', { userId });
  }
  async whatsappDenyUser(userId: string): Promise<void> {
    return invoke('engine_whatsapp_deny_user', { userId });
  }
  async whatsappRemoveUser(userId: string): Promise<void> {
    return invoke('engine_whatsapp_remove_user', { userId });
  }

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

  async listAllAgents(): Promise<BackendAgent[]> {
    return invoke<BackendAgent[]>('engine_list_all_agents');
  }

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

  async deleteAgent(agentId: string): Promise<void> {
    return invoke('engine_delete_agent', { agentId });
  }

  async projectMessages(projectId: string, limit?: number): Promise<EngineProjectMessage[]> {
    return invoke<EngineProjectMessage[]>('engine_project_messages', { projectId, limit });
  }

  async projectRun(projectId: string): Promise<string> {
    return invoke<string>('engine_project_run', { projectId });
  }

  // ── Browser Profiles ─────────────────────────────────────────────────

  async browserGetConfig(): Promise<BrowserConfig> {
    return invoke<BrowserConfig>('engine_browser_get_config');
  }

  async browserSetConfig(config: BrowserConfig): Promise<void> {
    return invoke('engine_browser_set_config', { config });
  }

  async browserCreateProfile(name: string): Promise<BrowserProfile> {
    return invoke<BrowserProfile>('engine_browser_create_profile', { name });
  }

  async browserDeleteProfile(profileId: string): Promise<void> {
    return invoke('engine_browser_delete_profile', { profileId });
  }

  // ── Screenshots ──────────────────────────────────────────────────────

  async screenshotsList(): Promise<ScreenshotEntry[]> {
    return invoke<ScreenshotEntry[]>('engine_screenshots_list');
  }

  async screenshotGet(filename: string): Promise<ScreenshotEntry> {
    return invoke<ScreenshotEntry>('engine_screenshot_get', { filename });
  }

  async screenshotDelete(filename: string): Promise<void> {
    return invoke('engine_screenshot_delete', { filename });
  }

  // ── Per-Agent Workspaces ─────────────────────────────────────────────

  async workspacesList(): Promise<WorkspaceInfo[]> {
    return invoke<WorkspaceInfo[]>('engine_workspaces_list');
  }

  async workspaceFiles(agentId: string, subdir?: string): Promise<WorkspaceFile[]> {
    return invoke<WorkspaceFile[]>('engine_workspace_files', { agentId, subdir: subdir ?? null });
  }

  async workspaceDelete(agentId: string): Promise<void> {
    return invoke('engine_workspace_delete', { agentId });
  }

  // ── Network Policy (Outbound Domain Allowlist) ───────────────────────

  async networkGetPolicy(): Promise<NetworkPolicy> {
    return invoke<NetworkPolicy>('engine_network_get_policy');
  }

  async networkSetPolicy(policy: NetworkPolicy): Promise<void> {
    return invoke('engine_network_set_policy', { policy });
  }

  async networkCheckUrl(url: string): Promise<[boolean, string]> {
    return invoke<[boolean, string]>('engine_network_check_url', { url });
  }

  // ── Tailscale (Remote Access) ──────────────────────────────────────

  async tailscaleStatus(): Promise<TailscaleStatus> {
    return invoke<TailscaleStatus>('engine_tailscale_status');
  }

  async tailscaleGetConfig(): Promise<TailscaleConfig> {
    return invoke<TailscaleConfig>('engine_tailscale_get_config');
  }

  async tailscaleSetConfig(config: TailscaleConfig): Promise<void> {
    return invoke('engine_tailscale_set_config', { config });
  }

  async tailscaleServeStart(port?: number): Promise<void> {
    return invoke('engine_tailscale_serve_start', { port: port ?? null });
  }

  async tailscaleServeStop(): Promise<void> {
    return invoke('engine_tailscale_serve_stop');
  }

  async tailscaleFunnelStart(port?: number): Promise<void> {
    return invoke('engine_tailscale_funnel_start', { port: port ?? null });
  }

  async tailscaleFunnelStop(): Promise<void> {
    return invoke('engine_tailscale_funnel_stop');
  }

  async tailscaleConnect(authKey?: string): Promise<void> {
    return invoke('engine_tailscale_connect', { authKey: authKey ?? null });
  }

  async tailscaleDisconnect(): Promise<void> {
    return invoke('engine_tailscale_disconnect');
  }

  // ── Webhook Server (Phase D) ───────────────────────────────────────

  async webhookStart(): Promise<void> {
    return invoke('engine_webhook_start');
  }

  async webhookStop(): Promise<void> {
    return invoke('engine_webhook_stop');
  }

  async webhookStatus(): Promise<ChannelStatus> {
    return invoke<ChannelStatus>('engine_webhook_status');
  }

  async webhookGetConfig(): Promise<WebhookConfig> {
    return invoke<WebhookConfig>('engine_webhook_get_config');
  }

  async webhookSetConfig(config: WebhookConfig): Promise<void> {
    return invoke('engine_webhook_set_config', { config });
  }

  async webhookRegenerateToken(): Promise<string> {
    return invoke<string>('engine_webhook_regenerate_token');
  }

  // ── MCP Servers (Phase E) ──────────────────────────────────────────

  async mcpListServers(): Promise<McpServerConfig[]> {
    return invoke<McpServerConfig[]>('engine_mcp_list_servers');
  }

  async mcpSaveServer(server: McpServerConfig): Promise<void> {
    return invoke('engine_mcp_save_server', { server });
  }

  async mcpRemoveServer(id: string): Promise<void> {
    return invoke('engine_mcp_remove_server', { id });
  }

  async mcpConnect(id: string): Promise<void> {
    return invoke('engine_mcp_connect', { id });
  }

  async mcpDisconnect(id: string): Promise<void> {
    return invoke('engine_mcp_disconnect', { id });
  }

  async mcpStatus(): Promise<McpServerStatus[]> {
    return invoke<McpServerStatus[]>('engine_mcp_status');
  }

  async mcpRefreshTools(id: string): Promise<void> {
    return invoke('engine_mcp_refresh_tools', { id });
  }

  async mcpConnectAll(): Promise<void> {
    return invoke<void>('engine_mcp_connect_all');
  }

  // ── Mail (Himalaya) ────────────────────────────────────────────────

  async mailReadConfig(): Promise<string> {
    return invoke<string>('read_himalaya_config');
  }

  async mailWriteConfig(opts: {
    accountName: string;
    email: string;
    displayName: string | null;
    imapHost: string;
    imapPort: number;
    smtpHost: string;
    smtpPort: number;
    password: string;
  }): Promise<void> {
    return invoke('write_himalaya_config', opts);
  }

  async mailRemoveAccount(accountName: string): Promise<void> {
    return invoke('remove_himalaya_account', { accountName });
  }

  async mailFetchEmails(account?: string, folder?: string, pageSize?: number): Promise<string> {
    return invoke<string>('fetch_emails', {
      account: account ?? null,
      folder: folder ?? null,
      pageSize: pageSize ?? null,
    });
  }

  async mailFetchContent(account: string | undefined, folder: string, id: string): Promise<string> {
    return invoke<string>('fetch_email_content', {
      account: account ?? null,
      folder,
      id,
    });
  }

  async mailSend(
    account: string | undefined,
    to: string,
    subject: string,
    body: string,
  ): Promise<void> {
    return invoke('send_email', { account: account ?? null, to, subject, body });
  }

  async mailMove(account: string | undefined, id: string, folder: string): Promise<void> {
    return invoke('move_email', { account: account ?? null, id, folder });
  }

  async mailDelete(account: string | undefined, id: string): Promise<void> {
    return invoke('delete_email', { account: account ?? null, id });
  }

  // ── Agent Messages ────────────────────────────────────────────────────

  /** Fetch agent-to-agent messages, optionally filtered by channel. */
  async agentMessages(
    agentId: string,
    channel?: string,
    limit?: number,
  ): Promise<EngineAgentMessage[]> {
    return invoke<EngineAgentMessage[]>('engine_agent_messages', {
      agentId,
      channel: channel ?? null,
      limit: limit ?? 50,
    });
  }

  // ── Squads ────────────────────────────────────────────────────────────

  async squadsList(): Promise<EngineSquad[]> {
    return invoke<EngineSquad[]>('engine_squads_list');
  }

  async squadCreate(squad: EngineSquad): Promise<void> {
    return invoke('engine_squad_create', { squad });
  }

  async squadUpdate(squad: EngineSquad): Promise<void> {
    return invoke('engine_squad_update', { squad });
  }

  async squadDelete(squadId: string): Promise<void> {
    return invoke('engine_squad_delete', { squadId });
  }

  async squadAddMember(squadId: string, member: EngineSquadMember): Promise<void> {
    return invoke('engine_squad_add_member', { squadId, member });
  }

  async squadRemoveMember(squadId: string, agentId: string): Promise<void> {
    return invoke('engine_squad_remove_member', { squadId, agentId });
  }
}

/** Create a new engine client instance — useful for testing or custom wiring. */
export function createPawEngine(): PawEngineClient {
  return new PawEngineClient();
}

/** Default singleton engine client — import this in application consumers. */
export const pawEngine: PawEngineClient = createPawEngine();
