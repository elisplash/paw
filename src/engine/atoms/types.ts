// src/engine/atoms/types.ts
// Pure TypeScript type definitions for the Paw engine IPC layer.
// Extracted from engine.ts — no runtime code here.

// ── Provider / Config ─────────────────────────────────────────────────

export interface EngineProviderConfig {
  id: string;
  kind:
    | 'openai'
    | 'anthropic'
    | 'google'
    | 'ollama'
    | 'openrouter'
    | 'custom'
    | 'deepseek'
    | 'grok'
    | 'mistral'
    | 'moonshot';
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
  /** Max simultaneous agent runs (chat + cron + tasks). Chat always gets priority. Default: 4 */
  max_concurrent_runs?: number;
  /** Daily budget in USD. When estimated spend exceeds this, new API calls are blocked. 0 = disabled. Default: 10 */
  daily_budget_usd?: number;
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
  /** Cheapest model for simple tasks (used when auto_tier is enabled) */
  cheap_model?: string;
  /** Enable automatic model tier selection: simple → cheap, complex → default */
  auto_tier?: boolean;
}

// ── Chat ─────────────────────────────────────────────────────────────

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
  /** Thinking/reasoning level: "none", "low", "medium", "high" */
  thinking_level?: string;
  /** Phase A: If true, all tool calls auto-approved (no HIL popups). */
  auto_approve_all?: boolean;
}

export interface EngineChatResponse {
  run_id: string;
  session_id: string;
}

// ── Sessions ─────────────────────────────────────────────────────────

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

// ── Events ───────────────────────────────────────────────────────────

export interface EngineEvent {
  kind:
    | 'delta'
    | 'tool_request'
    | 'tool_result'
    | 'complete'
    | 'error'
    | 'thinking_delta'
    | 'tool_auto_approved';
  session_id: string;
  run_id: string;
  // delta + thinking_delta
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
  // tool_auto_approved
  tool_name?: string;
}

export interface EngineStatus {
  ready: boolean;
  providers: number;
  has_api_key: boolean;
  default_model?: string;
  default_provider?: string;
}

// ── Agent Files (Soul / Persona) ─────────────────────────────────────

export interface EngineAgentFile {
  agent_id: string;
  file_name: string;
  content: string;
  updated_at: string;
}

// ── Memory ───────────────────────────────────────────────────────────

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

// ── Skills ───────────────────────────────────────────────────────────

export interface EngineSkillCredentialField {
  key: string;
  label: string;
  description: string;
  required: boolean;
  placeholder: string;
}

export type SkillTier = 'skill' | 'integration' | 'extension';

export interface EngineSkillStatus {
  id: string;
  name: string;
  description: string;
  icon: string;
  category: string;
  tier: SkillTier;
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
  /** Where this skill was loaded from (builtin or toml) */
  source?: SkillSource;
  /** Manifest version (TOML skills only) */
  version?: string;
  /** Manifest author (TOML skills only) */
  author?: string;
  /** Whether the skill bundles an MCP server */
  has_mcp?: boolean;
  /** Whether the skill declares a dashboard widget */
  has_widget?: boolean;
}

export type SkillSource = 'builtin' | 'toml';

// ── TOML Manifest Skills (Phase F.1) ─────────────────────────────────

export interface TomlSkillEntry {
  definition: EngineSkillStatus;
  source_dir: string;
  version: string;
  author: string;
  has_mcp: boolean;
  has_widget: boolean;
  has_view: boolean;
  view_label: string;
  view_icon: string;
}

// ── Skill Outputs (Phase F.2 — Dashboard Widgets) ────────────────────

/** A persisted skill output row for dashboard widget rendering. */
export interface SkillOutput {
  id: string;
  skill_id: string;
  agent_id: string;
  widget_type: 'status' | 'metric' | 'table' | 'log' | 'kv';
  title: string;
  /** JSON-encoded structured data. */
  data: string;
  created_at: string;
  updated_at: string;
}

// ── Community Skills (skills.sh) ─────────────────────────────────────

export interface CommunitySkill {
  id: string;
  name: string;
  description: string;
  instructions: string;
  source: string;
  enabled: boolean;
  agent_ids: string[];
  installed_at: string;
  updated_at: string;
}

export interface DiscoveredSkill {
  id: string;
  name: string;
  description: string;
  source: string;
  path: string;
  installed: boolean;
  installs: number;
}

// ── PawzHub Registry (Phase F.4) ─────────────────────────────────────

/** A skill entry from the PawzHub registry. */
export interface PawzHubEntry {
  id: string;
  name: string;
  description: string;
  author: string;
  category: string;
  version: string;
  /** "skill" | "integration" | "extension" | "mcp" */
  tier: string;
  source_repo: string;
  has_mcp: boolean;
  has_widget: boolean;
  verified: boolean;
  installed: boolean;
}

// ── Skill Wizard (Phase F.5) ─────────────────────────────────────────

/** A credential field in the wizard form. */
export interface WizardCredential {
  key: string;
  label: string;
  description: string;
  required: boolean;
  placeholder: string;
}

/** A widget field in the wizard form. */
export interface WizardWidgetField {
  key: string;
  label: string;
  field_type: string;
}

/** Widget configuration in the wizard form. */
export interface WizardWidget {
  widget_type: string;
  title: string;
  refresh: string;
  fields: WizardWidgetField[];
}

/** MCP server configuration in the wizard form. */
export interface WizardMcp {
  command: string;
  args: string[];
  transport: string;
  url: string;
}

/** Complete wizard form data for TOML generation. */
export interface WizardFormData {
  id: string;
  name: string;
  version: string;
  author: string;
  category: string;
  icon: string;
  description: string;
  install_hint: string;
  instructions: string;
  credentials: WizardCredential[];
  widget: WizardWidget | null;
  mcp: WizardMcp | null;
}

// ── Skill Storage (Phase F.6) ────────────────────────────────────────

/** A key-value entry from a skill's persistent storage. */
export interface SkillStorageItem {
  skill_id: string;
  key: string;
  value: string;
  updated_at: string;
}

// ── Trading ──────────────────────────────────────────────────────────

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

export interface Position {
  id: string;
  mint: string;
  symbol: string;
  entry_price_usd: number;
  entry_sol: number;
  amount: number;
  current_amount: number;
  stop_loss_pct: number;
  take_profit_pct: number;
  status: string;
  last_price_usd: number;
  last_checked_at: string | null;
  created_at: string;
  closed_at: string | null;
  close_tx: string | null;
  agent_id: string | null;
}

// ── Text-to-Speech ────────────────────────────────────────────────────

export interface TtsConfig {
  provider: string; // "google" | "openai" | "elevenlabs"
  voice: string; // e.g. "en-US-Chirp3-HD-Achernar" or "alloy" or ElevenLabs voice_id
  speed: number; // 0.25–4.0
  language_code: string; // e.g. "en-US"
  auto_speak: boolean; // automatically speak new responses
  elevenlabs_api_key: string; // ElevenLabs API key
  elevenlabs_model: string; // "eleven_multilingual_v2" | "eleven_turbo_v2_5"
  stability: number; // 0.0–1.0
  similarity_boost: number; // 0.0–1.0
}

// ── Tasks ─────────────────────────────────────────────────────────────

export type TaskStatus = 'inbox' | 'assigned' | 'in_progress' | 'review' | 'blocked' | 'done';
export type TaskPriority = 'low' | 'medium' | 'high' | 'urgent';

export interface EngineTask {
  id: string;
  title: string;
  description: string;
  status: TaskStatus;
  priority: TaskPriority;
  assigned_agent?: string;
  assigned_agents: TaskAgent[];
  session_id?: string;
  /** Override model for this task. If empty, uses agent routing / default. */
  model?: string;
  cron_schedule?: string;
  cron_enabled: boolean;
  last_run_at?: string;
  next_run_at?: string;
  created_at: string;
  updated_at: string;
}

export interface TaskAgent {
  agent_id: string;
  role: string; // 'lead' | 'collaborator'
}

export interface EngineTaskActivity {
  id: string;
  task_id: string;
  kind: string;
  agent?: string;
  content: string;
  created_at: string;
}

// ── Orchestrator: Projects ────────────────────────────────────────────

export interface EngineProject {
  id: string;
  title: string;
  goal: string;
  status: string; // planning, running, paused, completed, failed
  boss_agent: string;
  agents: EngineProjectAgent[];
  created_at: string;
  updated_at: string;
}

export interface EngineProjectAgent {
  agent_id: string;
  role: string; // boss, worker
  specialty: string; // coder, researcher, designer, communicator, security, general
  status: string; // idle, working, done, error
  current_task?: string;
  model?: string;
  system_prompt?: string;
  capabilities?: string[];
}

export interface EngineProjectMessage {
  id: string;
  project_id: string;
  from_agent: string;
  to_agent?: string;
  kind: string; // delegation, progress, result, error, message
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

// ── Channel Types ─────────────────────────────────────────────────────

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

export interface DiscordConfig {
  bot_token: string;
  enabled: boolean;
  dm_policy: string;
  allowed_users: string[];
  pending_users: ChannelPendingUser[];
  agent_id?: string;
  respond_to_mentions: boolean;
}

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

export interface NostrConfig {
  private_key_hex: string;
  relays: string[];
  enabled: boolean;
  dm_policy: string;
  allowed_users: string[];
  pending_users: ChannelPendingUser[];
  agent_id?: string;
}

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

export interface WhatsAppConfig {
  enabled: boolean;
  instance_name: string;
  api_url: string;
  api_key: string;
  api_port: number;
  webhook_port: number;
  dm_policy: string;
  allowed_users: string[];
  pending_users: ChannelPendingUser[];
  agent_id?: string;
  respond_in_groups: boolean;
  container_id?: string;
  session_connected: boolean;
  qr_code?: string;
}

// ── Browser Profiles & Sandbox ────────────────────────────────────────

export interface BrowserProfile {
  id: string;
  name: string;
  user_data_dir: string;
  created_at: string;
  last_used: string;
  size_bytes: number;
}

export interface BrowserConfig {
  default_profile: string;
  profiles: BrowserProfile[];
  headless: boolean;
  auto_close_tabs: boolean;
  idle_timeout_secs: number;
}

export interface ScreenshotEntry {
  filename: string;
  path: string;
  size_bytes: number;
  created_at: string;
  base64_png?: string;
}

// ── Per-Agent Workspaces ──────────────────────────────────────────────

export interface WorkspaceInfo {
  agent_id: string;
  path: string;
  total_files: number;
  total_size_bytes: number;
  exists: boolean;
}

export interface WorkspaceFile {
  name: string;
  path: string;
  is_dir: boolean;
  size_bytes: number;
  modified_at: string;
}

// ── Network Policy (Outbound Domain Allowlist) ────────────────────────

export interface NetworkPolicy {
  enabled: boolean;
  allowed_domains: string[];
  blocked_domains: string[];
  log_requests: boolean;
  recent_requests: NetworkRequest[];
}

export interface NetworkRequest {
  url: string;
  domain: string;
  allowed: boolean;
  timestamp: string;
  tool_name: string;
}

// ── Tailscale (Remote Access) ─────────────────────────────────────────

export interface TailscaleStatus {
  installed: boolean;
  running: boolean;
  hostname: string;
  tailnet: string;
  ip: string;
  version: string;
  serve_active: boolean;
  funnel_active: boolean;
  serve_url: string;
  funnel_url: string;
}

export interface TailscaleConfig {
  enabled: boolean;
  serve_port: number;
  funnel_enabled: boolean;
  auth_key: string;
  hostname_override: string;
}

export interface WebhookConfig {
  enabled: boolean;
  bind_address: string;
  port: number;
  auth_token: string;
  default_agent_id: string;
  rate_limit_per_minute: number;
  allow_dangerous_tools: boolean;
}

// ── MCP Servers (Phase E) ────────────────────────────────────────────

export type McpTransport = 'stdio' | 'sse';

export interface McpServerConfig {
  id: string;
  name: string;
  transport: McpTransport;
  command: string;
  args: string[];
  env: Record<string, string>;
  url: string;
  enabled: boolean;
}

export interface McpServerStatus {
  id: string;
  name: string;
  connected: boolean;
  error: string | null;
  tool_count: number;
}
