// Claw Desktop - Gateway WebSocket Client

export interface GatewayConfig {
  url: string;
  token: string;
}

export interface GatewayStatus {
  connected: boolean;
  version?: string;
  uptime?: number;
}

export interface Channel {
  id: string;
  type: string;
  name: string;
  status: 'connected' | 'disconnected' | 'pending' | 'qr_required';
  linked?: boolean;
}

export interface Session {
  key: string;
  kind: string;
  model?: string;
  lastMessage?: string;
  lastActive?: string;
}

export interface CronJob {
  id: string;
  name?: string;
  schedule: unknown;
  payload: unknown;
  enabled: boolean;
  lastRun?: string;
  nextRun?: string;
}

export interface Skill {
  id: string;
  name: string;
  description?: string;
  enabled: boolean;
  installed: boolean;
}

export interface Node {
  id: string;
  name: string;
  connected: boolean;
  paired: boolean;
  caps?: string[];
}

type MessageHandler = (event: unknown) => void;

class GatewayClient {
  private ws: WebSocket | null = null;
  private config: GatewayConfig | null = null;
  private messageId = 0;
  private pendingRequests = new Map<number, { resolve: (v: unknown) => void; reject: (e: Error) => void }>();
  private eventHandlers = new Map<string, Set<MessageHandler>>();
  private reconnectTimer: number | null = null;
  private connected = false;

  async connect(config: GatewayConfig): Promise<void> {
    this.config = config;
    
    return new Promise((resolve, reject) => {
      const wsUrl = config.url.replace(/^http/, 'ws');
      this.ws = new WebSocket(wsUrl);
      
      this.ws.onopen = async () => {
        try {
          // Send connect handshake
          await this.sendConnect();
          this.connected = true;
          resolve();
        } catch (e) {
          reject(e);
        }
      };
      
      this.ws.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data);
          this.handleMessage(data);
        } catch (e) {
          console.error('Failed to parse message:', e);
        }
      };
      
      this.ws.onclose = () => {
        this.connected = false;
        this.emit('disconnected', {});
        this.scheduleReconnect();
      };
      
      this.ws.onerror = (e) => {
        console.error('WebSocket error:', e);
        reject(new Error('WebSocket connection failed'));
      };
    });
  }

  private async sendConnect(): Promise<void> {
    const response = await this.request('connect', {
      minProtocol: 1,
      maxProtocol: 1,
      client: {
        id: 'claw-desktop',
        displayName: 'Claw Desktop',
        version: '0.1.0',
        platform: 'macos',
        mode: 'control-ui',
      },
      auth: this.config?.token ? { token: this.config.token } : undefined,
    });
    
    if (!response) {
      throw new Error('Connect failed');
    }
  }

  private handleMessage(data: { type: string; id?: number; ok?: boolean; payload?: unknown; error?: unknown; event?: string }) {
    if (data.type === 'res' && data.id !== undefined) {
      const pending = this.pendingRequests.get(data.id);
      if (pending) {
        this.pendingRequests.delete(data.id);
        if (data.ok) {
          pending.resolve(data.payload);
        } else {
          pending.reject(new Error(String(data.error) || 'Request failed'));
        }
      }
    } else if (data.type === 'event' && data.event) {
      this.emit(data.event, data.payload);
    }
  }

  private emit(event: string, payload: unknown) {
    const handlers = this.eventHandlers.get(event);
    if (handlers) {
      handlers.forEach(h => h(payload));
    }
  }

  on(event: string, handler: MessageHandler) {
    if (!this.eventHandlers.has(event)) {
      this.eventHandlers.set(event, new Set());
    }
    this.eventHandlers.get(event)!.add(handler);
  }

  off(event: string, handler: MessageHandler) {
    this.eventHandlers.get(event)?.delete(handler);
  }

  private scheduleReconnect() {
    if (this.reconnectTimer) return;
    this.reconnectTimer = window.setTimeout(() => {
      this.reconnectTimer = null;
      if (this.config) {
        this.connect(this.config).catch(() => {});
      }
    }, 5000);
  }

  async request<T>(method: string, params?: unknown): Promise<T> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      throw new Error('Not connected');
    }

    const id = ++this.messageId;
    
    return new Promise((resolve, reject) => {
      this.pendingRequests.set(id, { 
        resolve: resolve as (v: unknown) => void, 
        reject 
      });
      
      this.ws!.send(JSON.stringify({
        type: 'req',
        id,
        method,
        params,
      }));

      // Timeout after 30s
      setTimeout(() => {
        if (this.pendingRequests.has(id)) {
          this.pendingRequests.delete(id);
          reject(new Error('Request timeout'));
        }
      }, 30000);
    });
  }

  disconnect() {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.ws?.close();
    this.ws = null;
    this.connected = false;
  }

  isConnected() {
    return this.connected && this.ws?.readyState === WebSocket.OPEN;
  }

  // High-level API methods

  async getStatus(): Promise<GatewayStatus> {
    try {
      const status = await this.request<{ version?: string; uptimeMs?: number }>('status');
      return {
        connected: true,
        version: status.version,
        uptime: status.uptimeMs,
      };
    } catch {
      return { connected: false };
    }
  }

  async getHealth(): Promise<unknown> {
    return this.request('health');
  }

  async getChannels(): Promise<Channel[]> {
    try {
      const result = await this.request<{ channels: Channel[] }>('channels.status');
      return result.channels || [];
    } catch {
      return [];
    }
  }

  async getSessions(limit = 20): Promise<Session[]> {
    try {
      const result = await this.request<{ sessions: Session[] }>('sessions.list', { limit });
      return result.sessions || [];
    } catch {
      return [];
    }
  }

  async getCronJobs(): Promise<CronJob[]> {
    try {
      const result = await this.request<{ jobs: CronJob[] }>('cron.list');
      return result.jobs || [];
    } catch {
      return [];
    }
  }

  async getSkills(): Promise<Skill[]> {
    try {
      const result = await this.request<{ skills: Skill[] }>('skills.list');
      return result.skills || [];
    } catch {
      return [];
    }
  }

  async getNodes(): Promise<Node[]> {
    try {
      const result = await this.request<{ nodes: Node[] }>('node.list');
      return result.nodes || [];
    } catch {
      return [];
    }
  }

  async getConfig(): Promise<unknown> {
    return this.request('config.get');
  }

  async sendChat(message: string, sessionKey?: string): Promise<{ runId: string }> {
    return this.request('chat.send', { message, sessionKey });
  }

  async abortChat(sessionKey?: string): Promise<void> {
    await this.request('chat.abort', { sessionKey });
  }

  async getChatHistory(sessionKey?: string, limit = 50): Promise<unknown[]> {
    const result = await this.request<{ messages: unknown[] }>('chat.history', { sessionKey, limit });
    return result.messages || [];
  }

  async createCronJob(job: Partial<CronJob>): Promise<CronJob> {
    return this.request('cron.add', { job });
  }

  async updateCronJob(jobId: string, patch: Partial<CronJob>): Promise<void> {
    await this.request('cron.update', { jobId, patch });
  }

  async deleteCronJob(jobId: string): Promise<void> {
    await this.request('cron.remove', { jobId });
  }

  async runCronJob(jobId: string): Promise<void> {
    await this.request('cron.run', { jobId });
  }

  async enableSkill(skillId: string): Promise<void> {
    await this.request('skills.enable', { skillId });
  }

  async disableSkill(skillId: string): Promise<void> {
    await this.request('skills.disable', { skillId });
  }

  async patchConfig(patch: unknown): Promise<void> {
    await this.request('config.patch', { patch });
  }

  async restartGateway(): Promise<void> {
    await this.request('gateway.restart');
  }
}

export const gateway = new GatewayClient();
