// Paw — OpenClaw Gateway WebSocket Client (Protocol v3)

import type {
  GatewayConfig,
  HelloOk,
  HealthSummary,
  AgentsListResult,
  AgentIdentityResult,
  ChannelsStatusResult,
  SessionsListResult,
  SessionPreviewResult,
  SessionCompactResult,
  ChatHistoryResult,
  ChatSendResult,
  CronListResult,
  CronJob,
  CronRunLogEntry,
  SkillsStatusResult,
  ModelsListResult,
  NodeListResult,
  GatewayConfigResult,
  ConfigApplyResult,
  ConfigSchemaResult,
  PresenceEntry,
  ExecApprovalsSnapshot,
  AgentsFilesListResult,
  AgentsFilesGetResult,
  UsageStatusResult,
  UsageCostResult,
  LogsTailResult,
  AgentWaitResult,
} from './types';

const PROTOCOL_VERSION = 3;
const REQUEST_TIMEOUT_MS = 30_000;
const RECONNECT_BASE_MS = 3_000;
const RECONNECT_MAX_MS = 60_000;
const MAX_RECONNECT_ATTEMPTS = 20;

type EventHandler = (payload: unknown) => void;

function detectPlatform(): string {
  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes('win')) return 'windows';
  if (ua.includes('mac')) return 'macos';
  if (ua.includes('linux')) return 'linux';
  return 'unknown';
}

// ── Security: Localhost-only gateway validation ────────────────────────────
const LOCALHOST_HOSTS = new Set([
  'localhost',
  '127.0.0.1',
  '::1',
  '[::1]',
  '0.0.0.0',
]);

/**
 * Validates that a WebSocket URL points to a localhost address only.
 * Prevents token leakage to remote hosts via SSRF or config tampering.
 */
export function isLocalhostUrl(url: string): boolean {
  try {
    // Normalise ws:// → http:// so URL parser accepts it
    const normalised = url.replace(/^ws(s?):\/\//, 'http$1://');
    const parsed = new URL(normalised);
    return LOCALHOST_HOSTS.has(parsed.hostname.toLowerCase());
  } catch {
    return false;
  }
}

let _requestId = 0;
function nextId(): string {
  return `paw-${++_requestId}`;
}

class GatewayClient {
  private ws: WebSocket | null = null;
  private config: GatewayConfig | null = null;
  private pendingRequests = new Map<string, { resolve: (v: unknown) => void; reject: (e: Error) => void; timer: number }>();
  private eventHandlers = new Map<string, Set<EventHandler>>();
  private reconnectTimer: number | null = null;
  private reconnectAttempts = 0;
  private keepaliveTimer: number | null = null;
  private _connected = false;
  private _connecting = false;
  private _lastSeq = 0;
  private _challengeNonce: string | null = null;
  hello: HelloOk | null = null;

  // ── Connection lifecycle ─────────────────────────────────────────────

  get isConnecting(): boolean {
    return this._connecting;
  }

  async connect(config: GatewayConfig): Promise<HelloOk> {
    if (this._connecting) {
      console.warn('[gateway] Connection already in progress, skipping');
      throw new Error('Connection already in progress');
    }

    // ── Security: block non-localhost gateway URLs ──
    if (!isLocalhostUrl(config.url)) {
      console.error(`[gateway] BLOCKED: non-localhost gateway URL "${config.url}"`);
      throw new Error('Security: gateway URL must be localhost (127.0.0.1, ::1, or localhost)');
    }

    this.config = config;
    this._connecting = true;
    this.disconnect(); // clean up any previous connection

    const tokenMasked = config.token
      ? (config.token.length > 8
        ? `${config.token.slice(0, 4)}...${config.token.slice(-4)}`
        : '****')
      : '(empty)';
    console.log(`[gateway] Connecting to ${config.url} with token: ${tokenMasked}`);

    return new Promise((resolve, reject) => {
      const wsUrl = config.url.replace(/^http/, 'ws');
      console.log(`[gateway] Opening WebSocket: ${wsUrl}`);

      try {
        this.ws = new WebSocket(wsUrl);
      } catch (e) {
        this._connecting = false;
        console.error('[gateway] WebSocket constructor failed:', e);
        reject(e);
        return;
      }

      const openTimeout = setTimeout(() => {
        this._connecting = false;
        console.error('[gateway] WebSocket open timeout (10s)');
        reject(new Error('WebSocket open timeout'));
        this.ws?.close();
      }, 10_000);

      this.ws.onopen = async () => {
        clearTimeout(openTimeout);
        console.log('[gateway] WebSocket opened, starting handshake...');
        try {
          const hello = await this.handshake();
          this._connected = true;
          this._connecting = false;
          this.hello = hello;
          this.startKeepalive();
          console.log('[gateway] Handshake success:', hello?.type ?? 'hello-ok');
          this.emit('_connected', hello);
          resolve(hello);
        } catch (e) {
          this._connecting = false;
          console.error('[gateway] Handshake failed:', e);
          reject(e);
          this.ws?.close();
        }
      };

      this.ws.onmessage = (ev) => {
        // Guard: ignore messages from a superseded WebSocket
        if (ev.target !== this.ws) return;
        try {
          const frame = JSON.parse(ev.data);
          this.handleFrame(frame);
        } catch (e) {
          console.error('[gateway] Frame handling error:', e, ev.data?.slice?.(0, 200));
        }
      };

      this.ws.onclose = (ev) => {
        // Guard: ignore close events from a superseded WebSocket
        if (ev.target !== this.ws) return;
        const wasConnected = this._connected;
        this.stopKeepalive();
        this._connected = false;
        this._connecting = false;
        this.hello = null;
        console.log(`[gateway] WebSocket closed: code=${ev.code} reason="${ev.reason}" wasConnected=${wasConnected}`);
        this.rejectAllPending('connection closed');
        if (wasConnected) {
          this.emit('_disconnected', {});
          this.reconnectAttempts = 0;
          // Defer reconnect to avoid re-entrancy in the close handler
          setTimeout(() => this.scheduleReconnect(), 0);
        }
      };

      this.ws.onerror = (ev) => {
        console.error('[gateway] WebSocket error event:', ev);
        // onclose will fire after this
      };
    });
  }

  private async handshake(): Promise<HelloOk> {
    // Wait for the connect.challenge event from the gateway.
    // OpenClaw sends { type:"event", event:"connect.challenge", payload: { nonce, ts } }
    // immediately after the WebSocket opens. We listen for it briefly.
    this._challengeNonce = null;
    const challengeReceived = new Promise<void>((resolve) => {
      const off = this.on('connect.challenge', (payload: unknown) => {
        const p = payload as { nonce?: string } | null;
        if (p?.nonce) {
          this._challengeNonce = p.nonce;
          console.log('[gateway] Received connect.challenge nonce');
        }
        off();
        resolve();
      });
      // Don't wait forever — if no challenge arrives within 500ms, proceed anyway.
      // Some gateway configurations may not send a challenge for loopback connections.
      setTimeout(() => {
        off();
        resolve();
      }, 500);
    });
    await challengeReceived;

    // Build auth: only send auth object if we actually have a token.
    // Sending { token: '' } can confuse the gateway's auth flow.
    const token = this.config?.token?.trim();
    const auth = token ? { token } : undefined;

    // ── Device identity (Ed25519) ────────────────────────────────────
    // OpenClaw 2026.2.14+ requires a device object for scope-based auth.
    // Without it, scopes are stripped to empty and all RPC calls fail with
    // "missing scope: operator.*".
    const role = 'operator';
    const scopes = ['operator.read', 'operator.write', 'operator.admin'];

    let device: { id: string; publicKey: string; signature: string; signedAt: number; nonce?: string } | undefined;
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const identity = await invoke<{ deviceId: string; publicKeyBase64Url: string }>('get_device_identity');

      const signedAtMs = Date.now();
      const nonce = this._challengeNonce ?? undefined;
      const version = nonce ? 'v2' : 'v1';

      // Build payload matching OpenClaw's buildDeviceAuthPayload():
      //   version|deviceId|clientId|clientMode|role|scopes_csv|signedAtMs|token[|nonce]
      const payloadParts = [
        version,
        identity.deviceId,
        'gateway-client',
        'ui',
        role,
        scopes.join(','),
        String(signedAtMs),
        token ?? '',
      ];
      if (version === 'v2') payloadParts.push(nonce ?? '');
      const payload = payloadParts.join('|');

      const signature = await invoke<string>('sign_device_payload', { payload });

      device = {
        id: identity.deviceId,
        publicKey: identity.publicKeyBase64Url,
        signature,
        signedAt: signedAtMs,
        ...(nonce ? { nonce } : {}),
      };
      console.log(`[gateway] Device identity ready: ${identity.deviceId.slice(0, 12)}...`);
    } catch (e) {
      console.warn('[gateway] Device identity unavailable (scopes may be limited):', e);
    }

    console.log(`[gateway] Sending connect handshake (auth: ${auth ? 'token present' : 'none'}, nonce: ${this._challengeNonce ? 'yes' : 'no'}, device: ${device ? 'yes' : 'no'})`);

    const hello = await this.request<HelloOk>('connect', {
      minProtocol: PROTOCOL_VERSION,
      maxProtocol: PROTOCOL_VERSION,
      client: {
        id: 'gateway-client',
        version: '0.1.0',
        platform: detectPlatform(),
        mode: 'ui',
        displayName: 'Paw Desktop',
      },
      role,
      scopes,
      caps: [],
      commands: [],
      permissions: {},
      auth,
      device,
      locale: navigator.language || 'en-US',
      userAgent: `paw-desktop/0.1.0 (${detectPlatform()})`,
    });

    return hello;
  }

  disconnect() {
    this.stopKeepalive();
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.rejectAllPending('disconnected');
    const oldWs = this.ws;
    this.ws = null;
    this._connected = false;
    // Note: do NOT reset _connecting here — connect() sets it before calling disconnect()
    this.hello = null;
    // Null out handlers on the old socket to prevent ghost callbacks
    if (oldWs) {
      oldWs.onopen = null;
      oldWs.onmessage = null;
      oldWs.onclose = null;
      oldWs.onerror = null;
      try { oldWs.close(); } catch { /* ignore */ }
    }
  }

  private scheduleReconnect() {
    if (this.reconnectTimer) return;
    if (this._connecting) return; // already trying to connect
    this.reconnectAttempts++;
    if (this.reconnectAttempts > MAX_RECONNECT_ATTEMPTS) {
      console.warn('[gateway] Max reconnect attempts reached, giving up');
      this.emit('_reconnect_exhausted', { attempts: this.reconnectAttempts });
      return;
    }
    // Exponential backoff: 3s, 6s, 12s, 24s... capped at 60s
    const delay = Math.min(RECONNECT_BASE_MS * Math.pow(2, this.reconnectAttempts - 1), RECONNECT_MAX_MS);
    console.log(`[gateway] Reconnect attempt ${this.reconnectAttempts}/${MAX_RECONNECT_ATTEMPTS} in ${delay}ms`);
    this.reconnectTimer = window.setTimeout(() => {
      this.reconnectTimer = null;
      if (this.config && !this._connecting) {
        this.connect(this.config).catch((e) => {
          console.warn('[gateway] Reconnect failed:', e?.message ?? e);
          this.scheduleReconnect();
        });
      }
    }, delay);
  }

  /** Reset reconnect counter (call after a successful manual connect) */
  resetReconnect() {
    this.reconnectAttempts = 0;
  }

  private startKeepalive() {
    this.stopKeepalive();
    // Send a lightweight ping every 30s to keep the connection alive
    this.keepaliveTimer = window.setInterval(() => {
      try {
        if (this._connected && this.ws?.readyState === WebSocket.OPEN) {
          this.request('health').catch(() => {
            console.warn('[gateway] Keepalive ping failed');
          });
        }
      } catch {
        // Guard against synchronous throw if ws state changes mid-check
        console.warn('[gateway] Keepalive error (non-fatal)');
      }
    }, 30_000);
  }

  private stopKeepalive() {
    if (this.keepaliveTimer) {
      clearInterval(this.keepaliveTimer);
      this.keepaliveTimer = null;
    }
  }

  get connected(): boolean {
    return this._connected && this.ws?.readyState === WebSocket.OPEN;
  }

  // ── Frame handling ───────────────────────────────────────────────────

  private handleFrame(frame: { type: string; id?: string; ok?: boolean; payload?: unknown; error?: unknown; event?: string; seq?: number }) {
    if (frame.type === 'res' && frame.id != null) {
      const pending = this.pendingRequests.get(String(frame.id));
      if (pending) {
        clearTimeout(pending.timer);
        this.pendingRequests.delete(String(frame.id));
        if (frame.ok) {
          pending.resolve(frame.payload);
        } else {
          const errMsg = typeof frame.error === 'object' && frame.error !== null
            ? (frame.error as { message?: string }).message || JSON.stringify(frame.error)
            : String(frame.error ?? 'request failed');
          pending.reject(new Error(errMsg));
        }
      }
    } else if (frame.type === 'event' && frame.event) {
      // Track sequence for gap detection
      if (typeof frame.seq === 'number') {
        if (this._lastSeq > 0 && frame.seq > this._lastSeq + 1) {
          this.emit('_gap', { expected: this._lastSeq + 1, received: frame.seq });
        }
        this._lastSeq = frame.seq;
      }
      this.emit(frame.event, frame.payload);
      // Log events — include agent/chat for debugging streaming issues
      if (frame.event !== 'connect.challenge' && frame.event !== 'health' && frame.event !== 'tick') {
        try {
          const payloadStr = JSON.stringify(frame.payload).slice(0, 300);
          // For agent deltas, only log first 80 chars to avoid console spam
          if (frame.event === 'agent') {
            const p = frame.payload as Record<string, unknown> | undefined;
            console.log(`[gateway] agent event: stream=${p?.stream} session=${p?.sessionKey} runId=${String(p?.runId).slice(0,12)}`, payloadStr.slice(0, 80));
          } else if (frame.event === 'chat') {
            console.log(`[gateway] chat event:`, payloadStr.slice(0, 200));
          } else {
            console.log(`[gateway] Event: ${frame.event}`, payloadStr.slice(0, 200));
          }
        } catch {
          console.log(`[gateway] Event: ${frame.event}`);
        }
      }
    }
  }

  // ── Request / response ───────────────────────────────────────────────

  async request<T = unknown>(method: string, params?: unknown, timeoutMs?: number): Promise<T> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      throw new Error('Not connected to gateway');
    }

    const id = nextId();

    return new Promise<T>((resolve, reject) => {
      const timer = window.setTimeout(() => {
        this.pendingRequests.delete(id);
        reject(new Error(`Request timeout: ${method}`));
      }, timeoutMs ?? REQUEST_TIMEOUT_MS);

      this.pendingRequests.set(id, {
        resolve: resolve as (v: unknown) => void,
        reject,
        timer,
      });

      this.ws!.send(JSON.stringify({ type: 'req', id, method, params }));
    });
  }

  private rejectAllPending(reason: string) {
    for (const [id, p] of this.pendingRequests) {
      clearTimeout(p.timer);
      p.reject(new Error(reason));
      this.pendingRequests.delete(id);
    }
  }

  // ── Events ───────────────────────────────────────────────────────────

  on(event: string, handler: EventHandler): () => void {
    if (!this.eventHandlers.has(event)) {
      this.eventHandlers.set(event, new Set());
    }
    this.eventHandlers.get(event)!.add(handler);
    return () => this.off(event, handler);
  }

  off(event: string, handler: EventHandler) {
    this.eventHandlers.get(event)?.delete(handler);
  }

  private emit(event: string, payload: unknown) {
    const handlers = this.eventHandlers.get(event);
    if (handlers) {
      for (const h of handlers) {
        try { h(payload); } catch (e) { console.error(`[gateway] event handler error (${event}):`, e); }
      }
    }
  }

  // ── High-level API ───────────────────────────────────────────────────

  // Health
  async getHealth(): Promise<HealthSummary> {
    return this.request<HealthSummary>('health');
  }

  async getStatus(): Promise<unknown> {
    return this.request('status');
  }

  // Agents
  async listAgents(): Promise<AgentsListResult> {
    return this.request<AgentsListResult>('agents.list', {});
  }

  async getAgentIdentity(agentId?: string): Promise<AgentIdentityResult> {
    return this.request<AgentIdentityResult>('agent.identity.get', agentId ? { agentId } : {});
  }

  async createAgent(params: import('./types').AgentCreateParams): Promise<import('./types').AgentCreateResult> {
    return this.request<import('./types').AgentCreateResult>('agents.create', params);
  }

  async updateAgent(params: import('./types').AgentUpdateParams): Promise<import('./types').AgentUpdateResult> {
    return this.request<import('./types').AgentUpdateResult>('agents.update', params);
  }

  async deleteAgent(agentId: string, deleteFiles = false): Promise<import('./types').AgentDeleteResult> {
    return this.request<import('./types').AgentDeleteResult>('agents.delete', { agentId, deleteFiles });
  }

  // Channels
  async getChannelsStatus(probe = false, timeoutMs?: number): Promise<ChannelsStatusResult> {
    return this.request<ChannelsStatusResult>('channels.status', { probe, timeoutMs });
  }

  async startWebLogin(channelId: string, accountId?: string): Promise<unknown> {
    return this.request('web.login.start', { channelId, accountId });
  }

  async waitWebLogin(channelId: string, timeoutMs = 60_000): Promise<unknown> {
    return this.request('web.login.wait', { channelId, timeoutMs }, timeoutMs + 5000);
  }

  async logoutChannel(channelId: string, accountId?: string): Promise<unknown> {
    return this.request('channels.logout', { channelId, accountId });
  }

  // Sessions
  async listSessions(opts?: { limit?: number; includeGlobal?: boolean; includeUnknown?: boolean; agentId?: string; includeDerivedTitles?: boolean; includeLastMessage?: boolean }): Promise<SessionsListResult> {
    return this.request<SessionsListResult>('sessions.list', {
      limit: opts?.limit ?? 50,
      includeGlobal: opts?.includeGlobal ?? true,
      includeUnknown: opts?.includeUnknown ?? false,
      includeDerivedTitles: opts?.includeDerivedTitles ?? true,
      includeLastMessage: opts?.includeLastMessage ?? false,
      ...(opts?.agentId ? { agentId: opts.agentId } : {}),
    });
  }

  async patchSession(key: string, patch: Record<string, unknown>): Promise<unknown> {
    return this.request('sessions.patch', { key, ...patch });
  }

  async resetSession(key: string): Promise<unknown> {
    return this.request('sessions.reset', { key });
  }

  async deleteSession(key: string): Promise<unknown> {
    return this.request('sessions.delete', { key });
  }

  async sessionsPreview(key: string): Promise<SessionPreviewResult> {
    return this.request<SessionPreviewResult>('sessions.preview', { key });
  }

  async sessionsCompact(key?: string): Promise<SessionCompactResult> {
    return this.request<SessionCompactResult>('sessions.compact', key ? { key } : {});
  }

  // Chat
  async chatHistory(sessionKey: string, limit = 50): Promise<ChatHistoryResult> {
    return this.request<ChatHistoryResult>('chat.history', { sessionKey, limit });
  }

  async chatSend(sessionKey: string, message: string, opts?: {
    thinking?: string;
    idempotencyKey?: string;
    model?: string;
    thinkingLevel?: string;
    temperature?: number;
    attachments?: import('./types').ChatAttachment[];
    agentProfile?: Partial<import('./types').Agent>;
  }): Promise<ChatSendResult> {
    const idempotencyKey = opts?.idempotencyKey ?? crypto.randomUUID();

    // -- Agent Profile Injection --
    // If an agentProfile is provided, prepend personality context to the message.
    // The gateway chat.send does NOT accept a system/systemPrompt param,
    // so we bake it into the user message instead.
    let finalMessage = message;
    if (opts?.agentProfile) {
      const profile = opts.agentProfile;
      const parts: string[] = [];
      
      if (profile.name) {
        parts.push(`You are ${profile.name}.`);
      }
      if (profile.bio) {
        parts.push(profile.bio);
      }
      
      if (profile.personality) {
        const p = profile.personality;
        const personalityDesc: string[] = [];
        if (p.tone) personalityDesc.push(`your tone is ${p.tone}`);
        if (p.initiative) personalityDesc.push(`you are ${p.initiative} in your initiative`);
        if (p.detail) personalityDesc.push(`you are ${p.detail} in your responses`);
        if (personalityDesc.length > 0) {
          parts.push(`Your personality is defined as follows: ${personalityDesc.join(', ')}.`);
        }
      }
      
      if (profile.boundaries && profile.boundaries.length > 0) {
        parts.push(`You must strictly follow these rules:\n${profile.boundaries.map(b => `- ${b}`).join('\n')}`);
      }
      
      if (profile.systemPrompt) {
        parts.push(profile.systemPrompt);
      }
      
      if (parts.length > 0) {
        finalMessage = `[Context: ${parts.join(' ')}]\n\n${message}`;
      }
    }

    const params: Record<string, unknown> = {
      sessionKey,
      message: finalMessage,
      idempotencyKey,
      ...(opts?.thinking ? { thinking: opts.thinking } : {}),
      ...(opts?.model ? { model: opts.model } : {}),
      ...(opts?.thinkingLevel ? { thinkingLevel: opts.thinkingLevel } : {}),
      ...(opts?.temperature != null ? { temperature: opts.temperature } : {}),
      ...(opts?.attachments?.length ? { attachments: opts.attachments } : {}),
    };
    if (opts?.attachments?.length) {
      console.log('[gateway] chat.send with attachments:', opts.attachments.length, 'params.attachments:', (params.attachments as unknown[])?.length);
    }
    return this.request<ChatSendResult>('chat.send', params, 120_000);
  }

  async chatAbort(sessionKey: string, runId?: string): Promise<void> {
    await this.request('chat.abort', { sessionKey, ...(runId ? { runId } : {}) });
  }

  // Cron
  async cronList(): Promise<CronListResult> {
    return this.request<CronListResult>('cron.list', {});
  }

  async cronStatus(): Promise<unknown> {
    return this.request('cron.status', {});
  }

  async cronAdd(job: Partial<CronJob>): Promise<unknown> {
    return this.request('cron.add', job);
  }

  async cronUpdate(jobId: string, patch: Partial<CronJob>): Promise<unknown> {
    return this.request('cron.update', { id: jobId, ...patch });
  }

  async cronRemove(jobId: string): Promise<unknown> {
    return this.request('cron.remove', { id: jobId });
  }

  async cronRun(jobId: string): Promise<unknown> {
    return this.request('cron.run', { id: jobId });
  }

  async cronRuns(jobId?: string, limit = 20): Promise<{ runs: CronRunLogEntry[] }> {
    return this.request('cron.runs', { id: jobId, limit });
  }

  // Skills
  async skillsStatus(agentId?: string): Promise<SkillsStatusResult> {
    return this.request<SkillsStatusResult>('skills.status', agentId ? { agentId } : {});
  }

  async skillsBins(): Promise<{ bins: string[] }> {
    return this.request('skills.bins', {});
  }

  async skillsInstall(name: string, installId: string): Promise<unknown> {
    return this.request('skills.install', { name, installId }, 120_000);
  }

  async skillsUpdate(skillKey: string, updates: { enabled?: boolean; apiKey?: string; env?: Record<string, string> }): Promise<unknown> {
    return this.request('skills.update', { skillKey, ...updates });
  }

  // Models
  async modelsList(): Promise<ModelsListResult> {
    return this.request<ModelsListResult>('models.list', {});
  }

  // Nodes
  async nodeList(): Promise<NodeListResult> {
    return this.request<NodeListResult>('node.list', {});
  }

  // Config
  async configGet(): Promise<GatewayConfigResult> {
    return this.request<GatewayConfigResult>('config.get', {});
  }

  /** Write full config object via config.apply (sends only { raw } — no extra metadata). */
  async configWrite(config: Record<string, unknown>): Promise<ConfigApplyResult> {
    const raw = JSON.stringify(config, null, 2);
    return this.request<ConfigApplyResult>('config.apply', { raw }, 60_000);
  }

  /** Write raw JSON string directly via config.apply. Used by the raw config editor. */
  async configApplyRaw(rawJson: string): Promise<ConfigApplyResult> {
    return this.request<ConfigApplyResult>('config.apply', { raw: rawJson }, 60_000);
  }

  async configSchema(): Promise<ConfigSchemaResult> {
    return this.request<ConfigSchemaResult>('config.schema', {});
  }

  // Presence
  async systemPresence(): Promise<{ entries: PresenceEntry[] }> {
    return this.request('system-presence', {});
  }

  // System events
  async systemEvent(event: string, data?: Record<string, unknown>): Promise<unknown> {
    return this.request('system-event', { event, ...(data ?? {}) });
  }

  async lastHeartbeat(): Promise<{ ts?: number; ok?: boolean; [key: string]: unknown }> {
    return this.request('last-heartbeat', {});
  }

  async setHeartbeats(enabled: boolean, intervalMs?: number): Promise<unknown> {
    return this.request('set-heartbeats', { enabled, ...(intervalMs ? { intervalMs } : {}) });
  }

  // Exec Approvals
  async execApprovalsGet(): Promise<ExecApprovalsSnapshot> {
    return this.request<ExecApprovalsSnapshot>('exec.approvals.get', {});
  }

  async execApprovalsSet(updates: Partial<ExecApprovalsSnapshot>): Promise<unknown> {
    return this.request('exec.approvals.set', updates);
  }

  async execApprovalsNodeGet(): Promise<ExecApprovalsSnapshot> {
    return this.request<ExecApprovalsSnapshot>('exec.approvals.node.get', {});
  }

  async execApprovalsNodeSet(updates: Partial<ExecApprovalsSnapshot>): Promise<unknown> {
    return this.request('exec.approvals.node.set', updates);
  }

  async execApprovalResolve(id: string, allowed: boolean): Promise<unknown> {
    return this.request('exec.approvals.resolve', { id, allowed });
  }

  // Agent files (memory)
  async agentFilesList(agentId?: string): Promise<AgentsFilesListResult> {
    return this.request<AgentsFilesListResult>('agents.files.list', agentId ? { agentId } : {});
  }

  async agentFilesGet(filePath: string, agentId?: string): Promise<AgentsFilesGetResult> {
    return this.request<AgentsFilesGetResult>('agents.files.get', { path: filePath, ...(agentId ? { agentId } : {}) });
  }

  async agentFilesSet(filePath: string, content: string, agentId?: string): Promise<unknown> {
    return this.request('agents.files.set', { path: filePath, content, ...(agentId ? { agentId } : {}) });
  }

  // Logs
  async logsTail(lines = 100): Promise<LogsTailResult> {
    // Gateway expects { count } not { lines } — try both for compatibility
    try {
      return await this.request<LogsTailResult>('logs.tail', { count: lines });
    } catch {
      return this.request<LogsTailResult>('logs.tail', {});
    }
  }

  // Usage
  async usageStatus(): Promise<UsageStatusResult> {
    return this.request<UsageStatusResult>('usage.status', {});
  }

  async usageCost(): Promise<UsageCostResult> {
    return this.request<UsageCostResult>('usage.cost', {});
  }

  // Send message (direct channel send)
  async send(params: Record<string, unknown>): Promise<unknown> {
    return this.request('send', params);
  }

  // Agent run (direct, sessionless agent turn)
  async agent(params: Record<string, unknown>): Promise<import('./types').AgentRunResult> {
    return this.request<import('./types').AgentRunResult>('agent', params, 120_000);
  }

  // Agent wait for completion
  async agentWait(runId: string, timeoutMs = 120_000): Promise<AgentWaitResult> {
    return this.request<AgentWaitResult>('agent.wait', { runId }, timeoutMs + 5000);
  }

  // Wake event (system trigger)
  async wake(params?: Record<string, unknown>): Promise<unknown> {
    return this.request('wake', params ?? {});
  }

  // Session reset (clear history, keep session)
  async sessionReset(key: string): Promise<unknown> {
    return this.request('sessions.reset', { key });
  }

  // TTS (Text-to-Speech)
  async ttsStatus(): Promise<{ enabled: boolean; provider?: string; voice?: string; providers?: string[] }> {
    return this.request('tts.status', {});
  }

  async ttsProviders(): Promise<{ providers: Array<{ id: string; name: string; voices?: string[] }> }> {
    return this.request('tts.providers', {});
  }

  async ttsSetProvider(provider: string, voice?: string): Promise<unknown> {
    return this.request('tts.setProvider', { provider, voice });
  }

  async ttsEnable(enabled: boolean): Promise<unknown> {
    return this.request('tts.enable', { enabled });
  }

  async ttsConvert(text: string, voice?: string): Promise<{ audio?: string; url?: string; path?: string }> {
    return this.request('tts.convert', { text, voice });
  }

  // Talk Mode (continuous voice)
  async talkConfig(): Promise<{ enabled?: boolean; wakeWord?: string; voice?: string }> {
    return this.request('talk.config', {});
  }

  async talkMode(enabled: boolean): Promise<unknown> {
    return this.request('talk.mode', { enabled });
  }

  // Voice Wake
  async voicewakeGet(): Promise<{ triggers: string[] }> {
    return this.request('voicewake.get', {});
  }

  async voicewakeSet(triggers: string[]): Promise<unknown> {
    return this.request('voicewake.set', { triggers });
  }

  // Node management (extended)
  async nodeDescribe(nodeId: string): Promise<{ node: import('./types').GatewayNode; caps?: string[]; commands?: string[] }> {
    return this.request('node.describe', { nodeId });
  }

  async nodeInvoke(nodeId: string, command: string, params?: Record<string, unknown>): Promise<unknown> {
    return this.request('node.invoke', { nodeId, command, params });
  }

  async nodeRename(nodeId: string, name: string): Promise<unknown> {
    return this.request('node.rename', { nodeId, name });
  }

  async nodePairList(): Promise<{ requests: Array<{ id: string; nodeId: string; name?: string; requestedAt: number }> }> {
    return this.request('node.pair.list', {});
  }

  async nodePairApprove(requestId: string): Promise<unknown> {
    return this.request('node.pair.approve', { requestId });
  }

  async nodePairReject(requestId: string): Promise<unknown> {
    return this.request('node.pair.reject', { requestId });
  }

  // Device pairing
  async devicePairList(): Promise<{ devices: Array<{ id: string; name?: string; platform?: string; pairedAt?: number }> }> {
    return this.request('device.pair.list', {});
  }

  async devicePairApprove(deviceId: string): Promise<unknown> {
    return this.request('device.pair.approve', { deviceId });
  }

  async devicePairReject(deviceId: string): Promise<unknown> {
    return this.request('device.pair.reject', { deviceId });
  }

  async deviceTokenRotate(deviceId: string): Promise<{ token: string }> {
    return this.request('device.token.rotate', { deviceId });
  }

  async deviceTokenRevoke(deviceId: string): Promise<unknown> {
    return this.request('device.token.revoke', { deviceId });
  }

  // Onboarding wizard
  async wizardStatus(): Promise<{ active: boolean; step?: string; completed?: boolean }> {
    // Some gateway versions require sessionId — try with a placeholder, then without
    try {
      return await this.request('wizard.status', { sessionId: 'paw' });
    } catch {
      return { active: false, completed: false };
    }
  }

  async wizardStart(): Promise<{ step: string }> {
    return this.request('wizard.start', {});
  }

  async wizardNext(data?: Record<string, unknown>): Promise<{ step?: string; completed?: boolean }> {
    return this.request('wizard.next', data ?? {});
  }

  async wizardCancel(): Promise<unknown> {
    return this.request('wizard.cancel', {});
  }

  // Browser control
  async browserStatus(): Promise<{ running: boolean; tabs?: Array<{ id: string; url: string; title?: string }> }> {
    try {
      return await this.request('browser.status', {});
    } catch {
      return { running: false };
    }
  }

  async browserStart(): Promise<unknown> {
    return this.request('browser.start', {});
  }

  async browserStop(): Promise<unknown> {
    return this.request('browser.stop', {});
  }

  // Self-update
  async updateRun(): Promise<{ updated: boolean; version?: string }> {
    return this.request('update.run', {}, 120_000);
  }
}

export const gateway = new GatewayClient();
export type { EventHandler };
