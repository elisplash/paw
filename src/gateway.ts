// Paw — OpenClaw Gateway WebSocket Client (Protocol v3)

import type {
  GatewayConfig,
  HelloOk,
  HealthSummary,
  AgentsListResult,
  AgentIdentityResult,
  ChannelsStatusResult,
  SessionsListResult,
  ChatHistoryResult,
  ChatSendResult,
  CronListResult,
  CronJob,
  CronRunLogEntry,
  SkillsStatusResult,
  ModelsListResult,
  NodeListResult,
  GatewayConfigResult,
  PresenceEntry,
  ExecApprovalsSnapshot,
  AgentsFilesListResult,
  AgentsFilesGetResult,
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
        try {
          const frame = JSON.parse(ev.data);
          this.handleFrame(frame);
        } catch {
          console.error('[gateway] bad frame', ev.data);
        }
      };

      this.ws.onclose = (ev) => {
        const wasConnected = this._connected;
        this._connected = false;
        this._connecting = false;
        this.hello = null;
        console.log(`[gateway] WebSocket closed: code=${ev.code} reason="${ev.reason}" wasConnected=${wasConnected}`);
        this.rejectAllPending('connection closed');
        if (wasConnected) {
          this.emit('_disconnected', {});
          this.reconnectAttempts = 0; // reset since we had a successful connection
          this.scheduleReconnect();
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

    console.log(`[gateway] Sending connect handshake (auth: ${auth ? 'token present' : 'none'}, nonce: ${this._challengeNonce ? 'yes' : 'no'})`);

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
      role: 'operator',
      scopes: ['operator.read', 'operator.write', 'operator.admin'],
      caps: [],
      commands: [],
      permissions: {},
      auth,
      locale: navigator.language || 'en-US',
      userAgent: `paw-desktop/0.1.0 (${detectPlatform()})`,
    });

    return hello;
  }

  disconnect() {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.rejectAllPending('disconnected');
    if (this.ws) {
      try { this.ws.close(); } catch { /* ignore */ }
    }
    this.ws = null;
    this._connected = false;
    this._connecting = false;
    this.hello = null;
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

  // Chat
  async chatHistory(sessionKey: string, limit = 50): Promise<ChatHistoryResult> {
    return this.request<ChatHistoryResult>('chat.history', { sessionKey, limit });
  }

  async chatSend(sessionKey: string, message: string, opts?: { thinking?: string; idempotencyKey?: string }): Promise<ChatSendResult> {
    const idempotencyKey = opts?.idempotencyKey ?? crypto.randomUUID();
    return this.request<ChatSendResult>('chat.send', {
      sessionKey,
      message,
      idempotencyKey,
      ...(opts?.thinking ? { thinking: opts.thinking } : {}),
    }, 120_000); // chat can take a while
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

  async skillsUpdate(name: string, updates: Record<string, unknown>): Promise<unknown> {
    return this.request('skills.update', { name, ...updates });
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

  async configSet(config: Record<string, unknown>): Promise<unknown> {
    return this.request('config.set', { config });
  }

  async configPatch(patch: Record<string, unknown>): Promise<unknown> {
    return this.request('config.patch', { patch });
  }

  async configSchema(): Promise<unknown> {
    return this.request('config.schema', {});
  }

  // Presence
  async systemPresence(): Promise<{ entries: PresenceEntry[] }> {
    return this.request('system-presence', {});
  }

  // Exec Approvals
  async execApprovalsGet(): Promise<ExecApprovalsSnapshot> {
    return this.request<ExecApprovalsSnapshot>('exec.approvals.get', {});
  }

  async execApprovalsSet(updates: Partial<ExecApprovalsSnapshot>): Promise<unknown> {
    return this.request('exec.approvals.set', updates);
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
  async logsTail(lines = 100): Promise<{ lines: string[] }> {
    return this.request('logs.tail', { lines });
  }

  // Send message (direct channel send)
  async send(params: Record<string, unknown>): Promise<unknown> {
    return this.request('send', params);
  }

  // Agent run (agent turn)
  async agent(params: Record<string, unknown>): Promise<unknown> {
    return this.request('agent', params, 120_000);
  }
}

export const gateway = new GatewayClient();
export type { EventHandler };
