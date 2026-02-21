// Paw — Type Definitions

// ── App-level config ──────────────────────────────────────────────────────

export interface AppConfig {
  configured: boolean;
}

// ── Agents ─────────────────────────────────────────────────────────────────

export interface AgentSummary {
  id: string;
  name?: string;
  identity?: {
    name?: string;
    theme?: string;
    emoji?: string;
    avatar?: string;
    avatarUrl?: string;
  };
}

export interface AgentsListResult {
  defaultId: string;
  mainKey: string;
  scope: 'per-sender' | 'global';
  agents: AgentSummary[];
}

export interface AgentIdentityResult {
  agentId: string;
  name?: string;
  theme?: string;
  emoji?: string;
  avatar?: string;
  avatarUrl?: string;
}

// ── Agent CRUD ─────────────────────────────────────────────────────────────

export interface AgentCreateParams {
  name: string;
  workspace?: string;
  emoji?: string;
  avatar?: string;
}

export interface AgentCreateResult {
  ok: boolean;
  agentId: string;
  name: string;
  workspace: string;
}

export interface AgentUpdateParams {
  agentId: string;
  name?: string;
  workspace?: string;
  model?: string;
  avatar?: string;
}

export interface AgentUpdateResult {
  ok: boolean;
  agentId: string;
}

export interface AgentDeleteParams {
  agentId: string;
  deleteFiles?: boolean;
}

export interface AgentDeleteResult {
  ok: boolean;
  agentId: string;
  removedBindings?: number;
}

// ── Channels ───────────────────────────────────────────────────────────────

export interface ChannelAccountStatus {
  accountId: string;
  linked: boolean;
  configured: boolean;
  authAgeMs?: number | null;
  probe?: unknown;
  [key: string]: unknown;
}

export interface ChannelStatus {
  id: string;
  name: string;
  linked: boolean;
  configured: boolean;
  accounts?: Record<string, ChannelAccountStatus>;
  [key: string]: unknown;
}

export interface ChannelsStatusResult {
  channels: Record<string, ChannelStatus>;
}

// ── Sessions ───────────────────────────────────────────────────────────────

export interface Session {
  key: string;
  kind: 'direct' | 'group' | 'global' | 'unknown';
  sessionId?: string;
  updatedAt?: number;
  label?: string;
  displayName?: string;
  channel?: string;
  lastChannel?: string;
  lastTo?: string;
  lastAccountId?: string;
  abortedLastRun?: boolean;
  thinkingLevel?: string;
  agentId?: string;
}

export interface SessionsListResult {
  path?: string;
  sessions: Session[];
}

export interface SessionPreviewResult {
  key: string;
  lastMessage?: string;
  lastRole?: string;
  lastTs?: number;
  messageCount?: number;
}

export interface SessionCompactResult {
  ok: boolean;
  key?: string;
  removed?: number;
}

// ── Chat ───────────────────────────────────────────────────────────────────

export type ContentBlock =
  | { type: 'text'; text: string }
  | { type: 'tool_use'; id: string; name: string; input: unknown }
  | { type: 'tool_result'; tool_use_id: string; content: string | unknown };

/** Attachment for chat messages (images, documents, etc.) */
/** Attachment for chat.send - matches OpenClaw gateway format */
export interface ChatAttachment {
  /** Type identifier (e.g., 'image', 'file') */
  type?: string;
  /** MIME type (e.g., 'image/png', 'application/pdf') */
  mimeType?: string;
  /** Original filename */
  fileName?: string;
  /** Original filename (engine format) */
  name?: string;
  /** Base64-encoded content */
  content?: string;
}

export interface ChatMessage {
  id?: string;
  role: 'user' | 'assistant' | 'system' | 'tool';
  content: string | ContentBlock | ContentBlock[];
  ts?: string | number;
  timestamp?: string | number;
  toolCalls?: ToolCall[];
  /** Attachments (images, documents) */
  attachments?: ChatAttachment[];
  [key: string]: unknown;
}

export interface ToolCall {
  id: string;
  name: string;
  arguments?: string;
  result?: string;
  status?: 'pending' | 'running' | 'done' | 'error';
}

export interface ChatHistoryResult {
  sessionId: string;
  messages: ChatMessage[];
  thinkingLevel?: string;
}

export interface ChatSendResult {
  runId: string;
  sessionKey?: string;
  text?: string;
  response?: unknown;
  toolCalls?: ToolCall[];
}

export interface AgentRunResult {
  runId: string;
  ok?: boolean;
}

export interface AgentWaitResult {
  runId: string;
  status: string;
  text?: string;
  toolCalls?: ToolCall[];
}

// ── Agent Events (streamed over WS) ───────────────────────────────────────

export interface AgentEvent {
  sessionKey?: string;
  runId?: string;
  seq?: number;
  ts?: number;
  stream: 'assistant' | 'lifecycle' | 'tool' | 'error' | string;
  data: {
    // assistant stream
    text?: string; // accumulated text so far
    delta?: string; // incremental text
    // lifecycle stream
    phase?: 'start' | 'end' | string;
    // tool stream
    name?: string;
    tool?: string;
    // error stream
    message?: string;
    error?: string;
    [key: string]: unknown;
  };
  [key: string]: unknown;
}

export interface ChatEvent {
  runId?: string;
  sessionKey?: string;
  seq?: number;
  state: 'delta' | 'final' | string;
  message?: {
    role: string;
    content: string | ContentBlock | ContentBlock[];
    timestamp?: string | number;
    [key: string]: unknown;
  };
  [key: string]: unknown;
}

// ── Cron ───────────────────────────────────────────────────────────────────

export interface CronJob {
  id: string;
  label?: string;
  schedule?: string | { type: string; [key: string]: unknown };
  prompt?: string;
  enabled: boolean;
  lastRunAt?: number;
  nextRunAt?: number;
  sessionKey?: string;
  agentId?: string;
  [key: string]: unknown;
}

export interface CronListResult {
  jobs: CronJob[];
}

export interface CronRunLogEntry {
  runId: string;
  jobId: string;
  jobLabel?: string;
  startedAt: number;
  finishedAt?: number;
  status: string;
  error?: string;
}

// ── Skills ─────────────────────────────────────────────────────────────────

export interface SkillInstallOption {
  id: string;
  kind: string;
  label: string;
  bins?: string[];
}

export interface SkillEntry {
  name: string;
  description?: string;
  source?: string;
  bundled?: boolean;
  skillKey?: string;
  primaryEnv?: string;
  emoji?: string;
  homepage?: string;
  always?: boolean;
  disabled?: boolean;
  blockedByAllowlist?: boolean;
  eligible?: boolean;
  requirements?: {
    bins?: string[];
    anyBins?: string[];
    env?: string[];
    config?: string[];
    os?: string[];
  };
  missing?: {
    bins?: string[];
    anyBins?: string[];
    env?: string[];
    config?: string[];
    os?: string[];
  };
  configChecks?: { path: string; value: unknown; satisfied: boolean }[];
  install?: SkillInstallOption[];
  [key: string]: unknown;
}

export interface SkillsStatusResult {
  agentId?: string;
  workspaceDir: string;
  skills: SkillEntry[];
}

// ── Models ─────────────────────────────────────────────────────────────────

export interface ModelChoice {
  id: string;
  name: string;
  provider: string;
  contextWindow?: number;
  reasoning?: boolean;
}

export interface ModelsListResult {
  models: ModelChoice[];
}

export interface ConfigApplyResult {
  ok: boolean;
  restarted?: boolean;
  errors?: string[];
  path?: string;
  config?: Record<string, unknown>;
}

export interface ConfigPatchParams {
  raw: string; // JSON5 string with just the keys to change
  baseHash?: string; // SHA-256 hash from config.get — required when config exists
  sessionKey?: string;
  note?: string;
  restartDelayMs?: number;
}

export interface ConfigSchemaResult {
  schema: Record<string, unknown>;
}

// ── Presence ───────────────────────────────────────────────────────────────

export interface PresenceEntry {
  connId: string;
  client: { id: string; version: string; platform: string; mode: string };
  role: string;
  connectedAt: number;
  [key: string]: unknown;
}

// ── Exec Approvals ─────────────────────────────────────────────────────────

export interface ExecApprovalsSnapshot {
  allow: string[];
  deny: string[];
  askPolicy: string;
}

// ── Agent Files (Memory) ───────────────────────────────────────────────────

export interface AgentFileEntry {
  path: string;
  name?: string;
  size?: number;
  sizeBytes?: number;
  modifiedAt?: number;
}

export interface AgentsFilesListResult {
  ok: true;
  agentId: string;
  workspace: string;
  files: AgentFileEntry[];
}

export interface AgentsFilesGetResult {
  ok: true;
  agentId: string;
  workspace: string;
  file: AgentFileEntry;
  content: string;
}

// ── Usage ──────────────────────────────────────────────────────────────────

export interface UsageStatusResult {
  ok: boolean;
  total?: { requests: number; tokens: number; inputTokens?: number; outputTokens?: number };
  byModel?: Record<
    string,
    { requests: number; tokens: number; inputTokens?: number; outputTokens?: number }
  >;
  period?: string;
  [key: string]: unknown;
}

export interface UsageCostResult {
  ok: boolean;
  totalCost?: number;
  currency?: string;
  byModel?: Record<string, { cost: number; requests: number }>;
  period?: string;
  [key: string]: unknown;
}

// ── Logs ───────────────────────────────────────────────────────────────────

export interface LogsTailResult {
  lines: string[];
}

// ── Install (Tauri-side) ───────────────────────────────────────────────────

export interface InstallProgress {
  stage: string;
  percent: number;
  message: string;
}

// ── Legacy compat alias (used by main.ts UI messages) ──────────────────────

export interface Message {
  id?: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: Date;
  toolCalls?: ToolCall[];
  attachments?: Array<{ name?: string; mimeType: string; data?: string; url?: string }>;
}

// ── Paw Frontend Agent Profile ───────────────────────────────────────────

export interface Agent {
  id: string;
  name: string;
  avatar: string; // emoji or initials
  color: string;
  bio: string;
  model: string; // AI model to use
  template: 'general' | 'research' | 'creative' | 'technical' | 'custom';
  personality: {
    tone: 'casual' | 'balanced' | 'formal';
    initiative: 'reactive' | 'balanced' | 'proactive';
    detail: 'brief' | 'balanced' | 'thorough';
  };
  skills: string[];
  boundaries: string[];
  systemPrompt?: string; // Custom instructions
  createdAt: string;
  lastUsed?: string;
}
